#!/usr/bin/env python3
"""Offline tests of scripts/verify-live-roundtrip.py on its committed record (no network): the LEZ transactions'
bytes decode to the recorded messages and hash to their ids; the three links hold on the record; and each link fails
when the fact it rests on is altered (a byte of the claim's opening, a sibling of its path, the burn's amount, the
release's relayer field). Run: python3 -I scripts/test_verify_live_roundtrip.py"""
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
FAILED = []


def check(name, ok):
    print(("ok   " if ok else "FAIL ") + name)
    if not ok:
        FAILED.append(name)


def flip(hexstr, at=5):
    b = bytearray(bytes.fromhex(hexstr[2:]))
    b[at] ^= 1
    return "0x" + b.hex()


lez = {s["step"]: s for s in REC["lez"]}
sep = {s["step"]: s for s in REC["sepolia"]}
links = REC["links"]
for name, s in lez.items():
    t = vlr.read_tx(vlr.Reader(base64.b64decode(s["bytes_base64"])))
    check(f"LEZ {name}: the bytes hash to the transaction's id and decode to the recorded message",
          t["id"] == s["tx"] == s["id_of_bytes"] and vlr.decode_bridge(t["message"]) == s["message"] and t["signers"] == s["signers"])
c = vlr.claim_links(lez["claim"]["message"])
x = links["claim_opens_deposit"]
check("the claim's opening gives the Deposit event's commitment, and its leaf and path the vault's root after the deposit",
      c["commitment"] == x["deposit_commitment"] and c["root"] == x["deposit_root_after_deposit"] == x["imported_root"]
      and c["kind"] == 2 and c["dest_account"] == x["claim_signer"])
for field, label in (("s", "the opening's secret"), ("dest", "the destination")):
    m = copy.deepcopy(lez["claim"]["message"])
    m["args"]["witness"][field] = flip(m["args"]["witness"][field])
    check(f"a claim with {label} altered opens another commitment", vlr.claim_links(m)["commitment"] != x["deposit_commitment"])
m = copy.deepcopy(lez["claim"]["message"])
m["args"]["witness"]["path"][3] = flip(m["args"]["witness"]["path"][3])
check("a path with one sibling altered reaches another root", vlr.claim_links(m)["root"] != x["deposit_root_after_deposit"])
b = vlr.burn_links(lez["burn"]["message"])
y = links["burn_is_the_checkpoints"]
check("the burn's leaf alone is the candidate's, the checkpoint's and the release's burnRoot",
      b["root"] == y["candidate_burnRoot"] == y["finalized_burnRoot"] == y["release_burnRoot"] and y["burnCount"] == 1)
m = copy.deepcopy(lez["burn"]["message"])
m["args"]["amount"] = flip(m["args"]["amount"], 31)
check("a burn of another amount is not that burn root's leaf", vlr.burn_links(m)["root"] != y["candidate_burnRoot"])
check("the burn sits at index 0: at index 1 its leaf gives another root", vlr.burn_links(lez["burn"]["message"], 1)["root"] != y["candidate_burnRoot"])
rel = next(e for e in sep["release"]["vault_events"] if e["event"] == "Release")
cfg = lez["claim"]["message"]["args"]["config_id"]
j = vlr.release_journal(sep["release"]["input"], rel["amount"], cfg)
check("the release journal from the release's calldata hashes to the release proof's journal digest",
      j["journal_sha256"] == vlr.PROOFS["release"]["journal_sha256"] == links["proofs"]["release_journal_sha256_prover"])
bad = sep["release"]["input"][:10] + sep["release"]["input"][10:10 + 3 * 64] + "00" * 31 + "01" + sep["release"]["input"][10 + 4 * 64:]
check("a release naming another relayer has another journal", vlr.release_journal(bad, rel["amount"], cfg)["journal_sha256"] != j["journal_sha256"])
check("the candidate's journalDigest on the vault is the checkpoint proof's",
      links["proofs"]["checkpoint_journalDigest_on_chain"] == vlr.PROOFS["checkpoint"]["journal_sha256"]
      == sep["submitCandidate"]["vault_after"]["candidates(newState).journalDigest"])
st = {s["step"]: (s["vault_before"], s["vault_after"]) for s in REC["sepolia"]}
owed = next(e for e in sep["release"]["vault_events"] if e["event"] == "OwedCredited")  # the checkpoint fee, to its prover
check("the vault's state moves as the round trip says: a leaf, a final checkpoint, a used nullifier, the fee claimed",
      st["deposit"][1]["nextLeafIndex"] == st["deposit"][0]["nextLeafIndex"] + 1
      and st["finalizeAttested"][1]["checkpointCount"] == st["finalizeAttested"][0]["checkpointCount"] + 1
      and not st["release"][0]["nullifierUsed(NOut)"] and st["release"][1]["nullifierUsed(NOut)"]
      and st["release"][1]["balance(recipient)"] - st["release"][0]["balance(recipient)"] == rel["amount"] - rel["relayFee"] - owed["amount"]
      and st["claimOwed"][1]["owed(relayer, ETH)"] == 0 < st["claimOwed"][0]["owed(relayer, ETH)"])
print("all passed" if not FAILED else f"FAILED {len(FAILED)}")
sys.exit(1 if FAILED else 0)
