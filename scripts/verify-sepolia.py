#!/usr/bin/env python3
"""Check a Ferrata crown run on Sepolia yourself: Python 3 standard library only, any public JSON-RPC, read-only.

    python3 -I scripts/verify-sepolia.py RUN_DIR [--rpc URL] [--readme e2e/crown/README.md]

RUN_DIR holds a run's records as committed in e2e/runs/crown-rehearsal-3/ (manifest.json, deploy.json,
identity.json, params.json, run.json, crown.json, releases/, lez/session-a/, lez/session-b/). Every check reads the
chain itself and compares it with the records; nothing here signs or sends a transaction (only eth_chainId,
eth_blockNumber, eth_getBlockByNumber, eth_getCode, eth_getBalance, eth_getTransactionCount, eth_getTransactionByHash,
eth_getTransactionReceipt, eth_getLogs and eth_call). One line per check:

    PASS     the chain agrees with the record
    FAIL     it does not (the exit status is then 1; 2 if the endpoint does not answer as Sepolia)
    PENDING  the run has not reached that stage yet (on chain and in the records); not a failure

The image ids the vault must hold are pinned below (EXPECTED_IDS, the table of e2e/crown/README.md, which the script
also reads and compares when it is present); scripts/reproduce-image-ids.sh rebuilds the programs and prints the ids
they really have.
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

DEFAULT_RPC = "https://ethereum-sepolia-rpc.publicnode.com"
CHAIN_ID = 11155111
# The programs of the live run (e2e/crown/README.md, "The programs of the live run"): built at the crown build path.
EXPECTED_IDS = {
    "attestation": "0x2228bb462b83032ade1c5de62d381b8dcffe9d10d9e977dce9e9fc8da31467cf",
    "bridge": "0x0e9dcd79482f9e0b83762b965d7e1d4319ffeba8a4bdfbdff148bbeaba79bbb6",
    "checkpoint_b": "0xa475e8f9aaa41d0ae3bf9a20ae7d937ebc637a961808765f0ff175158760d3b2",
    "checkpoint": "0xcf38b945a70b66d9132fb493b4baf1db308de73ccb8b1108e1c1f8a4363525e6",
    "segment": "0xd2b30667292418ccf3cf68ee4fa2dc9ed7873de4034fcf1fb2ed3a6305a61cca",
    "release": "0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd",
}
CHECKPOINT_DOMAIN = "0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7"
# The order of deploy.sh's transactions from the deployer's first nonce N (identity.json createAddresses).
DEPLOY_STEPS = ["emergency stop", "FerrataRegistry library", "FerrataVault", "test token", "register ETH",
                "register token", "renounce stop owner"]

# ---------------------------------------------------------------------------------------------------------------
# Keccak-256 (the Ethereum hash; hashlib's sha3_256 pads differently), RLP of a CREATE address, ABI helpers.
# ---------------------------------------------------------------------------------------------------------------
_RC = [0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000, 0x000000000000808B,
       0x0000000080000001, 0x8000000080008081, 0x8000000000008009, 0x000000000000008A, 0x0000000000000088,
       0x0000000080008009, 0x000000008000000A, 0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
       0x8000000000008003, 0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
       0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008]
_ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]
_M64 = (1 << 64) - 1


def _rol(x: int, n: int) -> int:
    return ((x << n) | (x >> (64 - n))) & _M64 if n else x


def _keccak_f(a: list) -> list:
    for rc in _RC:
        c = [a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4] for x in range(5)]
        d = [c[(x - 1) % 5] ^ _rol(c[(x + 1) % 5], 1) for x in range(5)]
        a = [[a[x][y] ^ d[x] for y in range(5)] for x in range(5)]
        b = [[0] * 5 for _ in range(5)]
        for x in range(5):
            for y in range(5):
                b[y][(2 * x + 3 * y) % 5] = _rol(a[x][y], _ROT[x][y])
        a = [[b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]) for y in range(5)] for x in range(5)]
        a[0][0] ^= rc
    return a


def keccak256(data: bytes) -> bytes:
    rate = 136
    p = bytearray(data) + b"\x01"
    while len(p) % rate:
        p.append(0)
    p[-1] |= 0x80
    a = [[0] * 5 for _ in range(5)]
    for off in range(0, len(p), rate):
        for i in range(rate // 8):
            a[i % 5][i // 5] ^= int.from_bytes(p[off + 8 * i:off + 8 * i + 8], "little")
        a = _keccak_f(a)
    return b"".join(a[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


assert keccak256(b"").hex() == "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
assert keccak256(b"transfer(address,uint256)")[:4].hex() == "a9059cbb"


def create_address(sender: str, nonce: int) -> str:
    """CREATE(sender, nonce) = keccak256(rlp([sender, nonce]))[12:]."""
    s = bytes.fromhex(sender[2:])
    if nonce == 0:
        n = b"\x80"
    elif nonce < 0x80:
        n = bytes([nonce])
    else:
        raw = nonce.to_bytes((nonce.bit_length() + 7) // 8, "big")
        n = bytes([0x80 + len(raw)]) + raw
    payload = b"\x94" + s + n
    return "0x" + keccak256(bytes([0xC0 + len(payload)]) + payload)[12:].hex()


def b(x: str) -> bytes:
    return bytes.fromhex(x.lower().removeprefix("0x"))


def h(x: bytes) -> str:
    return "0x" + x.hex()


def words(data: str) -> list:
    raw = b(data)
    return [h(raw[i:i + 32]) for i in range(0, len(raw), 32)]


def selector(sig: str) -> str:
    return keccak256(sig.encode()).hex()[:8]


def topic(sig: str) -> str:
    return h(keccak256(sig.encode()))


def enc_word(v) -> str:
    if isinstance(v, int):
        return v.to_bytes(32, "big").hex()
    raw = b(v)
    return (bytes(32 - len(raw)) + raw).hex() if len(raw) <= 32 else raw.hex()


# ---------------------------------------------------------------------------------------------------------------
# JSON-RPC
# ---------------------------------------------------------------------------------------------------------------
class RpcError(Exception):
    pass


class Rpc:
    def __init__(self, url: str):
        self.url, self.n = url, 0

    def __call__(self, method: str, *params):
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
        for attempt in range(5):
            req = urllib.request.Request(self.url, data=body, headers={
                "Content-Type": "application/json", "User-Agent": "ferrata-verify-sepolia/1"})
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    out = json.load(r)
                self.n += 1
                if "error" in out:
                    raise RpcError(f"{method}: {out['error']}")
                return out["result"]
            except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
                if attempt == 4:
                    raise RpcError(f"{method}: {e}") from e
                time.sleep(1.5 * (attempt + 1))

    def call(self, to: str, sig: str, *args, block: str = "latest") -> str:
        data = "0x" + selector(sig) + "".join(enc_word(a) for a in args)
        return self("eth_call", {"to": to, "data": data}, block)

    def call_ok(self, to: str, data: str) -> tuple[bool, str]:
        """eth_call that may revert: (succeeded, return data or the error)."""
        try:
            return True, self("eth_call", {"to": to, "data": data}, "latest")
        except RpcError as e:
            return False, str(e)[:160]

    def logs(self, address: str, topic0: str, from_block: int, to_block: int) -> list:
        out, lo, step = [], from_block, 20000
        while lo <= to_block:
            hi = min(lo + step - 1, to_block)
            try:
                out += self("eth_getLogs", {"address": address, "topics": [topic0],
                                            "fromBlock": hex(lo), "toBlock": hex(hi)})
                lo = hi + 1
            except RpcError:
                if step <= 500:
                    raise
                step //= 4
        return out


def verify_call(seal: str, image_id: str, journal_digest: str) -> str:
    """Calldata of IRiscZeroVerifier.verify(bytes seal, bytes32 imageId, bytes32 journalDigest)."""
    raw = b(seal)
    pad = (32 - len(raw) % 32) % 32
    return ("0x" + selector("verify(bytes,bytes32,bytes32)") + enc_word(0x60) + enc_word(image_id)
            + enc_word(journal_digest) + enc_word(len(raw)) + raw.hex() + "00" * pad)


# ---------------------------------------------------------------------------------------------------------------
# The run's identity values (the same formulas as e2e/crown/identity.py and lez/bridge-wire's BridgeConfig::id)
# ---------------------------------------------------------------------------------------------------------------
def genesis_state(tip_id: int, tip_hash: bytes, keys: list, timeframe: int, config_tip_hash: bytes) -> bytes:
    key_set = hashlib.sha256(b"ferrata/keyset/v2" + len(keys).to_bytes(4, "big") + timeframe.to_bytes(4, "big")
                             + b"".join(keys)).digest()
    frontier = hashlib.sha256(b"ferrata/burn-frontier/v1" + bytes(32 * 32)).digest()
    return hashlib.sha256(b"ferrata/lez-state/v1" + tip_id.to_bytes(8, "big") + tip_hash + key_set + config_tip_hash
                          + (0).to_bytes(8, "big") + frontier + (0).to_bytes(8, "big") + b"\x00").digest()


def commitment(chain_id: int, vault: bytes, depositor: bytes, j: int, kind: int, dest: bytes) -> bytes:
    """Deposit commitment under the run's PUBLISHED TEST seed (bridge/e2e's); a real user's seed is secret."""
    seed = hashlib.sha256(b"e2e: user bridge seed (test value)").digest()
    k_auth = hmac.new(seed, b"ferrata/auth/v1", hashlib.sha256).digest()
    s = hmac.new(k_auth, b"ferrata/s/v1" + chain_id.to_bytes(32, "big") + vault + depositor + j.to_bytes(8, "big"),
                 hashlib.sha256).digest()
    return hashlib.sha256(b"ferrata/C/v1" + s + bytes([kind]) + dest).digest()


