//! What a third party can do to a run on a public LEZ v0.3.1 network, and how the run recovers
//! (`harness_v03::Instance::plan_from_state`, `harness_v03::deployment`): liveness only, never
//! soundness. Every transaction through LEZ v0.3.1's block application, as `v03.rs`; the third
//! party's transactions are sent first, then the run reads the chain and signs what is missing.
//!
//! `cargo test --release --locked -p harness-v03 --test third_party -- --nocapture`

use ferrata_v03_wire::{code, Step};
use harness_v03::*;

fn known(bootstrap: &ferrata_core::types::LightClientBootstrap, updates: &[ferrata_core::types::LightClientUpdate]) -> Vec<ferrata_core::types::SyncCommittee> {
    let mut k = vec![bootstrap.current_sync_committee.clone()];
    for u in updates {
        if u.next_sync_committee.pubkeys.iter().any(|p| *p != [0u8; 48]) {
            k.push(u.next_sync_committee.clone());
        }
    }
    k
}

/// The run's whole plan from the state it finds, sent; every step must apply.
fn run_from_state(chain: &mut Chain, inst: &mut Instance, updates: &[(String, ferrata_core::types::LightClientUpdate)], all: &[ferrata_core::types::LightClientUpdate]) -> Vec<String> {
    let bootstrap = mainnet_bootstrap();
    let mut notes = vec![];
    let plan = inst.plan_from_state(&*chain, &bootstrap, updates, &known(&bootstrap, all), &mut notes).expect("a plan");
    for n in &notes {
        println!("run: {n}");
    }
    for p in &plan {
        let r = inst.send(chain, &p.planned, GAS_LIMIT);
        println!("run: {} [{}] {} {}", p.planned.label, p.kind, r.status, r.exec_gas);
        assert!(r.ok(), "{r:?}");
    }
    plan.into_iter().map(|p| p.kind).collect()
}

fn mainnet_plan() -> Vec<(String, ferrata_core::types::LightClientUpdate)> {
    let u = mainnet_updates();
    vec![("update_0 (period)".into(), u[0].clone()), ("update_1 (finality)".into(), as_finality_update(&u[1]))]
}

/// A third party writes one of the run's segment accounts before the run deploys (a
/// `WriteSegment` needs no signature): the run finds the account taken and deploys at fresh
/// accounts instead; the program and the instance then work as planned.
#[test]
fn a_squatted_segment_account_makes_the_run_choose_fresh_accounts() {
    let mut chain = Chain::new();
    let seg = |label: &str, i: usize| account_of(&derived_key(&format!("deploy/{label}/segment/{i}")));
    let header = |label: &str| account_of(&derived_key(&format!("deploy/{label}/header")));
    let n = segment_shards(&[]).len();
    let planned: Vec<_> = (0..n).map(|i| seg("run-1", i)).collect();
    assert_eq!(deployment(&chain, header("run-1"), &planned), Deployment::Free { written: vec![false; n] });
    // The third party's WriteSegment of junk into the run's segment 0.
    let junk = borsh::to_vec(&program_loader_core::Instruction::WriteSegment { bytecode: vec![0x5a; 64], next_segment: None }).unwrap();
    let r = chain.submit("third party: WriteSegment of junk into the run's segment 0", lee_core::program::PROGRAM_LOADER_ACCOUNT_ID, &[planned[0]], junk, &[], LOADER_GAS_LIMIT, 1_000);
    assert!(r.ok(), "{r:?}");
    let found = deployment(&chain, header("run-1"), &planned);
    println!("run: {found:?}");
    assert!(matches!(found, Deployment::Taken(_)));
    // Fresh accounts: the deployment and Init go through.
    let program = chain.deploy("run-2", FERRATA_V03_ELF);
    assert_eq!(deployment(&chain, program, &(0..n).map(|i| seg("run-2", i)).collect::<Vec<_>>()), Deployment::Ours);
    let bootstrap = mainnet_bootstrap();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let kinds = run_from_state(&mut chain, &mut inst, &mainnet_plan(), &mainnet_updates());
    assert_eq!(kinds, ["init", "update:update_0 (period)", "continue:update_0 (period)", "update:update_1 (finality)"]);
    inst.assert_in_step(&chain);
    write_results("third-party-squat", &chain.reports);
}

