//! Tests (a) and (b) of the residue-witness check on real data.
//!
//! (a) Every signature check the core makes on the pinned official `sync` vectors
//! (consensus-specs v1.7.0-beta.4, `scripts/fetch-vectors.sh`), on the mainnet replay
//! (`fixtures/mainnet-p1899`) and on the Sepolia chain through the Gloas fork
//! (`fixtures/sepolia-gloas-2026-11-10`) goes through a backend that wraps `Blst`: where the
//! current path reaches the pairing, the wrapper recomputes the Miller-loop product
//! (`Blst::miller_half`), and asserts that the final exponentiation (`Blst::final_half`) and the
//! witness check with the honest witness give the backend's verdict. Both stores run: the spec
//! store (`fast_aggregate_verify`) and the policy store (`verify_checked`), with the Full and the
//! OnChain committee check. On the mainnet updates the step API's product (step 2, the product
//! the program's `Finish` and `VerifyAll` exponentiate) is checked the same way.
//! The products and witnesses are written to `bench/target/residue-tuples-a.bin` for test (d)
//! (the guest's check on the same inputs, `residue-host check`).
//!
//! (b) Mainnet update_1 against the store after update_0, through the step API, with its
//! signature, its message, its participant set, `c` or `w` tampered with: refused. Two witnesses
//! that satisfy the equation `f·w == c^λ` for a product that is NOT valid show that each side
//! condition (`c ≠ 0`, `w ∈ Fp6`) is needed.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::PathBuf,
};

use blst::{blst_fp12, blst_fp12_inverse, blst_fp_from_bendian};
use ferrata_core::{
    bls::{Aggregation, BlsBackend, Blst, FP12_BYTES as CORE_FP12_BYTES},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, Phase, StepError},
    store::{LightClientStore, StoreCore},
    types::{BlsSignature, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{CompactStore, CompactUpdate, SignerWitness},
    Error, Root,
};
use ferrata_spectests::{cases, run_sync_case, run_sync_case_with};
use num_bigint::BigUint;
use residue::*;
use sha2::{Digest, Sha256};

const _: () = assert!(CORE_FP12_BYTES == FP12_BYTES);

// ---------------------------------------------------------------------------------------------
// The wrapping backend
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Seen {
    /// Signature checks that reached the pairing, by verdict.
    accepted: usize,
    refused_by_pairing: usize,
    /// Signature checks refused before the pairing (the same code in both paths), by error code.
    refused_before: BTreeMap<u16, usize>,
    /// Distinct products (f, verdict).
    distinct: BTreeSet<([u8; FP12_BYTES], bool)>,
}

struct Residue {
    inner: Blst,
    seen: RefCell<Seen>,
}

impl Residue {
    fn new() -> Self {
        Self { inner: Blst::new(Aggregation::Auto), seen: RefCell::new(Seen::default()) }
    }

    /// Recompute the product the verdict came from and compare the two final checks on it.
    fn compare(
        &self,
        verdict: Result<(), Error>,
        composition: Blst,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        let mut seen = self.seen.borrow_mut();
        if !matches!(verdict, Ok(()) | Err(Error::SignatureInvalid)) {
            *seen.refused_before.entry(verdict.unwrap_err().code()).or_default() += 1;
            return verdict;
        }
        match composition.miller_half(committee, bits, signing_root, signature) {
            Err(e) => {
                // The identity signature: refused before the pairing, by the same code.
                assert_eq!(verdict, Err(e));
                *seen.refused_before.entry(e.code()).or_default() += 1;
            }
            Ok(bytes) => {
                let current = Blst::final_half(&bytes);
                assert_eq!(current, verdict, "the recomputed product gives the backend's verdict");
                let f = decode_fp12(&bytes).expect("the core's encoding is canonical");
                let (c, w) = witness(&f);
                let residue = check_bytes(&f, &encode_fp12(&c), &encode_fp12(&w));
                assert_eq!(residue.is_ok(), current.is_ok(), "witness check against final exponentiation");
                if current.is_ok() {
                    seen.accepted += 1;
                } else {
                    seen.refused_by_pairing += 1;
                }
                seen.distinct.insert((bytes, current.is_ok()));
            }
        }
        verdict
    }
}

impl BlsBackend for Residue {
    fn fast_aggregate_verify(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        let verdict = self.inner.fast_aggregate_verify(committee, bits, signing_root, signature);
        // The spec path sums the signers' keys.
        self.compare(verdict, Blst::new(Aggregation::Additive), committee, bits, signing_root, signature)
    }

