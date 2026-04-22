import argparse
import json
import sys
import tempfile
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from verification.tools.canonicalize import canonicalize_result
from verification.tools.common import (
    assemble_case_snippet,
    case_manifest_path,
    get_git_commit_short,
    load_case_manifest,
    load_json,
    load_profile,
    run_python_uica,
)


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
    parser.add_argument(
        "--golden-tag",
        help="golden tag directory below engine root (default: current git short SHA)",
    )
    return parser


def resolve_case_ids_and_profile_arches(args) -> tuple[list[str], list[str] | None]:
    if bool(args.profile) == bool(args.case):
        raise ValueError("pass exactly one of --profile or --case")

    if args.profile:
        profile = load_profile(args.profile)
        case_ids = profile.get("cases", [])
        if not case_ids:
            raise ValueError(f"profile has no cases: {args.profile}")
        return case_ids, profile.get("arches")

    return [args.case], None


def resolve_arches(
    cli_arches: list[str] | None,
    case_manifest: dict,
    profile_arches: list[str] | None,
) -> list[str]:
    if cli_arches:
        return cli_arches

    run_arches = case_manifest.get("run", {}).get("arches")
    if run_arches:
        return list(run_arches)

    if profile_arches:
        return list(profile_arches)

    raise ValueError(
        f"no architecture list available for case {case_manifest.get('id', '<unknown>')}"
    )


def capture_case_arch(
    *,
    case_id: str,
    arch: str,
    case_manifest: dict,
    engine: str,
    rust_bin: str | None,
    golden_root: Path,
    golden_tag: str,
) -> Path:
    if engine == "rust":
        raise NotImplementedError("rust engine capture not implemented yet")

    if rust_bin is not None:
        _ = rust_bin  # keep arg validated by parser for future use

    with tempfile.TemporaryDirectory() as td:
        work = Path(td)
        obj = assemble_case_snippet(case_id, work)
        out_json = work / "result.json"
        run_python_uica(
            obj,
            out_json,
            arch,
            case_manifest.get("run", {}),
            uica_commit=golden_tag,
        )
        result = load_json(out_json)

    out_path = golden_root / engine / golden_tag / case_id / f"{arch}.json"
    write_golden(result, str(out_path))
    return out_path


def main(argv=None) -> int:
    parser = build_parser()

    try:
        args = parser.parse_args(argv)
        case_ids, profile_arches = resolve_case_ids_and_profile_arches(args)

        golden_root = Path(args.golden_root)
        golden_tag = args.golden_tag or get_git_commit_short()

        captured = 0
        outputs: list[Path] = []
        for case_id in case_ids:
            manifest_path = case_manifest_path(case_id)
            if not manifest_path.exists():
                raise FileNotFoundError(f"missing case manifest: {manifest_path}")
            case_manifest = load_case_manifest(manifest_path)
            arches = resolve_arches(args.arches, case_manifest, profile_arches)
            for arch in arches:
                out_path = capture_case_arch(
                    case_id=case_id,
                    arch=arch,
                    case_manifest=case_manifest,
                    engine=args.engine,
                    rust_bin=args.rust_bin,
                    golden_root=golden_root,
                    golden_tag=golden_tag,
                )
                captured += 1
                outputs.append(out_path)

        target = args.profile if args.profile else args.case
        mode = "profile" if args.profile else "case"
        print(
            f"Captured {captured} golden files for {mode} {target} under "
            f"{(golden_root / args.engine / golden_tag).as_posix()}"
        )
        for path in outputs:
            print(path.as_posix())
        return 0

    except ValueError as exc:
        parser.error(str(exc))
    except (FileNotFoundError, RuntimeError, NotImplementedError) as exc:
        print(f"capture failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
