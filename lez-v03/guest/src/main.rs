//! # EXPERIMENTAL, UNAUDITED. Not the proposal's deliverable (the LEZ v0.4 programs are).
//!
//! Ferrata's Ethereum sync-committee light client as a program of LEZ **v0.3.1**, the version the
//! public LEZ testnet runs. The verification is `ferrata-core`'s, unchanged; the one-transaction
//! finality update replaces the final exponentiation by the residue-witness check of
//! `bench/residue` (`Verify::Residue`), and may compose the signing key from one multiproof of
//! the non-signers (`bench/residue-agg`, a signer witness marked `0xffff`/`0xfffe`); with
//! `Verify::ResidueSummary` a handoff adopts the next committee by its summary in the same
//! transaction (`bench/residue-handoff`, `bench/RESIDUE.md` §10: sound only under the light client's
//! honest-supermajority assumption). All three are bench code, experimental and unaudited.
//! `Verify::Core` and the stepwise `Continue` path use the core alone.
//!
//! See `ferrata_v03_wire` for the accounts, instructions and effects. The plan sees no state: it
//! checks that every account is the program's own shard at the address the instruction implies,
//! and turns the instruction into effects. Every check of the light client runs in the apply at
//! `instance(id)`, which also checks the claims (record, timestamp window) the other effects and
//! the plan relied on; an apply that refuses reverts the whole transaction. Refusals exit with a
//! code instead of panicking: LEZ v0.3.1 then charges only the cycles used
//! (`lee::ValidatedStateDiff::from_public_transaction_metered`), not the whole gas limit.

use ferrata_core::{
    attest as proven,
    fork::ChainSpec,
    steps::{self, Job, PendingApply, Phase, StepError},
    witness::{CompactBootstrap, CompactStore, CompactUpdate, ExecutionSummary, HeaderRecord, SignerWitness},
    Error, Root,
};
use ferrata_v03_wire::{
    code, header_account, instance_account, instance_id, statement_account, Config, Effect, InstanceOp,
    InstanceState, Instruction, Part, RecordClaim, RecordState, Statement, Step, StorageQuery, Verify, Window,
};
use lee_core::{
    account::{AccountId, ShardData},
    program::{
        read_input_frame, AccountMeta, ApplyInput, ApplyOutput, CallKind, GuestOutput, PlanInput, PlanOutput,
        ShardEffect, TimestampValidityWindow,
    },
};
use risc0_zkvm::guest::env;

type Refusal = u8;

fn main() {
    // Written by the (trusted) runtime: a call kind, then its input.
    let kind: CallKind = borsh::from_slice(&read_input_frame()).expect("the runtime's call kind decodes");
    let payload = read_input_frame();
    match kind {
        CallKind::Plan => {
            let input: PlanInput = borsh::from_slice(&payload).expect("the runtime's plan input decodes");
            drop(payload);
            match plan(input) {
                Ok(output) => GuestOutput::Plan(output).write(),
                Err(code) => env::exit(code),
            }
        }
        CallKind::Apply => {
            let input: ApplyInput = borsh::from_slice(&payload).expect("the runtime's apply input decodes");
            drop(payload);
            match apply(&input) {
                Ok(post) => GuestOutput::Apply(ApplyOutput::new(input, post)).write(),
                Err(code) => env::exit(code),
            }
        }
    }
    env::exit(0)
}

fn core(e: Error) -> Refusal {
    e.code() as Refusal
}

fn step(e: StepError) -> Refusal {
    e.code() as Refusal
}

fn encode<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn shard(data: Vec<u8>) -> Result<ShardData, Refusal> {
    ShardData::try_from(data).map_err(|_| code::BAD_STATE)
}

// ---------------------------------------------------------------------------------------------
// Plan: route the instruction to effects on the program's own shards
// ---------------------------------------------------------------------------------------------

