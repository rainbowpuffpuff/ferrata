#!/usr/bin/env python3
"""Runs every experiment of RESULTS.md over many seeds and prints/writes the tables.

    python3 experiments.py            # all experiments, 14 worker processes
    python3 experiments.py a h        # only experiments a and h

Writes ../results/raw.jsonl (one line per run) and ../results/tables.md.
"""

from __future__ import annotations

import json
import math
import os
import sys
import time
from collections import defaultdict
from multiprocessing import Pool

from sim import Params, run, threshold

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "results")
H = 3600


def configs():
    """(experiment, label, Params kwargs, seeds)."""
    out = []
    ns = (3, 4, 7, 10)
    for n in ns:
        t = threshold(n)
        out.append(("a", f"n={n} t={t} m={t-1}: honest operation, adversary keys behave", dict(n=n, m=t - 1), 100))
    for n in ns:
        t = threshold(n)
        out.append(("a", f"n={n} t={t} m={t-1}: fork attack, honest sequencers live", dict(n=n, m=t - 1, attack="forkstall", outage=False), 100))
    for n in ns:
        t = threshold(n)
        out.append(("b", f"n={n} t={t} m={t}: orphaned honest block, rule as specified (inscription verified)", dict(n=n, m=t, attack="orphan"), 50))
        out.append(("b", f"n={n} t={t} m={t}: invalid block, rule as specified (inscription verified)", dict(n=n, m=t, attack="invalid"), 50))
        out.append(("b", f"n={n} t={t} m={t}: invalid block, §4.2(4) as written (excluded key not checked)", dict(n=n, m=t, attack="invalid", rule="unverified"), 50))
    for n in ns:
        t = threshold(n)
        out.append(("c", f"n={n} t={t} m={t-1}: no exclusion, honest operation", dict(n=n, m=t - 1, rule="noexcl"), 100))
    for n in ns:
        t = threshold(n)
        for rule, name in (("v2", "exclusion, inscription verified"), ("unverified", "exclusion as written (not checked)"), ("noexcl", "no exclusion")):
            out.append(("c", f"n={n} t={t} m={t-1}: producer partitioned > Δ, {name}", dict(n=n, m=t - 1, attack="partition", rule=rule), 50))
    for n in ns:
        t = threshold(n)
        out.append(("d", f"n={n} t={t} m={t-1}: earlier timestamp rule, one honest clock +Δ", dict(n=n, m=t - 1, attack="skew", rule="ts", skew_key=1), 50))
        out.append(("d", f"n={n} t={t} m={t-1}: same scenario, v2 rule", dict(n=n, m=t - 1, attack="forkstall", outage=False, skew_key=1), 50))
    for delta, dname in ((H, "Δ = 1× k/f (1 h)"), (2 * H, "Δ = 2× k/f (2 h)")):
        for slow, sname in ((None, "normal"), ((2 * H, 28 * H, 0.5), "Bedrock at half rate")):
            out.append(("e", f"n=4 m=2 fastest finalisation, {dname}, {sname}", dict(n=4, m=2, accelerate=True, delta=delta, slowdown=slow), 100))
            out.append(("e", f"n=4 m=0 honest prover only, {dname}, {sname}", dict(n=4, m=0, delta=delta, slowdown=slow), 50))
    out += [
        ("f", "n=2 t=1 m=0: honest operation", dict(n=2, m=0), 100),
        ("f", "n=2 t=1 m=1: orphaned honest block (one key)", dict(n=2, m=1, attack="orphan"), 50),
        ("f", "n=2 t=1 m=1: invalid block, inscription verified", dict(n=2, m=1, attack="invalid"), 50),
        ("f", "n=2 t=1 m=1: invalid block, §4.2(4) as written", dict(n=2, m=1, attack="invalid", rule="unverified"), 50),
        ("f", "n=1 allowSingleKey: honest operation", dict(n=1, m=0, rule="single"), 50),
        ("f", "n=1 allowSingleKey: the key is the adversary's, invalid block", dict(n=1, m=1, rule="single", attack="invalid"), 50),
    ]
    for n in ns:
        t = threshold(n)
        for w in range(0, min(4, n)):
            out.append(("g", f"n={n} t={t}: {w} key(s) withhold", dict(n=n, m=w, attack="withhold" if w else "none"), 30))
    for n in ns:
        t = threshold(n)
        out.append(("h", f"n={n} t={t} m={t-1}: two short forks around an honest outage", dict(n=n, m=t - 1, attack="forkstall"), 100))
    for n in (2, 3, 4):
        out.append(("h", f"n={n} t={threshold(n)} m=0: same, no adversary key (honest producer's block)", dict(n=n, m=0, attack="forkstall", honest_tip=True), 50))
    for back in (0, 1800, 2700, 3300, 3600, 3900, 4500, 5400):
        out.append(("h", f"n=3 m=1: honest back {back // 60} min before the beacon", dict(n=3, m=1, attack="forkstall", back_before_b=back), 50))
    for back in (0, 2700, 3300, 3600, 3900, 4500):
        out.append(("h", f"n=3 m=1: Δ = 3 h, honest back {back // 60} min before the beacon", dict(n=3, m=1, attack="forkstall", delta=3 * H, back_before_b=back), 50))
    for n in ns:
        t = threshold(n)
        out.append(("h", f"n={n} t={t} m={t-1}: two honest nodes eclipsed for Δ, others live", dict(n=n, m=t - 1, attack="forkstall", eclipse=True), 50))
    # ---------------------------------------------------------------- SPEC-v3 follow-up
    V3 = dict(delta=10860, turn_wait=True)              # W = 3 h 01 min + (t - 1) * 15 min
    waits = (("flat Δ = 2 h", dict(delta=7200)), ("flat Δ = 3 h 01", dict(delta=10860)), ("v3 W", V3))
    for n in ns + (13, 16):
        t = threshold(n)
        for wname, wkw in waits:
            seeds = 100 if n <= 10 else 50
            out.append(("i", f"n={n} t={t} m={t-1}: fork attack + turn hogging, {wname}",
                        dict(n=n, m=t - 1, attack="forkstall", outage=False, hog=True, **wkw), seeds))
    for n in ns:
        t = threshold(n)
        out.append(("i", f"n={n} t={t} m={t-1}: + honest turns kept off chain (network), v3 W",
                    dict(n=n, m=t - 1, attack="forkstall", outage=False, hog=True, suppress=True, **V3), 50))
    attacks = []
    for n in ns:
        t = threshold(n)
        attacks.append((f"n={n} t={t} m={t-1}: outage + two forks", dict(n=n, m=t - 1, attack="forkstall")))
    for n in (2, 3, 4):
        attacks.append((f"n={n} t={threshold(n)} m=0: outage + two forks, no adversary key", dict(n=n, m=0, attack="forkstall", honest_tip=True)))
    for n in ns:
        t = threshold(n)
        attacks.append((f"n={n} t={t} m={t-1}: two nodes eclipsed for W", dict(n=n, m=t - 1, attack="forkstall", eclipse=True)))
    for n in ns:
        t = threshold(n)
        attacks.append((f"n={n} t={t} m={t-1}: hogging + honest turns kept off chain", dict(n=n, m=t - 1, attack="forkstall", outage=False, hog=True, suppress=True)))
    for guard in ("off", "freeze-hold", "freeze-lift", "cancel"):
        for name, kw in attacks:
            out.append(("j", f"{name}; guard {guard}", dict(kw, guard=guard, **V3), 50))
    for guard in ("freeze-hold", "cancel-tip"):
        for n in ns:
            t = threshold(n)
            out.append(("j", f"n={n} t={t}: honest operation; guard {guard}", dict(n=n, m=t - 1, guard=guard, **V3), 100))
    for guard in ("freeze-hold", "freeze-lift", "cancel"):
        out.append(("j", f"n=4 t=3: honest operation, Bedrock at half rate; guard {guard}", dict(n=4, m=2, guard=guard, slowdown=(2 * H, 28 * H, 0.5), **V3), 50))
    for guard in ("off", "freeze-lift", "cancel-tip"):
        for n in (7, 10):
            t = threshold(n)
            out.append(("j", f"n={n} t={t} m={t-1}: adversary hogs every turn but signs honest candidates; guard {guard}",
                        dict(n=n, m=t - 1, attack="hog", accelerate=True, guard=guard, **V3), 50))
    for n in ns:
        t = threshold(n)
        out.append(("k", f"n={n} t={t}: honest operation, v3 W = {(10860 + (t - 1) * 900) / H:.2f} h", dict(n=n, m=t - 1, **V3), 100))
    return out