    fn check_committee(&self, committee: &SyncCommittee) -> Result<(), Error> {
        self.inner.check_committee(committee)
    }

    fn check_committee_onchain(&self, committee: &SyncCommittee) -> Result<(), Error> {
        self.inner.check_committee_onchain(committee)
    }

    fn verify_checked(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        let verdict = self.inner.verify_checked(committee, bits, signing_root, signature);
        self.compare(verdict, self.inner, committee, bits, signing_root, signature)
    }
}

// ---------------------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------------------

fn fixtures(dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(dir)
}

/// A fixture file, checked against its directory's pinned `SHA256SUMS`.
fn fixture(dir: &str, name: &str) -> Vec<u8> {
    let bytes = fs::read(fixtures(dir).join(name)).unwrap();
    let sums = fs::read_to_string(fixtures(dir).join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{dir}/{name} does not match SHA256SUMS");
    bytes
}

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

const MAINNET_GVR: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const MAINNET_TRUSTED: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

fn mainnet_data() -> (LightClientBootstrap, Vec<LightClientUpdate>) {
    let d = "mainnet-p1899";
    let bootstrap = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let updates = (0..5)
        .map(|i| LightClientUpdate::decode(&fixture(d, &format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap())
        .collect();
    (bootstrap, updates)
}

/// Sepolia's fork schedule (as `ferrata-spectests/tests/sepolia_gloas.rs`).
fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

const SEPOLIA_GVR: &str = "0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
const SEPOLIA_TRUSTED: &str = "0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";

/// The Gloas `LightClientFinalityUpdate`, as `sepolia_gloas.rs` decodes it.
fn sepolia_finality_update(b: &[u8]) -> LightClientUpdate {
    const H: usize = 112 + 32 + 11 * 32;
    assert_eq!(b.len(), 2 * H + 9 * 32 + 160 + 8);
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).unwrap(),
        finality_branch: b[2 * H..2 * H + 288].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into().unwrap()),
    }
}

fn write_tuples(seen: &Seen, path: &str) {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(path);
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    let mut file = fs::File::create(&out).unwrap();
    for (bytes, valid) in &seen.distinct {
        let f = decode_fp12(bytes).unwrap();
        let (c, w) = witness(&f);
        file.write_all(bytes).unwrap();
        file.write_all(&encode_fp12(&c)).unwrap();
        file.write_all(&encode_fp12(&w)).unwrap();
        file.write_all(&[u8::from(*valid)]).unwrap();
    }
    println!("wrote {} records to bench/{path}", seen.distinct.len());
}

// ---------------------------------------------------------------------------------------------
// (a)
// ---------------------------------------------------------------------------------------------