fn plan(input: PlanInput) -> Result<PlanOutput, Refusal> {
    let program = input.self_account_id;
    if input.accounts.iter().any(|a| a.program_account_id != program) {
        return Err(code::WRONG_ACCOUNT);
    }
    let instruction: Instruction = borsh::from_slice(&input.instruction_data).map_err(|_| code::BAD_INSTRUCTION)?;
    let accounts = &input.accounts;
    let (effects, window) = match instruction {
        Instruction::Init { config, checkpoint, bootstrap, adopt, record, window } => {
            let id = instance_id(&config, &checkpoint);
            let op = InstanceOp::Init { config, checkpoint, bootstrap, adopt };
            (advance_effects(accounts, &program, &id, op, record, window)?, window)
        }
        Instruction::Update { instance_id, update, signers, verify, adopt, record, window } => {
            let op = InstanceOp::Update { instance_id, update, signers, verify, adopt };
            (advance_effects(accounts, &program, &instance_id, op, record, window)?, window)
        }
        Instruction::Continue { instance_id, steps, record } => {
            let op = InstanceOp::Continue { instance_id, steps };
            (advance_effects(accounts, &program, &instance_id, op, record, Window::default())?, Window::default())
        }
        Instruction::Attest { instance_id, slot, query, statements } => {
            (attest_effects(accounts, &program, &instance_id, slot, query, statements)?, Window::default())
        }
    };
    let timestamps: TimestampValidityWindow =
        (window.from_ms, window.to_ms).try_into().map_err(|_| code::WRONG_WINDOW)?;
    let mut output = PlanOutput::new(input).with_effects(effects);
    output.timestamp_validity_window = timestamps;
    Ok(output)
}

/// `instance(id)`, then `header(id, slot)` if a record is claimed.
fn advance_effects(
    accounts: &[AccountMeta],
    program: &AccountId,
    id: &Root,
    op: InstanceOp,
    record: Option<RecordClaim>,
    window: Window,
) -> Result<Vec<ShardEffect>, Refusal> {
    if accounts.len() != 1 + usize::from(record.is_some()) || accounts[0].account_id != instance_account(program, id) {
        return Err(code::WRONG_ACCOUNT);
    }
    let record_effect = match &record {
        Some(claim) => {
            let slot = HeaderRecord::decode(&claim.record).map_err(|_| code::WRONG_RECORD)?.beacon.slot;
            if accounts[1].account_id != header_account(program, id, slot) {
                return Err(code::WRONG_ACCOUNT);
            }
            Some(ShardEffect::new(&accounts[1], &Effect::Record { instance_id: *id, slot, claim: claim.clone() }))
        }
        None => None,
    };
    let mut effects = vec![ShardEffect::new(&accounts[0], &Effect::Instance { op, record, window })];
    effects.extend(record_effect);
    Ok(effects)
}

/// `header(id, slot)` checks the query; each `statement(id, s)` takes its claimed statement.
fn attest_effects(
    accounts: &[AccountMeta],
    program: &AccountId,
    id: &Root,
    slot: u64,
    query: StorageQuery,
    statements: Vec<Statement>,
) -> Result<Vec<ShardEffect>, Refusal> {
    if statements.is_empty()
        || accounts.len() != 1 + statements.len()
        || accounts[0].account_id != header_account(program, id, slot)
    {
        return Err(code::WRONG_ACCOUNT);
    }
    let mut written = Vec::with_capacity(statements.len());
    for (s, account) in statements.iter().zip(&accounts[1..]) {
        if account.account_id != statement_account(program, id, &s.id_at_block) {
            return Err(code::WRONG_ACCOUNT);
        }
        written.push(ShardEffect::new(account, &Effect::Statement(s.clone())));
    }
    let mut effects = vec![ShardEffect::new(&accounts[0], &Effect::Check { instance_id: *id, query, statements })];
    effects.extend(written);
    Ok(effects)
}

// ---------------------------------------------------------------------------------------------
// Apply: one effect on one of the program's shards
// ---------------------------------------------------------------------------------------------

fn apply(input: &ApplyInput) -> Result<Option<ShardData>, Refusal> {
    let program = input.self_account_id;
    if input.selector.program_account_id != program {
        return Err(code::WRONG_ACCOUNT);
    }
    let account = input.selector.account_id;
    let pre: &[u8] = &input.pre_data;
    let effect: Effect = borsh::from_slice(&input.effect_data).map_err(|_| code::BAD_INSTRUCTION)?;
    match effect {
        Effect::Instance { op, record, window } => {
            let state = instance(&program, &account, pre, op, record, window)?;
            Ok(Some(shard(encode(&state))?))
        }
        Effect::Record { instance_id, slot, claim } => {
            let recorded = HeaderRecord::decode(&claim.record).map_err(|_| code::WRONG_RECORD)?;
            if recorded.beacon.slot != slot || account != header_account(&program, &instance_id, slot) {
                return Err(code::WRONG_ACCOUNT);
            }
            write_once(pre, encode(&RecordState { instance_id, chain_id: claim.chain_id, record: claim.record }))
        }
        Effect::Check { instance_id, query, statements } => {
            check(&program, &account, pre, &instance_id, &query, &statements)?;
            Ok(None)
        }
        Effect::Statement(s) => {
            if account != statement_account(&program, &s.instance_id, &s.id_at_block) {
                return Err(code::WRONG_ACCOUNT);
            }
            write_once(pre, encode(&s))
        }
    }
}

