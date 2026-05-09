# NASM Web Assembly Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the main Emscripten/XED web UI accept NASM x86-64 assembly, assemble it in the browser to flat bytes, and feed those bytes into the existing uiCA analysis path.

**Architecture:** Build upstream NASM 3.01 with Emscripten as a separate `dist/nasm/` artifact. Browser assembly runs in a fresh Web Worker per analysis, uses NASM `-f bin`, converts bytes to hex, then calls existing `uica_emscripten` ABI unchanged.

**Tech Stack:** Bash, Emscripten 3.1.74, NASM 3.01, JavaScript ES modules, Web Workers, GitHub Actions, existing Rust/Emscripten uiCA build.

---

## File Structure

- Create `scripts/build-nasm-emscripten.sh`: downloads, verifies, builds, and copies NASM Emscripten artifacts.
- Create `scripts/smoke-nasm-emscripten.sh`: Node smoke test for `dist/nasm/nasm.js` + `.wasm`.
- Create `web/nasm-assemble.js`: main-thread promise wrapper around one-shot NASM Worker runs.
- Create `web/nasm-worker.js`: Worker that loads NASM module, writes MEMFS files, runs `callMain`, returns bytes or diagnostics.
- Modify `scripts/build-web.sh`: run NASM build/smoke and copy new web worker files into `dist/`.
- Modify `.github/workflows/pages.yml`: verify NASM artifacts in Pages build.
- Modify `.github/workflows/rust-parity.yml`: verify NASM artifacts in web build gate.
- Modify `web/index.html`: add Assembly/Hex mode control, assembly textarea, hex textarea panel, assembled-byte preview.
- Modify `web/main.js`: import assembler wrapper, wire input mode, assemble before analysis when needed.
- Modify `web/style.css`: style mode buttons, hidden panels, and assembled preview.
- Modify `README.rust.md`: document NASM assembly input and build artifacts.

## Task 1: Build NASM with Emscripten

**Files:**
- Create: `scripts/build-nasm-emscripten.sh`
- Create: `scripts/smoke-nasm-emscripten.sh`

- [ ] **Step 1: Add failing smoke script first**

Create `scripts/smoke-nasm-emscripten.sh` with this content:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${1:-$ROOT_DIR/dist/nasm}"
JS="$DIST_DIR/nasm.js"
WASM="$DIST_DIR/nasm.wasm"
LICENSE="$DIST_DIR/LICENSE"

for file in "$JS" "$WASM" "$LICENSE"; do
	if [[ ! -f "$file" ]]; then
		echo "missing $file" >&2
		exit 1
	fi
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

cat >"$TMP_DIR/package.json" <<'JSON'
{
  "type": "module"
}
JSON

cat >"$TMP_DIR/smoke.mjs" <<'JS'
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const distDir = process.argv[2];
const createNasm = (await import(pathToFileURL(`${distDir}/nasm.js`).href)).default;
const stderr = [];
const stdout = [];
const module = await createNasm({
	wasmBinary: readFileSync(`${distDir}/nasm.wasm`),
	noInitialRun: true,
	print: (line) => stdout.push(line),
	printErr: (line) => stderr.push(line),
});

module.FS.writeFile("/in.asm", "BITS 64\nadd rax, rbx\n");
const rc = module.callMain(["-f", "bin", "/in.asm", "-o", "/out.bin"]);
if (rc !== 0) {
	throw new Error(`NASM returned ${rc}: ${stderr.join("\n")}`);
}
const bytes = module.FS.readFile("/out.bin");
const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
if (hex !== "4801d8") {
	throw new Error(`unexpected NASM output ${hex}; stdout=${stdout.join("\n")} stderr=${stderr.join("\n")}`);
}
JS

