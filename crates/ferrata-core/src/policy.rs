//! Ferrata's on-chain policy.
//!
//! A header enters only through a finality proof signed by at least two thirds of the sync
//! committee. There is no optimistic header, no best-update cache and no forced update, so a
//! header in the store is always one the committee finalized. Validation is the spec's
//! [`validate_light_client_update`], and state changes are the spec's [`apply_core`]; the
//! policy only refuses more. Each committee the store adopts (at initialisation and at each
//! handoff) is checked once with [`BlsBackend::check_committee`]. After that, the per-update
//! check [`BlsBackend::verify_checked`] returns exactly what the spec's `FastAggregateVerify`
//! would, whichever signing-key composition the backend uses.
//!
//! **Liveness limit.** A store that accepts only finalized headers cannot cross a sync-committee
//! period in which Ethereum finalizes nothing it can see. The next committee is learned only from
//! a finalized update, so once updates are signed two periods ahead the store refuses them
//! (`PeriodSkipped`). The spec's light client keeps going in that case by forcing a non-finalized
//! header, which Ferrata will not do. Such an instance stops safely and is replaced from a
//! fresh checkpoint (`stops_safely_after_a_period_without_finality` in the tests).
//!
//! **Weak subjectivity.** A store given a period with
//! [`FinalizedStore::with_weak_subjectivity_period`] refuses every update once the current slot is
//! past the weak-subjectivity period of its finalized header (`OutsideWeakSubjectivityPeriod`,
//! [`crate::ws`]), however validly signed: the committee it trusts may no longer stand on stake.
//! Such a store is not repaired; a new one is initialised from a trusted checkpoint inside the
//! period. Without a period the store checks no bound (the official vectors run so).

use crate::{
    bls::BlsBackend,
    fork::{ChainSpec, LcFork},
    store::{apply_core, ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientUpdate},
    validate::{is_finality_update, validate_light_client_update, Context, SignaturePath},
    ws, Error, Root,
};

/// What a store checks when it adopts a committee.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Adoption {
    /// [`BlsBackend::check_committee`]: `KeyValidate` on every key, and the sum. The default
    /// off chain, for `verify_checkpoint` and `monitor`.
    #[default]
    Full,
    /// [`BlsBackend::check_committee_onchain`]: every key on the curve and not the identity, and
    /// the sum, without the per-key subgroup check. What the on-chain program runs.
    OnChain,
}

/// An instance's store under Ferrata's policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizedStore {
    core: StoreCore,
    adoption: Adoption,
    /// The weak-subjectivity period in epochs, if the store enforces one.
    ws_period: Option<u64>,
}

impl FinalizedStore {
    /// Initialise from a bootstrap matching a trusted checkpoint root.
    pub fn initialize(
        trusted_block_root: &Root,
        bootstrap: &LightClientBootstrap,
        fork: LcFork,
        spec: &ChainSpec,
        bls: &dyn BlsBackend,
    ) -> Result<Self, Error> {
        Self::initialize_with(trusted_block_root, bootstrap, fork, spec, bls, Adoption::Full)
    }

    /// [`FinalizedStore::initialize`] with a chosen committee check.
    pub fn initialize_with(
        trusted_block_root: &Root,
        bootstrap: &LightClientBootstrap,
        fork: LcFork,
        spec: &ChainSpec,
        bls: &dyn BlsBackend,
        adoption: Adoption,
    ) -> Result<Self, Error> {
        let core = StoreCore::from_bootstrap(trusted_block_root, bootstrap, fork, spec)?;
        let store = Self { core, adoption, ws_period: None };
        store.check(bls, &store.core.current_sync_committee)?;
        Ok(store)
    }

    fn check(&self, bls: &dyn BlsBackend, committee: &crate::types::SyncCommittee) -> Result<(), Error> {
        match self.adoption {
            Adoption::Full => bls.check_committee(committee),
            Adoption::OnChain => bls.check_committee_onchain(committee),
        }
    }

    /// A store from state that did not come from a bootstrap, for tests and for restoring a
    /// store from its own accounts. Both committees are checked, as at adoption.
    pub fn from_core(core: StoreCore, bls: &dyn BlsBackend) -> Result<Self, Error> {
        let store = Self { core, adoption: Adoption::Full, ws_period: None };
        store.check(bls, &store.core.current_sync_committee)?;
        if store.core.is_next_sync_committee_known() {
            store.check(bls, &store.core.next_sync_committee)?;
        }
        Ok(store)
    }

    /// The store's state.
    pub fn core(&self) -> &StoreCore {
        &self.core
    }

    /// Enforce a weak-subjectivity period of `epochs` (computed with
    /// [`ws::compute_weak_subjectivity_period`] from the checkpoint's state): from then on
    /// [`FinalizedStore::process_update`] refuses every update while `current_slot` is past the
    /// period of the store's finalized header.
    pub fn with_weak_subjectivity_period(mut self, epochs: u64) -> Self {
        self.ws_period = Some(epochs);
        self
    }

    /// The enforced weak-subjectivity period, in epochs.
    pub fn weak_subjectivity_period(&self) -> Option<u64> {
        self.ws_period
    }

    /// True if the store may still be advanced at `current_slot`: no period is enforced, or
    /// `current_slot` is inside the period of the finalized header.
    pub fn is_within_weak_subjectivity_period(&self, current_slot: u64, spec: &crate::fork::ChainSpec) -> bool {
        match self.ws_period {
            None => true,
            Some(p) => {
                ws::is_within_weak_subjectivity_period(self.core.finalized_header.beacon.slot, current_slot, p, spec)
            }
        }
    }

    /// Apply a finality update signed by a supermajority. Atomic: on error nothing changes.
    pub fn process_update(
        &mut self,
        update: &LightClientUpdate,
        current_slot: u64,
        ctx: &Context<'_>,
    ) -> Result<ApplyOutcome, Error> {
        if !is_finality_update(update) {
            return Err(Error::NotFinalized);
        }
        let n = ctx.spec.preset.sync_committee_size as u64;
        if update.sync_aggregate.participants() * 3 < n * 2 {
            return Err(Error::BelowSupermajority);
        }
        validate_light_client_update(&self.core, update, current_slot, ctx, SignaturePath::CheckedCommittee)?;
        if let Some(period) = self.ws_period {
            ws::check_within(self.core.finalized_header.beacon.slot, current_slot, period, ctx.spec)?;
        }
        if !self.core.update_advances(update, ctx.spec) {
            return Err(Error::NotNewer);
        }
        let mut next = self.core.clone();
        let outcome = apply_core(&mut next, update, ctx.spec)?;
        if outcome.adopted {
            self.check(ctx.bls, &next.next_sync_committee)?;
        }
        self.core = next;
        Ok(outcome)
    }

    /// Move the store to a later fork's layout.
    pub fn upgrade(&mut self, to: LcFork, spec: &ChainSpec) -> Result<(), Error> {
        let mut next = self.core.clone();
        next.upgrade(to, spec)?;
        self.core = next;
        Ok(())
    }
}
