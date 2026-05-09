# Emscripten XED Web Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a production Emscripten/XED web target for raw x86-64 byte analysis while preserving existing Rust/native and pure wasm configurations.

**Architecture:** Add a separate `uica-emscripten` Rust crate and separate `dist/emscripten/` artifacts. Build XED with an explicit Emscripten script, link it through `uica-xed-sys` when `target_os=emscripten`, and pass simple JSON/bytes across the JS/Rust ABI. Reuse existing manifest-selected `.uipack` data and browser caching; do not add JSON instruction fallbacks.

**Tech Stack:** Rust workspace, Intel XED C shim, Emscripten (`emcc`/`emar`), Cargo `wasm32-unknown-emscripten`, JavaScript ES modules, GitHub Actions Pages, existing UIPack data pipeline.

---

## Task checklist

- [ ] Task 1: Preserve pure wasm raw-hex regression boundary
- [ ] Task 2: Add failing CI/build contract tests
- [ ] Task 3: Add Emscripten XED build script skeleton
- [ ] Task 4: Teach XED build script primary mbuild path
- [ ] Task 5: Add documented XED fallback stop path
- [ ] Task 6: Split pure wasm vs Emscripten XED cfgs
- [ ] Task 7: Link script-built Emscripten XED in `uica-xed-sys`
- [ ] Task 8: Add `uica-emscripten` crate and ABI
- [ ] Task 9: Add `uica-emscripten` run contract tests
- [ ] Task 10: Add Emscripten Cargo build wrapper
- [ ] Task 11: Add Emscripten export smoke check
- [ ] Task 12: Factor shared manifest/UIPack cache helper
- [ ] Task 13: Replace main web UI with raw-hex Emscripten UI
- [ ] Task 14: Extend `scripts/build-web.sh`
- [ ] Task 15: Update Pages CI
- [ ] Task 16: Add/decide rust-parity PR gate
- [ ] Task 17: Update README docs
- [ ] Task 18: Run local non-Emscripten regression suite
- [ ] Task 19: Run local Emscripten validation
- [ ] Task 20: Run browser smoke
- [ ] Task 21: Monitor GitHub Actions and fix regressions

## Detailed plan

## Tasks

1. **Add contract tests for existing pure wasm boundary**
   - File: `rust/uica-wasm/tests/node_smoke.rs`
   - Changes: Keep existing `analyze_hex_reports_xed_required_after_hex_validation` and `analyze_hex_rejects_invalid_hex` unchanged; do not modify expected unsupported-XED string.
   - Acceptance:
     ```bash
     cargo test -p uica-wasm --test node_smoke
     ```
     Expected: pass before and after later tasks.
   - Executor boundary: regression guard only; small task, no build-system edits.

2. **Add CI/build contract tests before changing scripts**
   - File: `tests/verification/test_rust_ci_contract.py`
   - Changes: Extend test coverage to assert new web/CI guarantees. Add checks that:
     - `.github/workflows/pages.yml` contains `wasm32-unknown-emscripten`.
     - Pages workflow contains Emscripten setup marker, either `setup-emsdk` or raw `emsdk install`.
     - Pages workflow reinitializes `XED-to-XML` and `mbuild` after `setup.sh`.
     - `scripts/build-web.sh` references `scripts/build-xed-emscripten.sh`.
     - `scripts/build-web.sh` preserves `wasm-pack build "$ROOT_DIR/rust/uica-wasm"` and `dist/pkg`.
     - `scripts/build-web.sh` writes new artifacts under `dist/emscripten`.
   - Likely snippet:

     ```python
     def test_pages_workflow_builds_emscripten_xed_artifact(self):
         workflow = repo_root() / ".github" / "workflows" / "pages.yml"
         text = workflow.read_text()
         self.assertIn("wasm32-unknown-emscripten", text)
         self.assertTrue("setup-emsdk" in text or "emsdk install" in text)
         self.assertIn("git submodule update --init XED-to-XML mbuild", text)
         self.assertIn("dist/emscripten/uica_emscripten.js", text)
         self.assertIn("dist/emscripten/uica_emscripten.wasm", text)

     def test_web_build_script_keeps_pure_wasm_and_adds_emscripten(self):
         script = repo_root() / "scripts" / "build-web.sh"
         text = script.read_text()
         self.assertIn("wasm-pack build", text)
         self.assertIn("rust/uica-wasm", text)
         self.assertIn("dist/pkg", text)
         self.assertIn("build-xed-emscripten.sh", text)
         self.assertIn("dist/emscripten", text)
     ```

   - Acceptance:
     ```bash
     python3 -m unittest tests.verification.test_rust_ci_contract -v
     ```
     Expected now: fail on new Emscripten assertions.
   - Executor boundary: tests-only task.

