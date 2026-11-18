//! Evidence that a sync committee an instance holds signed finality contradicting what the instance
//! recorded, and the freeze that follows, for the bench and the LEZ v0.3.1 port only.
//!
//! **Experimental and unaudited. No deployed program has this** (`bench/MISBEHAVIOUR.md` gives the
//! design and the argument that an honest network never produces such evidence).
//!
//! A piece of evidence is one light-client update that passes the instance's own checks
//! ([`validate_evidence`]: the policy and every check of `steps::validate_compact` but relevance and
//! the next-committee mismatch; a committee the store holds; the signature, [`signature_job`] and
//! [`verify_signature`]) and contradicts ([`contradiction`]) either
//!
//! * (A) a header the instance recorded as finalized: the update finalizes a header at the same slot
//!   with another root, or
//! * (B) the next committee the instance adopted for its current period: the update's attested and
//!   finalized headers are in that period and its branch proves another next committee.
//!
//! [`check_evidence`] runs the whole check; [`Instance`] is a model of an instance (store, records,
//! freeze) for the tests. Nothing of `ferrata-core` changed: its public API is used as it is.

#![allow(unsafe_code)]

use std::collections::BTreeMap;

use blst::{
    blst_hash_to_g2, blst_p1, blst_p1_affine, blst_p1_affine_serialize, blst_p1_cneg, blst_p1_to_affine, blst_p2,
    blst_p2_affine, blst_p2_affine_in_g2, blst_p2_affine_is_inf, blst_p2_affine_serialize, blst_p2_to_affine,
    blst_p2_uncompress, BLST_ERROR,
};
use ferrata_core::{
    bls::DST,
    fork::{ChainSpec, LcFork},
    merkle::is_valid_normalized_merkle_branch,
    steps::{self, Job, PendingApply, Phase, SigningCommittee, StepError},
    store::ApplyOutcome,
    types::{BeaconBlockHeader, BlsSignature},
    validate::{
        compute_domain, compute_signing_root, finalized_root_gindex_at_slot, is_valid_light_client_header,
        next_sync_committee_gindex_at_slot, DOMAIN_SYNC_COMMITTEE, GENESIS_SLOT,
    },
    witness::{CommitteeSummary, CompactStore, CompactUpdate, HeaderRecord},
    ws, Error, Root,
};
use residue::FP12_BYTES;
use residue_agg::{NonSigners, Sum};

/// The update verifies under the instance's checks but contradicts nothing it holds.
pub const NO_CONTRADICTION: u16 = 85;
/// The instance is frozen ([`Instance`] only; the v0.3.1 port has its own code, 222).
pub const FROZEN: u16 = 86;
/// No record at the slot the evidence names ([`Instance`] only; the port refuses at the record account, 213).
pub const NO_RECORD: u16 = 87;

/// A refusal of evidence: the core's and the step API's codes, or one of this crate's (85-87).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceError {
    Step(StepError),
    NoContradiction,
    Frozen,
    NoRecord,
}

impl EvidenceError {
    pub const fn code(self) -> u16 {
        match self {
            EvidenceError::Step(e) => e.code(),
            EvidenceError::NoContradiction => NO_CONTRADICTION,
            EvidenceError::Frozen => FROZEN,
            EvidenceError::NoRecord => NO_RECORD,
        }
    }
}

impl From<StepError> for EvidenceError {
    fn from(e: StepError) -> Self {
        EvidenceError::Step(e)
    }
}

impl From<Error> for EvidenceError {
    fn from(e: Error) -> Self {
        EvidenceError::Step(StepError::Core(e))
    }
}

/// What a piece of evidence is checked against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Against {
    /// A header the instance recorded as finalized: its slot and the root of its beacon header. The
    /// caller vouches that the instance holds this record (the port checks it at the record account in
    /// the same transaction; [`Instance`] looks it up).
    Record { slot: u64, root: Root },
    /// The next committee the instance adopted for its current period (the store's `next`).
    NextCommittee,
}

/// What the evidence contradicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contradiction {
    /// (A) The update finalizes `conflicting` at `slot`, where the instance recorded `recorded`.
    Finalized { slot: u64, recorded: Root, conflicting: Root },
    /// (B) In `period`, the update proves the next committee `conflicting` (`SyncCommittee` root), where the
    /// instance adopted `adopted`.
    NextCommittee { period: u64, adopted: Root, conflicting: Root },
}

