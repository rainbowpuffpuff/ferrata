#!/usr/bin/env python3
"""The gas of the round trip's transactions as the node prices them, measured on the node itself.

Since Glamsterdam (Sepolia, 6 October 2026) a new account, a byte of code and a new storage slot cost
"state gas" at 1,530 gas per state byte (EIP-8037), which Foundry's EVM does not know: forge and anvil
price a contract creation about six times too low. So no gas figure of this run comes from Foundry; every
one comes from the node, through eth_simulateV1 and eth_estimateGas. Nothing is signed or sent: only the
read-only methods of READ_ONLY are ever called.

  gasplan.py plan <forge log>... --out plan.json
        the transactions of forge's logs (broadcast/<script>/<chain>/[dry-run/]<stage>-latest.json), in order
  gasplan.py check --rpc URL --plan plan.json [--margin 1.25] [--out result.json]
        a stage about to be sent, on the chain's latest state: every transaction in order, its gas used and the
        least gas limit with which it succeeds (bisection on eth_simulateV1; the stage's first transaction also
        through eth_estimateGas), then the whole stage again with each limit times the margin. Exit 1 if a
        transaction fails, or if a limit with the margin is over what one transaction may carry.
  gasplan.py replay --rpc URL --plan plan.json [--nonce N] [--beacon-index I --echo-nonce X]
                    [--wait S] [--release-wait S] [--margin 1.25] [--out result.json]
        a whole round trip of a rehearsal replayed on the node in five blocks (deploy, submit, beacon, finalize,
        release): the waits by the blocks' timestamps, the sender's nonce set to N (so that the creations land
        on the addresses the proofs bind), and the beacon the finalisation proof echoes written into the vault's
        storage (its nonce depends on the block, which a replay cannot reproduce).

Caps (EIP-8037 with EIP-7825): a transaction's execution gas is at most 2^24, its gas limit at most 2^32 - 1,
and not more than the block's gas limit. The node enforces the first when it executes a call under Amsterdam
(reth 2.8.0), so a call that succeeds with its limit is within it; this script checks the other two.
"""
import argparse
import json
import math
import sys
import time
import urllib.error
import urllib.request

TX_MAX_GAS = 1 << 24  # EIP-7825; under EIP-8037 it bounds a transaction's execution gas only
TX_MAX_TOTAL = (1 << 32) - 1  # EIP-8037: a transaction's gas limit, execution and state gas together
BEACONS_SLOT = 46  # FerrataVault: Beacon[] beacons (forge inspect FerrataVault storageLayout)
READ_ONLY = {"web3_clientVersion", "eth_chainId", "eth_blockNumber", "eth_getBlockByNumber", "eth_estimateGas", "eth_simulateV1",
             "eth_getTransactionCount", "eth_getBalance", "eth_gasPrice", "eth_maxPriorityFeePerGas", "eth_call"}
STAGES = ("deploy", "submit", "beacon", "finalize", "release")


class RpcError(Exception):
    pass


class Node:
    def __init__(self, url):
        self.url, self.calls = url, 0

    def __call__(self, method, *params):
        assert method in READ_ONLY, f"{method} is not a read-only method"
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
        for attempt in range(7):
            req = urllib.request.Request(self.url, body, {"Content-Type": "application/json",
                                                          "User-Agent": "ferrata-gasplan"})
            try:
                with urllib.request.urlopen(req, timeout=180) as r:
                    out = json.load(r)
            except urllib.error.HTTPError as e:
                if e.code in (429, 500, 502, 503, 504) and attempt < 6:
                    time.sleep(2 ** attempt)
                    continue
                raise
            except (urllib.error.URLError, TimeoutError, ConnectionError):
                if attempt < 6:
                    time.sleep(2 ** attempt)
                    continue
                raise
            self.calls += 1
            if "error" in out:
                raise RpcError(json.dumps(out["error"])[:400])
            return out["result"]


