//! Adversarial tests of the summary adoption (`bench/RESIDUE.md` §10), written by an independent review of this
//! crate (review-handoff-1tx, 18 Nov 2026) and kept here as written, with plain names. They run on a chain that
//! never existed: the store of the Electra minimal `sync` vector case, its current committee replaced by one the
//! test holds, and fabricated updates (`ferrata_spectests::testkit`). Needs the vectors (`scripts/fetch-vectors.sh`).
//!
//! 1. `forged_aggregates_after_summary_adoption`: a next committee whose aggregate field is `Q + P_31` (Q a key the
//!    forger holds, P_31 the public key of a member whose secret the forger does NOT hold), with the real keys and so
//!    the real `pubkeys_root`: the core's adoption refuses it (45), the summary adoption takes it; afterwards
//!    `aggregate − Σ non-signers` with member 31 absent is exactly Q, so the forger alone produces a "31 of 32" update
//!    that is accepted, and the 31 honest members' real signature is refused (40). Also an identity aggregate and one
//!    that does not decode: since the review's finding 9 was applied, the summary adoption decodes the field and
//!    refuses both with the core's code (45), so both paths refuse them alike (as written, the review expected the
//!    summary path to accept them and every later update to be refused). (Possible only with 342-of-512-equivalent
//!    signatures on a state the beacon chain's transition did not produce: outside the light client's assumption,
//!    §10.3.)
//! 2. `period_finality_and_replay_refusals`: not finalized, the wrong period, the same handoff twice, another committee
//!    for a period whose next committee is already adopted: both paths refuse with the same code.
//! 3. `adopt_summary_guards`: `adopt_summary` on jobs whose pending next committee is not the progress's: refused; a
//!    job adopted before its signature verified is refused by `apply`.
//! 4. `differential_mutations_of_a_handoff`: 160 mutations of the honest handoff: both paths give the same store or
//!    the same code.

