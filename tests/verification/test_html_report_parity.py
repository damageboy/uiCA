import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMPARE = ROOT / "verification" / "tools" / "compare_html_reports.py"
MANIFEST = ROOT / "rust" / "uica-data" / "generated" / "manifest.json"


class HtmlReportParityTests(unittest.TestCase):
   def test_python_and_rust_html_reports_match_for_add(self):
      if not MANIFEST.exists():
         raise AssertionError("missing rust/uica-data/generated/manifest.json; run Task 0 datapack setup")

      with tempfile.TemporaryDirectory() as tmp:
         tmp_path = Path(tmp)
         raw = tmp_path / "add.bin"
         raw.write_bytes(bytes([0x48, 0x01, 0xD8]))
         py_trace = tmp_path / "py-trace.html"
         py_graph = tmp_path / "py-graph.html"
         rs_trace = tmp_path / "rs-trace.html"
         rs_graph = tmp_path / "rs-graph.html"

         python_cmd = ["uv", "run", "python"] if shutil.which("uv") else [sys.executable]
         subprocess.run(
            [
               *python_cmd,
               "uiCA.py",
               str(raw),
               "-raw",
               "-arch",
               "SKL",
               "-trace",
               str(py_trace),
               "-graph",
               str(py_graph),
               "-TPonly",
               "-minCycles",
               "8",
               "-minIterations",
               "1",
            ],
            cwd=ROOT,
            check=True,
         )

         env = os.environ.copy()
         env.setdefault("PYTHON", getattr(sys, "_base_executable", sys.executable))
         env["UICA_RUST_DATAPACK"] = str(MANIFEST)
         subprocess.run(
            [
               "cargo",
               "run",
               "-p",
               "uica-cli",
               "--",
               str(raw),
               "--raw",
               "--arch",
               "SKL",
               "--trace",
               str(rs_trace),
               "--graph",
               str(rs_graph),
               "--tp-only",
               "--min-cycles",
               "8",
               "--min-iterations",
               "1",
            ],
            cwd=ROOT,
            env=env,
            check=True,
         )

         subprocess.run(
            [sys.executable, str(COMPARE), "--kind", "trace", str(py_trace), str(rs_trace)],
            cwd=ROOT,
            check=True,
         )
         subprocess.run(
            [sys.executable, str(COMPARE), "--kind", "graph", str(py_graph), str(rs_graph)],
            cwd=ROOT,
            check=True,
         )


if __name__ == "__main__":
   unittest.main()
