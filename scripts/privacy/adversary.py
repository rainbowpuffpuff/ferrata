#!/usr/bin/env python3
"""The privacy suite's analysis (proposal B3; RFP-022 Privacy #1-#7, F.9, R.5). Standard library only.

    python3 -I scripts/privacy/adversary.py DATA_DIR OUT_DIR

DATA_DIR holds what the two collectors wrote: lez.json (lez/harness/tests/privacy_suite.rs), eth.jsonl
(vault/test/PrivacySuite.t.sol), scenario.json, truth.json. OUT_DIR receives results.json, fields.md and
summary.md. Four parts:

1. The differential gate: within each operation type, the 20 records' public data (LEZ: transaction bytes,
   block body, events, state diff; Ethereum: sender, calldata, events, storage writes, balance changes) must
   be identical once every value of a field classified as a nullifier, fresh randomness, a user's own public
   address, or time (block, order-dependent shared state) is cut out.
2. The adversary: pairs each claim with a deposit and each release with a burn using all public bytes and
   every identifier it can derive from them (deposit leaf and fid, burn leaf and burnId, hashes), but no
   token, denomination or time order. Evidence is a shared value weighted by how rare it is on both sides;
   its guess for a record is the set of best-scoring candidates. Success = mean of 1/|set| over records
   whose true partner is in the set (chance: 1/20).
3. The field table, generated from the records and compared with the proposal's section B6.
4. Recovery from the seed alone: an independent wallet (this file's own HMAC/SHA-256 code) scans the full
   lists (all deposits, all LEZ nullifiers, all burn notes, all release nullifiers) at the point where half
   the claims and half the releases have happened; no per-item query exists in its interface.

The ground truth (truth.json) is read only to score the adversary and the recovery.

With real proofs (scripts/privacy/real.sh) the same analysis runs on real receipts: LEZ's proof is split by the
collector into its parts (seal, control id, claim, ...) and the release's Groth16 seal into selector and proof, and
each part is classified and tested like any other field (a fifth part, "proofs", reports what was checked about them).
"""

import hashlib
import hmac
import json
import math
import os
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

N_WINDOW = 16  # bytes: the adversary's shared-value granularity


# ------------------------------------------------------------------------------------------------
# Hashes and encodings (layouts: bridge/README.md "Encodings", lez/bridge-wire, SPEC-v2)
# ------------------------------------------------------------------------------------------------

def sha(b):
    return hashlib.sha256(b).digest()


def prf(k, m):
    return hmac.new(k, m, hashlib.sha256).digest()


def u64(n):
    return n.to_bytes(8, "big")


MASK = (1 << 64) - 1
RC = [0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000, 0x000000000000808B,
      0x0000000080000001, 0x8000000080008081, 0x8000000000008009, 0x000000000000008A, 0x0000000000000088,
      0x0000000080008009, 0x000000008000000A, 0x000000008000808B, 0x800000000000008B, 0x8000000000008089,
      0x8000000000008003, 0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
      0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008]
ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]]


def _rol(v, n):
    return ((v << n) | (v >> (64 - n))) & MASK if n else v


