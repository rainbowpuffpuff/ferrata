//! The second consumer of the attestation: `guests/holdings` (`ferrata_lez_holdings_wire`), a LEZ
//! program that accepts "`holder` held at least `minimum` of ERC-20 `token` at a finalized block",
//! driven on the Stage 4 chain (mainnet block 26,384,851, `attest::FIXTURE`): WETH's balance of
//! Uniswap v2's USDC/WETH pair, from the committed `eth_getProof` response.
//!
//! Also here: [`record_transitions`], which replays a public transaction and keeps each
//! transition's program and input, so that `prove_holdings` can prove them with `r0vm`.

use std::{borrow::Cow, collections::BTreeSet};

use ferrata_lez_holdings_wire::{holding_actor, holding_seed, Gate, HoldingsMessage, HoldingsState};
use ferrata_lez_wire::{
    attest::{Address, Word},
    header_actor, header_seed,
};
use ferrata_spectests::execkit::AccountProof;
use lee::{
    program::load_program, program::Program, AccountId, Actor, PublicAccountEvidence, PublicTransaction, V03State,
};
use lee_core::{
    account::ActorState,
    execution_state::{ExecutionEnvironment, ExecutionError, TransactionEntry, TransitionView, WholeExecution},
    from_frame,
    native_token::{self, NATIVE_TOKEN_PROGRAM_ID},
    program::{ReceiveInput, Transition, PROGRAM_LOADER_ACCOUNT_ID},
};
use risc0_zkvm::{default_executor, ExecutorEnv, ExitCode};
use serde_json::Value;

use crate::{
    attest::{fixture_file, u256, Stage4},
    TxReport,
};

pub fn holdings_program() -> Program {
    Program::new_unchecked(methods::FERRATA_HOLDINGS_ID, Cow::Borrowed(methods::FERRATA_HOLDINGS_ELF))
}

/// Where the genesis deployment of `ferrata_holdings` lives.
pub fn holdings_account() -> AccountId {
    AccountId::from_builtin_program(methods::FERRATA_HOLDINGS_ID)
}

/// WETH9 declares `balanceOf` at storage slot 3 (after `name`, `symbol`, `decimals`).
pub const WETH_BALANCES_SLOT: u128 = 3;

/// The Stage 4 chain with `ferrata_holdings` deployed, and the fixture's holder and proof.
pub struct Holdings {
    pub s: Stage4,
    /// The fixture's holder: Uniswap v2's USDC/WETH pair.
    pub holder: Address,
    /// `eth_getProof(WETH, [balance key, empty slot])` at the finalized block.
    pub proof: AccountProof,
}

impl Default for Holdings {
    fn default() -> Self {
        Self::new()
    }
}

impl Holdings {
    pub fn new() -> Self {
        let mut s = Stage4::new();
        // Deploy the program beside Ferrata's, as a genesis program (as `Chain::new` deploys the
        // reference consumer).
        let state = std::mem::replace(&mut s.w.chain.state, V03State::new());
        s.w.chain.state = state.with_genesis_programs([(holdings_program(), true)]);
        let manifest: Value = serde_json::from_slice(&fixture_file("fixture.json")).unwrap();
        let holder: Address =
            hex::decode(manifest["holder"].as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap();
        let proof = s.fx.weth_proof();
        Self { s, holder, proof }
    }

    /// The finalized record the fixture's block is bound to: `header(id, finalized slot)`.
    pub fn slot(&self) -> u64 {
        self.s.fx.finalized_slot
    }

    /// A gate on WETH for this instance and mainnet.
    pub fn gate(&self, minimum: Word) -> Gate {
        Gate {
            ferrata: self.s.w.inst.program,
            instance_id: self.s.w.inst.id,
            chain_id: self.s.fx.chain_id,
            token: self.s.fx.weth,
            balances_slot: u256(WETH_BALANCES_SLOT),
            minimum,
        }
    }

    /// The holder's balance in the committed `eth_getProof` response.
    pub fn balance(&self) -> Word {
        self.proof.storage.iter().find(|s| s.key == self.s.fx.weth_slot).expect("the balance slot").value
    }

    /// The proofs of the holder's balance: the account proof and the slot's proof.
    pub fn proofs(&self) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let slot = self.proof.storage.iter().find(|s| s.key == self.s.fx.weth_slot).expect("the balance slot");
        (self.proof.proof.clone(), slot.proof.clone())
    }

    /// `Check` of the fixture's holder against `gate` at `header(id, slot)`.
    pub fn check(&mut self, label: &str, gate: &Gate, slot: u64) -> TxReport {
        let (account_proof, storage_proof) = self.proofs();
        let holder = self.holder;
        self.check_with(label, gate, slot, &holder, account_proof, storage_proof, vec![])
    }

    /// `Check` with any holder and proofs. `evidence` admits extra accounts (a record actor that
    /// does not exist, so that the attestation program, not admission, refuses).
    #[allow(clippy::too_many_arguments)]
    pub fn check_with(
        &mut self,
        label: &str,
        gate: &Gate,
        slot: u64,
        holder: &Address,
        account_proof: Vec<Vec<u8>>,
        storage_proof: Vec<Vec<u8>>,
        mut evidence: Vec<PublicAccountEvidence>,
    ) -> TxReport {
        let program = holdings_account();
        let gate_id = gate.id();
        let actor = holding_actor(program, &gate_id, holder);
        let record = header_actor(gate.ferrata, &gate.instance_id, slot);
        let message = HoldingsMessage::Check {
            gate: gate.clone(),
            slot,
            holder: *holder,
            account_proof,
            storage_proof,
            header: None,
        };
        evidence.insert(0, PublicAccountEvidence::Pda { program, seed: holding_seed(&gate_id, holder) });
        self.s.w.chain.submit(
            label,
            actor,
            borsh::to_vec(&message).unwrap(),
            vec![actor, record],
            evidence,
            self.s.time,
        )
    }