3. **Add Emscripten XED build script skeleton with hard tool checks**
   - New File: `scripts/build-xed-emscripten.sh`
   - Changes: Create executable Bash script that:
     - sets `set -euo pipefail`, resolves `ROOT_DIR`, `OUT_DIR=${UICA_EMSCRIPTEN_XED_DIR:-$ROOT_DIR/target/xed-emscripten}`;
     - verifies `emcc`, `em++`, `emar`, `emranlib`, and `python3` are on `PATH`;
     - verifies `XED-to-XML/include/public/xed/xed-interface.h` and `mbuild/mbuild/env.py` exist;
     - removes/recreates staging dirs under `target/xed-emscripten/build`, `install`, and `toolwrap`;
     - writes wrapper symlinks in `toolwrap/` so mbuild `clang`/`clang++`/`ar`/`ranlib` resolve to Emscripten tools;
     - prints `UICA_EMSCRIPTEN_XED_DIR=<install-dir>` at end.
   - Likely snippet:

     ```bash
     #!/usr/bin/env bash
     set -euo pipefail

     ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
     OUT_DIR="${UICA_EMSCRIPTEN_XED_DIR:-$ROOT_DIR/target/xed-emscripten}"
     INSTALL_DIR="$OUT_DIR/install"
     BUILD_DIR="$OUT_DIR/build"
     TOOLWRAP_DIR="$OUT_DIR/toolwrap"

     for tool in emcc em++ emar emranlib python3; do
       if ! command -v "$tool" >/dev/null 2>&1; then
         echo "$tool not found; activate emsdk first" >&2
         exit 127
       fi
     done

     test -f "$ROOT_DIR/XED-to-XML/include/public/xed/xed-interface.h"
     test -f "$ROOT_DIR/mbuild/mbuild/env.py"

     rm -rf "$INSTALL_DIR" "$BUILD_DIR" "$TOOLWRAP_DIR"
     mkdir -p "$INSTALL_DIR" "$BUILD_DIR" "$TOOLWRAP_DIR"
     ln -sf "$(command -v emcc)" "$TOOLWRAP_DIR/clang"
     ln -sf "$(command -v em++)" "$TOOLWRAP_DIR/clang++"
     ln -sf "$(command -v emar)" "$TOOLWRAP_DIR/ar"
     ln -sf "$(command -v emranlib)" "$TOOLWRAP_DIR/ranlib"
     ```

   - Acceptance:
     ```bash
     bash -n scripts/build-xed-emscripten.sh
     chmod +x scripts/build-xed-emscripten.sh
     ```
   - Executor boundary: script/toolchain setup only; no Rust cfg changes yet.

4. **Teach XED build script primary mbuild path**
   - File: `scripts/build-xed-emscripten.sh`
   - Changes: Add primary attempt using XED `mfile.py` with Emscripten wrappers:
     - `PYTHONPATH="$ROOT_DIR/mbuild${PYTHONPATH:+:$PYTHONPATH}"`
     - `PATH="$TOOLWRAP_DIR:$PATH"`
     - run from `XED-to-XML`
     - use `--compiler=clang`, `--toolchain="$TOOLWRAP_DIR/"`, `--build-dir`, `--install-dir`, `--opt=2`, `--no-encoder`, `--no-werror`, and `--extra-flags` to quiet unsupported host flags if needed.
   - Likely snippet:

     ```bash
     export PYTHONPATH="$ROOT_DIR/mbuild${PYTHONPATH:+:$PYTHONPATH}"
     export PATH="$TOOLWRAP_DIR:$PATH"

     python3 "$ROOT_DIR/XED-to-XML/mfile.py" \
       --compiler=clang \
       --toolchain="$TOOLWRAP_DIR/" \
       --build-dir="$BUILD_DIR" \
       --install-dir="$INSTALL_DIR" \
       --opt=2 \
       --no-encoder \
       --no-werror \
       --extra-flags="-Wno-unused-command-line-argument" \
       install

     test -f "$INSTALL_DIR/include/xed/xed-interface.h"
     test -f "$INSTALL_DIR/lib/libxed.a"
     echo "$INSTALL_DIR" > "$OUT_DIR/xed-dir.txt"
     ```

   - Acceptance:
     ```bash
     source /path/to/emsdk/emsdk_env.sh
     scripts/build-xed-emscripten.sh
     test -f target/xed-emscripten/install/include/xed/xed-interface.h
     test -f target/xed-emscripten/install/lib/libxed.a
     ```
   - Executor boundary: XED mbuild port. Stop and report exact mbuild failure if no `libxed.a`.

5. **Add fallback path for XED if mbuild cannot cross-compile directly**
   - File: `scripts/build-xed-emscripten.sh`
   - Changes: Add opt-in fallback behind `UICA_XED_EMSCRIPTEN_FALLBACK=manual` only if Task 4 fails. Do not guess silently.
     - Run host XED generation/configuration into `$BUILD_DIR` without final wasm link if possible.
     - Collect generated C sources from build log or known XED kit dirs.
     - Compile each `.c` with `emcc -O2 -I... -c`.
     - Archive with `emar rcs "$INSTALL_DIR/lib/libxed.a" *.o` and `emranlib`.
   - Likely script shape:
     ```bash
     if [[ "${UICA_XED_EMSCRIPTEN_FALLBACK:-}" == "manual" ]]; then
       echo "manual fallback selected; compile generated XED C sources with emcc" >&2
       # Implement only after recording concrete source list from failed/direct mbuild run.
     else
       echo "XED mbuild did not produce Emscripten libxed.a" >&2
       echo "Re-run with UICA_XED_EMSCRIPTEN_FALLBACK=manual after documenting source list" >&2
       exit 2
     fi
     ```
   - Acceptance: same artifact tests as Task 4.
   - Executor boundary: high-risk XED fallback only. Requires evidence from Task 4 logs before implementation.

