#!/usr/bin/env python3
"""lab.sh status: the instances against the chains' own clocks, the sessions, the claim.

    status.py DIR    (DIR holds sessions.tsv, instances.tsv, jobs.tsv as the Volume has them)

A chain's own slot is computed from its genesis time and the clock (12 s slots, 8,192 slots a
period); nothing is fetched.
"""
import csv
import os
import sys
import time

GENESIS = {"sepolia": 1_655_733_600, "mainnet": 1_606_824_023}


def rows(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter="\t"))


def when(t):
    return time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime(int(t)))


def main(d):
    now = time.time()
    print(f"now {when(now)}")
    for r in rows(os.path.join(d, "instances.tsv")):
        chain = r["chain"]
        slot = int((now - GENESIS[chain]) // 12)
        fin = int(r["finalized_slot"])
        print(f"{chain:8} instance finalized slot {fin} (period {fin // 8192}, layout {r['store_layout']}); "
              f"the chain's clock: slot {slot} (period {slot // 8192}); behind by {slot - fin} slots "
              f"({(slot - fin) * 12 / 3600:.1f} h); jobs applied {r['jobs_applied']}")
    jobs = rows(os.path.join(d, "jobs.tsv"))
    for chain in GENESIS:
        js = [j for j in jobs if j["chain"] == chain and j["status"].startswith("applied")]
        if not js:
            continue
        init = [j for j in js if j["kind"] == "init"]
        upd = [j for j in js if j["kind"] != "init"]
        rot = [j for j in upd if j["rotated"] == "true"]
        since = init[0]["session"] if init else "?"
        print(f"{chain:8} following since session {since}: {len(upd)} updates applied "
              f"({sum(j['kind'] == 'period' for j in upd)} period, {sum(j['kind'] == 'finality' for j in upd)} finality), "
              f"{len(rot)} committee handoffs")
    ss = rows(os.path.join(d, "sessions.tsv"))
    bad = [s for s in ss if s["status"] != "ok"]
    print(f"sessions: {len(ss)}, not ok: {len(bad)}")
    for s in ss[-6:]:
        dur = int(s["finished"]) - int(s["started"])
        print(f"  {s['session']}  {s['status']:<16} {dur:>5} s  generation {s['generation_saved']:<4} "
              f"head block {s['head_block']:<6} starts {s['sequencer_starts']} phase {s['block_phase_s']}")
    for s in bad[-5:]:
        print(f"  not ok: {s['session']} {s['status']}: {s['note']}")


if __name__ == "__main__":
    main(sys.argv[1])
