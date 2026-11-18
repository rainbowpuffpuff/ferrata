//! Adversarial checks of the program by an independent reviewer (review-lez-v03, 18 Nov 2026, of commits
//! 4ad740e7..b786073d), kept as regression tests. Every transaction through LEZ v0.3.1's block application,
//! as `v03.rs`: the record claim is atomic with the verification (`Init` and a one-transaction update whose
//! record apply refuses leave nothing behind); `Init` at its window's end is reverted, a millisecond before
//! it applies; a third party's `Init` without the adoption is refused for nobody but recovered by a
//! `Continue` (see `third_party.rs` for the run's recoveries); a config with
//! `epochs_per_sync_committee_period = 0` passes `check_config` (as in the v0.4 program) and makes that
//! instance's updates panic, charged the whole gas limit, its own instance only (README, "The program").
//!
//! `cargo test --release --locked -p harness-v03 --test adversarial -- --nocapture`

use ferrata_core::{
    steps,
    witness::{uncompressed_keys, CompactBootstrap, CompactUpdate},
};
use ferrata_v03_wire::{
    code, header_account, instance_account, instance_id, Instruction, Part, RecordClaim, RecordState, Step, Verify,
    Window,
};
use harness_v03::*;
use lee_core::account::{Account, ShardData};

fn residue() -> Verify {
    Verify::Residue { c: vec![], w: vec![] }
}

/// The program account `Chain::deploy("ferrata-v03", ..)` creates (a derived key).
fn program_id() -> lee::AccountId {
    account_of(&derived_key("deploy/ferrata-v03/header"))
}

/// A chain whose genesis already holds `squats` (Ferrata shards at the given accounts), the
/// program deployed after.
fn chain_with(squats: Vec<(lee::AccountId, Vec<u8>)>) -> Chain {
    let program = program_id();
    let extra = squats
        .into_iter()
        .map(|(a, data)| (a, Account::default().with_shard(program, ShardData::try_from(data).unwrap())))
        .collect();
    let mut chain = Chain::with_accounts(extra);
    let p = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    assert_eq!(p, program);
    chain
}

fn period_update_residue(chain: &mut Chain, inst: &mut Instance, u: &ferrata_core::types::LightClientUpdate) {
    let (p, rest, record) = inst.update("update (residue)", u, residue(), Signers::Core, false);
    let r1 = inst.send(chain, &p, GAS_LIMIT);
    assert!(r1.ok(), "{r1:?}");
    let steps: Vec<Step> = rest.into_iter().map(Step::Adopt).collect();
    let c = inst.continue_with("continue (adopt, apply)", steps, record.as_ref(), p.timestamp_ms + 12_000);
    let r2 = inst.send(chain, &c, GAS_LIMIT);
    assert!(r2.ok(), "{r2:?}");
    inst.follow(u);
    inst.assert_in_step(chain);
}

/// Atomicity, `Init`: the instance's apply verifies and writes (instance active, store), then the
/// record's apply refuses (the header account already holds another record). Nothing of the
/// instance's write may remain.
#[test]
fn review_atomic_init_record_conflict() {
    let program = program_id();
    let bootstrap = mainnet_bootstrap();
    let inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let slot = bootstrap.header.beacon.slot;
    let squat = borsh::to_vec(&RecordState { instance_id: inst.id, chain_id: 1, record: vec![0xaa; 8] }).unwrap();
    let mut chain = chain_with(vec![(inst.header_account(slot), squat.clone())]);
    let (p, rest) = inst.init(&bootstrap, usize::MAX);
    assert!(rest.is_empty());
    let nonce0 = chain.state.get_account_by_id(chain.payer).nonce;
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    println!("REVIEW T1 init-with-record-conflict: status={} exit={:?} block={} exec_gas={} cycles={} fee={}", r.status, r.exit, r.block, r.exec_gas, r.cycles, r.fee);
    assert!(r.refused_with(code::RECORD_CONFLICT), "{r:?}");
    assert!(r.block != 0, "included in a block");
    assert!(inst.state(&chain).is_none(), "the instance apply's write is reverted");
    assert_eq!(chain.shard(inst.header_account(slot), program), squat, "the squatted record unchanged");
    assert_ne!(chain.state.get_account_by_id(chain.payer).nonce, nonce0, "the payer's nonce advanced");
    println!("REVIEW T1 OK: nothing of the Init kept");
}

