//! Stage 5: Sepolia through its Gloas fork, through the program (`harness::sepolia`).
//!
//! An instance with Sepolia's fork schedule starts from the bootstrap of period 1378 (Fulu era,
//! Electra layout) and takes every period's update to the newest one of the fixture, each in the
//! layout the beacon node served it in. With the first update attested under Gloas the store
//! upgrades itself to the Gloas layout; nothing else tells it to.

use ferrata_core::{
    fork::LcFork,
    validate::Context,
    witness::{uncompressed_keys, CompactUpdate, ExecutionSummary},
};
use ferrata_lez_wire::{header_actor, Message, UPGRADED_SELECTOR};
use harness::{
    ferrata_account, grouped,
    private::catalog,
    render, root,
    scenarios::BLS,
    sepolia::{self, BOUND_BLOCK_HASH, FINALIZED_SLOT, FIRST_PERIOD, GLOAS_PERIOD, LAST_PERIOD},
    Instance,
};
use lee::Actor;

const MAX_GAS_EXEC: u64 = 10_000_000;

/// The layout bytes of `Config::store_fork` and of a store's encoding.
const ELECTRA: u8 = 3;
const GLOAS: u8 = 4;

#[test]
fn sepolia_through_the_gloas_fork_over_the_public_path() {
    let mut world = sepolia::replay(2, LAST_PERIOD);
    // The fixture's finality update: the record of a finalized header in the Gloas layout.
    let finality = sepolia::sepolia_finality_update();
    let run = world.apply(&finality, "finality update");
    assert_eq!(run.reports.len(), 3, "a finality update is three transactions");
    println!("{}", render(&world.chain.reports));

    let periods = (LAST_PERIOD - FIRST_PERIOD + 1) as usize;
    assert_eq!(world.chain.reports.len(), 3 + 5 * periods + 3, "Init 3, each period's update 5, the finality update 3");
    for r in &world.chain.reports {
        assert!(r.ok(), "{}", r.label);
        assert!(r.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", r.label);
    }
    // Exactly one upgrade, Electra to Gloas, when the job of the first Gloas update is applied.
    let upgrades: Vec<(String, Vec<u8>)> = world
        .chain
        .reports
        .iter()
        .flat_map(|r| {
            r.events
                .iter()
                .filter(|e| e.event.selector == UPGRADED_SELECTOR)
                .map(move |e| (r.label.clone(), e.event.data.clone()))
        })
        .collect();
    assert_eq!(upgrades.len(), 1, "{upgrades:?}");
    assert!(upgrades[0].0.starts_with(&format!("period {GLOAS_PERIOD} Finish")), "{}", upgrades[0].0);
    assert_eq!(upgrades[0].1, vec![ELECTRA, GLOAS]);
    assert_eq!(world.inst.store(&world.chain).fork, LcFork::Gloas);
    assert_eq!(world.inst.config.store_fork, ELECTRA, "the configuration is unchanged");

    // The record of the newest finalized header binds the execution block hash only.
    let record = world.inst.record(&world.chain, FINALIZED_SLOT).expect("a record");
    assert_eq!(record.header().unwrap().execution, ExecutionSummary::BlockHash(root(BOUND_BLOCK_HASH)));

    let total = |f: &dyn Fn(&harness::TxReport) -> u64| world.chain.reports.iter().map(f).sum::<u64>();
    println!(
        "Sepolia replay: {} transactions, execution gas {}, storage gas {}, cycles {}",
        world.chain.reports.len(),
        grouped(total(&|r| r.charge.gas_exec)),
        grouped(total(&|r| r.charge.gas_stor)),
        grouped(total(&|r| r.charge.cycles)),
    );
}

/// The upgrade through the proven path (Stage 2's `VerifyAll` in one private call), in dev mode:
/// the store is upgraded by the same `update_and_store` as on the public path.
#[test]
fn the_upgrade_through_the_proven_path_in_dev_mode() {
    if std::env::var("RISC0_DEV_MODE").as_deref() != Ok("1") {
        eprintln!("skipped: needs RISC0_DEV_MODE=1 (fake receipts)");
        return;
    }
    let mut w = sepolia::sepolia_world(sepolia::sepolia_config(2));
    let first = sepolia::sepolia_update(FIRST_PERIOD);
    w.apply(&first, &format!("period {FIRST_PERIOD}"));
    assert_eq!(w.inst.store(&w.chain).fork, LcFork::Electra);

    let update = sepolia::sepolia_update(GLOAS_PERIOD);
    let mut native = w.native.clone();
    native.upgrade(LcFork::Gloas, &w.spec).unwrap();
    let message = borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: w.inst.store(&w.chain).encode(),
        update: CompactUpdate::of(&update).encode(LcFork::Gloas),
        signers: Instance::signer_witness(native.core(), &update, &w.spec).encode(),
        adoption: uncompressed_keys(&update.next_sync_committee).unwrap(),
    })
    .unwrap();
    let sender = w.chain.add_private_account(0x31, 1_000_000_000_000);
    let slot = update.finalized_header.beacon.slot;
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
    let run = w
        .chain
        .prove_and_settle_private(
            &sender,
            Actor::new(sender.id(), ferrata_account()),
            message,
            vec![w.inst.actor(), header],
            &catalog(),
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none());
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: w.inst.config.genesis_validators_root, bls: &BLS };
    native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.native = native;
    assert_eq!(w.inst.store(&w.chain).fork, LcFork::Gloas, "the store upgraded itself");
    w.check_store();
    w.check_record(slot);
    println!(
        "period {GLOAS_PERIOD} (the upgrade) in one private VerifyAll: public part {} cycles, tx {} bytes (dev mode)",
        grouped(run.charge.cycles),
        run.tx_bytes
    );
}

/// Real Sepolia facts under a record in the Gloas layout: the record binds the block hash only,
/// and each request carries the block's RLP header. Then the refusals, the chain binding among
/// them (021 F.5: the same proof asked as a mainnet statement).
#[test]
fn real_sepolia_facts_under_a_gloas_record() {
    let (s, accepted, refused) = sepolia::attestations();
    println!("{}", render(&accepted));
    println!("{}", render(&refused));
    for r in accepted.iter().chain(&refused) {
        assert!(r.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", r.label);
    }
    assert_eq!(s.w.inst.store(&s.w.chain).fork, LcFork::Gloas);
}
