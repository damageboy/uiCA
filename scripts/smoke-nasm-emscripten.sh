#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${1:-$ROOT_DIR/dist/nasm}"
JS="$DIST_DIR/nasm.js"
WASM="$DIST_DIR/nasm.wasm"
LICENSE="$DIST_DIR/LICENSE"

if ! command -v node >/dev/null 2>&1; then
	echo "node not found" >&2
	exit 127
fi

for file in "$JS" "$WASM" "$LICENSE"; do
	if [[ ! -f "$file" ]]; then
		echo "missing $file" >&2
		exit 1
	fi
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

cp "$JS" "$TMP_DIR/nasm.js"
cp "$WASM" "$TMP_DIR/nasm.wasm"

cat >"$TMP_DIR/package.json" <<'JSON'
{
  "type": "module"
}
JSON

cat >"$TMP_DIR/smoke.mjs" <<'JS'
import { readFileSync } from "node:fs";

const createNasm = (await import("./nasm.js")).default;
const stderr = [];
const stdout = [];
const module = await createNasm({
	wasmBinary: readFileSync(new URL("./nasm.wasm", import.meta.url)),
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

node "$TMP_DIR/smoke.mjs"
echo "NASM Emscripten smoke passed"