#[test]
fn every_signature_check_of_the_vectors_and_the_real_chains_agrees() {
    let bls = Residue::new();
    let mut report = Vec::new();

    // Official sync vectors, both stores, Full and OnChain committee checks.
    let all = cases("sync").expect("run scripts/fetch-vectors.sh first");
    assert_eq!(all.len(), 52);
    for (_, dir) in &all {
        run_sync_case(dir, &bls).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        run_sync_case_with(dir, &bls, Adoption::OnChain).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    }
    let s = bls.seen.borrow();
    report.push(format!(
        "official sync vectors (52 cases x Full and OnChain, spec and policy stores): {} reached the pairing \
         ({} accepted, {} refused by it), refused before it by code {:?}; {} distinct products",
        s.accepted + s.refused_by_pairing,
        s.accepted,
        s.refused_by_pairing,
        s.refused_before,
        s.distinct.len()
    ));
    let before = (s.accepted, s.refused_by_pairing, s.distinct.len());
    drop(s);

    // Mainnet: bootstrap and five updates, both stores; and the step API's product.
    let spec = mainnet();
    let gvr = root(MAINNET_GVR);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let (bootstrap, updates) = mainnet_data();
    let trusted = root(MAINNET_TRUSTED);
    let mut policy =
        FinalizedStore::initialize_with(&trusted, &bootstrap, LcFork::Electra, &spec, &bls, Adoption::OnChain).unwrap();
    let mut reference = LightClientStore::initialize(&trusted, &bootstrap, LcFork::Electra, &spec).unwrap();
    let mut step_checks = 0;
    for (i, update) in updates.iter().enumerate() {
        let compact = CompactStore::of(policy.core());
        let committee = signing_committee(policy.core(), update, &spec).clone();
        let witness_keys = SignerWitness::build(&committee, &update.sync_aggregate.bits).unwrap();
        let mut job = steps::step1(&compact, &CompactUpdate::of(update), &witness_keys, update.signature_slot, &spec, &gvr, 1)
            .unwrap();
        job.step2().unwrap();
        let Phase::FinalExp { product } = job.phase.clone() else { panic!("step 2 leaves the product") };
        let f = decode_fp12(&product).unwrap();
        let (c, w) = witness(&f);
        assert_eq!(check(&f, &c, &w), Ok(()), "mainnet update {i}: step API product");
        assert_eq!(job.clone().step3(), Ok(()));
        step_checks += 1;
        policy.process_update(update, update.signature_slot + 1, &ctx).unwrap();
        reference.process_update(update, update.signature_slot + 1, &ctx).unwrap();
    }
    let s = bls.seen.borrow();
    report.push(format!(
        "mainnet p1899 (5 updates, policy and spec stores): {} signature checks reached the pairing, all accepted \
         alike: {}; the step API's product of each update: {step_checks} accepted by both",
        s.accepted + s.refused_by_pairing - before.0 - before.1,
        s.refused_by_pairing == before.1
    ));
    assert_eq!(s.refused_by_pairing, before.1, "no mainnet update is refused");
    let before = (s.accepted, s.refused_by_pairing, s.distinct.len());
    drop(s);

    // Sepolia: bootstrap, periods 1378 to 1409 through the Gloas fork, the finality update.
    let spec = sepolia();
    let ctx = Context { spec: &spec, genesis_validators_root: root(SEPOLIA_GVR), bls: &bls };
    let d = "sepolia-gloas-2026-11-10";
    let boot = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let trusted = root(SEPOLIA_TRUSTED);
    let mut policy =
        FinalizedStore::initialize_with(&trusted, &boot, LcFork::Electra, &spec, &bls, Adoption::OnChain).unwrap();
    let mut reference = LightClientStore::initialize(&trusted, &boot, LcFork::Electra, &spec).unwrap();
    for period in 1378..=1409u64 {
        if period == 1379 {
            policy.upgrade(LcFork::Gloas, &spec).unwrap();
            reference.upgrade(LcFork::Gloas, &spec).unwrap();
        }
        let layout = if period < 1379 { LcFork::Electra } else { LcFork::Gloas };
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{period}.ssz")), layout, 512).unwrap();
        policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap_or_else(|e| panic!("period {period}: {e}"));
        reference.process_update(&u, u.signature_slot + 1, &ctx).unwrap();
    }
    let fu = sepolia_finality_update(&fixture(d, "finality_update.ssz"));
    policy.process_update(&fu, fu.signature_slot + 1, &ctx).unwrap();
    reference.process_update(&fu, fu.signature_slot + 1, &ctx).unwrap();
    let s = bls.seen.borrow();
    report.push(format!(
        "sepolia gloas (32 period updates and a finality update, policy and spec stores): {} signature checks \
         reached the pairing, all accepted alike: {}",
        s.accepted + s.refused_by_pairing - before.0 - before.1,
        s.refused_by_pairing == before.1
    ));
    assert_eq!(s.refused_by_pairing, before.1, "no Sepolia update is refused");
    report.push(format!(
        "total: {} signature checks reached the pairing ({} accepted, {} refused), the witness check agreeing \
         on every one; {} refused before it; {} distinct products",
        s.accepted + s.refused_by_pairing,
        s.accepted,
        s.refused_by_pairing,
        s.refused_before.values().sum::<usize>(),
        s.distinct.len()
    ));
    for line in &report {
        println!("(a) {line}");
    }
    write_tuples(&s, "target/residue-tuples-a.bin");
}

fn signing_committee<'a>(full: &'a StoreCore, update: &LightClientUpdate, spec: &ChainSpec) -> &'a SyncCommittee {
    let store_period = spec.period_at_slot(full.finalized_header.beacon.slot);
    if spec.period_at_slot(update.signature_slot) == store_period {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    }
}

// ---------------------------------------------------------------------------------------------
// (b)
// ---------------------------------------------------------------------------------------------

struct Setup {
    spec: ChainSpec,
    gvr: Root,
    compact: CompactStore,
    committee: SyncCommittee,
    updates: Vec<LightClientUpdate>,
}