def summarize(out):
    finals = out["finals"]
    honest = [r for r in finals if r["who"] == "honest"]
    return dict(
        finals=len(finals),
        adv_finals=len(finals) - len(honest),
        p1=sum(1 for r in finals if not r["canonical"] and not r["undecided"]),
        undecided=sum(1 for r in finals if r["undecided"]),
        p2=sum(1 for r in finals if not r["kdeep"]),
        invalid=sum(1 for r in finals if not r["valid"]),
        latencies=[r["latency"] for r in honest],
        depths=[r["depth"] for r in finals],
        honest_depths=[r["depth"] for r in honest],
        attack=out["attack"],
        monitor=out.get("monitor", {}),
        delays=[r.get("frozen_delay", 0) for r in honest],
    )


def job(args):
    exp, label, kw, seed = args
    return exp, label, kw, seed, summarize(run(Params(**kw), seed))


def q(xs, p):
    xs = sorted(xs)
    if not xs:
        return float("nan")
    return xs[min(len(xs) - 1, int(p * len(xs)))]


def hours(x):
    return "–" if x != x else f"{x / H:.2f}"


def table(rows):
    head = ("| experiment | runs | finalisations | not canonical (P1) | not k-deep at finalisation (P2) "
            "| invalid tip | runs with a violation | attack succeeded | latency h p10/p50/p90 | min depth |")
    lines = [head, "|" + "---|" * 10]
    for label, rs in rows:
        S = lambda key: sum(r[key] for r in rs)
        lat = [x for r in rs for x in r["latencies"]]
        dep = [x for r in rs for x in r["depths"] if x >= 0]
        viol = sum(1 for r in rs if r["p1"] or r["p2"] or r["invalid"])
        att = [r["attack"]["outcome"] for r in rs]
        succ = f"{att.count('finalised')}/{len(rs)}" if any(a is not None for a in att) else "–"
        lines.append(f"| {label} | {len(rs)} | {S('finals')} | {S('p1')} | {S('p2')} | {S('invalid')} | {viol} | {succ} "
                     f"| {hours(q(lat, .1))} / {hours(q(lat, .5))} / {hours(q(lat, .9))} | {min(dep) if dep else '–'} |")
    return "\n".join(lines)


