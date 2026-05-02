use serde_json::{json, Value};
use std::collections::BTreeMap;
use uica_model::{GraphReport, GraphSeries, TraceInstructionRow, TraceReport, TraceUopRow};

const TRACE_TEMPLATE: &str = include_str!("../../../traceTemplate.html");

pub fn render_trace_html(report: &TraceReport) -> Result<String, String> {
    let table_data_value = serde_json::to_value(&report.table_data)
        .map_err(|err| format!("failed to serialize trace report data: {err}"))?;
    let table_data = json_for_script(&table_data_value)
        .map_err(|err| format!("failed to serialize trace report data: {err}"))?;
    Ok(TRACE_TEMPLATE.replace(
        "var tableData = {}",
        &format!("var tableData = {table_data}"),
    ))
}

pub fn render_graph_html(report: &GraphReport) -> Result<String, String> {
    let plotly_series: Vec<serde_json::Value> =
        report.series.iter().map(graph_series_to_plotly).collect();
    let data_value = Value::Array(plotly_series);
    let data = json_for_script(&data_value)
        .map_err(|err| format!("failed to serialize graph series: {err}"))?;
    let enable_toggle = report.interpolation_toggle && !report.series.is_empty();
    let config = if enable_toggle {
        json!({
            "displayModeBar": true,
            "modeBarButtonsToRemove": ["autoScale2d", "select2d", "lasso2d"],
            "modeBarButtonsToAdd": [{
                "name": "Toggle interpolation mode"
            }]
        })
    } else {
        json!({
            "displayModeBar": true,
            "modeBarButtonsToRemove": ["autoScale2d", "select2d", "lasso2d"]
        })
    };
    let config = json_for_script(&config)
        .map_err(|err| format!("failed to serialize graph config: {err}"))?;
    let toggle_script = if enable_toggle {
        r#"function toggleInterpolation(gd) {
  const next = gd.data[0].line.shape === "hv" ? "linear" : "hv";
  Plotly.restyle(gd, "line.shape", next);
}
"#
    } else {
        ""
    };
    let toggle_setup = if enable_toggle {
        r#"config.modeBarButtonsToAdd[0].icon = Plotly.Icons.drawline;
config.modeBarButtonsToAdd[0].click = toggleInterpolation;
"#
    } else {
        ""
    };
    Ok(format!(
        r#"<html>
<head>
<meta charset="utf-8"/><title>Graph</title>
<script src="https://cdn.plot.ly/plotly-3.0.1.min.js"></script>
</head>
<body>
<div id="uica-graph"></div>
<script>
{toggle_script}const data = {data};
const layout = {{"xaxis": {{"title": {{"text": "Cycle"}}}}}};
const config = {config};
{toggle_setup}Plotly.newPlot("uica-graph", data, layout, config);
</script>
</body>
</html>
"#
    ))
}

