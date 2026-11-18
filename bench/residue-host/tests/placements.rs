//! Where the absent members sit, and how many there are: what the one-multiproof composition costs
//! at placements other than "every third member absent". Experimental and unaudited, like the
//! crates it measures (`bench/RESIDUE.md` §9.6).
//!
//! The bench guest `guests/residue-agg-accel` (r0vm executor) with the inputs of
//! `residue-host measure-agg`: finality update_1 against the store after update_0, signed by the
//! bench's committee substituted in the store.
//!
//! * `placements_and_participations` writes `results/residue/agg/placements.md`: the 342-signer
//!   update with its 170 absent members at 26 placements (blst's batch sum; five of them also with
//!   the chain of additions), and every number of absent members from 1 to 170, evenly spaced
//!   (the most hashes for each count). 171 absent (341 signers) must be refused on every path.
//! * `phases_of_placements_with_the_most_hashes` writes `results/residue/agg/placement-phases.txt`:
//!   the guest's own phases for seven placements that all need 425 hashes and 256 witness nodes.
//!
//! What the outputs show: every third member absent needs the most hashes and the most witness
//! nodes, but is not the costliest placement (the cost of step 1 also follows where the absent
//! positions sit in their groups of four); and the cost does not grow at every step with the
//! number of absent members (blst's batch addition works in rounds). The maximum over all
//! placements and committees is not established by this.
//!
//! Asserted: the guest's new store is the natively applied job's in every execution, the node
//! counts, the refusal. The cycle counts are written, not asserted. From an independent review of
//! the composition (18 Nov 2026). Needs RISC Zero's toolchain and r0vm, as `residue-host` itself:
//! `cargo test --release -p residue-host --test placements -- --nocapture`

use std::fs;

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, Phase},
    store::StoreCore,
    types::{LightClientBootstrap, LightClientUpdate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
    Error, Root,
};
use ferrata_spectests::testkit::{fake_committee, sign, FakeCommittee};
use residue::{check, decode_fp12, encode_fp12, inverse, witness};
use residue_agg::{NonSigners, Sum};
use risc0_zkvm::{default_executor, ExecutorEnv};
use sha2::{Digest, Sha256};

const GVR: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const TRUSTED: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

fn fixture(name: &str) -> Vec<u8> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/ferrata-spectests/fixtures/mainnet-p1899");
    let bytes = fs::read(dir.join(name)).unwrap();
    let sums = fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(name)).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want);
    bytes
}

fn results_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results/residue/agg");
    fs::create_dir_all(&dir).unwrap();
    dir
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

struct Base {
    spec: ChainSpec,
    gvr: Root,
    substituted: CompactStore,
    fake: FakeCommittee,
    finality: LightClientUpdate,
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
    let store = CompactStore::of(policy.core());
    let real = signing_committee(policy.core(), &u1, &spec).clone();
    let fake = fake_committee(512, 7);
    let mut substituted = store.clone();
    if substituted.current == CommitteeSummary::of(&real) {
        substituted.current = CommitteeSummary::of(&fake.committee);
    } else {
        substituted.next = Some(CommitteeSummary::of(&fake.committee));
    }
    Base { spec, gvr, substituted, fake, finality: as_finality(&u1) }
}

fn signed(b: &Base, bits: Vec<bool>) -> LightClientUpdate {
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &b.spec, genesis_validators_root: b.gvr, bls: &bls };
    let signing_root = sync_committee_signing_root(&b.finality, &ctx);
    let mut u = b.finality.clone();
    u.sync_aggregate.signature = sign(&b.fake, &bits, &signing_root);
    u.sync_aggregate.bits = bits;
    u
}

/// One execution of the bench guest: (user cycles, the guest's phases, the journal's extra bytes).
fn execute(words: &[u32], blobs: &[&[u8]]) -> (u64, Vec<(String, u64)>, Vec<u8>) {
    let mut e = ExecutorEnv::builder();
    e.write(&0u32).unwrap();
    for w in words {
        e.write(w).unwrap();
    }
    for x in blobs {
        e.write(&(x.len() as u32)).unwrap();
        let mut padded = x.to_vec();
        padded.resize((x.len() + 3) & !3, 0);
        e.write_slice(&padded);
    }
    let info = default_executor().execute(e.build().unwrap(), residue_methods::RESIDUE_AGG_ACCEL_ELF).unwrap();
    let (rec, extra): (Vec<(String, u64)>, Vec<u8>) = info.journal.decode().unwrap();
    (info.cycles(), rec, extra)
}

