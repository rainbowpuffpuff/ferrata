#!/usr/bin/env python3
"""The first full round trip on the Ferrata LEZ test network (Sepolia -> the network's LEZ zone -> Sepolia, 23-24
November 2026), checked from public chain data alone. Python 3 standard library only; read-only (no key, nothing
signed or sent).

    python3 -I scripts/verify-live-roundtrip.py --check [RECORD] [--rpc URL] [--lez-rpc URL]
        Read every fact again from the chains, rebuild the record from them exactly as --record does, and compare it
        with RECORD (default e2e/runs/live-roundtrip-2026-11-24/record.json) field by field; then check the links
        between the legs on what the chains returned, not on the record's copies. One line per part: PASS or FAIL
        (with the fields that differ); exit 1 on a FAIL, 2 when an endpoint cannot be read.
    python3 -I scripts/verify-live-roundtrip.py --record OUT [--rpc URL] [--lez-rpc URL]
        The maintainer's step: read the same facts and write them to OUT.

What is read. Sepolia (any public JSON-RPC that serves state at old blocks; scripts/pubnet.py's endpoints after
--rpc): each transaction's receipt (block, status, gas, sender and its nonce, target, calldata), its block's time, the
vault's events in it and that its block holds no other vault event, the vault's state by eth_call at the end of the
block before and of its own block with the balances involved, and the vault's image ids. The LEZ zone (its public
JSON-RPC, by default the network named in scripts/reproduce/deployed-programs.json): each transaction as the block
getTransaction names holds it (the block's hash recomputed from its bytes; in it, the transaction's bytes, the ones
getTransaction serves, hash to its id; a block that does not hold it is refused) and whether that block lists it as
reverted; its bridge message decoded with idl/ferrata_bridge.idl.json by idl/check.py's decoder (every byte used);
and the zone's blocks from the claim's to the checkpoint's tip, each hash recomputed and each linked to the one
before. Off chain: the prover client's records of the two GPU proofs, proofs.json beside the record.

The links, each from the chains' answers:

  1. the import and the claim: the root import_root wrote as root(cid, 1) is the vault's depositRoot at the end of the
     deposit's block; the claim's witness opens the deposit (SHA-256("ferrata/C/v1" || s || kind || dest) is the
     Deposit event's commitment; its depositor, token, amount and leaf index are the event's; dest, kind 2, is the
     account that signed the claim), and the vault's leaf (FerrataVault._leafHash) folded with the claim's path is
     that depositRoot; the import and the claim applied, and a second sender's import of the same root under the same
     record, later in the import's block, reverted;
  2. the burn: its burn leaf (encoding.rs, burn index 0, from the burn's own fields) folded with an empty path is the
     burnRoot of the candidate, of the finalized checkpoint and of the release, and the checkpoint counts one burn;
  3. the proofs: each proof record's image id is the vault's (checkpointImageId, releaseImageId), its seal is the one
     in the transaction that carried it (submitCandidate, release), and its journal is the one the vault verified (the
     candidate's journalDigest; the release journal, FerrataVault._releaseJournalDigest, rebuilt from the release's
     calldata and amount, byte for byte);
  4. the zone's blocks: the hash of LEZ block tipId (the candidate's tip) is the candidate's tipHash, and the
     prev-hash chain from it reaches the claim's and the burn's blocks, so both are in the history the checkpoint
     proof covers. The imports' blocks (2,276 and 2,278) are not walked (1,490 blocks); the claim, applied under
     root(cid, 1), rests on the import.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import sys
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "idl"))
import pubnet  # noqa: E402  (scripts/pubnet.py: failover, backoff; e2e/NETWORK.md)
import check as idl_check  # noqa: E402  (idl/check.py: the IDL decoder)

RECORD = os.path.join(ROOT, "e2e", "runs", "live-roundtrip-2026-11-24", "record.json")
DEFAULT_RPC = "https://ethereum-sepolia-rpc.publicnode.com"
CHAIN_ID = 11155111
VAULT = "0x19499061cec0bac2a3e1869ba5ae6b403aa82787"
ETH = "0x" + "00" * 20
# The test network runs ferrata_bridge_b1: its interface, frozen before ADR 0046 (idl/deployed/README.md).
BRIDGE_IDL = os.path.join(ROOT, "idl", "deployed", "ferrata_bridge_b1.idl.json")
# The round trip's transactions, in order. Sepolia: the deposit, the checkpoint's submission and finalisation (rule B),
# the release and the relayer's claim of the checkpoint fee it was owed. LEZ: the fee schedule's import, the root's
# import (and a second sender's import of the same root, refused in the same block), the claim and the burn.
SEPOLIA_STEPS = [
    ("deposit", "0x44709f7c31561f1682b67b95a1a01b774246fcdcbdb4f049d482e8284857e9e1"),
    ("submitCandidate", "0xcaa00efa1ec41cf7dfe7ebc20e35368784535fc0e6ec49934644148e13aea0d1"),
    ("finalizeAttested", "0x643d0c687b782aad00618d0d50bbaa3cd6aaa01908e0fcf2f2e724b09c5390f8"),
    ("release", "0x48b78ad406eae9c05eb665a1ce06e205cacc45f5905769425b4abbc700886f65"),
    ("claimOwed", "0xe2c264e63ec2ab2e2e9960405125c6359f711b3eaa16f8b534db41149c6e1239"),
]
LEZ_STEPS = [
    ("import_fees", "50f1d085b23fe47a6b1b4a6a12463311f272a7ea52df24b55a0a0dd62bc63ec3"),
    ("import_root", "f2fb461e534c17fbcca0de06ff8091542744bbfb7aee27d641e72bfc1213b1e3"),
    ("import_root, a second sender (refused)", "dd159aec47ec2d151575e6cff5818884fa3efd5cc193ad1176ca986ab5ad7075"),
    ("claim", "28771fd510b26758f39ee2b1b5e921568a6b0c14857267d58e91ffb8ef1c2e2f"),
    ("burn", "5c89d209852ee257bad4191cdd176e11dfea6b83b35a12f8a9487c6fa2e727cf"),
]
# Not on chain: the prover client's own records of the two GPU proofs, beside the record (journal, seal, image id, times).
PROOFS_FILE = os.path.join(os.path.dirname(RECORD), "proofs.json")
# The zone's blocks walked back from the checkpoint's tip to the claim's (link 4).
ANCHOR_FROM = 3746
LEAF_TAG = bytes.fromhex("17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b")  # sha256("ferrata/vault/leaf/v2")
BURN_TAG = bytes.fromhex("93474c5ad2f57f5fc610ad22e3f6d50e51b98a4c0eb611dde536a7317c0e8cf2")  # sha256("ferrata/burn/leaf/v2")
RELEASE_DOMAIN = bytes.fromhex("6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51")
TREE_DEPTH = 32
EVENTS = {
    "Deposit(address,uint64,uint64,address,uint256,bytes32)": (["depositor", "token"], ["index:u", "leafIndex:u", "amount:u", "commitment:h"]),
    "CandidateSubmitted(bytes32,bytes32,bytes32,uint64,uint64,bool,uint256,uint64,bytes32,uint32,uint32)": (
        ["newState", "parentState"], ["burnRoot:h", "burnCount:u", "importedLeafCount:u", "sunset:u", "deltaAtSubmit:u", "tipId:u", "tipHash:h", "n:u", "t:u"]),
    "CheckpointFinalized(uint256,bytes32,bytes32,bytes32,uint64,uint64,bool,uint64,bytes32,uint32,uint32)": (
        ["index", "newState"], ["parentState:h", "burnRoot:h", "burnCount:u", "importedLeafCount:u", "sunset:u", "tipId:u", "tipHash:h", "n:u", "t:u"]),
    "Release(bytes32,address,address,uint256,address,uint256,bytes32)": (
        ["nullifier", "token", "recipient"], ["amount:u", "relayer:a", "relayFee:u", "burnRoot:h"]),
    "OwedCredited(address,address,uint256)": (["payee", "token"], ["amount:u"]),
    "OwedClaimed(address,address,uint256)": (["payee", "token"], ["amount:u"]),
}
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
RESULTS: list[str] = []


class Failed(Exception):
    pass


class Unreadable(Exception):
    pass


def sha256(*parts: bytes) -> bytes:
    return hashlib.sha256(b"".join(parts)).digest()


def b58(b: bytes) -> str:
    n, s = int.from_bytes(b, "big"), ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def h(b: bytes) -> str:
    return "0x" + b.hex()


def word(x) -> str:
    """One ABI word (hex, no 0x) of an int, an address or a 32-byte hex value."""
    if isinstance(x, int):
        return format(x, "064x")
    x = x[2:] if x.startswith("0x") else x
    return x.rjust(64, "0")


def selector(sig: str) -> str:
    return pubnet.keccak256(sig.encode())[:4].hex()


def topic(sig: str) -> str:
    return "0x" + pubnet.keccak256(sig.encode()).hex()


# ---- the two chains ---------------------------------------------------------------------------------------------

class Sepolia:
    def __init__(self, url: str):
        self.net = pubnet.Net("sepolia", first=url, request_budget=float(os.environ.get("PUBNET_REQUEST_BUDGET_S", "120")))

    def __call__(self, method: str, *params, anywhere: bool = False):
        try:
            if anywhere:
                r = self.net.anywhere(method, list(params))
                if r is None:
                    raise Unreadable(f"{method}: no endpoint has it")
                return r
            return self.net.call(method, list(params))
        except pubnet.Permanent as e:
            raise Failed(f"{method}: {e}") from None
        except pubnet.Transient as e:
            raise Unreadable(f"{method}: {e}") from None

    def call(self, block: int, sig: str, *args) -> str:
        return self("eth_call", {"to": VAULT, "data": "0x" + selector(sig) + "".join(word(a) for a in args)}, hex(block))


class Lez:
    def __init__(self, url: str):
        self.url = url

    def __call__(self, method: str, *params):
        q = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)}).encode()
        req = urllib.request.Request(self.url, data=q, headers={"Content-Type": "application/json", "User-Agent": "ferrata-check/1"})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                d = json.loads(r.read())
        except (urllib.error.URLError, OSError, ValueError) as e:
            raise Unreadable(f"{method}: {e}") from None
        if "error" in d:
            raise Failed(f"{method}: {d['error']}")
        return d["result"]


# ---- LEZ's encodings (deploy/testnet/web/explorer/assets/lez.js, from LEZ v0.4.0-rc3) --------------------------

class Reader:
    def __init__(self, b: bytes):
        self.b, self.pos = b, 0

    def take(self, n: int) -> bytes:
        out = self.b[self.pos:self.pos + n]
        if len(out) != n:
            raise Failed(f"needs {n} byte(s) at {self.pos}")
        self.pos += n
        return out

    def u(self, n: int) -> int:
        return int.from_bytes(self.take(n), "little")

    def vec(self, f):
        return [f() for _ in range(self.u(4))]

    def opt(self, f):
        t = self.u(1)
        if t > 1:
            raise Failed(f"option tag {t}")
        return f() if t else None


def read_tx(r: Reader) -> dict:
    """One public LeeTransaction (tag 0); its id is SHA-256 of the bytes after the tag."""
    if r.u(1) != 0:
        raise Failed("not a public transaction")
    start = r.pos
    actor = lambda: (r.take(32), r.take(32))  # noqa: E731
    r.vec(actor)  # the public context: actors,
    r.vec(lambda: r.take(32))  # authorized accounts,
    r.vec(lambda: r.u(8))  # promotions
    to = actor()
    message = r.take(r.u(4))
    r.opt(lambda: (r.take(32), r.u(8), r.u(8), r.u(16)))  # the fee declaration
    nonces = r.vec(lambda: (r.take(32), r.u(16)))

    def evidence():
        t = r.u(1)
        if t == 0:
            return r.take(32)
        if t == 1:
            return r.take(32) + r.take(32)
        raise Failed(f"evidence tag {t}")
    r.vec(evidence)
    witnesses = r.vec(lambda: (r.take(64), r.take(32)))
    pad = "/LEE/v0.3/AccountId/Public/".encode().ljust(32, b"\0")
    return {"id": sha256(r.b[start:r.pos]).hex(), "program": b58(to[1]), "actor": b58(to[0]), "message": message,
            "signers": [b58(sha256(pad, pk)) for _, pk in witnesses], "nonces": [(b58(a), n) for a, n in nonces]}


def read_block(raw: bytes) -> dict:
    """getBlock's bytes: header (id, prev, hash, timestamp ms), the transactions, the reverted indices, Bedrock status.
    The hash is recomputed: SHA-256("/LEE/v0.3/Message/Block/" padded || id || prev || timestamp || body)."""
    r = Reader(raw)
    block_id, prev, block_hash, ts = r.u(8), r.take(32), r.take(32), r.u(8)

    def tx():
        at = r.pos
        t = read_tx(r)
        return t["id"], raw[at:r.pos]
    txs = r.vec(tx)
    reverted = r.vec(lambda: r.u(8))
    r.u(1)
    if r.pos != len(raw):
        raise Failed(f"block {block_id}: {len(raw) - r.pos} byte(s) after the status")
    computed = sha256("/LEE/v0.3/Message/Block/".encode().ljust(32, b"\0"), raw[0:40], raw[72:80], raw[80:-1])
    return {"id": block_id, "hash": block_hash.hex(), "prev": prev.hex(), "hash_recomputed": computed.hex(),
            "timestamp_ms": ts, "tx_ids": [i for i, _ in txs], "tx_bytes": [b for _, b in txs], "reverted": reverted}


def plain(v):
    """idl/check.py's decoded value as JSON: byte arrays as hex, structs as objects, enums with their variant."""
    k = v[0]
    if k == "struct":
        return {n: plain(x) for n, x in v[2]}
    if k == "enum":
        return {"variant": v[2], **{n: plain(x) for n, x in v[3]}}
    if k == "int":
        return v[2]
    if k == "bool":
        return v[1]
    if k in ("acct", "bytes32"):
        return h(v[2])
    if k == "opt":
        return None if v[1] is None else plain(v[1])
    if k == "list":
        if v[2] == "u8":
            return h(bytes(x[2] for x in v[3]))
        return [plain(x) for x in v[3]]
    if k == "string":
        return v[1]
    raise Failed(f"unknown decoded value {k}")


