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
  -> uica-wasm::analyze_decoded_json
      -> uica-core::engine_with_decoded
      -> JSON string
```

## 2) Build and run

### Prereqs

- Rust toolchain (cargo)
- For wasm build: `wasm-pack` + `wasm32-unknown-unknown` target
- Python env for verification harness
- Intel XED submodule initialized (`git submodule update --init`). Native Rust builds compile/link the repo-local XED library automatically through `uica-xed-sys` when needed.

Install wasm target:

```bash
rustup target add wasm32-unknown-unknown
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

### Build web bundle (wasm + static files)

```bash
./scripts/build-web.sh
```

Rust-only wasm builds do not depend on XED. `analyze_decoded_json` accepts caller-supplied decoded IR; `analyze_hex` validates hex then returns an explicit XED-required error until the planned Emscripten/XED target lands.

Outputs in `dist/`:

- `dist/index.html`
- `dist/main.js`
- `dist/style.css`
- `dist/pkg/*` (wasm-pack output)

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

Wasm API for Rust-only consumers:

- `analyze_decoded_json(decoded_json, arch) -> Result<String, String>`
- `analyze_hex(hex, arch) -> Result<String, String>` validates hex then returns an XED-required error in this target

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
- Rust-only wasm cannot decode raw x86 bytes; caller must provide `uica-decode-ir` JSON. Raw-byte wasm analysis is planned for a separate Emscripten/XED target.
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
