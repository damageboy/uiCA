#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${UICA_EMSCRIPTEN_XED_OUT_DIR:-$ROOT_DIR/target/xed-emscripten}"
INSTALL_DIR="${UICA_EMSCRIPTEN_XED_DIR:-$OUT_DIR/install}"
BUILD_DIR="$OUT_DIR/build"
TOOLWRAP_DIR="$OUT_DIR/toolwrap"
MBUILD_LOG="$OUT_DIR/xed-mbuild.log"

require_tool() {
	local tool="$1"
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "$tool not found; activate emsdk first" >&2
		exit 127
	fi
}

require_file() {
	local path="$1"
	local hint="$2"
	if [[ ! -f "$path" ]]; then
		echo "missing $path" >&2
		echo "$hint" >&2
		exit 1
	fi
}

for tool in emcc em++ emar emranlib python3; do
	require_tool "$tool"
done

require_file \
	"$ROOT_DIR/XED-to-XML/include/public/xed/xed-interface.h" \
	"run: git submodule update --init XED-to-XML"
require_file \
	"$ROOT_DIR/mbuild/mbuild/env.py" \
	"run: git submodule update --init mbuild"

rm -rf "$INSTALL_DIR" "$BUILD_DIR" "$TOOLWRAP_DIR"
mkdir -p "$INSTALL_DIR" "$BUILD_DIR" "$TOOLWRAP_DIR"

cat >"$TOOLWRAP_DIR/clang" <<'EOF'
#!/usr/bin/env bash
exec emcc "$@"
EOF
cat >"$TOOLWRAP_DIR/clang++" <<'EOF'
#!/usr/bin/env bash
exec em++ "$@"
EOF
cat >"$TOOLWRAP_DIR/ar" <<'EOF'
#!/usr/bin/env bash
exec emar "$@"
EOF
cat >"$TOOLWRAP_DIR/ranlib" <<'EOF'
#!/usr/bin/env bash
exec emranlib "$@"
EOF
chmod +x "$TOOLWRAP_DIR/clang" "$TOOLWRAP_DIR/clang++" "$TOOLWRAP_DIR/ar" "$TOOLWRAP_DIR/ranlib"

cat >"$OUT_DIR/README.txt" <<EOF
This directory contains an Emscripten XED build.

build:   $BUILD_DIR
install: $INSTALL_DIR
tools:   $TOOLWRAP_DIR

Expected artifacts:
  $INSTALL_DIR/include/xed/xed-interface.h
  $INSTALL_DIR/lib/libxed.a
EOF

export PYTHONPATH="$ROOT_DIR/mbuild${PYTHONPATH:+:$PYTHONPATH}"
export PATH="$TOOLWRAP_DIR:$PATH"

if ! (
	cd "$ROOT_DIR/XED-to-XML"
	python3 ./mfile.py \
		--compiler=clang \
		--toolchain="$TOOLWRAP_DIR/" \
		--build-dir="$BUILD_DIR" \
		--install-dir="$INSTALL_DIR" \
		--opt=2 \
		--no-encoder \
		--no-werror \
		--extra-flags="-Wno-unused-command-line-argument" \
		install
) 2>&1 | tee "$MBUILD_LOG"; then
	case "${UICA_XED_EMSCRIPTEN_FALLBACK:-}" in
	"")
		echo "XED mbuild failed for Emscripten." >&2
		echo "Log: $MBUILD_LOG" >&2
		echo "Next steps:" >&2
		echo "  1. Inspect failed compiler/linker commands in the log." >&2
		echo "  2. Capture the concrete XED source list and generated include paths." >&2
		echo "  3. Re-run with UICA_XED_EMSCRIPTEN_FALLBACK=manual only after that evidence exists." >&2
		exit 2
		;;
	manual)
		echo "Manual Emscripten XED fallback requested, but this script will not fake libxed.a." >&2
		echo "Manual fallback needs concrete source-list evidence from failed mbuild log: $MBUILD_LOG" >&2
		echo "Add an evidence-backed compile list before enabling any manual fallback implementation." >&2
		exit 2
		;;
	*)
		echo "Unsupported UICA_XED_EMSCRIPTEN_FALLBACK=${UICA_XED_EMSCRIPTEN_FALLBACK}" >&2
		echo "Supported value: manual" >&2
		exit 2
		;;
	esac
fi

if [[ ! -f "$INSTALL_DIR/include/xed/xed-interface.h" ]]; then
	echo "missing $INSTALL_DIR/include/xed/xed-interface.h after XED build" >&2
	exit 1
fi
if [[ ! -f "$INSTALL_DIR/lib/libxed.a" ]]; then
	echo "missing $INSTALL_DIR/lib/libxed.a after XED build" >&2
	exit 1
fi

printf '%s\n' "$INSTALL_DIR" >"$OUT_DIR/xed-dir.txt"
printf 'UICA_EMSCRIPTEN_XED_DIR=%q\n' "$INSTALL_DIR"
