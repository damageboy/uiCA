use serde_json::{json, Value};
use std::collections::BTreeMap;
use uica_model::{
    GraphReport, GraphSeries, RegularColumnKind, RegularOutputMetrics, RegularOutputReport,
    RegularOutputRow, TraceInstructionRow, TraceReport, TraceUopRow,
};

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

pub fn render_regular_text(report: &RegularOutputReport) -> String {
    let mut out = String::new();
    if let Some(throughput) = report.throughput_cycles_per_iteration {
        out.push_str(&format!(
            "Throughput (in cycles per iteration): {throughput:.2}\n"
        ));
    }
    out.push_str(&bottleneck_line(&report.bottlenecks));
    out.push('\n');

    if !report.limit_lines.is_empty() {
        out.push_str(
            "\nThe following throughputs could be achieved if the given property were the only bottleneck:\n\n",
        );
        for limit in &report.limit_lines {
            out.push_str(&format!("  - {}: {:.2}\n", limit.label, limit.throughput));
        }
    }

    if !report.notes.is_empty() {
        out.push('\n');
        for note in &report.notes {
            out.push_str(&format!("{} - {}\n", note.key, note.label));
        }
    }

    out.push('\n');
    out.push_str(&render_regular_table(report));
    out
}

pub fn render_regular_html(report: &RegularOutputReport) -> Result<String, String> {
    let mut html = String::new();
    html.push_str(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>uiCA Analysis</title>",
    );
    html.push_str("<style>body{font-family:system-ui,sans-serif;margin:1rem;color:#111;background:#fff}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}th,td{border:1px solid #ccd;padding:.35rem .5rem;text-align:right}th:last-child,td:last-child{text-align:left}.notes{text-align:center}.summary{margin-bottom:1rem}@media(prefers-color-scheme:dark){body{color:#eee;background:#111}th,td{border-color:#445}}</style>");
    html.push_str("</head><body>");
    html.push_str("<section class=\"summary\">");
    if let Some(tp) = report.throughput_cycles_per_iteration {
        html.push_str(&format!(
            "<p><strong>Throughput:</strong> {:.2} cycles/iteration</p>",
            tp
        ));
    }
    html.push_str(&format!(
        "<p><strong>{}</strong></p>",
        escape_html(&bottleneck_line(&report.bottlenecks))
    ));
    html.push_str("</section>");

    if !report.limit_lines.is_empty() {
        html.push_str("<section class=\"limits\"><p>The following throughputs could be achieved if the given property were the only bottleneck:</p><ul>");
        for limit in &report.limit_lines {
            html.push_str(&format!(
                "<li>{}: {:.2}</li>",
                escape_html(&limit.label),
                limit.throughput
            ));
        }
        html.push_str("</ul></section>");
    }

    if !report.notes.is_empty() {
        html.push_str("<section class=\"notes\"><ul>");
        for note in &report.notes {
            html.push_str(&format!(
                "<li><strong>{}</strong> - {}</li>",
                escape_html(&note.key),
                escape_html(&note.label)
            ));
        }
        html.push_str("</ul></section>");
    }

    html.push_str("<table class=\"regular-output\"><thead><tr>");
    for column in &report.columns {
        html.push_str(&format!(
            "<th scope=\"col\">{}</th>",
            escape_html(&column.label)
        ));
    }
    html.push_str("<th scope=\"col\">Instruction</th></tr></thead><tbody>");
    for row in &report.rows {
        html.push_str(&format!(
            "<tr class=\"regular-row regular-row-{}\"",
            row_kind_class(&row.kind)
        ));
        if let Some(instr_id) = row.instr_id {
            html.push_str(&format!(" data-instr-id=\"{}\"", instr_id));
        }
        html.push('>');
        html.push_str(&regular_html_row_cells(row, report));
        html.push_str("<th scope=\"row\">");
        if let Some(url) = &row.url {
            html.push_str(&format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noreferrer\">{}</a>",
                escape_html(url),
                escape_html(&row.asm)
            ));
        } else {
            html.push_str(&escape_html(&row.asm));
        }
        html.push_str("</th></tr>");
    }
    html.push_str("</tbody><tfoot><tr>");
    html.push_str(&regular_html_metric_cells(&report.totals, None, report));
    html.push_str("<th scope=\"row\">Total</th></tr></tfoot></table></body></html>");
    Ok(html)
}

