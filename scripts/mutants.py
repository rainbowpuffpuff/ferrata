#!/usr/bin/env python3
"""Hand-picked mutants for plumbline-core.

Each mutant replaces one exact snippet in a scratch copy of the workspace (under
target/mutants/), runs the whole test suite and records whether a test fails (killed) or all
pass (survived). The repository itself is only read. M01-M32 were proposed by an independent
review of the first version of this code; M33 onward cover code added since.

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
CORE = "crates/plumbline-core/src/"

V, S, P, B, T, U, M = "validate.rs", "store.rs", "policy.rs", "bls/blst_backend.rs", "types.rs", "upgrade.rs", "merkle.rs"
Z = "bls/zkcrypto_backend.rs"

MUTANTS = [
    ("M01", P, "no committee check at initialisation",
     "        let core = StoreCore::from_bootstrap(trusted_block_root, bootstrap, fork, spec)?;\n        bls.check_committee(&core.current_sync_committee)?;\n        Ok(Self { core })",
     "        let core = StoreCore::from_bootstrap(trusted_block_root, bootstrap, fork, spec)?;\n        Ok(Self { core })"),
    ("M02", P, "no committee check at adoption",
     "        if outcome.adopted {\n            ctx.bls.check_committee(&next.next_sync_committee)?;\n        }\n", ""),
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
    ("M33", B, "blst KeyValidate without identity and subgroup checks",
     "    if unsafe { blst_p1_affine_is_inf(&point) || !blst_p1_affine_in_g1(&point) } {", "    if false {"),
    ("M34", B, "blst checked path without the identity check",
     "            accumulate(&mut key, members(true), uncompress)?;\n        }\n        if is_identity(&key) {\n            return Err(Error::SigningKeyIdentity);\n        }",
     "            accumulate(&mut key, members(true), uncompress)?;\n        }"),
    ("M35", B, "blst check_committee ignores the aggregate",
     "        if sum_affine == aggregate {", "        if true {"),
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
    shutil.copytree(os.path.join(ROOT, "crates"), os.path.join(COPY, "crates"))
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = os.path.join(WORK, "target")
    env["TMPDIR"] = os.path.join(WORK, "tmp")
    os.makedirs(env["TMPDIR"], exist_ok=True)
    vectors = os.path.join(ROOT, "vectors")
    env.setdefault("PLUMBLINE_VECTORS", os.path.join(vectors, "v1.7.0-beta.4", "tests", "minimal"))
    env.setdefault("PLUMBLINE_BLS_VECTORS", os.path.join(vectors, "bls12-381-tests-v0.1.2"))
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