/// A third party creates the run's instance first without adopting its committee (216k gas):
/// the run's pre-signed `Init` would be refused (203) and its updates (206). Reading the chain,
/// the run adopts the committee with `Continue` instead of `Init`, then updates.
#[test]
fn an_instance_created_without_its_committee_is_adopted() {
    let mut chain = Chain::new();
    let program = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    let bootstrap = mainnet_bootstrap();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let (g, _) = inst.init(&bootstrap, 0);
    let mut g = g;
    g.label = "third party: Init without the adoption".into();
    let r = inst.send(&mut chain, &g, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    // What a pre-signed Init would get now.
    let (mut p, _) = inst.init(&bootstrap, usize::MAX);
    p.label = "a pre-signed Init after it (refused)".into();
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.refused_with(code::ALREADY_INITIALIZED), "{r:?}");
    let kinds = run_from_state(&mut chain, &mut inst, &mainnet_plan(), &mainnet_updates());
    assert_eq!(kinds, ["adopt-init", "update:update_0 (period)", "continue:update_0 (period)", "update:update_1 (finality)"]);
    inst.assert_in_step(&chain);
    write_results("third-party-init", &chain.reports);
}

/// A third party applies the run's first update itself (a handoff, both transactions): the run's
/// pre-signed update would be stale (52). Reading the chain, the run skips it, rebuilds its light
/// client from the chain's store, and applies the next.
#[test]
fn an_update_someone_else_applied_is_skipped() {
    let mut chain = Chain::new();
    let program = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let (p, _) = inst.init(&bootstrap, usize::MAX);
    assert!(inst.send(&mut chain, &p, GAS_LIMIT).ok());
    // The third party's handoff, with its own light client.
    let mut third = inst.clone();
    let (u, rest, record) = third.update("third party: update_0 (residue)", &updates[0], ferrata_v03_wire::Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
    assert!(third.send(&mut chain, &u, GAS_LIMIT).ok());
    let c = third.continue_with("third party: Continue (adopt, apply)", rest.into_iter().map(Step::Adopt).collect(), record.as_ref(), u.timestamp_ms + 12_000);
    assert!(third.send(&mut chain, &c, GAS_LIMIT).ok());
    let kinds = run_from_state(&mut chain, &mut inst, &mainnet_plan(), &updates);
    assert_eq!(kinds, ["update:update_1 (finality)"]);
    third.follow(&updates[0]);
    third.follow(&as_finality_update(&updates[1]));
    assert_eq!(inst.store(&chain), ferrata_core::witness::CompactStore::of(third.native.core()));
    write_results("third-party-newer", &chain.reports);
}

/// A third party verifies a handoff and leaves it without its adoption: every update is refused
/// (207) until someone adopts. Reading the chain, the run pays for the adoption, then updates.
#[test]
fn a_pending_handoff_is_adopted_by_the_run() {
    let mut chain = Chain::new();
    let program = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let (p, _) = inst.init(&bootstrap, usize::MAX);
    assert!(inst.send(&mut chain, &p, GAS_LIMIT).ok());
    let mut third = inst.clone();
    let (u, _, _) = third.update("third party: update_0 verified, its adoption left undone", &updates[0], ferrata_v03_wire::Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
    assert!(third.send(&mut chain, &u, GAS_LIMIT).ok());
    // A pre-signed update now: refused while the verified job waits.
    let mut early = inst.clone();
    let (q, _, _) = early.update("a pre-signed update_0 now (refused)", &updates[0], ferrata_v03_wire::Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
    let r = early.send(&mut chain, &q, GAS_LIMIT);
    assert!(r.refused_with(code::JOB_PENDING), "{r:?}");
    let kinds = run_from_state(&mut chain, &mut inst, &mainnet_plan(), &updates);
    assert_eq!(kinds, ["adopt-pending", "update:update_1 (finality)"]);
    write_results("third-party-pending", &chain.reports);
}
