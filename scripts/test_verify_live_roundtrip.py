#!/usr/bin/env python3
"""Offline tests of scripts/verify-live-roundtrip.py on its committed record and proofs.json (no network).

- The LEZ transactions' bytes hash to their ids and decode to the recorded messages.
- The four links hold on the record's chain sections and proofs.json, and do not read the record's own `links` copy.
- Chains whose legs do not link fail their link: the Deposit event's commitment not the claim's; the vault's
  depositRoot after the deposit not the claim's path's root (nor the imported root); the imported root not the vault's;
  the candidate's and the checkpoint's burnRoot not the burn's; the candidate's journalDigest not the checkpoint proof's;
  a seal not the one the transaction carried; a release journal not the proof's; the walked blocks not ending at the
  candidate's tip.
- `--check`'s comparison catches every field: each of the record's leaves altered alone (the transaction hashes, block
  times, LEZ block hashes, nonces, root actors, selectors, keys, proof records, the vault, the chain id, the links among
  them) makes it fail, and so does a missing or an extra part.
Run: python3 -I scripts/test_verify_live_roundtrip.py"""
import base64
import copy
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("vlr", os.path.join(HERE, "verify-live-roundtrip.py"))
vlr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vlr)
with open(vlr.RECORD, encoding="utf-8") as f:
    REC = json.load(f)
PROOFS = vlr.load_proofs()
FAILED = []


def check(name, ok):
    print(("ok   " if ok else "FAIL ") + name)
    if not ok:
        FAILED.append(name)


def flip(hexstr, at=5):
    b = bytearray(bytes.fromhex(hexstr[2:] if hexstr.startswith("0x") else hexstr))
    b[at] ^= 1
    return ("0x" if hexstr.startswith("0x") else "") + b.hex()


def fails(fn, rec, proofs=PROOFS):
    try:
        fn(rec, proofs)
    except (vlr.Failed, KeyError, ValueError, StopIteration):
        return True
    return False


def holds(fn, rec, proofs=PROOFS):
    return not fails(fn, rec, proofs)


# 1. the zone's bytes
for s in REC["lez"]:
    t = vlr.read_tx(vlr.Reader(base64.b64decode(s["bytes_base64"])))
    check(f"LEZ {s['step']}: the bytes hash to the transaction's id and decode to the recorded message",
          t["id"] == s["tx"] == s["id_of_bytes"] and vlr.decode_bridge(t["message"]) == s["message"] and t["signers"] == s["signers"])

# 2. the links on the record's chain sections
for name, fn in vlr.LINKS:
    check(f"link holds on the record: {name}", holds(fn, REC))
no_links = copy.deepcopy(REC)
no_links["links"] = {"claim": {"commitment": "0x00"}, "burn": {}, "proofs": {}, "anchor": {}}
check("the links do not read the record's own `links` copy (garbage there, every link still holds)",
      all(holds(fn, no_links) for _, fn in vlr.LINKS))
check("the record's `links` copy is what the record's sections give", vlr.deep_diff(vlr.links(REC, PROOFS), REC["links"]) == [])


# 3. chains whose legs do not link: each fails its link
def mutated(edit):
    r = copy.deepcopy(REC)
    edit(r)
    return r


def ev(r, step, event):
    return next(e for e in vlr._step(r, "sepolia", step)["vault_events"] if e["event"] == event)


