from copy import deepcopy
from typing import Any, cast


def _sort_key(d):
    return (
        d.get("rnd", -1),
        d.get("instrID", -1),
        d.get("lamUopID", -1),
        d.get("fUopID", -1),
        d.get("uopID", -1),
        d.get("source", ""),
        int(bool(d.get("regMergeUop", False))),
        int(bool(d.get("stackSyncUop", False))),
    )


def _canon(value):
    if isinstance(value, dict):
        return {k: _canon(v) for k, v in sorted(value.items(), key=lambda kv: kv[0])}

    if isinstance(value, list):
        out = [_canon(v) for v in value]
        if out and all(isinstance(v, dict) for v in out):
            return sorted(out, key=_sort_key)
        return out

    # Normalise numeric types so JSON ints and floats with the same value
    # compare equal: 1 == 1.0. Serde emits `1.0` for any Rust f64, while
    # Python's json writes `1` for int values.
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return float(value)

    return value


# Fields that identify the emitter but do not describe simulation output.
# These are stripped before comparison so rust-vs-python parity checks do
# not trip on metadata.
_META_FIELDS = {
    "engine",
    "engine_version",
    "uica_commit",
    "schema_version",
}

# Fields on instruction metadata that are human-facing documentation links
# rather than simulation output. Python derives the URL from uops.info iform
# strings that live in the original XML data, which the Rust port does not
# plumb through today. Ignore the URL during parity comparison.
_INSTRUCTION_META_FIELDS = {
    "url",
    # `asm` is pretty-printed by the disassembler and differs between
    # iced-x86 (Rust) and Python's formatter (e.g. `jne short 0` vs
    # `jnz 0xfffffffffffffff7`). Not a simulation output, so strip before
    # parity comparison. `opcode` bytes capture the canonical identity.
    "asm",
}


def _strip_instruction_meta(result: dict) -> dict:
    if not isinstance(result.get("instructions"), list):
        return result
    cleaned = []
    for instr in result["instructions"]:
        if isinstance(instr, dict):
            cleaned.append(
                {k: v for k, v in instr.items() if k not in _INSTRUCTION_META_FIELDS}
            )
        else:
            cleaned.append(instr)
    out = dict(result)
    out["instructions"] = cleaned
    return out


def canonicalize_result(result: dict) -> dict:
    stripped = {k: v for k, v in result.items() if k not in _META_FIELDS}
    stripped = _strip_instruction_meta(stripped)
    return cast(dict[Any, Any], _canon(deepcopy(stripped)))
