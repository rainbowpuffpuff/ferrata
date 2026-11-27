//! The `bls` target (`fuzz_targets/bls.rs`): its input layout, its edits and its differential
//! check of `ferrata_core::bls`, blst (the library the programs use) against zkcrypto/bls12_381
//! (an independent implementation). Shared with the seed maker (`examples/seeds.rs`) and with the
//! replay of the committed corpus that CI runs (`crates/ferrata-spectests/tests/fuzz_bls_corpus.rs`
//! includes this file).
//!
//! An input is a committee (`n` compressed keys and the aggregate key), one participation bit per
//! member, a signing root and a compressed signature, then up to [`MAX_EDITS`] edits ([`Edit`]).
//! The edits make what byte mutation seldom makes: a key whose secret the harness knows, a copied
//! or negated key, a key, aggregate or signature plus a point of small order or outside the
//! subgroup, the special encodings (the identity and its malformed variants, `x = 0`, `x >= p`, a
//! coordinate plus `p`, missing or extra flags), the aggregate recomputed from the keys, and the
//! root signed with the signers' known secrets. The secrets known are those of the edits and of
//! the consensus-spec test keys (`privkeys = [1, ..., 8192]`, the keys of every committee of the
//! spec's light-client vectors), so a valid signature is reachable for any root, participation and
//! committee the fuzzer builds, and a library that computed a wrong hash-to-curve, sum or pairing
//! would refuse it where the other accepts.
//!
//! [`check`] runs, with both libraries, every path: `fast_aggregate_verify`, `check_committee`,
//! `check_committee_onchain` and `verify_checked` under each [`Aggregation`], and blst's split
//! (`miller_half` then `final_half`); each key an edit wrote alone (`KeyValidate`, and the
//! on-curve decoding); and the signature alone when the keys stopped the spec path before it.
//! It panics, naming the path and both verdicts, where they differ, with one documented
//! exception: blst's decoder refuses the two points with `x = 0` (order 3, outside G1) even where
//! only an on-curve check is asked, and zkcrypto's on-curve decoding takes them; on the paths
//! that decode keys that way (`check_committee_onchain`, `verify_checked`) a committee holding
//! one gets blst's refusal, which the check then requires exactly. Within each library it also
//! requires what `bls/mod.rs` states: on a committee that passed `check_committee`, every
//! aggregation returns what `fast_aggregate_verify` returns; `Auto` returns what the composition
//! it picks returns; the split returns what `verify_checked` returns.
#![deny(unsafe_code)]

use std::{collections::HashMap, sync::OnceLock};

use bls12_381::{G1Affine, G1Projective, G2Affine, G2Projective, Scalar};
use ferrata_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto, DST},
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};

/// Largest committee an input holds (the mainnet sync committee).
pub const MAX_MEMBERS: usize = 512;
/// Edits read from one input; bytes after them are ignored.
pub const MAX_EDITS: usize = 32;
/// The consensus-spec test keys whose secrets the harness knows: `privkeys = [i + 1 for i in
/// range(32 * 256)]` (`eth2spec/test/helpers/keys.py`).
pub const PYSPEC_KEYS: u64 = 32 * 256;

/// The encoding of the identity of G1.
pub const G1_IDENTITY: BlsPubkey = {
    let mut k = [0u8; 48];
    k[0] = 0xc0;
    k
};
/// The encoding of the identity of G2.
pub const G2_IDENTITY: BlsSignature = {
    let mut s = [0u8; 96];
    s[0] = 0xc0;
    s
};

/// The base field modulus, big-endian.
const P: [u8; 48] = [
    0x1a, 0x01, 0x11, 0xea, 0x39, 0x7f, 0xe6, 0x9a, 0x4b, 0x1b, 0xa7, 0xb6, 0x43, 0x4b, 0xac, 0xd7, 0x64, 0x77, 0x4b,
    0x84, 0xf3, 0x85, 0x12, 0xbf, 0x67, 0x30, 0xd2, 0xa0, 0xf6, 0xb0, 0xf6, 0x24, 0x1e, 0xab, 0xff, 0xfe, 0xb1, 0x53,
    0xff, 0xff, 0xb9, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xaa, 0xab,
];

/// The group order r, hex.
const R: &str = "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";

