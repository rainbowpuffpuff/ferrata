//! The sync protocol's pure functions, transcribed from the light-client specs
//! (consensus-specs v1.7.0-beta.4). Each takes the store's layout (`LcFork`), because a store's
//! fork decides how headers are validated, exactly as in the spec.

use crate::{
    bls::BlsBackend,
    fork::{ChainSpec, Fork, LcFork},
    hash::hash_pair,
    merkle::{
        compute_merkle_branch_root, floorlog2, is_valid_merkle_branch, is_valid_normalized_merkle_branch,
        is_zero_branch, subtree_index, BLOCK_HASH_IN_CAPELLA_PAYLOAD_GINDEX, BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX,
        CURRENT_SYNC_COMMITTEE_GINDEX, CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS,
        EXECUTION_BLOCK_HASH_GINDEX, EXECUTION_BLOCK_HASH_GINDEX_DENEB, EXECUTION_BLOCK_HASH_GINDEX_GLOAS,
        EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA, FINALIZED_ROOT_GINDEX_GLOAS,
        NEXT_SYNC_COMMITTEE_GINDEX, NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, NEXT_SYNC_COMMITTEE_GINDEX_GLOAS, ZERO,
    },
    store::StoreCore,
    types::{Execution, ExecutionPayloadHeader, LightClientHeader, LightClientUpdate},
    Error, Root,
};

/// `DOMAIN_SYNC_COMMITTEE`.
pub const DOMAIN_SYNC_COMMITTEE: [u8; 4] = [0x07, 0x00, 0x00, 0x00];
/// `GENESIS_SLOT`.
pub const GENESIS_SLOT: u64 = 0;

/// Which BLS path verifies the sync-committee signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignaturePath {
    /// The spec's `FastAggregateVerify`, with `KeyValidate` on every signer key (spec store).
    Spec,
    /// For committees that passed `check_committee` when the store adopted them (policy store).
    CheckedCommittee,
}

/// Everything an update is verified against, besides the store.
pub struct Context<'a> {
    /// The instance's chain configuration.
    pub spec: &'a ChainSpec,
    /// `genesis_validators_root` of the chain (part of the signing domain).
    pub genesis_validators_root: Root,
    /// BLS implementation.
    pub bls: &'a dyn BlsBackend,
}

/// `compute_fork_data_root`.
pub fn compute_fork_data_root(version: [u8; 4], genesis_validators_root: &Root) -> Root {
    let mut v = ZERO;
    v[..4].copy_from_slice(&version);
    hash_pair(&v, genesis_validators_root)
}

/// `compute_domain`.
pub fn compute_domain(domain_type: [u8; 4], version: [u8; 4], genesis_validators_root: &Root) -> Root {
    let fork_data_root = compute_fork_data_root(version, genesis_validators_root);
    let mut d = ZERO;
    d[..4].copy_from_slice(&domain_type);
    d[4..].copy_from_slice(&fork_data_root[..28]);
    d
}

/// `compute_signing_root` for an object given by its hash-tree root.
pub fn compute_signing_root(object_root: &Root, domain: &Root) -> Root {
    hash_pair(object_root, domain)
}

/// `finalized_root_gindex_at_slot`.
pub fn finalized_root_gindex_at_slot(slot: u64, fork: LcFork, spec: &ChainSpec) -> u64 {
    match fork {
        LcFork::Gloas if spec.is_at_or_after(slot, Fork::Gloas) => FINALIZED_ROOT_GINDEX_GLOAS,
        LcFork::Electra | LcFork::Gloas if spec.is_at_or_after(slot, Fork::Electra) => FINALIZED_ROOT_GINDEX_ELECTRA,
        _ => FINALIZED_ROOT_GINDEX,
    }
}

/// `current_sync_committee_gindex_at_slot`.
pub fn current_sync_committee_gindex_at_slot(slot: u64, fork: LcFork, spec: &ChainSpec) -> u64 {
    match fork {
        LcFork::Gloas if spec.is_at_or_after(slot, Fork::Gloas) => CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS,
        LcFork::Electra | LcFork::Gloas if spec.is_at_or_after(slot, Fork::Electra) => {
            CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA
        }
        _ => CURRENT_SYNC_COMMITTEE_GINDEX,
    }
}

