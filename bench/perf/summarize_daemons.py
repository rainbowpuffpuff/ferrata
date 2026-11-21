#!/usr/bin/env python3
"""RFP-022 P.6: the two Ethereum-side daemons' CPU, memory and bandwidth from a bench/perf/daemons-anvil.sh run
(GNU time per daemon, proc_sample.py's samples every 5 s, each daemon's JSON-RPC bytes through its own counting
proxy, the phase marks). Usage: python3 -I bench/perf/summarize_daemons.py RUN_DIR"""

import json
import re
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from summarize_gate import gnu_time  # noqa: E402


def main():
    run = Path(sys.argv[1])
    phases = [(float(l.split("\t")[0]), l.split("\t", 1)[1]) for l in (run / "phases.tsv").read_text().splitlines()]
    samples = {}
    for line in (run / "samples.tsv").read_text().splitlines()[1:]:
        f = line.split("\t")
        samples.setdefault(f[1], []).append((float(f[0]), float(f[3]), int(f[4])))
    machine = (run / "machine.txt").read_text().strip()
    print(f"## Run `{run.name}`\n\n```\n{machine}\n```\n")
    print("Phases: " + "; ".join(f"{label} at +{t - phases[0][0]:.0f} s" for t, label in phases) + ".\n")
    print("| daemon | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | JSON-RPC to the node: bytes up / down | per day at this rate: CPU s / MB |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for name in sorted(samples):
        tm = gnu_time(run / f"time-{name}.txt")
        rpc = json.loads((run / f"rpc-{name}.json").read_text()) if (run / f"rpc-{name}.json").exists() else {}
        cpu, wall = tm.get("user", 0) + tm.get("sys", 0), tm.get("wall", 0)
        med = statistics.median(x[2] for x in samples[name]) / 1024
        up, down = rpc.get("up", 0), rpc.get("down", 0)
        day = 86400 / wall if wall else 0
        print(f"| {name} | {wall:.0f} | {cpu:.2f} | {cpu / wall if wall else 0:.5f} | {tm.get('rss_kb', 0) / 1024:.1f} | {med:.1f} | "
              f"{up:,} / {down:,} | {cpu * day:.0f} / {(up + down) * day / 1e6:.1f} |")
    print("\nPer phase (CPU seconds between the phase marks, from the samples):\n")
    names = ["idle (before)", "activity", "idle (after)"] if len(phases) == 4 else [label for _, label in phases[:-1]]
    print("| daemon | " + " | ".join(names) + " |")
    print("|---|" + "---:|" * (len(phases) - 1))
    for name in sorted(samples):
        s = samples[name]
        cells = []
        for (t0, _), (t1, _) in zip(phases, phases[1:]):
            inside = [x for x in s if t0 <= x[0] <= t1]
            if len(inside) >= 2:
                c = inside[-1][1] - inside[0][1]
                cells.append(f"{c:.2f} s over {inside[-1][0] - inside[0][0]:.0f} s ({c / (inside[-1][0] - inside[0][0]):.4f} of a core)")
            else:
                cells.append("—")
        print(f"| {name} | " + " | ".join(cells) + " |")
    log = (run / "log.txt").read_text() if (run / "log.txt").exists() else ""
    m = re.search(r"activity end \(final state (\S+)…; wanted (\S+)…\)", log)
    if m:
        print(f"\nThe activity: an honest candidate, checked by `monitor`, finalised by the relayer after the vault's wait; "
              f"final state {m.group(1)}… (wanted {m.group(2)}…: {'reached' if m.group(1) == m.group(2) else 'NOT reached'}).")


if __name__ == "__main__":
    main()