/// For each prime l of G1's cofactor (3, 11, 10177, 859267, 52437899): #E(Fp) / l^e as hex
/// (l^e the largest power of l dividing #E(Fp) = h1 * r), and l^e.
const G1_TORSION: [(&str, u64); 5] = [
    ("8ab05f8bdd54cde190937e76bc3e447cc27c3d6fbd7063fcd104635a790520c0a395554e5c6aaaad955555555558e39", 3),
    ("3704612471307385e8f4b11c0f6f71e98ebcebf11641bde11e05f8de12635b461258dc05b269c8ff0a941963702343", 11 * 11),
    ("4365bf803428bdeec60fd2e926108ec4349c3ed6101355fc141ebb62934a3754bcaa7f4893a24a99528dca02b", 10_177 * 10_177),
    ("26b97e14f9a9bfc6a1d68e51940e20533818e49972d05a4b963f8399f9c8df3382e9555b8e0c186760b113", 859_267 * 859_267),
    ("2a97289d2f199a7fe442ea79eb7aa6cc575f3bb074193d880722109579598622c3f85f4e51b4a558443", 52_437_899 * 52_437_899),
];

/// The same for G2's cofactor h2 (13, 23, 2713, 11953, 262069 and a 403-bit prime, for which the
/// order is not checked: 0).
const G2_TORSION: [(&str, u64); 6] = [
    (
        "4005449cda731a7136c440a0c65b728ba1c1fa6b6708356f3b9bdc84396cab33907d71557a7d33677f5d45f7cedb8cfdac10ff1fc5b48d\
         6461e907737d78e96568f2d18c750b4b3ca5c33c3fd8ff8a70629888281914529f4e3380941cfdd",
        13 * 13,
    ),
    (
        "1473e413236dd889cb5381880d0a576821c578a6e8c7952f26668aaed6d2623f6a2afa96b5e43f5d126da598b99dbb518655c33a8b236b\
         990d5fbd2d9b42b21fe9e0346d3c23104b06fb96e09de2555c861790a3cc012b5540aa5804bb595",
        23 * 23,
    ),
    (
        "3fcee61e74c25c987842842a1a26de4d74f74f14543213da8777344b8bfcf67fd34a2a90a45b3170df91252e784da8bcaa5cfea7bc15ce\
         204c72df226dc50975962d2d53a875b1ef426ad51a2cbb2185cf414efe8a6ae0d520ee8ae8bc2d",
        2_713,
    ),
    (
        "e7b91d864fd7d4c0c79957105c92b89ec85bf8e8a7d594110346483f17da69c7cf3a9d7278d9937c64966acc866446ec1db5dcd51748de\
         61764329a3e70094768767f1e6cc339643949c93915d33844c5ab2047701878fab9632ed78275",
        11_953,
    ),
    (
        "a91a4bdb3f26d1ead4914c4ce26b0fd01fa28a6b3f6c924d7093911c08709252e11d3c29df83baa337c6a28ec630c4603c694e666410999\
         452285c21b6cb6bb5ecb4478b228299bce84d8ec6ce12616966d30f7f44cbf41ccb9be93271",
        262_069,
    ),
    ("4c658e1d2a19cb91a3a9afea1c3245a3111bcf284ae2370fdb442fbe4bc7f9dc256f97848344975", 0),
];

/// One change to the decoded input, applied in order. Member indices are taken modulo the
/// committee size, table indices modulo the table's length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Toggle a member's participation bit.
    Bit(u16),
    /// Every bit from a pattern: member `i` signs if bit `i % 8` of the pattern is set.
    Fill(u8),
    /// A member's key becomes the key of a secret (0: the identity), which the harness knows.
    Secret(u16, u64),
    /// Member `.0` gets member `.1`'s key (and secret).
    Copy(u16, u16),
    /// Toggle the sign flag of a member's key: the negated point; the secret is negated.
    Negate(u16),
    /// A member's key plus a point outside G1 ([`torsion_g1`]); kept if the key does not decode.
    KeyTorsion(u16, u8),
    /// A member's key becomes a special encoding ([`special_g1`]).
    KeySpecial(u16, u8),
    /// The aggregate becomes the sum of the keys (on-curve decoding); kept if one does not decode.
    SumAggregate,
    /// The aggregate plus a point outside G1.
    AggregateTorsion(u8),
    /// The aggregate becomes a special encoding.
    AggregateSpecial(u8),
    /// The signature becomes the signature of the root by the signers' known secrets (the
    /// identity if they sum to zero, or none is known).
    Sign,
    /// The signature plus a point outside G2 ([`torsion_g2`]); kept if it does not decode.
    SignatureTorsion(u8),
    /// The signature becomes a special encoding ([`special_g2`]).
    SignatureSpecial(u8),
    /// Keep the first `1 + k % n` members.
    Truncate(u16),
}

