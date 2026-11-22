//! Per-transition cycles of a public transaction.
//!
//! `lee` reports one number per transaction (`ExecutionCharge::cycles`). To see where the cycles
//! go, this replays the transaction's action with `lee_core`'s own execution engine
//! (`WholeExecution`, which `ValidatedStateDiff::execute_authorized` also uses) and a backend that
//! runs every guest as `lee`'s `PublicBackend` does: the program resolved from the loader's
//! accounts (`lee::program::load_program`), the input written by `Program::write_receive_input`,
//! one `default_executor().execute` session per transition, under the remaining budget. The
//! callers check that the transitions sum to the cycles `lee` charged.

use std::collections::BTreeSet;

use lee::{program::load_program, PublicTransaction, V03State};
use lee_core::{
    account::{AccountId, Actor, ActorState},
    execution_state::{ExecutionEnvironment, ExecutionError, TransactionEntry, TransitionView, WholeExecution},
    from_frame,
    native_token::{self, NATIVE_TOKEN_PROGRAM_ID},
    program::{ReceiveInput, Transition, PROGRAM_LOADER_ACCOUNT_ID},
};
use risc0_zkvm::{default_executor, ExecutorEnv, ExitCode};

/// One handler execution.
#[derive(Clone, Debug)]
pub struct TransitionTrace {
    /// The message the handler ran (the `ferrata_lez_wire::Message` variant, or the program).
    pub handler: String,
    pub receiver: Actor,
    pub from: Option<Actor>,
    /// Size of the borsh `ReceiveInput` the guest reads.
    pub input_bytes: usize,
    pub pre_state_bytes: usize,
    pub message_bytes: usize,
    /// Size of the journal the guest commits (the echoed input and its response).
    pub journal_bytes: usize,
    /// User cycles of the session (`SessionInfo::cycles`, what LEZ meters).
    pub cycles: u64,
    /// Segments of the session (each up to 2^20 cycles).
    pub segments: usize,
    /// `Halted(code)`; a non-zero code is a refusal.
    pub exit_code: u32,
    /// The post-state size, if the handler set one.
    pub post_state_bytes: Option<usize>,
}

/// One traced transition kept for a measurement (`src/bin/perf_prove.rs`): the guest and the exact
/// input `lee`'s public backend gives it, so that the same execution can be proven later.
#[derive(Clone, Debug)]
pub struct Taped {
    pub handler: String,
    pub elf: Vec<u8>,
    pub input: ReceiveInput,
    pub session_limit: u64,
    pub cycles: u64,
    pub journal: Vec<u8>,
}

/// While this holds a vector, every transition `trace_public` executes is also appended to it
/// (off by default; only the measurement binaries switch it on).
pub static TAPE: std::sync::Mutex<Option<Vec<Taped>>> = std::sync::Mutex::new(None);

#[derive(Debug)]
pub struct TraceError(pub String);

impl From<ExecutionError> for TraceError {
    fn from(e: ExecutionError) -> Self {
        TraceError(e.to_string())
    }
}

struct Tracer<'a> {
    state: &'a V03State,
    evidence: BTreeSet<AccountId>,
    budget: u64,
    used: u64,
    traces: Vec<TransitionTrace>,
}

impl ExecutionEnvironment for Tracer<'_> {
    type Error = TraceError;

    fn handle_message(&mut self, input: &ReceiveInput, view: &TransitionView<'_>) -> Result<Transition, TraceError> {
        let program_account_id = input.receiver.program_account_id;
        if program_account_id == NATIVE_TOKEN_PROGRAM_ID {
            return native_token::handle_message(input).map_err(|e| TraceError(e.to_string()));
        }
        if program_account_id == PROGRAM_LOADER_ACCOUNT_ID {
            return Err(TraceError("the loader is not traced".into()));
        }
        let state = self.state;
        let program = load_program(program_account_id, |account_id| {
            view.staged_state(Actor::new(account_id, PROGRAM_LOADER_ACCOUNT_ID)).or_else(|| {
                state.get_account_by_id_ref(account_id).map(|a| a.data.actor_state(PROGRAM_LOADER_ACCOUNT_ID))
            })
        })
        .ok_or_else(|| TraceError(format!("unknown program {program_account_id}")))?;
        let input_bytes = borsh::to_vec(input).expect("borsh").len();
        let mut builder = ExecutorEnv::builder();
        builder.session_limit(Some(self.budget.saturating_sub(self.used)));
        lee::program::Program::write_receive_input(input, &mut builder).map_err(|e| TraceError(e.to_string()))?;
        let env = builder.build().map_err(|e| TraceError(e.to_string()))?;
        let session_limit = self.budget.saturating_sub(self.used);
        let info = default_executor().execute(env, program.elf()).map_err(|e| TraceError(e.to_string()))?;
        let cycles = info.cycles();
        if let Some(tape) = TAPE.lock().expect("tape").as_mut() {
            tape.push(Taped {
                handler: handler_name(input),
                elf: program.elf().to_vec(),
                input: input.clone(),
                session_limit,
                cycles,
                journal: info.journal.bytes.clone(),
            });
        }
        self.used += cycles;
        let exit_code = match info.exit_code {
            ExitCode::Halted(code) => code,
            other => return Err(TraceError(format!("unexpected exit {other:?}"))),
        };
        let mut trace = TransitionTrace {
            handler: handler_name(input),
            receiver: input.receiver,
            from: input.from,
            input_bytes,
            pre_state_bytes: input.pre_state.len(),
            message_bytes: input.message.len(),
            journal_bytes: info.journal.bytes.len(),
            cycles,
            segments: info.segments.len(),
            exit_code,
            post_state_bytes: None,
        };
        if exit_code != 0 {
            self.traces.push(trace);
            return Err(TraceError(format!("program exited with code {exit_code}")));
        }
        let transition: Transition = from_frame(&info.journal.bytes)
            .and_then(|payload| borsh::from_slice(payload).ok())
            .ok_or_else(|| TraceError("malformed journal".into()))?;
        trace.post_state_bytes = transition.response.post_state.as_ref().map(|s| s.len());
        self.traces.push(trace);
        Ok(transition)
    }

    fn public_actor_state(&mut self, actor: Actor) -> Result<ActorState, TraceError> {
        Ok(self
            .state
            .get_account_by_id_ref(actor.account_id)
            .map_or_else(ActorState::empty, |account| account.data.actor_state(actor.program_account_id).clone()))
    }

    fn admits(&mut self, account_id: AccountId) -> Result<bool, TraceError> {
        Ok(self.state.get_account_by_id_ref(account_id).is_some() || self.evidence.contains(&account_id))
    }
}