    /// Evidence that admits `header(id, slot)` of the gate's program when it holds no record.
    pub fn record_evidence(&self, gate: &Gate, slot: u64) -> Vec<PublicAccountEvidence> {
        vec![PublicAccountEvidence::Pda { program: gate.ferrata, seed: header_seed(&gate.instance_id, slot) }]
    }

    /// The state of `holding(gate, holder)`, if it has one.
    pub fn holding(&self, gate: &Gate, holder: &Address) -> Option<HoldingsState> {
        let state = self.s.w.chain.actor_state(holding_actor(holdings_account(), &gate.id(), holder));
        (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a holdings state"))
    }
}

/// The handler a transition of `ferrata_holdings` ran, for the tables: `Check`, or the reply it
/// waits for (an attestation).
pub fn handler_name(input: &ReceiveInput) -> String {
    match borsh::from_slice::<HoldingsMessage>(&input.message) {
        Ok(HoldingsMessage::Check { .. }) => "holdings Check".into(),
        Err(_) => "holdings (reply)".into(),
    }
}

// ---------------------------------------------------------------------------------------------
// The transitions of a transaction, with their inputs (for proving them)
// ---------------------------------------------------------------------------------------------

/// One handler execution of a public transaction: the program, its image and the input it read.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub program: AccountId,
    pub elf: Vec<u8>,
    pub input: ReceiveInput,
    pub cycles: u64,
    pub exit_code: u32,
    pub journal: Vec<u8>,
}

#[derive(Debug)]
pub struct RecordError(pub String);

impl From<ExecutionError> for RecordError {
    fn from(e: ExecutionError) -> Self {
        RecordError(e.to_string())
    }
}

struct Recorder<'a> {
    state: &'a V03State,
    evidence: BTreeSet<AccountId>,
    out: Vec<Recorded>,
}

impl ExecutionEnvironment for Recorder<'_> {
    type Error = RecordError;

    fn handle_message(&mut self, input: &ReceiveInput, view: &TransitionView<'_>) -> Result<Transition, RecordError> {
        let program_account_id = input.receiver.program_account_id;
        if program_account_id == NATIVE_TOKEN_PROGRAM_ID {
            return native_token::handle_message(input).map_err(|e| RecordError(e.to_string()));
        }
        let state = self.state;
        let program = load_program(program_account_id, |account_id| {
            view.staged_state(Actor::new(account_id, PROGRAM_LOADER_ACCOUNT_ID)).or_else(|| {
                state.get_account_by_id_ref(account_id).map(|a| a.data.actor_state(PROGRAM_LOADER_ACCOUNT_ID))
            })
        })
        .ok_or_else(|| RecordError(format!("unknown program {program_account_id}")))?;
        let mut builder = ExecutorEnv::builder();
        lee::program::Program::write_receive_input(input, &mut builder).map_err(|e| RecordError(e.to_string()))?;
        let env = builder.build().map_err(|e| RecordError(e.to_string()))?;
        let info = default_executor().execute(env, program.elf()).map_err(|e| RecordError(e.to_string()))?;
        let exit_code = match info.exit_code {
            ExitCode::Halted(code) => code,
            other => return Err(RecordError(format!("unexpected exit {other:?}"))),
        };
        self.out.push(Recorded {
            program: program_account_id,
            elf: program.elf().to_vec(),
            input: input.clone(),
            cycles: info.cycles(),
            exit_code,
            journal: info.journal.bytes.clone(),
        });
        if exit_code != 0 {
            return Err(RecordError(format!("program exited with code {exit_code}")));
        }
        from_frame(&info.journal.bytes)
            .and_then(|payload| borsh::from_slice(payload).ok())
            .ok_or_else(|| RecordError("malformed journal".into()))
    }

    fn public_actor_state(&mut self, actor: Actor) -> Result<ActorState, RecordError> {
        Ok(self
            .state
            .get_account_by_id_ref(actor.account_id)
            .map_or_else(ActorState::empty, |account| account.data.actor_state(actor.program_account_id).clone()))
    }

    fn admits(&mut self, account_id: AccountId) -> Result<bool, RecordError> {
        Ok(self.state.get_account_by_id_ref(account_id).is_some() || self.evidence.contains(&account_id))
    }
}

/// Replay `tx` on `state` (the state before it, which is not changed) and return every handler
/// execution in order, with its input, as `crate::trace::trace_public` replays it for the tables.
pub fn record_transitions(tx: &PublicTransaction, state: &V03State) -> Result<Vec<Recorded>, String> {
    let message = tx.message();
    let evidence = tx
        .witness_set()
        .signatures_and_public_keys()
        .iter()
        .map(|(_, key)| AccountId::from(key))
        .chain(message.admission_evidence.iter().map(lee::PublicAccountEvidence::account_id))
        .collect();
    let mut recorder = Recorder { state, evidence, out: Vec::new() };
    WholeExecution::new(
        message.public_context.clone(),
        TransactionEntry::Sync(message.execution.root.clone()),
        Vec::new(),
        None,
        &[],
    )
    .map_err(|e| e.to_string())
    .and_then(|execution| execution.execute(&mut recorder).map(|_| ()).map_err(|e| e.0))?;
    Ok(recorder.out)
}
