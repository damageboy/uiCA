import json
from pathlib import Path

from verification.tools.canonicalize import canonicalize_result


def write_golden(result: dict, out_path: str) -> None:
    out = Path(out_path)
    out.parent.mkdir(parents=True, exist_ok=True)

    with out.open("w") as f:
        json.dump(
            canonicalize_result(result),
            f,
            sort_keys=True,
            separators=(",", ":"),
        )