impl Edit {
    fn read(i: &mut Cursor<'_>) -> Edit {
        match i.u8() % 14 {
            0 => Edit::Bit(i.u16()),
            1 => Edit::Fill(i.u8()),
            2 => Edit::Secret(i.u16(), i.u64()),
            3 => Edit::Copy(i.u16(), i.u16()),
            4 => Edit::Negate(i.u16()),
            5 => Edit::KeyTorsion(i.u16(), i.u8()),
            6 => Edit::KeySpecial(i.u16(), i.u8()),
            7 => Edit::SumAggregate,
            8 => Edit::AggregateTorsion(i.u8()),
            9 => Edit::AggregateSpecial(i.u8()),
            10 => Edit::Sign,
            11 => Edit::SignatureTorsion(i.u8()),
            12 => Edit::SignatureSpecial(i.u8()),
            _ => Edit::Truncate(i.u16()),
        }
    }

    /// The bytes [`parse`] reads back as this edit.
    pub fn encode(&self, out: &mut Vec<u8>) {
        match *self {
            Edit::Bit(m) => push(out, 0, &m.to_le_bytes()),
            Edit::Fill(p) => push(out, 1, &[p]),
            Edit::Secret(m, s) => push(out, 2, &[&m.to_le_bytes()[..], &s.to_le_bytes()].concat()),
            Edit::Copy(m, j) => push(out, 3, &[m.to_le_bytes(), j.to_le_bytes()].concat()),
            Edit::Negate(m) => push(out, 4, &m.to_le_bytes()),
            Edit::KeyTorsion(m, t) => push(out, 5, &[&m.to_le_bytes()[..], &[t]].concat()),
            Edit::KeySpecial(m, w) => push(out, 6, &[&m.to_le_bytes()[..], &[w]].concat()),
            Edit::SumAggregate => push(out, 7, &[]),
            Edit::AggregateTorsion(t) => push(out, 8, &[t]),
            Edit::AggregateSpecial(w) => push(out, 9, &[w]),
            Edit::Sign => push(out, 10, &[]),
            Edit::SignatureTorsion(t) => push(out, 11, &[t]),
            Edit::SignatureSpecial(w) => push(out, 12, &[w]),
            Edit::Truncate(k) => push(out, 13, &k.to_le_bytes()),
        }
    }
}

fn push(out: &mut Vec<u8>, op: u8, operands: &[u8]) {
    out.push(op);
    out.extend_from_slice(operands);
}

/// A cursor over the input; reads past the end yield zeros.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        let (a, b) = self.0.split_at(n.min(self.0.len()));
        self.0 = b;
        a
    }
    fn array<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        let t = self.take(N);
        out[..t.len()].copy_from_slice(t);
        out
    }
    fn u8(&mut self) -> u8 {
        self.array::<1>()[0]
    }
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.array())
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.array())
    }
}

/// A decoded input after its edits.
#[derive(Clone, Debug)]
pub struct Case {
    pub committee: SyncCommittee,
    pub bits: Vec<bool>,
    pub root: Root,
    pub signature: BlsSignature,
    /// Each member's secret, where the harness knows it.
    pub secrets: Vec<Option<Scalar>>,
    /// Every key and aggregate an edit wrote, checked alone as well.
    pub written: Vec<BlsPubkey>,
    pub edits: Vec<Edit>,
}

/// Layout: committee size `1 + (u16 LE) % 512`; the keys (48 bytes each); the aggregate (48); the
/// participation bits (one byte per eight members, member `i` at bit `i % 8` of byte `i / 8`); the
/// signing root (32); the signature (96); then edits, each an opcode byte and its operands.
pub fn parse(data: &[u8]) -> Case {
    let mut i = Cursor(data);
    let n = 1 + usize::from(i.u16()) % MAX_MEMBERS;
    let pubkeys: Vec<BlsPubkey> = (0..n).map(|_| i.array()).collect();
    let aggregate_pubkey = i.array();
    let packed = i.take(n.div_ceil(8));
    let bits = (0..n).map(|k| packed.get(k / 8).is_some_and(|b| (b >> (k % 8)) & 1 == 1)).collect();
    let root = i.array();
    let signature = i.array();
    let secrets = pubkeys.iter().map(|k| tables().pyspec.get(k).map(|s| Scalar::from(*s))).collect();
    let mut case = Case {
        committee: SyncCommittee { pubkeys, aggregate_pubkey },
        bits,
        root,
        signature,
        secrets,
        written: Vec::new(),
        edits: Vec::new(),
    };
    while !i.0.is_empty() && case.edits.len() < MAX_EDITS {
        let edit = Edit::read(&mut i);
        case.apply(edit);
        case.edits.push(edit);
    }
    case
}

