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
# floorlog2 of EXECUTION_PAYLOAD_GINDEX (Capella to Electra) and EXECUTION_BLOCK_HASH_GINDEX_GLOAS (merkle.rs)
EXECUTION_BRANCH = {"Altair": 0, "Capella": 4, "Deneb": 4, "Electra": 4, "Gloas": 11}
MAX_EXTRA_DATA_BYTES = 32
# ExecutionPayloadHeader's fields (types.rs): sizes, None for the variable extra_data; Deneb on adds two u64
PAYLOAD_FIELDS = [32, 20, 32, 32, 256, 32, 8, 8, 8, 8, None, 32, 32, 32, 32]


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
        if not isinstance(td, dict) or "kind" not in td:
            return self.value(r, td)  # an alias of another type (the names IDL's `Sequence` is a u64)
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
    """`BeaconBlockHeader::decode`: exactly 112 bytes."""
    if len(header_ssz) != 112:
        raise DecodeError(f"a beacon block header is 112 bytes, not {len(header_ssz)}")
    return {"slot": int.from_bytes(header_ssz[0:8], "little"),
            "proposer_index": int.from_bytes(header_ssz[8:16], "little"),
            "parent_root": header_ssz[16:48], "state_root": header_ssz[48:80], "body_root": header_ssz[80:112]}


def _split(b: bytes, fields: list) -> list:
    """ssz.rs's `split_container`: fixed fields by size, variable ones (None) by their u32 offsets, the first offset
    at the end of the fixed part, offsets in order, the last part running to the end."""
    fixed = sum(4 if f is None else f for f in fields)
    if len(b) < fixed:
        raise DecodeError(f"SSZ container of {len(b)} bytes, its fixed part is {fixed}")
    parts, offsets, pos = [], [], 0
    for i, f in enumerate(fields):
        if f is None:
            offsets.append((i, int.from_bytes(b[pos:pos + 4], "little")))
            parts.append(b"")
            pos += 4
        else:
            parts.append(b[pos:pos + f])
            pos += f
    if not offsets and len(b) != fixed:
        raise DecodeError(f"SSZ container of {len(b)} bytes, fixed size {fixed}")
    if offsets and offsets[0][1] != fixed:
        raise DecodeError(f"SSZ: first offset {offsets[0][1]}, the fixed part ends at {fixed}")
    for k, (i, start) in enumerate(offsets):
        end = offsets[k + 1][1] if k + 1 < len(offsets) else len(b)
        if start > end or end > len(b):
            raise DecodeError("SSZ: offsets out of order or past the end")
        parts[i] = b[start:end]
    return parts


def light_client_header(b: bytes, fork: str) -> dict:
    """`LightClientHeader::decode` in the layout of `fork` (types.rs): the beacon header, with every other byte of the
    layout checked (the execution payload header and its branch, or Gloas's block hash and its branch)."""
    n = EXECUTION_BRANCH[fork]
    if fork == "Altair":
        return _beacon(b)
    if fork == "Gloas":
        if len(b) != 112 + 32 + 32 * n:
            raise DecodeError(f"a Gloas light-client header is {112 + 32 + 32 * n} bytes, not {len(b)}")
        return _beacon(b[:112])
    p = _split(b, [112, None, 32 * n])
    q = _split(p[1], PAYLOAD_FIELDS + ([] if fork == "Capella" else [8, 8]))
    if len(q[10]) > MAX_EXTRA_DATA_BYTES:
        raise DecodeError(f"extra_data of {len(q[10])} bytes")
    return _beacon(p[0])


def beacon_root(h: dict) -> bytes:
    """hash_tree_root(BeaconBlockHeader)."""
    import hashlib
    leaves = [h["slot"].to_bytes(8, "little").ljust(32, b"\0"), h["proposer_index"].to_bytes(8, "little").ljust(32, b"\0"),
              h["parent_root"], h["state_root"], h["body_root"]] + [b"\0" * 32] * 3
    while len(leaves) > 1:
        leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
    return leaves[0]


def compact_update(b: bytes, n: int = 512) -> dict:
    """ferrata-core's `CompactUpdate::decode` (witness.rs) for a committee of `n`, as strict: the fork byte, both
    light-client headers in that fork's layout, the branches, the sync aggregate's bits (n rounded up to bytes, the
    padding bits zero) and signature, the signature slot, nothing left over."""
    if n < 1:
        raise DecodeError(f"committee size {n}")
    r = Reader(b)
    fb = r.u8()
    if fb >= len(FORKS):
        raise DecodeError(f"fork byte {fb}")
    fork = FORKS[fb]
    attested = light_client_header(r.bytes_vec(), fork)
    has_next = r.flag()
    summary = r.take(SUMMARY_LEN) if has_next else None
    next_branch = [r.take(32) for _ in range(SYNC_BRANCH[fork])]
    finalized = light_client_header(r.bytes_vec(), fork)
    finality_branch = [r.take(32) for _ in range(FINALITY_BRANCH[fork])]
    bits = r.take((n + 7) // 8)  # SyncAggregate::ssz_len: the bitvector's bytes, n bits rounded up
    if n % 8 and bits[-1] >> (n % 8):
        raise DecodeError("sync committee bits: a padding bit is set")
    signature = r.take(96)
    signature_slot = r.u64()
    r.finish("CompactUpdate")
    return {"fork": fork, "attested_slot": attested["slot"], "finalized_slot": finalized["slot"],
            "finalized_root": beacon_root(finalized), "attested_root": beacon_root(attested),
            "attested_state_root": attested["state_root"], "signature_slot": signature_slot,
            "participants": sum(bin(x).count("1") for x in bits), "committee_size": n,
            "next_committee": has_next, "next_committee_summary": summary, "next_committee_branch": next_branch,
            "finality_branch": finality_branch, "sync_committee_bits": bits, "sync_committee_signature": signature,
            "sync_committee_update": any(x != b"\0" * 32 for x in next_branch),
            "finality_update": any(x != b"\0" * 32 for x in finality_branch)}
