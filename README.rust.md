# uiCA Rust Port README

Status: **in progress**. Rust workspace, tooling, verification hooks, CLI/wasm shells are in place. Full Python parity is **not** complete yet.

## 1) What exists today

Rust workspace (`Cargo.toml`):

- `rust/uica-model`
- `rust/uica-data`
- `rust/uica-data-gen`
- `rust/uica-xed-sys`
- `rust/uica-xed`
- `rust/uica-decode-ir`
- `rust/uica-decoder`
- `rust/uica-core`
- `rust/uica-cli`
- `rust/uica-wasm`
- `rust/uica-emscripten`

High-level flow today:

```text
instructions.xml
  -> uica-data-gen (Rust) -> manifest + per-arch UIPack files

input obj/raw
  -> uica-cli (Rust)
      -> uica-decoder + XED (decode bytes into uica-decode-ir)
      -> uica-core::engine / engine_with_decoded (partial analysis)
          -> uica-data (load UIPack data)
          -> matcher + analytical/simulation summary paths
      -> uica-model::UicaResult (JSON)

wasm decoded-IR input
  -> test-pure-wasm.html fetches shared data/manifest.json + data/arch/*.uipack
      -> browser Cache API stores selected UIPack
      -> uica-wasm::analyze_decoded_json_with_uipack
      -> uica-core::engine_with_decoded_uipack_runtime
      -> JSON string

Emscripten raw-byte/assembly input
  -> index.html fetches shared data/manifest.json + data/arch/*.uipack
      -> browser Cache API stores selected UIPack
      -> index.html optionally assembles NASM syntax in a browser Worker
      -> NASM wasm emits raw x86-64 bytes for flat binary input
      -> uica_emscripten.js calls uica_run JSON/bytes ABI
      -> XED decodes raw x86-64 bytes
      -> uica-core::engine_output_with_uipack_runtime(include_reports=true)
      -> trace HTML + UicaResult JSON tabs
```

## 2) Build and run

### Prereqs

- Rust toolchain (cargo)
- For pure wasm build: `wasm-pack` + `wasm32-unknown-unknown` target
- For Emscripten/XED web build: active emsdk (`emcc`, `em++`, `emar`, `emranlib`) + `wasm32-unknown-emscripten` target
- Node.js for web smoke scripts
- Python env for verification harness
- Intel XED submodule initialized (`git submodule update --init`). Native Rust builds compile/link the repo-local XED library automatically through `uica-xed-sys` when needed.

Install wasm target:

```bash
rustup target add wasm32-unknown-unknown
rustup target add wasm32-unknown-emscripten
```

### Build Rust CLI

```bash
cargo build -p uica-cli
```

### Run Rust CLI

Object input:

```bash
target/debug/uica-cli test.o --arch SKL --tp-only
```

Raw input:

```bash
target/debug/uica-cli test.bin --raw --arch SKL --json out.json --tp-only
```

### Run Rust tests

```bash
cargo test --workspace
```

### Deployment/build options

There are three supported Rust-facing deployment modes. They share the same
manifest-selected `.uipack` data path when instruction data is needed. Do not
add runtime fallbacks to `instructions.json` or `instructions_full.json`.

#### Option 1: Native Rust CLI

Use this for local command-line analysis, parity debugging, and trace/graph
file generation. Native builds compile/link repo-local XED through
`uica-xed-sys`.

Build:

```bash
cargo build -p uica-cli
```

Analyze raw x86 bytes:

```bash
printf '\x48\x01\xd8' > /tmp/add.bin
target/debug/uica-cli /tmp/add.bin --raw --arch SKL --tp-only
```

Write JSON plus HTML trace/graph:

```bash
target/debug/uica-cli /tmp/add.bin --raw --arch SKL \
  --json /tmp/add.json \
  --trace /tmp/trace.html \
  --graph /tmp/graph.html
```

Object-file input still works without `--raw`:

```bash
target/debug/uica-cli test.o --arch SKL --tp-only
```