def decode_bridge(message: bytes) -> dict:
    with open(BRIDGE_IDL, encoding="utf-8") as f:
        idl = json.load(f)
    ins, args = idl_check.decode_instruction(idl, message)  # raises unless every byte is used
    return {"instruction": ins["name"], "args": {n: plain(v) for n, v in args}}


# ---- the links ----------------------------------------------------------------------------------------------------

def fold(leaf: bytes, index: int, path: list[bytes]) -> bytes:
    """A depth-32 tree's root from a leaf and its siblings, bottom up (node = SHA-256(0x01 || left || right))."""
    node = leaf
    for level, sib in enumerate(path):
        node = sha256(b"\x01", sib, node) if (index >> level) & 1 else sha256(b"\x01", node, sib)
    return node


def empty_path() -> list[bytes]:
    out, z = [], bytes(32)
    for _ in range(TREE_DEPTH):
        out.append(z)
        z = sha256(b"\x01", z, z)
    return out


def claim_links(claim: dict) -> dict:
    """The claim's witness opened: its commitment, its deposit leaf and the root its path reaches."""
    w = claim["args"]["witness"]
    s, kind, dest = bytes.fromhex(w["s"][2:]), w["kind"], bytes.fromhex(w["dest"][2:])
    commitment = sha256(b"ferrata/C/v1", s, bytes([kind]), dest)
    leaf = sha256(b"\x00", LEAF_TAG, w["leaf_index"].to_bytes(8, "big"), bytes.fromhex(w["depositor"][2:]),
                  bytes.fromhex(w["token"][2:]), bytes.fromhex(w["amount"][2:]), commitment)
    root = fold(leaf, w["leaf_index"], [bytes.fromhex(p[2:]) for p in w["path"]])
    return {"commitment": h(commitment), "dest_account": b58(dest), "kind": kind, "leaf": h(leaf), "root": h(root),
            "leaf_index": w["leaf_index"]}