6. **Change XED Rust cfgs to distinguish pure wasm from Emscripten**
   - Files:
     - `rust/uica-xed/src/lib.rs`
     - `rust/uica-xed-sys/src/lib.rs`
   - Changes: Replace broad `target_arch = "wasm32"` stub cfgs with pure-wasm-only cfgs. Emscripten must use same externs and decode path as native.
   - Likely pattern:

     ```rust
     #[cfg(any(not(target_arch = "wasm32"), target_os = "emscripten"))]
     use std::os::raw::c_char;

     #[cfg(any(not(target_arch = "wasm32"), target_os = "emscripten"))]
     use anyhow::bail;

     #[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
     pub fn decode_raw(_bytes: &[u8]) -> Result<Vec<DecodedInstruction>> {
         anyhow::bail!("Intel XED decoder is not available for wasm32 target")
     }

     #[cfg(any(not(target_arch = "wasm32"), target_os = "emscripten"))]
     pub fn decode_raw(bytes: &[u8]) -> Result<Vec<DecodedInstruction>> { ... }
     ```

   - For `uica-xed-sys/src/lib.rs`:

     ```rust
     #[cfg(any(not(target_arch = "wasm32"), target_os = "emscripten"))]
     extern "C" { ... }

     #[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
     pub unsafe fn uica_xed_init() {}
     ```

   - Acceptance:
     ```bash
     cargo test -p uica-wasm --test node_smoke
     cargo test -p uica-xed -p uica-decoder
     ```
     Expected: pure wasm API unchanged; native XED tests pass.
   - Executor boundary: cfg-only Rust change, no build.rs branch yet.

7. **Teach `uica-xed-sys` build.rs to link script-built Emscripten XED**
   - File: `rust/uica-xed-sys/build.rs`
   - Changes:
     - Add `target_is_emscripten()` helper using `CARGO_CFG_TARGET_OS == "emscripten"`.
     - Change early return from `if target_is_wasm32() { return; }` to `if target_is_wasm32() && !target_is_emscripten() { return; }`.
     - Before native `prepare_xed_source`, add Emscripten branch that requires `UICA_EMSCRIPTEN_XED_DIR`.
     - Branch validates `<dir>/include/xed/xed-interface.h` and `<dir>/lib/libxed.a`.
     - Compile `src/uica_xed_shim.c` with `cc::Build` including `<dir>/include`.
     - Link `static=xed` from `<dir>/lib`.
   - Likely snippet:

     ```rust
     println!("cargo:rerun-if-env-changed=UICA_EMSCRIPTEN_XED_DIR");
     println!("cargo:rerun-if-env-changed=EMCC");

     if target_is_wasm32() && !target_is_emscripten() {
         return;
     }

     if target_is_emscripten() {
         link_emscripten_xed(&manifest_dir);
         return;
     }
     ```

     ```rust
     fn target_is_emscripten() -> bool {
         env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target_os| target_os == "emscripten")
     }

     fn link_emscripten_xed(manifest_dir: &Path) {
         let xed_dir = PathBuf::from(env::var("UICA_EMSCRIPTEN_XED_DIR").unwrap_or_else(|_| {
             panic!("UICA_EMSCRIPTEN_XED_DIR must point at scripts/build-xed-emscripten.sh install dir")
         }));
         let include_dir = xed_dir.join("include");
         let lib_dir = xed_dir.join("lib");
         let header = include_dir.join("xed/xed-interface.h");
         let lib = lib_dir.join("libxed.a");
         if !header.exists() || !lib.exists() {
             panic!("missing Emscripten XED artifacts under {}", xed_dir.display());
         }
         cc::Build::new()
             .std("c11")
             .file(manifest_dir.join("src/uica_xed_shim.c"))
             .include(&include_dir)
             .compile("uica_xed_shim");
         println!("cargo:rustc-link-search=native={}", lib_dir.display());
         println!("cargo:rustc-link-lib=static=xed");
     }
     ```

   - Acceptance:
     ```bash
     cargo test -p uica-xed-sys -p uica-xed
     # With emsdk + XED artifact:
     UICA_EMSCRIPTEN_XED_DIR="$PWD/target/xed-emscripten/install" \
     CC_wasm32_unknown_emscripten=emcc \
     CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER=emcc \
     cargo build -p uica-xed-sys --target wasm32-unknown-emscripten -vv
     ```
   - Executor boundary: build.rs/link integration only.

