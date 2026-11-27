//! `ferrata-advance`: the attestation program's public job path as an updater plans it. For an
//! instance's store and a light-client update: whether the update moves the instance, the job it
//! opens, and the transactions of the job's steps (`Open` at `instance(id)`, then `Miller`, the
//! committee's `Adopt` parts and `Finish` at `job(id, job_id)`), each checked first by running the
//! program's own step code (`ferrata_core::steps`) natively, so that a transaction is paid for only
//! when the program would accept it.
//!
//! Shared by `ferrata-updater` (which reads the chain and its sources, and sends) and
//! `ferrata_attest_module`'s `advance` (which takes the instance's state, the update and the
//! committee in its request): one implementation. Host code: no program or guest links it.

#![forbid(unsafe_code)]

use anyhow::{anyhow, bail, ensure, Result};
use ferrata_core::{
    fork::{ChainSpec, LcFork},
    merkle::ZERO,
    steps::{self, Job, Phase},
    store::ApplyOutcome,
    types::{LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    witness::{uncompressed_keys, CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_lez_wire::{header_actor, instance_actor, job_actor, job_id, Config, JobState, Message};
use lee_core::account::{AccountId, Actor};

// ---- what beacon nodes serve ------------------------------------------------------------------

/// The layout a beacon node serves light-client data of `slot` in.
pub fn served_layout(spec: &ChainSpec, slot: u64) -> Option<LcFork> {
    spec.fork_at_epoch(spec.epoch_at_slot(slot)).lc_fork()
}

/// A `LightClientUpdate` as the `updates` endpoint serves it, in the layout of the fork its
/// attested header falls in.
pub fn decode_update(bytes: &[u8], config: &Config) -> Result<(LightClientUpdate, LcFork)> {
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    for layout in [LcFork::Electra, LcFork::Gloas] {
        if let Ok(update) = LightClientUpdate::decode(bytes, layout, n) {
            if served_layout(&spec, update.attested_header.beacon.slot) == Some(layout) {
                return Ok((update, layout));
            }
        }
    }
    bail!("an update of {} bytes in no layout this chain's schedule allows", bytes.len())
}

/// A `LightClientFinalityUpdate` as the `LightClientUpdate` it stands for (empty committee
/// fields): Electra (offsets) or Gloas (fixed size) layout.
pub fn decode_finality_update(b: &[u8], config: &Config) -> Result<(LightClientUpdate, LcFork)> {
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8 bytes"));
    const H: usize = 112 + 32 + 11 * 32;
    if b.len() == 2 * H + 9 * 32 + 160 + 8 {
        let update = LightClientUpdate {
            attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).map_err(|e| anyhow!("{e:?}"))?,
            next_sync_committee: SyncCommittee::empty(n),
            next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
            finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).map_err(|e| anyhow!("{e:?}"))?,
            finality_branch: b[2 * H..2 * H + 288].chunks(32).map(|c| c.try_into().expect("32")).collect(),
            sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], n).map_err(|e| anyhow!("{e:?}"))?,
            signature_slot: u64_at(2 * H + 448),
        };
        ensure!(
            served_layout(&spec, update.attested_header.beacon.slot) == Some(LcFork::Gloas),
            "a Gloas finality update before Gloas"
        );
        return Ok((update, LcFork::Gloas));
    }
    ensure!(b.len() > 400, "a finality update of {} bytes", b.len());
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes")) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    ensure!(att == 400 && fin > att && fin < b.len(), "LightClientFinalityUpdate offsets {att}, {fin}");
    let update = LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).map_err(|e| anyhow!("{e:?}"))?,
        next_sync_committee: SyncCommittee::empty(n),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).map_err(|e| anyhow!("{e:?}"))?,
        finality_branch: b[8..232].chunks(32).map(|c| c.try_into().expect("32")).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], n).map_err(|e| anyhow!("{e:?}"))?,
        signature_slot: u64_at(392),
    };
    ensure!(
        served_layout(&spec, update.attested_header.beacon.slot) == Some(LcFork::Electra),
        "an Electra finality update under Gloas"
    );
    Ok((update, LcFork::Electra))
}

/// Whether a light-client header is an offset field of an SSZ container in `layout`: Capella's to Electra's carry an
/// execution payload header, whose `extra_data` varies in length; Altair's and Gloas's are of fixed size.
fn header_is_variable(layout: LcFork) -> bool {
    matches!(layout, LcFork::Capella | LcFork::Deneb | LcFork::Electra)
}