/// Atomicity, `Update`: the one-transaction finality update verifies at the instance (the store
/// would advance), then the record's apply refuses. The store must not move.
#[test]
fn review_atomic_update_record_conflict() {
    let program = program_id();
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let u1 = as_finality_update(&updates[1]);
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let fslot = u1.finalized_header.beacon.slot;
    let squat = borsh::to_vec(&RecordState { instance_id: inst.id, chain_id: 1, record: vec![0xbb; 8] }).unwrap();
    let mut chain = chain_with(vec![(inst.header_account(fslot), squat.clone())]);
    let (p, _) = inst.init(&bootstrap, usize::MAX);
    assert!(inst.send(&mut chain, &p, GAS_LIMIT).ok());
    period_update_residue(&mut chain, &mut inst, &updates[0]);
    let before = inst.state(&chain).unwrap();
    let (p, rest, record) = inst.update("u1 (finality, one tx)", &u1, residue(), Signers::Core, true);
    assert!(rest.is_empty() && record.is_some());
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    println!("REVIEW T2 update-with-record-conflict: status={} exit={:?} block={} exec_gas={} cycles={}", r.status, r.exit, r.block, r.exec_gas, r.cycles);
    assert!(r.refused_with(code::RECORD_CONFLICT), "{r:?}");
    assert_eq!(inst.state(&chain).unwrap(), before, "the instance unchanged");
    inst.assert_in_step(&chain);
    assert_eq!(chain.shard(inst.header_account(fslot), program), squat);
    println!("REVIEW T2 OK: the store did not move");
}

/// The timestamp window's end, at `Init`: `[from, to)`, enforced against the block's timestamp.
#[test]
fn review_init_window_end_enforced_and_exclusive() {
    let bootstrap = mainnet_bootstrap();
    let program = program_id();
    let inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let (mut p, _) = inst.init(&bootstrap, usize::MAX);
    let end = inst.config.ws_end_ms(bootstrap.header.beacon.slot).unwrap();
    let mut a = chain_with(vec![]);
    p.timestamp_ms = end;
    let r = inst.send(&mut a, &p, GAS_LIMIT);
    println!("REVIEW T3 Init at the window's end ({end}): status={} block={} exec_gas={} fee={}", r.status, r.block, r.exec_gas, r.fee);
    assert!(!r.ok(), "{r:?}");
    assert!(inst.state(&a).is_none());
    let mut b = chain_with(vec![]);
    p.timestamp_ms = end - 1;
    let r = inst.send(&mut b, &p, GAS_LIMIT);
    println!("REVIEW T3 Init 1 ms before the end: status={} exec_gas={}", r.status, r.exec_gas);
    assert!(r.ok(), "{r:?}");
}

