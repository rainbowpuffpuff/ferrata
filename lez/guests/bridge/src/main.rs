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
//!   destination (a private destination credits itself) and the fees to their accounts. A mint
//!   writes only at `holder(account)`, a PDA whose seed names the account: never at the account a
//!   claim names, which could be another actor's address (security review C-1).
//! * **Burn.** `Burn` at the holder's own actor, as a top-level call or a private root only, debits
//!   one denomination (its own state first, else its `holder(account)`, to which it forwards the
//!   note) and sends the note to `token(cid, token)`, which applies the burn freezes and buckets
//!   and records the note once at `burn(cid, H(note))`.
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
    burn_actor, burn_seed, check_fees, check_import, check_pairing, check_register, check_retired, code, config_actor,
    fees_slots, holder_actor, holder_seed, import_request, nullifier_actor, nullifier_seed, own_actor, pairing_request,
    record_time, register_slots, root_actor, root_seed, token_actor, token_seed, u128_of, vault_request, verify_claim,
    Address, BridgeConfig, BridgeMessage, BridgeState, BurnRecord, CapParams, ClaimWitness, Claimed, ConfigState,
    CosignedAction, DestKind, FeeSchedule, Gate, HolderState, ImportProof, Note, NullifierState, PairingProof, Pending,
    PendingClaim, PendingImport, PendingTime, RoleProposal, Roles, Root, RootState, ScheduleRange, TimeRecord, Timed,
    TokenEntry, TokenState, VaultProof, MAX_WATCHERS, PROPOSAL_RECORD_AGE_MS, ROLE_TIMELOCK_MS, SLOT_RETIRED,
};
use ferrata_lez_wire::{attest::Attestation, header_actor, Message as AttestMessage};
use lee_core::{
    account::AccountId,
    program::{read_input_frame, ReceiveInput, Response, StateReply, SyncMessage, ValidityWindow},
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

fn ready(config: BridgeConfig, pending: Option<Pending>, gate: Gate, sunset: bool, roles: Roles) -> BridgeState {
    BridgeState::Config(ConfigState::Ready { config, pending, gate, sunset, roles })
}

/// `config(cid)` between transactions: the configuration, its global gate, whether it is sunset,
/// and its live roles.
fn ready_config(state: Option<BridgeState>) -> Result<(BridgeConfig, Gate, bool, Roles), Refusal> {
    match state {
        None => Err(code::NOT_FOUND),
        Some(BridgeState::Config(ConfigState::Ready { config, pending: None, gate, sunset, roles })) => {
            Ok((config, gate, sunset, roles))
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
        BridgeMessage::Mint { nullifier, config_id, evm_chain_id, vault, token, holder, amount } => {
            if input.from != Some(nullifier_actor(program, evm_chain_id, &vault, &nullifier)) {
                return Err(code::WRONG_SENDER);
            }
            // C-1: credits land at `holder(account)` only, never at an address another role uses.
            if input.receiver != holder_actor(program, AccountId::new(holder)) {
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
        control @ (BridgeMessage::Sunset { .. }
        | BridgeMessage::Freeze { .. }
        | BridgeMessage::SetCap { .. }
        | BridgeMessage::ProposeRoles { .. }
        | BridgeMessage::CancelRoles { .. }
        | BridgeMessage::Pause { .. }) => role_message(input, program, state, control),
        BridgeMessage::AttestRetired { config_id, record_slot, proof } => {
            attest_retired(input, program, state, config_id, record_slot, &proof)
        }
        BridgeMessage::ApplyRoles { config_id, record } => apply_roles(input, program, state, config_id, record),
        BridgeMessage::Cosigned { config_id, signers, action } => {
            cosigned(input, program, state, config_id, signers, action)
        }
        BridgeMessage::Transfer { config_id, token, amount, to } => {
            transfer(input, program, state, config_id, token, amount, to)
        }
        BridgeMessage::Credit { config_id, token, amount, to } => {
            credit(input, program, state, config_id, token, amount, to)
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
    // D10, security review H-1: the checkpoint counts only the imports it sees, a top-level call or
    // a private root. A nested call or an asynchronous message carries a sender and is refused, so
    // no executed import is missing from the checkpoint's imported count (the refund boundary).
    if input.from.is_some() {
        return Err(code::NOT_AUTHORIZED);
    }
    let (config, gate, sunset, roles) = ready_config(state)?;
    // Sunset is one-way: no root is imported after it (SPEC-v2 §4.5).
    if sunset {
        return Err(code::FROZEN);
    }
    let header = header_actor(config.attestation, &config.instance_id, pending.record_slot);
    let request = import_request(&config, proof);
    Ok(set(ready(config, Some(Pending::Import(pending)), gate, sunset, roles))
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
    let (config, gate, sunset, roles) = ready_config(state)?;
    entry.check()?;
    if schedule.index != 0 {
        return Err(code::BAD_MESSAGE);
    }
    schedule.check(&entry)?;
    let header = header_actor(config.attestation, &config.instance_id, record_slot);
    let request = vault_request(&config, &register_slots(&entry.token), proof)?;
    Ok(set(ready(config, Some(Pending::Register { entry, schedule, record_slot }), gate, sunset, roles))
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
    let (config, gate, sunset, roles) = ready_config(state)?;
    let header = header_actor(config.attestation, &config.instance_id, record_slot);
    let request = vault_request(&config, &fees_slots(&token, index), proof)?;
    Ok(set(ready(config, Some(Pending::Fees { token, index, schedule, record_slot }), gate, sunset, roles))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// The attestation program's reply to `Init`, `ImportRoot`, `Register` or `ImportFees` (an
/// attestation), or to the `Read` of a record for the global clock (`Tick`, `Freeze`).
fn config_reply(input: &ReceiveInput, program: AccountId, state: &ConfigState) -> Result<Response, Refusal> {
    if let ConfigState::Ready { config, pending: Some(Pending::Time(pending)), gate, sunset, roles } = state {
        let mut gate = *gate;
        let mut roles = roles.clone();
        let t = apply_time(input, &config.instance_id, config.evm_chain_id, pending, &mut gate, Some(&mut roles))?;
        let response = set(ready(config.clone(), None, gate, *sunset, roles));
        if let Timed::Propose { .. } = pending.then {
            // The timelock runs from attested time: the proposal's record must be fresh, so it is
            // valid only in a block stamped within a day after that record.
            let end = t.checked_add(PROPOSAL_RECORD_AGE_MS).ok_or(code::BAD_STATE)?;
            let window = ValidityWindow::try_from(t..end).map_err(|_| code::BAD_STATE)?;
            return Ok(response.timestamp_window(window));
        }
        return Ok(not_before(response, t));
    }
    let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    let config = state.config().clone();
    let config_id = config.id(program);
    let (pending, gate, sunset, mut roles) = match state {
        ConfigState::Pairing { record_slot, .. } => {
            check_pairing(&config, &config_id, *record_slot, &attestation)?;
            // No record read yet: the clock starts at 0 and the buckets start counting at the
            // first attested time a `Tick` or `Freeze` brings (`Bucket::refill`).
            let gate = Gate::new(config.controls.global_cap, 0);
            let roles = Roles::new(&config.controls);
            return Ok(set(ready(config, None, gate, false, roles)));
        }
        ConfigState::Ready { pending: Some(pending), gate, sunset, roles, .. } => {
            (pending, *gate, *sunset, roles.clone())
        }
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
        Pending::Retired { record_slot } => {
            check_retired(&config, *record_slot, &attestation)?;
            roles.vault_retired = true;
            return Ok(set(ready(config, None, gate, sunset, roles)));
        }
        Pending::Time(_) => return Err(code::BAD_STATE),
    };
    Ok(set(ready(config, None, gate, sunset, roles)).send(message))
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
    let (config, mut gate, sunset, roles) = ready_config(state)?;
    gate.admit(mints, value)?;
    Ok(set(ready(config, None, gate, sunset, roles)))
}

/// At `nullifier(chain, vault, N)`, from `token(cid, token)` after a verified, admitted claim:
/// spend `N` once; mint the denomination to a public destination and each fee to its account, at
/// the account's `holder(account)` PDA (seeded, so LEZ admits it the first time).
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
    let mint = |holder: Root, amount: u128| BridgeMessage::Mint {
        nullifier: spent.nullifier,
        config_id: spent.config_id,
        evm_chain_id: spent.evm_chain_id,
        vault: spent.vault,
        token: spent.token,
        holder,
        amount,
    };
    let mut response = set(BridgeState::Nullifier(record));
    let mints = mint_to.map(|dest| (dest, spent.amount)).into_iter().chain(fees);
    for (account, amount) in mints {
        let to = holder_actor(program, AccountId::new(account));
        response = response.send(SyncMessage::new(to, &mint(account, amount)).with_pda_seeds([holder_seed(&account)]));
    }
    Ok(response)
}

// ---------------------------------------------------------------------------------------------
// Burn
// ---------------------------------------------------------------------------------------------

/// `Burn` at a holder's own actor (debit its own state, or forward to its `holder(account)`), at
/// `holder(account)` from that own actor (debit), at `token(cid, token)` (admit, then record), or
/// at `burn(cid, H(note))` (record once). A debit sends the note to the token.
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
    let holder = |state: Option<BridgeState>| match state {
        None => Ok(HolderState::default()),
        Some(BridgeState::Holder(holder)) if holder.pending.is_none() => Ok(holder),
        Some(_) => Err(code::BAD_STATE),
    };
    let burn = BridgeMessage::burn(config_id, &note);
    let account = input.receiver.account_id;
    match input.from {
        // D10: only a top-level call (or a private execution's root) is visible to the checkpoint.
        None => {
            if !input.is_authorized {
                return Err(code::NOT_AUTHORIZED);
            }
            let amount = u128_of(&note.amount).ok_or(code::NOT_REGISTERED)?;
            // The account's own state first (its own private claims; a private holder's balance).
            let mut own = holder(state)?;
            if own.debit(&config_id, &note.token, amount).is_some() {
                return Ok(set(BridgeState::Holder(own)).send_sync(token, &burn));
            }
            // Else its public holdings, at `holder(account)`: a nested call, which the checkpoint
            // does not see and need not see, since this root call is the burn it collects.
            let forward =
                SyncMessage::new(holder_actor(program, account), &burn).with_pda_seeds([holder_seed(account.value())]);
            Ok(Response::keep_state().send(forward))
        }
        // At `holder(owner)`: the forward from the owner's own actor (only this program's code runs
        // there, and it forwards only after the checks above).
        Some(from)
            if from == own_actor(program, from.account_id)
                && input.receiver == holder_actor(program, from.account_id) =>
        {
            let amount = u128_of(&note.amount).ok_or(code::NOT_REGISTERED)?;
            let mut held = holder(state)?;
            held.debit(&config_id, &note.token, amount).ok_or(code::INSUFFICIENT_BALANCE)?;
            Ok(set(BridgeState::Holder(held)).send_sync(token, &burn))
        }
        // D10: a nested call from any other actor, or an asynchronous message, is refused.
        Some(_) => Err(code::NOT_AUTHORIZED),
    }
}

// ---------------------------------------------------------------------------------------------
// Transfers between holders
// ---------------------------------------------------------------------------------------------

/// `Transfer` at a holder's own actor (a call it authorised: debit its own state, or forward to its
/// `holder(account)`), at `holder(account)` from that own actor (debit), or at `token(cid, token)`
/// from a holder's actor of this program after the debit (a multiple of the LEZ unit; then
/// `Credit` to `holder(to)`). From a private account the debit and the sender stay private: only
/// the delivery to the token (the boundary) is public.
fn transfer(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    token: Address,
    amount: u128,
    to: Root,
) -> Result<Response, Refusal> {
    let message = BridgeMessage::Transfer { config_id, token, amount, to };
    let token_at = token_actor(program, &config_id, &token);
    if input.receiver == token_at {
        // Only a holder's actor of this program sends here, after its debit.
        if !input.from_own_program() {
            return Err(code::WRONG_SENDER);
        }
        let state = token_state(state)?;
        if amount == 0 || amount % state.entry.lez_unit != 0 {
            return Err(code::BAD_MESSAGE);
        }
        let credit = SyncMessage::new(
            holder_actor(program, AccountId::new(to)),
            &BridgeMessage::Credit { config_id, token, amount, to },
        )
        .with_pda_seeds([holder_seed(&to)]);
        return Ok(Response::keep_state().send(credit));
    }
    let holder = |state: Option<BridgeState>| match state {
        None => Ok(HolderState::default()),
        Some(BridgeState::Holder(holder)) if holder.pending.is_none() => Ok(holder),
        Some(_) => Err(code::BAD_STATE),
    };
    // At `holder(owner)`: the forward from the owner's own actor.
    if let Some(from) = input.from {
        if from == own_actor(program, from.account_id) && input.receiver == holder_actor(program, from.account_id) {
            let mut held = holder(state)?;
            held.debit(&config_id, &token, amount).ok_or(code::INSUFFICIENT_BALANCE)?;
            return Ok(set(BridgeState::Holder(held)).send_sync(token_at, &message));
        }
    }
    // At the holder's own actor: the holder authorised this call (its signature, or its private
    // account's key in a private execution).
    if !input.is_authorized {
        return Err(code::NOT_AUTHORIZED);
    }
    let mut own = holder(state)?;
    if own.debit(&config_id, &token, amount).is_some() {
        return Ok(set(BridgeState::Holder(own)).send_sync(token_at, &message));
    }
    let account = input.receiver.account_id;
    let forward =
        SyncMessage::new(holder_actor(program, account), &message).with_pda_seeds([holder_seed(account.value())]);
    Ok(Response::keep_state().send(forward))
}

/// `token(cid, token)` → `holder(to)`: credit a transfer (as `Mint` credits a claim).
fn credit(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    token: Address,
    amount: u128,
    to: Root,
) -> Result<Response, Refusal> {
    if input.from != Some(token_actor(program, &config_id, &token)) {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != holder_actor(program, AccountId::new(to)) {
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

// ---------------------------------------------------------------------------------------------
// Roles: Sunset, Freeze, SetCap, the handover, watchers, the multisig; and the clock (Tick)
// ---------------------------------------------------------------------------------------------

/// The account of `input.from` if it is an actor of this program (a role's own actor).
fn from_account(input: &ReceiveInput, program: AccountId) -> Option<Root> {
    input.from.filter(|from| from.program_account_id == program).map(|from| *from.account_id.value())
}

/// `Sunset`, `Freeze`, `SetCap`, `ProposeRoles`, `CancelRoles` or `Pause`. At the role's own
/// actor (any actor that is not `config(cid)` or the token the message names), a top-level call
/// or private root the role signed is forwarded to `config(cid)`, which checks the role against
/// its live roles and applies the message, or forwards a token's to `token(cid, token)`, which
/// accepts role messages only from `config(cid)`. A `Sunset` sent at `config(cid)` itself, a
/// top-level call, executes for anyone once the vault is attested retired or the multisig
/// approved it. Every `Sunset` that executes is thus the root call (D10): the checkpoint sees it.
fn role_message(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    message: BridgeMessage,
) -> Result<Response, Refusal> {
    let (config_id, token) = match &message {
        BridgeMessage::Sunset { config_id }
        | BridgeMessage::ProposeRoles { config_id, .. }
        | BridgeMessage::CancelRoles { config_id } => (*config_id, None),
        BridgeMessage::Freeze { config_id, token, .. }
        | BridgeMessage::SetCap { config_id, token, .. }
        | BridgeMessage::Pause { config_id, token, .. } => (*config_id, *token),
        _ => return Err(code::BAD_MESSAGE),
    };
    let config_at = config_actor(program, &config_id);
    let token_at = token.map(|t| token_actor(program, &config_id, &t));
    if Some(input.receiver) == token_at {
        // The configuration checked the role.
        if input.from != Some(config_at) {
            return Err(code::NOT_AUTHORIZED);
        }
        let mut token = token_state(state)?;
        return match message {
            BridgeMessage::Freeze { mints, burns, until_ms, record, .. } => {
                Ok(Scope::Token(token)
                    .ask_time_at(PendingTime { record, then: Timed::Freeze { mints, burns, until_ms } }))
            }
            BridgeMessage::Pause { mints, burns, record, .. } => {
                Ok(Scope::Token(token).ask_time_at(PendingTime { record, then: Timed::Pause { mints, burns } }))
            }
            BridgeMessage::SetCap { mints, cap, .. } => {
                let floor = token.controls.floor;
                set_cap(&mut token.gate, mints, cap, &floor)?;
                Ok(set(BridgeState::Token(token)))
            }
            _ => Err(code::BAD_MESSAGE),
        };
    }
    if input.receiver != config_at {
        // The role's own actor: only a call it signed, at the top level or as a private root.
        if input.from.is_some() || !input.is_authorized {
            return Err(code::NOT_AUTHORIZED);
        }
        return Ok(Response::keep_state().send_sync(config_at, &message));
    }
    let (config, gate, sunset, mut roles) = ready_config(state)?;
    let from = from_account(input, program);
    let admin = config.controls.admin;
    let by_admin = from == Some(admin);
    let by_authority = from.is_some_and(|key| roles.freeze_authority.single(&key));
    let by_watcher = from.is_some_and(|key| roles.watchers.contains(&key));
    let forward = |message: &BridgeMessage| -> Result<Response, Refusal> {
        let to = token_at.ok_or(code::BAD_STATE)?;
        Ok(Response::keep_state().send_sync(to, message))
    };
    match message {
        BridgeMessage::Sunset { .. } => {
            // The freeze authority's key, through its own actor; or anyone, by a top-level call
            // here, once the vault is attested retired or the multisig approved it.
            let keyless = input.from.is_none() && roles.sunset_open();
            if !by_authority && !keyless {
                return Err(code::NOT_AUTHORIZED);
            }
            if sunset {
                return Err(code::ALREADY_WRITTEN);
            }
            Ok(set(ready(config, None, gate, true, roles)))
        }
        BridgeMessage::Freeze { mints, burns, until_ms, ref record, .. } => {
            if !by_authority {
                return Err(code::NOT_AUTHORIZED);
            }
            if token.is_some() {
                return forward(&message);
            }
            let pending = PendingTime { record: record.clone(), then: Timed::Freeze { mints, burns, until_ms } };
            Ok(Scope::Config(config, sunset, roles).ask_time(gate, pending))
        }
        BridgeMessage::Pause { mints, burns, ref record, .. } => {
            if !by_watcher {
                return Err(code::NOT_AUTHORIZED);
            }
            if token.is_some() {
                return forward(&message);
            }
            let pending = PendingTime { record: record.clone(), then: Timed::Pause { mints, burns } };
            Ok(Scope::Config(config, sunset, roles).ask_time(gate, pending))
        }
        BridgeMessage::SetCap { mints, cap, .. } => {
            if !by_admin {
                return Err(code::NOT_AUTHORIZED);
            }
            if token.is_some() {
                return forward(&message);
            }
            let mut gate = gate;
            set_cap(&mut gate, mints, cap, &config.controls.floor)?;
            Ok(set(ready(config, None, gate, sunset, roles)))
        }
        BridgeMessage::ProposeRoles { freeze_authority, watchers, record, .. } => {
            if !by_admin {
                return Err(code::NOT_AUTHORIZED);
            }
            freeze_authority.check(&admin)?;
            let repeated = watchers.iter().enumerate().any(|(i, w)| watchers[..i].contains(w));
            if watchers.len() > MAX_WATCHERS || repeated {
                return Err(code::BAD_MESSAGE);
            }
            let pending = PendingTime { record, then: Timed::Propose { freeze_authority, watchers } };
            Ok(Scope::Config(config, sunset, roles).ask_time(gate, pending))
        }
        BridgeMessage::CancelRoles { .. } => {
            if !by_admin {
                return Err(code::NOT_AUTHORIZED);
            }
            roles.proposal.take().ok_or(code::NOT_FOUND)?;
            Ok(set(ready(config, None, gate, sunset, roles)))
        }
        _ => Err(code::BAD_MESSAGE),
    }
}

/// Anyone, at `config(cid)`: attest the vault's `retired` flag; the reply opens `Sunset` to anyone.
fn attest_retired(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    record_slot: u64,
    proof: &VaultProof,
) -> Result<Response, Refusal> {
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    let (config, gate, sunset, roles) = ready_config(state)?;
    if roles.vault_retired {
        return Err(code::ALREADY_WRITTEN);
    }
    let header = header_actor(config.attestation, &config.instance_id, record_slot);
    let request = vault_request(&config, &[SLOT_RETIRED], proof)?;
    Ok(set(ready(config, Some(Pending::Retired { record_slot }), gate, sunset, roles))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// Anyone, at `config(cid)`: read `record`, then apply the pending role proposal if its 14 days
/// of attested time have passed.
fn apply_roles(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    record: TimeRecord,
) -> Result<Response, Refusal> {
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_SENDER);
    }
    let (config, gate, sunset, roles) = ready_config(state)?;
    if roles.proposal.is_none() {
        return Err(code::NOT_FOUND);
    }
    Ok(Scope::Config(config, sunset, roles).ask_time(gate, PendingTime { record, then: Timed::Apply }))
}

/// A multisig freeze authority's message: at each signer's own actor in turn (each checks that its
/// own account signed this transaction and that the previous signer's actor forwarded it), then
/// at `config(cid)`, which checks that the signers are distinct members, at least the threshold.
fn cosigned(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    signers: Vec<Root>,
    action: CosignedAction,
) -> Result<Response, Refusal> {
    let config_at = config_actor(program, &config_id);
    if input.receiver != config_at {
        let me = *input.receiver.account_id.value();
        let at = signers.iter().position(|s| *s == me).ok_or(code::NOT_AUTHORIZED)?;
        let previous = at.checked_sub(1).map(|i| own_actor(program, AccountId::new(signers[i])));
        if !input.is_authorized || input.from != previous {
            return Err(code::NOT_AUTHORIZED);
        }
        let next = signers.get(at + 1).map_or(config_at, |s| own_actor(program, AccountId::new(*s)));
        return Ok(Response::keep_state()
            .send_sync(next, &BridgeMessage::Cosigned { config_id, signers: signers.clone(), action }));
    }
    let last = signers.last().ok_or(code::NOT_AUTHORIZED)?;
    if input.from != Some(own_actor(program, AccountId::new(*last))) {
        return Err(code::NOT_AUTHORIZED);
    }
    let (config, gate, sunset, mut roles) = ready_config(state)?;
    if !roles.freeze_authority.approves(&signers) {
        return Err(code::NOT_AUTHORIZED);
    }
    match action {
        // Not a visible `Sunset` (the checkpoint decodes only its own 33 bytes at the root): an
        // approval, after which anyone sends that `Sunset` at `config(cid)`.
        CosignedAction::Sunset => {
            if sunset || roles.sunset_approved {
                return Err(code::ALREADY_WRITTEN);
            }
            roles.sunset_approved = true;
            Ok(set(ready(config, None, gate, sunset, roles)))
        }
        CosignedAction::Freeze { token: None, mints, burns, until_ms, record } => {
            let pending = PendingTime { record, then: Timed::Freeze { mints, burns, until_ms } };
            Ok(Scope::Config(config, sunset, roles).ask_time(gate, pending))
        }
        CosignedAction::Freeze { token: Some(token), mints, burns, until_ms, record } => {
            let freeze = BridgeMessage::Freeze { config_id, token: Some(token), mints, burns, until_ms, record };
            Ok(Response::keep_state().send_sync(token_actor(program, &config_id, &token), &freeze))
        }
    }
}

/// A gate's actor state, with what it keeps besides its gate.
enum Scope {
    Config(BridgeConfig, bool, Roles),
    Token(TokenState),
}

/// The target's gate and scope, between transactions.
fn load_scope(token: &Option<Address>, state: Option<BridgeState>) -> Result<(Gate, Scope), Refusal> {
    match token {
        None => {
            let (config, gate, sunset, roles) = ready_config(state)?;
            Ok((gate, Scope::Config(config, sunset, roles)))
        }
        Some(_) => {
            let token = token_state(state)?;
            Ok((token.gate, Scope::Token(token)))
        }
    }
}

impl Scope {
    /// Wait for the record `pending` names: `Read` it from the configuration's attestation
    /// program (the reply is accepted only from that record's actor, within this transaction).
    fn ask_time(self, gate: Gate, pending: PendingTime) -> Response {
        let slot = pending.record.record_slot;
        let (state, record) = match self {
            Scope::Config(config, sunset, roles) => {
                let record = header_actor(config.attestation, &config.instance_id, slot);
                (ready(config, Some(Pending::Time(pending)), gate, sunset, roles), record)
            }
            Scope::Token(token) => {
                let record = header_actor(token.attestation, &token.instance_id, slot);
                (BridgeState::Token(TokenState { gate, pending: Some(pending), ..token }), record)
            }
        };
        set(state).send_sync(record, &AttestMessage::Read)
    }

    /// [`Scope::ask_time`] with the gate the scope holds (a token's).
    fn ask_time_at(self, pending: PendingTime) -> Response {
        let gate = match &self {
            Scope::Config(..) => unreachable!("a configuration's gate is passed explicitly"),
            Scope::Token(token) => token.gate,
        };
        self.ask_time(gate, pending)
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
    let at = match &token {
        None => config_actor(program, &config_id),
        Some(token) => token_actor(program, &config_id, token),
    };
    if input.receiver != at {
        return Err(code::WRONG_SENDER);
    }
    let (gate, scope) = load_scope(&token, state)?;
    Ok(scope.ask_time(gate, PendingTime { record, then: Timed::Tick }))
}

/// The record's reply to a gate's `Read`: its attested time ([`record_time`]); the clock advanced
/// to it (never back), then what the gate was waiting to do: a freeze, a watcher's pause, or (at
/// `config(cid)`, whose `roles` are given) a role proposal or its application.
fn apply_time(
    input: &ReceiveInput,
    instance_id: &Root,
    evm_chain_id: u64,
    pending: &PendingTime,
    gate: &mut Gate,
    roles: Option<&mut Roles>,
) -> Result<u64, Refusal> {
    let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    let t = record_time(instance_id, evm_chain_id, &reply.state, pending.record.header.as_deref())?;
    gate.advance(t);
    match &pending.then {
        Timed::Tick => {}
        Timed::Freeze { mints, burns, until_ms } => gate.freeze(*mints, *burns, *until_ms)?,
        Timed::Pause { mints, burns } => gate.pause(*mints, *burns)?,
        Timed::Propose { freeze_authority, watchers } => {
            let roles = roles.ok_or(code::BAD_STATE)?;
            let effective_ms = gate.now_ms.checked_add(ROLE_TIMELOCK_MS).ok_or(code::BAD_STATE)?;
            roles.proposal = Some(RoleProposal {
                freeze_authority: freeze_authority.clone(),
                watchers: watchers.clone(),
                effective_ms,
            });
        }
        Timed::Apply => {
            let roles = roles.ok_or(code::BAD_STATE)?;
            let proposal = roles.proposal.take().ok_or(code::NOT_FOUND)?;
            if gate.now_ms < proposal.effective_ms {
                return Err(code::FROZEN);
            }
            roles.freeze_authority = proposal.freeze_authority;
            roles.watchers = proposal.watchers;
        }
    }
    Ok(t)
}

/// At `token(cid, token)`: the reply of the record it asked for.
fn token_time_reply(input: &ReceiveInput, token: &TokenState) -> Result<Response, Refusal> {
    let mut token = token.clone();
    let pending = token.pending.take().ok_or(code::BAD_STATE)?;
    let t = apply_time(input, &token.instance_id, token.evm_chain_id, &pending, &mut token.gate, None)?;
    Ok(not_before(set(BridgeState::Token(token)), t))
}
