#!/usr/bin/env python3
"""The first full round trip on the Ferrata LEZ test network (Sepolia -> the network's LEZ zone -> Sepolia, 23-24
November 2026), checked from public chain data alone. Python 3 standard library only; read-only (no key, nothing
signed or sent).

    python3 -I scripts/verify-live-roundtrip.py --check [RECORD] [--rpc URL] [--lez-rpc URL]
        Re-read every fact of RECORD (default e2e/runs/live-roundtrip-2026-11-24/record.json) and compare. One line per
        fact: PASS (the chains agree with the record) or FAIL; exit 1 on a FAIL, 2 when an endpoint cannot be read.
    python3 -I scripts/verify-live-roundtrip.py --record OUT [--rpc URL] [--lez-rpc URL]
        The maintainer's step: read the same facts and write them to OUT.

Sepolia (any public JSON-RPC that serves state at old blocks; scripts/pubnet.py's endpoints after --rpc): each
transaction's receipt (block, status, gas, sender, target), the vault's events in it (and that its block holds no other
vault event), and the vault's state by eth_call at the end of the block before and of the transaction's own block, with
the balances involved. The LEZ zone (its public JSON-RPC, by default the network named in
scripts/reproduce/deployed-programs.json): each transaction's bytes (their SHA-256 is the transaction's id), the block
that holds it and whether it reverted there, and its bridge message decoded with idl/ferrata_bridge.idl.json by
idl/check.py's decoder, every byte used. Then the links that make it one round trip:

  1. the import: the zone's root(cid, 1) is the vault's depositRoot after the deposit (the claim proves its leaf under
     the root actor root(cid, 1), and that root is folded below);
  2. the claim opens the deposit: SHA-256("ferrata/C/v1" || s || kind || dest), from the claim's own witness, is the
     Deposit event's commitment; dest is the account the claim credits (kind 2: a public destination); the leaf the
     vault hashed (FerrataVault._leafHash: depositor, token, amount, that commitment, leaf 0) folded with the claim's
     path is the vault's depositRoot after the deposit;
  3. the burn is the checkpoint's: its burn leaf (encoding.rs, burn index 0, from the burn's own fields) folded with an
     empty path is the burnRoot of the candidate, of the finalized checkpoint and of the release;
  4. the proofs: the candidate's journalDigest on the vault equals the checkpoint proof's journal digest in the prover
     client's record, and the release journal (FerrataVault._releaseJournalDigest, from the release's own calldata and
     amount) hashes to the release proof's journal digest. The proof times are the prover client's (not on chain).
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
BRIDGE_IDL = os.path.join(ROOT, "idl", "ferrata_bridge.idl.json")
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
# Not on chain: the prover client's own records of the two GPU proofs (deploy/testnet/host/bin/ferrata-bridge-prover,
# requests/done/). Their journal digests are checked against the chain (link 4); the times are as it measured them,
# from the request's pickup to the proof's return, Modal's start included.
PROOFS = {
    "checkpoint": {"name": "zone-1225-3767", "kind": "checkpoint-b", "program": "checkpoint_b_0_3_5",
                   "image_id": "0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04", "seconds": 1307.0,
                   "journal_bytes": 544,
                   "journal_sha256": "0xd71cefb9bc52b8e6b5d8d17b6a07b986c1223a2d3bd285aae7c8c5f0d332b0c6",
                   "done_utc": "2026-11-23T21:03:16Z", "lez_blocks": "1226..=3767 (2,542 blocks)",
                   "input_bytes": 3064586, "where": "a Modal GPU (deploy/testnet/host/bin/ferrata-bridge-prover)"},
    "release": {"name": "release-0", "kind": "release", "program": "release_31",
                "image_id": "0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd", "seconds": 425.6,
                "journal_bytes": 352,
                "journal_sha256": "0x54ee277200b23ba02de5ad49c0f4112feac41a9b16579997495b31f70e1147e4",
                "done_utc": "2026-11-24T09:09:10Z", "where": "a Modal GPU (Groth16)"},
}
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
    """getBlock's bytes: header (id, prev, hash, timestamp ms), the transactions, the reverted indices, Bedrock status."""
    r = Reader(raw)
    block_id, _prev, block_hash, ts = r.u(8), r.take(32), r.take(32), r.u(8)
    txs = r.vec(lambda: read_tx(r))
    reverted = r.vec(lambda: r.u(8))
    r.u(1)
    return {"id": block_id, "hash": block_hash.hex(), "timestamp_ms": ts, "tx_ids": [t["id"] for t in txs], "reverted": reverted}


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
    return {"journal_bytes": len(j), "journal_sha256": h(sha256(j)), "relayer_field": "0x" + relayer[24:],
            "nullifier": "0x" + nullifier, "burnRoot": "0x" + burn_root}


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
    out = []
    blocks = {}
    for name, tx in LEZ_STEPS:
        raw_b64, block = lez("getTransaction", tx)
        raw = base64.b64decode(raw_b64)
        t = read_tx(Reader(raw))
        if block not in blocks:
            blocks[block] = read_block(base64.b64decode(lez("getBlock", block)))
        b = blocks[block]
        idx = b["tx_ids"].index(t["id"]) if t["id"] in b["tx_ids"] else None
        out.append({"step": name, "tx": tx, "id_of_bytes": t["id"], "block": block, "block_hash": b["hash"],
                    "block_timestamp_ms": b["timestamp_ms"], "index_in_block": idx,
                    "reverted": idx in b["reverted"] if idx is not None else None, "program": t["program"],
                    "root_actor": t["actor"], "signers": t["signers"], "nonces": t["nonces"],
                    "message": decode_bridge(t["message"]), "bytes_base64": raw_b64})
    return out


