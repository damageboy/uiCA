#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
EMSCRIPTEN_DIR="$DIST_DIR/emscripten"

if ! command -v wasm-pack >/dev/null 2>&1; then
	echo "wasm-pack not found. Install wasm-pack, then re-run." >&2
	exit 127
fi

mkdir -p "$DIST_DIR" "$EMSCRIPTEN_DIR"
wasm-pack build "$ROOT_DIR/rust/uica-wasm" --target web --out-dir ../../dist/pkg
# build-uica-emscripten.sh invokes build-xed-emscripten.sh when needed.
"$ROOT_DIR/scripts/build-uica-emscripten.sh" "$EMSCRIPTEN_DIR"
cp "$ROOT_DIR/web/index.html" "$DIST_DIR/index.html"
cp "$ROOT_DIR/web/main.js" "$DIST_DIR/main.js"
cp "$ROOT_DIR/web/uipack-cache.js" "$DIST_DIR/uipack-cache.js"
cp "$ROOT_DIR/web/style.css" "$DIST_DIR/style.css"
cp "$ROOT_DIR/web/test-pure-wasm.html" "$DIST_DIR/test-pure-wasm.html"
cp "$ROOT_DIR/web/pure-wasm.js" "$DIST_DIR/pure-wasm.js"
cp "$ROOT_DIR/web/CNAME" "$DIST_DIR/CNAME"
mkdir -p "$DIST_DIR/data/arch"
cp "$ROOT_DIR/rust/uica-data/generated/manifest.json" "$DIST_DIR/data/manifest.json"
cp "$ROOT_DIR/rust/uica-data/generated/arch/"*.uipack "$DIST_DIR/data/arch/"

test -f "$DIST_DIR/pkg/uica_wasm.js"
test -f "$DIST_DIR/pkg/uica_wasm_bg.wasm"
test -f "$DIST_DIR/emscripten/uica_emscripten.js"
test -f "$DIST_DIR/emscripten/uica_emscripten.wasm"
test -f "$DIST_DIR/uipack-cache.js"
test -f "$DIST_DIR/data/manifest.json"
test -f "$DIST_DIR/data/arch/SKL.uipack"

echo "Built web bundle in $DIST_DIR"
