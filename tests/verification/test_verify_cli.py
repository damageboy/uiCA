import subprocess
import sys
import unittest

from tests.verification.helpers import repo_root


class TestVerifyCli(unittest.TestCase):
    def test_profile_quick_returns_zero(self):
        result = subprocess.run(
            [
                sys.executable,
                str(repo_root() / "verification" / "tools" / "verify.py"),
                "--profile",
                "quick",
                "--engine",
                "python",
            ],
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Verified profile quick: 2 cases resolved", result.stdout)

    def test_missing_profile_or_case_args_returns_nonzero(self):
        result = subprocess.run(
            [sys.executable, str(repo_root() / "verification" / "tools" / "verify.py")],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("pass exactly one of --profile or --case", result.stderr)
