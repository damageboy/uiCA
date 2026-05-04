# DataPack Index + UiPack View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce Rust uiCA instruction-data cloning by first making `DataPackIndex` borrow/index owned `DataPack` records, then migrating runtime lookup toward mmap-backed `UiPackView` records.

**Architecture:** Phase 1 keeps current owned `DataPack` API but changes index storage from cloned `InstructionRecord`s to record indices borrowed from `&DataPack`, then builds/passes one index through engine/frontend. Phase 2 introduces an explicit mmap runtime context (`MappedUiPack` + `UiPackViewIndex`) and a small record-access abstraction so the engine can avoid full `to_data_pack()` materialization.

**Tech Stack:** Rust workspace crates `uica-data`, `uica-core`, `uica-cli`; `.uipack` manifest data; existing verification tools under `verification/tools/`.

---

## Current Evidence

Subagent recon found these hot clone/index sites:

- `rust/uica-data/src/index.rs`: `DataPackIndex` owns `Vec<InstructionRecord>` and clones records for dual string/iform keys.
- `rust/uica-core/src/engine.rs:89`: analytical path builds `DataPackIndex::new(pack.clone())`.
- `rust/uica-core/src/engine.rs:2124`: simulator support check builds another index, but `is_instr_supported()` currently always returns `true`.
- `rust/uica-core/src/sim/frontend.rs:365`: temporary first-round DSB/LSD index build.
- `rust/uica-core/src/sim/frontend.rs:458`: stored frontend index build.
- `rust/uica-data/src/uipack.rs`: `MappedUiPack`/`UiPackView`/`UiPackViewIndex` exist, but `load_uipack()` calls `view.to_data_pack()` and materializes owned records.

## Subagent Workflow

Use fresh subagents per task:

1. `worker`: implement one task only.
2. `reviewer`: spec-compliance review after worker commit.
3. `reviewer`: code-quality/perf-risk review after spec review.
4. Parent: run verification gates and decide next task.

Do not run two writer subagents in parallel. These changes touch shared APIs and must be serial.

---

## Task 1: Add Regression Tests for Borrowed Index Semantics

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-data/tests/index.rs`

- [ ] **Step 1: Inspect existing index tests**

Run:

```bash
rg -n "DataPackIndex|noncanonical|iform|alias|candidates_for" rust/uica-data/tests/index.rs
```

Expected: existing tests around mnemonic aliases and iform-prefix indexing are visible.

- [ ] **Step 2: Add tests that lock candidate order and no-clone-compatible behavior**

Add tests to `rust/uica-data/tests/index.rs` using existing test style. Cover:

```rust
#[test]
fn candidates_preserve_pack_order_for_duplicate_keys() {
    let pack = DataPack {
        schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
        all_ports: vec![],
        alu_ports: vec![],
        instructions: vec![
            record("SKL", "ADD_GPR64q_GPR64q", "ADD (R64, R64)"),
            record("SKL", "ADD_GPR64q_IMMz", "ADD (R64, I32)"),
        ],
    };

    let index = DataPackIndex::new(&pack);
    let candidates: Vec<_> = index.candidates_for("SKL", "add").collect();

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].iform, "ADD_GPR64q_GPR64q");
    assert_eq!(candidates[1].iform, "ADD_GPR64q_IMMz");
}

#[test]
fn candidates_can_be_read_after_index_without_record_clone() {
    let pack = DataPack {
        schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
        all_ports: vec![],
        alu_ports: vec![],
        instructions: vec![record("SKL", "SUB_GPR64q_GPR64q", "SUB (R64, R64)")],
    };

    let index = DataPackIndex::new(&pack);
    let candidates: Vec<_> = index.candidates_for("SKL", "sub").collect();

    assert_eq!(candidates[0].string, "SUB (R64, R64)");
    assert_eq!(pack.instructions[0].iform, "SUB_GPR64q_GPR64q");
}
```

If helper names differ, adapt to the actual existing helper signatures in `rust/uica-data/tests/index.rs`.

- [ ] **Step 3: Run failing tests**

Run:

```bash
cargo test -q -p uica-data --test index
```

Expected: compile failure because current `DataPackIndex::new` takes owned `DataPack` and `candidates_for()` returns a slice, not an iterator. This confirms tests describe the new API.

---

## Task 2: Convert `DataPackIndex` to Borrowed Record Indices

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-data/src/index.rs`
- Modify: `rust/uica-data/tests/index.rs`

