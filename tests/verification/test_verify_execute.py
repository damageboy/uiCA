import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from verification.tools.verify import main


class TestVerifyExecute(unittest.TestCase):
    def test_execute_mode_success(self):
        with (
            patch(
                "verification.tools.verify.resolve_golden_tag", return_value="tag123"
            ),
            patch(
                "verification.tools.verify.verify_case_arch",
                return_value=(True, None, Path("g"), Path("c")),
            ) as mocked_verify,
        ):
            rc = main(["--profile", "quick", "--engine", "python", "--jobs", "1"])

        self.assertEqual(rc, 0)
        # 2 cases * 3 arches from case manifests
        self.assertEqual(mocked_verify.call_count, 6)

    def test_execute_mode_writes_diff_report_on_failure(self):
        with tempfile.TemporaryDirectory() as td:
            diff_path = Path(td) / "diff.txt"

            with (
                patch(
                    "verification.tools.verify.resolve_golden_tag",
                    return_value="tag123",
                ),
                patch(
                    "verification.tools.verify.verify_case_arch",
                    side_effect=[
                        (
                            False,
                            "$.summary.x",
                            Path("golden.json"),
                            Path("candidate.json"),
                        ),
                        (True, None, Path("g"), Path("c")),
                        (True, None, Path("g"), Path("c")),
                        (True, None, Path("g"), Path("c")),
                        (True, None, Path("g"), Path("c")),
                        (True, None, Path("g"), Path("c")),
                    ],
                ),
            ):
                rc = main(
                    [
                        "--profile",
                        "quick",
                        "--engine",
                        "python",
                        "--jobs",
                        "1",
                        "--dump-diff",
                        str(diff_path),
                    ]
                )

            self.assertEqual(rc, 1)
            self.assertTrue(diff_path.exists())
            text = diff_path.read_text()
            self.assertIn("mismatch=$.summary.x", text)