/// An SSZ container: each field's encoding in the fixed part, or, for a variable field (`true`), its offset there and
/// its encoding after the fixed part, in order.
fn ssz_container(fields: &[(bool, Vec<u8>)]) -> Vec<u8> {
    let fixed: usize = fields.iter().map(|(var, b)| if *var { 4 } else { b.len() }).sum();
    let (mut out, mut tail) = (Vec::new(), Vec::new());
    for (var, b) in fields {
        if *var {
            out.extend_from_slice(&u32::try_from(fixed + tail.len()).expect("small").to_le_bytes());
            tail.extend_from_slice(b);
        } else {
            out.extend_from_slice(b);
        }
    }
    out.extend_from_slice(&tail);
    out
}

fn ssz_roots(roots: &[Root]) -> Vec<u8> {
    roots.concat()
}

/// The SSZ of `update` as the `updates` endpoint serves it, in `layout`: the inverse of [`decode_update`] (SSZ has one
/// encoding per value, so the bytes a beacon node served come back). Used for an update read from another party's
/// `Open` (`CompactUpdate`, its next committee completed from the full committee whose summary it carries).
pub fn encode_update(update: &LightClientUpdate, layout: LcFork) -> Vec<u8> {
    let var = header_is_variable(layout);
    let mut committee = update.next_sync_committee.pubkeys.concat();
    committee.extend_from_slice(&update.next_sync_committee.aggregate_pubkey);
    ssz_container(&[
        (var, ferrata_core::witness::encode_light_client_header(&update.attested_header, layout)),
        (false, committee),
        (false, ssz_roots(&update.next_sync_committee_branch)),
        (var, ferrata_core::witness::encode_light_client_header(&update.finalized_header, layout)),
        (false, ssz_roots(&update.finality_branch)),
        (false, ferrata_core::witness::encode_sync_aggregate(&update.sync_aggregate)),
        (false, update.signature_slot.to_le_bytes().to_vec()),
    ])
}

/// The SSZ of the `LightClientFinalityUpdate` that `update` stands for (its committee fields empty), in `layout`: the
/// inverse of [`decode_finality_update`].
pub fn encode_finality_update(update: &LightClientUpdate, layout: LcFork) -> Vec<u8> {
    let var = header_is_variable(layout);
    ssz_container(&[
        (var, ferrata_core::witness::encode_light_client_header(&update.attested_header, layout)),
        (var, ferrata_core::witness::encode_light_client_header(&update.finalized_header, layout)),
        (false, ssz_roots(&update.finality_branch)),
        (false, ferrata_core::witness::encode_sync_aggregate(&update.sync_aggregate)),
        (false, update.signature_slot.to_le_bytes().to_vec()),
    ])
}

/// The update an `Open` carries (`Message::Open`'s compact encoding, in the layout its first byte names), as a source
/// would serve it: a period update (its next committee present: the full committee `committee` gives for the summary's
/// keys root; `None` if it gives none) or a finality update, with its bytes in the layout a beacon node serves it in
/// ([`encode_update`], [`encode_finality_update`]). `None` for an encoding that does not decode, or a layout no beacon
/// node serves the update in (an `Open` may name a layout past the one served, `layout_for`'s store layout): such an
/// `Open` names no update a source serves.
pub fn served_update_of_open(
    compact: &[u8],
    config: &Config,
    committee: impl Fn(&Root) -> Option<SyncCommittee>,
) -> Option<(bool, Vec<u8>, LightClientUpdate)> {
    let n = config.sync_committee_size as usize;
    let layout = CompactUpdate::layout_of(compact).ok()?;
    let c = CompactUpdate::decode(compact, layout, n).ok()?;
    let served = served_layout(&config.spec(), c.attested_header.beacon.slot)?;
    if served != layout {
        return None;
    }
    let (period, next) = match &c.next_sync_committee {
        Some(summary) => {
            let full = committee(&summary.pubkeys_root)?;
            if ferrata_core::witness::CommitteeSummary::of(&full) != *summary {
                return None;
            }
            (true, full)
        }
        None => (false, SyncCommittee::empty(n)),
    };
    let update = LightClientUpdate {
        attested_header: c.attested_header,
        next_sync_committee: next,
        next_sync_committee_branch: c.next_sync_committee_branch,
        finalized_header: c.finalized_header,
        finality_branch: c.finality_branch,
        sync_aggregate: c.sync_aggregate,
        signature_slot: c.signature_slot,
    };
    let raw = if period { encode_update(&update, layout) } else { encode_finality_update(&update, layout) };
    // The bytes decode back to the same update (they are what a source serving it would serve).
    let back = if period { decode_update(&raw, config) } else { decode_finality_update(&raw, config) };
    match back {
        Ok((u, l)) if u == update && l == layout => Some((period, raw, update)),
        _ => None,
    }
}

