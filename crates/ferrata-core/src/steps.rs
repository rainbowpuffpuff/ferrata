//! [`FinalizedStore::process_update`](crate::policy::FinalizedStore::process_update) split into
//! the steps of an on-chain job, over the compact store of [`crate::witness`].
//!
//! A LEZ v0.4 transaction should stay under 5M cycles (`TARGET_GAS_EXEC`) and must stay under
//! 10M (`MAX_GAS_EXEC`). Verifying one mainnet update takes about 10M, so a program runs it as a
//! job of several transactions:
//!
//! 1. [`step1`]: the policy's checks and every check of `validate_light_client_update` except the
//!    pairing, over the [`CompactStore`]; the signing key `aggregate − Σ non-signers` from a
//!    [`SignerWitness`]; the signature's decoding and subgroup check; hash-to-G2. The result is a
//!    [`Job`] holding H(m), −key and the signature (480 bytes), bound to the digests of the store
//!    and of the update.
//! 2. [`Job::step2`]: the Miller loops (one two-pair loop); the job then holds their 576-byte
//!    product, the same bytes as [`Blst::miller_half`].
//! 3. [`Job::step3`]: the final exponentiation (the core's [`Blst::final_half`]); the job is then
//!    verified.
//!
//! If applying the update adopts a committee, the job also needs the committee's adoption
//! ([`Job::adopt`], in parts). [`apply`] then applies the verified job to the store it started
//! from, and to no other: it compares the store's digest with the job's.
//!
//! **Forks.** A store keeps the layout it was created in until the chain reaches a later fork of
//! its own schedule. The first update attested under that fork arrives in the fork's layout;
//! [`store_for_update`] then gives the store upgraded to it (`upgrade_lc_store_to_*`), step 1
//! runs on that store, and [`apply`] makes the upgrade the store's together with the update. An
//! update in an earlier layout than the store's is refused: its submitter upgrades it first
//! ([`crate::upgrade::upgrade_update`]).
//!
//! **Equivalence.** For every update, the steps accept exactly when `process_update` with
//! `Adoption::OnChain` accepts, and the resulting store is the compact form of its store. The
//! refusal code is the same when an update has a single defect. With several, the steps report
//! the cheapest one first: `NotNewer` and `CommitteeUnknown` before the signature, where
//! `process_update` verifies the signature first. `tests/steps.rs` in `ferrata-spectests` checks
//! this on every spec vector, on the mainnet replay and on mutated mainnet updates; on the
//! vectors that cross a fork it also checks that a store which is never told to upgrade, and
//! moves only by [`store_for_update`], accepts and refuses the same updates.
//!
//! The signing key is always composed subtractively. The policy's two-thirds floor bounds the
//! non-signers to a third of the committee, so this is never the longer composition.

#![allow(unsafe_code)]

use alloc::vec::Vec;

use blst::{
    blst_bendian_from_fp, blst_fp, blst_fp12, blst_fp_from_bendian, blst_hash_to_g2, blst_miller_loop_n, blst_p1,
    blst_p1_add_or_double, blst_p1_affine, blst_p1_affine_generator, blst_p1_affine_serialize, blst_p1_cneg,
    blst_p1_from_affine, blst_p1_is_inf, blst_p1_to_affine, blst_p1_uncompress, blst_p2, blst_p2_affine,
    blst_p2_affine_in_g2, blst_p2_affine_is_inf, blst_p2_affine_serialize, blst_p2_deserialize, blst_p2_to_affine,
    blst_p2_uncompress, BLST_ERROR,
};

