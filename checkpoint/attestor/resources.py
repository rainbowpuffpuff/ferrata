#!/usr/bin/env python3
"""The compute resources of the rule (B) attestor (RFP-022 Performance #6: "Document the compute resources (CPU, RAM,
time) required to run any off-chain component the design requires"), from a zone-bridge run's records.

    python3 -I checkpoint/attestor/resources.py RUN_DIR [--round-s S] [--json]

Reads RUN_DIR/zone/attestor-<i>/resources.tsv, which checkpoint/attestor/zone-bridge.sh writes for every round of every
attestor: the UTC time, GNU time's wall seconds, user and system CPU seconds and peak resident set (KiB), the bytes of
the node's and the LEZ node's answers the round read (its --save directory), and the round's exit status. Prints, per
attestor and for all of them, the rounds and how many signed, the median and largest wall time, the CPU seconds per
round (median, largest), the peak resident set (median, largest) and the bytes read per round (median, largest), and,
at one round every S seconds (default 60, the run's ZB_ROUND_S), the share of one core and the bytes a day (derived).
Exits 1 when a run has no resources.tsv or a malformed row.
"""
import json
import os
import statistics
import sys


def parse(path):
    rows = []
    with open(path) as f:
        for n, line in enumerate(f, 1):
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 7:
                raise ValueError(f"{path}:{n}: {len(parts)} fields, want 7")
            _, wall, user, sys_s, rss, nbytes, rc = parts
            rows.append({"wall": float(wall), "cpu": float(user) + float(sys_s), "rss_kib": int(rss),
                         "bytes": int(nbytes), "ok": rc == "0"})
    return rows


def summary(rows, round_s):
    walls = [r["wall"] for r in rows]
    cpus = [r["cpu"] for r in rows]
    rss = [r["rss_kib"] for r in rows]
    nb = [r["bytes"] for r in rows]
    per_day = 86400 / round_s
    return {
        "rounds": len(rows),
        "signed": sum(r["ok"] for r in rows),
        "wall_s_median": round(statistics.median(walls), 3), "wall_s_max": round(max(walls), 3),
        "cpu_s_median": round(statistics.median(cpus), 3), "cpu_s_max": round(max(cpus), 3),
        "rss_mib_median": round(statistics.median(rss) / 1024, 1), "rss_mib_max": round(max(rss) / 1024, 1),
        "bytes_median": int(statistics.median(nb)), "bytes_max": max(nb),
        # derived: one round every round_s seconds, each costing the run's mean CPU and reading its mean bytes
        "core_share_at_cadence": round(statistics.mean(cpus) / round_s, 5),
        "mib_per_day_at_cadence": round(statistics.mean(nb) * per_day / 2**20, 1),
    }


def main(argv):
    import argparse
    ap = argparse.ArgumentParser(add_help=True, usage=__doc__)
    ap.add_argument("run")
    ap.add_argument("--round-s", type=float, default=60.0)
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv[1:])
    if a.round_s <= 0:
        print("resources.py: --round-s must be positive", file=sys.stderr)
        return 2
    run, round_s = a.run, a.round_s
    zone = os.path.join(run, "zone")
    names = sorted(d for d in os.listdir(zone) if d.startswith("attestor-")) if os.path.isdir(zone) else []
    per, every = {}, []
    for d in names:
        p = os.path.join(zone, d, "resources.tsv")
        if not os.path.exists(p):
            continue
        try:
            rows = parse(p)
        except ValueError as e:
            print(f"resources.py: {e}", file=sys.stderr)
            return 1
        if rows:
            per[d] = summary(rows, round_s)
            every += rows
    if not every:
        print(f"resources.py: no zone/attestor-*/resources.tsv rows under {run}", file=sys.stderr)
        return 1
    per["all attestors"] = summary(every, round_s)
    if a.json:
        print(json.dumps(per, indent=2))
        return 0
    print(f"| attestor | rounds (signed) | wall s median / max | CPU s a round median / max | peak RSS MiB median / max "
          f"| bytes read a round median / max | share of one core, one round every {round_s:g} s (derived) "
          f"| MiB read a day (derived) |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for k, s in per.items():
        print(f"| {k} | {s['rounds']} ({s['signed']}) | {s['wall_s_median']} / {s['wall_s_max']} | {s['cpu_s_median']} / "
              f"{s['cpu_s_max']} | {s['rss_mib_median']} / {s['rss_mib_max']} | {s['bytes_median']:,} / {s['bytes_max']:,} "
              f"| {s['core_share_at_cadence']} | {s['mib_per_day_at_cadence']:,} |")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
