import unittest

from verification.tools.verify import first_mismatch_path


class TestVerifyTool(unittest.TestCase):
    def test_first_mismatch_path(self):
        left = {"summary": {"throughput_cycles_per_iteration": 1.0}}
        right = {"summary": {"throughput_cycles_per_iteration": 1.25}}

        self.assertEqual(
            first_mismatch_path(left, right),
            "$.summary.throughput_cycles_per_iteration",
        )
