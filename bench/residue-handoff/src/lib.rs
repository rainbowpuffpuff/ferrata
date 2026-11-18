//! A committee handoff (a period update) in one execution, for the bench and the experimental
//! program copies only.
//!
//! **Experimental and unaudited. No deployed program uses this code, and it is sound only under the
//! honest-supermajority assumption the light client already makes** (fewer than 342 of a committee's
//! 512 members dishonest; `bench/RESIDUE.md` §10 gives the argument and its sources).
//!
//! The core adopts a committee only after a witness of its uncompressed keys has shown that every
//! key decodes and that the keys hash to the committee's key root and sum to its aggregate key
//! (`ferrata_core::witness::AdoptionProgress`, 512 keys on mainnet, a transaction of its own on
//! LEZ). Here the next committee is adopted by its **summary**: the key root and the aggregate key
//! exactly as the update's next-committee branch proves them in the attested state, which the
//! signing committee signed (`steps::validate_compact`, run by step 1). Nothing else changes:
//! every non-signer's key is still decoded canonically and proven against the key root where it is
//! used (`residue_agg`, or the core's composition), and the aggregate is decoded there.
//!
//! [`adopt_summary`] is the one new step. [`one_step`] runs a whole update as the experimental
//! programs do: `residue_agg::step1` (one multiproof for the non-signers), `Job::step2` (the Miller
//! loop), the residue-witness check of `bench/residue` in place of the final exponentiation
//! ([`residue_check`]), the summary adoption if applying the update adopts a committee, and the
//! core's `steps::apply`. The store it gives is the core's store after its own steps (the 512-key
//! adoption included) on every update whose next committee's aggregate is the sum of its keys,
//! which every state the beacon chain's state transition produces satisfies; only the job's
//! `adoption` field differs (`None` here, a completed `AdoptionProgress` there), and no store holds
//! it.

#![allow(unsafe_code)]

use blst::{blst_p1_affine, blst_p1_affine_is_inf, blst_p1_uncompress, BLST_ERROR};
use ferrata_core::{
    fork::ChainSpec,
    steps::{self, Job, PendingApply, Phase, StepError},
    store::ApplyOutcome,
    witness::{CompactStore, CompactUpdate},
    Error, Root,
};
use residue::FP12_BYTES;
use residue_agg::{NonSigners, Sum};

/// Adopt the committee `job` adopts by its summary, without its keys: the job no longer waits for
/// an adoption, and `steps::apply` then writes the update's next-committee summary into the store
/// as it does after the core's adoption.
///
/// Refused with [`StepError::NoAdoption`] (84) when the job adopts no committee or its adoption is
/// complete, and for an initialization: the bootstrap's committee keeps the core's adoption (its
/// trust comes from the checkpoint, not from a signature; not covered by `bench/RESIDUE.md` §10).
/// Refused with `CommitteeInvalid` (45), as the core's adoption refuses it, when the summary's
/// aggregate field does not decode to a curve point or is the identity: such a committee could never
/// verify an update (an independent review's finding 9; `bench/RESIDUE.md` §10.3).
pub fn adopt_summary(job: &mut Job) -> Result<(), StepError> {
    let Some(progress) = job.adoption.as_ref().filter(|a| !a.complete) else {
        return Err(StepError::NoAdoption);
    };
    match &job.pending {
        // `steps::step1` builds the progress from the update's next committee, the summary step 1
        // proved against the attested state root; the application writes the same summary.
        PendingApply::Update { next: Some(next), .. } if *next == progress.committee => {
            if !aggregate_decodes(&next.aggregate_pubkey) {
                return Err(StepError::Core(Error::CommitteeInvalid));
            }
            job.adoption = None;
            Ok(())
        }
        _ => Err(StepError::NoAdoption),
    }
}

/// The aggregate field decodes (blst's compressed G1 decoding: canonical, on the curve) to a point
/// that is not the identity. No subgroup check: the core's adoption makes none either.
fn aggregate_decodes(aggregate: &[u8; 48]) -> bool {
    let mut point = blst_p1_affine::default();
    // SAFETY: valid destination; `aggregate` holds the 48 bytes blst reads.
    unsafe {
        blst_p1_uncompress(&mut point, aggregate.as_ptr()) == BLST_ERROR::BLST_SUCCESS && !blst_p1_affine_is_inf(&point)
    }
}

/// A chain specification the period arithmetic can run on: `ChainSpec::period_at_slot` divides by the
/// slots per epoch and the epochs per sync-committee period, so a zero in either would panic (an
/// independent review's finding 7, for the programs' `check_config`). Refused as `Malformed` (1).
pub fn check_spec(spec: &ChainSpec) -> Result<(), StepError> {
    if spec.preset.slots_per_epoch == 0 || spec.preset.epochs_per_sync_committee_period == 0 {
        return Err(StepError::Core(Error::Malformed));
    }
    Ok(())
}

/// The residue-witness check of the job's Miller-loop product (`bench/residue`, `bench/RESIDUE.md`
/// §2-§3) in place of the final exponentiation. A witness that fails is `SignatureInvalid` (40), as
/// in the experimental programs.
pub fn residue_check(job: &mut Job, c: &[u8; FP12_BYTES], w: &[u8; FP12_BYTES]) -> Result<(), StepError> {
    let Phase::FinalExp { product } = &job.phase else {
        return Err(StepError::Phase);
    };
    let f = residue::decode_fp12(product).ok_or(StepError::Core(Error::Malformed))?;
    residue::check_bytes(&f, c, w).map_err(|_| StepError::Core(Error::SignatureInvalid))?;
    job.phase = Phase::Verified;
    Ok(())
}

/// One whole update in one execution, with the summary adoption: `store` is the store the update is
/// verified against (`steps::store_for_update` of `stored`), `current_slot` the slot the caller
/// vouches for (the programs pass `signature_slot`), `c` and `w` the residue witness of the
/// Miller-loop product. Returns the new store, what applying changed, and the applied job.
#[allow(clippy::too_many_arguments)]
pub fn one_step(
    stored: &CompactStore,
    store: &CompactStore,
    update: &CompactUpdate,
    signers: &NonSigners,
    current_slot: u64,
    c: &[u8; FP12_BYTES],
    w: &[u8; FP12_BYTES],
    spec: &ChainSpec,
    genesis_validators_root: &Root,
    sum: Sum,
) -> Result<(CompactStore, ApplyOutcome, Job), StepError> {
    check_spec(spec)?;
    let mut job = residue_agg::step1(store, update, signers, current_slot, spec, genesis_validators_root, 1, sum)?;
    job.step2()?;
    residue_check(&mut job, c, w)?;
    if job.adoption_pending() {
        adopt_summary(&mut job)?;
    }
    let (next, outcome) = steps::apply(stored, &job, spec)?;
    Ok((next, outcome, job))
}