fn siblings_noted(rec: &[(String, u64)]) -> u64 {
    rec.iter().filter(|(l, _)| l.starts_with("info: multiproof siblings")).map(|(_, v)| *v).sum()
}

struct Inputs {
    store: Vec<u8>,
    update: Vec<u8>,
    agg_signers: Vec<u8>,
    c: Vec<u8>,
    w: Vec<u8>,
    c_inv: Vec<u8>,
    expected: Vec<u8>,
}

fn inputs(b: &Base, u: &LightClientUpdate) -> Inputs {
    let bits = &u.sync_aggregate.bits;
    let old = SignerWitness::build(&b.fake.committee, bits).unwrap();
    let new = NonSigners::build(&b.fake.committee, bits).unwrap();
    let cu = CompactUpdate::of(u);
    let mut job = steps::step1(&b.substituted, &cu, &old, u.signature_slot, &b.spec, &b.gvr, 1).unwrap();
    for sum in [Sum::Batch, Sum::Chain] {
        let mine = residue_agg::step1(&b.substituted, &cu, &new, u.signature_slot, &b.spec, &b.gvr, 1, sum).unwrap();
        assert_eq!(mine, job, "residue_agg::step1 = core");
    }
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!() };
    let f = decode_fp12(&product).unwrap();
    let (c, w) = witness(&f);
    assert_eq!(check(&f, &c, &w), Ok(()));
    job.step3().unwrap();
    let (next, _) = steps::apply(&b.substituted, &job, &b.spec).unwrap();
    Inputs {
        store: b.substituted.encode(),
        update: cu.encode(b.substituted.fork),
        agg_signers: new.encode(),
        c: encode_fp12(&c).to_vec(),
        w: encode_fp12(&w).to_vec(),
        c_inv: encode_fp12(&inverse(&c)).to_vec(),
        expected: next.encode(),
    }
}

