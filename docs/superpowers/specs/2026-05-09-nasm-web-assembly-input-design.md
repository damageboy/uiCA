# NASM Web Assembly Input Design

## Goal

Add browser-side NASM assembly input to the Emscripten/XED uiCA web build so users can paste x86-64 assembly instead of only raw hex bytes. NASM compiles assembly to flat machine-code bytes in the browser, then existing uiCA Emscripten/XED analysis consumes those bytes through the current JSON/hex ABI.

## Research Summary

Upstream NASM can be compiled with Emscripten without patching assembler logic. Release tarballs are safer than git checkouts because generated sources are already present and the build avoids host-vs-wasm generator traps. A local proof of concept built NASM 3.01 with Emscripten and assembled:

```asm
add rax, rbx
```

through a wrapper:

```asm
BITS 64
DEFAULT REL
%include "/work/user.asm"
```

using `nasm -f bin`, producing expected bytes:

```text
48 01 d8
```

tweetx86 vendors NASM 2.15rc0 and patches `main()` into a custom exported `nasm(char *argsStr)` wrapper called from an iframe. That proved viability, but uiCA should not copy that approach because modern Emscripten exposes `callMain([...])`, avoids fragile string command-line splitting, and can run cleanly inside a Web Worker.

## Architecture

Add a separate NASM Emscripten artifact alongside the existing uiCA Emscripten artifact:

```text
index.html / main.js
  -> web/nasm-assemble.js
      -> Worker(web/nasm-worker.js)
          -> import dist/nasm/nasm.js
          -> load dist/nasm/nasm.wasm
          -> write /work/user.asm
          -> write /work/input.asm wrapper
          -> callMain(["-f", "bin", "/work/input.asm", "-o", "/work/out.bin"])
          -> read /work/out.bin
  -> convert bytes to hex
  -> existing uica_emscripten _uica_run request
  -> trace HTML + JSON output
```

No Rust ABI change is required. `uica_emscripten` continues to receive `{ hex, arch, invocation }` and UIPack bytes. No `instructions.json` fallback or alternate instruction-data path is introduced.

## Build Design

Add `scripts/build-nasm-emscripten.sh`:

- Pin a NASM release, initially `3.01`.
- Download official release tarball from `https://www.nasm.us/pub/nasm/releasebuilds/3.01/`.
- Verify a hard-coded SHA-256 before unpacking.
- Build with active emsdk using `emconfigure` and `emmake`.
- Emit `dist/nasm/nasm.js`, `dist/nasm/nasm.wasm`, and `dist/nasm/LICENSE`.

Expected build flags:

```bash
emconfigure ./configure \
  --host=wasm32-unknown-emscripten \
  CC=emcc AR=emar RANLIB=emranlib \
  CFLAGS="-O3 -DNDEBUG" \
  LDFLAGS="-O3 \
    -sWASM=1 \
    -sMODULARIZE=1 \
    -sEXPORT_ES6=1 \
    -sENVIRONMENT=web,worker \
    -sINVOKE_RUN=0 \
    -sEXIT_RUNTIME=1 \
    -sALLOW_MEMORY_GROWTH=1 \
    -sFORCE_FILESYSTEM=1 \
    -sEXPORTED_RUNTIME_METHODS=FS,callMain"
emmake make -j"$(nproc)" nasm
```

`-sEXIT_RUNTIME=1` is acceptable because each assembly run uses a fresh worker and fresh module instance. This avoids NASM process-global state issues seen when calling `callMain()` twice in one module instance.

Add `scripts/smoke-nasm-emscripten.sh`:

- Assert `nasm.js`, `nasm.wasm`, and `LICENSE` exist.
- Run a small Node ES-module smoke test using `wasmBinary`.
- Assemble `BITS 64\nadd rax, rbx\n` with `-f bin`.
- Assert output hex is `4801d8`.

Update `scripts/build-web.sh` to run NASM build and smoke before copying/checking the final site. Update Pages and rust-parity workflows to check the NASM artifacts.

## Runtime Design