fn json_for_script(value: &Value) -> serde_json::Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(json
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

fn graph_series_to_plotly(series: &GraphSeries) -> serde_json::Value {
    json!({
        "name": series.name,
        "y": series.y,
        "mode": series.mode,
        "line": {"shape": series.line_shape},
    })
}

pub fn build_graph_report(
    frontend: &crate::sim::FrontEnd,
    _arch_name: &str,
    max_cycle: u32,
) -> GraphReport {
    let len = max_cycle as usize + 1;
    let mut events: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for name in ["IQ", "IDQ", "Scheduler", "Reorder buffer"] {
        add_series(&mut events, &mut order, name, len);
    }

    for instr_i in &frontend.all_generated_instr_instances {
        add_event(&mut events, "IQ", instr_i.predecoded, 1, max_cycle);
        add_event(&mut events, "IQ", instr_i.removed_from_iq, -1, max_cycle);
        for &lam_idx in &instr_i.laminated_uops {
            let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
                continue;
            };
            add_event(&mut events, "IDQ", lam.added_to_idq, 1, max_cycle);
            for (f_i, &fused_idx) in lam.fused_uop_idxs.iter().enumerate() {
                let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                    continue;
                };
                if f_i == 0 && lam.added_to_idq.is_some() {
                    add_event(&mut events, "IDQ", fused.issued, -1, max_cycle);
                }
                add_event(&mut events, "Reorder buffer", fused.issued, 1, max_cycle);
                add_event(&mut events, "Reorder buffer", fused.retired, -1, max_cycle);
                for &uop_idx in &fused.unfused_uop_idxs {
                    let Some(uop) = frontend.uop_storage.get_uop(uop_idx) else {
                        continue;
                    };
                    if fused.issued != uop.executed {
                        add_event(&mut events, "Scheduler", fused.issued, 1, max_cycle);
                        add_event(&mut events, "Scheduler", uop.dispatched, -1, max_cycle);
                    }
                }
            }
        }
    }

    add_series(&mut events, &mut order, "Instr. predecoded", len);
    for instr_i in &frontend.all_generated_instr_instances {
        add_event(
            &mut events,
            "Instr. predecoded",
            instr_i.predecoded,
            1,
            max_cycle,
        );
    }

    add_series(&mut events, &mut order, "&mu;ops added to IDQ", len);
    for instr_i in &frontend.all_generated_instr_instances {
        for &lam_idx in &instr_i.laminated_uops {
            let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
                continue;
            };
            add_event(
                &mut events,
                "&mu;ops added to IDQ",
                lam.added_to_idq,
                1,
                max_cycle,
            );
        }
    }

    for name in ["&mu;ops issued", "&mu;ops retired"] {
        add_series(&mut events, &mut order, name, len);
    }
    for instr_i in &frontend.all_generated_instr_instances {
        for &lam_idx in &instr_i.laminated_uops {
            let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
                continue;
            };
            for &fused_idx in &lam.fused_uop_idxs {
                let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                    continue;
                };
                add_event(&mut events, "&mu;ops issued", fused.issued, 1, max_cycle);
                add_event(&mut events, "&mu;ops retired", fused.retired, 1, max_cycle);
            }
        }
    }

    for name in ["&mu;ops dispatched", "&mu;ops executed"] {
        add_series(&mut events, &mut order, name, len);
    }
    for instr_i in &frontend.all_generated_instr_instances {
        for &lam_idx in &instr_i.laminated_uops {
            let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
                continue;
            };
            for &fused_idx in &lam.fused_uop_idxs {
                let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                    continue;
                };
                for &uop_idx in &fused.unfused_uop_idxs {
                    let Some(uop) = frontend.uop_storage.get_uop(uop_idx) else {
                        continue;
                    };
                    add_event(
                        &mut events,
                        "&mu;ops dispatched",
                        uop.dispatched,
                        1,
                        max_cycle,
                    );
                    add_event(&mut events, "&mu;ops executed", uop.executed, 1, max_cycle);
                }
            }
        }
    }

    for port in &frontend.pack.all_ports {
        let name = format!("&mu;ops port {port}");
        add_series(&mut events, &mut order, &name, len);
    }
    for instr_i in &frontend.all_generated_instr_instances {
        for &lam_idx in &instr_i.laminated_uops {
            let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
                continue;
            };
            for &fused_idx in &lam.fused_uop_idxs {
                let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                    continue;
                };
                for &uop_idx in &fused.unfused_uop_idxs {
                    let Some(uop) = frontend.uop_storage.get_uop(uop_idx) else {
                        continue;
                    };
                    if let Some(port) = &uop.actual_port {
                        let name = format!("&mu;ops port {port}");
                        add_event(&mut events, &name, uop.dispatched, 1, max_cycle);
                    }
                }
            }
        }
    }

    GraphReport {
        series: order
            .into_iter()
            .map(|name| GraphSeries {
                y: cumulative(events.remove(&name).unwrap_or_else(|| vec![0; len])),
                name,
                mode: "lines+markers".to_string(),
                line_shape: "hv".to_string(),
            })
            .collect(),
        interpolation_toggle: true,
    }
}

fn add_series(
    events: &mut BTreeMap<String, Vec<i64>>,
    order: &mut Vec<String>,
    name: &str,
    len: usize,
) {
    if !events.contains_key(name) {
        events.insert(name.to_string(), vec![0; len]);
        order.push(name.to_string());
    }
}

fn add_event(
    events: &mut BTreeMap<String, Vec<i64>>,
    name: &str,
    cycle: Option<u32>,
    value: i64,
    max_cycle: u32,
) {
    let Some(cycle) = cycle else {
        return;
    };
    if cycle <= max_cycle {
        if let Some(series) = events.get_mut(name) {
            series[cycle as usize] += value;
        }
    }
}

