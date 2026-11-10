//! The Ferrata bridge program on LEZ v0.4 (`ferrata_lez_bridge_wire`; the plan is `lez/BRIDGE.md`).
//!
//! * **Configuration.** `Init` at `config(cid)` attests the vault's slot 2 through the attestation
//!   program's direct path and writes the configuration only if it holds `cid`.
//! * **Root import.** `ImportRoot` at `config(cid)` attests the vault's slots 0 and 1 under one
//!   record, inside its own transaction, and writes `root(cid, n)` once.
//! * **Claim.** `Claim` at `root(cid, n)` (plaintext) or `ClaimPrivately` at the claimant's own
//!   actor (meant to run privately; it reads the root and verifies inside its own execution)
//!   verify the deposit with the bridge core and send `Spend` to `nullifier(chain, vault, N)`,
//!   which spends `N` once and mints one denomination to a public destination; a private
//!   destination credits itself.
//! * **Burn.** `Burn` at a holder's actor, as a top-level call or a private root only, destroys
//!   one denomination and records the note once at `burn(cid, H(note))`.
//!
//! A reply is accepted only from the actor this one waits for. Refusals exit with a code
//! (`ferrata_lez_bridge_wire::code`, 240–255); a refusal of the attestation program ends the whole
//! transaction with that program's code, so nothing is written without an attestation.

use ferrata_lez_bridge_wire::{
    burn_actor, burn_seed, check_import, DestKind, check_pairing, code, config_actor, import_request, nullifier_actor,
    nullifier_seed, pairing_request, root_actor, root_seed, verify_claim, BridgeConfig, BridgeMessage, BridgeState,
    BurnRecord, ClaimWitness, ConfigState, HolderState, ImportProof, Note, NullifierState, PairingProof,
    PendingClaim, PendingImport, Root, RootState, Terms,
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
            BridgeState::Holder(HolderState { pending: Some(pending), .. })
                if from == root_actor(program, &pending.config_id, pending.leaf_count) =>
            {
                return claim_reply(input, program, waiting);
            }
            _ => {}
        }
    }
    match borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)? {
        BridgeMessage::Burn { config_id, r, memo, mac, token, amount, c_out } => {
            burn(input, program, state, config_id, Note { r, memo, mac, token, amount, c_out })
        }
        BridgeMessage::ImportRoot { config_id, record_slot, root, leaf_count, proof } => {
            import_root(input, program, state, config_id, PendingImport { record_slot, root, leaf_count }, &proof)
        }
        BridgeMessage::Sunset { .. } => Err(code::BAD_MESSAGE),
        BridgeMessage::Echo { .. } => Ok(Response::keep_state()),
        BridgeMessage::Init { config, record_slot, proof } => init(input, program, state, config, record_slot, &proof),
        BridgeMessage::Claim { config_id, leaf_count, witness } => {
            claim(input, program, state, config_id, leaf_count, witness)
        }
        BridgeMessage::ClaimPrivately { config_id, leaf_count, witness } => {
            claim_privately(input, program, state, config_id, leaf_count, witness)
        }
        BridgeMessage::Read => {
            let reader = input.from.ok_or(code::WRONG_SENDER)?;
            Ok(Response::keep_state().send_sync(reader, &StateReply::from(input)))
        }
        BridgeMessage::WriteRoot { root } => write_root(input, program, state, root),
        BridgeMessage::Spend { nullifier, config_id, terms, leaf_count, amount, mint_to } => {
            spend(input, program, state, nullifier, config_id, terms, leaf_count, amount, mint_to)
        }
        BridgeMessage::Mint { nullifier, config_id, terms, amount } => {
            mint(input, program, state, nullifier, config_id, terms, amount)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// config(cid): Init and ImportRoot
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
    let config_id = config.id(program);
    if input.receiver != config_actor(program, &config_id) {
        return Err(code::WRONG_RECEIVER);
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
        return Err(code::WRONG_RECEIVER);
    }
    let config = match state {
        None => return Err(code::NOT_FOUND),
        Some(BridgeState::Config(ConfigState::Ready { config, pending: None })) => config,
        Some(_) => return Err(code::BAD_STATE),
    };
    let header = header_actor(config.attestation, &config.instance_id, pending.record_slot);
    let request = import_request(&config, proof);
    Ok(set(BridgeState::Config(ConfigState::Ready { config, pending: Some(pending) }))
        .send_sync(header, &AttestMessage::Attest { request }))
}

/// The attestation program's reply to `Init` or `ImportRoot`.
fn config_reply(input: &ReceiveInput, program: AccountId, state: &ConfigState) -> Result<Response, Refusal> {
    let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    let config = state.config().clone();
    let config_id = config.id(program);
    match state {
        ConfigState::Pairing { record_slot, .. } => {
            check_pairing(&config, &config_id, *record_slot, &attestation)?;
            Ok(set(BridgeState::Config(ConfigState::Ready { config, pending: None })))
        }
        ConfigState::Ready { pending: Some(pending), .. } => {
            let root = check_import(&config, &config_id, pending, &attestation)?;
            let to = root_actor(program, &config_id, root.leaf_count);
            let seed = root_seed(&config_id, root.leaf_count);
            Ok(set(BridgeState::Config(ConfigState::Ready { config, pending: None }))
                .send(SyncMessage::new(to, &BridgeMessage::WriteRoot { root }).with_pda_seeds([seed])))
        }
        ConfigState::Ready { pending: None, .. } => Err(code::BAD_STATE),
    }
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
        return Err(code::WRONG_RECEIVER);
    }
    if state.is_some() {
        return Err(code::ALREADY_WRITTEN);
    }
    Ok(set(BridgeState::Root(root)))
}