def burn_links(burn: dict, burn_index: int = 0) -> dict:
    a = burn["args"]
    leaf = sha256(b"\x02", BURN_TAG, bytes.fromhex(a["config_id"][2:]), burn_index.to_bytes(8, "big"),
                  bytes.fromhex(a["token"][2:]), bytes.fromhex(a["amount"][2:]), bytes.fromhex(a["c_out"][2:]),
                  bytes.fromhex(a["r"][2:]))
    return {"burn_index": burn_index, "leaf": h(leaf), "root": h(fold(leaf, burn_index, empty_path()))}


def release_journal(calldata: str, amount: int, config_id: str) -> dict:
    """FerrataVault._releaseJournalDigest from `release((token, denomIndex, recipient, relayer, relayFee, burnRoot,
    nullifier), seal)`'s calldata: abi.encode(RELEASE_DOMAIN, chainid, vault, lezConfigId, burnRoot, NOut, token,
    amount, recipient, relayer, relayFee)."""
    d = calldata[10:]
    f = [d[i * 64:(i + 1) * 64] for i in range(7)]
    token, _denom, recipient, relayer, fee, burn_root, nullifier = f
    j = bytes.fromhex(word(RELEASE_DOMAIN.hex()) + word(CHAIN_ID) + word(VAULT) + word(config_id) + burn_root + nullifier
                      + token + word(amount) + recipient + relayer + fee)
    return {"journal": h(j), "journal_sha256": h(sha256(j)), "relayer_field": "0x" + relayer[24:],
            "nullifier": "0x" + nullifier, "burnRoot": "0x" + burn_root}


