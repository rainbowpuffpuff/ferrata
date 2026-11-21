//! `ferrata_statements`: records Ferrata's verified statements to public accounts (RFP-021 soft requirement 1).
//! `ferrata_lez_statements_wire` has the whole design.
//!
//! * `Record { ferrata, slot, request }` at `statement(key)`, `key` from the request without its proofs: send the
//!   attestation program's own `Attest { request }` to `header(ferrata, request.instance_id, slot)`.
//! * The reply, accepted only from that record while this actor waits: kept as it came, once.
//! * `Read` from any actor: reply with the entry; nothing changes.

use ferrata_lez_statements_wire::{
    code, statement_actor, statement_key, Pending, Recorded, StatementsMessage, StatementsState,
};
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
    let state: Option<StatementsState> = if input.pre_state.is_empty() {
        None
    } else {
        Some(borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?)
    };
    match state {
        // While it waits, only the reply of the record it asked is accepted.
        Some(StatementsState::Pending(pending)) => {
            if input.from != Some(pending.record) {
                return Err(code::WRONG_SENDER);
            }
            let attestation: Attestation = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
            let entry = Recorded { ferrata: pending.ferrata, record: pending.record, key: pending.key, attestation };
            Ok(Response::set_state(encode(&StatementsState::Recorded(entry))))
        }
        Some(StatementsState::Recorded(entry)) => match borsh::from_slice(&input.message) {
            Ok(StatementsMessage::Read) => {
                let reader = input.from.ok_or(code::WRONG_SENDER)?;
                Ok(Response::keep_state().send_sync(reader, &entry))
            }
            Ok(StatementsMessage::Record { .. }) => Err(code::ALREADY_RECORDED),
            Err(_) => Err(code::BAD_MESSAGE),
        },
        None => match borsh::from_slice(&input.message) {
            Ok(StatementsMessage::Record { ferrata, slot, request }) => {
                let key = statement_key(&ferrata, slot, &request);
                // The actor is the statement the request asks: one per program, record and question.
                if input.receiver != statement_actor(program, &key) {
                    return Err(code::WRONG_RECEIVER);
                }
                let record = header_actor(ferrata, &request.instance_id, slot);
                let pending = Pending { ferrata, record, key };
                Ok(Response::set_state(encode(&StatementsState::Pending(pending)))
                    .send_sync(record, &Message::Attest { request }))
            }
            Ok(StatementsMessage::Read) => Err(code::NOT_RECORDED),
            Err(_) => Err(code::BAD_MESSAGE),
        },
    }
}
