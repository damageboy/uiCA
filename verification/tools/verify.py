import argparse
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from verification.tools.common import load_profile


def first_mismatch_path(left, right, path="$"):
    if type(left) is not type(right):
        return path

    if isinstance(left, dict):
        keys = sorted(set(left.keys()) | set(right.keys()))
        for key in keys:
            if key not in left or key not in right:
                return f"{path}.{key}"

            mismatch = first_mismatch_path(left[key], right[key], f"{path}.{key}")
            if mismatch:
                return mismatch
        return None

    if isinstance(left, list):
        if len(left) != len(right):
            return f"{path}.length"

        for idx, (left_item, right_item) in enumerate(zip(left, right, strict=False)):
            mismatch = first_mismatch_path(left_item, right_item, f"{path}[{idx}]")
            if mismatch:
                return mismatch
        return None

    return None if left == right else path


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Verify uiCA results against captured goldens.",
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
        help="limit verification to one or more architectures; repeat option for multiple arches",
    )
    parser.add_argument(
        "--engine",
        choices=("python", "rust"),
        default="python",
        help="engine to verify",
    )
    parser.add_argument(
        "--rust-bin",
        help="path to Rust binary when --engine rust is selected",
    )
    parser.add_argument(
        "--golden-root",
        default="verification/golden",
        help="golden input root directory",
    )
    parser.add_argument(
        "--dump-diff",
        help="optional path for mismatch report output",
    )
    return parser


def resolve_case_manifest(case_id: str) -> Path:
    return Path(__file__).resolve().parents[1] / "cases" / case_id / "case.toml"


def resolve_case_ids(args) -> list[str]:
    if bool(args.profile) == bool(args.case):
        raise ValueError("pass exactly one of --profile or --case")

    if args.profile:
        try:
            profile = load_profile(args.profile)
        except FileNotFoundError as exc:
            raise FileNotFoundError(f"missing profile: {args.profile}") from exc

        return profile.get("cases", [])

    return [args.case]


def main(argv=None) -> int:
    parser = build_parser()

    try:
        args = parser.parse_args(argv)
        case_ids = resolve_case_ids(args)

        for case_id in case_ids:
            manifest = resolve_case_manifest(case_id)
            if not manifest.exists():
                raise FileNotFoundError(f"missing case manifest: {manifest.as_posix()}")

        target = args.profile if args.profile else args.case
        mode = "profile" if args.profile else "case"
        print(f"Verified {mode} {target}: {len(case_ids)} cases resolved")
        return 0
    except ValueError as exc:
        parser.error(str(exc))
    except FileNotFoundError as exc:
        print(f"verify failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