def calldata_bytes_arg(calldata: str, head_words: int) -> str:
    """The `bytes` argument (the seal) of a call whose static head is `head_words` words then that argument's offset."""
    d = calldata[10:]
    off = int(d[head_words * 64:(head_words + 1) * 64], 16) * 2
    n = int(d[off:off + 64], 16)
    return "0x" + d[off + 64:off + 64 + 2 * n]


# ---- reading the facts --------------------------------------------------------------------------------------------

def decode_log(log: dict) -> dict:
    for sig, (indexed, data) in EVENTS.items():
        if log["topics"] and log["topics"][0] == topic(sig):
            out = {"event": sig.split("(")[0]}
            for name, t in zip(indexed, log["topics"][1:]):
                out[name] = "0x" + t[26:] if name in ("depositor", "token", "recipient", "payee") else (int(t, 16) if name == "index" else t)
            ws = [log["data"][2 + 64 * i:2 + 64 * (i + 1)] for i in range(len(data))]
            for spec, w in zip(data, ws):
                name, kind = spec.split(":")
                out[name] = int(w, 16) if kind == "u" else ("0x" + w[24:] if kind == "a" else "0x" + w)
            return out
    return {"event": "unknown", "topic0": log["topics"][0] if log["topics"] else None}


def vault_state(eth: Sepolia, block: int, k: dict) -> dict:
    """The vault's state at the end of `block`, and the balances of the parties."""
    def u(sig, *a):
        return int(eth.call(block, sig, *a), 16)

    def b32(sig, *a):
        return "0x" + eth.call(block, sig, *a)[2:66]
    cand = eth.call(block, "candidates(bytes32)", k["newState"])[2:]
    return {
        "depositRoot": b32("depositRoot()"), "nextLeafIndex": u("nextLeafIndex()"),
        "depositCount(depositor)": u("depositCount(address)", k["depositor"]),
        "escrowed(ETH)": u("escrowed(address)", ETH), "totalOwed(ETH)": u("totalOwed(address)", ETH),
        "owed(relayer, ETH)": u("owed(address,address)", k["relayer"], ETH),
        "checkpointCount": u("checkpointCount()"), "finalState": b32("finalState()"),
        "candidates(newState).journalDigest": "0x" + cand[:64],
        "candidates(newState).submittedAt": int(cand[4 * 64:5 * 64], 16),
        "burnRootCheckpoint(burnRoot)": u("burnRootCheckpoint(bytes32)", k["burnRoot"]),
        "nullifierUsed(NOut)": bool(u("nullifierUsed(bytes32)", k["nullifier"])),
        "balance(vault)": int(eth("eth_getBalance", VAULT, hex(block)), 16),
        "balance(recipient)": int(eth("eth_getBalance", k["recipient"], hex(block)), 16),
        "balance(relayer)": int(eth("eth_getBalance", k["relayer"], hex(block)), 16),
    }