use ferrata_core::{
    bls::{Aggregation, Blst, DST},
    fork::ChainSpec,
    policy::FinalizedStore,
    steps::{self, Job, PendingApply, Phase, StepError},
    store::{ApplyOutcome, StoreCore},
    types::{LightClientUpdate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{uncompressed_keys, CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_spectests::{
    load_sync_case,
    testkit::{fabricated_update, fake_committee, Fabrication},
    vectors_dir,
};
use residue::{decode_fp12, encode_fp12, witness};
use residue_agg::{NonSigners, Sum};
use residue_handoff::{adopt_summary, one_step};

type Out = Result<(CompactStore, ApplyOutcome, Job), StepError>;

/// The core's own steps with the 512-key (here 32-key) adoption.
fn core_path(s: &CompactStore, u: &LightClientUpdate, c: &SyncCommittee, slot: u64, spec: &ChainSpec, gvr: &Root) -> Out {
    let sw = SignerWitness::build(c, &u.sync_aggregate.bits).map_err(StepError::Core)?;
    let cu = CompactUpdate::of(u);
    let mut job = steps::step1(s, &cu, &sw, slot, spec, gvr, 1)?;
    job.step2()?;
    job.step3()?;
    if job.adoption_pending() {
        let keys = uncompressed_keys(&u.next_sync_committee).map_err(StepError::Core)?;
        let n = keys.len();
        job.adopt(0, &keys, n)?;
    }
    let (next, o) = steps::apply(s, &job, spec)?;
    Ok((next, o, job))
}

/// The one-step path (multiproof, residue witness computed from the product, summary adoption).
fn summary_path(s: &CompactStore, u: &LightClientUpdate, c: &SyncCommittee, slot: u64, spec: &ChainSpec, gvr: &Root) -> Out {
    let ns = NonSigners::build(c, &u.sync_aggregate.bits).map_err(StepError::Core)?;
    let cu = CompactUpdate::of(u);
    let mut probe = residue_agg::step1(s, &cu, &ns, slot, spec, gvr, 1, Sum::Batch)?;
    probe.step2()?;
    let Phase::FinalExp { product } = &probe.phase else { unreachable!() };
    let (cw, ww) = witness(&decode_fp12(product).unwrap());
    one_step(s, s, &cu, &ns, slot, &encode_fp12(&cw), &encode_fp12(&ww), spec, gvr, Sum::Batch)
}

fn code(r: &Out) -> Option<u16> {
    r.as_ref().err().map(|e| e.code())
}

fn pk_add(a: &[u8; 48], b: &[u8; 48]) -> [u8; 48] {
    let a = blst::min_pk::PublicKey::uncompress(a).unwrap();
    let b = blst::min_pk::PublicKey::uncompress(b).unwrap();
    blst::min_pk::AggregatePublicKey::aggregate(&[&a, &b], false).unwrap().to_public_key().compress()
}

struct World {
    spec: ChainSpec,
    gvr: Root,
    store: CompactStore,
    current: ferrata_spectests::testkit::FakeCommittee,
    next: ferrata_spectests::testkit::FakeCommittee,
    slots: (u64, u64, u64, u64), // finalized, attested, signature, current slot
    len: u64,
}

fn world() -> World {
    let bls = Blst::new(Aggregation::Auto);
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = case.config.spec.clone();
    let gvr = case.genesis_validators_root;
    let (u, current_slot) = &case.updates[0];
    let current = fake_committee(32, 0x31);
    let next = fake_committee(32, 0x32);
    let policy = FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, &spec, &bls).unwrap();
    let core = StoreCore { current_sync_committee: current.committee.clone(), ..policy.core().clone() };
    let store = CompactStore::of(&core);
    let len = spec.preset.slots_per_epoch * spec.preset.epochs_per_sync_committee_period;
    let slots = (u.finalized_header.beacon.slot, u.attested_header.beacon.slot, u.signature_slot, *current_slot);
    println!(
        "world: store finalized slot {} (period {}), next known {}; handoff slots finalized {} attested {} signature {} current {}; period length {len}",
        store.finalized.slot,
        spec.period_at_slot(store.finalized.slot),
        store.next.is_some(),
        slots.0,
        slots.1,
        slots.2,
        slots.3
    );
    World { spec, gvr, store, current, next, slots, len }
}

fn handoff(w: &World, next: Option<&SyncCommittee>, finalized: Option<u64>, attested: u64, signature: u64, signers: usize) -> LightClientUpdate {
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &w.spec, genesis_validators_root: w.gvr, bls: &bls };
    let f = Fabrication { finalized_slot: finalized, attested_slot: attested, signature_slot: signature, signers, next_committee: next };
    fabricated_update(&f, &w.current, &ctx)
}

#[test]
fn forged_aggregates_after_summary_adoption() {
    let w = world();
    let (fin, att, sig, cur) = w.slots;
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &w.spec, genesis_validators_root: w.gvr, bls: &bls };

    // The forger's key Q, and the aggregate field Q + P_31 (P_31 public, its secret not held).
    let q = blst::min_pk::SecretKey::key_gen(&[0x5a; 32], &[]).unwrap();
    let q_pk = q.sk_to_pk().compress();
    let forged_field = pk_add(&q_pk, &w.next.committee.pubkeys[31]);
    let forged = SyncCommittee { pubkeys: w.next.committee.pubkeys.clone(), aggregate_pubkey: forged_field };
    let mut identity = [0u8; 48];
    identity[0] = 0xc0;
    let with_identity = SyncCommittee { pubkeys: w.next.committee.pubkeys.clone(), aggregate_pubkey: identity };
    let undecodable = SyncCommittee { pubkeys: w.next.committee.pubkeys.clone(), aggregate_pubkey: [0xff; 48] };

    let mut adopted = Vec::new();
    for (label, c) in [("Q + P_31", &forged), ("identity", &with_identity), ("undecodable", &undecodable), ("consistent", &w.next.committee)] {
        let u = handoff(&w, Some(c), Some(fin), att, sig, 32);
        let core = core_path(&w.store, &u, &w.current.committee, cur, &w.spec, &w.gvr);
        let one = summary_path(&w.store, &u, &w.current.committee, cur, &w.spec, &w.gvr);
        println!("handoff with aggregate field {label}: core {:?}, summary path {:?}", code(&core), code(&one));
        if label == "consistent" {
            assert_eq!(core.as_ref().unwrap().0, one.as_ref().unwrap().0, "consistent: same store");
        } else if label == "Q + P_31" {
            assert_eq!(code(&core), Some(45), "{label}: the core's adoption refuses (CommitteeInvalid)");
            assert!(one.is_ok(), "{label}: the summary adoption accepts");
        } else {
            // Finding 9 applied: the summary adoption decodes the field and refuses it as the core does.
            assert_eq!(code(&core), Some(45), "{label}: the core's adoption refuses (CommitteeInvalid)");
            assert_eq!(code(&one), Some(45), "{label}: the summary adoption refuses alike");
            continue;
        }
        let (s, o, _) = one.unwrap();
        assert!(o.adopted);
        assert_eq!(s.next, Some(CommitteeSummary::of(c)));
        adopted.push((label, s));
    }

    // The next period, signed by the adopted committee (rotation): member 31 absent.
    let f = Fabrication {
        finalized_slot: Some(fin + w.len),
        attested_slot: att + w.len,
        signature_slot: sig + w.len,
        signers: 31,
        next_committee: None,
    };
    let honest31 = fabricated_update(&f, &w.next, &ctx);
    assert!(!honest31.sync_aggregate.bits[31] && honest31.sync_aggregate.bits[..31].iter().all(|b| *b));
    let mut forger31 = honest31.clone();
    forger31.sync_aggregate.signature = q.sign(&sync_committee_signing_root(&forger31, &ctx), DST, &[]).compress();
    let f32 = Fabrication { signers: 32, ..f };
    let honest32 = fabricated_update(&f32, &w.next, &ctx);
    let later = cur + w.len;
    for (label, s) in &adopted {
        for (ulabel, u) in [("honest 31 of 32", &honest31), ("forger alone as 31 of 32", &forger31), ("honest 32 of 32", &honest32)] {
            let r = summary_path(s, u, &w.next.committee, later, &w.spec, &w.gvr);
            let rc = core_path(s, u, &w.next.committee, later, &w.spec, &w.gvr);
            println!("after adopting [{label}]: {ulabel}: summary path {:?}, core steps on the same store {:?}", code(&r), code(&rc));
            assert_eq!(code(&r), code(&rc), "the composition is the same on both paths once a store holds the field");
            let want = match (*label, ulabel) {
                ("consistent", "forger alone as 31 of 32") => Some(40),
                ("consistent", _) => None,
                ("Q + P_31", "forger alone as 31 of 32") => None, // a forged signature ACCEPTED
                ("Q + P_31", _) => Some(40),
                _ => unreachable!(),
            };
            assert_eq!(code(&r), want, "after [{label}] {ulabel}");
        }
    }
    println!("forged aggregate Q + P_31 adopted by the summary; the forger alone (secret of Q only) signs an update with 31 of 32 bits set, accepted; the honest 31 refused (40)");
}

