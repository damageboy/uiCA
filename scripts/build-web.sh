#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack not found. Install wasm-pack, then re-run." >&2
  exit 127
fi

mkdir -p "$DIST_DIR"
wasm-pack build "$ROOT_DIR/rust/uica-wasm" --target web --out-dir ../../dist/pkg
cp "$ROOT_DIR/web/index.html" "$DIST_DIR/index.html"
cp "$ROOT_DIR/web/main.js" "$DIST_DIR/main.js"
cp "$ROOT_DIR/web/style.css" "$DIST_DIR/style.css"

echo "Built web bundle in $DIST_DIR"
