//! An illustration of the nullifier trap for private consumers (proposal §5). It is not
//! evidence about Plumbline's code: no `plumbline-core` function runs here.
//!
//! A statement identifier is a deterministic function of public Ethereum data. If a private
//! consumer publishes it, or any deterministic function of it, as its nullifier, anyone can
//! recompute it for every public deposit and link each private mint to its deposit. The pattern
//! binds the nullifier to a secret committed in the deposit event: `N = H("nf" ‖ s ‖ fid)`.
//!
//! Limits of the illustration: every deposit here has a different amount, so if mints revealed
//! amounts an observer could link them by amount anyway (RFP-022 expects fixed denominations);
//! and "unlinkable" is checked only against a few functions an observer might guess. The
//! identifier layout stands in for the one M2 specifies.

use std::collections::HashSet;

use sha2::{Digest, Sha256};

type Hash = [u8; 32];

fn h(parts: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    for p in parts {
        hasher.update(p);
    }
    hasher.finalize().into()
}

/// A deposit as everyone sees it on Ethereum.
struct PublicDeposit {
    block: u64,
    tx_index: u32,
    log_index: u32,
    /// The deposit event's data, including the commitment `C = H(s, …)` to the note secret.
    log_data: Vec<u8>,
}

/// The per-fact identifier (F.6): a public function of the fact.
fn fact_id(chain_id: u64, d: &PublicDeposit) -> Hash {
    h(&[
        b"plumbline/fact/log/v1",
        &chain_id.to_be_bytes(),
        &d.block.to_be_bytes(),
        &d.tx_index.to_be_bytes(),
        &d.log_index.to_be_bytes(),
        &h(&[&d.log_data]),
    ])
}

/// The note secret a user derives from their seed for one deposit.
fn note_secret(seed: &[u8], i: u32) -> Hash {
    h(&[b"note", seed, &i.to_be_bytes()])
}

fn commitment(s: &Hash) -> Hash {
    h(&[b"commit", s])
}

/// The recommended nullifier: unique per deposit, unlinkable without `s`.
fn nullifier(s: &Hash, fid: &Hash) -> Hash {
    h(&[b"nf", s, fid])
}

const CHAIN_ID: u64 = 11_155_111; // Sepolia

/// 100 users each deposit once; each deposit event carries the user's commitment.
fn deposits() -> Vec<(Hash, PublicDeposit)> {
    (0..100u32)
        .map(|i| {
            let s = note_secret(format!("user-{i}").as_bytes(), 0);
            let d = PublicDeposit {
                block: 9_000_000 + u64::from(i / 7),
                tx_index: i % 7,
                log_index: 1,
                log_data: [&commitment(&s)[..], &(1_000u64 + u64::from(i)).to_be_bytes()].concat(),
            };
            (s, d)
        })
        .collect()
}

#[test]
fn naive_nullifiers_link_every_mint_to_its_deposit() {
    let deposits = deposits();
    // Published by a naive private consumer: the identifier, or a hash of it.
    let published: Vec<Hash> = deposits.iter().map(|(_, d)| h(&[&fact_id(CHAIN_ID, d)])).collect();
    // An observer recomputes the same function for every public deposit.
    let linked = deposits.iter().zip(&published).filter(|((_, d), n)| h(&[&fact_id(CHAIN_ID, d)]) == **n).count();
    assert_eq!(linked, deposits.len(), "every private mint is linked to its deposit");
}

#[test]
fn secret_bound_nullifiers_are_unique_and_unlinkable() {
    let deposits = deposits();
    let published: Vec<Hash> = deposits.iter().map(|(s, d)| nullifier(s, &fact_id(CHAIN_ID, d))).collect();

    // Unique per deposit: a replayed claim produces the same nullifier and is refused.
    let unique: HashSet<&Hash> = published.iter().collect();
    assert_eq!(unique.len(), deposits.len());
    let (s0, d0) = &deposits[0];
    assert!(unique.contains(&nullifier(s0, &fact_id(CHAIN_ID, d0))));

    // Unlinkable: everything an observer can compute from public data misses.
    let observer_guesses: HashSet<Hash> = deposits
        .iter()
        .flat_map(|(_, d)| {
            let fid = fact_id(CHAIN_ID, d);
            [fid, h(&[&fid]), nullifier(&[0; 32], &fid), h(&[b"nf", &fid])]
        })
        .collect();
    assert!(published.iter().all(|n| !observer_guesses.contains(n)));

    // Reusing a secret for two deposits still yields distinct nullifiers (bound to `fid`).
    let s = note_secret(b"reused", 0);
    let a = PublicDeposit { block: 1, tx_index: 0, log_index: 0, log_data: commitment(&s).to_vec() };
    let b = PublicDeposit { block: 2, tx_index: 0, log_index: 0, log_data: commitment(&s).to_vec() };
    assert_ne!(nullifier(&s, &fact_id(CHAIN_ID, &a)), nullifier(&s, &fact_id(CHAIN_ID, &b)));
}
