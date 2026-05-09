#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${1:-$ROOT_DIR/dist/emscripten}"
JS="$DIST_DIR/uica_emscripten.js"
WASM="$DIST_DIR/uica_emscripten.wasm"

for file in "$JS" "$WASM"; do
	if [[ ! -f "$file" ]]; then
		echo "missing $file" >&2
		exit 1
	fi
done

for symbol in uica_run uica_free_string _malloc _free UTF8ToString stringToUTF8 lengthBytesUTF8; do
	if ! grep -q "$symbol" "$JS"; then
		echo "missing $symbol in $JS" >&2
		exit 1
	fi
done

if command -v wasm-objdump >/dev/null 2>&1; then
	wasm-objdump -x "$WASM" | grep -q "uica_run" || {
		echo "missing uica_run export in $WASM" >&2
		exit 1
	}
fi

echo "Emscripten exports smoke passed"
