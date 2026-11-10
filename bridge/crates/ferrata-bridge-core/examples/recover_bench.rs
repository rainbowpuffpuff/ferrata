//! Wall-clock time of `inbound::recover` (SPEC-v2: g candidate destinations per deposit, both kinds)
//! over synthetic vault deposits, 8 of them the wallet's own (7 inside the gap-5 window). One thread.
//! Also prints how many bytes of public data the scan reads.
//!
//! `cargo run --release -p ferrata-bridge-core --example recover_bench [runs] [sizes...]`
//! (defaults: 5 runs; 100000 and 1000000 deposits)

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Instant;

use common::{addr, chain, dest_of, keys, synthetic_deposits};
use ferrata_bridge_core::inbound::recover;

const GAP: u32 = 5;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let runs: usize = args.first().map(|a| a.parse().unwrap()).unwrap_or(5);
    let sizes: Vec<u64> =
        if args.len() > 1 { args[1..].iter().map(|a| a.parse().unwrap()).collect() } else { vec![100_000, 1_000_000] };
    let k = keys("our bridge seed");
    let (chain_id, vault) = (chain(1), addr("vault"));
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("loadavg before: {}", load.trim());
    for total in sizes {
        let (deposits, planted) = synthetic_deposits(&k.auth, &chain_id, &vault, total);
        let want: Vec<u64> = planted.iter().filter(|p| p.account < GAP).map(|p| p.leaf_index).collect();
        let mut times = Vec::new();
        for _ in 0..runs {
            let start = Instant::now();
            let found = recover(&k.auth, &chain_id, &vault, &deposits, GAP, dest_of);
            times.push(start.elapsed().as_secs_f64() * 1e3);
            let got: Vec<u64> = found.iter().map(|o| o.deposit.leaf_index).collect();
            assert_eq!(got, want, "must find exactly the own deposits inside the window");
        }
        let mut sorted = times.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Public inputs per deposit, from the vault's Deposit events:
        //   matching needs depositor (20) + j (8) + C (32) = 60 bytes;
        //   building claim paths needs every leaf, i.e. also leafIndex (8) + token (20) + amount (32).
        let (matching, all_fields) = (total * 60, total * 120);
        println!(
            "recover, {total} deposits, g = {GAP} (2g = {} candidate C per deposit), {} own found ({} planted): {runs} runs, ms: {:?}",
            2 * GAP,
            want.len(),
            planted.len(),
            times.iter().map(|t| format!("{t:.1}")).collect::<Vec<_>>()
        );
        println!(
            "  min {:.1} ms, median {:.1} ms, max {:.1} ms; public data read: {} bytes to match (60/deposit), {} bytes with every leaf for paths (120/deposit)",
            sorted[0],
            sorted[sorted.len() / 2],
            sorted[sorted.len() - 1],
            matching,
            all_fields
        );
    }
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("loadavg after: {}", load.trim());
}
