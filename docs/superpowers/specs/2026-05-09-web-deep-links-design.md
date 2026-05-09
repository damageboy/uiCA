# Web Deep Links Design

## Goal

Allow shared links to prefill the web UI with a complete analysis selection, automatically run the analysis for valid full selections, and expose a post-analysis button that copies the current selection as a shareable URL.

## Scope

This change affects only JavaScript, HTML, and CSS in the web UI. The uiCA WASM boundary remains unchanged because analysis already receives `hex` bytes and `arch`, and assembly input is assembled to hex in the browser before calling WASM.

Modified files:

- `web/index.html` adds the hidden copy button.
- `web/main.js` parses incoming query parameters, builds deep links, and wires clipboard behavior.
- `web/style.css` adds minimal styling for the copy button if needed.

No Rust, Emscripten, or WASM changes are needed.

## URL Parameters

Supported query parameters:

- `hex`: raw x86-64 bytes as text, URL encoded by normal query-string rules.
- `asm`: x86-64 assembly source encoded as UTF-8 then base64.
- `uarch`: microarchitecture name matching the manifest, such as `SKL`.

Rules:

- `hex` and `asm` are mutually exclusive.
- If both `hex` and `asm` appear, the web UI rejects the conflict, shows a status message, and keeps default input values.
- If only `hex` appears, the UI selects Hex mode and fills the hex textarea.
- If only `asm` appears, the UI decodes base64 as UTF-8, selects Assembly mode, and fills the assembly textarea.
- If `asm` is invalid base64 or invalid UTF-8, the UI shows a status message and keeps default input values.
- If `uarch` exists and matches the manifest, the UI selects it after manifest load.
- If `uarch` is unknown, the UI keeps the default `SKL` selection and shows a status warning.
- If the URL contains a present, non-empty `hex` or `asm` parameter and a known `uarch`, the UI automatically starts analysis after WASM and the manifest are loaded.
- If the URL lacks an input parameter, lacks `uarch`, has an empty input value, or has unknown `uarch`, the UI pre-fills what it can but does not auto-run.

## Deep-Link Generation

After a successful analysis, the UI reveals a `Copy deep-link` button near the existing Analyze action.

When clicked, the button builds a URL from the current page origin and path. It replaces query parameters with the current selection:

- Always writes `uarch=<current arch>`.
- If Assembly mode is active, writes `asm=<base64 utf8 assembly>`.
- If Hex mode is active, writes `hex=<current hex text>`.

The generated URL does not include analysis output. It is intentionally a prefill link, not a serialized report.

## Clipboard Behavior

The copy button uses `navigator.clipboard.writeText()` when available. On success it briefly updates its text to `Copied!` and leaves the button visible. If the Clipboard API is unavailable or rejects, the UI writes the generated URL into the status text so the user can copy it manually.

The copy button is hidden on page load and at the start of each analysis attempt. It becomes visible only after a successful analysis.

## Error Handling

Deep-link parsing should never prevent WASM or UIPack boot from continuing. Invalid URL parameters are reported through the existing status text while the page remains usable.

Auto-analysis failures use the same failure path as manual analysis. Analysis failures hide or reset the copy button so failed selections are not presented as successfully analyzed links.

## Testing

Manual browser checks cover:

1. `?uarch=SKL&hex=48%2001%20d8` selects Hex mode, fills hex input, selects SKL, and auto-runs analysis.
2. `?uarch=SKL&asm=<base64>` selects Assembly mode, fills assembly input, selects SKL, and auto-runs analysis.
3. `?hex=48%2001%20d8&asm=<base64>` rejects conflicting input parameters.
4. `?uarch=NOPE&hex=48%2001%20d8` keeps default architecture, reports unknown `uarch`, and does not auto-run.
5. Bad `asm` base64 reports decode failure and keeps defaults.
6. `?uarch=SKL` alone selects the architecture but does not auto-run.
7. Successful analysis reveals `Copy deep-link`.
8. Copied link reloads into the same input mode, text, and architecture, then auto-runs.

## Self-Review

- No placeholders remain.
- Scope is limited to web UI files.
- WASM boundary remains unchanged.
- Conflict behavior matches user choice: reject `hex` plus `asm`.