use crate::{
    bls::{Blst, DST, FP12_BYTES},
    fork::{ChainSpec, LcFork},
    hash::sha256,
    merkle::{is_valid_merkle_branch, is_valid_normalized_merkle_branch, ZERO},
    store::ApplyOutcome,
    types::{BeaconBlockHeader, BlsSignature},
    validate::{
        compute_domain, compute_signing_root, current_sync_committee_gindex_at_slot, finalized_root_gindex_at_slot,
        is_valid_light_client_header, next_sync_committee_gindex_at_slot, DOMAIN_SYNC_COMMITTEE, GENESIS_SLOT,
    },
    witness::{
        add_affine, deserialize_g1, key_branch_depth, pubkey_leaf, witness_key, AdoptionProgress, CommitteeSummary,
        CompactBootstrap, CompactStore, CompactUpdate, Reader, SignerWitness, UncompressedPubkey, Writer,
    },
    Error, Root,
};

/// A refusal of the step API: the core's codes, plus the job's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepError {
    /// A light-client refusal, with the core's stable code.
    Core(Error),
    /// The signer witness does not fit the committee: wrong count, encoding, key or branch.
    Witness,
    /// The step does not follow the job's last one (skipped or replayed).
    Phase,
    /// The job was started on a store with another digest.
    StoreChanged,
    /// The update adopts a committee whose adoption is not complete.
    AdoptionIncomplete,
    /// The job adopts no committee, or its adoption is already complete.
    NoAdoption,
}

impl StepError {
    /// The stable numeric code: the core's code, or 80 to 84.
    pub const fn code(self) -> u16 {
        match self {
            StepError::Core(e) => e.code(),
            StepError::Witness => 80,
            StepError::Phase => 81,
            StepError::StoreChanged => 82,
            StepError::AdoptionIncomplete => 83,
            StepError::NoAdoption => 84,
        }
    }
}

impl From<Error> for StepError {
    fn from(e: Error) -> Self {
        StepError::Core(e)
    }
}

// ---------------------------------------------------------------------------------------------
// Header record
// ---------------------------------------------------------------------------------------------

// The record a store keeps of a finalized header. It lives in [`crate::witness`], which builds
// without a BLS backend, so that a consumer can decode a record it reads without linking one.
pub use crate::witness::{ExecutionSummary, HeaderRecord};

// ---------------------------------------------------------------------------------------------
// Validation over the compact store
// ---------------------------------------------------------------------------------------------

/// Which committee signs an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigningCommittee {
    Current,
    Next,
}