/// Write `data` to an empty shard; keep a shard that holds exactly `data`; refuse any other.
fn write_once(pre: &[u8], data: Vec<u8>) -> Result<Option<ShardData>, Refusal> {
    if pre.is_empty() {
        Ok(Some(shard(data)?))
    } else if *pre == *data {
        Ok(None)
    } else {
        Err(code::RECORD_CONFLICT)
    }
}

// ---------------------------------------------------------------------------------------------
// instance(id)
// ---------------------------------------------------------------------------------------------

fn instance(
    program: &AccountId,
    account: &AccountId,
    pre: &[u8],
    op: InstanceOp,
    record: Option<RecordClaim>,
    window: Window,
) -> Result<InstanceState, Refusal> {
    match op {
        InstanceOp::Init { config, checkpoint, bootstrap, adopt } => {
            if !pre.is_empty() {
                return Err(code::ALREADY_INITIALIZED);
            }
            let id = instance_id(&config, &checkpoint);
            if *account != instance_account(program, &id) {
                return Err(code::WRONG_ACCOUNT);
            }
            check_config(&config)?;
            let fork = config.fork().ok_or(code::BAD_CONFIG)?;
            let spec = config.spec();
            let bootstrap = CompactBootstrap::decode(&bootstrap, fork).map_err(core)?;
            let (store, job) =
                steps::initialize(&checkpoint, &bootstrap, fork, &spec, config.adoption_parts).map_err(step)?;
            // The checkpoint must be inside its own weak-subjectivity period when the instance is
            // created (as on LEZ v0.4).
            expect_window(window, Window { from_ms: None, to_ms: config.ws_end_ms(bootstrap.header.beacon.slot) })?;
            let state =
                InstanceState { id, config, checkpoint, active: false, store: store.encode(), applied: 0, job: None };
            advance(state, job, adopt, record)
        }
        InstanceOp::Update { instance_id, update, signers, verify, adopt } => {
            let state = instance_state(program, account, pre, &instance_id)?;
            if !state.active {
                return Err(code::INSTANCE_INACTIVE);
            }
            // A job whose signature verified can always be completed (its adoption keys are
            // public): it is not replaced. One that has not may never verify: a new update
            // replaces it.
            if let Some(pending) = &state.job {
                if Job::decode(pending).map_err(|_| code::BAD_STATE)?.phase == Phase::Verified {
                    return Err(code::JOB_PENDING);
                }
            }
            let config = &state.config;
            let spec = config.spec();
            let stored = CompactStore::decode(&state.store).map_err(|_| code::BAD_STATE)?;
            let (update, store) = update_and_store(&stored, &update, config.sync_committee_size as usize, &spec)?;
            expect_window(window, update_window(config, stored.finalized.slot, update.signature_slot)?)?;
            let mut job = step1(&store, &update, &signers, &spec, config)?;
            match verify {
                Verify::Open => {}
                Verify::Miller => job.step2().map_err(step)?,
                Verify::Core => {
                    job.step2().map_err(step)?;
                    job.step3().map_err(step)?;
                }
                Verify::Residue { c, w } => {
                    job.step2().map_err(step)?;
                    residue_check(&mut job, &c, &w)?;
                }
                Verify::ResidueSummary { c, w } => {
                    job.step2().map_err(step)?;
                    residue_check(&mut job, &c, &w)?;
                    if job.adoption_pending() {
                        // EXPERIMENTAL (bench/RESIDUE.md §10; sound only under the light client's
                        // honest-supermajority assumption): the next committee adopted by its summary.
                        residue_handoff::adopt_summary(&mut job).map_err(step)?;
                    }
                }
            }
            advance(state, job, adopt, record)
        }
        InstanceOp::Continue { instance_id, steps: list } => {
            let state = instance_state(program, account, pre, &instance_id)?;
            if window != Window::default() {
                return Err(code::WRONG_WINDOW);
            }
            let mut job = Job::decode(state.job.as_deref().ok_or(code::NO_JOB)?).map_err(|_| code::BAD_STATE)?;
            let n = state.config.sync_committee_size as usize;
            for s in list {
                match s {
                    Step::Miller => job.step2().map_err(step)?,
                    Step::FinalExp => job.step3().map_err(step)?,
                    Step::Residue { c, w } => residue_check(&mut job, &c, &w)?,
                    Step::Adopt(part) => job.adopt(part.index, &part.keys, n).map_err(step)?,
                }
            }
            advance(state, job, Vec::new(), record)
        }
    }
}

