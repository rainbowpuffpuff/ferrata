//! EXPERIMENTAL, bench-only: where the absent members sit, through LEZ's state machine. The
//! experimental copy of the program (`guests/program-residue-experimental`, deployed here at a named
//! account of its own and nowhere else) takes the 342-signer finality update of
//! `residue_experimental.rs` in one transaction with the non-signers as one multiproof, at
//! placements of the 170 absent members other than every third (`bench/RESIDUE.md` §9.6; the bare
//! guest's counterpart is `bench/residue-host/tests/placements.rs`).
//!
//! * `a_342_signer_update_at_other_placements`: 12 placements summed by blst's batch addition and
//!   two by the chain of additions, each on a fresh instance whose store holds the bench's
//!   committee; every one must be applied and leave the natively applied job's store. The gas is
//!   printed, not asserted: every third member absent needs the most hashes and witness nodes but
//!   is not the costliest placement, and the maximum over all placements is not established.
//! * `what_the_copy_refuses`: 341 signers under either marker and with the core's witness (exit
//!   51, `BelowSupermajority`); the multiproof witness without its marker, the core's witness
//!   behind a marker, the marker twice; and no two-byte prefix other than the markers makes the
//!   multiproof witness a valid `SignerWitness`. Nothing changes in the store.
//!
//! From an independent review of the composition (18 Nov 2026); the helpers are those of
//! `residue_experimental.rs`. Behind the feature `residue-experimental`, as that test (no ordinary
//! build of the harness builds the copy):
//! `RISC0_DEV_MODE=1 cargo test -p harness --features residue-experimental --test residue_placements -- --nocapture --test-threads 1`
#![cfg(feature = "residue-experimental")]

use std::borrow::Cow;

use ferrata_core::{
    fork::LcFork,
    policy::Adoption,
    steps::{self, Phase},
    types::{LightClientUpdate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
};
use ferrata_lez_wire::{header_actor, InstanceState, Message};
use ferrata_spectests::testkit::{fake_committee, sign, FakeCommittee};
use harness::{scenarios::World, *};
use lee::{program::Program, AccountId, Actor, V03State};
use lee_core::account::ActorState;
use residue_agg::{NonSigners, Sum};

/// Where the experimental copy is deployed in this test.
const EXPERIMENTAL: AccountId = AccountId::new([0x5e; 32]);

fn experimental_program() -> Program {
    Program::new_unchecked(
        methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ID,
        Cow::Borrowed(methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ELF),
    )
}

fn outcome(r: &TxReport) -> String {
    match (&r.charge.revert, r.exit_code) {
        (None, _) => "applied".to_string(),
        (Some(_), Some(code)) => format!("refused: exit {code}"),
        (Some(e), None) => format!("reverted: {e}"),
    }
}

fn world_after_update_0() -> World {
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    chain.state = state.with_named_programs([(EXPERIMENTAL, experimental_program())]);
    let config = mainnet_config(2);
    let checkpoint = root(TRUSTED_BLOCK_ROOT);
    let bootstrap = mainnet_bootstrap();
    let spec = config.spec();
    let inst = Instance::new(EXPERIMENTAL, config, checkpoint);
    let native = ferrata_core::policy::FinalizedStore::initialize_with(
        &checkpoint,
        &bootstrap,
        inst.fork(),
        &spec,
        &scenarios::BLS,
        Adoption::OnChain,
    )
    .unwrap();
    let run = inst.init(&mut chain, &bootstrap, "Init");
    assert!(run.reports.iter().all(TxReport::ok));
    let mut w = World { chain, inst, native, spec, bootstrap, updates: mainnet_updates(), runs: vec![run] };
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    w.check_store();
    w
}

fn signing_committee<'a>(
    full: &'a ferrata_core::store::StoreCore,
    update: &LightClientUpdate,
    spec: &ferrata_core::fork::ChainSpec,
) -> &'a SyncCommittee {
    if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    }
}