/// The bytes [`parse`] reads as this committee, participation, root and signature followed by
/// `edits`.
pub fn encode(
    committee: &SyncCommittee,
    bits: &[bool],
    root: &Root,
    signature: &BlsSignature,
    edits: &[Edit],
) -> Vec<u8> {
    let n = committee.pubkeys.len();
    assert!((1..=MAX_MEMBERS).contains(&n) && bits.len() == n, "a committee of 1 to 512 members, one bit each");
    let mut out = ((n - 1) as u16).to_le_bytes().to_vec();
    for k in &committee.pubkeys {
        out.extend_from_slice(k);
    }
    out.extend_from_slice(&committee.aggregate_pubkey);
    let mut packed = vec![0u8; n.div_ceil(8)];
    for (k, _) in bits.iter().enumerate().filter(|(_, b)| **b) {
        packed[k / 8] |= 1 << (k % 8);
    }
    out.extend_from_slice(&packed);
    out.extend_from_slice(root);
    out.extend_from_slice(signature);
    for e in edits {
        e.encode(&mut out);
    }
    out
}

impl Case {
    fn member(&self, m: u16) -> usize {
        usize::from(m) % self.committee.pubkeys.len()
    }

    fn set_key(&mut self, m: usize, key: BlsPubkey, secret: Option<Scalar>) {
        self.committee.pubkeys[m] = key;
        self.secrets[m] = secret;
        self.written.push(key);
    }

    fn set_aggregate(&mut self, key: BlsPubkey) {
        self.committee.aggregate_pubkey = key;
        self.written.push(key);
    }

    /// Apply one edit.
    pub fn apply(&mut self, edit: Edit) {
        match edit {
            Edit::Bit(m) => {
                let m = self.member(m);
                self.bits[m] = !self.bits[m];
            }
            Edit::Fill(p) => {
                for (k, b) in self.bits.iter_mut().enumerate() {
                    *b = (p >> (k % 8)) & 1 == 1;
                }
            }
            Edit::Secret(m, s) => {
                let m = self.member(m);
                self.set_key(m, public_key(s), Some(Scalar::from(s)));
            }
            Edit::Copy(m, j) => {
                let (m, j) = (self.member(m), self.member(j));
                self.set_key(m, self.committee.pubkeys[j], self.secrets[j]);
            }
            Edit::Negate(m) => {
                let m = self.member(m);
                let mut key = self.committee.pubkeys[m];
                key[0] ^= 0x20;
                self.set_key(m, key, self.secrets[m].map(|s| -s));
            }
            Edit::KeyTorsion(m, t) => {
                let m = self.member(m);
                if let Some(key) = add_g1(&self.committee.pubkeys[m], t) {
                    self.set_key(m, key, self.secrets[m]);
                }
            }
            Edit::KeySpecial(m, w) => {
                let m = self.member(m);
                let (key, same_point) = special_g1(&self.committee.pubkeys[m], w);
                self.set_key(m, key, if same_point { self.secrets[m] } else { None });
            }
            Edit::SumAggregate => {
                let sum = self.committee.pubkeys.iter().try_fold(G1Projective::identity(), |acc, k| Some(acc + g1(k)?));
                if let Some(sum) = sum {
                    self.set_aggregate(G1Affine::from(sum).to_compressed());
                }
            }
            Edit::AggregateTorsion(t) => {
                if let Some(key) = add_g1(&self.committee.aggregate_pubkey, t) {
                    self.set_aggregate(key);
                }
            }
            Edit::AggregateSpecial(w) => {
                let (key, _) = special_g1(&self.committee.aggregate_pubkey, w);
                self.set_aggregate(key);
            }
            Edit::Sign => {
                let secret = self
                    .bits
                    .iter()
                    .zip(&self.secrets)
                    .filter(|(b, _)| **b)
                    .filter_map(|(_, s)| *s)
                    .fold(Scalar::zero(), |acc, s| acc + s);
                self.signature = sign(&secret, &self.root);
            }
            Edit::SignatureTorsion(t) => {
                if let Some(sig) = Option::<G2Affine>::from(G2Affine::from_compressed_unchecked(&self.signature)) {
                    let table = &tables().g2;
                    let t = &table[usize::from(t) % table.len()];
                    self.signature = G2Affine::from(G2Projective::from(sig) + t).to_compressed();
                }
            }
            Edit::SignatureSpecial(w) => self.signature = special_g2(&self.signature, w),
            Edit::Truncate(k) => {
                let n = 1 + usize::from(k) % self.committee.pubkeys.len();
                self.committee.pubkeys.truncate(n);
                self.bits.truncate(n);
                self.secrets.truncate(n);
            }
        }
    }
}

