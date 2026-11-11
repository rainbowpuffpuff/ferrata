//! The Ferrata bridge program on LEZ v0.4 (`ferrata_lez_bridge_wire`; the plan is `lez/BRIDGE.md`).
//!
//! * **Configuration.** `Init` at `config(cid)` attests the vault's slot 2 through the attestation
//!   program's direct path and writes the configuration only if it holds `cid`.
//! * **Root import.** `ImportRoot` at `config(cid)` attests the vault's slots 0 and 1 under one
//!   record, inside its own transaction, and writes `root(cid, n)` once; refused after `Sunset`.
//! * **Registry and fees.** `Register` at `config(cid)` attests a token's registry entry and fee
//!   schedules from the vault and writes `token(cid, token)` once; `ImportFees` extends its fee
//!   schedules, keyed by leaf index.
//! * **Claim.** `Claim` at `root(cid, n)` (plaintext) or `ClaimPrivately` at the claimant's own
//!   actor (meant to run privately; it reads the root and verifies inside its own execution)
//!   verify the deposit with the bridge core and send `Issue` to `token(cid, token)`, which checks
//!   the denomination and the fee schedule, applies the token's mint freeze and bucket, has
//!   `config(cid)` apply the global ones (`Admit`), and sends `Spend` to
//!   `nullifier(chain, vault, N)`: it spends `N` once and mints the denomination to a public
//!   destination (a private destination credits itself) and the fees to their accounts.
//! * **Burn.** `Burn` at a holder's actor, as a top-level call or a private root only, debits one
//!   denomination and sends the note to `token(cid, token)`, which applies the burn freezes and
//!   buckets and records the note once at `burn(cid, H(note))`.
//! * **Roles.** `Sunset`, `Freeze` and `SetCap` are top-level calls at the role's own actor, which
//!   forwards them to `config(cid)` or `token(cid, token)`; those accept them only from the
//!   configured freeze authority (`Sunset`, `Freeze`) or admin (`SetCap`).
//! * **Clock.** Freezes and buckets run on attested Ethereum time: `Tick` (anyone) and `Freeze`
//!   make the target `Read` a record of the configuration's attestation instance and advance its
//!   clock to that record's execution timestamp, never backwards, in a transaction valid only in a
//!   block stamped at or after it. No message carries a caller's time.
//!
//! A reply is accepted only from the actor this one waits for. Refusals exit with a code
//! (`ferrata_lez_bridge_wire::code`, 240–255); a refusal of the attestation program ends the whole
//! transaction with that program's code, so nothing is written without an attestation.

