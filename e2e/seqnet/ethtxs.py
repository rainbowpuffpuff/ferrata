#!/usr/bin/env python3
"""Every transaction of the seqnet run's local anvil chain, one TSV line each: block, time, hash, from, to (named),
the function called (from forge's method identifiers in vault/out), value, status, gas used, contract created.

  ethtxs.py --rpc URL --vault-out DIR --names 'deployer=0x..,relayer=0x..,...' > transactions.tsv
"""
import argparse
import json
import os
import sys
import urllib.request


def rpc(url, method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, body, {"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        out = json.load(r)
    if "error" in out:
        raise RuntimeError(f"{method}: {out['error']}")
    return out["result"]


def selectors(out_dir):
    names = {}
    for root, _, files in os.walk(out_dir):
        for f in files:
            if not f.endswith(".json"):
                continue
            try:
                with open(os.path.join(root, f)) as fh:
                    ids = json.load(fh).get("methodIdentifiers") or {}
            except (OSError, ValueError):
                continue
            for sig, sel in ids.items():
                names.setdefault("0x" + sel, sig.split("(")[0])
    return names


def main():
    a = argparse.ArgumentParser()
    a.add_argument("--rpc", required=True)
    a.add_argument("--vault-out", required=True)
    a.add_argument("--names", default="")
    o = a.parse_args()
    who = {}
    for kv in filter(None, o.names.split(",")):
        k, v = kv.split("=", 1)
        who[v.lower()] = k
    sels = selectors(o.vault_out)
    last = int(rpc(o.rpc, "eth_blockNumber", []), 16)
    print("block\ttime\ttx_hash\tfrom\tto\tcall\tvalue_wei\tstatus\tgas_used\tcreated")
    empty = 0
    for n in range(0, last + 1):
        if int(rpc(o.rpc, "eth_getBlockTransactionCountByNumber", [hex(n)]), 16) == 0:
            empty += 1
            continue
        b = rpc(o.rpc, "eth_getBlockByNumber", [hex(n), True])
        for tx in b["transactions"]:
            r = rpc(o.rpc, "eth_getTransactionReceipt", [tx["hash"]])
            to = (tx.get("to") or "").lower()
            created = (r.get("contractAddress") or "").lower()
            call = "create" if not to else sels.get(tx["input"][:10], tx["input"][:10] if len(tx["input"]) >= 10 else "transfer")
            name = lambda x: f"{who[x]} {x}" if x in who else x
            print("\t".join([str(n), str(int(b["timestamp"], 16)), tx["hash"], name(tx["from"].lower()), name(to) if to else "-", call,
                             str(int(tx["value"], 16)), "ok" if r["status"] == "0x1" else "REVERTED", str(int(r["gasUsed"], 16)),
                             name(created) if created else "-"]))
    print(f"blocks 0..={last}: {empty} without transactions", file=sys.stderr)


if __name__ == "__main__":
    main()