/// The instance after update_0 with the bench's committee written into its store in place of the
/// signing committee (LEZ's `force_insert_account`), as `residue_experimental.rs`.
fn world_with_the_bench_committee() -> (World, FakeCommittee, CompactStore) {
    let mut w = world_after_update_0();
    let update = as_finality_update(&w.updates[1]);
    let store = w.inst.store(&w.chain);
    let real = signing_committee(w.native.core(), &update, &w.spec).clone();
    let fake = fake_committee(512, 7);
    let mut substituted = store.clone();
    if substituted.current == CommitteeSummary::of(&real) {
        substituted.current = CommitteeSummary::of(&fake.committee);
    } else {
        substituted.next = Some(CommitteeSummary::of(&fake.committee));
    }
    let actor = w.inst.actor();
    let mut account = w.chain.state.get_account_by_id(actor.account_id);
    let mut state: InstanceState = borsh::from_slice(account.data.actor_state(actor.program_account_id)).unwrap();
    state.store = substituted.encode();
    account.data.set_actor_state(actor.program_account_id, ActorState::from(borsh::to_vec(&state).unwrap()));
    w.chain.state.force_insert_account(actor.account_id, account);
    assert_eq!(w.inst.store(&w.chain), substituted);
    (w, fake, substituted)
}

fn signed_by(w: &World, fake: &FakeCommittee, bits: Vec<bool>) -> LightClientUpdate {
    let update = as_finality_update(&w.updates[1]);
    let gvr = w.inst.config.genesis_validators_root;
    let ctx = Context { spec: &w.spec, genesis_validators_root: gvr, bls: &scenarios::BLS };
    let mut u = update.clone();
    u.sync_aggregate.signature = sign(fake, &bits, &sync_committee_signing_root(&update, &ctx));
    u.sync_aggregate.bits = bits;
    u
}

fn verify_all_message(w: &World, update: &LightClientUpdate, signers: Vec<u8>, c: &[u8], wit: &[u8]) -> Vec<u8> {
    let mut s = signers;
    s.extend_from_slice(c);
    s.extend_from_slice(wit);
    borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: w.inst.store(&w.chain).encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: s,
        adoption: vec![],
    })
    .unwrap()
}

/// The marked encoding the copy reads: 0xffff (blst's batch addition) or 0xfffe (the chain of
/// additions), then the `NonSigners` witness.
fn marked(n: &NonSigners, sum: Sum) -> Vec<u8> {
    let mut s = match sum {
        Sum::Batch => vec![0xff, 0xff],
        Sum::Chain => vec![0xfe, 0xff],
    };
    s.extend_from_slice(&n.encode());
    s
}

#[allow(clippy::type_complexity)]
fn native_job(
    w: &World,
    store: &CompactStore,
    update: &LightClientUpdate,
    committee: &SyncCommittee,
) -> (NonSigners, SignerWitness, Vec<u8>, Vec<u8>, CompactStore) {
    let gvr = w.inst.config.genesis_validators_root;
    let bits = &update.sync_aggregate.bits;
    let new = NonSigners::build(committee, bits).unwrap();
    let old = SignerWitness::build(committee, bits).unwrap();
    let cu = CompactUpdate::of(update);
    let mut job = residue_agg::step1(store, &cu, &new, update.signature_slot, &w.spec, &gvr, 1, Sum::Batch).unwrap();
    let core_job = steps::step1(store, &cu, &old, update.signature_slot, &w.spec, &gvr, 1).unwrap();
    assert_eq!(job, core_job);
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!() };
    let f = residue::decode_fp12(&product).unwrap();
    let (c, wit) = residue::witness(&f);
    job.step3().unwrap();
    let (expected, _) = steps::apply(store, &job, &w.spec).unwrap();
    (new, old, residue::encode_fp12(&c).to_vec(), residue::encode_fp12(&wit).to_vec(), expected)
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn random_bits(absent: usize, seed: u64) -> Vec<bool> {
    let mut idx: Vec<usize> = (0..512).collect();
    let mut s = seed;
    for i in (1..512).rev() {
        let j = (splitmix(&mut s) % (i as u64 + 1)) as usize;
        idx.swap(i, j);
    }
    let mut bits = vec![true; 512];
    for &i in &idx[..absent] {
        bits[i] = false;
    }
    bits
}

fn from_absent(abs: &[usize]) -> Vec<bool> {
    let mut bits = vec![true; 512];
    for &i in abs {
        bits[i] = false;
    }
    bits
}

fn every_third() -> Vec<bool> {
    (0..512).map(|i| i % 3 != 2).collect()
}