use ferrata_lez_bridge_wire::{
    burn_actor, burn_seed, check_fees, check_import, check_pairing, check_register, code, config_actor, fees_slots,
    holder_actor, import_request, nullifier_actor, nullifier_seed, pairing_request, record_time, register_slots,
    root_actor, root_seed, token_actor, token_seed, u128_of, vault_request, verify_claim, Address, BridgeConfig,
    BridgeMessage, BridgeState, BurnRecord, CapParams, ClaimWitness, Claimed, ConfigState, Controls, DestKind,
    FeeSchedule, Gate, HolderState, ImportProof, Note, NullifierState, PairingProof, Pending, PendingClaim,
    PendingImport, PendingTime, Root, RootState, ScheduleRange, TimeRecord, Timed, TokenEntry, TokenState, VaultProof,
};
use ferrata_lez_wire::{attest::Attestation, header_actor, Message as AttestMessage};
use lee_core::{
    account::{AccountId, Actor},
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

fn set(state: BridgeState) -> Response {
    Response::set_state(encode(&state))
}

/// A clock moved to the attested time `t` is valid only in a block stamped at or after `t`. An
/// honest record is always behind the block (finality takes minutes); a record ahead of it, which
/// only a captured sync committee could write, then moves no clock unless the block's producer
/// also stamps its block that far ahead.
fn not_before(response: Response, t: u64) -> Response {
    response.timestamp_window(t..)
}

fn ready(config: BridgeConfig, pending: Option<Pending>, gate: Gate, sunset: bool) -> BridgeState {
    BridgeState::Config(ConfigState::Ready { config, pending, gate, sunset })
}

/// `config(cid)` between transactions: the configuration, its global gate, and whether it is sunset.
fn ready_config(state: Option<BridgeState>) -> Result<(BridgeConfig, Gate, bool), Refusal> {
    match state {
        None => Err(code::NOT_FOUND),
        Some(BridgeState::Config(ConfigState::Ready { config, pending: None, gate, sunset })) => {
            Ok((config, gate, sunset))
        }
        Some(_) => Err(code::BAD_STATE),
    }
}

/// `token(cid, token)` between transactions.
fn token_state(state: Option<BridgeState>) -> Result<TokenState, Refusal> {
    match state {
        None => Err(code::NOT_REGISTERED),
        Some(BridgeState::Token(token)) if token.pending.is_none() => Ok(token),
        Some(_) => Err(code::BAD_STATE),
    }
}

fn handle(input: &ReceiveInput) -> Result<Response, Refusal> {
    let program = input.receiver.program_account_id;
    let state: Option<BridgeState> = if input.pre_state.is_empty() {
        None
    } else {
        Some(borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?)
    };
    // A reply: accepted only from the actor this one asked, and only while it waits.
    if let (Some(from), Some(waiting)) = (input.from, &state) {
        match waiting {
            BridgeState::Config(config) if config.awaiting() == Some(from) => {
                return config_reply(input, program, config);
            }
            BridgeState::Token(token) if token.awaiting() == Some(from) => {
                return token_time_reply(input, token);
            }
            BridgeState::Holder(HolderState { pending: Some(pending), .. })
                if from == root_actor(program, &pending.config_id, pending.leaf_count) =>
            {
                return claim_reply(input, program, waiting);
            }
            _ => {}
        }
    }
    let message: BridgeMessage = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    match message {
        BridgeMessage::Burn { config_id, r, memo, mac, token, amount, c_out } => {
            burn(input, program, state, config_id, Note { r, memo, mac, token, amount, c_out })
        }
        BridgeMessage::ImportRoot { config_id, record_slot, root, leaf_count, proof } => {
            import_root(input, program, state, config_id, PendingImport { record_slot, root, leaf_count }, &proof)
        }
        BridgeMessage::Echo { .. } => Ok(Response::keep_state()),
        BridgeMessage::Init { config, record_slot, proof } => init(input, program, state, config, record_slot, &proof),
        BridgeMessage::Claim { config_id, leaf_count, witness, range, relayer } => {
            claim(input, program, state, config_id, leaf_count, witness, range, relayer)
        }
        BridgeMessage::ClaimPrivately { config_id, leaf_count, witness, range, relayer } => {
            let pending = PendingClaim { config_id, leaf_count, witness, range, relayer };
            claim_privately(input, program, state, pending)
        }
        BridgeMessage::Read => {
            let reader = input.from.ok_or(code::WRONG_SENDER)?;
            Ok(Response::keep_state().send_sync(reader, &StateReply::from(input)))
        }
        BridgeMessage::WriteRoot { root } => write_root(input, program, state, root),
        BridgeMessage::Spend {
            nullifier,
            config_id,
            evm_chain_id,
            vault,
            token,
            leaf_count,
            amount,
            mint_to,
            fees,
        } => {
            let spent = Spent { nullifier, config_id, evm_chain_id, vault, token, leaf_count, amount };
            spend(input, program, state, spent, mint_to, fees)
        }
        BridgeMessage::Mint { nullifier, config_id, evm_chain_id, vault, token, amount } => {
            if input.from != Some(nullifier_actor(program, evm_chain_id, &vault, &nullifier)) {
                return Err(code::WRONG_SENDER);
            }
            let mut holder = match state {
                None => HolderState::default(),
                Some(BridgeState::Holder(holder)) => holder,
                Some(_) => return Err(code::BAD_STATE),
            };
            holder.credit(&config_id, &token, amount).ok_or(code::BAD_STATE)?;
            Ok(set(BridgeState::Holder(holder)))
        }
        BridgeMessage::Register { config_id, entry, schedule, record_slot, proof } => {
            register(input, program, state, config_id, entry, schedule, record_slot, &proof)
        }
        BridgeMessage::ImportFees { config_id, token, index, schedule, record_slot, proof } => {
            import_fees(input, program, state, config_id, token, index, schedule, record_slot, &proof)
        }
        BridgeMessage::WriteToken { state: token } => {
            if input.from != Some(config_actor(program, &token.config_id)) {
                return Err(code::WRONG_SENDER);
            }
            if input.receiver != token_actor(program, &token.config_id, &token.entry.token) {
                return Err(code::WRONG_SENDER);
            }
            if state.is_some() {
                return Err(code::ALREADY_WRITTEN);
            }
            Ok(set(BridgeState::Token(token)))
        }
        BridgeMessage::ExtendFees { config_id, token, index, schedule, count } => {
            extend_fees(input, program, state, config_id, token, index, schedule, count)
        }
        BridgeMessage::Issue { config_id, token, nullifier, amount, range, leaf_count, mint_to, relayer } => {
            let claim = Issued { config_id, token, nullifier, amount, leaf_count, mint_to, relayer };
            issue(input, program, state, claim, range)
        }
        BridgeMessage::Admit { config_id, token, mints, value } => {
            admit(input, program, state, config_id, token, mints, value)
        }
        BridgeMessage::Tick { config_id, token, record } => tick(input, program, state, config_id, token, record),
        control @ (BridgeMessage::Sunset { .. } | BridgeMessage::Freeze { .. } | BridgeMessage::SetCap { .. }) => {
            role_message(input, program, state, control)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// config(cid): Init, ImportRoot, Register, ImportFees and their attestations
// ---------------------------------------------------------------------------------------------

/// Ask the vault's slot 2 of the record at `record_slot`; the reply writes the configuration.
fn init(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config: BridgeConfig,
    record_slot: u64,
    proof: &PairingProof,
) -> Result<Response, Refusal> {
    config.controls.check()?;
    let config_id = config.id(program);
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    if state.is_some() {
        return Err(code::ALREADY_WRITTEN);
    }
    let header = header_actor(config.attestation, &config.instance_id, record_slot);
    let request = pairing_request(&config, proof);
    Ok(set(BridgeState::Config(ConfigState::Pairing { config, record_slot }))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// Ask the vault's slots 0 and 1 of the record at `record_slot`; the reply writes the root.
fn import_root(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    pending: PendingImport,
    proof: &ImportProof,
) -> Result<Response, Refusal> {
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    let (config, gate, sunset) = ready_config(state)?;
    // Sunset is one-way: no root is imported after it (SPEC-v2 §4.5).
    if sunset {
        return Err(code::FROZEN);
    }
    let header = header_actor(config.attestation, &config.instance_id, pending.record_slot);
    let request = import_request(&config, proof);
    Ok(set(ready(config, Some(Pending::Import(pending)), gate, sunset))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// Ask the vault's registry entry of `entry.token`, its fee schedules 0 and 1 and its leaf count;
/// the reply writes `token(cid, token)`.
#[allow(clippy::too_many_arguments)]
fn register(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    entry: TokenEntry,
    schedule: FeeSchedule,
    record_slot: u64,
    proof: &VaultProof,
) -> Result<Response, Refusal> {
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    let (config, gate, sunset) = ready_config(state)?;
    entry.check()?;
    if schedule.index != 0 {
        return Err(code::BAD_MESSAGE);
    }
    schedule.check(&entry)?;
    let header = header_actor(config.attestation, &config.instance_id, record_slot);
    let request = vault_request(&config, &register_slots(&entry.token), proof)?;
    Ok(set(ready(config, Some(Pending::Register { entry, schedule, record_slot }), gate, sunset))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// Ask the vault's fee schedule `index` of `token` and its leaf count; the reply extends
/// `token(cid, token)`.
#[allow(clippy::too_many_arguments)]
fn import_fees(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    token: Address,
    index: u64,
    schedule: Option<FeeSchedule>,
    record_slot: u64,
    proof: &VaultProof,
) -> Result<Response, Refusal> {
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    let (config, gate, sunset) = ready_config(state)?;
    let header = header_actor(config.attestation, &config.instance_id, record_slot);
    let request = vault_request(&config, &fees_slots(&token, index), proof)?;
    Ok(set(ready(config, Some(Pending::Fees { token, index, schedule, record_slot }), gate, sunset))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// The attestation program's reply to `Init`, `ImportRoot`, `Register` or `ImportFees` (an
/// attestation), or to the `Read` of a record for the global clock (`Tick`, `Freeze`).
fn config_reply(input: &ReceiveInput, program: AccountId, state: &ConfigState) -> Result<Response, Refusal> {
    if let ConfigState::Ready { config, pending: Some(Pending::Time(pending)), gate, sunset } = state {
        let mut gate = *gate;
        let t = apply_time(input, &config.instance_id, config.evm_chain_id, pending, &mut gate)?;
        return Ok(not_before(set(ready(config.clone(), None, gate, *sunset)), t));
    }
    let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    let config = state.config().clone();
    let config_id = config.id(program);
    let (pending, gate, sunset) = match state {
        ConfigState::Pairing { record_slot, .. } => {
            check_pairing(&config, &config_id, *record_slot, &attestation)?;
            // No record read yet: the clock starts at 0 and the buckets start counting at the
            // first attested time a `Tick` or `Freeze` brings (`Bucket::refill`).
            let gate = Gate::new(config.controls.global_cap, 0);
            return Ok(set(ready(config, None, gate, false)));
        }
        ConfigState::Ready { pending: Some(pending), gate, sunset, .. } => (pending, *gate, *sunset),
        ConfigState::Ready { pending: None, .. } => return Err(code::BAD_STATE),
    };
    let message = match pending {
        Pending::Import(pending) => {
            let root = check_import(&config, &config_id, pending, &attestation)?;
            let to = root_actor(program, &config_id, root.leaf_count);
            let seed = root_seed(&config_id, root.leaf_count);
            SyncMessage::new(to, &BridgeMessage::WriteRoot { root }).with_pda_seeds([seed])
        }
        Pending::Register { entry, schedule, record_slot } => {
            let token = check_register(&config, &config_id, entry, schedule, *record_slot, &attestation, gate.now_ms)?;
            let to = token_actor(program, &config_id, &entry.token);
            SyncMessage::new(to, &BridgeMessage::WriteToken { state: token })
                .with_pda_seeds([token_seed(&config_id, &entry.token)])
        }
        Pending::Fees { token, index, schedule, record_slot } => {
            let count = check_fees(&config, token, *index, schedule, *record_slot, &attestation)?;
            let extend = BridgeMessage::ExtendFees {
                config_id,
                token: *token,
                index: *index,
                schedule: schedule.clone(),
                count,
            };
            SyncMessage::new(token_actor(program, &config_id, token), &extend)
                .with_pda_seeds([token_seed(&config_id, token)])
        }
        Pending::Time(_) => return Err(code::BAD_STATE),
    };
    Ok(set(ready(config, None, gate, sunset)).send(message))
}

/// `config(cid)` → `root(cid, n)`: write the verified root once.
fn write_root(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    root: RootState,
) -> Result<Response, Refusal> {
    if input.from != Some(config_actor(program, &root.config_id)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != root_actor(program, &root.config_id, root.leaf_count) {
        return Err(code::WRONG_SENDER);
    }
    if state.is_some() {
        return Err(code::ALREADY_WRITTEN);
    }
    Ok(set(BridgeState::Root(root)))
}

/// `config(cid)` → `token(cid, token)`: append an attested fee schedule, or extend the leaves the
/// schedules cover to an attested count at which schedule `index` did not exist.
#[allow(clippy::too_many_arguments)]
fn extend_fees(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    token: Address,
    index: u64,
    schedule: Option<FeeSchedule>,
    count: u64,
) -> Result<Response, Refusal> {
    if input.from != Some(config_actor(program, &config_id)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != token_actor(program, &config_id, &token) {
        return Err(code::WRONG_SENDER);
    }
    let mut state = token_state(state)?;
    if index != state.schedules.len() as u64 {
        return Err(code::IMPORT_MISMATCH);
    }
    match schedule {
        Some(schedule) => {
            schedule.check(&state.entry)?;
            let last = state.schedules.last().map_or(0, |s| s.from_leaf);
            // A schedule takes effect at a leaf no earlier than any count at which it did not exist.
            if schedule.from_leaf < state.known_until || schedule.from_leaf < last {
                return Err(code::IMPORT_MISMATCH);
            }
            state.known_until = schedule.from_leaf;
            state.schedules.push(schedule);
        }
        None => state.known_until = state.known_until.max(count),
    }
    Ok(set(BridgeState::Token(state)))
}

// ---------------------------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------------------------

/// What `Spend` spends, at the nullifier.
struct Spent {
    nullifier: Root,
    config_id: Root,
    evm_chain_id: u64,
    vault: Address,
    token: Address,
    leaf_count: u64,
    amount: u128,
}

/// A verified claim as `token(cid, token)` receives it (`Issue`).
struct Issued {
    config_id: Root,
    token: Address,
    nullifier: Root,
    amount: u128,
    leaf_count: u64,
    mint_to: Option<Root>,
    relayer: Option<Root>,
}

/// The `Issue` of a verified claim, to `token(cid, token)`.
fn issue_message(
    program: AccountId,
    root: &RootState,
    token: Address,
    claimed: &Claimed,
    range: ScheduleRange,
    mint_to: Option<Root>,
    relayer: Option<Root>,
) -> SyncMessage {
    let message = BridgeMessage::Issue {
        config_id: root.config_id,
        token,
        nullifier: claimed.nullifier,
        amount: claimed.amount,
        range,
        leaf_count: root.leaf_count,
        mint_to,
        relayer,
    };
    SyncMessage::new(token_actor(program, &root.config_id, &token), &message)
}

/// At `root(cid, n)`: the plaintext claim. The whole witness is in the transaction; the
/// destination must be a public account.
#[allow(clippy::too_many_arguments)]
fn claim(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    leaf_count: u64,
    witness: ClaimWitness,
    range: ScheduleRange,
    relayer: Option<Root>,
) -> Result<Response, Refusal> {
    if input.receiver != root_actor(program, &config_id, leaf_count) {
        return Err(code::WRONG_SENDER);
    }
    let root = match state {
        None => return Err(code::NOT_FOUND),
        Some(BridgeState::Root(root)) => root,
        Some(_) => return Err(code::BAD_STATE),
    };
    let claimed = verify_claim(&root, &witness, &range)?;
    if claimed.kind != DestKind::Public {
        return Err(code::WRONG_DESTINATION);
    }
    let issue = issue_message(program, &root, witness.token, &claimed, range, Some(witness.dest), relayer);
    Ok(Response::keep_state().send(issue))
}

/// At the claimant's own actor: keep the claim in this (private) state and read the root.
fn claim_privately(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    pending: PendingClaim,
) -> Result<Response, Refusal> {
    // Only the account's owner runs its actor (a private account's key, inside the proof).
    if !input.is_authorized {
        return Err(code::WRONG_DESTINATION);
    }
    let holder = match state {
        None => HolderState::default(),
        Some(BridgeState::Holder(holder)) if holder.pending.is_none() => holder,
        Some(_) => return Err(code::BAD_STATE),
    };
    let root = root_actor(program, &pending.config_id, pending.leaf_count);
    let holder = HolderState { pending: Some(pending), ..holder };
    Ok(set(BridgeState::Holder(holder)).send_sync(root, &BridgeMessage::Read))
}

/// The root's reply to a private claim: verify here (the leaf index stays in this execution),
/// credit this account if it is the destination (a private-kind deposit), and `Issue`.
fn claim_reply(input: &ReceiveInput, program: AccountId, state: &BridgeState) -> Result<Response, Refusal> {
    let BridgeState::Holder(HolderState { holdings, pending: Some(pending) }) = state else {
        return Err(code::BAD_STATE);
    };
    let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    if reply.state.is_empty() {
        return Err(code::NOT_FOUND);
    }
    let BridgeState::Root(root) = borsh::from_slice(&reply.state).map_err(|_| code::BAD_STATE)? else {
        return Err(code::BAD_STATE);
    };
    let witness = &pending.witness;
    let claimed = verify_claim(&root, witness, &pending.range)?;
    let mut holder = HolderState { holdings: holdings.clone(), pending: None };
    let mint_to = match claimed.kind {
        // Executed as the destination account itself: the mint is this private state.
        DestKind::Private => {
            if input.receiver.account_id.value() != &witness.dest {
                return Err(code::WRONG_DESTINATION);
            }
            holder.credit(&root.config_id, &witness.token, claimed.amount).ok_or(code::BAD_STATE)?;
            None
        }
        DestKind::Public => Some(witness.dest),
    };
    let issue = issue_message(program, &root, witness.token, &claimed, pending.range, mint_to, pending.relayer);
    Ok(set(BridgeState::Holder(holder)).send(issue))
}

/// At `token(cid, token)`, from an actor of this program after a verified claim: the denomination,
/// the fees of the claimed schedule range, the token's mint freeze and bucket; then the global
/// ones at `config(cid)` and the spend at the nullifier.
fn issue(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    claim: Issued,
    range: ScheduleRange,
) -> Result<Response, Refusal> {
    if !input.from_own_program() {
        return Err(code::WRONG_SENDER);
    }
    let (config_id, token) = (claim.config_id, claim.token);
    if input.receiver != token_actor(program, &config_id, &token) {
        return Err(code::WRONG_SENDER);
    }
    let mut state = token_state(state)?;
    let (mint_fee, relay_fee) = state.fees_for(claim.amount, &range)?;
    let minted = claim.amount.checked_add(mint_fee).and_then(|m| m.checked_add(relay_fee)).ok_or(code::BAD_STATE)?;
    let value = state.entry.value(minted).ok_or(code::BAD_STATE)?;
    state.gate.admit(true, value)?;
    let fee_recipient = state.controls.fee_recipient;
    let mut fees = Vec::new();
    if mint_fee > 0 {
        fees.push((fee_recipient, mint_fee));
    }
    if relay_fee > 0 {
        fees.push((claim.relayer.unwrap_or(fee_recipient), relay_fee));
    }
    let (evm_chain_id, vault) = (state.evm_chain_id, state.vault);
    let to = nullifier_actor(program, evm_chain_id, &vault, &claim.nullifier);
    let seed = nullifier_seed(evm_chain_id, &vault, &claim.nullifier);
    let spend = BridgeMessage::Spend {
        nullifier: claim.nullifier,
        config_id,
        evm_chain_id,
        vault,
        token,
        leaf_count: claim.leaf_count,
        amount: claim.amount,
        mint_to: claim.mint_to,
        fees,
    };
    let admit = BridgeMessage::Admit { config_id, token, mints: true, value };
    Ok(set(BridgeState::Token(state))
        .send_sync(config_actor(program, &config_id), &admit)
        .send(SyncMessage::new(to, &spend).with_pda_seeds([seed])))
}

/// At `config(cid)`, from `token(cid, token)`: the global freeze and bucket of a mint or a burn, at
/// the configuration's attested time.
fn admit(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    token: Address,
    mints: bool,
    value: u128,
) -> Result<Response, Refusal> {
    if input.from != Some(token_actor(program, &config_id, &token)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    let (config, mut gate, sunset) = ready_config(state)?;
    gate.admit(mints, value)?;
    Ok(set(ready(config, None, gate, sunset)))
}

/// At `nullifier(chain, vault, N)`, from `token(cid, token)` after a verified, admitted claim:
/// spend `N` once; mint the denomination to a public destination and each fee to its account.
fn spend(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    spent: Spent,
    mint_to: Option<Root>,
    fees: Vec<(Root, u128)>,
) -> Result<Response, Refusal> {
    if input.from != Some(token_actor(program, &spent.config_id, &spent.token)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != nullifier_actor(program, spent.evm_chain_id, &spent.vault, &spent.nullifier) {
        return Err(code::WRONG_SENDER);
    }
    if state.is_some() {
        return Err(code::ALREADY_CLAIMED);
    }
    let record = NullifierState {
        config_id: spent.config_id,
        token: spent.token,
        leaf_count: spent.leaf_count,
        amount: spent.amount,
    };
    let mint = |amount: u128| BridgeMessage::Mint {
        nullifier: spent.nullifier,
        config_id: spent.config_id,
        evm_chain_id: spent.evm_chain_id,
        vault: spent.vault,
        token: spent.token,
        amount,
    };
    let mut response = set(BridgeState::Nullifier(record));
    let mints = mint_to.map(|dest| (dest, spent.amount)).into_iter().chain(fees);
    for (account, amount) in mints {
        response = response.send_sync(holder_actor(program, AccountId::new(account)), &mint(amount));
    }
    Ok(response)
}

// ---------------------------------------------------------------------------------------------
// Burn
// ---------------------------------------------------------------------------------------------

/// `Burn` at a holder's actor (debit, then send to the token), at `token(cid, token)` (admit, then
/// record), or at `burn(cid, H(note))` (record once).
fn burn(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    note: Note,
) -> Result<Response, Refusal> {
    let note_hash = note.hash();
    let record = burn_actor(program, &config_id, &note_hash);
    let token = token_actor(program, &config_id, &note.token);
    if input.receiver == record {
        if input.from != Some(token) {
            return Err(code::WRONG_SENDER);
        }
        if state.is_some() {
            return Err(code::ALREADY_WRITTEN);
        }
        return Ok(set(BridgeState::Burn(BurnRecord { config_id, note })));
    }
    if input.receiver == token {
        // Only a holder's actor of this program sends here, after the debit. In a private burn
        // this delivery is the boundary message the checkpoint reads.
        if !input.from_own_program() {
            return Err(code::WRONG_SENDER);
        }
        let mut state = token_state(state)?;
        let amount = u128_of(&note.amount).ok_or(code::NOT_REGISTERED)?;
        state.entry.index_of(amount).ok_or(code::NOT_REGISTERED)?;
        let value = state.entry.value(amount).ok_or(code::BAD_STATE)?;
        // The token's attested clock applies, as for a claim.
        state.gate.admit(false, value)?;
        let admit = BridgeMessage::Admit { config_id, token: note.token, mints: false, value };
        let publish = SyncMessage::new(record, &BridgeMessage::burn(config_id, &note))
            .with_pda_seeds([burn_seed(&config_id, &note_hash)]);
        return Ok(set(BridgeState::Token(state)).send_sync(config_actor(program, &config_id), &admit).send(publish));
    }
    // D10: only a top-level call (or a private execution's root) is visible to the checkpoint; a
    // nested call or an asynchronous message carries a sender and is refused.
    if input.from.is_some() || !input.is_authorized {
        return Err(code::NOT_AUTHORIZED);
    }
    let mut holder = match state {
        None => return Err(code::INSUFFICIENT_BALANCE),
        Some(BridgeState::Holder(holder)) if holder.pending.is_none() => holder,
        Some(_) => return Err(code::BAD_STATE),
    };
    let index = holder
        .holdings
        .iter()
        .position(|h| h.config_id == config_id && h.token == note.token)
        .ok_or(code::INSUFFICIENT_BALANCE)?;
    let amount = u128_of(&note.amount).ok_or(code::NOT_REGISTERED)?;
    let left = holder.holdings[index].amount.checked_sub(amount).ok_or(code::INSUFFICIENT_BALANCE)?;
    if left == 0 {
        holder.holdings.remove(index);
    } else {
        holder.holdings[index].amount = left;
    }
    Ok(set(BridgeState::Holder(holder)).send_sync(token, &BridgeMessage::burn(config_id, &note)))
}

// ---------------------------------------------------------------------------------------------
// Roles: Sunset, Freeze, SetCap; and the clock (Tick)
// ---------------------------------------------------------------------------------------------

/// The gate a role message or a `Tick` addresses: `config(cid)`, or `token(cid, token)`.
fn gate_actor(program: AccountId, config_id: &Root, token: &Option<Address>) -> Actor {
    match token {
        None => config_actor(program, config_id),
        Some(token) => token_actor(program, config_id, token),
    }
}

/// `Sunset`, `Freeze` or `SetCap`. At the role's own actor (any actor that is not the target),
/// a top-level call or private root the role signed is forwarded to the target; the target
/// (`config(cid)`, or `token(cid, token)`) accepts it only from the configured role's actor.
/// `Sunset` must be such a visible call (D10): the checkpoint sees only those. A `Freeze` is
/// applied once the record it names has replied, against the attested time it gives.
fn role_message(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    message: BridgeMessage,
) -> Result<Response, Refusal> {
    let (config_id, token) = match &message {
        BridgeMessage::Sunset { config_id } => (*config_id, None),
        BridgeMessage::Freeze { config_id, token, .. } | BridgeMessage::SetCap { config_id, token, .. } => {
            (*config_id, *token)
        }
        _ => return Err(code::BAD_MESSAGE),
    };
    let target = gate_actor(program, &config_id, &token);
    if input.receiver != target {
        if input.from.is_some() || !input.is_authorized {
            return Err(code::NOT_AUTHORIZED);
        }
        return Ok(Response::keep_state().send_sync(target, &message));
    }
    let from_role = |role: &Root| input.from == Some(holder_actor(program, AccountId::new(*role)));
    let (controls, mut gate, scope) = load_scope(&token, state)?;
    match message {
        BridgeMessage::Sunset { .. } => {
            if !from_role(&controls.freeze_authority) {
                return Err(code::NOT_AUTHORIZED);
            }
            let Scope::Config(config, sunset) = scope else { return Err(code::BAD_MESSAGE) };
            if sunset {
                return Err(code::ALREADY_WRITTEN);
            }
            Ok(set(ready(config, None, gate, true)))
        }
        BridgeMessage::Freeze { mints, burns, until_ms, record, .. } => {
            if !from_role(&controls.freeze_authority) {
                return Err(code::NOT_AUTHORIZED);
            }
            Ok(scope.ask_time(gate, PendingTime { record, then: Timed::Freeze { mints, burns, until_ms } }))
        }
        BridgeMessage::SetCap { mints, cap, .. } => {
            if !from_role(&controls.admin) {
                return Err(code::NOT_AUTHORIZED);
            }
            set_cap(&mut gate, mints, cap, &controls.floor)?;
            Ok(set(scope.with_gate(gate)))
        }
        _ => Err(code::BAD_MESSAGE),
    }
}

/// A gate's actor state, with what it keeps besides its gate.
enum Scope {
    Config(BridgeConfig, bool),
    Token(TokenState),
}

/// The target's roles, gate and scope, between transactions.
fn load_scope(token: &Option<Address>, state: Option<BridgeState>) -> Result<(Controls, Gate, Scope), Refusal> {
    match token {
        None => {
            let (config, gate, sunset) = ready_config(state)?;
            Ok((config.controls, gate, Scope::Config(config, sunset)))
        }
        Some(_) => {
            let token = token_state(state)?;
            Ok((token.controls, token.gate, Scope::Token(token)))
        }
    }
}

impl Scope {
    fn with_gate(self, gate: Gate) -> BridgeState {
        match self {
            Scope::Config(config, sunset) => ready(config, None, gate, sunset),
            Scope::Token(token) => BridgeState::Token(TokenState { gate, ..token }),
        }
    }

    /// Wait for the record `pending` names: `Read` it from the configuration's attestation
    /// program (the reply is accepted only from that record's actor, within this transaction).
    fn ask_time(self, gate: Gate, pending: PendingTime) -> Response {
        let slot = pending.record.record_slot;
        let (state, record) = match self {
            Scope::Config(config, sunset) => {
                let record = header_actor(config.attestation, &config.instance_id, slot);
                (ready(config, Some(Pending::Time(pending)), gate, sunset), record)
            }
            Scope::Token(token) => {
                let record = header_actor(token.attestation, &token.instance_id, slot);
                (BridgeState::Token(TokenState { gate, pending: Some(pending), ..token }), record)
            }
        };
        set(state).send_sync(record, &AttestMessage::Read)
    }
}

/// The admin's new mint (or burn) bucket, refilled at the gate's attested time under the old
/// parameters first: never below the floor.
fn set_cap(gate: &mut Gate, mints: bool, cap: CapParams, floor: &CapParams) -> Result<(), Refusal> {
    if !cap.at_least(floor) {
        return Err(code::BAD_MESSAGE);
    }
    let now = gate.now_ms;
    let bucket = if mints { &mut gate.mint } else { &mut gate.burn };
    bucket.set(cap, now);
    Ok(())
}

/// Anyone, at `config(cid)` or `token(cid, token)`: read `record` and advance the clock to it.
fn tick(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    token: Option<Address>,
    record: TimeRecord,
) -> Result<Response, Refusal> {
    if input.receiver != gate_actor(program, &config_id, &token) {
        return Err(code::WRONG_SENDER);
    }
    let (_, gate, scope) = load_scope(&token, state)?;
    Ok(scope.ask_time(gate, PendingTime { record, then: Timed::Tick }))
}

/// The record's reply to a gate's `Read`: its attested time ([`record_time`]); the clock advanced
/// to it (never back), then the freeze the gate was waiting to apply, if any.
fn apply_time(
    input: &ReceiveInput,
    instance_id: &Root,
    evm_chain_id: u64,
    pending: &PendingTime,
    gate: &mut Gate,
) -> Result<u64, Refusal> {
    let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    let t = record_time(instance_id, evm_chain_id, &reply.state, pending.record.header.as_deref())?;
    gate.advance(t);
    if let Timed::Freeze { mints, burns, until_ms } = pending.then {
        gate.freeze(mints, burns, until_ms)?;
    }
    Ok(t)
}

/// At `token(cid, token)`: the reply of the record it asked for.
fn token_time_reply(input: &ReceiveInput, token: &TokenState) -> Result<Response, Refusal> {
    let mut token = token.clone();
    let pending = token.pending.take().ok_or(code::BAD_STATE)?;
    let t = apply_time(input, &token.instance_id, token.evm_chain_id, &pending, &mut token.gate)?;
    Ok(not_before(set(BridgeState::Token(token)), t))
}