- [ ] **Step 1: Change `DataPackIndex` shape**

In `rust/uica-data/src/index.rs`, change index storage to:

```rust
pub struct DataPackIndex<'a> {
    pack: &'a DataPack,
    by_arch_and_mnemonic: BTreeMap<(String, String), Vec<usize>>,
    empty: Vec<usize>,
}
```

Change constructor to:

```rust
impl<'a> DataPackIndex<'a> {
    pub fn new(pack: &'a DataPack) -> Self {
        let mut by_arch_and_mnemonic: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();

        for (index, record) in pack.instructions.iter().enumerate() {
            let arch = record.arch.to_ascii_uppercase();
            let string_mnemonic = normalize_mnemonic(&record.string);
            let iform_mnemonic = normalize_iform_prefix(&record.iform);

            by_arch_and_mnemonic
                .entry((arch.clone(), string_mnemonic.clone()))
                .or_default()
                .push(index);

            if string_mnemonic != iform_mnemonic {
                by_arch_and_mnemonic
                    .entry((arch, iform_mnemonic))
                    .or_default()
                    .push(index);
            }
        }

        Self {
            pack,
            by_arch_and_mnemonic,
            empty: Vec::new(),
        }
    }

    pub fn candidates_for(
        &'a self,
        arch: &str,
        mnemonic: &str,
    ) -> impl Iterator<Item = &'a InstructionRecord> + 'a {
        self.by_arch_and_mnemonic
            .get(&(arch.to_ascii_uppercase(), normalize_mnemonic(mnemonic)))
            .unwrap_or(&self.empty)
            .iter()
            .map(|&index| &self.pack.instructions[index])
    }
}
```

- [ ] **Step 2: Update `uica-data` tests to collect iterators**

Replace direct slice assertions like:

```rust
let candidates = index.candidates_for("SKL", "add");
assert_eq!(candidates.len(), 1);
```

with:

```rust
let candidates: Vec<_> = index.candidates_for("SKL", "add").collect();
assert_eq!(candidates.len(), 1);
```

- [ ] **Step 3: Run data tests**

Run:

```bash
cargo test -q -p uica-data --test index
cargo test -q -p uica-data
```

Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add rust/uica-data/src/index.rs rust/uica-data/tests/index.rs
git commit -m "perf: borrow datapack index records"
```

---

## Task 3: Update Matcher and Core Call Sites for Borrowed Candidate Iterators

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-core/src/matcher.rs`
- Modify: `rust/uica-core/src/engine.rs`
- Modify: `rust/uica-core/src/sim/frontend.rs`
- Modify: `rust/uica-core/src/sim/uop_expand.rs`
- Modify tests if compile requires iterator collection.

- [ ] **Step 1: Add matcher iterator entry point**

In `rust/uica-core/src/matcher.rs`, add:

```rust
pub fn match_instruction_record_iter<'a, I>(
    normalized: NormalizedInstrRef<'_>,
    candidates: I,
) -> Option<&'a InstructionRecord>
where
    I: IntoIterator<Item = &'a InstructionRecord>,
{
    let candidates: Vec<&'a InstructionRecord> = candidates.into_iter().collect();
    best_record_match(normalized, &candidates)
}
```

Keep existing `match_instruction_record_ref()` as compatibility wrapper:

```rust
pub fn match_instruction_record_ref<'a>(
    normalized: NormalizedInstrRef<'_>,
    candidates: &'a [InstructionRecord],
) -> Option<&'a InstructionRecord> {
    match_instruction_record_iter(normalized, candidates.iter())
}
```