/// `next_sync_committee_gindex_at_slot`.
pub fn next_sync_committee_gindex_at_slot(slot: u64, fork: LcFork, spec: &ChainSpec) -> u64 {
    match fork {
        LcFork::Gloas if spec.is_at_or_after(slot, Fork::Gloas) => NEXT_SYNC_COMMITTEE_GINDEX_GLOAS,
        LcFork::Electra | LcFork::Gloas if spec.is_at_or_after(slot, Fork::Electra) => {
            NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA
        }
        _ => NEXT_SYNC_COMMITTEE_GINDEX,
    }
}

/// Root of the execution payload header that a Gloas-layout branch proves `block_hash` in.
fn payload_root_from_block_hash(block_hash: &Root, branch: &[Root], payload_gindex: u64) -> Root {
    let inner = &branch[..branch.len().saturating_sub(floorlog2(EXECUTION_PAYLOAD_GINDEX))];
    let depth = floorlog2(payload_gindex);
    let tail = &inner[inner.len().saturating_sub(depth)..];
    compute_merkle_branch_root(block_hash, tail, depth, subtree_index(payload_gindex)).unwrap_or(ZERO)
}

/// `get_lc_execution_root`: the execution root the header binds, or zero before Capella.
pub fn get_lc_execution_root(header: &LightClientHeader, fork: LcFork, spec: &ChainSpec) -> Root {
    let slot = header.beacon.slot;
    match &header.execution {
        Execution::None => ZERO,
        Execution::Payload { header: payload, .. } => {
            if fork >= LcFork::Deneb && spec.is_at_or_after(slot, Fork::Deneb) {
                payload.hash_tree_root_deneb()
            } else if spec.is_at_or_after(slot, Fork::Capella) {
                payload.hash_tree_root_capella()
            } else {
                ZERO
            }
        }
        Execution::BlockHash { block_hash, branch } => {
            if spec.is_at_or_after(slot, Fork::Gloas) {
                *block_hash
            } else if spec.is_at_or_after(slot, Fork::Deneb) {
                if slot == GENESIS_SLOT {
                    ExecutionPayloadHeader::empty().hash_tree_root_deneb()
                } else {
                    payload_root_from_block_hash(block_hash, branch, BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX)
                }
            } else if spec.is_at_or_after(slot, Fork::Capella) {
                if slot == GENESIS_SLOT {
                    ExecutionPayloadHeader::empty().hash_tree_root_capella()
                } else {
                    payload_root_from_block_hash(block_hash, branch, BLOCK_HASH_IN_CAPELLA_PAYLOAD_GINDEX)
                }
            } else {
                ZERO
            }
        }
    }
}

/// `is_valid_light_client_header`: the header's execution binding verifies against its
/// `body_root` (or is empty before Capella).
pub fn is_valid_light_client_header(header: &LightClientHeader, fork: LcFork, spec: &ChainSpec) -> bool {
    if !header.has_layout(fork) {
        return false;
    }
    let slot = header.beacon.slot;
    match &header.execution {
        Execution::None => true,
        Execution::Payload { header: payload, branch } => {
            let has_blob_gas = payload.blob_gas_used != 0 || payload.excess_blob_gas != 0;
            if has_blob_gas && (fork == LcFork::Capella || !spec.is_at_or_after(slot, Fork::Deneb)) {
                return false;
            }
            if !spec.is_at_or_after(slot, Fork::Capella) {
                return payload.is_empty() && is_zero_branch(branch);
            }
            is_valid_merkle_branch(
                &get_lc_execution_root(header, fork, spec),
                branch,
                floorlog2(EXECUTION_PAYLOAD_GINDEX),
                subtree_index(EXECUTION_PAYLOAD_GINDEX),
                &header.beacon.body_root,
            )
        }
        Execution::BlockHash { block_hash, branch } => {
            let gindex = if spec.is_at_or_after(slot, Fork::Gloas) {
                EXECUTION_BLOCK_HASH_GINDEX_GLOAS
            } else if spec.is_at_or_after(slot, Fork::Deneb) {
                EXECUTION_BLOCK_HASH_GINDEX_DENEB
            } else if spec.is_at_or_after(slot, Fork::Capella) {
                EXECUTION_BLOCK_HASH_GINDEX
            } else {
                return *block_hash == ZERO && is_zero_branch(branch);
            };
            is_valid_normalized_merkle_branch(block_hash, branch, gindex, &header.beacon.body_root)
        }
    }
}