node "$TMP_DIR/smoke.mjs" "$DIST_DIR"
echo "NASM Emscripten smoke passed"
```

- [ ] **Step 2: Verify smoke fails before artifacts exist**

Run:

```bash
chmod +x scripts/smoke-nasm-emscripten.sh
./scripts/smoke-nasm-emscripten.sh dist/nasm
```

Expected: FAIL with `missing .../dist/nasm/nasm.js`.

- [ ] **Step 3: Add NASM build script**

Create `scripts/build-nasm-emscripten.sh` with this content:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/dist/nasm}"
BUILD_ROOT="${UICA_NASM_EMSCRIPTEN_OUT_DIR:-$ROOT_DIR/target/nasm-emscripten}"
VERSION="3.01"
TARBALL="nasm-$VERSION.tar.xz"
URL="https://www.nasm.us/pub/nasm/releasebuilds/$VERSION/$TARBALL"
SHA256="b7324cbe86e767b65f26f467ed8b12ad80e124e3ccb89076855c98e43a9eddd4"
SRC_DIR="$BUILD_ROOT/nasm-$VERSION"
ARCHIVE="$BUILD_ROOT/$TARBALL"

require_tool() {
	local tool="$1"
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "$tool not found; activate emsdk first" >&2
		exit 127
	fi
}

for tool in emcc emconfigure emmake emar emranlib curl sha256sum tar make; do
	require_tool "$tool"
done

mkdir -p "$BUILD_ROOT" "$OUT_DIR"

if [[ ! -f "$ARCHIVE" ]]; then
	curl -fsSL "$URL" -o "$ARCHIVE"
fi
printf '%s  %s\n' "$SHA256" "$ARCHIVE" | sha256sum -c -

rm -rf "$SRC_DIR"
tar -C "$BUILD_ROOT" -xf "$ARCHIVE"

(
	cd "$SRC_DIR"
	emconfigure ./configure \
		--host=wasm32-unknown-emscripten \
		CC=emcc \
		AR=emar \
		RANLIB=emranlib \
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
)

cp "$SRC_DIR/nasm" "$OUT_DIR/nasm.js"
cp "$SRC_DIR/nasm.wasm" "$OUT_DIR/nasm.wasm"
cp "$SRC_DIR/LICENSE" "$OUT_DIR/LICENSE"

test -f "$OUT_DIR/nasm.js"
test -f "$OUT_DIR/nasm.wasm"
test -f "$OUT_DIR/LICENSE"
printf 'Built NASM Emscripten bundle in %s\n' "$OUT_DIR"
```

- [ ] **Step 4: Make scripts executable**

Run:

```bash
chmod +x scripts/build-nasm-emscripten.sh scripts/smoke-nasm-emscripten.sh
```

Expected: no output.

- [ ] **Step 5: Build and smoke NASM locally**

Run:

```bash
source ~/emsdk/emsdk_env.sh
./scripts/build-nasm-emscripten.sh dist/nasm
./scripts/smoke-nasm-emscripten.sh dist/nasm
```

Expected output includes:

```text
Built NASM Emscripten bundle in .../dist/nasm
NASM Emscripten smoke passed
```

- [ ] **Step 6: Commit Task 1**

Run:

```bash
git add scripts/build-nasm-emscripten.sh scripts/smoke-nasm-emscripten.sh
git commit -m "Add NASM Emscripten build scripts"
```

## Task 2: Add browser NASM assembly wrapper

**Files:**
- Create: `web/nasm-worker.js`
- Create: `web/nasm-assemble.js`

- [ ] **Step 1: Add worker file**

Create `web/nasm-worker.js` with this content:

```js
import createNasm from "./nasm/nasm.js";

function toPlainError(error) {
	if (error instanceof Error) {
		return error.message;
	}
	return String(error);
}

self.onmessage = async (event) => {
	const { id, source } = event.data;
	const stdout = [];
	const stderr = [];
	try {
		const nasm = await createNasm({
			locateFile: (path) => `./nasm/${path}`,
			noInitialRun: true,
			print: (line) => stdout.push(line),
			printErr: (line) => stderr.push(line),
		});
		const workDir = "/work";
		nasm.FS.mkdir(workDir);
		nasm.FS.writeFile(`${workDir}/user.asm`, source);
		nasm.FS.writeFile(
			`${workDir}/input.asm`,
			'BITS 64\nDEFAULT REL\n%include "/work/user.asm"\n',
		);
		const rc = nasm.callMain([
			"-f",
			"bin",
			`${workDir}/input.asm`,
			"-o",
			`${workDir}/out.bin`,
		]);
		if (rc !== 0) {
			throw new Error(stderr.join("\n") || `NASM failed with exit code ${rc}`);
		}
		let bytes;
		try {
			bytes = nasm.FS.readFile(`${workDir}/out.bin`);
		} catch (_error) {
			throw new Error("NASM produced no output");
		}
		self.postMessage({
			id,
			ok: true,
			bytes,
			stdout: stdout.join("\n"),
			stderr: stderr.join("\n"),
		});
	} catch (error) {
		self.postMessage({
			id,
			ok: false,
			error: toPlainError(error),
			stdout: stdout.join("\n"),
			stderr: stderr.join("\n"),
		});
	}
};
```

- [ ] **Step 2: Add main-thread wrapper**

Create `web/nasm-assemble.js` with this content:

```js
let nextRequestId = 1;

export function bytesToHex(bytes) {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(" ");
}

export function assembleNasm(source) {
	const id = nextRequestId++;
	const worker = new Worker(new URL("./nasm-worker.js", import.meta.url), {
		type: "module",
	});
	return new Promise((resolve, reject) => {
		worker.onmessage = (event) => {
			if (event.data.id !== id) {
				return;
			}
			worker.terminate();
			if (!event.data.ok) {
				const details = event.data.stderr || event.data.error;
				reject(new Error(details || "NASM assembly failed"));
				return;
			}
			const bytes = new Uint8Array(event.data.bytes);
			resolve({
				bytes,
				hex: bytesToHex(bytes),
				stdout: event.data.stdout ?? "",
				stderr: event.data.stderr ?? "",
			});
		};
		worker.onerror = (event) => {
			worker.terminate();
			reject(new Error(event.message || "NASM worker failed"));
		};
		worker.postMessage({ id, source });
	});
}
```

- [ ] **Step 3: Syntax check new modules**

Run:

```bash
node --check web/nasm-worker.js
node --check web/nasm-assemble.js
```

Expected: both commands exit 0.

- [ ] **Step 4: Commit Task 2**

Run:

```bash
git add web/nasm-worker.js web/nasm-assemble.js
git commit -m "Add browser NASM assembly wrapper"
```

## Task 3: Integrate NASM into web build and CI checks

**Files:**
- Modify: `scripts/build-web.sh`
- Modify: `.github/workflows/pages.yml`
- Modify: `.github/workflows/rust-parity.yml`

- [ ] **Step 1: Update `scripts/build-web.sh`**

Edit `scripts/build-web.sh`:

1. Add NASM dist variable after `EMSCRIPTEN_DIR`:

```bash
NASM_DIR="$DIST_DIR/nasm"
```

2. Add NASM build after uiCA Emscripten build:

```bash
"$ROOT_DIR/scripts/build-nasm-emscripten.sh" "$NASM_DIR"
"$ROOT_DIR/scripts/smoke-nasm-emscripten.sh" "$NASM_DIR"
```

3. Copy new JS files after existing `main.js` copy:

```bash
cp "$ROOT_DIR/web/nasm-assemble.js" "$DIST_DIR/nasm-assemble.js"
cp "$ROOT_DIR/web/nasm-worker.js" "$DIST_DIR/nasm-worker.js"
```

4. Add final assertions:

```bash
test -f "$DIST_DIR/nasm-assemble.js"
test -f "$DIST_DIR/nasm-worker.js"
test -f "$DIST_DIR/nasm/nasm.js"
test -f "$DIST_DIR/nasm/nasm.wasm"
test -f "$DIST_DIR/nasm/LICENSE"
```

- [ ] **Step 2: Verify `build-web.sh` produces NASM files**

Run:

```bash
source ~/emsdk/emsdk_env.sh
./scripts/build-web.sh
```

Expected output includes:

```text
NASM Emscripten smoke passed
Built web bundle in .../dist
```

Also verify:

```bash
test -f dist/nasm/nasm.js
test -f dist/nasm/nasm.wasm
test -f dist/nasm/LICENSE
test -f dist/nasm-assemble.js
test -f dist/nasm-worker.js
```

Expected: all commands exit 0.

- [ ] **Step 3: Update Pages workflow checks**

In `.github/workflows/pages.yml`, inside `Build Pages artifact`, add after existing Emscripten artifact checks:

```yaml
          test -f dist/nasm/nasm.js
          test -f dist/nasm/nasm.wasm
          test -f dist/nasm/LICENSE
          test -f dist/nasm-assemble.js
          test -f dist/nasm-worker.js
```

- [ ] **Step 4: Update rust-parity workflow checks**

In `.github/workflows/rust-parity.yml`, inside `Build web bundle`, add after existing Emscripten artifact checks:

```yaml
          test -f dist/nasm/nasm.js
          test -f dist/nasm/nasm.wasm
          test -f dist/nasm/LICENSE
          test -f dist/nasm-assemble.js
          test -f dist/nasm-worker.js
```

- [ ] **Step 5: Validate YAML and shell syntax**

Run:

```bash
bash -n scripts/build-web.sh
bash -n scripts/build-nasm-emscripten.sh
bash -n scripts/smoke-nasm-emscripten.sh
python3 - <<'PY'
import yaml
for path in ['.github/workflows/pages.yml', '.github/workflows/rust-parity.yml']:
    with open(path) as f:
        yaml.safe_load(f)
print('workflow yaml ok')
PY
```

Expected:

```text
workflow yaml ok
```

- [ ] **Step 6: Commit Task 3**

Run:

```bash
git add scripts/build-web.sh .github/workflows/pages.yml .github/workflows/rust-parity.yml
git commit -m "Build NASM with web bundle"
```

## Task 4: Add Assembly/Hex UI mode

**Files:**
- Modify: `web/index.html`
- Modify: `web/style.css`

- [ ] **Step 1: Update input markup**

In `web/index.html`, replace the current hex label/textarea block:

```html
        <label for="hex-input">x86-64 bytes (hex)</label>
        <textarea id="hex-input" rows="6" spellcheck="false">48 01 d8</textarea>
```

with:

```html
        <div class="input-mode-row" role="radiogroup" aria-label="Input mode">
          <span class="input-mode-label">Input mode</span>
          <button
            id="asm-mode"
            class="mode-button active"
            type="button"
            role="radio"
            aria-checked="true"
          >
            Assembly
          </button>
          <button
            id="hex-mode"
            class="mode-button"
            type="button"
            role="radio"
            aria-checked="false"
          >
            Hex
          </button>
        </div>

        <div id="asm-panel" class="input-panel">
          <label for="asm-input">x86-64 assembly (NASM syntax)</label>
          <textarea id="asm-input" rows="8" spellcheck="false">add rax, rbx</textarea>
          <p class="subtitle">Assembled with NASM as 64-bit flat binary before analysis.</p>
        </div>

        <div id="hex-panel" class="input-panel" hidden>
          <label for="hex-input">x86-64 bytes (hex)</label>
          <textarea id="hex-input" rows="6" spellcheck="false">48 01 d8</textarea>
        </div>

        <p id="assembled-preview" class="assembled-preview" hidden></p>
```

- [ ] **Step 2: Add styles**

In `web/style.css`, add near existing form-control styles:

```css
.input-mode-row {
	display: flex;
	flex-wrap: wrap;
	gap: 8px;
	align-items: center;
	margin: 20px 0 10px;
}

.input-mode-label {
	font-weight: 600;
	color: var(--muted);
}

.mode-button {
	width: auto;
	padding: 8px 12px;
	border: 1px solid var(--border);
	border-radius: 999px;
	background: var(--input-bg);
	color: var(--text);
}

.mode-button.active {
	border-color: var(--button-bg);
	background: var(--button-bg);
	color: var(--button-text);
}

.input-panel[hidden],
.assembled-preview[hidden] {
	display: none;
}

.assembled-preview {
	margin: 10px 0 0;
	font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
	font-size: 0.9rem;
	color: var(--muted);
	word-break: break-word;
}
```