/// In each of the 128 groups of four members: member `4g + one` absent, and in groups 0..41 also
/// member `4g + other` (170 absent, 425 hashes and 256 witness nodes, as every third).
fn groups(one: usize, other: usize) -> Vec<bool> {
    let abs: Vec<usize> =
        (0..128).flat_map(|g| if g < 42 { vec![4 * g + one, 4 * g + other] } else { vec![4 * g + one] }).collect();
    from_absent(&abs)
}

#[test]
fn a_342_signer_update_at_other_placements() {
    let fake = fake_committee(512, 7);
    let odd = "every absent index odd: 4g+3 in every group of four, and 4g+1 in groups 0..41";
    let mut placements: Vec<(String, Vec<bool>, Sum)> = vec![
        ("every third (2, 5, ..., 509)".into(), every_third(), Sum::Batch),
        (odd.into(), groups(3, 1), Sum::Batch),
        (
            "every absent index even: 4g in every group of four, and 4g+2 in groups 0..41".into(),
            groups(0, 2),
            Sum::Batch,
        ),
        (
            "evenly spaced (the boundary's placement)".into(),
            (0..512).map(|i| !(0..170).any(|j| j * 512 / 170 == i)).collect(),
            Sum::Batch,
        ),
        (
            "two per group of four (4g, 4g+2) in groups 0..41, one (4g+1) in the others".into(),
            from_absent(
                &(0..128)
                    .flat_map(|g| if g < 42 { vec![4 * g, 4 * g + 2] } else { vec![4 * g + 1] })
                    .collect::<Vec<_>>(),
            ),
            Sum::Batch,
        ),
        ("first 170 absent (one block)".into(), (0..512).map(|i| i >= 170).collect(), Sum::Batch),
        ("last 170 absent (one block)".into(), (0..512).map(|i| i < 342).collect(), Sum::Batch),
    ];
    for seed in [1u64, 11, 12, 13, 14] {
        placements.push((format!("random 170, seed {seed}"), random_bits(170, seed), Sum::Batch));
    }
    assert_eq!(placements.len(), 12);
    placements.push(("every third (2, 5, ..., 509)".into(), every_third(), Sum::Chain));
    placements.push((odd.into(), groups(3, 1), Sum::Chain));

    println!("| placement of the 170 absent members (342 of 512 sign) | sum | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | minus every third | witness bytes (with the marker) | result |");
    println!("|---|---|---|---:|---:|---:|---:|---|");
    // (every third, the most, its label, the least, its label) for each sum.
    let mut batch = (0u64, 0u64, String::new(), u64::MAX, String::new());
    let mut chain_every_third = 0u64;
    for (label, bits, sum) in placements {
        // Each placement on a fresh instance (the store moves on when an update is applied).
        let (mut w, _, substituted) = world_with_the_bench_committee();
        let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
        let u = signed_by(&w, &fake, bits);
        assert_eq!(u.sync_aggregate.participants(), 342);
        let (new, _, c, wit, expected) = native_job(&w, &substituted, &u, &fake.committee);
        let header = header_actor(w.inst.program, &w.inst.id, u.finalized_header.beacon.slot);
        let time = w.inst.config.slot_start_ms(u.signature_slot) + 12_000;
        let signers = marked(&new, sum);
        let bytes = signers.len();
        let msg = verify_all_message(&w, &u, signers, &c, &wit);
        let report = w.chain.submit(&label, verifier, msg, vec![verifier, w.inst.actor(), header], vec![], time);
        assert!(report.ok(), "{label}: {:?}", report.charge.revert);
        assert_eq!(w.inst.store(&w.chain), expected, "{label}: the store is the natively applied job's");
        let gas = report.charge.gas_exec;
        let handlers: Vec<String> = report.transitions.iter().map(|t| t.cycles.to_string()).collect();
        let base = match sum {
            Sum::Batch => {
                if batch.0 == 0 {
                    batch.0 = gas;
                }
                if gas > batch.1 {
                    (batch.1, batch.2) = (gas, label.clone());
                }
                if gas < batch.3 {
                    (batch.3, batch.4) = (gas, label.clone());
                }
                batch.0
            }
            Sum::Chain => {
                if chain_every_third == 0 {
                    chain_every_third = gas;
                }
                chain_every_third
            }
        };
        println!(
            "| {label} | {} | {} | {gas} | {:.1}% | {} | {bytes} | {} |",
            match sum {
                Sum::Batch => "batch",
                Sum::Chain => "chain",
            },
            handlers.join(" → "),
            gas as f64 / 100_000.0,
            gas as i64 - base as i64,
            outcome(&report)
        );
    }
    println!(
        "342 of 512 at 12 placements, blst's batch sum: every third {} execution gas; the most {} ({:.1}% of MAX_GAS_EXEC, {} more than every third, {} below the limit): \"{}\"; the least {}: \"{}\". The maximum over all placements and committees is not established.",
        batch.0,
        batch.1,
        batch.1 as f64 / 100_000.0,
        batch.1 as i64 - batch.0 as i64,
        10_000_000 - batch.1,
        batch.2,
        batch.3,
        batch.4
    );
}

