import tempfile
import textwrap
import unittest

from verification.tools.common import load_case_manifest


class TestCaseManifest(unittest.TestCase):
    def test_load_case_manifest(self):
        with tempfile.TemporaryDirectory() as td:
            path = f"{td}/case.toml"
            with open(path, "w") as f:
                f.write(
                    textwrap.dedent(
                        """
                        id = "curated/add-loop"
                        [run]
                        arches = ["HSW", "SKL", "ICL"]
                        alignmentOffset = 0
                        """
                    )
                )

            manifest = load_case_manifest(path)

        self.assertEqual(manifest["id"], "curated/add-loop")
        self.assertEqual(manifest["run"]["arches"][1], "SKL")
