//! Stage 2 with a real proof: one finality update (mainnet update_1 without its next committee)
//! verified in one privately executed actor of the program, proven on this machine's CPU
//! (`RISC0_DEV_MODE` must be unset or 0), then settled against the public instance.
//!
//! Run (from `lez/`, see `RESULTS.md`):
//! `RISC0_DEV_MODE=0 /usr/bin/time -v cargo run -p harness --bin prove_private` (the dev profile:
//! LEZ refuses to build its test helpers into a release build; dependencies are optimised, and
//! the proving runs in `r0vm`).

use std::time::Instant;

use ferrata_core::{fork::LcFork, validate::Context, witness::CompactUpdate};
use ferrata_lez_wire::{header_actor, Message};
use harness::{private::*, scenarios::World, *};
use lee::Actor;
use lee_core::program::ReceiveInput;
use risc0_zkvm::{default_executor, ExecutorEnv};

fn main() {
    assert!(
        !matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true")),
        "unset RISC0_DEV_MODE (or set it to 0) for a real proof"
    );
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let sender = w.chain.add_private_account(0x31, 1_000_000_000_000);
    let update = as_finality_update(&w.updates[1]);
    let store = w.inst.store(&w.chain);
    let witness = Instance::signer_witness(w.native.core(), &update, &w.spec);
    let message = borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: store.encode(),
        update: CompactUpdate::of(&update).encode(LcFork::Electra),
        signers: witness.encode(),
        adoption: vec![],
    })
    .unwrap();
    let root = Actor::new(sender.id(), ferrata_account());

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
        "private transition (VerifyAll, finality update): {} user cycles, {} segments, {} cycles padded (sum of 2^po2), exit {:?}",
        grouped(info.cycles()),
        info.segments.len(),
        grouped(padded),
        info.exit_code
    );

    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    let started = Instant::now();
    let run = w
        .chain
        .prove_and_settle_private(&sender, root, message, vec![w.inst.actor(), header], &catalog(), 100_000_000, time)
        .expect("proves and settles");
    let total = started.elapsed();
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: root_hex(), bls: &harness::scenarios::BLS };
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    w.check_record(update.finalized_header.beacon.slot);
    println!("{}", describe_public_outputs(&run.tx));
    println!(
        "REAL PROOF: execute_and_prove {:.1} s (prove + settle {:.1} s); receipt (succinct InnerReceipt, borsh) {} bytes; transaction {} bytes",
        run.prove_time.as_secs_f64(),
        total.as_secs_f64(),
        run.proof_bytes,
        run.tx_bytes
    );
    println!(
        "settled: execution gas {} (PRIVATE_VERIFY_GAS {} + public part {} cycles), storage gas {}, base fee {}",
        grouped(run.charge.gas_exec),
        grouped(fee_core::market::PRIVATE_VERIFY_GAS),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
        grouped(run.charge.fee_base as u64)
    );
}

fn root_hex() -> ferrata_core::Root {
    harness::root(GENESIS_VALIDATORS_ROOT)
}
