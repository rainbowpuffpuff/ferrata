//! Test vehicle, not a protocol component: runs the inbound claim statement inside the zkVM so the
//! host can check that a privately proven claim yields the same nullifier as a public one.
//! Private input: a 1208-byte `ClaimInput`. Journal: `root ‖ N ‖ token ‖ amount` (116 bytes).
#![no_main]

use ferrata_bridge_core::encoding::{claim_journal, ClaimInput, CLAIM_INPUT_LEN};
use ferrata_bridge_core::inbound::verify_claim;
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let mut bytes = [0u8; CLAIM_INPUT_LEN];
    env::read_slice(&mut bytes);
    let input = ClaimInput::decode(&bytes);
    let nullifier = match verify_claim(&input.root, &input.witness, input.chain_id, &input.vault) {
        Ok(nullifier) => nullifier,
        Err(e) => panic!("claim witness rejected: {e:?}"),
    };
    env::commit_slice(&claim_journal(&input.root, &nullifier, &input.witness.token, &input.witness.amount));
}