#[test]
fn period_finality_and_replay_refusals() {
    let w = world();
    let (fin, att, sig, cur) = w.slots;
    let other = fake_committee(32, 0x33);
    let cases: Vec<(&str, LightClientUpdate, u64)> = vec![
        ("not finalized", handoff(&w, Some(&w.next.committee), None, att, sig, 32), cur),
        ("signature one period later (next unknown)", handoff(&w, Some(&w.next.committee), Some(fin), att, sig + w.len, 32), cur + w.len),
        ("attested and signature one period later", handoff(&w, Some(&w.next.committee), Some(fin), att + w.len, sig + w.len, 32), cur + w.len),
        ("finalized one period later (next unknown)", handoff(&w, Some(&w.next.committee), Some(fin + w.len), att + w.len, sig + w.len, 32), cur + w.len),
        ("341-equivalent: 21 of 32", handoff(&w, Some(&w.next.committee), Some(fin), att, sig, 21), cur),
    ];
    for (label, u, slot) in &cases {
        let a = core_path(&w.store, u, &w.current.committee, *slot, &w.spec, &w.gvr);
        let b = summary_path(&w.store, u, &w.current.committee, *slot, &w.spec, &w.gvr);
        println!("from the start store: {label}: core {:?}, summary path {:?}", code(&a), code(&b));
        assert!(a.is_err() && b.is_err(), "{label}: refused");
        assert_eq!(code(&a), code(&b), "{label}: same code");
    }
    // Adopt the honest handoff, then try it again and try another committee for the same period.
    let h = handoff(&w, Some(&w.next.committee), Some(fin), att, sig, 32);
    let (after, ..) = summary_path(&w.store, &h, &w.current.committee, cur, &w.spec, &w.gvr).unwrap();
    assert_eq!(w.spec.period_at_slot(att + 1), w.spec.period_at_slot(fin), "precondition: attested + 1 in the same period");
    let again = summary_path(&after, &h, &w.current.committee, cur, &w.spec, &w.gvr);
    let again_core = core_path(&after, &h, &w.current.committee, cur, &w.spec, &w.gvr);
    println!("the same handoff again: core {:?}, summary {:?}", code(&again_core), code(&again));
    assert!(again.is_err());
    assert_eq!(code(&again), code(&again_core));
    let swap = handoff(&w, Some(&other.committee), Some(fin), att + 1, sig + 1, 32);
    let s1 = summary_path(&after, &swap, &w.current.committee, cur + 1, &w.spec, &w.gvr);
    let s2 = core_path(&after, &swap, &w.current.committee, cur + 1, &w.spec, &w.gvr);
    println!("another next committee for the adopted period: core {:?}, summary {:?}", code(&s2), code(&s1));
    assert_eq!(code(&s1), Some(28), "NextCommitteeMismatch");
    assert_eq!(code(&s1), code(&s2));
}

