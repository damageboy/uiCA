import unittest

from verification.tools.verify import compare_summary_only


class TestSummaryParity(unittest.TestCase):
    def test_compare_summary_only_returns_no_diffs_for_equal_fields(self):
        golden = {
            "invocation": {"arch": "SKL", "minIterations": 10, "minCycles": 500},
            "summary": {
                "throughput_cycles_per_iteration": 1.0,
                "iterations_simulated": 10,
                "cycles_simulated": 500,
                "mode": "loop",
                "bottlenecks_predicted": ["issue"],
                "limits": {"issue": 1.0, "ports": 1.0},
            },
            "parameters": {"uArchName": "SKL", "issueWidth": 4},
        }
        candidate = {
            "invocation": {"arch": "SKL", "minIterations": 10, "minCycles": 500},
            "summary": {
                "throughput_cycles_per_iteration": 1.0,
                "iterations_simulated": 10,
                "cycles_simulated": 500,
                "mode": "loop",
                "bottlenecks_predicted": ["issue"],
                "limits": {"issue": 1.0, "ports": 1.0},
            },
            "parameters": {"uArchName": "SKL", "issueWidth": 4},
        }

        self.assertEqual(compare_summary_only(golden, candidate), [])

    def test_compare_summary_only_reports_field_mismatch(self):
        golden = {
            "invocation": {"arch": "SKL"},
            "summary": {"throughput_cycles_per_iteration": 1.0},
            "parameters": {"uArchName": "SKL"},
        }
        candidate = {
            "invocation": {"arch": "ICL"},
            "summary": {"throughput_cycles_per_iteration": 2.0},
            "parameters": {"uArchName": "ICL"},
        }

        diffs = compare_summary_only(golden, candidate)
        self.assertIn("invocation.arch", diffs)
        self.assertIn("summary.throughput_cycles_per_iteration", diffs)
        self.assertIn("parameters.uArchName", diffs)
