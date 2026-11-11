//! Stage 4 with a real proof: one private attestation. A consumer executed at a private account
//! reads the record of mainnet block 26,384,851 and verifies an ERC-20 `Transfer` log of that
//! block against it (`ConsumerMessage::AttestPrivately`); the two transitions of the consumer, the
//! fee transfer and LEZ's privacy-preserving circuit over them are proven by `r0vm`
//! (`RISC0_DEV_MODE` unset or 0; on the CPU, or on a GPU with a CUDA build of r0vm 3.0.5), and the
//! transaction is settled through LEZ's private settlement with the receipt verified.
//!
//! The same commands as `prove_private` (see `RESULTS.md`, §3 and §5):
//!
//! * `prove_attest [--shape wallet|bare] [--tx-out FILE]`: prove, then settle. With `--tx-out`,
//!   the proven transaction (borsh `LeeTransaction`, the receipt included) is also written to FILE.
//!   `--shape wallet` (the default) pads the transaction as LEZ v0.4.0-rc1's wallet pads every
//!   private transaction (seven private actions, 512-byte notes, the fee's recovery note:
//!   [`harness::private::Shape::Wallet`]); `--shape bare` builds it as this binary did before
//!   11 November 2026 (one private action, no padding, the refund to a public account).
//! * `prove_attest --settle FILE`: settle a transaction written by `--tx-out`, without proving.
//!   The same state is rebuilt from the fixture, the transaction must claim this binary's
//!   consumer image, and LEZ's settlement verifies its receipt (a fake one only passes in dev
//!   mode).
//! * `--allow-dev-mode`: let the proving step run in RISC Zero dev mode, which makes a fake
//!   receipt in seconds, to try the commands; the output then says it is not a proof.
//! * `prove_attest --image-ids`: print the image ids this binary holds, and stop.
//!
//! `FERRATA_EXPECT_CONSUMER_IMAGE_ID=<hex>` stops it before any work unless the consumer image
//! compiled into this binary is that one (the consumer is the program the proof covers; the
//! attestation program runs only publicly here). `FERRATA_ATTEST_FIXTURE_DIR` overrides where the
//! fixture is read; its SHA-256 sums are compiled in.

use std::{fs, time::Instant};

use fee_core::BlockFeeSummary;
use ferrata_lez_wire::{
    consumer::{ConsumerState, Deposit},
    Message,
};
use harness::{
    attest::{render_executed, Stage4},
    private::*,
    settle::{classify, opening_fee_state, settle_private_transaction, LeeTransaction},
    *,
};
use lee::{Actor, PrivacyPreservingTransaction};
use lee_core::{execution_state::BoundaryStep, ProgramImageClaim};

