# Remove Core DataPack APIs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure `uica-core` engine/sim APIs and tests exercise the manifest/UIPack runtime path, not legacy `DataPack` execution APIs.

**Architecture:** Remove pack-based engine and frontend/uop-expand public entry points from `uica-core`. Keep `DataPack` in `uica-data`/`uica-data-gen` as generator/encoder IR. Tests that need synthetic instruction data may construct a `DataPack` only to call `encode_uipack` and then run core through `MappedUiPackRuntime`.

**Tech Stack:** Rust, `uica-core`, `uica-data` UIPack v9, Cargo tests.

---

## Task 1: Remove pack engine APIs

**Files:**
- Modify: `rust/uica-core/src/engine.rs`
- Modify: `rust/uica-core/src/lib.rs`
- Modify: `rust/uica-core/tests/uipack_runtime_engine.rs`

Steps:
- Delete `engine_with_decoded_pack`, `engine_output_with_decoded_pack`, `engine_with_pack`, `engine_output_with_pack` public APIs.
- Remove `engine_with_decoded_pack_internal` and pack branch usage from engine runtime path.
- Remove `engine_with_decoded_pack` re-export from `lib.rs`.
- Update `uipack_runtime_engine.rs` to compare manifest runtime to another runtime call, or delete pack comparison if redundant.
- Run `cargo test -p uica-core uipack_runtime_engine`.

## Task 2: Simplify InstructionDataSource to runtime-only in production

**Files:**
- Modify: `rust/uica-core/src/instruction_data.rs`
- Modify: `rust/uica-core/src/engine.rs`

Steps:
- Remove `InstructionDataSource::Pack` variant.
- Remove `MatchedRecord::Borrowed` variant.
- Keep `MatchedRecord::Owned` or replace with direct owned record return.
- `InstructionDataSource` should wrap only `&MappedUiPackRuntime`.
- Update engine calls accordingly.
- Tests may keep fixture DataPacks only inside `#[cfg(test)]` helpers that encode to UIPack runtime.
- Run `cargo test -p uica-core instruction_data`.

## Task 3: Remove pack frontend/uop-expand APIs

**Files:**
- Modify: `rust/uica-core/src/sim/frontend.rs`
- Modify: `rust/uica-core/src/sim/uop_expand.rs`
- Modify tests in `rust/uica-core/src/sim/*` and `rust/uica-core/tests/*`

Steps:
- Remove `FrontEnd::new(pack,index)` and `new_with_init_policy(pack,index)` public pack constructors.
- Keep `FrontEnd::new_with_runtime(...)` or rename to `new(...)` runtime-only.
- Remove `expand_instr_instance_to_lam_uops_with_storage(pack, pack_index)` legacy wrapper.
- Update tests to create `MappedUiPackRuntime` from `encode_uipack(&fixture_pack, arch)` and call runtime constructors/functions.
- Run `cargo test -p uica-core --lib`.

## Task 4: Migrate core tests off engine_with_pack/load_manifest_pack

**Files:**
- Modify: `rust/uica-core/tests/decoded_ir_engine.rs`
- Modify: `rust/uica-core/tests/engine_summary.rs`
- Modify: `rust/uica-core/tests/manifest_runtime.rs`
- Modify in-module tests in `rust/uica-core/src/engine.rs`, `frontend.rs`, `uop_expand.rs`, `report.rs`

Steps:
- Replace calls to `load_manifest_pack` with `load_manifest_runtime`.
- Replace synthetic `engine_with_pack` calls with helper:
  - build fixture `DataPack`
  - `encode_uipack`
  - `MappedUiPackRuntime::from_bytes`
  - `engine_output_with_uipack_runtime` or `engine_output_with_decoded_uipack_runtime`
- This keeps `DataPack` only as encoder fixture, not core runtime API.
- Run `cargo test -p uica-core`.

## Task 5: Enforce no core DataPack runtime/API usage

**Files:**
- Modify comments/docstrings as needed.

Steps:
- Run:
  `rg "engine_with_pack|engine_output_with_pack|engine_with_decoded_pack|engine_output_with_decoded_pack|InstructionDataSource::Pack|DataPackIndex|load_manifest_pack" rust/uica-core/src rust/uica-core/tests`
- Expected: no production API/runtime matches. Allow `DataPack` only in test fixture helper names/comments where immediately encoded to UIPack.
- Run `cargo test --workspace`.
- Run BHive 1k quick verification if time permits.
