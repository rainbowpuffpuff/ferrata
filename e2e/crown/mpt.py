"""Ethereum Merkle-Patricia proofs (eth_getProof) checked against a block header, standard library only.

Used by e2e/crown/capture.py (every captured slot checked against the captured header's state root) and
e2e/crown/verify.py. Keccak-256 is the pure-Python one of scripts/fetch-mainnet-e2e.py.
"""
import importlib.util
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location(
    "fetch_mainnet_e2e", os.path.join(_HERE, "..", "..", "scripts", "fetch-mainnet-e2e.py"))
_e2e = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_e2e)
keccak256 = _e2e.keccak256


def rlp_decode(data: bytes):
    item, end = _rlp_item(data, 0)
    if end != len(data):
        raise ValueError("trailing bytes after an RLP item")
    return item


def _rlp_item(b: bytes, i: int):
    p = b[i]
    if p < 0x80:
        return b[i:i + 1], i + 1
    if p < 0xb8:
        n = p - 0x80
        return b[i + 1:i + 1 + n], i + 1 + n
    if p < 0xc0:
        ln = p - 0xb7
        n = int.from_bytes(b[i + 1:i + 1 + ln], "big")
        return b[i + 1 + ln:i + 1 + ln + n], i + 1 + ln + n
    if p < 0xf8:
        n, start = p - 0xc0, i + 1
    else:
        ln = p - 0xf7
        n, start = int.from_bytes(b[i + 1:i + 1 + ln], "big"), i + 1 + ln
    out, j = [], start
    while j < start + n:
        x, j = _rlp_item(b, j)
        out.append(x)
    if j != start + n:
        raise ValueError("RLP list length")
    return out, j


def _nibbles(b: bytes) -> list:
    return [x for c in b for x in (c >> 4, c & 15)]


def _hp(path: bytes):
    n = _nibbles(path)
    flag = n[0]
    return n[2:] if flag in (0, 2) else n[1:], flag >= 2


def mpt_get(root: bytes, key: bytes, proof: list):
    """The value stored under keccak(key) in the trie with this root, or None if the proof shows it absent."""
    path = _nibbles(keccak256(key))
    nodes = [bytes.fromhex(x.removeprefix("0x")) for x in proof]
    want, i, k = root, 0, 0
    node = None
    while True:
        if node is None:
            if k >= len(nodes):
                raise ValueError("the proof ends before the key's path does")
            raw = nodes[k]
            k += 1
            if keccak256(raw) != want:
                raise ValueError(f"proof node {k - 1} does not hash to its parent's reference")
            node = rlp_decode(raw)
        if len(node) == 17:
            if i == len(path):
                return node[16] or None
            child = node[path[i]]
            i += 1
        elif len(node) == 2:
            seg, leaf = _hp(node[0])
            if path[i:i + len(seg)] != seg:
                return None
            i += len(seg)
            if leaf:
                return node[1] if i == len(path) else None
            child = node[1]
        else:
            raise ValueError("a trie node is neither a branch nor a short node")
        if isinstance(child, list):  # an embedded node (its RLP is shorter than 32 bytes)
            node = child
        elif child == b"":
            return None
        else:
            want, node = child, None


def header_state_root(header_rlp: bytes) -> bytes:
    return rlp_decode(header_rlp)[3]


def check_account_and_slots(state_root: bytes, proof: dict) -> dict:
    """eth_getProof's answer checked against state_root: the account (nonce, balance, storage root, code hash)
    and every storage slot's value. Returns {"slots": {slot: value int}, ...} or raises ValueError."""
    address = bytes.fromhex(proof["address"].removeprefix("0x"))
    acct = mpt_get(state_root, address, proof["accountProof"])
    if acct is None:
        raise ValueError(f"{proof['address']} is absent from the state")
    nonce, balance, storage_root, code_hash = rlp_decode(acct)
    if "0x" + storage_root.hex() != proof["storageHash"].lower() or "0x" + code_hash.hex() != proof["codeHash"].lower():
        raise ValueError("the account proof's storage root or code hash is not the answer's")
    slots = {}
    for sp in proof["storageProof"]:
        key = int(sp["key"], 16).to_bytes(32, "big")
        raw = mpt_get(storage_root, key, sp["proof"])
        got = 0 if raw is None else int.from_bytes(rlp_decode(raw), "big")
        if got != int(sp["value"], 16):
            raise ValueError(f"slot {sp['key']}: the proof gives {got:#x}, the answer {sp['value']}")
        slots["0x" + key.hex()] = got
    return {"nonce": int.from_bytes(nonce, "big"), "balance": int.from_bytes(balance, "big"),
            "codeHash": "0x" + code_hash.hex(), "slots": slots}