#[test]
fn adopt_summary_guards() {
    let w = world();
    let (fin, att, sig, cur) = w.slots;
    let h = handoff(&w, Some(&w.next.committee), Some(fin), att, sig, 32);
    let ns = NonSigners::build(&w.current.committee, &h.sync_aggregate.bits).unwrap();
    let job = residue_agg::step1(&w.store, &CompactUpdate::of(&h), &ns, cur, &w.spec, &w.gvr, 1, Sum::Batch).unwrap();
    assert!(job.adoption_pending());
    // pending.next another summary than the progress's
    let mut tampered = job.clone();
    if let PendingApply::Update { next, .. } = &mut tampered.pending {
        *next = Some(CommitteeSummary::of(&fake_committee(32, 0x34).committee));
    }
    assert_eq!(adopt_summary(&mut tampered), Err(StepError::NoAdoption));
    let mut none = job.clone();
    if let PendingApply::Update { next, .. } = &mut none.pending {
        *next = None;
    }
    assert_eq!(adopt_summary(&mut none), Err(StepError::NoAdoption));
    // the progress swapped instead
    let mut swapped = job.clone();
    swapped.adoption.as_mut().unwrap().committee = CommitteeSummary::of(&fake_committee(32, 0x35).committee);
    assert_eq!(adopt_summary(&mut swapped), Err(StepError::NoAdoption));
    // before the signature verified: adopt_summary itself does not look at the phase; apply does.
    let mut early = job.clone();
    adopt_summary(&mut early).unwrap();
    assert_eq!(steps::apply(&w.store, &early, &w.spec).unwrap_err(), StepError::Phase);
    println!("adopt_summary refuses a tampered pending/progress (84); a job adopted before its signature verified is refused by apply (81)");
}

#[test]
fn differential_mutations_of_a_handoff() {
    let w = world();
    let (fin, att, sig, cur) = w.slots;
    let h = handoff(&w, Some(&w.next.committee), Some(fin), att, sig, 32);
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rnd = |m: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % m
    };
    let (mut same_ok, mut same_err) = (0, 0);
    let mut codes = std::collections::BTreeMap::new();
    for i in 0..160 {
        let mut u = h.clone();
        let kind = i % 10;
        match kind {
            0 => u.sync_aggregate.signature[rnd(96) as usize] ^= 1 << rnd(8),
            1 => {
                let j = rnd(32) as usize;
                u.sync_aggregate.bits[j] = !u.sync_aggregate.bits[j]
            }
            2 => {
                let n = u.finality_branch.len() as u64;
                u.finality_branch[rnd(n) as usize][rnd(32) as usize] ^= 1 << rnd(8)
            }
            3 => {
                let n = u.next_sync_committee_branch.len() as u64;
                u.next_sync_committee_branch[rnd(n) as usize][rnd(32) as usize] ^= 1 << rnd(8)
            }
            4 => u.next_sync_committee.aggregate_pubkey[rnd(48) as usize] ^= 1 << rnd(8),
            5 => u.next_sync_committee.pubkeys[rnd(32) as usize][rnd(48) as usize] ^= 1 << rnd(8),
            6 => u.attested_header.beacon.slot = u.attested_header.beacon.slot.wrapping_add(rnd(3)).wrapping_sub(1),
            7 => u.finalized_header.beacon.slot = u.finalized_header.beacon.slot.wrapping_add(rnd(3)).wrapping_sub(1),
            8 => u.signature_slot = u.signature_slot.wrapping_add(rnd(3 * w.len)),
            _ => u.attested_header.beacon.state_root[rnd(32) as usize] ^= 1 << rnd(8),
        }
        let slot = cur.max(u.signature_slot);
        let a = core_path(&w.store, &u, &w.current.committee, slot, &w.spec, &w.gvr);
        let b = summary_path(&w.store, &u, &w.current.committee, slot, &w.spec, &w.gvr);
        match (&a, &b) {
            (Ok(x), Ok(y)) => {
                assert_eq!(x.0, y.0, "mutation {i} kind {kind}: stores differ");
                same_ok += 1;
            }
            (Err(x), Err(y)) => {
                assert_eq!(x.code(), y.code(), "mutation {i} kind {kind}");
                *codes.entry(x.code()).or_insert(0) += 1;
                same_err += 1;
            }
            _ => panic!("mutation {i} kind {kind}: core {:?} summary {:?}", code(&a), code(&b)),
        }
    }
    println!("differential: 160 mutations: {same_ok} accepted alike (same store), {same_err} refused alike, codes {codes:?}");
}