def readme_ids(path: str) -> dict:
    """The image-id table of e2e/crown/README.md ("| Program | Image id |")."""
    names = [("attestation program", "attestation"), ("bridge program", "bridge"), ("`checkpoint_b`", "checkpoint_b"),
             ("`checkpoint`", "checkpoint"), ("`segment`", "segment"), ("release", "release")]
    out = {}
    for line in open(path, encoding="utf-8"):
        m = re.match(r"^\|\s*(.+?)\s*\|\s*`(0x[0-9a-f]{64})`\s*\|\s*$", line)
        if m:
            for prefix, key in names:
                if m.group(1).startswith(prefix) and key not in out:
                    out[key] = m.group(2)
                    break
    return out


# ---------------------------------------------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------------------------------------------
class Pending(Exception):
    pass


class Failed(Exception):
    pass


RESULTS: list = []


def check(name: str):
    def run(fn):
        try:
            status, detail = "PASS", fn()
        except Pending as e:
            status, detail = "PENDING", str(e)
        except Failed as e:
            status, detail = "FAIL", str(e)
        except (RpcError, KeyError, ValueError, TypeError, OSError, IndexError) as e:
            status, detail = "FAIL", f"{type(e).__name__}: {e}"
        RESULTS.append(status)
        print(f"{status:<8} {name}: {detail}", flush=True)
        return fn
    return run


