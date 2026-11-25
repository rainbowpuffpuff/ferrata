#!/usr/bin/env python3
"""RFP-021 P.4 and RFP-022 P.4 on containers sized like a mid-range laptop: tables from what
lez/results/p4-minimum/minimum.py wrote (fetched to bench/perf/results/p4-laptop/<tag>/<job>/), parsed with that
pass's own helpers (lez/results/p4-minimum/summarize_scale.py). Usage:
  python3 bench/perf/summarize_p4.py bench/perf/results/p4-laptop > lez/results/perf-p4-laptop.md"""

import json
import pathlib
import statistics
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "lez/results/p4-minimum"))
from summarize import parse_log  # noqa: E402
from summarize_scale import machine, short  # noqa: E402

GIB = 1 << 30


def proof_runs(job_dir: pathlib.Path) -> list:
    """summarize_scale.proof_runs for a directory outside lez/results/p4-minimum."""
    c = json.load(open(job_dir / "container.json"))
    m = machine(c)
    out = []
    for rd in sorted(job_dir.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
        r = json.load(open(rd / "run.json"))
        parsed = parse_log(open(rd / "prove.log", errors="replace").read(), "private attestation")
        out.append({"machine": short(m), "run": r["run"], "exit_code": r["exit_code"], "wall_s": r["wall_s"],
                    "gnu_time": r["gnu_time"], "sampled_max": r["sampled"]["max"], "settle_exit": r["settle_exit"],
                    "tamper": [t["settle_exit"] for t in r["tamper"]], "proofs": parsed["proofs"]})
    return out


def owner(root: pathlib.Path):
    """Rows for a laptop run of bench/perf/p4-laptop-owner.sh (OUT_DIR/<job>/prove.log, samples.txt, exit.txt; the
    release job's prove.log ends with ferrata-e2e's JSON record). Memory: the prover's peak RSS (GNU time: the process
    and the r0vm it waits for), r0vm's sampled peak, and the whole machine's used memory (MemTotal - MemAvailable) at
    the job's first sample and at its peak: on a laptop the machine's figure includes the desktop and anything else."""
    import re
    lscpu = (root / "machine.txt").read_text(errors="replace")
    model = re.search(r"^Model name:\s*(.*)$", lscpu, re.M)
    threads = re.search(r"^CPU\(s\):\s*(\d+)$", lscpu, re.M)
    mem = re.search(r"^Mem:\s+(\d+)", lscpu, re.M)
    shape = (f"{model.group(1).strip() if model else '?'}, {threads.group(1) if threads else '?'} threads, "
             f"{int(mem.group(1)) / GIB:.0f} GiB" if mem else "?")
    held = re.search(r"^prover CPUs: (\d+) of \d+", lscpu, re.M)  # P4_CPUS
    if held:
        shape += f", provers held to {held.group(1)} threads"
    print("| proof | machine | wall-clock (s) | user cycles (segments) | peak GiB: prover RSS / r0vm sampled | machine used GiB: "
          "at start / peak | receipt (B) | transaction (B) | settled, receipt verified |")
    print("|---|---|---:|---:|---:|---:|---:|---:|---|")
    for job in ("attest-pad", "bridge-pad", "release"):
        d = root / job
        if not (d / "prove.log").exists():
            continue
        log = (d / "prove.log").read_text(errors="replace")
        rss = re.search(r"Maximum resident set size \(kbytes\): (\d+)", log)
        samples = d / "samples.txt"
        rows = [l.split() for l in (samples.read_text().splitlines() if samples.exists() else [])]
        rows = [r for r in rows if len(r) == 3]
        used = f"{int(rows[0][1]) * 1024 / GIB:.2f} / {max(int(r[1]) for r in rows) * 1024 / GIB:.2f}" if rows else "n/a"
        r0vm = f"{max(int(r[2]) for r in rows) * 1024 / GIB:.2f}" if rows else "n/a"
        rss_s = f"{int(rss.group(1)) * 1024 / GIB:.2f}" if rss else "n/a"
        exit_txt = (d / "exit.txt").read_text() if (d / "exit.txt").exists() else ""
        settled = "yes" if "settle exit 0" in exit_txt else ("not run" if not exit_txt else "no")
        if job == "release":
            recs = [json.loads(l) for l in log.splitlines() if l.startswith("{") and '"wall_s"' in l]
            if not recs:
                print(f"| release, succinct (the holder's proof) | {shape} (laptop) | n/a | n/a | {rss_s} / {r0vm} | {used} | "
                      f"n/a | n/a | {settled}: {exit_txt.strip()} |")
                continue
            r = recs[-1]
            print(f"| release, succinct (the holder's proof) | {shape} (laptop) | {r['wall_s']:.1f} | {r['user_cycles']:,} "
                  f"({r['segments']}) | {rss_s} / {r0vm} | {used} | {r['receipt_bytes']:,} | none (to a relayer) | {settled} |")
            continue
        for pr in parse_log(log, "private attestation")["proofs"]:
            print(f"| {pr['label']} | {shape} (laptop) | {pr['execute_and_prove_s']:.1f} | {pr['user_cycles']:,} ({pr['segments']}) | "
                  f"{rss_s} / {r0vm} | {used} | {pr['receipt_bytes']:,} | {pr['tx_bytes']:,} | {settled} |")


def main():
    if sys.argv[1] == "--owner":
        return owner(pathlib.Path(sys.argv[2]))
    root = pathlib.Path(sys.argv[1])
    runs = []
    for d in sorted(root.glob("laptop-cpu*-*/*")):
        if not (d / "container.json").exists():
            continue
        cpus = int(d.parent.name.split("-")[1][3:])
        for r in proof_runs(d):
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
