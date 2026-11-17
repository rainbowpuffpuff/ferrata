//! `residue-host profile` and `residue-host measure-agg`: the non-signers' cost, and the whole
//! update with the signing key composed by `residue_agg` (one multiproof for all non-signers, the
//! compressed key read off its bytes, blst's batch addition) against the core's composition, in
//! the bench guest `guests/residue-agg-accel` (executor only, r0vm 3.0.5). Experimental and
//! unaudited; no deployed program runs this code. Outputs: `results/residue/agg/`.

use std::{fmt::Write as _, fs, time::Instant};

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork},
    policy::{Adoption, FinalizedStore},
    steps,
    types::{LightClientBootstrap, LightClientUpdate},
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_spectests::testkit::{fake_committee, sign};
use residue_agg::{NonSigners, Sum};
use risc0_zkvm::{default_executor, ExecutorEnv};

use super::{as_finality, bench_dir, fixture, get, mainnet, native, report, root, signing_committee, Case, Run, GVR, TRUSTED};

const MODE_ONE_TX: u32 = 0;
const MODE_PROFILE: u32 = 3;

fn execute(mode: u32, words: &[u32], blobs: &[&[u8]]) -> Run {
    let mut b = ExecutorEnv::builder();
    b.write(&mode).unwrap();
    for w in words {
        b.write(w).unwrap();
    }
    for x in blobs {
        b.write(&(x.len() as u32)).unwrap();
        let mut padded = x.to_vec();
        padded.resize((x.len() + 3) & !3, 0);
        b.write_slice(&padded);
    }
    let env = b.build().unwrap();
    let started = Instant::now();
    let info = default_executor().execute(env, residue_methods::RESIDUE_AGG_ACCEL_ELF).unwrap();
    let secs = started.elapsed().as_secs_f64();
    let (rec, extra): (Vec<(String, u64)>, Vec<u8>) = info.journal.decode().unwrap();
    Run { rec, extra, info, secs }
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A bitfield of `n` members with exactly `absent` of them absent, chosen by a seeded shuffle.
pub fn random_bits(n: usize, absent: usize, seed: u64) -> Vec<bool> {
    let mut idx: Vec<usize> = (0..n).collect();
    let mut s = seed;
    for i in (1..n).rev() {
        let j = (splitmix(&mut s) % (i as u64 + 1)) as usize;
        idx.swap(i, j);
    }
    let mut bits = vec![true; n];
    for &i in &idx[..absent] {
        bits[i] = false;
    }
    bits
}

/// Every third member absent (indices 2, 5, …, 509): 170 non-signers, each in a pair of leaves
/// of its own, so the multiproof needs the most nodes any 170 non-signers can need (425 hashes).
fn every_third() -> Vec<bool> {
    (0..512).map(|i| i % 3 != 2).collect()
}

/// The store after mainnet update_0, update_1 as a finality update (or as served, a period
/// update), the real signing committee, and the bench's committee substituted in the store.
struct Base {
    spec: ChainSpec,
    gvr: Root,
    store: ferrata_core::witness::CompactStore,
    substituted: ferrata_core::witness::CompactStore,
    real: ferrata_core::types::SyncCommittee,
    fake: ferrata_spectests::testkit::FakeCommittee,
    finality: LightClientUpdate,
    period: LightClientUpdate,
}

fn base() -> Base {
    let spec = mainnet();
    let gvr = root(GVR);
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let u0 = LightClientUpdate::decode(&fixture("update_0.ssz"), LcFork::Electra, 512).unwrap();
    let u1 = LightClientUpdate::decode(&fixture("update_1.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy =
        FinalizedStore::initialize_with(&root(TRUSTED), &bootstrap, LcFork::Electra, &spec, &bls, Adoption::OnChain)
            .unwrap();
    policy.process_update(&u0, u0.signature_slot + 1, &ctx).unwrap();
    let store = ferrata_core::witness::CompactStore::of(policy.core());
    let real = signing_committee(policy.core(), &u1, &spec).clone();
    let fake = fake_committee(512, 7);
    let mut substituted = store.clone();
    let (real_summary, fake_summary) = (CommitteeSummary::of(&real), CommitteeSummary::of(&fake.committee));
    if substituted.current == real_summary {
        substituted.current = fake_summary;
    } else {
        assert_eq!(substituted.next, Some(real_summary));
        substituted.next = Some(fake_summary);
    }
    Base { finality: as_finality(&u1), period: u1, spec, gvr, store, substituted, real, fake }
}

/// The update signed by the bench's committee with `bits`.
fn signed(b: &Base, update: &LightClientUpdate, bits: Vec<bool>) -> LightClientUpdate {
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &b.spec, genesis_validators_root: b.gvr, bls: &bls };
    let signing_root = sync_committee_signing_root(update, &ctx);
    let mut u = update.clone();
    u.sync_aggregate.signature = sign(&b.fake, &bits, &signing_root);
    u.sync_aggregate.bits = bits;
    u
}

/// The case's `NonSigners` witness, after checking natively that `residue_agg::step1` (batch and
/// chain) gives the job the core's `step1` gives.
fn agg_witness(case: &Case, spec: &ChainSpec, gvr: &Root) -> Vec<u8> {
    let bits = &case.update.sync_aggregate.bits;
    let old = SignerWitness::build(&case.committee, bits).unwrap();
    let new = NonSigners::build(&case.committee, bits).unwrap();
    let compact = CompactUpdate::of(&case.update);
    let slot = case.update.signature_slot;
    let core = steps::step1(&case.store, &compact, &old, slot, spec, gvr, 1).unwrap();
    for sum in [Sum::Batch, Sum::Chain] {
        let mine = residue_agg::step1(&case.store, &compact, &new, slot, spec, gvr, 1, sum).unwrap();
        assert_eq!(mine, core, "{}: residue_agg::step1 ({sum:?}) = the core's step1", case.name);
    }
    let bytes = new.encode();
    assert_eq!(NonSigners::decode(&bytes, 512).unwrap(), new);
    bytes
}

pub fn profile() {
    let b = base();
    let mut raw = String::new();
    let choices: Vec<(String, Vec<bool>)> = vec![
        ("342 signers, every third member absent (the multiproof's worst case)".into(), every_third()),
        ("342 signers, 170 absent at random (seed 1)".into(), random_bits(512, 170, 1)),
        ("400 signers, 112 absent at random (seed 2)".into(), random_bits(512, 112, 2)),
        ("510 signers, update_1's bits".into(), b.finality.sync_aggregate.bits.clone()),
    ];
    for (label, bits) in choices {
        let summary = CommitteeSummary::of(&b.fake.committee).encode();
        let bit_bytes: Vec<u8> = bits.iter().map(|x| u8::from(*x)).collect();
        let old = SignerWitness::build(&b.fake.committee, &bits).unwrap().encode();
        let new = NonSigners::build(&b.fake.committee, &bits).unwrap().encode();
        let run = execute(MODE_PROFILE, &[], &[&summary, &bit_bytes, &old, &new]);
        report(&mut raw, &format!("PROFILE: the bench's committee, {label}"), &run);
        println!("profile {label}: {} user cycles", run.info.cycles());
    }
    let dir = bench_dir().join("results/residue/agg");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("profile.txt"), &raw).unwrap();
    println!("{raw}");
}

pub fn measure() {
    let b = base();
    let mut raw = String::new();
    let mut table = String::from(
        "| case | path | aggregation | D3 decode witness | S1 step 1 | S2+S3 (folded: one phase) | A adoption | rest | whole update (cycle_count) | execution user cycles | signer witness bytes | segments |\n|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    let mut cases: Vec<(Case, Vec<(u32, u32)>)> = Vec::new();
    let all: Vec<(u32, u32)> = vec![(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)];
    cases.push((
        Case {
            name: "finality, mainnet update_1 as served: 510 of 512 (real committee)".into(),
            store: b.store.clone(),
            update: b.finality.clone(),
            committee: b.real.clone(),
        },
        all.clone(),
    ));
    let mut choices: Vec<(String, Vec<bool>, Vec<(u32, u32)>)> = vec![
        ("512 (all)".into(), vec![true; 512], all.clone()),
        ("511 (member 300 absent)".into(), (0..512).map(|i| i != 300).collect(), all.clone()),
        ("510 (update_1's bits)".into(), b.finality.sync_aggregate.bits.clone(), all.clone()),
        ("450 (62 absent at random, seed 3)".into(), random_bits(512, 62, 3), all.clone()),
        ("400 (112 absent at random, seed 2)".into(), random_bits(512, 112, 2), all.clone()),
        ("343 (169 absent at random, seed 4)".into(), random_bits(512, 169, 4), all.clone()),
        ("342 (170 absent at random, seed 1)".into(), random_bits(512, 170, 1), all.clone()),
    ];
    let mut worst = all.clone();
    worst.push((2, 2));
    choices.push(("342 (every third member absent: the multiproof's worst case)".into(), every_third(), worst));
    for (label, bits, runs) in choices {
        let u = signed(&b, &b.finality, bits);
        cases.push((
            Case {
                name: format!("finality, update_1 signed by the bench's committee: {label}"),
                store: b.substituted.clone(),
                update: u,
                committee: b.fake.committee.clone(),
            },
            runs,
        ));
    }
    // Period updates (adoption of 512 keys): folded path only.
    cases.push((
        Case {
            name: "period (adopts 512 keys), mainnet update_1 as served: 510 of 512 (real committee)".into(),
            store: b.store.clone(),
            update: b.period.clone(),
            committee: b.real.clone(),
        },
        vec![(2, 0), (2, 1)],
    ));
    cases.push((
        Case {
            name: "period (adopts 512 keys), signed by the bench's committee: 342 (every third absent)".into(),
            store: b.substituted.clone(),
            update: signed(&b, &b.period, every_third()),
            committee: b.fake.committee.clone(),
        },
        vec![(2, 0), (2, 1)],
    ));

    for (case, runs) in cases {
        let inp = native(&case, &b.spec, &b.gvr);
        let new_signers = agg_witness(&case, &b.spec, &b.gvr);
        for (path, aggregation) in runs {
            let (c, w, c_inv): (&[u8], &[u8], &[u8]) = match path {
                0 => (&[], &[], &[]),
                1 => (&inp.c, &inp.w, &[]),
                _ => (&inp.c, &inp.w, &inp.c_inv),
            };
            let signers: &[u8] = if aggregation == 0 { &inp.signers } else { &new_signers };
            let run = execute(
                MODE_ONE_TX,
                &[path, aggregation],
                &[&inp.store, &inp.update, signers, &inp.keys, c, w, c_inv],
            );
            let path_name = ["current (final exponentiation)", "witness", "witness folded"][path as usize];
            let agg_name = ["core (a branch per non-signer)", "multiproof + batch sum", "multiproof + chain sum"]
                [aggregation as usize];
            assert_eq!(run.extra, inp.expected_store, "{} / {path_name} / {agg_name}: the guest's new store", case.name);
            report(&mut raw, &format!("{} / {path_name} / {agg_name}", case.name), &run);
            let d3 = get(&run, "D3 ");
            let s1 = get(&run, "S1 ");
            let s23 = get(&run, "S2 ") + get(&run, "S3") + get(&run, "S23f ");
            let a = get(&run, "A0 ") + get(&run, "A1 ");
            let whole = get(&run, "T  ");
            writeln!(
                table,
                "| {} | {path_name} | {agg_name} | {d3} | {s1} | {s23} | {a} | {} | {whole} | {} | {} | {} |",
                case.name,
                whole - s1 - s23 - a,
                run.info.cycles(),
                get(&run, "info: signer witness bytes"),
                run.info.segments.len()
            )
            .unwrap();
            println!("{} / {path_name} / {agg_name}: {} user cycles", case.name, run.info.cycles());
        }
    }

    // Refusals in the guest, multiproof path: a sibling altered, a key swapped for a key outside
    // the committee, the witness of another bitfield.
    let u = signed(&b, &b.finality, every_third());
    let case = Case { name: "342".into(), store: b.substituted.clone(), update: u, committee: b.fake.committee.clone() };
    let inp = native(&case, &b.spec, &b.gvr);
    let good = NonSigners::build(&case.committee, &case.update.sync_aggregate.bits).unwrap();
    let mut altered = good.clone();
    altered.siblings[100][5] ^= 1;
    let other = fake_committee(512, 9);
    let mut foreign = good.clone();
    foreign.keys[17] = ferrata_core::witness::uncompress_key(&other.committee.pubkeys[0]).unwrap();
    let shifted = NonSigners::build(&case.committee, &random_bits(512, 170, 1)).unwrap();
    for (label, w) in [
        ("a multiproof node with one bit flipped", altered),
        ("one non-signer's key replaced by a key outside the committee", foreign),
        ("the witness of another bitfield with as many non-signers", shifted),
    ] {
        let run = execute(MODE_ONE_TX, &[2, 1], &[&inp.store, &inp.update, &w.encode(), &inp.keys, &inp.c, &inp.w, &inp.c_inv]);
        assert_eq!(run.extra, vec![0xff, 80], "{label}: refused with the witness code (80)");
        report(&mut raw, &format!("REFUSAL (342, folded, multiproof): {label}"), &run);
    }

    let dir = bench_dir().join("results/residue/agg");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("one-tx.txt"), &raw).unwrap();
    fs::write(dir.join("tables.md"), &table).unwrap();
    println!("{table}");
}

/// `absent` members absent, evenly spaced: with one multiproof, the most hashes that many need.
fn spread(absent: usize) -> Vec<bool> {
    let mut bits = vec![true; 512];
    for j in 0..absent {
        bits[j * 512 / absent] = false;
    }
    bits
}

/// Around the fewest signers that fit 10M user cycles: a branch per non-signer with the witness
/// folded and with the witness check, and one multiproof with the final exponentiation.
pub fn boundary() {
    let b = base();
    let mut raw = String::new();
    let mut table = String::from(
        "| path | aggregation | signers (absent evenly spaced) | execution user cycles | of 10M | fits |\n|---|---|---:|---:|---:|---|\n",
    );
    let runs: Vec<(u32, u32, Vec<usize>)> =
        vec![(2, 0, vec![347, 348, 349]), (1, 0, vec![373, 374, 375]), (0, 1, vec![407, 408, 409, 410])];
    for (path, aggregation, list) in runs {
        for signers in list {
            let u = signed(&b, &b.finality, spread(512 - signers));
            let case = Case {
                name: format!("finality, update_1 signed by the bench's committee: {signers}"),
                store: b.substituted.clone(),
                update: u,
                committee: b.fake.committee.clone(),
            };
            let inp = native(&case, &b.spec, &b.gvr);
            let new_signers = agg_witness(&case, &b.spec, &b.gvr);
            let (c, w, c_inv): (&[u8], &[u8], &[u8]) = match path {
                0 => (&[], &[], &[]),
                1 => (&inp.c, &inp.w, &[]),
                _ => (&inp.c, &inp.w, &inp.c_inv),
            };
            let signers_b: &[u8] = if aggregation == 0 { &inp.signers } else { &new_signers };
            let run = execute(MODE_ONE_TX, &[path, aggregation], &[&inp.store, &inp.update, signers_b, &inp.keys, c, w, c_inv]);
            assert_eq!(run.extra, inp.expected_store, "{}: the guest's new store", case.name);
            let path_name = ["current (final exponentiation)", "witness", "witness folded"][path as usize];
            let agg_name = ["core (a branch per non-signer)", "multiproof + batch sum"][aggregation as usize];
            report(&mut raw, &format!("{} / {path_name} / {agg_name}", case.name), &run);
            let cycles = run.info.cycles();
            writeln!(
                table,
                "| {path_name} | {agg_name} | {signers} | {cycles} | {:.2}% | {} |",
                cycles as f64 / 100_000.0,
                if cycles <= 10_000_000 { "yes" } else { "no" }
            )
            .unwrap();
            println!("{path_name} / {agg_name} / {signers}: {cycles}");
        }
    }
    let dir = bench_dir().join("results/residue/agg");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("boundary.txt"), &raw).unwrap();
    fs::write(dir.join("boundary.md"), &table).unwrap();
    println!("{table}");
}