def read_sepolia(eth: Sepolia) -> tuple[list, dict]:
    steps, keys = [], {}
    for name, tx in SEPOLIA_STEPS:
        rc = eth("eth_getTransactionReceipt", tx, anywhere=True)
        t = eth("eth_getTransactionByHash", tx, anywhere=True)
        block = int(rc["blockNumber"], 16)
        ts = int(eth("eth_getBlockByNumber", hex(block), False, anywhere=True)["timestamp"], 16)
        logs = [decode_log(lg) for lg in rc["logs"] if lg["address"].lower() == VAULT]
        others = [lg["transactionHash"] for lg in eth("eth_getLogs", {"address": VAULT, "fromBlock": hex(block), "toBlock": hex(block)})
                  if lg["transactionHash"].lower() != tx]
        steps.append({"step": name, "tx": tx, "block": block, "timestamp": ts, "status": int(rc["status"], 16),
                      "gas_used": int(rc["gasUsed"], 16), "from": rc["from"].lower(), "nonce": int(t["nonce"], 16),
                      "to": (rc["to"] or "").lower(),
                      "selector": t["input"][:10], "vault_events": logs, "other_vault_txs_in_block": others,
                      "input": t["input"]})
        for lg in logs:
            if lg["event"] == "Deposit":
                keys["depositor"] = lg["depositor"]
            if lg["event"] == "CandidateSubmitted":
                keys["newState"], keys["burnRoot"] = lg["newState"], lg["burnRoot"]
            if lg["event"] == "Release":
                keys["nullifier"], keys["recipient"], keys["relayer"] = lg["nullifier"], lg["recipient"], lg["relayer"]
    for s in steps:
        s["vault_before"] = vault_state(eth, s["block"] - 1, keys)
        s["vault_after"] = vault_state(eth, s["block"], keys)
    return steps, keys


def read_lez(lez: Lez) -> list:
    """Each step's transaction as its block holds it. getTransaction names the block; that block's bytes (its hash
    recomputed from them) must hold a transaction whose bytes hash to the step's id, the very bytes getTransaction
    served; the block's reverted list says whether it applied. A zone RPC that names another block, or serves other
    bytes, is refused here."""
    out = []
    blocks = {}
    for name, tx in LEZ_STEPS:
        raw_b64, block = lez("getTransaction", tx)
        if block not in blocks:
            blocks[block] = read_block(base64.b64decode(lez("getBlock", block)))
        b = blocks[block]
        need(b["hash_recomputed"] == b["hash"], f"LEZ block {block}: its hash is not the SHA-256 of its bytes")
        need(tx in b["tx_ids"], f"LEZ {name}: {tx[:12]}… is not in block {block}, the block the RPC names")
        idx = b["tx_ids"].index(tx)
        raw = b["tx_bytes"][idx]  # the transaction's bytes as the hashed block holds them
        need(base64.b64decode(raw_b64) == raw, f"LEZ {name}: the bytes getTransaction serves are not the ones block {block} holds")
        t = read_tx(Reader(raw))
        need(t["id"] == tx, f"LEZ {name}: the bytes do not hash to {tx[:12]}…")
        out.append({"step": name, "tx": tx, "id_of_bytes": t["id"], "block": block, "block_hash": b["hash"],
                    "block_timestamp_ms": b["timestamp_ms"], "index_in_block": idx,
                    "reverted": idx in b["reverted"], "program": t["program"],
                    "root_actor": t["actor"], "signers": t["signers"], "nonces": t["nonces"],
                    "message": decode_bridge(t["message"]), "bytes_base64": raw_b64})
    return out


def load_proofs(path: str = PROOFS_FILE) -> dict:
    with open(path, encoding="utf-8") as f:
        d = json.load(f)
    return {k: d[k] for k in ("checkpoint", "release")}


def proof_facts(proofs: dict) -> dict:
    """What the record shows of the prover client's records (all of it derived from proofs.json)."""
    out = {}
    for k, r in proofs.items():
        j, seal = bytes.fromhex(r["journal"][2:]), bytes.fromhex(r["seal"][2:])
        out[k] = {"name": r["name"], "kind": r["kind"], "image_id": r["image_id"], "ok": r["ok"], "seconds": r["seconds"],
                  "prover_wall_s": r["prover"]["wall_s"], "journal_bytes": len(j), "journal_sha256": h(sha256(j)),
                  "seal_bytes": len(seal), "seal_sha256": h(sha256(seal))}
    return out


def read_immutables(eth: Sepolia) -> dict:
    head = int(eth("eth_blockNumber"), 16)
    return {k: "0x" + eth.call(head, k + "()")[2:66] for k in ("checkpointImageId", "releaseImageId", "lezConfigId")}


def read_anchor(lez: Lez, tip_id: int, lo: int = ANCHOR_FROM) -> dict:
    """The zone's blocks lo..=tip_id: each hash, whether it is the SHA-256 of the block's bytes, the previous hash."""
    blocks = []
    for i in range(lo, tip_id + 1):
        b = read_block(base64.b64decode(lez("getBlock", i)))
        blocks.append({"id": b["id"], "hash": b["hash"], "prev": b["prev"], "hash_ok": b["hash_recomputed"] == b["hash"]})
    return {"from": lo, "to": tip_id, "blocks": blocks}


def _step(rec: dict, part: str, name: str) -> dict:
    return next(x for x in rec[part] if x["step"] == name)