// ---------------------------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------------------------

/// The `Spend` of a verified claim, to `nullifier(chain, vault, N)`, which it admits by seed.
fn spend_message(program: AccountId, root: &RootState, nullifier: Root, amount: u128, mint_to: Option<Root>) -> SyncMessage {
    let terms = root.terms;
    let to = nullifier_actor(program, terms.evm_chain_id, &terms.vault, &nullifier);
    let seed = nullifier_seed(terms.evm_chain_id, &terms.vault, &nullifier);
    let message = BridgeMessage::Spend {
        nullifier,
        config_id: root.config_id,
        terms,
        leaf_count: root.leaf_count,
        amount,
        mint_to,
    };
    SyncMessage::new(to, &message).with_pda_seeds([seed])
}

/// At `root(cid, n)`: the plaintext claim. The whole witness is in the transaction; the
/// destination must be a public account.
fn claim(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    leaf_count: u64,
    witness: ClaimWitness,
) -> Result<Response, Refusal> {
    if input.receiver != root_actor(program, &config_id, leaf_count) {
        return Err(code::WRONG_RECEIVER);
    }
    let root = match state {
        None => return Err(code::NOT_FOUND),
        Some(BridgeState::Root(root)) => root,
        Some(_) => return Err(code::BAD_STATE),
    };
    let claimed = verify_claim(&root, &witness)?;
    if claimed.kind != DestKind::Public {
        return Err(code::WRONG_DESTINATION);
    }
    Ok(Response::keep_state().send(spend_message(program, &root, claimed.nullifier, claimed.amount, Some(witness.dest))))
}

/// At the claimant's own actor: keep the witness in this (private) state and read the root.
fn claim_privately(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    leaf_count: u64,
    witness: ClaimWitness,
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
    let root = root_actor(program, &config_id, leaf_count);
    let holder = HolderState { pending: Some(PendingClaim { config_id, leaf_count, witness }), ..holder };
    Ok(set(BridgeState::Holder(holder)).send_sync(root, &BridgeMessage::Read))
}