fn cumulative(mut deltas: Vec<i64>) -> Vec<i64> {
    for i in 1..deltas.len() {
        let prev = deltas[i - 1];
        deltas[i] += prev;
    }
    deltas
}

#[cfg(test)]
pub fn cumulative_for_test(deltas: Vec<i64>) -> Vec<i64> {
    cumulative(deltas)
}

pub fn build_trace_report(
    frontend: &crate::sim::FrontEnd,
    last_relevant_round: u32,
    max_cycle: u32,
) -> TraceReport {
    let mut table_data = Vec::<Vec<TraceInstructionRow>>::new();
    let mut prev_rnd: Option<u32> = None;
    let mut prev_instr_predecoded: Option<u32> = None;

    for instr_i in &frontend.all_generated_instr_instances {
        if prev_rnd != Some(instr_i.rnd) {
            prev_rnd = Some(instr_i.rnd);
            if instr_i.rnd > last_relevant_round {
                break;
            }
            table_data.push(Vec::new());
        }

        let predecoded = if instr_i.macro_fused_with_prev_instr {
            prev_instr_predecoded
        } else {
            instr_i.predecoded
        };

        if !instr_i.reg_merge_uops.is_empty() {
            table_data
                .last_mut()
                .unwrap()
                .push(build_trace_instruction_row(
                    "&lt;Register Merge Uop&gt;".to_string(),
                    &instr_i.reg_merge_uops,
                    None,
                    frontend,
                    max_cycle,
                ));
        }
        if !instr_i.stack_sync_uops.is_empty() {
            table_data
                .last_mut()
                .unwrap()
                .push(build_trace_instruction_row(
                    "&lt;Stack Sync Uop&gt;".to_string(),
                    &instr_i.stack_sync_uops,
                    None,
                    frontend,
                    max_cycle,
                ));
        }

        let display = if instr_i.rnd == 0 && !instr_i.instr_str.is_empty() {
            format!(
                "<a href=\"{}\" target=\"_blank\">{}</a>",
                uops_info_url(&instr_i.instr_str),
                instr_i.disasm
            )
        } else {
            instr_i.disasm.clone()
        };
        table_data
            .last_mut()
            .unwrap()
            .push(build_trace_instruction_row(
                display,
                &instr_i.laminated_uops,
                predecoded,
                frontend,
                max_cycle,
            ));

        prev_instr_predecoded = instr_i.predecoded;
    }

    TraceReport { table_data }
}

fn build_trace_instruction_row(
    display: String,
    lam_idxs: &[u64],
    predecoded: Option<u32>,
    frontend: &crate::sim::FrontEnd,
    max_cycle: u32,
) -> TraceInstructionRow {
    let mut row = TraceInstructionRow {
        display,
        uops: Vec::new(),
    };
    if lam_idxs.is_empty() {
        let mut events = BTreeMap::new();
        insert_no_uop_predecode_event(&mut events, predecoded);
        row.uops.push(TraceUopRow {
            possible_ports: "-".to_string(),
            actual_port: "-".to_string(),
            events,
        });
        return row;
    }

    for &lam_idx in lam_idxs {
        let lam = frontend
            .uop_storage
            .get_laminated_uop(lam_idx)
            .expect("laminated uop index should exist in trace storage");
        for &fused_idx in &lam.fused_uop_idxs {
            let fused = frontend
                .uop_storage
                .get_fused_uop(fused_idx)
                .expect("fused uop index should exist in trace storage");
            for &uop_idx in &fused.unfused_uop_idxs {
                let uop = frontend
                    .uop_storage
                    .get_uop(uop_idx)
                    .expect("uop index should exist in trace storage");
                let mut events = BTreeMap::new();
                insert_event(&mut events, predecoded, "P", max_cycle);
                insert_event(&mut events, lam.added_to_idq, "Q", max_cycle);
                insert_event(&mut events, fused.issued, "I", max_cycle);
                insert_event(&mut events, uop.ready_for_dispatch, "r", max_cycle);
                insert_event(&mut events, uop.dispatched, "D", max_cycle);
                insert_event(&mut events, uop.executed, "E", max_cycle);
                insert_event(&mut events, fused.retired, "R", max_cycle);
                row.uops.push(TraceUopRow {
                    possible_ports: format_possible_ports(&uop.prop.possible_ports),
                    actual_port: uop.actual_port.clone().unwrap_or_else(|| "-".to_string()),
                    events,
                });
            }
        }
    }
    row
}

