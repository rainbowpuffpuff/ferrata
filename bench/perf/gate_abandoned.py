#!/usr/bin/env python3
"""RFP-021 R.9's "beyond what contention makes unavoidable", from one or more bench/perf/gate-sequencer.sh runs: each
abandoned job (a job with an applied step that never completed: another job completed after it began and moved the
store it began on) beside the job that overtook it, and whether the later of the two `Open`s could have seen the
earlier job when it was sent. Usage: python3 -I bench/perf/gate_abandoned.py RUN_DIR [RUN_DIR ...]

What a daemon can read: the sequencer backend reads an account as the sequencer serves it (its head, which holds a
block from the moment the sequencer publishes it) and checks it against the follower of the blocks final on Bedrock; when
the two differ the read fails and the round sends nothing (`lez/live/driver/src/live.rs`, `actor_state`). So a job is in
effect seen from the publication of the block of its `Open` (a read of it fails until the block is final, then shows
it), provided the daemon reads its account, which needs the job's update (a job's account is found by its id, which
hashes the update). In these runs both daemons are served the same updates by one replay.

Times: a transaction's send time is its settlement time (the journal, to the second) less the time from send to
settlement its daemon logged (`[updater] ... + 11.6s <hash>`); a block's publication and finality times are the
sequencer's log lines `Publishing block N` and `Channel update: ... finalized A..=B` (to the second; every process of a
run runs under one libfaketime offset; the send time is known to within about a second above it, so the sequencer's
second is compared with a one-second margin each way). The later `Open` of a pair is classed:
  same block    both `Open`s in one block, and no other job open on that store published before them;
  in flight     sent at least one second before the earlier `Open`'s block was published, and before any other job open
                on that store was published: no read could show a job open on the store;
  ambiguous     within a second of that block's publication (the times' resolution);
  served        sent once a job open on the store was published (the earlier `Open`'s block, or another job's), at least
                one second before that block was final: a read of the job then fails, so a daemon that reads it (join or
                wait does) sends nothing that round;
  final         sent later: a read shows the job;
  unknown       a time is missing.
The first two are contention no read could avoid; served and final are what join or wait (`policy.join_wait_rounds`)
removes, for a daemon served the other job's update. Steps of an abandoned job after its `Open` (applied or refused) are
also counted when sent once the job that overtook it was published and led it, as `leads` in
lez/updater/src/updater.rs orders jobs: more of its steps published, or as many and its update ranking higher (adopting
a committee first, then the later finalized slot): join or wait does not send those either."""

import gzip
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

STEP = {"open": "Open", "miller": "Miller", "adopt0": "adopt 1/2", "adopt1": "adopt 2/2", "finish": "Finish"}
CLASSES = ("same block", "in flight", "ambiguous", "served", "final", "unknown")


