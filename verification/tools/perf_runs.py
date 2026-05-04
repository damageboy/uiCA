#!/usr/bin/env python3
"""Record emitted verification command scripts with Linux perf.

Example:
  python3 verification/tools/perf_runs.py \
    --command-script /tmp/rust-bhive-1k.sh \
    --out-dir /tmp/uica-perf-rust-bhive-1k
"""

import argparse
import csv
import json
import os
import re
import resource
import shlex
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class PerfReportRow:
    dso: str
    symbol: str
    samples: int
    self_percent: float
    source: str


_REPORT_ROW_RE = re.compile(
    r"^\s*(?P<percent>\d+(?:\.\d+)?)%\s+"
    r"(?P<samples>\d+)\s+"
    r"(?P<dso>\S+)\s+"
    r"(?P<symbol>.+?)\s*$"
)
_SYMBOL_PREFIX_RE = re.compile(r"^\[[^\]]+\]\s+")
_TRAILING_IPC_COLUMNS_RE = re.compile(r"\s{2,}\S+\s+\S+\s*$")


CONTROL_PREFIXES = ("set ", "export ", "cd ", "ulimit ")


def load_command_script(path: Path) -> list[list[str]]:
    """Return runnable command lines from emitted shell script.

    Used for metadata/counts only. Recording uses shell script directly so env/control
    lines still apply exactly.
    """
    commands: list[list[str]] = []
    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or line.startswith("#!"):
            continue
        if line.startswith(CONTROL_PREFIXES):
            continue
        commands.append(shlex.split(line))
    return commands


def build_perf_record_command(
    command: list[str],
    perf_data: Path,
    *,
    perf_bin: str = "perf",
    event: str = "cpu-clock:u",
    frequency: int = 997,
    call_graph: str | None = "dwarf,8192",
    quiet: bool = True,
) -> list[str]:
    cmd = [perf_bin, "record"]
    if quiet:
        cmd.append("--quiet")
    cmd.extend(["--event", event, "--freq", str(frequency)])
    if call_graph:
        cmd.extend(["--call-graph", call_graph])
    cmd.extend(["--output", str(perf_data), "--"])
    cmd.extend(command)
    return cmd


def build_perf_report_command(
    perf_data: Path,
    *,
    perf_bin: str = "perf",
    percent_limit: float = 0.0,
) -> list[str]:
    return [
        perf_bin,
        "report",
        "--stdio",
        "--no-children",
        "--show-nr-samples",
        "--percent-limit",
        str(percent_limit),
        "--sort",
        "dso,symbol",
        "-F",
        "overhead,sample,dso,symbol",
        "-i",
        str(perf_data),
    ]


def parse_perf_report(text: str, *, source: str) -> list[PerfReportRow]:
    rows: list[PerfReportRow] = []
    for line in text.splitlines():
        match = _REPORT_ROW_RE.match(line)
        if not match:
            continue
        symbol = _TRAILING_IPC_COLUMNS_RE.sub("", match.group("symbol")).strip()
        symbol = _SYMBOL_PREFIX_RE.sub("", symbol).strip()
        rows.append(
            PerfReportRow(
                dso=match.group("dso"),
                symbol=symbol,
                samples=int(match.group("samples")),
                self_percent=float(match.group("percent")),
                source=source,
            )
        )
    return rows


def aggregate_report_rows(rows: list[PerfReportRow]) -> list[dict[str, Any]]:
    totals: dict[tuple[str, str], dict[str, Any]] = {}
    total_samples = sum(row.samples for row in rows)
    for row in rows:
        key = (row.dso, row.symbol)
        entry = totals.setdefault(
            key,
            {"dso": row.dso, "symbol": row.symbol, "samples": 0, "sources": set()},
        )
        entry["samples"] += row.samples
        entry["sources"].add(row.source)

    summary: list[dict[str, Any]] = []
    for entry in totals.values():
        samples = int(entry["samples"])
        summary.append(
            {
                "self_percent": (samples / total_samples * 100.0) if total_samples else 0.0,
                "samples": samples,
                "dso": entry["dso"],
                "symbol": entry["symbol"],
                "runs": len(entry["sources"]),
            }
        )

    summary.sort(key=lambda item: (-item["samples"], item["dso"], item["symbol"]))
    return summary


