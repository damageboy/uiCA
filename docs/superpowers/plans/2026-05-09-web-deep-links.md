# Web Deep Links Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add shareable web URLs that prefill input bytes/source and microarchitecture, plus a post-analysis copy button.

**Architecture:** Keep WASM unchanged. Add pure deep-link helpers in a small browser ESM module, test them with Node's built-in test runner, then wire helpers into the existing web UI lifecycle after manifest load and after successful analysis.

**Tech Stack:** Browser JavaScript modules, DOM APIs, URLSearchParams, Clipboard API, Node `node:test` for pure helper tests.

---

## File Structure

- Create: `web/deep-link.mjs`
  - Pure helper functions for base64 UTF-8 encode/decode, query parsing, and deep-link URL generation.
  - No DOM access.
- Create: `tests/web-deep-link.test.mjs`
  - Node tests for helper behavior.
- Modify: `web/index.html:145-148`
  - Add hidden `Copy deep-link` button next to `Analyze`.
- Modify: `web/main.js:1-35`, `web/main.js:63-72`, `web/main.js:224-270`, `web/main.js:273-296`
  - Import helpers, read copy button element, apply URL parameters after manifest load, show/hide copy button, wire Clipboard API.
- Modify: `web/style.css:220-242`
  - Style secondary copy button and ensure `[hidden]` wins over button display.

---

### Task 1: Deep-Link Helper Module

**Files:**

- Create: `web/deep-link.mjs`
- Create: `tests/web-deep-link.test.mjs`

- [ ] **Step 1: Write failing helper tests**

Create `tests/web-deep-link.test.mjs`:

```js
import assert from "node:assert/strict";
import test from "node:test";

import {
  buildDeepLink,
  decodeBase64Utf8,
  encodeBase64Utf8,
  readDeepLinkParams,
} from "../web/deep-link.mjs";

test("base64 helpers round-trip UTF-8 assembly", () => {
  const asm = "add rax, rbx\n; µarch comment";
  assert.equal(decodeBase64Utf8(encodeBase64Utf8(asm)), asm);
});

test("base64 decoder accepts URL-safe base64 without padding", () => {
  assert.equal(decodeBase64Utf8("YWJjZA"), "abcd");
});

test("readDeepLinkParams rejects hex plus asm conflict", () => {
  const params = new URLSearchParams({
    hex: "48 01 d8",
    asm: encodeBase64Utf8("add rax, rbx"),
  });
  assert.throws(
    () => readDeepLinkParams(params),
    /URL can not contain both hex and asm parameters/,
  );
});

test("readDeepLinkParams returns hex selection", () => {
  const params = new URLSearchParams({ hex: "48 01 d8", uarch: "SKL" });
  assert.deepEqual(readDeepLinkParams(params), {
    inputMode: "hex",
    hex: "48 01 d8",
    asm: "",
    uarch: "SKL",
  });
});

test("readDeepLinkParams returns decoded asm selection", () => {
  const asm = "add rax, rbx\ndec r15";
  const params = new URLSearchParams({ asm: encodeBase64Utf8(asm) });
  assert.deepEqual(readDeepLinkParams(params), {
    inputMode: "asm",
    hex: "",
    asm,
    uarch: "",
  });
});

test("readDeepLinkParams reports invalid asm base64", () => {
  const params = new URLSearchParams({ asm: "@@@" });
  assert.throws(
    () => readDeepLinkParams(params),
    /Invalid asm base64 parameter/,
  );
});

test("buildDeepLink writes asm and uarch only", () => {
  const href = buildDeepLink({
    baseUrl: "https://uica.houmus.org/index.html?old=1#top",
    inputMode: "asm",
    asmText: "add rax, rbx",
    hexText: "48 01 d8",
    uarch: "SKL",
  });
  const url = new URL(href);
  assert.equal(url.origin + url.pathname, "https://uica.houmus.org/index.html");
  assert.equal(url.hash, "#top");
  assert.equal(url.searchParams.get("uarch"), "SKL");
  assert.equal(url.searchParams.get("hex"), null);
  assert.equal(decodeBase64Utf8(url.searchParams.get("asm")), "add rax, rbx");
});

test("buildDeepLink writes hex and uarch only", () => {
  const href = buildDeepLink({
    baseUrl: "https://uica.houmus.org/",
    inputMode: "hex",
    asmText: "add rax, rbx",
    hexText: "48 01 d8",
    uarch: "HSW",
  });
  const url = new URL(href);
  assert.equal(url.searchParams.get("uarch"), "HSW");
  assert.equal(url.searchParams.get("hex"), "48 01 d8");
  assert.equal(url.searchParams.get("asm"), null);
});
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
node --test tests/web-deep-link.test.mjs
```