/// A key decoded with the on-curve check only (zkcrypto).
fn g1(key: &BlsPubkey) -> Option<G1Projective> {
    Option::<G1Affine>::from(G1Affine::from_compressed_unchecked(key)).map(G1Projective::from)
}

fn add_g1(key: &BlsPubkey, t: u8) -> Option<BlsPubkey> {
    let table = &tables().g1;
    Some(G1Affine::from(g1(key)? + table[usize::from(t) % table.len()]).to_compressed())
}

/// The key of a secret: `s * g1` (the identity for 0).
pub fn public_key(s: u64) -> BlsPubkey {
    G1Affine::from(mul_g1(&G1Projective::generator(), &s.to_be_bytes())).to_compressed()
}

/// `secret * H(root)` (blst's signing), or the identity for the zero secret.
pub fn sign(secret: &Scalar, root: &Root) -> BlsSignature {
    let mut be = secret.to_bytes();
    be.reverse();
    blst::min_pk::SecretKey::from_bytes(&be).map(|sk| sk.sign(root, DST, &[]).compress()).unwrap_or(G2_IDENTITY)
}

/// True for an encoding of `(0, ±2)`: compressed, not the identity, `x = 0`.
pub fn is_x0(key: &BlsPubkey) -> bool {
    key[0] & 0xc0 == 0x80 && key[0] & 0x1f == 0 && key[1..].iter().all(|b| *b == 0)
}

/// `x + p` over 384 bits, or `None` if it overflows.
fn plus_p(x: &[u8]) -> Option<[u8; 48]> {
    let mut out = [0u8; 48];
    let mut carry = 0u16;
    for k in (0..48).rev() {
        let s = u16::from(x[k]) + u16::from(P[k]) + carry;
        out[k] = s as u8;
        carry = s >> 8;
    }
    (carry == 0).then_some(out)
}

/// A 48-byte field holding a coordinate under the three flag bits, replaced by the coordinate
/// plus p (the same value, not reduced) where it still fits under the flags.
fn coordinate_plus_p(field: &mut [u8]) {
    let flags = field[0] & 0xe0;
    let mut x = [0u8; 48];
    x.copy_from_slice(field);
    x[0] &= 0x1f;
    if let Some(mut y) = plus_p(&x).filter(|y| y[0] & 0xe0 == 0) {
        y[0] |= flags;
        field.copy_from_slice(&y);
    }
}

/// Special encodings of G1, and whether the result still names the key's point (so its secret
/// is kept): the identity (0), with the sign flag (1), with `x != 0` (2), without the compression
/// flag (3); `x = 0`, the points (0, 2) and (0, -2) of order 3 (4, 5); the key without the
/// compression flag (6), with the identity flag (7), with `x + p` for `x` (8); `x = p` (9),
/// `x = p - 1` (10); every bit set (11).
pub fn special_g1(key: &BlsPubkey, w: u8) -> (BlsPubkey, bool) {
    let mut k = [0u8; 48];
    let mut same = false;
    match w % 12 {
        0 => k[0] = 0xc0,
        1 => k[0] = 0xe0,
        2 => (k[0], k[47]) = (0xc0, 1),
        3 => k[0] = 0x40,
        4 => k[0] = 0x80,
        5 => k[0] = 0xa0,
        6 => (k, same) = (*key, true),
        7 => (k, same) = (*key, true),
        8 => (k, same) = (*key, true),
        9 => k = P,
        10 => {
            k = P;
            k[47] -= 1;
        }
        _ => k = [0xff; 48],
    }
    match w % 12 {
        6 => k[0] &= 0x7f,
        7 => k[0] |= 0x40,
        8 => coordinate_plus_p(&mut k),
        9 | 10 => k[0] |= 0x80,
        _ => {}
    }
    (k, same)
}

