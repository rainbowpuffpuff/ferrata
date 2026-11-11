//! Light-client stores.
//!
//! [`StoreCore`] is the state both stores share: the finalized header and the two committees.
//! [`LightClientStore`] adds the spec's optimistic header, best-update cache and forced updates;
//! it exists to replay the official test vectors step for step. Ferrata's on-chain policy,
//! which never accepts a non-finalized or forced header, is [`crate::policy::FinalizedStore`].
//! Both run the same [`crate::validate::validate_light_client_update`] and the same
//! [`apply_core`].

use core::cmp::max;

use crate::{
    fork::{ChainSpec, LcFork},
    merkle::is_valid_normalized_merkle_branch,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncCommittee},
    upgrade::{upgrade_header, upgrade_update},
    validate::{
        current_sync_committee_gindex_at_slot, is_better_update, is_finality_update, is_sync_committee_update,
        is_valid_light_client_header, validate_light_client_update, Context, SignaturePath,
    },
    Error, Root,
};

/// State shared by every store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreCore {
    /// The layout the store's headers are in.
    pub fork: LcFork,
    pub finalized_header: LightClientHeader,
    pub current_sync_committee: SyncCommittee,
    pub next_sync_committee: SyncCommittee,
}

impl StoreCore {
    /// `is_next_sync_committee_known`.
    pub fn is_next_sync_committee_known(&self) -> bool {
        !self.next_sync_committee.is_empty()
    }

    /// The checks of `initialize_light_client_store`, in spec order.
    pub fn from_bootstrap(
        trusted_block_root: &Root,
        bootstrap: &LightClientBootstrap,
        fork: LcFork,
        spec: &ChainSpec,
    ) -> Result<Self, Error> {
        if !bootstrap.header.has_layout(fork)
            || bootstrap.current_sync_committee_branch.len() != fork.sync_committee_branch_len()
        {
            return Err(Error::ForkMismatch);
        }
        if bootstrap.current_sync_committee.pubkeys.len() != spec.preset.sync_committee_size {
            return Err(Error::Malformed);
        }
        if !is_valid_light_client_header(&bootstrap.header, fork, spec) {
            return Err(Error::BootstrapHeaderInvalid);
        }
        if bootstrap.header.beacon.hash_tree_root() != *trusted_block_root {
            return Err(Error::CheckpointMismatch);
        }
        if !is_valid_normalized_merkle_branch(
            &bootstrap.current_sync_committee.hash_tree_root(),
            &bootstrap.current_sync_committee_branch,
            current_sync_committee_gindex_at_slot(bootstrap.header.beacon.slot, fork, spec),
            &bootstrap.header.beacon.state_root,
        ) {
            return Err(Error::CurrentCommitteeBranchInvalid);
        }
        Ok(Self {
            fork,
            finalized_header: bootstrap.header.clone(),
            current_sync_committee: bootstrap.current_sync_committee.clone(),
            next_sync_committee: SyncCommittee::empty(spec.preset.sync_committee_size),
        })
    }

    pub(crate) fn upgrade(&mut self, to: LcFork, spec: &ChainSpec) -> Result<(), Error> {
        self.finalized_header = upgrade_header(self.finalized_header.clone(), self.fork, to, spec)?;
        self.fork = to;
        Ok(())
    }

    /// True if a supermajority update finalizing `update` should be applied: it advances the
    /// finalized header, or it supplies the missing next committee for the store period.
    pub fn update_advances(&self, update: &LightClientUpdate, spec: &ChainSpec) -> bool {
        let update_has_finalized_next_sync_committee = !self.is_next_sync_committee_known()
            && is_sync_committee_update(update)
            && is_finality_update(update)
            && spec.period_at_slot(update.finalized_header.beacon.slot)
                == spec.period_at_slot(update.attested_header.beacon.slot);
        update.finalized_header.beacon.slot > self.finalized_header.beacon.slot
            || update_has_finalized_next_sync_committee
    }
}

/// What [`apply_core`] changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// The store moved to the next sync committee period.
    pub rotated: bool,
    /// The finalized header advanced.
    pub advanced: bool,
    /// The store took the update's (non-empty) next sync committee as its own.
    pub adopted: bool,
}

/// `apply_light_client_update`, minus the optimistic-header and participation bookkeeping
/// that only the spec store keeps. Checks its precondition before changing anything.
pub fn apply_core(core: &mut StoreCore, update: &LightClientUpdate, spec: &ChainSpec) -> Result<ApplyOutcome, Error> {
    let store_period = spec.period_at_slot(core.finalized_header.beacon.slot);
    let update_finalized_period = spec.period_at_slot(update.finalized_header.beacon.slot);
    let mut outcome = ApplyOutcome::default();
    if !core.is_next_sync_committee_known() {
        if update_finalized_period != store_period {
            return Err(Error::CommitteeUnknown);
        }
        core.next_sync_committee = update.next_sync_committee.clone();
        outcome.adopted = !update.next_sync_committee.is_empty();
    } else if update_finalized_period == store_period + 1 {
        core.current_sync_committee =
            core::mem::replace(&mut core.next_sync_committee, update.next_sync_committee.clone());
        outcome.rotated = true;
        outcome.adopted = !core.next_sync_committee.is_empty();
    }
    if update.finalized_header.beacon.slot > core.finalized_header.beacon.slot {
        core.finalized_header = update.finalized_header.clone();
        outcome.advanced = true;
    }
    Ok(outcome)
}