def links(sep: list, lezs: list) -> dict:
    by = {s["step"]: s for s in sep}
    lz = {s["step"]: s for s in lezs}
    dep = next(e for e in by["deposit"]["vault_events"] if e["event"] == "Deposit")
    cand = next(e for e in by["submitCandidate"]["vault_events"] if e["event"] == "CandidateSubmitted")
    fin = next(e for e in by["finalizeAttested"]["vault_events"] if e["event"] == "CheckpointFinalized")
    rel = next(e for e in by["release"]["vault_events"] if e["event"] == "Release")
    c = claim_links(lz["claim"]["message"])
    b = burn_links(lz["burn"]["message"])
    config_id = lz["claim"]["message"]["args"]["config_id"]
    j = release_journal(by["release"]["input"], rel["amount"], config_id)
    return {
        "claim_opens_deposit": {**c, "deposit_commitment": dep["commitment"],
                                "deposit_root_after_deposit": by["deposit"]["vault_after"]["depositRoot"],
                                "imported_root": lz["import_root"]["message"]["args"]["root"],
                                "imported_leaf_count": lz["import_root"]["message"]["args"]["leaf_count"],
                                "claim_leaf_count": lz["claim"]["message"]["args"]["leaf_count"],
                                "claim_signer": lz["claim"]["signers"][0]},
        "burn_is_the_checkpoints": {**b, "candidate_burnRoot": cand["burnRoot"], "finalized_burnRoot": fin["burnRoot"],
                                    "release_burnRoot": rel["burnRoot"], "burnCount": cand["burnCount"]},
        "proofs": {"checkpoint_journalDigest_on_chain": by["submitCandidate"]["vault_after"]["candidates(newState).journalDigest"],
                   "checkpoint_journal_sha256_prover": PROOFS["checkpoint"]["journal_sha256"],
                   **{"release_" + k: v for k, v in j.items()},
                   "release_journal_sha256_prover": PROOFS["release"]["journal_sha256"]},
    }


# ---- check -----------------------------------------------------------------------------------------------------------