/// Special encodings of G2: the identity (0), with the sign flag (1), with `x != 0` (2), without
/// the compression flag (3); the signature without the compression flag (4), with the identity
/// flag (5), with `x1 + p` (6), with `x0 + p` (7); `x1 = p` (8), `x0 = p` (9) over the signature;
/// the two halves of `x` swapped, flags kept (10); the sign flag toggled, the negated point (11);
/// `x = 1` (12); every bit set (13).
pub fn special_g2(sig: &BlsSignature, w: u8) -> BlsSignature {
    let mut s = [0u8; 96];
    match w % 14 {
        0 => s[0] = 0xc0,
        1 => s[0] = 0xe0,
        2 => (s[0], s[95]) = (0xc0, 1),
        3 => s[0] = 0x40,
        4 => {
            s = *sig;
            s[0] &= 0x7f;
        }
        5 => {
            s = *sig;
            s[0] |= 0x40;
        }
        6 => {
            s = *sig;
            coordinate_plus_p(&mut s[..48]);
        }
        7 => {
            s = *sig;
            if let Some(y) = plus_p(&s[48..]) {
                s[48..].copy_from_slice(&y);
            }
        }
        8 => {
            s = *sig;
            s[..48].copy_from_slice(&P);
            s[0] |= sig[0] & 0xe0;
        }
        9 => {
            s = *sig;
            s[48..].copy_from_slice(&P);
        }
        10 => {
            s[..48].copy_from_slice(&sig[48..]);
            s[48..].copy_from_slice(&sig[..48]);
            s[48] &= 0x1f;
            s[0] = (s[0] & 0x1f) | (sig[0] & 0xe0);
        }
        11 => {
            s = *sig;
            s[0] ^= 0x20;
        }
        12 => (s[0], s[95]) = (0x80, 1),
        _ => s = [0xff; 96],
    }
    s
}

/// Points the harness adds, and the secrets it knows.
struct Tables {
    /// The consensus-spec test keys and their secrets.
    pyspec: HashMap<BlsPubkey, u64>,
    /// Points of E(Fp) outside G1: of order 3 (`x = 0`), 11^k, 10177^k, 859267^k, 52437899^k; one
    /// of the whole cofactor's order (`r * Q`); a point with a component in G1 and in every one.
    g1: Vec<G1Projective>,
    /// Points of E'(Fp2) outside G2: of order 13^k, 23^k, 2713, 11953, 262069, the 403-bit prime;
    /// `r * Q`; `Q`.
    g2: Vec<G2Projective>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut points = Vec::with_capacity(PYSPEC_KEYS as usize);
        let mut p = G1Projective::identity();
        for _ in 0..PYSPEC_KEYS {
            p += G1Projective::generator();
            points.push(p);
        }
        let mut affine = vec![G1Affine::identity(); points.len()];
        G1Projective::batch_normalize(&points, &mut affine);
        let pyspec = affine.iter().zip(1..).map(|(a, s)| (a.to_compressed(), s)).collect();
        Tables { pyspec, g1: torsion_g1(), g2: torsion_g2() }
    })
}

fn hex(s: &str) -> Vec<u8> {
    let digit = |c: u8| (c as char).to_digit(16).expect("hex digit") as u8;
    let s = s.as_bytes();
    let pad = s.len() % 2;
    (0..s.len().div_ceil(2))
        .map(|k| {
            let hi = if k == 0 && pad == 1 { 0 } else { digit(s[2 * k - pad]) };
            (hi << 4) | digit(s[2 * k + 1 - pad])
        })
        .collect()
}

fn mul_g1(p: &G1Projective, k: &[u8]) -> G1Projective {
    let mut acc = G1Projective::identity();
    for byte in k {
        for bit in (0..8).rev() {
            acc = acc.double();
            if (byte >> bit) & 1 == 1 {
                acc += p;
            }
        }
    }
    acc
}

fn mul_g2(p: &G2Projective, k: &[u8]) -> G2Projective {
    let mut acc = G2Projective::identity();
    for byte in k {
        for bit in (0..8).rev() {
            acc = acc.double();
            if (byte >> bit) & 1 == 1 {
                acc += p;
            }
        }
    }
    acc
}

