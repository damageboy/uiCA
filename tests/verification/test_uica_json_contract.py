import json
import tempfile
import unittest

from tests.verification.helpers import assemble_fixture, run_uica_json


class TestUiCAJsonContract(unittest.TestCase):
    def test_json_contains_v1_metadata_and_summary(self):
        with tempfile.TemporaryDirectory() as td:
            obj = assemble_fixture("loop_add.s", td)
            out_json = f"{td}/out.json"
            run_uica_json(
                obj,
                out_json,
                arch="SKL",
                env_overrides={"UICA_COMMIT": "test-commit"},
            )

            with open(out_json) as f:
                data = json.load(f)

        self.assertEqual(data["schema_version"], "uica-result-v1")
        self.assertEqual(data["engine"], "python")
        self.assertEqual(data["engine_version"], "uiCA-python")
        self.assertEqual(data["uica_commit"], "test-commit")
        self.assertIn("invocation", data)
        self.assertEqual(data["invocation"]["arch"], "SKL")
        self.assertIn("alignmentOffset", data["invocation"])
        self.assertIn("initPolicy", data["invocation"])
        self.assertIn("noMicroFusion", data["invocation"])
        self.assertIn("noMacroFusion", data["invocation"])
        self.assertIn("simpleFrontEnd", data["invocation"])
        self.assertIn("minIterations", data["invocation"])
        self.assertIn("minCycles", data["invocation"])

        self.assertIn("summary", data)
        self.assertIn("throughput_cycles_per_iteration", data["summary"])
        self.assertIn("iterations_simulated", data["summary"])
        self.assertIn("cycles_simulated", data["summary"])
        self.assertIn("mode", data["summary"])
        self.assertIn("bottlenecks_predicted", data["summary"])
        self.assertIn("limits", data["summary"])
        self.assertEqual(
            set(data["summary"]["limits"].keys()),
            {
                "predecoder",
                "decoder",
                "dsb",
                "lsd",
                "issue",
                "ports",
                "dependencies",
            },
        )