Expected: FAIL with module-not-found error for `web/deep-link.mjs`.

- [ ] **Step 3: Implement helper module**

Create `web/deep-link.mjs`:

```js
function normalizeBase64(value) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const paddingLength = (4 - (normalized.length % 4)) % 4;
  return `${normalized}${"=".repeat(paddingLength)}`;
}

export function encodeBase64Utf8(text) {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

export function decodeBase64Utf8(value) {
  try {
    const binary = atob(normalizeBase64(value));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch (error) {
    throw new Error("Invalid asm base64 parameter", { cause: error });
  }
}

export function readDeepLinkParams(searchParams) {
  const hex = searchParams.get("hex") ?? "";
  const encodedAsm = searchParams.get("asm") ?? "";
  const uarch = searchParams.get("uarch") ?? "";

  if (hex && encodedAsm) {
    throw new Error("URL can not contain both hex and asm parameters");
  }

  if (hex) {
    return { inputMode: "hex", hex, asm: "", uarch };
  }

  if (encodedAsm) {
    return {
      inputMode: "asm",
      hex: "",
      asm: decodeBase64Utf8(encodedAsm),
      uarch,
    };
  }

  return { inputMode: "", hex: "", asm: "", uarch };
}

export function buildDeepLink({ baseUrl, inputMode, asmText, hexText, uarch }) {
  const url = new URL(baseUrl);
  url.search = "";
  url.searchParams.set("uarch", uarch);
  if (inputMode === "hex") {
    url.searchParams.set("hex", hexText);
  } else {
    url.searchParams.set("asm", encodeBase64Utf8(asmText));
  }
  return url.toString();
}
```

- [ ] **Step 4: Run helper tests to verify pass**

Run:

```bash
node --test tests/web-deep-link.test.mjs
```

Expected: PASS, 8 tests.

- [ ] **Step 5: Commit helper module**

```bash
git add web/deep-link.mjs tests/web-deep-link.test.mjs
git commit -m "test: add web deep-link helpers"
```

---

### Task 2: Copy Button Markup and Styling

**Files:**

- Modify: `web/index.html:145-148`
- Modify: `web/style.css:220-242`

- [ ] **Step 1: Add hidden button markup**

In `web/index.html`, replace the actions block:

```html
<div class="actions">
  <button id="analyze-button" type="button" disabled>Analyze</button>
  <span id="status">Loading wasm...</span>
</div>
```

with:

```html
<div class="actions">
  <button id="analyze-button" type="button" disabled>Analyze</button>
  <button
    id="copy-deep-link-button"
    class="secondary-button"
    type="button"
    hidden
  >
    Copy deep-link
  </button>
  <span id="status">Loading wasm...</span>
</div>
```

- [ ] **Step 2: Add secondary button CSS**

In `web/style.css`, after the existing `button` rule ending at line 225, insert:

```css
button[hidden] {
  display: none;
}

.secondary-button {
  border: 1px solid var(--border);
  background: var(--input-bg);
  color: var(--text);
}

.secondary-button:disabled {
  cursor: wait;
  opacity: 0.7;
}
```

- [ ] **Step 3: Smoke-check markup**

Run:

```bash
python3 -m http.server 8000 --directory web
```

Open `http://127.0.0.1:8000/`.

Expected:

- Page loads.
- `Copy deep-link` button is not visible before JavaScript wiring.
- `Analyze` button remains visible.