/// Curve points outside the subgroup: the compressed x = 1, 2, ... that decode.
fn outside_g1() -> impl Iterator<Item = G1Projective> {
    (1u8..=255).filter_map(|x| {
        let mut k = [0u8; 48];
        (k[0], k[47]) = (0x80, x);
        Option::<G1Affine>::from(G1Affine::from_compressed_unchecked(&k))
            .filter(|a| !bool::from(a.is_torsion_free()))
            .map(G1Projective::from)
    })
}

fn outside_g2() -> impl Iterator<Item = G2Projective> {
    (1u8..=255).filter_map(|x| {
        let mut s = [0u8; 96];
        (s[0], s[95]) = (0x80, x);
        Option::<G2Affine>::from(G2Affine::from_compressed_unchecked(&s))
            .filter(|a| !bool::from(a.is_torsion_free()))
            .map(G2Projective::from)
    })
}

/// See [`Tables::g1`]. Each point is checked: not the identity, outside G1, and of the order it
/// should have.
pub fn torsion_g1() -> Vec<G1Projective> {
    let mut out: Vec<G1Projective> = G1_TORSION
        .iter()
        .map(|(cofactor, order)| {
            let t = outside_g1()
                .map(|q| mul_g1(&q, &hex(cofactor)))
                .find(|t| !bool::from(t.is_identity()))
                .expect("a torsion point");
            assert!(bool::from(mul_g1(&t, &order.to_be_bytes()).is_identity()), "order of a G1 torsion point");
            t
        })
        .collect();
    let q = outside_g1().next().expect("a point outside G1");
    out.push(mul_g1(&q, &hex(R)));
    out.push(q);
    for t in &out {
        assert!(!bool::from(t.is_identity()) && !bool::from(G1Affine::from(t).is_torsion_free()));
    }
    out
}

/// See [`Tables::g2`].
pub fn torsion_g2() -> Vec<G2Projective> {
    let mut out: Vec<G2Projective> = G2_TORSION
        .iter()
        .map(|(cofactor, order)| {
            let t = outside_g2()
                .map(|q| mul_g2(&q, &hex(cofactor)))
                .find(|t| !bool::from(t.is_identity()))
                .expect("a torsion point");
            if *order != 0 {
                assert!(bool::from(mul_g2(&t, &order.to_be_bytes()).is_identity()), "order of a G2 torsion point");
            }
            t
        })
        .collect();
    let q = outside_g2().next().expect("a point outside G2");
    out.push(mul_g2(&q, &hex(R)));
    out.push(q);
    for t in &out {
        assert!(!bool::from(t.is_identity()) && !bool::from(G2Affine::from(t).is_torsion_free()));
    }
    out
}

/// What the paths returned (blst's verdicts, equal to zkcrypto's wherever compared).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdicts {
    pub fast_aggregate_verify: Result<(), Error>,
    pub check_committee: Result<(), Error>,
    pub check_committee_onchain: Result<(), Error>,
    /// `verify_checked` (Additive, Subtractive, Auto), run when either library's
    /// `check_committee_onchain` accepted the committee (the program calls it on no other).
    pub checked: Option<[Result<(), Error>; 3]>,
}

fn agree(what: &str, blst: Result<(), Error>, zkcrypto: Result<(), Error>) -> Result<(), Error> {
    if blst != zkcrypto {
        panic!("blst and zkcrypto disagree on {what}: blst {blst:?}, zkcrypto {zkcrypto:?}");
    }
    blst
}

/// [`agree`], except where the path decodes a key with `x = 0` with the on-curve check only:
/// blst's decoder refuses it (`BLST_POINT_NOT_IN_GROUP`) and zkcrypto's takes it, so blst's
/// verdict must be `refusal` and zkcrypto's is not compared.
fn agree_unless_x0(
    what: &str,
    x0: bool,
    refusal: Error,
    blst: Result<(), Error>,
    zkcrypto: Result<(), Error>,
) -> Result<(), Error> {
    if !x0 {
        return agree(what, blst, zkcrypto);
    }
    if blst != Err(refusal) {
        panic!("blst does not refuse a key with x = 0 on {what}: blst {blst:?} (expected {refusal:?}), zkcrypto {zkcrypto:?}");
    }
    blst
}

const AGGREGATIONS: [Aggregation; 3] = [Aggregation::Additive, Aggregation::Subtractive, Aggregation::Auto];

