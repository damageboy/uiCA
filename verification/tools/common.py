import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import tomllib


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def verification_root() -> Path:
    return repo_root() / "verification"


def load_profile(name: str) -> dict:
    base = verification_root() / "profiles"
    with open(base / f"{name}.toml", "rb") as f:
        return tomllib.load(f)


def load_case_manifest(path: str | Path) -> dict:
    with open(path, "rb") as f:
        data = tomllib.load(f)

    if "id" not in data or "run" not in data:
        raise ValueError("case manifest missing id/run")

    return data


def iter_case_dirs(root: Path):
    for path in sorted(root.glob("*/*")):
        if path.is_dir() and (path / "case.toml").exists():
            yield path


def case_dir_for_id(case_id: str) -> Path:
    return verification_root() / "cases" / case_id


def case_manifest_path(case_id: str) -> Path:
    curated_path = case_dir_for_id(case_id) / "case.toml"
    if curated_path.exists():
        return curated_path

    if "/" in case_id:
        corpus, name = case_id.split("/", 1)
        corpus_path = verification_root() / "corpora" / corpus / "cases" / f"{name}.toml"
        if corpus_path.exists() or corpus != "curated":
            return corpus_path

    return curated_path


def snippet_path(case_id: str) -> Path:
    return case_dir_for_id(case_id) / "snippet.s"


def get_git_commit_short(default: str = "unknown") -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=repo_root(),
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.strip() or default
    except Exception:
        return default


def _run_command(
    cmd: list[str], description: str, env: dict[str, str] | None = None
) -> None:
    try:
        subprocess.run(
            cmd,
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
    except FileNotFoundError as exc:
        raise FileNotFoundError(
            f"{description} failed: command not found: {cmd[0]}"
        ) from exc
    except subprocess.CalledProcessError as exc:
        stderr = exc.stderr.strip() or "<empty stderr>"
        raise RuntimeError(
            f"{description} failed with exit code {exc.returncode}\n"
            f"command: {' '.join(cmd)}\n"
            f"stderr:\n{stderr}"
        ) from exc


def assemble_case_snippet(case_id: str, out_dir: Path) -> Path:
    src = snippet_path(case_id)
    obj = out_dir / "snippet.o"

    as_cmd = ["as", str(src), "-o", str(obj)]
    llvm_mc_cmd = [
        "llvm-mc",
        "-filetype=obj",
        "-triple=x86_64-pc-linux-gnu",
        str(src),
        "-o",
        str(obj),
    ]

    try:
        _run_command(as_cmd, "Case assembly via as")
    except (FileNotFoundError, RuntimeError):
        _run_command(llvm_mc_cmd, "Case assembly fallback via llvm-mc")

    return obj


def prepare_case_input(
    case_id: str, case_manifest: dict[str, Any], out_dir: Path
) -> tuple[Path, bool]:
    input_config = case_manifest.get("input", {})
    if input_config.get("format") != "hex":
        return assemble_case_snippet(case_id, out_dir), False

    hex_text = "".join(str(input_config.get("hex", "")).split())
    if not hex_text or len(hex_text) % 2:
        raise ValueError(f"invalid raw hex input for case {case_id}")

    try:
        raw_bytes = bytes.fromhex(hex_text)
    except ValueError as exc:
        raise ValueError(f"invalid raw hex input for case {case_id}") from exc

    raw_path = out_dir / "snippet.bin"
    raw_path.write_bytes(raw_bytes)
    return raw_path, True


def _append_python_run_config_flags(cmd: list[str], run_config: dict[str, Any]) -> None:
    """Python uiCA.py uses single-dash camelCase flags (argparse convention)."""
    if "alignmentOffset" in run_config:
        cmd.extend(["-alignmentOffset", str(run_config["alignmentOffset"])])
    if "initPolicy" in run_config:
        cmd.extend(["-initPolicy", str(run_config["initPolicy"])])
    if "minIterations" in run_config:
        cmd.extend(["-minIterations", str(run_config["minIterations"])])
    if "minCycles" in run_config:
        cmd.extend(["-minCycles", str(run_config["minCycles"])])

    if run_config.get("noMicroFusion", False):
        cmd.append("-noMicroFusion")
    if run_config.get("noMacroFusion", False):
        cmd.append("-noMacroFusion")
    if run_config.get("simpleFrontEnd", False):
        cmd.append("-simpleFrontEnd")


def _append_rust_run_config_flags(cmd: list[str], run_config: dict[str, Any]) -> None:
    """Rust uica-cli uses GNU-style double-dash kebab-case flags (clap)."""
    if "alignmentOffset" in run_config:
        cmd.extend(["--alignment-offset", str(run_config["alignmentOffset"])])
    if "initPolicy" in run_config:
        cmd.extend(["--init-policy", str(run_config["initPolicy"])])
    if "minIterations" in run_config:
        cmd.extend(["--min-iterations", str(run_config["minIterations"])])
    if "minCycles" in run_config:
        cmd.extend(["--min-cycles", str(run_config["minCycles"])])

    if run_config.get("noMicroFusion", False):
        cmd.append("--no-micro-fusion")
    if run_config.get("noMacroFusion", False):
        cmd.append("--no-macro-fusion")
    if run_config.get("simpleFrontEnd", False):
        cmd.append("--simple-front-end")


def run_python_uica(
    obj_path: Path,
    out_json: Path,
    arch: str,
    run_config: dict[str, Any],
    *,
    uica_commit: str,
    raw: bool = False,
) -> None:
    cmd = [
        sys.executable,
        str(repo_root() / "uiCA.py"),
        str(obj_path),
        "-arch",
        arch,
        "-json",
        str(out_json),
        "-TPonly",
    ]

    if raw:
        cmd.append("-raw")

    _append_python_run_config_flags(cmd, run_config)

    env = os.environ.copy()
    env["UICA_COMMIT"] = uica_commit
    _run_command(cmd, "uiCA python engine run", env=env)


def run_rust_uica(
    rust_bin: str | Path,
    obj_path: Path,
    out_json: Path,
    arch: str,
    run_config: dict[str, Any],
    *,
    uica_commit: str,
    raw: bool = False,
) -> None:
    cmd = [
        str(rust_bin),
        str(obj_path),
        "--arch",
        arch,
        "--json",
        str(out_json),
        "--tp-only",
    ]

    if raw:
        cmd.append("--raw")

    _append_rust_run_config_flags(cmd, run_config)

    env = os.environ.copy()
    env["UICA_COMMIT"] = uica_commit
    _run_command(cmd, "uiCA rust engine run", env=env)


def load_json(path: Path) -> dict:
    with path.open() as f:
        return json.load(f)