/// How the signature's pairing is completed after the Miller loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pairing {
    /// The core's final exponentiation (`Job::step3`).
    FinalExp,
    /// EXPERIMENTAL: the residue-witness check (`bench/residue`, `bench/RESIDUE.md` §2-§3).
    Residue { c: [u8; FP12_BYTES], w: [u8; FP12_BYTES] },
}

/// What a frozen instance keeps of the evidence that froze it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Freeze {
    pub contradiction: Contradiction,
    /// `hash_tree_root` of the evidence's attested beacon header.
    pub attested_root: Root,
    pub signature_slot: u64,
    /// `CompactUpdate::digest` of the evidence in the layout of the store it was checked against.
    pub update_digest: Root,
}

/// `steps::validate_compact` for evidence, with the policy of `steps::step1` first. The same checks in
/// the same order and with the same codes, **except two**, both about the past being evidence rather
/// than an advance:
///
/// * no relevance check (`UpdateIrrelevant`): evidence names a slot at or below the store's finalized
///   slot, and its attested header may be older than the store's finalized header;
/// * no `NextCommitteeMismatch`: that mismatch is evidence of kind (B), checked by [`contradiction`].
///
/// And one more refusal: a finalized header at the genesis slot (the zero-root placeholder names no
/// header), `FinalizedHeaderInvalid`. Returns the committee that must have signed (one the store
/// holds: the period check refuses any other with `PeriodSkipped`) and the signing root.
pub fn validate_evidence(
    store: &CompactStore,
    update: &CompactUpdate,
    current_slot: u64,
    spec: &ChainSpec,
    genesis_validators_root: &Root,
) -> Result<(SigningCommittee, Root), Error> {
    // The policy (as `steps::step1`): finality updates only, signed by two thirds of the committee.
    if !update.is_finality_update() {
        return Err(Error::NotFinalized);
    }
    let n = spec.preset.sync_committee_size;
    if update.sync_aggregate.participants() * 3 < n as u64 * 2 {
        return Err(Error::BelowSupermajority);
    }
    let fork = store.fork;

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

    // Sufficient participants.
    let sync_aggregate = &update.sync_aggregate;
    if sync_aggregate.participants() < spec.preset.min_sync_committee_participants {
        return Err(Error::ParticipationLow);
    }

    // The attested header, the slot order, and a signing committee the store holds.
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

    // (No relevance check: see above.)

    // The finalized header and its finality branch. Evidence names a real header: not the genesis
    // placeholder.
    if update_finalized_slot == GENESIS_SLOT || !is_valid_light_client_header(&update.finalized_header, fork, spec) {
        return Err(Error::FinalizedHeaderInvalid);
    }
    if !is_valid_normalized_merkle_branch(
        &update.finalized_header.beacon.hash_tree_root(),
        &update.finality_branch,
        finalized_root_gindex_at_slot(update_attested_slot, fork, spec),
        &update.attested_header.beacon.state_root,
    ) {
        return Err(Error::FinalityBranchInvalid);
    }

    // The next committee, if present, and its branch (no comparison with the store's: see above).
    if !update.is_sync_committee_update() {
        if update.next_sync_committee.is_some() {
            return Err(Error::NextCommitteeUnexpected);
        }
    } else {
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

    // The committee that signs, and what it signs (as `validate_compact`).
    let signer =
        if update_signature_period == store_period { SigningCommittee::Current } else { SigningCommittee::Next };
    let fork_version_slot = update.signature_slot.saturating_sub(1);
    let fork_version = spec.compute_fork_version(spec.epoch_at_slot(fork_version_slot));
    let domain = compute_domain(DOMAIN_SYNC_COMMITTEE, fork_version, genesis_validators_root);
    let signing_root = compute_signing_root(&update.attested_header.beacon.hash_tree_root(), &domain);
    Ok((signer, signing_root))
}

/// The contradiction between a validated update and what the instance holds, or `NoContradiction`.
pub fn contradiction(
    store: &CompactStore,
    update: &CompactUpdate,
    against: &Against,
    spec: &ChainSpec,
) -> Result<Contradiction, EvidenceError> {
    match *against {
        Against::Record { slot, root } => {
            let finalized = &update.finalized_header.beacon;
            if finalized.slot != slot {
                return Err(EvidenceError::NoContradiction);
            }
            let conflicting = finalized.hash_tree_root();
            if conflicting == root {
                return Err(EvidenceError::NoContradiction);
            }
            Ok(Contradiction::Finalized { slot, recorded: root, conflicting })
        }
        Against::NextCommittee => {
            let Some(adopted) = store.next else { return Err(EvidenceError::NoContradiction) };
            let period = spec.period_at_slot(store.finalized.slot);
            if !update.is_sync_committee_update()
                || spec.period_at_slot(update.attested_header.beacon.slot) != period
                || spec.period_at_slot(update.finalized_header.beacon.slot) != period
                || update.next_sync_committee == Some(adopted)
            {
                return Err(EvidenceError::NoContradiction);
            }
            let n = spec.preset.sync_committee_size;
            let conflicting = update.next_sync_committee.unwrap_or_else(|| CommitteeSummary::empty(n)).root();
            Ok(Contradiction::NextCommittee { period, adopted: adopted.root(), conflicting })
        }
    }
}

/// The signature's job: the signing key `aggregate − Σ non-signers` of the committee `signer` names, the
/// non-signers' keys proven by one multiproof (`residue_agg::signing_key`), the signature decoded and
/// checked in G2, H(m) hashed to G2. The job is used for its signature phases only (`Job::step2`, then
/// [`verify_signature`]); nothing applies it, and its `pending` is never written.
pub fn signature_job(
    store: &CompactStore,
    update: &CompactUpdate,
    signer: SigningCommittee,
    signing_root: &Root,
    signers: &NonSigners,
    sum: Sum,
    spec: &ChainSpec,
) -> Result<Job, StepError> {
    let committee = match signer {
        SigningCommittee::Current => &store.current,
        SigningCommittee::Next => store.next.as_ref().ok_or(Error::CommitteeUnknown)?,
    };
    let key = residue_agg::signing_key(committee, &update.sync_aggregate.bits, signers, sum)?;
    let signature = decode_signature(&update.sync_aggregate.signature)?;
    let (hm, neg_key) = hash_and_negate(signing_root, &key);
    Ok(Job {
        store_digest: store.digest(),
        update_digest: update.digest(store.fork),
        phase: Phase::Miller { hm, neg_key, signature: serialize_g2(&signature) },
        adoption: None,
        pending: PendingApply::Update {
            finalized: update.finalized_header.beacon.clone(),
            next: update.next_sync_committee,
            record: HeaderRecord::of(&update.finalized_header, store.fork, spec),
        },
    })
}

/// The Miller loop (the core's `Job::step2`), then the final exponentiation or the residue check. Any
/// failure of the check is `SignatureInvalid` (40), as in the programs.
pub fn verify_signature(job: &mut Job, pairing: &Pairing) -> Result<(), StepError> {
    job.step2()?;
    match pairing {
        Pairing::FinalExp => job.step3(),
        Pairing::Residue { c, w } => residue_handoff::residue_check(job, c, w),
    }
}

/// The whole check: [`validate_evidence`], [`contradiction`] (both cheap, first), then the signature.
/// `store` is the store the update is verified against (`steps::store_for_update` of the stored one).
#[allow(clippy::too_many_arguments)]
pub fn check_evidence(
    store: &CompactStore,
    update: &CompactUpdate,
    signers: &NonSigners,
    sum: Sum,
    against: &Against,
    current_slot: u64,
    pairing: &Pairing,
    spec: &ChainSpec,
    genesis_validators_root: &Root,
) -> Result<Freeze, EvidenceError> {
    residue_handoff::check_spec(spec)?;
    let (signer, signing_root) = validate_evidence(store, update, current_slot, spec, genesis_validators_root)?;
    let contradiction = contradiction(store, update, against, spec)?;
    let mut job = signature_job(store, update, signer, &signing_root, signers, sum, spec)?;
    verify_signature(&mut job, pairing)?;
    Ok(Freeze {
        contradiction,
        attested_root: update.attested_header.beacon.hash_tree_root(),
        signature_slot: update.signature_slot,
        update_digest: update.digest(store.fork),
    })
}

// ---------------------------------------------------------------------------------------------
// A model of an instance: store, records, freeze
// ---------------------------------------------------------------------------------------------

/// An instance as the programs keep it, for the tests: the compact store, the root of every header it
/// recorded as finalized (by slot), and the freeze. Updates run as the v0.3.1 port's one-transaction
/// `Update` with `Verify::ResidueSummary` (one multiproof, a handoff adopted by its summary; the pairing
/// completed as given).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub store: CompactStore,
    pub records: BTreeMap<u64, Root>,
    pub frozen: Option<Freeze>,
    pub spec: ChainSpec,
    pub genesis_validators_root: Root,
    /// The weak-subjectivity period in epochs, if enforced (as the programs' validity window).
    pub ws_period: Option<u64>,
}