8. **Add new `uica-emscripten` crate to workspace**
   - Files:
     - `Cargo.toml`
     - New `rust/uica-emscripten/Cargo.toml`
     - New `rust/uica-emscripten/src/lib.rs`
     - New `rust/uica-emscripten/src/main.rs`
   - Changes:
     - Add `"rust/uica-emscripten"` to workspace members.
     - Crate depends on `uica-core` default features, `uica-data`, `uica-model`, `serde`, `serde_json`.
     - Provide testable safe function `run_request_json(request_json: &str, uipack_bytes: &[u8]) -> String`.
     - Provide Emscripten C ABI functions `uica_run(...) -> *mut c_char` and `uica_free_string(...)`.
     - `src/main.rs` contains empty `fn main() {}` to produce Emscripten JS glue for Cargo binary builds.
   - Likely `Cargo.toml`:

     ```toml
     [package]
     name = "uica-emscripten"
     version = "0.1.0"
     edition = "2021"

     [lib]
     name = "uica_emscripten"
     crate-type = ["rlib"]

     [[bin]]
     name = "uica_emscripten"
     path = "src/main.rs"

     [dependencies]
     serde = { version = "1", features = ["derive"] }
     serde_json = "1"
     uica-core = { path = "../uica-core" }
     uica-data = { path = "../uica-data" }
     uica-model = { path = "../uica-model" }
     ```

   - Likely request/ABI snippet:

     ```rust
     use std::ffi::{c_char, CStr, CString};
     use std::slice;

     use serde::{Deserialize, Serialize};
     use uica_data::MappedUiPackRuntime;
     use uica_model::Invocation;

     #[derive(Debug, Deserialize)]
     #[serde(default)]
     struct RunRequest {
         hex: String,
         arch: String,
         invocation: Invocation,
     }

     impl Default for RunRequest {
         fn default() -> Self {
             Self { hex: String::new(), arch: String::new(), invocation: Invocation::default() }
         }
     }

     #[derive(Serialize)]
     struct RunError<'a> {
         schema_version: &'a str,
         engine: &'a str,
         error: String,
     }

     pub fn run_request_json(request_json: &str, uipack_bytes: &[u8]) -> String {
         match run_request_json_inner(request_json, uipack_bytes) {
             Ok(json) => json,
             Err(error) => serde_json::to_string(&RunError {
                 schema_version: "uica-error-v1",
                 engine: "rust-emscripten-xed",
                 error,
             }).unwrap_or_else(|_| "{\"schema_version\":\"uica-error-v1\",\"error\":\"serialization failed\"}".to_string()),
         }
     }

     fn run_request_json_inner(request_json: &str, uipack_bytes: &[u8]) -> Result<String, String> {
         let request: RunRequest = serde_json::from_str(request_json).map_err(|err| err.to_string())?;
         let code = decode_hex(&request.hex)?;
         let arch = if request.arch.trim().is_empty() { request.invocation.arch.clone() } else { request.arch.clone() };
         let mut invocation = request.invocation;
         invocation.arch = arch.trim().to_ascii_uppercase();

         let runtime = MappedUiPackRuntime::from_bytes_verified(uipack_bytes.to_vec()).map_err(|err| err.to_string())?;
         let pack_arch = runtime.view().map_err(|err| err.to_string())?.arch().to_string();
         if !pack_arch.eq_ignore_ascii_case(&invocation.arch) {
             return Err(format!(
                 "UIPack architecture {pack_arch} does not match requested architecture {}",
                 invocation.arch
             ));
         }
         let output = uica_core::engine::engine_output_with_uipack_runtime(&code, &invocation, &runtime, false)?;
         serde_json::to_string(&output.result).map_err(|err| err.to_string())
     }
     ```

     ```rust
     #[no_mangle]
     pub unsafe extern "C" fn uica_run(
         request_ptr: *const c_char,
         uipack_ptr: *const u8,
         uipack_len: usize,
     ) -> *mut c_char {
         let request = CStr::from_ptr(request_ptr).to_string_lossy();
         let uipack = slice::from_raw_parts(uipack_ptr, uipack_len);
         CString::new(run_request_json(&request, uipack))
             .unwrap_or_else(|_| CString::new("{\"schema_version\":\"uica-error-v1\",\"error\":\"nul byte in response\"}").unwrap())
             .into_raw()
     }

     #[no_mangle]
     pub unsafe extern "C" fn uica_free_string(ptr: *mut c_char) {
         if !ptr.is_null() {
             drop(CString::from_raw(ptr));
         }
     }
     ```

   - Acceptance:
     ```bash
     cargo test -p uica-emscripten
     cargo test -p uica-wasm --test node_smoke
     ```
   - Executor boundary: crate/API only. Do not touch web or CI in same task.

9. **Add unit tests for new crate request parsing and errors**
   - New File: `rust/uica-emscripten/tests/run_contract.rs`
   - Changes: Add tests for invalid hex and arch mismatch. Use `include_bytes!("../../uica-data/generated/arch/SKL.uipack")` like `uica-wasm` tests.
   - Likely snippets:

     ```rust
     use serde_json::Value;

     #[test]
     fn run_reports_invalid_hex_as_json_error() {
         let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
         let output = uica_emscripten::run_request_json(
             r#"{"hex":"9z","arch":"SKL"}"#,
             pack,
         );
         let value: Value = serde_json::from_str(&output).unwrap();
         assert_eq!(value["schema_version"], "uica-error-v1");
         assert!(value["error"].as_str().unwrap().contains("invalid hex digit 'z'"));
     }

     #[test]
     fn run_rejects_uipack_arch_mismatch() {
         let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
         let output = uica_emscripten::run_request_json(
             r#"{"hex":"48 01 d8","arch":"HSW"}"#,
             pack,
         );
         let value: Value = serde_json::from_str(&output).unwrap();
         assert_eq!(value["schema_version"], "uica-error-v1");
         assert!(value["error"].as_str().unwrap().contains("UIPack architecture SKL does not match requested architecture HSW"));
     }
     ```

   - If native XED available, add positive test:
     ```rust
     #[test]
     fn run_decodes_hex_and_returns_uica_result() {
         let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
         let output = uica_emscripten::run_request_json(
             r#"{"hex":"48 01 d8","arch":"SKL"}"#,
             pack,
         );
         let value: Value = serde_json::from_str(&output).unwrap();
         assert_eq!(value["schema_version"], "uica-result-v1");
         assert_eq!(value["engine"], "rust");
         assert_eq!(value["invocation"]["arch"], "SKL");
         assert!(value["summary"]["throughput_cycles_per_iteration"].is_number());
     }
     ```
   - Acceptance:
     ```bash
     cargo test -p uica-emscripten
     ```
   - Executor boundary: crate tests only.

