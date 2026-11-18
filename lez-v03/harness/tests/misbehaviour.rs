//! `Instruction::Evidence` through LEZ v0.3.1's own block application: an instance frozen on evidence that a sync
//! committee it holds signed finality contradicting its records. EXPERIMENTAL, UNAUDITED (`bench/MISBEHAVIOUR.md`,
//! `bench/misbehaviour`); the deployed programs do not have it.
//!
//! * `contradictions_freeze_and_frozen_refuses`: on a chain that never existed (a 512-member committee the test
//!   holds, a fabricated checkpoint), a recorded finalized header contradicted at its slot (510 and 342 signers, the
//!   residue witness; 510 with the final exponentiation) and an adopted next committee contradicted: each refused as
//!   an `Update`, applied as `Evidence` in one transaction; the frozen instance refuses `Update`, `Continue`,
//!   `Attest` and `Evidence` (`FROZEN`, 222) and keeps its store and records; griefing and replay refused with their
//!   codes, nothing changed.
//! * `honest_mainnet_replay_never_freezes`: the mainnet replay (the bootstrap, then update_0 to update_3, each a
//!   handoff in one transaction), and after each step every one of the five updates (update_0 to update_4) sent as
//!   `Evidence` against the record at its finalized slot (if the instance holds one) and against the adopted next
//!   committee: all refused.
//!
//! `cargo test --release -p harness-v03 --test misbehaviour -- --nocapture --test-threads 1`; every transaction is
//! written to `lez-v03/results/misbehaviour.tsv` and `misbehaviour-honest.tsv`.

use std::collections::BTreeMap;

use ferrata_core::{
    fork::{LcFork, FAR_FUTURE_EPOCH},
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, FINALIZED_ROOT_GINDEX_ELECTRA, NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::CompactUpdate,
    Error,
};
use ferrata_spectests::testkit::{fabricated_header, fake_committee, sign, sparse_tree, FakeCommittee};
use ferrata_v03_wire::{code, Against, Config, Freeze, Instruction, SlotProof, Statement, Step, StorageQuery, Verify};
use harness_v03::*;

/// The first slot of period 10 of the fabricated chain (mainnet preset: 8,192 slots a period).
const BASE: u64 = 10 * 8192;

fn residue() -> Verify {
    Verify::Residue { c: vec![], w: vec![] }
}

fn core_code(e: Error) -> u8 {
    e.code() as u8
}

/// A chain that never existed: mainnet's preset and fork versions, Electra from genesis, its own genesis root.
fn config(chain_id: u64) -> Config {
    Config {
        chain_id,
        sync_committee_size: 512,
        slots_per_epoch: 32,
        epochs_per_sync_committee_period: 256,
        min_sync_committee_participants: 1,
        fork_versions: mainnet_spec().versions,
        fork_epochs: [0, 0, 0, 0, 0, 0, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
        genesis_validators_root: sha256(&[b"ferrata misbehaviour test chain".as_slice()]),
        genesis_time: 1_700_000_000,
        seconds_per_slot: 12,
        store_fork: 3,
        adoption_parts: 1,
        ws_period_epochs: test_ws_period_epochs(),
    }
}

/// A bootstrap at `slot` whose state holds `c` as its current committee.
fn bootstrap(c: &FakeCommittee, slot: u64, tag: u64) -> LightClientBootstrap {
    let mut header = fabricated_header(slot, tag);
    let (state_root, branches) = sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.committee.hash_tree_root())], tag);
    header.beacon.state_root = state_root;
    LightClientBootstrap {
        header,
        current_sync_committee: c.committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    }
}

