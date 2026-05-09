# Regular Output Report Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Rust structured regular-output reports that render Python-like CLI text and web-ready HTML from one model.

**Architecture:** Add presentation-neutral report structs to `uica-model`, collect metrics from simulator steady-state data in `uica-core::engine`, and render text/HTML in `uica-core::report`. CLI consumes text renderer; Emscripten envelope exposes text and HTML.

**Tech Stack:** Rust, serde, existing uiCA simulator data, existing CLI and Emscripten crates, existing Python parity fixtures.

---

## File Map

- Modify `rust/uica-model/src/lib.rs`
  - Add `RegularOutputReport`, rows, columns, metrics, notes.
  - Extend `ReportBundle` with `regular`.
  - Add serialization tests.

- Modify `rust/uica-core/src/engine.rs`
  - Build regular report inside `engine_with_decoded_pack_internal` while `frontend` and `uops_for_round` are available.
  - Keep `UopsForRound` private.
  - Add collector helper functions near existing simulation helpers.

- Modify `rust/uica-core/src/report.rs`
  - Add float-cell formatting helper.
  - Add `render_regular_text`.
  - Add `render_regular_html`.
  - Add renderer tests.

- Modify `rust/uica-cli/src/main.rs`
  - Print regular text by default.
  - Preserve `--tp-only` numeric-only behavior.
  - Keep JSON/trace/graph output paths working.

- Modify `rust/uica-emscripten/src/lib.rs`
  - Add `regular_text` and `regular_html` to web envelope.

- Modify `rust/uica-emscripten/tests/run_contract.rs`
  - Assert envelope contains regular output fields.

- Modify `web/main.js`, `web/index.html`, `web/style.css`
  - Add Analysis tab displaying `regular_html` in sandboxed iframe or `regular_text` fallback.

---

## Task 1: Add Model Types

**Files:**

- Modify: `rust/uica-model/src/lib.rs`

- [ ] **Step 1: Add failing serialization test**

Append to `mod report_tests`:

```rust
    #[test]
    fn regular_report_serializes_structured_rows() {
        use super::{
            RegularColumn, RegularColumnKind, RegularNote, RegularOutputMetrics,
            RegularOutputReport, RegularOutputRow, RegularRowKind,
        };

        let mut metrics = RegularOutputMetrics::default();
        metrics.dsb = 1.0;
        metrics.issued = 1.0;
        metrics.executed = 1.0;
        metrics.ports.insert("0".to_string(), 0.5);

        let report = RegularOutputReport {
            arch: "SKL".to_string(),
            throughput_cycles_per_iteration: Some(2.0),
            bottlenecks: vec!["Dependencies".to_string()],
            columns: vec![RegularColumn {
                key: "port_0".to_string(),
                label: "Port 0".to_string(),
                kind: RegularColumnKind::Port,
            }],
            rows: vec![RegularOutputRow {
                row_id: "instr-0".to_string(),
                kind: RegularRowKind::Instruction,
                instr_id: Some(0),
                asm: "add rax, rbx".to_string(),
                opcode: Some("4801D8".to_string()),
                url: Some("https://www.uops.info/html-instr/ADD_R64_R64.html".to_string()),
                notes: Vec::new(),
                metrics,
            }],
            notes: vec![RegularNote {
                key: "M".to_string(),
                label: "Macro-fused with previous instruction".to_string(),
                url: None,
            }],
            ..RegularOutputReport::default()
        };

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["schema_version"], "uica-regular-report-v1");
        assert_eq!(json["arch"], "SKL");
        assert_eq!(json["rows"][0]["kind"], "instruction");
        assert_eq!(json["rows"][0]["metrics"]["ports"]["0"], 0.5);
        assert_eq!(json["columns"][0]["kind"], "port");
    }
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p uica-model regular_report_serializes_structured_rows
```

Expected failure includes unresolved names such as:

```text
cannot find type `RegularOutputReport` in this scope
```

- [ ] **Step 3: Add model structs**

Insert after `ReportBundle` and before `TraceReport`:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularOutputReport {
    pub schema_version: String,
    pub arch: String,
    pub throughput_cycles_per_iteration: Option<f64>,
    pub bottlenecks: Vec<String>,
    pub limits: BTreeMap<String, Option<f64>>,
    pub limit_lines: Vec<RegularLimitLine>,
    pub columns: Vec<RegularColumn>,
    pub rows: Vec<RegularOutputRow>,
    pub totals: RegularOutputMetrics,
    pub notes: Vec<RegularNote>,
}

