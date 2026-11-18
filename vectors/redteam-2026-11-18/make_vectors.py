#!/usr/bin/env python3
"""Assemble the blind red-team vectors of 18 Nov 2026 into vectors.json and check their shape.

Run with `python3 -I make_vectors.py` from this directory (it only reads the vectors_*.py files beside
it and writes vectors.json). The expectations were written from the specification documents alone;
see README.md, "Blindness".
"""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from vectors_lc import LC
from vectors_attest import ATTEST, CONSUMER
from vectors_bridge import BRIDGE, VAULT, CHECKPOINT, BRIDGE_CORE

# Code names and numbers as the documents read in phase 1 give them (crates/ferrata-core/src/error.rs,
# lez/wire/src/lib.rs `code` and `consumer::code`, lez/bridge-wire/src/lib.rs `code`). Step-API numbers
# other than Phase (81, docs/design/attestation.md §4.4) are not in those documents: null, resolved in
# phase 2 from the code by name.
CORE = {"Malformed": 1, "CheckpointMismatch": 10, "BootstrapHeaderInvalid": 11, "CurrentCommitteeBranchInvalid": 12,
        "ParticipationLow": 20, "AttestedHeaderInvalid": 21, "SlotOrderInvalid": 22, "PeriodSkipped": 23,
        "UpdateIrrelevant": 24, "FinalizedHeaderInvalid": 25, "FinalityBranchInvalid": 26,
        "NextCommitteeBranchInvalid": 27, "NextCommitteeMismatch": 28, "NextCommitteeUnexpected": 29,
        "SignatureInvalid": 40, "SignatureMalformed": 41, "PubkeyInvalid": 42, "SigningKeyIdentity": 43,
        "CommitteeInvalid": 45, "NotFinalized": 50, "BelowSupermajority": 51, "NotNewer": 52,
        "OutsideWeakSubjectivityPeriod": 53, "CommitteeUnknown": 60, "ForkMismatch": 61,
        "RlpNonCanonical": 100, "RlpMalformed": 101, "ProofNodeMismatch": 110, "ProofIncomplete": 111,
        "ProofTrailingNodes": 112, "ProofNodeInvalid": 113, "HeaderHashMismatch": 120, "HeaderUnsupported": 121,
        "ReceiptNotIncluded": 122, "ReceiptUnsupported": 123, "ReceiptFailed": 124, "LogIndexOutOfRange": 125,
        "PredicateFailed": 130, "QueryInvalid": 131, "ChainMismatch": 140, "BindingMismatch": 141,
        "HistoryOutOfWindow": 150, "HistoryHashMismatch": 151, "HistoryContractMissing": 152,
        "AccumulatorPathInvalid": 160, "AccumulatorIndexOutOfRange": 161, "AccumulatorLeafEmpty": 162,
        "AccumulatorRootMismatch": 163}
STEP = {"Witness": None, "Phase": 81, "StoreChanged": None, "AdoptionIncomplete": None, "NoAdoption": None}
PROG = {"BAD_MESSAGE": 200, "WRONG_RECEIVER": 201, "WRONG_SENDER": 202, "ALREADY_INITIALIZED": 203, "NO_INSTANCE": 204,
        "NO_JOB": 205, "INSTANCE_INACTIVE": 206, "JOB_EXISTS": 207, "RECORD_CONFLICT": 208, "BAD_STATE": 209,
        "UNROUTABLE": 210, "WRONG_INSTANCE": 211, "BAD_CONFIG": 212, "NO_RECORD": 213, "BAD_QUERY": 214,
        "HEADER_REQUIRED": 215, "BAD_SUCCESSOR": 216, "STALE_CHECKPOINT": 217}
CONS = {"COMMITMENT_MISMATCH": 218, "WRONG_KIND": 219, "BAD_MESSAGE": 220, "BAD_STATE": 221, "BAD_REPLY": 222,
        "ALREADY_CLAIMED": 223, "WRONG_RECEIVER": 224, "WRONG_SENDER": 225, "BINDING_MISMATCH": 226,
        "UNEXPECTED_FACT": 227, "AMOUNT_TOO_LARGE": 228, "NULLIFIER_MISMATCH": 229}
BRIDGE_CODES = {"BAD_MESSAGE": 240, "BAD_STATE": 241, "FROZEN": 242, "WRONG_SENDER": 243, "ALREADY_WRITTEN": 244,
                "NOT_FOUND": 245, "PAIRING_MISMATCH": 246, "IMPORT_MISMATCH": 247, "LEAF_NOT_IMPORTED": 248,
                "CLAIM_INVALID": 249, "ALREADY_CLAIMED": 250, "NOT_REGISTERED": 251, "WRONG_DESTINATION": 252,
                "NOT_AUTHORIZED": 253, "INSUFFICIENT_BALANCE": 254, "CAPPED": 255}
OTHER = {"lez:validity-window": "LEZ refuses the transaction (timestamp validity window); no exit code",
         "lez:any": "LEZ refuses the transaction itself (signature, authorisation); no program exit code",
         "evm:revert": "the EVM call reverts (any reason)",
         "any": "refused with any error (the specification names no code)"}

