//! Ferrata on LEZ v0.4: an Ethereum sync-committee light client as one program of actors.
//!
//! See `ferrata_lez_wire` for the actors and messages. Every handler checks that it runs at the
//! actor the message is for (the receiver is the PDA the message implies), and every internal
//! message is accepted only from the actor of this program that sends it. Refusals exit with a
//! code instead of panicking: LEZ then charges only the cycles used
//! (`lee::validated_state_diff::charged_cycles`), not the whole gas limit.
//!
//! `Attest` at `header(id, slot)` proves facts of the recorded block (accounts, storage slots,
//! event logs, accumulator leaves, with conditions) and replies to its caller with their
//! statement identifiers: `ferrata_lez_wire::attest::evaluate`, the function a private consumer
//! runs on the record it reads.
//!
//! An instance's store starts in the layout of its bootstrap and moves to the layout of a later
//! fork of the instance's own schedule with the first update attested under that fork
//! (`ferrata_core::steps::store_for_update`): the job starts from the upgraded store, and applying
//! the verified job writes the upgrade.
//!
//! **Weak subjectivity.** Every transaction that creates or extends an instance (`Init`, `Open`,
//! `VerifyAll`, `Succeed`) is valid only before the weak-subjectivity period of the instance's
//! finalized header ends (`Config::ws_end_ms`), by the sequencer's clock: a store older than its
//! period is never advanced, and is replaced by a new `Init` from a trusted checkpoint (a new
//! root id). An update signed at or after that end is refused in the guest (core code 53).
//!
//! **Successors.** A fork that is not in the schedule fixed at `Init` is crossed by a successor
//! (`Succeed` at the parent, `Spawn` at the successor): the parent checks the three invariants of
//! `docs/design/attestation.md` §3.5 and hands its store and root id to the successor, which
//! follows the chain under the new schedule. Records carry the root id, and `Attest` serves a
//! request bound to the root id as well as one bound to the instance.

use ferrata_core::{
    fork::{ChainSpec, LcFork},
    steps::{self, Job, PendingApply, StepError},
    witness::{CompactBootstrap, CompactStore, CompactUpdate, SignerWitness},
    Error, Root,
};
use ferrata_lez_wire::{
    attest, code, header_actor, header_seed, instance_actor, instance_id, instance_seed, job_actor, job_id, job_seed,
    successor_id, Config, InstanceState, JobState, Message, RecordState, UncompressedKey, ATTESTED_SELECTOR,
    FINALIZED_SELECTOR, SUCCEEDED_SELECTOR, UPGRADED_SELECTOR,
};
use lee_core::{
    account::{AccountId, ActorState},
    program::{read_input_frame, ProgramEvent, ReceiveInput, Response, StateReply, SyncMessage},
};
use risc0_zkvm::guest::env;

type Refusal = u8;

fn main() {
    let input: ReceiveInput = borsh::from_slice(&read_input_frame()).expect("the runtime's input decodes");
    match handle(&input) {
        Ok(response) => {
            response.into_transition(input).commit();
            env::exit(0)
        }
        Err(code) => env::exit(code),
    }
}

fn core(e: Error) -> Refusal {
    e.code() as Refusal
}

fn step(e: StepError) -> Refusal {
    e.code() as Refusal
}

