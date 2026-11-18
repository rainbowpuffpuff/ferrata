//! Written by an independent review (review-misbehaviour, 18 Nov 2026) and kept here as written, with plain labels and
//! the transactions written to `lez-v03/results/misbehaviour-window.tsv`. EXPERIMENTAL, UNAUDITED
//! (`bench/MISBEHAVIOUR.md` §5). Through LEZ v0.3.1's block application, on the fabricated chain of `misbehaviour.rs`:
//!
//! * `evidence_front_run_by_an_update`: evidence planned against the instance's store, then any `Update` that
//!   advances the store lands first: the evidence's window no longer matches (218); re-planned, it freezes.
//! * `evidence_expires_when_the_instance_rotates`: a conflict signed by the committee of period 10; the instance
//!   rotates into period 11 by an ordinary handoff: the same conflict is refused (23) for good, the contradicted
//!   record stays, the instance is not frozen.

use ferrata_core::{
    fork::{LcFork, FAR_FUTURE_EPOCH},
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, FINALIZED_ROOT_GINDEX_ELECTRA, NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    Error,
};
use ferrata_spectests::testkit::{fabricated_header, fake_committee, sign, sparse_tree, FakeCommittee};
use ferrata_v03_wire::{code, Against, Config, Verify};
use harness_v03::*;

const BASE: u64 = 10 * 8192;
const LEN: u64 = 8192;

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

fn bootstrap(c: &FakeCommittee, slot: u64, tag: u64) -> LightClientBootstrap {
    let mut header = fabricated_header(slot, tag);
    let (state_root, branches) = sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.committee.hash_tree_root())], tag);
    header.beacon.state_root = state_root;
    LightClientBootstrap { header, current_sync_committee: c.committee.clone(), current_sync_committee_branch: branches[0].clone() }
}

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

fn most() -> Vec<bool> {
    (0..512).map(|i| i < 510).collect()
}

struct Fixture {
    cfg: Config,
    current: FakeCommittee,
    next: FakeCommittee,
    rival: FakeCommittee,
}

fn fixture() -> Fixture {
    Fixture {
        cfg: config(31_337),
        current: fake_committee(512, 0x61),
        next: fake_committee(512, 0x62),
        rival: fake_committee(512, 0x63),
    }
}

fn chain_with_program() -> (Chain, lee::AccountId) {
    let mut chain = Chain::new();
    let program = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    println!("program {program} image {} ({} ELF bytes)", image_id_hex(FERRATA_V03_ID), FERRATA_V03_ELF.len());
    (chain, program)
}

