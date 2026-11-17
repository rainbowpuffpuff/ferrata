//! Test (c): differential, native. For 11,034 pairing products, valid and invalid, the witness
//! check with the honest witness accepts exactly when the full final exponentiation gives one.
//! Each product's ground truth is computed twice, by blst's final exponentiation and by an
//! independent square-and-multiply `f^h` with `h = (p^12 − 1)/r`, and both must agree with the
//! verdict the product was built to have. For an invalid product the honest algorithm finds no
//! witness (its output is refused); that no witness exists at all is the soundness argument
//! (`bench/RESIDUE.md`), not something a test can show.
//!
//! Deterministic: every case is built from its category and index. Run with
//! `cargo test --release -p residue --test differential -- --nocapture`.

use std::{collections::BTreeMap, sync::Mutex, thread};

use blst::{
    blst_final_exp, blst_fp12, blst_fp_from_bendian, blst_hash_to_g2, blst_miller_loop, blst_p1, blst_p1_affine,
    blst_p1_affine_generator, blst_p1_cneg, blst_p1_deserialize, blst_p1_from_affine, blst_p1_to_affine, blst_p2,
    blst_p2_affine, blst_p2_deserialize, blst_p2_to_affine,
    min_pk::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature},
    BLST_ERROR,
};
use residue::*;

const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

struct Rng(u64);

