import json
import tempfile
import unittest

from tests.verification.helpers import assemble_fixture, run_uica_json


class TestUiCAJsonContract(unittest.TestCase):
    def test_json_contains_v1_metadata_and_summary(self):
        with tempfile.TemporaryDirectory() as td:
            obj = assemble_fixture("loop_add.s", td)
            out_json = f"{td}/out.json"
            run_uica_json(obj, out_json, arch="SKL")

            with open(out_json) as f:
                data = json.load(f)

        self.assertEqual(data["schema_version"], "uica-result-v1")
        self.assertIn("summary", data)
        self.assertIn("throughput_cycles_per_iteration", data["summary"])
        self.assertIn("bottlenecks_predicted", data["summary"])