/// Adopt `adopt`; if the job is then complete, apply it to the store and check the claimed
/// record against the job's (one is written exactly when the store activates or advances, as on
/// LEZ v0.4); otherwise keep the job for a later `Continue`.
fn advance(
    mut state: InstanceState,
    mut job: Job,
    adopt: Vec<Part>,
    record: Option<RecordClaim>,
) -> Result<InstanceState, Refusal> {
    let n = state.config.sync_committee_size as usize;
    for part in adopt {
        job.adopt(part.index, &part.keys, n).map_err(step)?;
    }
    if !job.is_verified() {
        if record.is_some() {
            return Err(code::WRONG_RECORD);
        }
        state.job = Some(job.encode());
        return Ok(state);
    }
    let activate = matches!(job.pending, PendingApply::Activate { .. });
    if activate == state.active {
        return Err(code::INSTANCE_INACTIVE);
    }
    let spec = state.config.spec();
    let store = CompactStore::decode(&state.store).map_err(|_| code::BAD_STATE)?;
    let (next, outcome) = steps::apply(&store, &job, &spec).map_err(step)?;
    let writes = activate || outcome.advanced;
    match (&record, writes) {
        (None, false) => {}
        (Some(claim), true)
            if claim.chain_id == state.config.chain_id && claim.record == job.pending.record().encode() => {}
        _ => return Err(code::WRONG_RECORD),
    }
    state.store = next.encode();
    state.applied += 1;
    state.active = true;
    state.job = None;
    Ok(state)
}

fn instance_state(program: &AccountId, account: &AccountId, pre: &[u8], id: &Root) -> Result<InstanceState, Refusal> {
    if pre.is_empty() {
        return Err(code::NO_INSTANCE);
    }
    let state: InstanceState = borsh::from_slice(pre).map_err(|_| code::BAD_STATE)?;
    if state.id != *id {
        return Err(code::WRONG_INSTANCE);
    }
    if *account != instance_account(program, &state.id) {
        return Err(code::WRONG_ACCOUNT);
    }
    Ok(state)
}

fn check_config(config: &Config) -> Result<(), Refusal> {
    let n = config.sync_committee_size as usize;
    let parts = config.adoption_parts as usize;
    if config.fork().is_none() || !n.is_power_of_two() || parts == 0 || !parts.is_power_of_two() || n % parts != 0 {
        return Err(code::BAD_CONFIG);
    }
    if config.ws_period_epochs == 0 || config.seconds_per_slot == 0 || config.slots_per_epoch == 0 {
        return Err(code::BAD_CONFIG);
    }
    // An independent review's finding 7: a sync-committee period of zero epochs would make every update
    // panic in the period arithmetic (a division by zero) and be charged the whole gas limit; refused at `Init`.
    if config.epochs_per_sync_committee_period == 0 {
        return Err(code::BAD_CONFIG);
    }
    Ok(())
}

/// The window LEZ v0.4's program sets for an update: from the start of the signature slot to the
/// end of the weak-subjectivity period of the stored finalized header; an update signed at or
/// after that end is refused.
fn update_window(config: &Config, finalized_slot: u64, signature_slot: u64) -> Result<Window, Refusal> {
    let from_ms = Some(config.slot_start_ms(signature_slot));
    match config.ws_end_slot(finalized_slot) {
        None => Ok(Window { from_ms, to_ms: None }),
        Some(end) if signature_slot >= end => Err(core(Error::OutsideWeakSubjectivityPeriod)),
        Some(end) => Ok(Window { from_ms, to_ms: Some(config.slot_start_ms(end)) }),
    }
}

fn expect_window(claimed: Window, expected: Window) -> Result<(), Refusal> {
    if claimed == expected {
        Ok(())
    } else {
        Err(code::WRONG_WINDOW)
    }
}

/// An update decoded in the layout its encoding names, and the store it is verified against (the
/// stored one, or the stored one upgraded to a later fork's layout of the instance's schedule).
fn update_and_store(
    store: &CompactStore,
    update: &[u8],
    n: usize,
    spec: &ChainSpec,
) -> Result<(CompactUpdate, CompactStore), Refusal> {
    let layout = CompactUpdate::layout_of(update).map_err(core)?;
    let update = CompactUpdate::decode(update, layout, n).map_err(core)?;
    let store = steps::store_for_update(store, layout, update.attested_header.beacon.slot, spec).map_err(core)?;
    Ok((update, store))
}

