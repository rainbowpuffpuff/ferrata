//! The gas-drop receipt (bridge.md §6, v3 D13): the claim statement over a deposit under an imported
//! root, with a journal that binds its nullifier `N` to a one-time address and one relayer, so that a
//! relayer can send one native credit per `N` and nobody can replay the receipt to another relayer or
//! redirect the credit. Private input: a 1324-byte `GasDropInput`. Journal (212 bytes):
//! `DROP_DOMAIN ‖ root ‖ evmChainId ‖ vault ‖ N ‖ oneTimeAddress ‖ relayer`.
#![no_main]

use ferrata_bridge_core::gasdrop::{verify_gas_drop, GasDropInput, GAS_DROP_INPUT_LEN};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let mut bytes = [0u8; GAS_DROP_INPUT_LEN];
    env::read_slice(&mut bytes);
    let input = GasDropInput::decode(&bytes);
    let journal = match verify_gas_drop(&input) {
        Ok(journal) => journal,
        Err(e) => panic!("gas-drop witness rejected: {e:?}"),
    };
    env::commit_slice(&journal.encode());
}