/// `is_sync_committee_update`.
pub fn is_sync_committee_update(update: &LightClientUpdate) -> bool {
    !is_zero_branch(&update.next_sync_committee_branch)
}

/// `is_finality_update`.
pub fn is_finality_update(update: &LightClientUpdate) -> bool {
    !is_zero_branch(&update.finality_branch)
}

/// `is_better_update`: the canonical ranking of updates.
pub fn is_better_update(new: &LightClientUpdate, old: &LightClientUpdate, spec: &ChainSpec) -> bool {
    let period = |slot| spec.period_at_slot(slot);
    // Compare supermajority (> 2/3) sync committee participation
    let max_active = new.sync_aggregate.bits.len() as u64;
    let new_active = new.sync_aggregate.participants();
    let old_active = old.sync_aggregate.participants();
    let new_super = new_active * 3 >= max_active * 2;
    let old_super = old_active * 3 >= max_active * 2;
    if new_super != old_super {
        return new_super;
    }
    if !new_super && new_active != old_active {
        return new_active > old_active;
    }
    // Compare presence of relevant sync committee
    let relevant = |u: &LightClientUpdate| {
        is_sync_committee_update(u) && period(u.attested_header.beacon.slot) == period(u.signature_slot)
    };
    let (new_rel, old_rel) = (relevant(new), relevant(old));
    if new_rel != old_rel {
        return new_rel;
    }
    // Compare indication of any finality
    let (new_fin, old_fin) = (is_finality_update(new), is_finality_update(old));
    if new_fin != old_fin {
        return new_fin;
    }
    // Compare sync committee finality
    if new_fin {
        let sc_fin =
            |u: &LightClientUpdate| period(u.finalized_header.beacon.slot) == period(u.attested_header.beacon.slot);
        let (new_scf, old_scf) = (sc_fin(new), sc_fin(old));
        if new_scf != old_scf {
            return new_scf;
        }
    }
    // Tiebreaker 1: Sync committee participation beyond supermajority
    if new_active != old_active {
        return new_active > old_active;
    }
    // Tiebreaker 2: Prefer older data (fewer changes to best)
    if new.attested_header.beacon.slot != old.attested_header.beacon.slot {
        return new.attested_header.beacon.slot < old.attested_header.beacon.slot;
    }
    // Tiebreaker 3: Prefer updates with earlier signature slots
    new.signature_slot < old.signature_slot
}

/// The signing root a sync committee signs for `update`, with the fork version of the slot
/// before `signature_slot`.
pub fn sync_committee_signing_root(update: &LightClientUpdate, ctx: &Context<'_>) -> Root {
    let fork_version_slot = update.signature_slot.saturating_sub(1);
    let fork_version = ctx.spec.compute_fork_version(ctx.spec.epoch_at_slot(fork_version_slot));
    let domain = compute_domain(DOMAIN_SYNC_COMMITTEE, fork_version, &ctx.genesis_validators_root);
    compute_signing_root(&update.attested_header.beacon.hash_tree_root(), &domain)
}

