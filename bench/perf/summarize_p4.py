#!/usr/bin/env python3
"""RFP-021 P.4 and RFP-022 P.4 on containers sized like a mid-range laptop: tables from what
lez/results/p4-minimum/minimum.py wrote (fetched to bench/perf/results/p4-laptop/<tag>/<job>/), parsed with that
pass's own helpers (lez/results/p4-minimum/summarize_scale.py). Usage:
  python3 bench/perf/summarize_p4.py bench/perf/results/p4-laptop > lez/results/perf-p4-laptop.md"""

import pathlib
import statistics
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "lez/results/p4-minimum"))
from summarize_scale import proof_runs  # noqa: E402

GIB = 1 << 30


def main():
    root = pathlib.Path(sys.argv[1])
    runs = []
    for d in sorted(root.glob("laptop-cpu*-*/*")):
        if not (d / "container.json").exists():
            continue
        cpus = int(d.parent.name.split("-")[1][3:])
        for r in proof_runs(d, d.name):
            runs.append({"cpus": cpus, "tag": d.parent.name, "job": d.name, **r})
    print("| proof (padded as LEZ's wallet pads it) | container | `execute_and_prove` (s), each run | median (s) | user cycles proven (segments) | peak GiB: r0vm RSS / container | receipt (B) | transaction (B) | settled, receipt verified |")
    print("|---|---|---|---:|---:|---:|---:|---:|---|")
    for label in ("private attestation", "private claim", "private burn"):
        for cpus in (8, 16):
            got = [(r, p) for r in runs if r["cpus"] == cpus for p in r["proofs"] if p["label"] == label]
            if not got:
                continue
            times = [p["execute_and_prove_s"] for _, p in got]
            cyc = sorted({p["user_cycles"] for _, p in got})
            segs = sorted({p["segments"] for _, p in got})
            rss = max(int(r["gnu_time"].get("max_rss_kib", 0)) * 1024 for r, _ in got) / GIB
            cg = max(r["sampled_max"]["cgroup_current_bytes"] for r, _ in got) / GIB
            rec = sorted({p["receipt_bytes"] for _, p in got})
            txb = sorted({p["tx_bytes"] for _, p in got})
            ok = sum(1 for r, _ in got if r["settle_exit"] == 0)
            print(f"| {label} | {cpus} vCPU, 16 GiB | {' / '.join(f'{t:.1f}' for t in times)} | {statistics.median(times):.1f} | "
                  f"{'–'.join(f'{c:,}' for c in (cyc[0], cyc[-1]) if True) if len(cyc) > 1 else f'{cyc[0]:,}'} ({'/'.join(map(str, segs))}) | "
                  f"{rss:.2f} / {cg:.2f} | {', '.join(f'{x:,}' for x in rec)} | {', '.join(f'{x:,}' for x in txb)} | {ok} of {len(got)} |")
    print()
    print("| container (tag / job) | CPUs | machine | runs: execute_and_prove (s) | tamper: settle exit |")
    print("|---|---:|---|---|---|")
    for r in runs:
        times = "; ".join(f"{p['label']} {p['execute_and_prove_s']:.1f}" for p in r["proofs"])
        print(f"| {r['tag']} / {r['job']} | {r['cpus']} | {r.get('machine', '')} | {times} | {r.get('tamper', '')} |")


if __name__ == "__main__":
    main()