fn setup() -> Setup {
    let spec = mainnet();
    let gvr = root(MAINNET_GVR);
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let (bootstrap, updates) = mainnet_data();
    let mut policy =
        FinalizedStore::initialize_with(&root(MAINNET_TRUSTED), &bootstrap, LcFork::Electra, &spec, &bls, Adoption::OnChain)
            .unwrap();
    policy.process_update(&updates[0], updates[0].signature_slot + 1, &ctx).unwrap();
    let compact = CompactStore::of(policy.core());
    let committee = signing_committee(policy.core(), &updates[1], &spec).clone();
    Setup { spec, gvr, compact, committee, updates }
}

/// Steps 1 and 2 of the program for `update` against the store after update_0, the signer
/// witness built honestly for its bits; returns the job after step 2 and its product.
fn product(s: &Setup, update: &LightClientUpdate) -> (steps::Job, blst_fp12) {
    let w = SignerWitness::build(&s.committee, &update.sync_aggregate.bits).unwrap();
    let mut job =
        steps::step1(&s.compact, &CompactUpdate::of(update), &w, update.signature_slot, &s.spec, &s.gvr, 1).unwrap();
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!("step 2 leaves the product") };
    (job, decode_fp12(&product).unwrap())
}

fn fp12_scalar(n: &BigUint) -> blst_fp12 {
    let mut f = blst_fp12::default();
    let mut b = [0u8; 48];
    let bytes = n.to_bytes_be();
    b[48 - bytes.len()..].copy_from_slice(&bytes);
    // SAFETY: `b` holds the 48 bytes blst reads.
    unsafe { blst_fp_from_bendian(&mut f.fp6[0].fp2[0].fp[0], b.as_ptr()) };
    f
}

/// A primitive cube root of unity of Fp, embedded in Fp12: 2^((p-1)/3) (2 is not a cube mod p).
fn cube_root_of_unity() -> blst_fp12 {
    let p = BigUint::parse_bytes(
        b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
        16,
    )
    .unwrap();
    let z = pow_be(&fp12_scalar(&BigUint::from(2u8)), &((p - 1u8) / 3u8).to_bytes_be());
    assert!(!eq(&z, &one()) && eq(&pow_be(&z, &[3]), &one()));
    z
}

fn inverse(a: &blst_fp12) -> blst_fp12 {
    let mut out = blst_fp12::default();
    // SAFETY: valid destination and source.
    unsafe { blst_fp12_inverse(&mut out, a) };
    out
}

