//! The release proof. Private input: a 1280-byte `ReleaseInput` (burn root, witness, public
//! fields). It runs `verify_release` and commits exactly the vault's 352-byte release journal;
//! an invalid witness panics, so no receipt exists for it.
#![no_main]

use ferrata_bridge_core::encoding::{ReleaseInput, RELEASE_INPUT_LEN};
use ferrata_bridge_core::outbound::{release_journal, verify_release};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let mut bytes = [0u8; RELEASE_INPUT_LEN];
    env::read_slice(&mut bytes);
    let input = ReleaseInput::decode(&bytes);
    let nullifier = match verify_release(&input.burn_root, &input.witness, &input.public) {
        Ok(nullifier) => nullifier,
        Err(e) => panic!("release witness rejected: {e:?}"),
    };
    env::commit_slice(&release_journal(&input.burn_root, &nullifier, &input.public));
}