fn usage() -> ! {
    eprintln!(
        "usage: prove_attest [--shape wallet|bare] [--tx-out FILE] [--allow-dev-mode] | --settle FILE | --image-ids"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut tx_out, mut settle_from, mut allow_dev_mode, mut only_ids) = (None, None, false, false);
    let mut shape = Shape::Wallet;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--tx-out" => tx_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--shape" => shape = Shape::parse(it.next().unwrap_or_else(|| usage())).unwrap_or_else(|| usage()),
            "--settle" => settle_from = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--allow-dev-mode" => allow_dev_mode = true,
            "--image-ids" => only_ids = true,
            _ => usage(),
        }
    }
    if settle_from.is_some() && (tx_out.is_some() || allow_dev_mode) {
        usage();
    }
    let dev_mode = matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true"));
    let consumer_image = image_id_hex(methods::FERRATA_CONSUMER_ID);
    println!(
        "consumer image {consumer_image} ({} ELF bytes); program image {} ({} ELF bytes); RISC0_DEV_MODE {}; shape {}",
        methods::FERRATA_CONSUMER_ELF.len(),
        image_id_hex(methods::FERRATA_LEZ_ID),
        methods::FERRATA_LEZ_ELF.len(),
        if dev_mode { "on" } else { "off" },
        shape.name()
    );
    if only_ids {
        return;
    }
    if let Ok(expected) = std::env::var("FERRATA_EXPECT_CONSUMER_IMAGE_ID") {
        assert_eq!(consumer_image, expected.trim().to_lowercase(), "this binary holds another consumer image");
    }
    assert!(
        settle_from.is_some() || !dev_mode || allow_dev_mode,
        "unset RISC0_DEV_MODE (or set it to 0) for a real proof; --allow-dev-mode tries the commands with a fake receipt"
    );

    // The state the proof is made against, and settled on: `Init` from the fixture's bootstrap and
    // its finality update through the public job, then a funded private account. Deterministic.
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let t = s.fx.transfer.clone();
    let pool = s.pool();
    let header = s.header(slot);
    let account = s.w.chain.add_private_account(0x81, 1_000_000_000_000);
    let message = s.private_message(&pool, slot, &t);
    let root = Actor::new(account.id(), consumer_account());
    println!(
        "record: header(id, {slot}) of instance {}, mainnet block {}; fact: Transfer of token {} in transaction {}, log {}",
        hex::encode(&s.w.inst.id[..6]),
        s.fx.block,
        hex::encode(t.token),
        t.tx_index,
        t.log_index
    );

    let run = match &settle_from {
        Some(path) => settle(&mut s, path),
        None => {
            // The consumer's two private transitions, executed: what the prover proves for it.
            let executed = s.private_transitions(account.id(), slot, &message);
            assert!(executed.iter().all(|e| e.exit_code == 0), "the attestation verifies");
            println!("the consumer's private transitions (executor):\n{}", render_executed(&executed));
            let state = executed[1].transition.as_ref().unwrap().response.post_state.clone().unwrap();
            let ConsumerState::Private(private) = borsh::from_slice(&state).unwrap() else { panic!("a private state") };
            let deposit = Deposit {
                statement_id: s.transfer_id(&t),
                block_number: s.fx.block,
                owner: t.from,
                amount: t.amount(),
            };
            assert_eq!(private.last, Some((pool.id(), deposit)), "the fact the consumer keeps");
            let started = Instant::now();
            let run =
                s.w.chain
                    .prove_and_settle_private_as(
                        &account,
                        root,
                        borsh::to_vec(&message).unwrap(),
                        vec![header],
                        &catalog(),
                        100_000_000,
                        s.time,
                        shape,
                    )
                    .expect("proves and settles");
            let total = started.elapsed();
            let (what, receipt) = if dev_mode {
                ("DRY RUN, NOT A PROOF (RISC0_DEV_MODE=1)", "fake receipt")
            } else {
                ("REAL PROOF", "succinct InnerReceipt, borsh")
            };
            println!(
                "{what}: execute_and_prove {:.1} s (prove + settle {:.1} s); receipt ({receipt}) {} bytes; transaction {} bytes",
                run.prove_time.as_secs_f64(),
                total.as_secs_f64(),
                run.proof_bytes,
                run.tx_bytes
            );
            if let Some(path) = &tx_out {
                let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
                fs::write(path, &bytes).unwrap_or_else(|e| panic!("writing {path}: {e}"));
                println!("wrote the proven transaction to {path} ({} bytes)", bytes.len());
            }
            run
        }
    };

    // The settled transaction read the record and wrote nothing public for the consumer.
    let read = borsh::to_vec(&Message::Read).unwrap();
    let reads = run.tx.message.execution.boundary.iter().filter(|step| {
        matches!(step, BoundaryStep::PrivateToPublic(d) if d.envelope.to == header && d.envelope.message == read)
    });
    assert_eq!(reads.count(), 1, "one Read to the record");
    assert!(s.claim_state(&pool, &s.transfer_id(&t)).is_none() && s.balance(&pool, &t.from).is_none());
    s.w.check_store();
    println!("{}", describe_public_outputs(&run.tx));
    println!(
        "settled: execution gas {} (PRIVATE_VERIFY_GAS {} + public part {} cycles), storage gas {}, base fee {}",
        grouped(run.charge.gas_exec),
        grouped(fee_core::market::PRIVATE_VERIFY_GAS),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
        grouped(run.charge.fee_base as u64)
    );
}

/// Settle the transaction in `path` on `s`, as `Chain::prove_and_settle_private` settles the one
/// it proves: the same opening fee state, block and time.
fn settle(s: &mut Stage4, path: &str) -> PrivateRun {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let lee_tx: LeeTransaction = borsh::from_slice(&bytes).expect("a borsh LeeTransaction");
    let LeeTransaction::PrivacyPreserving(tx) = &lee_tx else { panic!("{path} holds a public transaction") };
    let claims_consumer = tx.message.execution.program_image_claims.iter().any(|claim| {
        matches!(claim, ProgramImageClaim::Disclosed { account_id, image_id }
            if *account_id == consumer_account() && *image_id == methods::FERRATA_CONSUMER_ID)
    });
    assert!(claims_consumer, "the transaction does not claim this binary's consumer image");
    let view = classify(&lee_tx).expect("classifies");
    let opening = opening_fee_state(&s.w.chain.state);
    let mut summary = BlockFeeSummary::default();
    let started = Instant::now();
    let time = s.time;
    let block_id = s.w.chain.block_id;
    let (events, charge) =
        settle_private_transaction(&lee_tx, &view, &mut s.w.chain.state, &opening, block_id, time, &mut summary)
            .expect("settlement accepts the transaction");
    s.w.chain.block_id += 1;
    let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
    let verified = if matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true")) {
        "RISC0_DEV_MODE on: a fake receipt passes too"
    } else {
        "receipt verified"
    };
    println!(
        "SETTLED FROM {path} ({verified}): receipt {proof_bytes} bytes, transaction {} bytes, settled in {:.1} s",
        bytes.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(charge.revert.is_none(), "the public part reverted: {:?}", charge.revert);
    let tx: PrivacyPreservingTransaction = tx.clone();
    PrivateRun { tx, charge, events, prove_time: Default::default(), tx_bytes: bytes.len(), proof_bytes }
}