def _event(rec: dict, step: str, event: str) -> dict:
    return next(e for e in _step(rec, "sepolia", step)["vault_events"] if e["event"] == event)


def links(rec: dict, proofs: dict) -> dict:
    """The link facts, for reading; link_* below check them, on whatever the record's sections hold."""
    claim, imp, burn = _step(rec, "lez", "claim"), _step(rec, "lez", "import_root"), _step(rec, "lez", "burn")
    rel, rel_step = _event(rec, "release", "Release"), _step(rec, "sepolia", "release")
    j = release_journal(rel_step["input"], rel["amount"], rec["vault_immutables"]["lezConfigId"])
    return {
        "claim": {**claim_links(claim["message"]), "deposit_commitment": _event(rec, "deposit", "Deposit")["commitment"],
                  "deposit_root_after_deposit": _step(rec, "sepolia", "deposit")["vault_after"]["depositRoot"],
                  "imported_root": imp["message"]["args"]["root"], "imported_leaf_count": imp["message"]["args"]["leaf_count"],
                  "claim_signer": claim["signers"][0]},
        "burn": {**burn_links(burn["message"]), "candidate_burnRoot": _event(rec, "submitCandidate", "CandidateSubmitted")["burnRoot"],
                 "finalized_burnRoot": _event(rec, "finalizeAttested", "CheckpointFinalized")["burnRoot"], "release_burnRoot": rel["burnRoot"]},
        "proofs": {"candidate_journalDigest": _step(rec, "sepolia", "submitCandidate")["vault_after"]["candidates(newState).journalDigest"],
                   "checkpoint_journal_sha256": h(sha256(bytes.fromhex(proofs["checkpoint"]["journal"][2:]))),
                   "release_journal_rebuilt_sha256": j["journal_sha256"], "release_relayer_field": j["relayer_field"]},
        "anchor": {"blocks": f"{rec['anchor']['from']}..={rec['anchor']['to']}", "tip_hash": "0x" + rec["anchor"]["blocks"][-1]["hash"]},
    }


def in_block(s: dict) -> bool:
    """The step's transaction is in the block named (an index there) and its bytes hash to its id."""
    return s["id_of_bytes"] == s["tx"] and isinstance(s["index_in_block"], int) and not isinstance(s["index_in_block"], bool)


def applied(s: dict) -> bool:
    return in_block(s) and s["reverted"] is False


def link_claim(rec: dict, proofs: dict) -> str:
    dep, claim, imp = _event(rec, "deposit", "Deposit"), _step(rec, "lez", "claim"), _step(rec, "lez", "import_root")
    need(applied(claim) and applied(imp), "the claim or the import is not an applied transaction of the block named")
    root = _step(rec, "sepolia", "deposit")["vault_after"]["depositRoot"]
    c, w, ia = claim_links(claim["message"]), claim["message"]["args"]["witness"], imp["message"]["args"]
    # F.3's refusal: a second sender's import of the same root under the same record, later in the same block, reverted.
    ref = _step(rec, "lez", "import_root, a second sender (refused)")
    ra = ref["message"]["args"]
    need(in_block(ref) and ref["reverted"] is True and ref["block"] == imp["block"] and ref["block_hash"] == imp["block_hash"]
         and ref["index_in_block"] > imp["index_in_block"] and ref["signers"] != imp["signers"]
         and ref["message"]["instruction"] == imp["message"]["instruction"] == "import_root"
         and all(ra[k] == ia[k] for k in ("config_id", "record_slot", "root", "leaf_count")),
         "the second sender's import of the same root is not a later, reverted transaction of the import's block")
    need(c["commitment"] == dep["commitment"], "the claim's opening is not the Deposit event's commitment")
    need((w["depositor"], w["token"], int(w["amount"], 16), w["leaf_index"]) == (dep["depositor"], dep["token"], dep["amount"], dep["leafIndex"]),
         "the claim's leaf is not the deposit's (depositor, token, amount, index)")
    need(c["root"] == root, "the claim's path does not reach the vault's depositRoot after the deposit")
    need(ia["root"] == root and ia["leaf_count"] == claim["message"]["args"]["leaf_count"] == dep["leafIndex"] + 1,
         "the imported root(cid, n) is not the vault's depositRoot after the deposit")
    need(ia["config_id"] == claim["message"]["args"]["config_id"] == rec["vault_immutables"]["lezConfigId"],
         "the import or the claim is of another configuration than the vault's")
    need(c["kind"] == 2 and c["dest_account"] == claim["signers"][0], "the claim's destination is not the account that signed it")
    return (f"commitment {c['commitment'][:12]}… = the Deposit event's; leaf {w['leaf_index']} + path -> {root[:12]}… = depositRoot "
            f"after the deposit = the root import_root wrote; dest = the signer {c['dest_account'][:8]}…; a second sender's import "
            f"of that root, later in block {imp['block']}, reverted")