Create `web/nasm-assemble.js` as the main-thread wrapper. It exposes:

```js
assembleNasm(source: string): Promise<{ bytes: Uint8Array, hex: string, stderr: string }>
```

It creates a new Worker per call, passes user source, waits for success/error, then terminates the worker. Worker startup cost is acceptable for interactive analysis and keeps NASM state isolated.

Create `web/nasm-worker.js` for the actual Emscripten/NASM call. The worker:

- Imports `./nasm/nasm.js` relative to the deployed `dist/` root.
- Loads `./nasm/nasm.wasm` through `locateFile`.
- Captures `print` and `printErr` output.
- Creates a unique `/work` directory per worker instance.
- Writes `/work/user.asm` from the posted source.
- Writes `/work/input.asm` as:

```asm
BITS 64
DEFAULT REL
%include "/work/user.asm"
```

- Calls NASM with `-f bin` and absolute MEMFS paths.
- Reads `/work/out.bin` as bytes.
- Posts bytes, stdout, and stderr back to the main thread.

Error handling:

- Non-zero NASM return code becomes a user-visible error with stderr.
- Worker load failure becomes `NASM failed to load`.
- Missing output file becomes `NASM produced no output`.
- Empty source is allowed only if NASM emits an empty binary; uiCA analysis will then fail naturally on empty code if unsupported.

## UI Design

Default web input mode becomes Assembly. Hex mode remains available and preserves current behavior.

Controls:

```text
Input mode: [ Assembly | Hex ]
```

Assembly textarea default:

```asm
add rax, rbx
```

Hex textarea/default still supports:

```text
48 01 d8
```

When Assembly mode succeeds, show an assembled-byte preview near the status text:

```text
Assembled: 48 01 d8
```

The Analyze button flow becomes:

- Load selected UIPack.
- If mode is Assembly, assemble with NASM first.
- Feed resulting hex into existing uiCA call.
- Render Trace tab by default and JSON tab as today.

NASM diagnostics go to the JSON/output panel on failure so line numbers and messages remain copyable.

## Constraints and Non-Goals

- Do not modify Rust engine, XED decoder, UIPack runtime, or pure wasm decoded-IR path unless tests reveal a direct integration bug.
- Do not add `instructions.json` or `instructions_full.json` fallbacks.
- Do not support uploaded include files in the first iteration.
- Do not parse ELF/object output in JavaScript; use NASM `-f bin` only.
- Do not vendor tweetx86's patched NASM source.
- Do not make NASM available in pure wasm `test-pure-wasm.html`; this feature targets main Emscripten/XED `index.html`.

## Tests and Verification

Local checks:

```bash
source ~/emsdk/emsdk_env.sh
./scripts/build-nasm-emscripten.sh dist/nasm
./scripts/smoke-nasm-emscripten.sh dist/nasm
./scripts/build-web.sh
scripts/smoke-emscripten-exports.sh dist/emscripten
node --check web/main.js
node --check web/nasm-assemble.js
node --check web/nasm-worker.js
```

Browser smoke:

1. Serve `dist/` locally.
2. Open `index.html`.
3. Select Assembly mode, `SKL`, source `add rax, rbx`.
4. Confirm assembled preview is `48 01 d8`.
5. Confirm Trace tab renders and JSON tab shows same throughput as hex input.
6. Enter invalid assembly and confirm NASM diagnostics appear.

CI checks:

- Pages workflow builds NASM and uploads `dist/nasm/*`.
- rust-parity workflow builds NASM as part of `build-web.sh`.
- NASM smoke script fails CI if assembly output changes or artifacts are missing.

## Documentation

Update `README.rust.md` Option 3 to mention assembly input, NASM build prerequisites, NASM artifacts, and the fact that web Assembly mode compiles to flat bytes before uiCA analysis. Add a short UI note that the assembly syntax is NASM, not GNU `.intel_syntax`.

## Licensing

NASM uses the simplified BSD license. The build must copy NASM's `LICENSE` into `dist/nasm/LICENSE` and keep source URL/version in the build script. No tweetx86 code or patched NASM source will be bundled.