def unix(utc):
    return datetime.strptime(utc, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()


def g(n):
    return f"{int(round(n)):,}"


def load(run):
    run = Path(run)
    names = sorted(p.name[len("state-"):] for p in run.glob("state-*") if p.is_dir())
    # The time each daemon's backend logged from send to settlement, by transaction hash.
    delay = {}
    for n in names:
        log = run / f"daemon-{n}.log"
        if not log.exists():
            continue
        for line in log.read_text().splitlines():
            m = re.match(r"\[updater\] .* \+\s*([\d.]+)s ([0-9a-f]{64})$", line)
            if m:
                delay[m.group(2)] = float(m.group(1))
    rows = []
    for n in names:
        p = run / f"state-{n}" / "journal.tsv"
        for line in p.read_text().splitlines() if p.exists() else []:
            f = line.split("\t")
            if len(f) < 13:
                continue
            h = f[12].split(" ")[0]
            settled = unix(f[10])
            rows.append(dict(daemon=n, step=f[1], job=f[2], slot=int(f[3]), status=f[4], gas=int(f[6] or 0),
                             block=int(f[9]), sent=settled - delay[h] if h in delay else None, label=f[11]))
    published, final = {}, {}
    seq = run / "sequencer.log.gz"
    if seq.exists():
        for line in gzip.decompress(seq.read_bytes()).decode("utf-8", "replace").splitlines():
            m = re.match(r"\[(\S+Z) INFO\s+sequencer_core\] Publishing block (\d+) ", line)
            if m:
                published.setdefault(int(m.group(2)), unix(m.group(1)))
            m = re.match(r"\[(\S+Z) INFO\s+sequencer_core::logging\] Channel update: .*finalized (\d+)\.\.=(\d+)", line)
            if m:
                for b in range(int(m.group(2)), int(m.group(3)) + 1):
                    final.setdefault(b, unix(m.group(1)))
    return rows, published, final


def analyse(run):
    rows, published, final = load(run)
    last = lambda r: r["step"] == "finish" or r["label"].endswith("init adopt 2/2")
    applied = [r for r in rows if r["status"] == "applied"]
    done = {r["job"]: r["block"] for r in applied if last(r)}
    jobs, refused = {}, {}
    for r in applied:
        jobs.setdefault(r["job"], []).append(r)
    opens = {r["job"]: r for r in applied if r["step"] == "open"}
    adopting = {r["job"] for r in rows if r["step"].startswith("adopt")}
    for r in rows:
        if r["status"] == "refused":
            refused.setdefault(r["job"], []).append(r)
    out, skipped = [], 0
    for j, rs in jobs.items():
        if j in done:
            continue
        first = min(r["block"] for r in rs)
        over = sorted((b, k) for k, b in done.items() if b >= first)
        if not over:
            continue  # unfinished: in flight at the run's end
        k = over[0][1]
        open_j = next((r for r in rs if r["step"] == "open"), None)
        open_k = next((r for r in jobs[k] if r["step"] == "open"), None)
        if open_j is None or open_k is None:
            skipped += 1
            continue
        early, late = sorted((open_j, open_k), key=lambda r: (r["block"], r["sent"] or 0))
        sent = late["sent"]
        # Jobs open on the same store before the later Open: those first applied after the last completion before it.
        before = max((b for b in done.values() if b < late["block"]), default=-1)
        others = [r for x, r in opens.items() if r["block"] > before and r["block"] <= late["block"] and r is not late]
        pub = min((published[r["block"]] for r in others if r["block"] in published), default=None)
        fin_of = lambda t: min((final[r["block"]] for r in others if published.get(r["block"]) == t and r["block"] in final), default=None)
        fin = fin_of(pub) if pub is not None else None
        same = all(r["block"] == late["block"] for r in others)
        if same:
            cls = "same block"
        elif sent is None or pub is None or fin is None:
            cls = "unknown"
        elif sent + 1 <= pub:
            cls = "in flight"
        elif sent < pub + 1:
            cls = "ambiguous"
        elif sent + 1 <= fin:
            cls = "served"
        else:
            cls = "final"
        # Later steps of the abandoned job sent once the overtaking job was published and led it.
        mine = sorted((r for r in rs + refused.get(j, []) if r["step"] != "open"), key=lambda r: (r["block"], r["sent"] or 0))
        led = []
        for r in mine:
            if r["sent"] is None:
                continue
            seen = lambda x: published.get(x["block"]) is not None and published[x["block"]] + 1 <= r["sent"]
            theirs = sum(1 for x in jobs[k] if seen(x))
            ours = sum(1 for x in rs if x is not r and x["block"] < r["block"] and seen(x))
            rank = lambda job, slot: (job in adopting, slot)
            if theirs and (theirs > ours or (theirs == ours and rank(k, jobs[k][0]["slot"]) > rank(j, rs[0]["slot"]))):
                led.append(r)
        out.append(dict(job=j, slot=rs[0]["slot"], opener=open_j["daemon"], open_block=open_j["block"],
                        steps=" + ".join(STEP.get(r["step"], r["step"]) for r in sorted(rs, key=lambda r: r["block"])),
                        gas=sum(r["gas"] for r in rs), refused=len(refused.get(j, [])),
                        refused_gas=sum(r["gas"] for r in refused.get(j, [])), over=k, over_slot=jobs[k][0]["slot"],
                        over_block=open_k["block"], over_opener=open_k["daemon"], late_sent=sent, early_pub=pub,
                        early_final=fin, cls=cls, led=len(led), led_gas=sum(r["gas"] for r in led)))
    out.sort(key=lambda d: d["open_block"])
    return out, skipped


def clock(t):
    return datetime.fromtimestamp(t, timezone.utc).strftime("%H:%M:%S") if t is not None else "?"


def main():
    for i, run in enumerate(sys.argv[1:]):
        out, skipped = analyse(run)
        name = Path(run).name
        if i:
            print()
        print(f"### Abandoned jobs of `{name}`\n")
        if not out:
            print(f"None{f' ({skipped} pairs without an applied `Open` skipped)' if skipped else ''}.")
            continue
        print("| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |")
        print("|---|---|---|---|---|---:|---:|---|---:|")
        for d in out:
            ref = f"; {d['refused']} refused ({g(d['refused_gas'])})" if d["refused"] else ""
            print(f"| 0x{d['job'][:8]} ({d['slot']}) | {d['opener']}, {d['open_block']} | {d['steps']} ({g(d['gas'])}){ref} | "
                  f"0x{d['over'][:8]} ({d['over_slot']}) | {d['over_opener']}, {d['over_block']} | {clock(d['late_sent'])} | "
                  f"{clock(d['early_pub'])} / {clock(d['early_final'])} | {d['cls']} | "
                  f"{d['led']}{f' ({g(d['led_gas'])})' if d['led'] else ''} |")
        by = {c: [d for d in out if d["cls"] == c] for c in CLASSES}
        gas = lambda ds: sum(d["gas"] + d["refused_gas"] for d in ds)
        unavoidable = by["same block"] + by["in flight"]
        led = [d for d in unavoidable if d["led"]]
        parts = ", ".join(f"{c} {len(by[c])} ({g(gas(by[c]))})" for c in CLASSES if by[c] or c not in ("unknown", "ambiguous"))
        print(f"\n`{name}`: {len(out)} abandoned jobs, {g(gas(out))} execution gas (applied and refused steps): {parts}; "
              f"**{len(by['served']) + len(by['final'])} with the later `Open` sent once a job open on the store was published** "
              f"({g(gas(by['served'] + by['final']))}); of the {len(unavoidable)} whose `Open`s no read could have avoided, "
              f"{len(led)} with later steps sent once the overtaking job was published and led them "
              f"({sum(d['led'] for d in led)} steps, {g(sum(d['led_gas'] for d in led))})"
              + (f"; {skipped} pairs without an applied `Open` skipped" if skipped else "") + ".")


if __name__ == "__main__":
    main()