/// `validate_light_client_update`. Pure: it never changes the store.
pub fn validate_light_client_update(
    store: &StoreCore,
    update: &LightClientUpdate,
    current_slot: u64,
    ctx: &Context<'_>,
    path: SignaturePath,
) -> Result<(), Error> {
    let spec = ctx.spec;
    let fork = store.fork;
    let n = spec.preset.sync_committee_size;

    // Shape: the update must be in the store's layout and committee size.
    if !update.attested_header.has_layout(fork)
        || !update.finalized_header.has_layout(fork)
        || update.next_sync_committee_branch.len() != fork.sync_committee_branch_len()
        || update.finality_branch.len() != fork.finality_branch_len()
    {
        return Err(Error::ForkMismatch);
    }
    if update.sync_aggregate.bits.len() != n || update.next_sync_committee.pubkeys.len() != n {
        return Err(Error::Malformed);
    }

    // Verify sync committee has sufficient participants
    let sync_aggregate = &update.sync_aggregate;
    if sync_aggregate.participants() < spec.preset.min_sync_committee_participants {
        return Err(Error::ParticipationLow);
    }

    // Verify update does not skip a sync committee period
    if !is_valid_light_client_header(&update.attested_header, fork, spec) {
        return Err(Error::AttestedHeaderInvalid);
    }
    let update_attested_slot = update.attested_header.beacon.slot;
    let update_finalized_slot = update.finalized_header.beacon.slot;
    if !(current_slot >= update.signature_slot
        && update.signature_slot > update_attested_slot
        && update_attested_slot >= update_finalized_slot)
    {
        return Err(Error::SlotOrderInvalid);
    }
    let store_period = spec.period_at_slot(store.finalized_header.beacon.slot);
    let update_signature_period = spec.period_at_slot(update.signature_slot);
    let next_known = store.is_next_sync_committee_known();
    let period_ok = if next_known {
        update_signature_period == store_period || update_signature_period == store_period + 1
    } else {
        update_signature_period == store_period
    };
    if !period_ok {
        return Err(Error::PeriodSkipped);
    }

    // Verify update is relevant
    let update_attested_period = spec.period_at_slot(update_attested_slot);
    let update_has_next_sync_committee =
        !next_known && is_sync_committee_update(update) && update_attested_period == store_period;
    if !(update_attested_slot > store.finalized_header.beacon.slot || update_has_next_sync_committee) {
        return Err(Error::UpdateIrrelevant);
    }

    // Verify that the `finality_branch`, if present, confirms `finalized_header` to match the
    // finalized checkpoint root saved in the state of `attested_header`. The genesis finalized
    // checkpoint root is represented as a zero hash.
    if !is_finality_update(update) {
        if !update.finalized_header.is_empty() {
            return Err(Error::FinalizedHeaderInvalid);
        }
    } else {
        let finalized_root = if update_finalized_slot == GENESIS_SLOT {
            if !update.finalized_header.is_empty() {
                return Err(Error::FinalizedHeaderInvalid);
            }
            ZERO
        } else {
            if !is_valid_light_client_header(&update.finalized_header, fork, spec) {
                return Err(Error::FinalizedHeaderInvalid);
            }
            update.finalized_header.beacon.hash_tree_root()
        };
        if !is_valid_normalized_merkle_branch(
            &finalized_root,
            &update.finality_branch,
            finalized_root_gindex_at_slot(update_attested_slot, fork, spec),
            &update.attested_header.beacon.state_root,
        ) {
            return Err(Error::FinalityBranchInvalid);
        }
    }

    // Verify that the `next_sync_committee`, if present, actually is the next sync committee
    // saved in the state of the `attested_header`
    if !is_sync_committee_update(update) {
        if !update.next_sync_committee.is_empty() {
            return Err(Error::NextCommitteeUnexpected);
        }
    } else {
        if update_attested_period == store_period
            && next_known
            && update.next_sync_committee != store.next_sync_committee
        {
            return Err(Error::NextCommitteeMismatch);
        }
        if !is_valid_normalized_merkle_branch(
            &update.next_sync_committee.hash_tree_root(),
            &update.next_sync_committee_branch,
            next_sync_committee_gindex_at_slot(update_attested_slot, fork, spec),
            &update.attested_header.beacon.state_root,
        ) {
            return Err(Error::NextCommitteeBranchInvalid);
        }
    }

    // Verify sync committee aggregate signature
    let sync_committee = if update_signature_period == store_period {
        &store.current_sync_committee
    } else {
        &store.next_sync_committee
    };
    let signing_root = sync_committee_signing_root(update, ctx);
    let (bits, signature) = (&sync_aggregate.bits, &sync_aggregate.signature);
    match path {
        SignaturePath::Spec => ctx.bls.fast_aggregate_verify(sync_committee, bits, &signing_root, signature),
        SignaturePath::CheckedCommittee => ctx.bls.verify_checked(sync_committee, bits, &signing_root, signature),
    }
}