10. **Add Emscripten Cargo build wrapper script**
    - New File: `scripts/build-uica-emscripten.sh`
    - Changes: Create script that:
      - verifies `emcc` and Rust target are available;
      - runs `scripts/build-xed-emscripten.sh` unless `UICA_EMSCRIPTEN_XED_DIR` already points at valid artifacts;
      - exports `UICA_EMSCRIPTEN_XED_DIR`, `CC_wasm32_unknown_emscripten=emcc`, `CXX_wasm32_unknown_emscripten=em++`, `AR_wasm32_unknown_emscripten=emar`, `CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER=emcc`;
      - sets `RUSTFLAGS` export list for Emscripten;
      - runs `cargo build -p uica-emscripten --bin uica_emscripten --target wasm32-unknown-emscripten --release`;
      - copies/normalizes artifacts to output dir passed as first arg or `dist/emscripten`.
    - Likely export flags:
      ```bash
      export RUSTFLAGS="${RUSTFLAGS:-} \
        -C link-arg=-sMODULARIZE=1 \
        -C link-arg=-sEXPORT_ES6=1 \
        -C link-arg=-sENVIRONMENT=web \
        -C link-arg=-sALLOW_MEMORY_GROWTH=1 \
        -C link-arg=-sEXPORTED_FUNCTIONS=['_uica_run','_uica_free_string','_malloc','_free'] \
        -C link-arg=-sEXPORTED_RUNTIME_METHODS=['UTF8ToString','stringToUTF8','lengthBytesUTF8']"
      ```
    - Artifact copy logic should check both likely names and fail with listing:
      ```bash
      TARGET_DIR="$ROOT_DIR/target/wasm32-unknown-emscripten/release"
      mkdir -p "$OUT_DIR"
      cp "$TARGET_DIR/uica_emscripten.js" "$OUT_DIR/uica_emscripten.js"
      cp "$TARGET_DIR/uica_emscripten.wasm" "$OUT_DIR/uica_emscripten.wasm"
      ```
    - Acceptance:
      ```bash
      bash -n scripts/build-uica-emscripten.sh
      source /path/to/emsdk/emsdk_env.sh
      scripts/build-uica-emscripten.sh dist/emscripten
      test -f dist/emscripten/uica_emscripten.js
      test -f dist/emscripten/uica_emscripten.wasm
      ```
    - Executor boundary: Emscripten Rust build wrapper only.

11. **Add export smoke check for Emscripten artifact**
    - New File: `scripts/smoke-emscripten-exports.sh`
    - Changes: Add script to inspect `dist/emscripten/uica_emscripten.js` and `.wasm` for required exports. Prefer `wasm-objdump` if available; otherwise grep JS glue.
    - Likely checks:
      ```bash
      test -f dist/emscripten/uica_emscripten.js
      test -f dist/emscripten/uica_emscripten.wasm
      grep -q "uica_run" dist/emscripten/uica_emscripten.js
      grep -q "uica_free_string" dist/emscripten/uica_emscripten.js
      ```
    - Acceptance:
      ```bash
      scripts/smoke-emscripten-exports.sh
      ```
    - Executor boundary: artifact validation only.

12. **Refactor shared manifest/UIPack browser cache helper**
    - New File: `web/uipack-cache.js`
    - File: `web/pure-wasm.js`
    - Changes:
      - Move constants and functions from `web/pure-wasm.js` into `web/uipack-cache.js`: `MANIFEST_URL`, cache name, FNV constants, `loadManifest`, `fetchCachedUipack`, `validateUipackBytes`.
      - Export functions that accept DOM callback hooks instead of directly editing `cacheStatus`.
      - Update `web/pure-wasm.js` to import helper and keep existing decoded-only demo behavior.
    - Likely helper API:
      ```js
      export async function loadManifest() { ... return manifest; }
      export async function populateArchSelect(select, manifest, preferred = "SKL") { ... }
      export async function fetchCachedUipack(manifest, arch, { setCacheStatus = () => {} } = {}) { ... }
      ```
    - Acceptance:
      ```bash
      ./scripts/build-web.sh
      python3 -m http.server -d dist 8000
      # Manual: open /test-pure-wasm.html, click analyze, confirm decoded demo still returns result.
      cargo test -p uica-wasm --test node_smoke
      ```
    - Executor boundary: browser data helper and pure page compatibility. No Emscripten module code.

