# uiCA Decode IR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split decoded-instruction data structures into `uica-decode-ir` so Rust analysis can run from caller-supplied decoded IR without linking XED.

**Architecture:** Create a neutral IR crate with serde-capable `DecodedInstruction` and `DecodedMemAddr`. Make XED produce that IR, make core consume that IR, and expose decoded-IR entrypoints. Keep byte-decoding entrypoints behind an optional `xed-decoder` feature.

**Tech Stack:** Rust workspace crates, Cargo features, serde JSON, existing `uica-data` UIPack runtime.

---

### Task 1: Add failing decoded-IR core test

**Files:**

- Create: `rust/uica-core/tests/decoded_ir_engine.rs`
- Modify later: `rust/uica-core/src/engine.rs`
- Modify later: `rust/uica-core/Cargo.toml`

- [ ] **Step 1: Write test proving core can analyze decoded IR**

```rust
use std::collections::{BTreeMap, BTreeSet};

use uica_data::{DataPack, InstructionRecord, PerfRecord};
use uica_decode_ir::DecodedInstruction;
use uica_model::Invocation;

#[test]
fn analyzes_caller_supplied_decoded_ir_with_pack() {
    let decoded = vec![DecodedInstruction {
        ip: 0,
        len: 3,
        mnemonic: "add".to_string(),
        disasm: "add rax, rbx".to_string(),
        bytes: vec![0x48, 0x01, 0xd8],
        pos_nominal_opcode: 1,
        input_regs: vec!["RAX".to_string(), "RBX".to_string()],
        output_regs: vec!["RAX".to_string()],
        reads_flags: false,
        writes_flags: true,
        has_memory_read: false,
        has_memory_write: false,
        mem_addrs: vec![],
        implicit_rsp_change: 0,
        immediate: None,
        immediate_width_bits: 0,
        has_66_prefix: false,
        iform: "ADD_GPRv_GPRv".to_string(),
        iform_signature: "ADD_GPRv_GPRv".to_string(),
        max_op_size_bytes: 8,
        uses_high8_reg: false,
        explicit_reg_operands: vec!["RAX".to_string(), "RBX".to_string()],
        agen: None,
        xml_attrs: BTreeMap::new(),
    }];

    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        all_ports: vec!["0".to_string(), "1".to_string(), "5".to_string(), "6".to_string()],
        alu_ports: vec!["0".to_string(), "1".to_string(), "5".to_string(), "6".to_string()],
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "ADD_GPRv_GPRv".to_string(),
            string: "ADD".to_string(),
            all_ports: vec!["0".to_string(), "1".to_string(), "5".to_string(), "6".to_string()],
            alu_ports: vec!["0".to_string(), "1".to_string(), "5".to_string(), "6".to_string()],
            locked: false,
            xml_attrs: BTreeMap::new(),
            imm_zero: false,
            perf: PerfRecord {
                operands: vec![],
                latencies: vec![],
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: Some(1.0),
                ports: BTreeMap::from([("0156".to_string(), 1)]),
                div_cycles: 0,
                may_be_eliminated: false,
                complex_decoder: false,
                n_available_simple_decoders: 0,
                lcp_stall: false,
                implicit_rsp_change: 0,
                can_be_used_by_lsd: false,
                cannot_be_in_dsb_due_to_jcc_erratum: false,
                no_micro_fusion: false,
                no_macro_fusion: false,
                macro_fusible_with: vec![],
                variants: BTreeMap::new(),
            },
        }],
    };

    let result = uica_core::engine::engine_with_decoded_pack(
        &decoded,
        &Invocation {
            arch: "SKL".to_string(),
            ..Invocation::default()
        },
        &pack,
    );

    assert_eq!(result.invocation.arch, "SKL");
    assert_eq!(result.summary.mode, "unroll");
    assert!(result.summary.throughput_cycles_per_iteration.is_some());
    assert_eq!(result.parameters["uArchName"], "SKL");
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test -p uica-core --test decoded_ir_engine analyzes_caller_supplied_decoded_ir_with_pack`
Expected: FAIL because `uica_decode_ir` crate and `engine_with_decoded_pack` do not exist.

### Task 2: Create neutral IR crate

**Files:**