fn regular_html_row_cells(row: &RegularOutputRow, report: &RegularOutputReport) -> String {
    regular_html_metric_cells(&row.metrics, Some(&row.notes), report)
}

fn regular_html_metric_cells(
    metrics: &RegularOutputMetrics,
    notes: Option<&[String]>,
    report: &RegularOutputReport,
) -> String {
    let mut html = String::new();
    for column in &report.columns {
        let cell = match column.kind {
            RegularColumnKind::Notes => notes.map(|notes| notes.join("")).unwrap_or_default(),
            _ => regular_metric_cell(metrics, &column.key, &column.kind),
        };
        if matches!(column.kind, RegularColumnKind::Port) {
            let port = column
                .key
                .strip_prefix("port_")
                .unwrap_or(column.key.as_str());
            html.push_str(&format!(
                "<td data-port=\"{}\">{}</td>",
                escape_html(port),
                escape_html(&cell)
            ));
        } else {
            html.push_str(&format!("<td>{}</td>", escape_html(&cell)));
        }
    }
    html
}

fn row_kind_class(kind: &uica_model::RegularRowKind) -> &'static str {
    match kind {
        uica_model::RegularRowKind::Instruction => "instruction",
        uica_model::RegularRowKind::RegisterMerge => "register-merge",
        uica_model::RegularRowKind::StackSync => "stack-sync",
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn bottleneck_line(bottlenecks: &[String]) -> String {
    match bottlenecks.len() {
        0 => "Bottleneck: unknown".to_string(),
        1 => format!("Bottleneck: {}", bottlenecks[0]),
        _ => format!("Bottlenecks: {}", bottlenecks.join(", ")),
    }
}

fn format_regular_cell(value: f64) -> String {
    if value.abs() < 0.005 {
        return String::new();
    }
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn render_regular_table(report: &RegularOutputReport) -> String {
    let mut widths: Vec<usize> = report
        .columns
        .iter()
        .map(|column| column.label.len().max(5))
        .collect();
    let mut header_cells: Vec<String> = report
        .columns
        .iter()
        .map(|column| column.label.clone())
        .collect();
    header_cells.push(String::new());
    widths.push("Total".len());

    let mut rendered_rows = Vec::new();
    for row in &report.rows {
        let mut cells = regular_row_cells(row, report);
        cells.push(row.asm.clone());
        for (idx, cell) in cells.iter().enumerate() {
            widths[idx] = widths[idx].max(cell.chars().count());
        }
        rendered_rows.push(cells);
    }

    let mut total_cells = regular_metric_cells(&report.totals, report);
    total_cells.push("Total".to_string());
    for (idx, cell) in total_cells.iter().enumerate() {
        widths[idx] = widths[idx].max(cell.chars().count());
    }

    let mut lines = Vec::new();
    lines.push(join_regular_table_row(&header_cells, &widths));
    lines.push(join_regular_separator(&widths));
    for cells in rendered_rows {
        lines.push(join_regular_table_row(&cells, &widths));
    }
    lines.push(join_regular_separator(&widths));
    lines.push(join_regular_table_row(&total_cells, &widths));
    lines.push(String::new());
    lines.join("\n")
}

fn regular_row_cells(row: &RegularOutputRow, report: &RegularOutputReport) -> Vec<String> {
    let mut cells = Vec::new();
    for column in &report.columns {
        match column.kind {
            RegularColumnKind::Notes => cells.push(row.notes.join("")),
            _ => cells.push(regular_metric_cell(&row.metrics, &column.key, &column.kind)),
        }
    }
    cells
}

fn regular_metric_cells(
    metrics: &RegularOutputMetrics,
    report: &RegularOutputReport,
) -> Vec<String> {
    report
        .columns
        .iter()
        .map(|column| match column.kind {
            RegularColumnKind::Notes => String::new(),
            _ => regular_metric_cell(metrics, &column.key, &column.kind),
        })
        .collect()
}

fn regular_metric_cell(
    metrics: &RegularOutputMetrics,
    key: &str,
    kind: &RegularColumnKind,
) -> String {
    match kind {
        RegularColumnKind::Frontend => match key {
            "mite" => format_regular_cell(metrics.mite),
            "ms" => format_regular_cell(metrics.ms),
            "dsb" => format_regular_cell(metrics.dsb),
            "lsd" => format_regular_cell(metrics.lsd),
            _ => String::new(),
        },
        RegularColumnKind::Issue => format_regular_cell(metrics.issued),
        RegularColumnKind::Execute => format_regular_cell(metrics.executed),
        RegularColumnKind::Port => {
            let port = key.strip_prefix("port_").unwrap_or(key);
            format_regular_cell(*metrics.ports.get(port).unwrap_or(&0.0))
        }
        RegularColumnKind::Divider => format_regular_cell(metrics.div),
        RegularColumnKind::Notes => String::new(),
    }
}

fn join_regular_table_row(cells: &[String], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(idx, cell)| format!(" {:>width$} ", cell, width = widths[idx]))
        .collect::<Vec<_>>()
        .join("│")
}

fn join_regular_separator(widths: &[usize]) -> String {
    widths
        .iter()
        .map(|width| "─".repeat(width + 2))
        .collect::<Vec<_>>()
        .join("┼")
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

    for port in &frontend.all_ports {
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
            instr_i.disasm.to_string()
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
        build_graph_report, build_trace_report, format_regular_cell, render_graph_html,
        render_regular_html, render_regular_text, render_trace_html, uops_info_url,
    };
    use std::collections::BTreeMap;
    use uica_data::{
        encode_uipack, DataPack as UiPackFixture, MappedUiPackRuntime, DATAPACK_SCHEMA_VERSION,
    };
    use uica_model::{GraphReport, GraphSeries, TraceInstructionRow, TraceReport, TraceUopRow};

    #[test]
    fn regular_text_formats_blank_zero_and_trimmed_floats() {
        assert_eq!(format_regular_cell(0.0), "");
        assert_eq!(format_regular_cell(1.0), "1");
        assert_eq!(format_regular_cell(0.5), "0.5");
        assert_eq!(format_regular_cell(0.22), "0.22");
    }

    #[test]
    fn regular_text_renders_summary_notes_and_instruction() {
        use uica_model::{
            RegularColumn, RegularColumnKind, RegularNote, RegularOutputMetrics,
            RegularOutputReport, RegularOutputRow, RegularRowKind,
        };

        let report = RegularOutputReport {
            arch: "SKL".to_string(),
            throughput_cycles_per_iteration: Some(2.0),
            bottlenecks: vec!["Dependencies".to_string()],
            columns: vec![
                RegularColumn {
                    key: "dsb".to_string(),
                    label: "DSB".to_string(),
                    kind: RegularColumnKind::Frontend,
                },
                RegularColumn {
                    key: "issued".to_string(),
                    label: "Issued".to_string(),
                    kind: RegularColumnKind::Issue,
                },
                RegularColumn {
                    key: "port_0".to_string(),
                    label: "Port 0".to_string(),
                    kind: RegularColumnKind::Port,
                },
                RegularColumn {
                    key: "notes".to_string(),
                    label: "Notes".to_string(),
                    kind: RegularColumnKind::Notes,
                },
            ],
            rows: vec![RegularOutputRow {
                row_id: "instr-0".to_string(),
                kind: RegularRowKind::Instruction,
                instr_id: Some(0),
                asm: "add rax, rbx".to_string(),
                opcode: Some("4801D8".to_string()),
                url: None,
                notes: vec!["M".to_string()],
                metrics: RegularOutputMetrics {
                    dsb: 1.0,
                    issued: 1.0,
                    ports: [("0".to_string(), 0.5)].into_iter().collect(),
                    ..RegularOutputMetrics::default()
                },
            }],
            notes: vec![RegularNote {
                key: "M".to_string(),
                label: "Macro-fused with previous instruction".to_string(),
                url: None,
            }],
            ..RegularOutputReport::default()
        };

        let text = render_regular_text(&report);
        assert!(text.contains("Throughput (in cycles per iteration): 2.00"));
        assert!(text.contains("Bottleneck: Dependencies"));
        assert!(text.contains("M - Macro-fused with previous instruction"));
        assert!(text.contains("add rax, rbx"));
        assert!(text.contains("0.5"));
    }

    #[test]
    fn regular_html_escapes_instruction_text() {
        use uica_model::{
            RegularColumn, RegularColumnKind, RegularOutputMetrics, RegularOutputReport,
            RegularOutputRow, RegularRowKind,
        };

        let report = RegularOutputReport {
            columns: vec![
                RegularColumn {
                    key: "issued".to_string(),
                    label: "Issued".to_string(),
                    kind: RegularColumnKind::Issue,
                },
                RegularColumn {
                    key: "port_2&amp;3".to_string(),
                    label: "Port 2&3".to_string(),
                    kind: RegularColumnKind::Port,
                },
            ],
            rows: vec![RegularOutputRow {
                row_id: "instr-0".to_string(),
                kind: RegularRowKind::Instruction,
                instr_id: Some(0),
                asm: "cmp rax, <bad>&\"".to_string(),
                metrics: RegularOutputMetrics {
                    ports: [("2&amp;3".to_string(), 0.5)].into_iter().collect(),
                    ..RegularOutputMetrics::default()
                },
                ..RegularOutputRow::default()
            }],
            ..RegularOutputReport::default()
        };

        let html = render_regular_html(&report).unwrap();
        assert!(html.contains("&lt;bad&gt;&amp;&quot;"));
        assert!(html.contains("data-port=\"2&amp;amp;3\""));
        assert!(html.contains("<table"));
        assert!(!html.contains("<script src="));
    }

    #[test]
    fn regular_html_renders_limit_lines() {
        use uica_model::{RegularLimitLine, RegularOutputReport};

        let report = RegularOutputReport {
            limit_lines: vec![RegularLimitLine {
                key: "dsb".to_string(),
                label: "DSB & Decode".to_string(),
                throughput: 1.25,
                is_bottleneck: false,
            }],
            ..RegularOutputReport::default()
        };

        let html = render_regular_html(&report).unwrap();
        assert!(html.contains("<section class=\"limits\">"));
        assert!(html.contains("DSB &amp; Decode: 1.25"));
        assert!(!html.contains("<script src="));
    }

    fn empty_fixture() -> UiPackFixture {
        UiPackFixture {
            schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
            all_ports: vec!["0".to_string()],
            alu_ports: vec!["0".to_string()],
            instructions: Vec::new(),
        }
    }

    fn trace_frontend_with(
        instances: Vec<crate::sim::InstrInstance>,
    ) -> crate::sim::FrontEnd<'static> {
        let arch = crate::get_micro_arch("SKL").expect("SKL config should exist");
        let runtime = Box::leak(Box::new(
            MappedUiPackRuntime::from_bytes(encode_uipack(&empty_fixture(), "SKL").unwrap())
                .unwrap(),
        ));
        let mut frontend = crate::sim::FrontEnd::new_with_runtime(
            arch,
            true,
            Vec::new(),
            0,
            runtime,
            "diff",
            false,
            false,
            false,
        )
        .unwrap();
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
        instr.mnemonic = "ADD".into();
        instr.disasm = "add qword ptr [rax], rbx".into();
        instr.instr_str = "ADD (M64, R64)".into();
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
    fn trace_renderer_uses_first_party_dual_range_slider() {
        let report = TraceReport {
            table_data: vec![vec![TraceInstructionRow {
                display: "add rax, rbx".to_string(),
                uops: Vec::new(),
            }]],
        };

        let html = render_trace_html(&report).expect("trace html should render");

        assert!(html.contains("id=\"firstItRange\" type=\"range\""));
        assert!(html.contains("id=\"lastItRange\" type=\"range\""));
        assert!(html.contains("class=\"range-track\""));
        assert!(html.contains("function syncRangeControls"));
        assert!(html.contains("min-width: 1280px;"));
        assert!(html.contains("--range-input-top: 9px;"));
        assert!(html.contains("width: min(900px, 100%);"));
        assert!(!html.contains("https://ajax.googleapis.com"));
        assert!(!html.contains("cdnjs.cloudflare.com"));
        assert!(!html.contains("noUiSlider"));
        assert!(!html.contains("wNumb"));
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