/// `validate_light_client_update` over the compact store, without the signature: a
/// transcription of [`crate::validate::validate_light_client_update`] in which the update's
/// next committee is its summary. Returns the committee that must have signed and the signing
/// root.
pub fn validate_compact(
    store: &CompactStore,
    update: &CompactUpdate,
    current_slot: u64,
    spec: &ChainSpec,
    genesis_validators_root: &Root,
) -> Result<(SigningCommittee, Root), Error> {
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
    if update.sync_aggregate.bits.len() != n {
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
    let store_period = spec.period_at_slot(store.finalized.slot);
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
        !next_known && update.is_sync_committee_update() && update_attested_period == store_period;
    if !(update_attested_slot > store.finalized.slot || update_has_next_sync_committee) {
        return Err(Error::UpdateIrrelevant);
    }

    // Verify that the `finality_branch`, if present, confirms `finalized_header` to match the
    // finalized checkpoint root saved in the state of `attested_header`.
    if !update.is_finality_update() {
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
    if !update.is_sync_committee_update() {
        if update.next_sync_committee.is_some() {
            return Err(Error::NextCommitteeUnexpected);
        }
    } else {
        if update_attested_period == store_period && next_known && update.next_sync_committee != store.next {
            return Err(Error::NextCommitteeMismatch);
        }
        let root = match &update.next_sync_committee {
            Some(summary) => summary.root(),
            None => CommitteeSummary::empty(n).root(),
        };
        if !is_valid_normalized_merkle_branch(
            &root,
            &update.next_sync_committee_branch,
            next_sync_committee_gindex_at_slot(update_attested_slot, fork, spec),
            &update.attested_header.beacon.state_root,
        ) {
            return Err(Error::NextCommitteeBranchInvalid);
        }
    }

    // The committee that signs, and what it signs.
    let signer =
        if update_signature_period == store_period { SigningCommittee::Current } else { SigningCommittee::Next };
    let fork_version_slot = update.signature_slot.saturating_sub(1);
    let fork_version = spec.compute_fork_version(spec.epoch_at_slot(fork_version_slot));
    let domain = compute_domain(DOMAIN_SYNC_COMMITTEE, fork_version, genesis_validators_root);
    let signing_root = compute_signing_root(&update.attested_header.beacon.hash_tree_root(), &domain);
    Ok((signer, signing_root))
}

/// The store an update encoded in `layout` is verified against and applied to.
///
/// * `layout` is the store's: the store itself.
/// * `layout` is later: the store upgraded to it ([`CompactStore::upgraded`]), provided `layout`
///   is the layout of the fork that the schedule of `spec` puts at the update's attested header.
///   So a store moves to a fork's layout with the first update attested under that fork and at
///   no other time, and only to a fork of the schedule its instance was created with: a layout
///   the schedule has not reached at the attested slot is `ForkMismatch`.
/// * `layout` is earlier: `ForkMismatch`.
///
/// Nothing is written here: the upgrade becomes the store's when a job that started from the
/// upgraded store is applied ([`apply`]).
pub fn store_for_update(
    store: &CompactStore,
    layout: LcFork,
    attested_slot: u64,
    spec: &ChainSpec,
) -> Result<CompactStore, Error> {
    if layout == store.fork {
        return Ok(store.clone());
    }
    let scheduled = spec.fork_at_epoch(spec.epoch_at_slot(attested_slot)).lc_fork();
    if layout < store.fork || scheduled != Some(layout) {
        return Err(Error::ForkMismatch);
    }
    store.upgraded(layout)
}

/// `store`, or `store` upgraded to a later layout, whichever has `digest`: the store a job that
/// names `digest` started from.
fn started_from(store: &CompactStore, digest: &Root) -> Option<CompactStore> {
    let mut candidate = store.clone();
    loop {
        if candidate.digest() == *digest {
            return Some(candidate);
        }
        candidate.fork = candidate.fork.next()?;
    }
}

/// `StoreCore::update_advances` over the compact store.
pub fn update_advances(store: &CompactStore, update: &CompactUpdate, spec: &ChainSpec) -> bool {
    let update_has_finalized_next_sync_committee = !store.is_next_sync_committee_known()
        && update.is_sync_committee_update()
        && update.is_finality_update()
        && spec.period_at_slot(update.finalized_header.beacon.slot)
            == spec.period_at_slot(update.attested_header.beacon.slot);
    update.finalized_header.beacon.slot > store.finalized.slot || update_has_finalized_next_sync_committee
}

/// `apply_core` over the compact store: the update's finalized beacon header and next committee.
pub fn apply_core_compact(
    store: &CompactStore,
    finalized: &BeaconBlockHeader,
    next: Option<CommitteeSummary>,
    spec: &ChainSpec,
) -> Result<(CompactStore, ApplyOutcome), Error> {
    let mut s = store.clone();
    let store_period = spec.period_at_slot(s.finalized.slot);
    let update_finalized_period = spec.period_at_slot(finalized.slot);
    let mut outcome = ApplyOutcome::default();
    match s.next.take() {
        None => {
            if update_finalized_period != store_period {
                return Err(Error::CommitteeUnknown);
            }
            s.next = next;
            outcome.adopted = next.is_some();
        }
        Some(known) if update_finalized_period == store_period + 1 => {
            s.current = known;
            s.next = next;
            outcome.rotated = true;
            outcome.adopted = next.is_some();
        }
        Some(known) => s.next = Some(known),
    }
    if finalized.slot > s.finalized.slot {
        s.finalized = finalized.clone();
        outcome.advanced = true;
    }
    Ok((s, outcome))
}

// ---------------------------------------------------------------------------------------------
// The job
// ---------------------------------------------------------------------------------------------

/// Where a job's signature check stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Awaiting the Miller loops: H(m) (G2), −key (G1) and the signature (G2), uncompressed.
    Miller { hm: [u8; 192], neg_key: [u8; 96], signature: [u8; 192] },
    /// Awaiting the final exponentiation of the Miller-loop product.
    FinalExp { product: [u8; FP12_BYTES] },
    /// The signature verified (or, for an initialization, there is none).
    Verified,
}

/// What a verified job changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingApply {
    /// Activate a store initialized from a bootstrap, once its committee is adopted; the
    /// checkpoint header is recorded.
    Activate { record: HeaderRecord },
    /// Apply an update: its finalized beacon header and next committee, and the record of the
    /// finalized header.
    Update { finalized: BeaconBlockHeader, next: Option<CommitteeSummary>, record: HeaderRecord },
}