/// The spec's `LightClientStore`, with `process_light_client_update`,
/// `process_light_client_store_force_update` and store upgrades. Every method is atomic: on
/// error the store is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightClientStore {
    pub core: StoreCore,
    pub best_valid_update: Option<LightClientUpdate>,
    pub optimistic_header: LightClientHeader,
    pub previous_max_active_participants: u64,
    pub current_max_active_participants: u64,
}

impl LightClientStore {
    /// `initialize_light_client_store`.
    pub fn initialize(
        trusted_block_root: &Root,
        bootstrap: &LightClientBootstrap,
        fork: LcFork,
        spec: &ChainSpec,
    ) -> Result<Self, Error> {
        let core = StoreCore::from_bootstrap(trusted_block_root, bootstrap, fork, spec)?;
        Ok(Self {
            optimistic_header: core.finalized_header.clone(),
            core,
            best_valid_update: None,
            previous_max_active_participants: 0,
            current_max_active_participants: 0,
        })
    }

    /// `get_safety_threshold`.
    pub fn safety_threshold(&self) -> u64 {
        max(self.previous_max_active_participants, self.current_max_active_participants) / 2
    }

    fn apply(&mut self, update: &LightClientUpdate, spec: &ChainSpec) -> Result<(), Error> {
        let outcome = apply_core(&mut self.core, update, spec)?;
        if outcome.rotated {
            self.previous_max_active_participants = self.current_max_active_participants;
            self.current_max_active_participants = 0;
        }
        if outcome.advanced && self.core.finalized_header.beacon.slot > self.optimistic_header.beacon.slot {
            self.optimistic_header = self.core.finalized_header.clone();
        }
        Ok(())
    }

    /// `process_light_client_update`.
    pub fn process_update(
        &mut self,
        update: &LightClientUpdate,
        current_slot: u64,
        ctx: &Context<'_>,
    ) -> Result<(), Error> {
        validate_light_client_update(&self.core, update, current_slot, ctx, SignaturePath::Spec)?;
        let spec = ctx.spec;
        let mut next = self.clone();
        let participants = update.sync_aggregate.participants();

        // Update the best update in case we have to force-update to it if the timeout elapses
        let better = match &next.best_valid_update {
            None => true,
            Some(best) => is_better_update(update, best, spec),
        };
        if better {
            next.best_valid_update = Some(update.clone());
        }

        // Track the maximum number of active participants in the committee signatures
        next.current_max_active_participants = max(next.current_max_active_participants, participants);

        // Update the optimistic header
        if participants > next.safety_threshold()
            && update.attested_header.beacon.slot > next.optimistic_header.beacon.slot
        {
            next.optimistic_header = update.attested_header.clone();
        }

        // Update finalized header: normal update through 2/3 threshold
        let n = update.sync_aggregate.bits.len() as u64;
        if participants * 3 >= n * 2 && next.core.update_advances(update, spec) {
            next.apply(update, spec)?;
            next.best_valid_update = None;
        }
        *self = next;
        Ok(())
    }

    /// `process_light_client_store_force_update`. Ferrata's on-chain policy never calls this.
    pub fn process_force_update(&mut self, current_slot: u64, spec: &ChainSpec) -> Result<(), Error> {
        // The spec's integers are unbounded: a deadline past u64::MAX is never reached (and
        // must not wrap around to an early one).
        let timed_out = self
            .core
            .finalized_header
            .beacon
            .slot
            .checked_add(spec.preset.update_timeout())
            .is_some_and(|deadline| current_slot > deadline);
        if !timed_out || self.best_valid_update.is_none() {
            return Ok(());
        }
        let mut next = self.clone();
        let mut best = next.best_valid_update.take().ok_or(Error::CommitteeUnknown)?;
        // Forced best update when the update timeout has elapsed: the attested header may be
        // treated as finalized to guarantee progression into later sync committee periods.
        if best.finalized_header.beacon.slot <= next.core.finalized_header.beacon.slot {
            best.finalized_header = best.attested_header.clone();
        }
        next.apply(&best, spec)?;
        *self = next;
        Ok(())
    }

    /// `upgrade_lc_store_to_*`, composed up to `to`.
    pub fn upgrade(&mut self, to: LcFork, spec: &ChainSpec) -> Result<(), Error> {
        let from = self.core.fork;
        let mut next = self.clone();
        next.core.upgrade(to, spec)?;
        next.optimistic_header = upgrade_header(next.optimistic_header, from, to, spec)?;
        next.best_valid_update = match next.best_valid_update {
            None => None,
            Some(u) => Some(upgrade_update(u, from, to, spec)?),
        };
        *self = next;
        Ok(())
    }
}
