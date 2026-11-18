//! Runs the residue bench guest (`guests/residue-accel`) with the RISC Zero executor (r0vm 3.0.5,
//! no proving). Experimental and unaudited; no deployed program runs this code.
//!
//! `residue-host measure`: one whole update per execution, as the LEZ program's `VerifyAll`, on
//! mainnet update_1 against the store after update_0: a finality update and a period update
//! (handoff with adoption), at the real participation (510 of 512) and, with a committee whose
//! keys the bench holds substituted for the signing committee, at 510, 342 (the fewest the
//! policy accepts) and 512 signers; each with the current path and the witness path. Every new
//! store is compared with the native run's. Also two refusals in the guest and the cost of the
//! Fp12 operations. Writes `results/residue/one-tx.txt` and `results/residue/tables.md`.
//!
//! `residue-host check`: test (d), the guest's verdicts on every product of test (a)
//! (`target/residue-tuples-a.bin`, written by `cargo test -p residue --test real_data`).

use std::{fmt::Write as _, fs, path::PathBuf, time::Instant};

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, Phase},
    store::StoreCore,
    types::{LightClientBootstrap, LightClientUpdate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{uncompressed_keys, CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_spectests::testkit::{fake_committee, sign};
use residue::{check, decode_fp12, encode_fp12, final_exp_is_one, inverse, witness, FP12_BYTES};
use risc0_zkvm::{default_executor, ExecutorEnv, SessionInfo};
use sha2::{Digest, Sha256};

const MODE_ONE_TX: u32 = 0;
const MODE_BATCH: u32 = 1;
const MODE_OPS: u32 = 2;

fn bench_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fixture(name: &str) -> Vec<u8> {
    let dir = bench_dir().join("../crates/ferrata-spectests/fixtures/mainnet-p1899");
    let bytes = fs::read(dir.join(name)).unwrap();
    let sums = fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(name)).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

const GVR: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const TRUSTED: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

struct Run {
    rec: Vec<(String, u64)>,
    extra: Vec<u8>,
    info: SessionInfo,
    secs: f64,
}

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
    let info = default_executor().execute(env, residue_methods::RESIDUE_ACCEL_ELF).unwrap();
    let secs = started.elapsed().as_secs_f64();
    let (rec, extra): (Vec<(String, u64)>, Vec<u8>) = info.journal.decode().unwrap();
    Run { rec, extra, info, secs }
}

fn report(out: &mut String, title: &str, run: &Run) {
    let user = run.info.cycles();
    let padded: u64 = run.info.segments.iter().map(|s| 1u64 << s.po2).sum();
    writeln!(out, "### {title}").unwrap();
    for (label, v) in &run.rec {
        writeln!(out, "{v:>14}  {label}").unwrap();
    }
    writeln!(
        out,
        "{user:>14}  EXECUTION user cycles (SessionInfo::cycles)\n{padded:>14}  EXECUTION total cycles incl. padding/paging (sum of 2^po2 over {} segments)\n{:>14.2}  executor wall-clock seconds\n",
        run.info.segments.len(),
        run.secs
    )
    .unwrap();
}

fn get(run: &Run, prefix: &str) -> u64 {
    run.rec.iter().filter(|(l, _)| l.starts_with(prefix)).map(|(_, v)| *v).sum()
}

/// One measured case: the store the update is checked against, the update, the signing committee.
struct Case {
    name: String,
    store: CompactStore,
    update: LightClientUpdate,
    committee: SyncCommittee,
}

struct Inputs {
    store: Vec<u8>,
    update: Vec<u8>,
    signers: Vec<u8>,
    keys: Vec<u8>,
    c: [u8; FP12_BYTES],
    w: [u8; FP12_BYTES],
    c_inv: [u8; FP12_BYTES],
    expected_store: Vec<u8>,
    product: [u8; FP12_BYTES],
}

/// The native run of the case through the step API (the current path), and the honest witness.
fn native(case: &Case, spec: &ChainSpec, gvr: &Root) -> Inputs {
    let n = 512;
    let signers = SignerWitness::build(&case.committee, &case.update.sync_aggregate.bits).unwrap();
    let compact = CompactUpdate::of(&case.update);
    let mut job = steps::step1(&case.store, &compact, &signers, case.update.signature_slot, spec, gvr, 1)
        .unwrap_or_else(|e| panic!("{}: step 1: {e:?}", case.name));
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!("step 2 leaves the product") };
    let f = decode_fp12(&product).unwrap();
    let (c, w) = witness(&f);
    assert_eq!(check(&f, &c, &w), Ok(()), "{}: the honest witness", case.name);
    job.step3().unwrap_or_else(|e| panic!("{}: step 3: {e:?}", case.name));
    let keys: Vec<u8> = if case.update.next_sync_committee.is_empty() {
        Vec::new()
    } else {
        uncompressed_keys(&case.update.next_sync_committee).unwrap().concat()
    };
    if job.adoption_pending() {
        let k: Vec<[u8; 96]> = keys.chunks_exact(96).map(|c| c.try_into().unwrap()).collect();
        job.adopt(0, &k, n).unwrap();
    }
    let (next, _) = steps::apply(&case.store, &job, spec).unwrap();
    Inputs {
        store: case.store.encode(),
        update: compact.encode(case.store.fork),
        signers: signers.encode(),
        keys,
        c: encode_fp12(&c),
        w: encode_fp12(&w),
        c_inv: encode_fp12(&inverse(&c)),
        expected_store: next.encode(),
        product,
    }
}

