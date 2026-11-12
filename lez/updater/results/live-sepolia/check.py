#!/usr/bin/env python3
"""Re-check the record of ferrata-updater's run on a LEZ sequencer (this directory's README).

    python3 -I check.py [DIR] [--beacon URL] [--write]

DIR (default: this script's directory) holds what session U of lez/crown/session.sh recorded:
  updater.log        the daemon's log (every job it opened: the update's finalized slot and root, its sources)
  journal.tsv        the daemon's journal: one line per transaction it sent, as the chain settled it
  run.tsv            the sequencer backend's manifest (lez/live/driver/src/manifest.rs): per transaction its hash, block,
                     status, gas and fee as LEZ's settlement charged them while following blocks final on Bedrock, the
                     payer's balance before minus after, and the totals the block's own fee transaction states
  probes/health.jsonl  /health and /metrics read every minute
  export/instances.tsv crown-driver's export after the run: the instance as crown-driver's own follower reads it
Checks (no network unless --beacon):
  1. every journal line has its manifest row (same block, same result) and the fee it records is what the payer's
     balance fell by; a transaction alone in its block carries the block's own gas totals (the sequencer's fee
     transaction states them, so this ties the reading to the sequencer)
  2. the jobs: each applied job is Open, Miller, [Adopt 1/2, Adopt 2/2,] Finish in increasing blocks; the finalized
     slots of the applied jobs increase
  3. the instance at the end (export/instances.tsv) holds the last applied job's finalized slot and root
  4. --beacon URL: every applied job's finalized root is the block a Sepolia beacon node (any; read only, generic
     User-Agent) holds canonical at that slot
--write: also writes summary.md (the tables of the README) beside the record.
"""
import json
import os
import re
import subprocess
import sys
from collections import Counter, OrderedDict

args = sys.argv[1:]
beacon = args[args.index("--beacon") + 1] if "--beacon" in args else None
write = "--write" in args
pos = [a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or args[i - 1] != "--beacon")]
D = pos[0] if pos else os.path.dirname(os.path.abspath(__file__))
fails = []


def check(ok, what):
    print(("PASS " if ok else "FAIL ") + what)
    if not ok:
        fails.append(what)


def tsv(name):
    path = os.path.join(D, name)
    if not os.path.isfile(path):
        return None, []
    lines = open(path).read().splitlines()
    return lines, [l.split("\t") for l in lines]


# ---- the journal and the manifest ---------------------------------------------------------------------------------
_, journal = tsv("journal.tsv")
J = ["round", "step", "job", "finalized_slot", "result", "exit", "gas_exec", "gas_stor", "fee", "block", "utc", "label"]
journal = [dict(zip(J, r)) for r in journal]
rlines, run = tsv("run.tsv")
head = run[0] if run else []
rows = [dict(zip(head, r)) for r in run[1:]]
by_label_block = {}
for r in rows:
    by_label_block.setdefault((r["step"], r["block"]), []).append(r)
matched = 0
for j in journal:
    cands = by_label_block.get((j["label"], j["block"] if j["result"] != "rejected" else ""), [])
    if not cands and j["result"] == "rejected":
        cands = [r for r in rows if r["step"] == j["label"] and r["status"] in ("dropped", "rejected")]
    ok = any(
        (r["status"] == j["result"] or (j["result"] == "rejected" and r["status"] in ("dropped", "rejected")))
        and (j["result"] == "rejected" or (r["payer_debit"] == j["fee"] and r["exec_gas"] == j["gas_exec"]))
        for r in cands
    )
    matched += ok
    if not ok:
        print(f"  journal line without its manifest row: {j}")
check(matched == len(journal) and len(journal) > 0,
      f"{matched} of {len(journal)} journal lines have their manifest row (block, result, execution gas, fee = payer debit)")
included = [r for r in rows if r["block"] not in ("", "-")]
debit_ok = sum(r["fee"] == r["payer_debit"] for r in included)
check(debit_ok == len(included), f"{debit_ok} of {len(included)} included transactions: charged fee = the payer's balance fell by")
alone = [r for r in included if r["block_txs"] == "1"]
alone_ok = sum(r["block_exec_gas"] == r["exec_gas"] and r["block_stor_gas"] == r["stor_gas"] for r in alone)
check(alone_ok == len(alone), f"{alone_ok} of {len(alone)} transactions alone in their block carry the block's own gas totals")
blocks = [int(r["block"]) for r in included]
check(blocks == sorted(blocks), "transactions settle in increasing blocks")

# ---- the jobs ------------------------------------------------------------------------------------------------------
log = open(os.path.join(D, "updater.log"), errors="replace").read().splitlines() if os.path.isfile(os.path.join(D, "updater.log")) else []
opened = {}
pat = re.compile(r"^(\S+) \[\S+ \S+\] job 0x([0-9a-f]{16}): finalized slot (\d+) root 0x([0-9a-f]{64}) \(attested slot (\d+), "
                 r"signature slot (\d+), (\d+) of (\d+) signers([^)]*)\), served or confirmed by (.*)$")
for line in log:
    m = pat.match(line)
    if m:
        opened[m.group(2)] = dict(t=m.group(1), slot=int(m.group(3)), root=m.group(4), attested=int(m.group(5)),
                                  signature=int(m.group(6)), signers=int(m.group(7)), of=int(m.group(8)),
                                  outcome=m.group(9).strip("; ").replace("; ", ", "), sources=m.group(10))
