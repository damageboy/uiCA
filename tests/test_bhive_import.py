import tempfile
import unittest
from pathlib import Path
from unittest import mock

from verification.tools import common
from verification.tools import import_bhive


class RawHexCaseSupportTests(unittest.TestCase):
   def test_corpus_manifest_path_falls_back_to_corpora(self):
      path = common.case_manifest_path("bhive/skl_000000_deadbeef")

      self.assertEqual(
         path,
         common.verification_root()
         / "corpora"
         / "bhive"
         / "cases"
         / "skl_000000_deadbeef.toml",
      )

   def test_prepare_case_input_writes_raw_hex_bytes(self):
      manifest = {
         "input": {"format": "hex", "hex": "4889de"},
         "run": {"arches": ["SKL"]},
      }
      with tempfile.TemporaryDirectory() as td:
         path, is_raw = common.prepare_case_input("bhive/skl_000000_deadbeef", manifest, Path(td))

         self.assertTrue(is_raw)
         self.assertEqual(path.read_bytes(), bytes.fromhex("4889de"))
         self.assertEqual(path.name, "snippet.bin")

   def test_python_runner_adds_raw_flag(self):
      with tempfile.TemporaryDirectory() as td:
         out_json = Path(td) / "out.json"
         obj_path = Path(td) / "snippet.bin"
         obj_path.write_bytes(b"\x90")

         with mock.patch.object(common, "_run_command") as run_command:
            common.run_python_uica(
               obj_path,
               out_json,
               "SKL",
               {},
               uica_commit="test",
               raw=True,
            )

      cmd = run_command.call_args.args[0]
      self.assertIn("-raw", cmd)

   def test_rust_runner_adds_raw_flag(self):
      with tempfile.TemporaryDirectory() as td:
         out_json = Path(td) / "out.json"
         obj_path = Path(td) / "snippet.bin"
         obj_path.write_bytes(b"\x90")

         with mock.patch.object(common, "_run_command") as run_command:
            common.run_rust_uica(
               "uica-cli",
               obj_path,
               out_json,
               "SKL",
               {},
               uica_commit="test",
               raw=True,
            )

      cmd = run_command.call_args.args[0]
      self.assertIn("--raw", cmd)


class BhiveImporterTests(unittest.TestCase):
   def test_importer_writes_sampled_case_manifests_and_profile(self):
      csv_text = "\n".join(
         [
            ",0.000000",
            "4889de,91.000000",
            "90,100.000000",
            "zz,1.000000",
            "4889c2,123.000000",
         ]
      )

      with tempfile.TemporaryDirectory() as td:
         root = Path(td)
         source = root / "skl.csv"
         cases_root = root / "cases"
         profiles_root = root / "profiles"
         source.write_text(csv_text)

         written = import_bhive.import_bhive(
            arch="SKL",
            source=source.as_posix(),
            limit=2,
            profile="bhive_smoke",
            cases_root=cases_root,
            profiles_root=profiles_root,
         )

         self.assertEqual(len(written.case_ids), 2)
         self.assertTrue((profiles_root / "bhive_smoke.toml").exists())
         profile = (profiles_root / "bhive_smoke.toml").read_text()
         self.assertIn('name = "bhive_smoke"', profile)
         self.assertIn('arches = ["SKL"]', profile)

         manifests = sorted(cases_root.glob("*.toml"))
         self.assertEqual(len(manifests), 2)
         first_manifest = manifests[0].read_text()
         self.assertIn('id = "bhive/', first_manifest)
         self.assertIn('[input]', first_manifest)
         self.assertIn('format = "hex"', first_manifest)
         self.assertIn('measuredCyclesPer100Iterations = 91.0', first_manifest)
         self.assertIn('measuredCyclesPerIteration = 0.91', first_manifest)


if __name__ == "__main__":
   unittest.main()