- [ ] **Step 3: Validate HTML/CSS smoke**

Run:

```bash
python3 - <<'PY'
from pathlib import Path
html = Path('web/index.html').read_text()
for text in ['id="asm-mode"', 'id="hex-mode"', 'id="asm-input"', 'id="assembled-preview"']:
    assert text in html, text
css = Path('web/style.css').read_text()
for text in ['.input-mode-row', '.mode-button.active', '.assembled-preview']:
    assert text in css, text
print('ui markup ok')
PY
```

Expected:

```text
ui markup ok
```

- [ ] **Step 4: Commit Task 4**

Run:

```bash
git add web/index.html web/style.css
git commit -m "Add assembly input mode UI"
```

## Task 5: Wire assembly mode into `main.js`

**Files:**
- Modify: `web/main.js`

- [ ] **Step 1: Import assembler wrapper**

At top of `web/main.js`, add after the `uipack-cache.js` import:

```js
import { assembleNasm } from "./nasm-assemble.js";
```

- [ ] **Step 2: Add DOM references and state**

Add after existing `hexInput` declaration:

```js
const asmInput = document.getElementById("asm-input");
const asmMode = document.getElementById("asm-mode");
const hexMode = document.getElementById("hex-mode");
const asmPanel = document.getElementById("asm-panel");
const hexPanel = document.getElementById("hex-panel");
const assembledPreview = document.getElementById("assembled-preview");
```

Add after `let manifest = null;`:

```js
let inputMode = "asm";
```

- [ ] **Step 3: Add input-mode helpers**

Add after `selectTab(name)`:

```js
function setInputMode(mode) {
	inputMode = mode === "hex" ? "hex" : "asm";
	const asmActive = inputMode === "asm";
	asmMode.classList.toggle("active", asmActive);
	hexMode.classList.toggle("active", !asmActive);
	asmMode.setAttribute("aria-checked", String(asmActive));
	hexMode.setAttribute("aria-checked", String(!asmActive));
	asmPanel.hidden = !asmActive;
	hexPanel.hidden = asmActive;
	assembledPreview.hidden = true;
	assembledPreview.textContent = "";
}

function previewHex(hex) {
	if (!hex) {
		assembledPreview.hidden = true;
		assembledPreview.textContent = "";
		return;
	}
	assembledPreview.textContent = `Assembled: ${hex}`;
	assembledPreview.hidden = false;
}

async function getInputHex() {
	if (inputMode === "hex") {
		previewHex("");
		return hexInput.value;
	}
	status.textContent = "Assembling...";
	const assembled = await assembleNasm(asmInput.value);
	previewHex(assembled.hex);
	return assembled.hex;
}
```

- [ ] **Step 4: Use input mode during analysis**

In `runAnalyze()`, replace:

```js
		status.textContent = "Analyzing...";
		const response = callRun(
			{
				hex: hexInput.value,
				arch,
				invocation: { arch },
			},
			packBytes,
		);
```

with:

```js
		const inputHex = await getInputHex();
		status.textContent = "Analyzing...";
		const response = callRun(
			{
				hex: inputHex,
				arch,
				invocation: { arch },
			},
			packBytes,
		);
```

- [ ] **Step 5: Improve failure output for NASM errors**

In the `catch` block of `runAnalyze()`, replace:

```js
		output.textContent = String(error);
```

with:

```js
		output.textContent = error instanceof Error ? error.message : String(error);
```

- [ ] **Step 6: Add event listeners and initialize mode**

Before `themeToggle.addEventListener(...)`, add:

```js
asmMode.addEventListener("click", () => setInputMode("asm"));
hexMode.addEventListener("click", () => setInputMode("hex"));
setInputMode("asm");
```

- [ ] **Step 7: Syntax check JS**

Run:

```bash
node --check web/main.js
node --check web/nasm-assemble.js
node --check web/nasm-worker.js
```

Expected: all commands exit 0.

- [ ] **Step 8: Commit Task 5**

Run:

```bash
git add web/main.js
git commit -m "Wire NASM assembly input into web UI"
```