def link_burn(rec: dict, proofs: dict) -> str:
    burn = _step(rec, "lez", "burn")
    need(applied(burn), "the burn is not an applied transaction of the block named")
    b = burn_links(burn["message"])
    cand, fin = _event(rec, "submitCandidate", "CandidateSubmitted"), _event(rec, "finalizeAttested", "CheckpointFinalized")
    rel = _event(rec, "release", "Release")
    need(b["root"] == cand["burnRoot"] == fin["burnRoot"] == rel["burnRoot"], "the burn's leaf is not the burnRoot of the candidate, the checkpoint and the release")
    need(cand["burnCount"] == fin["burnCount"] == 1 and cand["newState"] == fin["newState"], "the finalized checkpoint is not the candidate, or counts other burns")
    a = burn["message"]["args"]
    need((int(a["amount"], 16), a["token"]) == (rel["amount"], rel["token"]), "the release is not of the burn's token and amount")
    return f"burn leaf {b['leaf'][:12]}… alone -> {b['root'][:12]}… = the candidate's, the checkpoint's and the release's burnRoot; 1 burn"


def link_proofs(rec: dict, proofs: dict) -> str:
    imm, cp, rp = rec["vault_immutables"], proofs["checkpoint"], proofs["release"]
    sub, rel_step, rel = _step(rec, "sepolia", "submitCandidate"), _step(rec, "sepolia", "release"), _event(rec, "release", "Release")
    need((cp["image_id"], rp["image_id"]) == (imm["checkpointImageId"], imm["releaseImageId"]), "a proof's image id is not the vault's")
    need(calldata_bytes_arg(sub["input"], 13) == cp["seal"], "the submitted candidate's seal is not the checkpoint proof's")
    need(calldata_bytes_arg(rel_step["input"], 7) == rp["seal"], "the release's seal is not the release proof's")
    need(h(sha256(bytes.fromhex(cp["journal"][2:]))) == sub["vault_after"]["candidates(newState).journalDigest"],
         "the candidate's journalDigest on the vault is not the checkpoint proof's journal")
    j = release_journal(rel_step["input"], rel["amount"], imm["lezConfigId"])
    need(j["journal"] == rp["journal"], "the release journal rebuilt from the release's calldata is not the release proof's")
    need(j["nullifier"] == rel["nullifier"] and rel_step["vault_after"]["nullifierUsed(NOut)"] and not rel_step["vault_before"]["nullifierUsed(NOut)"],
         "the release proof's nullifier is not the one the vault marked used")
    return "image ids = the vault's; each seal = the one in its transaction; journals = the candidate's digest and the release's, rebuilt"


def link_anchor(rec: dict, proofs: dict) -> str:
    a, cand = rec["anchor"], _event(rec, "submitCandidate", "CandidateSubmitted")
    bl = a["blocks"]
    need([b["id"] for b in bl] == list(range(a["from"], a["to"] + 1)) and a["to"] == cand["tipId"], "the walked blocks do not end at the candidate's tip")
    need(all(b["hash_ok"] for b in bl), "a block's hash is not the SHA-256 of its bytes")
    need(all(bl[i]["prev"] == bl[i - 1]["hash"] for i in range(1, len(bl))), "the prev-hash chain is broken")
    need("0x" + bl[-1]["hash"] == cand["tipHash"], "the last block's hash is not the candidate's tipHash")
    by_id = {b["id"]: b["hash"] for b in bl}
    for name in ("claim", "burn"):
        s = _step(rec, "lez", name)
        need(by_id.get(s["block"]) == s["block_hash"], f"the {name}'s block is not on the chain to the checkpoint's tip")
    return f"blocks {a['from']}..={a['to']}: hashes recomputed, linked; block {a['to']} = the candidate's tipHash; the claim's and the burn's on it"


LINKS = [("the import and the claim are the deposit's", link_claim), ("the burn is the checkpoint's only burn, the one released", link_burn),
         ("the GPU proofs are the ones the vault verified", link_proofs), ("the claim and the burn are in the checkpoint's history", link_anchor)]
ABOUT = ("The first full round trip on the Ferrata LEZ test network, as public chain data: Sepolia (the vault) and the "
         "network's LEZ v0.4.0-rc3 zone. Written and re-checked field by field by scripts/verify-live-roundtrip.py; "
         "README.md beside this file says what each part shows; proofs.json holds the prover client's records.")


def build_record(eth: Sepolia, lez: Lez) -> dict:
    """The record, from the chains (and proofs.json) alone: what --record writes and --check compares with."""
    sep, keys = read_sepolia(eth)
    lezs = read_lez(lez)
    rec = {"about": ABOUT, "vault": VAULT, "chain_id": int(eth("eth_chainId"), 16), "keys": keys, "sepolia": sep, "lez": lezs}
    rec["anchor"] = read_anchor(lez, _event(rec, "submitCandidate", "CandidateSubmitted")["tipId"])
    rec["vault_immutables"] = read_immutables(eth)
    proofs = load_proofs()
    rec["proofs"] = proof_facts(proofs)
    rec["links"] = links(rec, proofs)
    return json.loads(json.dumps(rec))  # tuples as lists, as the file holds them


def deep_diff(got, want, path: str = "") -> list:
    """Every path where two JSON values differ (a missing key, another length, another leaf)."""
    if isinstance(got, dict) and isinstance(want, dict):
        out = []
        for k in sorted(set(got) | set(want)):
            if k not in got or k not in want:
                out.append(f"{path}.{k} ({'not in the record' if k not in want else 'not on the chains'})")
            else:
                out += deep_diff(got[k], want[k], f"{path}.{k}")
        return out
    if isinstance(got, list) and isinstance(want, list):
        if len(got) != len(want):
            return [f"{path} (length {len(got)} on the chains, {len(want)} in the record)"]
        return [d for i, (a, b) in enumerate(zip(got, want)) for d in deep_diff(a, b, f"{path}[{i}]")]
    return [] if (got == want and type(got) is type(want)) else [path or "."]


