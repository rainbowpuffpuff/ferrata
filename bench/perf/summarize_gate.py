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
    key.update(jobs=len(jobs), twice=len(twice))

    # Latency: an entry's appearance to the first applied Finish whose finalized slot reaches it.
    fin = sorted((r["t"], r["slot"], n) for n in names for r in J[n] if r["step"] == "finish" and r["status"] == "applied")
    lat, who = [], {}
    for e in reveals:
        slot, t0, kind = int(e[4]), float(e[7]), e[2]
        hit = next(((t, s, n) for t, s, n in fin if s >= slot and t >= t0 - 1), None)
        if hit and kind == "finality":
            lat.append(hit[0] - t0)
            who[hit[2]] = who.get(hit[2], 0) + 1
    if lat:
        key.update(lat_med=statistics.median(lat), lat_p90=pct(lat, 0.9), lat_n=len(lat), who=dict(who))
        print(f"\n**Latency** (a recorded finality update appearing at the beacon API to its job's `Finish` applied in a block "
              f"final on Bedrock), {len(lat)} updates: median **{statistics.median(lat):.0f} s**, p90 {pct(lat, 0.9):.0f} s, "
              f"min {min(lat):.0f} s, max {max(lat):.0f} s; finished by {', '.join(f'{k} {v}' for k, v in sorted(who.items()))}.")

    # P.6: resources.
    stats = json.loads((run / "beacon-stats.json").read_text()) if (run / "beacon-stats.json").exists() else {"per_port": {}}
    samples = {}
    if (run / "samples.tsv").exists():
        for line in (run / "samples.tsv").read_text().splitlines()[1:]:
            f = line.split("\t")
            samples.setdefault(f[1], []).append((float(f[0]), float(f[3]), int(f[4]), int(f[5])))
    print("\n| daemon | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes down | sequencer RPC: bytes up / down |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for i, n in enumerate(names):
        tm = gnu_time(run / f"time-{n}.txt")
        port = str(9601 + i)
        bport = stats["per_port"].get(port, {})
        breq = sum(v["requests"] for v in bport.values()); bbytes = sum(v["bytes"] for v in bport.values())
        rpc = json.loads((run / f"rpc-{n}.json").read_text()) if (run / f"rpc-{n}.json").exists() else {}
        s = samples.get(n, [])
        med = statistics.median([x[2] for x in s]) / 1024 if s else float("nan")
        cpu = tm.get("user", 0) + tm.get("sys", 0)
        wall = tm.get("wall", 0)
        print(f"| {n} | {wall:.0f} | {cpu:.1f} | {cpu / wall if wall else 0:.4f} | {tm.get('rss_kb', 0) / 1024:.1f} | {med:.1f} | "
              f"{g(breq)} / {g(bbytes)} | {g(rpc.get('up', 0))} / {g(rpc.get('down', 0))} |")
    print()
    m = re.search(r"GATE_WINDOW=(\d+) GATE_POLL=(\d+) GATE_ADVERSARY=(\d+)", machine)
    if m:
        key.update(window=int(m.group(1)), poll=int(m.group(2)), adversary=int(m.group(3)))
    return key


def main():
    keys = [summarize(run) for run in sys.argv[1:]]
    if len(keys) > 1:
        print("## The runs side by side\n")
        print("| run | honest daemons' deferral window (rounds of s) | adversary | jobs | steps applied twice | latency median / p90 (s) | finished by | R.9 refused / applied | R.10 honest-1's share |")
        print("|---|---|---|---:|---:|---|---|---:|---:|")
        for k in keys:
            fmt = lambda v, f: (f.format(v) if v is not None else "—")
            print(f"| {k['run']} | {k.get('window', '?')} of {k.get('poll', '?')} | {'yes' if k.get('adversary') else 'no'} | "
                  f"{k.get('jobs', 0)} | {k.get('twice', 0)} | {fmt(k.get('lat_med'), '{:.0f}')} / {fmt(k.get('lat_p90'), '{:.0f}')} | "
                  f"{', '.join(f'{a} {b}' for a, b in sorted(k.get('who', {}).items()))} | {fmt(k.get('r9'), '{:.4f}')} | "
                  f"{fmt(k.get('share'), '{:.3f}')} |")


if __name__ == "__main__":
    main()