/// Run every path with both libraries; panic where they disagree (see the module documentation).
pub fn check(case: &Case) -> Verdicts {
    let (c, bits, root, sig) = (&case.committee, &case.bits[..], &case.root, &case.signature);
    let (b, z) = (Blst::default(), Zkcrypto::default());

    let fav = agree(
        "fast_aggregate_verify",
        b.fast_aggregate_verify(c, bits, root, sig),
        z.fast_aggregate_verify(c, bits, root, sig),
    );
    let committee = agree("check_committee", b.check_committee(c), z.check_committee(c));
    let (onchain_b, onchain_z) = (b.check_committee_onchain(c), z.check_committee_onchain(c));
    let any_x0 = c.pubkeys.iter().chain([&c.aggregate_pubkey]).any(is_x0);
    let onchain = agree_unless_x0("check_committee_onchain", any_x0, Error::CommitteeInvalid, onchain_b, onchain_z);

    let mut checked = None;
    if onchain_b.is_ok() || onchain_z.is_ok() {
        let n = c.pubkeys.len();
        let signers = bits.iter().filter(|b| **b).count();
        let mut out = [Ok(()); 3];
        let mut out_z = [Ok(()); 3];
        for (k, a) in AGGREGATIONS.into_iter().enumerate() {
            // The keys this composition decodes, on-curve only.
            let subtract = signers > 0 && a.subtract(signers, n);
            let x0 = signers > 0
                && if subtract {
                    is_x0(&c.aggregate_pubkey) || c.pubkeys.iter().zip(bits).any(|(k, b)| !*b && is_x0(k))
                } else {
                    c.pubkeys.iter().zip(bits).any(|(k, b)| *b && is_x0(k))
                };
            let what = format!("verify_checked ({a:?})");
            let vb = Blst::new(a).verify_checked(c, bits, root, sig);
            let vz = Zkcrypto::new(a).verify_checked(c, bits, root, sig);
            out[k] = agree_unless_x0(&what, x0, Error::PubkeyInvalid, vb, vz);
            out_z[k] = vz;
            // blst's split, as the program runs it across transactions.
            let split = Blst::new(a).miller_half(c, bits, root, sig).and_then(|p| Blst::final_half(&p));
            if split != vb {
                panic!("blst's miller_half/final_half ({a:?}) returned {split:?}, its verify_checked {vb:?}");
            }
            if committee.is_ok() && (vb != fav || vz != fav) {
                panic!(
                    "on a committee that passed check_committee, verify_checked ({a:?}) returned blst {vb:?}, \
                     zkcrypto {vz:?}, fast_aggregate_verify {fav:?}"
                );
            }
        }
        // Auto is the composition it picks.
        let picked = if signers > 0 && Aggregation::Auto.subtract(signers, n) { 1 } else { 0 };
        for (lib, v) in [("blst", out), ("zkcrypto", out_z)] {
            if v[2] != v[picked] {
                panic!("{lib}: verify_checked (Auto) returned {:?}, the composition it picks {:?}", v[2], v[picked]);
            }
        }
        checked = Some(out);
    }

    // Each key an edit wrote, alone: KeyValidate, then the on-curve decoding.
    let mut seen: Vec<&BlsPubkey> = Vec::new();
    for key in &case.written {
        if seen.contains(&key) || seen.len() == 8 {
            continue;
        }
        seen.push(key);
        let alone = SyncCommittee { pubkeys: vec![*key], aggregate_pubkey: *key };
        let _ = agree("KeyValidate of one key (check_committee)", b.check_committee(&alone), z.check_committee(&alone));
        let _ = agree_unless_x0(
            "the on-curve decoding of one key (check_committee_onchain)",
            is_x0(key),
            Error::CommitteeInvalid,
            b.check_committee_onchain(&alone),
            z.check_committee_onchain(&alone),
        );
    }

    // The signature alone, when the keys stopped the spec path before it: under the key of secret 1.
    if matches!(fav, Err(Error::PubkeyInvalid | Error::SigningKeyIdentity | Error::ParticipationLow | Error::Malformed))
    {
        let one = public_key(1);
        let alone = SyncCommittee { pubkeys: vec![one], aggregate_pubkey: one };
        let _ = agree(
            "the signature under one valid key (fast_aggregate_verify)",
            b.fast_aggregate_verify(&alone, &[true], root, sig),
            z.fast_aggregate_verify(&alone, &[true], root, sig),
        );
    }

    Verdicts { fast_aggregate_verify: fav, check_committee: committee, check_committee_onchain: onchain, checked }
}

/// [`parse`] then [`check`].
pub fn run(data: &[u8]) -> Verdicts {
    check(&parse(data))
}