/// A third party's `Init` of the same instance without the adoption: the owner's pre-signed `Init`
/// is refused (203) and its updates (206) until someone adopts the committee.
#[test]
fn review_init_front_run_without_adoption() {
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut chain = chain_with(vec![]);
    let program = program_id();
    let inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let (g, rest) = inst.init(&bootstrap, 0);
    assert_eq!(rest.len(), 1);
    let r = inst.send(&mut chain, &g, GAS_LIMIT);
    println!("REVIEW T4 griefer's Init without adoption: status={} exec_gas={} fee={}", r.status, r.exec_gas, r.fee);
    assert!(r.ok(), "{r:?}");
    let (p, _) = inst.init(&bootstrap, usize::MAX);
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    println!("REVIEW T4 owner's Init after it: status={} exit={:?} exec_gas={}", r.status, r.exit, r.exec_gas);
    assert!(r.refused_with(code::ALREADY_INITIALIZED), "{r:?}");
    let (u, _, _) = inst.update("owner's update_0", &updates[0], residue(), Signers::Core, true);
    let r = inst.send(&mut chain, &u, GAS_LIMIT);
    println!("REVIEW T4 owner's update after it: status={} exit={:?} exec_gas={}", r.status, r.exit, r.exec_gas);
    assert!(r.refused_with(code::INSTANCE_INACTIVE), "{r:?}");
    // Recovery: anyone adopts the committee with a Continue, which activates the instance.
    let fork = inst.config.fork().unwrap();
    let (_, job) = steps::initialize(&inst.checkpoint, &CompactBootstrap::of(&bootstrap), fork, &inst.spec, 1).unwrap();
    let c = inst.continue_with("recovery: Continue (adopt)", rest.into_iter().map(Step::Adopt).collect(), Some(job.pending.record()), g.timestamp_ms + 12_000);
    let r = inst.send(&mut chain, &c, GAS_LIMIT);
    println!("REVIEW T4 recovery Continue: status={} exec_gas={}", r.status, r.exec_gas);
    assert!(r.ok(), "{r:?}");
}

/// `check_config` accepts `epochs_per_sync_committee_period = 0`; `period_at_slot` divides by it.
/// Does an update of such an instance exit with a code, or panic (charged the whole limit)?
#[test]
fn review_zero_period_config() {
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut chain = chain_with(vec![]);
    let program = program_id();
    let normal = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let mut config = mainnet_config(1);
    config.epochs_per_sync_committee_period = 0;
    let checkpoint = root(MAINNET_CHECKPOINT);
    let id = instance_id(&config, &checkpoint);
    let fork = config.fork().unwrap();
    let spec = config.spec();
    let compact = CompactBootstrap::of(&bootstrap);
    let slot = bootstrap.header.beacon.slot;
    let (_, job) = steps::initialize(&checkpoint, &compact, fork, &spec, 1).expect("host initialize");
    let keys = uncompressed_keys(&bootstrap.current_sync_committee).unwrap();
    let init = Instruction::Init {
        config: config.clone(),
        checkpoint,
        bootstrap: compact.encode(fork),
        adopt: vec![Part { index: 0, keys }],
        record: Some(RecordClaim { chain_id: config.chain_id, record: job.pending.record().encode() }),
        window: Window { from_ms: None, to_ms: config.ws_end_ms(slot) },
    };
    let accounts = vec![instance_account(&program, &id), header_account(&program, &id, slot)];
    let r = chain.submit("zero-period Init", program, &accounts, borsh::to_vec(&init).unwrap(), &[], GAS_LIMIT, config.slot_start_ms(slot + 1));
    println!("REVIEW T5 zero-period Init: status={} exit={:?} exec_gas={} fee={}", r.status, r.exit, r.exec_gas, r.fee);
    if !r.ok() {
        return;
    }
    let u0 = &updates[0];
    let layout = normal.layout_for(fork, u0);
    let update = Instruction::Update {
        instance_id: id,
        update: CompactUpdate::of(u0).encode(layout),
        signers: normal.signer_bytes(u0, Signers::Core),
        verify: Verify::Open,
        adopt: vec![],
        record: None,
        window: Window { from_ms: Some(config.slot_start_ms(u0.signature_slot)), to_ms: config.ws_end_ms(slot) },
    };
    let accounts = vec![instance_account(&program, &id)];
    let r = chain.submit("zero-period Update", program, &accounts, borsh::to_vec(&update).unwrap(), &[], GAS_LIMIT, config.slot_start_ms(u0.signature_slot) + 12_000);
    println!("REVIEW T5 zero-period Update (Open): status={} exit={:?} exec_gas={} gas_limit={} fee={}", r.status, r.exit, r.exec_gas, r.gas_limit, r.fee);
}