impl Default for RegularOutputReport {
    fn default() -> Self {
        Self {
            schema_version: "uica-regular-report-v1".to_string(),
            arch: String::new(),
            throughput_cycles_per_iteration: None,
            bottlenecks: Vec::new(),
            limits: BTreeMap::new(),
            limit_lines: Vec::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            totals: RegularOutputMetrics::default(),
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularLimitLine {
    pub key: String,
    pub label: String,
    pub throughput: f64,
    pub is_bottleneck: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegularColumnKind {
    Frontend,
    Issue,
    Execute,
    Port,
    Divider,
    Notes,
}

impl Default for RegularColumnKind {
    fn default() -> Self {
        Self::Frontend
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularColumn {
    pub key: String,
    pub label: String,
    pub kind: RegularColumnKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegularRowKind {
    Instruction,
    RegisterMerge,
    StackSync,
}

impl Default for RegularRowKind {
    fn default() -> Self {
        Self::Instruction
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularOutputRow {
    pub row_id: String,
    pub kind: RegularRowKind,
    pub instr_id: Option<u32>,
    pub asm: String,
    pub opcode: Option<String>,
    pub url: Option<String>,
    pub notes: Vec<String>,
    pub metrics: RegularOutputMetrics,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularOutputMetrics {
    pub mite: f64,
    pub ms: f64,
    pub dsb: f64,
    pub lsd: f64,
    pub issued: f64,
    pub executed: f64,
    pub div: f64,
    pub ports: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularNote {
    pub key: String,
    pub label: String,
    pub url: Option<String>,
}
```

Change `ReportBundle` to:

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportBundle {
    pub trace: TraceReport,
    pub graph: GraphReport,
    pub regular: RegularOutputReport,
}
```

- [ ] **Step 4: Run model tests**

Run:

```bash
cargo test -p uica-model report_tests
```

Expected:

```text
test result: ok.
```

- [ ] **Step 5: Commit**

Run:

```bash
git add rust/uica-model/src/lib.rs
git commit -m "feat: add regular output report model"
```

---

## Task 2: Collect Regular Report Data

**Files:**

- Modify: `rust/uica-core/src/engine.rs`

- [ ] **Step 1: Add failing engine test for simple row metrics**

Add to existing `#[cfg(test)]` module in `engine.rs` or create one near bottom if absent:

```rust
#[test]
fn regular_report_collects_simple_add_row() {
    let bytes = [0x48, 0x01, 0xd8];
    let invocation = uica_model::Invocation {
        arch: "SKL".to_string(),
        min_iterations: 10,
        min_cycles: 80,
        ..uica_model::Invocation::default()
    };

    let output = crate::engine::engine_output(&bytes, &invocation, true, false)
        .expect("engine output should succeed");
    let regular = &output.reports.expect("reports should exist").regular;

    assert_eq!(regular.schema_version, "uica-regular-report-v1");
    assert_eq!(regular.arch, "SKL");
    assert_eq!(regular.rows.len(), 1);
    assert_eq!(regular.rows[0].asm, "add rax, rbx");
    assert!(regular.rows[0].metrics.issued > 0.0);
    assert!(regular.rows[0].metrics.executed > 0.0);
    assert!(regular.totals.issued > 0.0);
}
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p uica-core regular_report_collects_simple_add_row
```

Expected failure:

```text
no field `regular` on type `ReportBundle`
```

If Task 1 already changed `ReportBundle`, expected failure references missing regular construction.

- [ ] **Step 3: Import regular model types**

At top of `engine.rs`, extend imports if present or add:

```rust
use uica_model::{
    RegularColumn, RegularColumnKind, RegularLimitLine, RegularNote, RegularOutputMetrics,
    RegularOutputReport, RegularOutputRow, RegularRowKind,
};
```

- [ ] **Step 4: Build regular report when reports are requested**

Replace report bundle construction:

```rust
            reports = Some(uica_model::ReportBundle { trace, graph });
```

with:

```rust
            let regular = build_regular_report(
                &frontend,
                &uops_for_round,
                &result.summary,
                &normalized_invocation.arch,
                &pack.all_ports,
            );
            reports = Some(uica_model::ReportBundle {
                trace,
                graph,
                regular,
            });
```

- [ ] **Step 5: Add collector helpers**

Insert after `python_relevant_round_window` helper group, before cycles JSON helpers:

```rust
fn build_regular_report(
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
    summary: &uica_model::Summary,
    arch_name: &str,
    all_ports: &[String],
) -> RegularOutputReport {
    let mut report = RegularOutputReport {
        arch: arch_name.to_string(),
        throughput_cycles_per_iteration: summary.throughput_cycles_per_iteration,
        bottlenecks: summary.bottlenecks_predicted.clone(),
        limits: summary.limits.clone(),
        limit_lines: build_regular_limit_lines(summary),
        notes: Vec::new(),
        ..RegularOutputReport::default()
    };

    let Some((first_round, last_round)) = python_relevant_round_window(frontend, uops_for_round) else {
        report.columns = regular_columns(all_ports, false, false);
        return report;
    };
    let denom = (last_round - first_round + 1) as f64;

    let mut instr_ids: Vec<u32> = frontend
        .all_generated_instr_instances
        .iter()
        .filter(|inst| inst.rnd == 0)
        .map(|inst| inst.instr_id)
        .collect();
    instr_ids.sort_unstable();
    instr_ids.dedup();

    let mut any_div = false;
    let mut any_notes = false;

    for instr_id in instr_ids {
        let instances: Vec<&crate::sim::types::InstrInstance> = frontend
            .all_generated_instr_instances
            .iter()
            .filter(|inst| {
                inst.instr_id == instr_id && inst.rnd >= first_round && inst.rnd <= last_round
            })
            .collect();
        let Some(first_instance) = instances.first().copied() else {
            continue;
        };

        if instances.iter().any(|inst| !inst.reg_merge_uops.is_empty()) {
            let mut row = regular_synthetic_row(
                RegularRowKind::RegisterMerge,
                instr_id,
                "<Register Merge Uop>",
            );
            accumulate_regular_row(frontend, &instances, RegularUopSource::RegisterMerge, denom, &mut row.metrics);
            any_div |= row.metrics.div > 0.0;
            report.totals.add_assign(&row.metrics);
            report.rows.push(row);
        }

        if instances.iter().any(|inst| !inst.stack_sync_uops.is_empty()) {
            let mut row = regular_synthetic_row(
                RegularRowKind::StackSync,
                instr_id,
                "<Stack Sync Uop>",
            );
            accumulate_regular_row(frontend, &instances, RegularUopSource::StackSync, denom, &mut row.metrics);
            any_div |= row.metrics.div > 0.0;
            report.totals.add_assign(&row.metrics);
            report.rows.push(row);
        }

        let mut row = RegularOutputRow {
            row_id: format!("instr-{instr_id}"),
            kind: RegularRowKind::Instruction,
            instr_id: Some(instr_id),
            asm: first_instance.disasm.to_string(),
            opcode: Some(first_instance.opcode_hex.to_string()),
            url: Some(instr_url(first_instance.disasm.as_ref())),
            notes: Vec::new(),
            metrics: RegularOutputMetrics::default(),
        };

        if first_instance.macro_fused_with_prev_instr {
            row.notes.push("M".to_string());
        } else {
            accumulate_regular_row(frontend, &instances, RegularUopSource::Instruction, denom, &mut row.metrics);
            any_div |= row.metrics.div > 0.0;
            report.totals.add_assign(&row.metrics);
        }
        if first_instance.cannot_be_in_dsb_due_to_jcc_erratum {
            row.notes.push("J".to_string());
        }
        any_notes |= !row.notes.is_empty();
        report.rows.push(row);
    }

    if any_notes {
        report.notes = regular_note_legend(&report.rows);
    }
    report.columns = regular_columns(all_ports, any_div, any_notes);
    report
}
```

Add supporting helpers:

```rust
#[derive(Clone, Copy)]
enum RegularUopSource {
    Instruction,
    RegisterMerge,
    StackSync,
}

fn regular_synthetic_row(kind: RegularRowKind, instr_id: u32, asm: &str) -> RegularOutputRow {
    let key = match kind {
        RegularRowKind::Instruction => "instr",
        RegularRowKind::RegisterMerge => "reg-merge",
        RegularRowKind::StackSync => "stack-sync",
    };
    RegularOutputRow {
        row_id: format!("{key}-{instr_id}"),
        kind,
        instr_id: Some(instr_id),
        asm: asm.to_string(),
        opcode: None,
        url: None,
        notes: Vec::new(),
        metrics: RegularOutputMetrics::default(),
    }
}

fn accumulate_regular_row(
    frontend: &crate::sim::FrontEnd,
    instances: &[&crate::sim::types::InstrInstance],
    source: RegularUopSource,
    denom: f64,
    metrics: &mut RegularOutputMetrics,
) {
    for inst in instances {
        let lam_idxs: &[u64] = match source {
            RegularUopSource::Instruction => &inst.laminated_uops,
            RegularUopSource::RegisterMerge => &inst.reg_merge_uops,
            RegularUopSource::StackSync => &inst.stack_sync_uops,
        };
        accumulate_laminated_uops(frontend, lam_idxs, metrics);
    }
    metrics.scale(1.0 / denom);
}

fn accumulate_laminated_uops(
    frontend: &crate::sim::FrontEnd,
    lam_idxs: &[u64],
    metrics: &mut RegularOutputMetrics,
) {
    for &lam_idx in lam_idxs {
        let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
            continue;
        };
        match lam.uop_source {
            Some(crate::sim::types::UopSource::Mite) => metrics.mite += 1.0,
            Some(crate::sim::types::UopSource::Ms) => metrics.ms += 1.0,
            Some(crate::sim::types::UopSource::Dsb) => metrics.dsb += 1.0,
            Some(crate::sim::types::UopSource::Lsd) => metrics.lsd += 1.0,
            Some(crate::sim::types::UopSource::Se) | None => {}
        }
        for &fused_idx in &lam.fused_uop_idxs {
            let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                continue;
            };
            metrics.issued += 1.0;
            for &uop_idx in &fused.unfused_uop_idxs {
                let Some(uop) = frontend.uop_storage.get_uop(uop_idx) else {
                    continue;
                };
                if let Some(port) = &uop.actual_port {
                    metrics.executed += 1.0;
                    *metrics.ports.entry(port.clone()).or_insert(0.0) += 1.0;
                }
                metrics.div += f64::from(uop.prop.div_cycles);
            }
        }
    }
}
```

Add metric methods in `engine.rs` because model types are external:

```rust
trait RegularMetricsExt {
    fn scale(&mut self, factor: f64);
    fn add_assign(&mut self, other: &RegularOutputMetrics);
}

impl RegularMetricsExt for RegularOutputMetrics {
    fn scale(&mut self, factor: f64) {
        self.mite *= factor;
        self.ms *= factor;
        self.dsb *= factor;
        self.lsd *= factor;
        self.issued *= factor;
        self.executed *= factor;
        self.div *= factor;
        for value in self.ports.values_mut() {
            *value *= factor;
        }
    }

    fn add_assign(&mut self, other: &RegularOutputMetrics) {
        self.mite += other.mite;
        self.ms += other.ms;
        self.dsb += other.dsb;
        self.lsd += other.lsd;
        self.issued += other.issued;
        self.executed += other.executed;
        self.div += other.div;
        for (port, value) in &other.ports {
            *self.ports.entry(port.clone()).or_insert(0.0) += value;
        }
    }
}
```

Add columns and notes:

```rust
fn regular_columns(all_ports: &[String], include_div: bool, include_notes: bool) -> Vec<RegularColumn> {
    let mut columns = vec![
        RegularColumn { key: "mite".to_string(), label: "MITE".to_string(), kind: RegularColumnKind::Frontend },
        RegularColumn { key: "ms".to_string(), label: "MS".to_string(), kind: RegularColumnKind::Frontend },
        RegularColumn { key: "dsb".to_string(), label: "DSB".to_string(), kind: RegularColumnKind::Frontend },
        RegularColumn { key: "lsd".to_string(), label: "LSD".to_string(), kind: RegularColumnKind::Frontend },
        RegularColumn { key: "issued".to_string(), label: "Issued".to_string(), kind: RegularColumnKind::Issue },
        RegularColumn { key: "executed".to_string(), label: "Exec.".to_string(), kind: RegularColumnKind::Execute },
    ];
    for port in all_ports {
        columns.push(RegularColumn {
            key: format!("port_{port}"),
            label: format!("Port {port}"),
            kind: RegularColumnKind::Port,
        });
    }
    if include_div {
        columns.push(RegularColumn { key: "div".to_string(), label: "Div".to_string(), kind: RegularColumnKind::Divider });
    }
    if include_notes {
        columns.push(RegularColumn { key: "notes".to_string(), label: "Notes".to_string(), kind: RegularColumnKind::Notes });
    }
    columns
}

fn regular_note_legend(rows: &[RegularOutputRow]) -> Vec<RegularNote> {
    let mut notes = Vec::new();
    if rows.iter().any(|row| row.notes.iter().any(|note| note == "J")) {
        notes.push(RegularNote {
            key: "J".to_string(),
            label: "Block not in DSB due to JCC erratum".to_string(),
            url: Some("https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/technical-documentation/jcc-erratum.html".to_string()),
        });
    }
    if rows.iter().any(|row| row.notes.iter().any(|note| note == "M")) {
        notes.push(RegularNote {
            key: "M".to_string(),
            label: "Macro-fused with previous instruction".to_string(),
            url: None,
        });
    }
    if rows.iter().any(|row| row.notes.iter().any(|note| note == "X")) {
        notes.push(RegularNote {
            key: "X".to_string(),
            label: "Instruction not supported".to_string(),
            url: None,
        });
    }
    notes
}

fn build_regular_limit_lines(summary: &uica_model::Summary) -> Vec<RegularLimitLine> {
    let throughput = summary.throughput_cycles_per_iteration.unwrap_or(f64::NAN);
    let labels = [
        ("predecoder", "Predecoder"),
        ("decoder", "Decoder"),
        ("dsb", "DSB"),
        ("lsd", "LSD"),
        ("issue", "Issue"),
        ("ports", "Ports"),
        ("dependencies", "Dependencies"),
    ];
    labels
        .into_iter()
        .filter_map(|(key, label)| {
            let value = summary.limits.get(key).and_then(|value| *value)?;
            Some(RegularLimitLine {
                key: key.to_string(),
                label: label.to_string(),
                throughput: value,
                is_bottleneck: value == throughput,
            })
        })
        .collect()
}
```

- [ ] **Step 6: Run targeted test**

Run:

```bash
cargo test -p uica-core regular_report_collects_simple_add_row
```

Expected:

```text
test result: ok.
```

- [ ] **Step 7: Run core tests**

Run:

```bash
cargo test -p uica-core
```

Expected:

```text
test result: ok.
```

- [ ] **Step 8: Commit**

Run:

```bash
git add rust/uica-core/src/engine.rs
git commit -m "feat: collect regular output report"
```

---

## Task 3: Add Text Renderer

**Files:**

- Modify: `rust/uica-core/src/report.rs`

- [ ] **Step 1: Add renderer unit tests**

Append to `#[cfg(test)]` module in `report.rs`:

```rust
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
            RegularColumn { key: "dsb".to_string(), label: "DSB".to_string(), kind: RegularColumnKind::Frontend },
            RegularColumn { key: "issued".to_string(), label: "Issued".to_string(), kind: RegularColumnKind::Issue },
            RegularColumn { key: "port_0".to_string(), label: "Port 0".to_string(), kind: RegularColumnKind::Port },
            RegularColumn { key: "notes".to_string(), label: "Notes".to_string(), kind: RegularColumnKind::Notes },
        ],
        rows: vec![RegularOutputRow {
            row_id: "instr-0".to_string(),
            kind: RegularRowKind::Instruction,
            instr_id: Some(0),
            asm: "add rax, rbx".to_string(),
            opcode: Some("4801D8".to_string()),
            url: None,
            notes: vec!["M".to_string()],
            metrics: RegularOutputMetrics { dsb: 1.0, issued: 1.0, ports: [("0".to_string(), 0.5)].into_iter().collect(), ..RegularOutputMetrics::default() },
        }],
        notes: vec![RegularNote { key: "M".to_string(), label: "Macro-fused with previous instruction".to_string(), url: None }],
        ..RegularOutputReport::default()
    };

    let text = render_regular_text(&report);
    assert!(text.contains("Throughput (in cycles per iteration): 2.00"));
    assert!(text.contains("Bottleneck: Dependencies"));
    assert!(text.contains("M - Macro-fused with previous instruction"));
    assert!(text.contains("add rax, rbx"));
    assert!(text.contains("0.5"));
}
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p uica-core regular_text_
```

Expected failure includes unresolved function names:

```text
cannot find function `render_regular_text` in this scope
```

- [ ] **Step 3: Import regular model types**

At top of `report.rs`, change import to:

```rust
use uica_model::{
    GraphReport, GraphSeries, RegularColumnKind, RegularOutputMetrics, RegularOutputReport,
    RegularOutputRow, TraceInstructionRow, TraceReport, TraceUopRow,
};
```

- [ ] **Step 4: Add text renderer**

Insert after graph rendering helpers:

```rust
pub fn render_regular_text(report: &RegularOutputReport) -> String {
    let mut out = String::new();
    if let Some(tp) = report.throughput_cycles_per_iteration {
        out.push_str(&format!("Throughput (in cycles per iteration): {tp:.2}\n"));
    }
    out.push_str(&format!("{}\n", bottleneck_line(&report.bottlenecks)));

    if !report.limit_lines.is_empty() {
        out.push_str("\nThe following throughputs could be achieved if the given property were the only bottleneck:\n\n");
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

fn bottleneck_line(bottlenecks: &[String]) -> String {
    match bottlenecks.len() {
        0 => "Bottleneck: unknown".to_string(),
        1 => format!("Bottleneck: {}", bottlenecks[0]),
        _ => format!("Bottlenecks: {}", bottlenecks.join(", ")),
    }
}

pub(crate) fn format_regular_cell(value: f64) -> String {
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
    let mut lines = Vec::new();
    let headers: Vec<String> = report.columns.iter().map(|column| column.label.clone()).collect();
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len().max(5)).collect();
    let mut rendered_rows = Vec::new();

    for row in &report.rows {
        let mut cells = regular_row_cells(row, report);
        cells.push(row.asm.clone());
        if widths.len() < cells.len() {
            widths.resize(cells.len(), 5);
        }
        for (idx, cell) in cells.iter().enumerate() {
            widths[idx] = widths[idx].max(cell.len());
        }
        rendered_rows.push(cells);
    }

    let mut header_cells = headers;
    header_cells.push(String::new());
    lines.push(join_regular_table_row(&header_cells, &widths));
    lines.push(join_regular_separator(&widths));
    for cells in rendered_rows {
        lines.push(join_regular_table_row(&cells, &widths));
    }
    lines.push(join_regular_separator(&widths));
    let mut total_cells = regular_metric_cells(&report.totals, report);
    total_cells.push("Total".to_string());
    lines.push(join_regular_table_row(&total_cells, &widths));
    lines.push(String::new());
    lines.join("\n")
}

fn regular_row_cells(row: &RegularOutputRow, report: &RegularOutputReport) -> Vec<String> {
    let mut cells = regular_metric_cells(&row.metrics, report);
    if report.columns.iter().any(|column| matches!(column.kind, RegularColumnKind::Notes)) {
        cells.push(row.notes.join(""));
    }
    cells
}

fn regular_metric_cells(metrics: &RegularOutputMetrics, report: &RegularOutputReport) -> Vec<String> {
    let mut cells = Vec::new();
    for column in &report.columns {
        match column.kind {
            RegularColumnKind::Frontend => match column.key.as_str() {
                "mite" => cells.push(format_regular_cell(metrics.mite)),
                "ms" => cells.push(format_regular_cell(metrics.ms)),
                "dsb" => cells.push(format_regular_cell(metrics.dsb)),
                "lsd" => cells.push(format_regular_cell(metrics.lsd)),
                _ => cells.push(String::new()),
            },
            RegularColumnKind::Issue => cells.push(format_regular_cell(metrics.issued)),
            RegularColumnKind::Execute => cells.push(format_regular_cell(metrics.executed)),
            RegularColumnKind::Port => {
                let port = column.key.strip_prefix("port_").unwrap_or(column.key.as_str());
                cells.push(format_regular_cell(*metrics.ports.get(port).unwrap_or(&0.0)));
            }
            RegularColumnKind::Divider => cells.push(format_regular_cell(metrics.div)),
            RegularColumnKind::Notes => {}
        }
    }
    cells
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
```

- [ ] **Step 5: Run renderer tests**

Run:

```bash
cargo test -p uica-core regular_text_
```

Expected:

```text
test result: ok.
```

- [ ] **Step 6: Commit**

Run:

```bash
git add rust/uica-core/src/report.rs
git commit -m "feat: render regular text report"
```

---

## Task 4: Wire CLI Default Text Output

**Files:**

- Modify: `rust/uica-cli/src/main.rs`

- [ ] **Step 1: Add CLI test command to run manually**

Create sample bytes during test run:

```bash
printf '\x48\x01\xd8' > /tmp/uica-add.bin
cargo run -p uica-cli -- /tmp/uica-add.bin --raw --arch SKL
```

Expected current behavior before implementation:

```text

```

- [ ] **Step 2: Modify engine invocation decision**

Replace:

```rust
    let wants_reports = args.trace.is_some() || args.graph.is_some();
```

with:

```rust
    let wants_default_text = !args.tp_only;
    let wants_reports = args.trace.is_some() || args.graph.is_some() || wants_default_text;
```

- [ ] **Step 3: Print regular text when not tp-only**

After JSON write block and before `if args.tp_only`, insert:

```rust
    if !args.tp_only {
        if let Some(reports) = output.reports.as_ref() {
            print!("{}", uica_core::report::render_regular_text(&reports.regular));
        }
    }
```

- [ ] **Step 4: Run manual CLI check**

Run:

```bash
printf '\x48\x01\xd8' > /tmp/uica-add.bin
cargo run -p uica-cli -- /tmp/uica-add.bin --raw --arch SKL | head -40
```

Expected output contains:

```text
Throughput (in cycles per iteration):
Bottleneck:
add rax, rbx
Total
```

- [ ] **Step 5: Confirm tp-only unchanged**

Run:

```bash
cargo run -p uica-cli -- /tmp/uica-add.bin --raw --arch SKL --tp-only
```

Expected output is one numeric line, for example:

```text
1
```

- [ ] **Step 6: Commit**

Run:

```bash
git add rust/uica-cli/src/main.rs
git commit -m "feat: print regular CLI output by default"
```

---

## Task 5: Add HTML Renderer

**Files:**

- Modify: `rust/uica-core/src/report.rs`

- [ ] **Step 1: Add HTML tests**

Append to `report.rs` test module:

```rust
#[test]
fn regular_html_escapes_instruction_text() {
    use uica_model::{RegularColumn, RegularColumnKind, RegularOutputReport, RegularOutputRow, RegularRowKind};

    let report = RegularOutputReport {
        columns: vec![RegularColumn { key: "issued".to_string(), label: "Issued".to_string(), kind: RegularColumnKind::Issue }],
        rows: vec![RegularOutputRow {
            row_id: "instr-0".to_string(),
            kind: RegularRowKind::Instruction,
            instr_id: Some(0),
            asm: "cmp rax, <bad>&\"".to_string(),
            ..RegularOutputRow::default()
        }],
        ..RegularOutputReport::default()
    };

    let html = render_regular_html(&report).unwrap();
    assert!(html.contains("&lt;bad&gt;&amp;&quot;"));
    assert!(html.contains("<table"));
    assert!(!html.contains("<script src="));
}
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p uica-core regular_html_escapes_instruction_text
```

Expected failure:

```text
cannot find function `render_regular_html` in this scope
```

- [ ] **Step 3: Add HTML renderer**

Insert after `render_regular_text` helpers:

```rust
pub fn render_regular_html(report: &RegularOutputReport) -> Result<String, String> {
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>uiCA Analysis</title>");
    html.push_str("<style>body{font-family:system-ui,sans-serif;margin:1rem;color:#111;background:#fff}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}th,td{border:1px solid #ccd;padding:.35rem .5rem;text-align:right}th:last-child,td:last-child{text-align:left}.notes{text-align:center}.summary{margin-bottom:1rem}@media(prefers-color-scheme:dark){body{color:#eee;background:#111}th,td{border-color:#445}}</style>");
    html.push_str("</head><body>");
    html.push_str("<section class=\"summary\">");
    if let Some(tp) = report.throughput_cycles_per_iteration {
        html.push_str(&format!("<p><strong>Throughput:</strong> {:.2} cycles/iteration</p>", tp));
    }
    html.push_str(&format!("<p><strong>{}</strong></p>", escape_html(&bottleneck_line(&report.bottlenecks))));
    html.push_str("</section>");

    if !report.notes.is_empty() {
        html.push_str("<section class=\"notes\"><ul>");
        for note in &report.notes {
            html.push_str(&format!("<li><strong>{}</strong> - {}</li>", escape_html(&note.key), escape_html(&note.label)));
        }
        html.push_str("</ul></section>");
    }

    html.push_str("<table class=\"regular-output\"><thead><tr>");
    for column in &report.columns {
        html.push_str(&format!("<th scope=\"col\">{}</th>", escape_html(&column.label)));
    }
    html.push_str("<th scope=\"col\">Instruction</th></tr></thead><tbody>");
    for row in &report.rows {
        html.push_str(&format!("<tr class=\"regular-row regular-row-{}\"", row_kind_class(&row.kind)));
        if let Some(instr_id) = row.instr_id {
            html.push_str(&format!(" data-instr-id=\"{}\"", instr_id));
        }
        html.push('>');
        for cell in regular_row_cells(row, report) {
            html.push_str(&format!("<td>{}</td>", escape_html(&cell)));
        }
        html.push_str("<th scope=\"row\">");
        if let Some(url) = &row.url {
            html.push_str(&format!("<a href=\"{}\" target=\"_blank\" rel=\"noreferrer\">{}</a>", escape_html(url), escape_html(&row.asm)));
        } else {
            html.push_str(&escape_html(&row.asm));
        }
        html.push_str("</th></tr>");
    }
    html.push_str("</tbody><tfoot><tr>");
    for cell in regular_metric_cells(&report.totals, report) {
        html.push_str(&format!("<td>{}</td>", escape_html(&cell)));
    }
    html.push_str("<th scope=\"row\">Total</th></tr></tfoot></table></body></html>");
    Ok(html)
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
```

- [ ] **Step 4: Run HTML tests**

Run:

```bash
cargo test -p uica-core regular_html_escapes_instruction_text
```

Expected:

```text
test result: ok.
```

- [ ] **Step 5: Commit**

Run:

```bash
git add rust/uica-core/src/report.rs
git commit -m "feat: render regular HTML report"
```

---

## Task 6: Extend Emscripten Envelope

**Files:**

- Modify: `rust/uica-emscripten/src/lib.rs`
- Modify: `rust/uica-emscripten/tests/run_contract.rs`

- [ ] **Step 1: Add failing contract assertions**

In `run_contract.rs`, extend JSON assertions with:

```rust
    assert!(value["regular_text"].as_str().unwrap().contains("Throughput"));
    assert!(value["regular_text"].as_str().unwrap().contains("add rax, rbx"));
    assert!(value["regular_html"].as_str().unwrap().contains("<table"));
```

- [ ] **Step 2: Run contract test and confirm failure**

Run:

```bash
cargo test -p uica-emscripten run_contract
```

Expected failure:

```text
called `Option::unwrap()` on a `None` value
```

- [ ] **Step 3: Add fields to envelope construction**

In `rust/uica-emscripten/src/lib.rs`, locate web result JSON object. Add report rendering after trace rendering:

```rust
let regular_text = reports
    .as_ref()
    .map(|reports| uica_core::report::render_regular_text(&reports.regular))
    .unwrap_or_default();
let regular_html = reports
    .as_ref()
    .map(|reports| uica_core::report::render_regular_html(&reports.regular).unwrap_or_default())
    .unwrap_or_default();
```

Add fields to JSON envelope:

```rust
"regular_text": regular_text,
"regular_html": regular_html,
```

- [ ] **Step 4: Run contract test**

Run:

```bash
cargo test -p uica-emscripten run_contract
```

Expected:

```text
test result: ok.
```

- [ ] **Step 5: Commit**

Run:

```bash
git add rust/uica-emscripten/src/lib.rs rust/uica-emscripten/tests/run_contract.rs
git commit -m "feat: expose regular reports in web envelope"
```

---

## Task 7: Add Web Analysis Tab

**Files:**

- Modify: `web/index.html`
- Modify: `web/main.js`
- Modify: `web/style.css`

- [ ] **Step 1: Add Analysis tab markup**

In `web/index.html`, near existing output tabs, add a button:

```html
<button
  class="tab-button"
  id="analysis-tab"
  type="button"
  role="tab"
  aria-controls="analysis-panel"
  aria-selected="false"
>
  Analysis
</button>
```

Add panel:

```html
<section
  id="analysis-panel"
  class="tab-panel"
  role="tabpanel"
  aria-labelledby="analysis-tab"
  hidden
>
  <iframe
    id="analysis-frame"
    title="uiCA analysis output"
    sandbox="allow-popups allow-popups-to-escape-sandbox"
  ></iframe>
  <pre id="analysis-text" hidden></pre>
</section>
```

- [ ] **Step 2: Wire main.js render path**

In `web/main.js`, cache elements:

```js
const analysisFrame = document.getElementById("analysis-frame");
const analysisText = document.getElementById("analysis-text");
```

After successful run result:

```js
renderAnalysis(result);
```

Add function:

```js
function renderAnalysis(result) {
  const html = result.regular_html || "";
  const text = result.regular_text || "";
  if (html) {
    analysisFrame.hidden = false;
    analysisText.hidden = true;
    analysisFrame.srcdoc = html;
    return;
  }
  analysisFrame.hidden = true;
  analysisText.hidden = false;
  analysisText.textContent = text || "No analysis output available.";
}
```

- [ ] **Step 3: Add CSS**

In `web/style.css`, add:

```css
#analysis-frame {
  width: 100%;
  min-height: 900px;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--panel);
}

#analysis-text {
  white-space: pre;
  overflow: auto;
  padding: 1rem;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--panel);
}
```

- [ ] **Step 4: Run web smoke**

Run:

```bash
./scripts/build-web.sh
```

Expected:

```text
NASM smoke passed
Emscripten export smoke passed
```

- [ ] **Step 5: Commit**

Run:

```bash
git add web/index.html web/main.js web/style.css
git commit -m "feat: show regular analysis in web UI"
```

---

## Task 8: Parity Verification

**Files:**

- No source edits unless verification finds a mismatch.

- [ ] **Step 1: Compare Python and Rust simple output**

Run:

```bash
printf '\x48\x01\xd8' > /tmp/uica-add.bin
./uiCA.py /tmp/uica-add.bin -raw -arch SKL > /tmp/python-add.txt
cargo run -p uica-cli -- /tmp/uica-add.bin --raw --arch SKL > /tmp/rust-add.txt
head -80 /tmp/python-add.txt
head -80 /tmp/rust-add.txt
```

Expected: both outputs contain throughput, bottleneck, `add rax, rbx`, and `Total`.

- [ ] **Step 2: Compare macro-fusion sample**

Run:

```bash
printf '\x48\x01\xd8\x48\x01\xc3\x49\xff\xcf\x75\xf4' > /tmp/uica-loop.bin
./uiCA.py /tmp/uica-loop.bin -raw -arch SKL > /tmp/python-loop.txt
cargo run -p uica-cli -- /tmp/uica-loop.bin --raw --arch SKL > /tmp/rust-loop.txt
grep -n "M - Macro-fused" /tmp/python-loop.txt /tmp/rust-loop.txt
grep -n "jnz" /tmp/python-loop.txt /tmp/rust-loop.txt
```

Expected: both outputs include `M - Macro-fused with previous instruction` and `jnz` row with `M` note.

- [ ] **Step 3: Run workspace checks**

Run:

```bash
cargo test --workspace
./scripts/build-web.sh
```

Expected:

```text
test result: ok.
NASM smoke passed
Emscripten export smoke passed
```

- [ ] **Step 4: Commit verification fixtures only if source changed**

If no source changes happened during verification, do not commit. If source changes happened, run:

```bash
git add rust/uica-model/src/lib.rs rust/uica-core/src/engine.rs rust/uica-core/src/report.rs rust/uica-cli/src/main.rs rust/uica-emscripten/src/lib.rs rust/uica-emscripten/tests/run_contract.rs web/index.html web/main.js web/style.css
git commit -m "fix: align regular output report parity"
```

---

## Self-Review

Spec coverage:

- Structured model: Task 1.
- Steady-state per-instruction collection: Task 2.
- Synthetic merge/stack rows: Task 2.
- Notes `M/J/X`: Task 2 creates `M/J` support and preserves `X` legend path. Explicit unsupported tracking remains separate because current Rust metadata needs a focused parity investigation.
- Text renderer: Task 3.
- CLI default output: Task 4.
- HTML renderer: Task 5.
- Emscripten envelope: Task 6.
- Web display: Task 7.
- Verification: Task 8.

Known follow-up after this plan:

- Add explicit unsupported-instruction marker for exact `X` note parity once Rust unsupported detection path is mapped.
- Tighten text renderer box drawing to match Python table width exactly after numeric/content parity lands.
