//! Stage 2: the whole verification of an update in one privately executed actor of the program,
//! applied to the public instance. Run in RISC Zero dev mode (`RISC0_DEV_MODE=1`, fake receipts):
//! the private transitions and the privacy-preserving circuit are executed, not proven; the
//! settlement path, the public part's execution and its metering are the real ones. The real
//! proof is `cargo run --release -p harness --bin prove_private` (see `lez/RESULTS.md`).

use ferrata_core::{fork::LcFork, witness::CompactUpdate};
use ferrata_lez_wire::{header_actor, Message};
use harness::{private::*, scenarios::World, *};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, Actor};
use test_guest_core::Script;

fn require_dev_mode() {
    assert_eq!(
        std::env::var("RISC0_DEV_MODE").as_deref(),
        Ok("1"),
        "run these tests with RISC0_DEV_MODE=1 (fake receipts); the real proof is the prove_private binary"
    );
}

/// `VerifyAll` for `update` against the instance's current store.
fn verify_all(w: &World, update: &ferrata_core::types::LightClientUpdate) -> Vec<u8> {
    let store = w.inst.store(&w.chain);
    let witness = Instance::signer_witness(w.native.core(), update, &w.spec);
    let adoption = if update.next_sync_committee.is_empty() {
        vec![]
    } else {
        ferrata_core::witness::uncompressed_keys(&update.next_sync_committee).unwrap()
    };
    borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: store.encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: witness.encode(),
        adoption,
    })
    .unwrap()
}

#[test]
fn an_update_verified_privately_applies_to_the_public_instance() {
    require_dev_mode();
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let sender = w.chain.add_private_account(0x31, 1_000_000_000_000);
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;

    // 1. A finality update (update_1 without its next committee), verified in one private call.
    let update = as_finality_update(&w.updates[1]);
    let message = verify_all(&w, &update);
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let root = Actor::new(sender.id(), ferrata_account());
    let run = w
        .chain
        .prove_and_settle_private(&sender, root, message, vec![w.inst.actor(), header], &catalog(), 100_000_000, time)
        .expect("settles");
    let spec = w.spec.clone();
    let ctx = ferrata_core::validate::Context {
        spec: &spec,
        genesis_validators_root: harness::root(GENESIS_VALIDATORS_ROOT),
        bls: &harness::scenarios::BLS,
    };
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    w.check_record(update.finalized_header.beacon.slot);
    println!("finality update, verified privately:\n{}", describe_public_outputs(&run.tx));
    println!(
        "charged: execution gas {} (PRIVATE_VERIFY_GAS {} + public part {} cycles), storage gas {}, base fee {}; tx {} bytes, proof {} bytes (dev mode)",
        grouped(run.charge.gas_exec),
        grouped(fee_core::market::PRIVATE_VERIFY_GAS),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
        grouped(run.charge.fee_base as u64),
        run.tx_bytes,
        run.proof_bytes,
    );
    assert!(run.charge.revert.is_none());
    assert_eq!(run.charge.gas_stor, fee_core::market::PRIVATE_GAS_STOR);
    assert_eq!(run.charge.gas_exec, fee_core::market::PRIVATE_VERIFY_GAS + run.charge.cycles);

    // 2. A handoff (update_1 as served: it hands over the period-1901 committee), with the
    //    adoption's 512 keys in the same private call.
    let sender2 = w.chain.add_private_account(0x41, 1_000_000_000_000);
    let update = w.updates[1].clone();
    let message = verify_all(&w, &update);
    let root = Actor::new(sender2.id(), ferrata_account());
    let run = w
        .chain
        .prove_and_settle_private(&sender2, root, message, vec![w.inst.actor()], &catalog(), 100_000_000, time)
        .expect("settles");
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    println!(
        "handoff verified privately: execution gas {} (public part {} cycles), storage gas {}",
        grouped(run.charge.gas_exec),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor)
    );
}

#[test]
fn a_private_sender_of_another_program_is_refused() {
    require_dev_mode();
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let sender = w.chain.add_private_account(0x51, 1_000_000_000_000);
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    // A verified-looking job for the instance's current store, sent by a private actor of LEZ's
    // `scripted` program.
    let store = w.inst.store(&w.chain);
    let job = ferrata_core::steps::Job {
        store_digest: store.digest(),
        update_digest: [0; 32],
        phase: ferrata_core::steps::Phase::Verified,
        adoption: None,
        pending: ferrata_core::steps::PendingApply::Update {
            finalized: ferrata_core::types::BeaconBlockHeader { slot: 99_999_999, ..Default::default() },
            next: None,
            record: ferrata_core::steps::HeaderRecord::of(&w.bootstrap.header, LcFork::Electra, &w.spec),
        },
    };
    let apply = Message::ApplyVerified { instance_id: w.inst.id, job: job.encode() };
    let script = Script::default().send_sync(w.inst.actor(), &apply);
    let root = Actor::new(sender.id(), scripted_account());
    let programs =
        ProgramCatalog::from([(scripted_account(), scripted_program()), (ferrata_account(), ferrata_program())]);
    // An honest prover simulates the public part first and stops: the instance refuses.
    let simulated = w.chain.prove_and_settle_private(
        &sender,
        root,
        borsh::to_vec(&script).unwrap(),
        vec![w.inst.actor()],
        &programs,
        100_000_000,
        time,
    );
    let err = simulated.err().expect("the simulation of the public part fails");
    println!("another program's private actor, simulated: {err}");
    let wrong_sender = format!("code {}", ferrata_lez_wire::code::WRONG_SENDER);
    assert!(err.contains(&wrong_sender), "WrongSender ({wrong_sender}): {err}");
    // A prover that skips the simulation gets a proof; settlement replays the public part, the
    // instance refuses, and the transaction falls back to paying its fee alone.
    let settled = w
        .chain
        .prove_and_settle_private_with(
            &sender,
            root,
            borsh::to_vec(&script).unwrap(),
            vec![w.inst.actor()],
            &programs,
            100_000_000,
            time,
            Some(vec![Vec::new()]),
        )
        .expect("settles as a reverted transaction");
    let revert = settled.charge.revert.clone().expect("the public part reverted");
    println!(
        "another program's private actor, settled: reverted ({revert}); charged execution gas {}, storage gas {}",
        grouped(settled.charge.gas_exec),
        grouped(settled.charge.gas_stor)
    );
    assert!(revert.contains(&wrong_sender), "{revert}");
    println!("{}", describe_public_outputs(&settled.tx));
    w.check_store();
}

#[test]
fn a_forged_update_cannot_be_proven() {
    require_dev_mode();
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let sender = w.chain.add_private_account(0x61, 1_000_000_000_000);
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    let mut forged = as_finality_update(&w.updates[1]);
    forged.sync_aggregate.signature = w.updates[2].sync_aggregate.signature;
    let message = verify_all(&w, &forged);
    let root = Actor::new(sender.id(), ferrata_account());
    let result =
        w.chain.prove_and_settle_private(&sender, root, message, vec![w.inst.actor()], &catalog(), 100_000_000, time);
    let err = result.err().expect("the private transition exits with SignatureInvalid, so there is no proof");
    println!("forged update, private: {err}");
    assert!(err.contains("code 40"), "{err}");
    w.check_store();
}