impl Rng {
    fn new(category: u64, index: u64) -> Self {
        Rng(category.wrapping_mul(0x1000_0000_01b3) ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x5eed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for chunk in out.chunks_mut(8) {
            let v = self.next().to_be_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
        out
    }
    fn fp12(&mut self) -> blst_fp12 {
        let mut f = blst_fp12::default();
        for fp in f.fp6.iter_mut().flat_map(|x| x.fp2.iter_mut()).flat_map(|x| x.fp.iter_mut()) {
            let mut b: [u8; 48] = self.bytes();
            b[0] &= 0x0f; // below 2^380 < p
                          // SAFETY: `b` holds the 48 bytes blst reads.
            unsafe { blst_fp_from_bendian(fp, b.as_ptr()) };
        }
        f
    }
    fn key(&mut self) -> SecretKey {
        SecretKey::key_gen(&self.bytes::<32>(), &[]).unwrap()
    }
}

fn g1(pk: &PublicKey) -> blst_p1_affine {
    let mut out = blst_p1_affine::default();
    // SAFETY: 96 serialized bytes, valid destination.
    assert_eq!(unsafe { blst_p1_deserialize(&mut out, pk.serialize().as_ptr()) }, BLST_ERROR::BLST_SUCCESS);
    out
}

fn neg_g1(p: &blst_p1_affine) -> blst_p1_affine {
    let (mut j, mut out) = (blst_p1::default(), blst_p1_affine::default());
    // SAFETY: valid points.
    unsafe {
        blst_p1_from_affine(&mut j, p);
        blst_p1_cneg(&mut j, true);
        blst_p1_to_affine(&mut out, &j);
    }
    out
}

fn g2_sig(sig: &Signature) -> blst_p2_affine {
    let mut out = blst_p2_affine::default();
    // SAFETY: 192 serialized bytes, valid destination.
    assert_eq!(unsafe { blst_p2_deserialize(&mut out, sig.serialize().as_ptr()) }, BLST_ERROR::BLST_SUCCESS);
    out
}

fn hash(msg: &[u8]) -> blst_p2_affine {
    let (mut p, mut out) = (blst_p2::default(), blst_p2_affine::default());
    // SAFETY: valid buffers and lengths.
    unsafe {
        blst_hash_to_g2(&mut p, msg.as_ptr(), msg.len(), DST.as_ptr(), DST.len(), core::ptr::null(), 0);
        blst_p2_to_affine(&mut out, &p);
    }
    out
}

fn ml(q: &blst_p2_affine, p: &blst_p1_affine) -> blst_fp12 {
    let mut out = blst_fp12::default();
    // SAFETY: valid points.
    unsafe { blst_miller_loop(&mut out, q, p) };
    out
}

/// The product the core's `miller` computes: `ML(H(m), −key) · ML(signature, g1)`.
fn product(key: &blst_p1_affine, msg: &[u8], sig: &blst_p2_affine) -> blst_fp12 {
    // SAFETY: blst's static generator.
    let gen = unsafe { *blst_p1_affine_generator() };
    mul(&ml(&hash(msg), &neg_g1(key)), &ml(sig, &gen))
}

fn sign(sk: &SecretKey, msg: &[u8]) -> Signature {
    sk.sign(msg, DST, &[])
}

fn r_bytes() -> [u8; 32] {
    let mut r = [0u8; 32];
    hex_to(&mut r, "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001");
    r
}

fn hex_to(out: &mut [u8], s: &str) {
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
    }
}

/// An element of the subgroup of order 27·POLY: `y^(F·r)` (its order divides h/F = 27·POLY).
fn small_part(rng: &mut Rng) -> blst_fp12 {
    pow_be(&pow_be(&rng.fp12(), &F), &r_bytes())
}

/// A non-trivial element of order r: a reduced pairing value.
fn order_r(rng: &mut Rng) -> blst_fp12 {
    let sk = rng.key();
    let mut t = blst_fp12::default();
    let f = ml(&hash(&rng.bytes::<32>()), &g1(&sk.sk_to_pk()));
    // SAFETY: valid destination and source.
    unsafe { blst_final_exp(&mut t, &f) };
    assert!(!eq(&t, &one()));
    t
}

/// The categories: name, number of cases, whether its products are valid.
const CATEGORIES: [(&str, u64, bool); 9] = [
    ("V1 valid signature, random key and message", 3000, true),
    ("V2 valid aggregate signature, 2-16 signers", 1000, true),
    ("V3 constructed valid: x^r times an element of order dividing 27*POLY", 1000, true),
    ("V4 edge valid: 1, pure 27*POLY, 3-part and POLY-part elements", 32, true),
    ("I1 invalid: signature on another message", 2000, false),
    ("I2 invalid: signature by another key", 1000, false),
    ("I3 invalid: random Fp12 element", 2000, false),
    ("I4 invalid only in the r-part: valid product times an element of order r", 1000, false),
    ("I5 edge invalid: zero, and an element of order r alone", 0, false),
];

fn build(category: usize, index: u64) -> blst_fp12 {
    let mut rng = Rng::new(category as u64, index);
    match category {
        0 => {
            let (sk, msg) = (rng.key(), rng.bytes::<32>());
            product(&g1(&sk.sk_to_pk()), &msg, &g2_sig(&sign(&sk, &msg)))
        }
        1 => {
            let k = 2 + (rng.next() % 15) as usize;
            let msg = rng.bytes::<32>();
            let sks: Vec<SecretKey> = (0..k).map(|_| rng.key()).collect();
            let pks: Vec<PublicKey> = sks.iter().map(SecretKey::sk_to_pk).collect();
            let sigs: Vec<Signature> = sks.iter().map(|sk| sign(sk, &msg)).collect();
            let apk = AggregatePublicKey::aggregate(&pks.iter().collect::<Vec<_>>(), false).unwrap().to_public_key();
            let asig = AggregateSignature::aggregate(&sigs.iter().collect::<Vec<_>>(), false).unwrap().to_signature();
            product(&g1(&apk), &msg, &g2_sig(&asig))
        }
        2 => {
            let x = rng.fp12();
            mul(&pow_be(&x, &r_bytes()), &small_part(&mut rng))
        }
        3 => match index % 4 {
            0 => one(),
            1 => small_part(&mut rng),
            2 => pow_be(&small_part(&mut rng), &POLY.to_be_bytes()), // order divides 27
            _ => pow_be(&small_part(&mut rng), &[27]),               // order divides POLY
        },
        4 => {
            let (sk, msg, other) = (rng.key(), rng.bytes::<32>(), rng.bytes::<32>());
            product(&g1(&sk.sk_to_pk()), &other, &g2_sig(&sign(&sk, &msg)))
        }
        5 => {
            let (sk, other, msg) = (rng.key(), rng.key(), rng.bytes::<32>());
            product(&g1(&other.sk_to_pk()), &msg, &g2_sig(&sign(&sk, &msg)))
        }
        6 => rng.fp12(),
        7 => {
            let (sk, msg) = (rng.key(), rng.bytes::<32>());
            let valid = product(&g1(&sk.sk_to_pk()), &msg, &g2_sig(&sign(&sk, &msg)));
            mul(&valid, &order_r(&mut rng))
        }
        _ => unreachable!(),
    }
}

#[derive(Default)]
struct Tally {
    cases: usize,
    accepted: usize,
    refused: BTreeMap<String, usize>,
    /// Valid products by the order of their 3-part (1, 3, 9, 27) and whether their POLY-part is 1.
    three_part: BTreeMap<u32, usize>,
    poly_part_trivial: usize,
}

fn three_order(f: &blst_fp12) -> u32 {
    // f^(POLY F) is f's 3-part raised to a unit mod 27: same order.
    let mut t = pow_be(&pow_be(f, &F), &POLY.to_be_bytes());
    let mut order = 1;
    while !eq(&t, &one()) {
        t = pow_be(&t, &[3]);
        order *= 3;
        assert!(order <= 27);
    }
    order
}

fn run_case(name: &str, valid: bool, f: &blst_fp12, tally: &Mutex<BTreeMap<String, Tally>>) {
    let by_blst = final_exp_is_one(f);
    let by_pow = eq(&pow_be(f, &H), &one());
    assert_eq!(by_blst, by_pow, "{name}: blst's final exponentiation and f^h disagree");
    assert_eq!(by_blst, valid, "{name}: the product does not have the verdict it was built to have");
    let (c, w) = witness(f);
    let verdict = check_bytes(f, &encode_fp12(&c), &encode_fp12(&w));
    assert_eq!(verdict, check(f, &c, &w));
    assert_eq!(verdict.is_ok(), by_blst, "{name}: the witness check disagrees with the final exponentiation");
    let (order, trivial_poly) =
        if valid { (three_order(f), eq(&pow_be(&pow_be(f, &F), &[27]), &one())) } else { (0, false) };
    let mut all = tally.lock().unwrap();
    let t = all.entry(name.to_string()).or_default();
    t.cases += 1;
    match verdict {
        Ok(()) => {
            t.accepted += 1;
            *t.three_part.entry(order).or_default() += 1;
            t.poly_part_trivial += usize::from(trivial_poly);
        }
        Err(e) => *t.refused.entry(format!("{e:?}")).or_default() += 1,
    }
}

#[test]
fn the_witness_check_agrees_with_the_final_exponentiation() {
    let tally = Mutex::new(BTreeMap::new());
    let jobs: Vec<(usize, u64)> =
        CATEGORIES.iter().enumerate().flat_map(|(c, (_, n, _))| (0..*n).map(move |i| (c, i))).collect();
    let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    thread::scope(|s| {
        for t in 0..threads {
            let (jobs, tally) = (&jobs, &tally);
            s.spawn(move || {
                for (c, i) in jobs.iter().skip(t).step_by(threads) {
                    let (name, _, valid) = CATEGORIES[*c];
                    run_case(name, valid, &build(*c, *i), tally);
                }
            });
        }
    });
    // I5: the edge invalid products. Zero is decoded from 576 zero bytes: in the blst crate,
    // `blst_fp12::default()` is one, not zero.
    let (name, _, _) = CATEGORIES[8];
    let zero = decode_fp12(&[0u8; FP12_BYTES]).unwrap();
    assert!(is_zero(&zero) && !eq(&zero, &one()));
    run_case(name, false, &zero, &tally);
    run_case(name, false, &order_r(&mut Rng::new(8, 0)), &tally);

    let tally = tally.into_inner().unwrap();
    let (mut cases, mut accepted) = (0, 0);
    println!(
        "differential: category | cases | accepted | refused (reason: count) | valid: 3-part order: count; POLY-part 1"
    );
    for (name, t) in &tally {
        cases += t.cases;
        accepted += t.accepted;
        println!(
            "  {name} | {} | {} | {:?} | {:?}; {}",
            t.cases, t.accepted, t.refused, t.three_part, t.poly_part_trivial
        );
    }
    println!("differential: {cases} cases, {accepted} accepted, {} refused; threads {threads}", cases - accepted);
    assert!(cases >= 10_000);
    // Every 3-part order occurs among the valid products, so the scaling factor's every case ran.
    let mut orders = BTreeMap::new();
    for t in tally.values() {
        for (o, n) in &t.three_part {
            *orders.entry(*o).or_insert(0) += n;
        }
    }
    assert_eq!(orders.keys().copied().collect::<Vec<_>>(), vec![1, 3, 9, 27], "3-part orders seen: {orders:?}");
}