impl Instance {
    /// An active instance with its store and the checkpoint's record.
    pub fn new(
        store: CompactStore,
        checkpoint: &BeaconBlockHeader,
        spec: ChainSpec,
        genesis_validators_root: Root,
        ws_period: Option<u64>,
    ) -> Self {
        let records = BTreeMap::from([(checkpoint.slot, checkpoint.hash_tree_root())]);
        Self { store, records, frozen: None, spec, genesis_validators_root, ws_period }
    }

    fn live(&self) -> Result<(), EvidenceError> {
        if self.frozen.is_some() {
            return Err(EvidenceError::Frozen);
        }
        Ok(())
    }

    fn within_period(&self, current_slot: u64) -> Result<(), EvidenceError> {
        if let Some(p) = self.ws_period {
            ws::check_within(self.store.finalized.slot, current_slot, p, &self.spec)?;
        }
        Ok(())
    }

    /// Apply `update` (encoded in `layout`); record its finalized header if the store advances.
    pub fn update(
        &mut self,
        update: &CompactUpdate,
        layout: LcFork,
        signers: &NonSigners,
        pairing: &Pairing,
        current_slot: u64,
    ) -> Result<ApplyOutcome, EvidenceError> {
        self.live()?;
        self.within_period(current_slot)?;
        let spec = &self.spec;
        let store = steps::store_for_update(&self.store, layout, update.attested_header.beacon.slot, spec)?;
        let mut job =
            residue_agg::step1(&store, update, signers, current_slot, spec, &self.genesis_validators_root, 1, Sum::Batch)?;
        verify_signature(&mut job, pairing)?;
        if job.adoption_pending() {
            residue_handoff::adopt_summary(&mut job)?;
        }
        let (next, outcome) = steps::apply(&self.store, &job, spec)?;
        if outcome.advanced {
            self.records.insert(next.finalized.slot, next.finalized.hash_tree_root());
        }
        self.store = next;
        Ok(outcome)
    }