fn insert_event(
    events: &mut BTreeMap<u32, String>,
    cycle: Option<u32>,
    label: &str,
    max_cycle: u32,
) {
    if let Some(cycle) = cycle {
        if cycle <= max_cycle {
            events.insert(cycle, label.to_string());
        }
    }
}

fn insert_no_uop_predecode_event(events: &mut BTreeMap<u32, String>, cycle: Option<u32>) {
    if let Some(cycle) = cycle {
        if cycle != 0 {
            events.insert(cycle, "P".to_string());
        }
    }
}

fn format_possible_ports(ports: &[String]) -> String {
    if ports.is_empty() {
        "-".to_string()
    } else {
        format!("{{{}}}", ports.join(","))
    }
}

fn uops_info_url(instr_str: &str) -> String {
    let mut canonical = String::new();
    let mut prev_was_separator = false;
    for ch in instr_str.chars() {
        if matches!(ch, '(' | ')' | '{' | '}' | ',' | ' ') {
            if !canonical.is_empty() && !prev_was_separator {
                canonical.push('_');
            }
            prev_was_separator = true;
        } else {
            canonical.push(ch);
            prev_was_separator = false;
        }
    }
    let canonical = canonical.trim_matches('_');
    format!("https://www.uops.info/html-instr/{canonical}.html")
}

#[cfg(test)]
pub fn extract_trace_table_data_for_test(
    html: &str,
) -> Result<Vec<Vec<uica_model::TraceInstructionRow>>, String> {
    let marker = "var tableData = ";
    let start = html
        .find(marker)
        .ok_or_else(|| "missing tableData marker".to_string())?
        + marker.len();
    let end = html[start..]
        .find("\n\nvar eventToColor")
        .ok_or_else(|| "missing eventToColor marker".to_string())?
        + start;
    serde_json::from_str(&html[start..end])
        .map_err(|err| format!("failed to parse tableData: {err}"))
}

#[cfg(test)]
mod tests {
    use super::{
        build_graph_report, build_trace_report, render_graph_html, render_trace_html, uops_info_url,
    };
    use std::collections::BTreeMap;
    use uica_data::{DataPack, DATAPACK_SCHEMA_VERSION};
    use uica_model::{GraphReport, GraphSeries, TraceInstructionRow, TraceReport, TraceUopRow};

    fn empty_pack() -> DataPack {
        DataPack {
            schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
            all_ports: vec!["0".to_string()],
            alu_ports: vec!["0".to_string()],
            instructions: Vec::new(),
        }
    }

    fn trace_frontend_with(instances: Vec<crate::sim::InstrInstance>) -> crate::sim::FrontEnd {
        let arch = crate::get_micro_arch("SKL").expect("SKL config should exist");
        let pack = empty_pack();
        let mut frontend = crate::sim::FrontEnd::new(arch, true, Vec::new(), 0, &pack);
        frontend.all_generated_instr_instances = instances;
        frontend
    }

    fn no_uop_instr(predecoded: Option<u32>) -> crate::sim::InstrInstance {
        let mut instr =
            crate::sim::InstrInstance::new(0, 0, 0, 0, 1, "NOP".to_string(), "nop".to_string());
        instr.uops_mite = 0;
        instr.retire_slots = 0;
        instr.predecoded = predecoded;
        instr
    }