/// A committee as a list of keys: the keys, then the aggregate key (48 bytes each).
pub fn decode_committee(b: &[u8]) -> Result<SyncCommittee> {
    ensure!(!b.is_empty() && b.len().is_multiple_of(48), "a committee file of {} bytes", b.len());
    let mut keys: Vec<[u8; 48]> = b.chunks(48).map(|c| c.try_into().expect("48")).collect();
    let aggregate_pubkey = keys.pop().expect("non-empty");
    Ok(SyncCommittee { pubkeys: keys, aggregate_pubkey })
}

// ---- the program's checks, natively -----------------------------------------------------------

/// The committee that signs `update`: `current` if its signature slot is in the store's period,
/// else `next`.
pub fn signing_committee<'a>(
    store: &CompactStore,
    update: &LightClientUpdate,
    spec: &ChainSpec,
    current: Option<&'a SyncCommittee>,
    next: Option<&'a SyncCommittee>,
) -> Option<&'a SyncCommittee> {
    if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(store.finalized.slot) {
        current
    } else {
        next
    }
}

/// The witness of `update`'s non-signers, from the committee that signs it.
pub fn signer_witness(committee: &SyncCommittee, update: &LightClientUpdate) -> SignerWitness {
    SignerWitness::build(committee, &update.sync_aggregate.bits).unwrap_or_default()
}

/// The job as the program would run it: the job after step 1, and, if every step passes, what
/// applying it does to `store` (as `harness::plan_job`).
pub fn plan_job(
    store: &CompactStore,
    compact: &CompactUpdate,
    witness: &SignerWitness,
    update: &LightClientUpdate,
    config: &Config,
) -> (Option<Job>, Option<ApplyOutcome>) {
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let Ok(opened) = steps::step1(
        store,
        compact,
        witness,
        update.signature_slot,
        &spec,
        &config.genesis_validators_root,
        config.adoption_parts,
    ) else {
        return (None, None);
    };
    let finish = || -> Option<ApplyOutcome> {
        let mut job = opened.clone();
        job.step2().ok()?;
        job.step3().ok()?;
        if job.adoption_pending() {
            let keys = uncompressed_keys(&update.next_sync_committee).ok()?;
            let parts = config.adoption_parts;
            let chunk = n / parts as usize;
            for p in 0..parts {
                job.adopt(p, &keys[p as usize * chunk..(p as usize + 1) * chunk], n).ok()?;
            }
        }
        steps::apply(store, &job, &spec).ok().map(|(_, outcome)| outcome)
    };
    let outcome = finish();
    (Some(opened), outcome)
}

/// The layout an update is sent in to a store in `store_fork`: the layout it is served in, or the
/// store's own if that is later.
pub fn layout_for(config: &Config, store_fork: LcFork, update: &LightClientUpdate) -> Option<LcFork> {
    served_layout(&config.spec(), update.attested_header.beacon.slot).map(|l| l.max(store_fork))
}

// ---- validity windows -------------------------------------------------------------------------

/// The timestamp validity window the program gives a step's transaction, in Unix milliseconds: from `start_ms`
/// (inclusive) to `end_ms` (exclusive); `None` is unbounded. LEZ refuses a transaction executed outside its window and
/// charges it the whole gas limit (`OutOfValidityWindow`), and the native checks of [`evaluate`] do not see the clock,
/// so a sender checks the window against the chain's clock before it pays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

impl Window {
    /// No window (`Miller`, `Adopt`, `Finish`).
    pub const ANY: Window = Window { start_ms: None, end_ms: None };

    /// Whether a transaction settled at `now_ms`, or up to `margin_ms` later, falls inside.
    pub fn admits(&self, now_ms: u64, margin_ms: u64) -> bool {
        self.start_ms.is_none_or(|s| now_ms >= s) && self.end_ms.is_none_or(|e| now_ms.saturating_add(margin_ms) < e)
    }
}

