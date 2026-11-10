//! Measurement guest, not a protocol component: runs the inbound claim statement (SPEC-v2 §3.3:
//! `C` with the kind byte, the deposit leaf binding the depositor, the path with direction bits equal
//! to `leafIndex`, `fid`, `N`) inside the zkVM. It is the part of a user's privacy-preserving claim
//! that is the bridge's own, without LEZ's circuit around it, so its cycle count is a lower bound for
//! user-side claim proving, and its receipt is what a relayer would check before a one-drop-per-
//! nullifier gas drop. Private input: a 1260-byte `ClaimInput`. Journal (168 bytes):
//! `root ‖ evmChainId ‖ vault ‖ N ‖ token ‖ amount`.
#![no_main]

use ferrata_bridge_core::encoding::{claim_journal, ClaimInput, CLAIM_INPUT_LEN};
use ferrata_bridge_core::inbound::verify_claim;
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let mut bytes = [0u8; CLAIM_INPUT_LEN];
    env::read_slice(&mut bytes);
    let input = ClaimInput::decode(&bytes);
    let w = &input.witness;
    let nullifier = match verify_claim(&input.root, w, &input.chain_id, &input.vault) {
        Ok(nullifier) => nullifier,
        Err(e) => panic!("claim witness rejected: {e:?}"),
    };
    env::commit_slice(&claim_journal(&input.root, &input.chain_id, &input.vault, &nullifier, &w.token, &w.amount));
}