Stop server with `Ctrl-C`.

- [ ] **Step 4: Commit markup and CSS**

```bash
git add web/index.html web/style.css
git commit -m "feat: add copy deep-link button"
```

---

### Task 3: Wire URL Prefill and Clipboard Copy

**Files:**

- Modify: `web/main.js:1-35`
- Modify: `web/main.js:63-72`
- Modify: `web/main.js:224-270`
- Modify: `web/main.js:273-296`

- [ ] **Step 1: Import helpers and read copy button**

At top of `web/main.js`, after the NASM import, add:

```js
import { buildDeepLink, readDeepLinkParams } from "./deep-link.mjs";
```

After:

```js
const button = document.getElementById("analyze-button");
```

add:

```js
const copyDeepLinkButton = document.getElementById("copy-deep-link-button");
```

- [ ] **Step 2: Add copy button state helpers**

After `applyTheme(localStorage.getItem(THEME_STORAGE_KEY) ?? "system");`, add:

```js
function resetCopyDeepLinkButton() {
  copyDeepLinkButton.hidden = true;
  copyDeepLinkButton.disabled = false;
  copyDeepLinkButton.textContent = "Copy deep-link";
}

function showCopyDeepLinkButton() {
  copyDeepLinkButton.hidden = false;
  copyDeepLinkButton.disabled = false;
  copyDeepLinkButton.textContent = "Copy deep-link";
}
```

- [ ] **Step 3: Add URL selection applier**

After `showCopyDeepLinkButton()`, add:

```js
function applyDeepLinkSelection() {
  const selection = readDeepLinkParams(
    new URLSearchParams(window.location.search),
  );
  let warning = "";

  if (selection.inputMode === "hex") {
    hexInput.value = selection.hex;
    setInputMode("hex");
  } else if (selection.inputMode === "asm") {
    asmInput.value = selection.asm;
    setInputMode("asm");
  }

  if (selection.uarch) {
    if (manifest.architectures[selection.uarch]) {
      archSelect.value = selection.uarch;
    } else {
      warning = `Unknown uarch ${selection.uarch}; using ${archSelect.value}.`;
    }
  }

  return warning;
}
```

- [ ] **Step 4: Apply URL params during boot**

In `boot()`, replace:

```js
populateArchSelect(archSelect, manifest, "SKL");
archSelect.disabled = false;
button.disabled = false;
status.textContent = "Wasm ready";
```

with:

```js
populateArchSelect(archSelect, manifest, "SKL");
archSelect.disabled = false;
button.disabled = false;
try {
  const warning = applyDeepLinkSelection();
  status.textContent = warning || "Wasm ready";
} catch (error) {
  status.textContent = `Deep-link ignored: ${
    error instanceof Error ? error.message : String(error)
  }`;
}
```

- [ ] **Step 5: Hide copy button at analysis start and show after success**

In `runAnalyze()`, after disabling input buttons:

```js
button.disabled = true;
asmMode.disabled = true;
hexMode.disabled = true;
```

add:

```js
resetCopyDeepLinkButton();
```

After:

```js
status.textContent = `Analysis complete: ${tp} cycles/iteration`;
```

add:

```js
showCopyDeepLinkButton();
```

- [ ] **Step 6: Add copy action**

After `runAnalyze()` and before existing event listeners, add:

```js
async function copyDeepLink() {
  const href = buildDeepLink({
    baseUrl: window.location.href,
    inputMode,
    asmText: asmInput.value,
    hexText: hexInput.value,
    uarch: archSelect.value,
  });

  copyDeepLinkButton.disabled = true;
  try {
    await navigator.clipboard.writeText(href);
    copyDeepLinkButton.textContent = "Copied!";
    status.textContent = "Deep-link copied.";
  } catch (error) {
    status.textContent = `Copy failed; deep-link: ${href}`;
  } finally {
    copyDeepLinkButton.disabled = false;
  }
}
```

Add event listener after the existing Analyze listener:

```js
copyDeepLinkButton.addEventListener("click", () => {
  void copyDeepLink();
});
```

- [ ] **Step 7: Run helper tests**

Run:

```bash
node --test tests/web-deep-link.test.mjs
```

Expected: PASS, 8 tests.

- [ ] **Step 8: Browser smoke check URL loading**

Run:

```bash
python3 -m http.server 8000 --directory web
```

Check these URLs:

```text
http://127.0.0.1:8000/?uarch=SKL&hex=48%2001%20d8
http://127.0.0.1:8000/?uarch=SKL&asm=YWRkIHJheCwgcmJ4
http://127.0.0.1:8000/?hex=48%2001%20d8&asm=YWRkIHJheCwgcmJ4
http://127.0.0.1:8000/?uarch=NOPE&hex=48%2001%20d8
http://127.0.0.1:8000/?asm=@@@
```

Expected:

- First URL selects Hex mode and fills `48 01 d8`.
- Second URL selects Assembly mode and fills `add rax, rbx`.
- Third URL reports `Deep-link ignored: URL can not contain both hex and asm parameters` and keeps defaults.
- Fourth URL keeps default arch and reports `Unknown uarch NOPE; using SKL.`
- Fifth URL reports `Deep-link ignored: Invalid asm base64 parameter` and keeps defaults.

Stop server with `Ctrl-C`.

- [ ] **Step 9: Commit JS wiring**

```bash
git add web/main.js
git commit -m "feat: wire web deep links"
```

---

### Task 4: End-to-End Copy Verification

**Files:**

- Modify if needed: `web/main.js`
- Modify if needed: `web/deep-link.mjs`
- Modify if needed: `web/index.html`
- Modify if needed: `web/style.css`

- [ ] **Step 1: Start local server**

Run:

```bash
python3 -m http.server 8000 --directory web
```

Expected: server listens on `http://0.0.0.0:8000/`.

- [ ] **Step 2: Verify Assembly copy flow**

Open:

```text
http://127.0.0.1:8000/?uarch=SKL&asm=YWRkIHJheCwgcmJ4
```

Click `Analyze`.

Expected:

- Status ends with `Analysis complete: ... cycles/iteration`.
- `Copy deep-link` appears.
- Clicking `Copy deep-link` changes button text to `Copied!`.
- Pasting copied URL into a new tab restores Assembly mode, `add rax, rbx`, and `SKL`.

- [ ] **Step 3: Verify Hex copy flow**

Open:

```text
http://127.0.0.1:8000/?uarch=SKL&hex=48%2001%20d8
```

Click `Analyze`, then `Copy deep-link`.

Expected:

- Status ends with `Analysis complete: ... cycles/iteration`.
- Pasted copied URL restores Hex mode, `48 01 d8`, and `SKL`.
- Copied URL contains `hex=` and does not contain `asm=`.

- [ ] **Step 4: Verify failure hides copy button**

Enter invalid hex in Hex mode:

```text
zz
```

Click `Analyze`.

Expected:

- Status is `Analysis failed`.
- `Copy deep-link` is hidden.

- [ ] **Step 5: Stop server**

Stop local server with `Ctrl-C`.

- [ ] **Step 6: Run final helper tests**

Run:

```bash
node --test tests/web-deep-link.test.mjs
```

Expected: PASS, 8 tests.

- [ ] **Step 7: Commit verification fixes if any**

If Step 2-4 required code changes, run:

```bash
git add web/main.js web/deep-link.mjs web/index.html web/style.css tests/web-deep-link.test.mjs
git commit -m "fix: polish web deep-link behavior"
```

If no code changes were needed, do not create an empty commit.

---

## Plan Self-Review

- Spec coverage: incoming `hex`, `asm`, `uarch`; conflict rejection; copy button after success; clipboard fallback; no WASM changes all covered.
- Placeholder scan: no `TBD`, `TODO`, or deferred implementation remains.
- Type consistency: helper function names match imports and tests: `buildDeepLink`, `decodeBase64Utf8`, `encodeBase64Utf8`, `readDeepLinkParams`.
