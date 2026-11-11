#!/usr/bin/env python3
"""Tables of the padded pass, from what `minimum.py` wrote under runs/pad-cpu<N>-<x>/<job>/ (jobs `attest-pad`
and `bridge-pad`: the private attestation, claim and burn padded as LEZ's wallet pads every private transaction;
one run per container, two containers per job and CPU count). usage: python3 -I summarize_pad.py (writes
summary-pad.json beside this file, prints the Markdown tables). The unpadded medians it compares with are the
first and second passes' (summarize.py, summarize_scale.py: the tables above in README.md).
"""
import json
import pathlib
import statistics
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from summarize_scale import cell, proof_runs, short  # noqa: E402

GIB = 1 << 30
# Unpadded medians, execute_and_prove (s), at 8 and 16 CPUs: README.md, "Result at 8 and at 16 CPUs".
UNPADDED = {"private attestation": (904.3, 497.3), "private claim": (905.4, 463.7), "private burn": (756.3, 389.0)}
WALLET_TRANSFER = (991.7, 488.6)  # LEZ's wallet's deshielded native transfer, same table


def main() -> None:
    runs = []
    for cpus in (8, 16):
        for job in ("attest-pad", "bridge-pad"):
            for d in sorted(HERE.glob(f"runs/pad-cpu{cpus}-*/{job}")):
                if (d / "container.json").exists():
                    for r in proof_runs(d, job):
                        runs.append({"cpus": cpus, "job": job, **r})
    rows, out = [], {"runs": runs, "proofs": {}}
    for label in ("private attestation", "private claim", "private burn"):
        per = {}
        for cpus in (8, 16):
            got = [(r, p) for r in runs if r["cpus"] == cpus for p in r["proofs"] if p["label"] == label]
            per[cpus] = got
        if not per[8] and not per[16]:
            continue
        times = {c: [p["execute_and_prove_s"] for _, p in per[c]] for c in (8, 16)}
        med = {c: (round(statistics.median(times[c]), 1) if times[c] else None) for c in (8, 16)}
        allp = per[8] + per[16]
        cyc = sorted({p["user_cycles"] for _, p in allp})
        segs = sorted({p["segments"] for _, p in allp})
        circuit = [max(p["proven_sessions"], key=lambda s: s["user_cycles"]) for _, p in allp]
        c_cyc = sorted({s["user_cycles"] for s in circuit})
        c_seg = sorted({(s["segments"], s["total_cycles"]) for s in circuit})
        rss = max(int(r["gnu_time"].get("max_rss_kib", 0)) * 1024 for r, _ in allp) / GIB
        cg = max(r["sampled_max"]["cgroup_current_bytes"] for r, _ in allp) / GIB
        receipts = sorted({p["receipt_bytes"] for _, p in allp})
        txs = sorted({p["tx_bytes"] for _, p in allp})
        verified = sum(1 for r, _ in allp if r["settle_exit"] == 0)
        tampered = [t for r, _ in allp for t in r["tamper"]]
        un = UNPADDED[label]
        ratio = {c: (round(med[c] / un[i], 2) if med[c] else None) for i, c in enumerate((8, 16))}
        vs_wallet = {c: (round(med[c] / WALLET_TRANSFER[i], 2) if med[c] else None) for i, c in enumerate((8, 16))}
        out["proofs"][label] = {
            "execute_and_prove_s": times, "median_s": med, "user_cycles": cyc, "segments": segs,
            "circuit_user_cycles": c_cyc, "circuit_segments_total_cycles": c_seg, "peak_rss_gib": round(rss, 2),
            "peak_container_gib": round(cg, 2), "receipt_bytes": receipts, "tx_bytes": txs,
            "settled_with_receipt_verified": verified, "of": len(allp), "tamper_settle_exits": tampered,
            "unpadded_median_s": un, "padded_over_unpadded": ratio, "padded_over_wallet_transfer": vs_wallet,
        }
        rng = lambda xs: f"{xs[0]:,}" if len(xs) == 1 else f"{xs[0]:,}–{xs[-1]:,}"  # noqa: E731
        rows.append(
            f"| {label} | {cell(times[8]) or '–'} | {med[8] if med[8] else '–'} | {cell(times[16]) or '–'} | "
            f"{med[16] if med[16] else '–'} | {rng(cyc)} ({'/'.join(map(str, segs))}) | {rng(c_cyc)} | "
            f"{', '.join(f'{s} ({t:,})' for s, t in c_seg)} | {rss:.2f} / {cg:.2f} | {rng(receipts)} | {rng(txs)} | "
            f"{verified} of {len(allp)} | {ratio[8] or '–'}× / {ratio[16] or '–'}× |"
        )
    print("| proof (padded) | 8 CPUs: `execute_and_prove` (s), each run | median | 16 CPUs: each run | median | "
          "user cycles proven (segments) | LEZ's circuit: user cycles | its segments (cycles with padding) | "
          "peak GiB, r0vm RSS / container | receipt (B) | transaction (B) | settled, receipt verified | "
          "padded / unpadded median, 8 / 16 CPUs |")
    print("|---|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---|---:|")
    print("\n".join(rows))
    print()
    print("| container (tag / job) | CPUs | CPU, cloud region | runs: execute_and_prove (s) | tamper: settle exit |")
    print("|---|---:|---|---|---|")
    for r in runs:
        times = "; ".join(f"{p['label']} {p['execute_and_prove_s']:.1f}" for p in r["proofs"])
        print(f"| `{r['container'].removeprefix('runs/')}` | {r['machine']['nproc']} | {short(r['machine'])} | "
              f"{times} | {r['tamper'] or '–'} |")
    json.dump(out, open(HERE / "summary-pad.json", "w"), indent=1)


if __name__ == "__main__":
    main()
