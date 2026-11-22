//! Stage 2 with a real proof: one finality update (mainnet update_1 without its next committee)
//! verified in one privately executed actor of the program, proven by `r0vm` (`RISC0_DEV_MODE`
//! unset or 0; on the CPU, or on a GPU with a CUDA build of r0vm 3.0.5), then settled against the
//! public instance through LEZ's private settlement, with the receipt verified.
//!
//! Run from `lez/` in the dev profile (LEZ refuses to build its test helpers into a release
//! build; dependencies are optimised, and the proving runs in `r0vm`); see `RESULTS.md`, §3 and §7:
//!
//! * `prove_private [--tx-out FILE]`: prove, then settle. With `--tx-out`, the proven transaction
//!   (borsh `LeeTransaction`, the receipt included) is also written to FILE.
//! * `prove_private --settle FILE`: settle a transaction written by `--tx-out`, without proving.
//!   The same state is rebuilt from the fixtures, the transaction must claim this binary's
//!   program image, and LEZ's settlement verifies its receipt (a fake one only passes in dev mode).
//! * `--allow-dev-mode`: let the proving step run in RISC Zero dev mode, which makes a fake
//!   receipt in seconds, to try the commands; the output then says it is not a proof.
//! * `--handoff` (with either form): the private transaction is a committee handoff instead: update_1
//!   as served, which hands over the next committee, its 512 keys adopted in the same call, after
//!   update_1's finality update went through the public job (no new header: the handoff of
//!   `tests/private_placement.rs`, there in dev mode).
//!
//! `FERRATA_EXPECT_IMAGE_ID=<hex>` stops it before any work unless the program image compiled into
//! this binary is that one. `FERRATA_FIXTURE_DIR` overrides where the mainnet fixtures are read.

use std::{fs, time::Instant};

use fee_core::BlockFeeSummary;
use ferrata_core::{fork::LcFork, validate::Context, witness::CompactUpdate};
use ferrata_lez_wire::{header_actor, Message};
use harness::{
    private::*,
    scenarios::World,
    settle::{classify, opening_fee_state, settle_private_transaction, LeeTransaction},
    *,
};
use lee::{Actor, PrivacyPreservingTransaction};
use lee_core::{program::ReceiveInput, ProgramImageClaim};
use risc0_zkvm::{default_executor, ExecutorEnv};

fn usage() -> ! {
    eprintln!(
        "usage: prove_private [--handoff] [--tx-out FILE] [--allow-dev-mode] | prove_private [--handoff] --settle FILE"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut tx_out, mut settle_from, mut allow_dev_mode, mut handoff) = (None, None, false, false);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--tx-out" => tx_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--settle" => settle_from = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--allow-dev-mode" => allow_dev_mode = true,
            "--handoff" => handoff = true,
            _ => usage(),
        }
    }
    if settle_from.is_some() && (tx_out.is_some() || allow_dev_mode) {
        usage();
    }
    let dev_mode = matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true"));
    let program_image = image_id_hex(methods::FERRATA_LEZ_ID);
    println!(
        "program image {program_image} ({} ELF bytes); consumer image {}; RISC0_DEV_MODE {}",
        methods::FERRATA_LEZ_ELF.len(),
        image_id_hex(methods::FERRATA_CONSUMER_ID),
        if dev_mode { "on" } else { "off" }
    );
    if let Ok(expected) = std::env::var("FERRATA_EXPECT_IMAGE_ID") {
        assert_eq!(program_image, expected.trim().to_lowercase(), "this binary holds another program image");
    }
    assert!(
        settle_from.is_some() || !dev_mode || allow_dev_mode,
        "unset RISC0_DEV_MODE (or set it to 0) for a real proof; --allow-dev-mode tries the commands with a fake receipt"
    );

    // The state the proof is made against, and settled on: Init, then update_0 through the public
    // path, then a funded private account. Everything here is deterministic.
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    if handoff {
        let finality = as_finality_update(&w.updates[1]);
        w.apply(&finality, "update_1 as finality update");
    }
    let sender = w.chain.add_private_account(0x31, 1_000_000_000_000);
    let update = if handoff { w.updates[1].clone() } else { as_finality_update(&w.updates[1]) };
    let store = w.inst.store(&w.chain);
    let witness = Instance::signer_witness(w.native.core(), &update, &w.spec);
    let message = borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: store.encode(),
        update: CompactUpdate::of(&update).encode(LcFork::Electra),
        signers: witness.encode(),
        adoption: if handoff {
            ferrata_core::witness::uncompressed_keys(&update.next_sync_committee).unwrap()
        } else {
            vec![]
        },
    })
    .unwrap();
    let root = Actor::new(sender.id(), ferrata_account());
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    // A handoff without a new header writes no record: the instance alone.
    let actors = if handoff { vec![w.inst.actor()] } else { vec![w.inst.actor(), header] };
    let what = if handoff { "committee handoff, 512 keys adopted" } else { "finality update" };
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;

    let run = match &settle_from {
        Some(path) => settle(&mut w, path, time),
        None => {
            // The private transition alone, executed: what the prover proves for the program.
            let input = ReceiveInput {
                receiver: root,
                from: None,
                is_authorized: true,
                pre_state: Default::default(),
                message: message.clone(),
            };
            let mut builder = ExecutorEnv::builder();
            lee::program::Program::write_receive_input(&input, &mut builder).unwrap();
            let info = default_executor().execute(builder.build().unwrap(), ferrata_program().elf()).unwrap();
            let padded: u64 = info.segments.iter().map(|s| 1u64 << s.po2).sum();
            println!(
                "private transition (VerifyAll, {what}): {} user cycles, {} segments, {} cycles padded (sum of 2^po2), exit {:?}",
                grouped(info.cycles()),
                info.segments.len(),
                grouped(padded),
                info.exit_code
            );
            let started = Instant::now();
            let run = w
                .chain
                .prove_and_settle_private(&sender, root, message, actors, &catalog(), 100_000_000, time)
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

    // The settled transaction applied the update: the instance holds the native store's compact
    // form and the record of its finalized header.
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: root_hex(), bls: &harness::scenarios::BLS };
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    if !handoff {
        w.check_record(update.finalized_header.beacon.slot);
    }
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

/// Settle the transaction in `path` on `w`, as `Chain::prove_and_settle_private` settles the one
/// it proves: the same opening fee state, block and time.
fn settle(w: &mut World, path: &str, time: u64) -> PrivateRun {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let lee_tx: LeeTransaction = borsh::from_slice(&bytes).expect("a borsh LeeTransaction");
    let LeeTransaction::PrivacyPreserving(tx) = &lee_tx else { panic!("{path} holds a public transaction") };
    let claims_program = tx.message.execution.program_image_claims.iter().any(|claim| {
        matches!(claim, ProgramImageClaim::Disclosed { account_id, image_id }
            if *account_id == ferrata_account() && *image_id == methods::FERRATA_LEZ_ID)
    });
    assert!(claims_program, "the transaction does not claim this binary's program image");
    let view = classify(&lee_tx).expect("classifies");
    let opening = opening_fee_state(&w.chain.state);
    let mut summary = BlockFeeSummary::default();
    let started = Instant::now();
    let (events, charge) =
        settle_private_transaction(&lee_tx, &view, &mut w.chain.state, &opening, w.chain.block_id, time, &mut summary)
            .expect("settlement accepts the transaction");
    w.chain.block_id += 1;
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

fn root_hex() -> ferrata_core::Root {
    harness::root(GENESIS_VALIDATORS_ROOT)
}