13. **Replace main web UI with Emscripten raw-hex interface**
    - Files:
      - `web/index.html`
      - `web/main.js`
      - `web/style.css`
    - Changes:
      - `web/index.html` becomes production UI for raw hex analysis.
      - Add DOM ids used by controller: `arch-select`, `hex-input`, `analyze-button`, `status`, `cache-status`, `output`.
      - Keep link to `test-pure-wasm.html` for pure wasm smoke.
      - `web/main.js` imports `createUica` from `./emscripten/uica_emscripten.js` and shared helper from `./uipack-cache.js`.
      - Implement Emscripten memory bridge with `_malloc`, `_free`, `_uica_run`, `_uica_free_string`, `UTF8ToString`, `stringToUTF8`, `lengthBytesUTF8`.
    - Likely `main.js` bridge:

      ```js
      import createUica from "./emscripten/uica_emscripten.js";
      import {
        fetchCachedUipack,
        loadManifest,
        populateArchSelect,
      } from "./uipack-cache.js";

      let Module;
      let manifest;

      async function boot() {
        Module = await createUica({
          locateFile: (path) => `./emscripten/${path}`,
        });
        manifest = await loadManifest();
        populateArchSelect(archSelect, manifest, "SKL");
        button.disabled = false;
        status.textContent = "Wasm ready";
      }

      function callRun(request, uipackBytes) {
        const requestJson = JSON.stringify(request);
        const requestLen = Module.lengthBytesUTF8(requestJson) + 1;
        const requestPtr = Module._malloc(requestLen);
        const uipackPtr = Module._malloc(uipackBytes.byteLength);
        try {
          Module.stringToUTF8(requestJson, requestPtr, requestLen);
          Module.HEAPU8.set(uipackBytes, uipackPtr);
          const resultPtr = Module._uica_run(
            requestPtr,
            uipackPtr,
            uipackBytes.byteLength,
          );
          try {
            return Module.UTF8ToString(resultPtr);
          } finally {
            Module._uica_free_string(resultPtr);
          }
        } finally {
          Module._free(requestPtr);
          Module._free(uipackPtr);
        }
      }
      ```

    - Acceptance:
      ```bash
      ./scripts/build-web.sh
      test -f dist/index.html
      test -f dist/main.js
      test -f dist/uipack-cache.js
      test -f dist/emscripten/uica_emscripten.js
      ```
      Manual browser smoke with `48 01 d8`, `SKL`.
    - Executor boundary: production UI only.

14. **Extend `scripts/build-web.sh` for new artifacts without breaking pure wasm**
    - File: `scripts/build-web.sh`
    - Changes:
      - Keep existing `wasm-pack build "$ROOT_DIR/rust/uica-wasm" --target web --out-dir ../../dist/pkg` unchanged.
      - Create `DIST_DIR/emscripten`.
      - Call `scripts/build-uica-emscripten.sh "$DIST_DIR/emscripten"` after pure wasm build or before static copy.
      - Copy `web/uipack-cache.js` to `dist/uipack-cache.js`.
      - Add file assertions before success message.
    - Likely diff shape:
      ```bash
      mkdir -p "$DIST_DIR" "$DIST_DIR/emscripten"
      wasm-pack build "$ROOT_DIR/rust/uica-wasm" --target web --out-dir ../../dist/pkg
      "$ROOT_DIR/scripts/build-uica-emscripten.sh" "$DIST_DIR/emscripten"
      cp "$ROOT_DIR/web/uipack-cache.js" "$DIST_DIR/uipack-cache.js"
      test -f "$DIST_DIR/pkg/uica_wasm.js"
      test -f "$DIST_DIR/pkg/uica_wasm_bg.wasm"
      test -f "$DIST_DIR/emscripten/uica_emscripten.js"
      test -f "$DIST_DIR/emscripten/uica_emscripten.wasm"
      ```

- Acceptance:
  ```bash
  ./scripts/build-web.sh
  test -f dist/pkg/uica_wasm.js
  test -f dist/pkg/uica_wasm_bg.wasm
  test -f dist/emscripten/uica_emscripten.js
  test -f dist/emscripten/uica_emscripten.wasm
  test -f dist/data/manifest.json
  test -f dist/data/arch/SKL.uipack
  ```
- Executor boundary: top-level web build integration only.

15. **Update Pages CI to install Emscripten and validate new artifact**

- File: `.github/workflows/pages.yml`
- Changes:
  - Rust toolchain target list includes both `wasm32-unknown-unknown` and `wasm32-unknown-emscripten`.
  - Add pinned Emscripten setup before `Build Pages artifact`. Prefer pinned action for speed:
    ```yaml
    - uses: mymindstorm/setup-emsdk@v14
      with:
        version: "3.1.74"
        actions-cache-folder: emsdk-cache
    ```
  - After `bash ./setup.sh`, reinitialize submodules:
    ```bash
    git submodule update --init XED-to-XML mbuild
    ```
  - Extend artifact checks:
    ```bash
    test -f dist/emscripten/uica_emscripten.js
    test -f dist/emscripten/uica_emscripten.wasm
    test -f dist/uipack-cache.js
    ```
- Acceptance:
  ```bash
  python3 -m unittest tests.verification.test_rust_ci_contract -v
  ```
  On GitHub: Pages build job passes and deploys `dist/`.
- Executor boundary: Pages workflow only.

16. **Decide and add rust-parity PR gate for Emscripten**

- File: `.github/workflows/rust-parity.yml`
- Changes: Recommended: add same Emscripten target/setup to PR workflow but keep timeout under observation.
  - Add `wasm32-unknown-emscripten` target.
  - Add pinned `setup-emsdk` step before web build.
  - Ensure `git submodule update --init` already exists after `setup.sh`; change to explicit `git submodule update --init XED-to-XML mbuild` if preferred.
  - Existing `Build web bundle` step will run new Emscripten build through `scripts/build-web.sh`; extend checks for `dist/emscripten/*`.
