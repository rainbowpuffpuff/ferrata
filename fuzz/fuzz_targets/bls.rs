//! Differential fuzzing of the BLS layer (`ferrata_core::bls`): blst, the library the programs use,
//! against zkcrypto/bls12_381 on attacker-chosen committees, participation bits, signing roots and
//! signatures, on every verification path and every key aggregation. The input layout, the edits
//! and the properties are in `src/bls.rs`; a disagreement panics with the path and both verdicts.
#![no_main]
#![deny(unsafe_code)]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    ferrata_fuzz::bls::run(data);
});
