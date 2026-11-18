#!/usr/bin/env python3
"""The invariants of one netfault seed (e2e/netfault/run.sh), read from anvil directly and from the network journal:
every transaction of the deployer on chain is in the journal (signed and journaled before it left), all succeeded,
nonces have no gap, and no step's effect happened twice: when the run completed, 19 transactions (13 deploy steps,
2 deposits, submit, finalize, 2 releases), 2 deposits, both nullifiers used once, 6e12 wei left in the vault
(crown-fork-3r's figure: 2 relay fees + 2 checkpoint fees). Prints one JSON line."""
import argparse
import json
import os
import urllib.request
from collections import Counter


def rpc(url, method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        res = json.loads(r.read())
    if "error" in res:
        raise RuntimeError(res["error"])
    return res["result"]


def call(url, to, data):
    return rpc(url, "eth_call", [{"to": to, "data": data}, "latest"])


def main():
    ap = argparse.ArgumentParser()
    for k in ("rpc", "run", "deployer", "vault", "outcome", "stopped", "resumes", "seed", "seconds"):
        ap.add_argument(f"--{k}", default="")
    a = ap.parse_args()
    dep = a.deployer.lower()
    head = int(rpc(a.rpc, "eth_blockNumber", []), 16)
    chain_txs = []
    for n in range(head + 1):
        b = rpc(a.rpc, "eth_getBlockByNumber", [hex(n), True])
        for t in b["transactions"]:
            if t["from"].lower() == dep:
                r = rpc(a.rpc, "eth_getTransactionReceipt", [t["hash"]])
                chain_txs.append({"hash": t["hash"], "nonce": int(t["nonce"], 16), "status": r["status"], "block": n})
    jpath = os.path.join(a.run, "net", "journal.jsonl")
    signed, fates, stages, agreed = {}, {}, Counter(), []
    for line in open(jpath) if os.path.isfile(jpath) else []:
        r = json.loads(line)
        if r.get("event") == "signed":
            signed[r["hash"]] = r
            agreed.append(r.get("nonce_agreed_by"))
        elif r.get("event") == "fate":
            fates[r["hash"]] = r
    v = []  # violations
    nonce = int(rpc(a.rpc, "eth_getTransactionCount", [dep, "latest"]), 16)
    if sorted(t["nonce"] for t in chain_txs) != list(range(nonce)):
        v.append(f"nonces on chain {sorted(t['nonce'] for t in chain_txs)} are not 0..{nonce - 1}")
    for t in chain_txs:
        if t["hash"] not in signed:
            v.append(f"{t['hash']} on chain but not journaled before it left")
        if t["status"] != "0x1":
            v.append(f"{t['hash']} failed on chain")
        stages[signed.get(t["hash"], {}).get("stage", "?")] += 1
    if any(x is None or x < 2 for x in agreed):
        v.append(f"a send without the nonce agreed by two endpoints: {agreed}")
    for h, f in fates.items():
        if f["fate"] != "mined":
            v.append(f"{h}: fate {f['fate']} ({f.get('why', '')})")
    for st, n in stages.items():
        if st != "deploy" and n != 1:
            v.append(f"stage {st}: {n} transactions on chain")
    word = lambda x: x[2:].rjust(64, "0")  # noqa: E731
    facts = {}
    if a.outcome == "complete":
        sel = lambda sig: "0x" + __import__("subprocess").run(["cast", "sig", sig], capture_output=True,  # noqa: E731
                                                                  text=True).stdout.strip()[2:]
        facts["deposits"] = int(call(a.rpc, a.vault, sel("depositCount(address)") + word(dep)), 16)
        facts["vault_balance"] = int(rpc(a.rpc, "eth_getBalance", [a.vault, "latest"]), 16)
        crown = json.load(open(os.path.join(a.run, "crown.json")))
        facts["nullifiers_used"] = sum(int(call(a.rpc, a.vault, sel("nullifierUsed(bytes32)") + r["nullifier"][2:]), 16)
                                       for r in crown["releases"])
        if len(chain_txs) != 19:
            v.append(f"{len(chain_txs)} transactions on chain, 19 expected")
        if facts["deposits"] != 2 or facts["nullifiers_used"] != 2 or facts["vault_balance"] != 6 * 10**12:
            v.append(f"effects: {facts}")
    faults = Counter()
    fpath = os.path.join(a.run, "w", "faults.jsonl")
    for line in open(fpath) if os.path.isfile(fpath) else []:
        faults[json.loads(line)["fault"]] += 1
    events = Counter()
    epath = os.path.join(a.run, "net", "events.jsonl")
    for line in open(epath) if os.path.isfile(epath) else []:
        e = json.loads(line)
        events[e["kind"] + ("-" + e["cls"] if e.get("cls") else "")] += 1
    retries = sum(1 for _, _, fs in os.walk(os.path.join(a.run, "logs")) for f in fs if ".transient-" in f)
    print(json.dumps({"seed": int(a.seed), "outcome": a.outcome, "stopped_at": a.stopped or None,
                      "resumes": int(a.resumes or 0), "stage_reruns": retries, "txs_on_chain": len(chain_txs),
                      "journaled": len(signed), "seconds": int(a.seconds or 0), "faults": dict(faults),
                      "proxy_events": dict(events), "facts": facts, "violations": v, "ok": not v}, sort_keys=True))


if __name__ == "__main__":
    main()
