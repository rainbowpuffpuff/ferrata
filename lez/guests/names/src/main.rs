//! `ferrata_names`: human-readable names for LEZ accounts. `ferrata_lez_names_wire` has the whole design and the
//! handler; this is its entry point.

use lee_core::program::{read_input_frame, ReceiveInput};
use risc0_zkvm::guest::env;

fn main() {
    let input: ReceiveInput = borsh::from_slice(&read_input_frame()).expect("the runtime's input decodes");
    match ferrata_lez_names_wire::handle(&input) {
        Ok(response) => {
            response.into_transition(input).commit();
            env::exit(0)
        }
        Err(code) => env::exit(code),
    }
}
