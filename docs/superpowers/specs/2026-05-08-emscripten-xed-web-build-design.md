# Emscripten XED Web Build Design

## Goal

Add a production-facing Emscripten web build for `uica.houmus.org` that compiles Rust uiCA and Intel XED into one Emscripten-linked wasm artifact. Browser users enter raw x86-64 bytes as hex, select a microarchitecture, and get the standard Rust uiCA analysis result. Existing pure `wasm32-unknown-unknown` wasm build remains unchanged.

## Approved Decisions

- XED build strategy: script-first. A dedicated script builds Emscripten-compatible XED artifacts with `emcc`/`emar`; Rust build scripts link env-provided artifacts.
- Pure wasm isolation: keep `rust/uica-wasm` behavior unchanged, including raw-hex unsupported error.
- Data path: use only manifest-selected `.uipack` files shared with existing web build. Do not add JSON instruction-data runtime fallback.
- Entry shape: expose one JS-facing `run` entrypoint for raw hex/bytes + arch/options + UIPack bytes.
- Deployment: new Emscripten artifacts live in a separate `dist/emscripten/` path; existing `dist/pkg/` pure wasm artifacts stay intact.

## Architecture

### Rust crates

Add a new Emscripten-focused crate, tentatively `rust/uica-emscripten`, instead of changing `rust/uica-wasm`. It depends on `uica-core` with the XED decoder enabled, `uica-data`, `uica-model`, and serde/JSON support.

The crate exports a single stable C ABI function for Emscripten JS glue. The function accepts simple pointers/lengths or a JSON request plus byte buffers. It returns a JSON string containing either a serialized `UicaResult` or a structured error. Complex Rust types do not cross the JS/wasm boundary.

### XED linking

Current XED cfgs treat all `wasm32` as no-XED. Change those cfgs so only pure `wasm32-unknown-unknown` uses stubs, while `wasm32-unknown-emscripten` uses real extern C declarations and the existing C shim ABI.

Add `scripts/build-xed-emscripten.sh` to build XED for Emscripten before Cargo builds the new crate. `rust/uica-xed-sys/build.rs` then detects Emscripten target and links artifacts from an explicit environment variable such as `UICA_EMSCRIPTEN_XED_DIR`. This keeps Cargo deterministic and makes CI failures easier to debug.

### Data flow

1. Browser loads `data/manifest.json` with cache revalidation.
2. Browser selects matching `data/arch/<ARCH>.uipack`, validates size/checksum, and caches it via Cache API.
3. Browser sends normalized hex bytes, selected arch, invocation options, and UIPack bytes to Emscripten module.
4. Rust verifies the UIPack with `MappedUiPackRuntime::from_bytes_verified` and checks arch match.
5. Rust decodes raw bytes through XED into `uica-decode-ir` instructions.
6. Rust runs the existing decoded/UIPack uiCA pipeline.
7. Rust returns `UicaResult` JSON to JS for display.

### Web UI

Replace or update the main served `web/index.html` to target the Emscripten build. The page provides:

- microarchitecture dropdown populated from manifest architectures,
- hex input textarea for x86-64 bytes,
- run button and status/error area,
- JSON/result display,
- cache status for downloaded UIPacks.

Keep `web/test-pure-wasm.html` and pure wasm controller for no-XED smoke testing.

### Build layout

`scripts/build-web.sh` remains the top-level web bundler. It still builds pure wasm via `wasm-pack` into `dist/pkg/`, copies static web assets, and copies shared `.uipack` data into `dist/data/`.

New Emscripten steps:

- verify `emcc`, `emar`, and Rust target `wasm32-unknown-emscripten`,
- run `scripts/build-xed-emscripten.sh`,
- run Cargo build for the new crate with `emcc` linker and export flags,
- copy Emscripten `.js`/`.wasm` artifacts to `dist/emscripten/`,
- assert both pure wasm and Emscripten outputs exist.

### CI and deployment

GitHub Pages workflow installs and activates a pinned Emscripten SDK, adds Rust `wasm32-unknown-emscripten`, runs `setup.sh`, reinitializes XED/mbuild submodules after setup deinitializes them, then runs `scripts/build-web.sh`.

Rust parity workflow keeps existing native and pure wasm checks. If CI runtime is acceptable, add Emscripten build checks there too so pull requests catch regressions before Pages deploy.

### Validation

Baseline checks must continue passing:

```bash
cargo test --workspace
cargo test -p uica-wasm --test node_smoke
./scripts/build-web.sh
```

New checks:

```bash
rustup target add wasm32-unknown-emscripten
emcc -v
scripts/build-xed-emscripten.sh
cargo build -p uica-emscripten --target wasm32-unknown-emscripten --release
./scripts/build-web.sh
test -f dist/emscripten/uica_emscripten.js
test -f dist/emscripten/uica_emscripten.wasm
```

Browser smoke:

- serve `dist/`,
- open main page,
- select `SKL`,
- enter `48 01 d8`,
- run analysis,
- verify JSON result has `schema_version = "uica-result-v1"`, `engine = "rust"`, `invocation.arch = "SKL"`, and numeric throughput.

## Error Handling

- Invalid hex reports a user-facing parse error before calling Rust when possible; Rust validates again.
- Missing or checksum-mismatched UIPack reports cache/download error and retries network fetch.
- UIPack arch mismatch reports explicit requested/actual arch mismatch.
- XED decode failure reports byte offset and XED status from existing decoder path.
- Emscripten module load failure reports missing artifact/toolchain hints during local development.

## Non-goals

- No object-file upload in first phase.
- No pthread build in first phase, avoiding GitHub Pages COOP/COEP header requirements.
- No JSON instruction-data fallback.
- No change to Python analyzer behavior.
- No change to pure wasm decoded-only APIs.

## Risks

- Intel XED may need Emscripten-specific build patches because public build docs do not list Emscripten support.
- `setup.sh` deinitializes submodules; CI must reinitialize before Emscripten XED build.
- Cargo/Emscripten linker flags may strip exported functions unless export list is exact.
- Browser Cache API logic can drift if duplicated; factor shared manifest/UIPack helper if practical.
- CI time may increase materially due emsdk and XED builds.

## Open Implementation Notes

Use smallest viable script-first prototype. If XED mbuild cannot cross-build directly, generate/configure XED on host and compile the produced C source set with `emcc`/`emar`. Stop and document exact blockers before patching large upstream code paths.
