//! Plumbline's on-chain policy.
//!
//! A header enters only through a finality proof signed by at least two thirds of the sync
//! committee. There is no optimistic header, no best-update cache and no forced update, so a
//! header in the store is always one the committee finalized. Validation is the spec's
//! [`validate_light_client_update`], and state changes are the spec's [`apply_core`]; the
//! policy only refuses more. Each committee the store adopts is checked once with
//! [`BlsBackend::check_committee`], which makes signing-key subtraction exact.

use crate::{
    bls::BlsBackend,
    fork::{ChainSpec, LcFork},
    store::{apply_core, ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientUpdate},
    validate::{is_finality_update, is_sync_committee_update, validate_light_client_update, Context},
    Error, Root,
};

/// An instance's store under Plumbline's policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizedStore {
    pub core: StoreCore,
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
        validate_light_client_update(&self.core, update, current_slot, ctx)?;
        if !self.core.update_advances(update, ctx.spec) {
            return Err(Error::NotNewer);
        }
        if is_sync_committee_update(update) {
            ctx.bls.check_committee(&update.next_sync_committee)?;
        }
        let mut next = self.core.clone();
        let outcome = apply_core(&mut next, update, ctx.spec)?;
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
