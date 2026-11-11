#!/usr/bin/env python3
"""Hand-picked mutants for ferrata-core.

Each mutant replaces one exact snippet in a scratch copy of the workspace (under
target/mutants/), runs the whole test suite and records whether a test fails (killed) or all
pass (survived). The repository itself is only read. M01-M32 were proposed by an independent
review of the first version of this code; M33-M43 cover light-client code added since,
M44-M93 the execution layer (RLP, Merkle-Patricia proofs, execution objects, attestations), and
M94-M107 accumulator facts, log-index conversion, relations and block-bound identifiers, and
M108-M115 the weak-subjectivity bound (milestone A1).

Usage: scripts/mutants.py [ID ...]
"""
import os
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORK = os.path.join(ROOT, "target", "mutants")
COPY = os.path.join(WORK, "ws")
CORE = "crates/ferrata-core/src/"

V, S, P, B, T, U, M = "validate.rs", "store.rs", "policy.rs", "bls/blst_backend.rs", "types.rs", "upgrade.rs", "merkle.rs"
Z = "bls/zkcrypto_backend.rs"
R, MP, X, A, ACC = "rlp.rs", "mpt.rs", "exec.rs", "attest.rs", "accumulator.rs"
WS = "ws.rs"

MUTANTS = [
    ("M01", P, "no committee check at initialisation",
     "        let store = Self { core, adoption, ws_period: None };\n        store.check(bls, &store.core.current_sync_committee)?;\n        Ok(store)",
     "        let store = Self { core, adoption, ws_period: None };\n        Ok(store)"),
    ("M02", P, "no committee check at adoption",
     "        if outcome.adopted {\n            self.check(ctx.bls, &next.next_sync_committee)?;\n        }\n", ""),
    ("M03", V, "no NextCommitteeMismatch check",
     "        if update_attested_period == store_period\n            && next_known\n            && update.next_sync_committee != store.next_sync_committee\n        {\n            return Err(Error::NextCommitteeMismatch);\n        }\n", ""),
    ("M04", V, "attested >= finalized becomes >",
     "        && update_attested_slot >= update_finalized_slot)", "        && update_attested_slot > update_finalized_slot)"),
    ("M05", T, "no extra_data length limit",
     "        if p[10].len() > MAX_EXTRA_DATA_BYTES {\n            return Err(Error::Malformed);\n        }\n", ""),
    ("M06", T, "no bitvector padding check",
     "        if n % 8 != 0 && b[nbytes - 1] >> (n % 8) != 0 {\n            return Err(Error::Malformed);\n        }\n", ""),
    ("M07", B, "no G2 subgroup check (blst)",
     "    if !unsafe { blst_p2_affine_in_g2(&sig) } {\n        return Err(Error::SignatureMalformed);\n    }\n", ""),
    ("M08", V, "current_slot >= signature_slot becomes >",
     "    if !(current_slot >= update.signature_slot", "    if !(current_slot > update.signature_slot"),
    ("M09", V, "fork version at signature_slot (no -1)",
     "let fork_version_slot = update.signature_slot.saturating_sub(1);", "let fork_version_slot = update.signature_slot;"),
    ("M10", V, "no pre-Deneb blob-gas rejection",
     "            if has_blob_gas && (fork == LcFork::Capella || !spec.is_at_or_after(slot, Fork::Deneb)) {\n                return false;\n            }\n", ""),
    ("M11", V, "pre-Capella payload header always valid",
     "                return payload.is_empty() && is_zero_branch(branch);", "                return true;"),
    ("M12", V, "pre-Capella Gloas-layout header always valid",
     "                return *block_hash == ZERO && is_zero_branch(branch);", "                return true;"),
    ("M13", V, "genesis-finalized header need not be empty",
     "        let finalized_root = if update_finalized_slot == GENESIS_SLOT {\n            if !update.finalized_header.is_empty() {\n                return Err(Error::FinalizedHeaderInvalid);\n            }\n            ZERO",
     "        let finalized_root = if update_finalized_slot == GENESIS_SLOT {\n            ZERO"),
    ("M14", V, "non-finality update may carry a finalized header",
     "    if !is_finality_update(update) {\n        if !update.finalized_header.is_empty() {\n            return Err(Error::FinalizedHeaderInvalid);\n        }\n    } else {",
     "    if !is_finality_update(update) {\n    } else {"),
    ("M15", V, "zero-branch update may carry a committee",
     "        if !update.next_sync_committee.is_empty() {\n            return Err(Error::NextCommitteeUnexpected);\n        }\n", ""),
    ("M16", V, "no period-skip check",
     "    if !period_ok {\n        return Err(Error::PeriodSkipped);\n    }\n", ""),
    ("M17", V, "no relevance check",
     "    if !(update_attested_slot > store.finalized_header.beacon.slot || update_has_next_sync_committee) {\n        return Err(Error::UpdateIrrelevant);\n    }\n", ""),
    ("M18", V, "is_better_update without tiebreaker 3",
     "    new.signature_slot < old.signature_slot\n}", "    false\n}"),
    ("M19", V, "is_better_update without tiebreaker 2",
     "    if new.attested_header.beacon.slot != old.attested_header.beacon.slot {\n        return new.attested_header.beacon.slot < old.attested_header.beacon.slot;\n    }\n", ""),
    ("M20", V, "Gloas store without the Deneb genesis case",
     "                if slot == GENESIS_SLOT {\n                    ExecutionPayloadHeader::empty().hash_tree_root_deneb()\n                } else {\n                    payload_root_from_block_hash(block_hash, branch, BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX)\n                }",
     "                payload_root_from_block_hash(block_hash, branch, BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX)"),
    ("M21", V, "is_better_update without sync-committee finality",
     "        if new_scf != old_scf {\n            return new_scf;\n        }\n", ""),
    ("M22", V, "finalized header validity not checked",
     "            if !is_valid_light_client_header(&update.finalized_header, fork, spec) {\n                return Err(Error::FinalizedHeaderInvalid);\n            }\n", ""),
    ("M23", S, "optimistic header ignores the safety threshold",
     "        if participants > next.safety_threshold()\n", "        if participants > 0\n"),
    ("M24", Z, "zkcrypto KeyValidate without the subgroup check",
     "Option::from(G1Affine::from_compressed(bytes)).ok_or(Error::PubkeyInvalid)?;",
     "Option::from(G1Affine::from_compressed_unchecked(bytes)).ok_or(Error::PubkeyInvalid)?;"),
    ("M25", V, "finality gindex by the finalized slot",
     "            finalized_root_gindex_at_slot(update_attested_slot, fork, spec),", "            finalized_root_gindex_at_slot(update_finalized_slot, fork, spec),"),
    ("M26", U, "header_to_gloas without the empty-header case",
     "    if pre.is_empty() {\n        return Ok(LightClientHeader::empty(LcFork::Gloas));\n    }\n", ""),
    ("M27", S, "force update: <= becomes <",
     "        if best.finalized_header.beacon.slot <= next.core.finalized_header.beacon.slot {",
     "        if best.finalized_header.beacon.slot < next.core.finalized_header.beacon.slot {"),
    ("M28", V, "relevance ignores whether the next committee is known",
     "        !next_known && is_sync_committee_update(update) && update_attested_period == store_period;",
     "        is_sync_committee_update(update) && update_attested_period == store_period;"),
    ("M29", S, "update_advances without the finalized-next-committee clause",
     "        update.finalized_header.beacon.slot > self.finalized_header.beacon.slot\n            || update_has_finalized_next_sync_committee\n",
     "        update.finalized_header.beacon.slot > self.finalized_header.beacon.slot\n"),
    ("M30", P, "policy without the NotNewer guard",
     "        if !self.core.update_advances(update, ctx.spec) {\n            return Err(Error::NotNewer);\n        }\n", ""),
    ("M31", V, "next-committee gindex by the finalized slot",
     "            next_sync_committee_gindex_at_slot(update_attested_slot, fork, spec),", "            next_sync_committee_gindex_at_slot(update_finalized_slot, fork, spec),"),
    ("M32", V, "signature always checked against the current committee",
     "    let sync_committee = if update_signature_period == store_period {", "    let sync_committee = if true || update_signature_period == store_period {"),
    ("M33", B, "blst KeyValidate without the subgroup check",
     "    if !unsafe { blst_p1_affine_in_g1(&point) } {", "    if false {"),
    ("M34", B, "blst checked path without the identity check",
     "        if is_identity(&key) {\n            return Err(Error::SigningKeyIdentity);\n        }\n        Ok(key)", "        Ok(key)"),
    ("M35", B, "blst committee check ignores the aggregate",
     "    if sum_affine == aggregate {", "    if true {"),
    ("M36", M, "normalized branch padding not checked",
     "    if branch[..num_extra].iter().any(|b| *b != ZERO) {\n        return false;\n    }\n", ""),
    ("M37", V, "attested header validity not checked",
     "    if !is_valid_light_client_header(&update.attested_header, fork, spec) {\n        return Err(Error::AttestedHeaderInvalid);\n    }\n", ""),
    ("M38", S, "bootstrap root not compared with the checkpoint",
     "        if bootstrap.header.beacon.hash_tree_root() != *trusted_block_root {\n            return Err(Error::CheckpointMismatch);\n        }\n", ""),
    ("M39", P, "supermajority floor at one third",
     "        if update.sync_aggregate.participants() * 3 < n * 2 {", "        if update.sync_aggregate.participants() * 3 < n {"),
    ("M40", P, "policy accepts non-finality updates",
     "        if !is_finality_update(update) {\n            return Err(Error::NotFinalized);\n        }\n", ""),
    ("M41", B, "blst on-chain check accepts the identity",
     "    // SAFETY: `point` is a valid affine point.\n    if unsafe { blst_p1_affine_is_inf(&point) } {",
     "    // SAFETY: `point` is a valid affine point.\n    if false && unsafe { blst_p1_affine_is_inf(&point) } {"),
    ("M42", P, "full adoption runs the on-chain check",
     "            Adoption::Full => bls.check_committee(committee),", "            Adoption::Full => bls.check_committee_onchain(committee),"),
    ("M43", B, "final_half accepts any product",
     "    if unsafe { blst_fp12_is_one(&result) } {", "    if true || unsafe { blst_fp12_is_one(&result) } {"),
    # RLP.
    ("M44", R, "single byte below 0x80 accepted as a one-byte string",
     "    if !is_list && len == 1 && payload[0] < 0x80 {\n        return Err(Error::RlpNonCanonical);\n    }\n", ""),
    ("M45", R, "long-form length may start with a zero byte",
     "    if be[0] == 0 {\n        return Err(Error::RlpNonCanonical);\n    }\n", ""),
    ("M46", R, "long form accepted for a 55-byte payload",
     "    if len < 56 {", "    if len < 55 {"),
    ("M47", R, "an item may claim one byte past its input",
     "    if end > input.len() {", "    if end > input.len() + 1 {"),
    ("M48", R, "bytes after the item accepted",
     "    let (item, rest) = split_first(input)?;\n    if !rest.is_empty() {\n        return Err(Error::RlpMalformed);\n    }\n",
     "    let (item, _rest) = split_first(input)?;\n"),
    ("M49", R, "integers may have a leading zero",
     "        if b.first() == Some(&0) {\n            return Err(Error::RlpNonCanonical);\n        }\n", ""),
    ("M50", R, "integers one byte longer than their field",
     "        if b.len() > max {", "        if b.len() > max + 1 {"),
    ("M51", R, "a fixed-arity list may have extra items",
     "        if list.next().is_some() {\n            return Err(Error::RlpMalformed);\n        }\n", ""),
    ("M52", R, "RLP(0x80) encoded as a single byte",
     "    if n < 0x80 {", "    if n <= 0x80 {"),
    # Merkle-Patricia proofs.
    ("M53", MP, "node hash not compared with its reference",
     "                if keccak256(node) != hash {\n                    return Err(Error::ProofNodeMismatch);\n                }\n", ""),
    ("M54", MP, "unused proof elements accepted",
     "    if used != proof.len() {\n        return Err(Error::ProofTrailingNodes);\n    }\n", ""),
    ("M55", MP, "a node shorter than 32 bytes may be referenced by hash",
     "                if used > 0 && node.len() < 32 {", "                if used > 0 && node.len() < 31 {"),
    ("M56", MP, "the root may not be shorter than 32 bytes",
     "                if used > 0 && node.len() < 32 {", "                if node.len() < 32 {"),
    ("M57", MP, "an embedded node of 32 bytes accepted",
     "        if item.raw.len() >= 32 {", "        if item.raw.len() > 32 {"),
    ("M58", MP, "odd extension flag read as even",
     "        1 => (false, true),", "        1 => (false, false),"),
    ("M59", MP, "even path's padding nibble not checked",
     "    if !odd && first & 0x0f != 0 {", "    if false {"),
    ("M60", MP, "extension with an empty path accepted",
     "            if path.len == 0 {\n                return Err(Error::ProofNodeInvalid);\n            }\n", ""),
    ("M61", MP, "a leaf matches any key it is a prefix of",
     "                break (path.len == total - pos && path.is_prefix_of(key, pos)).then_some(value);",
     "                break path.is_prefix_of(key, pos).then_some(value);"),
    ("M62", MP, "branch value slot ignored when the key ends there",
     "                if pos == total {\n                    break value;\n                }",
     "                if pos == total {\n                    break None;\n                }"),
    ("M63", MP, "leaf with an empty value accepted",
     "                if value.is_empty() {\n                    return Err(Error::ProofNodeInvalid);\n                }\n", ""),
    ("M64", MP, "malformed child reference read as empty",
     "        _ => Err(Error::ProofNodeInvalid),\n    }\n}\n\n/// A string item", "        _ => Ok(None),\n    }\n}\n\n/// A string item"),
    ("M65", MP, "a three-item node decoded as leaf or extension",
     "        2 => {\n            let (leaf, path) = hex_prefix(string(items[0])?)?;",
     "        2 | 3 => {\n            let (leaf, path) = hex_prefix(string(items[0])?)?;"),
    ("M66", MP, "an empty branch slot read as an incomplete proof",
     "                    None => break None,", "                    None => return Err(Error::ProofIncomplete),"),
    # Execution objects.
    ("M67", X, "22-field header accepted as Amsterdam",
     "        23 => HeaderShape::Amsterdam,", "        22 | 23 => HeaderShape::Amsterdam,"),
    ("M68", X, "extra_data of 33 bytes accepted",
     "    if f[12].bytes()?.len() > MAX_EXTRA_DATA_BYTES {", "    if f[12].bytes()?.len() > MAX_EXTRA_DATA_BYTES + 1 {"),
    ("M69", X, "header hash not compared with the bound hash",
     "    if keccak256(encoded) != *block_hash {\n        return Err(Error::HeaderHashMismatch);\n    }\n", ""),
    ("M70", X, "account with a zero code hash accepted",
     "        if account.storage_root == [0; 32] || account.code_hash == [0; 32] {",
     "        if account.storage_root == [0; 32] {"),
    ("M71", X, "a stored zero accepted as a storage value",
     "    if value == [0; 32] {\n        return Err(Error::RlpMalformed);\n    }\n", ""),
    ("M72", X, "an absent account's storage root used as is",
     "    let root = if account.exists() { account.storage_root } else { EMPTY_ROOT };", "    let root = account.storage_root;"),
    ("M73", X, "pre-Byzantium receipt not refused",
     "        if status.bytes()?.len() == 32 {\n            return Err(Error::ReceiptUnsupported);\n        }\n", ""),
    ("M74", X, "any non-zero status read as success",
     "            1 => true,\n            _ => return Err(Error::RlpMalformed),", "            _ => true,"),
    ("M75", X, "receipt type 5 accepted",
     "            0x01..=0x04 => (first, rest),", "            0x01..=0x05 => (first, rest),"),
    ("M76", X, "log index bound off by one",
     "        if i >= self.log_count {", "        if i > self.log_count {"),
    # Attestations.
    ("M77", A, "failed transaction's logs attested",
     "    if !receipt.status {\n        return Err(Error::ReceiptFailed);\n    }\n", ""),
    ("M78", A, "record of another chain accepted",
     "        if self.chain_id != chain_id {\n            return Err(Error::ChainMismatch);\n        }\n", ""),
    ("M79", A, "history window one block too long",
     "record.number - n <= HISTORY_SERVE_WINDOW", "record.number - n <= HISTORY_SERVE_WINDOW + 1"),
    ("M80", A, "history window includes the record's own block",
     "    if !(n < record.number && ", "    if !(n <= record.number && "),
    ("M81", A, "history contract's code not checked",
     "    if contract.code_hash != HISTORY_CODE_HASH {\n        return Err(Error::HistoryContractMissing);\n    }\n", ""),
    ("M82", A, "history slot only required to be non-zero",
     "    if prove_storage(&contract, &slot, storage_proof)? != block_hash {",
     "    if prove_storage(&contract, &slot, storage_proof)? == [0; 32] {"),
    ("M83", A, "history slot taken modulo 8192",
     "(n % HISTORY_SERVE_WINDOW).to_be_bytes()", "(n % 8192).to_be_bytes()"),
    ("M84", A, "hops do not accumulate",
     "        Binding::History { hops } => hops.saturating_add(1),", "        Binding::History { hops } => hops,"),
    ("M85", A, "restored history record may have zero hops",
     "            (2, h) if h > 0 => Binding::History { hops: h },", "            (2, h) => Binding::History { hops: h },"),
    ("M86", A, "record bytes may be longer than the record",
     "        if bytes.len() != Self::SSZ_LEN {", "        if bytes.len() < Self::SSZ_LEN {"),
    ("M87", A, "empty payload header accepted as a record",
     "        if payload.block_hash == ZERO {\n            return Err(Error::BindingMismatch);\n        }\n", ""),
    ("M88", A, "<= compares as <",
     "            Cmp::Le => ord.is_le(),", "            Cmp::Le => ord.is_lt(),"),
    ("M89", A, "conditions not checked",
     "            if !c.predicate.holds(&self.field(c.field)?) {", "            if false && !c.predicate.holds(&self.field(c.field)?) {"),
    ("M90", A, "an account's conditions skipped in a batch",
     "        let fact = Fact::Account(account.fact);\n        fact.check(q.conditions)?;\n",
     "        let fact = Fact::Account(account.fact);\n"),
    ("M91", A, "empty batch accepted",
     "    if batch.accounts.is_empty() && batch.logs.is_empty() {\n        return Err(Error::QueryInvalid);\n    }\n", ""),
    ("M92", A, "storage identifier without the value",
     "statement(KIND_STORAGE, chain_id);\n    h.update(address);\n    h.update(slot);\n    h.update(value);\n",
     "statement(KIND_STORAGE, chain_id);\n    h.update(address);\n    h.update(slot);\n"),
    ("M93", A, "log identifier without the log's position",
     "    h.update(log_index.to_be_bytes());\n", ""),
    # Accumulator facts.
    ("M94", ACC, "a path of fewer than 32 siblings accepted",
     "    if path.len() != ACCUMULATOR_DEPTH {", "    if path.len() > ACCUMULATOR_DEPTH {"),
    ("M95", ACC, "leaf index bound at 2^33",
     "    if leaf_index >> ACCUMULATOR_DEPTH != 0 {", "    if leaf_index >> (ACCUMULATOR_DEPTH + 1) != 0 {"),
    ("M96", ACC, "direction bits read the other way",
     "        node = if (leaf_index >> height) & 1 == 1 {", "        node = if (leaf_index >> height) & 1 == 0 {"),
    ("M97", ACC, "inner nodes without the 0x01 prefix",
     "    h.update([0x01]);", "    h.update([0x00]);"),
    ("M98", ACC, "the empty leaf accepted",
     "    if *leaf == [0u8; 32] {\n        return Err(Error::AccumulatorLeafEmpty);\n    }\n", ""),
    ("M99", ACC, "root not compared",
     "    if computed != root.root {", "    if false {"),
    ("M100", ACC, "leaf identifier without the index",
     "    h.update(leaf_index.to_be_bytes());\n    h.update(leaf);", "    h.update(leaf);"),
    ("M101", ACC, "leaf identifier with an 8-byte chain id",
     "    h.update(chain);", "    h.update(chain_id.to_be_bytes());"),
    # Log-index conversion, relations, block-bound identifiers, anchor slot.
    ("M102", X, "locate_log off by one at a receipt's end",
     "        if block_log_index < end {", "        if block_log_index <= end {"),
    ("M103", X, "block_log_index accepts one log past a receipt",
     "    if position >= *log_counts.get(tx)? {", "    if position > *log_counts.get(tx)? {"),
    ("M104", A, "relations not checked",
     "        if !(Predicate { cmp: r.cmp, rhs: value(&r.right)? }).holds(&value(&r.left)?) {",
     "        if false && !(Predicate { cmp: r.cmp, rhs: value(&r.right)? }).holds(&value(&r.left)?) {"),
    ("M105", A, "block-bound storage identifier without the block",
     "    let mut h = statement(KIND_STORAGE_AT_BLOCK, chain_id);\n    h.update(block_number.to_be_bytes());\n",
     "    let mut h = statement(KIND_STORAGE_AT_BLOCK, chain_id);\n"),
    ("M106", A, "block-bound storage identifier shares the default kind",
     "const KIND_STORAGE_AT_BLOCK: u8 = 0x12;", "const KIND_STORAGE_AT_BLOCK: u8 = 2;"),
    ("M107", A, "a hop loses the anchor slot",
     "        anchor_slot: record.anchor_slot,", "        anchor_slot: 0,"),
    ("M108", P, "the store checks no weak-subjectivity bound",
     "        if let Some(period) = self.ws_period {\n            ws::check_within(self.core.finalized_header.beacon.slot, current_slot, period, ctx.spec)?;\n        }\n", ""),
    ("M109", P, "with_weak_subjectivity_period sets nothing",
     "        self.ws_period = Some(epochs);\n", ""),
    ("M110", WS, "the last epoch of the period is outside",
     "        Some(last_epoch) => spec.epoch_at_slot(current_slot) <= last_epoch,",
     "        Some(last_epoch) => spec.epoch_at_slot(current_slot) < last_epoch,"),
    ("M111", WS, "an end past u64 counts as reached",
     "        Some(last_epoch) => spec.epoch_at_slot(current_slot) <= last_epoch,\n        None => true,",
     "        Some(last_epoch) => spec.epoch_at_slot(current_slot) <= last_epoch,\n        None => false,"),
    ("M112", WS, "the end slot one epoch early",
     "    spec.epoch_at_slot(ws_slot).checked_add(ws_period)?.checked_add(1)?.checked_mul(spec.preset.slots_per_epoch)",
     "    spec.epoch_at_slot(ws_slot).checked_add(ws_period)?.checked_mul(spec.preset.slots_per_epoch)"),
    ("M113", WS, "the balance churn not rounded down to an increment",
     "    churn.checked_sub(churn.checked_rem(c.effective_balance_increment)?)", "    Some(churn)"),
    ("M114", WS, "Electra's period without the factor 2",
     "        c.safety_decay.checked_mul(t)?.checked_div(delta.checked_mul(2)?.checked_mul(100)?)?;",
     "        c.safety_decay.checked_mul(t)?.checked_div(delta.checked_mul(100)?)?;"),
    ("M115", WS, "phase 0's period ignores the balance top-ups",
     "        max(churn, top_ups)", "        churn"),
]


