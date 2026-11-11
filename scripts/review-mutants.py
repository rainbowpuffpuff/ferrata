#!/usr/bin/env python3
"""Extra mutants from the independent review of 11 November 2026 (R01-R13, and S01-S11 in steps.rs).

Each one is a plausible bug in a function of docs/REVIEW-core.md that none of M01-M107 plants.
The runner is scripts/mutants.py's own (same scratch copy, same whole-workspace test run); only
the list differs. A mutant that survives is a behaviour no test pins.

Usage: scripts/review-mutants.py WORKDIR_SUFFIX [ID ...]
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("mutants", os.path.join(HERE, "mutants.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

V, S, P, U, M, X, A = "validate.rs", "store.rs", "policy.rs", "upgrade.rs", "merkle.rs", "exec.rs", "attest.rs"
ST = "steps.rs"

REVIEW_MUTANTS = [
    ("R01", S, "a committee taken at rotation is not reported as adopted (policy skips its check)",
     "        outcome.rotated = true;\n        outcome.adopted = !core.next_sync_committee.is_empty();\n",
     "        outcome.rotated = true;\n"),
    ("R02", U, "header_to_gloas proves block_hash in the Deneb layout for Capella-era headers too",
     "        let deneb = spec.is_at_or_after(slot, Fork::Deneb);\n",
     "        let deneb = true;\n"),
    ("R03", V, "get_lc_execution_root ignores the store layout (a Capella store hashes as Deneb)",
     "            if fork >= LcFork::Deneb && spec.is_at_or_after(slot, Fork::Deneb) {\n",
     "            if spec.is_at_or_after(slot, Fork::Deneb) {\n"),
    ("R04", A, "a slot's conditions skipped in a batch",
     "            let fact = Fact::Storage(storage.fact);\n            fact.check(s.conditions)?;\n",
     "            let fact = Fact::Storage(storage.fact);\n"),
    ("R05", A, "a log's conditions skipped in a batch",
     "        let fact = Fact::Log(log.fact);\n        fact.check(q.conditions)?;\n",
     "        let fact = Fact::Log(log.fact);\n"),
    ("R06", S, "best_valid_update kept after a normal (2/3) apply",
     "            next.apply(update, spec)?;\n            next.best_valid_update = None;\n",
     "            next.apply(update, spec)?;\n"),
    ("R07", P, "from_core does not check a known next committee",
     "        if store.core.is_next_sync_committee_known() {\n            store.check(bls, &store.core.next_sync_committee)?;\n        }\n",
     ""),
    ("R08", V, "Gloas-layout header of a Capella-era slot validated at the Deneb block-hash gindex",
     "            } else if spec.is_at_or_after(slot, Fork::Capella) {\n                EXECUTION_BLOCK_HASH_GINDEX\n",
     "            } else if spec.is_at_or_after(slot, Fork::Capella) {\n                EXECUTION_BLOCK_HASH_GINDEX_DENEB\n"),
    ("R09", V, "get_lc_execution_root of a Capella-era Gloas header uses the Deneb payload gindex",
     "                    payload_root_from_block_hash(block_hash, branch, BLOCK_HASH_IN_CAPELLA_PAYLOAD_GINDEX)\n",
     "                    payload_root_from_block_hash(block_hash, branch, BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX)\n"),
    ("R10", M, "normalize_merkle_branch pads at the root end",
     "    let mut out = vec![ZERO; num_extra];\n    out.extend_from_slice(branch);\n    out\n",
     "    let mut out = branch.to_vec();\n    out.extend(core::iter::repeat(ZERO).take(num_extra));\n    out\n"),
    ("R11", A, "Emitter field left-aligned instead of left-padded",
     "            (Fact::Log(l), Field::Emitter) => w[12..].copy_from_slice(&l.log.address),\n",
     "            (Fact::Log(l), Field::Emitter) => w[..20].copy_from_slice(&l.log.address),\n"),
    ("R12", V, "finalized header's layout not checked in the shape checks",
     "    if !update.attested_header.has_layout(fork)\n        || !update.finalized_header.has_layout(fork)\n",
     "    if !update.attested_header.has_layout(fork)\n"),
    ("R13", X, "account with a zero storage root accepted",
     "        if account.storage_root == [0; 32] || account.code_hash == [0; 32] {",
     "        if account.code_hash == [0; 32] {"),
    # steps.rs: the transcription the on-chain program runs (validate_compact, update_advances,
    # apply_core_compact, store_for_update). M01-M107 plant nothing in this file.
    ("S01", ST, "steps: no period-skip check",
     "    if !period_ok {\n        return Err(Error::PeriodSkipped);\n    }\n", ""),
    ("S02", ST, "steps: no relevance check",
     "    if !(update_attested_slot > store.finalized.slot || update_has_next_sync_committee) {\n        return Err(Error::UpdateIrrelevant);\n    }\n", ""),
    ("S03", ST, "steps: no NextCommitteeMismatch check",
     "        if update_attested_period == store_period && next_known && update.next_sync_committee != store.next {\n            return Err(Error::NextCommitteeMismatch);\n        }\n", ""),
    ("S04", ST, "steps: fork version at signature_slot (no -1)",
     "    let fork_version_slot = update.signature_slot.saturating_sub(1);\n",
     "    let fork_version_slot = update.signature_slot;\n"),
    ("S05", ST, "steps: finality gindex by the finalized slot",
     "            finalized_root_gindex_at_slot(update_attested_slot, fork, spec),\n",
     "            finalized_root_gindex_at_slot(update_finalized_slot, fork, spec),\n"),
    ("S06", ST, "steps: signature always checked against the current committee",
     "        if update_signature_period == store_period { SigningCommittee::Current } else { SigningCommittee::Next };\n",
     "        SigningCommittee::Current;\n"),
    ("S07", ST, "steps: update_advances without the finalized-next-committee clause",
     "    update.finalized_header.beacon.slot > store.finalized.slot || update_has_finalized_next_sync_committee\n",
     "    update.finalized_header.beacon.slot > store.finalized.slot\n"),
    ("S08", ST, "steps: a committee taken at rotation is not adopted (no adoption check)",
     "            outcome.rotated = true;\n            outcome.adopted = next.is_some();\n",
     "            outcome.rotated = true;\n"),
    ("S09", ST, "steps: attested >= finalized becomes >",
     "        && update_attested_slot >= update_finalized_slot)\n",
     "        && update_attested_slot > update_finalized_slot)\n"),
    ("S10", ST, "steps: a store may move to a layout its schedule has not reached",
     "    if layout < store.fork || scheduled != Some(layout) {\n",
     "    if layout < store.fork {\n"),
    ("S11", ST, "steps: current_slot >= signature_slot becomes >",
     "    if !(current_slot >= update.signature_slot\n",
     "    if !(current_slot > update.signature_slot\n"),
]

if __name__ == "__main__":
    suffix = sys.argv[1]
    m.WORK = os.path.join(m.ROOT, "target", "review-mutants-" + suffix)
    m.COPY = os.path.join(m.WORK, "ws")
    m.MUTANTS = REVIEW_MUTANTS
    sys.argv = [sys.argv[0]] + sys.argv[2:]
    m.main()