fn advanced(chain: &mut Chain, program: lee::AccountId, f: &Fixture, chain_id: u64, label: &str) -> (Instance, LightClientHeader) {
    let cfg = Config { chain_id, ..f.cfg.clone() };
    let boot = bootstrap(&f.current, BASE, 9_000);
    let mut inst = Instance::new(program, cfg.clone(), boot.header.beacon.hash_tree_root(), &boot);
    let (mut p, rest) = inst.init(&boot, usize::MAX);
    assert!(rest.is_empty());
    p.label = format!("{label}: Init");
    let r = inst.send(chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{label}: Init: {r:?}");
    let f1 = fabricated_header(BASE + 64, 40 + BASE + 64);
    let u1 = fabricate(&cfg, Some(&f1), BASE + 96, BASE + 97, &most(), Some(&f.next.committee), &f.current);
    let (p, rest, record) =
        inst.update(&format!("{label}: Update u1"), &u1, Verify::ResidueSummary { c: vec![], w: vec![] }, Signers::Multiproof, false);
    assert!(rest.is_empty() && record.is_some());
    let r = inst.send(chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{label}: u1: {r:?}");
    assert!(inst.follow(&u1).adopted);
    inst.assert_in_step(chain);
    (inst, f1)
}

fn record_of(h: &LightClientHeader) -> Against {
    Against::Record { slot: h.beacon.slot, root: h.beacon.hash_tree_root() }
}

fn residue() -> Verify {
    Verify::Residue { c: vec![], w: vec![] }
}

#[test]
fn evidence_front_run_by_an_update() {
    let f = fixture();
    let (mut chain, program) = chain_with_program();
    let (mut inst, f1) = advanced(&mut chain, program, &f, 41_001, "front-run");
    let f1x = fabricated_header(f1.beacon.slot, 5_000);
    let conflict = fabricate(&inst.config, Some(&f1x), BASE + 200, BASE + 201, &most(), None, &f.current);
    let planned = inst.evidence("front-run: Evidence (A), planned on the store at slot 81984", &conflict, record_of(&f1), residue());

    // Anyone's ordinary update lands first: it finalizes a later header of the same period.
    let f2 = fabricated_header(BASE + 256, 40 + BASE + 256);
    let u2 = fabricate(&inst.config, Some(&f2), BASE + 288, BASE + 289, &most(), None, &f.current);
    let (p, rest, record) = inst.update("front-run: an ordinary Update (finalizes slot 82176)", &u2, residue(), Signers::Multiproof, false);
    assert!(rest.is_empty() && record.is_some());
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.ok(), "u2: {r:?}");
    inst.follow(&u2);
    let before = inst.state(&chain).unwrap();

    let mut reports = vec![r];
    let r = inst.send(&mut chain, &planned, GAS_LIMIT);
    println!("{}", r.tsv());
    assert!(r.refused_with(code::WRONG_WINDOW), "the evidence planned before the update: {r:?}");
    reports.push(r.clone());
    assert_eq!(inst.state(&chain).unwrap(), before, "refused: nothing changed");

    // Re-planned on the new store: the committee is still held, the record still there.
    let again = inst.evidence("front-run: Evidence (A), re-planned on the store at slot 82176", &conflict, record_of(&f1), residue());
    let r = inst.send(&mut chain, &again, GAS_LIMIT);
    println!("{}", r.tsv());
    assert!(r.ok(), "re-planned evidence: {r:?}");
    assert!(inst.state(&chain).unwrap().frozen.is_some());
    reports.push(r);
    write_results("misbehaviour-window-front-run", &reports);
    println!("front-run: evidence planned before an advancing Update refused {} (WRONG_WINDOW); re-planned: applied, frozen", code::WRONG_WINDOW);
}

#[test]
fn evidence_expires_when_the_instance_rotates() {
    let f = fixture();
    let (mut chain, program) = chain_with_program();
    let (mut inst, f1) = advanced(&mut chain, program, &f, 41_002, "expiry");
    let f1x = fabricated_header(f1.beacon.slot, 5_001);
    // The conflict, signed by the committee of period 10 (two thirds and more of it) in period 10.
    let conflict = fabricate(&inst.config, Some(&f1x), BASE + 200, BASE + 201, &most(), None, &f.current);

    // An ordinary handoff into period 11, signed by the held next committee, adopting another next committee.
    let g = fabricated_header(BASE + LEN + 64, 40 + BASE + LEN + 64);
    let u3 = fabricate(&inst.config, Some(&g), BASE + LEN + 96, BASE + LEN + 97, &most(), Some(&f.rival.committee), &f.next);
    let (p, rest, record) =
        inst.update("expiry: handoff into period 11", &u3, Verify::ResidueSummary { c: vec![], w: vec![] }, Signers::Multiproof, false);
    assert!(rest.is_empty() && record.is_some());
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.ok(), "u3: {r:?}");
    inst.follow(&u3);
    let before = inst.state(&chain).unwrap();

    // The same conflict now (`Verify::Core`: the harness's native pre-check would refuse it first).
    let e = inst.evidence("expiry: Evidence (A) signed in period 10, after the rotation", &conflict, record_of(&f1), Verify::Core);
    let mut reports = vec![r];
    let r = inst.send(&mut chain, &e, GAS_LIMIT);
    println!("{}", r.tsv());
    assert!(r.refused_with(Error::PeriodSkipped.code() as u8), "a conflict signed by a committee no longer held: {r:?}");
    reports.push(r.clone());
    write_results("misbehaviour-window-expiry", &reports);
    assert_eq!(inst.state(&chain).unwrap(), before, "refused: nothing changed");
    assert!(before.frozen.is_none());
    assert!(inst.record(&chain, f1.beacon.slot).is_some(), "the contradicted record stays");
    println!("expiry: after the rotation the period-10 conflict is refused {} for good; the instance live, the record kept", Error::PeriodSkipped.code());
}