/// The root's reply to a private claim: verify here, spend `N`, and credit this account if it is
/// the destination (a private-kind deposit), or have the nullifier mint to a public one.
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
    let claimed = verify_claim(&root, &pending.witness)?;
    let mut holder = HolderState { holdings: holdings.clone(), pending: None };
    let mint_to = match claimed.kind {
        // Executed as the destination account itself: the mint is this private state.
        DestKind::Private => {
            if input.receiver.account_id.value() != &pending.witness.dest {
                return Err(code::WRONG_DESTINATION);
            }
            holder.credit(&root.config_id, &root.terms, claimed.amount).ok_or(code::BAD_STATE)?;
            None
        }
        DestKind::Public => Some(pending.witness.dest),
    };
    Ok(set(BridgeState::Holder(holder)).send(spend_message(program, &root, claimed.nullifier, claimed.amount, mint_to)))
}

/// At `nullifier(chain, vault, N)`, from an actor of this program after a verified claim: spend
/// `N` once; mint to a public destination.
#[allow(clippy::too_many_arguments)]
fn spend(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    nullifier: Root,
    config_id: Root,
    terms: Terms,
    leaf_count: u64,
    amount: u128,
    mint_to: Option<Root>,
) -> Result<Response, Refusal> {
    if !input.from_own_program() {
        return Err(code::WRONG_SENDER);
    }
    if input.receiver != nullifier_actor(program, terms.evm_chain_id, &terms.vault, &nullifier) {
        return Err(code::WRONG_RECEIVER);
    }
    if state.is_some() {
        return Err(code::ALREADY_CLAIMED);
    }
    let response = set(BridgeState::Nullifier(NullifierState { config_id, leaf_count, amount }));
    Ok(match mint_to {
        Some(dest) => {
            let holder = Actor::new(AccountId::new(dest), program);
            response.send_sync(holder, &BridgeMessage::Mint { nullifier, config_id, terms, amount })
        }
        None => response,
    })
}

/// At a holder's actor, from `nullifier(chain, vault, N)` only: credit one denomination.
fn mint(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    nullifier: Root,
    config_id: Root,
    terms: Terms,
    amount: u128,
) -> Result<Response, Refusal> {
    if input.from != Some(nullifier_actor(program, terms.evm_chain_id, &terms.vault, &nullifier)) {
        return Err(code::WRONG_SENDER);
    }
    let mut holder = match state {
        None => HolderState::default(),
        Some(BridgeState::Holder(holder)) => holder,
        Some(_) => return Err(code::BAD_STATE),
    };
    holder.credit(&config_id, &terms, amount).ok_or(code::BAD_STATE)?;
    Ok(set(BridgeState::Holder(holder)))
}

// ---------------------------------------------------------------------------------------------
// Burn
// ---------------------------------------------------------------------------------------------

/// `Burn` at a holder's actor (debit, then publish), or at `burn(cid, H(note))` (record once).
fn burn(
    input: &ReceiveInput,
    program: AccountId,
    state: Option<BridgeState>,
    config_id: Root,
    note: Note,
) -> Result<Response, Refusal> {
    let note_hash = note.hash();
    let record = burn_actor(program, &config_id, &note_hash);
    if input.receiver == record {
        // Only the holder's actor of this program sends here, after the debit. In a private burn
        // this delivery is the boundary message the checkpoint reads.
        if !input.from_own_program() {
            return Err(code::WRONG_SENDER);
        }
        if state.is_some() {
            return Err(code::BURN_EXISTS);
        }
        return Ok(set(BridgeState::Burn(BurnRecord { config_id, note })));
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
    let index = holder.holdings.iter().position(|h| h.config_id == config_id).ok_or(code::INSUFFICIENT_BALANCE)?;
    let holding = holder.holdings[index];
    if note.token != holding.terms.token {
        return Err(code::NOT_REGISTERED);
    }
    let amount = holding.terms.denomination(&note.amount).ok_or(code::NOT_REGISTERED)?;
    let left = holding.amount.checked_sub(amount).ok_or(code::INSUFFICIENT_BALANCE)?;
    if left == 0 {
        holder.holdings.remove(index);
    } else {
        holder.holdings[index].amount = left;
    }
    let publish = SyncMessage::new(record, &BridgeMessage::burn(config_id, &note))
        .with_pda_seeds([burn_seed(&config_id, &note_hash)]);
    Ok(set(BridgeState::Holder(holder)).send(publish))
}
