# UIPack Binary Blobs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace UIPack per-record JSON blobs with binary encoded sections so runtime no longer uses `serde_json` to read instruction data from `.uipack` files.

**Architecture:** Keep the current staged runtime shape: data generation may still build `DataPack`, and engine/sim may still materialize candidate `InstructionRecord`s. Change UIPack v9 blob payloads from JSON bytes to compact little-endian binary payloads while preserving existing public `UiPackRecordView::{operands,latencies,variants,macro_fusible_with,xml_attrs}` methods. This removes runtime JSON parse first; direct `UiPackRecordView` engine consumption remains later work.

**Tech Stack:** Rust workspace, `uica-data`, `uica-data-gen`, `uica-core`, generated `.uipack` fixtures, Cargo tests.

---

## Files

- Modify: `rust/uica-data/src/uipack.rs`
  - Bump `UIPACK_VERSION` from 8 to 9.
  - Add binary encoders/decoders for operands, latencies, variants, macro-fusion strings, and XML attrs.
  - Replace `serde_json::to_vec`/`serde_json::from_slice` usage in UIPack payload path.
  - Keep public view API returning owned structs for now.
- Modify: `rust/uica-data/src/lib.rs`
  - Usually no type changes needed; only adjust docs if needed.
- Modify/generated: `rust/uica-data/generated/manifest.json`
  - Regenerated with v9 header values/checksums.
- Modify/generated: `rust/uica-data/generated/arch/*.uipack`
  - Regenerated v9 binary blobs.
- Test: `rust/uica-data/tests/uipack.rs`
  - Add/adjust roundtrip tests proving nested fields survive binary encoding.
  - Add test proving current encoded bytes do not contain obvious JSON object/array blob markers for nested fields.
- Test: `rust/uica-data-gen/tests/convert_minimal_xml.rs`
  - Existing tests should pass once generator emits v9 packs.
- Test: `rust/uica-emscripten/tests/run_contract.rs`
  - Existing web contract should pass after generated SKL pack update.

---

## Binary Formats

Use existing `RecordEntry` offset/size fields. The field names may remain `operands_offset`, `operands_size`, etc.; their payload changes to binary in v9.

Sentinel values:

```rust
const NONE_U32: u32 = u32::MAX;
```

String values are encoded as offsets into existing UIPack string table. All integers are little-endian.

### Operand blob

```text
u32 count
repeat count:
  u32 name_offset
  u32 type_offset
  u32 flags_bits
  u32 flags_count
  u32 flags_offsets[flags_count]
  u32 flags_read_count
  u32 flags_read_offsets[flags_read_count]
  u32 flags_write_count
  u32 flags_write_offsets[flags_write_count]
  u32 mem_base_offset_or_NONE_U32
  u32 mem_index_offset_or_NONE_U32
  u32 has_mem_scale
  i32 mem_scale_or_0
  u32 has_mem_disp
  i64 mem_disp_or_0
  u32 mem_operand_role_offset_or_NONE_U32
```

`flags_bits`:

```text
bit 0: read
bit 1: write
bit 2: implicit
bit 3: is_agen
```

### Latency blob

```text
u32 count
repeat count:
  u32 start_op_offset
  u32 target_op_offset
  i32 cycles
  u32 option_bits
  i32 cycles_addr_or_0
  i32 cycles_addr_index_or_0
  i32 cycles_mem_or_0
  i32 cycles_same_reg_or_0
```

`option_bits`:

```text
bit 0: cycles_addr present
bit 1: cycles_addr_index present
bit 2: cycles_mem present
bit 3: cycles_same_reg present
```

### Variants blob

```text
u32 variant_count
repeat variant_count:
  u32 variant_name_offset
  u32 option_bits
  i32 uops_or_0
  i32 retire_slots_or_0
  i32 uops_mite_or_0
  i32 uops_ms_or_0
  u64 tp_bits_or_0
  u32 div_cycles_or_0
  u32 complex_decoder_or_0
  u32 n_available_simple_decoders_or_0
  u32 ports_count_if_present_else_0
  repeat ports_count:
    u32 port_name_offset
    i32 port_count
```

`option_bits`:

```text
bit 0: uops present
bit 1: retire_slots present
bit 2: uops_mite present
bit 3: uops_ms present
bit 4: tp present
bit 5: ports present
bit 6: div_cycles present
bit 7: complex_decoder present
bit 8: n_available_simple_decoders present
```

