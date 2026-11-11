//! A real proof of `ferrata_holdings`'s accepted check (`tests/holdings.rs`: "the pair held
//! ≥ 1,000 WETH" at mainnet block 26,384,851). The transaction is built and settled on the Stage 4
//! chain, then replayed with each handler's input recorded (`holdings::record_transitions`), and
//! every handler execution of it (`ferrata_holdings` `Check`, the attestation program's `Attest`,
//! `ferrata_holdings`'s reply) is proven by `r0vm` (`RISC0_DEV_MODE` unset or 0), its receipt
//! verified against the program's image id and its journal compared with the executor's.
//!
//! * `prove_holdings`: succinct receipts (composite, then lifted and joined to one per handler).
//! * `--composite`: composite receipts only.
//! * `--allow-dev-mode`: let it run in RISC Zero dev mode (fake receipts in seconds), to try the
//!   command; the output then says it is not a proof.
//!
//! LEZ v0.4 does not prove public transactions itself (a sequencer executes them); these are the
//! proofs of the same executions a prover of the transaction would have to make.

use std::time::Instant;

use harness::{
    attest::u256,
    ferrata_account, grouped,
    holdings::{holdings_account, record_transitions, Holdings},
    image_id_hex, render, render_transitions,
    settle::LeeTransaction,
};
use risc0_zkvm::{default_prover, ExecutorEnv, ProverOpts};

/// One WETH, in wei.
const WETH: u128 = 1_000_000_000_000_000_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let allow_dev = args.iter().any(|a| a == "--allow-dev-mode");
    let composite = args.iter().any(|a| a == "--composite");
    let dev = std::env::var("RISC0_DEV_MODE").is_ok_and(|v| !v.is_empty() && v != "0" && v != "false");
    if dev && !allow_dev {
        eprintln!("RISC0_DEV_MODE is set: the receipts would be fake. Unset it, or pass --allow-dev-mode.");
        std::process::exit(2);
    }
    println!("ferrata_holdings image id: {}", image_id_hex(methods::FERRATA_HOLDINGS_ID));
    println!("ferrata (attestation) image id: {}", image_id_hex(methods::FERRATA_LEZ_ID));
    println!("mode: {}", if dev { "DEV MODE: fake receipts, not a proof" } else { "real proofs (r0vm)" });

    let mut h = Holdings::new();
    let gate = h.gate(u256(1_000 * WETH));
    let slot = h.slot();
    let before = h.s.w.chain.state.clone();
    let report = h.check("accepted: the pair held ≥ 1,000 WETH", &gate, slot);
    assert!(report.ok(), "{:?} (exit {:?})", report.charge.revert, report.exit_code);
    println!("\n{}\n{}", render(std::slice::from_ref(&report)), render_transitions(std::slice::from_ref(&report)));

    let LeeTransaction::Public(tx) = borsh::from_slice::<LeeTransaction>(&h.s.w.chain.last_tx).unwrap() else {
        panic!("a public transaction")
    };
    let recorded = record_transitions(&tx, &before).expect("the replay succeeds");
    assert_eq!(recorded.len(), report.transitions.len(), "the replay runs the same handlers");
    for (r, t) in recorded.iter().zip(&report.transitions) {
        assert_eq!(r.cycles, t.cycles, "the replay runs the same handlers on the same inputs");
    }

    let opts = if composite { ProverOpts::composite() } else { ProverOpts::succinct() };
    let prover = default_prover();
    let mut total = 0.0;
    println!(
        "| # | program | handler | user cycles | total cycles (padded) | segments | proving time (s) | receipt | receipt bytes |\n\
         |---|---|---|---:|---:|---:|---:|---|---:|"
    );
    for (i, (r, t)) in recorded.iter().zip(&report.transitions).enumerate() {
        let (name, id) = if r.program == holdings_account() {
            ("ferrata_holdings", methods::FERRATA_HOLDINGS_ID)
        } else if r.program == ferrata_account() {
            ("ferrata (attestation)", methods::FERRATA_LEZ_ID)
        } else {
            panic!("an unexpected program {}", r.program)
        };
        let mut builder = ExecutorEnv::builder();
        lee::program::Program::write_receive_input(&r.input, &mut builder).unwrap();
        let env = builder.build().unwrap();
        let start = Instant::now();
        let info = prover.prove_with_opts(env, &r.elf, &opts).expect("proving succeeds");
        let secs = start.elapsed().as_secs_f64();
        total += secs;
        info.receipt.verify(id).expect("the receipt verifies against the program's image id");
        assert_eq!(info.receipt.journal.bytes, r.journal, "the proven journal is the executor's");
        let kind = match &info.receipt.inner {
            risc0_zkvm::InnerReceipt::Composite(_) => "composite",
            risc0_zkvm::InnerReceipt::Succinct(_) => "succinct",
            risc0_zkvm::InnerReceipt::Groth16(_) => "groth16",
            risc0_zkvm::InnerReceipt::Fake(_) => "FAKE (dev mode)",
            _ => "other",
        };
        let bytes = borsh::to_vec(&info.receipt).map(|b| b.len()).unwrap_or(0);
        println!(
            "| {} | {name} | {} | {} | {} | {} | {secs:.1} | {kind} | {} |",
            i + 1,
            t.handler,
            grouped(info.stats.user_cycles),
            grouped(info.stats.total_cycles),
            info.stats.segments,
            grouped(bytes as u64),
        );
    }
    println!("\nall {} handler executions proven and verified; proving time in total {total:.1} s", recorded.len());
}