#[test]
fn what_the_copy_refuses() {
    let (mut w, fake, substituted) = world_with_the_bench_committee();
    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);

    // 171 absent (341 signers): refused (BelowSupermajority, 51) under either marker and with the
    // core's witness; nothing changed.
    let bits: Vec<bool> = (0..512).map(|i| i % 3 != 2 && i != 0).collect();
    let u = signed_by(&w, &fake, bits.clone());
    assert_eq!(u.sync_aggregate.participants(), 341);
    let new = NonSigners::build(&fake.committee, &bits).unwrap();
    let old = SignerWitness::build(&fake.committee, &bits).unwrap();
    let dummy = vec![0u8; 576];
    let header = header_actor(w.inst.program, &w.inst.id, u.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(u.signature_slot) + 12_000;
    for (label, signers) in [
        ("341 signers, one multiproof behind 0xffff", marked(&new, Sum::Batch)),
        ("341 signers, one multiproof behind 0xfffe", marked(&new, Sum::Chain)),
        ("341 signers, the core's witness", old.encode()),
    ] {
        let msg = verify_all_message(&w, &u, signers, &dummy, &dummy);
        let r = w.chain.submit(label, verifier, msg, vec![verifier, w.inst.actor(), header], vec![], time);
        println!("{label}: {}", outcome(&r));
        assert_eq!(r.exit_code, Some(51), "{label}");
        assert_eq!(w.inst.store(&w.chain), substituted);
    }

    // The two witness formats at 342 (every third absent): the multiproof witness without its
    // marker, the core's witness behind a marker, the marker twice: all refused, nothing changed.
    let u = signed_by(&w, &fake, every_third());
    let (new, old, c, wit, _) = native_job(&w, &substituted, &u, &fake.committee);
    let header = header_actor(w.inst.program, &w.inst.id, u.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(u.signature_slot) + 12_000;
    let mut core_marked = vec![0xff, 0xff];
    core_marked.extend_from_slice(&old.encode());
    let mut twice = vec![0xff, 0xff];
    twice.extend_from_slice(&marked(&new, Sum::Batch));
    for (label, signers) in [
        ("342 signers, the multiproof witness without its marker", new.encode()),
        ("342 signers, the core's witness behind 0xffff", core_marked),
        ("342 signers, the marker twice", twice),
    ] {
        let msg = verify_all_message(&w, &u, signers, &c, &wit);
        let r = w.chain.submit(label, verifier, msg, vec![verifier, w.inst.actor(), header], vec![], time);
        println!("{label}: {}", outcome(&r));
        assert!(!r.ok(), "{label}");
        assert_eq!(w.inst.store(&w.chain), substituted);
    }

    // Every two-byte prefix other than the two markers, before the honest multiproof witness: read
    // as a `SignerWitness` (the decode alone, no transaction), none is valid.
    let enc = new.encode();
    let mut decoded = 0;
    for p in 0u16..=0xffff {
        if p == 0xffff || p == 0xfffe {
            continue;
        }
        let mut b = p.to_le_bytes().to_vec();
        b.extend_from_slice(&enc);
        if SignerWitness::decode(&b, 512).is_ok() {
            decoded += 1;
        }
    }
    println!("prefix sweep: {decoded} of 65,534 two-byte prefixes make the multiproof witness a valid SignerWitness encoding");
    assert_eq!(decoded, 0);
}