/// The whole update in the guest with the multiproof witness: `path` 1 = witness check, 2 = folded;
/// `sum` 1 = blst's batch addition, 2 = the chain of additions. Asserts the guest's new store.
fn run(inp: &Inputs, path: u32, sum: u32, label: &str) -> (u64, Vec<(String, u64)>) {
    let c_inv: &[u8] = if path == 2 { &inp.c_inv } else { &[] };
    let (cycles, rec, extra) =
        execute(&[path, sum], &[&inp.store, &inp.update, &inp.agg_signers, &[], &inp.c, &inp.w, c_inv]);
    assert_eq!(extra, inp.expected, "{label}: the guest's store");
    (cycles, rec)
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn random_bits(n: usize, absent: usize, seed: u64) -> Vec<bool> {
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

/// `absent` members absent, evenly spaced (`residue-host boundary`'s placement).
fn spread(absent: usize) -> Vec<bool> {
    let mut bits = vec![true; 512];
    for j in 0..absent {
        bits[j * 512 / absent] = false;
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

/// In each of the 128 groups of four members (4g .. 4g+3): member `4g + one` absent, and in the
/// groups of `two_in` also member `4g + other`. With 42 groups in `two_in`, 170 absent members,
/// no two of them siblings and every group touched: 425 hashes and 256 witness nodes.
fn groups(one: usize, other: usize, two_in: std::ops::Range<usize>) -> Vec<bool> {
    let abs: Vec<usize> = (0..128)
        .flat_map(|g| if two_in.contains(&g) { vec![4 * g + one, 4 * g + other] } else { vec![4 * g + one] })
        .collect();
    from_absent(&abs)
}

/// The placements that need the most hashes (425) and witness nodes (256) for 170 absent members.
fn most_hashes_placements() -> Vec<(String, Vec<bool>)> {
    vec![
        ("every third (2, 5, ..., 509)".into(), (0..512).map(|i| i % 3 != 2).collect()),
        ("every absent index odd: 4g+3 in every group of four, and 4g+1 in groups 0..41".into(), groups(3, 1, 0..42)),
        (
            "every absent index odd: 4g+3 in every group of four, and 4g+1 in groups 86..127".into(),
            groups(3, 1, 86..128),
        ),
        ("every absent index even: 4g in every group of four, and 4g+2 in groups 0..41".into(), groups(0, 2, 0..42)),
        (
            "every absent index even: 4g in every group of four, and 4g+2 in groups 86..127".into(),
            groups(0, 2, 86..128),
        ),
        ("4g+1 in every group of four, and 4g+3 in groups 86..127".into(), groups(1, 3, 86..128)),
        ("4g+2 in every group of four, and 4g in groups 86..127".into(), groups(2, 0, 86..128)),
    ]
}

/// (hashes, witness nodes) of the multiproof for a bitfield of 512.
fn hashes(bits: &[bool]) -> (usize, usize) {
    let mut level: std::collections::BTreeSet<usize> =
        bits.iter().enumerate().filter(|(_, b)| !**b).map(|(i, _)| i).collect();
    let k = level.len();
    let mut total = 0;
    for _ in 0..9 {
        level = level.iter().map(|i| i >> 1).collect();
        total += level.len();
    }
    (total, total + 1 - k)
}

/// The most hashes any `k` absent members of 512 can need: the sum over the nine levels of
/// min(k, the level's nodes).
fn most_hashes(k: usize) -> usize {
    (1..=9).map(|level| k.min(512 >> level)).sum()
}

/// How many of `k` points blst's batch addition adds one by one, read off `bulk_addition.c`
/// (`POINTonE1s_accumulate`): while 16 or more points remain, an odd one is added singly and the
/// rest are added in pairs (one inversion for the round), halving their number; the fewer than 16
/// left are then added one by one. Not measured: the rule, for reading the sweep.
fn single_additions(mut k: usize) -> usize {
    let mut singles = 0;
    while k >= 16 {
        singles += k & 1;
        k /= 2;
    }
    singles + k
}

fn log(out: &mut String, line: &str) {
    println!("{line}");
    out.push_str(line);
    out.push('\n');
}

#[test]
fn placements_and_participations() {
    let b = base();
    let mut out = String::from(
        "# Where the absent members sit: the one-multiproof composition at other placements and participations\n\n\
         Written by `cargo test --release -p residue-host --test placements` (bench/residue-host/tests/placements.rs;\n\
         bench/RESIDUE.md §9.6). The bench guest guests/residue-agg-accel, r0vm executor: execution user cycles of one\n\
         whole update (finality update_1 against the store after update_0, signed by the bench's committee substituted in\n\
         the store), the non-signers as one multiproof. Experimental and unaudited.\n\n\
         ## 342 signers: 170 absent members at 26 placements (blst's batch sum)\n\n",
    );
    let mut placements = most_hashes_placements();
    placements.extend([
        ("evenly spaced (the boundary's placement)".to_string(), spread(170)),
        (
            "two per group of four (4g, 4g+2) in groups 0..41, one (4g+1) in the others".to_string(),
            from_absent(
                &(0..128)
                    .flat_map(|g| if g < 42 { vec![4 * g, 4 * g + 2] } else { vec![4 * g + 1] })
                    .collect::<Vec<_>>(),
            ),
        ),
        ("odd indices 1..339".to_string(), from_absent(&(0..170).map(|j| 2 * j + 1).collect::<Vec<_>>())),
        ("first 170 absent (one block)".to_string(), (0..512).map(|i| i >= 170).collect()),
        ("last 170 absent (one block)".to_string(), (0..512).map(|i| i < 342).collect()),
        ("middle block 171..340".to_string(), (0..512).map(|i| !(171..341).contains(&i)).collect()),
        (
            "two blocks of 85 at 0 and 256".to_string(),
            (0..512).map(|i| !((0..85).contains(&i) || (256..341).contains(&i))).collect(),
        ),
    ]);
    for seed in 11..=22u64 {
        placements.push((format!("random 170, seed {seed}"), random_bits(512, 170, seed)));
    }
    assert_eq!(placements.len(), 26);
    let mut every_third = (0u64, 0u64);
    let (mut most, mut least) = ((0u64, String::new()), (u64::MAX, String::new()));
    log(&mut out, "| placement (170 absent) | hashes | witness nodes | witness check | minus every third | folded | minus every third |\n|---|---:|---:|---:|---:|---:|---:|");
    for (label, bits) in &placements {
        assert_eq!(bits.iter().filter(|x| !**x).count(), 170, "{label}");
        let (h, s) = hashes(bits);
        let inp = inputs(&b, &signed(&b, bits.clone()));
        let mut cyc = [0u64; 2];
        for (j, path) in [1u32, 2].into_iter().enumerate() {
            let (cycles, rec) = run(&inp, path, 1, label);
            assert_eq!(siblings_noted(&rec) as usize, s, "{label}: witness nodes");
            cyc[j] = cycles;
        }
        if label.starts_with("every third") {
            every_third = (cyc[0], cyc[1]);
        }
        if cyc[0] > most.0 {
            most = (cyc[0], label.clone());
        }
        if cyc[0] < least.0 {
            least = (cyc[0], label.clone());
        }
        log(
            &mut out,
            &format!(
                "| {label} | {h} | {s} | {} | {} | {} | {} |",
                cyc[0],
                cyc[0] as i64 - every_third.0 as i64,
                cyc[1],
                cyc[1] as i64 - every_third.1 as i64
            ),
        );
    }
    log(
        &mut out,
        &format!(
            "\nOf these 26 placements the costliest is \"{}\" ({} user cycles with the witness check, {} more than every third) and the cheapest \"{}\" ({}). The maximum over all placements and committees is not established.",
            most.1,
            most.0,
            most.0 as i64 - every_third.0 as i64,
            least.1,
            least.0
        ),
    );

    // The chain of additions (the conservative sum) at five of the placements.
    log(&mut out, "\n## 342 signers, the non-signers summed by the chain of additions\n\n| placement (170 absent) | witness check | minus every third | folded | minus every third |\n|---|---:|---:|---:|---:|");
    let mut chain_every_third = (0u64, 0u64);
    for i in [0usize, 1, 3, 7, 14] {
        let (label, bits) = &placements[i];
        let inp = inputs(&b, &signed(&b, bits.clone()));
        let cyc = [run(&inp, 1, 2, label).0, run(&inp, 2, 2, label).0];
        if i == 0 {
            chain_every_third = (cyc[0], cyc[1]);
        }
        log(
            &mut out,
            &format!(
                "| {label} | {} | {} | {} | {} |",
                cyc[0],
                cyc[0] as i64 - chain_every_third.0 as i64,
                cyc[1],
                cyc[1] as i64 - chain_every_third.1 as i64
            ),
        );
    }

    // Every number of absent members from 1 to 170, evenly spaced: the most hashes for each count
    // (asserted). The witness check at every count, folded at a few.
    log(&mut out, "\n## 1 to 170 absent members, evenly spaced (the most hashes for each count; blst's batch sum)\n\n\"single additions\": how many of the points blst's batch addition adds one by one, by the rule of its bulk_addition.c (rounds of pairs while 16 or more points remain); not measured.\n\n| absent | signers | hashes | witness nodes | single additions | witness check | change from one fewer | folded |\n|---:|---:|---:|---:|---:|---:|---:|---:|");
    let (mut last, mut drops, mut most_k) = (0u64, Vec::new(), (0u64, 0usize));
    for k in 1usize..=170 {
        let bits = spread(k);
        let (h, s) = hashes(&bits);
        assert_eq!(h, most_hashes(k), "evenly spaced: the most hashes for {k} absent");
        let label = format!("{k} absent");
        let inp = inputs(&b, &signed(&b, bits));
        let (a, _) = run(&inp, 1, 1, &label);
        let folded = if [1usize, 99, 126, 127, 128, 129, 167, 169, 170].contains(&k) {
            run(&inp, 2, 1, &label).0.to_string()
        } else {
            String::new()
        };
        let change = if k == 1 { String::new() } else { (a as i64 - last as i64).to_string() };
        if k > 1 && a <= last {
            drops.push((k, a as i64 - last as i64));
        }
        if a > most_k.0 {
            most_k = (a, k);
        }
        log(
            &mut out,
            &format!("| {k} | {} | {h} | {s} | {} | {a} | {change} | {folded} |", 512 - k, single_additions(k)),
        );
        last = a;
    }
    log(
        &mut out,
        &format!(
            "\nOver 1 to 170 absent members, evenly spaced: the costliest count is {} ({} user cycles with the witness check); counts whose cost is not above that of one fewer (absent, change): {:?}",
            most_k.1, most_k.0, drops
        ),
    );

    // 171 absent: refused (BelowSupermajority) on every path, with either witness.
    let refused = signed(&b, (0..512).map(|i| i % 3 != 2 && i != 0).collect());
    assert_eq!(refused.sync_aggregate.participants(), 341);
    let cu = CompactUpdate::of(&refused);
    let old = SignerWitness::build(&b.fake.committee, &refused.sync_aggregate.bits).unwrap();
    let new = NonSigners::build(&b.fake.committee, &refused.sync_aggregate.bits).unwrap();
    let native_core = steps::step1(&b.substituted, &cu, &old, refused.signature_slot, &b.spec, &b.gvr, 1).unwrap_err();
    let native_agg =
        residue_agg::step1(&b.substituted, &cu, &new, refused.signature_slot, &b.spec, &b.gvr, 1, Sum::Batch)
            .unwrap_err();
    assert_eq!(native_core.code(), native_agg.code());
    let below = Error::BelowSupermajority.code();
    // c, w and c^-1 of an accepted case (never reached).
    let any = inputs(&b, &signed(&b, spread(170)));
    for (path, agg, signers) in [
        (1u32, 0u32, old.encode()),
        (1, 1, new.encode()),
        (2, 1, new.encode()),
        (2, 2, new.encode()),
        (0, 1, new.encode()),
    ] {
        let (_, _, extra) = execute(
            &[path, agg],
            &[&any.store, &cu.encode(b.substituted.fork), &signers, &[], &any.c, &any.w, &any.c_inv],
        );
        assert_eq!(extra, vec![0xff, below as u8], "171 absent, path {path}, aggregation {agg}");
    }
    log(
        &mut out,
        &format!(
            "\n## 171 absent (341 signers)\n\nRefused natively by both compositions (code {}) and in the guest on 5 combinations of path and witness (code {below}).",
            native_core.code()
        ),
    );
    fs::write(results_dir().join("placements.md"), &out).unwrap();
}

/// Where the cost of placements with the same hashes and witness nodes differs: the guest's own
/// phases (the witness check, blst's batch sum).
#[test]
fn phases_of_placements_with_the_most_hashes() {
    let b = base();
    let mut out = String::from(
        "# The guest's phases for placements of 170 absent members that all need 425 hashes and 256 witness nodes\n\n\
         Written by `cargo test --release -p residue-host --test placements` (bench/residue-host/tests/placements.rs;\n\
         bench/RESIDUE.md §9.6). The whole 342-signer finality update in the bench guest guests/residue-agg-accel (r0vm\n\
         executor), the witness check, the non-signers as one multiproof summed by blst's batch addition. User cycles per\n\
         phase, as `residue-host measure-agg` prints them (results/residue/agg/one-tx.txt). Experimental and unaudited.\n\n",
    );
    let mut step1 = Vec::new();
    for (label, bits) in most_hashes_placements() {
        assert_eq!(bits.iter().filter(|x| !**x).count(), 170, "{label}");
        let (h, s) = hashes(&bits);
        assert_eq!((h, s), (425, 256), "{label}");
        let inp = inputs(&b, &signed(&b, bits));
        let (cycles, rec) = run(&inp, 1, 1, &label);
        out.push_str(&format!("### {label} (hashes {h}, witness nodes {s}): {cycles} user cycles\n"));
        for (l, v) in &rec {
            out.push_str(&format!("{v:>12}  {l}\n"));
        }
        let s1: u64 = rec.iter().filter(|(l, _)| l.starts_with("S1 ")).map(|(_, v)| *v).sum();
        step1.push((label, cycles, s1));
    }
    out.push_str("\n### Step 1 against every third\n");
    let (_, base_cycles, base_s1) = step1[0].clone();
    for (label, cycles, s1) in &step1 {
        out.push_str(&format!(
            "{:>8} whole execution, {:>8} step 1 (S1)  {label}\n",
            *cycles as i64 - base_cycles as i64,
            *s1 as i64 - base_s1 as i64
        ));
    }
    println!("{out}");
    fs::write(results_dir().join("placement-phases.txt"), &out).unwrap();
}