impl PendingApply {
    /// The header record this job writes if it advances (always, for an activation).
    pub fn record(&self) -> &HeaderRecord {
        match self {
            PendingApply::Activate { record } | PendingApply::Update { record, .. } => record,
        }
    }
}

/// One update (or initialization) in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// Digest of the store the job started from; [`apply`] accepts no other.
    pub store_digest: Root,
    /// Digest of the update (or bootstrap) the job verifies.
    pub update_digest: Root,
    pub phase: Phase,
    /// The adoption of the committee the job's application adopts, if it adopts one.
    pub adoption: Option<AdoptionProgress>,
    pub pending: PendingApply,
}

const JOB_VERSION: u8 = 1;
impl Job {
    /// True once the signature verified and any adoption is complete.
    #[allow(clippy::unnecessary_map_or, reason = "the attestation program's image id depends on this code")]
    pub fn is_verified(&self) -> bool {
        self.phase == Phase::Verified && self.adoption.as_ref().map_or(true, |a| a.complete)
    }

    /// True while an adoption is required and not complete.
    pub fn adoption_pending(&self) -> bool {
        self.adoption.as_ref().is_some_and(|a| !a.complete)
    }

    /// Step 2: the Miller loops. Requires [`Phase::Miller`].
    pub fn step2(&mut self) -> Result<(), StepError> {
        let Phase::Miller { hm, neg_key, signature } = &self.phase else {
            return Err(StepError::Phase);
        };
        let product = miller_from_points(hm, neg_key, signature)?;
        self.phase = Phase::FinalExp { product };
        Ok(())
    }

    /// Step 3: the final exponentiation ([`Blst::final_half`]). Requires [`Phase::FinalExp`].
    pub fn step3(&mut self) -> Result<(), StepError> {
        let Phase::FinalExp { product } = &self.phase else {
            return Err(StepError::Phase);
        };
        if !canonical_fp12(product) {
            return Err(Error::Malformed.into());
        }
        Blst::final_half(product)?;
        self.phase = Phase::Verified;
        Ok(())
    }

    /// One part of the committee adoption (see [`AdoptionProgress::add_part`]).
    pub fn adopt(&mut self, index: u8, keys: &[UncompressedPubkey], n: usize) -> Result<(), StepError> {
        let Some(progress) = self.adoption.as_mut().filter(|a| !a.complete) else {
            return Err(StepError::NoAdoption);
        };
        progress.add_part(index, keys, n)?;
        Ok(())
    }

