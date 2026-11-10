//! The release proof (SPEC-v2 §5.2). Private input: a 1340-byte `ReleaseInput` (burn root, witness
//! `burnIndex`, path, `sOut`, `r`, and the public fields). It runs `verify_release` and commits
//! exactly the vault's 352-byte release journal; an invalid witness panics, so no receipt exists
//! for it. The work done does not depend on the witness (constant-shape path hashing), so the cycle
//! count reveals nothing about it; the host asserts this over random witnesses.
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