### Macro-fusible blob

```text
u32 count
repeat count:
  u32 string_offset
```

### XML attrs blob

```text
u32 count
repeat count:
  u32 key_offset
  u32 value_offset
```

---

## Task 1: Failing nested-field roundtrip test

**Files:**

- Modify: `rust/uica-data/tests/uipack.rs`

- [ ] **Step 1: Add a test with all nested field categories**

Add one `InstructionRecord` in the existing test module or helper pack with:

```rust
xml_attrs: BTreeMap::from([
    ("agen".to_string(), "1".to_string()),
    ("immzero".to_string(), "true".to_string()),
]),
perf: PerfRecord {
    variants: BTreeMap::from([(
        "_indexed".to_string(),
        PerfVariantRecord {
            uops: Some(2),
            retire_slots: Some(2),
            uops_mite: Some(2),
            uops_ms: Some(0),
            tp: Some(0.5),
            ports: Some(BTreeMap::from([("0".to_string(), 1), ("1".to_string(), 1)])),
            div_cycles: Some(3),
            complex_decoder: Some(true),
            n_available_simple_decoders: Some(1),
        },
    )]),
    macro_fusible_with: vec!["JZ".to_string(), "JNZ".to_string()],
    operands: vec![OperandRecord {
        name: "REG0".to_string(),
        r#type: "reg".to_string(),
        read: true,
        write: false,
        implicit: false,
        flags: vec!["C".to_string(), "SPAZO".to_string()],
        flags_read: vec!["C".to_string()],
        flags_write: vec!["SPAZO".to_string()],
        mem_base: Some("RAX".to_string()),
        mem_index: Some("RBX".to_string()),
        mem_scale: Some(4),
        mem_disp: Some(16),
        is_agen: true,
        mem_operand_role: Some("agen".to_string()),
    }],
    latencies: vec![LatencyRecord {
        start_op: "REG0".to_string(),
        target_op: "REG1".to_string(),
        cycles: 1,
        cycles_addr: Some(2),
        cycles_addr_index: Some(3),
        cycles_mem: Some(4),
        cycles_same_reg: Some(0),
    }],
    ..existing_perf_fields
}
```

Assert:

```rust
let bytes = encode_uipack(&pack, "SKL").unwrap();
let decoded = load_uipack_bytes(&bytes).unwrap();
assert_eq!(decoded, pack);
```

- [ ] **Step 2: Run test and verify baseline**

Run:

```bash
cargo test -p uica-data roundtrips_single_arch_uipack_and_keeps_index_compatibility
```

Expected before implementation: existing roundtrip may pass because JSON preserves fields. This test is a guard, not red by itself.

---

## Task 2: Failing “no JSON blob” test

**Files:**

- Modify: `rust/uica-data/tests/uipack.rs`

- [ ] **Step 1: Add test that detects JSON blob bytes in encoded pack**

Create a pack with unique nested strings. Encode it. Assert binary payload no longer contains JSON key/object markers that old encoding emitted.

```rust
#[test]
fn uipack_nested_payloads_are_binary_not_json() {
    let pack = sample_pack_with_nested_fields();
    let bytes = encode_uipack(&pack, "SKL").unwrap();
    let haystack = String::from_utf8_lossy(&bytes);

    assert!(!haystack.contains("\"start_op\""));
    assert!(!haystack.contains("\"target_op\""));
    assert!(!haystack.contains("\"cycles_same_reg\""));
    assert!(!haystack.contains("\"mem_operand_role\""));
    assert!(!haystack.contains("\"macro_fusible_with\""));
}
```

- [ ] **Step 2: Run test and verify it fails on current JSON encoding**

Run:

```bash
cargo test -p uica-data uipack_nested_payloads_are_binary_not_json
```

Expected failure: assertion fails because current UIPack blobs contain JSON field names.

---

## Task 3: Add binary reader/writer primitives

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Add helper constants and functions near low-level read/write helpers**

Add:

```rust
const NONE_U32: u32 = u32::MAX;

fn push_u32(dst: &mut Vec<u8>, value: u32) { dst.extend_from_slice(&value.to_le_bytes()); }
fn push_i32(dst: &mut Vec<u8>, value: i32) { dst.extend_from_slice(&value.to_le_bytes()); }
fn push_i64(dst: &mut Vec<u8>, value: i64) { dst.extend_from_slice(&value.to_le_bytes()); }
fn push_u64(dst: &mut Vec<u8>, value: u64) { dst.extend_from_slice(&value.to_le_bytes()); }
```

Add a small cursor type:

```rust
struct BlobCursor<'a> { bytes: &'a [u8], offset: usize }
```

with `read_u32`, `read_i32`, `read_i64`, `read_u64` methods returning `Result<_, UiPackError>` and bounds-checking with error text `uipack {name} blob truncated`.

- [ ] **Step 2: Run tests**

Run:

```bash
cargo test -p uica-data
```

Expected: tests still pass except `uipack_nested_payloads_are_binary_not_json` remains failing.

---

## Task 4: Intern nested strings before binary encoding

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Add helper to intern nested record strings**

Add function:

```rust
fn intern_record_blob_strings(record: &InstructionRecord, strings: &mut StringTable) -> Result<(), UiPackError>
```

It must call `strings.intern(...)` for:

- each operand `name`, `type`, every `flags`, `flags_read`, `flags_write`, optional `mem_base`, optional `mem_index`, optional `mem_operand_role`
- each latency `start_op`, `target_op`
- each variant key and each variant port key
- each macro-fusible string
- each xml attr key and value

- [ ] **Step 2: Call helper before building raw records**

Inside `encode_uipack`, after interning `iform`, `string`, and ports for each record, call:

```rust
intern_record_blob_strings(record, &mut strings)?;
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test -p uica-data
```

Expected: existing tests pass except no-JSON test remains failing.

---

## Task 5: Encode/decode operands as binary

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Add `encode_operands` and `decode_operands`**

Implement:

```rust
fn encode_operands(operands: &[OperandRecord], strings: &StringTable) -> Result<Vec<u8>, UiPackError>
fn decode_operands(view: &UiPackView<'_>, bytes: &[u8]) -> Result<Vec<OperandRecord>, UiPackError>
```

Use format from “Operand blob”. Use `strings.offset_of(value)` helper or add `StringTable::get(value) -> Result<u32, UiPackError>`.

- [ ] **Step 2: Replace JSON path for operands only**

In encoder replace:

```rust
let operands = serde_json::to_vec(&record.perf.operands)?;
```

with:

```rust
let operands = encode_operands(&record.perf.operands, &strings)?;
```