    fn add_lam_fixture(
        frontend: &mut crate::sim::FrontEnd,
        lam_idx: u64,
        dispatched: u32,
        executed: u32,
    ) {
        let fused_idx = lam_idx + 1000;
        let uop_idx = lam_idx + 2000;
        frontend
            .uop_storage
            .add_laminated_uop(crate::sim::LaminatedUop {
                idx: lam_idx,
                fused_uop_idxs: vec![fused_idx],
                added_to_idq: Some(0),
                uop_source: None,
                instr_instance_idx: 0,
            });
        frontend.uop_storage.add_fused_uop(crate::sim::FusedUop {
            idx: fused_idx,
            unfused_uop_idxs: vec![uop_idx],
            laminated_uop_idx: Some(lam_idx),
            issued: Some(0),
            retired: Some(5),
            retire_idx: None,
        });
        frontend.uop_storage.add_uop(crate::sim::Uop {
            idx: uop_idx,
            queue_idx: uop_idx,
            prop: crate::sim::UopProperties::default(),
            actual_port: Some("0".to_string()),
            eliminated: false,
            ready_for_dispatch: Some(dispatched),
            dispatched: Some(dispatched),
            executed: Some(executed),
            lat_reduced_due_to_fast_ptr_chasing: false,
            renamed_input_operands: Vec::new(),
            renamed_output_operands: Vec::new(),
            store_buffer_entry: None,
            fused_uop_idx: Some(fused_idx),
            instr_instance_idx: 0,
        });
    }

