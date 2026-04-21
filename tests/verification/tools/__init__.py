from pathlib import Path

# Allow imports like `verification.tools.*` to resolve repo implementation when
# unittest discover is invoked with `-s tests` (which makes `tests/verification`
# top-level `verification` package).
_repo_tools = Path(__file__).resolve().parents[3] / "verification" / "tools"
__path__.append(str(_repo_tools))
