# Rust-Only Wasm Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `wasm32-unknown-unknown` uiCA target with no XED dependency and prove it from `test-pure-wasm.html`, a test page under `uica.houmus.org` that loads shared architecture UIPack data as static resources, caches it, runs a hardcoded decoded-instruction stream, and displays a throughput number.

**Architecture:** Keep `rust/uica-wasm` as the Rust-only wasm target built by `wasm-pack`. Do not decode raw x86 bytes in this target. `test-pure-wasm.html` fetches shared `data/manifest.json` and selected `data/arch/<arch>.uipack` files from static site URLs, stores UIPack responses in the browser Cache API, passes pack bytes into wasm, and calls decoded-IR analysis. Future Emscripten/XED wasm page will consume the same `data/manifest.json` and `data/arch/*.uipack` resources; only decoder/runtime artifact differs.

**Tech Stack:** Rust `wasm32-unknown-unknown`, `wasm-bindgen`, `wasm-pack --target web`, existing `uica-decode-ir`, existing `uica-core` with `default-features = false`, `uica-data::MappedUiPackRuntime::from_bytes`, browser `fetch`, browser Cache API, static hosting at `uica.houmus.org`.

---

## File Structure

- Modify `rust/uica-wasm/Cargo.toml`
  - Add `uica-data` dependency; keep `uica-core` with `default-features = false`.
- Modify `rust/uica-wasm/src/lib.rs`
  - Add `analyze_decoded_json_with_uipack(decoded_json, arch, uipack_bytes)`.
  - Keep `analyze_decoded_json` as compatibility API or route it to clear error if no pack supplied.
  - Keep `analyze_hex` as validation-only unsupported raw-byte path.
- Modify `rust/uica-wasm/tests/node_smoke.rs`
  - Test analysis from supplied SKL UIPack bytes.
- Create `web/test-pure-wasm.html`
  - Rust-only decoded-stream test page.
  - uArch dropdown and hardcoded-stream explanation.
- Create `web/pure-wasm.js`
  - Load static manifest.
  - Populate architecture dropdown.
  - Fetch selected `.uipack` with Cache API caching.
  - Pass `Uint8Array` bytes to wasm.
  - Display throughput + cache status + JSON.
- Modify `web/style.css`
  - Add dropdown/sample/cache status styling usable by current/future pages.
- Modify `web/index.html`
  - Keep root page as neutral placeholder/link, not pure wasm demo and not raw-byte demo.
- Modify `scripts/build-web.sh`
  - Copy `web/test-pure-wasm.html` to `dist/test-pure-wasm.html`.
  - Copy `web/pure-wasm.js` to `dist/pure-wasm.js`.
  - Copy shared `rust/uica-data/generated/manifest.json` and `rust/uica-data/generated/arch/*.uipack` into `dist/data/` preserving relative paths from manifest.
  - Copy neutral `web/index.html` to `dist/index.html` until future real landing page replaces it.
- Modify `README.rust.md`
  - Document Rust-only wasm test page, shared static UIPack resources, Cache API behavior, and no-XED boundary.

## Approach Decision

Use shared static UIPack fetch + browser Cache API.

Accepted user constraints:

- Published site domain: `uica.houmus.org`.
- Pure wasm demo URL: `https://uica.houmus.org/test-pure-wasm.html`.
- Main site `index.html` is reserved for future real landing page.
- Web page has uArch dropdown.
- UIPack files are shared static resources for both pure wasm and future Emscripten/XED wasm implementations.
- UIPack files are downloaded once and served from browser cache afterward.
- No XED in pure wasm target.

Shared static resource contract:

```text
/test-pure-wasm.html        pure Rust wasm test page
/pure-wasm.js               pure Rust wasm page controller
/pkg/uica_wasm.js           wasm-bindgen JS glue
/pkg/uica_wasm_bg.wasm      pure Rust wasm binary
/data/manifest.json         shared data manifest
/data/arch/*.uipack         shared per-uArch data packs
```

Rejected alternatives:

- Embedded `SKL.uipack`: violates shared static-resource/dropdown requirement and bloats wasm.
- Fallback-only demo: gives throughput but not real uiCA data path.
- Service worker first: stronger offline story, but extra lifecycle complexity. Cache API from page script is enough for “download once, serve cache next time.”
- Raw bytes or assembly text: impossible in no-XED target unless another decoder is added.

---

### Task 1: Add failing wasm test for supplied UIPack bytes

**Files:**

- Modify: `rust/uica-wasm/tests/node_smoke.rs`

- [ ] **Step 1: Add test import**

Change import:

```rust
use uica_wasm::{analyze_decoded_json, analyze_hex};
```

to:

```rust
use uica_wasm::{analyze_decoded_json, analyze_decoded_json_with_uipack, analyze_hex};
```

- [ ] **Step 2: Add supplied-pack test**

Add test:

```rust
#[test]
fn analyze_decoded_json_with_uipack_uses_supplied_pack() {
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
    let decoded_json = serde_json::to_string(&decoded).expect("decoded IR should serialize");
    let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");

    let output = analyze_decoded_json_with_uipack(&decoded_json, "SKL", pack)
        .expect("decoded IR with supplied UIPack should analyze");
    let value: Value = serde_json::from_str(&output).expect("result should be json");

    assert_eq!(value["schema_version"], "uica-result-v1");
    assert_eq!(value["engine"], "rust");
    assert_eq!(value["invocation"]["arch"], "SKL");
    assert_eq!(value["parameters"]["uArchName"], "SKL");
    assert!(value["summary"]["throughput_cycles_per_iteration"].is_number());
    assert!(value["summary"]["limits"]["ports"].is_number());
}
```

- [ ] **Step 3: Run failing test**

Run:

```bash
cargo test -p uica-wasm --test node_smoke analyze_decoded_json_with_uipack_uses_supplied_pack
```

Expected: FAIL because `analyze_decoded_json_with_uipack` does not exist.

### Task 2: Add wasm API that accepts UIPack bytes

**Files:**

- Modify: `rust/uica-wasm/Cargo.toml`
- Modify: `rust/uica-wasm/src/lib.rs`

- [ ] **Step 1: Add data dependency**

In `rust/uica-wasm/Cargo.toml`, add:

```toml
uica-data = { path = "../uica-data" }
```

Keep:

```toml
uica-core = { path = "../uica-core", default-features = false }
```

- [ ] **Step 2: Import runtime type**

In `rust/uica-wasm/src/lib.rs`, add:

```rust
use uica_data::MappedUiPackRuntime;
```

- [ ] **Step 3: Add supplied-pack API**

Add:

```rust
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_decoded_json_with_uipack(
    decoded_json: &str,
    arch: &str,
    uipack_bytes: &[u8],
) -> Result<String, String> {
    let decoded: Vec<DecodedInstruction> =
        serde_json::from_str(decoded_json).map_err(|err| err.to_string())?;
    let invocation = Invocation {
        arch: arch.to_string(),
        ..Invocation::default()
    };
    let runtime = MappedUiPackRuntime::from_bytes(uipack_bytes.to_vec())
        .map_err(|err| err.to_string())?;
    let output = engine::engine_output_with_decoded_uipack_runtime(
        &decoded,
        &invocation,
        &runtime,
        false,
    )?;

    serde_json::to_string(&output.result).map_err(|err| err.to_string())
}
```

- [ ] **Step 4: Make legacy decoded API explicit**

Change existing `analyze_decoded_json(decoded_json, arch)` to keep working for native tests only if desired, but for browser semantics prefer explicit error:

```rust
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_decoded_json(_decoded_json: &str, _arch: &str) -> Result<String, String> {
    Err("decoded analysis requires a UIPack; use analyze_decoded_json_with_uipack".to_string())
}
```

If compatibility with existing Rust tests is preferred, keep current function unchanged and add new test only for supplied-pack path. Web must call `analyze_decoded_json_with_uipack`.