In `UiPackRecordView::operands`, replace `serde_json::from_slice(...)` with `decode_operands(self.view, self.blob(...)? )`.

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test -p uica-data uipack
```

Expected: tests pass except no-JSON test still fails due other JSON blobs.

---

## Task 6: Encode/decode latencies as binary

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Add `encode_latencies` and `decode_latencies`**

Implement:

```rust
fn encode_latencies(latencies: &[LatencyRecord], strings: &StringTable) -> Result<Vec<u8>, UiPackError>
fn decode_latencies(view: &UiPackView<'_>, bytes: &[u8]) -> Result<Vec<LatencyRecord>, UiPackError>
```

Use format from “Latency blob”.

- [ ] **Step 2: Replace JSON path for latencies**

Replace `serde_json::to_vec(&record.perf.latencies)?` and `serde_json::from_slice(...)` in `latencies()`.

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test -p uica-data uipack
```

Expected: tests pass except no-JSON test may still fail due variants/xml attrs.

---

## Task 7: Encode/decode variants as binary

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Add `encode_variants` and `decode_variants`**

Implement:

```rust
fn encode_variants(variants: &BTreeMap<String, PerfVariantRecord>, strings: &StringTable) -> Result<Vec<u8>, UiPackError>
fn decode_variants(view: &UiPackView<'_>, bytes: &[u8]) -> Result<BTreeMap<String, PerfVariantRecord>, UiPackError>
```

Use format from “Variants blob”. Preserve `BTreeMap` ordering naturally.

- [ ] **Step 2: Replace JSON path for variants**

Replace `serde_json::to_vec(&record.perf.variants)?` and `serde_json::from_slice(...)` in `variants()`.

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test -p uica-data uipack
```

Expected: tests pass except no-JSON test may still fail due macro/xml attrs.

---

## Task 8: Encode/decode macro-fusion list and XML attrs as binary

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Add string-list and string-map blob helpers**

Implement:

```rust
fn encode_string_list(values: &[String], strings: &StringTable) -> Result<Vec<u8>, UiPackError>
fn decode_string_list(view: &UiPackView<'_>, bytes: &[u8]) -> Result<Vec<String>, UiPackError>
fn encode_string_map(values: &BTreeMap<String, String>, strings: &StringTable) -> Result<Vec<u8>, UiPackError>
fn decode_string_map(view: &UiPackView<'_>, bytes: &[u8]) -> Result<BTreeMap<String, String>, UiPackError>
```

- [ ] **Step 2: Replace JSON path for macro-fusion and XML attrs**

Replace `serde_json::to_vec(&record.perf.macro_fusible_with)?`, `serde_json::to_vec(&record.xml_attrs)?`, and corresponding `from_slice` calls.

- [ ] **Step 3: Run no-JSON test**

Run:

```bash
cargo test -p uica-data uipack_nested_payloads_are_binary_not_json
```

Expected: pass.

---

## Task 9: Bump UIPack version and verify rejection/update path

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`
- Modify tests if exact version assertions exist.

- [ ] **Step 1: Bump version**

Change:

```rust
pub const UIPACK_VERSION: u16 = 8;
```

to:

```rust
pub const UIPACK_VERSION: u16 = 9;
```

- [ ] **Step 2: Run data tests**

Run:

```bash
cargo test -p uica-data
```

Expected: generated-pack-dependent tests may fail until packs regenerated; pure encode/decode tests should pass.

---

## Task 10: Regenerate UIPack files

**Files:**

- Modify: `rust/uica-data/generated/manifest.json`
- Modify: `rust/uica-data/generated/arch/*.uipack`

- [ ] **Step 1: Regenerate from uops.info XML**

Use the uops.info `instructions.xml` at the repository root, matching `setup.sh`. If absent, download it first:

```bash
wget https://www.uops.info/instructions.xml
cargo run -p uica-data-gen -- instructions.xml rust/uica-data/generated
```

Expected: command exits 0 and generated manifest has `"uipack_version": 9`.

- [ ] **Step 2: Verify generated manifest version**

Run:

```bash
python3 - <<'PY'
import json
m=json.load(open('rust/uica-data/generated/manifest.json'))
assert m['uipack_version'] == 9, m['uipack_version']
print('uipack_version', m['uipack_version'])
PY
```

Expected output:

```text
uipack_version 9
```

---

## Task 11: Remove runtime JSON dependency from UIPack path

**Files:**

- Modify: `rust/uica-data/src/uipack.rs`

- [ ] **Step 1: Search for remaining serde JSON in UIPack payload path**

Run:

```bash
rg "serde_json::(to_vec|from_slice)" rust/uica-data/src/uipack.rs
```

Expected: no matches. If matches remain, replace them with binary helper calls.

- [ ] **Step 2: Keep JSON error variant only if used elsewhere**

If `UiPackError::Json` and `impl From<serde_json::Error>` become unused, remove them and adjust imports. If compiler shows they remain useful outside UIPack payloads, keep them.

- [ ] **Step 3: Run compiler/tests**

Run:

```bash
cargo test -p uica-data
```

Expected: pass.

---

## Task 12: Full verification

**Files:**

- No planned edits unless failures require fixes.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: no output or formatting changes only.

- [ ] **Step 2: Run focused crate tests**

Run:

```bash
cargo test -p uica-data -p uica-data-gen -p uica-core -p uica-emscripten
```

Expected: all tests pass.

- [ ] **Step 3: Run smoke CLI if available**

Run:

```bash
cargo run -p uica-cli -- --help >/dev/null
```

Expected: exits 0.

- [ ] **Step 4: Inspect diff**

Run:

```bash
git diff --stat
rg "serde_json::(to_vec|from_slice)" rust/uica-data/src/uipack.rs
```

Expected: diff includes UIPack code and regenerated packs; `rg` returns no matches.

---

## Self-review

- Scope matches selected staged plan: binary blobs first; engine direct-view refactor deferred.
- No runtime JSON parse remains in `uipack.rs` payload access after Task 11.
- Existing public APIs remain stable except UIPACK version and generated binary contents.
- Full direct `UiPackRecordView` engine consumption remains future work, not mixed into this step.
