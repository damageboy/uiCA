import argparse
import json
import os
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path
from typing import Any

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
    parser.add_argument(
        "--jobs",
        type=int,
        default=(os.cpu_count() or 1),
        help="number of worker processes",
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
        _ = rust_bin  # reserved for future Rust capture

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


def _capture_task(task: dict[str, Any]) -> str:
    out_path = capture_case_arch(
        case_id=task["case_id"],
        arch=task["arch"],
        case_manifest=task["case_manifest"],
        engine=task["engine"],
        rust_bin=task["rust_bin"],
        golden_root=Path(task["golden_root"]),
        golden_tag=task["golden_tag"],
    )
    return out_path.as_posix()


def build_capture_tasks(
    *,
    case_ids: list[str],
    profile_arches: list[str] | None,
    cli_arches: list[str] | None,
    engine: str,
    rust_bin: str | None,
    golden_root: Path,
    golden_tag: str,
) -> list[dict[str, Any]]:
    tasks: list[dict[str, Any]] = []

    for case_id in case_ids:
        manifest_path = case_manifest_path(case_id)
        if not manifest_path.exists():
            raise FileNotFoundError(f"missing case manifest: {manifest_path}")
        case_manifest = load_case_manifest(manifest_path)
        arches = resolve_arches(cli_arches, case_manifest, profile_arches)
        for arch in arches:
            tasks.append(
                {
                    "case_id": case_id,
                    "arch": arch,
                    "case_manifest": case_manifest,
                    "engine": engine,
                    "rust_bin": rust_bin,
                    "golden_root": golden_root.as_posix(),
                    "golden_tag": golden_tag,
                }
            )

    return tasks


def main(argv=None) -> int:
    parser = build_parser()

    try:
        args = parser.parse_args(argv)
        case_ids, profile_arches = resolve_case_ids_and_profile_arches(args)

        golden_root = Path(args.golden_root)
        golden_tag = args.golden_tag or get_git_commit_short()

        if args.jobs < 1:
            raise ValueError("--jobs must be >= 1")

        tasks = build_capture_tasks(
            case_ids=case_ids,
            profile_arches=profile_arches,
            cli_arches=args.arches,
            engine=args.engine,
            rust_bin=args.rust_bin,
            golden_root=golden_root,
            golden_tag=golden_tag,
        )

        outputs: list[Path] = []
        if args.jobs == 1 or len(tasks) <= 1:
            for task in tasks:
                outputs.append(Path(_capture_task(task)))
        else:
            with ProcessPoolExecutor(max_workers=args.jobs) as pool:
                for out_path in pool.map(_capture_task, tasks):
                    outputs.append(Path(out_path))

        target = args.profile if args.profile else args.case
        mode = "profile" if args.profile else "case"
        print(
            f"Captured {len(outputs)} golden files for {mode} {target} under "
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