/// The handler a transition ran, for the tables.
pub fn handler_name(input: &ReceiveInput) -> String {
    let program = input.receiver.program_account_id;
    if program == crate::ferrata_account() || program == crate::TWIN || program == crate::KECCAK_SOFT {
        use ferrata_lez_wire::Message as M;
        let name = match borsh::from_slice::<M>(&input.message) {
            Ok(M::Init { .. }) => "Init",
            Ok(M::Open { .. }) => "Open",
            Ok(M::Miller) => "Miller",
            Ok(M::Adopt { .. }) => "Adopt",
            Ok(M::Finish) => "Finish",
            Ok(M::Read) => "Read",
            Ok(M::ReadPadded { .. }) => "ReadPadded",
            Ok(M::Attest { .. }) => "Attest",
            Ok(M::VerifyAll { .. }) => "VerifyAll",
            Ok(M::JobOpen { .. }) => "JobOpen",
            Ok(M::Apply { .. }) => "Apply",
            Ok(M::ApplyVerified { .. }) => "ApplyVerified",
            Ok(M::Record { .. }) => "Record",
            Ok(M::Succeed { .. }) => "Succeed",
            Ok(M::Spawn { .. }) => "Spawn",
            Err(_) => "(undecodable)",
        };
        return name.to_string();
    }
    if input.receiver.program_account_id == crate::consumer_account() {
        use ferrata_lez_wire::consumer::ConsumerMessage as C;
        // Anything that is not one of the consumer's messages is a reply it waits for.
        let name = match borsh::from_slice::<C>(&input.message) {
            Ok(C::ReadRecord { .. }) => "consumer ReadRecord",
            Ok(C::ReadPadded { .. }) => "consumer ReadPadded",
            Ok(C::CallAttest { .. }) => "consumer CallAttest",
            Ok(C::Claim { .. }) => "consumer Claim",
            Ok(C::AttestPrivately { .. }) => "consumer AttestPrivately",
            Ok(C::Credit { .. }) => "consumer Credit",
            Ok(C::ClaimDeposit { .. }) => "consumer ClaimDeposit",
            Ok(C::ClaimDepositPrivately { .. }) => "consumer ClaimDepositPrivately",
            Ok(C::Spend { .. }) => "consumer Spend",
            Ok(C::CreditHolder { .. }) => "consumer CreditHolder",
            Err(_) => "consumer (reply)",
        };
        return name.to_string();
    }
    if input.receiver.program_account_id == crate::bridge::bridge_account() {
        return crate::bridge::handler_name(input);
    }
    if input.receiver.program_account_id == crate::holdings::holdings_account() {
        return crate::holdings::handler_name(input);
    }
    if input.receiver.program_account_id == crate::scripted_account() {
        return "scripted".into();
    }
    format!("program {}", input.receiver.program_account_id)
}

/// Replay `tx`'s action on `state` (which it does not change), under `budget` cycles.
pub fn trace_public(
    tx: &PublicTransaction,
    state: &V03State,
    budget: u64,
) -> (Vec<TransitionTrace>, Result<(), String>) {
    let message = tx.message();
    let evidence = tx
        .witness_set()
        .signatures_and_public_keys()
        .iter()
        .map(|(_, key)| AccountId::from(key))
        .chain(message.admission_evidence.iter().map(lee::PublicAccountEvidence::account_id))
        .collect();
    let mut tracer = Tracer { state, evidence, budget, used: 0, traces: Vec::new() };
    let result = WholeExecution::new(
        message.public_context.clone(),
        TransactionEntry::Sync(message.execution.root.clone()),
        Vec::new(),
        None,
        &[],
    )
    .map_err(|e| e.to_string())
    .and_then(|execution| execution.execute(&mut tracer).map(|_| ()).map_err(|e| e.0));
    (tracer.traces, result)
}