    fn graph_y<'a>(report: &'a GraphReport, name: &str) -> &'a [i64] {
        &report
            .series
            .iter()
            .find(|series| series.name == name)
            .expect("graph series should exist")
            .y
    }

    #[test]
    fn graph_event_series_exclude_special_uops() {
        let mut instr = no_uop_instr(None);
        instr.laminated_uops = vec![12];
        instr.reg_merge_uops = vec![10];
        instr.stack_sync_uops = vec![11];
        let mut frontend = trace_frontend_with(vec![instr]);
        add_lam_fixture(&mut frontend, 10, 1, 1);
        add_lam_fixture(&mut frontend, 11, 1, 1);
        add_lam_fixture(&mut frontend, 12, 2, 3);

        let report = build_graph_report(&frontend, "SKL", 5);

        assert_eq!(graph_y(&report, "&mu;ops dispatched"), &[0, 0, 1, 1, 1, 1]);
        assert_eq!(graph_y(&report, "&mu;ops executed"), &[0, 0, 0, 1, 1, 1]);
        assert_eq!(graph_y(&report, "&mu;ops port 0"), &[0, 0, 1, 1, 1, 1]);
    }

    #[test]
    fn trace_report_omits_zero_predecode_for_no_uop_instr() {
        let frontend = trace_frontend_with(vec![no_uop_instr(Some(0))]);

        let report = build_trace_report(&frontend, 0, 4);

        assert!(report.table_data[0][0].uops[0].events.is_empty());
    }

    #[test]
    fn trace_report_keeps_late_predecode_for_no_uop_instr() {
        let frontend = trace_frontend_with(vec![no_uop_instr(Some(9))]);

        let report = build_trace_report(&frontend, 0, 4);

        assert_eq!(
            report.table_data[0][0].uops[0].events,
            BTreeMap::from([(9, "P".to_string())])
        );
    }

    #[test]
    fn trace_report_links_round_zero_instr_to_uops_info() {
        let mut instr = no_uop_instr(None);
        instr.mnemonic = "ADD".to_string();
        instr.disasm = "add qword ptr [rax], rbx".to_string();
        instr.instr_str = "ADD (M64, R64)".to_string();
        let frontend = trace_frontend_with(vec![instr]);

        let report = build_trace_report(&frontend, 0, 4);

        assert!(report.table_data[0][0]
            .display
            .contains("href=\"https://www.uops.info/html-instr/ADD_M64_R64.html\""));
    }

    #[test]
    fn uops_info_url_collapses_separators() {
        assert_eq!(
            uops_info_url("ADD (M64, R64)"),
            "https://www.uops.info/html-instr/ADD_M64_R64.html"
        );
        assert_eq!(
            uops_info_url("  VPGATHERDD {K1}, (ZMM0)  "),
            "https://www.uops.info/html-instr/VPGATHERDD_K1_ZMM0.html"
        );
        assert_eq!(
            uops_info_url("_ADD"),
            "https://www.uops.info/html-instr/ADD.html"
        );
        assert_eq!(
            uops_info_url("__ADD__"),
            "https://www.uops.info/html-instr/ADD.html"
        );
    }

    #[test]
    fn trace_renderer_injects_table_data() {
        let report = TraceReport {
            table_data: vec![vec![TraceInstructionRow {
                display: "add rax, rbx".to_string(),
                uops: vec![TraceUopRow {
                    possible_ports: "{0}".to_string(),
                    actual_port: "0".to_string(),
                    events: BTreeMap::from([(1, "D".to_string())]),
                }],
            }]],
        };

        let html = render_trace_html(&report).expect("trace html should render");

        assert!(html.contains("<h1>Execution Trace</h1>"));
        assert!(html.contains("var tableData = [[{"));
        assert!(html.contains("\"possiblePorts\":\"{0}\""));
        assert!(html.contains("\"1\":\"D\""));
    }

    #[test]
    fn trace_report_data_contains_python_event_letters() {
        let mut report = TraceReport::default();
        report.table_data.push(vec![TraceInstructionRow {
            display: "add rax, rbx".to_string(),
            uops: vec![TraceUopRow {
                possible_ports: "{0}".to_string(),
                actual_port: "0".to_string(),
                events: BTreeMap::from([
                    (0, "P".to_string()),
                    (1, "Q".to_string()),
                    (2, "I".to_string()),
                    (3, "r".to_string()),
                    (4, "D".to_string()),
                    (5, "E".to_string()),
                    (6, "R".to_string()),
                ]),
            }],
        }]);

        let html = render_trace_html(&report).unwrap();
        let normalized = crate::report::extract_trace_table_data_for_test(&html).unwrap();
        assert_eq!(normalized, report.table_data);
    }

    #[test]
    fn cumulative_series_sums_deltas() {
        let values = crate::report::cumulative_for_test(vec![0, 1, 0, -1, 2]);
        assert_eq!(values, vec![0, 1, 1, 0, 2]);
    }

    #[test]
    fn graph_renderer_emits_plotly_series() {
        let report = GraphReport {
            series: vec![GraphSeries {
                name: "IQ".to_string(),
                y: vec![0, 1, 1],
                mode: "lines+markers".to_string(),
                line_shape: "hv".to_string(),
            }],
            interpolation_toggle: true,
        };

        let html = render_graph_html(&report).expect("graph html should render");

        assert!(html.contains("Plotly.newPlot"));
        assert!(html.contains("\"name\":\"IQ\""));
        assert!(html.contains("\"y\":[0,1,1]"));
        assert!(html.contains("\"shape\":\"hv\""));
        assert!(html.contains("Toggle interpolation mode"));
    }

    #[test]
    fn trace_renderer_escapes_script_sensitive_json() {
        let report = TraceReport {
            table_data: vec![vec![TraceInstructionRow {
                display: "</script><script>alert(1)</script>".to_string(),
                uops: Vec::new(),
            }]],
        };

        let html = render_trace_html(&report).expect("trace html should render");

        assert!(!html.contains("</script><script>"));
        assert!(
            html.contains("\\u003c/script\\u003e\\u003cscript\\u003ealert(1)\\u003c/script\\u003e")
        );
    }

    #[test]
    fn graph_renderer_escapes_script_sensitive_json() {
        let report = GraphReport {
            series: vec![GraphSeries {
                name: "</script><script>alert(1)</script>".to_string(),
                y: vec![0],
                mode: "lines".to_string(),
                line_shape: "linear".to_string(),
            }],
            interpolation_toggle: true,
        };

        let html = render_graph_html(&report).expect("graph html should render");

        assert!(!html.contains("</script><script>"));
        assert!(
            html.contains("\\u003c/script\\u003e\\u003cscript\\u003ealert(1)\\u003c/script\\u003e")
        );
    }

    #[test]
    fn graph_renderer_omits_toggle_when_disabled() {
        let report = GraphReport {
            series: vec![GraphSeries {
                name: "IQ".to_string(),
                y: vec![0],
                mode: "lines".to_string(),
                line_shape: "linear".to_string(),
            }],
            interpolation_toggle: false,
        };

        let html = render_graph_html(&report).expect("graph html should render");

        assert!(!html.contains("modeBarButtonsToAdd"));
        assert!(!html.contains("config.modeBarButtonsToAdd[0]"));
        assert!(!html.contains("toggleInterpolation"));
    }

    #[test]
    fn empty_graph_renderer_omits_toggle_handler() {
        let report = GraphReport {
            series: Vec::new(),
            interpolation_toggle: true,
        };

        let html = render_graph_html(&report).expect("graph html should render");

        assert!(html.contains("const data = [];"));
        assert!(!html.contains("modeBarButtonsToAdd"));
        assert!(!html.contains("gd.data[0]"));
    }
}
