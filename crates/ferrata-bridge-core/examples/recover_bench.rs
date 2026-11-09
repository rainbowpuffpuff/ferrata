//! Wall-clock time of `inbound::recover` over 100,000 synthetic vault deposits, 7 of them the
//! key's own (the same data `tests/inbound.rs` checks). Single thread.
//!
//! `cargo run --release -p ferrata-bridge-core --example recover_bench [runs]`

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Instant;

use common::{addr, dest_for, h, synthetic_deposits};
use ferrata_bridge_core::inbound::recover;
use ferrata_bridge_core::BridgeKey;

fn main() {
    let runs: usize = std::env::args().nth(1).map(|a| a.parse().unwrap()).unwrap_or(7);
    let key = BridgeKey::new(&h("our bridge key"));
    let (chain_id, vault) = (1u64, addr("vault"));
    let (deposits, ours) = synthetic_deposits(&key, chain_id, &vault);

    let mut times = Vec::new();
    for _ in 0..runs {
        let start = Instant::now();
        let found = recover(&key, chain_id, &vault, &deposits, dest_for);
        times.push(start.elapsed().as_secs_f64() * 1e3);
        let at: Vec<usize> = found.iter().map(|o| o.deposit.leaf_index as usize).collect();
        assert_eq!(at, ours, "must find exactly the 7 own deposits");
    }
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "recover over {} deposits ({} own): {} runs, ms each: {:?}",
        deposits.len(),
        ours.len(),
        runs,
        times.iter().map(|t| format!("{t:.1}")).collect::<Vec<_>>()
    );
    println!(
        "min {:.1} ms, median {:.1} ms, max {:.1} ms",
        sorted[0],
        sorted[sorted.len() / 2],
        sorted[sorted.len() - 1]
    );
}
