import os
import subprocess
import sys
from pathlib import Path
from shlex import join as shell_join


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _run_command(cmd, description: str, env=None) -> None:
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
            f"{description} failed: command not found: {cmd[0]}\ncommand: {shell_join(cmd)}"
        ) from exc
    except subprocess.CalledProcessError as exc:
        stderr = exc.stderr.strip() or "<empty stderr>"
        raise RuntimeError(
            f"{description} failed with exit code {exc.returncode}\n"
            f"command: {shell_join(cmd)}\n"
            f"stderr:\n{stderr}"
        ) from exc


def assemble_fixture(fixture_name: str, out_dir: str) -> str:
    src = repo_root() / "tests" / "verification" / "fixtures" / fixture_name
    obj = Path(out_dir) / "fixture.o"
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
        _run_command(as_cmd, "Fixture assembly via as")
    except (FileNotFoundError, RuntimeError):
        _run_command(llvm_mc_cmd, "Fixture assembly fallback via llvm-mc")
    return str(obj)


def run_uica_json(
    obj_path: str,
    out_json: str,
    arch: str = "SKL",
    env_overrides=None,
) -> None:
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    _run_command(
        [
            sys.executable,
            str(repo_root() / "uiCA.py"),
            obj_path,
            "-arch",
            arch,
            "-json",
            out_json,
            "-TPonly",
        ],
        "uiCA JSON run",
        env=env,
    )