- Alternative if CI time too high: add manual-only workflow later; but Pages still validates deploy. This needs explicit owner decision if runtime exceeds 90 min.
- Acceptance:
  ```bash
  python3 -m unittest tests.verification.test_rust_ci_contract -v
  ```
  On GitHub PR: `rust-parity` passes or timeout data captured.
- Executor boundary: PR workflow only.

17. **Update README docs for local Emscripten build**

- File: `README.rust.md`
- Changes:
  - Replace “future Emscripten/XED target” limitation with local commands.
  - Document emsdk activation, Rust target, `setup.sh`, submodule reinit, `scripts/build-web.sh`.
  - Document output layout:
    - `dist/index.html` main raw hex UI,
    - `dist/emscripten/uica_emscripten.js/.wasm`,
    - `dist/pkg/*` pure wasm decoded-only smoke,
    - `dist/data/manifest.json`, `dist/data/arch/*.uipack` shared data.
  - Re-state no JSON runtime fallback.
- Acceptance:
  ```bash
  grep -n "uica_emscripten" README.rust.md
  grep -n "manifest-selected" README.rust.md
  ```
- Executor boundary: docs only.

18. **Run local non-Emscripten regression suite**

- Files: no edits.
- Commands:
  ```bash
  cargo test -p uica-wasm --test node_smoke
  cargo test -p uica-xed-sys -p uica-xed -p uica-decoder
  cargo test -p uica-core --features xed-decoder
  python3 -m unittest tests.verification.test_rust_ci_contract -v
  ```
- Acceptance: all commands exit 0.
- Executor boundary: validation only.

19. **Run local Emscripten build validation**

- Files: no edits.
- Commands:
  ```bash
  rustup target add wasm32-unknown-emscripten
  source /path/to/emsdk/emsdk_env.sh
  emcc -v
  bash ./setup.sh
  git submodule update --init XED-to-XML mbuild
  scripts/build-xed-emscripten.sh
  UICA_EMSCRIPTEN_XED_DIR="$PWD/target/xed-emscripten/install" \
    CC_wasm32_unknown_emscripten=emcc \
    CXX_wasm32_unknown_emscripten=em++ \
    AR_wasm32_unknown_emscripten=emar \
    CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER=emcc \
    cargo build -p uica-emscripten --bin uica_emscripten --target wasm32-unknown-emscripten --release
  ./scripts/build-web.sh
  scripts/smoke-emscripten-exports.sh
  ```
- Acceptance:
  ```bash
  test -f dist/emscripten/uica_emscripten.js
  test -f dist/emscripten/uica_emscripten.wasm
  test -f dist/data/manifest.json
  test -f dist/data/arch/SKL.uipack
  ```
- Executor boundary: validation only. If XED Emscripten build fails, return exact command/log excerpt and stop.

20. **Run browser smoke**

- Files: no edits.
- Commands:
  ```bash
  python3 -m http.server -d dist 8000
  ```
- Manual checks:
  - Open `http://127.0.0.1:8000/`.
  - Select `SKL`.
  - Enter `48 01 d8`.
  - Click Analyze.
  - Confirm output JSON has:
    - `schema_version: "uica-result-v1"`
    - `engine: "rust"`
    - `invocation.arch: "SKL"`
    - numeric `summary.throughput_cycles_per_iteration`
  - Reload page, run again, confirm cache status says UIPack loaded from browser cache.
  - Open `/test-pure-wasm.html`, click Analyze, confirm decoded-only pure wasm smoke still works.
- Acceptance: manual browser smoke passes.
- Executor boundary: browser/manual validation only.

21. **Monitor GitHub Actions and fix regressions**

- Files: only if CI fails.
- Steps:
  - Push branch / open PR.
  - Watch `rust-parity` and Pages build.
  - If pure wasm fails, inspect changes to `rust/uica-wasm`, `dist/pkg`, or cfg predicates first.
  - If Emscripten fails in XED script, capture failed command, mbuild log, and tool versions; decide whether Task 5 fallback is needed.
  - If Pages fails after setup, confirm `git submodule update --init XED-to-XML mbuild` ran after `setup.sh`.
- Acceptance: existing Rust/pure-wasm CI remains green; new Emscripten artifact checks pass.
- Executor boundary: CI monitoring and targeted fixes.

## Files to Modify

- `Cargo.toml` - add `rust/uica-emscripten` workspace member.
- `rust/uica-xed/src/lib.rs` - change cfgs so pure wasm stubs remain but Emscripten uses real XED path.
- `rust/uica-xed-sys/src/lib.rs` - change extern/stub cfgs for Emscripten.
- `rust/uica-xed-sys/build.rs` - add `target_os=emscripten` branch linking `UICA_EMSCRIPTEN_XED_DIR` artifacts.
- `web/index.html` - production raw-hex UI with arch select and status/output areas.
- `web/main.js` - Emscripten module loader and JS/wasm ABI bridge.
- `web/pure-wasm.js` - import shared UIPack cache helper; keep decoded-only behavior.
- `web/style.css` - style raw-hex UI and cache status.
- `scripts/build-web.sh` - invoke Emscripten build, copy shared helper, assert new artifacts while preserving pure wasm output.
- `.github/workflows/pages.yml` - install Emscripten target/sdk, reinit submodules after setup, validate new artifacts.
- `.github/workflows/rust-parity.yml` - recommended PR gate for Emscripten build/artifacts.
- `tests/verification/test_rust_ci_contract.py` - add workflow/build-script contract assertions.
- `README.rust.md` - document Emscripten/XED build and dist layout.

