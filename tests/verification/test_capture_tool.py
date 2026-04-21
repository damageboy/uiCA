import json
import tempfile
import unittest

from verification.tools.capture import write_golden


class TestCaptureTool(unittest.TestCase):
    def test_write_golden(self):
        with tempfile.TemporaryDirectory() as td:
            out = f"{td}/golden/result.json"
            write_golden({"schema_version": "uica-result-v1"}, out)

            with open(out) as f:
                data = json.load(f)

        self.assertEqual(data["schema_version"], "uica-result-v1")