- Create: `rust/uica-decode-ir/Cargo.toml`
- Create: `rust/uica-decode-ir/src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

- [ ] **Step 1: Add workspace member**

Add `"rust/uica-decode-ir"` to root workspace before `uica-decoder`.

- [ ] **Step 2: Add crate manifest**

```toml
[package]
name = "uica-decode-ir"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 3: Move IR structs with serde derives**

```rust
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedMemAddr {
    pub base: Option<String>,
    pub index: Option<String>,
    pub scale: i32,
    pub disp: i64,
    pub is_implicit_stack_operand: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedInstruction {
    pub ip: u64,
    pub len: u32,
    pub mnemonic: String,
    pub disasm: String,
    pub bytes: Vec<u8>,
    pub pos_nominal_opcode: u32,
    pub input_regs: Vec<String>,
    pub output_regs: Vec<String>,
    pub reads_flags: bool,
    pub writes_flags: bool,
    pub has_memory_read: bool,
    pub has_memory_write: bool,
    pub mem_addrs: Vec<DecodedMemAddr>,
    pub implicit_rsp_change: i32,
    pub immediate: Option<i64>,
    pub immediate_width_bits: u32,
    pub has_66_prefix: bool,
    pub iform: String,
    pub iform_signature: String,
    pub max_op_size_bytes: u8,
    pub uses_high8_reg: bool,
    pub explicit_reg_operands: Vec<String>,
    pub agen: Option<String>,
    pub xml_attrs: BTreeMap<String, String>,
}
```

- [ ] **Step 4: Run crate test/check**

Run: `cargo check -p uica-decode-ir`
Expected: PASS.

### Task 3: Make XED produce neutral IR

**Files:**

- Modify: `rust/uica-xed/Cargo.toml`
- Modify: `rust/uica-xed/src/lib.rs`
- Modify: `rust/uica-decoder/Cargo.toml`

- [ ] **Step 1: Add `uica-decode-ir` dependency to `uica-xed`**

```toml
uica-decode-ir = { path = "../uica-decode-ir" }
```

- [ ] **Step 2: Replace local struct definitions with re-export**

In `rust/uica-xed/src/lib.rs`, remove local `DecodedMemAddr` and `DecodedInstruction` definitions and add:

```rust
pub use uica_decode_ir::{DecodedInstruction, DecodedMemAddr};
```

- [ ] **Step 3: Keep decoder public API unchanged**

`uica-xed::decode_raw` and `uica-decoder::decode_raw` still return `Vec<DecodedInstruction>` and existing decoder tests keep passing.

- [ ] **Step 4: Run decoder tests**

Run: `cargo test -p uica-xed -p uica-decoder`
Expected: PASS.

### Task 4: Make core consume decoded IR and gate byte decoding

**Files:**

- Modify: `rust/uica-core/Cargo.toml`
- Modify: `rust/uica-core/src/engine.rs`
- Modify: `rust/uica-core/src/lib.rs`
- Modify: `rust/uica-core/src/sim/types.rs`
- Modify: `rust/uica-core/src/sim/frontend.rs`

- [ ] **Step 1: Add features/dependencies**

```toml
[features]
default = ["xed-decoder"]
xed-decoder = ["dep:uica-decoder"]

[dependencies]
serde_json = "1"
uica-model = { path = "../uica-model" }
uica-data = { path = "../uica-data" }
uica-decode-ir = { path = "../uica-decode-ir" }
uica-decoder = { path = "../uica-decoder", optional = true }
```

- [ ] **Step 2: Add decoded entrypoints**

Add functions:

```rust
pub fn engine_with_decoded(decoded: &[DecodedInstruction], invocation: &Invocation) -> UicaResult;
pub fn engine_output_with_decoded(decoded: &[DecodedInstruction], invocation: &Invocation, include_reports: bool, verify_uipack: bool) -> Result<EngineOutput, String>;
pub fn engine_with_decoded_pack(decoded: &[DecodedInstruction], invocation: &Invocation, pack: &DataPack) -> UicaResult;
pub fn engine_output_with_decoded_pack(decoded: &[DecodedInstruction], invocation: &Invocation, pack: &DataPack, include_reports: bool) -> Result<EngineOutput, String>;
pub fn engine_output_with_decoded_uipack_runtime(decoded: &[DecodedInstruction], invocation: &Invocation, runtime: &MappedUiPackRuntime, include_reports: bool) -> Result<EngineOutput, String>;
```

- [ ] **Step 3: Keep byte entrypoints behind `xed-decoder`**

