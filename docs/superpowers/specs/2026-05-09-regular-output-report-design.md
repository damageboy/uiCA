# Regular Output Report Design

## Goal

Recreate uiCA's regular textual output in the Rust implementation, including per-instruction port pressure analysis, while making the underlying data structured enough to render as text, JSON, and HTML. The Python-like text output should be available for CLI parity, and the same report model should power a richer web/wasm presentation.

## Python Source Behavior

The original Python output is produced in `uiCA.py` when `runSimulation(..., printDetails=True)` is used.

Key functions and types:

- `TableLineData`: row input model for table generation.
- `getUopsTableColumns(tableLineData, uArchConfig)`: computes numeric columns and notes.
- `printUopsTable(tableLineData, uArchConfig, addHyperlink=True)`: renders a box-drawing terminal table.
- `getBottlenecks(...)`: computes frontend, issue, ports, divider, and dependency bottleneck data.
- `printBottlenecks(...)`: renders bottleneck summary text.

Python regular output contains:

1. Throughput line:
   ```text
   Throughput (in cycles per iteration): 2.00
   ```
2. Bottleneck summary:
   ```text
   Bottleneck: Dependencies
   ```
3. Optional limit lines:

   ```text
   The following throughputs could be achieved if the given property were the only bottleneck:

     - DSB: 1.00
     - Issue: 0.75
     - Ports: 1.00
     - Dependencies: 2.00
   ```

4. Optional note legend:
   ```text
   M - Macro-fused with previous instruction
   J - Block not in DSB due to JCC erratum
   X - Instruction not supported
   ```
5. Per-instruction uop/port table with columns:
   - `MITE`
   - `MS`
   - `DSB`
   - `LSD`
   - `Issued`
   - `Exec.`
   - `Port <n>` for all architecture ports
   - optional `Div`
   - optional `Notes`

Rows include normal instructions plus synthetic rows:

- `<Register Merge Uop>`
- `<Stack Sync Uop>`

For each row, numeric metrics are averaged across the selected steady-state rounds. Python skips numeric work for instructions marked as macro-fused with the previous instruction and emits note `M` instead.

## Design Principles

- Keep data model presentation-neutral.
- Do not embed terminal box-drawing text or HTML inside the model.
- Preserve Python semantics where possible: same steady-state window, same synthetic rows, same columns, same notes.
- Allow renderers to choose presentation:
  - text renderer for CLI and parity tests
  - HTML renderer for web/wasm
  - JSON serialization for API/UI use
- Avoid new instruction-data paths. Continue using manifest-selected UIPacks only.

## Structured Data Model

Add report types to `rust/uica-model/src/lib.rs`.

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
```

`schema_version` should be `uica-regular-report-v1`.

### Limit Lines

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularLimitLine {
    pub key: String,
    pub label: String,
    pub throughput: f64,
    pub is_bottleneck: bool,
}
```

`key` uses stable identifiers such as `dsb`, `issue`, `ports`, `dependencies`; `label` preserves human-readable Python names.

### Columns

```rust
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularColumn {
    pub key: String,
    pub label: String,
    pub kind: RegularColumnKind,
}
```

Example columns:

```json
[
  { "key": "mite", "label": "MITE", "kind": "frontend" },
  { "key": "ms", "label": "MS", "kind": "frontend" },
  { "key": "dsb", "label": "DSB", "kind": "frontend" },
  { "key": "lsd", "label": "LSD", "kind": "frontend" },
  { "key": "issued", "label": "Issued", "kind": "issue" },
  { "key": "executed", "label": "Exec.", "kind": "execute" },
  { "key": "port_0", "label": "Port 0", "kind": "port" },
  { "key": "notes", "label": "Notes", "kind": "notes" }
]
```