def code_number(name):
    ns, _, n = name.partition(":")
    table = {"core": CORE, "step": STEP, "prog": PROG, "cons": CONS, "bridge": BRIDGE_CODES}.get(ns)
    if table is None:
        if name in OTHER:
            return None
        raise SystemExit(f"unknown code namespace in {name!r}")
    if n not in table:
        raise SystemExit(f"unknown code name {name!r}")
    return table[n]

BASELINES = {
 "B-LC-MAIN": "Mainnet light-client fixture (Fulu, 512-member committees, periods 1899-1903 or the newer mainnet fixture): an instance of the attestation program initialised from its checkpoint, committees adopted; the next real finality update U and its signer witness. LEZ v0.4 in-process harness.",
 "B-LC-SEP": "Sepolia fixture: an instance initialised from the period-1378 bootstrap with Sepolia's schedule (Gloas at epoch 353,024); real Fulu- and Gloas-era updates.",
 "B-LC-SYN": "A synthetic chain: a committee whose secret keys the test holds (minimal or mainnet preset), a checkpoint and states built by the test, so the test can sign any header with any subset.",
 "B-AT-MAIN": "The record of a finalized mainnet block (payload layout) written by the program, with eth_getProof account and storage proofs and receipt proofs of that block.",
 "B-AT-GLOAS": "A Sepolia record in the Gloas layout (execution block hash only), the block's RLP header and proofs.",
 "B-AT-ACC": "A record whose state holds the vault's deposit-root slot (FerrataVault v5 on Sepolia, or a synthetic state) and real leaves with their 32-sibling paths.",
 "B-CON": "The reference consumer's Pool (ERC-20 Transfer to a recipient) bound to B-AT-MAIN's instance.",
 "B-CON-VAULT": "The reference consumer's VaultPool over FerrataVault v5's deposits (rules 5-7), with depositors' secrets.",
 "B-BR": "The bridge program with the attestation program: configuration A paired with vault V, token T registered with denominations d0<d1<d2, root(A, n) imported, deposits whose (s, kind, dest) the test knows.",
 "B-VLT": "FerrataVault v5 under forge (vault/test), mock or real verifier as in its suite.",
 "B-CKP": "The checkpoint core (checkpoint/crates) with its v0.4-format fixture blocks and test sequencer keys.",
 "B-BC": "ferrata-bridge-core's commitment, nullifier and burn-note functions.",
}

AREAS = [("lc", LC), ("attest", ATTEST), ("consumer", CONSUMER), ("bridge", BRIDGE), ("vault", VAULT),
         ("checkpoint", CHECKPOINT), ("bridge-core", BRIDGE_CORE)]

def main():
    out, seen = [], set()
    for area, rows in AREAS:
        for row in rows:
            if len(row) != 10:
                raise SystemExit(f"bad tuple length {len(row)} in {row[0]}")
            vid, target, kind, title, setup, mutation, expect, codes, spec, notes = row
            if vid in seen:
                raise SystemExit(f"duplicate id {vid}")
            seen.add(vid)
            if kind not in ("adversarial", "control"):
                raise SystemExit(f"{vid}: kind {kind}")
            if expect not in ("accepted", "refused"):
                raise SystemExit(f"{vid}: expect {expect}")
            if kind == "control" and expect != "accepted":
                raise SystemExit(f"{vid}: a control must expect acceptance")
            if (expect == "refused") != bool(codes):
                raise SystemExit(f"{vid}: refused needs codes, accepted none")
            if not spec:
                raise SystemExit(f"{vid}: no specification sentence")
            out.append({
                "id": vid, "area": area, "target": target, "kind": kind, "title": title,
                "setup": setup, "mutation": mutation,
                "expect": {"outcome": expect,
                           "codes": [{"name": c, "number": code_number(c)} for c in codes]},
                "spec": spec, "notes": notes,
            })
    doc = {
        "name": "Ferrata blind red-team vectors",
        "written": "2026-11-18, phase 1 (blind): from the specification documents only; see README.md",
        "schema": {
            "outcome": "accepted | refused",
            "codes": "for a refusal, every code the specification allows (the first is the one it points to most directly); namespaces core:, step:, prog:, cons:, bridge: are exit codes; lez:validity-window, lez:any, evm:revert, any are described in other_codes",
            "kind": "adversarial (an attack or a malformed input) | control (a valid input that must be accepted)",
        },
        "baselines": BASELINES,
        "other_codes": OTHER,
        "counts": {area: len(rows) for area, rows in AREAS} | {"total": len(out),
                   "controls": sum(v["kind"] == "control" for v in out),
                   "adversarial": sum(v["kind"] == "adversarial" for v in out)},
        "vectors": out,
    }
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "vectors.json")
    with open(path, "w") as f:
        json.dump(doc, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(json.dumps(doc["counts"]))

if __name__ == "__main__":
    main()
