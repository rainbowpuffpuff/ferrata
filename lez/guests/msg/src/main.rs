//! `ferrata_msg`: encrypted wallet-to-wallet messages (prototype, unaudited). The whole logic is
//! `ferrata_lez_msg_wire::handle`; this maps LEZ's input and response onto it.

use ferrata_lez_msg_wire::{handle, Input};
use lee_core::{
    account::{AccountId, Actor},
    program::{read_input_frame, ProgramEvent, ReceiveInput, Response, SyncMessage},
};
use risc0_zkvm::guest::env;

fn main() {
    let input: ReceiveInput = borsh::from_slice(&read_input_frame()).expect("the runtime's input decodes");
    let program = input.receiver.program_account_id;
    let outcome = handle(&Input {
        receiver: *input.receiver.account_id.value(),
        program: *program.value(),
        from: input.from.map(|a| (*a.account_id.value(), *a.program_account_id.value())),
        is_authorized: input.is_authorized,
        pre_state: &input.pre_state,
        message: &input.message,
    });
    let out = match outcome {
        Ok(out) => out,
        Err(code) => env::exit(code),
    };
    let mut response = match out.state {
        Some(state) => Response::set_state(state),
        None => Response::keep_state(),
    };
    if let Some((to, message)) = out.deliver {
        let to = Actor::new(AccountId::new(to), program);
        response = response.send(SyncMessage { to, message, pda_seeds: Default::default() });
    }
    for (selector, data) in out.events {
        response = response.event(ProgramEvent { selector, data });
    }
    if let Some((from, to)) = out.timestamp_window {
        response = response.try_timestamp_window(from..to).expect("handle checks from < to");
    }
    response.into_transition(input).commit();
    env::exit(0)
}
