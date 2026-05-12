# Engine Simulate Request API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse `uica-core::engine` public entry points into one request-shaped simulation API.

**Architecture:** Add `SimulationRequest`, `SimulationInput`, `UipackSource`, `SimulationOptions`, and `SimulationOutput`. Route raw bytes and decoded IR through one `simulate(request)` function, then migrate CLI/wasm/emscripten/tests and remove old wrapper APIs.

**Tech Stack:** Rust workspace, `uica-core`, `uica-cli`, `uica-wasm`, `uica-emscripten`, Cargo tests.

---

## Task 1: Add request-shaped API alongside old wrappers

Files:

- Modify `rust/uica-core/src/engine.rs`
- Modify `rust/uica-core/src/lib.rs`

Steps:

- Add public types:
  - `SimulationRequest<'a>`
  - `SimulationInput<'a>` with `Decoded(&'a [DecodedInstruction])` and cfg `Bytes(&'a [u8])`
  - `UipackSource<'a>` with `Runtime(&'a MappedUiPackRuntime)` and `Default { verify_checksum: bool }`
  - `SimulationOptions`
  - `MissingUipackPolicy`
  - `DecodeErrorPolicy`
  - `SimulationOutput`
- Implement `Default` for options.
- Add `pub fn simulate(request: SimulationRequest<'_>) -> Result<SimulationOutput, String>`.
- `simulate` does decode, runtime resolution, fallback/error policy, and delegates to `simulate_with_decoded_data_internal`.
- Keep old wrappers temporarily; make them call new `simulate`.
- Re-export new API types from `lib.rs` if needed.
- Run `cargo test -p uica-core`.

## Task 2: Migrate crate callers to new API

Files:

- Modify `rust/uica-cli/src/main.rs`
- Modify `rust/uica-emscripten/src/lib.rs`
- Modify `rust/uica-wasm/src/lib.rs`

Steps:

- CLI uses `engine::simulate(SimulationRequest { input: Bytes(&bytes), uipack: Default { verify_checksum: args.verify_uipack }, options: ... })`.
- Event trace uses same API with `include_trace=true`.
- Reports use `include_reports=true`.
- wasm decoded-with-uipack uses `SimulationInput::Decoded` + `UipackSource::Runtime`.
- emscripten uses `SimulationInput::Bytes` + `UipackSource::Runtime` + reports.
- Run `cargo test -p uica-cli -p uica-wasm -p uica-emscripten`.

## Task 3: Migrate core tests and remove old wrappers

Files:

- Modify `rust/uica-core/src/engine.rs`
- Modify `rust/uica-core/tests/*.rs`
- Modify any remaining workspace callers.

Steps:

- Replace uses of old APIs:
  - `engine_output`
  - `engine_with_decoded`
  - `simulate_with_decoded_input`
  - `simulate_with_decoded_uipack`
  - `simulate_output_with_uipack_runtime`
  - `engine_trace`
- Delete old wrapper functions.
- Keep only `simulate(request)` as public entry point in `engine.rs`.
- Update `lib.rs` to export only `simulate` + request types.
- Run `rg "engine_output|engine_with_decoded|simulate_with_decoded_input|simulate_with_decoded_uipack|simulate_output_with_uipack_runtime|engine_trace\(" rust -g'*.rs'` expecting no old call sites.
- Run `cargo test --workspace`.

## Task 4: Verify parity

Steps:

- Run `cargo fmt --all -- --check`.
- Run `cargo test --workspace`.
- Run BHive 1k SKL/HSW/IVB if time permits.
