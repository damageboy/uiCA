# Runtime UIPack No DataPack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the runtime `MappedUiPackRuntime -> DataPack` candidate materialization bridge from engine runs while preserving behavior.

**Architecture:** Add a small runtime instruction-data adapter that can match records either from legacy `DataPack` tests/APIs or from `MappedUiPackRuntime`. Runtime path will use `UiPackViewIndex` and materialize only the final matched record as an owned `InstructionRecord` when existing engine/sim helpers require it. Full borrowed operand/latency views remain future work after performance measurement.

**Tech Stack:** Rust workspace, `uica-core`, `uica-data` UIPack v9 views, Cargo tests, BHive verification.

---

## Files

- Create: `rust/uica-core/src/instruction_data.rs`
  - Internal adapter enum over `DataPack` or `MappedUiPackRuntime`.
  - Record matching helper using existing matcher semantics.
- Modify: `rust/uica-core/src/lib.rs`
  - Add internal module.
- Modify: `rust/uica-core/src/engine.rs`
  - Remove runtime calls to `materialize_runtime_pack_for_decoded`.
  - Route runtime engine and trace through adapter.
- Modify: `rust/uica-core/src/sim/frontend.rs`
  - Store adapter instead of `pack` + `DataPackIndex` only.
  - Keep public `FrontEnd::new` for tests.
  - Add runtime constructor.
- Modify: `rust/uica-core/src/sim/uop_expand.rs`
  - Add adapter-based expand function.
  - Keep legacy pack wrapper for tests.
- Tests: existing `uica-core`, `uica-cli`, `uica-emscripten`, BHive 1k.

---

## Task 1: Add instruction-data adapter

**Files:**
- Create: `rust/uica-core/src/instruction_data.rs`
- Modify: `rust/uica-core/src/lib.rs`

- [ ] **Step 1: Add adapter types**

Create internal enum:

```rust
#[derive(Clone, Copy)]
pub(crate) enum InstructionDataSource<'a> {
    Pack {
        pack: &'a uica_data::DataPack,
        index: &'a uica_data::DataPackIndex<'a>,
    },
    Runtime {
        runtime: &'a uica_data::MappedUiPackRuntime,
    },
}

pub(crate) enum MatchedRecord<'a> {
    Borrowed(&'a uica_data::InstructionRecord),
    Owned(uica_data::InstructionRecord),
}
```

Add `MatchedRecord::as_record(&self) -> &InstructionRecord`.

- [ ] **Step 2: Add runtime candidate wrapper**

Inside `instruction_data.rs`, add private `RuntimeCandidate<'a>` with:

```rust
record_index: u32,
record: uica_data::UiPackRecordView<'a>,
xml_attrs: std::collections::BTreeMap<String, String>,
```

Implement `crate::matcher::InstrRecordLike` for it using `record.iform()`, `record.string()`, `record.imm_zero()`, and owned `xml_attrs`.

- [ ] **Step 3: Add matching helpers**

Implement:

```rust
impl<'a> InstructionDataSource<'a> {
    pub(crate) fn all_ports(&self) -> Result<Vec<String>, String>;
    pub(crate) fn match_record(
        &self,
        arch: &str,
        mnemonic: &str,
        norm: crate::matcher::NormalizedInstrRef<'_>,
    ) -> Result<Option<MatchedRecord<'a>>, String>;
}
```

Pack variant uses existing `DataPackIndex` and returns `MatchedRecord::Borrowed`.

Runtime variant:
- gets `view = runtime.view()`
- uses `runtime.index().record_indices_for_mnemonic(mnemonic)`
- builds `Vec<RuntimeCandidate>` by reading each record and `record.xml_attrs()?`
- calls `match_instruction_record_iter(norm, candidates.iter())`
- on match, converts matched `UiPackRecordView` with `uica_data::record_view_to_instruction_record(...)`
- returns `MatchedRecord::Owned(...)`

- [ ] **Step 4: Wire module**

Add `mod instruction_data;` in `rust/uica-core/src/lib.rs`.

- [ ] **Step 5: Test**

Run:

```bash
cargo test -p uica-core instruction_data
```

Add a focused unit test if needed; otherwise compile-only is acceptable for this adapter task.

---

## Task 2: Route analytical engine through adapter

**Files:**
- Modify: `rust/uica-core/src/engine.rs`

- [ ] **Step 1: Split internal function**

Keep public pack API:

```rust
fn engine_with_decoded_pack_internal(... pack: &DataPack, ...) -> Result<EngineOutput, String>
```

but make it create:

```rust
let index = DataPackIndex::new(pack);
let data = InstructionDataSource::Pack { pack, index: &index };
engine_with_decoded_data_internal(decoded, invocation, data, include_reports)
```

Add new internal:

```rust
fn engine_with_decoded_data_internal<'a>(
    decoded: &[DecodedInstruction],
    invocation: &Invocation,
    data: InstructionDataSource<'a>,
    include_reports: bool,
) -> Result<EngineOutput, String>
```

- [ ] **Step 2: Replace loop matching**

Inside loop, replace `index.candidates_for(...)` and `match_instruction_record_iter(...)` with:

```rust
let matched = data.match_record(&result.invocation.arch, &decoded_instr.mnemonic, norm)?;
if let Some(matched) = matched {
    let record = matched.as_record();
    ... existing code ...
}
```

- [ ] **Step 3: Replace port source access**