## New Files

- `scripts/build-xed-emscripten.sh` - script-first Emscripten XED build producing `include/` + `lib/libxed.a`.
- `scripts/build-uica-emscripten.sh` - Cargo/Emscripten wrapper producing `uica_emscripten.js/.wasm`.
- `scripts/smoke-emscripten-exports.sh` - validates required Emscripten artifacts/exports.
- `rust/uica-emscripten/Cargo.toml` - new crate manifest.
- `rust/uica-emscripten/src/lib.rs` - safe run function and C ABI exports.
- `rust/uica-emscripten/src/main.rs` - empty Emscripten binary entry for JS glue generation.
- `rust/uica-emscripten/tests/run_contract.rs` - run ABI/request contract tests.
- `web/uipack-cache.js` - shared manifest/UIPack fetch, checksum validation, browser cache helper.

## Dependencies

- Task 2 should happen before CI/script edits so contract tests fail first.
- Tasks 3-5 block all Emscripten linking work; no usable wasm artifact without `libxed.a`.
- Task 6 must happen before Task 7/8 Emscripten Rust build; otherwise XED remains stubbed on `wasm32`.
- Task 7 depends on Task 3/4 artifact layout.
- Tasks 8-9 depend on Task 6 and use existing `engine_output_with_uipack_runtime` from `uica-core`.
- Task 10 depends on Tasks 3, 7, and 8.
- Task 11 depends on Task 10 artifact names.
- Task 12 should happen before Task 13 to avoid duplicate cache logic.
- Task 13 depends on Task 10 JS artifact contract and Task 12 helper API.
- Task 14 depends on Tasks 10, 12, and 13.
- Task 15 depends on Task 14 build-web contract.
- Task 16 depends on Task 14 and may be deferred if CI runtime is too high, but Pages must still include Task 15.
- Tasks 18-20 depend on implementation tasks.
- Task 21 depends on pushed branch/PR.

## Risks

- Intel XED may not build under Emscripten through mbuild without patches. Stop on first concrete failure and decide whether manual fallback is worth implementing.
- `setup.sh` deinitializes submodules. Any workflow that runs `build-web.sh` after `setup.sh` must re-run `git submodule update --init XED-to-XML mbuild`.
- Emscripten export flags are brittle. Missing `_uica_run`, `_uica_free_string`, `_malloc`, or `_free` breaks browser bridge.
- Cargo may emit different Emscripten artifact names depending on crate/bin shape. Build wrapper must list target dir on failure and copy only confirmed names.
- `cc` crate may not pick `emcc` unless target env vars are set. Build wrapper and CI must set `CC_wasm32_unknown_emscripten=emcc`.
- Browser helper refactor can regress pure wasm demo. Keep `rust/uica-wasm` and `test-pure-wasm.html` smoke intact.
- Cache API stores large UIPacks. Validate size/checksum before every use; delete invalid cache entries.
- CI time may exceed current 90-minute rust-parity timeout once emsdk + XED build are added. If timeout happens, keep Pages build required and split PR Emscripten gate after owner decision.
- No pthread build in first phase. Do not add `-sUSE_PTHREADS=1`; GitHub Pages lacks COOP/COEP headers needed for browser pthreads.
- Data invariant: do not add `instructions.json` or `instructions_full.json` runtime fallbacks. All runtime instruction data must come from manifest-selected `.uipack` bytes.

## Explicit Validation Checklist

- Pure wasm unchanged:
  ```bash
  cargo test -p uica-wasm --test node_smoke
  ```
- Native XED unchanged:
  ```bash
  cargo test -p uica-xed-sys -p uica-xed -p uica-decoder
  ```
- Core raw runtime path intact:
  ```bash
  cargo test -p uica-core --features xed-decoder
  ```
- New crate native tests:
  ```bash
  cargo test -p uica-emscripten
  ```
- Web build artifact:
  ```bash
  ./scripts/build-web.sh
  test -f dist/pkg/uica_wasm.js
  test -f dist/pkg/uica_wasm_bg.wasm
  test -f dist/emscripten/uica_emscripten.js
  test -f dist/emscripten/uica_emscripten.wasm
  test -f dist/data/manifest.json
  test -f dist/data/arch/SKL.uipack
  ```
- Manifest walk:
  ```bash
  python3 - <<'PY'
  import json
  from pathlib import Path
  manifest = json.loads(Path('dist/data/manifest.json').read_text())
  missing = []
  for arch, entry in manifest['architectures'].items():
      path = Path('dist/data') / entry['path']
      if not path.exists():
          missing.append((arch, str(path)))
  if missing:
      raise SystemExit(f'missing UIPack files: {missing}')
  PY
  ```
- CI contract:
  ```bash
  python3 -m unittest tests.verification.test_rust_ci_contract -v
  ```
- Browser smoke: `SKL` + `48 01 d8` returns `uica-result-v1` JSON with numeric throughput.