def check(name: str, fn) -> None:
    try:
        status, detail = "PASS", fn() or ""
    except Failed as e:
        status, detail = "FAIL", str(e)
    except (Unreadable, KeyError, ValueError, TypeError, IndexError, idl_check.DecodeError) as e:
        status, detail = ("UNREAD" if isinstance(e, Unreadable) else "FAIL"), f"{type(e).__name__}: {e}"
    RESULTS.append(status)
    print(f"{status:<8} {name}{': ' + detail if detail else ''}", flush=True)


def need(cond: bool, msg: str) -> None:
    if not cond:
        raise Failed(msg)


def same(label: str, got, want) -> str:
    need(got == want, f"{label}: chain {got!r}, record {want!r}")
    return ""


def run_check(record: dict, eth: Sepolia, lez: Lez) -> int:
    rec_sep = {s["step"]: s for s in record["sepolia"]}
    rec_lez = {s["step"]: s for s in record["lez"]}
    keys = record["keys"]
    check("Sepolia: the endpoint is Sepolia", lambda: same("chain id", int(eth("eth_chainId"), 16), CHAIN_ID))
    for name, tx in SEPOLIA_STEPS:
        r = rec_sep[name]

        def tx_facts(tx=tx, r=r):
            rc = eth("eth_getTransactionReceipt", tx, anywhere=True)
            same("block", int(rc["blockNumber"], 16), r["block"])
            same("status", int(rc["status"], 16), r["status"])
            same("gas used", int(rc["gasUsed"], 16), r["gas_used"])
            same("from", rc["from"].lower(), r["from"])
            same("to", (rc["to"] or "").lower(), r["to"])
            same("vault events", [decode_log(lg) for lg in rc["logs"] if lg["address"].lower() == VAULT], r["vault_events"])
            t = eth("eth_getTransactionByHash", tx, anywhere=True)
            same("calldata", t["input"], r["input"])
            same("sender's nonce", int(t["nonce"], 16), r["nonce"])
            return (f"block {r['block']}, status {r['status']}, gas {r['gas_used']:,}, nonce {r['nonce']} of {r['from'][:10]}…, "
                    + ", ".join(e["event"] for e in r["vault_events"]))
        check(f"Sepolia {name} {tx[:12]}…: receipt and the vault's events", tx_facts)

        def alone(r=r, tx=tx):
            others = [lg["transactionHash"] for lg in eth("eth_getLogs", {"address": VAULT, "fromBlock": hex(r["block"]), "toBlock": hex(r["block"])})
                      if lg["transactionHash"].lower() != tx]
            same("other vault transactions in the block", others, r["other_vault_txs_in_block"])
            return "no other vault event in its block" if not others else f"{len(others)} other"
        check(f"Sepolia {name}: the vault's only event in block {r['block']}", alone)
        for when, block in (("before", r["block"] - 1), ("after", r["block"])):
            check(f"Sepolia {name}: the vault's state {when} (eth_call at block {block})",
                  lambda block=block, when=when, r=r: same("state", vault_state(eth, block, keys), r["vault_" + when]))
    for name, tx in LEZ_STEPS:
        r = rec_lez[name]

        def lez_facts(tx=tx, r=r):
            raw_b64, block = lez("getTransaction", tx)
            same("bytes", raw_b64, r["bytes_base64"])
            t = read_tx(Reader(base64.b64decode(raw_b64)))
            same("SHA-256 of the bytes (the transaction's id)", t["id"], tx)
            same("block", block, r["block"])
            b = read_block(base64.b64decode(lez("getBlock", block)))
            idx = b["tx_ids"].index(tx) if tx in b["tx_ids"] else None
            same("index in the block", idx, r["index_in_block"])
            same("reverted in its block", idx in b["reverted"], r["reverted"])
            same("program", t["program"], r["program"])
            same("signers", t["signers"], r["signers"])
            same("decoded message", decode_bridge(t["message"]), r["message"])
            return (f"block {block}, {'REVERTED' if r['reverted'] else 'applied'}, {r['message']['instruction']} "
                    f"(every byte decoded by the bridge IDL), signed by {r['signers'][0][:8]}…")
        check(f"LEZ {name} {tx[:12]}…: bytes, block, outcome, decoded message", lez_facts)
    lk = record["links"]

    def l1():
        c = claim_links(rec_lez["claim"]["message"])
        same("recomputed", c, {k: lk["claim_opens_deposit"][k] for k in c})
        need(c["commitment"] == lk["claim_opens_deposit"]["deposit_commitment"], "the commitment is not the deposit's")
        x = lk["claim_opens_deposit"]
        need(c["root"] == x["deposit_root_after_deposit"], "the path does not reach the vault's root")
        imp = rec_lez["import_root"]["message"]["args"]
        need(imp["root"] == x["deposit_root_after_deposit"] == x["imported_root"] and imp["leaf_count"] == x["claim_leaf_count"] == 1,
             "the imported root(cid, 1) is not the vault's root after the deposit")
        need(c["kind"] == 2 and c["dest_account"] == lk["claim_opens_deposit"]["claim_signer"], "the destination is not the claimer")
        return (f"commitment {c['commitment'][:12]}… = the Deposit event's; leaf 0 + path -> {c['root'][:12]}… = depositRoot after "
                f"the deposit = the root import_root wrote as root(cid, 1); dest = {c['dest_account'][:8]}…")
    check("link: the import and the claim (the imported root, the claim's commitment, leaf and path)", l1)

    def l2():
        b = burn_links(rec_lez["burn"]["message"])
        x = lk["burn_is_the_checkpoints"]
        same("recomputed", b, {k: x[k] for k in b})
        need(b["root"] == x["candidate_burnRoot"] == x["finalized_burnRoot"] == x["release_burnRoot"] and x["burnCount"] == 1,
             "the burn root differs between the burn, the checkpoint and the release")
        return f"burn leaf {b['leaf'][:12]}… alone -> {b['root'][:12]}… = the candidate's, the checkpoint's and the release's burnRoot"
    check("link: the burn is the checkpoint's only burn, the one released", l2)

    def l3():
        x = lk["proofs"]
        j = release_journal(rec_sep["release"]["input"], next(e for e in rec_sep["release"]["vault_events"] if e["event"] == "Release")["amount"],
                            rec_lez["claim"]["message"]["args"]["config_id"])
        same("release journal", {"release_" + k: v for k, v in j.items()}, {k: x[k] for k in x if k.startswith("release_") and k != "release_journal_sha256_prover"})
        need(j["journal_sha256"] == PROOFS["release"]["journal_sha256"] == x["release_journal_sha256_prover"], "the release journal is not the proof's")
        need(x["checkpoint_journalDigest_on_chain"] == PROOFS["checkpoint"]["journal_sha256"] == x["checkpoint_journal_sha256_prover"],
             "the candidate's journalDigest is not the checkpoint proof's")
        return "the candidate's journalDigest = the checkpoint proof's; the release journal (from its calldata) hashes to the release proof's"
    check("link: the two GPU proofs are the ones the vault verified", l3)
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
    sep, keys = read_sepolia(eth)
    lezs = read_lez(lez)
    record = {
        "about": "The first full round trip on the Ferrata LEZ test network, as public chain data: Sepolia (the vault) "
                 "and the network's LEZ v0.4.0-rc3 zone. Written and re-checked by scripts/verify-live-roundtrip.py; "
                 "README.md beside this file says what each part shows.",
        "vault": VAULT, "chain_id": CHAIN_ID, "keys": keys, "sepolia": sep, "lez": lezs,
        "proofs_offchain": PROOFS, "links": links(sep, lezs),
    }
    with open(a.record, "w", encoding="utf-8") as f:
        json.dump(record, f, indent=1)
        f.write("\n")
    print(f"wrote {a.record}: {len(sep)} Sepolia transactions, {len(lezs)} LEZ transactions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
