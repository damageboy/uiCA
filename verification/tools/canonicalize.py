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

    return value


def canonicalize_result(result: dict) -> dict:
    return cast(dict[Any, Any], _canon(deepcopy(result)))
