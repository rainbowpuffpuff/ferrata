//! Plumbline's on-chain policy.
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
//! header, which Plumbline will not do. Such an instance stops safely and is replaced from a
//! fresh checkpoint (`stops_safely_after_a_period_without_finality` in the tests).

use crate::{
    bls::BlsBackend,
    fork::{ChainSpec, LcFork},
    store::{apply_core, ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientUpdate},
    validate::{is_finality_update, validate_light_client_update, Context, SignaturePath},
    Error, Root,
};

/// An instance's store under Plumbline's policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizedStore {
    core: StoreCore,
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
        let core = StoreCore::from_bootstrap(trusted_block_root, bootstrap, fork, spec)?;
        bls.check_committee(&core.current_sync_committee)?;
        Ok(Self { core })
    }

    /// A store from state that did not come from a bootstrap, for tests and for restoring a
    /// store from its own accounts. Both committees are checked, as at adoption.
    pub fn from_core(core: StoreCore, bls: &dyn BlsBackend) -> Result<Self, Error> {
        bls.check_committee(&core.current_sync_committee)?;
        if core.is_next_sync_committee_known() {
            bls.check_committee(&core.next_sync_committee)?;
        }
        Ok(Self { core })
    }

    /// The store's state.
    pub fn core(&self) -> &StoreCore {
        &self.core
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
        if !self.core.update_advances(update, ctx.spec) {
            return Err(Error::NotNewer);
        }
        let mut next = self.core.clone();
        let outcome = apply_core(&mut next, update, ctx.spec)?;
        if outcome.adopted {
            ctx.bls.check_committee(&next.next_sync_committee)?;
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
