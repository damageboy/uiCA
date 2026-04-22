import argparse
import sys
import tempfile
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
        "--golden-tag",
        help="golden tag directory below engine root (default: current git short SHA, with fallback when unique)",
    )
    parser.add_argument(
        "--dump-diff",
        help="optional path for mismatch report output",
    )
    parser.add_argument(
        "--execute",
        action="store_true",
        help="execute engine and compare against captured goldens (default checks case/profile resolution only)",
    )
    return parser


def resolve_case_ids_and_profile_arches(args) -> tuple[list[str], list[str] | None]:
    if bool(args.profile) == bool(args.case):
        raise ValueError("pass exactly one of --profile or --case")

    if args.profile:
        try:
            profile = load_profile(args.profile)
        except FileNotFoundError as exc:
            raise FileNotFoundError(f"missing profile: {args.profile}") from exc

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


def resolve_golden_tag(golden_root: Path, engine: str, explicit_tag: str | None) -> str:
    engine_root = golden_root / engine
    if explicit_tag:
        return explicit_tag

    commit_tag = get_git_commit_short()
    if (engine_root / commit_tag).exists():
        return commit_tag

    if not engine_root.exists():
        raise FileNotFoundError(
            f"missing engine golden root: {engine_root} (use capture or --golden-tag)"
        )

    tags = sorted(p.name for p in engine_root.iterdir() if p.is_dir())
    if len(tags) == 1:
        return tags[0]

    raise FileNotFoundError(
        f"cannot infer golden tag under {engine_root}; use --golden-tag"
    )


def collect_diff_lines(
    *,
    case_id: str,
    arch: str,
    mismatch_path: str,
    golden_path: Path,
    candidate_path: Path,
) -> list[str]:
    return [
        f"case={case_id} arch={arch}",
        f"mismatch={mismatch_path}",
        f"golden={golden_path.as_posix()}",
        f"candidate={candidate_path.as_posix()}",
        "",
    ]


def compare_json_files(golden_path: Path, candidate_path: Path) -> str | None:
    golden = canonicalize_result(load_json(golden_path))
    candidate = canonicalize_result(load_json(candidate_path))
    return first_mismatch_path(golden, candidate)


def verify_case_arch(
    *,
    case_id: str,
    arch: str,
    case_manifest: dict,
    engine: str,
    rust_bin: str | None,
    golden_root: Path,
    golden_tag: str,
) -> tuple[bool, str | None, Path, Path]:
    if engine == "rust":
        raise NotImplementedError("rust engine verify not implemented yet")

    if rust_bin is not None:
        _ = rust_bin

    golden_path = golden_root / engine / golden_tag / case_id / f"{arch}.json"
    if not golden_path.exists():
        raise FileNotFoundError(f"missing golden file: {golden_path}")

    with tempfile.TemporaryDirectory() as td:
        work = Path(td)
        obj = assemble_case_snippet(case_id, work)
        candidate_path = work / "candidate.json"
        run_python_uica(
            obj,
            candidate_path,
            arch,
            case_manifest.get("run", {}),
            uica_commit=golden_tag,
        )

        mismatch = compare_json_files(golden_path, candidate_path)
        if mismatch:
            persisted_candidate = (
                golden_root / "_candidates" / engine / case_id / f"{arch}.json"
            )
            persisted_candidate.parent.mkdir(parents=True, exist_ok=True)
            persisted_candidate.write_text(candidate_path.read_text())
            return False, mismatch, golden_path, persisted_candidate

    return True, None, golden_path, Path("<tmp>")


def write_diff_report(path: Path, lines: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")


def main(argv=None) -> int:
    parser = build_parser()

    try:
        args = parser.parse_args(argv)
        case_ids, profile_arches = resolve_case_ids_and_profile_arches(args)

        manifests: dict[str, dict[str, Any]] = {}
        for case_id in case_ids:
            manifest_path = case_manifest_path(case_id)
            if not manifest_path.exists():
                raise FileNotFoundError(
                    f"missing case manifest: {manifest_path.as_posix()}"
                )
            manifests[case_id] = load_case_manifest(manifest_path)

        target = args.profile if args.profile else args.case
        mode = "profile" if args.profile else "case"

        if not args.execute:
            print(f"Verified {mode} {target}: {len(case_ids)} cases resolved")
            return 0

        golden_root = Path(args.golden_root)
        golden_tag = resolve_golden_tag(golden_root, args.engine, args.golden_tag)

        diff_lines: list[str] = []
        passed = 0
        failed = 0
        for case_id in case_ids:
            case_manifest = manifests[case_id]
            arches = resolve_arches(args.arches, case_manifest, profile_arches)
            for arch in arches:
                ok, mismatch, golden_path, candidate_path = verify_case_arch(
                    case_id=case_id,
                    arch=arch,
                    case_manifest=case_manifest,
                    engine=args.engine,
                    rust_bin=args.rust_bin,
                    golden_root=golden_root,
                    golden_tag=golden_tag,
                )
                if ok:
                    passed += 1
                    continue

                failed += 1
                diff_lines.extend(
                    collect_diff_lines(
                        case_id=case_id,
                        arch=arch,
                        mismatch_path=mismatch or "<unknown>",
                        golden_path=golden_path,
                        candidate_path=candidate_path,
                    )
                )

        if args.dump_diff and diff_lines:
            write_diff_report(Path(args.dump_diff), diff_lines)

        if failed:
            print(
                f"Verification failed: {failed} mismatch(es), {passed} match(es)",
                file=sys.stderr,
            )
            if args.dump_diff:
                print(f"Diff report: {args.dump_diff}", file=sys.stderr)
            return 1

        print(
            f"Verified {mode} {target}: {passed} case/arch result(s) matched "
            f"against golden tag {golden_tag}"
        )
        return 0

    except ValueError as exc:
        parser.error(str(exc))
    except (FileNotFoundError, RuntimeError, NotImplementedError) as exc:
        print(f"verify failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
