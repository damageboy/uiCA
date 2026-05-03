#!/usr/bin/env bash
set -euo pipefail

PROFILE=${1:-quick}
JOBS=${JOBS:-8}
RUST_BIN=${RUST_BIN:-target/debug/uica-cli}
TMP_GOLDEN_DIR=${TMP_GOLDEN_DIR:-$(mktemp -d)}
TAG=${TAG:-py-${PROFILE}-local}

cargo build -p uica-cli -q
python3 verification/tools/capture.py --profile "$PROFILE" --engine python --golden-root "$TMP_GOLDEN_DIR" --golden-tag "$TAG" --jobs "$JOBS"
mkdir -p "$TMP_GOLDEN_DIR/rust"
rm -rf "$TMP_GOLDEN_DIR/rust/$TAG"
cp -R "$TMP_GOLDEN_DIR/python/$TAG" "$TMP_GOLDEN_DIR/rust/$TAG"
python3 verification/tools/verify.py --profile "$PROFILE" --engine rust --rust-bin "$RUST_BIN" --golden-root "$TMP_GOLDEN_DIR" --golden-tag "$TAG" --jobs "$JOBS" --dump-diff "$TMP_GOLDEN_DIR/${PROFILE}.diff"
echo "TMP_GOLDEN_DIR=$TMP_GOLDEN_DIR"
echo "DIFF=$TMP_GOLDEN_DIR/${PROFILE}.diff"