/// Step 1: the core's, or (EXPERIMENTAL) `residue_agg::step1` for a signer witness marked `0xffff`
/// (blst's batch addition) or `0xfffe` (one mixed addition per non-signer). `current_slot` is the
/// signature slot; the transaction's window makes it valid only from that slot's start.
fn step1(
    store: &CompactStore,
    update: &CompactUpdate,
    signers: &[u8],
    spec: &ChainSpec,
    config: &Config,
) -> Result<Job, Refusal> {
    let n = config.sync_committee_size as usize;
    let gvr = &config.genesis_validators_root;
    let parts = config.adoption_parts;
    let marked = match signers {
        [0xff, 0xff, rest @ ..] => Some((rest, residue_agg::Sum::Batch)),
        [0xfe, 0xff, rest @ ..] => Some((rest, residue_agg::Sum::Chain)),
        _ => None,
    };
    match marked {
        Some((rest, sum)) => {
            let signers = residue_agg::NonSigners::decode(rest, n).map_err(core)?;
            residue_agg::step1(store, update, &signers, update.signature_slot, spec, gvr, parts, sum).map_err(step)
        }
        None => {
            let signers = SignerWitness::decode(signers, n).map_err(core)?;
            steps::step1(store, update, &signers, update.signature_slot, spec, gvr, parts).map_err(step)
        }
    }
}

/// EXPERIMENTAL, UNAUDITED: the residue-witness check of the Miller-loop product (`bench/residue`,
/// `bench/RESIDUE.md`) in place of the final exponentiation.
fn residue_check(job: &mut Job, c: &[u8], w: &[u8]) -> Result<(), Refusal> {
    let (Ok(c), Ok(w)) =
        (<&[u8; residue::FP12_BYTES]>::try_from(c), <&[u8; residue::FP12_BYTES]>::try_from(w))
    else {
        return Err(code::BAD_WITNESS);
    };
    let Phase::FinalExp { product } = &job.phase else { return Err(step(StepError::Phase)) };
    let f = residue::decode_fp12(product).ok_or(core(Error::Malformed))?;
    residue::check_bytes(&f, c, w).map_err(|_| core(Error::SignatureInvalid))?;
    job.phase = Phase::Verified;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// header(id, slot): Attest
// ---------------------------------------------------------------------------------------------

/// Prove the query's slots under this record (`ferrata_core::attest::attest_storage`, every root
/// from the record) and require that they are exactly the claimed statements.
fn check(
    program: &AccountId,
    account: &AccountId,
    pre: &[u8],
    instance_id: &Root,
    query: &StorageQuery,
    statements: &[Statement],
) -> Result<(), Refusal> {
    if pre.is_empty() {
        return Err(code::NO_RECORD);
    }
    let state: RecordState = borsh::from_slice(pre).map_err(|_| code::BAD_STATE)?;
    if state.instance_id != *instance_id {
        return Err(code::WRONG_INSTANCE);
    }
    let recorded = HeaderRecord::decode(&state.record).map_err(|_| code::BAD_STATE)?;
    if *account != header_account(program, &state.instance_id, recorded.beacon.slot) {
        return Err(code::WRONG_ACCOUNT);
    }
    if query.chain_id != state.chain_id {
        return Err(core(Error::ChainMismatch));
    }
    let record = match (&recorded.execution, &query.header) {
        (ExecutionSummary::BlockHash(_), Some(header)) => {
            proven::HeaderRecord::from_recorded_block_hash(&recorded, state.chain_id, header)
        }
        (ExecutionSummary::BlockHash(_), None) => return Err(code::HEADER_REQUIRED),
        (ExecutionSummary::Payload { .. }, Some(_)) => return Err(code::BAD_QUERY),
        (ExecutionSummary::Payload { .. } | ExecutionSummary::None, _) => {
            proven::HeaderRecord::from_recorded(&recorded, state.chain_id)
        }
    }
    .map_err(core)?;
    if statements.len() != query.slots.len() {
        return Err(code::WRONG_STATEMENT);
    }
    for (slot, claimed) in query.slots.iter().zip(statements) {
        let attested = proven::attest_storage(
            &record,
            state.chain_id,
            &query.address,
            query.account_proof.as_slice(),
            &slot.slot,
            slot.proof.as_slice(),
        )
        .map_err(core)?;
        let proven_statement = Statement {
            instance_id: state.instance_id,
            chain_id: attested.chain_id,
            record_slot: recorded.beacon.slot,
            block_number: attested.block_number,
            block_hash: attested.block_hash,
            address: attested.fact.address,
            slot: attested.fact.slot,
            value: attested.fact.value,
            id: attested.id,
            id_at_block: attested.id_at_block(),
        };
        if proven_statement != *claimed {
            return Err(code::WRONG_STATEMENT);
        }
    }
    Ok(())
}