jobs = OrderedDict()
for j in journal:
    jobs.setdefault(j["job"], []).append(j)
applied_jobs = []
for jid, steps in jobs.items():
    done = [s for s in steps if s["result"] == "applied"]
    if any(s["step"] == "finish" for s in done):
        names = [s["step"] for s in done]
        want_tail = ["miller"] + (["adopt0", "adopt1"] if "adopt0" in names else []) + ["finish"]
        shape = names[0] == "open" and names[1:] == want_tail
        b = [int(s["block"]) for s in done]
        info = opened.get(jid[:16], {})
        applied_jobs.append(dict(job=jid, steps=done, shape=shape, blocks=b, **info))
check(all(a["shape"] and a["blocks"] == sorted(a["blocks"]) for a in applied_jobs) and applied_jobs,
      f"{len(applied_jobs)} jobs applied, each Open, Miller, [Adopt 1/2, Adopt 2/2,] Finish in increasing blocks")
check(all("slot" in a for a in applied_jobs), "every applied job's update is in updater.log (finalized slot, root, sources)")
advancing = [a for a in applied_jobs if "advances" in a.get("outcome", "")]
slots = [a["slot"] for a in advancing]
check(all(x < y for x, y in zip(slots, slots[1:])),
      f"{len(advancing)} applied jobs advance the finalized header, each further than the one before "
      f"(the others only adopt a committee: {len(applied_jobs) - len(advancing)})")
open_unfinished = [jid for jid, steps in jobs.items() if not any(s["step"] == "finish" and s["result"] == "applied" for s in steps)]

# ---- the instance at the end -----------------------------------------------------------------------------------------
_, inst = tsv("export/instances.tsv")
if inst and advancing:
    h = dict(zip(inst[0], inst[1]))
    last = advancing[-1]
    check(int(h["finalized_slot"]) == last["slot"] and h["finalized_root"] == last["root"],
          f"export/instances.tsv: the instance holds finalized slot {h['finalized_slot']} root {h['finalized_root'][:16]}..., "
          f"the last advancing job's")
    check(int(h["jobs_applied"]) == 2 + len(applied_jobs),
          f"export/instances.tsv: {h['jobs_applied']} jobs applied = session A's 2 + the updater's {len(applied_jobs)}")
else:
    print("SKIP export/instances.tsv (not in the record yet)")

# ---- against a beacon node -------------------------------------------------------------------------------------------
if beacon:
    agree = 0
    for a in applied_jobs:
        if "slot" not in a:
            continue
        out = subprocess.run(["curl", "-sS", "--max-time", "30", "-H", "Accept: application/json", "-A", "ferrata-check",
                              f"{beacon.rstrip('/')}/eth/v1/beacon/headers/{a['slot']}"], capture_output=True, text=True)
        try:
            d = json.loads(out.stdout)["data"]
            ok = d["root"] == "0x" + a["root"] and d["canonical"] is True
        except Exception:
            ok = False
        agree += ok
        if not ok:
            print(f"  slot {a['slot']}: {out.stdout[:200]}")
    check(agree == len([a for a in applied_jobs if "slot" in a]),
          f"{agree} applied finalized roots are canonical at their slots on {beacon}")

# ---- health ----------------------------------------------------------------------------------------------------------
probes = []
hp = os.path.join(D, "probes/health.jsonl")
if os.path.isfile(hp):
    probes = [json.loads(l) for l in open(hp) if l.strip()]
status = Counter((p["health"] or [{}])[0].get("status", f"http {p['http']}") if p.get("health") else f"no answer (http {p['http']})"
                 for p in probes)
print(f"health: {len(probes)} probes, {dict(status)}; lives {sorted(set(p['life'] for p in probes))}")
print(f"journal: {len(journal)} transactions, {dict(Counter(j['result'] for j in journal))}; jobs applied {len(applied_jobs)}; "
      f"jobs opened and not finished {len(open_unfinished)}")

if write:
    out = ["| # | finalized slot | finalized root | attested slot | signature slot | signers | effect | Open block | Finish block | Finish (UTC) | gas (all steps) | fee |",
           "|---:|---:|---|---:|---:|---:|---|---:|---:|---|---:|---:|"]
    for i, a in enumerate(applied_jobs, 1):
        gas = sum(int(s["gas_exec"]) + int(s["gas_stor"]) for s in a["steps"])
        fee = sum(int(s["fee"]) for s in a["steps"])
        out.append(f"| {i} | {a.get('slot', '?'):,} | `{a.get('root', '?')[:16]}…` | {a.get('attested', 0):,} | {a.get('signature', 0):,} | "
                   f"{a.get('signers', '?')}/{a.get('of', '?')} | {a.get('outcome', '')} | {a['blocks'][0]} | {a['blocks'][-1]} | "
                   f"{a['steps'][-1]['utc'][11:19]} | {gas:,} | {fee:,} |")
    open(os.path.join(D, "summary.md"), "w").write(
        "# The applied updates (written by check.py --write from journal.tsv and updater.log)\n\n" + "\n".join(out) + "\n")
    print(f"wrote {os.path.join(D, 'summary.md')}")
print("ALL PASS" if not fails else f"{len(fails)} FAILED")
sys.exit(1 if fails else 0)
