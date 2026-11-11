//! The reference consumer of Ferrata's attestations: a program of its own (see
//! `ferrata_lez_wire::consumer`).
//!
//! * **Public claim.** `Claim` at `claim(pool, N)` calls `Attest` at the pool's `header(id, slot)`
//!   and, in the reply, credits the sender of an ERC-20 `Transfer` to the pool's recipient, once:
//!   the claim actor is the nullifier, addressed by the hash of the statement identifier.
//! * **Private attestation.** `AttestPrivately` at a private account's actor reads the record
//!   (`Read` → `StateReply`) and verifies the same fact with the same function inside this
//!   program's own execution. Nothing but the read leaves the private part.
//! * **The minimal reader of Stage 3**: `ReadRecord`, `ReadPadded` and `CallAttest`.
//!
//! A reply is accepted only from the actor this one asked. Refusals exit with a code; a refusal
//! of the attestation program ends the whole transaction with that program's code, so this
//! program never acts on an attestation that was not produced.

use ferrata_lez_wire::{
    attest::{self, Attestation},
    consumer::{
        balance_actor, balance_seed, claim_actor, claim_seed, code, commitment, holder_actor, holder_seed, nullifier,
        secret_nullifier, BalanceState, ClaimState, ClaimantState, ConsumerMessage, ConsumerState, DepositProof,
        HolderState, NullifierState, PendingAttest, PendingDeposit, Pool, PrivateState, ReaderState, VaultPool,
        KIND_PRIVATE, KIND_PUBLIC,
    },
    header_actor, Message, RecordState,
};
use lee_core::{
    account::AccountId,
    program::{read_input_frame, ReceiveInput, Response, StateReply, SyncMessage},
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

fn encode<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn handle(input: &ReceiveInput) -> Result<Response, Refusal> {
    let program = input.receiver.program_account_id;
    let state: Option<ConsumerState> = if input.pre_state.is_empty() {
        None
    } else {
        Some(borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?)
    };
    // A reply: accepted only from the actor this one asked, and only while it waits.
    if let (Some(from), Some(waiting)) = (input.from, &state) {
        if waiting.awaiting() == Some(from) {
            return reply(input, program, waiting);
        }
    }
    match borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)? {
        ConsumerMessage::ReadRecord { ferrata, instance_id, slot, query } => {
            let reader = reader(state)?;
            let header = header_actor(ferrata, &instance_id, slot);
            // Reading sends only `Read`; the query stays in this actor's (private) state.
            let state = ReaderState { awaiting: Some(header), calling: false, query: Some(query), ..reader };
            Ok(Response::set_state(encode(&ConsumerState::Reader(state))).send_sync(header, &Message::Read))
        }
        ConsumerMessage::ReadPadded { ferrata, instance_id, slot, padding } => {
            let reader = reader(state)?;
            let header = header_actor(ferrata, &instance_id, slot);
            let state = ReaderState { awaiting: Some(header), calling: false, query: None, ..reader };
            Ok(Response::set_state(encode(&ConsumerState::Reader(state)))
                .send_sync(header, &Message::ReadPadded { padding }))
        }
        ConsumerMessage::CallAttest { ferrata, slot, request } => {
            let reader = reader(state)?;
            let header = header_actor(ferrata, &request.instance_id, slot);
            // Calling sends the whole request to a public actor.
            let state = ReaderState { awaiting: Some(header), calling: true, query: None, ..reader };
            Ok(Response::set_state(encode(&ConsumerState::Reader(state)))
                .send_sync(header, &Message::Attest { request }))
        }
        ConsumerMessage::Claim { pool, slot, tx_index, log_index, proof, header } => {
            claim(state, pool, slot, tx_index, log_index, proof, header)
        }
        ConsumerMessage::AttestPrivately { pool, slot, tx_index, log_index, proof, header } => {
            let private = match state {
                None => PrivateState::default(),
                Some(ConsumerState::Private(private)) => private,
                Some(_) => return Err(code::BAD_STATE),
            };
            let record = header_actor(pool.ferrata, &pool.instance_id, slot);
            let state = PrivateState {
                awaiting: Some(record),
                pending: Some(PendingAttest { pool, tx_index, log_index, proof, header }),
                ..private
            };
            Ok(Response::set_state(encode(&ConsumerState::Private(state))).send_sync(record, &Message::Read))
        }
        ConsumerMessage::Credit { pool_id, nullifier, owner, amount } => {
            credit(input, program, state, pool_id, nullifier, owner, amount)
        }
        ConsumerMessage::ClaimDeposit { pool, slot, deposit, s, kind, dest } => {
            claim_deposit(state, pool, slot, deposit, s, kind, dest)
        }
        ConsumerMessage::ClaimDepositPrivately { pool, slot, deposit, s, kind, dest } => {
            claim_deposit_privately(input, state, PendingDeposit { pool, deposit, s, kind, dest }, slot)
        }
        ConsumerMessage::Spend { pool_id, nullifier, credit } => {
            spend(input, program, state, pool_id, nullifier, credit)
        }
        ConsumerMessage::CreditHolder { pool_id, nullifier, dest, amount } => {
            credit_holder(input, program, state, pool_id, nullifier, dest, amount)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Vault deposits: claim(pool, N) with N = H(s ‖ statement id), holder(pool, dest), claimants
// ---------------------------------------------------------------------------------------------

/// The public claim at `claim(pool, N)`: a public-kind deposit only (a private-kind one is
/// credited to its private destination, which a public claim cannot be). Refused before anything
/// is verified if `N` is spent, on either path.
fn claim_deposit(
    state: Option<ConsumerState>,
    pool: VaultPool,
    slot: u64,
    deposit: DepositProof,
    s: [u8; 32],
    kind: u8,
    dest: [u8; 32],
) -> Result<Response, Refusal> {
    match state {
        None | Some(ConsumerState::Nullifier(NullifierState::Pending { .. })) => {}
        Some(ConsumerState::Nullifier(NullifierState::Spent { .. })) => return Err(code::ALREADY_CLAIMED),
        Some(_) => return Err(code::BAD_STATE),
    }
    if kind != KIND_PUBLIC {
        return Err(code::WRONG_KIND);
    }
    let record = header_actor(pool.ferrata, &pool.instance_id, slot);
    let request = pool.request(&deposit);
    let state = ConsumerState::Nullifier(NullifierState::Pending { pool, header: record, s, kind, dest });
    Ok(Response::set_state(encode(&state)).send_sync(record, &Message::Attest { request }))
}

/// The private claim at the claimant's own actor: keep the claim in this (private) state and read
/// the record. Only the account's owner runs it; a private-kind deposit only at its destination.
fn claim_deposit_privately(
    input: &ReceiveInput,
    state: Option<ConsumerState>,
    pending: PendingDeposit,
    slot: u64,
) -> Result<Response, Refusal> {
    if !input.is_authorized {
        return Err(code::WRONG_RECEIVER);
    }
    match pending.kind {
        KIND_PRIVATE if input.receiver.account_id.value() != &pending.dest => return Err(code::WRONG_RECEIVER),
        KIND_PRIVATE | KIND_PUBLIC => {}
        _ => return Err(code::WRONG_KIND),
    }
    let claimant = match state {
        None => ClaimantState::default(),
        Some(ConsumerState::Claimant(claimant)) if claimant.pending.is_none() => claimant,
        Some(_) => return Err(code::BAD_STATE),
    };
    let record = header_actor(pending.pool.ferrata, &pending.pool.instance_id, slot);
    let state = ClaimantState { awaiting: Some(record), pending: Some(pending), ..claimant };
    Ok(Response::set_state(encode(&ConsumerState::Claimant(state))).send_sync(record, &Message::Read))
}

/// At `claim(pool, N)`, from an actor of this program after a verified private claim: spend `N`.
fn spend(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<ConsumerState>,
    pool_id: [u8; 32],
    spent: [u8; 32],
    credit: Option<([u8; 32], u128)>,
) -> Result<Response, Refusal> {
    if !input.from_own_program() {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != claim_actor(program, &pool_id, &spent) {
        return Err(code::WRONG_RECEIVER);
    }
    match state {
        None => {}
        Some(ConsumerState::Nullifier(NullifierState::Spent { .. })) => return Err(code::ALREADY_CLAIMED),
        Some(_) => return Err(code::BAD_STATE),
    }
    let response = Response::set_state(encode(&ConsumerState::Nullifier(NullifierState::Spent { pool_id })));
    Ok(match credit {
        None => response,
        Some((dest, amount)) => response.send(holder_credit(program, pool_id, spent, dest, amount)),
    })
}

/// `claim(pool, N)` → `holder(pool, dest)`.
fn holder_credit(program: AccountId, pool_id: [u8; 32], spent: [u8; 32], dest: [u8; 32], amount: u128) -> SyncMessage {
    let message = ConsumerMessage::CreditHolder { pool_id, nullifier: spent, dest, amount };
    SyncMessage::new(holder_actor(program, &pool_id, &dest), &message).with_pda_seeds([holder_seed(&pool_id, &dest)])
}

/// `holder(pool, dest)`: add a claimed deposit, from the claim actor of its nullifier only.
fn credit_holder(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<ConsumerState>,
    pool_id: [u8; 32],
    spent: [u8; 32],
    dest: [u8; 32],
    amount: u128,
) -> Result<Response, Refusal> {
    if input.from != Some(claim_actor(program, &pool_id, &spent)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != holder_actor(program, &pool_id, &dest) {
        return Err(code::WRONG_RECEIVER);
    }
    let holder = match state {
        None => HolderState { pool_id, dest, amount: 0, credits: 0 },
        Some(ConsumerState::Holder(holder)) => holder,
        Some(_) => return Err(code::BAD_STATE),
    };
    let state = HolderState {
        amount: holder.amount.checked_add(amount).ok_or(code::AMOUNT_TOO_LARGE)?,
        credits: holder.credits + 1,
        ..holder
    };
    Ok(Response::set_state(encode(&ConsumerState::Holder(state))))
}

fn reader(state: Option<ConsumerState>) -> Result<ReaderState, Refusal> {
    match state {
        None => Ok(ReaderState::default()),
        Some(ConsumerState::Reader(reader)) => Ok(reader),
        Some(_) => Err(code::BAD_STATE),
    }
}

// ---------------------------------------------------------------------------------------------
// claim(pool, N)
// ---------------------------------------------------------------------------------------------

/// Ask the pool's record for the deposit. The actor this runs at is the nullifier: if it holds a
/// credited deposit, the claim is a replay and is refused before anything is verified.
fn claim(
    state: Option<ConsumerState>,
    pool: Pool,
    slot: u64,
    tx_index: u64,
    log_index: u64,
    proof: Vec<Vec<u8>>,
    header: Option<Vec<u8>>,
) -> Result<Response, Refusal> {
    match state {
        None | Some(ConsumerState::Claim(ClaimState::Pending { .. })) => {}
        Some(ConsumerState::Claim(ClaimState::Spent(_))) => return Err(code::ALREADY_CLAIMED),
        Some(_) => return Err(code::BAD_STATE),
    }
    // The record is addressed from the pool alone: its program, its instance.
    let record = header_actor(pool.ferrata, &pool.instance_id, slot);
    let request = pool.request(tx_index, log_index, proof, header);
    let state = ConsumerState::Claim(ClaimState::Pending { pool, header: record });
    Ok(Response::set_state(encode(&state)).send_sync(record, &Message::Attest { request }))
}

/// `claim(pool, N)` → `balance(pool, owner)`: add a claimed deposit to its owner's balance.
fn credit(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<ConsumerState>,
    pool_id: [u8; 32],
    nullifier: [u8; 32],
    owner: [u8; 20],
    amount: u128,
) -> Result<Response, Refusal> {
    // Only the claim actor of this program and this pool that the nullifier names credits here,
    // and it sends `Credit` only from `reply`, after the attestation.
    if input.from != Some(claim_actor(program, &pool_id, &nullifier)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != balance_actor(program, &pool_id, &owner) {
        return Err(code::WRONG_RECEIVER);
    }
    let balance = match state {
        None => BalanceState { pool_id, owner, amount: 0, credits: 0 },
        Some(ConsumerState::Balance(balance)) => balance,
        Some(_) => return Err(code::BAD_STATE),
    };
    let state = BalanceState {
        amount: balance.amount.checked_add(amount).ok_or(code::AMOUNT_TOO_LARGE)?,
        credits: balance.credits + 1,
        ..balance
    };
    Ok(Response::set_state(encode(&ConsumerState::Balance(state))))
}

// ---------------------------------------------------------------------------------------------
// Replies
// ---------------------------------------------------------------------------------------------

fn reply(input: &ReceiveInput, program: AccountId, state: &ConsumerState) -> Result<Response, Refusal> {
    match state {
        ConsumerState::Reader(reader) => {
            // A call is answered with an attestation, a read with the record.
            let mut next = ReaderState { awaiting: None, calling: false, ..reader.clone() };
            if reader.calling {
                let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
                next.attestation = Some(encode(&attestation));
            } else {
                let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
                let record: RecordState = borsh::from_slice(&reply.state).map_err(|_| code::BAD_REPLY)?;
                next.record = Some(record.record);
            }
            Ok(Response::set_state(encode(&ConsumerState::Reader(next))))
        }
        ConsumerState::Claim(ClaimState::Pending { pool, .. }) => {
            let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
            let deposit = pool.accept(&attestation)?;
            // The nullifier is the statement's: this actor must be the one it names.
            let pool_id = pool.id();
            let spent = nullifier(&pool_id, &deposit.statement_id);
            if input.receiver != claim_actor(program, &pool_id, &spent) {
                return Err(code::NULLIFIER_MISMATCH);
            }
            let balance = balance_actor(program, &pool_id, &deposit.owner);
            let credit =
                ConsumerMessage::Credit { pool_id, nullifier: spent, owner: deposit.owner, amount: deposit.amount };
            Ok(Response::set_state(encode(&ConsumerState::Claim(ClaimState::Spent(deposit))))
                .send(SyncMessage::new(balance, &credit).with_pda_seeds([balance_seed(&pool_id, &deposit.owner)])))
        }
        ConsumerState::Private(private) => {
            let Some(pending) = &private.pending else {
                return Err(code::BAD_STATE);
            };
            let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
            let record: RecordState = borsh::from_slice(&reply.state).map_err(|_| code::BAD_REPLY)?;
            let recorded = record.header().map_err(|_| code::BAD_REPLY)?;
            // The program's own verification, here: the same request against the record read.
            let request = pending.pool.request(
                pending.tx_index,
                pending.log_index,
                pending.proof.clone(),
                pending.header.clone(),
            );
            let attestation = attest::evaluate(&record.instance_id, record.chain_id, &recorded, &request)?;
            let deposit = pending.pool.accept(&attestation)?;
            let state = PrivateState {
                awaiting: None,
                pending: None,
                last: Some((pending.pool.id(), deposit)),
                attested: private.attested + 1,
            };
            Ok(Response::set_state(encode(&ConsumerState::Private(state))))
        }
        ConsumerState::Nullifier(NullifierState::Pending { pool, s, kind, dest, .. }) => {
            let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
            let deposit = pool.accept(&attestation)?;
            if commitment(s, *kind, dest) != deposit.commitment {
                return Err(code::COMMITMENT_MISMATCH);
            }
            let pool_id = pool.id();
            let spent = secret_nullifier(s, &deposit.statement_id);
            if input.receiver != claim_actor(program, &pool_id, &spent) {
                return Err(code::NULLIFIER_MISMATCH);
            }
            Ok(Response::set_state(encode(&ConsumerState::Nullifier(NullifierState::Spent { pool_id })))
                .send(holder_credit(program, pool_id, spent, *dest, deposit.amount)))
        }
        ConsumerState::Claimant(claimant) => {
            let Some(pending) = &claimant.pending else {
                return Err(code::BAD_STATE);
            };
            let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
            let record: RecordState = borsh::from_slice(&reply.state).map_err(|_| code::BAD_REPLY)?;
            let recorded = record.header().map_err(|_| code::BAD_REPLY)?;
            // The program's own verification, inside this execution: the deposit, its hops and
            // its proof never leave it.
            let request = pending.pool.request(&pending.deposit);
            let attestation = attest::evaluate(&record.instance_id, record.chain_id, &recorded, &request)?;
            let deposit = pending.pool.accept(&attestation)?;
            if commitment(&pending.s, pending.kind, &pending.dest) != deposit.commitment {
                return Err(code::COMMITMENT_MISMATCH);
            }
            let pool_id = pending.pool.id();
            let spent = secret_nullifier(&pending.s, &deposit.statement_id);
            let mut holdings = claimant.holdings.clone();
            let credit = if pending.kind == KIND_PRIVATE {
                // Executed as the destination itself (checked when the claim began): the credit
                // is this private state.
                match holdings.iter_mut().find(|(id, _)| *id == pool_id) {
                    Some((_, amount)) => *amount = amount.checked_add(deposit.amount).ok_or(code::AMOUNT_TOO_LARGE)?,
                    None => holdings.push((pool_id, deposit.amount)),
                }
                None
            } else {
                Some((pending.dest, deposit.amount))
            };
            let state = ClaimantState { awaiting: None, pending: None, holdings, claims: claimant.claims + 1 };
            let spend = ConsumerMessage::Spend { pool_id, nullifier: spent, credit };
            Ok(Response::set_state(encode(&ConsumerState::Claimant(state))).send(
                SyncMessage::new(claim_actor(program, &pool_id, &spent), &spend)
                    .with_pda_seeds([claim_seed(&pool_id, &spent)]),
            ))
        }
        ConsumerState::Claim(ClaimState::Spent(_))
        | ConsumerState::Balance(_)
        | ConsumerState::Nullifier(NullifierState::Spent { .. })
        | ConsumerState::Holder(_) => Err(code::BAD_STATE),
    }
}