#### Option 2: Pure wasm decoded-IR web build

Use this for browser analysis when decoded instruction IR is supplied by the
caller. This target is `wasm32-unknown-unknown`, intentionally excludes XED,
and is served by `test-pure-wasm.html`.

Install target/tool:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
```

Generate data and build the pure wasm artifact:

```bash
./setup.sh
wasm-pack build rust/uica-wasm --target web --out-dir ../../dist/pkg
mkdir -p dist/data/arch
cp web/test-pure-wasm.html web/pure-wasm.js web/uipack-cache.js web/style.css dist/
cp rust/uica-data/generated/manifest.json dist/data/manifest.json
cp rust/uica-data/generated/arch/*.uipack dist/data/arch/
```

Serve the pure wasm smoke page:

```bash
python3 -m http.server -d dist 8000
```

Open:

```text
http://127.0.0.1:8000/test-pure-wasm.html
```

`./scripts/build-web.sh` also builds this pure wasm artifact, but it now builds the
full site and therefore requires the Emscripten/XED prerequisites from option 3.

Pure wasm output:

```text
dist/test-pure-wasm.html
dist/pure-wasm.js
dist/uipack-cache.js
dist/style.css
dist/pkg/uica_wasm.js
dist/pkg/uica_wasm_bg.wasm
dist/data/manifest.json
dist/data/arch/*.uipack
```

Pure wasm API:

- `analyze_decoded_json_with_uipack(decoded_json, arch, uipack_bytes)`
- `analyze_decoded_json(decoded_json, arch)`
- `analyze_hex(hex, arch)` validates hex, then returns an XED-required error
  in this target.

#### Option 3: Emscripten/XED raw-byte/assembly web build

Use this for the main browser UI served at `index.html`. This target builds XED
with Emscripten, builds Rust for `wasm32-unknown-emscripten`, builds NASM for
browser-side assembly, links Rust/XED with `emcc`, and exposes one JS-facing
`uica_run` JSON/bytes ABI.

Install/activate emsdk and target:

```bash
git clone https://github.com/emscripten-core/emsdk.git ~/emsdk
cd ~/emsdk
./emsdk install 3.1.74
./emsdk activate 3.1.74
source ./emsdk_env.sh
cd /path/to/uiCA
rustup target add wasm32-unknown-emscripten
```

Generate data, then reinitialize XED submodules because `setup.sh` deinitializes
submodules after building Python/XED setup artifacts:

```bash
./setup.sh
git submodule update --init XED-to-XML mbuild
```

Build XED for Emscripten:

```bash
scripts/build-xed-emscripten.sh
```

Expected XED artifacts:

```text
target/xed-emscripten/install/include/xed/xed-interface.h
target/xed-emscripten/install/lib/libxed.a
```

Build Emscripten uiCA only:

```bash
scripts/build-uica-emscripten.sh dist/emscripten
scripts/smoke-emscripten-exports.sh dist/emscripten
```

Expected Emscripten-only artifacts:

```text
dist/emscripten/uica_emscripten.js
dist/emscripten/uica_emscripten.wasm
```

Build complete web site, including pure wasm, Emscripten/XED wasm, NASM wasm,
static files, and `.uipack` data:

```bash
./scripts/build-web.sh
scripts/smoke-emscripten-exports.sh dist/emscripten
```

Serve locally:

```bash
python3 -m http.server -d dist 8000
```

Open:

```text
http://127.0.0.1:8000/
```

The main page defaults to **Assembly** input. Assembly is NASM syntax, wrapped
as a 64-bit flat binary with `BITS 64` and `DEFAULT REL`, then passed to uiCA as
raw bytes. **Hex** mode bypasses NASM and sends bytes directly.

Smoke input:

```text
Microarchitecture: SKL
Assembly: add rax, rbx
```

The main page downloads/caches the selected UIPack, optionally assembles NASM
input, calls `dist/emscripten/uica_emscripten.js`, and displays:

- **Trace** tab: Rust-generated HTML execution trace in a sandboxed iframe.
- **JSON** tab: formatted `UicaResult` JSON.

#### GitHub Pages deployment

GitHub Pages deployment uses `.github/workflows/pages.yml` on pushes to
`master`. The workflow installs Rust, Python, `wasm-pack`, emsdk, and both wasm
targets. It runs `./setup.sh`, reinitializes `XED-to-XML mbuild`, builds both
wasm variants through `./scripts/build-web.sh`, checks Emscripten exports, and
uploads `dist/` as the Pages artifact.

The build artifact includes `CNAME` with `uica.houmus.org`. DNS for the
subdomain should point `uica.houmus.org` at the repository owner's GitHub Pages
host with a CNAME record (for this fork, typically `damageboy.github.io`). After
DNS resolves, configure the Pages custom domain in GitHub repository settings
and enable HTTPS.

Outputs in `dist/`:

- `dist/index.html` (Emscripten/XED raw-byte/assembly UI)
- `dist/test-pure-wasm.html` (pure wasm decoded-IR smoke page)
- `dist/main.js`
- `dist/pure-wasm.js`
- `dist/uipack-cache.js`
- `dist/style.css`
- `dist/nasm-assemble.js`
- `dist/nasm-worker.js`
- `dist/pkg/*` (pure wasm-pack output)
- `dist/emscripten/uica_emscripten.js`
- `dist/emscripten/uica_emscripten.wasm`
- `dist/nasm/nasm.js`
- `dist/nasm/nasm.wasm`
- `dist/nasm/LICENSE`
- `dist/data/manifest.json`
- `dist/data/arch/*.uipack`

## 3) Verification flow (Python oracle -> Rust candidate)

Canonical parity flow:

1. Capture Python baseline
2. Mirror baseline into `rust/<tag>` (current verifier is engine-scoped)
3. Verify Rust against same tag

Example (`quick` profile):

```bash
cargo build -p uica-cli

TMP_GOLDEN_DIR=$(mktemp -d)
TAG=py-local-$(date +%s)

python3 verification/tools/capture.py \
  --profile quick \
  --engine python \
  --golden-root "$TMP_GOLDEN_DIR" \
  --golden-tag "$TAG"

mkdir -p "$TMP_GOLDEN_DIR/rust"
cp -R "$TMP_GOLDEN_DIR/python/$TAG" "$TMP_GOLDEN_DIR/rust/$TAG"

python3 verification/tools/verify.py \
  --profile quick \
  --engine rust \
  --rust-bin target/debug/uica-cli \
  --golden-root "$TMP_GOLDEN_DIR" \
  --golden-tag "$TAG" \
  --dump-diff "$TMP_GOLDEN_DIR/quick.diff"
```

Note: currently expected to fail full parity until remaining engine behavior lands.

## 4) Crate responsibilities

### `uica-model`

Shared result contract (serde structs):

- `Invocation`
- `Summary`
- `UicaResult`

Defines v1 result envelope used by CLI + wasm + verification.

### `uica-data`

Loads generated manifest-selected UIPack data:

- `DataPack`
- `InstructionRecord`
- `load_pack(...)` / `load_uipack(...)`

### `uica-data-gen`

Converts `instructions.xml` into manifest + per-architecture `.uipack` files:

- `convert_xml_to_pack(...)`
- CLI entry in `src/main.rs`

### `uica-decode-ir`

Neutral decoded-instruction contract shared by XED, core, and wasm:

- `DecodedInstruction`
- `DecodedMemAddr`

Types derive serde so non-XED wasm consumers can pass decoded IR as JSON.

### `uica-decoder`

Input decoding helpers:

- `extract_text_from_object(...)`
- `decode_raw(...)`

Uses `object` for `.text` extraction and Intel XED via `uica-xed` / `uica-xed-sys` for x86-64 decoding, producing `uica-decode-ir` values.

### `uica-core`

Core analysis building blocks (partial/in progress):

- `micro_arch` (HSW/SKL/ICL config subset)
- `x64` register canonicalization
- `matcher` (minimal instruction matching)
- `analytical` (`compute_port_usage_limit`, `compute_issue_limit`)
- `engine` (XED-enabled byte wrapper; default feature)
- `engine_with_decoded` / `engine_with_decoded_pack` (Rust-only decoded-IR analysis path)
- loads UIPack data, matches instruction records, and computes current analytical/simulation summary paths; full Python/cycle parity is not complete

### `uica-cli`

Native Rust binary:

- parses clap-based flags (`--raw`, `--arch`, `--json`, `--tp-only`, `--event-trace`, `--min-cycles`, `--min-iterations`, `--alignment-offset`, `--init-policy`, `--no-micro-fusion`, `--no-macro-fusion`, `--simple-front-end`, `--verify-uipack`)
- loads bytes
- calls `uica_core::engine`
- writes JSON / prints throughput

### `uica-wasm`

Wasm API for Rust-only consumers. Public surface by caller:

- Pure wasm/browser callers: `analyze_decoded_json_with_uipack(decoded_json, arch, uipack_bytes) -> Result<String, String>`. JavaScript supplies decoded IR plus manifest-selected `.uipack` bytes; this is the preferred non-XED wasm API.
- Rust/native smoke tests and legacy/transitional callers: `analyze_decoded_json(decoded_json, arch) -> Result<String, String>`. This keeps the no-pack compatibility path and may fall back when default data is absent.
- Compatibility probes of the older/raw-byte shape: `analyze_hex(hex, arch) -> Result<String, String>`. It validates hex then returns an XED-required error; real raw-byte browser analysis belongs to `uica-emscripten`.

### `uica-emscripten`

Emscripten/XED web binary:

- links Rust uiCA with Emscripten-built XED
- exposes `uica_run(request_json, uipack_bytes)` through a C ABI consumed by `web/main.js`
- JS call chain: `web/main.js::callRun` -> `Module._uica_run` -> `rust/uica-emscripten/src/main.rs::uica_run` -> `uica_emscripten::run_request_json` in `src/lib.rs`
- accepts raw x86-64 hex, arch/options, and caller-supplied UIPack bytes
- returns `uica-web-result-v1` containing `trace_html` plus nested `UicaResult` JSON

## 5) Mapping: Python modules -> Rust crates

| Python source                                  | Rust target                       |
| ---------------------------------------------- | --------------------------------- |
| `uiCA.py` (CLI entry + simulation)             | `uica-cli` + `uica-core::engine`  |
| `facile.py` (analytical helpers)               | `uica-core::analytical` (partial) |
| `microArchConfigs.py`                          | `uica-core::micro_arch` (subset)  |
| `x64_lib.py`                                   | `uica-core::x64` (partial)        |
| `convertXML.py`                                | `uica-data-gen`                   |
| `instructions.py`/`instrData/*` runtime tables | `uica-data` datapack path         |
| Python JSON contract in `uiCA.py`              | `uica-model`                      |
| XED decoded instruction wrapper structs        | `uica-decode-ir`                  |

## 6) Current limitations

- `uica-core::engine` is partial/in progress; full Python/cycle parity is not complete.
- Rust-only wasm cannot decode raw x86 bytes; caller must provide `uica-decode-ir` JSON. Raw-byte browser analysis is provided by the separate Emscripten/XED target.
- Rust verify vs Python goldens currently mismatches broadly (expected at this stage).
- `micro_arch`, matcher, analytical logic, and simulation summary paths are partial slices, not full behavioral equivalence.

## 7) Development sequence (recommended)

1. Keep `quick` profile as primary parity loop.
2. Fix one case/arch mismatch path at time.
3. Expand to `curated12` -> `curated24` -> `curated48` only after stable gains.
4. Keep capture tags immutable for reproducible diff tracking.

For detailed command matrix and progressive runbook, see:

- `docs/verification-pipeline.md`
- `verification/README.md`