def write_summary_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(
            f,
            fieldnames=["self_percent", "samples", "dso", "symbol", "runs"],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(
                {
                    "self_percent": f"{row['self_percent']:.4f}",
                    "samples": row["samples"],
                    "dso": row["dso"],
                    "symbol": row["symbol"],
                    "runs": row["runs"],
                }
            )


def run_perf_capture(
    *,
    command_script: Path,
    out_dir: Path,
    perf_bin: str,
    event: str,
    frequency: int,
    call_graph: str | None,
    percent_limit: float,
) -> dict[str, Any]:
    if not command_script.exists():
        raise FileNotFoundError(f"missing command script: {command_script}")
    if shutil.which(perf_bin) is None:
        raise FileNotFoundError(f"perf binary not found: {perf_bin}")

    out_dir.mkdir(parents=True, exist_ok=True)
    perf_data = out_dir / "perf.data"
    report_txt = out_dir / "perf.report.txt"
    summary_csv = out_dir / "perf.summary.csv"
    summary_json = out_dir / "perf.summary.json"
    metadata_json = out_dir / "metadata.json"
    stdout_log = out_dir / "perf-record.stdout.log"
    stderr_log = out_dir / "perf-record.stderr.log"

    commands = load_command_script(command_script)
    record_cmd = build_perf_record_command(
        ["bash", str(command_script)],
        perf_data,
        perf_bin=perf_bin,
        event=event,
        frequency=frequency,
        call_graph=call_graph,
    )

    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    with stdout_log.open("w") as stdout, stderr_log.open("w") as stderr:
        result = subprocess.run(
            record_cmd,
            cwd=Path.cwd(),
            text=True,
            stdout=stdout,
            stderr=stderr,
        )
    elapsed = time.perf_counter() - start
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if result.returncode != 0:
        raise RuntimeError(
            f"perf record failed with exit code {result.returncode}\n"
            f"command: {shlex.join(record_cmd)}\n"
            f"stdout: {stdout_log}\n"
            f"stderr: {stderr_log}"
        )

    report_cmd = build_perf_report_command(
        perf_data,
        perf_bin=perf_bin,
        percent_limit=percent_limit,
    )
    report = subprocess.run(report_cmd, check=True, capture_output=True, text=True)
    report_txt.write_text(report.stdout)

    rows = parse_perf_report(report.stdout, source=perf_data.name)
    summary = aggregate_report_rows(rows)
    write_summary_csv(summary_csv, summary)
    summary_json.write_text(json.dumps(summary, indent=2) + "\n")

    metadata = {
        "command_script": command_script.as_posix(),
        "command_count": len(commands),
        "out_dir": out_dir.as_posix(),
        "perf_data": perf_data.as_posix(),
        "perf_report": report_txt.as_posix(),
        "summary_csv": summary_csv.as_posix(),
        "summary_json": summary_json.as_posix(),
        "event": event,
        "frequency": frequency,
        "call_graph": call_graph,
        "record_command": record_cmd,
        "report_command": report_cmd,
        "stdout_log": stdout_log.as_posix(),
        "stderr_log": stderr_log.as_posix(),
        "elapsed_seconds": elapsed,
        "user_seconds": usage_after.ru_utime - usage_before.ru_utime,
        "system_seconds": usage_after.ru_stime - usage_before.ru_stime,
        "perf_data_bytes": perf_data.stat().st_size,
        "sampled_symbols": len(summary),
    }
    metadata_json.write_text(json.dumps(metadata, indent=2) + "\n")
    return metadata


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run emitted uiCA verification command script under one unified perf record.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--command-script", required=True, help="shell script emitted by verify.py")
    parser.add_argument("--out-dir", required=True, help="directory for perf.data and summaries")
    parser.add_argument("--perf-bin", default="perf", help="perf executable")
    parser.add_argument("--event", default="cpu-clock:u", help="perf event to sample")
    parser.add_argument("--frequency", type=int, default=997, help="sampling frequency")
    parser.add_argument(
        "--call-graph",
        default="dwarf,8192",
        help="perf call graph mode; pass empty string to disable",
    )
    parser.add_argument(
        "--percent-limit",
        type=float,
        default=0.0,
        help="perf report percent-limit for generated summary",
    )
    return parser


def main(argv=None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    call_graph = args.call_graph if args.call_graph else None
    try:
        metadata = run_perf_capture(
            command_script=Path(args.command_script),
            out_dir=Path(args.out_dir),
            perf_bin=args.perf_bin,
            event=args.event,
            frequency=args.frequency,
            call_graph=call_graph,
            percent_limit=args.percent_limit,
        )
    except (FileNotFoundError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"perf run failed: {exc}", file=sys.stderr)
        return 1

    print(f"Recorded {metadata['command_count']} command(s) into {metadata['perf_data']}")
    print(f"Wall time: {metadata['elapsed_seconds']:.3f}s")
    print(f"Report: {metadata['perf_report']}")
    print(f"Summary CSV: {metadata['summary_csv']}")
    print(f"Open interactive view: perf report -i {shlex.quote(metadata['perf_data'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