def depth_table(rows, k=120):
    lines = ["| experiment | finalisations | depth p1 / p10 / p50 | share below k = 120 |", "|---|---|---|---|"]
    for label, rs in rows:
        d = [x for r in rs for x in r["depths"]]
        below = sum(1 for x in d if x < k)
        lines.append(f"| {label} | {len(d)} | {q(d, .01)} / {q(d, .1)} / {q(d, .5)} | {below}/{len(d)} = {below / max(1, len(d)):.3f} |")
    return "\n".join(lines)


def wait_table(rows):
    lines = ["| experiment | runs | attack succeeded | not canonical (P1) | why it failed (count) | view control, max s | empty inscriptions posted, max |",
             "|---|---|---|---|---|---|---|"]
    for label, rs in rows:
        att = [r["attack"] for r in rs]
        reasons = defaultdict(int)
        for a in att:
            if a["outcome"] != "finalised":
                reasons[a["reason"] or str(a["outcome"])] += 1
        why = "; ".join(f"{k} ({v})" for k, v in reasons.items()) or "–"
        succ = sum(1 for a in att if a["outcome"] == "finalised")
        lines.append(f"| {label} | {len(rs)} | {succ}/{len(rs)} | {sum(r['p1'] for r in rs)} | {why} "
                     f"| {max((a.get('capture_time') or 0) for a in att)} | {max((a.get('hog_posts') or 0) for a in att)} |")
    return "\n".join(lines)