## Task 6: Documentation and full verification

**Files:**
- Modify: `README.rust.md`

- [ ] **Step 1: Update README web flow summary**

In `README.rust.md`, in the high-level `Emscripten raw-byte input` flow, replace:

```text
Emscripten raw-byte input
  -> index.html fetches shared data/manifest.json + data/arch/*.uipack
```

with:

```text
Emscripten raw-byte/assembly input
  -> index.html optionally assembles NASM syntax to flat bytes in a browser Worker
      -> NASM wasm emits raw x86-64 bytes
  -> index.html fetches shared data/manifest.json + data/arch/*.uipack
```

- [ ] **Step 2: Update Option 3 artifacts**

In `README.rust.md`, in Option 3 expected full web artifacts, add:

```text
dist/nasm/nasm.js
dist/nasm/nasm.wasm
dist/nasm/LICENSE
dist/nasm-assemble.js
dist/nasm-worker.js
```

- [ ] **Step 3: Add NASM UI note**

In `README.rust.md`, near Option 3 serve/open instructions, add:

```markdown
The main page defaults to Assembly input. Assembly is NASM syntax, wrapped as
64-bit flat binary with `BITS 64` and `DEFAULT REL`, then passed to uiCA as raw
bytes. Switch to Hex mode to bypass NASM and enter bytes directly.
```

- [ ] **Step 4: Run full local verification**

Run:

```bash
source ~/emsdk/emsdk_env.sh
node --check web/main.js
node --check web/nasm-assemble.js
node --check web/nasm-worker.js
./scripts/build-nasm-emscripten.sh dist/nasm
./scripts/smoke-nasm-emscripten.sh dist/nasm
./scripts/build-web.sh
./scripts/smoke-emscripten-exports.sh dist/emscripten
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected:

```text
NASM Emscripten smoke passed
Built web bundle in .../dist
Emscripten exports smoke passed
```

`cargo test` and `cargo clippy` must exit 0.

- [ ] **Step 5: Browser smoke test**

Run:

```bash
python3 -m http.server -d dist 8000
```

Open `http://127.0.0.1:8000/` and verify:

1. Assembly mode is selected by default.
2. Source `add rax, rbx` on `SKL` analyzes successfully.
3. Preview says `Assembled: 48 01 d8`.
4. Trace tab renders.
5. JSON tab renders result.
6. Hex mode with `48 01 d8` produces same result.
7. Invalid assembly such as `not_an_instruction` shows NASM diagnostic in JSON panel.

- [ ] **Step 6: Commit Task 6**

Run:

```bash
git add README.rust.md
git commit -m "Document NASM assembly web input"
```

## Task 7: Push and monitor CI/Pages

**Files:**
- No new file edits unless CI fails.

- [ ] **Step 1: Push branch**

Run:

```bash
git push origin master
```

Expected: push succeeds.

- [ ] **Step 2: Monitor Actions**

Run:

```bash
gh run list --limit 10
gh run watch --exit-status
```

Expected: `verification-quick`, `rust-parity`, and `pages` pass.

- [ ] **Step 3: If CI fails, inspect root cause before patching**

Run:

```bash
gh run view --log-failed
```

Apply a fix only after identifying the exact failed command and failing artifact. Re-run the matching local command before pushing the fix.

- [ ] **Step 4: Verify live Pages assets**

Run:

```bash
for path in \
  main.js \
  nasm-assemble.js \
  nasm-worker.js \
  nasm/nasm.js \
  nasm/nasm.wasm \
  nasm/LICENSE \
  emscripten/uica_emscripten.js \
  emscripten/uica_emscripten.wasm \
  data/manifest.json \
  data/arch/SKL.uipack; do
  curl -L --max-time 30 -sS -o /tmp/uica_asset \
    -w "$path %{http_code} %{size_download} %{content_type}\n" \
    "https://uica.houmus.org/$path"
done
```

Expected: every line starts with path and `200`.

- [ ] **Step 5: Final browser smoke**

Open `https://uica.houmus.org/` and repeat the local browser smoke from Task 6 Step 5.