def sh(cmd, env, cwd):
    p = subprocess.run(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    return p.returncode, p.stdout


def main():
    only = set(sys.argv[1:])
    shutil.rmtree(COPY, ignore_errors=True)
    os.makedirs(COPY)
    for name in ["Cargo.toml", "Cargo.lock", "rustfmt.toml"]:
        shutil.copy(os.path.join(ROOT, name), COPY)
    # Plain copies get fresh modification times, so cargo never reuses a build of an earlier
    # mutant left in target/mutants/target.
    shutil.copytree(os.path.join(ROOT, "crates"), os.path.join(COPY, "crates"), copy_function=shutil.copy)
    # The committed vector file the accumulator tests read (the downloaded vectors come by env).
    os.makedirs(os.path.join(COPY, "vectors"))
    shutil.copy(os.path.join(ROOT, "vectors", "acc-leaf.json"), os.path.join(COPY, "vectors"))
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = os.path.join(WORK, "target")
    env["TMPDIR"] = os.path.join(WORK, "tmp")
    os.makedirs(env["TMPDIR"], exist_ok=True)
    vectors = os.path.join(ROOT, "vectors")
    env.setdefault("FERRATA_VECTORS", os.path.join(vectors, "v1.7.0-beta.4", "tests", "minimal"))
    env.setdefault("FERRATA_BLS_VECTORS", os.path.join(vectors, "bls12-381-tests-v0.1.2"))
    env.setdefault("FERRATA_TRIE_VECTORS", os.path.join(vectors, "ethereum-tests-c67e485", "TrieTests"))
    test = ["cargo", "test", "--workspace", "--offline", "--locked", "--no-fail-fast"]
    rc, out = sh(test, env, COPY)
    if rc != 0:
        sys.exit("the unmutated copy fails its tests:\n" + out[-3000:])
    killed = survived = 0
    for mid, rel, desc, old, new in MUTANTS:
        if only and mid not in only:
            continue
        pristine_path = os.path.join(ROOT, CORE, rel)
        target = os.path.join(COPY, CORE, rel)
        pristine = open(pristine_path).read()
        if pristine.count(old) != 1:
            print(f"{mid} SKIP   snippet occurs {pristine.count(old)} times in {rel}", flush=True)
            continue
        open(target, "w").write(pristine.replace(old, new))
        started = time.time()
        try:
            rc, out = sh(test, env, COPY)
        finally:
            open(target, "w").write(pristine)
        if "error[E" in out:
            verdict = "COMPILE"
        elif rc == 0:
            verdict, survived = "SURVIVED", survived + 1
        else:
            verdict, killed = "killed", killed + 1
        failing = [l.split()[1] for l in out.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
        print(f"{mid} {verdict:8} {rel:24} {desc}  [{', '.join(failing[:3])}] ({time.time() - started:.0f}s)", flush=True)
    print(f"\n{killed} killed, {survived} survived")


if __name__ == "__main__":
    main()
