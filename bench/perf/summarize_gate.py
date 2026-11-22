#!/usr/bin/env python3
"""Tables of one bench/perf/gate-sequencer.sh run: RFP-021 R.9 (gas on refused steps against applied steps), R.10
(each honest daemon's share of the fees; latency from an update's appearance to its application) and P.6 (each
daemon's CPU, memory and bandwidth). Usage: python3 -I bench/perf/summarize_gate.py RUN_DIR [RUN_DIR ...]"""

import json
import re
import statistics
import sys
from datetime import datetime, timezone
from pathlib import Path


STEP = {"init": "Init", "open": "Open", "miller": "Miller", "adopt0": "adopt 1/2", "adopt1": "adopt 2/2", "finish": "Finish"}


def g(n):
    return f"{int(round(n)):,}"


def unix(utc):
    return datetime.strptime(utc, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()


def journal(path):
    rows = []
    if not path.exists():
        return rows
    for line in path.read_text().splitlines():
        f = line.split("\t")
        if len(f) < 12:
            continue
        rows.append(dict(round=int(f[0]), step=f[1], job=f[2], slot=int(f[3]), status=f[4], exit=f[5],
                         gas_exec=int(f[6] or 0), gas_stor=int(f[7] or 0), fee=int(f[8] or 0), block=f[9],
                         t=unix(f[10]), label=f[11]))
    return rows


def gnu_time(path):
    if not path.exists():
        return {}
    t = path.read_text()
    out = {}
    for key, pat in [("user", r"User time \(seconds\): ([\d.]+)"), ("sys", r"System time \(seconds\): ([\d.]+)"),
                     ("rss_kb", r"Maximum resident set size \(kbytes\): (\d+)"),
                     ("wall", r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([\d:.]+)")]:
        m = re.search(pat, t)
        if m:
            v = m.group(1)
            if key == "wall":
                parts = [float(p) for p in v.split(":")]
                out[key] = sum(p * 60 ** i for i, p in enumerate(reversed(parts)))
            else:
                out[key] = float(v)
    return out


def pct(xs, q):
    """The value at index round(q (n - 1)) of the sorted values (Python's round: halves to even); not the nearest rank."""
    xs = sorted(xs)
    if not xs:
        return float("nan")
    i = min(len(xs) - 1, int(round(q * (len(xs) - 1))))
    return xs[i]


def summarize(run):
    run = Path(run)
    key = {"run": run.name}
    names = sorted(p.name[len("state-"):] for p in run.glob("state-*") if p.is_dir())
    J = {n: journal(run / f"state-{n}" / "journal.tsv") for n in names}
    reveals = [l.split("\t") for l in (run / "reveals.tsv").read_text().splitlines()[1:]]
    machine = (run / "machine.txt").read_text().strip() if (run / "machine.txt").exists() else ""
    done = (run / "done.txt").read_text().strip() if (run / "done.txt").exists() else "?"
    print(f"## Run `{run.name}`\n\n```\n{machine}\n{done}\n```\n")

    # R.9: gas on refused and applied steps, per daemon.
    print("| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    tot = {}
    for n in names:
        rows = J[n]
        a = [r for r in rows if r["status"] == "applied"]
        b = [r for r in rows if r["status"] == "refused"]
        c = [r for r in rows if r["status"] == "rejected"]
        t = dict(ae=sum(r["gas_exec"] for r in a), be=sum(r["gas_exec"] for r in b), as_=sum(r["gas_stor"] for r in a),
                 bs=sum(r["gas_stor"] for r in b), af=sum(r["fee"] for r in a), bf=sum(r["fee"] for r in b))
        tot[n] = t
        print(f"| {n} | {len(a)} | {len(b)} | {len(c)} | {g(t['ae'])} | {g(t['be'])} | {g(t['as_'])} | {g(t['bs'])} | {g(t['af'])} | {g(t['bf'])} |")
    honest = [n for n in names if n.startswith("honest")]
    if honest:
        ae = sum(tot[n]["ae"] for n in honest); be = sum(tot[n]["be"] for n in honest)
        af = sum(tot[n]["af"] for n in honest); bf = sum(tot[n]["bf"] for n in honest)
        key["r9"] = be / ae if ae else None
        print(f"\n**R.9:** the honest daemons' execution gas on refused steps / on applied steps = {g(be)} / {g(ae)} = "
              f"**{be / ae:.4f}** (fees: {bf / af:.4f}; the gate: at most 0.10)." if ae else "\nno applied step")
        if len(honest) == 2 and af + bf:
            paid = {n: tot[n]["af"] + tot[n]["bf"] for n in honest}
            share = paid[honest[0]] / (paid[honest[0]] + paid[honest[1]])
            key["share"] = share
            print(f"**R.10:** {honest[0]}'s share of the honest daemons' fees = **{share:.3f}** "
                  f"({honest[1]} {1 - share:.3f}; the gate: 0.35 to 0.65).")
    # Each step applied once.
    applied = {}
    for n in names:
        for r in J[n]:
            if r["status"] == "applied":
                applied.setdefault((r["job"], r["step"], r["label"]), []).append(n)
    twice = {k: v for k, v in applied.items() if len(v) > 1}
    jobs = {k[0] for k in applied}
    print(f"\nJobs with an applied step: {len(jobs)}; steps applied: {len(applied)}; applied more than once: {len(twice)}.")
    # Refused steps: which step, what it cost, and whether it was sent before the applied one was in a block (the same
    # block: concurrent submission, invisible to a re-check) or after (a re-check that missed a final step).
    blk = {(r["job"], r["step"]): r["block"] for n in names for r in J[n] if r["status"] == "applied"}
    ref = [(n, r) for n in names for r in J[n] if r["status"] == "refused"]
    if ref:
        by = {}
        for n, r in ref:
            k = (n.startswith("honest"), r["step"], r["exit"])
            e = by.setdefault(k, [0, 0, 0])
            e[0] += 1
            e[1] += r["gas_exec"]
            e[2] += 1 if blk.get((r["job"], r["step"])) == r["block"] else 0
        print("\n| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |")
        print("|---|---|---:|---:|---:|---:|")
        for (h, step, code), (c, gas, same) in sorted(by.items(), key=lambda kv: -kv[1][1]):
            print(f"| {'honest daemons' if h else 'adversary'} | {step} | {code} | {c} | {g(gas)} | {same} of {c} |")
    tot_a = sum(r["gas_exec"] for n in names for r in J[n] if r["status"] == "applied")
    tot_r = sum(r["gas_exec"] for n in names for r in J[n] if r["status"] == "refused")
    key["total_gas"] = tot_a + tot_r
    key["revealed"] = len(reveals)
    key.update(jobs=len(jobs), twice=len(twice))

    # Job fates. A job is completed when its last step is applied (`Finish`; for `Init`, adoption part 2/2). Otherwise
    # it is abandoned: another job moved the instance's store first, and the job id binds the store, so its remaining
    # steps can no longer apply. Its applied steps are spent gas as much as a refused step is.
    def last_step(r):
        return r["step"] == "finish" or r["label"].endswith("init adopt 2/2")
    done_jobs = {r["job"] for n in names for r in J[n] if r["status"] == "applied" and last_step(r)}
    gas = {n: dict(done=0, aband=0, ref=0) for n in names}
    aband = {}
    for n in names:
        for r in J[n]:
            if r["status"] == "applied":
                gas[n]["done" if r["job"] in done_jobs else "aband"] += r["gas_exec"]
                if r["job"] not in done_jobs:
                    aband.setdefault(r["job"], []).append(STEP.get(r["step"], r["step"]))
            elif r["status"] == "refused":
                gas[n]["ref"] += r["gas_exec"]
    print("\n| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |")
    print("|---|---:|---:|---:|---:|")
    for n in names:
        x = gas[n]
        print(f"| {n} | {g(x['done'])} | {g(x['aband'])} | {g(x['ref'])} | {g(sum(x.values()))} |")
    D = sum(x["done"] for x in gas.values()); A = sum(x["aband"] for x in gas.values()); R = sum(x["ref"] for x in gas.values())
    print(f"| all daemons | {g(D)} | {g(A)} | {g(R)} | {g(D + A + R)} |")
    kinds = {}
    for steps in aband.values():
        k = " + ".join(steps)
        kinds[k] = kinds.get(k, 0) + 1
    print(f"\nJobs: {len(jobs)} with an applied step, {len(jobs) - len(aband)} completed, **{len(aband)} abandoned**"
          + (f" ({'; '.join(f'{c} with {k} applied' for k, c in sorted(kinds.items(), key=lambda kv: -kv[1]))})." if aband else "."))
    key.update(completed=len(jobs) - len(aband), abandoned=len(aband), gas_done=D, gas_aband=A, gas_ref=R)
    if D:
        key["mult"] = (D + A + R) / D
        key["mult_applied"] = (tot_a + tot_r) / tot_a
        print(f"\nAll daemons together: the updates cost **{key['mult']:.3f}** times the execution gas of the completed jobs "
              f"(total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused "
              f"steps as waste, (applied + refused) / applied = {key['mult_applied']:.3f}.")
    if honest:
        hd = sum(gas[n]["done"] for n in honest); ha = sum(gas[n]["aband"] for n in honest); hr = sum(gas[n]["ref"] for n in honest)
        if hd:
            key["waste"] = (ha + hr) / hd
            print(f"**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = "
                  f"{g(ha + hr)} / {g(hd)} = **{key['waste']:.4f}** (A3's ratio above counts refused steps only).")

    # Latency: an entry's appearance to the first applied Finish whose finalized slot reaches it.
    fin = sorted((r["t"], r["slot"], n) for n in names for r in J[n] if r["step"] == "finish" and r["status"] == "applied")
    lat, who, own = [], {}, 0
    for e in reveals:
        slot, t0, kind = int(e[4]), float(e[7]), e[2]
        hit = next(((t, s, n) for t, s, n in fin if s >= slot and t >= t0 - 1), None)
        if hit and kind == "finality":
            lat.append(hit[0] - t0)
            who[hit[2]] = who.get(hit[2], 0) + 1
            own += hit[1] == slot
    if lat:
        key.update(lat_med=statistics.median(lat), lat_p90=pct(lat, 0.9), lat_n=len(lat), who=dict(who), own=own,
                   later=len(lat) - own, fin_revealed=sum(1 for e in reveals if e[2] == "finality"))
        print(f"\n**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block "
              f"final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), "
              f"{len(lat)} of {key['fin_revealed']} revealed finality updates covered ({own} by their own job, {len(lat) - own} "
              f"by a later update's): median **{statistics.median(lat):.0f} s**, p90 {pct(lat, 0.9):.0f} s (the value at index round(0.9 (n − 1)) of the sorted latencies), "
              f"min {min(lat):.0f} s, max {max(lat):.0f} s; finished by {', '.join(f'{k} {v}' for k, v in sorted(who.items()))}.")

    # The replay's rate against mainnet's.
    tr = [float(e[7]) for e in reveals]
    fs = [int(e[4]) for e in reveals]
    if len(tr) > 1:
        step = statistics.median(b - a for a, b in zip(tr, tr[1:]))
        rec = [(b - a) * 12 / 60 for a, b in zip(fs, fs[1:])]
        key.update(reveal_step=step, recorded_min=statistics.median(rec))
        print(f"\n**Replay rate:** {len(reveals)} recorded updates revealed one every {step:.0f} s (median); on mainnet they "
              f"are {statistics.median(rec):.1f} min apart (median of the finalized headers' spacing; {min(rec):.1f} to "
              f"{max(rec):.1f} min), so the replay runs about {statistics.median(rec) * 60 / step:.0f} times faster than "
              f"mainnet" + (f"; the reveal step is shorter than this run's median latency ({key['lat_med']:.0f} s), so jobs overlap by construction." if key.get("lat_med", 0) > step else f"; this run's median latency ({key.get('lat_med', float('nan')):.0f} s) is shorter than the reveal step."))

    # P.6: resources.
    stats = json.loads((run / "beacon-stats.json").read_text()) if (run / "beacon-stats.json").exists() else {"per_port": {}}
    samples = {}
    if (run / "samples.tsv").exists():
        for line in (run / "samples.tsv").read_text().splitlines()[1:]:
            f = line.split("\t")
            samples.setdefault(f[1], []).append((float(f[0]), float(f[3]), int(f[4]), int(f[5])))
    print("\n| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    per_round, shares, ups, downs = [], [], [], []
    for n in names:
        tm = gnu_time(run / f"time-{n}.txt")
        cfg = (run / f"updater-{n}.toml").read_text() if (run / f"updater-{n}.toml").exists() else ""
        mp = re.search(r'url = "http://127\.0\.0\.1:(\d+)"', cfg)  # the daemon's own beacon port (gate-hook.sh)
        port = mp.group(1) if mp else "?"
        mpoll = re.search(r"poll_secs = (\d+)", cfg)
        mt = (run / f"metrics-{n}.txt").read_text() if (run / f"metrics-{n}.txt").exists() else ""
        mr = re.search(r"^ferrata_updater_rounds(?:_total)?(?:\{[^}]*\})? (\d+)", mt, re.M)
        bport = stats["per_port"].get(port, {})
        breq = sum(v["requests"] for v in bport.values()); bbytes = sum(v["bytes"] for v in bport.values())
        rpc = json.loads((run / f"rpc-{n}.json").read_text()) if (run / f"rpc-{n}.json").exists() else {}
        s = samples.get(n, [])
        med = statistics.median([x[2] for x in s]) / 1024 if s else float("nan")
        cpu = tm.get("user", 0) + tm.get("sys", 0)
        wall = tm.get("wall", 0)
        rounds = int(mr.group(1)) if mr else 0
        if rounds and wall:
            per_round.append(wall / rounds)
        if wall:
            shares.append(cpu / wall)
        ups.append(rpc.get("up", 0)); downs.append(rpc.get("down", 0))
        print(f"| {n} | {mpoll.group(1) if mpoll else '?'} | {rounds or '?'}, {wall / rounds if rounds and wall else float('nan'):.1f} | "
              f"{wall:.0f} | {cpu:.1f} | {cpu / wall if wall else 0:.4f} | {tm.get('rss_kb', 0) / 1024:.1f} | {med:.1f} | "
              f"{g(breq)} / {g(bbytes)} | {g(rpc.get('up', 0))} / {g(rpc.get('down', 0))} |")
    if per_round:
        key.update(round_s=(min(per_round), max(per_round)), cpu_share=(min(shares), max(shares)),
                   rpc=(min(ups), max(ups), min(downs), max(downs)))
    print("\nBeacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are "
          "counted by a proxy in front of each daemon.")
    print()
    m = re.search(r"GATE_WINDOW=(\d+) GATE_POLL=(\d+) GATE_ADVERSARY=(\d+)", machine)
    if m:
        key.update(window=int(m.group(1)), poll=int(m.group(2)), adversary=int(m.group(3)))
    return key


def side_by_side(keys):
    print("## The runs side by side\n")
    print("| run | honest daemons' deferral window: rounds of a nominal s (measured s per round, all daemons) | adversary | reveal step s | jobs: completed / abandoned | steps applied twice | finality updates covered: own job / a later one | latency median / p90 (s) | execution gas, all daemons, per recorded update revealed | gas multiple: total / completed jobs ((applied + refused) / applied) | honest waste: refused and abandoned / completed | A3's ratio: honest refused / applied (gate at most 0.10) | honest-1's share of the honest fees (gate 0.35–0.65) |")
    print("|---|---|---|---:|---|---:|---|---|---:|---|---:|---:|---:|")
    fmt = lambda v, f: f.format(v) if v is not None else "—"
    for k in keys:
        rs = k.get("round_s")
        print(f"| {k['run']} | {k.get('window', '?')} of {k.get('poll', '?')} ({fmt(rs[0] if rs else None, '{:.1f}')}–{fmt(rs[1] if rs else None, '{:.1f}')}) | "
              f"{'yes' if k.get('adversary') else 'no'} | {fmt(k.get('reveal_step'), '{:.0f}')} | {k.get('completed', 0)} / {k.get('abandoned', 0)} | "
              f"{k.get('twice', 0)} | {k.get('own', 0)} / {k.get('later', 0)} | {fmt(k.get('lat_med'), '{:.0f}')} / {fmt(k.get('lat_p90'), '{:.0f}')} | "
              f"{fmt(k['total_gas'] / k['revealed'] if k.get('revealed') else None, '{:,.0f}')} | "
              f"**{fmt(k.get('mult'), '{:.3f}')}** ({fmt(k.get('mult_applied'), '{:.3f}')}) | {fmt(k.get('waste'), '{:.4f}')} | "
              f"{fmt(k.get('r9'), '{:.4f}')} | {fmt(k.get('share'), '{:.3f}')} |")


def main():
    keys = [summarize(run) for run in sys.argv[1:]]
    if len(keys) > 1:
        side_by_side(keys)


if __name__ == "__main__":
    main()