def guard_table(rows):
    lines = ["| experiment | runs | honest finalisations | runs with a freeze | freezes (stall / tip trigger) | candidates cancelled | honest finalisations delayed | delay p50 / max (h) | honest candidates lost | frozen time per run (h) |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    for label, rs in rows:
        mon = [r["monitor"] for r in rs]
        d = [x for r in rs for x in r["delays"] if x > 0]
        hon = sum(len(r["delays"]) for r in rs)
        lines.append(f"| {label} | {len(rs)} | {hon} | {sum(1 for m in mon if m.get('trips'))} "
                     f"| {sum(m.get('stall_trips', 0) for m in mon)} / {sum(m.get('tip_trips', 0) for m in mon)} "
                     f"| {sum(m.get('cancelled', 0) for m in mon)} | {len(d)} "
                     f"| {hours(q(d, .5))} / {hours(max(d)) if d else '–'} | {sum(m.get('lost', 0) for m in mon)} "
                     f"| {sum(m.get('frozen_time', 0) for m in mon) / len(rs) / H:.2f} |")
    return "\n".join(lines)


def withhold_table(rows):
    lines = ["| committee | finalisations per 24 h (mean) | runs with none | latency p50 (h) |", "|---|---|---|---|"]
    for label, rs in rows:
        f = [r["finals"] for r in rs]
        lat = [x for r in rs for x in r["latencies"]]
        lines.append(f"| {label} | {sum(f) / len(f):.1f} | {sum(1 for x in f if x == 0)}/{len(f)} | {hours(q(lat, .5))} |")
    return "\n".join(lines)


def attack_table(rows):
    lines = ["| experiment | attack succeeded | P1 violations | P2 violations | why it failed (count) | view control, max s | private Bedrock blocks |",
             "|---|---|---|---|---|---|---|"]
    for label, rs in rows:
        att = [r["attack"] for r in rs]
        reasons = defaultdict(int)
        for a in att:
            if a["outcome"] != "finalised":
                reasons[a["reason"] or str(a["outcome"])] += 1
        why = "; ".join(f"{k} ({v})" for k, v in reasons.items()) or "–"
        cap = max((a.get("capture_time") or 0) for a in att)
        pb = max((a.get("private_blocks") or 0) for a in att)
        succ = sum(1 for a in att if a["outcome"] == "finalised")
        lines.append(f"| {label} | {succ}/{len(rs)} | {sum(r['p1'] for r in rs)} | {sum(r['p2'] for r in rs)} | {why} | {cap} | {pb} |")
    return "\n".join(lines)


# ------------------------------------------------------------------ analytics
def log_binom_pmf(N, i, p):
    return math.lgamma(N + 1) - math.lgamma(i + 1) - math.lgamma(N - i + 1) + i * math.log(p) + (N - i) * math.log1p(-p)


def p_fewer_than(k, N, p):
    """P(Binomial(N, p) < k), summed exactly in log space."""
    logs = [log_binom_pmf(N, i, p) for i in range(0, k)]
    m = max(logs)
    return math.exp(m) * sum(math.exp(x - m) for x in logs)


def analytics():
    k, f = 120, 1 / 30
    lines = ["P(fewer than k = 120 Bedrock blocks within Δ), slots of 1 s, leader rate per slot f "
             "(Binomial(Δ, f) lower tail; forks and withheld blocks only lower the growth):", "",
             "| Δ | f = 1/30 | f = 1/60 (half the participation) |", "|---|---|---|"]
    for hrs in (1, 1.5, 2, 3, 4):
        N = int(hrs * 3600)
        lines.append(f"| {hrs} h | {p_fewer_than(k, N, f):.3g} | {p_fewer_than(k, N, f / 2):.3g} |")
    lines += ["", "Smallest window Δ_g with P(fewer than 120 blocks) below a target, f = 1/30:", "",
              "| target | Δ_g |", "|---|---|"]
    for target in (1e-6, 1e-9, 1e-12, 1e-18):
        N = 3600
        while p_fewer_than(k, N, f) >= target:
            N += 60
        lines.append(f"| {target:g} | {N} s = {N / 3600:.2f} h |")
    lines += ["", "Probability that a Bedrock adversary with share q of the stake ever overtakes a lead of z blocks "
              "(gambler's ruin, (q/(1-q))^z; the classic bound, ignoring Praos details):", "",
              "| z | q = 0.10 | q = 0.25 | q = 0.40 | q = 0.45 |", "|---|---|---|---|---|"]
    for z in (30, 60, 90, 120):
        lines.append(f"| {z} | " + " | ".join(f"{(qq / (1 - qq)) ** z:.2g}" for qq in (0.10, 0.25, 0.40, 0.45)) + " |")
    return "\n".join(lines)


def main():
    wanted = set(sys.argv[1:])
    cfgs = [c for c in configs() if not wanted or c[0] in wanted]
    jobs = [(exp, label, kw, seed) for exp, label, kw, n in cfgs for seed in range(1, n + 1)]
    t0 = time.time()
    with Pool(14) as pool:
        results = pool.map(job, jobs, chunksize=4)
    os.makedirs(OUT, exist_ok=True)
    suffix = "" if not wanted else "-" + "".join(sorted(wanted))
    with open(os.path.join(OUT, f"raw{suffix}.jsonl"), "w") as fh:
        for exp, label, kw, seed, s in results:
            fh.write(json.dumps(dict(exp=exp, label=label, params=kw, seed=seed, **s)) + "\n")
    groups = defaultdict(lambda: defaultdict(list))
    order = defaultdict(list)
    for exp, label, kw, seed, s in results:
        if label not in groups[exp]:
            order[exp].append(label)
        groups[exp][label].append(s)
    md = [f"Generated by sim/experiments.py in {time.time() - t0:.0f} s ({len(jobs)} runs).", ""]
    for exp in sorted(groups):
        rows = [(label, groups[exp][label]) for label in order[exp]]
        md.append(f"## ({exp})")
        md.append(table(rows))
        if exp == "i":
            md += ["", wait_table(rows)]
        if exp == "j":
            att = [(l, rs) for l, rs in rows if any(r["attack"].get("outcome") is not None for r in rs)]
            md += ["", wait_table(att), "", guard_table([(l, rs) for l, rs in rows if l not in {a for a, _ in att}])]
        if exp in ("a", "b", "c", "d", "h", "f"):
            att = [(l, rs) for l, rs in rows if any(r["attack"]["outcome"] is not None for r in rs)]
            if att:
                md += ["", attack_table(att)]
        if exp == "e":
            md += ["", depth_table(rows)]
        if exp == "g":
            md += ["", withhold_table(rows)]
        md.append("")
    md += ["## Analytic", analytics(), ""]
    text = "\n".join(md)
    with open(os.path.join(OUT, f"tables{suffix}.md"), "w") as fh:
        fh.write(text)
    print(text)


if __name__ == "__main__":
    main()
