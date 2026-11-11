"""Summarise a `just bench-user` run in the format `just m0 --p4 DIR` reads (summary-pad.json), from what
lez/results/p4-minimum/minimum.py wrote (DIR/<tag>/<job>/container.json and run-<i>/{run.json,prove.log}).

    python3 -I scripts/bench_user_summary.py DIR        writes DIR/summary-pad.json, prints one line per proof

The logs are parsed by lez/results/p4-minimum's own parser (summarize.parse_log, summarize_scale.machine), so the
figures mean what that README's tables mean. Standard library only.
"""

import json
import os
import pathlib
import statistics
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "lez" / "results" / "p4-minimum"))
from summarize import parse_log  # noqa: E402
from summarize_scale import machine  # noqa: E402

GIB = 1 << 30
LABELS = {"attest-pad": "private attestation", "bridge-pad": None}  # bridge-pad: the claim and the burn


def runs_of(job_dir: pathlib.Path, job: str) -> list:
    c = json.load(open(job_dir / "container.json"))
    cpus = int(float(c.get("declared", {}).get("modal_cpu_request_and_limit", 0)))
    out = []
    for rd in sorted(job_dir.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
        r = json.load(open(rd / "run.json"))
        parsed = parse_log(open(rd / "prove.log", errors="replace").read(), LABELS.get(job) or "private claim")
        out.append({"cpus": cpus, "job": job, "machine": machine(c), "run": r["run"], "exit_code": r["exit_code"],
                    "wall_s": r["wall_s"], "gnu_time": r["gnu_time"], "sampled_max": r["sampled"]["max"],
                    "settle_exit": r["settle_exit"], "tamper": [t["settle_exit"] for t in r["tamper"]],
                    "proofs": parsed["proofs"]})
    return out


def main(d: str) -> None:
    base = pathlib.Path(d)
    runs = []
    for job in ("attest-pad", "bridge-pad"):
        for jd in sorted(base.glob(f"*/{job}")):
            if (jd / "container.json").exists():
                runs += runs_of(jd, job)
    out = {"runs": runs, "proofs": {}}
    for label in ("private attestation", "private claim", "private burn"):
        got = [(r, p) for r in runs for p in r["proofs"] if p["label"] == label]
        if not got:
            continue
        by = {}
        for r, p in got:
            by.setdefault(str(r["cpus"]), []).append(p["execute_and_prove_s"])
        out["proofs"][label] = {
            "execute_and_prove_s": by,
            "median_s": {c: round(statistics.median(v), 1) for c, v in by.items()},
            "user_cycles": sorted({p["user_cycles"] for _, p in got}),
            "peak_rss_gib": round(max(int(r["gnu_time"].get("max_rss_kib", 0)) * 1024 for r, _ in got) / GIB, 2),
            "peak_container_gib": round(max(r["sampled_max"]["cgroup_current_bytes"] for r, _ in got) / GIB, 2),
            "receipt_bytes": sorted({p["receipt_bytes"] for _, p in got}),
            "tx_bytes": sorted({p["tx_bytes"] for _, p in got}),
            "settled_with_receipt_verified": sum(1 for r, _ in got if r["settle_exit"] == 0),
            "of": len(got),
            "tamper_settle_exits": [t for r, _ in got for t in r["tamper"]],
            "machines": sorted({json.dumps(r["machine"], sort_keys=True) for r, _ in got}),
        }
        q = out["proofs"][label]
        print(f"{label}: {q['median_s']} s (execute_and_prove), peak {max(q['peak_rss_gib'], q['peak_container_gib'])} GiB, "
              f"{q['settled_with_receipt_verified']}/{q['of']} settled with the receipt verified, tampered: {q['tamper_settle_exits']}")
    json.dump(out, open(base / "summary-pad.json", "w"), indent=1)
    # The release receipt (minimum.py's release_prove): summary-scale.json's "release" part, as m0.py reads it.
    rel = {}
    for jd in sorted(base.glob("*/release")):
        if not (jd / "container.json").exists():
            continue
        c = json.load(open(jd / "container.json"))
        cpus = str(int(float(c.get("declared", {}).get("modal_cpu_request_and_limit", 0))))
        for rd in sorted(jd.glob("run-*"), key=lambda p: int(p.name.split("-")[1])):
            r = json.load(open(rd / "run.json"))
            pr = r.get("prove") or {}
            if pr.get("exit_code") != 0 or not pr.get("record"):
                continue
            rel.setdefault(cpus, []).append({"machine": machine(c), "run": r["run"], "succinct_verified": r.get("succinct_verified"),
                                             "prove": {"s": pr["record"]["wall_s"], "gnu_time": pr["gnu_time"],
                                                       "sampled_max": pr["sampled"]["max"], "record": pr["record"]},
                                             "p254": {k: (r.get("p254") or {}).get(k) for k in ("exit_code", "wall_s")}})
            print(f"release receipt at {cpus} CPUs: {pr['record']['wall_s']:.1f} s, verified {r.get('succinct_verified')}, "
                  f"peak {int(pr['gnu_time'].get('max_rss_kib', 0)) / 2**20:.2f} GiB")
    if rel:
        json.dump({"release": rel}, open(base / "summary-scale.json", "w"), indent=1)


if __name__ == "__main__":
    main(sys.argv[1])
