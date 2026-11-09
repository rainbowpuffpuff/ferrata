//! Guest-side cycle measurements of ferrata-core's execution layer on real mainnet data
//! (`crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09`).
//!
//! The host passes a mode, the record of the finalized block (its SSZ bytes, as a consumer reads
//! them from the program's record account), the proofs and the expected results, which it
//! computed natively. Every call is timed with `env::cycle_count()` deltas (user cycles) and
//! every result is asserted, so no number comes from a run that computed a wrong answer.
//!
//! With RISC Zero's accelerated Keccak, a Keccak-f permutation inside the timed call costs eight
//! accelerated SHA-256 compressions and a host call; the permutations are proven in a separate
//! Keccak circuit, flushed when the execution ends (counted in the execution's total, not in the
//! deltas), and resolved as an assumption when the receipt is composed. The host reports the
//! permutation count of each mode.

use ferrata_core::{
    attest::{attest_log, attest_storage, history_hop, storage_id, HeaderRecord},
    keccak::keccak256,
    Root,
};
use risc0_zkvm::guest::env;

pub const MODE_STORAGE: u32 = 0;
pub const MODE_LOG: u32 = 1;
pub const MODE_HOP: u32 = 2;
pub const MODE_HOP_STORAGE: u32 = 3;
pub const MODE_PRIMITIVES: u32 = 4;

const CHAIN_ID: u64 = 1;

/// A length-prefixed byte blob (the host pads it to a multiple of 4 bytes).
fn blob() -> Vec<u8> {
    let len: u32 = env::read();
    let mut buf = vec![0u8; (len as usize + 3) & !3];
    env::read_slice(&mut buf);
    buf.truncate(len as usize);
    buf
}

fn array<const N: usize>() -> [u8; N] {
    blob().try_into().expect("array length")
}

/// A proof: a count, then each node as a blob.
fn nodes() -> Vec<Vec<u8>> {
    let n: u32 = env::read();
    (0..n).map(|_| blob()).collect()
}

/// Run `f` and record its user cycles under `label`.
fn time<T>(rec: &mut Vec<(String, u64)>, label: &str, f: impl FnOnce() -> T) -> T {
    let start = env::cycle_count();
    let out = std::hint::black_box(f());
    let end = env::cycle_count();
    rec.push((label.to_string(), end - start));
    out
}

/// Guest entry point shared by both configurations.
pub fn run() {
    let mode: u32 = env::read();
    let mut rec = Vec::new();
    let record = time(&mut rec, "HeaderRecord::from_ssz", || HeaderRecord::from_ssz(&blob()).unwrap());
    match mode {
        MODE_STORAGE => {
            let (address, slot, account_proof, storage_proof) = (array::<20>(), array::<32>(), nodes(), nodes());
            let (value, id) = (array::<32>(), array::<32>());
            let a = time(&mut rec, "attest_storage", || {
                attest_storage(&record, CHAIN_ID, &address, &account_proof, &slot, &storage_proof).unwrap()
            });
            assert_eq!((a.fact.value, a.id), (value, id));
        }
        MODE_LOG => {
            let tx_index: u64 = env::read();
            let log_index: u64 = env::read();
            let proof = nodes();
            let (emitter, id) = (array::<20>(), array::<32>());
            let a = time(&mut rec, "attest_log", || attest_log(&record, CHAIN_ID, tx_index, log_index, &proof).unwrap());
            assert_eq!((a.fact.log.address, a.id), (emitter, id));
        }
        MODE_HOP | MODE_HOP_STORAGE => {
            let (account_proof, storage_proof, header) = (nodes(), nodes(), blob());
            let number: u64 = env::read();
            let older =
                time(&mut rec, "history_hop", || history_hop(&record, &account_proof, &storage_proof, &header).unwrap());
            assert_eq!(older.number(), number);
            if mode == MODE_HOP_STORAGE {
                let (address, slot, account_proof, storage_proof) = (array::<20>(), array::<32>(), nodes(), nodes());
                let (value, id) = (array::<32>(), array::<32>());
                let a = time(&mut rec, "attest_storage under the derived record", || {
                    attest_storage(&older, CHAIN_ID, &address, &account_proof, &slot, &storage_proof).unwrap()
                });
                assert_eq!((a.fact.value, a.id), (value, id));
            }
        }
        MODE_PRIMITIVES => {
            let node = blob();
            let want: Root = array::<32>();
            let got = time(&mut rec, "keccak256 of one proof node", || keccak256(&node));
            assert_eq!(got, want);
            time(&mut rec, "storage_id (SHA-256 identifier)", || storage_id(CHAIN_ID, &[1; 20], &[2; 32], &[3; 32]));
        }
        _ => panic!("unknown mode {mode}"),
    }
    env::commit(&rec);
}