cases = [
    ("the Deposit event's commitment is not the claim's", vlr.link_claim,
     lambda r: ev(r, "deposit", "Deposit").__setitem__("commitment", flip(ev(r, "deposit", "Deposit")["commitment"]))),
    ("the vault's depositRoot after the deposit is not where the claim's path leads (nor the imported root)", vlr.link_claim,
     lambda r: vlr._step(r, "sepolia", "deposit")["vault_after"].__setitem__("depositRoot", flip(vlr._step(r, "sepolia", "deposit")["vault_after"]["depositRoot"]))),
    ("the imported root is not the vault's", vlr.link_claim,
     lambda r: vlr._step(r, "lez", "import_root")["message"]["args"].__setitem__("root", flip(vlr._step(r, "lez", "import_root")["message"]["args"]["root"]))),
    ("the Deposit event's amount is not the claim's", vlr.link_claim,
     lambda r: ev(r, "deposit", "Deposit").__setitem__("amount", ev(r, "deposit", "Deposit")["amount"] * 10)),
    ("the candidate's and the checkpoint's burnRoot are not the burn's", vlr.link_burn,
     lambda r: [ev(r, s, e).__setitem__("burnRoot", flip(ev(r, s, e)["burnRoot"])) for s, e in
                (("submitCandidate", "CandidateSubmitted"), ("finalizeAttested", "CheckpointFinalized"))]),
    ("the candidate's journalDigest is not the checkpoint proof's", vlr.link_proofs,
     lambda r: vlr._step(r, "sepolia", "submitCandidate")["vault_after"].__setitem__(
         "candidates(newState).journalDigest", flip(vlr._step(r, "sepolia", "submitCandidate")["vault_after"]["candidates(newState).journalDigest"]))),
    ("the vault's release image id is not the release proof's", vlr.link_proofs,
     lambda r: r["vault_immutables"].__setitem__("releaseImageId", flip(r["vault_immutables"]["releaseImageId"]))),
    ("the walked blocks do not end at the candidate's tip", vlr.link_anchor,
     lambda r: r["anchor"]["blocks"][-1].__setitem__("hash", flip(r["anchor"]["blocks"][-1]["hash"]))),
    ("a walked block's prev hash is not the block before's", vlr.link_anchor,
     lambda r: r["anchor"]["blocks"][5].__setitem__("prev", flip(r["anchor"]["blocks"][5]["prev"]))),
    ("the claim's block is not the one on the chain to the tip", vlr.link_anchor,
     lambda r: vlr._step(r, "lez", "claim").__setitem__("block_hash", flip(vlr._step(r, "lez", "claim")["block_hash"]))),
]
for name, fn, edit in cases:
    check(f"a chain where {name}: its link fails", fails(fn, mutated(edit)))
bad = copy.deepcopy(PROOFS)
bad["checkpoint"]["seal"] = flip(bad["checkpoint"]["seal"], 40)
check("a checkpoint proof whose seal is not the one the candidate carried: the proofs link fails", fails(vlr.link_proofs, REC, bad))
bad = copy.deepcopy(PROOFS)
bad["release"]["journal"] = flip(bad["release"]["journal"], 300)
check("a release proof whose journal is not the release's: the proofs link fails", fails(vlr.link_proofs, REC, bad))

# 4. every field compared: each leaf altered alone makes --check's comparison fail
def leaves(v, path=()):
    if isinstance(v, dict):
        for k, x in v.items():
            yield from leaves(x, path + (k,))
    elif isinstance(v, list):
        for i, x in enumerate(v):
            yield from leaves(x, path + (i,))
    else:
        yield path


def altered(v):
    if isinstance(v, bool):
        return not v
    if isinstance(v, int):
        return v + 1
    if isinstance(v, float):
        return v + 0.5
    if isinstance(v, str):
        return v[:-1] + ("0" if v[-1:] != "0" else "1") if v else "x"
    return "x"


def comparison_fails(fresh, recorded):
    try:
        for part in fresh:
            vlr.no_diff(fresh[part], recorded.get(part))
        vlr.need(sorted(fresh) == sorted(recorded), "parts differ")
    except vlr.Failed:
        return True
    return False


paths = list(leaves(REC))
missed = []
for p in paths:
    r = copy.deepcopy(REC)
    node = r
    for k in p[:-1]:
        node = node[k]
    node[p[-1]] = altered(node[p[-1]])
    if not comparison_fails(REC, r):
        missed.append(p)
check(f"each of the record's {len(paths)} fields, altered alone, makes the comparison fail", not missed)
named = [("sepolia", 0, "tx"), ("lez", 3, "tx"), ("sepolia", 0, "timestamp"), ("sepolia", 2, "timestamp"), ("sepolia", 0, "selector"),
         ("lez", 3, "id_of_bytes"), ("lez", 3, "block_hash"), ("lez", 3, "block_timestamp_ms"), ("lez", 3, "root_actor"),
         ("lez", 3, "nonces", 0, 1), ("keys", "recipient"), ("proofs", "checkpoint", "seconds"), ("proofs", "release", "journal_sha256"),
         ("vault",), ("chain_id",), ("links", "claim", "imported_leaf_count")]
check("among them the fields the review named (hashes, times, LEZ block hash, nonces, root actor, selector, keys, proof "
      "records, vault, chain id, links)", all(tuple(n) in set(paths) and tuple(n) not in missed for n in named))
r = copy.deepcopy(REC)
del r["anchor"]
check("a record without a part fails the comparison", comparison_fails(REC, r))
r = copy.deepcopy(REC)
r["lez"] = r["lez"][:-1]
check("a record with a step missing fails the comparison", comparison_fails(REC, r))
print("all passed" if not FAILED else f"FAILED {len(FAILED)}")
sys.exit(1 if FAILED else 0)