def need(cond: bool, msg: str) -> None:
    if not cond:
        raise Failed(msg)


def load(path: str):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("run", help="the run's records (e.g. e2e/runs/crown-live-1)")
    ap.add_argument("--rpc", default=os.environ.get("SEPOLIA_RPC_URL") or DEFAULT_RPC)
    here = os.path.dirname(os.path.abspath(__file__))
    ap.add_argument("--readme", default=os.path.join(here, "..", "e2e", "crown", "README.md"))
    a = ap.parse_args()
    R = a.run.rstrip("/")
    rpc = Rpc(a.rpc)
    opt = lambda p: load(os.path.join(R, p)) if os.path.isfile(os.path.join(R, p)) else None  # noqa: E731
    run, idn, dep, params = opt("run.json") or {}, load(os.path.join(R, "identity.json")), opt("deploy.json"), opt("params.json")
    crown, man = opt("crown.json") or {}, (opt("manifest.json") or {}).get("stages", {})
    sa, sb = opt("lez/session-a/session-a.json"), opt("lez/session-b/session-b.json")
    notes = opt("lez/session-b/burn-notes.json")
    vault, deployer = idn["vault"].lower(), idn["deployer"].lower()
    first_nonce = int(idn["firstNonce"])
    from_block = int(run.get("deploy_from_block") or 0)
    print(f"run {os.path.basename(R)} ({run.get('mode', '?')}), vault {idn['vault']}, deployer {idn['deployer']}, rpc {a.rpc}")
    state: dict = {}

    def vcall(sig: str, *args) -> str:
        return rpc.call(vault, sig, *args)

    def word(sig: str, *args, i: int = 0) -> str:
        return words(vcall(sig, *args))[i]

    @check("chain")
    def _():
        cid = int(rpc("eth_chainId"), 16)
        need(cid == CHAIN_ID == int(idn["chainId"]), f"eth_chainId {cid}, records {idn['chainId']}")
        head = int(rpc("eth_blockNumber"), 16)
        fin = rpc("eth_getBlockByNumber", "finalized", False)
        state["head"], state["finalized"] = head, int(fin["number"], 16)
        return f"Sepolia (chain id {cid}), head {head}, finalized {state['finalized']}"

    if RESULTS[-1] != "PASS":
        print(f"the endpoint {a.rpc} does not answer as Sepolia; nothing else checked (try another --rpc)")
        return 2

    @check("image ids: e2e/crown/README.md's table = the ids pinned here = the run's records")
    def _():
        notes_ = []
        if os.path.isfile(a.readme):
            t = readme_ids(a.readme)
            need(t == EXPECTED_IDS, f"README table {t} differs from the pinned ids")
            notes_.append("README table equal")
        else:
            notes_.append("README not found (pinned ids used)")
        if params:
            need(params["checkpointImageIdB"].lower() == EXPECTED_IDS["checkpoint_b"], "params checkpointImageIdB")
            need(params["checkpointImageIdA"].lower() == EXPECTED_IDS["checkpoint"], "params checkpointImageIdA")
            need(params["releaseImageId"].lower() == EXPECTED_IDS["release"], "params releaseImageId")
            notes_.append("params.json equal")
        if sa:
            need(sa["attestation"]["imageId"].lower() == EXPECTED_IDS["attestation"], "session A attestation imageId")
            need(sa["bridge"]["imageId"].lower() == EXPECTED_IDS["bridge"], "session A bridge imageId")
            notes_.append("LEZ session A's deployed programs equal")
        ids = os.path.join(R, "programs", "ids.jsonl")
        if os.path.isfile(ids):
            for line in open(ids, encoding="utf-8"):
                if line.strip():
                    row = json.loads(line)
                    key = row["program"].removesuffix(".bin")
                    need(row["image_id"].lower() == EXPECTED_IDS.get(key), f"programs/ids.jsonl {key} {row['image_id']}")
            notes_.append("programs/ids.jsonl equal")
        return ", ".join(notes_)

    @check("deployment: every CREATE address from the deployer's nonces, code where a contract was created")
    def _():
        if dep is None:
            raise Pending("no deploy.json yet")
        need(bool(dep.get("complete")), "deploy.json: the deployment is not complete")
        addrs = [x.lower() for x in idn["createAddresses"]]
        out = []
        for i, (step, addr) in enumerate(zip(DEPLOY_STEPS, addrs)):
            want = create_address(deployer, first_nonce + i)
            need(want == addr, f"{step}: CREATE(deployer, {first_nonce + i}) = {want}, recorded {addr}")
            code = rpc("eth_getCode", addr, "latest")
            has = len(code) > 2
            need(has == (i < 4), f"{step} at {addr}: code {'present' if has else 'absent'}")
            if i < 4:
                out.append(f"{step} {addr[:10]}… nonce {first_nonce + i} ({(len(code) - 2) // 2} B)")
        named = {"emergencyStop": 0, "registry": 1, "vault": 2, "token": 3}
        for k, i in named.items():
            need(dep[k].lower() == addrs[i], f"deploy.json {k} {dep[k]} is not CREATE nonce {first_nonce + i}")
        need(vault == addrs[2], "identity vault is not the third CREATE address")
        return "; ".join(out) + "; nonces N+4..N+6 are calls (no code)"

    @check("vault immutables (eth_call) = README ids, configId, genesisState, rule (B), verifier, registry library")
    def _():
        code = rpc("eth_getCode", vault, "latest")
        need(len(code) > 2, "no runtime code at the vault")
        cp = word("checkpointImageId()")
        rl = word("releaseImageId()")
        need(cp == EXPECTED_IDS["checkpoint_b"], f"checkpointImageId {cp} is not checkpoint_b's")
        need(rl == EXPECTED_IDS["release"], f"releaseImageId {rl} is not release's")
        mode = int(word("mode()"), 16)
        need(mode == 1, f"mode {mode}: not rule (B)")
        cfg, stored = word("lezConfigId()"), word("storedLezConfigId()")
        need(cfg == stored == idn["configId"].lower(), f"lezConfigId {cfg}, slot 2 {stored}, identity {idn['configId']}")
        gs = word("genesisState()")
        need(gs == idn["genesisState"].lower(), f"genesisState {gs}, identity {idn['genesisState']}")
        if params:
            need(cfg == params["lezConfigId"].lower() and gs == params["genesisState"].lower(), "params.json differs")
            need(int(word("nMin()"), 16) == params["nMin"] and int(word("tMin()"), 16) == params["tMin"], "nMin/tMin")
        ver = "0x" + word("verifier()")[-40:]
        if dep:
            need(ver == dep["emergencyStop"].lower(), f"verifier {ver} is not the deployment's emergency stop")
            inner = "0x" + words(rpc.call(ver, "verifier()"))[0][-40:]
            need(inner == dep["groth16Verifier"].lower() == idn["groth16Verifier"].lower(),
                 f"the emergency stop wraps {inner}, records {dep['groth16Verifier']}")
            reg = dep["registry"].lower()
            need(reg[2:] in code.lower(), "the registry library's address is not linked into the vault's code")
            need(len(rpc("eth_getCode", reg, "latest")) > 2, "no code at the registry library")
            fr = "0x" + word("feeRecipient()")[-40:]
            need(fr == dep["feeRecipient"].lower(), f"feeRecipient {fr}")
        state["verifier"] = ver
        return (f"code {(len(code) - 2) // 2} B; checkpointImageId = checkpoint_b {cp[:10]}…; releaseImageId = release "
                f"{rl[:10]}…; mode B; lezConfigId = slot 2 = {cfg[:10]}…; genesisState {gs[:10]}…; verifier = emergency "
                f"stop {ver[:10]}… over RISC Zero's Groth16 verifier; FerrataRegistry linked")

    @check("configId and genesisState recomputed from LEZ session A's records")
    def _():
        if not sa:
            raise Pending("no lez/session-a/session-a.json")
        pre = (b"ferrata/config/v1" + b(sa["bedrockGenesisId"]) + b(sa["channel"]["id"]) + b(sa["bridge"]["header"])
               + b(sa["bridge"]["imageId"]) + int(sa["evmChainId"]).to_bytes(32, "big") + b(vault)
               + b(sa["attestationRootId"]) + b(sa["salt"]))
        need(len(pre) == 261, f"preimage of {len(pre)} bytes")
        cid = h(hashlib.sha256(pre).digest())
        need(cid == idn["configId"].lower(), f"sha256(config preimage) {cid}, identity {idn['configId']}")
        ds = os.path.join(R, "lez/session-a/deployment-settings.yaml")
        extra = ""
        if os.path.isfile(ds):
            with open(ds, "rb") as f:
                got = h(hashlib.sha256(f.read()).digest())
            need(got == sa["bedrockGenesisId"].lower(), f"sha256(deployment settings) {got}")
            extra = "; bedrockGenesisId = sha256(deployment-settings.yaml)"
        ch = sa["channel"]
        gs = h(genesis_state(int(sa["tip"]["id"]), b(sa["tip"]["hash"]), [b(k) for k in ch["keys"]],
                             int(ch["postingTimeframe"]), b(ch["configTipHash"])))
        need(gs == idn["genesisState"].lower(), f"genesisState recomputed {gs}, identity {idn['genesisState']}")
        return f"configId {cid[:10]}… (bridge image {sa['bridge']['imageId'][:10]}…), genesisState at T0 = {sa['tip']['id']}{extra}"

    txs = [(stage, t) for stage, rows in man.items() for t in rows]

    @check("every recorded transaction: status, block, sender, target, gas used")
    def _():
        if not txs:
            raise Pending("manifest.json has no transactions yet")
        out = []
        for stage, t in txs:
            rc = rpc("eth_getTransactionReceipt", t["hash"])
            need(rc is not None, f"{stage} {t['hash']}: no receipt on chain")
            tx = rpc("eth_getTransactionByHash", t["hash"])
            need(rc["status"] == "0x1" == t["status"], f"{stage}: status {rc['status']}")
            need(int(rc["blockNumber"], 16) == int(t["blockNumber"], 16), f"{stage}: block {int(rc['blockNumber'], 16)}")
            need(rc["from"].lower() == tx["from"].lower() == deployer, f"{stage}: from {rc['from']}")
            need((rc["to"] or "").lower() == (t.get("to") or "").lower() == vault, f"{stage}: to {rc['to']}")
            need(int(rc["gasUsed"], 16) == int(t["gasUsed"], 16), f"{stage}: gas used {int(rc['gasUsed'], 16)}")
            fin = int(rc["blockNumber"], 16) <= state.get("finalized", -1)
            out.append(f"{stage} block {int(rc['blockNumber'], 16)} gas {int(rc['gasUsed'], 16):,}{'' if fin else ' (not final yet)'}")
            state.setdefault("tx", {})[stage] = (rc, tx)
        return f"{len(txs)} transactions: " + "; ".join(out)

    @check("deployer's nonce = first nonce + 7 deployment transactions + the recorded ones")
    def _():
        now = int(rpc("eth_getTransactionCount", deployer, "latest"), 16)
        want = first_nonce + len(DEPLOY_STEPS) + len(txs)
        need(now >= want, f"nonce {now} < {want}: the records list transactions the chain does not have")
        return f"nonce {now} = {first_nonce} + 7 + {len(txs)}" + ("" if now == want else f" (+{now - want} later, not in these records)")

    dep_logs: list = []

    @check("deposits: Deposit events, commitments recomputed, leaf count, escrow")
    def _():
        want = crown.get("deposits") or []
        if not want:
            raise Pending("no deposits recorded")
        dep_logs.extend(rpc.logs(vault, topic("Deposit(address,uint64,uint64,address,uint256,bytes32)"), from_block,
                                 state.get("head") or int(rpc("eth_blockNumber"), 16)))
        need(len(dep_logs) == len(want), f"{len(dep_logs)} Deposit events, {len(want)} recorded")
        out, total_value = [], 0
        for lg, d in zip(dep_logs, want):
            w = words(lg["data"])
            j, leaf, amount, c = int(w[0], 16), int(w[1], 16), int(w[2], 16), w[3]
            need("0x" + lg["topics"][1][-40:] == deployer, f"deposit {j}: depositor")
            need("0x" + lg["topics"][2][-40:] == "0x" + "00" * 20, f"deposit {j}: not ETH")
            kind = 2 if d["kind"] == "public" else 1
            mine = h(commitment(CHAIN_ID, b(vault), b(deployer), j, kind, b(d["dest"])))
            need(c == mine == d["commitment"].lower(), f"deposit {j}: event {c}, recomputed {mine}, record {d['commitment']}")
            need(j == d["j"] and leaf == j, f"deposit {j}: leaf {leaf}")
            tx = rpc("eth_getTransactionByHash", lg["transactionHash"])
            total_value += int(tx["value"], 16)
            out.append(f"j{j} leaf {leaf} {amount} wei {d['kind']} (tx value {int(tx['value'], 16)})")
        n = int(word("nextLeafIndex()"), 16)
        need(n == len(want), f"nextLeafIndex {n}")
        state["deposited"] = total_value
        bal = int(rpc("eth_getBalance", vault, "latest"), 16)
        state["balance"] = bal
        return "; ".join(out) + f"; nextLeafIndex {n}; deposits sent {total_value} wei; vault balance {bal} wei"

    jw = words(crown["candidate"]["journal"]) if crown.get("candidate") else []

    @check("checkpoint journal fields = the vault, the identity and the LEZ records")
    def _():
        if not jw:
            raise Pending("no checkpoint candidate recorded")
        need(len(jw) == 17, f"{len(jw)} words")
        need(jw[0] == CHECKPOINT_DOMAIN and int(jw[1], 16) == CHAIN_ID, "domain or chain id")
        need(jw[2][-40:] == vault[2:] and jw[3] == idn["configId"].lower(), "vault or configId")
        need(int(jw[4], 16) == 1 and int(jw[5], 16) == 1, "nMin, tMin")
        need(jw[6] == idn["genesisState"].lower(), "parent state is not genesisState")
        n_dep = len(crown.get("deposits") or [])
        need(int(jw[10], 16) == n_dep, f"imported leaves {int(jw[10], 16)}, deposits {n_dep}")
        extra = ""
        if sb and sa:
            need(sb["tipT0"]["id"] == sa["tip"]["id"] and sb["tipT0"]["hash"].lower() == sa["tip"]["hash"].lower(),
                 "session B's T0 is not session A's tip")
            need(int(jw[13], 16) == sb["tipT1"]["id"] and jw[14] == sb["tipT1"]["hash"].lower(), "tip is not session B's T1")
            need(sb["configId"].lower() == idn["configId"].lower() and sb["vault"].lower() == vault, "session B identity")
            extra = f", LEZ blocks {sb['blocks']['from']}..={sb['blocks']['to']}"
        if notes is not None:
            need(int(jw[9], 16) == len(notes), f"burns {int(jw[9], 16)}, burn notes {len(notes)}")
            for x in notes:
                need(x["configId"].lower() == idn["configId"].lower() and x["vault"].lower() == vault, "burn note identity")
        return (f"parent = genesisState, new state {jw[7][:10]}…, tip T1 = {int(jw[13], 16)} ({jw[14][:10]}…){extra}, "
                f"burns {int(jw[9], 16)}, imported leaves {int(jw[10], 16)}, burn root {jw[8][:10]}…")

    @check("checkpoint submitted: CandidateSubmitted event = the journal; seal verified by the vault's verifier (eth_call)")
    def _():
        if not jw:
            raise Pending("no checkpoint candidate recorded")
        if "submit" not in man:
            raise Pending("submitCandidate not recorded yet")
        rc, _tx = state["tx"]["submit"]
        t0 = topic("CandidateSubmitted(bytes32,bytes32,bytes32,uint64,uint64,bool,uint256,uint64,bytes32,uint32,uint32)")
        ev = [lg for lg in rc["logs"] if lg["address"].lower() == vault and lg["topics"][0] == t0]
        need(len(ev) == 1, f"{len(ev)} CandidateSubmitted events in the submit receipt")
        lg, w = ev[0], words(ev[0]["data"])
        need(lg["topics"][1] == jw[7] and lg["topics"][2] == jw[6], "event new/parent state")
        need(w[0] == jw[8] and int(w[1], 16) == int(jw[9], 16) and int(w[2], 16) == int(jw[10], 16), "burn root/counts")
        need(int(w[5], 16) == int(jw[13], 16) and w[6] == jw[14], "tip")
        dig = h(hashlib.sha256(b(crown["candidate"]["journal"])).digest())
        image = word("checkpointImageId()")
        ok, err = rpc.call_ok(state.get("verifier") or "0x" + word("verifier()")[-40:], verify_call(crown["candidate"]["seal"], image, dig))
        need(ok, f"the verifier rejects the recorded seal: {err}")
        seal = crown["candidate"]["seal"]
        bad = seal[:-2] + ("00" if seal[-2:] != "00" else "01")
        ok2, _ = rpc.call_ok(state["verifier"], verify_call(bad, image, dig))
        need(not ok2, "the verifier accepts a changed seal (control failed)")
        return f"event = journal (tip {int(w[5], 16)}, delta {int(w[4], 16)} s); seal accepted for the vault's checkpointImageId, a changed seal refused"

    @check("checkpoint final on the vault (finalizeAttested after the wait)")
    def _():
        if not jw:
            raise Pending("no checkpoint candidate recorded")
        fs = word("finalState()")
        count = int(word("checkpointCount()"), 16)
        acc = words(vcall("burnRootStatus(bytes32)", jw[8]))
        if fs != jw[7]:
            if "submit" in man:
                blk = rpc("eth_getBlockByNumber", hex(int(man["submit"][0]["blockNumber"], 16)), False)
                delta = int(idn.get("delta") or 0)
                opens = int(blk["timestamp"], 16) + delta
                raise Pending(f"finalState is still {fs[:10]}…; finalize opens at {time.strftime('%Y-%m-%d %H:%M:%SZ', time.gmtime(opens))} "
                              f"(submit block time + delta {delta} s)")
            raise Pending("not submitted")
        need(count >= 1 and int(acc[0], 16) == 1, f"checkpointCount {count}, burn root accepted {acc[0]}")
        rec = " (finalizeAttested in the records)" if "finalizeAttested" in man else " (on chain; the records predate it)"
        return f"finalState = the journal's new state, checkpointCount {count}, burn root accepted at {int(acc[1], 16)}{rec}"

    @check("releases: recipients, amounts, nullifiers spent, seals verified")
    def _():
        rels = crown.get("releases") or []
        if not rels:
            raise Pending("no releases recorded")
        t0 = topic("Release(bytes32,address,address,uint256,address,uint256,bytes32)")
        logs = rpc.logs(vault, t0, from_block, state.get("head") or int(rpc("eth_blockNumber"), 16))
        by_null = {lg["topics"][1].lower(): lg for lg in logs}
        out, waiting, paid = [], [], 0
        image = word("releaseImageId()")
        for r in rels:
            meta = load(os.path.join(R, "releases", f"release-{r['name']}.json"))
            need(r["burnRoot"].lower() == jw[8] == meta["burn_root"].lower(), f"{r['name']}: burn root is not the checkpoint's")
            need(meta["nullifier"].lower() == r["nullifier"].lower(), f"{r['name']}: nullifier")
            dig = h(hashlib.sha256(b(meta["journal"])).digest())
            ok, err = rpc.call_ok(state.get("verifier") or "0x" + word("verifier()")[-40:], verify_call(r["seal"], image, dig))
            need(ok, f"{r['name']}: the verifier rejects the recorded seal: {err}")
            used = int(word("nullifierUsed(bytes32)", r["nullifier"]), 16) == 1
            lg = by_null.get(r["nullifier"].lower())
            amount = int(meta["amount"], 16)
            if not used:
                need(lg is None, f"{r['name']}: a Release event but the nullifier is not spent")
                waiting.append(f"{r['name']} ({amount} wei to {r['recipient']}; seal verifies)")
                continue
            need(lg is not None, f"{r['name']}: nullifier spent but no Release event")
            w = words(lg["data"])
            need("0x" + lg["topics"][3][-40:] == r["recipient"].lower(), f"{r['name']}: recipient")
            need("0x" + lg["topics"][2][-40:] == r["token"].lower(), f"{r['name']}: token")
            need(int(w[0], 16) == amount and w[3] == jw[8], f"{r['name']}: amount {int(w[0], 16)} or burn root")
            paid += int(w[0], 16)
            out.append(f"{r['name']}: {int(w[0], 16)} wei to {r['recipient']}, nullifier spent (block {int(lg['blockNumber'], 16)})")
        need(len(logs) == len(out), f"{len(logs)} Release events, {len(out)} spent nullifiers")
        if waiting and not out:
            raise Pending("not released yet: " + "; ".join(waiting))
        if waiting:
            raise Pending("; ".join(out) + "; not released yet: " + "; ".join(waiting))
        bal = int(rpc("eth_getBalance", vault, "latest"), 16)
        dep_total = state.get("deposited")
        if dep_total is not None:
            need(bal <= dep_total - paid, f"vault balance {bal} > deposits {dep_total} - released {paid}")
        return "; ".join(out) + f"; vault balance now {bal} wei"

    p, f, pe = RESULTS.count("PASS"), RESULTS.count("FAIL"), RESULTS.count("PENDING")
    print(f"{p} passed, {pe} pending, {f} failed ({rpc.n} RPC calls)")
    return 1 if f else 0


if __name__ == "__main__":
    sys.exit(main())
