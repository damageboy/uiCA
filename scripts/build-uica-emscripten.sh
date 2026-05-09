#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/dist/emscripten}"
XED_OUT_DIR="${UICA_EMSCRIPTEN_XED_OUT_DIR:-$ROOT_DIR/target/xed-emscripten}"
XED_INSTALL_DIR="${UICA_EMSCRIPTEN_XED_DIR:-$XED_OUT_DIR/install}"
TARGET_DIR="$ROOT_DIR/target/wasm32-unknown-emscripten/release"
XED_CACHE_KEY_FILE="$XED_OUT_DIR/cache-key.txt"

hash_stdin() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum | awk '{print $1}'
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 | awk '{print $1}'
	else
		echo "sha256sum or shasum not found" >&2
		exit 127
	fi
}

hash_file() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | awk '{print $1}'
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | awk '{print $1}'
	else
		echo "sha256sum or shasum not found" >&2
		exit 127
	fi
}

xed_cache_key() {
	{
		hash_file "$ROOT_DIR/scripts/build-xed-emscripten.sh"
		emcc --version | head -n 1
		git -C "$ROOT_DIR/XED-to-XML" rev-parse HEAD 2>/dev/null || true
		git -C "$ROOT_DIR/mbuild" rev-parse HEAD 2>/dev/null || true
	} | hash_stdin
}

xed_cache_valid() {
	[[ -n "${UICA_EMSCRIPTEN_XED_DIR:-}" ]] && return 0
	[[ -f "$XED_CACHE_KEY_FILE" ]] || return 1
	[[ "$(cat "$XED_CACHE_KEY_FILE")" == "$(xed_cache_key)" ]]
}

require_tool() {
	local tool="$1"
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "$tool not found; activate emsdk first" >&2
		exit 127
	fi
}

require_tool emcc
require_tool em++
require_tool emar
require_tool rustup
require_tool cargo

if ! rustup target list --installed | grep -qx 'wasm32-unknown-emscripten'; then
	echo "Rust target wasm32-unknown-emscripten is not installed" >&2
	echo "run: rustup target add wasm32-unknown-emscripten" >&2
	exit 1
fi

if [[ ! -f "$XED_INSTALL_DIR/include/xed/xed-interface.h" || ! -f "$XED_INSTALL_DIR/lib/libxed.a" ]] || ! xed_cache_valid; then
	"$ROOT_DIR/scripts/build-xed-emscripten.sh"
	XED_INSTALL_DIR="$(cat "$XED_OUT_DIR/xed-dir.txt")"
else
	printf 'Reusing cached Emscripten XED build at %s\n' "$XED_INSTALL_DIR"
fi

export UICA_EMSCRIPTEN_XED_DIR="$XED_INSTALL_DIR"
export CC_wasm32_unknown_emscripten=emcc
export CXX_wasm32_unknown_emscripten=em++
export AR_wasm32_unknown_emscripten=emar
export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER=emcc

EMSCRIPTS_FLAGS="-C link-arg=-sMODULARIZE=1 \
-C link-arg=-sEXPORT_ES6=1 \
-C link-arg=-sENVIRONMENT=web \
-C link-arg=-sALLOW_MEMORY_GROWTH=1 \
-C link-arg=-sEXPORTED_FUNCTIONS=_uica_run,_uica_free_string,_malloc,_free \
-C link-arg=-sEXPORTED_RUNTIME_METHODS=UTF8ToString,stringToUTF8,lengthBytesUTF8"
export RUSTFLAGS="${RUSTFLAGS:-} $EMSCRIPTS_FLAGS"

cargo build \
	-p uica-emscripten \
	--bin uica_emscripten \
	--target wasm32-unknown-emscripten \
	--release

mkdir -p "$OUT_DIR"
if [[ ! -f "$TARGET_DIR/uica_emscripten.js" || ! -f "$TARGET_DIR/uica_emscripten.wasm" ]]; then
	echo "missing expected Emscripten artifacts in $TARGET_DIR" >&2
	find "$TARGET_DIR" -maxdepth 1 -type f -printf '%f\n' >&2 || true
	exit 1
fi

cp "$TARGET_DIR/uica_emscripten.js" "$OUT_DIR/uica_emscripten.js"
cp "$TARGET_DIR/uica_emscripten.wasm" "$OUT_DIR/uica_emscripten.wasm"
printf 'Built Emscripten uiCA bundle in %s\n' "$OUT_DIR"