    /// Strict encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(JOB_VERSION);
        w.bytes(&self.store_digest);
        w.bytes(&self.update_digest);
        match &self.phase {
            Phase::Miller { hm, neg_key, signature } => {
                w.u8(0);
                w.bytes(hm);
                w.bytes(neg_key);
                w.bytes(signature);
            }
            Phase::FinalExp { product } => {
                w.u8(1);
                w.bytes(product);
            }
            Phase::Verified => w.u8(2),
        }
        w.u8(u8::from(self.adoption.is_some()));
        if let Some(a) = &self.adoption {
            a.write(&mut w);
        }
        match &self.pending {
            PendingApply::Activate { record } => {
                w.u8(0);
                record.write(&mut w);
            }
            PendingApply::Update { finalized, next, record } => {
                w.u8(1);
                w.bytes(&crate::witness::encode_beacon_header(finalized));
                w.u8(u8::from(next.is_some()));
                if let Some(s) = next {
                    w.bytes(&s.encode());
                }
                record.write(&mut w);
            }
        }
        w.out
    }

    /// Strict decoding.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        if r.u8()? != JOB_VERSION {
            return Err(Error::Malformed);
        }
        let store_digest = r.array()?;
        let update_digest = r.array()?;
        let phase = match r.u8()? {
            0 => Phase::Miller { hm: r.array()?, neg_key: r.array()?, signature: r.array()? },
            1 => Phase::FinalExp { product: r.array()? },
            2 => Phase::Verified,
            _ => return Err(Error::Malformed),
        };
        let adoption = if r.flag()? { Some(AdoptionProgress::read(&mut r)?) } else { None };
        let pending = match r.u8()? {
            0 => PendingApply::Activate { record: HeaderRecord::read(&mut r)? },
            1 => {
                let finalized = BeaconBlockHeader::decode(r.take(BeaconBlockHeader::SSZ_LEN)?)?;
                let next = if r.flag()? {
                    Some(CommitteeSummary::decode(r.take(CommitteeSummary::ENCODED_LEN)?)?)
                } else {
                    None
                };
                PendingApply::Update { finalized, next, record: HeaderRecord::read(&mut r)? }
            }
            _ => return Err(Error::Malformed),
        };
        r.finish()?;
        Ok(Self { store_digest, update_digest, phase, adoption, pending })
    }
}

/// Initialization as a job: the checks of `initialize_light_client_store` on a compact
/// bootstrap. Returns the new store, not yet active, and a job that activates it once the
/// bootstrap committee is adopted (in `parts` parts).
pub fn initialize(
    trusted_block_root: &Root,
    bootstrap: &CompactBootstrap,
    fork: LcFork,
    spec: &ChainSpec,
    parts: u8,
) -> Result<(CompactStore, Job), StepError> {
    if !bootstrap.header.has_layout(fork)
        || bootstrap.current_sync_committee_branch.len() != fork.sync_committee_branch_len()
    {
        return Err(Error::ForkMismatch.into());
    }
    if !is_valid_light_client_header(&bootstrap.header, fork, spec) {
        return Err(Error::BootstrapHeaderInvalid.into());
    }
    if bootstrap.header.beacon.hash_tree_root() != *trusted_block_root {
        return Err(Error::CheckpointMismatch.into());
    }
    if !is_valid_normalized_merkle_branch(
        &bootstrap.current.root(),
        &bootstrap.current_sync_committee_branch,
        current_sync_committee_gindex_at_slot(bootstrap.header.beacon.slot, fork, spec),
        &bootstrap.header.beacon.state_root,
    ) {
        return Err(Error::CurrentCommitteeBranchInvalid.into());
    }
    let store =
        CompactStore { fork, finalized: bootstrap.header.beacon.clone(), current: bootstrap.current, next: None };
    let job = Job {
        store_digest: store.digest(),
        update_digest: sha256(&bootstrap.encode(fork)),
        phase: Phase::Verified,
        adoption: Some(AdoptionProgress::new(bootstrap.current, parts)),
        pending: PendingApply::Activate { record: HeaderRecord::of(&bootstrap.header, fork, spec) },
    };
    Ok((store, job))
}

