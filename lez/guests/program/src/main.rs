//! Ferrata on LEZ v0.4: an Ethereum sync-committee light client as one program of actors.
//!
//! See `ferrata_lez_wire` for the actors and messages. Every handler checks that it runs at the
//! actor the message is for (the receiver is the PDA the message implies), and every internal
//! message is accepted only from the actor of this program that sends it. Refusals exit with a
//! code instead of panicking: LEZ then charges only the cycles used
//! (`lee::validated_state_diff::charged_cycles`), not the whole gas limit.

use ferrata_core::{
    steps::{self, Job, PendingApply, StepError},
    witness::{CompactBootstrap, CompactStore, CompactUpdate, SignerWitness},
    Error, Root,
};
use ferrata_lez_wire::{
    code, header_actor, header_seed, instance_actor, instance_id, job_actor, job_id, job_seed, Config, InstanceState,
    JobState, Message, RecordState, UncompressedKey, FINALIZED_SELECTOR,
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
        Message::Read | Message::Attest { .. } => read(input),
        Message::VerifyAll { config, checkpoint, store, update, signers, adoption } => {
            verify_all(program, &config, &checkpoint, &store, &update, &signers, &adoption)
        }
        Message::JobOpen { instance_id, job_id, committee_size, job } => {
            job_open(input, program, instance_id, job_id, committee_size, job)
        }
        Message::Apply { instance_id, job_id, job } => apply(input, program, &instance_id, &job_id, &job),
        Message::ApplyVerified { instance_id, job } => apply_verified(input, program, &instance_id, &job),
        Message::Record { instance_id, slot, record } => write_record(input, program, instance_id, slot, record),
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
    Ok(())
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
    let state = InstanceState { id, config, active: false, store: store.encode(), applied: 0 };
    Ok(Response::set_state(encode(&state)).send(open_job(program, &id, n, &job)))
}

fn open(input: &ReceiveInput, program: AccountId, update: &[u8], signers: &[u8]) -> Result<Response, Refusal> {
    let state = instance_state(input, program)?;
    if !state.active {
        return Err(code::INSTANCE_INACTIVE);
    }
    let config = &state.config;
    let fork = config.fork().ok_or(code::BAD_STATE)?;
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let store = CompactStore::decode(&state.store).map_err(|_| code::BAD_STATE)?;
    let update = CompactUpdate::decode(update, fork, n).map_err(core)?;
    let signers = SignerWitness::decode(signers, n).map_err(core)?;
    // `current_slot >= signature_slot` is enforced by the transaction's validity window: it is
    // valid only from the start of the signature slot, by the sequencer's clock.
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
    Ok(Response::keep_state()
        .send(open_job(program, &state.id, config.sync_committee_size, &job))
        .timestamp_window(config.slot_start_ms(update.signature_slot)..))
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
    let (next, outcome) = steps::apply(&store, job, &spec).map_err(step)?;
    state.store = next.encode();
    state.applied += 1;
    state.active = true;
    let id = state.id;
    let mut response = Response::set_state(encode(&state));
    if activate || outcome.advanced {
        let record = job.pending.record();
        let slot = record.beacon.slot;
        let mut data = slot.to_le_bytes().to_vec();
        data.extend_from_slice(&record.beacon.hash_tree_root());
        response = response
            .send(
                SyncMessage::new(
                    header_actor(program, &id, slot),
                    &Message::Record { instance_id: id, slot, record: record.encode() },
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
    record: Vec<u8>,
) -> Result<Response, Refusal> {
    if input.from != Some(instance_actor(program, &instance)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != header_actor(program, &instance, slot) {
        return Err(code::WRONG_RECEIVER);
    }
    let state = encode(&RecordState { instance_id: instance, record });
    if !input.pre_state.is_empty() && *input.pre_state != *state {
        return Err(code::RECORD_CONFLICT);
    }
    Ok(Response::set_state(state))
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
    let fork = config.fork().ok_or(code::BAD_CONFIG)?;
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let store = CompactStore::decode(store).map_err(core)?;
    let update = CompactUpdate::decode(update, fork, n).map_err(core)?;
    let signers = SignerWitness::decode(signers, n).map_err(core)?;
    let mut job =
        steps::step1(&store, &update, &signers, update.signature_slot, &spec, &config.genesis_validators_root, 1)
            .map_err(step)?;
    job.step2().map_err(step)?;
    job.step3().map_err(step)?;
    if job.adoption_pending() {
        job.adopt(0, adoption, n).map_err(step)?;
    }
    Ok(Response::keep_state()
        .send(SyncMessage::new(
            instance_actor(program, &id),
            &Message::ApplyVerified { instance_id: id, job: job.encode() },
        ))
        .timestamp_window(config.slot_start_ms(update.signature_slot)..))
}