def word(n):
    return "0x" + format(n, "064x")


def keccak256(data):
    # Keccak-256 (the pre-standard padding Ethereum uses), for the storage slot of beacons[i].
    rc = [0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000, 0x000000000000808B,
          0x0000000080000001, 0x8000000080008081, 0x8000000000008009, 0x000000000000008A, 0x0000000000000088,
          0x0000000080008009, 0x000000008000000A, 0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
          0x8000000000008003, 0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
          0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008]
    rot = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]
    mask = (1 << 64) - 1
    rate = 136
    msg = bytearray(data) + b"\x01"
    msg += b"\x00" * (-len(msg) % rate)
    msg[-1] |= 0x80
    s = [[0] * 5 for _ in range(5)]
    for off in range(0, len(msg), rate):
        for i in range(rate // 8):
            s[i % 5][i // 5] ^= int.from_bytes(msg[off + 8 * i:off + 8 * i + 8], "little")
        for r in rc:
            c = [s[x][0] ^ s[x][1] ^ s[x][2] ^ s[x][3] ^ s[x][4] for x in range(5)]
            d = [c[(x - 1) % 5] ^ (((c[(x + 1) % 5] << 1) | (c[(x + 1) % 5] >> 63)) & mask) for x in range(5)]
            s = [[s[x][y] ^ d[x] for y in range(5)] for x in range(5)]
            b = [[0] * 5 for _ in range(5)]
            for x in range(5):
                for y in range(5):
                    v, n = s[x][y], rot[x][y]
                    b[y][(2 * x + 3 * y) % 5] = ((v << n) | (v >> (64 - n))) & mask if n else v
            s = [[b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]) for y in range(5)] for x in range(5)]
            s[0][0] ^= r
    return b"".join(s[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


def beacon_nonce_slot(index):
    base = int.from_bytes(keccak256(BEACONS_SLOT.to_bytes(32, "big")), "big")
    return word(base + 2 * index + 1)


def plan_from_logs(paths):
    plan = []
    for path in paths:
        name = path.rsplit("/", 1)[-1].split("-latest.json")[0]
        stage = name if name in STAGES else name.split("-")[0]
        for t in json.load(open(path))["transactions"]:
            tx = t["transaction"]
            what = t.get("function") or ("create " + (t.get("contractName") or "")) if t["transactionType"] in (
                "CREATE", "CREATE2") else (t.get("contractName") or "") + " " + (t.get("function") or "")
            plan.append({"stage": stage, "what": what.strip().split("(")[0], "from": tx["from"], "to": tx.get("to"),
                         "input": tx.get("input") or tx.get("data") or "0x", "value": tx.get("value") or "0x0",
                         "address": t.get("contractAddress")})
    return plan


def call_of(tx, gas=None):
    c = {"from": tx["from"], "input": tx["input"], "value": tx["value"]}
    if tx.get("to"):
        c["to"] = tx["to"]
    if gas is not None:
        c["gas"] = hex(gas)
    return c


class Sim:
    """The planned transactions as blocks of eth_simulateV1, and the bisection of each one's least gas limit."""

    def __init__(self, node, plan, blocks, probe_gas):
        # blocks: [{"time": seconds after the base block or None, "overrides": {...} or None, "txs": [index...]}]
        # probe_gas: the limit of a call whose own limit is not being searched (a node runs calls with at most
        # its RPC gas cap, 50M by default in reth and geth; left to the node, a call's default can be too low)
        self.node, self.plan, self.blocks, self.probe_gas = node, plan, blocks, probe_gas
        self.base = node("eth_getBlockByNumber", "latest", False)

    def run(self, upto=None, gas=None):
        """Simulate the blocks up to the transaction index `upto` (all when None); `gas` maps index -> limit."""
        gas = gas or {}
        req = []
        t0 = int(self.base["timestamp"], 16)
        for b in self.blocks:
            txs = [i for i in b["txs"] if upto is None or i <= upto]
            if not txs and upto is not None and min(b["txs"]) > upto:
                break
            bs = {"calls": [call_of(self.plan[i], gas.get(i, self.probe_gas)) for i in txs]}
            if b.get("time") is not None:
                bs["blockOverrides"] = {"time": hex(t0 + b["time"])}
            if b.get("overrides"):
                bs["stateOverrides"] = b["overrides"]
            req.append(bs)
        out = self.node("eth_simulateV1", {"blockStateCalls": req, "validation": False, "traceTransfers": False,
                                           "returnFullTransactions": False}, self.base["number"])
        res = {}
        for b, blk in zip(self.blocks, out):
            txs = [i for i in b["txs"] if upto is None or i <= upto]
            for i, c in zip(txs, blk["calls"]):
                res[i] = {"status": int(c["status"], 16), "gasUsed": int(c["gasUsed"], 16),
                          "maxUsedGas": int(c.get("maxUsedGas", c["gasUsed"]), 16),
                          "error": (c.get("error") or {}).get("message"), "returnData": c.get("returnData", "0x"),
                          "block": int(blk["number"], 16), "time": int(blk["timestamp"], 16)}
        return res

    def least_gas(self, i, first, cap):
        """The least gas limit with which transaction i succeeds after its predecessors (to 0.1 %)."""
        ok = lambda g: self.run(upto=i, gas={i: g})[i]["status"] == 1
        lo = max(first[i]["gasUsed"] - 1, 21000)
        hi = math.ceil(first[i]["maxUsedGas"] * 64 / 63) + 2300
        while not ok(hi):
            lo, hi = hi, math.ceil(hi * 1.25)
            if hi > cap:
                return None
        if ok(lo):
            return lo
        while hi - lo > max(64, hi // 1000):
            mid = (lo + hi) // 2
            lo, hi = (lo, mid) if ok(mid) else (mid, hi)
        return hi


def measure(node, plan, blocks, margin, probe_gas, estimate_first=()):
    head = node("eth_getBlockByNumber", "latest", False)
    block_gas = int(head["gasLimit"], 16)
    cap = min(TX_MAX_TOTAL, block_gas)
    sim = Sim(node, plan, blocks, probe_gas)
    first = sim.run()
    rows, failures = [], []
    for i, tx in enumerate(plan):
        r = first[i]
        row = {"stage": tx["stage"], "what": tx["what"], "to": tx.get("to"), "status": r["status"],
               "gasUsed": r["gasUsed"], "maxUsedGas": r["maxUsedGas"], "error": r["error"]}
        if r["status"] != 1:
            failures.append(f"{i} {tx['what']}: fails on the node ({r['error']})")
            rows.append(row)
            continue
        least = sim.least_gas(i, first, min(cap, probe_gas))
        row["leastLimit"] = least
        if least is None:
            failures.append(f"{i} {tx['what']}: no limit up to {cap} lets it succeed")
        else:
            row["limit"] = math.ceil(least * margin)
            if row["limit"] > cap:
                failures.append(f"{i} {tx['what']}: limit {row['limit']} with the margin is over the cap {cap}")
            row["overExecutionCap"] = row["limit"] > TX_MAX_GAS
        if i in estimate_first:
            try:
                row["estimateGas"] = int(node("eth_estimateGas", call_of(tx), "latest"), 16)
            except RpcError as e:
                row["estimateGas"] = None
                failures.append(f"{i} {tx['what']}: eth_estimateGas fails: {e}")
        rows.append(row)
    if not failures:  # everything again, in order, with the limits that will be used
        again = sim.run(gas={i: r["limit"] for i, r in enumerate(rows)})
        for i, r in enumerate(rows):
            r["withLimit"] = {"status": again[i]["status"], "gasUsed": again[i]["gasUsed"]}
            if again[i]["status"] != 1:
                failures.append(f"{i} {r['what']}: fails with its limit {r['limit']} ({again[i]['error']})")
    return {"block": int(head["number"], 16), "blockGasLimit": block_gas, "cap": cap, "executionCap": TX_MAX_GAS,
            "margin": margin, "rpcCalls": node.calls, "transactions": rows, "failures": failures}


def report(result, out):
    if out:
        with open(out, "w") as f:
            json.dump(result, f, indent=1)
    print(f"node block {result['block']}, block gas limit {result['blockGasLimit']:,}, cap {result['cap']:,}, "
          f"margin {result['margin']}")
    for i, r in enumerate(result["transactions"]):
        lim = r.get("limit")
        print(f"  {i:2} {r['stage']:8} {r['what'][:34]:34} used {r['gasUsed']:>12,}  least limit "
              f"{r.get('leastLimit') or 0:>12,}  limit {lim or 0:>12,}"
              + (f"  eth_estimateGas {r['estimateGas']:,}" if r.get("estimateGas") else "")
              + ("  (over 2^24: EIP-8037 state gas)" if r.get("overExecutionCap") else "")
              + ("" if r["status"] == 1 else f"  FAILS: {r['error']}"))
    total = sum(r["gasUsed"] for r in result["transactions"])
    print(f"  total gas used {total:,}")
    for f in result["failures"]:
        print("FAIL: " + f)
    return 1 if result["failures"] else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan")
    p.add_argument("logs", nargs="+")
    p.add_argument("--out", required=True)
    for name in ("check", "replay"):
        p = sub.add_parser(name)
        p.add_argument("--rpc", required=True)
        p.add_argument("--plan", required=True)
        p.add_argument("--margin", type=float, default=1.25)
        p.add_argument("--out")
        p.add_argument("--probe-gas", type=int, default=50_000_000, help="the node's RPC gas cap")
    p.add_argument("--nonce", type=int)
    p.add_argument("--beacon-index", type=int)
    p.add_argument("--echo-nonce")
    p.add_argument("--wait", type=int, default=3600, help="seconds from submit to the beacon (at least W)")
    p.add_argument("--release-wait", type=int, default=600, help="seconds from finalize to the releases")
    a = ap.parse_args()

    if a.cmd == "plan":
        plan = plan_from_logs(a.logs)
        with open(a.out, "w") as f:
            json.dump(plan, f, indent=1)
        print(f"{len(plan)} transactions: " + ", ".join(f"{t['stage']}:{t['what']}" for t in plan))
        return 0

    node = Node(a.rpc)
    plan = json.load(open(a.plan))
    if a.cmd == "check":
        blocks = [{"time": None, "txs": list(range(len(plan)))}]
        return report(measure(node, plan, blocks, a.margin, a.probe_gas, estimate_first=(0,)), a.out)

    sender = plan[0]["from"]
    by_stage = {s: [i for i, t in enumerate(plan) if t["stage"] == s] for s in STAGES}
    offsets = {"deploy": 12, "submit": 24, "beacon": 24 + a.wait, "finalize": 36 + a.wait,
               "release": 36 + a.wait + a.release_wait}
    blocks = []
    for s in STAGES:
        if not by_stage[s]:
            continue
        b = {"time": offsets[s], "txs": by_stage[s], "overrides": {}}
        if s == STAGES[0] and a.nonce is not None:
            b["overrides"][sender] = {"nonce": hex(a.nonce)}
        if s == "finalize" and a.echo_nonce:
            vault = plan[by_stage[s][0]]["to"]
            b["overrides"][vault] = {"stateDiff": {beacon_nonce_slot(a.beacon_index): a.echo_nonce}}
        blocks.append(b)
    creations = tuple(i for i, t in enumerate(plan) if not t.get("to"))[:1]
    return report(measure(node, plan, blocks, a.margin, a.probe_gas, estimate_first=creations), a.out)


if __name__ == "__main__":
    sys.exit(main())
