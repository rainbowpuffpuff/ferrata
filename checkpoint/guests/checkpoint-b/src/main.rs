//! Rule (B) checkpoint guest (SPEC-v3 D7, D13): runs `run_checkpoint_b` and commits the 544-byte
//! journal, laid out as rule (A)'s with `echoNonce` = 0; a vault in mode (B) pins this guest's image
//! id. When the input continues from segments, each segment's journal is verified as a receipt of
//! the segment guest (`env::verify`) before the rule checks that they chain to the parent state.
#![no_main]

use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::rule_b::run_checkpoint_b;
use risc0_zkvm::guest::env;
use risc0_zkvm::sha::Digest;

risc0_zkvm::guest::entry!(main);

/// The segment guest's image id, fixed when this guest is built (methods/build.rs), as eight
/// comma-separated u32 words.
const SEGMENT_ID: &str = env!("FERRATA_SEGMENT_ID");

fn segment_id() -> Digest {
    let mut words = [0u32; 8];
    for (w, s) in words.iter_mut().zip(SEGMENT_ID.split(',')) {
        *w = s.parse().expect("segment image id");
    }
    Digest::new(words)
}

fn read_input() -> Vec<u8> {
    let mut len = [0u8; 4];
    env::read_slice(&mut len);
    let mut bytes = vec![0u8; u32::from_le_bytes(len) as usize];
    env::read_slice(&mut bytes);
    bytes
}

fn main() {
    let bytes = read_input();
    let input = CheckpointInputB::decode(&bytes).unwrap_or_else(|e| panic!("checkpoint input: {e:?}"));
    if !input.segments.is_empty() {
        let id = segment_id();
        for segment in &input.segments {
            env::verify(id, &segment.encode()).expect("segment receipt");
        }
    }
    let outcome = run_checkpoint_b(&input).unwrap_or_else(|e| panic!("checkpoint refused: {e:?}"));
    env::commit_slice(&outcome.journal.encode());
}