def keccak256(data):
    """Keccak-256 (Ethereum's), to name the vault's mapping slots and check event selectors."""
    rate = 136
    msg = bytearray(data) + b"\x01"
    while len(msg) % rate:
        msg.append(0)
    msg[-1] |= 0x80
    a = [[0] * 5 for _ in range(5)]
    for off in range(0, len(msg), rate):
        for i in range(rate // 8):
            a[i % 5][i // 5] ^= int.from_bytes(msg[off + 8 * i:off + 8 * i + 8], "little")
        for rc in RC:
            c = [a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4] for x in range(5)]
            d = [c[(x - 1) % 5] ^ _rol(c[(x + 1) % 5], 1) for x in range(5)]
            a = [[a[x][y] ^ d[x] for y in range(5)] for x in range(5)]
            b = [[0] * 5 for _ in range(5)]
            for x in range(5):
                for y in range(5):
                    b[y][(2 * x + 3 * y) % 5] = _rol(a[x][y], ROT[x][y])
            a = [[b[x][y] ^ ((~b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]) for y in range(5)] for x in range(5)]
            a[0][0] ^= rc
    return b"".join(a[i % 5][i // 5].to_bytes(8, "little") for i in range(4))


assert keccak256(b"").hex() == "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"

LEAF_TAG = sha(b"ferrata/vault/leaf/v2")
BURN_TAG = sha(b"ferrata/burn/leaf/v2")


def deposit_leaf(index, depositor, token, amount, c):
    return sha(b"\x00" + LEAF_TAG + u64(index) + depositor + token + amount + c)


def fid_of(chain, vault, index, leaf):
    return sha(b"ferrata/fact/acc-leaf/v1" + chain + vault + bytes(32) + u64(index) + leaf)


def burn_leaf(cid, index, token, amount, c_out, r):
    return sha(b"\x02" + BURN_TAG + cid + u64(index) + token + amount + c_out + r)


def burn_id(cid, index, leaf):
    return sha(b"ferrata/burn/v1" + cid + u64(index) + leaf)


# RISC Zero receipts (risc0-zkvm 3.0.5), read from the public bytes alone: the claim's borsh layout and digest rules
# (`ReceiptClaim`, `Output`, `SystemState`, tagged structs and lists) and the seal's output words, BabyBear elements in
# Montgomery form (`SuccinctReceipt::verify_integrity_with_context` decodes them with `as_u32`).
BB_P = 2013265921
BB_RINV = pow((1 << 32) % BB_P, -1, BB_P)
ZERO32 = bytes(32)


def tagged_struct(tag, down, data=()):
    return sha(sha(tag.encode()) + b"".join(down) + b"".join(w.to_bytes(4, "little") for w in data)
               + len(down).to_bytes(2, "little"))


def claim_ok_digest(image_id, journal):
    """The digest of ReceiptClaim::ok(image_id, journal)."""
    post = tagged_struct("risc0.SystemState", [ZERO32], [0])
    output = tagged_struct("risc0.Output", [sha(journal), ZERO32])
    return tagged_struct("risc0.ReceiptClaim", [ZERO32, image_id, post, output], [0, 0])


def parse_claim(buf):
    """borsh MaybePruned<ReceiptClaim> -> (its digest, the journal if it is there unpruned)."""
    pos = [0]

    def take(n):
        if pos[0] + n > len(buf):
            raise ValueError("short claim")
        pos[0] += n
        return buf[pos[0] - n:pos[0]]

    u8 = lambda: take(1)[0]
    u32 = lambda: int.from_bytes(take(4), "little")

    def maybe(value):
        t = u8()
        if t == 0:
            return value()
        if t == 1:
            return take(32), None
        raise ValueError("MaybePruned tag")

    def option(value):
        t = u8()
        if t == 0:
            return ZERO32, None
        if t == 1:
            return value()
        raise ValueError("Option tag")

    def system_state():
        pc = u32()
        return tagged_struct("risc0.SystemState", [take(32)], [pc]), None

    def journal():
        j = take(u32())
        return sha(j), j

    def assumptions():
        ds = [maybe(lambda: (tagged_struct("risc0.Assumption", [take(32), take(32)]), None))[0] for _ in range(u32())]
        d = ZERO32
        for e in reversed(ds):
            d = tagged_struct("risc0.Assumptions", [e, d])
        return d, None

    def output():
        jd, j = maybe(journal)
        ad, _ = maybe(assumptions)
        return tagged_struct("risc0.Output", [jd, ad]), j

    def uninhabited():
        raise ValueError("Input is uninhabited")

    def claim():
        pre, _ = maybe(system_state)
        post, _ = maybe(system_state)
        t = u8()
        code = (t, u32()) if t in (0, 1) else {2: (2, 0), 3: (2, 2)}[t]
        inp, _ = maybe(lambda: option(uninhabited))
        out, j = maybe(lambda: option(output))
        return tagged_struct("risc0.ReceiptClaim", [inp, pre, post, out], list(code)), j

    d, j = maybe(claim)
    if pos[0] != len(buf):
        raise ValueError("trailing bytes after the claim")
    return d, j


def seal_claim_digest(part):
    """The claim digest a succinct seal's output words 16..32 carry (16-bit halves, Montgomery form), or None."""
    hs = [(int.from_bytes(part[4 * i:4 * i + 4], "little") * BB_RINV) % BB_P for i in range(16)]
    return None if any(h >= 1 << 16 for h in hs) else b"".join(bytes([h & 0xFF, h >> 8]) for h in hs)


def receipt_checks(x, circuit_id):
    """From a private LEZ record's public bytes: is the receipt's claim ReceiptClaim::ok(circuit id, journal), with
    the journal the frame LEZ builds from the transaction's message (a prefix of the message's bytes); does the
    seal's output carry that claim's digest."""
    f = dict(x.fields)
    if "proof.claim" not in f:
        return {}
    out = {}
    try:
        digest, journal = parse_claim(f["proof.claim"])
    except (ValueError, KeyError) as e:
        return {"claim_parsed": False, "error": str(e)}
    tx = dict(x.blobs)["tx"]
    out["claim_parsed"] = True
    out["journal_is_message_prefix"] = (journal is not None and int.from_bytes(journal[:4], "little") == len(journal) - 4
                                        and tx[1:len(journal) - 3] == journal[4:])
    out["claim_is_ok_of_journal"] = journal is not None and digest == claim_ok_digest(circuit_id, journal)
    if "proof.seal.claim_digest" in f:
        out["seal_outputs_carry_claim_digest"] = seal_claim_digest(f["proof.seal.claim_digest"]) == digest
    return out


def control_seal(tx):
    """The STARK part (seal words 32..) of the receipt that ends a private LEZ transaction's bytes, or None."""
    for p in range(len(tx) - 8):
        if tx[p:p + 4] == bytes(4) and int.from_bytes(tx[p + 4:p + 8], "little") == len(tx) - p - 8:
            rc = tx[p + 8:]
            if rc[0] != 1:
                return None
            n = int.from_bytes(rc[1:5], "little")
            return rc[5 + 128:5 + 4 * n]
    return None


def b(x):
    x = x[2:] if x.startswith("0x") else x
    return bytes.fromhex(x)


def word(n):
    return int(n).to_bytes(32, "big")


def norm(name):
    return re.sub(r"\[\d+\]", "[]", name)


# ------------------------------------------------------------------------------------------------
# Loading: each record becomes (blobs, fields, meta)
# ------------------------------------------------------------------------------------------------

DEPOSIT_SIG = keccak256(b"Deposit(address,uint64,uint64,address,uint256,bytes32)")
RELEASE_SIG = keccak256(b"Release(bytes32,address,address,uint256,address,uint256,bytes32)")


class Rec:
    def __init__(self, rid, chain, kind, variant, seq):
        self.id, self.chain, self.kind, self.variant, self.seq = rid, chain, kind, variant, seq
        self.blobs = []   # (name, bytes): everything public, in a fixed order
        self.fields = []  # (name, bytes): named parts (values that also occur inside the blobs)
        self.time = []    # values that are time or order (block id, timestamps)
        self.meta = {}    # not public: how the record was made (proving time, receipt checks)

    def corpus(self):
        return b"".join(v for _, v in self.blobs)


def load_lez(data):
    recs = []
    for r in data["records"]:
        x = Rec(r["id"], "lez", r["kind"], r["variant"], r["seq"])
        x.blobs.append(("tx", b(r["tx"])))
        blk = b(r["block"])
        x.blobs.append(("block.header", blk[:80]))  # id, previous hash, hash, timestamp
        x.blobs.append(("block.rest", blk[80:]))
        for i, e in enumerate(r["events"]):
            x.blobs.append((f"event[{i}]", b(e)))
        for d in r["diff"]:
            x.blobs.append(("diff.account", b(d["account"])))
            x.blobs.append(("diff.before", b(d["before"])))
            x.blobs.append(("diff.after", b(d["after"])))
        x.blobs.append(("receipt", b"".join(b(v) for n, v in r["fields"] if n.startswith("receipt."))))
        x.blobs.append(("commitment_root.before", b(r["commitment_root_before"])))
        x.blobs.append(("commitment_root.after", b(r["commitment_root_after"])))
        x.fields = [(n, b(v)) for n, v in r["fields"]]
        x.fields.append(("block.id", int(r["block_id"]).to_bytes(8, "little")))
        x.fields.append(("block.timestamp", int(r["time_ms"]).to_bytes(8, "little")))
        x.fields.append(("commitment_root.before", b(r["commitment_root_before"])))
        x.fields.append(("commitment_root.after", b(r["commitment_root_after"])))
        x.diff = [(b(d["account"]), b(d["before"]), b(d["after"])) for d in r["diff"]]
        x.time = [int(r["block_id"]).to_bytes(8, "little"), int(r["time_ms"]).to_bytes(8, "little")]
        x.meta = r.get("meta") or {}
        recs.append(x)
    return recs


def load_eth(lines):
    recs = []
    for line in lines:
        if not line.strip():
            continue
        r = json.loads(line)
        x = Rec(r["id"], "eth", r["kind"], r["variant"], r["seq"])
        cd = b(r["calldata"])
        frm = b(r["from"])
        x.blobs += [("from", frm), ("value", word(int(r["value"]))), ("calldata", cd)]
        x.fields += [("tx.from", frm), ("tx.value", word(int(r["value"]))), ("calldata.selector", cd[:4])]
        words = [cd[4 + 32 * i:4 + 32 * (i + 1)] for i in range((len(cd) - 4) // 32)]
        if r["kind"] == "deposit":
            names = ["denomIndex", "commitment", "index"]
        else:
            names = ["request.token", "request.denomIndex", "request.recipient", "request.relayer",
                     "request.relayFee", "request.burnRoot", "request.nullifier", "seal.offset", "seal.length"]
        seal_len = int.from_bytes(words[8], "big") if r["kind"] == "release" else 0
        if seal_len == 260:
            # selector (4 bytes of the verifier parameters' digest) || the Groth16 proof: pi_A (G1, 64 bytes),
            # pi_B (G2, 128), pi_C (G1, 64), as risc0-ethereum's RiscZeroGroth16Verifier takes it; then ABI padding.
            seal = cd[4 + 32 * 9:4 + 32 * 9 + 260]
            for i, w in enumerate(words[:9]):
                x.fields.append((f"calldata.{names[i]}", w))
            x.fields += [("calldata.seal.selector", seal[:4]), ("calldata.seal.groth16.a", seal[4:68]),
                         ("calldata.seal.groth16.b", seal[68:196]), ("calldata.seal.groth16.c", seal[196:260]),
                         ("calldata.seal.padding", cd[4 + 32 * 9 + 260:])]
        else:
            for i, w in enumerate(words):
                x.fields.append((f"calldata.{names[i]}" if i < len(names) else f"calldata.seal[{i - len(names)}]", w))
        # A transaction's receipt shows its gas used, which includes the calldata's cost (EIP-2028: 4 per zero
        # byte, 16 per other byte) on top of the execution gas the Foundry call measures: a function of the calldata.
        zeros = cd.count(0)
        x.fields.append(("receipt.calldataGas", word(4 * zeros + 16 * (len(cd) - zeros))))
        for li, lg in enumerate(r["logs"]):
            topics = [b(t) for t in lg["topics"]]
            data = b(lg["data"])
            x.blobs += [("log.emitter", b(lg["emitter"]))] + [("log.topic", t) for t in topics] + [("log.data", data)]
            dw = [data[32 * i:32 * (i + 1)] for i in range(len(data) // 32)]
            if topics and topics[0] == DEPOSIT_SIG:
                tn, dn = ["Deposit.sig", "Deposit.depositor", "Deposit.token"], ["Deposit.index", "Deposit.leafIndex",
                                                                                  "Deposit.amount", "Deposit.commitment"]
            elif topics and topics[0] == RELEASE_SIG:
                tn, dn = ["Release.sig", "Release.nullifier", "Release.token", "Release.recipient"], [
                    "Release.amount", "Release.relayer", "Release.relayFee", "Release.burnRoot"]
            else:
                tn, dn = [f"log[{li}].topic[{i}]" for i in range(len(topics))], [f"log[{li}].data[{i}]" for i in range(len(dw))]
            x.fields += [(f"event.{n}", t) for n, t in zip(tn, topics)] + [(f"event.{n}", w) for n, w in zip(dn, dw)]
        x.writes = [(b(w["account"]), b(w["slot"]), b(w["old"]), b(w["new"])) for w in r["writes"]]
        for i, (acc, slot, old, new) in enumerate(x.writes):
            x.blobs += [("write.account", acc), ("write.slot", slot), ("write.old", old), ("write.new", new)]
        x.balances = [(b(v["account"]), word(int(v["old"])), word(int(v["new"]))) for v in r["balances"]]
        for acc, old, new in x.balances:
            x.blobs += [("balance.account", acc), ("balance.old", old), ("balance.new", new)]
        x.fields += [("block.number", word(r["block"])), ("block.timestamp", word(r["time"]))]
        # Gas used is public (the receipt). A deposit's depends on which level of the tree's frontier
        # its leaf index writes, so it is named apart and checked against the other universe below.
        x.fields.append(("receipt.gasUsed" + (".deposit" if r["kind"] == "deposit" else ""), word(r["gas"])))
        x.time = [word(r["block"]), word(r["time"])]
        recs.append(x)
    return recs


# ------------------------------------------------------------------------------------------------
# Field classes
# ------------------------------------------------------------------------------------------------

NULLIFIER, FRESH, USER, TIME = "nullifier", "fresh randomness", "the user's own public address", "time / order"
DERIVED = "a function of public data"
CLASSES = [
    # LEZ: the bridge's nullifier and the account it creates; LEZ's own private-state plumbing.
    (r"Issue\.nullifier$", NULLIFIER, "N"),
    (r"^derived\.nullifier_account$", NULLIFIER, "the nullifier account, a PDA of N"),
    (r"(^|\.)private_actions\[\]\.nullifier$|^fee\.fallback\.action\.nullifier$", NULLIFIER,
     "LEZ: the private account's nullifier (main action; fee fallback)"),
    (r"\.commitment$", FRESH, "LEZ: the private account's new commitment"),
    (r"\.note\.(ciphertext|epk)$", FRESH, "LEZ: the private account's encrypted note and ephemeral key"),
    (r"\.commitment_root$|^commitment_root\.", TIME, "LEZ: the commitment-set root (state the proof read)"),
    (r"^boundary\[\]\.(out|in)\.(from|to)$|^fee\.fallback\.boundary\[\]\.(out|in)\.(from|to)$", FRESH,
     "LEZ: the sender's one-time alias (where it varies)"),
    (r"Burn\.(r|memo|mac|c_out)$", FRESH, "the burn note (tag, encrypted recipient, mac, COut)"),
    (r"^derived\.burn_record_account$", FRESH, "the burn record account, a PDA of the note's tag r"),
    (r"^witness_set$", FRESH, "the proof (dev mode: a fake receipt) or the submitter's signature"),
    (r"^proof\.seal\.stark$", FRESH,
     "LEZ's receipt: the STARK proper (trace commitments, FRI openings) of a zero-knowledge proof, drawn by the prover"),
    (r"^calldata\.seal\.groth16\.(a|b|c)$", FRESH,
     "the release's Groth16 proof (pi_A, pi_B, pi_C), randomized by the prover"),
    (r"^receipt\.calldataGas$", DERIVED, "the calldata's gas (EIP-2028), a function of the calldata"),
    (r"(Issue|Claim)\.time_ms$|^validity$|^nonces$|^block\.(id|timestamp|number)$", TIME, "time"),
    (r"^receipt\.fee_base$", TIME, "LEZ: the base fee at the block's opening (moves with the chain's history)"),
    (r"^receipt\.gasUsed\.deposit$", TIME, "gas used: depends on the leaf position (order), see the summary"),
    # Ethereum.
    (r"^calldata\.commitment$|^event\.Deposit\.commitment$", FRESH, "C, a hiding commitment"),
    (r"^tx\.from$|^event\.Deposit\.depositor$", USER, "the depositor (or the submitting relayer)"),
    (r"^calldata\.request\.recipient$|^event\.Release\.recipient$", USER, "the release recipient"),
    (r"^calldata\.request\.nullifier$|^event\.Release\.nullifier$", NULLIFIER, "NOut"),
    (r"^event\.Deposit\.leafIndex$", TIME, "the deposit's position (its order)"),
]


EXPLAINED = {}  # receipt fields shown (in main) to depend on position only


def classify(name):
    if norm(name) in EXPLAINED:
        return EXPLAINED[norm(name)]
    for pat, cls, what in CLASSES:
        if re.search(pat, norm(name)):
            return cls, what
    return None, None


# ------------------------------------------------------------------------------------------------
# The differential gate
# ------------------------------------------------------------------------------------------------

def cut(blob, values):
    for v in sorted(values, key=len, reverse=True):
        if len(v) >= 4:
            blob = blob.replace(v, b"")
    return blob


def gate(recs, ctx):
    """True if the records are identical once allowed values are cut; else the first difference."""
    canon = []
    # Only values that vary are cut: a classified field that is constant across the records stays.
    per_record = [{(norm(n), v) for n, v in x.fields} for x in recs]
    common = set.intersection(*per_record)
    for x in recs:
        allowed = set()  # block ids and timestamps are in the block header, which is not compared
        for n, v in x.fields:
            cls, _ = classify(n)
            # The block's id and timestamp and the commitment-set roots around the transaction are
            # not inside it (header and state); cutting their short encodings would also cut
            # unrelated bytes (block 5 is `05 00…`, which the 10^8 fee reserve also contains).
            if cls is not None and (norm(n), v) not in common and not n.startswith(("block.", "commitment_root.")):
                allowed.add(v)
        for n, v in x.fields:  # a declared actor is account ‖ program: cut it whole
            if norm(n) == "public_context.actors[]" and v[:32] in allowed:
                allowed.add(v)
        parts = []
        if x.chain == "lez":
            parts.append(("tx", cut(dict(x.blobs)["tx"], allowed)))
            parts.append(("receipt", cut(dict(x.blobs)["receipt"], allowed)))
            parts.append(("block.rest", cut(dict(x.blobs)["block.rest"], allowed)))
            for name, v in x.blobs:
                if name.startswith("event"):
                    parts.append((name, cut(v, allowed)))
            # Diff: per-record accounts are cut by value; an account every record changes is shared
            # state (fee accounts, relayer, token gate), whose content is cut as time / order.
            entries = []
            for acc, before, after in x.diff:
                if acc in ctx["shared"]:
                    entries.append(acc + b"|shared")
                else:
                    entries.append(cut(acc, allowed) + b"|" + cut(before, allowed) + b"|" + cut(after, allowed))
            parts.append(("diff", b"\n".join(sorted(entries))))
        else:
            for n, v in x.fields:
                if classify(n)[0] is None:
                    parts.append((norm(n), v))
            ws = []
            for acc, slot, old, new in x.writes:
                key = ctx["slot_names"].get(slot, slot.hex())
                cls = ctx["slot_class"].get(key)
                ws.append((key, b"" if cls else old, b"" if cls else new))
            parts.append(("writes", repr(ws).encode()))
            bs = [(ctx["acct_name"].get(acc, "per-record"), b"" if acc in ctx["acct_time"] else old,
                   b"" if acc in ctx["acct_time"] else new) for acc, old, new in x.balances]
            parts.append(("balances", repr(bs).encode()))
        canon.append(parts)
    first = canon[0]
    for x, parts in zip(recs, canon):
        if len(parts) != len(first):
            return False, f"{x.id}: {len(parts)} parts, {recs[0].id}: {len(first)}"
        for (n1, v1), (n2, v2) in zip(first, parts):
            if n1 != n2 or v1 != v2:
                off = next((i for i in range(min(len(v1), len(v2))) if v1[i] != v2[i]), min(len(v1), len(v2)))
                return False, (f"{x.id} vs {recs[0].id}: part {n2} differs at byte {off} of {len(v2)}/{len(v1)} "
                               f"after the cut ({v2[max(0, off - 8):off + 24].hex()} / {v1[max(0, off - 8):off + 24].hex()})")
    return True, "identical after the cut"


# ------------------------------------------------------------------------------------------------
# The adversary
# ------------------------------------------------------------------------------------------------

def derived(x, meta):
    """Identifiers the adversary computes from a left record's public data alone."""
    f = dict((norm(n), v) for n, v in x.fields)
    out = []
    if x.chain == "eth" and x.kind == "deposit":
        dep, tok, amt, c = f["event.Deposit.depositor"][12:], f["event.Deposit.token"][12:], f["event.Deposit.amount"], f["event.Deposit.commitment"]
        idx = int.from_bytes(f["event.Deposit.leafIndex"], "big")
        leaf = deposit_leaf(idx, dep, tok, amt, c)
        fid = fid_of(meta["chain"], meta["vault"], idx, leaf)
        out += [leaf, fid, sha(c), sha(leaf), sha(fid), dep, c]
    if x.chain == "lez" and x.kind == "burn":
        g = lambda k: next(v for n, v in x.fields if n.endswith(k))
        r, memo, mac, tok, amt, c_out, cid = (g("Burn.r"), g("Burn.memo"), g("Burn.mac"), g("Burn.token"),
                                              g("Burn.amount"), g("Burn.c_out"), g("Burn.config_id"))
        i = x.seq  # the burn's index in the burn tree: the order the checkpoint collects burns in
        leaf = burn_leaf(cid, i, tok, amt, c_out, r)
        bid = burn_id(cid, i, leaf)
        note_hash = sha(r + memo + mac + tok + amt + c_out)
        out += [leaf, bid, note_hash, sha(leaf), sha(bid), sha(c_out), r, c_out, memo]
    if x.chain == "lez" and any(n.endswith("Claim.witness.s") for n, _ in x.fields):
        # A plaintext claim: the adversary checks its witness the way the program does.
        g = lambda k: next(v for n, v in x.fields if n.endswith(k))
        c = sha(b"ferrata/C/v1" + g("Claim.witness.s") + g("Claim.witness.kind") + g("Claim.witness.dest"))
        idx = int.from_bytes(g("Claim.witness.leaf_index"), "little")
        leaf = deposit_leaf(idx, g("Claim.witness.depositor"), g("Claim.witness.token"), g("Claim.witness.amount"), c)
        out += [c, leaf, fid_of(meta["chain"], meta["vault"], idx, leaf)]
    return out


def items(x, ctx, keep_order_state=False):
    """The public byte strings the adversary reads, without the allowed signals: time and order
    (block ids, timestamps, a claim's time and validity window, nonces, commitment-set roots, and
    the cumulative state of accounts every operation changes). A private transaction's proof is read part by
    part (`proof.*`, the collector splits it): a real receipt's seal and a release's Groth16 seal are read as
    any other bytes; only a public transaction's witness set (the submitter's signature) is left out, and so are
    fields shown to be a function of the record's own public data (class DERIVED: the receipt's claim, checked
    to be ReceiptClaim::ok(circuit id, the journal rebuilt from the message), and the claim digest in the seal's
    outputs): they add nothing to the message the adversary reads field by field, and read as one string (a
    fake receipt's claim holds the whole message) they would make windows across the boundaries of a sorted
    set, the noise the field split avoids."""
    out = []
    if x.chain == "lez":
        # Each named field is an item of its own; the rest of the transaction (its structure) is one
        # more. Reading the transaction as one string would make windows across the boundary of two
        # adjacent entries of a sorted set (the declared actors), which exist in pseudo-random subsets
        # of the records and are noise.
        tx = dict(x.blobs)["tx"]
        for n, v in sorted(x.fields, key=lambda f: len(f[1]), reverse=True):
            if len(v) >= 4:
                tx = tx.replace(v, b"")
            if classify(n)[0] not in (TIME, DERIVED) and norm(n) != "witness_set" and not n.startswith(("block.", "commitment_root.")):
                out.append(v)
        out.append(tx)
        out += [v for name, v in x.blobs if name.startswith("event")]
        for acc, before, after in x.diff:
            out.append(acc)
            if keep_order_state or acc not in ctx["shared"]:
                out += [before, after]
    else:
        out += [v for n, v in x.fields if classify(n)[0] not in (TIME, DERIVED) and not n.startswith("block.")]
        out += [v for name, v in x.blobs if name == "log.emitter"]
        for acc, slot, old, new in x.writes:
            if keep_order_state or ctx["slot_class"].get(ctx["slot_names"].get(slot, "")) != TIME:
                out += [acc, slot, old, new]
        for acc, old, new in x.balances:
            if keep_order_state or acc not in ctx["acct_time"]:
                out += [acc, old, new]
    return out


def features(byte_items, keep=None):
    """16-byte windows with at least 12 non-zero bytes (hash-like content, not padded small numbers); with
    `keep`, only those in it."""
    out = set()
    for data in byte_items:
        for i in range(len(data) - N_WINDOW + 1):
            w = data[i:i + N_WINDOW]
            if (keep is None or w in keep) and N_WINDOW - w.count(0) >= 12:
                out.add(w)
    return out


def attack(left, right, truth_left, truth_right, meta, ctx_l, ctx_r, use_derived=True, keep_order_state=False):
    n = len(left)
    li = [items(x, ctx_l, keep_order_state) + (derived(x, meta) if use_derived else []) for x in left]
    ri = [items(y, ctx_r, keep_order_state) + (derived(y, meta) if use_derived else []) for y in right]
    # A window counts only if both sides have it, so the larger side (real receipts: 230 kB a record) keeps only
    # windows the smaller side has: the same scores, without holding every window of every receipt.
    if sum(len(v) for it in li for v in it) <= sum(len(v) for it in ri for v in it):
        lf = [features(it) for it in li]
        union = set().union(*lf)
        rf = [features(it, union) for it in ri]
    else:
        rf = [features(it) for it in ri]
        union = set().union(*rf)
        lf = [features(it, union) for it in li]
    cl = Counter(w for s in lf for w in s)
    cr = Counter(w for s in rf for w in s)
    score = [[0.0] * n for _ in right]
    for j, s in enumerate(rf):
        for w in s:
            if w in cl:
                wt = math.log(n / cl[w]) * math.log(n / cr[w]) / math.log(n)
                if wt > 0:
                    for i, t in enumerate(lf):
                        if w in t:
                            score[j][i] += wt
    p, unique_right, cand_sizes = 0.0, 0, []
    for j in range(len(right)):
        best = max(score[j])
        cand = [i for i in range(n) if score[j][i] == best]
        cand_sizes.append(len(cand))
        true_i = truth_left.index(truth_right[j])
        if true_i in cand:
            p += 1 / len(cand)
            if len(cand) == 1:
                unique_right += 1
    return {"success": p / len(right), "chance": 1 / n, "exactly_identified": unique_right,
            "candidate_set_sizes": sorted(Counter(cand_sizes).items()), "max_score": max(max(r) for r in score)}


def order_attack(left, right, truth_left, truth_right):
    """Informational: pair the k-th left with the k-th right in time order (what the delay must defeat)."""
    hits = sum(1 for k in range(len(right)) if truth_left[k] == truth_right[k])
    return hits / len(right)


# ------------------------------------------------------------------------------------------------
# The field table
# ------------------------------------------------------------------------------------------------

B6 = {
    "deposit": ["depositor", "token", "denomination", "time", "C"],
    "import-root": ["the vault's root at a block"],
    "claim": ["N", "token", "denomination", "time", "the root read", "the bridge program",
              "a one-time fee alias with a fixed reserve", "the destination if it is public"],
    "claim-plaintext": ["everything, including which deposit"],
    "burn": ["token", "denomination", "time", "the burn note"],
    "release": ["recipient", "token", "denomination", "time", "N_out", "the root", "the relayer"],
}
B6_MAP = [  # (operation kinds, normalized field pattern, the section B6 item it is, or None)
    ("claim burn", r"Issue\.nullifier$|derived\.nullifier_account|actors\[\] \(other members\)$", "N"),
    ("claim burn import-root", r"Issue\.token$|Burn\.token$", "token"),
    ("claim burn import-root", r"\.config_id$", "the bridge program"),
    ("claim burn", r"Issue\.amount$|Burn\.amount$", "denomination"),
    ("claim burn", r"native\.Credit$", "a one-time fee alias with a fixed reserve"),
    ("claim burn", r"time_ms$|^validity$|^block\.", "time"),
    ("claim", r"Issue\.leaf_count$|StateReply\.state$|message\.Read$", "the root read"),
    ("claim burn", r"\.(out|in)\.(from|to)$", "a one-time fee alias with a fixed reserve", "varies"),
    ("burn", r"Burn\.(r|memo|mac|c_out)$|derived\.burn_record_account", "the burn note"),
    ("import-root", r"ImportRoot\.", "the vault's root at a block"),
    ("deposit", r"^tx\.from$|Deposit\.depositor$|^balance", "depositor"),
    ("deposit release", r"Deposit\.token$|Release\.token$|request\.token$", "token"),
    ("deposit release", r"denomIndex$|Deposit\.amount$|Release\.amount$|^tx\.value$", "denomination"),
    ("deposit release", r"^block\.", "time"),
    ("deposit", r"calldata\.commitment$|Deposit\.commitment$", "C"),
    ("release", r"recipient$", "recipient"),
    ("release", r"nullifier$", "N_out"),
    ("release", r"burnRoot$", "the root"),
    ("release", r"request\.relayer$|Release\.relayer$|^tx\.from$", "the relayer"),
]


def b6_item(kind, name, varies=True):
    for entry in B6_MAP:
        kinds, pat, item = entry[:3]
        when = entry[3] if len(entry) > 3 else None
        if when == "varies" and not varies:
            continue
        if kind == "burn" and "actors" in pat:
            continue
        if kind in kinds.split() and re.search(pat, norm(name)):
            return item
    if kind == "burn" and re.search(r"actors\[\] \(other members\)$", name):
        return "the burn note"
    return None


DESCRIBE = [  # what a field is, for the rows the classes do not describe
    (r"^public_context\.actors\[\] \(members", "the declared public accounts every record shares (root, token, configuration, fee recipient, relayer, fee inbox; per step)"),
    (r"^public_context\.actors\[\] \(other", "the per-record declared account"),
    (r"\.(out|in)\.(from|to)( \(members in every record\))?$", "a public end of a boundary message (where constant: the root, the token's account, the fee inbox)"),
    (r"Issue\.config_id$|Burn\.config_id$|Claim\.config_id$|ImportRoot\.config_id$", "configId (the bridge program, vault, chain)"),
    (r"Issue\.relayer$|Claim\.relayer$", "the relayer the claim names (relay fee minted to it publicly)"),
    (r"Issue\.leaf_count$|Claim\.leaf_count$", "the root read (its leaf count)"),
    (r"StateReply\.state$|message\.Read$", "the root read (its state, returned to the private part)"),
    (r"native\.Credit$", "the fee reserve, moved from the alias to the fee inbox"),
    (r"^fee\.refund_to$", "where the unspent reserve goes (here the harness's one payer)"),
    (r"^validity$", "validity windows (none set here)"),
    (r"^fee$|^nonces$", "fee declaration / nonces of the public submitter (the harness's payer)"),
    (r"^calldata\.index$|^event\.Deposit\.index$", "j = depositCount[depositor] (0 for a fresh depositor)"),
    (r"relayFee$", "the registered relay fee"),
    (r"^calldata\.seal\.selector$", "the Groth16 verifier's selector (the first 4 bytes of its parameters' digest)"),
    (r"^calldata\.seal\.(offset|length)$", "the seal's ABI offset and length"),
    (r"^calldata\.seal\.padding$", "the seal's ABI padding"),
    (r"^calldata\.seal", "the proof (mock verifier here: a constant; Groth16 on a real deployment)"),
    (r"^witness_set$", "proof (dev mode) or signature"),
    (r"^witness_set\.signatures$", "the witness set's signatures: none (the sender is an alias)"),
    (r"^proof\.length$", "the proof's length: the receipt's size in bytes"),
    (r"^proof\.kind$", "the receipt's kind (borsh tag of InnerReceipt: 1 succinct, 3 fake)"),
    (r"^proof\.seal\.length$", "the seal's length in 32-bit words"),
    (r"^proof\.seal\.control_root$", "seal outputs: the recursion control root (which recursion programs are allowed)"),
    (r"^proof\.control_id$", "the recursion program that made the final receipt"),
    (r"^proof\.hashfn$", "the STARK's hash function"),
    (r"^proof\.verifier_parameters$", "the digest of the verifier parameters (RISC Zero's version)"),
    (r"^proof\.control_inclusion_proof$", "the control id's Merkle path under the control root"),
]


def describe(name):
    for pat, what in DESCRIBE:
        if re.search(pat, name):
            return what
    return ""


def field_table(recs, kind, ctx):
    """One row per field name. A field with several values per record (a set, such as the declared
    actors) is split into the members common to every record and the others. A varying field whose
    values are, record by record, values of classified fields inherits their class."""
    by = defaultdict(list)
    known = []  # per record: value -> (class, what, field) of its classified fields
    for x in recs:
        per = defaultdict(list)
        k = {}
        for n, v in x.fields:
            per[norm(n)].append(v)
            cls, what = classify(n)
            if cls is not None:
                k.setdefault(v, (cls, what, norm(n)))
        known.append(k)
        for n, vs in per.items():
            by[n].append(sorted(vs))
    rows = []
    for n, vals in by.items():
        size = max((len(v) for vs in vals for v in vs), default=0)
        if len(vals) < len(recs):
            rows.append((n, size, f"in {len(vals)} of {len(recs)}", "UNCLASSIFIED", "", b6_item(kind, n)))
            continue
        common = set(vals[0]).intersection(*map(set, vals[1:]))
        multi = max(len(vs) for vs in vals) > 1
        if multi and common:
            label = n + " (members in every record)"
            rows.append((label, size, f"constant ({len(common)})", "constant", describe(label),
                         b6_item(kind, label, varies=False)))
        rest = [[v for v in vs if v not in common] for vs in vals]
        if not any(rest):
            if not (multi and common):
                rows.append((n, size, "constant", "constant", describe(n) or classify(n)[1] or "",
                             b6_item(kind, n, varies=False)))
            continue
        distinct = len({tuple(r) for r in rest})
        cls, what = classify(n)
        if cls is None:
            inherited = [[known[i].get(v) or known[i].get(v[:32]) for v in r] for i, r in enumerate(rest)]
            flat = [t for r in inherited for t in r]
            if flat and all(t is not None for t in flat) and len({(t[0], t[2]) for t in flat}) == 1:
                cls, what = flat[0][0], f"= `{flat[0][2]}`: {flat[0][1]}"
        label = n + (" (other members)" if multi and common else "")
        rows.append((label, size, f"{distinct} distinct", cls or "UNCLASSIFIED", what or describe(label),
                     b6_item(kind, label)))
    return rows + state_rows(recs, kind, known, ctx)


def state_rows(recs, kind, known, ctx):
    """What each step changes in public state: LEZ accounts (before/after), the vault's storage
    slots, and Ethereum balances."""
    rows, n = [], len(recs)
    if recs[0].chain == "lez":
        shared = set.intersection(*[{a for a, _, _ in x.diff} for x in recs])
        seen = defaultdict(list)
        for i, x in enumerate(recs):
            for acc, before, after in x.diff:
                if acc in shared:
                    seen[("shared", ctx["labels"].get(acc.hex(), "unlabelled account"))].append((before, after))
                else:
                    cls = known[i].get(acc)
                    name = ctx["labels"].get(acc.hex()) or (f"the account in `{cls[2]}`" if cls else "unlabelled account")
                    seen[("own", name, cls[0] if cls else None)].append(acc)
        for key, vals in seen.items():
            if key[0] == "shared":
                varies = len(set(vals)) > 1
                rows.append((f"state: {key[1]}", 0, "changed in every record" + (", contents vary" if varies else ""),
                             TIME if varies else "constant", "cumulative state (order)" if varies else "", None))
            else:
                rows.append((f"state: {key[1]}", 0, f"one per record ({len(set(vals))} distinct)",
                             key[2] or "UNCLASSIFIED", "created or changed by this record only",
                             "N" if key[2] == NULLIFIER and kind == "claim" else ("the burn note" if kind == "burn" else None)))
    else:
        by = defaultdict(list)
        for x in recs:
            for acc, slot, old, new in x.writes:
                by[("storage", ctx["slot_names"].get(slot, "slot " + slot.hex()[-8:]))].append((old, new))
            for acc, old, new in x.balances:
                by[("balance", ctx["acct_name"].get(acc, "per record"))].append((acc, old, new))
        for (what, name), vals in by.items():
            if what == "storage":
                cls = ctx["slot_class"].get(name)
                status = f"written in {len(vals)} of {n} records" + (", values vary" if len(set(vals)) > 1 else "")
                rows.append((f"storage: {name}", 32, status, cls or ("constant" if len(set(vals)) == 1 else "UNCLASSIFIED"),
                             {TIME: "tree state, counters, escrow (order)", USER: "per-depositor counter",
                              NULLIFIER: "nullifierUsed[NOut]"}.get(cls, ""), None))
            else:
                cls = TIME if name in ("vault", "relayer") else USER
                rows.append((f"balance: {name}", 32, f"changed in {len(vals)} of {n} records", cls,
                             "cumulative (order)" if cls == TIME else "the recipient's own address", None))
    return rows


# ------------------------------------------------------------------------------------------------
# Recovery from the seed alone (an independent wallet)
# ------------------------------------------------------------------------------------------------

def recover(user, meta, deposits, spent_n, notes, released, gap):
    seed = b(user["bridge_seed"])
    k_auth, k_view = prf(seed, b"ferrata/auth/v1"), prf(seed, b"ferrata/view/v1")
    chain, vault, cid = meta["chain"], meta["vault"], meta["cid"]
    accounts = [(1, b(a)) for a in user["accounts"]["private"]] + [(2, b(a)) for a in user["accounts"]["public"]]
    owned_deposits = []
    for d in deposits:  # one pass over the vault's whole deposit list
        s = prf(k_auth, b"ferrata/s/v1" + chain + vault + d["depositor"] + u64(d["j"]))
        for kind, dest in accounts:
            if sha(b"ferrata/C/v1" + s + bytes([kind]) + dest) == d["c"]:
                leaf = deposit_leaf(d["index"], d["depositor"], d["token"], d["amount"], d["c"])
                n = sha(b"ferrata/nf/v1" + s + fid_of(chain, vault, d["index"], leaf))
                owned_deposits.append({"leaf_index": d["index"], "N": n, "claimed": n in spent_n})
    owned_burns = []
    by_tag = defaultdict(list)
    for note in notes:  # the whole list of burn notes, in burn-tree order
        by_tag[note["r"]].append(note)
    i, last = 0, -1
    while i - last <= gap:
        r = prf(k_view, b"ferrata/burn-tag/v1" + cid + u64(i))
        for note in by_tag.get(r, []):
            if prf(k_view, b"ferrata/burn-mac/v2" + r + note["memo"] + note["token"] + note["amount"] + note["c_out"])[:16] != note["mac"]:
                continue
            recipient = bytes(a ^ p for a, p in zip(note["memo"], prf(k_view, b"ferrata/memo/v1" + r)[:20]))
            s_out = prf(k_auth, b"ferrata/s-out/v1" + cid + r)
            if sha(b"ferrata/Cout/v1" + s_out + chain + vault + recipient) != note["c_out"]:
                continue
            n_out = sha(b"ferrata/nf-out/v2" + s_out)  # D13 (review M-3): NOut no longer depends on the burn's index
            owned_burns.append({"burn_index": note["index"], "recipient": recipient.hex(), "NOut": n_out,
                                "released": n_out in released})
            last = i
        i += 1
    return owned_deposits, owned_burns


# ------------------------------------------------------------------------------------------------

def spread(vals):
    vs = sorted(vals)
    return {"min": vs[0], "median": vs[len(vs) // 2] if len(vs) % 2 else (vs[len(vs) // 2 - 1] + vs[len(vs) // 2]) / 2,
            "max": vs[-1], "sum": sum(vs), "n": len(vs)} if vs else None


def proof_summary(recs):
    """What the collector recorded about each record's proof (not public) and how each proof field varies."""
    ms = [x.meta for x in recs]
    out = {"records": len(recs), "kinds": sorted({m["kind"] for m in ms if m.get("kind")})}
    for key in ("prove_s", "proof_bytes", "tx_bytes", "seal_words", "journal_bytes"):
        out[key] = spread([m[key] for m in ms if key in m])
    for key in ("claim_is_ok_of_journal", "seal_outputs_carry_claim_digest", "claim_is_pruned"):
        vals = [m[key] for m in ms if key in m]
        out[key] = [sum(1 for v in vals if v is True), len(vals)]
    f = defaultdict(list)
    for x in recs:
        for n, v in x.fields:
            if n.startswith(("proof.", "witness_set", "calldata.seal", "receipt.calldataGas")):
                f[n].append(v)
    out["fields"] = {n: {"distinct": len(set(vs)), "bytes": sorted({len(v) for v in vs}), "class": classify(n)[0]}
                     for n, vs in sorted(f.items())}
    return out


def shared_windows(blobs, controls=()):
    """Shared runs between different records' blobs ((record id, user, bytes)): every 16-byte window with at least
    12 non-zero bytes at offsets that are multiples of 4 is indexed, every window at any offset is looked up, so
    any common run of 19 bytes or more between two records is found (at any alignment). Returns how many indexed
    windows are shared by how many records, and the pairs of records that share one, with whether they are the
    same user's."""
    where = defaultdict(set)
    for rid, _, data in blobs:
        for i in range(0, len(data) - N_WINDOW + 1, 4):
            w = data[i:i + N_WINDOW]
            if N_WINDOW - w.count(0) >= 12:
                where[w].add(rid)
    user = {rid: u for rid, u, _ in blobs}
    hits = defaultdict(set)
    for rid, _, data in blobs:
        for i in range(len(data) - N_WINDOW + 1):
            owners = where.get(data[i:i + N_WINDOW])
            if owners:
                for o in owners:
                    if o != rid:
                        hits[tuple(sorted((rid, o)))].add(data[i:i + N_WINDOW])
    by = Counter(len(v) for v in where.values() if len(v) > 1)
    pairs = [{"records": list(k), "same_user": user[k[0]] == user[k[1]], "windows": len(v)} for k, v in sorted(hits.items())]
    kind = lambda rid: rid.split(":")[1]
    cats = defaultdict(list)
    for p in pairs:
        a, c = p["records"]
        cross = kind(a) != kind(c)
        cats[f"{'-'.join(sorted((kind(a), kind(c))))}{', same user' if cross and p['same_user'] else ''}"].append(p["windows"])
    out = {"indexed_windows": len(where), "shared_by_n_records": sorted(by.items()), "pairs_total": len(pairs),
           "record_pairs_possible": len(blobs) * (len(blobs) - 1) // 2,
           "same_user_pairs": sum(1 for p in pairs if p["same_user"]),
           "windows_per_pair": {k: spread(v) for k, v in sorted(cats.items())}}
    # Unrelated receipts of the same recursion circuit (other statements, other programs, another day): how many
    # windows a record's seal shares with each, against how many it shares with another record of this run.
    for name, seal in controls:
        cw = {seal[i:i + N_WINDOW] for i in range(len(seal) - N_WINDOW + 1)}
        per = []
        for rid, _, data in blobs:
            per.append(sum(1 for i in range(0, len(data) - N_WINDOW + 1, 4)
                           if data[i:i + N_WINDOW] in cw and N_WINDOW - data[i:i + N_WINDOW].count(0) >= 12))
        out.setdefault("unrelated_receipts", {})[name] = spread(per)
    return out


# ------------------------------------------------------------------------------------------------
# Section B6's "Hidden" column: the values each step must hide, and the search for them
# ------------------------------------------------------------------------------------------------

# A value is searched for in a record's public bytes as it is, byte-reversed (a little-endian reading
# of a big-endian value) and as hex text (lower and upper case); whole, and as every 16-byte window of
# it (a truncated or shifted copy) that has at least 12 non-zero bytes, the adversary's rule for
# hash-like content. A value without such a window (a small number: a leaf index, `j`) cannot be told
# from unrelated bytes, and only the differential gate can see it (docs/privacy/CONTROLS.md).
LEAK_FORMS = ("raw", "reversed", "hex", "HEX")


def encode(v, form):
    return {"raw": v, "reversed": v[::-1], "hex": v.hex().encode(), "HEX": v.hex().upper().encode()}[form]


def leak_pieces(v, windows=True, window=N_WINDOW):
    """The whole value and its 16-byte windows, those with at least 12 non-zero bytes."""
    pieces = [v] + ([v[i:i + window] for i in range(len(v) - window + 1)] if windows and len(v) > window else [])
    return [p for p in pieces if len(p) >= window and len(p) - p.count(0) >= 12]


def find_leak(data, v, forms=LEAK_FORMS, windows=True):
    """The form in which `v`, or one of its 16-byte windows, occurs in `data`; None if it does not."""
    for piece in leak_pieces(v, windows):
        for form in forms:
            if encode(piece, form) in data:
                return form
    return None


def everything(x):
    """A record's whole public data: every blob and every named field."""
    return b"".join(v for _, v in x.blobs) + b"".join(v for _, v in x.fields)


def hidden_steps(G, T, users, meta):
    """Per step of section B6: (step, group, ground-truth key, the values the record of a user must hide)."""
    seeds = [b(u["bridge_seed"]) for u in users]
    k_auth = [prf(sd, b"ferrata/auth/v1") for sd in seeds]
    k_view = [prf(sd, b"ferrata/view/v1") for sd in seeds]
    dest = [b(u["accounts"]["private"][0]) for u in users]
    s_of = [prf(k_auth[i], b"ferrata/s/v1" + meta["chain"] + meta["vault"] + b(u["depositor"]) + u64(0))
            for i, u in enumerate(users)]
    dep_by_user = {T["deposit"][x.seq]: dict((norm(n), v) for n, v in x.fields) for x in G["deposit (A: private destinations)"]}
    burn_by_user = {T["burn"][x.seq]: x for x in G["burn, private holding"]}

    def keys(u):  # the seed and the two keys it derives (design note section 4): hidden everywhere
        return [("bridge seed", seeds[u]), ("kAuth", k_auth[u]), ("kView", k_view[u])]

    def deposit_secrets(u):
        return [("destination", dest[u]), ("s", s_of[u])] + keys(u)

    def claim_secrets(u):
        d = dep_by_user[u]
        leaf = deposit_leaf(u, d["event.Deposit.depositor"][12:], d["event.Deposit.token"][12:], d["event.Deposit.amount"], d["event.Deposit.commitment"])
        return [("destination", dest[u]), ("s", s_of[u]), ("C", d["event.Deposit.commitment"]), ("leaf", leaf),
                ("fid", fid_of(meta["chain"], meta["vault"], u, leaf)), ("depositor", d["event.Deposit.depositor"][12:])] + keys(u)

    def burn_note(u):
        x = burn_by_user[u]
        g = lambda k: next(v for n, v in x.fields if n.endswith(k))
        s_out = prf(k_auth[u], b"ferrata/s-out/v1" + meta["cid"] + g("Burn.r"))
        return x, g, s_out, prf(k_view[u], b"ferrata/memo/v1" + g("Burn.r"))[:20]

    def burn_secrets(u):
        _, _, s_out, pad = burn_note(u)
        return [("holder (private account)", dest[u]), ("recipient", b(users[u]["recipient"])), ("sOut", s_out),
                ("NOut", sha(b"ferrata/nf-out/v2" + s_out)), ("memo pad", pad)] + keys(u)

    def release_secrets(u):
        x, g, s_out, pad = burn_note(u)
        leaf = burn_leaf(meta["cid"], x.seq, g("Burn.token"), g("Burn.amount"), g("Burn.c_out"), g("Burn.r"))
        return [("r", g("Burn.r")), ("COut", g("Burn.c_out")), ("memo", g("Burn.memo")), ("mac", g("Burn.mac")),
                ("burn leaf", leaf), ("burnId", burn_id(meta["cid"], x.seq, leaf)), ("holder (private account)", dest[u]),
                ("sOut", s_out), ("memo pad", pad)] + keys(u)

    return [("deposit hides the LEZ destination", "deposit (A: private destinations)", "deposit", deposit_secrets),
            ("claim hides which deposit and a private destination", "claim, private (default)", "claim", claim_secrets),
            ("burn hides the holder and the recipient", "burn, private holding", "burn", burn_secrets),
            ("release hides which burn", "release, NOut of the release statement", "release", release_secrets)]


def hidden_check(steps, G, T, find=find_leak, data_of=everything):
    """Searches every record of each step for the values its user must hide; counts what was found."""
    hidden = {}
    for step, gk, tk, values_of in steps:
        found, checked = 0, 0
        for x in G[gk]:
            data = data_of(x)
            for label, v in values_of(T[tk][x.seq]):
                checked += 1
                form = find(data, v)
                if form:
                    found += 1
                    hidden.setdefault(step + ": FOUND", []).append(f"{x.id}: {label} ({form})")
        hidden[step] = {"values searched": checked, "found": found}
    return hidden


def main():
    data_dir, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)
    lez_json = json.loads((data_dir / "lez.json").read_text())
    truth = json.loads((data_dir / "truth.json").read_text())
    scenario = json.loads((data_dir / "scenario.json").read_text())
    lez = load_lez(lez_json)
    eth = load_eth((data_dir / "eth.jsonl").read_text().splitlines())
    meta = {"chain": word(scenario["chain_id"]), "vault": b(scenario["vault"]), "cid": b(scenario["config_id"])}
    users = truth["users"]
    n_users = len(users)

    def group(chain, kind, variant):
        rs = [x for x in (lez if chain == "lez" else eth) if x.kind == kind and x.variant == variant]
        return sorted(rs, key=lambda x: x.seq)

    G = {
        "deposit (A: private destinations)": group("eth", "deposit", "private"),
        "deposit (B: public destinations)": group("eth", "deposit", "plaintext"),
        "claim, private (default)": group("lez", "claim", "private"),
        "claim, naive nullifier N = fid": group("lez", "claim", "naive"),
        "claim, plaintext": group("lez", "claim", "plaintext"),
        "burn, private holding": group("lez", "burn", "private"),
        "release, NOut of the release statement": group("eth", "release", "private"),
        "release, naive NOut = burnId": group("eth", "release", "naive"),
    }
    for k, v in G.items():
        assert len(v) == n_users, (k, len(v))

    # The proof's claim and the claim digest in a real seal's public outputs are functions of the transaction's
    # message, if (and only if) the collector's checks held for every private transaction: then they may vary.
    # Checked here from the public bytes alone (receipt_checks); the collector's own claim check must agree.
    private_txs = [x for x in lez if x.variant == "private" and x.kind in ("claim", "burn")]
    circuit_id = b(lez_json.get("circuit_image_id") or "")
    public_checks = {x.id: receipt_checks(x, circuit_id) for x in private_txs}
    if private_txs and all(c.get("journal_is_message_prefix") and c.get("claim_is_ok_of_journal") and
                           x.meta.get("claim_is_ok_of_journal") is not False
                           for x, c in ((x, public_checks[x.id]) for x in private_txs)):
        EXPLAINED["proof.claim"] = (DERIVED, "the receipt's claim: ReceiptClaim::ok(circuit id, journal), the journal "
                                             "the frame of the transaction's message (checked from the public bytes)")
    succinct = [x for x in private_txs if "proof.seal.claim_digest" in dict(x.fields)]
    if succinct and all(public_checks[x.id].get("seal_outputs_carry_claim_digest") for x in succinct):
        EXPLAINED["proof.seal.claim_digest"] = (DERIVED, "seal outputs: the claim's digest in 16-bit halves (Montgomery "
                                                         "form), that of `proof.claim` (checked from the public bytes)")

    # Ground truth: the user behind each record.
    co, po, bo, ro = truth["claim_order"], truth["plaintext_order"], truth["burn_order"], truth["release_order"]
    T = {
        "deposit": list(range(n_users)),
        "claim": co, "plaintext": po, "burn": bo,
        "release": [bo[i] for i in ro],
    }

    # Context the gate needs: shared LEZ accounts, the vault's slots and accounts by name.
    shared_by_group = {}
    for k, recs in G.items():
        if recs[0].chain == "lez":
            sets = [{a for a, _, _ in x.diff} for x in recs]
            shared_by_group[k] = set.intersection(*sets)
    vault = meta["vault"]
    slot_names, slot_class = {}, {}
    for i in range(0, 35):
        slot_names[word(i)] = {0: "depositRoot", 1: "nextLeafIndex", 2: "storedLezConfigId"}.get(i, "_branch[level]")
    for k in ("depositRoot", "nextLeafIndex", "_branch[level]"):
        slot_class[k] = TIME
    keys = defaultdict(set)
    for recs in G.values():
        for x in recs:
            if x.chain != "eth":
                continue
            f = dict((norm(n), v) for n, v in x.fields)
            for n in ("tx.from", "calldata.request.recipient", "calldata.request.nullifier", "calldata.request.token"):
                if n in f:
                    keys[n].add(f[n] if len(f[n]) == 32 else bytes(12) + f[n])
    keys["ETH"].add(bytes(32))
    for name, ks in keys.items():
        for kv in ks:
            for p in range(30, 80):
                s = keccak256(kv + word(p))
                label = {"tx.from": "per-sender", "calldata.request.recipient": "per-recipient",
                         "calldata.request.nullifier": "per-nullifier", "calldata.request.token": "per-token",
                         "ETH": "per-token"}[name]
                slot_names[s] = f"mapping@{p}[{label}]"
    for key in list(set(slot_names.values())):
        if key.startswith("mapping") and ("per-sender" in key or "per-recipient" in key):
            slot_class[key] = USER
        elif key.startswith("mapping") and "per-nullifier" in key:
            slot_class[key] = NULLIFIER
        elif key.startswith("mapping") and "per-token" in key:
            slot_class[key] = TIME  # escrow totals and buckets: cumulative
    relayer = b(scenario["relayer"])
    acct_name = {vault: "vault", relayer: "relayer"}
    acct_time = {vault, relayer}

    results = {"gate": {}, "adversary": {}, "order_only": {}, "recovery": {}, "fields": {}, "sizes": {}}
    # Proofs: the receipts on LEZ and the release seals on Ethereum, what was checked and how they vary.
    results["proofs"] = {
        "mode": scenario.get("proofs", lez_json.get("proofs", "dev")),
        "circuit_image_id": lez_json.get("circuit_image_id"),
        "steps": {k: proof_summary(G[k]) for k in ("claim, private (default)", "burn, private holding",
                                                    "release, NOut of the release statement",
                                                    "release, naive NOut = burnId")},
    }
    for k in ("claim, private (default)", "burn, private holding"):
        cs = [public_checks[x.id] for x in G[k]]
        results["proofs"]["steps"][k]["public_checks"] = {
            key: [sum(1 for c in cs if c.get(key) is True), sum(1 for c in cs if key in c)]
            for key in ("claim_parsed", "journal_is_message_prefix", "claim_is_ok_of_journal",
                        "seal_outputs_carry_claim_digest")}
    user_of = {}
    for k, kind in (("claim, private (default)", "claim"), ("burn, private holding", "burn"),
                    ("release, NOut of the release statement", "release")):
        for x in G[k]:
            user_of[x.id] = {"claim": truth["claim_order"], "burn": truth["burn_order"],
                             "release": [truth["burn_order"][i] for i in truth["release_order"]]}[kind][x.seq]
    def part(x, name):
        return b"".join(v for n, v in x.fields if n == name)
    seal_blobs = [(x.id, user_of[x.id], part(x, "proof.seal.stark")) for k in ("claim, private (default)", "burn, private holding")
                  for x in G[k] if part(x, "proof.seal.stark")]
    groth_blobs = [(x.id, user_of[x.id], part(x, "calldata.seal.groth16.a") + part(x, "calldata.seal.groth16.b") +
                    part(x, "calldata.seal.groth16.c")) for x in G["release, NOut of the release statement"]
                   if part(x, "calldata.seal.groth16.a")]
    controls = []
    for path in [p for p in os.environ.get("FERRATA_PRIVACY_CONTROL_RECEIPTS", "").split(",") if p]:
        seal = control_seal(Path(path).read_bytes())
        if seal:
            controls.append((Path(path).name, seal))
    if seal_blobs:
        results["proofs"]["shared_windows_lez_seals"] = shared_windows(seal_blobs, controls)
    if groth_blobs:
        results["proofs"]["shared_windows_groth16"] = shared_windows(groth_blobs)
        results["proofs"]["shared_windows_groth16_vs_lez_seals"] = shared_windows(groth_blobs + seal_blobs)
    # Receipts: gas varies; it is order (time) only if it follows the position, not the user.
    def by_seq(k, name, endian):
        return [int.from_bytes(next(v for n, v in x.fields if n == name), endian) for x in G[k]]
    steps = lambda v: [a - v[0] for a in v]
    pc = by_seq("claim, private (default)", "receipt.gas_exec", "little")
    pp = by_seq("claim, plaintext", "receipt.gas_exec", "little")
    bu = by_seq("burn, private holding", "receipt.gas_exec", "little")
    rg = by_seq("release, NOut of the release statement", "receipt.gasUsed", "big")
    checks = {
        "LEZ claims: the same gas steps by position in both universes (users in different orders)":
            steps(pc) == steps(pp) and T["claim"] != T["plaintext"],
        "LEZ burns: only the first differs": len(set(bu[1:])) == 1,
        "Ethereum releases: only the first differs, by 25,000 (the relayer's account created)":
            len(set(rg[1:])) == 1 and rg[0] - rg[1] == 25000,
    }
    results["receipts_by_position"] = {"claims private": steps(pc), "claims plaintext": steps(pp), "burns": steps(bu),
                                       "releases": steps(rg), "checks": checks}
    if all(list(checks.values())[:2]):
        for n in ("receipt.gas_exec", "receipt.cycles"):
            EXPLAINED[n] = (TIME, "LEZ execution gas: follows the position (the shared state the public part updates)")
    if list(checks.values())[2]:
        EXPLAINED["receipt.gasUsed"] = (TIME, "gas used: the first release's relay fee creates the relayer's account")
    for k, recs in G.items():
        sz = [len(dict(x.blobs)["tx"]) if x.chain == "lez" else len(dict(x.blobs)["calldata"]) for x in recs]
        results["sizes"][k] = [min(sz), max(sz)]
    for k, recs in G.items():
        ok, why = gate(recs, {"shared": shared_by_group.get(k, set()), "slot_names": slot_names,
                              "slot_class": slot_class, "acct_name": acct_name, "acct_time": acct_time})
        results["gate"][k] = {"pass": ok, "detail": why}

    # The adversary: left = what the pair starts with, right = what it ends with.
    games = [
        ("inbound, private (default)", "deposit (A: private destinations)", "claim, private (default)", "deposit", "claim"),
        ("inbound, naive nullifier N = fid", "deposit (A: private destinations)", "claim, naive nullifier N = fid", "deposit", "claim"),
        ("inbound, plaintext claim", "deposit (B: public destinations)", "claim, plaintext", "deposit", "plaintext"),
        ("outbound, private (default)", "burn, private holding", "release, NOut of the release statement", "burn", "release"),
        ("outbound, naive NOut = burnId", "burn, private holding", "release, naive NOut = burnId", "burn", "release"),
        ("round trip: deposit to release (informational)", "deposit (A: private destinations)", "release, NOut of the release statement", "deposit", "release"),
        ("on LEZ: claim to burn (informational)", "claim, private (default)", "burn, private holding", "claim", "burn"),
    ]
    def ctx_of(k):
        return {"shared": shared_by_group.get(k, set()), "slot_names": slot_names, "slot_class": slot_class,
                "acct_name": acct_name, "acct_time": acct_time}

    for name, lk, rk, lt, rt in games:
        cl_, cr_ = ctx_of(lk), ctx_of(rk)
        res = attack(G[lk], G[rk], T[lt], T[rt], meta, cl_, cr_)
        res["without_derived_identifiers"] = attack(G[lk], G[rk], T[lt], T[rt], meta, cl_, cr_, use_derived=False)["success"]
        res["with_order_state"] = attack(G[lk], G[rk], T[lt], T[rt], meta, cl_, cr_, keep_order_state=True)["success"]
        results["adversary"][name] = res
        results["order_only"][name] = order_attack(G[lk], G[rk], T[lt], T[rt])

    # Field tables.
    kinds = {"deposit (A: private destinations)": "deposit", "claim, private (default)": "claim",
             "claim, plaintext": "claim-plaintext", "burn, private holding": "burn",
             "release, NOut of the release statement": "release"}
    imports = [x for x in lez if x.kind == "import-root" and x.variant == "private"]
    tctx = {"labels": lez_json.get("accounts", {}), "slot_names": slot_names, "slot_class": slot_class, "acct_name": acct_name}
    tables = {k: field_table(G[k], kinds[k], tctx) for k in kinds}
    tables["root import"] = field_table(imports, "import-root", tctx)
    for k, rows in tables.items():
        results["fields"][k] = [dict(zip(("field", "bytes", "across records", "class", "what", "B6"), r)) for r in rows]

    # Recovery at the half-way point: 10 claims and 10 releases done.
    half = truth["recovery_snapshot"]
    deps = []
    for x in G["deposit (A: private destinations)"]:
        f = dict((norm(n), v) for n, v in x.fields)
        deps.append({"depositor": f["event.Deposit.depositor"][12:], "j": int.from_bytes(f["event.Deposit.index"], "big"),
                     "index": int.from_bytes(f["event.Deposit.leafIndex"], "big"), "token": f["event.Deposit.token"][12:],
                     "amount": f["event.Deposit.amount"], "c": f["event.Deposit.commitment"]})
    spent = {next(v for n, v in x.fields if n.endswith("Issue.nullifier")) for x in G["claim, private (default)"] if x.seq < half}
    notes = []
    for x in G["burn, private holding"]:
        g = lambda s: next(v for n, v in x.fields if n.endswith(s))
        notes.append({"index": x.seq, "r": g("Burn.r"), "memo": g("Burn.memo"), "mac": g("Burn.mac"),
                      "token": g("Burn.token"), "amount": g("Burn.amount"), "c_out": g("Burn.c_out")})
    rel = {dict(x.fields)["calldata.request.nullifier"] for x in G["release, NOut of the release statement"] if x.seq < half}
    all_released = {dict(x.fields)["calldata.request.nullifier"] for x in G["release, NOut of the release statement"]}
    claimed_users = set(co[:half])
    released_users = {bo[i] for i in ro[:half]}
    rec = {"unclaimed_found": 0, "unclaimed_expected": n_users - len(claimed_users), "claimed_seen_as_claimed": 0,
           "unreleased_found": 0, "unreleased_expected": n_users - len(released_users), "released_seen_as_released": 0,
           "foreign_items": 0, "nout_matches_vault": 0, "per_user": []}
    for u, user in enumerate(users):
        od, ob = recover(user, meta, deps, spent, notes, rel, truth["gap"])
        own_dep = [d for d in od if d["leaf_index"] == u]
        own_burn = [x for x in ob if bo[x["burn_index"]] == u]
        rec["foreign_items"] += (len(od) - len(own_dep)) + (len(ob) - len(own_burn))
        rec["unclaimed_found"] += sum(1 for d in own_dep if not d["claimed"] and u not in claimed_users)
        rec["claimed_seen_as_claimed"] += sum(1 for d in own_dep if d["claimed"] and u in claimed_users)
        rec["unreleased_found"] += sum(1 for x in own_burn if not x["released"] and u not in released_users)
        rec["released_seen_as_released"] += sum(1 for x in own_burn if x["released"] and u in released_users)
        rec["nout_matches_vault"] += sum(1 for x in own_burn if x["NOut"] in all_released)
        rec["per_user"].append({"user": u, "deposits_found": len(od), "burns_found": len(ob),
                                "recipient_ok": all(x["recipient"] == user["recipient"] for x in ob)})
    rec["recipients_decrypted_correctly"] = sum(1 for p in rec["per_user"] if p["recipient_ok"] and p["burns_found"] == 1)
    rec["rust_core"] = truth["rust_recovery"]["core_recover_burns_found"]
    results["recovery"] = rec

    # Section B6's "Hidden" column, checked: each record's whole public data searched for the
    # values that step must hide (the ground truth names them; the adversary never sees it).
    results["hidden"] = hidden_check(hidden_steps(G, T, users, meta), G, T)
    # Which commitment-set root each private transaction was proven against: the state's root just
    # before it (the newest), or an older one.
    newest, total = 0, 0
    for k in ("claim, private (default)", "burn, private holding"):
        for x in G[k]:
            before = dict(x.fields)["commitment_root.before"]
            for n, v in x.fields:
                if norm(n) in ("private_actions[].commitment_root", "fee.fallback.action.commitment_root"):
                    total += 1
                    newest += v == before
    results["proof_roots_newest"] = [newest, total]
    gas = lambda k: [int.from_bytes(next(v for n, v in x.fields if n.startswith("receipt.gasUsed")), "big") for x in G[k]]
    ga, gb = gas("deposit (A: private destinations)"), gas("deposit (B: public destinations)")
    results["gas"] = {"deposit A by leaf index": ga, "deposit B by leaf index": gb, "deposit A equals B": ga == gb,
                      "release distinct values": sorted(set(gas("release, NOut of the release statement"))),
                      "release naive distinct values": sorted(set(gas("release, naive NOut = burnId")))}
    lez_receipts = {}
    for k in ("claim, private (default)", "burn, private holding", "claim, plaintext"):
        vals = defaultdict(set)
        for x in G[k]:
            for n, v in x.fields:
                if n.startswith("receipt."):
                    vals[n].add(int.from_bytes(v, "little"))
        lez_receipts[k] = {n: sorted(v) for n, v in vals.items()}
    results["lez_receipts"] = lez_receipts

    (out_dir / "results.json").write_text(json.dumps(results, indent=1, default=lambda v: v.hex() if isinstance(v, bytes) else str(v)))
    write_markdown(out_dir, results, lez_json, scenario, n_users, half)
    print(json.dumps({"gate": {k: v["pass"] for k, v in results["gate"].items()},
                      "adversary": {k: round(v["success"], 4) for k, v in results["adversary"].items()},
                      "recovery": {k: v for k, v in rec.items() if k != "per_user"}}, indent=1))


def proofs_markdown(pr, n):
    """The proofs section of summary.md."""
    if not pr:
        return []
    out = ["", "## Proofs", "",
           "What the collectors recorded about each proof (not public: proving time, the checks) and how each "
           "part of the proof varies across the records of a step (public).", "",
           "| step | receipt kind | proof bytes | transaction bytes | seal words | proving time, s (min / median / max; sum) | journal = the message's frame | claim = ok(circuit, journal) | seal outputs = claim digest |",
           "|---|---|---|---|---|---|---|---|---|"]
    rng = lambda d, f="{:,}": "-" if not d else (f.format(d["min"]) if d["min"] == d["max"] else f"{f.format(d['min'])}-{f.format(d['max'])}")
    for k, v in pr["steps"].items():
        t = v.get("prove_s")
        times = "-" if not t else f"{t['min']:.1f} / {t['median']:.1f} / {t['max']:.1f}; {t['sum']:.0f}"
        pc = v.get("public_checks", {})
        ok = lambda key: "-" if not pc.get(key, [0, 0])[1] else f"{pc[key][0]} of {pc[key][1]}"
        out.append(f"| {k} | {', '.join(v['kinds']) or '-'} | {rng(v['proof_bytes'])} | {rng(v['tx_bytes'])} | "
                   f"{rng(v['seal_words'])} | {times} | {ok('journal_is_message_prefix')} | {ok('claim_is_ok_of_journal')} | "
                   f"{ok('seal_outputs_carry_claim_digest')} |")
    out += ["", "The three check columns are computed by this script from the public bytes alone (the claim parsed and "
            "hashed by RISC Zero's rules, the seal's output words decoded from Montgomery form)."]
    out += ["", "| step | proof field | bytes | distinct across the records | class |", "|---|---|---|---:|---|"]
    for k, v in pr["steps"].items():
        for name, f in v["fields"].items():
            out.append(f"| {k} | `{name}` | {', '.join(f'{b:,}' for b in f['bytes'])} | {f['distinct']} of {v['records']} | "
                       f"{f['class'] or ('constant' if f['distinct'] == 1 else 'UNCLASSIFIED')} |")
    out += [""]
    for key, label in (("shared_windows_lez_seals", "LEZ receipts' STARK parts (claims and burns)"),
                       ("shared_windows_groth16", "the releases' Groth16 proofs"),
                       ("shared_windows_groth16_vs_lez_seals", "Groth16 proofs and LEZ STARK parts together")):
        w = pr.get(key)
        if not w:
            continue
        hist = w["shared_by_n_records"]
        out.append(f"- Shared runs, {label}: {w['indexed_windows']:,} windows indexed; "
                   + (f"{sum(c for _, c in hist):,} recur in 2 to {max(k for k, _ in hist)} records; record pairs sharing "
                      f"a run {w['pairs_total']} of {w['record_pairs_possible']}" if hist else "none recurs in a second record"))
        for cat, d in (w.get("windows_per_pair") or {}).items():
            out.append(f"  - windows shared per pair, {cat}: {d['min']}-{d['max']}, median {d['median']} ({d['n']} pairs)")
        for name, d in (w.get("unrelated_receipts") or {}).items():
            out.append(f"  - shared with an unrelated receipt (`{name}`): {d['min']}-{d['max']} windows per record, "
                       f"median {d['median']}")
    return out


def write_markdown(out_dir, results, lez_json, scenario, n, half):
    pr = results.get("proofs", {})
    s = ["# Privacy suite: generated results", "",
         f"Bridge program image `{lez_json['bridge_image_id']}`; vault "
         f"`{scenario['vault']}` on chain {scenario['chain_id']}; configuration `{scenario['config_id']}`; "
         f"{n} users per direction; proofs: **{pr.get('mode', 'dev')}** (LEZ's privacy-preserving circuit "
         f"`{pr.get('circuit_image_id')}`). Written by `scripts/privacy/adversary.py`.", "",
         "## The differential gate", "", "| records | bytes (LEZ transaction / Ethereum calldata), min-max | identical once nullifiers, fresh randomness, own addresses and time are cut |", "|---|---|---|"]
    for k, v in results["gate"].items():
        lo, hi = results["sizes"][k]
        s.append(f"| {k} | {lo:,}-{hi:,} | {'yes' if v['pass'] else 'NO: ' + v['detail']} |")
    s += ["", "## The adversary", "",
          f"Chance is 1/{n} = {100 / n:.1f}%. Success = mean over the right-hand records of 1/|best candidates| when "
          "the true partner is among them. Order-only = pairing the k-th of each side by time (allowed signal; "
          "informational).", "",
          "| game | success | exactly identified | candidate-set sizes (size: records) | without derived identifiers | with order-bearing state | order only |",
          "|---|---:|---:|---|---:|---:|---:|"]
    for k, v in results["adversary"].items():
        sizes = ", ".join(f"{a}: {c}" for a, c in v["candidate_set_sizes"])
        s.append(f"| {k} | {100 * v['success']:.1f}% | {v['exactly_identified']} of {n} | {sizes} | "
                 f"{100 * v['without_derived_identifiers']:.1f}% | {100 * v['with_order_state']:.1f}% | "
                 f"{100 * results['order_only'][k]:.1f}% |")
    r = results["recovery"]
    s += ["", f"## Recovery from the seed alone (after {half} claims and {half} releases)", "",
          f"- unclaimed deposits recovered: {r['unclaimed_found']} of {r['unclaimed_expected']}; claimed ones seen "
          f"as claimed: {r['claimed_seen_as_claimed']} of {n - r['unclaimed_expected']}",
          f"- unreleased burns recovered: {r['unreleased_found']} of {r['unreleased_expected']}; released ones seen "
          f"as released: {r['released_seen_as_released']} of {n - r['unreleased_expected']}",
          f"- other users' items wrongly taken as own: {r['foreign_items']}; recipients decrypted and authenticated: "
          f"{r['recipients_decrypted_correctly']} of {n}; recovered NOut equal to the vault's: {r['nout_matches_vault']} of {n}",
          f"- the bridge core's `recover_burns` (D12's v2 mac, review I-4) on the same notes: {r['rust_core']} found", "",
          f"Private actions proven against the newest commitment-set root: {results['proof_roots_newest'][0]} of "
          f"{results['proof_roots_newest'][1]}.", "",
          "## Receipts", "",
          f"- Ethereum gas used, deposits by leaf index (universe A): {results['gas']['deposit A by leaf index']}; "
          f"the same in universe B (other commitments, public destinations): {results['gas']['deposit A equals B']}",
          f"- Ethereum gas used, releases: distinct values {results['gas']['release distinct values']} (NOut of the "
          f"release statement), {results['gas']['release naive distinct values']} (naive)"]
    rp = results["receipts_by_position"]
    s.append(f"- execution gas by position, minus the first: private claims {rp['claims private']}; plaintext claims "
             f"{rp['claims plaintext']}; burns {rp['burns']}; releases (Ethereum) {rp['releases']}")
    for k, v in rp["checks"].items():
        s.append(f"- {k}: {'yes' if v else 'NO'}")
    for k, v in results["lez_receipts"].items():
        s.append(f"- LEZ, {k}: " + "; ".join(f"{n[8:]} {vals if len(vals) > 3 else ', '.join(map(str, vals))}"
                                              for n, vals in v.items()))
    s += proofs_markdown(pr, n)
    s.append("")
    (out_dir / "summary.md").write_text("\n".join(s) + "\n")
    # Section B6, compared.
    names = {"deposit (A: private destinations)": "deposit", "claim, private (default)": "claim",
             "burn, private holding": "burn", "release, NOut of the release statement": "release",
             "root import": "import-root", "claim, plaintext": "claim-plaintext"}
    c = ["# Privacy suite: section B6 against the generated table", "",
         "For each step: each item section B6 lists as public, and the fields that carry it; then every public field "
         "or state change section B6 does not list (a difference); then the search for what section B6 says is hidden.", ""]
    for step, kind in names.items():
        rows = results["fields"][step]
        c += [f"## {step}", "", "| §B6 public item | fields (generated) |", "|---|---|"]
        for item in B6[kind]:
            fs = [f"`{r['field']}`" for r in rows if r["B6"] == item]
            c.append(f"| {item} | {', '.join(fs) if fs else 'NOT FOUND'} |")
        c += ["", "Public and not in §B6:", ""]
        for r in rows:
            if r["B6"] is None:
                c.append(f"- `{r['field']}`: {r['across records']}; {r['class']}{'; ' + r['what'] if r['what'] else ''}")
        c.append("")
    c += ["## §B6's hidden column, searched", "", "| what must stay hidden | values searched | found |", "|---|---:|---:|"]
    for k, v in results["hidden"].items():
        if isinstance(v, dict):
            c.append(f"| {k} | {v['values searched']} | {v['found']} |")
    (out_dir / "b6.md").write_text("\n".join(c) + "\n")
    t = ["# Privacy suite: what is public at each step (generated)", "",
         "Every field the collectors can name, across the 20 records of each step: constant, or how many distinct "
         "values; its class when it varies; the item of the proposal's section B6 it falls under (`-`: not in B6).", ""]
    for k, rows in results["fields"].items():
        t += [f"## {k}", "", "| field | bytes | across records | class | what | §B6 |", "|---|---:|---|---|---|---|"]
        for row in rows:
            t.append(f"| `{row['field']}` | {row['bytes']} | {row['across records']} | {row['class']} | {row['what']} | {row['B6'] or '-'} |")
        t.append("")
    (out_dir / "fields.md").write_text("\n".join(t) + "\n")


if __name__ == "__main__":
    main()
