"""The attestation program's messages (through its IDL, idl/ferrata_attestation.idl.json) and the light-client update
an `Open` carries (ferrata-core's strict `CompactUpdate` encoding, crates/ferrata-core/src/witness.rs), decoded with
the Python standard library for scripts/verify.py."""
from __future__ import annotations

import json
from pathlib import Path

from .lez import DecodeError, Reader

INTS = {"u8": 1, "u16": 2, "u32": 4, "u64": 8, "u128": 16}
PRIMS32 = {"account_id", "AccountId", "program_id", "ProgramId"}
FORKS = ("Altair", "Capella", "Deneb", "Electra", "Gloas")
# floorlog2 of the generalized indices of crates/ferrata-core/src/merkle.rs, by fork (types.rs: *_branch_len)
SYNC_BRANCH = {"Altair": 5, "Capella": 5, "Deneb": 5, "Electra": 6, "Gloas": 11}
FINALITY_BRANCH = {"Altair": 6, "Capella": 6, "Deneb": 6, "Electra": 7, "Gloas": 9}
SUMMARY_LEN = 80  # CommitteeSummary: pubkeys_root (32) ‖ aggregate_pubkey (48)


class Idl:
    """SPEL's IDL as the explorer reads it (deploy/testnet/web/explorer/assets/idl.js): an instruction is its one-byte
    discriminator, then its args in borsh."""

    def __init__(self, doc: dict):
        self.doc = doc
        self.defs = {t["name"]: t.get("type", t) for t in doc.get("types", [])}
        for a in doc.get("accounts", []):
            self.defs.setdefault(a["name"], a.get("type", a))
        self.by_disc = {i["discriminator"][0]: i for i in doc.get("instructions", []) if len(i.get("discriminator", [])) == 1}
        self.errors = {e["code"]: e for e in doc.get("errors", [])}

    @classmethod
    def load(cls, path: Path) -> "Idl":
        return cls(json.loads(path.read_text()))

    def instruction(self, msg: bytes):
        if not msg:
            raise DecodeError("empty message")
        ins = self.by_disc.get(msg[0])
        if ins is None:
            raise DecodeError(f"no instruction has discriminator {msg[0]}")
        r = Reader(msg, 1)
        args = {a["name"]: self.value(r, a["type"]) for a in ins["args"]}
        r.finish(ins["name"])
        return ins["name"], args

    def decode_type(self, data: bytes, name: str):
        r = Reader(data)
        v = self.defined(r, name)
        r.finish(name)
        return v

    def defined(self, r: Reader, name: str):
        td = self.defs.get(name)
        if td is None:
            raise DecodeError(f"undefined type {name}")
        if td.get("kind") == "struct":
            return {f["name"]: self.value(r, f["type"]) for f in td.get("fields", [])}
        idx = r.u8()
        variants = td.get("variants", [])
        if idx >= len(variants):
            raise DecodeError(f"{name}: variant {idx} of {len(variants)}")
        v = variants[idx]
        return {"variant": v["name"], **{f["name"]: self.value(r, f["type"]) for f in v.get("fields", [])}}

    def value(self, r: Reader, t):
        if isinstance(t, str):
            if t in INTS:
                return r.uint(INTS[t])
            if t == "bool":
                return r.flag()
            if t == "string":
                return r.bytes_vec().decode("utf-8")
            if t in PRIMS32:
                return r.take(32)
            raise DecodeError(f"unknown primitive {t}")
        (kind, inner), = t.items()
        if kind == "vec":
            if inner == "u8":
                return r.bytes_vec()
            return r.vec(lambda rr: self.value(rr, inner))
        if kind == "array":
            elem, n = inner
            if elem == "u8":
                return r.take(n)
            return [self.value(r, elem) for _ in range(n)]
        if kind == "option":
            return r.option(lambda rr: self.value(rr, inner))
        if kind == "defined":
            return self.defined(r, inner if isinstance(inner, str) else inner["name"])
        raise DecodeError(f"type form {kind}")


def _beacon(header_ssz: bytes) -> dict:
    """The beacon block header at the start of a light-client header's SSZ (112 bytes)."""
    if len(header_ssz) < 112:
        raise DecodeError("light-client header shorter than its beacon header")
    return {"slot": int.from_bytes(header_ssz[0:8], "little"),
            "proposer_index": int.from_bytes(header_ssz[8:16], "little"),
            "parent_root": header_ssz[16:48], "state_root": header_ssz[48:80], "body_root": header_ssz[80:112]}


def beacon_root(h: dict) -> bytes:
    """hash_tree_root(BeaconBlockHeader)."""
    import hashlib
    leaves = [h["slot"].to_bytes(8, "little").ljust(32, b"\0"), h["proposer_index"].to_bytes(8, "little").ljust(32, b"\0"),
              h["parent_root"], h["state_root"], h["body_root"]] + [b"\0" * 32] * 3
    while len(leaves) > 1:
        leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
    return leaves[0]


def compact_update(b: bytes, n: int = 512) -> dict:
    """ferrata-core's `CompactUpdate::decode` for a committee of `n`: the fields the run record needs."""
    r = Reader(b)
    fb = r.u8()
    if fb >= len(FORKS):
        raise DecodeError(f"fork byte {fb}")
    fork = FORKS[fb]
    attested = _beacon(r.bytes_vec())
    has_next = r.flag()
    if has_next:
        r.take(SUMMARY_LEN)
    next_branch = [r.take(32) for _ in range(SYNC_BRANCH[fork])]
    finalized = _beacon(r.bytes_vec())
    finality_branch = [r.take(32) for _ in range(FINALITY_BRANCH[fork])]
    bits = r.take(n // 8)
    r.take(96)  # the aggregate signature
    signature_slot = r.u64()
    r.finish("CompactUpdate")
    return {"fork": fork, "attested_slot": attested["slot"], "finalized_slot": finalized["slot"],
            "finalized_root": beacon_root(finalized), "signature_slot": signature_slot,
            "participants": sum(bin(x).count("1") for x in bits), "committee_size": n,
            "next_committee": has_next,
            "sync_committee_update": any(x != b"\0" * 32 for x in next_branch),
            "finality_update": any(x != b"\0" * 32 for x in finality_branch)}