/// Step 1: everything up to the Miller loops. See the module documentation.
///
/// `current_slot` is the slot the caller vouches for (a program passes `signature_slot` and makes
/// the transaction valid only from that slot's start, through its validity window). `parts` is the
/// number of parts an adoption arrives in.
pub fn step1(
    store: &CompactStore,
    update: &CompactUpdate,
    signers: &SignerWitness,
    current_slot: u64,
    spec: &ChainSpec,
    genesis_validators_root: &Root,
    parts: u8,
) -> Result<Job, StepError> {
    // The policy: finality updates only, signed by two thirds of the committee.
    if !update.is_finality_update() {
        return Err(Error::NotFinalized.into());
    }
    let n = spec.preset.sync_committee_size as u64;
    if update.sync_aggregate.participants() * 3 < n * 2 {
        return Err(Error::BelowSupermajority.into());
    }
    let (signer, signing_root) = validate_compact(store, update, current_slot, spec, genesis_validators_root)?;
    if !update_advances(store, update, spec) {
        return Err(Error::NotNewer.into());
    }
    let (_, outcome) = apply_core_compact(store, &update.finalized_header.beacon, update.next_sync_committee, spec)?;

    // The signature, as `verify_checked` with the subtractive composition: key, then signature.
    let committee = match signer {
        SigningCommittee::Current => &store.current,
        SigningCommittee::Next => store.next.as_ref().ok_or(Error::CommitteeUnknown)?,
    };
    let key = signing_key(committee, &update.sync_aggregate.bits, signers)?;
    let signature = decode_signature(&update.sync_aggregate.signature)?;
    let (hm, neg_key) = hash_and_negate(&signing_root, &key);

    let fork = store.fork;
    let adoption = if outcome.adopted {
        let next = update.next_sync_committee.ok_or(Error::CommitteeUnknown)?;
        Some(AdoptionProgress::new(next, parts))
    } else {
        None
    };
    Ok(Job {
        store_digest: store.digest(),
        update_digest: update.digest(fork),
        phase: Phase::Miller { hm, neg_key, signature: serialize_g2(&signature) },
        adoption,
        pending: PendingApply::Update {
            finalized: update.finalized_header.beacon.clone(),
            next: update.next_sync_committee,
            record: HeaderRecord::of(&update.finalized_header, fork, spec),
        },
    })
}