/// An Electra-layout update whose finalized header (if any) is `finalized`, proven in a fabricated attested state,
/// signed by the members of `signer` whose bit is set.
fn fabricate(
    cfg: &Config,
    finalized: Option<&LightClientHeader>,
    attested_slot: u64,
    signature_slot: u64,
    bits: &[bool],
    next: Option<&SyncCommittee>,
    signer: &FakeCommittee,
) -> LightClientUpdate {
    let n = bits.len();
    let mut attested = fabricated_header(attested_slot, 80 + attested_slot);
    let mut leaves = Vec::new();
    if let Some(h) = finalized {
        leaves.push((FINALIZED_ROOT_GINDEX_ELECTRA, h.beacon.hash_tree_root()));
    }
    if let Some(c) = next {
        leaves.push((NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.hash_tree_root()));
    }
    let (state_root, branches) = sparse_tree(&leaves, 7_000 + attested_slot);
    attested.beacon.state_root = state_root;
    let mut branches = branches.into_iter();
    let finality_branch = if finalized.is_some() { branches.next().unwrap() } else { vec![ZERO; 7] };
    let next_sync_committee_branch = if next.is_some() { branches.next().unwrap() } else { vec![ZERO; 6] };
    let mut update = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: next.cloned().unwrap_or_else(|| SyncCommittee::empty(n)),
        next_sync_committee_branch,
        finalized_header: finalized.cloned().unwrap_or_else(|| LightClientHeader::empty(LcFork::Electra)),
        finality_branch,
        sync_aggregate: SyncAggregate { bits: bits.to_vec(), signature: [0; 96] },
        signature_slot,
    };
    let spec = cfg.spec();
    let ctx = Context { spec: &spec, genesis_validators_root: cfg.genesis_validators_root, bls: &BLS };
    update.sync_aggregate.signature = sign(signer, bits, &sync_committee_signing_root(&update, &ctx));
    update
}

fn bits(present: impl Fn(usize) -> bool) -> Vec<bool> {
    (0..512).map(present).collect()
}

/// 510 of 512 (the last two absent), as mainnet update_1.
fn most() -> Vec<bool> {
    bits(|i| i < 510)
}

/// 342 of 512, every absent index odd (the costliest placement `bench/RESIDUE.md` §9.6 found).
fn fewest() -> Vec<bool> {
    bits(|i| !(i % 2 == 1 && i < 340))
}

struct Fixture {
    cfg: Config,
    current: FakeCommittee,
    next: FakeCommittee,
    rival: FakeCommittee,
    stranger: FakeCommittee,
}

fn fixture() -> Fixture {
    Fixture {
        cfg: config(31_337),
        current: fake_committee(512, 0x61),
        next: fake_committee(512, 0x62),
        rival: fake_committee(512, 0x63),
        stranger: fake_committee(512, 0x64),
    }
}

/// A chain with the program deployed through LEZ's program loader.
fn chain_with_program() -> (Chain, lee::AccountId) {
    let mut chain = Chain::new();
    let program = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    println!("program {program} image {} ({} ELF bytes)", image_id_hex(FERRATA_V03_ID), FERRATA_V03_ELF.len());
    (chain, program)
}