fn handle(input: &ReceiveInput) -> Result<Response, Refusal> {
    let program = input.receiver.program_account_id;
    let message: Message = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    match message {
        Message::Init { config, checkpoint, bootstrap } => init(input, program, config, &checkpoint, &bootstrap),
        Message::Open { update, signers } => open(input, program, &update, &signers),
        Message::Miller => miller(input, program),
        Message::Adopt { part, keys } => adopt(input, program, part, &keys),
        Message::Finish => finish(input, program),
        Message::Read | Message::ReadPadded { .. } => read(input),
        Message::Attest { request } => attest(input, program, &request),
        Message::VerifyAll { config, checkpoint, store, update, signers, adoption } => {
            verify_all(program, &config, &checkpoint, &store, &update, &signers, &adoption)
        }
        Message::JobOpen { instance_id, job_id, committee_size, job } => {
            job_open(input, program, instance_id, job_id, committee_size, job)
        }
        Message::Apply { instance_id, job_id, job } => apply(input, program, &instance_id, &job_id, &job),
        Message::ApplyVerified { instance_id, job } => apply_verified(input, program, &instance_id, &job),
        Message::Record { instance_id, slot, chain_id, record, root_id } => {
            write_record(input, program, instance_id, slot, chain_id, record, root_id)
        }
        Message::Succeed { config, checkpoint } => succeed(input, program, config, &checkpoint),
        Message::Spawn { root_id, parent_id, checkpoint, config, store } => {
            spawn(input, program, root_id, parent_id, &checkpoint, config, store)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// State helpers
// ---------------------------------------------------------------------------------------------

fn instance_state(input: &ReceiveInput, program: AccountId) -> Result<InstanceState, Refusal> {
    if input.pre_state.is_empty() {
        return Err(code::NO_INSTANCE);
    }
    let state: InstanceState = borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?;
    if input.receiver != instance_actor(program, &state.id) {
        return Err(code::WRONG_RECEIVER);
    }
    Ok(state)
}

fn job_state(input: &ReceiveInput, program: AccountId) -> Result<(JobState, Job), Refusal> {
    if input.pre_state.is_empty() {
        return Err(code::NO_JOB);
    }
    let state: JobState = borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?;
    if input.receiver != job_actor(program, &state.instance_id, &state.job_id) {
        return Err(code::WRONG_RECEIVER);
    }
    let job = Job::decode(&state.job).map_err(|_| code::BAD_STATE)?;
    Ok((state, job))
}

fn encode<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn check_config(config: &Config) -> Result<(), Refusal> {
    let n = config.sync_committee_size as usize;
    let parts = config.adoption_parts as usize;
    if config.fork().is_none() || !n.is_power_of_two() || parts == 0 || !parts.is_power_of_two() || n % parts != 0 {
        return Err(code::BAD_CONFIG);
    }
    // A weak-subjectivity period, and a clock to end it by.
    if config.ws_period_epochs == 0 || config.seconds_per_slot == 0 || config.slots_per_epoch == 0 {
        return Err(code::BAD_CONFIG);
    }
    Ok(())
}

/// The timestamp validity window of a transaction that extends a store whose finalized header is
/// at `finalized_slot` with an update signed at `signature_slot`: from the start of the signature
/// slot (`current_slot >= signature_slot`) to the end of the weak-subjectivity period of the
/// finalized header (`ferrata_core::ws`). An update signed at or after that end could only be
/// applied outside the period: `OutsideWeakSubjectivityPeriod`, in the guest.
fn update_window(
    response: Response,
    config: &Config,
    finalized_slot: u64,
    signature_slot: u64,
) -> Result<Response, Refusal> {
    let start = config.slot_start_ms(signature_slot);
    match config.ws_end_slot(finalized_slot) {
        None => Ok(response.timestamp_window(start..)),
        Some(end) if signature_slot >= end => Err(core(Error::OutsideWeakSubjectivityPeriod)),
        Some(end) => response
            .try_timestamp_window(start..config.slot_start_ms(end))
            .map_err(|_| core(Error::OutsideWeakSubjectivityPeriod)),
    }
}

/// The window of a transaction that only needs the store inside its period: up to its end.
fn period_window(response: Response, config: &Config, finalized_slot: u64) -> Response {
    match config.ws_end_ms(finalized_slot) {
        None => response,
        Some(end) => response.timestamp_window(..end),
    }
}

/// An update decoded in the layout its encoding names, and the store it is verified against:
/// `store`, or `store` upgraded to that layout when it is the layout of a later fork that the
/// instance's schedule puts at the update's attested header (`steps::store_for_update`). Any
/// other layout is `ForkMismatch`.
fn update_and_store(
    store: &CompactStore,
    update: &[u8],
    committee_size: usize,
    spec: &ChainSpec,
) -> Result<(CompactUpdate, CompactStore), Refusal> {
    let layout = CompactUpdate::layout_of(update).map_err(core)?;
    let update = CompactUpdate::decode(update, layout, committee_size).map_err(core)?;
    let store = steps::store_for_update(store, layout, update.attested_header.beacon.slot, spec).map_err(core)?;
    Ok((update, store))
}

/// `instance(id)` → `job(id, job_id)`, creating the job's account under its seed.
fn open_job(program: AccountId, id: &Root, committee_size: u32, job: &Job) -> SyncMessage {
    let jid = job_id(id, &job.store_digest, &job.update_digest);
    SyncMessage::new(
        job_actor(program, id, &jid),
        &Message::JobOpen { instance_id: *id, job_id: jid, committee_size, job: job.encode() },
    )
    .with_pda_seeds([job_seed(id, &jid)])
}

/// The job is verified: clear it and hand it to its instance.
fn complete(program: AccountId, state: &JobState, job: &Job) -> Response {
    Response::set_state(ActorState::empty()).send(SyncMessage::new(
        instance_actor(program, &state.instance_id),
        &Message::Apply { instance_id: state.instance_id, job_id: state.job_id, job: job.encode() },
    ))
}

// ---------------------------------------------------------------------------------------------
// instance(id)
// ---------------------------------------------------------------------------------------------

fn init(
    input: &ReceiveInput,
    program: AccountId,
    config: Config,
    checkpoint: &Root,
    bootstrap: &[u8],
) -> Result<Response, Refusal> {
    let id = instance_id(&config, checkpoint);
    if input.receiver != instance_actor(program, &id) {
        return Err(code::WRONG_RECEIVER);
    }
    if !input.pre_state.is_empty() {
        return Err(code::ALREADY_INITIALIZED);
    }
    check_config(&config)?;
    let fork = config.fork().ok_or(code::BAD_CONFIG)?;
    let spec = config.spec();
    let bootstrap = CompactBootstrap::decode(bootstrap, fork).map_err(core)?;
    let (store, job) = steps::initialize(checkpoint, &bootstrap, fork, &spec, config.adoption_parts).map_err(step)?;
    let n = config.sync_committee_size;
    let checkpoint_slot = bootstrap.header.beacon.slot;
    let window = config.clone();
    let state =
        InstanceState { id, config, active: false, store: store.encode(), applied: 0, root_id: id, parent_id: [0; 32] };
    // The checkpoint must be inside its own weak-subjectivity period when the instance is created.
    Ok(period_window(
        Response::set_state(encode(&state)).send(open_job(program, &id, n, &job)),
        &window,
        checkpoint_slot,
    ))
}

fn open(input: &ReceiveInput, program: AccountId, update: &[u8], signers: &[u8]) -> Result<Response, Refusal> {
    let state = instance_state(input, program)?;
    if !state.active {
        return Err(code::INSTANCE_INACTIVE);
    }
    let config = &state.config;
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let stored = CompactStore::decode(&state.store).map_err(|_| code::BAD_STATE)?;
    let (update, store) = update_and_store(&stored, update, n, &spec)?;
    let signers = SignerWitness::decode(signers, n).map_err(core)?;
    // `current_slot >= signature_slot` is enforced by the transaction's validity window: it is
    // valid only from the start of the signature slot, by the sequencer's clock, and only until
    // the weak-subjectivity period of the stored finalized header ends.
    let job = steps::step1(
        &store,
        &update,
        &signers,
        update.signature_slot,
        &spec,
        &config.genesis_validators_root,
        config.adoption_parts,
    )
    .map_err(step)?;
    let response = Response::keep_state().send(open_job(program, &state.id, config.sync_committee_size, &job));
    update_window(response, config, stored.finalized.slot, update.signature_slot)
}

fn apply(
    input: &ReceiveInput,
    program: AccountId,
    instance: &Root,
    job_id: &Root,
    job: &[u8],
) -> Result<Response, Refusal> {
    let state = instance_state(input, program)?;
    if *instance != state.id {
        return Err(code::WRONG_INSTANCE);
    }
    // Only this instance's job actor of this program may apply a job here.
    if input.from != Some(job_actor(program, &state.id, job_id)) {
        return Err(code::WRONG_SENDER);
    }
    let job = Job::decode(job).map_err(|_| code::BAD_MESSAGE)?;
    apply_job(program, state, &job)
}

fn apply_verified(input: &ReceiveInput, program: AccountId, instance: &Root, job: &[u8]) -> Result<Response, Refusal> {
    let state = instance_state(input, program)?;
    if *instance != state.id {
        return Err(code::WRONG_INSTANCE);
    }
    // The private placement: the sender ran privately, so its account is not known, but its
    // program is: LEZ binds a private transition's `from.program_account_id` to the image the
    // proof verified. Every handler of this program that sends `ApplyVerified` verified the job
    // first (`verify_all`).
    if !input.from_own_program() {
        return Err(code::WRONG_SENDER);
    }
    let job = Job::decode(job).map_err(|_| code::BAD_MESSAGE)?;
    if matches!(job.pending, PendingApply::Activate { .. }) {
        return Err(code::WRONG_SENDER);
    }
    apply_job(program, state, &job)
}

fn apply_job(program: AccountId, mut state: InstanceState, job: &Job) -> Result<Response, Refusal> {
    let activate = matches!(job.pending, PendingApply::Activate { .. });
    if activate == state.active {
        // An activation of an active instance, or an update of an inactive one.
        return Err(code::INSTANCE_INACTIVE);
    }
    let spec = state.config.spec();
    let store = CompactStore::decode(&state.store).map_err(|_| code::BAD_STATE)?;
    // The job started from this store, or from this store upgraded to a later fork's layout
    // (`open`, `verify_all`); in the second case the store returned has that layout.
    let (next, outcome) = steps::apply(&store, job, &spec).map_err(step)?;
    let was = state.store.first().copied();
    state.store = next.encode();
    let now = state.store.first().copied();
    state.applied += 1;
    state.active = true;
    let id = state.id;
    let chain_id = state.config.chain_id;
    let root_id = state.root_id;
    let mut response = Response::set_state(encode(&state));
    if let (Some(was), Some(now)) = (was, now) {
        // The store's encoding starts with its layout.
        if was != now {
            response = response.event(ProgramEvent { selector: UPGRADED_SELECTOR, data: vec![was, now] });
        }
    }
    if activate || outcome.advanced {
        let record = job.pending.record();
        let slot = record.beacon.slot;
        let mut data = slot.to_le_bytes().to_vec();
        data.extend_from_slice(&record.beacon.hash_tree_root());
        response = response
            .send(
                SyncMessage::new(
                    header_actor(program, &id, slot),
                    &Message::Record { instance_id: id, slot, chain_id, record: record.encode(), root_id },
                )
                .with_pda_seeds([header_seed(&id, slot)]),
            )
            .event(ProgramEvent { selector: FINALIZED_SELECTOR, data });
    }
    Ok(response)
}

// ---------------------------------------------------------------------------------------------
// job(id, job_id)
// ---------------------------------------------------------------------------------------------

fn job_open(
    input: &ReceiveInput,
    program: AccountId,
    instance: Root,
    job_id: Root,
    committee_size: u32,
    job: Vec<u8>,
) -> Result<Response, Refusal> {
    if input.from != Some(instance_actor(program, &instance)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != job_actor(program, &instance, &job_id) {
        return Err(code::WRONG_RECEIVER);
    }
    if !input.pre_state.is_empty() {
        return Err(code::JOB_EXISTS);
    }
    let state = JobState { instance_id: instance, job_id, committee_size, job };
    Ok(Response::set_state(encode(&state)))
}

fn miller(input: &ReceiveInput, program: AccountId) -> Result<Response, Refusal> {
    let (state, mut job) = job_state(input, program)?;
    job.step2().map_err(step)?;
    Ok(Response::set_state(encode(&JobState { job: job.encode(), ..state })))
}

fn adopt(input: &ReceiveInput, program: AccountId, part: u8, keys: &[UncompressedKey]) -> Result<Response, Refusal> {
    let (state, mut job) = job_state(input, program)?;
    job.adopt(part, keys, state.committee_size as usize).map_err(step)?;
    if job.is_verified() {
        // An initialization verifies no signature: its last adoption part completes it.
        return Ok(complete(program, &state, &job));
    }
    Ok(Response::set_state(encode(&JobState { job: job.encode(), ..state })))
}

fn finish(input: &ReceiveInput, program: AccountId) -> Result<Response, Refusal> {
    let (state, mut job) = job_state(input, program)?;
    if job.adoption_pending() {
        return Err(step(StepError::AdoptionIncomplete));
    }
    job.step3().map_err(step)?;
    Ok(complete(program, &state, &job))
}

// ---------------------------------------------------------------------------------------------
// header(id, slot)
// ---------------------------------------------------------------------------------------------

fn write_record(
    input: &ReceiveInput,
    program: AccountId,
    instance: Root,
    slot: u64,
    chain_id: u64,
    record: Vec<u8>,
    root_id: Root,
) -> Result<Response, Refusal> {
    if input.from != Some(instance_actor(program, &instance)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != header_actor(program, &instance, slot) {
        return Err(code::WRONG_RECEIVER);
    }
    let state = encode(&RecordState { instance_id: instance, chain_id, record, root_id });
    if !input.pre_state.is_empty() && *input.pre_state != *state {
        return Err(code::RECORD_CONFLICT);
    }
    Ok(Response::set_state(state))
}

/// Prove the facts of `request` under this actor's record and hand the statements to the caller.
///
/// The record is one an instance of this program wrote: only `Record`, accepted from
/// `instance(id)` alone, writes a `RecordState`, and it writes it at `header(id, slot)`, which is
/// checked here again. Nothing else is trusted: the state and receipts roots come from the
/// record, the instance and the chain id the caller expects must be the record's, and every
/// proof is verified by the core. The handler changes no state, so a record serves any number of
/// attestations, in any order, for as long as it exists.
fn attest(input: &ReceiveInput, program: AccountId, request: &attest::Request) -> Result<Response, Refusal> {
    if input.pre_state.is_empty() {
        return Err(code::NO_RECORD);
    }
    let state: RecordState = borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?;
    let recorded = state.header()?;
    if input.receiver != header_actor(program, &state.instance_id, recorded.beacon.slot) {
        return Err(code::WRONG_RECEIVER);
    }
    // A request is bound to the instance that wrote the record or to its root id: the root id
    // stands for every successor of the root, which `Spawn` alone creates (from its parent).
    let bound = if request.instance_id == state.root_id { &state.root_id } else { &state.instance_id };
    let attestation = attest::evaluate(bound, state.chain_id, &recorded, request)?;
    Ok(match input.from {
        // A consumer called: the statements are its reply, in the same transaction.
        Some(caller) => Response::keep_state().send_sync(caller, &attestation),
        // Sent as a transaction's root there is nobody to reply to: the identifiers are an event.
        None => {
            Response::keep_state().event(ProgramEvent { selector: ATTESTED_SELECTOR, data: attestation.event_data() })
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Any actor
// ---------------------------------------------------------------------------------------------

/// The read idiom of LEZ v0.4 (as the native token's `ReadState`): reply with this actor's state.
fn read(input: &ReceiveInput) -> Result<Response, Refusal> {
    let reader = input.from.ok_or(code::UNROUTABLE)?;
    Ok(Response::keep_state().send_sync(reader, &StateReply::from(input)))
}

/// The private placement: verify an update in one call, then hand the verified job to the
/// instance. The actor's own state is not used.
fn verify_all(
    program: AccountId,
    config: &Config,
    checkpoint: &Root,
    store: &[u8],
    update: &[u8],
    signers: &[u8],
    adoption: &[UncompressedKey],
) -> Result<Response, Refusal> {
    check_config(config)?;
    let id = instance_id(config, checkpoint);
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let stored = CompactStore::decode(store).map_err(core)?;
    let (update, store) = update_and_store(&stored, update, n, &spec)?;
    let signers = SignerWitness::decode(signers, n).map_err(core)?;
    let mut job =
        steps::step1(&store, &update, &signers, update.signature_slot, &spec, &config.genesis_validators_root, 1)
            .map_err(step)?;
    job.step2().map_err(step)?;
    job.step3().map_err(step)?;
    if job.adoption_pending() {
        job.adopt(0, adoption, n).map_err(step)?;
    }
    let response = Response::keep_state().send(SyncMessage::new(
        instance_actor(program, &id),
        &Message::ApplyVerified { instance_id: id, job: job.encode() },
    ));
    update_window(response, config, stored.finalized.slot, update.signature_slot)
}

// ---------------------------------------------------------------------------------------------
// Successors (docs/design/attestation.md §3.5)
// ---------------------------------------------------------------------------------------------

/// Invariant 1: `next` is `parent` with another fork schedule, which agrees with the parent's on
/// every fork at or before `checkpoint_epoch`; the store layout is the parent store's `layout`;
/// the schedule is in activation order.
fn check_successor(parent: &Config, next: &Config, layout: LcFork, checkpoint_epoch: u64) -> Result<(), Refusal> {
    let same = parent.chain_id == next.chain_id
        && parent.sync_committee_size == next.sync_committee_size
        && parent.slots_per_epoch == next.slots_per_epoch
        && parent.epochs_per_sync_committee_period == next.epochs_per_sync_committee_period
        && parent.min_sync_committee_participants == next.min_sync_committee_participants
        && parent.genesis_validators_root == next.genesis_validators_root
        && parent.genesis_time == next.genesis_time
        && parent.seconds_per_slot == next.seconds_per_slot
        && parent.adoption_parts == next.adoption_parts
        && parent.ws_period_epochs == next.ws_period_epochs;
    if !same || next.fork() != Some(layout) || next == parent {
        return Err(code::BAD_SUCCESSOR);
    }
    // Phase 0 is at epoch 0 in both (it activates at genesis); every fork that either schedule
    // puts at or before the checkpoint must be the same fork, at the same epoch, in both.
    for i in 0..next.fork_epochs.len() {
        let settled = parent.fork_epochs[i] <= checkpoint_epoch || next.fork_epochs[i] <= checkpoint_epoch;
        if settled && (parent.fork_epochs[i] != next.fork_epochs[i] || parent.fork_versions[i] != next.fork_versions[i])
        {
            return Err(code::BAD_SUCCESSOR);
        }
        if i > 0 && next.fork_epochs[i] < next.fork_epochs[i - 1] {
            return Err(code::BAD_SUCCESSOR);
        }
    }
    Ok(())
}

fn succeed(input: &ReceiveInput, program: AccountId, config: Config, checkpoint: &Root) -> Result<Response, Refusal> {
    let state = instance_state(input, program)?;
    if !state.active {
        return Err(code::INSTANCE_INACTIVE);
    }
    check_config(&config)?;
    let store = CompactStore::decode(&state.store).map_err(|_| code::BAD_STATE)?;
    // Invariant 2: the successor's checkpoint is this instance's latest record.
    let slot = store.finalized.slot;
    if store.finalized.hash_tree_root() != *checkpoint {
        return Err(code::STALE_CHECKPOINT);
    }
    // Invariant 1.
    check_successor(&state.config, &config, store.fork, slot / state.config.slots_per_epoch)?;
    let id = successor_id(&state.root_id, &state.id, &config, checkpoint);
    let mut data = id.to_vec();
    data.extend_from_slice(&state.root_id);
    data.extend_from_slice(&slot.to_le_bytes());
    let spawn = SyncMessage::new(
        instance_actor(program, &id),
        &Message::Spawn {
            root_id: state.root_id,
            parent_id: state.id,
            checkpoint: *checkpoint,
            config,
            store: state.store.clone(),
        },
    )
    .with_pda_seeds([instance_seed(&id)]);
    let response = Response::keep_state().send(spawn).event(ProgramEvent { selector: SUCCEEDED_SELECTOR, data });
    // Invariant 3: the checkpoint is inside its weak-subjectivity period, by the sequencer's clock.
    Ok(period_window(response, &state.config, slot))
}

fn spawn(
    input: &ReceiveInput,
    program: AccountId,
    root_id: Root,
    parent_id: Root,
    checkpoint: &Root,
    config: Config,
    store: Vec<u8>,
) -> Result<Response, Refusal> {
    // Only the parent instance of this program creates its successors, after checking them.
    if input.from != Some(instance_actor(program, &parent_id)) {
        return Err(code::WRONG_SENDER);
    }
    let id = successor_id(&root_id, &parent_id, &config, checkpoint);
    if input.receiver != instance_actor(program, &id) {
        return Err(code::WRONG_RECEIVER);
    }
    if !input.pre_state.is_empty() {
        return Err(code::ALREADY_INITIALIZED);
    }
    let state = InstanceState { id, config, active: true, store, applied: 0, root_id, parent_id };
    Ok(Response::set_state(encode(&state)))
}