Replace `pack.alu_ports.join("")` with `record.alu_ports.join("")` or `data.all_ports()?` where appropriate.
Replace report/parameter `pack.all_ports` with `data.all_ports()?`.

- [ ] **Step 4: Runtime engine uses data directly**

Change `engine_output_with_decoded_uipack_runtime` to:

```rust
engine_with_decoded_data_internal(
    decoded,
    invocation,
    InstructionDataSource::Runtime { runtime },
    include_reports,
)
```

Do not call `materialize_runtime_pack_for_decoded` here.

- [ ] **Step 5: Test**

Run:

```bash
cargo test -p uica-core uipack_runtime_engine
```

---

## Task 3: Route simulator/frontend through adapter

**Files:**
- Modify: `rust/uica-core/src/sim/frontend.rs`
- Modify: `rust/uica-core/src/sim/uop_expand.rs`
- Modify: `rust/uica-core/src/engine.rs`

- [ ] **Step 1: FrontEnd stores data source**

Replace fields:

```rust
pub pack: &'a uica_data::DataPack,
pub pack_index: &'a uica_data::DataPackIndex<'a>,
```

with:

```rust
pub instruction_data: crate::instruction_data::InstructionDataSource<'a>,
```

Keep `FrontEnd::new(...)` signature and construct `InstructionDataSource::Pack`.

Add:

```rust
pub fn new_with_runtime(..., runtime: &'a uica_data::MappedUiPackRuntime, ...) -> Self
```

or add a shared `new_with_instruction_data(...)` constructor.

- [ ] **Step 2: Uop expand adapter path**

Add:

```rust
pub(crate) fn expand_instr_instance_to_lam_uops_with_data(
    instr: &InstrInstance,
    ...,
    arch_name: &str,
    data: crate::instruction_data::InstructionDataSource<'_>,
) -> Result<Vec<u64>, String>
```

Move current implementation into this function. Legacy public `expand_instr_instance_to_lam_uops_with_storage(... pack, pack_index)` becomes a wrapper that creates `InstructionDataSource::Pack`.

Use `data.match_record(...)` instead of `DataPackIndex` lookup.

- [ ] **Step 3: FrontEnd calls adapter expand**

In `FrontEnd::cycle`, replace call to pack-based expand with `expand_instr_instance_to_lam_uops_with_data(..., self.instruction_data)`.

- [ ] **Step 4: Engine simulation uses runtime directly**

Change `run_simulation_for_cycles` to take `InstructionDataSource<'a>` instead of pack/index.

Pack path passes `InstructionDataSource::Pack`; runtime path passes `InstructionDataSource::Runtime`.

- [ ] **Step 5: Trace uses runtime directly**

Update `engine_trace` to remove `materialize_runtime_pack_for_decoded` and `DataPackIndex` creation.

- [ ] **Step 6: Test**

Run:

```bash
cargo test -p uica-core -p uica-cli -p uica-emscripten
```

---

## Task 4: Delete runtime materialization bridge

**Files:**
- Modify: `rust/uica-core/src/engine.rs`

- [ ] **Step 1: Remove `materialize_runtime_pack_for_decoded`**

Delete function from `engine.rs`.

- [ ] **Step 2: Remove unused imports**

Remove from `engine.rs` imports if unused:

```rust
record_view_to_instruction_record
BTreeSet
```

Keep `DataPack`/`DataPackIndex` only for legacy pack API/tests.

- [ ] **Step 3: Assert no bridge remains**

Run:

```bash
rg "materialize_runtime_pack_for_decoded|record_view_to_instruction_record" rust/uica-core/src/engine.rs
```

Expected no matches.

- [ ] **Step 4: Test**

Run:

```bash
cargo test --workspace
```

---

## Task 5: Parity and timing measurement

**Files:**
- No code edits expected.

- [ ] **Step 1: Build release CLI**

```bash
cargo build -q -r -p uica-cli
```

- [ ] **Step 2: Run BHive 1k parity**

Use fresh Python goldens:

```bash
TAG=py-bhive-uipack-direct-$(git rev-parse --short HEAD)-$(date +%Y%m%d-%H%M%S)
ROOT=/tmp/uica-bhive-uipack-direct
for profile in bhive_skl_1k bhive_hsw_1k bhive_ivb_1k; do
  python3 verification/tools/capture.py --profile "$profile" --engine python --golden-root "$ROOT" --golden-tag "$TAG" --jobs 8
done
mkdir -p "$ROOT/rust"
cp -R "$ROOT/python/$TAG" "$ROOT/rust/$TAG"
for profile in bhive_skl_1k bhive_hsw_1k bhive_ivb_1k; do
  python3 verification/tools/verify.py --profile "$profile" --engine rust --rust-bin "$PWD/target/release/uica-cli" --golden-root "$ROOT" --golden-tag "$TAG" --jobs 8 --dump-diff "$ROOT/${profile}.diff"
done
```

Expected: all three profiles report `1000 case/arch result(s) matched`.

- [ ] **Step 3: Basic runtime timing**

Run SKL 1k verify twice with `time` and record wall clock in final summary. Compare informally to previous v9 baseline if available.

---

## Self-review

- This plan removes DataPack materialization from runtime engine/trace path.
- It intentionally keeps legacy pack APIs and tests.
- It intentionally permits per-matched-record `InstructionRecord` ownership as a stepping stone.
- Full borrowed operand/latency/variant views are deferred until after timing measurement.