- [ ] **Step 5: Run supplied-pack test**

Run:

```bash
cargo test -p uica-wasm --test node_smoke analyze_decoded_json_with_uipack_uses_supplied_pack
```

Expected: PASS.

### Task 3: Copy shared UIPack static resources and test page into web dist

**Files:**

- Modify: `scripts/build-web.sh`

- [ ] **Step 1: Copy page/controller and data resources**

After static JS/CSS copies, add:

```bash
cp "$ROOT_DIR/web/test-pure-wasm.html" "$DIST_DIR/test-pure-wasm.html"
cp "$ROOT_DIR/web/pure-wasm.js" "$DIST_DIR/pure-wasm.js"
mkdir -p "$DIST_DIR/data/arch"
cp "$ROOT_DIR/rust/uica-data/generated/manifest.json" "$DIST_DIR/data/manifest.json"
cp "$ROOT_DIR/rust/uica-data/generated/arch/"*.uipack "$DIST_DIR/data/arch/"
```

Keep `dist/index.html` as a neutral placeholder/link page; do not make pure wasm test the root page.

- [ ] **Step 2: Build web bundle**

Run:

```bash
./scripts/build-web.sh
```

Expected: `dist/test-pure-wasm.html`, `dist/pure-wasm.js`, `dist/data/manifest.json`, and `dist/data/arch/SKL.uipack` exist.

- [ ] **Step 3: Verify manifest paths resolve under dist**

Run:

```bash
python3 - <<'PY'
import json
from pathlib import Path
manifest = json.loads(Path('dist/data/manifest.json').read_text())
missing=[]
for arch, entry in manifest['architectures'].items():
    path = Path('dist/data') / entry['path']
    if not path.exists():
        missing.append((arch, str(path)))
print('missing', missing)
assert not missing
PY
```

Expected: prints `missing []`.

### Task 4: Add `test-pure-wasm.html` with uArch dropdown and Cache API loader

**Files:**

- Create: `web/test-pure-wasm.html`
- Create: `web/pure-wasm.js`
- Modify: `web/style.css`

- [ ] **Step 1: Create test page HTML**

Create `web/test-pure-wasm.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>uiCA pure wasm test</title>
    <link rel="stylesheet" href="./style.css" />
  </head>
  <body>
    <main class="page">
      <section class="panel">
        <h1>uiCA pure wasm test</h1>
        <p class="subtitle">
          Rust-only wasm target: no XED, hardcoded decoded instruction stream.
        </p>

        <div class="sample-box">
          <strong>Sample stream</strong>
          <p>
            <code>add rax, rbx</code> as decoded IR. No raw x86 decoding happens
            in this target.
          </p>
        </div>

        <label for="arch-select">Microarchitecture</label>
        <select id="arch-select"></select>
        <p id="cache-status" class="subtitle">UIPack cache not loaded yet.</p>

        <div class="actions">
          <button id="analyze-button" type="button" disabled>Analyze</button>
          <span id="status">Loading wasm…</span>
        </div>
      </section>

      <section class="panel">
        <h2>Result</h2>
        <pre id="output">Waiting for wasm…</pre>
      </section>
    </main>

    <script type="module" src="./pure-wasm.js"></script>
  </body>
</html>
```

- [ ] **Step 2: Create JS controller imports and DOM refs**

Create `web/pure-wasm.js` starting with:

```js
import init, { analyze_decoded_json_with_uipack } from "./pkg/uica_wasm.js";

const button = document.getElementById("analyze-button");
const status = document.getElementById("status");
const output = document.getElementById("output");
const archSelect = document.getElementById("arch-select");
const cacheStatus = document.getElementById("cache-status");
```

- [ ] **Step 3: Add constants**

Add:

```js
const MANIFEST_URL = "./data/manifest.json";
const UIPACK_CACHE = "uica-uipack-v1";

const SAMPLE_DECODED_STREAM = [
  {
    ip: 0,
    len: 3,
    mnemonic: "add",
    disasm: "add rax, rbx",
    bytes: [0x48, 0x01, 0xd8],
    pos_nominal_opcode: 1,
    input_regs: ["RAX", "RBX"],
    output_regs: ["RAX"],
    reads_flags: false,
    writes_flags: true,
    has_memory_read: false,
    has_memory_write: false,
    mem_addrs: [],
    implicit_rsp_change: 0,
    immediate: null,
    immediate_width_bits: 0,
    has_66_prefix: false,
    iform: "ADD_GPRv_GPRv",
    iform_signature: "ADD_GPRv_GPRv",
    max_op_size_bytes: 8,
    uses_high8_reg: false,
    explicit_reg_operands: ["RAX", "RBX"],
    agen: null,
    xml_attrs: {},
  },
];
```

- [ ] **Step 4: Add manifest loader**

Add:

```js
let manifest = null;

async function loadManifest() {
  const response = await fetch(MANIFEST_URL, { cache: "no-cache" });
  if (!response.ok) {
    throw new Error(`Failed to load manifest: ${response.status}`);
  }
  manifest = await response.json();
  const arches = Object.keys(manifest.architectures).sort();
  archSelect.replaceChildren(
    ...arches.map((arch) => {
      const option = document.createElement("option");
      option.value = arch;
      option.textContent = arch;
      option.selected = arch === "SKL";
      return option;
    }),
  );
}
```

- [ ] **Step 5: Add Cache API UIPack loader**

Add:

```js
async function fetchCachedUipack(arch) {
  const entry = manifest.architectures[arch];
  if (!entry) {
    throw new Error(`Unknown architecture ${arch}`);
  }
  const url = new URL(`./data/${entry.path}`, window.location.href).toString();
  const request = new Request(url);

  if (!("caches" in window)) {
    cacheStatus.textContent =
      "Cache API unavailable; fetching UIPack from network.";
    const response = await fetch(request);
    if (!response.ok) {
      throw new Error(`Failed to fetch ${arch} UIPack: ${response.status}`);
    }
    return new Uint8Array(await response.arrayBuffer());
  }

  const cache = await caches.open(UIPACK_CACHE);
  const cached = await cache.match(request);
  if (cached) {
    cacheStatus.textContent = `${arch} UIPack loaded from browser cache.`;
    return new Uint8Array(await cached.arrayBuffer());
  }

  cacheStatus.textContent = `Downloading ${arch} UIPack...`;
  const response = await fetch(request);
  if (!response.ok) {
    throw new Error(`Failed to fetch ${arch} UIPack: ${response.status}`);
  }
  await cache.put(request, response.clone());
  cacheStatus.textContent = `${arch} UIPack downloaded and cached.`;
  return new Uint8Array(await response.arrayBuffer());
}
```

- [ ] **Step 6: Update boot**

Add:

```js
async function boot() {
  try {
    await init();
    await loadManifest();
    status.textContent = "Wasm ready";
    button.disabled = false;
  } catch (error) {
    status.textContent = "Wasm load failed";
    output.textContent = String(error);
  }
}
```

- [ ] **Step 7: Update analyze flow**

Add:

```js
async function runAnalyze() {
  button.disabled = true;
  try {
    const arch = archSelect.value;
    const packBytes = await fetchCachedUipack(arch);
    const result = analyze_decoded_json_with_uipack(
      JSON.stringify(SAMPLE_DECODED_STREAM),
      arch,
      packBytes,
    );
    const parsed = JSON.parse(result);
    const tp = parsed.summary.throughput_cycles_per_iteration;
    output.textContent = `Throughput: ${tp} cycles/iteration\nArchitecture: ${arch}\n\n${JSON.stringify(parsed, null, 2)}`;
    status.textContent = "Analysis complete";
  } catch (error) {
    output.textContent = String(error);
    status.textContent = "Analysis failed";
  } finally {
    button.disabled = false;
  }
}

button.addEventListener("click", () => {
  void runAnalyze();
});

boot();
```

- [ ] **Step 8: Add styles**

Add:

```css
select {
  width: 100%;
  border-radius: 10px;
  border: 1px solid #3c4d60;
  background: #0a0f14;
  color: inherit;
  font: inherit;
  padding: 12px;
}

.sample-box {
  border: 1px solid #3c4d60;
  border-radius: 10px;
  background: #0a0f14;
  padding: 12px;
}

code {
  color: #9ec1ff;
}
```

### Task 5: Build web bundle and inspect static/cached path assumptions

**Files:**

- Generated: `dist/` (do not commit unless project policy says so)

- [ ] **Step 1: Build wasm web target**

Run:

```bash
./scripts/build-web.sh
```

Expected: succeeds with `wasm-pack build`, writes `dist/pkg/uica_wasm.js`, `dist/pkg/uica_wasm_bg.wasm`, `dist/test-pure-wasm.html`, `dist/pure-wasm.js`, and `dist/data/**`.

- [ ] **Step 2: Verify generated JS exports supplied-pack API**

Run:

```bash
rg "analyze_decoded_json_with_uipack" dist/pkg/uica_wasm.js dist/pure-wasm.js
```

Expected: both generated wasm bindings and test page JS reference `analyze_decoded_json_with_uipack`.

- [ ] **Step 3: Verify no raw-byte demo path remains in test page**

Run:

```bash
rg "Hex bytes|analyze_hex" web/test-pure-wasm.html web/pure-wasm.js dist/test-pure-wasm.html dist/pure-wasm.js || true
```

Expected: no output.

- [ ] **Step 4: Verify shared data copied**

Run:

```bash
ls dist/data/manifest.json dist/data/arch/SKL.uipack
```

Expected: both files exist.

### Task 6: Browser smoke test path

**Files:**

- No source changes unless manual smoke reveals bug.

- [ ] **Step 1: Start static server**

Run:

```bash
python3 -m http.server 8000 -d dist
```

Expected: server starts at `http://0.0.0.0:8000/`.

- [ ] **Step 2: Manual browser smoke**

Open:

```text
http://localhost:8000/test-pure-wasm.html
```

Expected:

- dropdown populated from `dist/data/manifest.json`
- first click downloads selected UIPack and shows cache message `downloaded and cached`
- output starts with `Throughput: <number> cycles/iteration`
- second click shows cache message `loaded from browser cache`

If no browser is available in agent harness, document this as manual verification required and rely on build/static checks.

### Task 7: Documentation and final verification

**Files:**

- Modify: `README.rust.md`

- [ ] **Step 1: Document Rust-only wasm target**

Add notes:

```markdown
Rust-only wasm (`uica-wasm`) targets `wasm32-unknown-unknown` through `wasm-pack` and intentionally excludes XED. The test page is `test-pure-wasm.html` under the static site (for production: `https://uica.houmus.org/test-pure-wasm.html`). It accepts decoded IR JSON plus caller-supplied UIPack bytes through `analyze_decoded_json_with_uipack`. The page loads shared `data/manifest.json` and `data/arch/*.uipack` resources and caches `.uipack` responses with the browser Cache API. Future wasm implementations, including Emscripten/XED, should reuse the same manifest and UIPack URLs. Raw x86 byte decoding belongs to the future Emscripten/XED wasm target. The root `index.html` is only a placeholder/link until the real site landing page is added.
```

- [ ] **Step 2: Run full relevant verification**

Run:

```bash
cargo test -p uica-wasm --no-default-features
cargo test -p uica-core --no-default-features
cargo test --workspace
cargo check -p uica-core --no-default-features
cargo check -p uica-wasm --no-default-features
./scripts/build-web.sh
cargo tree -p uica-wasm --no-default-features | rg 'uica-(xed|decoder)' || true
```

Expected:

- tests/checks pass
- web build passes
- dependency tree grep prints no XED/decoder crates

- [ ] **Step 3: Git hygiene**

Run:

```bash
git status --short
git diff --check
```

Expected: only intended source/docs changes; no generated `dist/` committed unless explicitly requested.
