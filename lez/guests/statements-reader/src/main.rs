//! `ferrata_statements_reader`: a consumer of recorded statements (RFP-021 soft requirement 1).
//! `ferrata_lez_statements_wire` has the whole design.
//!
//! * `Use { recorder, key, ferrata, instance_id, chain_id, statement_id, tag }` at `use(config, statement_id)`: send
//!   `Read` to `statement(key)` of `recorder`.
//! * The reply, accepted only from that actor while this one waits: `accept` (the entry's program, key, instance
//!   and chain, and the statement in it), then keep `Used`. A second `Use` of the statement under the same trust
//!   configuration finds `Used` and is refused: the nullifier keyed on the statement identifier.

use ferrata_lez_statements_wire::{
    accept, code, statement_actor, use_actor, use_config, ReaderMessage, ReaderState, Recorded, StatementsMessage,
    Waiting,
};
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
    let state: Option<ReaderState> = if input.pre_state.is_empty() {
        None
    } else {
        Some(borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?)
    };
    match state {
        Some(ReaderState::Waiting(waiting)) => {
            if input.from != Some(waiting.entry) {
                return Err(code::WRONG_SENDER);
            }
            let entry: Recorded = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
            let used = accept(&waiting, &entry)?;
            Ok(Response::set_state(encode(&ReaderState::Used(used))))
        }
        Some(ReaderState::Used(_)) => Err(code::ALREADY_USED),
        None => {
            let ReaderMessage::Use { recorder, key, ferrata, instance_id, chain_id, statement_id, tag } =
                borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
            let config = use_config(&recorder, &ferrata, &instance_id, chain_id, &tag);
            if input.receiver != use_actor(program, &config, &statement_id) {
                return Err(code::WRONG_RECEIVER);
            }
            let entry = statement_actor(recorder, &key);
            let waiting = Waiting { entry, key, ferrata, instance_id, chain_id, statement_id };
            Ok(Response::set_state(encode(&ReaderState::Waiting(waiting))).send_sync(entry, &StatementsMessage::Read))
        }
    }
}
