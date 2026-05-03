import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tests.verification.helpers import repo_root


class TestVerifyCli(unittest.TestCase):
    def test_profile_quick_resolve_only_returns_zero(self):
        result = subprocess.run(
            [
                sys.executable,
                str(repo_root() / "verification" / "tools" / "verify.py"),
                "--profile",
                "quick",
                "--engine",
                "python",
                "--resolve-only",
            ],
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Verified profile quick: 2 cases resolved", result.stdout)

    def test_profile_quick_execute_without_golden_fails(self):
        result = subprocess.run(
            [
                sys.executable,
                str(repo_root() / "verification" / "tools" / "verify.py"),
                "--profile",
                "quick",
                "--engine",
                "python",
                "--golden-root",
                "/tmp/definitely-missing-uica-goldens",
            ],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing engine golden root", result.stderr)

    def test_missing_profile_or_case_args_returns_nonzero(self):
        result = subprocess.run(
            [sys.executable, str(repo_root() / "verification" / "tools" / "verify.py")],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("pass exactly one of --profile or --case", result.stderr)

    def test_emit_command_script_prepares_bhive_fixture_without_goldens(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            script = root / "python.sh"
            fixtures = root / "fixtures"

            result = subprocess.run(
                [
                    sys.executable,
                    str(repo_root() / "verification" / "tools" / "verify.py"),
                    "--case",
                    "bhive/skl_000000_56fde11a",
                    "--engine",
                    "python",
                    "--golden-root",
                    str(root / "missing-goldens"),
                    "--golden-tag",
                    "timing-tag",
                    "--emit-command-script",
                    str(script),
                    "--fixture-root",
                    str(fixtures),
                ],
                capture_output=True,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(script.exists())
            fixture = fixtures / "inputs" / "bhive" / "skl_000000_56fde11a" / "snippet.bin"
            self.assertTrue(fixture.exists())
            text = script.read_text()
            self.assertIn("set -euo pipefail", text)
            self.assertIn("export UICA_COMMIT=timing-tag", text)
            self.assertIn("uiCA.py", text)
            self.assertIn(str(fixture), text)
            self.assertIn("-raw", text)
            self.assertIn("-json", text)
            self.assertIn(str(fixtures / "candidates" / "python"), text)
            self.assertIn("Wrote 1 command(s)", result.stdout)

    def test_emit_command_script_writes_rust_cli_commands(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            script = root / "rust.sh"
            fixtures = root / "fixtures"
            rust_bin = root / "uica-cli"

            result = subprocess.run(
                [
                    sys.executable,
                    str(repo_root() / "verification" / "tools" / "verify.py"),
                    "--case",
                    "bhive/skl_000000_56fde11a",
                    "--engine",
                    "rust",
                    "--rust-bin",
                    str(rust_bin),
                    "--golden-root",
                    str(root / "missing-goldens"),
                    "--golden-tag",
                    "timing-tag",
                    "--emit-command-script",
                    str(script),
                    "--fixture-root",
                    str(fixtures),
                ],
                capture_output=True,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            text = script.read_text()
            self.assertIn(str(rust_bin), text)
            self.assertIn("--raw", text)
            self.assertIn("--json", text)
            self.assertIn("--tp-only", text)
            self.assertIn(str(fixtures / "candidates" / "rust"), text)
