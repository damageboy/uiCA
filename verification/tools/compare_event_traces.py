#!/usr/bin/env python3
"""Compare two event traces produced by `uiCA.py -eventTrace` or the Rust port.

Reports the first line where the traces diverge and a short context window
around it. Exits non-zero when a divergence is found.

Trace format (see generateEventTrace in uiCA.py):
    C<cycle> <EV> instr=<id> rnd=<n> lam=<n> fused=<n> uop=<n> [port=<P>] [source=<S>]

Events are sorted by (cycle, kind_order, instrID, rnd, lam, fused, uop) on
the emitter side, so line-by-line comparison is meaningful.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def read_trace(path: Path) -> list[str]:
    with open(path, encoding="utf-8") as f:
        return [line.rstrip("\n") for line in f]


def find_first_divergence(a: list[str], b: list[str]) -> int | None:
    n = min(len(a), len(b))
    for i in range(n):
        if a[i] != b[i]:
            return i
    if len(a) != len(b):
        return n
    return None


def print_context(label: str, trace: list[str], idx: int, window: int) -> None:
    start = max(0, idx - window)
    end = min(len(trace), idx + window + 1)
    print(f"--- {label} [{start}..{end}) ---")
    for i in range(start, end):
        marker = ">>>" if i == idx else "   "
        print(f"{marker} {i:6d} {trace[i]}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("oracle", type=Path, help="oracle trace (Python)")
    parser.add_argument("candidate", type=Path, help="candidate trace (Rust)")
    parser.add_argument(
        "--context",
        type=int,
        default=5,
        help="lines of context around first divergence (default: 5)",
    )
    args = parser.parse_args()

    oracle = read_trace(args.oracle)
    candidate = read_trace(args.candidate)

    idx = find_first_divergence(oracle, candidate)
    if idx is None:
        print(f"match: {len(oracle)} events identical")
        return 0

    print(f"divergence at line {idx} (1-based: {idx + 1})")
    print(f"oracle    length = {len(oracle)}")
    print(f"candidate length = {len(candidate)}")
    print()
    print_context("oracle", oracle, idx, args.context)
    print()
    print_context("candidate", candidate, idx, args.context)
    return 1


if __name__ == "__main__":
    sys.exit(main())
