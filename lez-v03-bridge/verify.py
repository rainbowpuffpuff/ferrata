#!/usr/bin/env python3
"""Re-read every verdict of a lez-v03-bridge run from both chains' RPCs against its record (EXPERIMENTAL).

  python3 -I lez-v03-bridge/verify.py --lez URL --eth URL RECORD_DIR

RECORD_DIR is results/<net>/ as testnet.sh wrote it (run.json, state.json, deposits.json, evm.json). Checks:
- LEZ: every recorded transaction is in the block the record names (getTransaction); every account the run touched
  holds, in the program's shard, exactly the bytes the record holds (getAccount): the configuration, the imported
  roots, the two nullifiers, the public holder, the burn notes, the burn tree, the light client's record;
- the EVM chain: the vault's slots 0, 1 and 2 at the captured block (eth_getStorageAt) are the root the bridge
  imported, the count, and the configuration id; each deposit's receipt has status 1.
A LEZ v0.3.1 node keeps no per-transaction status, so a refusal shows as an included transaction that changed
nothing: the record's account bytes (written after the refusals) are what the accepted transactions alone produce.
A check whose block is not final (its Bedrock status, the last byte of `getBlock`) is PENDING, not FAIL: the public
testnet rewinds (spike/tmp/testnet-ops/FOR-SIBLINGS.md); exit 0 all pass, 3 some pending, 1 a failure.
Python standard library only; read-only calls; generic User-Agent.
"""
import argparse
import json
import os
import sys
import urllib.request

import base64

fails = []
pending = []
count = 0
STATUS = {0: "Pending", 1: "Safe", 2: "Finalized"}


def rpc(url, method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, body, {"content-type": "application/json", "user-agent": "ferrata-verify"})
    got = json.loads(urllib.request.urlopen(req, timeout=60).read())
    if "error" in got:
        raise RuntimeError(f"{method}: {got['error']}")
    return got["result"]


def check(name, ok, detail="", wait=False):
    """`wait`: the facts behind this check are in blocks that are not final yet; a mismatch is PENDING, not FAIL."""
    global count
    count += 1
    verdict = "PASS" if ok else ("PENDING" if wait else "FAIL")
    print(f"{verdict} {name}" + (f": {detail}" if detail else ""))
    if not ok:
        (pending if wait else fails).append(name)


def block_status(url, block):
    """A v0.3.x block's Bedrock status: the last byte of its borsh encoding (getBlock gives base64)."""
    got = rpc(url, "getBlock", [block])
    return None if got is None else STATUS.get(base64.b64decode(got)[-1], "?")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lez", required=True)
    ap.add_argument("--eth", required=True)
    ap.add_argument("--any-status", action="store_true",
                    help="a local standalone sequencer (Bedrock mocked): do not require final blocks")
    ap.add_argument("record")
    a = ap.parse_args()
    load = lambda f: json.load(open(os.path.join(a.record, f)))
    run, state = load("run.json"), load("state.json")
    print(f"LEZ {a.lez}, last block {rpc(a.lez, 'getLastBlockId', [])}; record {a.record}")

    unsettled = False
    for step, s in sorted(run.get("steps", {}).items()):
        if "tx" not in s:
            continue
        got = rpc(a.lez, "getTransaction", [s["tx"]])
        st = block_status(a.lez, s["block"]) if got else None
        final = a.any_status or st == "Finalized"
        unsettled |= not final
        check(f"LEZ {step}: tx {s['tx'][:16]}… in block {s['block']} ({st})",
              got is not None and got[1] == s["block"] and final,
              "" if got else "not found (a rewound block's transaction waits in the mempool, or was lost)", wait=not final)

    for name, acc in sorted(state["accounts"].items()):
        got = rpc(a.lez, "getAccount", [acc["id"]])
        shard = bytes(got["data"]["shards"].get(acc["program"], [])).hex()
        check(f"LEZ {name} ({acc['id'][:10]}…)", shard == acc["shard"],
              (acc["decoded"] or "empty")[:160] if shard == acc["shard"] else f"holds {shard[:64]}…, record {acc['shard'][:64]}…",
              wait=unsettled)

    if os.path.exists(os.path.join(a.record, "evm.json")) or run.get("config_json"):
        cfg = run["config_json"]
        vault = "0x" + cfg["vault"]
        proof = [f for f in run.get("capture_files", [])]
        block = run.get("capture_block")
        if block is not None:
            at = hex(block)
            word = lambda n: rpc(a.eth, "eth_getStorageAt", [vault, hex(n), at])
            root = word(0)
            check(f"EVM vault slot 2 at block {block} = configId", word(2)[2:] == run["config_id"], word(2))
            check(f"EVM vault slot 1 at block {block} = 2 leaves", int(word(1), 16) == run.get("capture_leaves", 2), word(1))
            imported = [v for k, v in state["accounts"].items() if k.startswith("root_") and v["decoded"]]
            check(f"EVM vault slot 0 at block {block} is the root LEZ imported",
                  any(root[2:] in v["decoded"] or bytes.fromhex(root[2:]).hex() in v["shard"] for v in imported), root)
        for k in (0, 1):
            f = os.path.join(a.record, f"deposit-{k}.json")
            if os.path.exists(f):
                tx = json.load(open(f))["transactionHash"]
                r = rpc(a.eth, "eth_getTransactionReceipt", [tx])
                check(f"EVM deposit {k} {tx[:12]}… status 1", r is not None and int(r["status"], 16) == 1)

    print(f"{count} checks, {len(fails)} fail, {len(pending)} pending (blocks not final yet)")
    sys.exit(1 if fails else (3 if pending else 0))


if __name__ == "__main__":
    main()