    /// What an attestation reads: the root of the record at `slot`.
    pub fn attest(&self, slot: u64) -> Result<Root, EvidenceError> {
        self.live()?;
        self.records.get(&slot).copied().ok_or(EvidenceError::NoRecord)
    }

    /// Submit `update` (encoded in `layout`) as evidence against the record at `record_slot`, or against
    /// the adopted next committee (`None`). On success the instance is frozen with the evidence recorded.
    #[allow(clippy::too_many_arguments)]
    pub fn evidence(
        &mut self,
        update: &CompactUpdate,
        layout: LcFork,
        signers: &NonSigners,
        sum: Sum,
        record_slot: Option<u64>,
        pairing: &Pairing,
        current_slot: u64,
    ) -> Result<Freeze, EvidenceError> {
        self.live()?;
        self.within_period(current_slot)?;
        let against = match record_slot {
            Some(slot) => Against::Record { slot, root: *self.records.get(&slot).ok_or(EvidenceError::NoRecord)? },
            None => Against::NextCommittee,
        };
        let spec = &self.spec;
        let store = steps::store_for_update(&self.store, layout, update.attested_header.beacon.slot, spec)?;
        let freeze = check_evidence(
            &store,
            update,
            signers,
            sum,
            &against,
            current_slot,
            pairing,
            spec,
            &self.genesis_validators_root,
        )?;
        self.frozen = Some(freeze);
        Ok(freeze)
    }
}

// ---------------------------------------------------------------------------------------------
// BLS pieces: copies of `residue_agg`'s private ones (themselves copies of the core's, `steps.rs`)
// ---------------------------------------------------------------------------------------------

/// The signature as a point of G2, refused as in `verify_checked` (the core's `decode_signature`).
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

/// H(m) on G2 and −key, both affine and serialized uncompressed (the core's `hash_and_negate`).
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