fn signing_committee<'a>(full: &'a StoreCore, update: &LightClientUpdate, spec: &ChainSpec) -> &'a SyncCommittee {
    if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    }
}

fn as_finality(u: &LightClientUpdate) -> LightClientUpdate {
    let mut f = u.clone();
    f.next_sync_committee = SyncCommittee::empty(512);
    f.next_sync_committee_branch = vec![ZERO; u.next_sync_committee_branch.len()];
    f
}

fn cases(spec: &ChainSpec, gvr: &Root) -> Vec<Case> {
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec, genesis_validators_root: *gvr, bls: &bls };
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let u0 = LightClientUpdate::decode(&fixture("update_0.ssz"), LcFork::Electra, 512).unwrap();
    let u1 = LightClientUpdate::decode(&fixture("update_1.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy =
        FinalizedStore::initialize_with(&root(TRUSTED), &bootstrap, LcFork::Electra, spec, &bls, Adoption::OnChain)
            .unwrap();
    policy.process_update(&u0, u0.signature_slot + 1, &ctx).unwrap();
    let store = CompactStore::of(policy.core());
    let real = signing_committee(policy.core(), &u1, spec).clone();
    let mut out = vec![];
    for (kind, update) in [("finality", as_finality(&u1)), ("period (handoff, adopts 512 keys)", u1.clone())] {
        let signers = update.sync_aggregate.participants();
        out.push(Case {
            name: format!("{kind}, mainnet update_1 as served: {signers} of 512 signers"),
            store: store.clone(),
            update: update.clone(),
            committee: real.clone(),
        });
        // The same update signed by a committee whose keys the bench holds, substituted in the
        // store for the signing committee (its summary); the headers and branches stay real.
        let fake = fake_committee(512, 7);
        let mut substituted = store.clone();
        let (real_summary, fake_summary) = (CommitteeSummary::of(&real), CommitteeSummary::of(&fake.committee));
        if substituted.current == real_summary {
            substituted.current = fake_summary;
        } else {
            assert_eq!(substituted.next, Some(real_summary));
            substituted.next = Some(fake_summary);
        }
        let signing_root = sync_committee_signing_root(&update, &ctx);
        let real_bits = update.sync_aggregate.bits.clone();
        let choices: [(&str, Vec<bool>); 3] = [
            ("510 (update_1's bits)", real_bits),
            ("342 (every third member absent: 170)", (0..512).map(|i| i % 3 != 2).collect()),
            ("512 (all)", vec![true; 512]),
        ];
        for (label, bits) in choices {
            let mut u = update.clone();
            u.sync_aggregate.signature = sign(&fake, &bits, &signing_root);
            u.sync_aggregate.bits = bits;
            out.push(Case {
                name: format!("{kind}, update_1 signed by the bench's committee: {label} signers"),
                store: substituted.clone(),
                update: u,
                committee: fake.committee.clone(),
            });
        }
    }
    out
}

fn measure() {
    let spec = mainnet();
    let gvr = root(GVR);
    let mut raw = String::new();
    let mut table = String::from(
        "| case | path | S1 step 1 | S2 Miller | S3 final check | S2+S3 (folded: one phase) | A adoption | rest (read, decode, encode, apply) | whole update (cycle_count) | execution user cycles | segments |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    let mut ops_input = None;
    for case in cases(&spec, &gvr) {
        let inp = native(&case, &spec, &gvr);
        ops_input.get_or_insert((inp.product, inp.c, inp.w));
        for (path, flag) in [
            ("current (final exponentiation)", 0u32),
            ("witness (residue check)", 1),
            ("witness folded into the Miller loop", 2),
        ] {
            let (c, w, c_inv): (&[u8], &[u8], &[u8]) = match flag {
                0 => (&[], &[], &[]),
                1 => (&inp.c, &inp.w, &[]),
                _ => (&inp.c, &inp.w, &inp.c_inv),
            };
            let run = execute(MODE_ONE_TX, &[flag], &[&inp.store, &inp.update, &inp.signers, &inp.keys, c, w, c_inv]);
            assert_eq!(run.extra, inp.expected_store, "{} / {path}: the guest's new store", case.name);
            report(&mut raw, &format!("{} / {path}", case.name), &run);
            let s1 = get(&run, "S1 ");
            let s2 = get(&run, "S2 ");
            let s3 = get(&run, "S3");
            let s23 = s2 + s3 + get(&run, "S23f ");
            let a = get(&run, "A0 ") + get(&run, "A1 ");
            let whole = get(&run, "T  ");
            let dash = |v: u64| if flag == 2 { "-".to_string() } else { v.to_string() };
            writeln!(
                table,
                "| {} | {path} | {s1} | {} | {} | {s23} | {a} | {} | {whole} | {} | {} |",
                case.name,
                dash(s2),
                dash(s3),
                whole - s1 - s23 - a,
                run.info.cycles(),
                run.info.segments.len()
            )
            .unwrap();
            println!("{} / {path}: {} user cycles", case.name, run.info.cycles());
        }
    }

    // Refusals in the guest: a tampered c, and update_2's witness, for the real finality update.
    let case = cases(&spec, &gvr).into_iter().next().unwrap();
    let inp = native(&case, &spec, &gvr);
    let mut flipped = inp.c;
    flipped[47] ^= 1;
    let run = execute(MODE_ONE_TX, &[1], &[&inp.store, &inp.update, &inp.signers, &inp.keys, &flipped, &inp.w, &[]]);
    assert_eq!(run.extra, vec![0xff, 243], "a tampered c is refused with Mismatch (243)");
    report(&mut raw, "REFUSAL: finality update_1, witness path, c with one bit flipped", &run);
    let mut w_outside = inp.w;
    w_outside[300] = 1; // a coefficient of w's c1: w is no longer in Fp6
    let run = execute(MODE_ONE_TX, &[1], &[&inp.store, &inp.update, &inp.signers, &inp.keys, &inp.c, &w_outside, &[]]);
    assert_eq!(run.extra, vec![0xff, 242], "w outside Fp6 is refused with ScalingNotInFp6 (242)");
    report(&mut raw, "REFUSAL: finality update_1, witness path, w with a coefficient outside Fp6", &run);
    let run = execute(MODE_ONE_TX, &[2], &[&inp.store, &inp.update, &inp.signers, &inp.keys, &inp.c, &inp.w, &inp.c]);
    assert_eq!(run.extra, vec![0xff, 241], "a c^-1 that is not the inverse is refused (241)");
    report(&mut raw, "REFUSAL: finality update_1, folded path, c given as its own inverse", &run);
    let run =
        execute(MODE_ONE_TX, &[2], &[&inp.store, &inp.update, &inp.signers, &inp.keys, &flipped, &inp.w, &inp.c_inv]);
    assert_eq!(run.extra, vec![0xff, 241], "a flipped c no longer matches its inverse (241)");
    report(&mut raw, "REFUSAL: finality update_1, folded path, c with one bit flipped", &run);

    let (f, c, w) = ops_input.unwrap();
    let run = execute(MODE_OPS, &[], &[&f, &c, &w]);
    report(&mut raw, "OPS: the Fp12 operations, on update_1's product and witness", &run);

    let dir = bench_dir().join("results/residue");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("one-tx.txt"), &raw).unwrap();
    fs::write(dir.join("tables.md"), &table).unwrap();
    println!("{table}");
}

fn guest_check() {
    let path = bench_dir().join("target/residue-tuples-a.bin");
    let records = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} (run the real_data test first)", path.display()));
    const L: usize = 3 * FP12_BYTES + 1;
    assert_eq!(records.len() % L, 0);
    let run = execute(MODE_BATCH, &[], &[&records]);
    assert_eq!(run.extra.len(), records.len() / L);
    let (mut valid, mut invalid) = (0, 0);
    for (r, verdicts) in records.chunks_exact(L).zip(&run.extra) {
        let f = decode_fp12(r[..576].try_into().unwrap()).unwrap();
        let c = r[576..1152].try_into().unwrap();
        let w = r[1152..1728].try_into().unwrap();
        let expected = r[1728] == 1;
        let native_witness = residue::check_bytes(&f, c, w).is_ok();
        let native_final = final_exp_is_one(&f);
        assert_eq!((native_witness, native_final), (expected, expected), "native verdicts");
        assert_eq!(verdicts & 1 == 1, native_witness, "the guest's witness check against the native one");
        assert_eq!(verdicts & 2 == 2, native_final, "the guest's final exponentiation against the native one");
        if expected {
            valid += 1
        } else {
            invalid += 1
        }
    }
    let mut out = String::new();
    report(
        &mut out,
        "CHECK (d): every product of test (a), the witness check and the final exponentiation in the guest",
        &run,
    );
    writeln!(
        out,
        "(d) {} records ({valid} valid, {invalid} invalid): the guest's verdicts equal the native ones, by both paths",
        valid + invalid
    )
    .unwrap();
    let dir = bench_dir().join("results/residue");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("guest-check.txt"), &out).unwrap();
    println!("{out}");
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("measure") => measure(),
        Some("check") => guest_check(),
        Some("profile") => agg::profile(),
        Some("measure-agg") => agg::measure(),
        Some("boundary") => agg::boundary(),
        Some("measure-handoff") => handoff::measure(),
        _ => {
            eprintln!("usage: residue-host measure | check | profile | measure-agg | boundary | measure-handoff");
            std::process::exit(2);
        }
    }
}

mod agg;
mod handoff;
