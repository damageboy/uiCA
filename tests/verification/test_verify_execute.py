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

    def test_execute_mode_rust_requires_rust_bin(self):
        with self.assertRaises(SystemExit) as exc:
            main(["--profile", "quick", "--engine", "rust", "--jobs", "1"])

        self.assertEqual(exc.exception.code, 2)

    def test_execute_mode_rust_passes_rust_bin_to_verify(self):
        with (
            patch(
                "verification.tools.verify.resolve_golden_tag", return_value="tag123"
            ),
            patch(
                "verification.tools.verify.verify_case_arch",
                return_value=(True, None, Path("g"), Path("c")),
            ) as mocked_verify,
        ):
            rc = main(
                [
                    "--profile",
                    "quick",
                    "--engine",
                    "rust",
                    "--rust-bin",
                    "/tmp/uica-rs",
                    "--jobs",
                    "1",
                ]
            )

        self.assertEqual(rc, 0)
        self.assertEqual(mocked_verify.call_count, 6)
        for call in mocked_verify.call_args_list:
            self.assertEqual(call.kwargs["engine"], "rust")
            self.assertEqual(call.kwargs["rust_bin"], "/tmp/uica-rs")

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
