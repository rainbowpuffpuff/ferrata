#!/usr/bin/env python3
"""The gas of the round trip's transactions as the node prices them, measured on the node itself.

Since Glamsterdam (Sepolia, 6 October 2026) a new account, a byte of code and a new storage slot cost
"state gas" at 1,530 gas per state byte (EIP-8037), which Foundry's EVM does not know: forge and anvil
price a contract creation about six times too low. So no gas limit of this run comes from Foundry.

  gasplan.py plan <forge log>... --out plan.json
        the transactions of forge's logs (broadcast/<script>/<chain>/[dry-run/]<stage>-latest.json), in order
  gasplan.py run --node URL --state URL --plan plan.json [--margin 1.25] [--out result.json]
                 [--nonce N] [--beacon-index I --echo-nonce X] [--wait S] [--release-wait S] [--keep]
        plays the planned transactions in order on `--state`, an anvil fork that holds the chain's state (a fresh
        fork of the node before a stage is sent, or the fork of a rehearsal), and before each one asks the node
        `--node` (Sepolia itself) for its gas on that state: eth_estimateGas with the state and the block's time
        as overrides (the node enforces the 2^24 execution cap there: a transaction over it has no estimate), then
        eth_simulateV1 of the transaction alone with its limit, estimate x margin, for the gas it uses. anvil's
        receipt gives Foundry's gas of the same transaction. Exit 1 if a transaction has no estimate, fails, or
        its limit is over the cap. The fork is put back as it was (evm_revert) unless --keep.
        For a replay of a whole rehearsal: --nonce sets the sender's nonce first (the creations then land on the
        addresses the proofs bind), the waits move the fork's clock before the beacon and the releases, and
        --echo-nonce writes the beacon the finalisation proof echoes into the vault's storage at --beacon-index
        (its nonce hashes the block's number and prevrandao, which a replay does not reproduce).

Caps (EIP-8037 with EIP-7825): a transaction's execution gas is at most 2^24, its gas limit at most 2^32 - 1 and
at most the block's gas limit. Nothing is signed or sent to the node: only the read-only methods of READ_ONLY.
"""
import argparse
import gzip
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


