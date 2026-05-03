import json
import tempfile
import unittest
from pathlib import Path

from verification.tools.common import run_rust_uica


class TestRustEngineAdapter(unittest.TestCase):
    def test_run_rust_uica_passes_run_config_flags(self):
        with tempfile.TemporaryDirectory() as td:
            work = Path(td)
            rust_bin = work / "mock-rust-uica"
            obj = work / "snippet.o"
            out_json = work / "result.json"

            rust_bin.write_text(
                "#!/usr/bin/env python3\n"
                "import json\n"
                "import os\n"
                "import sys\n"
                "args = sys.argv[1:]\n"
                "out_json = args[args.index('--json') + 1]\n"
                "with open(out_json, 'w') as f:\n"
                "    json.dump({'argv': args, 'uica_commit': os.environ.get('UICA_COMMIT')}, f)\n"
            )
            rust_bin.chmod(0o755)
            obj.write_bytes(b"object")

            run_rust_uica(
                rust_bin,
                obj,
                out_json,
                "SKL",
                {
                    "alignmentOffset": 4,
                    "initPolicy": "same",
                    "minIterations": 20,
                    "minCycles": 600,
                    "noMicroFusion": True,
                    "noMacroFusion": True,
                    "simpleFrontEnd": True,
                },
                uica_commit="abc123",
            )

            with out_json.open() as f:
                data = json.load(f)

        self.assertEqual(data["uica_commit"], "abc123")
        self.assertEqual(
            data["argv"],
            [
                str(obj),
                "--arch",
                "SKL",
                "--json",
                str(out_json),
                "--tp-only",
                "--alignment-offset",
                "4",
                "--init-policy",
                "same",
                "--min-iterations",
                "20",
                "--min-cycles",
                "600",
                "--no-micro-fusion",
                "--no-macro-fusion",
                "--simple-front-end",
            ],
        )
