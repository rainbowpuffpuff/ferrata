//! A minimal consumer of Ferrata's records (Stage 3), meant to run privately: it reads one
//! `header(id, slot)` record with LEZ v0.4's read idiom (`Read` → `StateReply`) and keeps it in its
//! own state, or, for comparison, calls the program with a query.

use ferrata_lez_wire::{
    consumer::{code, ConsumerMessage, ConsumerState},
    header_actor, Message, RecordState,
};
use lee_core::program::{read_input_frame, ReceiveInput, Response, StateReply};
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
    let state: ConsumerState = if input.pre_state.is_empty() {
        ConsumerState::default()
    } else {
        borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?
    };
    // The reply to a read: accepted only from the actor this one asked.
    if input.from.is_some() && input.from == state.awaiting {
        let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
        let record: RecordState = borsh::from_slice(&reply.state).map_err(|_| code::BAD_REPLY)?;
        return Ok(Response::set_state(encode(&ConsumerState {
            awaiting: None,
            record: Some(record.record),
            ..state
        })));
    }
    // Reading sends only `Read`; the query stays in this actor's (private) state. Calling sends
    // the query to the program.
    let (ferrata, instance_id, slot, message, kept) = match borsh::from_slice(&input.message) {
        Ok(ConsumerMessage::ReadRecord { ferrata, instance_id, slot, query }) => {
            (ferrata, instance_id, slot, Message::Read, Some(query))
        }
        Ok(ConsumerMessage::CallAttest { ferrata, instance_id, slot, query }) => {
            (ferrata, instance_id, slot, Message::Attest { query }, None)
        }
        Err(_) => return Err(code::BAD_MESSAGE),
    };
    let header = header_actor(ferrata, &instance_id, slot);
    let state = ConsumerState { awaiting: Some(header), record: state.record, query: kept };
    Ok(Response::set_state(encode(&state)).send_sync(header, &message))
}
