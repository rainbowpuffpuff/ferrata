#!/usr/bin/env python3
"""Re-check a day-one rehearsal from its committed records alone (no network, no keys, no zone).

    python3 -I lez/testnet04/recheck.py lez/testnet04/results/rehearsal-1

Recomputes from the files: each stage's transactions, blocks, execution and storage gas and fees (the stored results,
state/tx/<stage>/NN.result.json); the daemon's journaled transactions (state/daemon/state/ledger/run.tsv), their gas
and fees, each fee against what the payer's balance fell by; how many the daemon key sent (its nonce in the final
inspection, state/verify.json) and what it was debited in all; that every transaction hash of the stages and of the
daemon's journal is in the sequencer's log as validated (zone/logs/sequencer.log); the attestation's verdict against
eth_getProof's answer (state/tx/attest/stage.json against state/fetch/proofs/<slot>/proof.json); and the checks
state/verify.json recorded. Prints the figures; exit 0 when every comparison holds, 1 otherwise.
"""
import csv
import glob
import json
import os
import sys

R = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "results", "rehearsal-1")
bad = []


def need(ok, what):
    print(f"[{'ok' if ok else 'FAIL'}] {what}")
    if not ok:
        bad.append(what)


log = open(os.path.join(R, "zone", "logs", "sequencer.log"), errors="replace").read()
validated = {line.split("hash ")[1].split(",")[0] for line in log.splitlines() if "Validated transaction with hash " in line}
payer_fees = 0
for stage in ["deploy-program", "init", "attest", "deploy-bridge"]:
    files = sorted(glob.glob(os.path.join(R, "state", "tx", stage, "[0-9][0-9].result.json")))
    if not files:
        continue
    rs = [json.load(open(f)) for f in files]
    fees = sum(int(r["fee_base"]) for r in rs)
    payer_fees += fees
    print(f"{stage}: {len(rs)} transactions, {sum(r['status'] == 'applied' for r in rs)} applied, blocks "
          f"{rs[0]['block']}-{rs[-1]['block']}, execution gas {sum(r['gas_exec'] for r in rs):,}, storage gas "
          f"{sum(r['gas_stor'] for r in rs):,}, fees {fees:,}")
    need(all(r["status"] == "applied" for r in rs), f"{stage}: every transaction applied")
    need(all(r["hash"] in validated for r in rs), f"{stage}: every hash validated in the sequencer's log")

rows = list(csv.DictReader(open(os.path.join(R, "state", "daemon", "state", "ledger", "run.tsv")), delimiter="\t"))
jfees = sum(int(r["fee"]) for r in rows)
print(f"daemon journal: {len(rows)} transactions, execution gas {sum(int(r['exec_gas']) for r in rows):,}, storage gas "
      f"{sum(int(r['stor_gas']) for r in rows):,}, fees {jfees:,}")
need(all(r["status"] == "applied" for r in rows), "daemon: every journaled transaction applied")
need(all(r["fee"] == r["payer_debit"] for r in rows), "daemon: each journaled fee equals what the payer's balance fell by")
need(all(r["tx_hash"] in validated for r in rows), "daemon: every journaled hash validated in the sequencer's log")

v = json.load(open(os.path.join(R, "state", "verify.json")))
d = v["inspect"]["daemon"]
sent, debit = int(d["nonce"]), 50_000_000_000 - int(d["balance"])
print(f"daemon key at the end: nonce {sent} (transactions it sent), debited {debit:,} of the 50,000,000,000 funded; "
      f"not in its journal: {sent - len(rows)} transactions, {debit - jfees:,}")
p = v["inspect"]["payer"]
pdebit = 50_000_000_000 - int(p["balance"])
need(pdebit == payer_fees, f"payer: debited {pdebit:,}, equal to the stages' fees {payer_fees:,}")

st = json.load(open(os.path.join(R, "state", "tx", "attest", "stage.json")))
slot = st["info"]["record_slot"]
proof = json.load(open(os.path.join(R, "state", "fetch", "proofs", str(slot), "proof.json")))
proof = proof.get("result", proof)
word = lambda x: "0x" + int(x, 16).to_bytes(32, "big").hex()
facts = [s["fact"] for s in st["info"]["verdict"]["statements"]]
acct = next(f for f in facts if "account" in f)
values = [f["value"] for f in facts if "value" in f]
need(values == [word(e["value"]) for e in proof["storageProof"]], f"attestation: slot values {values} equal eth_getProof's")
need(int(acct["nonce"]) == int(proof["nonce"], 16) and int(acct["balance"], 16) == int(proof["balance"], 16)
     and acct["storage_root"] == proof["storageHash"] and acct["code_hash"] == proof["codeHash"],
     "attestation: the account's nonce, balance, storage root and code hash equal eth_getProof's")
print(f"attestation: under the record of slot {slot}, Sepolia block {st['info']['verdict']['block_number']:,}")

checks = v["checks"]
print(f"verify.json: {sum(c['ok'] for c in checks)} of {len(checks)} checks recorded ok")
need(all(c["ok"] for c in checks), "verify.json: every recorded check ok")
print(f"{'all comparisons hold' if not bad else str(len(bad)) + ' do not hold'}")
sys.exit(1 if bad else 0)
