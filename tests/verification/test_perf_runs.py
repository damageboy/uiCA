import tempfile
import unittest
from pathlib import Path

from verification.tools.perf_runs import (
    PerfReportRow,
    aggregate_report_rows,
    build_perf_record_command,
    load_command_script,
    parse_perf_report,
)


class TestPerfRuns(unittest.TestCase):
    def test_load_command_script_returns_only_run_commands(self):
        with tempfile.TemporaryDirectory() as td:
            script = Path(td) / "commands.sh"
            script.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "export UICA_COMMIT=timing-tag\n"
                "\n"
                "echo one --json out1.json\n"
                "# comment\n"
                "echo two --json out2.json\n"
            )

            commands = load_command_script(script)

        self.assertEqual(commands, [["echo", "one", "--json", "out1.json"], ["echo", "two", "--json", "out2.json"]])

    def test_build_perf_record_command_wraps_command(self):
        command = build_perf_record_command(
            ["uica-cli", "input.bin", "--arch", "SKL"],
            Path("run.perf.data"),
            perf_bin="perf",
            event="cpu-clock:u",
            frequency=997,
            call_graph="dwarf,8192",
        )

        self.assertEqual(
            command,
            [
                "perf",
                "record",
                "--quiet",
                "--event",
                "cpu-clock:u",
                "--freq",
                "997",
                "--call-graph",
                "dwarf,8192",
                "--output",
                "run.perf.data",
                "--",
                "uica-cli",
                "input.bin",
                "--arch",
                "SKL",
            ],
        )

    def test_parse_perf_report_extracts_rows(self):
        report = """
# Samples: 10  of event 'cpu-clock:u'
# Event count (approx.): 10000000
#
# Overhead       Samples  Shared Object      Symbol
# ........  ............  .................  ................
#
    55.50%             6  uica-cli           [.] uica::simulate::run                                                               -      -
    44.50%             4  libc.so.6          [.] __memmove_avx_unaligned_erms                                                       -      -
"""

        rows = parse_perf_report(report, source="000001.perf.data")

        self.assertEqual(
            rows,
            [
                PerfReportRow("uica-cli", "uica::simulate::run", 6, 55.50, "000001.perf.data"),
                PerfReportRow("libc.so.6", "__memmove_avx_unaligned_erms", 4, 44.50, "000001.perf.data"),
            ],
        )

    def test_aggregate_report_rows_sums_samples(self):
        rows = [
            PerfReportRow("uica-cli", "decode", 3, 30.0, "a"),
            PerfReportRow("uica-cli", "decode", 7, 70.0, "b"),
            PerfReportRow("uica-cli", "simulate", 10, 100.0, "b"),
        ]

        summary = aggregate_report_rows(rows)

        self.assertEqual(summary[0]["samples"], 10)
        self.assertEqual(summary[0]["self_percent"], 50.0)
        self.assertEqual(summary[0]["dso"], "uica-cli")
        self.assertEqual(summary[0]["symbol"], "decode")
        self.assertEqual(summary[0]["runs"], 2)
        self.assertEqual(summary[1]["symbol"], "simulate")
        self.assertEqual(summary[1]["self_percent"], 50.0)


if __name__ == "__main__":
    unittest.main()