/// `Init`'s window, as the program's `period_window`: until the end of the weak-subjectivity period of the
/// checkpoint's header (at `checkpoint_slot`).
pub fn init_window(config: &Config, checkpoint_slot: u64) -> Window {
    Window { start_ms: None, end_ms: config.ws_end_ms(checkpoint_slot) }
}

/// `Open`'s window, as the program's `update_window`: from the start of the update's signature slot to the end of the
/// weak-subjectivity period of the store's finalized header (at `store_finalized_slot`). `None` when the update is
/// signed at or after that end: the program refuses it whatever the time (`OutsideWeakSubjectivityPeriod`).
pub fn open_window(config: &Config, store_finalized_slot: u64, signature_slot: u64) -> Option<Window> {
    let start_ms = Some(config.slot_start_ms(signature_slot));
    match config.ws_end_slot(store_finalized_slot) {
        None => Some(Window { start_ms, end_ms: None }),
        Some(end) if signature_slot >= end => None,
        Some(end) => Some(Window { start_ms, end_ms: Some(config.slot_start_ms(end)) }),
    }
}

// ---- a job and its steps ----------------------------------------------------------------------

/// A step of the public job path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Init,
    Open,
    Miller,
    Adopt(u8),
    Finish,
}

impl Step {
    pub fn name(&self) -> String {
        match self {
            Step::Init => "init".into(),
            Step::Open => "open".into(),
            Step::Miller => "miller".into(),
            Step::Adopt(p) => format!("adopt{p}"),
            Step::Finish => "finish".into(),
        }
    }

    pub fn byte(&self) -> u8 {
        match self {
            Step::Init => 0,
            Step::Open => 1,
            Step::Miller => 2,
            Step::Adopt(p) => 3 + p,
            Step::Finish => 255,
        }
    }
}

/// What the program would do with a candidate update on the current store.
pub enum Verdict {
    Useful(Box<Planned>),
    /// Valid or not, it would not move the store (older than what the instance holds).
    NoChange,
    /// Its signature falls in a period whose committee the store does not hold yet.
    Unverifiable,
    /// The program would refuse it.
    Invalid(String),
}

/// An update the program would accept, with its job.
#[derive(Clone, Debug)]
pub struct Planned {
    pub update: LightClientUpdate,
    pub layout: LcFork,
    pub compact: CompactUpdate,
    pub witness: SignerWitness,
    pub jid: Root,
    pub outcome: ApplyOutcome,
}

/// Judges `update` against the instance `instance_id` (configuration `config`) holding `store`, with
/// the full committees the store summarises (`current`, `next`, when known): the program's own
/// checks, run natively.
pub fn evaluate(
    config: &Config,
    instance_id: &Root,
    store: &CompactStore,
    update: &LightClientUpdate,
    current: Option<&SyncCommittee>,
    next: Option<&SyncCommittee>,
) -> Verdict {
    let spec = config.spec();
    let Some(layout) = layout_for(config, store.fork, update) else {
        return Verdict::Invalid("no light-client layout at its slot".into());
    };
    let Ok(store_u) = steps::store_for_update(store, layout, update.attested_header.beacon.slot, &spec) else {
        return Verdict::Invalid(format!("the store cannot move to layout {layout:?}"));
    };
    let sig_period = spec.period_at_slot(update.signature_slot);
    let store_period = spec.period_at_slot(store.finalized.slot);
    if sig_period != store_period && sig_period != store_period + 1 {
        return if sig_period < store_period { Verdict::NoChange } else { Verdict::Unverifiable };
    }
    let Some(committee) = signing_committee(store, update, &spec, current, next) else {
        return Verdict::Unverifiable;
    };
    let compact = CompactUpdate::of(update);
    let witness = signer_witness(committee, update);
    let (opened, outcome) = plan_job(&store_u, &compact, &witness, update, config);
    match (opened, outcome) {
        (Some(_), Some(o)) if o.advanced || o.adopted || o.rotated => {
            let jid = job_id(instance_id, &store_u.digest(), &compact.digest(layout));
            Verdict::Useful(Box::new(Planned { update: update.clone(), layout, compact, witness, jid, outcome: o }))
        }
        (Some(_), Some(_)) => Verdict::NoChange,
        _ if !steps::update_advances(&store_u, &compact, &spec)
            && update.finalized_header.beacon.slot <= store.finalized.slot
            && (update.next_sync_committee.is_empty() || store.next.is_some()) =>
        {
            Verdict::NoChange
        }
        _ => Verdict::Invalid("the program's checks refuse it".into()),
    }
}