Annotate existing byte APIs with `#[cfg(feature = "xed-decoder")]`:

```rust
pub fn engine_output(code: &[u8], invocation: &Invocation, include_reports: bool, verify_uipack: bool) -> Result<EngineOutput, String>;
pub fn engine(code: &[u8], invocation: &Invocation) -> UicaResult;
pub fn engine_with_pack(code: &[u8], invocation: &Invocation, pack: &DataPack) -> UicaResult;
pub fn engine_output_with_pack(code: &[u8], invocation: &Invocation, pack: &DataPack, include_reports: bool) -> Result<EngineOutput, String>;
pub fn engine_output_with_uipack_runtime(code: &[u8], invocation: &Invocation, runtime: &MappedUiPackRuntime, include_reports: bool) -> Result<EngineOutput, String>;
```

- [ ] **Step 4: Refactor internals from code bytes to decoded slices**

Change `engine_with_pack_internal`, `materialize_runtime_pack_for_code`, `run_simulation_for_cycles`, and `build_instructions_json_from_decode` to decoded-slice variants. Byte entrypoints decode once and forward.

- [ ] **Step 5: Update type references**

Replace core signatures using `uica_decoder::DecodedInstruction` and `uica_decoder::DecodedMemAddr` with `uica_decode_ir::DecodedInstruction` and `uica_decode_ir::DecodedMemAddr`.

- [ ] **Step 6: Run red test again**

Run: `cargo test -p uica-core --test decoded_ir_engine analyzes_caller_supplied_decoded_ir_with_pack`
Expected: PASS.

- [ ] **Step 7: Verify no-XED core compile**

Run: `cargo check -p uica-core --no-default-features`
Expected: PASS and no `uica-decoder` required for library build.

### Task 5: Add Rust-only wasm decoded JSON API

**Files:**

- Modify: `rust/uica-wasm/Cargo.toml`
- Modify: `rust/uica-wasm/src/lib.rs`
- Modify: `rust/uica-wasm/tests/node_smoke.rs`

- [ ] **Step 1: Add failing test for decoded JSON API**

Add test that serializes one `DecodedInstruction`, calls `analyze_decoded_json`, and verifies result JSON has `engine = "rust"`, `invocation.arch = "SKL"`, and non-null throughput.

- [ ] **Step 2: Run failing test**

Run: `cargo test -p uica-wasm --test node_smoke analyze_decoded_json_returns_rust_result_json`
Expected: FAIL because API does not exist.

- [ ] **Step 3: Make wasm depend on Rust-only core path**

Use:

```toml
uica-core = { path = "../uica-core", default-features = false }
uica-decode-ir = { path = "../uica-decode-ir" }
```

- [ ] **Step 4: Add API**

```rust
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_decoded_json(decoded_json: &str, arch: &str) -> Result<String, String> {
    let decoded: Vec<uica_decode_ir::DecodedInstruction> = serde_json::from_str(decoded_json)
        .map_err(|err| err.to_string())?;
    let invocation = Invocation { arch: arch.to_string(), ..Invocation::default() };
    serde_json::to_string(&engine::engine_with_decoded(&decoded, &invocation))
        .map_err(|err| err.to_string())
}
```

- [ ] **Step 5: Change raw-byte wasm API to explicit unsupported path**

`analyze_hex` keeps hex validation but returns `Err("raw x86 byte analysis requires an XED-enabled wasm build; use analyze_decoded_json")` after validation in Rust-only wasm.

- [ ] **Step 6: Run wasm tests**

Run: `cargo test -p uica-wasm`
Expected: PASS.

### Task 6: Full verification

**Files:**

- Verify all touched crates.

- [ ] **Step 1: Run core/decoder/wasm checks**

Run:

```bash
cargo test -p uica-decode-ir -p uica-xed -p uica-decoder -p uica-core -p uica-wasm
cargo check -p uica-core --no-default-features
cargo check -p uica-wasm --no-default-features
```

Expected: all commands exit 0.

- [ ] **Step 2: Inspect dependency separation**

Run:

```bash
cargo tree -p uica-wasm --no-default-features | rg "uica-(xed|decoder)" || true
```

Expected: no `uica-xed`, `uica-xed-sys`, or `uica-decoder` entries.

- [ ] **Step 3: Report result**

Summarize changed files, test commands, and any remaining wasm/XED work.