If `best_record_match()` currently owns candidate collection internally, refactor only enough to avoid duplicate collection and preserve behavior.

- [ ] **Step 2: Update index call sites**

Replace patterns like:

```rust
let candidates = index.candidates_for(arch_name, &instr_i.mnemonic);
let Some(record) = match_instruction_record_ref(norm, candidates) else { ... };
```

with:

```rust
let candidates = index.candidates_for(arch_name, &instr_i.mnemonic);
let Some(record) = match_instruction_record_iter(norm, candidates) else { ... };
```

Update imports from `match_instruction_record_ref` to include `match_instruction_record_iter` where needed.

- [ ] **Step 3: Update `DataPackIndex::new` calls**

Replace:

```rust
DataPackIndex::new(pack.clone())
```

with:

```rust
DataPackIndex::new(pack)
```

where `pack: &DataPack`. In tests with owned `pack`, use:

```rust
let index = DataPackIndex::new(&pack);
```

- [ ] **Step 4: Run compile and core tests**

Run:

```bash
cargo check -q -p uica-core
cargo test -q -p uica-core
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add rust/uica-core/src/matcher.rs rust/uica-core/src/engine.rs rust/uica-core/src/sim/frontend.rs rust/uica-core/src/sim/uop_expand.rs rust/uica-core/tests rust/uica-core/src
git commit -m "perf: use borrowed datapack candidates"
```

---

