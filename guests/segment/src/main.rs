//! Segment guest: proves `startState → endState` over a bounded run of LEZ blocks (hashing,
//! linking, extraction; no support check). Its journal is only accepted by the checkpoint guest,
//! which verifies it with `env::verify` (composition).
#![no_main]

use checkpoint_core::input::SegmentInput;
use checkpoint_core::rule::run_segment;
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn read_input() -> Vec<u8> {
    let mut len = [0u8; 4];
    env::read_slice(&mut len);
    let mut bytes = vec![0u8; u32::from_le_bytes(len) as usize];
    env::read_slice(&mut bytes);
    bytes
}

fn main() {
    let bytes = read_input();
    let input = SegmentInput::decode(&bytes).unwrap_or_else(|e| panic!("segment input: {e:?}"));
    let (journal, _) = run_segment(&input).unwrap_or_else(|e| panic!("segment refused: {e:?}"));
    env::commit_slice(&journal.encode());
}
