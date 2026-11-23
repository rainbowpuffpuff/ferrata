#!/usr/bin/env python3
"""The private transactions' circuits from r0vm's session log (RFP-021 P.9; bench/perf/state-actions.sh).

LEZ's API drops the prover's statistics (`prove` in `lee/.../privacy_preserving_transaction/circuit/mod.rs`), but
r0vm logs every session's cycles when run with `RISC0_INFO=1 RUST_LOG=risc0_zkvm=info`, to the stdout it inherits.
`perf_state actions` writes to the same stdout a begin marker before each private transaction of its sweeps and an end
marker after it, naming the cycles of the
transaction's private transitions; the circuit's session is the one right after the last proof of one of those
transitions (the inner proofs come first, then the circuit, then the settlement's public executions). Prints a
markdown table for lez/results/perf-state-actions.md. Standard library only.
Usage: python3 -I bench/perf/circuit-cycles.py LOG
"""
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
CYCLES = re.compile(r"\b(\d+) user cycles\b")
MARK = "perf-state-actions: "


def grouped(n):
    return f"{n:,}"


def main(path):
    rows, sessions, open_mark = [], [], None
    with open(path, encoding="utf-8", errors="replace") as log:
        for raw in log:
            line = ANSI.sub("", raw)
            if MARK in line:
                fields = line.split(MARK, 1)[1].strip().split("|")
                if fields[0] == "begin":
                    open_mark, sessions = (fields[1], int(fields[2])), []
                elif fields[0] == "end":
                    what, k, levels, private = fields[1], int(fields[2]), int(fields[3]), fields[4]
                    if open_mark != (what, k):
                        sys.exit(f"circuit-cycles: unmatched end marker for {what} K={k}")
                    transitions = {int(c) for c in private.split(",") if c}
                    last = max((i for i, c in enumerate(sessions) if c in transitions), default=None)
                    if last is None or last + 1 >= len(sessions):
                        sys.exit(f"circuit-cycles: no circuit session after the transitions for {what} K={k}")
                    rows.append((what, k, levels, sessions[last + 1]))
                    open_mark = None
                continue
            match = CYCLES.search(line)
            if match and open_mark is not None:
                sessions.append(int(match.group(1)))
    if not rows:
        sys.exit("circuit-cycles: no marked private transaction in the log")
    print("\n## The private transactions' circuits, from r0vm's session log\n")
    print(
        "LEZ's API drops the prover's statistics; r0vm logs every session's cycles (`RISC0_INFO=1 "
        "RUST_LOG=risc0_zkvm=info`, as `bench/perf/state-actions.sh` runs `perf_state actions`). For each private "
        "transaction of the sweeps above, its circuit's session is the one right after the last proof of its private "
        "transitions (`bench/perf/circuit-cycles.py`). The circuit hashes the fee payer's membership path one level "
        "at a time; LEZ does not charge these cycles (the sender proves them, RISC Zero dev mode here)."
    )
    print(
        "\n| private transaction | K | membership path (levels) | the circuit's user cycles | over K = 0 | per level |"
        "\n|---|---:|---:|---:|---:|---:|"
    )
    base = {}
    for what, k, levels, cycles in rows:
        if k == 0:
            base[what] = (levels, cycles)
        b_levels, b_cycles = base[what]
        more = cycles - b_cycles
        per = "—" if levels == b_levels else f"{more / (levels - b_levels):,.1f}"
        print(f"| {what} | {grouped(k)} | {levels} | {grouped(cycles)} | {more:+,} | {per} |")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