## Task 4: Build One Index Per Invocation and Pass It Through Simulation

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-core/src/engine.rs`
- Modify: `rust/uica-core/src/sim/frontend.rs`

- [ ] **Step 1: Remove no-op support-check index**

In `run_simulation_for_cycles()`, delete this block:

```rust
let check_index = uica_data::DataPackIndex::new(pack.clone());
for inst in &base_instances {
    if !crate::sim::uop_expand::is_instr_supported(inst, &invocation.arch, &check_index) {
        return Err(format!(
            "unsupported instruction for simulator: {}",
            inst.mnemonic
        ));
    }
}
```

Reason: `is_instr_supported()` currently returns true unconditionally.

- [ ] **Step 2: Make `FrontEnd` borrow pack and index**

Change `FrontEnd` to carry lifetime parameters:

```rust
pub struct FrontEnd<'a> {
    ...
    pub pack: &'a uica_data::DataPack,
    pub pack_index: &'a uica_data::DataPackIndex<'a>,
}
```

Change constructors to accept index:

```rust
pub fn new_with_init_policy(
    arch: MicroArchConfig,
    unroll: bool,
    base_instructions: Vec<InstrInstance>,
    alignment_offset: u32,
    pack: &'a uica_data::DataPack,
    pack_index: &'a uica_data::DataPackIndex<'a>,
    init_policy: impl Into<String>,
    simple_front_end: bool,
    no_micro_fusion: bool,
    no_macro_fusion: bool,
) -> Self
```

Update `FrontEnd::new()` similarly.

- [ ] **Step 3: Reuse the passed index for first-round DSB/LSD check**

Remove temporary build:

```rust
let pack_index = uica_data::DataPackIndex::new(pack.clone());
```

Use `pack_index` parameter for:

```rust
populate_and_recompute_cache_blocks(..., pack_index, ...);
```

- [ ] **Step 4: Build one index in `engine_with_pack_internal()` and pass it into simulation**

Keep:

```rust
let index = DataPackIndex::new(pack);
```

Change simulation call to:

```rust
run_simulation_for_cycles(code, &normalized_invocation, pack, &index)
```

Change `run_simulation_for_cycles()` signature:

```rust
fn run_simulation_for_cycles(
    code: &[u8],
    invocation: &Invocation,
    pack: &DataPack,
    pack_index: &DataPackIndex<'_>,
) -> Result<(crate::sim::FrontEnd<'_>, Vec<UopsForRound>, u32), String>
```

Pass `pack_index` into `FrontEnd::new_with_init_policy()`.

- [ ] **Step 5: Fix tests that return `FrontEnd`**

Tests/helpers that return `FrontEnd` from a locally-created `DataPack` need either:

```rust
let pack = Box::leak(Box::new(pack));
let index = Box::leak(Box::new(uica_data::DataPackIndex::new(pack)));
```

or should keep `pack` and `index` in the same test scope without returning `FrontEnd`. Prefer local-scope ownership where possible; use `Box::leak` only for test helper returns.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo check -q -p uica-core
cargo test -q -p uica-core
cargo test --workspace
```

Expected: pass.

- [ ] **Step 7: Verify bhive_skl_1k correctness**

Capture previous commit baseline, then verify current:

```bash
PREV=$(git rev-parse HEAD)
# after implementation commit candidate exists, replace PREV with parent SHA if needed
BASE=/tmp/uica-bhive1k-borrowed-index-baseline-$(date +%Y%m%d-%H%M%S)
TAG=pre-borrowed-index
mkdir -p "$BASE"

python3 verification/tools/capture.py \
  --profile bhive_skl_1k \
  --engine rust \
  --rust-bin target/release/uica-cli \
  --golden-root "$BASE" \
  --golden-tag "$TAG" \
  --jobs 8

python3 verification/tools/verify.py \
  --profile bhive_skl_1k \
  --engine rust \
  --rust-bin target/release/uica-cli \
  --golden-root "$BASE" \
  --golden-tag "$TAG" \
  --jobs 8
```

Expected: `1000 case/arch result(s) matched`.

- [ ] **Step 8: Run serial perf timing**

Use generated script:

```bash
TIMEFORMAT=$'real %3R\nuser %3U\nsys %3S'
{ time PERF_MODE=none /tmp/uica-rust-bhive1k.sh >/tmp/uica-borrowed-index.noperf.out; } 2>/tmp/uica-borrowed-index.noperf.time
cat /tmp/uica-borrowed-index.noperf.time

PERF_OUT=/tmp/uica-borrowed-index.perf.data \
PERF_TXT=/tmp/uica-borrowed-index.perf.txt \
/tmp/uica-rust-bhive1k.sh >/tmp/uica-borrowed-index.perf.out
```

Expected: `DataPackIndex::new` drops sharply in `/tmp/uica-borrowed-index.perf.txt`.

- [ ] **Step 9: Commit**

```bash
git add rust/uica-core/src/engine.rs rust/uica-core/src/sim/frontend.rs rust/uica-core/tests rust/uica-core/src
git commit -m "perf: reuse borrowed datapack index"
```

---

## Review Gate After Phase 1

**Subagents:** two `reviewer` runs.

- [ ] **Spec compliance review**

Dispatch reviewer with:

```text
Review commits from Task 1-4. Confirm DataPackIndex no longer clones InstructionRecord records, index is built once per engine invocation, run_simulation_for_cycles no longer builds its own no-op index, and bhive_skl_1k verification remains green. No edits.
```

- [ ] **Quality/performance review**

Dispatch reviewer with:

```text
Review borrowed DataPackIndex implementation for lifetime hazards, unnecessary collections, behavior drift in candidate order, and perf regressions. Inspect perf report /tmp/uica-borrowed-index.perf.txt if present. No edits.
```

- [ ] **Parent gate**

Proceed to Phase 2 only if:

```text
cargo test --workspace passes
bhive_skl_1k verify passes
reviewers find no correctness blockers
```

---

## Task 5: Add UiPackViewIndex Parity Tests

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-data/tests/uipack_view.rs`
- Modify: `rust/uica-data/src/uipack.rs` only after failing test.

- [ ] **Step 1: Add view-index test for iform-prefix fallback**

Add a test mirroring `DataPackIndex` noncanonical string behavior. The test should build a minimal pack with an instruction whose displayed string mnemonic normalizes differently from iform prefix, encode to uipack, open `UiPackView`, build `UiPackViewIndex`, and assert lookup by iform-prefix mnemonic returns the record index.

Test shape:

```rust
#[test]
fn view_index_indexes_noncanonical_string_under_iform_prefix() {
    let pack = minimal_pack_with_record("SKL", "VADDPD_YMMqq_YMMqq_YMMqq", "VADD (YMM, YMM, YMM)");
    let bytes = encode_uipack(&pack, "SKL").unwrap();
    let mapped = MappedUiPack::from_bytes(bytes);
    let view = mapped.view().unwrap();
    let index = UiPackViewIndex::new(&view).unwrap();

    let candidates = index.record_indices_for_mnemonic("VADDPD");
    assert_eq!(candidates.len(), 1);
    assert_eq!(view.record(candidates[0]).unwrap().iform(), "VADDPD_YMMqq_YMMqq_YMMqq");
}
```

Use actual helpers already present in `rust/uica-data/tests/uipack_view.rs`.

- [ ] **Step 2: Run failing test**

```bash
cargo test -q -p uica-data --test uipack_view view_index_indexes_noncanonical_string_under_iform_prefix
```

Expected: fail if current `UiPackViewIndex` does not index normalized iform prefix under mnemonic.

- [ ] **Step 3: Fix `UiPackViewIndex::new()`**

In `rust/uica-data/src/uipack.rs`, mirror `DataPackIndex` behavior:

```rust
let string_mnemonic = crate::index::normalize_mnemonic(record.string());
let iform_mnemonic = crate::index::normalize_iform_prefix(record.iform());

by_mnemonic.entry(string_mnemonic.clone()).or_insert_with(Vec::new).push(index);
if string_mnemonic != iform_mnemonic {
    by_mnemonic.entry(iform_mnemonic).or_insert_with(Vec::new).push(index);
}
```

Keep `by_iform` exact full-iform index unchanged.

- [ ] **Step 4: Run data tests**

```bash
cargo test -q -p uica-data --test uipack_view
cargo test -q -p uica-data
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add rust/uica-data/src/uipack.rs rust/uica-data/tests/uipack_view.rs
git commit -m "test: align uipack view index with datapack index"
```

---

## Task 6: Design UiPack Runtime Context API Without Engine Integration

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`
- Modify: `rust/uica-data/src/manifest.rs`
- Modify: `rust/uica-data/src/lib.rs`
- Add tests: `rust/uica-data/tests/uipack_runtime.rs`

- [ ] **Step 1: Add owned runtime wrapper avoiding self-referential view storage**

Add to `rust/uica-data/src/uipack.rs`:

```rust
pub struct MappedUiPackRuntime {
    mapped: MappedUiPack,
    index: UiPackViewIndex,
}

impl MappedUiPackRuntime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, UiPackError> {
        let mapped = MappedUiPack::open(path)?;
        let index = {
            let view = mapped.view()?;
            UiPackViewIndex::new(&view)?
        };
        Ok(Self { mapped, index })
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, UiPackError> {
        let mapped = MappedUiPack::from_bytes(bytes);
        let index = {
            let view = mapped.view()?;
            UiPackViewIndex::new(&view)?
        };
        Ok(Self { mapped, index })
    }

    pub fn view(&self) -> Result<UiPackView<'_>, UiPackError> {
        self.mapped.view()
    }

    pub fn index(&self) -> &UiPackViewIndex {
        &self.index
    }
}
```

This stores owner + index only. It recreates cheap borrowed `UiPackView` when needed and avoids storing view borrowing its owner.

- [ ] **Step 2: Add manifest loader returning runtime**

In `rust/uica-data/src/manifest.rs`, add:

```rust
pub fn load_manifest_runtime(
    manifest_path: impl AsRef<Path>,
    arch: &str,
) -> Result<MappedUiPackRuntime, ManifestLoadError> {
    let manifest = load_manifest(&manifest_path)?;
    let (pack_path, manifest_arch) = resolve_manifest_pack_path(&manifest_path, &manifest, arch)?;
    let runtime = MappedUiPackRuntime::open(&pack_path)?;
    let view = runtime.view()?;
    // Reuse existing version/checksum/schema/arch validation logic from load_manifest_pack.
    // Return same ManifestLoadError variants on mismatch.
    Ok(runtime)
}
```

Do not leave a comment as implementation. Factor current validation from `load_manifest_pack()` into a helper:

```rust
fn validate_manifest_view(manifest: &DataPackManifest, manifest_arch: &str, view: &UiPackView<'_>) -> Result<(), ManifestLoadError>
```

Use it from both `load_manifest_pack()` and `load_manifest_runtime()`.

- [ ] **Step 3: Export runtime API**

In `rust/uica-data/src/lib.rs`, export:

```rust
pub use manifest::load_manifest_runtime;
pub use uipack::MappedUiPackRuntime;
```

- [ ] **Step 4: Add runtime tests**

Create `rust/uica-data/tests/uipack_runtime.rs` with tests:

```rust
#[test]
fn runtime_keeps_view_and_index_available() {
    let pack = minimal_pack("SKL");
    let bytes = encode_uipack(&pack, "SKL").unwrap();
    let runtime = MappedUiPackRuntime::from_bytes(bytes).unwrap();
    let view = runtime.view().unwrap();
    let candidates = runtime.index().record_indices_for_mnemonic("ADD");

    assert_eq!(view.arch(), "SKL");
    assert_eq!(candidates.len(), 1);
}
```

Use existing helpers from nearby tests or define complete local helpers.

- [ ] **Step 5: Run data tests**

```bash
cargo test -q -p uica-data --test uipack_runtime
cargo test -q -p uica-data
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add rust/uica-data/src/uipack.rs rust/uica-data/src/manifest.rs rust/uica-data/src/lib.rs rust/uica-data/tests/uipack_runtime.rs
git commit -m "feat: add mapped uipack runtime context"
```

---

## Task 7: Introduce Record Access Abstraction for Owned Records First

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-core/src/matcher.rs`
- Modify: `rust/uica-core/src/sim/uop_expand.rs`
- Modify: `rust/uica-core/src/engine.rs`

- [ ] **Step 1: Add trait for matcher-needed fields**

In `rust/uica-core/src/matcher.rs`, add:

```rust
pub trait InstrRecordLike {
    fn arch(&self) -> &str;
    fn iform(&self) -> &str;
    fn string(&self) -> &str;
    fn xml_attrs(&self) -> &std::collections::BTreeMap<String, String>;
    fn imm_zero(&self) -> bool;
    fn operands(&self) -> &[uica_data::OperandRecord];
    fn perf(&self) -> &uica_data::PerfRecord;
}

impl InstrRecordLike for uica_data::InstructionRecord {
    fn arch(&self) -> &str { &self.arch }
    fn iform(&self) -> &str { &self.iform }
    fn string(&self) -> &str { &self.string }
    fn xml_attrs(&self) -> &std::collections::BTreeMap<String, String> { &self.xml_attrs }
    fn imm_zero(&self) -> bool { self.imm_zero }
    fn operands(&self) -> &[uica_data::OperandRecord] { &self.perf.operands }
    fn perf(&self) -> &uica_data::PerfRecord { &self.perf }
}
```

- [ ] **Step 2: Make matcher generic over `InstrRecordLike`**

Change `match_instruction_record_iter` to accept:

```rust
pub fn match_instruction_record_iter<'a, R, I>(
    normalized: NormalizedInstrRef<'_>,
    candidates: I,
) -> Option<&'a R>
where
    R: InstrRecordLike + 'a,
    I: IntoIterator<Item = &'a R>,
