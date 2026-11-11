//! `ferrata_holdings`: "`holder` held at least `minimum` of ERC-20 `token` at a finalized Ethereum
//! block", read through Ferrata's attestation (`ferrata_lez_holdings_wire` has the whole design).
//!
//! * `Check` at `holding(gate, holder)`: build the request (the token's account, the holder's
//!   balance slot, `value ≥ minimum`) and send `Attest` to `header(id, slot)` of the gate's
//!   attestation program.
//! * The reply, accepted only from that actor while this one waits: `Gate::accept`, then keep the
//!   `Holding`. A refusal of the attestation program ends the whole transaction with that
//!   program's exit code, so nothing is written on a statement that was not proven.

use ferrata_lez_holdings_wire::{code, holding_actor, HoldingsMessage, HoldingsState, Pending};
use ferrata_lez_wire::{attest::Attestation, header_actor, Message};
use lee_core::program::{read_input_frame, ReceiveInput, Response};
use risc0_zkvm::guest::env;

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

fn handle(input: &ReceiveInput) -> Result<Response, u8> {
    let program = input.receiver.program_account_id;
    let state: Option<HoldingsState> = if input.pre_state.is_empty() {
        None
    } else {
        Some(borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?)
    };
    // While a check waits, only the reply of the record it asked is accepted.
    if let Some(HoldingsState::Pending(pending)) = &state {
        if input.from != Some(pending.record) {
            return Err(code::WRONG_SENDER);
        }
        let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
        let held = pending.gate.accept(&pending.holder, &attestation)?;
        if pending.previous_block.is_some_and(|previous| held.block_number <= previous) {
            return Err(code::STALE);
        }
        return Ok(Response::set_state(encode(&HoldingsState::Held(held))));
    }
    let HoldingsMessage::Check { gate, slot, holder, account_proof, storage_proof, header } =
        borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    // The actor is the holding the check is about: one per gate and holder.
    if input.receiver != holding_actor(program, &gate.id(), &holder) {
        return Err(code::WRONG_RECEIVER);
    }
    let previous_block = match state {
        None => None,
        Some(HoldingsState::Held(held)) => Some(held.block_number),
        Some(HoldingsState::Pending(_)) => return Err(code::BAD_STATE),
    };
    // The record is addressed from the gate alone: its program, its instance.
    let record = header_actor(gate.ferrata, &gate.instance_id, slot);
    let request = gate.request(&holder, account_proof, storage_proof, header);
    let pending = Pending { gate, holder, record, previous_block };
    Ok(Response::set_state(encode(&HoldingsState::Pending(pending))).send_sync(record, &Message::Attest { request }))
}
