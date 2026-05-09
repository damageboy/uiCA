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
CACHE_DIR="$BUILD_ROOT/install"
CACHE_KEY_FILE="$CACHE_DIR/cache-key.txt"

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

nasm_cache_key() {
	{
		printf '%s\n' "$VERSION" "$SHA256"
		hash_file "$ROOT_DIR/scripts/build-nasm-emscripten.sh"
		emcc --version | head -n 1
	} | hash_stdin
}

copy_cached_nasm() {
	mkdir -p "$OUT_DIR"
	cp "$CACHE_DIR/nasm.js" "$OUT_DIR/nasm.js"
	cp "$CACHE_DIR/nasm.wasm" "$OUT_DIR/nasm.wasm"
	cp "$CACHE_DIR/LICENSE" "$OUT_DIR/LICENSE"
}

require_tool() {
	local tool="$1"
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "$tool not found; activate emsdk first" >&2
		exit 127
	fi
}

for tool in emcc emconfigure emmake emar emranlib curl tar make; do
	require_tool "$tool"
done

sha256_check() {
	local expected="$1"
	local path="$2"
	if command -v sha256sum >/dev/null 2>&1; then
		printf '%s  %s\n' "$expected" "$path" | sha256sum -c -
	elif command -v shasum >/dev/null 2>&1; then
		printf '%s  %s\n' "$expected" "$path" | shasum -a 256 -c -
	else
		echo "sha256sum or shasum not found" >&2
		exit 127
	fi
}

job_count() {
	getconf _NPROCESSORS_ONLN 2>/dev/null || \
		sysctl -n hw.ncpu 2>/dev/null || \
		echo 1
}

mkdir -p "$BUILD_ROOT" "$OUT_DIR"
EXPECTED_CACHE_KEY="$(nasm_cache_key)"

if [[ -f "$CACHE_DIR/nasm.js" && -f "$CACHE_DIR/nasm.wasm" && -f "$CACHE_DIR/LICENSE" && -f "$CACHE_KEY_FILE" && "$(cat "$CACHE_KEY_FILE")" == "$EXPECTED_CACHE_KEY" ]]; then
	copy_cached_nasm
	printf 'Reusing cached NASM Emscripten bundle in %s\n' "$CACHE_DIR"
	exit 0
fi

if [[ ! -f "$ARCHIVE" ]]; then
	curl -fsSL "$URL" -o "$ARCHIVE"
fi
if ! sha256_check "$SHA256" "$ARCHIVE"; then
	echo "cached NASM archive failed checksum; redownloading" >&2
	rm -f "$ARCHIVE"
	curl -fsSL "$URL" -o "$ARCHIVE"
	sha256_check "$SHA256" "$ARCHIVE"
fi

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
			-sENVIRONMENT=web,worker,node \
			-sINVOKE_RUN=0 \
			-sEXIT_RUNTIME=1 \
			-sALLOW_MEMORY_GROWTH=1 \
			-sFORCE_FILESYSTEM=1 \
			-sEXPORTED_RUNTIME_METHODS=FS,callMain"
	emmake make -j"$(job_count)" nasm
)

rm -rf "$CACHE_DIR"
mkdir -p "$CACHE_DIR"
cp "$SRC_DIR/nasm" "$CACHE_DIR/nasm.js"
cp "$SRC_DIR/nasm.wasm" "$CACHE_DIR/nasm.wasm"
cp "$SRC_DIR/LICENSE" "$CACHE_DIR/LICENSE"
printf '%s\n' "$EXPECTED_CACHE_KEY" >"$CACHE_KEY_FILE"
copy_cached_nasm

test -f "$OUT_DIR/nasm.js"
test -f "$OUT_DIR/nasm.wasm"
test -f "$OUT_DIR/LICENSE"
printf 'Built NASM Emscripten bundle in %s\n' "$OUT_DIR"