class Fork:
    """The anvil fork that holds the state: the sender impersonated, the transactions played in order."""

    def __init__(self, url):
        self.url, self.calls = url, 0

    def __call__(self, method, *params):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
        req = urllib.request.Request(self.url, body, {"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=300) as r:
            out = json.load(r)
        self.calls += 1
        if "error" in out:
            raise RpcError(f"{method}: " + json.dumps(out["error"])[:400])
        return out["result"]

    def accounts(self):
        raw = bytes.fromhex(self("anvil_dumpState")[2:])
        return json.loads(gzip.decompress(raw))["accounts"]

    def overrides(self, addresses):
        """The fork's state of these accounts as node overrides. Storage as stateDiff: the fork holds only the slots
        it wrote or read, so the node keeps every other slot of an account that exists on chain."""
        dump = {a.lower(): v for a, v in self.accounts().items()}
        out = {}
        for a in sorted({a.lower() for a in addresses if a}):
            acct = dump.get(a)
            if acct is None:
                continue
            nonce = acct.get("nonce", 0)
            o = {"balance": hex(int(str(acct.get("balance", "0x0")), 0)),
                 "nonce": hex(nonce if isinstance(nonce, int) else int(nonce, 0))}
            if acct.get("code") not in (None, "0x", ""):
                o["code"] = acct["code"]
            storage = {k: v for k, v in (acct.get("storage") or {}).items()}
            if storage:
                o["stateDiff"] = {word(int(k, 16)): word(int(v, 16)) for k, v in storage.items()}
            out[a] = o
        return out


def run(node, fork, plan, a):
    head = node("eth_getBlockByNumber", "latest", False)
    block_gas = int(head["gasLimit"], 16)
    cap = min(TX_MAX_TOTAL, block_gas)
    sender = plan[0]["from"]
    snapshot = fork("evm_snapshot")
    fork("anvil_impersonateAccount", sender)
    if a.nonce is not None:
        fork("anvil_setNonce", sender, hex(a.nonce))
    touched = {sender} | {t["to"] for t in plan if t.get("to")}
    rows, failures, stage = [], [], None
    try:
        for i, tx in enumerate(plan):
            jump = 0
            if tx["stage"] != stage:
                stage = tx["stage"]
                jump = {"beacon": a.wait, "release": a.release_wait}.get(stage, 0)
                if stage == "finalize" and a.echo_nonce:
                    fork("anvil_setStorageAt", tx["to"], beacon_nonce_slot(a.beacon_index), a.echo_nonce)
            latest = fork("eth_getBlockByNumber", "latest", False)
            # the next block's time: after the fork's last block and after the node's (eth_simulateV1 checks it)
            node_head = node("eth_getBlockByNumber", "latest", False)
            when = max(int(latest["timestamp"], 16), int(node_head["timestamp"], 16)) + 12 + jump
            fork("evm_setNextBlockTimestamp", hex(when))
            state = fork.overrides(touched)
            row = {"stage": tx["stage"], "what": tx["what"], "to": tx.get("to"), "time": when}
            try:
                est = int(node("eth_estimateGas", call_of(tx), "latest", state, {"time": hex(when)}), 16)
            except RpcError as e:
                est, row["error"] = None, str(e)
                failures.append(f"{i} {tx['what']}: the node gives no estimate: {e}")
            row["estimateGas"] = est
            if est is not None:
                limit = math.ceil(est * a.margin)
                row["limit"] = limit
                row["overExecutionCap"] = limit > TX_MAX_GAS
                if limit > cap:
                    failures.append(f"{i} {tx['what']}: limit {limit:,} with the margin is over the cap {cap:,}")
                sim = node("eth_simulateV1", {"blockStateCalls": [{"blockOverrides": {"time": hex(when)},
                    "stateOverrides": state, "calls": [call_of(tx, min(limit, a.probe_gas))]}],
                    "validation": False}, "latest")[0]["calls"][0]
                row["gasUsed"] = int(sim["gasUsed"], 16)
                row["status"] = int(sim["status"], 16)
                if row["status"] != 1:
                    failures.append(f"{i} {tx['what']}: fails on the node with its limit "
                                    f"({(sim.get('error') or {}).get('message')})")
            # play it on the fork: the state for the next one, and Foundry's gas of it
            sent = {k: v for k, v in call_of(tx, 30_000_000).items()}
            h = fork("eth_sendTransaction", sent)
            rc = None
            for k in range(40):  # automine answers before the block is in; mine by hand if it is not
                rc = fork("eth_getTransactionReceipt", h)
                if rc:
                    break
                if k == 10:
                    fork("evm_mine")
                time.sleep(0.25)
            if rc is None:
                raise RpcError(f"the fork did not mine {h}")
            row["foundryGasUsed"] = int(rc["gasUsed"], 16)
            if int(rc["status"], 16) != 1:
                failures.append(f"{i} {tx['what']}: reverts on the fork")
            if rc.get("contractAddress"):
                touched.add(rc["contractAddress"])
                row["address"] = rc["contractAddress"]
            rows.append(row)
            if failures:
                break
    finally:
        if not a.keep:
            fork("evm_revert", snapshot)
    return {"node": a.node, "block": int(head["number"], 16), "blockGasLimit": block_gas, "cap": cap,
            "executionCap": TX_MAX_GAS, "margin": a.margin, "nodeCalls": node.calls, "transactions": rows,
            "failures": failures}


def report(result, out):
    if out:
        with open(out, "w") as f:
            json.dump(result, f, indent=1)
    print(f"node block {result['block']}, block gas limit {result['blockGasLimit']:,}, cap {result['cap']:,}, "
          f"margin {result['margin']}")
    print(f"  {'#':>2} {'stage':8} {'transaction':32} {'Foundry used':>13} {'node used':>12} {'node estimate':>14}"
          f" {'limit':>12}  node/Foundry")
    for i, r in enumerate(result["transactions"]):
        f, n = r.get("foundryGasUsed") or 0, r.get("gasUsed") or 0
        print(f"  {i:2} {r['stage']:8} {r['what'][:32]:32} {f:>13,} {n:>12,} {r.get('estimateGas') or 0:>14,}"
              f" {r.get('limit') or 0:>12,}  {n / f if f else 0:5.2f}"
              + ("  limit > 2^24 (EIP-8037 state gas)" if r.get("overExecutionCap") else ""))
    tf = sum(r.get("foundryGasUsed") or 0 for r in result["transactions"])
    tn = sum(r.get("gasUsed") or 0 for r in result["transactions"])
    print(f"  total: Foundry {tf:,}, node {tn:,}" + (f" ({tn / tf:.2f}x)" if tf else ""))
    for f in result["failures"]:
        print("FAIL: " + f)
    return 1 if result["failures"] else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan")
    p.add_argument("logs", nargs="+")
    p.add_argument("--out", required=True)
    p = sub.add_parser("run")
    p.add_argument("--node", required=True, help="the node whose gas counts (Sepolia)")
    p.add_argument("--state", required=True, help="an anvil fork holding the state before the first transaction")
    p.add_argument("--plan", required=True)
    p.add_argument("--margin", type=float, default=1.25)
    p.add_argument("--probe-gas", type=int, default=50_000_000, help="the node's RPC gas cap")
    p.add_argument("--out")
    p.add_argument("--nonce", type=int)
    p.add_argument("--beacon-index", type=int)
    p.add_argument("--echo-nonce")
    p.add_argument("--wait", type=int, default=0, help="seconds the clock moves before the beacon (at least the wait W)")
    p.add_argument("--release-wait", type=int, default=0, help="seconds it moves before the releases")
    p.add_argument("--keep", action="store_true", help="leave the transactions on the fork")
    a = ap.parse_args()
    if a.cmd == "plan":
        plan = plan_from_logs(a.logs)
        with open(a.out, "w") as f:
            json.dump(plan, f, indent=1)
        print(f"{len(plan)} transactions: " + ", ".join(f"{t['stage']}:{t['what']}" for t in plan))
        return 0
    return report(run(Node(a.node), Fork(a.state), json.load(open(a.plan)), a), a.out)


if __name__ == "__main__":
    sys.exit(main())