#[test]
fn tampered_inputs_are_refused() {
    let s = setup();
    let u1 = &s.updates[1];
    let (job, f) = product(&s, u1);
    let (c, w) = witness(&f);
    let (cb, wb) = (encode_fp12(&c), encode_fp12(&w));
    assert_eq!(check_bytes(&f, &cb, &wb), Ok(()), "update_1 as served");
    assert_eq!(job.clone().step3(), Ok(()));
    let mut lines = vec![];

    // The update tampered with: the current path refuses at the final exponentiation, the
    // witness path with the honest witness for the tampered product, and with update_1's.
    let mut variants: Vec<(&str, LightClientUpdate)> = vec![];
    let mut t = u1.clone();
    t.sync_aggregate.signature = s.updates[2].sync_aggregate.signature;
    variants.push(("signature: update_2's", t));
    let mut t = u1.clone();
    t.attested_header.beacon.proposer_index ^= 1;
    variants.push(("message: the attested header's proposer index changed", t));
    let mut t = u1.clone();
    let k = t.sync_aggregate.bits.iter().position(|b| *b).unwrap();
    t.sync_aggregate.bits[k] = false;
    variants.push(("participants: a signer marked absent", t));
    let mut t = u1.clone();
    let k = t.sync_aggregate.bits.iter().position(|b| !*b).unwrap();
    t.sync_aggregate.bits[k] = true;
    variants.push(("participants: an absent member marked a signer", t));
    for (name, t) in &variants {
        let (job, g) = product(&s, t);
        assert_eq!(job.clone().step3(), Err(StepError::Core(Error::SignatureInvalid)), "{name}");
        let (gc, gw) = witness(&g);
        let honest = check(&g, &gc, &gw);
        assert!(honest.is_err(), "{name}");
        assert_eq!(check(&g, &c, &w), Err(Refusal::Mismatch), "{name}: update_1's witness");
        lines.push(format!("{name}: final exponentiation refuses; witness check refuses ({honest:?} with the honest witness, Mismatch with update_1's)"));
    }

    // c and w tampered with, for update_1's (valid) product.
    let zeta = cube_root_of_unity();
    let (_, f2) = product(&s, &s.updates[2]);
    let (c2, w2) = witness(&f2);
    let mut flipped = cb;
    flipped[47] ^= 1;
    let mut non_canonical = cb;
    non_canonical[..48].copy_from_slice(&add_p(&cb[..48]));
    let mut w_out = w;
    w_out.fp6[1] = one().fp6[0];
    let mut w_non_canonical = wb;
    w_non_canonical[..48].copy_from_slice(&add_p(&wb[..48]));
    let cases: Vec<(&str, [u8; FP12_BYTES], [u8; FP12_BYTES], Refusal)> = vec![
        ("c: one bit flipped", flipped, wb, Refusal::Mismatch),
        ("c: update_2's", encode_fp12(&c2), wb, Refusal::Mismatch),
        ("c: zero", [0u8; FP12_BYTES], wb, Refusal::ZeroResidue),
        ("c: a coefficient plus p (non-canonical)", non_canonical, wb, Refusal::NonCanonical),
        ("w: times a cube root of unity", cb, encode_fp12(&mul(&w, &zeta)), Refusal::Mismatch),
        ("w: update_2's", cb, encode_fp12(&w2), Refusal::Mismatch),
        ("w: with a coefficient outside Fp6", cb, encode_fp12(&w_out), Refusal::ScalingNotInFp6),
        ("w: zero", cb, [0u8; FP12_BYTES], Refusal::Mismatch),
        ("w: a coefficient plus p (non-canonical)", cb, w_non_canonical, Refusal::NonCanonical),
    ];
    for (name, cx, wx, want) in &cases {
        assert_eq!(check_bytes(&f, cx, wx), Err(*want), "{name}");
        lines.push(format!("{name}: refused, {want:?}"));
    }
    // w = 1 passes only if f has no 27- or POLY-part.
    let w_is_one = eq(&w, &one());
    let r = check(&f, &c, &one());
    assert_eq!(r.is_ok(), w_is_one);
    lines.push(format!("w: one instead of the honest scaling factor: {r:?} (the honest w is one: {w_is_one})"));
    // Witnesses are not unique: c times a cube root of unity is another (λ = 3·POLY²·r). This
    // does not weaken the check; it means c must not be used as an identifier.
    assert_eq!(check(&f, &mul(&c, &zeta), &w), Ok(()));
    lines.push("c times a cube root of unity: accepted (another valid witness; witnesses are not unique)".into());

    // Each side condition is needed: for the product of update_1 with update_2's signature (not
    // valid), two witnesses satisfy the equation alone.
    let mut t = u1.clone();
    t.sync_aggregate.signature = s.updates[2].sync_aggregate.signature;
    let (_, bad) = product(&s, &t);
    assert!(!final_exp_is_one(&bad));
    let zero = blst_fp12::default();
    assert!(equation_holds(&bad, &zero, &zero), "0 · 0 == 0^λ");
    assert_eq!(check(&bad, &zero, &zero), Err(Refusal::ZeroResidue));
    lines.push("necessity of c != 0: c = 0, w = 0 satisfy f·w == c^λ for an invalid f; refused, ZeroResidue".into());
    let any_c = mul(&c2, &zeta);
    let forged_w = mul(&pow_lambda(&any_c), &inverse(&bad));
    assert!(equation_holds(&bad, &any_c, &forged_w));
    assert_eq!(check(&bad, &any_c, &forged_w), Err(Refusal::ScalingNotInFp6));
    lines.push("necessity of w in Fp6: w = c^λ/f satisfies f·w == c^λ for an invalid f and any c; refused, ScalingNotInFp6".into());
    for l in &lines {
        println!("(b) {l}");
    }
    println!("(b) {} tampered or forged inputs refused; 2 informational cases", lines.len() - 2);
}

/// A 48-byte big-endian coefficient plus p (still below 2^384 for these values).
fn add_p(be: &[u8]) -> [u8; 48] {
    let p = BigUint::parse_bytes(
        b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
        16,
    )
    .unwrap();
    let v = BigUint::from_bytes_be(be) + p;
    let bytes = v.to_bytes_be();
    assert!(bytes.len() <= 48);
    let mut out = [0u8; 48];
    out[48 - bytes.len()..].copy_from_slice(&bytes);
    out
}