/// Apply a verified job to `store`, which must be the store the job started from: `store`
/// itself, or, for an update, `store` upgraded to the later layout the update came in
/// ([`store_for_update`]). The store returned then has that layout.
pub fn apply(store: &CompactStore, job: &Job, spec: &ChainSpec) -> Result<(CompactStore, ApplyOutcome), StepError> {
    if job.phase != Phase::Verified {
        return Err(StepError::Phase);
    }
    if job.adoption_pending() {
        return Err(StepError::AdoptionIncomplete);
    }
    match &job.pending {
        PendingApply::Activate { .. } => {
            if store.digest() != job.store_digest {
                return Err(StepError::StoreChanged);
            }
            Ok((store.clone(), ApplyOutcome::default()))
        }
        PendingApply::Update { finalized, next, .. } => {
            let base = started_from(store, &job.store_digest).ok_or(StepError::StoreChanged)?;
            Ok(apply_core_compact(&base, finalized, *next, spec)?)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// BLS pieces (blst), mirroring `bls::blst_backend`
// ---------------------------------------------------------------------------------------------

/// `aggregate − Σ non-signers`, the non-signers' keys taken from `witness` and each checked
/// against the committee's key root.
fn signing_key(committee: &CommitteeSummary, bits: &[bool], witness: &SignerWitness) -> Result<blst_p1, StepError> {
    let depth = key_branch_depth(bits.len());
    let absent = bits.iter().enumerate().filter(|(_, b)| !**b).map(|(i, _)| i);
    if witness.keys.len() != bits.iter().filter(|b| !**b).count() || witness.branches.len() != witness.keys.len() {
        return Err(StepError::Witness);
    }
    let mut aggregate = blst_p1_affine::default();
    // SAFETY: valid destination; the key holds the 48 bytes blst reads.
    if unsafe { blst_p1_uncompress(&mut aggregate, committee.aggregate_pubkey.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::PubkeyInvalid.into());
    }
    let mut absent_sum = blst_p1::default();
    for ((index, key), branch) in absent.zip(&witness.keys).zip(&witness.branches) {
        let (point, compressed) = witness_key(key).map_err(|_| StepError::Witness)?;
        if branch.len() != depth
            || !is_valid_merkle_branch(&pubkey_leaf(&compressed), branch, depth, index as u64, &committee.pubkeys_root)
        {
            return Err(StepError::Witness);
        }
        add_affine(&mut absent_sum, &point);
    }
    let mut key = blst_p1::default();
    let key_ptr: *mut blst_p1 = &mut key;
    // SAFETY: valid points; blst allows the output to alias the first input.
    unsafe {
        blst_p1_from_affine(key_ptr, &aggregate);
        blst_p1_cneg(&mut absent_sum, true);
        blst_p1_add_or_double(key_ptr, key_ptr, &absent_sum);
    }
    // SAFETY: `key` is a valid point.
    if unsafe { blst_p1_is_inf(&key) } {
        return Err(Error::SigningKeyIdentity.into());
    }
    Ok(key)
}

/// The signature as a point of G2, refused as in `verify_checked`.
fn decode_signature(signature: &BlsSignature) -> Result<blst_p2_affine, StepError> {
    let mut sig = blst_p2_affine::default();
    // SAFETY: valid destination; `signature` holds the 96 bytes blst reads.
    if unsafe { blst_p2_uncompress(&mut sig, signature.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::SignatureMalformed.into());
    }
    // SAFETY: `sig` is a valid affine point.
    if !unsafe { blst_p2_affine_in_g2(&sig) } {
        return Err(Error::SignatureMalformed.into());
    }
    // The identity signature verifies only under the identity key, which is refused earlier.
    // SAFETY: `sig` is a valid affine point.
    if unsafe { blst_p2_affine_is_inf(&sig) } {
        return Err(Error::SignatureInvalid.into());
    }
    Ok(sig)
}

/// H(m) on G2 and −key, both affine and serialized uncompressed.
fn hash_and_negate(signing_root: &Root, key: &blst_p1) -> ([u8; 192], [u8; 96]) {
    let mut hashed = blst_p2::default();
    let mut hashed_affine = blst_p2_affine::default();
    let mut neg_key = *key;
    let mut neg_key_affine = blst_p1_affine::default();
    let (mut hm, mut nk) = ([0u8; 192], [0u8; 96]);
    // SAFETY: every pointer refers to a live, initialised value of the type blst expects, and the
    // lengths passed are those of the slices pointed to.
    unsafe {
        blst_hash_to_g2(
            &mut hashed,
            signing_root.as_ptr(),
            signing_root.len(),
            DST.as_ptr(),
            DST.len(),
            core::ptr::null(),
            0,
        );
        blst_p2_to_affine(&mut hashed_affine, &hashed);
        blst_p1_cneg(&mut neg_key, true);
        blst_p1_to_affine(&mut neg_key_affine, &neg_key);
        blst_p2_affine_serialize(hm.as_mut_ptr(), &hashed_affine);
        blst_p1_affine_serialize(nk.as_mut_ptr(), &neg_key_affine);
    }
    (hm, nk)
}

fn serialize_g2(p: &blst_p2_affine) -> [u8; 192] {
    let mut out = [0u8; 192];
    // SAFETY: `out` has the 192 bytes blst writes.
    unsafe { blst_p2_affine_serialize(out.as_mut_ptr(), p) };
    out
}

fn deserialize_g2(bytes: &[u8; 192]) -> Result<blst_p2_affine, Error> {
    if bytes[0] & 0x80 != 0 {
        return Err(Error::Malformed);
    }
    let mut out = blst_p2_affine::default();
    // SAFETY: valid destination; `bytes` holds the 192 bytes blst reads.
    if unsafe { blst_p2_deserialize(&mut out, bytes.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::Malformed);
    }
    Ok(out)
}

/// `ML(H(m), −key) · ML(signature, g1)` from the points step 1 stored, as one two-pair Miller
/// loop (shared squarings; 3.25M cycles in the guest instead of 4.09M for two loops, `bench/`).
/// The product is the same element as the core's `miller` computes with two loops, and is encoded
/// as [`Blst::miller_half`] encodes it: `ferrata-spectests/tests/steps.rs` checks the 576 bytes
/// against `miller_half` on every mainnet update.
fn miller_from_points(hm: &[u8; 192], neg_key: &[u8; 96], signature: &[u8; 192]) -> Result<[u8; FP12_BYTES], Error> {
    let hm = deserialize_g2(hm)?;
    let neg_key = deserialize_g1(neg_key).map_err(|_| Error::Malformed)?;
    let sig = deserialize_g2(signature)?;
    let mut product = blst_fp12::default();
    // SAFETY: the arrays hold two valid points each, and blst reads exactly two of each.
    unsafe {
        let qs: [*const blst_p2_affine; 2] = [&hm, &sig];
        let ps: [*const blst_p1_affine; 2] = [&neg_key, blst_p1_affine_generator()];
        blst_miller_loop_n(&mut product, qs.as_ptr(), ps.as_ptr(), 2);
    }
    Ok(encode_fp12(&product))
}

fn encode_fp12(f: &blst_fp12) -> [u8; FP12_BYTES] {
    let mut out = [0u8; FP12_BYTES];
    let coefficients = f.fp6.iter().flat_map(|fp6| fp6.fp2.iter()).flat_map(|fp2| fp2.fp.iter());
    for (chunk, fp) in out.chunks_exact_mut(48).zip(coefficients) {
        // SAFETY: `chunk` has the 48 bytes blst writes; `fp` is a valid field element.
        unsafe { blst_bendian_from_fp(chunk.as_mut_ptr(), fp) };
    }
    out
}

/// True if every 48-byte coefficient is below the modulus (it survives a decode–encode round
/// trip), so a stored product has exactly one meaning.
fn canonical_fp12(bytes: &[u8; FP12_BYTES]) -> bool {
    bytes.chunks_exact(48).all(|chunk| {
        let mut fp = blst_fp::default();
        let mut back = [0u8; 48];
        // SAFETY: `chunk` holds the 48 bytes blst reads; `back` has the 48 bytes it writes.
        unsafe {
            blst_fp_from_bendian(&mut fp, chunk.as_ptr());
            blst_bendian_from_fp(back.as_mut_ptr(), &fp);
        }
        back == chunk
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_error_codes_are_distinct_from_core_codes() {
        let job_codes = [
            StepError::Witness,
            StepError::Phase,
            StepError::StoreChanged,
            StepError::AdoptionIncomplete,
            StepError::NoAdoption,
        ]
        .map(StepError::code);
        assert_eq!(job_codes, [80, 81, 82, 83, 84]);
        assert_eq!(StepError::Core(Error::NotNewer).code(), 52);
    }

    #[test]
    fn non_canonical_product_is_refused() {
        // 0xff.. exceeds the modulus in every coefficient.
        let mut job = Job {
            store_digest: ZERO,
            update_digest: ZERO,
            phase: Phase::FinalExp { product: [0xff; FP12_BYTES] },
            adoption: None,
            pending: PendingApply::Activate {
                record: HeaderRecord {
                    beacon: BeaconBlockHeader::default(),
                    execution_root: ZERO,
                    execution: ExecutionSummary::None,
                },
            },
        };
        assert_eq!(job.step3(), Err(StepError::Core(Error::Malformed)));
        assert_eq!(Job::decode(&job.encode()), Ok(job.clone()));
        // Replaying an earlier step is refused.
        assert_eq!(job.step2(), Err(StepError::Phase));
    }
}