/// A new instance of the fabricated chain (its own chain id, so its own id), initialized in one transaction, then
/// one update in the checkpoint's period that finalizes a header at `BASE + 64` and adopts `f.next` by its summary
/// (one transaction). Returns the instance and the recorded header.
fn advanced(chain: &mut Chain, program: lee::AccountId, f: &Fixture, chain_id: u64, label: &str) -> (Instance, LightClientHeader) {
    let cfg = Config { chain_id, ..f.cfg.clone() };
    let boot = bootstrap(&f.current, BASE, 9_000);
    let mut inst = Instance::new(program, cfg.clone(), boot.header.beacon.hash_tree_root(), &boot);
    let (p, rest) = inst.init(&boot, usize::MAX);
    assert!(rest.is_empty());
    let mut p = p;
    p.label = format!("{label}: Init (512 keys adopted)");
    let r = inst.send(chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{label}: Init: {r:?}");
    let f1 = fabricated_header(BASE + 64, 40 + BASE + 64);
    let u1 = fabricate(&cfg, Some(&f1), BASE + 96, BASE + 97, &most(), Some(&f.next.committee), &f.current);
    let (p, rest, record) = inst.update(
        &format!("{label}: Update (510/512, finalizes slot {}, adopts the next committee by its summary)", f1.beacon.slot),
        &u1,
        Verify::ResidueSummary { c: vec![], w: vec![] },
        Signers::Multiproof,
        false,
    );
    assert!(rest.is_empty() && record.is_some());
    let r = inst.send(chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{label}: u1: {r:?}");
    assert!(inst.follow(&u1).adopted);
    inst.assert_in_step(chain);
    (inst, f1)
}

/// The same update and window as `Update` (a dummy residue witness: step 1 refuses first).
fn as_update(e: &Planned, label: &str) -> Planned {
    let Instruction::Evidence { instance_id, update, signers, window, .. } = &e.instruction else { unreachable!() };
    Planned {
        label: label.into(),
        instruction: Instruction::Update {
            instance_id: *instance_id,
            update: update.clone(),
            signers: signers.clone(),
            verify: Verify::Residue { c: vec![0; 576], w: vec![0; 576] },
            adopt: vec![],
            record: None,
            window: *window,
        },
        accounts: vec![e.accounts[0]],
        timestamp_ms: e.timestamp_ms,
    }
}

fn record_of(h: &LightClientHeader) -> Against {
    Against::Record { slot: h.beacon.slot, root: h.beacon.hash_tree_root() }
}

/// A query and a statement that prove nothing (an attestation that must be refused either way).
fn dummy_attest(inst: &Instance, slot: u64, label: &str, at_ms: u64) -> Planned {
    let query = StorageQuery {
        chain_id: inst.config.chain_id,
        address: [0; 20],
        account_proof: vec![],
        slots: vec![SlotProof { slot: [0; 32], proof: vec![] }],
        header: None,
    };
    let statement = Statement {
        instance_id: inst.id,
        chain_id: inst.config.chain_id,
        record_slot: slot,
        block_number: 0,
        block_hash: [0; 32],
        address: [0; 20],
        slot: [0; 32],
        value: [0; 32],
        id: [0; 32],
        id_at_block: [1; 32],
    };
    inst.attest(label, slot, query, vec![statement], at_ms)
}

#[test]
fn contradictions_freeze_and_frozen_refuses() {
    let f = fixture();
    let (mut chain, program) = chain_with_program();
    let mut reports = Vec::new();
    let mut gas = Vec::new();

    // (A) A recorded finalized header contradicted at its slot, 510 of 512, residue witness.
    let (inst, f1) = advanced(&mut chain, program, &f, 31_337, "instance A");
    let cfg = inst.config.clone();
    let f1x = fabricated_header(f1.beacon.slot, 5_000);
    let u1x = fabricate(&cfg, Some(&f1x), BASE + 100, BASE + 101, &most(), None, &f.current);

    // Griefing and replay first, on the live instance: each refused, nothing changed.
    let state0 = inst.state(&chain).unwrap();
    let mut refusals: Vec<(String, Planned, u8)> = Vec::new();
    let e = inst.evidence("x", &u1x, record_of(&f1), residue());
    refusals.push(("the contradicting update sent as an Update".into(), as_update(&e, "x"), core_code(Error::NotNewer)));
    let fake_root = Against::Record { slot: f1.beacon.slot, root: [7; 32] };
    refusals.push((
        "evidence claiming a root the record does not hold".into(),
        inst.evidence("x", &u1x, fake_root, residue()),
        code::WRONG_RECORD,
    ));
    let f9 = fabricated_header(BASE + 65, 6_000);
    let u9 = fabricate(&cfg, Some(&f9), BASE + 100, BASE + 101, &most(), None, &f.current);
    refusals.push((
        "evidence against a slot the instance holds no record of".into(),
        inst.evidence("x", &u9, Against::Record { slot: f9.beacon.slot, root: [7; 32] }, residue()),
        code::NO_RECORD,
    ));
    let u1 = fabricate(&cfg, Some(&f1), BASE + 96, BASE + 97, &most(), Some(&f.next.committee), &f.current);
    refusals.push((
        "the update that wrote the record, replayed against it".into(),
        inst.evidence("x", &u1, record_of(&f1), Verify::Core),
        85,
    ));
    refusals.push((
        "the same, against the adopted next committee".into(),
        inst.evidence("x", &u1, Against::NextCommittee, Verify::Core),
        85,
    ));
    let ahead = fabricate(&cfg, Some(&f1x), BASE + 2 * 8192 + 2, BASE + 2 * 8192 + 3, &most(), None, &f.current);
    refusals.push((
        "signed two periods ahead (no committee held)".into(),
        inst.evidence("x", &ahead, record_of(&f1), Verify::Core),
        core_code(Error::PeriodSkipped),
    ));
    let stranger = fabricate(&cfg, Some(&f1x), BASE + 100, BASE + 101, &most(), None, &f.stranger);
    refusals.push((
        "the right period, keys the instance does not hold".into(),
        inst.evidence("x", &stranger, record_of(&f1), Verify::Core),
        core_code(Error::SignatureInvalid),
    ));
    let unfinalized = fabricate(&cfg, None, BASE + 100, BASE + 101, &most(), None, &f.current);
    refusals.push((
        "no finality branch (an attested head only)".into(),
        inst.evidence("x", &unfinalized, record_of(&f1), Verify::Core),
        core_code(Error::NotFinalized),
    ));
    let below = fabricate(&cfg, Some(&f1x), BASE + 100, BASE + 101, &bits(|i| i < 341), None, &f.current);
    refusals.push((
        "341 of 512 signers".into(),
        inst.evidence("x", &below, record_of(&f1), Verify::Core),
        core_code(Error::BelowSupermajority),
    ));
    let mut open = inst.evidence("x", &u1x, record_of(&f1), Verify::Open);
    open.label = "x".into();
    refusals.push(("Verify::Open (not one transaction)".into(), open, code::EVIDENCE_FORM));
    let mut unmarked = inst.evidence("x", &u1x, record_of(&f1), residue());
    if let Instruction::Evidence { signers, .. } = &mut unmarked.instruction {
        signers.drain(..2);
    }
    refusals.push(("a signer witness that is not one multiproof".into(), unmarked, code::EVIDENCE_FORM));
    let mut flipped = inst.evidence("x", &u1x, record_of(&f1), residue());
    if let Instruction::Evidence { verify: Verify::Residue { c, .. }, .. } = &mut flipped.instruction {
        c[100] ^= 1;
    }
    refusals.push(("the residue witness with one bit of c flipped".into(), flipped, core_code(Error::SignatureInvalid)));
    let mut window = inst.evidence("x", &u1x, record_of(&f1), Verify::Core);
    if let Instruction::Evidence { window, .. } = &mut window.instruction {
        window.to_ms = None;
    }
    refusals.push(("a window other than an update's".into(), window, code::WRONG_WINDOW));
    let mut wrong_account = inst.evidence("x", &u1x, record_of(&f1), Verify::Core);
    wrong_account.accounts[1] = inst.header_account(BASE);
    refusals.push(("the record account of another slot".into(), wrong_account, code::WRONG_ACCOUNT));
    for (label, mut p, want) in refusals {
        p.label = format!("refusal: instance A: {label}");
        let r = inst.send(&mut chain, &p, GAS_LIMIT);
        assert!(r.refused_with(want), "{label}: want {want}, got {r:?}");
        assert_eq!(inst.state(&chain).unwrap(), state0, "{label}: nothing changed");
        println!("refused ({want}): {label}: {} execution gas", r.exec_gas);
        reports.push(r);
    }

    // The evidence itself.
    let e = inst.evidence(
        &format!("Evidence (A): record at slot {} contradicted, 510/512, residue witness", f1.beacon.slot),
        &u1x,
        record_of(&f1),
        residue(),
    );
    let r = inst.send(&mut chain, &e, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    let state = inst.state(&chain).unwrap();
    let want = Freeze {
        kind: 1,
        at: f1.beacon.slot,
        held: f1.beacon.hash_tree_root(),
        conflicting: f1x.beacon.hash_tree_root(),
        attested_root: u1x.attested_header.beacon.hash_tree_root(),
        signature_slot: u1x.signature_slot,
        update_digest: CompactUpdate::of(&u1x).digest(LcFork::Electra),
    };
    assert_eq!(state.frozen.as_ref(), Some(&want));
    assert_eq!((state.store.clone(), state.applied, state.active), (state0.store.clone(), state0.applied, true));
    let record_before = inst.record(&chain, f1.beacon.slot).unwrap();
    gas.push(("(A) 510/512, residue witness", r.exec_gas));
    reports.push(r);

    // The frozen instance.
    let f2 = fabricated_header(BASE + 128, 40 + BASE + 128);
    let u2 = fabricate(&cfg, Some(&f2), BASE + 160, BASE + 161, &bits(|_| true), None, &f.current);
    let (p, _, _) = inst.update("refusal: frozen: Update (512/512)", &u2, residue(), Signers::Multiproof, true);
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.refused_with(code::FROZEN), "{r:?}");
    reports.push(r);
    let c = inst.continue_with("refusal: frozen: Continue", vec![Step::Miller], None, chain.last_timestamp + 1000);
    let r = inst.send(&mut chain, &c, GAS_LIMIT);
    assert!(r.refused_with(code::FROZEN), "{r:?}");
    reports.push(r);
    let a = dummy_attest(&inst, f1.beacon.slot, "refusal: frozen: Attest under the record", chain.last_timestamp + 1000);
    let r = inst.send(&mut chain, &a, GAS_LIMIT);
    assert!(r.refused_with(code::FROZEN), "{r:?}");
    reports.push(r);
    let mut again = inst.evidence("refusal: frozen: the same Evidence again", &u1x, record_of(&f1), residue());
    again.timestamp_ms = chain.last_timestamp + 1000;
    let r = inst.send(&mut chain, &again, GAS_LIMIT);
    assert!(r.refused_with(code::FROZEN), "{r:?}");
    reports.push(r);
    let after = inst.state(&chain).unwrap();
    assert_eq!(after, state, "a frozen instance keeps its state");
    assert_eq!(inst.record(&chain, f1.beacon.slot).unwrap(), record_before, "and its records");
    assert!(inst.record(&chain, BASE).is_some(), "the checkpoint's record too");

    // (A) with the final exponentiation (the core's step 3), 510/512.
    let (inst_c, f1) = advanced(&mut chain, program, &f, 31_338, "instance A-core");
    let u1x = fabricate(&inst_c.config, Some(&f1x), BASE + 100, BASE + 101, &most(), None, &f.current);
    let e = inst_c.evidence("Evidence (A): 510/512, final exponentiation", &u1x, record_of(&f1), Verify::Core);
    let r = inst_c.send(&mut chain, &e, GAS_LIMIT);
    println!("(A) 510/512 with the final exponentiation: {r:?}");
    gas.push(("(A) 510/512, final exponentiation", r.exec_gas));
    let core_ok = r.ok();
    reports.push(r);

    // (A) at the fewest signers the policy accepts (342 of 512, every absent index odd), residue witness.
    let (inst_f, f1) = advanced(&mut chain, program, &f, 31_339, "instance A-342");
    let u1x = fabricate(&inst_f.config, Some(&f1x), BASE + 100, BASE + 101, &fewest(), None, &f.current);
    assert_eq!(u1x.sync_aggregate.participants(), 342);
    let e = inst_f.evidence("Evidence (A): 342/512 (every absent index odd), residue witness", &u1x, record_of(&f1), residue());
    let r = inst_f.send(&mut chain, &e, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    assert_eq!(inst_f.state(&chain).unwrap().frozen.map(|z| z.kind), Some(1));
    gas.push(("(A) 342/512, residue witness", r.exec_gas));
    reports.push(r);

    // (B) The adopted next committee contradicted, 510/512, residue witness.
    let (inst_b, _) = advanced(&mut chain, program, &f, 31_340, "instance B");
    let live = dummy_attest(&inst_b, BASE + 64, "refusal: live instance B: the same dummy Attest (not refused as frozen)", chain.last_timestamp + 1000);
    let r = inst_b.send(&mut chain, &live, GAS_LIMIT);
    assert!(!r.ok() && !r.refused_with(code::FROZEN), "{r:?}");
    println!("the dummy Attest on a live instance: exit {:?}", r.exit);
    reports.push(r);
    let f3 = fabricated_header(BASE + 128, 40 + BASE + 128);
    let u3 = fabricate(&inst_b.config, Some(&f3), BASE + 160, BASE + 161, &most(), Some(&f.rival.committee), &f.current);
    let e = inst_b.evidence("Evidence (B): the adopted next committee contradicted, 510/512, residue witness", &u3, Against::NextCommittee, residue());
    let r = inst_b.send(&mut chain, &as_update(&e, "refusal: instance B: the contradicting update sent as an Update"), GAS_LIMIT);
    assert!(r.refused_with(core_code(Error::NextCommitteeMismatch)), "{r:?}");
    reports.push(r);
    let r = inst_b.send(&mut chain, &e, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    let frozen = inst_b.state(&chain).unwrap().frozen.unwrap();
    assert_eq!(
        (frozen.kind, frozen.at, frozen.held, frozen.conflicting),
        (2, 10, f.next.committee.hash_tree_root(), f.rival.committee.hash_tree_root())
    );
    gas.push(("(B) 510/512, residue witness", r.exec_gas));
    reports.push(r);

    write_results("misbehaviour", &reports);
    for (label, g) in &gas {
        println!("ONE-TX evidence {label}: {g} execution gas ({:.1}% of MAX_GAS_EXEC)", *g as f64 / 1e5);
    }
    println!("the final exponentiation path applied: {core_ok}");
}

#[test]
fn honest_mainnet_replay_never_freezes() {
    let (mut chain, program) = chain_with_program();
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    let (p, rest) = inst.init(&bootstrap, usize::MAX);
    assert!(rest.is_empty());
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.ok(), "Init: {r:?}");
    let mut reports = vec![r];
    let mut tally: BTreeMap<u8, usize> = BTreeMap::new();
    let mut submit_all = |chain: &mut Chain, inst: &Instance, at: &str, reports: &mut Vec<TxReport>| {
        for (j, u) in updates.iter().enumerate() {
            let slot = u.finalized_header.beacon.slot;
            let mut against = vec![Against::NextCommittee];
            if let Some(rec) = inst.record(chain, slot) {
                let held = ferrata_core::witness::HeaderRecord::decode(&rec.record).unwrap();
                against.push(Against::Record { slot, root: held.beacon.hash_tree_root() });
            }
            for a in against {
                let mut p = inst.evidence(&format!("honest: {at}: update_{j} as Evidence against {a:?}"), u, a, Verify::Core);
                p.timestamp_ms = p.timestamp_ms.max(chain.last_timestamp + 1);
                let r = inst.send(chain, &p, GAS_LIMIT);
                assert!(!r.ok(), "{}: an honest update froze the instance: {r:?}", p.label);
                *tally.entry(r.exit.map_or(0, |e| e as u8)).or_default() += 1;
                reports.push(r);
            }
        }
        assert!(inst.state(chain).unwrap().frozen.is_none());
    };
    submit_all(&mut chain, &inst, "after Init", &mut reports);
    // update_0 to update_3, each a handoff in one transaction (as `v03.rs`, `handoffs_in_one_transaction_each`).
    for (i, u) in updates.iter().enumerate().take(4) {
        let (p, rest, _) = inst.update(&format!("update_{i}: Update (one transaction)"), u, Verify::ResidueSummary { c: vec![], w: vec![] }, Signers::Multiproof, false);
        assert!(rest.is_empty());
        let r = inst.send(&mut chain, &p, GAS_LIMIT);
        assert!(r.ok(), "update_{i}: {r:?}");
        reports.push(r);
        inst.follow(u);
        inst.assert_in_step(&chain);
        submit_all(&mut chain, &inst, &format!("after update_{i}"), &mut reports);
    }
    write_results("misbehaviour-honest", &reports);
    println!("honest mainnet replay as evidence, refusals by exit code: {tally:?}");
}