### Rows

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegularRowKind {
    Instruction,
    RegisterMerge,
    StackSync,
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
```

For synthetic rows:

- `kind = register_merge` and `asm = "<Register Merge Uop>"`
- `kind = stack_sync` and `asm = "<Stack Sync Uop>"`
- `instr_id` should point to the associated instruction when available, but renderers must not depend on it.

For normal instruction rows:

- `kind = instruction`
- `instr_id = Some(id)`
- `asm` is the decoded display string.
- `opcode` is the instruction bytes as uppercase hex when available.
- `url` is the uops.info URL when `instr_str` is available.

### Metrics

```rust
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
```

Port keys use raw architecture port labels such as `"0"`, `"1"`, `"2"`, `"3"`, `"4"`, `"5"`, `"6"`, `"7"`, `"8"`, `"9"`.

### Notes

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularNote {
    pub key: String,
    pub label: String,
    pub url: Option<String>,
}
```

Initial notes:

- `M`: `Macro-fused with previous instruction`
- `J`: `Block not in DSB due to JCC erratum`, with Intel URL when rendered as text/html
- `X`: `Instruction not supported`

## Report Collection Algorithm

Add `build_regular_report(...)` to `rust/uica-core/src/report.rs` or a focused new module such as `rust/uica-core/src/regular_report.rs`.

Inputs:

```rust
pub fn build_regular_report(
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
    summary: &Summary,
) -> RegularOutputReport
```

`UopsForRound` is currently private in `engine.rs`; implementation should either:

1. Move regular-report construction into `engine.rs` and keep helper functions local, or
2. Move `UopsForRound` to an internal report-accessible module, or
3. Pass a presentation-friendly projection of the relevant rounds into `report.rs`.

Recommended: keep the first implementation in `engine.rs` or an `engine::regular` submodule to avoid exposing simulator internals prematurely. Refactor after parity is established.

Collection steps:

1. Determine the steady-state window with the existing `python_relevant_round_window(frontend, uops_for_round)` helper.
2. Iterate instructions in original instruction ID order.
3. For each instruction ID, collect relevant `InstrInstance`s whose `rnd` is in `[first_round, last_round]`.
4. If any relevant instruction instance has `reg_merge_uops`, emit a `RegisterMerge` row before the instruction row.
5. If any relevant instruction instance has `stack_sync_uops`, emit a `StackSync` row before the instruction row.
6. Emit the instruction row.
7. For each row, aggregate all referenced laminated/fused/unfused uops:
   - Count `lam.uop_source` values into `mite`, `ms`, `dsb`, `lsd`.
   - Count each fused uop as `issued += 1`.
   - For each unfused uop:
     - if `actual_port` is present, `executed += 1` and increment `ports[actual_port]`.
     - if `div_cycles > 0`, add to `div`.
8. Divide all numeric row metrics by the number of relevant row instances, matching Python's `len(tld.uopsForRnd)` denominator.
9. Add row notes:
   - `M` when instruction is macro-fused with previous instruction; skip numeric aggregation like Python.
   - `J` when `cannot_be_in_dsb_due_to_jcc_erratum` is true.
   - `X` when unsupported. If Rust lacks an explicit unsupported marker, add one to `InstrInstance` or derive from matched metadata only if unambiguous.
10. Build `totals` by summing all numeric row metrics, excluding notes.
11. Build `columns` from architecture ports plus optional `Div` and optional `Notes` exactly as Python does:
    - Include `Div` only if any row has non-zero `div`.
    - Include `Notes` only if any row has notes.

## Unsupported Instruction Tracking

Python uses `UnknownInstr` and emits note `X`. Rust currently falls back through matched/default facts in multiple paths, and `InstrInstance` may not expose a clean unsupported flag.

Design requirement:

- Add an explicit internal boolean such as `matched_record: bool` or `is_unknown: bool` to the simulator instruction metadata if existing state is insufficient.
- The regular report must not guess `X` from empty strings unless tests prove it is equivalent.

## Report Integration

Extend `ReportBundle`:

```rust
pub struct ReportBundle {
    pub trace: TraceReport,
    pub graph: GraphReport,
    pub regular: RegularOutputReport,
}
```

When `include_reports` is true in `engine_with_decoded_pack_internal`, build:

```rust
let regular = build_regular_report(&frontend, &uops_for_round, &result.summary);
reports = Some(ReportBundle { trace, graph, regular });
```

If exact construction order creates borrow or visibility issues, use a temporary internal struct and convert to `RegularOutputReport` before returning.

## Text Rendering

Add:

```rust
pub fn render_regular_text(report: &RegularOutputReport) -> String
```

Renderer requirements:

- Use the structured model only.
- Print throughput line if throughput exists.
- Print bottleneck summary with pluralization matching Python:
  - `Bottleneck: unknown`
  - `Bottleneck: Ports`
  - `Bottlenecks: DSB, Ports`
- Print limit-line prose when `limit_lines` is non-empty.
- Print note legend when notes are present.
- Render a box-drawing table compatible with Python's visual layout.
- Format floats like Python `formatTableValue`:
  - `0.00` becomes blank.
  - trailing zeros and decimal point are stripped.
  - examples: `1.00 -> 1`, `0.50 -> 0.5`, `0.22 -> 0.22`.
- Include instruction text after the row table. Terminal hyperlinks are optional in first implementation; renderer should accept an option later if needed.

First parity target: stable content and numeric cells. Exact box widths can be tightened after golden tests.

## HTML Rendering

Add:

```rust
pub fn render_regular_html(report: &RegularOutputReport) -> Result<String, String>
```

Initial HTML requirements:

- No remote CDN dependencies.
- Escape all user/instruction strings.
- Render summary and bottlenecks.
- Render limit lines.
- Render note legend.
- Render a semantic `<table>` with:
  - `<thead>` for columns.
  - `<tbody>` for rows.
  - `<tfoot>` for totals.
- Link instruction `asm` to `row.url` when available.
- Use CSS classes, not inline styles where practical.
- Preserve machine-readable `data-port` attributes on port columns to support later UI highlighting.

Example row shape:

```html
<tr class="regular-row regular-row-instruction" data-instr-id="0">
  <td class="metric metric-dsb">1</td>
  <td class="metric metric-issued">1</td>
  <td class="metric metric-port" data-port="0">0.22</td>
  <td class="notes"></td>
  <th scope="row"><a href="...">add rax, rbx</a></th>
</tr>
```

## CLI Behavior

Current Rust CLI only prints output for `--tp-only`, JSON, trace, graph, or event-trace. Python default prints regular output.

Desired behavior:

- If `--tp-only` is set: preserve current numeric-only output.
- If `--json` is set: write JSON as today; do not force text output unless no other visible output is requested.
- If no output-only flag suppresses text, print `render_regular_text(&reports.regular)`.
- Ensure the engine is called with `include_reports = true` when regular text is needed.
- Keep trace/graph generation working from the same `ReportBundle`.

A conservative first implementation can print regular text when neither `--tp-only` nor `--json` is the sole requested output, but final CLI rules should be explicit in tests.

## Web/Wasm Behavior

`rust/uica-emscripten` currently returns an envelope containing `result` and `trace_html`.

Extend envelope with:

```json
{
  "result": { ... },
  "trace_html": "...",
  "regular_text": "...",
  "regular_html": "..."
}
```

Web UI can initially add a `Text` or `Analysis` tab that displays either:

- `regular_html` in a sandboxed iframe, or
- `regular_text` in a `<pre>`.

Because the user asked for structured model and renderer planning first, UI tab work can be a later implementation task. However, the Emscripten envelope should expose the data/rendered output once available.

## JSON Shape Compatibility

The report should serialize inside `ReportBundle` as:

```json
{
  "regular": {
    "schema_version": "uica-regular-report-v1",
    "arch": "SKL",
    "throughput_cycles_per_iteration": 2.0,
    "bottlenecks": ["Dependencies"],
    "limits": {"dsb": 1.0, "issue": 0.75, "ports": 1.0, "dependencies": 2.0},
    "columns": [...],
    "rows": [...],
    "totals": {...},
    "notes": [...]
  }
}
```

Use `snake_case` for Rust-native structured fields unless existing public JSON has a prior casing convention. Renderer-specific envelopes can use legacy names only where needed.

## Tests

### Unit Tests

Add model/render tests in `rust/uica-model` and `rust/uica-core`:

- `RegularOutputReport::default()` has schema `uica-regular-report-v1`.
- float formatter matches Python examples.
- text renderer omits zero cells.
- text renderer includes note legend only when notes exist.
- HTML renderer escapes `<`, `>`, `&`, quotes in instruction text.

### Integration Tests

Use small raw-byte samples and compare against Python output:

1. Simple one-instruction block:
   ```text
   48 01 d8     ; add rax, rbx
   ```
2. Macro-fused branch sample:
   ```text
   48 01 d8 48 01 c3 49 ff cf 75 f4
   ; add rax, rbx; add rbx, rax; dec r15; jnz loop
   ```
   Expected: `M` note on branch row.
3. Load/store sample to exercise ports 2/3 and 4/7/8/9 where applicable.
4. Divider sample to exercise optional `Div` column.
5. Stack-sync sample involving implicit RSP changes.
6. Unsupported instruction sample if a stable one exists for target arch.

Test comparison strategy:

- First compare structured report values with tolerances for floats.
- Then compare text output after normalizing terminal hyperlinks and whitespace around box borders.
- Add exact text golden only after the renderer stabilizes.

### CLI Tests

- `uica-cli --raw --arch SKL sample.bin` prints regular text.
- `uica-cli --raw --arch SKL sample.bin --tp-only` still prints only a number.
- `uica-cli --raw --arch SKL sample.bin --json out.json` writes JSON and follows the decided text-output rule.

### Web Tests

- `uica-emscripten` run contract includes `regular_text` and `regular_html` fields.
- `regular_html` contains a table and no remote scripts.
- Existing `trace_html` remains unchanged.

## Migration Steps

1. Add model structs and serialization tests.
2. Implement report collection behind `include_reports`, without changing CLI defaults yet.
3. Add text renderer and Python-sample tests.
4. Wire CLI default text output.
5. Add HTML renderer.
6. Extend Emscripten envelope.
7. Add web tab or display integration.
8. Tighten Python parity goldens after initial implementation.

## Open Questions

1. Should CLI print regular text when `--json` is also present, or should JSON-only remain quiet unless no output mode is selected?
2. Should terminal hyperlinks be preserved in Rust text output by default, optional, or omitted?
3. Should `regular_html` be embedded in the existing trace iframe/tab UI, or should it get a separate tab?
4. How strict should first text parity be: exact box drawing, or numeric/content parity with stable renderer snapshots?

## Recommended First Implementation Scope

For first implementation, target:

- structured `RegularOutputReport`
- text renderer
- CLI default text output
- Emscripten `regular_text` field

Then add:

- HTML renderer
- web tab integration
- exact visual polish

This reduces risk while preserving the long-term model/render separation.
