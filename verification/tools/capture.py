import argparse
import json
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

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


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Capture uiCA verification goldens.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--profile",
        help="profile name from verification/profiles without .toml suffix",
    )
    parser.add_argument(
        "--case",
        help="single case id such as curated/add_loop_001",
    )
    parser.add_argument(
        "--arch",
        action="append",
        dest="arches",
        metavar="ARCH",
        help="limit capture to one or more architectures; repeat option for multiple arches",
    )
    parser.add_argument(
        "--engine",
        choices=("python", "rust"),
        default="python",
        help="engine to capture from",
    )
    parser.add_argument(
        "--rust-bin",
        help="path to Rust binary when --engine rust is selected",
    )
    parser.add_argument(
        "--golden-root",
        default="verification/golden",
        help="golden output root directory",
    )
    return parser


def main(argv=None) -> int:
    parser = build_parser()
    parser.parse_args(argv)
    print(
        "capture CLI scaffold placeholder: full CLI execution not implemented yet",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
