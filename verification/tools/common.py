import tomllib
from pathlib import Path


def load_case_manifest(path: str) -> dict:
    with open(path, "rb") as f:
        data = tomllib.load(f)

    if "id" not in data or "run" not in data:
        raise ValueError("case manifest missing id/run")

    return data


def iter_case_dirs(root: Path):
    for path in sorted(root.glob("*/*")):
        if path.is_dir() and (path / "case.toml").exists():
            yield path