/// The job's next step, as the chain holds the job (`None`: not opened yet).
pub fn next_step(job: Option<&Job>) -> Step {
    match job {
        None => Step::Open,
        Some(job) => match &job.phase {
            Phase::Miller { .. } => Step::Miller,
            Phase::FinalExp { .. } | Phase::Verified => match job.adoption.as_ref().filter(|a| !a.complete) {
                Some(a) => Step::Adopt(a.chunk_roots.len() as u8),
                None => Step::Finish,
            },
        },
    }
}

/// The job as the chain holds it at `job(id, job_id)` (`JobState`, Borsh; empty: no job).
pub fn job_of_state(state: &[u8]) -> Result<Option<Job>> {
    if state.is_empty() {
        return Ok(None);
    }
    let js: JobState = borsh::from_slice(state).map_err(|e| anyhow!("job state: {e}"))?;
    Ok(Some(Job::decode(&js.job).map_err(|e| anyhow!("job: {e:?}"))?))
}

/// Every step of a fresh job, in order: `Open`, `Miller`, the `Adopt` parts when the update adopts
/// the next committee, `Finish`.
pub fn job_steps(config: &Config, p: &Planned) -> Vec<Step> {
    let mut out = vec![Step::Open, Step::Miller];
    if !p.update.next_sync_committee.is_empty() && p.outcome.adopted {
        out.extend((0..config.adoption_parts).map(Step::Adopt));
    }
    out.push(Step::Finish);
    out
}

/// One transaction of the job path: its root actor, its message (Borsh) and every actor it names.
#[derive(Clone, Debug)]
pub struct StepTx {
    pub root: Actor,
    pub message: Vec<u8>,
    pub actors: Vec<Actor>,
}

/// The `Adopt` part `p` of the committee `keys` (uncompressed) at `job_actor`; the last part of an
/// initialization (`record_slot`) also names the instance and its first record.
pub fn adopt_step(
    program: AccountId,
    instance_id: &Root,
    adoption_parts: u8,
    job_actor: Actor,
    keys: &[[u8; 96]],
    p: u8,
    record_slot: Option<u64>,
) -> StepTx {
    let chunk = keys.len() / adoption_parts as usize;
    let mut actors = vec![job_actor];
    if p + 1 == adoption_parts {
        if let Some(slot) = record_slot {
            actors.extend([instance_actor(program, instance_id), header_actor(program, instance_id, slot)]);
        }
    }
    StepTx {
        root: job_actor,
        message: borsh::to_vec(&Message::Adopt {
            part: p,
            keys: keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec(),
        })
        .expect("borsh"),
        actors,
    }
}

/// The transaction of `step` of the job `p` opens on the instance `instance_id` of the program at
/// `program`.
pub fn job_step(program: AccountId, instance_id: &Root, config: &Config, p: &Planned, step: Step) -> Result<StepTx> {
    let instance = instance_actor(program, instance_id);
    let job = job_actor(program, instance_id, &p.jid);
    let fin = p.update.finalized_header.beacon.slot;
    Ok(match step {
        Step::Open => StepTx {
            root: instance,
            message: borsh::to_vec(&Message::Open { update: p.compact.encode(p.layout), signers: p.witness.encode() })?,
            actors: vec![instance, job],
        },
        Step::Miller => StepTx { root: job, message: borsh::to_vec(&Message::Miller)?, actors: vec![job] },
        Step::Adopt(part) => {
            let keys = uncompressed_keys(&p.update.next_sync_committee).map_err(|e| anyhow!("{e:?}"))?;
            adopt_step(program, instance_id, config.adoption_parts, job, &keys, part, None)
        }
        Step::Finish => {
            let mut actors = vec![job, instance];
            if p.outcome.advanced {
                actors.push(header_actor(program, instance_id, fin));
            }
            StepTx { root: job, message: borsh::to_vec(&Message::Finish)?, actors }
        }
        Step::Init => bail!("not a job step"),
    })
}