# ---- check -----------------------------------------------------------------------------------------------------------

def check(name: str, fn) -> None:
    try:
        status, detail = "PASS", fn() or ""
    except Failed as e:
        status, detail = "FAIL", str(e)
    except (Unreadable, KeyError, ValueError, TypeError, IndexError, StopIteration, idl_check.DecodeError) as e:
        status, detail = ("UNREAD" if isinstance(e, Unreadable) else "FAIL"), f"{type(e).__name__}: {e}"
    RESULTS.append(status)
    print(f"{status:<8} {name}{': ' + detail if detail else ''}", flush=True)


def need(cond: bool, msg: str) -> None:
    if not cond:
        raise Failed(msg)


def no_diff(got, want) -> str:
    d = deep_diff(got, want)
    need(not d, f"{len(d)} field(s) differ: " + ", ".join(d[:4]) + (" …" if len(d) > 4 else ""))
    return ""


def run_check(record: dict, eth: Sepolia, lez: Lez) -> int:
    check("Sepolia: the endpoint is Sepolia", lambda: need(int(eth("eth_chainId"), 16) == CHAIN_ID, "another chain"))
    try:
        fresh = build_record(eth, lez)
    except Unreadable as e:
        print(f"UNREAD   the chains could not be read: {e}")
        return 2
    except (Failed, KeyError, ValueError, IndexError, StopIteration, idl_check.DecodeError) as e:
        print(f"FAIL     the chains' answers do not make a record: {type(e).__name__}: {e}")
        return 1
    check("the record's parts and the steps, in order", lambda: (
        need(sorted(record) == sorted(fresh), f"parts {sorted(record)}, expected {sorted(fresh)}"),
        need([s["step"] for s in record["sepolia"]] == [n for n, _ in SEPOLIA_STEPS], "the Sepolia steps differ"),
        need([s["step"] for s in record["lez"]] == [n for n, _ in LEZ_STEPS], "the LEZ steps differ")) and "")
    for i, (name, tx) in enumerate(SEPOLIA_STEPS):
        f = fresh["sepolia"][i]
        check(f"Sepolia {name} {tx[:12]}…: every field = the chain (block {f['block']}, status {f['status']}, gas {f['gas_used']:,}, "
              f"nonce {f['nonce']}; the vault's state before and after)", lambda i=i: no_diff(fresh["sepolia"][i], record["sepolia"][i]))
    for i, (name, tx) in enumerate(LEZ_STEPS):
        f = fresh["lez"][i]
        check(f"LEZ {name} {tx[:12]}…: every field = the zone (block {f['block']}, {'reverted' if f['reverted'] else 'applied'}, "
              f"{f['message']['instruction']} decoded whole)", lambda i=i: no_diff(fresh["lez"][i], record["lez"][i]))
    for part in ("about", "vault", "chain_id", "keys", "vault_immutables", "anchor", "proofs", "links"):
        check(f"the record's {part} = what the chains and proofs.json give", lambda part=part: no_diff(fresh[part], record[part]))
    proofs = load_proofs()
    for name, fn in LINKS:
        check(f"link, on the chains' answers: {name}", lambda fn=fn: fn(fresh, proofs))
    fail, unread = RESULTS.count("FAIL"), RESULTS.count("UNREAD")
    print(f"{len(RESULTS)} checks: {RESULTS.count('PASS')} PASS, {fail} FAIL, {unread} could not be read")
    return 1 if fail else (2 if unread else 0)


def default_lez_rpc() -> str:
    with open(os.path.join(ROOT, "scripts", "reproduce", "deployed-programs.json"), encoding="utf-8") as f:
        return json.load(f)["network"].rstrip("/") + "/rpc"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", nargs="?", const=RECORD, metavar="RECORD")
    mode.add_argument("--record", metavar="OUT")
    ap.add_argument("--rpc", default=os.environ.get("SEPOLIA_RPC_URL") or DEFAULT_RPC)
    ap.add_argument("--lez-rpc", default=None, help="the LEZ network's JSON-RPC (default: deployed-programs.json's network + /rpc)")
    a = ap.parse_args()
    eth, lez = Sepolia(a.rpc), Lez(a.lez_rpc or default_lez_rpc())
    if a.check:
        with open(a.check, encoding="utf-8") as f:
            return run_check(json.load(f), eth, lez)
    record = build_record(eth, lez)
    proofs = load_proofs()
    for name, fn in LINKS:  # never written unless every link holds
        print(f"link: {name}: {fn(record, proofs)}")
    with open(a.record, "w", encoding="utf-8") as f:
        json.dump(record, f, indent=1)
        f.write("\n")
    print(f"wrote {a.record}: {len(record['sepolia'])} Sepolia transactions, {len(record['lez'])} LEZ transactions, "
          f"{len(record['anchor']['blocks'])} LEZ blocks walked")
    return 0


if __name__ == "__main__":
    sys.exit(main())