```

Update internals from field access to trait methods. Keep wrapper for owned records.

- [ ] **Step 3: Keep uop expansion on owned records**

Do not convert all `uop_expand` functions yet. This task should compile without runtime behavior changes. If matcher generic changes require annotations, add explicit type where needed:

```rust
let record: &uica_data::InstructionRecord = match_instruction_record_iter(norm, candidates)?;
```

- [ ] **Step 4: Run tests and verify**

```bash
cargo test -q -p uica-core
cargo test --workspace
python3 verification/tools/verify.py --profile bhive_skl_1k --engine rust --rust-bin target/release/uica-cli --golden-root "$BASE" --golden-tag "$TAG" --jobs 8
```

Expected: pass and `1000 matched`.

- [ ] **Step 5: Commit**

```bash
git add rust/uica-core/src/matcher.rs rust/uica-core/src/sim/uop_expand.rs rust/uica-core/src/engine.rs
git commit -m "refactor: abstract instruction record matching"
```

---

## Task 8: Prototype UiPack Runtime Lookup Without Replacing CLI Default

**Subagent:** `worker`

**Files:**

- Modify: `rust/uica-core/src/engine.rs`
- Modify: `rust/uica-data/src/uipack.rs` if record materialization helper belongs there.
- Add tests under `rust/uica-core/tests/` or `rust/uica-data/tests/`.

- [ ] **Step 1: Add selected-record materialization helper**

Add helper that converts one `UiPackRecordView` to owned `InstructionRecord` only for matched records:

```rust
pub fn record_view_to_instruction_record(record: UiPackRecordView<'_>) -> Result<InstructionRecord, UiPackError> {
    let all_ports = record.view().all_ports();
    let alu_ports = record.view().alu_ports();
    let mut ports = BTreeMap::new();
    for port in record.ports()? {
        ports.insert(port.key().to_string(), port.count());
    }
    Ok(InstructionRecord { /* same fields as to_data_pack for one record */ })
}
```

If `UiPackRecordView` lacks `view()` accessor, add:

```rust
pub fn view(&self) -> &'a UiPackView<'a> { self.view }
```

This is not final zero-copy, but it avoids materializing every instruction in the pack.

- [ ] **Step 2: Add hidden engine entry for experiment**

Add non-public or crate-visible experimental function in `engine.rs`:

```rust
fn engine_output_with_uipack_runtime(
    code: &[u8],
    invocation: &Invocation,
    runtime: &uica_data::MappedUiPackRuntime,
    include_reports: bool,
) -> Result<EngineOutput, String>
```

Initial implementation may materialize only selected candidate records into a small temporary `DataPack` for the current decoded snippet, then call existing `engine_with_pack_internal()`. This keeps behavior safe while measuring pack-wide clone reduction.

- [ ] **Step 3: Add equality test against owned path**

Test a small SKL block:

```rust
#[test]
fn uipack_runtime_experimental_path_matches_owned_pack() {
    let code = vec![0x48, 0x01, 0xd8];
    let invocation = Invocation { arch: "SKL".to_string(), ..Invocation::default() };
    let pack = load_manifest_pack(manifest_path(), "SKL").unwrap();
    let runtime = load_manifest_runtime(manifest_path(), "SKL").unwrap();

    let owned = engine_output_with_pack(&code, &invocation, &pack, false).unwrap().result;
    let mapped = engine_output_with_uipack_runtime(&code, &invocation, &runtime, false).unwrap().result;

    assert_eq!(mapped.summary.throughput_cycles_per_iteration, owned.summary.throughput_cycles_per_iteration);
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -q -p uica-core uipack_runtime_experimental_path_matches_owned_pack
cargo test --workspace
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add rust/uica-core/src/engine.rs rust/uica-data/src/uipack.rs rust/uica-core/tests rust/uica-data/tests
git commit -m "perf: prototype mapped uipack runtime lookup"
```

---

## Phase 2 Review Gate

**Subagents:** `reviewer` x2.

- [ ] **Spec compliance review**

Prompt:

```text
Review Phase 2 commits. Confirm UiPackViewIndex parity tests exist, mapped runtime wrapper avoids self-referential storage, owned engine path remains default, and experimental runtime path matches owned output for tested cases. No edits.
```

- [ ] **Design risk review**

Prompt:

```text
Review experimental UiPack runtime design for lifetime unsafety, repeated JSON blob parsing, hidden full-pack materialization, and mismatch with DataPackIndex semantics. Recommend whether to proceed to CLI integration or add cache/trait work first. No edits.
```

Proceed only if reviewers agree no correctness blocker exists.

---

## Task 9: Decide and Implement CLI Runtime Switch Only After Prototype Evidence

**Subagent:** `worker`, only after user approval.

**Files:**

- Modify: `rust/uica-core/src/engine.rs`
- Modify: `rust/uica-cli/src/main.rs` only if public API changes require it.

- [ ] **Step 1: Capture prototype perf evidence**

Run owned path and experimental path on 1000-run batch or equivalent script. Required metrics:

```text
DataPackIndex::new children %
to_data_pack/materialization cost
run time no perf
run time under perf
bhive_skl_1k correctness
```

- [ ] **Step 2: Ask user before default switch**

Ask one focused question:

```text
Prototype mapped UiPack runtime results: [numbers]. Switch CLI default now, or keep experimental and add record-view/cache abstraction first?
```

- [ ] **Step 3: Implement chosen path**

If switching default, update `load_default_pack` equivalent to return runtime-backed engine path for `.uipack` manifest loads. Keep tests that compare owned vs runtime output.

- [ ] **Step 4: Full verification**

```bash
cargo build -q -r -p uica-cli
cargo test --workspace
python3 verification/tools/verify.py --profile bhive_skl_1k --engine rust --rust-bin target/release/uica-cli --golden-root "$BASE" --golden-tag "$TAG" --jobs 8
PERF_MODE=none /tmp/uica-rust-bhive1k.sh >/tmp/uica-runtime-default.out
PERF_OUT=/tmp/uica-runtime-default.perf.data PERF_TXT=/tmp/uica-runtime-default.perf.txt /tmp/uica-rust-bhive1k.sh
```

Expected:

```text
workspace tests pass
bhive_skl_1k: 1000 matched
DataPackIndex::new no longer appears as major hotspot
```

- [ ] **Step 5: Commit**

```bash
git add rust/uica-core/src/engine.rs rust/uica-cli/src/main.rs rust/uica-core/tests rust/uica-data/tests
git commit -m "perf: use mapped uipack runtime path"
```

---

## Final Verification

Run after all approved tasks:

```bash
cargo fmt --check
cargo build -q -r -p uica-cli
cargo test --workspace
python3 verification/tools/verify.py --profile bhive_skl_1k --engine rust --rust-bin target/release/uica-cli --golden-root "$BASE" --golden-tag "$TAG" --jobs 8
```

Serial timing/perf:

```bash
TIMEFORMAT=$'real %3R\nuser %3U\nsys %3S'
{ time PERF_MODE=none /tmp/uica-rust-bhive1k.sh >/tmp/uica-final.noperf.out; } 2>/tmp/uica-final.noperf.time
cat /tmp/uica-final.noperf.time

PERF_OUT=/tmp/uica-final.perf.data PERF_TXT=/tmp/uica-final.perf.txt /tmp/uica-rust-bhive1k.sh >/tmp/uica-final.perf.out
rg -n "DataPackIndex::new|to_data_pack|run_simulation_for_cycles|FrontEnd::cycle|build_cycles_json" /tmp/uica-final.perf.txt | sed -n '1,120p'
```

Expected final report:

```text
Correctness: 1000 matched
Timing: compare against /tmp/uica-rust-bhive1k.1000.v2.noperf.time
Perf: DataPackIndex::new significantly reduced or removed from top hotspots
```

## Stop Rules

Stop and ask user if:

- Borrowed `FrontEnd<'a>` causes large API churn beyond listed files.
- `UiPackView` runtime requires decoding JSON blobs so often that perf regresses.
- Any `bhive_skl_1k` mismatch appears.
- Switching CLI default would remove owned `DataPack` APIs used by tests or wasm.
