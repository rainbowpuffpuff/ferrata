//! The official light-client vectors, run with every BLS backend and composition.
//!
//! These tests fail (they never skip) when the vectors are missing, and they pin the number of
//! cases, so a moved or empty directory cannot pass vacuously.

use plumbline_core::bls::{Aggregation, BlsBackend, Blst, Zkcrypto};
use plumbline_spectests::{cases, run_sync_case, run_update_ranking_case, SyncOutcome};

/// `sync` cases in v1.7.0-beta.4 (minimal preset): altair 8, bellatrix 11, capella 10,
/// deneb 8, electra 5, fulu 6, gloas 4.
const EXPECTED_SYNC_CASES: usize = 52;
/// One `update_ranking` case per fork from Altair to Gloas.
const EXPECTED_RANKING_CASES: usize = 7;

fn run_all_sync(bls: &dyn BlsBackend) -> Vec<(String, SyncOutcome)> {
    let all = cases("sync").unwrap();
    assert_eq!(all.len(), EXPECTED_SYNC_CASES, "unexpected number of sync cases");
    let mut failures = Vec::new();
    let mut results = Vec::new();
    for (fork, dir) in &all {
        let name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
        match run_sync_case(dir, bls) {
            Ok(outcome) => results.push((name, outcome)),
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    assert!(failures.is_empty(), "{} sync case(s) failed:\n{}", failures.len(), failures.join("\n"));
    results
}

#[test]
fn sync_vectors_blst_auto() {
    let results = run_all_sync(&Blst::default());
    let total = |f: fn(&SyncOutcome) -> usize| results.iter().map(|(_, o)| f(o)).sum::<usize>();
    println!(
        "sync: {} cases, {} steps ({} process_update, {} force_update, {} upgrade_store, {} older-fork updates upgraded)",
        results.len(),
        total(|o| o.steps),
        total(|o| o.process_updates),
        total(|o| o.force_updates),
        total(|o| o.upgrades),
        total(|o| o.data_upgrades),
    );
    println!(
        "policy store: {} steps compared equal to the spec store, {} updates applied, {} cases stopped at a forced update",
        total(|o| o.policy_steps_compared),
        total(|o| o.policy_applied),
        results.iter().filter(|(_, o)| o.forced_divergence).count(),
    );
    for (name, o) in &results {
        println!(
            "  {name:<55} steps {:>3}  policy compared {:>3}  applied {:>2}  refused {:?}{}",
            o.steps,
            o.policy_steps_compared,
            o.policy_applied,
            o.policy_refused,
            if o.forced_divergence { "  (stopped: forced update)" } else { "" }
        );
    }
}

#[test]
fn sync_vectors_blst_additive() {
    run_all_sync(&Blst::new(Aggregation::Additive));
}

#[test]
fn sync_vectors_blst_subtractive() {
    run_all_sync(&Blst::new(Aggregation::Subtractive));
}

#[test]
fn sync_vectors_zkcrypto_auto() {
    run_all_sync(&Zkcrypto::default());
}

#[test]
fn update_ranking_vectors() {
    let all = cases("update_ranking").unwrap();
    assert_eq!(all.len(), EXPECTED_RANKING_CASES, "unexpected number of update_ranking cases");
    let mut ranked = 0;
    for (fork, dir) in &all {
        ranked += run_update_ranking_case(dir, *fork).unwrap_or_else(|e| panic!("{}: {e}", fork.name()));
    }
    println!("update_ranking: {} cases, {ranked} updates in canonical order", all.len());
}
