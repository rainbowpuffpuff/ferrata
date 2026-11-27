//! The committed corpus of the `bls` fuzz target (`fuzz/corpus/bls`), replayed through the target's
//! own differential check (`fuzz/src/bls.rs`, included below): for every input, blst and
//! zkcrypto/bls12_381 give the same verdict on every path of `ferrata_core::bls`, under every key
//! aggregation, and blst's split verification agrees with its one-shot verification
//! (`fuzz/RESULTS.md`). Deterministic, without a fuzzer: the fuzz crate is a workspace of its own,
//! so this is how CI keeps the corpus clean.

use std::{collections::BTreeMap, fs, path::Path};

// The seed maker's helpers in the shared file are not used here.
#[allow(dead_code)]
#[path = "../../../fuzz/src/bls.rs"]
mod bls;

#[test]
fn bls_fuzz_corpus_replays_without_a_disagreement() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/corpus/bls");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    files.sort();
    assert!(files.len() >= 50, "{} inputs in {}", files.len(), dir.display());
    let mut spec_path: BTreeMap<String, usize> = BTreeMap::new();
    let mut checked_ok = 0;
    for path in &files {
        let data = fs::read(path).unwrap();
        let v = std::panic::catch_unwind(|| bls::run(&data)).unwrap_or_else(|_| {
            panic!("{} (replay: fuzz/target/x86_64-unknown-linux-gnu/release/bls <file>)", path.display())
        });
        *spec_path.entry(format!("{:?}", v.fast_aggregate_verify)).or_default() += 1;
        if v.checked.is_some_and(|c| c.iter().all(Result::is_ok)) {
            checked_ok += 1;
        }
    }
    println!(
        "bls corpus: {} inputs; fast_aggregate_verify verdicts {spec_path:?}; {checked_ok} accepted by every aggregation",
        files.len()
    );
    // The corpus reaches verify_checked's acceptance under every aggregation (and so the split and Auto checks).
    assert!(checked_ok > 0, "no input accepted by every aggregation of verify_checked");
    // The corpus reaches acceptance and the refusals on the spec path.
    for verdict in ["Ok(())", "Err(PubkeyInvalid)", "Err(SignatureInvalid)", "Err(SignatureMalformed)"] {
        assert!(spec_path.contains_key(verdict), "no input with {verdict}: {spec_path:?}");
    }
}
