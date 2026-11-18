//! Tests of `residue_agg` (experimental, unaudited): the signing key composed with one multiproof
//! for all non-signers against the core's composition (a Merkle branch per non-signer).
//!
//! * The constants and the building blocks: `(p − 1)/2`; [`hash64`] against SHA-256; the
//!   compressed key read off its bytes against blst; the key decoding against the core's.
//! * The multiproof: round trips on random leaf sets of every size, the number of hashes, every
//!   single tampering refused, and three forgeries that show why each of its rules is needed.
//! * The composition against the core's `steps::step1` (the real function, unchanged), result for
//!   result: every update of the official `sync` vectors, the mainnet replay and the Sepolia chain
//!   through Gloas; and a differential of at least 2,000 (committee, bitfield) pairs at 342, 343,
//!   400, 510, 511, 512 signers and at random, over the bench's committees and every real committee
//!   of the mainnet and Sepolia data, plus a committee with a duplicated and a negated key.
//! * Refusals: a wrong key, a key outside the committee, a duplicated non-signer, an altered branch
//!   or multiproof node, a bitfield that disagrees with the witness, 341 signers, and malformed keys.

use std::{collections::BTreeSet, fs, path::PathBuf};

use blst::{
    blst_p1, blst_p1_add_or_double_affine, blst_p1_affine, blst_p1_affine_compress, blst_p1_affine_is_inf,
    blst_p1_cneg, blst_p1_compress, blst_p1_deserialize, blst_p1_from_affine, blst_p1_to_affine, blst_p1_uncompress,
    min_pk::{PublicKey, SecretKey},
    BLST_ERROR,
};
use ferrata_core::{
    bls::{Aggregation, Blst, DST},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    hash::hash_pair,
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, Job, StepError},
    store::StoreCore,
    types::{BlsPubkey, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{sync_committee_signing_root, Context},
    witness::{
        pubkey_leaf, uncompress_key, CommitteeSummary, CompactStore, CompactUpdate, SignerWitness, UncompressedPubkey,
    },
    Error, Root,
};
use ferrata_spectests::{
    cases, parse_config, read_ssz_snappy,
    testkit::{fake_committee, sign},
};
use num_bigint::BigUint;
use residue_agg::*;
use serde_yaml::Value;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// `n` members, exactly `absent` absent, by a seeded shuffle.
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

const MAINNET_GVR: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const MAINNET_TRUSTED: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

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

/// The committee that signs `update` in the full store, if known.
fn signing_committee<'a>(
    full: &'a StoreCore,
    update: &LightClientUpdate,
    spec: &ChainSpec,
) -> Option<&'a SyncCommittee> {
    let c = if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    };
    (!c.is_empty()).then_some(c)
}

fn as_finality(u: &LightClientUpdate) -> LightClientUpdate {
    let mut f = u.clone();
    f.next_sync_committee = SyncCommittee::empty(u.sync_aggregate.bits.len());
    f.next_sync_committee_branch = vec![ZERO; u.next_sync_committee_branch.len()];
    f
}

/// The core's acceptance of a witness key (`witness.rs`: `deserialize_g1`, then `witness_key`'s
/// identity check), copied.
fn core_accepts(key: &UncompressedPubkey) -> bool {
    if key[0] & 0x80 != 0 {
        return false;
    }
    let mut p = blst_p1_affine::default();
    // SAFETY: 96 bytes read.
    unsafe { blst_p1_deserialize(&mut p, key.as_ptr()) == BLST_ERROR::BLST_SUCCESS && !blst_p1_affine_is_inf(&p) }
}

/// The core's `step1` and `residue_agg::step1` (batch and chain sums) on the same input, with
/// honest witnesses for `committee` (or empty ones if it is unknown): the same result, job for
/// job or code for code. Returns the core's result.
fn compare(
    label: &str,
    store: &CompactStore,
    update: &CompactUpdate,
    committee: Option<&SyncCommittee>,
    current_slot: u64,
    spec: &ChainSpec,
    gvr: &Root,
) -> Result<Job, StepError> {
    let bits = &update.sync_aggregate.bits;
    let old = committee.and_then(|c| SignerWitness::build(c, bits).ok()).unwrap_or_default();
    let new = committee.and_then(|c| NonSigners::build(c, bits).ok()).unwrap_or_default();
    assert_eq!(NonSigners::decode(&new.encode(), bits.len()).as_ref(), Ok(&new), "{label}: encoding round trip");
    let core = steps::step1(store, update, &old, current_slot, spec, gvr, 1);
    for sum in [Sum::Batch, Sum::Chain] {
        let mine = residue_agg::step1(store, update, &new, current_slot, spec, gvr, 1, sum);
        match (&core, &mine) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{label} ({sum:?}): the jobs differ"),
            (Err(a), Err(b)) => assert_eq!(a.code(), b.code(), "{label} ({sum:?}): refused with {a:?} and {b:?}"),
            _ => panic!("{label} ({sum:?}): core {:?}, residue_agg {:?}", core.as_ref().map(|_| ()), mine.map(|_| ())),
        }
    }
    core
}

fn neg(pk: &BlsPubkey) -> BlsPubkey {
    let mut a = blst_p1_affine::default();
    let mut p = blst_p1::default();
    let mut out = [0u8; 48];
    // SAFETY: valid inputs and outputs.
    unsafe {
        assert_eq!(blst_p1_uncompress(&mut a, pk.as_ptr()), BLST_ERROR::BLST_SUCCESS);
        blst_p1_from_affine(&mut p, &a);
        blst_p1_cneg(&mut p, true);
        blst_p1_compress(out.as_mut_ptr(), &p);
    }
    out
}

fn sum_keys(keys: &[BlsPubkey]) -> BlsPubkey {
    let mut acc = blst_p1::default();
    for k in keys {
        let mut a = blst_p1_affine::default();
        // SAFETY: valid inputs and outputs; aliasing allowed.
        unsafe {
            assert_eq!(blst_p1_uncompress(&mut a, k.as_ptr()), BLST_ERROR::BLST_SUCCESS);
            let p: *mut blst_p1 = &mut acc;
            blst_p1_add_or_double_affine(p, p, &a);
        }
    }
    let mut out = [0u8; 48];
    // SAFETY: valid point.
    unsafe { blst_p1_compress(out.as_mut_ptr(), &acc) };
    out
}

// ---------------------------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------------------------

#[test]
fn constants_and_the_hash() {
    let p = BigUint::parse_bytes(
        b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
        16,
    )
    .unwrap();
    let half = (&p - 1u32) / 2u32;
    let mut want = half.to_bytes_be();
    while want.len() < 48 {
        want.insert(0, 0);
    }
    assert_eq!(P_HALF.to_vec(), want, "P_HALF = (p - 1)/2");

    let mut s = 1u64;
    for i in 0..20_000 {
        let mut m = [0u8; 64];
        for chunk in m.chunks_exact_mut(8) {
            chunk.copy_from_slice(&splitmix(&mut s).to_le_bytes());
        }
        if i % 7 == 0 {
            m[48..].fill(0);
        }
        let want: Root = Sha256::digest(m).into();
        assert_eq!(hash64(&m), want, "hash64 = SHA-256 on message {i}");
        let (l, r): (Root, Root) = (m[..32].try_into().unwrap(), m[32..].try_into().unwrap());
        assert_eq!(node(&l, &r), hash_pair(&l, &r));
        let pk: BlsPubkey = m[..48].try_into().unwrap();
        assert_eq!(leaf(&pk), pubkey_leaf(&pk));
    }
    println!("hash64, node and leaf equal SHA-256, hash_pair and pubkey_leaf on 20,000 messages");
}

/// Every key of the bench's committees and of the real data, and its negation.
fn all_real_and_bench_committees() -> Vec<(String, SyncCommittee)> {
    let mut out: Vec<(String, SyncCommittee)> = Vec::new();
    for seed in [7u8, 9, 11, 13] {
        out.push((format!("bench committee, seed {seed}"), fake_committee(512, seed).committee));
    }
    let d = "mainnet-p1899";
    let boot = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    out.push(("mainnet bootstrap's current committee".into(), boot.current_sync_committee));
    for i in 0..5 {
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap();
        if !u.next_sync_committee.is_empty() {
            out.push((format!("mainnet update_{i}'s next committee"), u.next_sync_committee));
        }
    }
    let d = "sepolia-gloas-2026-11-10";
    let boot = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    out.push(("Sepolia bootstrap's current committee".into(), boot.current_sync_committee));
    for period in 1378..=1409u64 {
        let layout = if period < 1379 { LcFork::Electra } else { LcFork::Gloas };
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{period}.ssz")), layout, 512).unwrap();
        if !u.next_sync_committee.is_empty() {
            out.push((format!("Sepolia period {period}'s next committee"), u.next_sync_committee));
        }
    }
    // Distinct committees only.
    let mut seen = BTreeSet::new();
    out.retain(|(_, c)| seen.insert(CommitteeSummary::of(c).root()));
    out
}

#[test]
fn compressed_and_key_point_agree_with_blst_and_the_core() {
    let committees = all_real_and_bench_committees();
    let (mut keys, mut mutated) = (0usize, 0usize);
    let mut s = 5u64;
    for (label, c) in &committees {
        for pk in &c.pubkeys {
            for k in [*pk, neg(pk)] {
                let u = uncompress_key(&k).unwrap();
                let point = key_point(&u).unwrap_or_else(|_| panic!("{label}: an honest key"));
                let mut blst_bytes = [0u8; 48];
                // SAFETY: 48 bytes written.
                unsafe { blst_p1_affine_compress(blst_bytes.as_mut_ptr(), &point) };
                assert_eq!(compressed(&u), blst_bytes, "{label}: compressed() = blst");
                assert_eq!(compressed(&u), k, "{label}: compressed() = the committed bytes");
                keys += 1;
            }
            // Mutations of the uncompressed bytes: the same acceptance as the core's decoding.
            let u = uncompress_key(pk).unwrap();
            let mut variants: Vec<UncompressedPubkey> = Vec::new();
            for flag in [0x20u8, 0x40, 0x60, 0x80, 0xa0, 0xc0, 0xe0] {
                let mut v = u;
                v[0] |= flag;
                variants.push(v);
            }
            let mut identity = [0u8; 96];
            identity[0] = 0x40;
            variants.push(identity);
            variants.push([0u8; 96]);
            let mut x_big = u;
            x_big[..48].copy_from_slice(&BigUint_to48(&(BigUint::from_bytes_be(&u[..48]) + p_big())));
            variants.push(x_big);
            let mut y_big = u;
            y_big[48..].copy_from_slice(&BigUint_to48(&(BigUint::from_bytes_be(&u[48..]) + p_big())));
            variants.push(y_big);
            let mut y_neg = u;
            y_neg[48..].copy_from_slice(&BigUint_to48(&(p_big() - BigUint::from_bytes_be(&u[48..]))));
            variants.push(y_neg);
            for _ in 0..4 {
                let mut v = u;
                let bit = (splitmix(&mut s) % 768) as usize;
                v[bit / 8] ^= 1 << (bit % 8);
                variants.push(v);
            }
            for v in variants {
                assert_eq!(key_point(&v).is_ok(), core_accepts(&v), "{label}: key_point = the core's decoding");
                mutated += 1;
            }
        }
    }
    println!(
        "{} committees: compressed() = blst and = the committed bytes on {keys} keys (each key and its \
         negation); key_point accepts exactly what the core accepts on {mutated} mutated encodings",
        committees.len()
    );
}

#[allow(non_snake_case)]
fn BigUint_to48(v: &BigUint) -> [u8; 48] {
    let b = v.to_bytes_be();
    let mut out = [0u8; 48];
    if b.len() > 48 {
        return [0xff; 48];
    }
    out[48 - b.len()..].copy_from_slice(&b);
    out
}

fn p_big() -> BigUint {
    BigUint::parse_bytes(
        b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
        16,
    )
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// The multiproof
// ---------------------------------------------------------------------------------------------

/// The number of nodes a multiproof hashes: the distinct parents at each level.
fn hashes_for(indices: &[usize], depth: usize) -> usize {
    let mut level: BTreeSet<usize> = indices.iter().copied().collect();
    let mut total = 0;
    for _ in 0..depth {
        level = level.iter().map(|i| i >> 1).collect();
        total += level.len();
    }
    total
}

#[test]
fn multiproofs_round_trip_and_every_tampering_is_refused() {
    let mut s = 11u64;
    let (mut sets, mut tampered) = (0usize, 0usize);
    for depth in 1..=10usize {
        let width = 1usize << depth;
        let leaves: Vec<Root> = (0..width).map(|i| hash64(&[(i % 251) as u8; 64])).collect();
        let tree = layers(&leaves, width);
        let root = tree[depth][0];
        let mut patterns: Vec<Vec<usize>> = vec![
            (0..width).collect(),
            vec![0],
            vec![width - 1],
            (0..width).step_by(2).collect(),
            (0..width).filter(|i| i % 3 == 2).collect(),
            (0..width / 2).collect(),
        ];
        for _ in 0..40 {
            let k = 1 + (splitmix(&mut s) as usize % width);
            let mut set: BTreeSet<usize> = BTreeSet::new();
            while set.len() < k {
                set.insert(splitmix(&mut s) as usize % width);
            }
            patterns.push(set.into_iter().collect());
        }
        for indices in patterns.into_iter().filter(|p| !p.is_empty()) {
            let at: Vec<(u32, Root)> = indices.iter().map(|&i| (i as u32, leaves[i])).collect();
            let siblings = multiproof_siblings(&tree, &indices);
            assert_eq!(multiproof_root(&at, &siblings, depth), Some(root), "depth {depth}, {} leaves", indices.len());
            // Siblings: two per hashed node, less the computed ones.
            assert_eq!(
                siblings.len(),
                2 * hashes_for(&indices, depth) - (hashes_for(&indices, depth) + indices.len() - 1)
            );
            sets += 1;
            // Every single sibling altered, a leaf altered, one sibling dropped, one added, two swapped.
            for j in 0..siblings.len().min(64) {
                let mut t = siblings.clone();
                t[j][31] ^= 1;
                assert_ne!(multiproof_root(&at, &t, depth), Some(root));
                tampered += 1;
            }
            let mut bad_leaf = at.clone();
            bad_leaf[0].1[0] ^= 0x80;
            assert_ne!(multiproof_root(&bad_leaf, &siblings, depth), Some(root));
            if !siblings.is_empty() {
                assert_eq!(multiproof_root(&at, &siblings[..siblings.len() - 1], depth), None);
            }
            let mut extra = siblings.clone();
            extra.push(root);
            assert_eq!(multiproof_root(&at, &extra, depth), None, "a leftover sibling");
            if siblings.len() >= 2 && siblings[0] != siblings[1] {
                let mut swapped = siblings.clone();
                swapped.swap(0, 1);
                assert_ne!(multiproof_root(&at, &swapped, depth), Some(root));
            }
            tampered += 4;
            if at.len() >= 2 {
                let mut unordered = at.clone();
                unordered.swap(0, 1);
                assert_eq!(multiproof_root(&unordered, &siblings, depth), None, "leaves out of order");
                let mut duplicated = at.clone();
                duplicated[1] = duplicated[0];
                assert_eq!(multiproof_root(&duplicated, &siblings, depth), None, "a duplicated leaf");
            }
        }
        let beyond = [(width as u32, leaves[0])];
        assert_eq!(multiproof_root(&beyond, &[], depth), None, "an index beyond the tree");
        assert_eq!(multiproof_root(&[], &[], depth), None, "no leaves");
    }
    // 170 non-signers of 512: every third member (the most hashes) and the separate branches.
    let every_third: Vec<usize> = (0..512).filter(|i| i % 3 == 2).collect();
    assert_eq!(hashes_for(&every_third, 9), 425);
    println!(
        "multiproofs: {sets} leaf sets at depths 1-10 round trip; {tampered} tampered proofs refused; 170 \
              non-signers at every third index need 425 hashes (separate branches: 1,530)"
    );
}

/// The rules of `multiproof_root`, weakened one at a time: each weakened verifier accepts a forgery
/// the real one refuses.
mod weakened {
    use super::*;

    /// Takes the right sibling from the proof even when it is a known (computed) node.
    pub fn trusts_the_proof_for_known_siblings(
        leaves: &[(u32, Root)],
        siblings: &[Root],
        depth: usize,
    ) -> Option<Root> {
        let mut layer = leaves.to_vec();
        let mut used = 0;
        for _ in 0..depth {
            let mut next = Vec::new();
            let mut i = 0;
            while i < layer.len() {
                let (index, value) = layer[i];
                let sibling = *siblings.get(used)?;
                used += 1;
                let parent = if index & 1 == 0 { node(&value, &sibling) } else { node(&sibling, &value) };
                // The known right sibling, if any, is skipped: its value is never used.
                i += if index & 1 == 0 && i + 1 < layer.len() && layer[i + 1].0 == index + 1 { 2 } else { 1 };
                next.push((index >> 1, parent));
            }
            layer = next;
        }
        (used == siblings.len()).then_some(layer[0].1)
    }

    /// Ignores siblings left over.
    pub fn ignores_leftovers(leaves: &[(u32, Root)], siblings: &[Root], depth: usize) -> Option<Root> {
        for cut in 0..=siblings.len() {
            if let Some(r) = multiproof_root(leaves, &siblings[..cut], depth) {
                return Some(r);
            }
        }
        None
    }
}

#[test]
fn forged_multiproofs_show_why_each_rule_is_needed() {
    let fake = fake_committee(512, 7);
    let committee = &fake.committee;
    let summary = CommitteeSummary::of(committee);
    let leaves: Vec<Root> = committee.pubkeys.iter().map(pubkey_leaf).collect();
    let tree = layers(&leaves, 512);
    assert_eq!(tree[9][0], summary.pubkeys_root);

    // 1. A sibling that is itself a non-signer's node must be computed, never read from the proof.
    // Non-signers 40 and 41 share a parent. The forger replaces 41's key by any point P' and puts
    // the true leaf of 41 in the proof where the weakened verifier reads 41's node.
    let absent: Vec<usize> = vec![40, 41, 100];
    let mut bits = vec![true; 512];
    for &i in &absent {
        bits[i] = false;
    }
    // P' chosen so that the signing key becomes the attacker's own key A.
    let attacker = SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
    let a_pk = attacker.sk_to_pk().compress();
    let present: Vec<BlsPubkey> = committee.pubkeys.iter().zip(&bits).filter(|(_, b)| **b).map(|(k, _)| *k).collect();
    let true_key = sum_keys(&present);
    // key' = aggregate − (Q40 + P' + Q100) = true_key + Q41 − P'; P' = true_key + Q41 − A.
    let p_prime = sum_keys(&[true_key, committee.pubkeys[41], neg(&a_pk)]);
    let honest = NonSigners::build(committee, &bits).unwrap();
    let mut forged = honest.clone();
    forged.keys[1] = uncompress_key(&p_prime).unwrap();
    // Build the weakened verifier's proof generically: simulate it on the true tree.
    let mut weak_proof = Vec::new();
    {
        let mut layer: Vec<usize> = absent.clone();
        for nodes in tree.iter().take(9) {
            let mut next = Vec::new();
            let mut i = 0;
            while i < layer.len() {
                let index = layer[i];
                weak_proof.push(nodes[index ^ 1]);
                i += if index & 1 == 0 && i + 1 < layer.len() && layer[i + 1] == index + 1 { 2 } else { 1 };
                next.push(index >> 1);
            }
            next.dedup();
            layer = next;
        }
    }
    forged.siblings = weak_proof.clone();
    let forged_leaves: Vec<(u32, Root)> =
        absent.iter().zip(&forged.keys).map(|(&i, k)| (i as u32, leaf(&compressed(k)))).collect();
    assert_eq!(
        weakened::trusts_the_proof_for_known_siblings(&forged_leaves, &forged.siblings, 9),
        Some(summary.pubkeys_root),
        "the weakened verifier accepts a key outside the committee"
    );
    // Under the weakened rule the composed key is the attacker's: aggregate − (Q40 + P' + Q100) = A.
    let composed = sum_keys(&[
        committee.aggregate_pubkey,
        neg(&sum_keys(&[committee.pubkeys[40], p_prime, committee.pubkeys[100]])),
    ]);
    assert_eq!(composed, a_pk, "the forged witness turns the signing key into the attacker's");
    let msg = [9u8; 32];
    let sig = attacker.sign(&msg, DST, &[]);
    assert_eq!(
        sig.verify(true, &msg, DST, &[], &PublicKey::uncompress(&composed).unwrap(), true),
        BLST_ERROR::BLST_SUCCESS
    );
    // The real rule refuses it: with the forged siblings, and with the honest ones.
    assert_ne!(multiproof_root(&forged_leaves, &forged.siblings, 9), Some(summary.pubkeys_root));
    assert_ne!(multiproof_root(&forged_leaves, &honest.siblings, 9), Some(summary.pubkeys_root));
    assert_eq!(signing_key(&summary, &bits, &forged, Sum::Batch).unwrap_err(), StepError::Witness);
    let mut with_honest = forged.clone();
    with_honest.siblings = honest.siblings.clone();
    assert_eq!(signing_key(&summary, &bits, &with_honest, Sum::Batch).unwrap_err(), StepError::Witness);

    // 2. Every sibling must be consumed: a leftover makes the witness non-canonical.
    let honest_leaves: Vec<(u32, Root)> = absent.iter().map(|&i| (i as u32, leaves[i])).collect();
    let mut padded = honest.siblings.clone();
    padded.push([7u8; 32]);
    assert_eq!(weakened::ignores_leftovers(&honest_leaves, &padded, 9), Some(summary.pubkeys_root));
    assert_eq!(multiproof_root(&honest_leaves, &padded, 9), None);
    let mut long = honest.clone();
    long.siblings = padded;
    assert_eq!(signing_key(&summary, &bits, &long, Sum::Batch).unwrap_err(), StepError::Witness);

    // 3. The positions come from the bitfield, not from the witness: a valid multiproof for other
    // members (with their true keys) proves those members, not the absent ones.
    let other: Vec<usize> = vec![1, 2, 3];
    let mut other_bits = vec![true; 512];
    for &i in &other {
        other_bits[i] = false;
    }
    let other_witness = NonSigners::build(committee, &other_bits).unwrap();
    let other_leaves: Vec<(u32, Root)> = other.iter().map(|&i| (i as u32, leaves[i])).collect();
    assert_eq!(multiproof_root(&other_leaves, &other_witness.siblings, 9), Some(summary.pubkeys_root));
    assert_eq!(signing_key(&summary, &bits, &other_witness, Sum::Batch).unwrap_err(), StepError::Witness);
    println!(
        "forgeries: a known sibling read from the proof lets a key outside the committee in (here the \
              attacker's own key becomes the signing key and its signature verifies); a leftover sibling; a \
              valid multiproof of other members: each accepted by the weakened rule, refused by multiproof_root"
    );
}

// ---------------------------------------------------------------------------------------------
// The composition against the core, on real data
// ---------------------------------------------------------------------------------------------

fn yaml(path: PathBuf) -> Value {
    serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

fn hex_n<const N: usize>(v: &Value) -> [u8; N] {
    hex::decode(v.as_str().unwrap().trim().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn as_u64(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap(),
        Value::String(s) => s.trim().parse().unwrap(),
        other => panic!("not an integer: {other:?}"),
    }
}

fn as_version(v: &Value) -> [u8; 4] {
    match v {
        Value::Number(n) => u32::try_from(n.as_u64().unwrap()).unwrap().to_be_bytes(),
        other => hex_n::<4>(other),
    }
}

#[test]
fn step1_equals_the_core_on_every_sync_vector_and_the_real_chains() {
    let bls = Blst::new(Aggregation::Auto);
    let (mut accepted, mut refused) = (0usize, 0usize);
    let all = cases("sync").expect("run scripts/fetch-vectors.sh first");
    assert_eq!(all.len(), 52);
    for (fork, dir) in &all {
        let name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
        let meta = yaml(dir.join("meta.yaml"));
        let cfg = parse_config(&dir.join("config.yaml")).unwrap();
        let spec = &cfg.spec;
        let n = spec.preset.sync_committee_size;
        let gvr: Root = hex_n(&meta["genesis_validators_root"]);
        let trusted: Root = hex_n(&meta["trusted_block_root"]);
        let lc = |f: ferrata_core::fork::Fork| f.lc_fork().unwrap();
        let bootstrap_fork = lc(cfg.fork_for_digest(&gvr, hex_n(&meta["bootstrap_fork_digest"])).unwrap());
        let store_fork = lc(cfg.fork_for_version(as_version(&meta["store_fork_version"])).unwrap());
        let bootstrap = LightClientBootstrap::decode(
            &read_ssz_snappy(&dir.join("bootstrap.ssz_snappy")).unwrap(),
            bootstrap_fork,
            n,
        )
        .unwrap();
        let bootstrap = upgrade_bootstrap(bootstrap, bootstrap_fork, store_fork, spec).unwrap();
        let mut policy =
            FinalizedStore::initialize_with(&trusted, &bootstrap, store_fork, spec, &bls, Adoption::OnChain).unwrap();
        let ctx = Context { spec, genesis_validators_root: gvr, bls: &bls };
        for (i, step) in yaml(dir.join("steps.yaml")).as_sequence().unwrap().iter().enumerate() {
            let (kind, body) = step.as_mapping().and_then(|m| m.iter().next()).unwrap();
            match kind.as_str().unwrap() {
                "process_update" => {
                    let update_fork = lc(cfg.fork_for_digest(&gvr, hex_n(&body["update_fork_digest"])).unwrap());
                    let file = body["update"].as_str().unwrap().trim();
                    let update = LightClientUpdate::decode(
                        &read_ssz_snappy(&dir.join(format!("{file}.ssz_snappy"))).unwrap(),
                        update_fork,
                        n,
                    )
                    .unwrap();
                    let update = upgrade_update(update, update_fork, policy.core().fork, spec).unwrap();
                    let current_slot = as_u64(&body["current_slot"]);
                    let store = CompactStore::of(policy.core());
                    let committee = signing_committee(policy.core(), &update, spec).cloned();
                    let label = format!("{name} step {i}");
                    match compare(
                        &label,
                        &store,
                        &CompactUpdate::of(&update),
                        committee.as_ref(),
                        current_slot,
                        spec,
                        &gvr,
                    ) {
                        Ok(_) => accepted += 1,
                        Err(_) => refused += 1,
                    }
                    let _ = policy.process_update(&update, current_slot, &ctx);
                }
                "force_update" => {}
                "upgrade_store" => {
                    let to = lc(cfg.fork_for_version(as_version(&body["store_fork_version"])).unwrap());
                    policy.upgrade(to, spec).unwrap();
                }
                other => panic!("{name}: unknown step {other}"),
            }
        }
    }
    let vectors = (accepted, refused);
    assert!(accepted > 50 && refused > 10, "the vectors exercise both outcomes: {vectors:?}");

    // Mainnet: the bootstrap and five updates.
    let spec = mainnet();
    let gvr = root(MAINNET_GVR);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let d = "mainnet-p1899";
    let bootstrap = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy = FinalizedStore::initialize_with(
        &root(MAINNET_TRUSTED),
        &bootstrap,
        LcFork::Electra,
        &spec,
        &bls,
        Adoption::OnChain,
    )
    .unwrap();
    let (mut mainnet_accepted, mut mainnet_refused) = (0, 0);
    for i in 0..5 {
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap();
        let store = CompactStore::of(policy.core());
        let committee = signing_committee(policy.core(), &u, &spec).cloned();
        for (kind, update) in [("as served", u.clone()), ("as a finality update", as_finality(&u))] {
            // Both outcomes are compared; update_0 as a finality update does not advance the
            // bootstrap's store (NotNewer), alike.
            match compare(
                &format!("mainnet update_{i} {kind}"),
                &store,
                &CompactUpdate::of(&update),
                committee.as_ref(),
                u.signature_slot,
                &spec,
                &gvr,
            ) {
                Ok(_) => mainnet_accepted += 1,
                Err(_) => mainnet_refused += 1,
            }
        }
        policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap();
    }

    // Sepolia through Gloas.
    let spec = sepolia();
    let gvr = root(SEPOLIA_GVR);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let d = "sepolia-gloas-2026-11-10";
    let boot = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy =
        FinalizedStore::initialize_with(&root(SEPOLIA_TRUSTED), &boot, LcFork::Electra, &spec, &bls, Adoption::OnChain)
            .unwrap();
    let mut sepolia_accepted = 0;
    for period in 1378..=1409u64 {
        if period == 1379 {
            policy.upgrade(LcFork::Gloas, &spec).unwrap();
        }
        let layout = if period < 1379 { LcFork::Electra } else { LcFork::Gloas };
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{period}.ssz")), layout, 512).unwrap();
        let store = CompactStore::of(policy.core());
        let committee = signing_committee(policy.core(), &u, &spec).cloned();
        compare(
            &format!("Sepolia period {period}"),
            &store,
            &CompactUpdate::of(&u),
            committee.as_ref(),
            u.signature_slot,
            &spec,
            &gvr,
        )
        .unwrap_or_else(|e| panic!("Sepolia period {period}: {e:?}"));
        sepolia_accepted += 1;
        policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap();
    }
    let fu = sepolia_finality_update(&fixture(d, "finality_update.ssz"));
    let store = CompactStore::of(policy.core());
    let committee = signing_committee(policy.core(), &fu, &spec).cloned();
    compare(
        "Sepolia finality update",
        &store,
        &CompactUpdate::of(&fu),
        committee.as_ref(),
        fu.signature_slot,
        &spec,
        &gvr,
    )
    .unwrap();
    sepolia_accepted += 1;
    println!(
        "step1 = the core's step1, job for job and code for code (batch and chain sums): official sync vectors \
         {} updates ({} accepted by step 1, {} refused alike); mainnet {} (5 updates as served and as finality \
         updates: {mainnet_accepted} accepted, {mainnet_refused} refused alike); Sepolia {sepolia_accepted} (32 period \
         updates through Gloas and the finality update, all accepted)",
        vectors.0 + vectors.1,
        vectors.0,
        vectors.1,
        mainnet_accepted + mainnet_refused
    );
    assert!(mainnet_accepted >= 9, "the mainnet updates reach the signing key");
}

/// The store after mainnet update_0, update_1 as a finality update, with a committee's summary in
/// place of the signing committee (the method of `residue-host measure`).
fn substituted_base() -> (ChainSpec, Root, CompactStore, LightClientUpdate, bool) {
    let spec = mainnet();
    let gvr = root(MAINNET_GVR);
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let d = "mainnet-p1899";
    let bootstrap = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let u0 = LightClientUpdate::decode(&fixture(d, "update_0.ssz"), LcFork::Electra, 512).unwrap();
    let u1 = LightClientUpdate::decode(&fixture(d, "update_1.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy = FinalizedStore::initialize_with(
        &root(MAINNET_TRUSTED),
        &bootstrap,
        LcFork::Electra,
        &spec,
        &bls,
        Adoption::OnChain,
    )
    .unwrap();
    policy.process_update(&u0, u0.signature_slot + 1, &ctx).unwrap();
    let store = CompactStore::of(policy.core());
    let real = signing_committee(policy.core(), &u1, &spec).unwrap();
    let current = store.current == CommitteeSummary::of(real);
    (spec, gvr, store, as_finality(&u1), current)
}

fn substitute(store: &CompactStore, current: bool, c: &SyncCommittee) -> CompactStore {
    let mut s = store.clone();
    if current {
        s.current = CommitteeSummary::of(c);
    } else {
        s.next = Some(CommitteeSummary::of(c));
    }
    s
}

#[test]
fn differential_of_at_least_2000_committee_bitfield_pairs() {
    let (spec, gvr, store, update, current) = substituted_base();
    let mut committees = all_real_and_bench_committees();
    // A committee with a duplicated key and a key and its negation (absent together: doubling
    // and a sum through the identity inside the batch addition).
    let mut odd = fake_committee(512, 21).committee;
    odd.pubkeys[1] = neg(&odd.pubkeys[0]);
    odd.pubkeys[3] = odd.pubkeys[2];
    odd.pubkeys[5] = odd.pubkeys[4];
    odd.aggregate_pubkey = sum_keys(&odd.pubkeys);
    committees.push(("bench committee with a negated and two duplicated keys".into(), odd));
    let mut s = 99u64;
    let (mut cases_run, mut by_participation) = (0usize, std::collections::BTreeMap::<u64, usize>::new());
    for (ci, (label, committee)) in committees.iter().enumerate() {
        let store = substitute(&store, current, committee);
        let mut fields: Vec<Vec<bool>> = Vec::new();
        for signers in [342usize, 343, 400, 510, 511, 512] {
            for seed in 0..2u64 {
                fields.push(random_bits(512, 512 - signers, (ci as u64) << 32 | (signers as u64) << 8 | seed));
            }
        }
        fields.push((0..512).map(|i| i % 3 != 2).collect());
        fields.push((0..512).map(|i| i >= 170).collect());
        fields.push((0..512).map(|i| i < 342).collect());
        let mut special = vec![true; 512];
        for i in 0..170 {
            special[i * 3 % 512] = false;
        }
        for i in [0, 1, 2, 3, 4, 5] {
            special[i] = false;
        }
        let missing = 170 - special.iter().filter(|b| !**b).count().min(170);
        let mut j = 6;
        let mut need = missing;
        while need > 0 && j < 512 {
            if special[j] {
                special[j] = false;
                need -= 1;
            }
            j += 1;
        }
        while special.iter().filter(|b| !**b).count() > 170 {
            let k = special.iter().rposition(|b| !*b).unwrap();
            special[k] = true;
        }
        fields.push(special);
        while fields.len() < 52 {
            let signers = 342 + (splitmix(&mut s) % 171) as usize;
            fields.push(random_bits(512, 512 - signers, splitmix(&mut s)));
        }
        for bits in fields {
            let signers = bits.iter().filter(|b| **b).count() as u64;
            let mut u = update.clone();
            u.sync_aggregate.bits = bits;
            let r = compare(
                &format!("{label}, {signers} signers"),
                &store,
                &CompactUpdate::of(&u),
                Some(committee),
                u.signature_slot,
                &spec,
                &gvr,
            );
            assert!(r.is_ok(), "{label}, {signers} signers: {r:?}");
            cases_run += 1;
            *by_participation.entry(signers).or_default() += 1;
        }
    }
    assert!(cases_run >= 2000, "{cases_run} cases");
    let fixed: Vec<String> = [342u64, 343, 400, 510, 511, 512]
        .iter()
        .map(|p| format!("{p}: {}", by_participation.get(p).copied().unwrap_or(0)))
        .collect();
    println!(
        "differential: {cases_run} (committee, bitfield) pairs over {} committees ({} distinct participations; \
         at {}): residue_agg::step1 = the core's step1, job for job (the same -key), batch and chain sums",
        committees.len(),
        by_participation.len(),
        fixed.join(", ")
    );
}

#[test]
fn refusals_are_the_cores() {
    let (spec, gvr, store, update, current) = substituted_base();
    let fake = fake_committee(512, 7);
    let committee = &fake.committee;
    let store = substitute(&store, current, committee);
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let bits: Vec<bool> = (0..512).map(|i| i % 3 != 2).collect();
    let mut u = update.clone();
    u.sync_aggregate.signature = sign(&fake, &bits, &sync_committee_signing_root(&update, &ctx));
    u.sync_aggregate.bits = bits.clone();
    let cu = CompactUpdate::of(&u);
    let slot = u.signature_slot;
    let old = SignerWitness::build(committee, &bits).unwrap();
    let new = NonSigners::build(committee, &bits).unwrap();
    let core = |w: &SignerWitness| steps::step1(&store, &cu, w, slot, &spec, &gvr, 1).map(|_| ()).map_err(|e| e.code());
    let mine = |w: &NonSigners| {
        let a = residue_agg::step1(&store, &cu, w, slot, &spec, &gvr, 1, Sum::Batch).map(|_| ()).map_err(|e| e.code());
        let b = residue_agg::step1(&store, &cu, w, slot, &spec, &gvr, 1, Sum::Chain).map(|_| ()).map_err(|e| e.code());
        assert_eq!(a, b);
        a
    };
    assert_eq!(core(&old), Ok(()));
    assert_eq!(mine(&new), Ok(()));
    let witness = StepError::Witness.code();
    let mut count = 0;
    let mut both = |label: &str, o: SignerWitness, n: NonSigners, want: u16| {
        assert_eq!(core(&o), Err(want), "{label}: the core");
        assert_eq!(mine(&n), Err(want), "{label}: residue_agg");
        count += 1;
    };
    // A wrong key: another member's (a signer's) key at a non-signer's place.
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys[5] = uncompress_key(&committee.pubkeys[0]).unwrap();
    n.keys[5] = o.keys[5];
    both("another member's key", o, n, witness);
    // A key not in the committee.
    let other = fake_committee(512, 9);
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys[5] = uncompress_key(&other.committee.pubkeys[5]).unwrap();
    n.keys[5] = o.keys[5];
    both("a key outside the committee", o, n, witness);
    // A duplicated non-signer: one non-signer's key twice.
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys[6] = o.keys[5];
    o.branches[6] = o.branches[5].clone();
    n.keys[6] = n.keys[5];
    both("a non-signer's key given twice", o, n, witness);
    // The negated key (on the curve, the other sign): its leaf is not the committed one.
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys[7] = uncompress_key(&neg(&committee.pubkeys[2 + 3 * 7])).unwrap();
    n.keys[7] = o.keys[7];
    both("a non-signer's key negated", o, n, witness);
    // y + 1: off the curve.
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys[8][95] ^= 1;
    n.keys[8] = o.keys[8];
    both("a key off the curve", o, n, witness);
    // Flags.
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys[9][0] |= 0x80;
    n.keys[9] = o.keys[9];
    both("a key with the compression flag", o, n, witness);
    // One altered node: a branch (the core), the multiproof (residue_agg); every node of the
    // multiproof in turn.
    for j in 0..new.siblings.len() {
        let (mut o, mut n) = (old.clone(), new.clone());
        o.branches[j % old.branches.len()][j % 9][0] ^= 1;
        n.siblings[j][0] ^= 1;
        both("one altered node", o, n, witness);
    }
    // A missing and a leftover node.
    let mut n = new.clone();
    n.siblings.pop();
    let mut o = old.clone();
    o.branches[0].pop();
    both("a node missing", o, n, witness);
    let mut n = new.clone();
    n.siblings.push([1; 32]);
    let mut o = old.clone();
    o.branches[0].push([1; 32]);
    both("a node left over", o, n, witness);
    // Fewer and more keys than absent members.
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys.pop();
    o.branches.pop();
    n.keys.pop();
    both("a key missing", o, n, witness);
    let (mut o, mut n) = (old.clone(), new.clone());
    o.keys.push(o.keys[0]);
    o.branches.push(o.branches[0].clone());
    n.keys.push(n.keys[0]);
    both("a key too many", o, n, witness);
    // A bitfield that disagrees with the witness: the witness of another set of 170.
    let shifted: Vec<bool> = (0..512).map(|i| i % 3 != 1).collect();
    both(
        "the witness of another bitfield",
        SignerWitness::build(committee, &shifted).unwrap(),
        NonSigners::build(committee, &shifted).unwrap(),
        witness,
    );
    // 341 signers: refused by the policy before any witness is read.
    let mut low = u.clone();
    let mut bits341 = bits.clone();
    bits341[0] = false;
    low.sync_aggregate.bits = bits341.clone();
    let cl = CompactUpdate::of(&low);
    let o = SignerWitness::build(committee, &bits341).unwrap();
    let n = NonSigners::build(committee, &bits341).unwrap();
    let below = Error::BelowSupermajority.code();
    assert_eq!(steps::step1(&store, &cl, &o, slot, &spec, &gvr, 1).map(|_| ()).map_err(|e| e.code()), Err(below));
    assert_eq!(
        residue_agg::step1(&store, &cl, &n, slot, &spec, &gvr, 1, Sum::Batch).map(|_| ()).map_err(|e| e.code()),
        Err(below)
    );
    count += 1;
    // Strict encoding.
    let mut bytes = new.encode();
    bytes.push(0);
    assert_eq!(NonSigners::decode(&bytes, 512), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&new.encode()[..10], 512), Err(Error::Malformed));
    println!(
        "refusals: {count} tampered witnesses and the 341-signer update refused with the core's code, \
              by both sums; trailing and truncated encodings refused"
    );
}

/// A sum through the identity and a doubling inside the batch addition, against the chain.
#[test]
fn batch_sum_edge_cases() {
    let fake = fake_committee(512, 21);
    let mut c = fake.committee.clone();
    for i in 0..16 {
        c.pubkeys[2 * i + 1] = neg(&c.pubkeys[2 * i]); // pairs that cancel
    }
    for i in 16..24 {
        c.pubkeys[2 * i + 1] = c.pubkeys[2 * i]; // pairs that double
    }
    c.aggregate_pubkey = sum_keys(&c.pubkeys);
    let summary = CommitteeSummary::of(&c);
    for absent in [32usize, 48, 64, 100, 170] {
        let mut bits = vec![true; 512];
        for b in bits.iter_mut().take(absent) {
            *b = false;
        }
        let w = NonSigners::build(&c, &bits).unwrap();
        let a = signing_key(&summary, &bits, &w, Sum::Batch).unwrap();
        let b = signing_key(&summary, &bits, &w, Sum::Chain).unwrap();
        assert_eq!(neg_key_bytes(&a), neg_key_bytes(&b), "{absent} absent");
        let present: Vec<BlsPubkey> = c.pubkeys.iter().zip(&bits).filter(|(_, b)| **b).map(|(k, _)| *k).collect();
        let mut want = blst_p1_affine::default();
        let mut got = blst_p1_affine::default();
        let mut back = [0u8; 48];
        // SAFETY: valid inputs and outputs.
        unsafe {
            assert_eq!(blst_p1_uncompress(&mut want, sum_keys(&present).as_ptr()), BLST_ERROR::BLST_SUCCESS);
            blst_p1_to_affine(&mut got, &a);
            blst::blst_p1_affine_compress(back.as_mut_ptr(), &got);
        }
        assert_eq!(back, sum_keys(&present), "{absent} absent: the key is the sum of the signers' keys");
    }
    println!("batch sum: cancelling and doubling pairs (the first 32 absent sum to the identity), equal to the chain and to the signers' sum");
}

/// blst's batch addition outside G1. Its pairwise step (`HEAD` in `bulk_addition.c`) takes a pair
/// whose x-coordinates are equal for a doubling or a cancellation, so the identity (stored as
/// (0, 0)) paired with a point of order 3, (0, ±2), in a later round is not added correctly. Keys in
/// G1, as every Ethereum committee's (deposits pass KeyValidate), never sum to a point of order 3;
/// here a committee is built with one on purpose: member 0 is K + T (T = (0, 2)), member 1 is −K, and
/// members 2 to 31 cancel in pairs, all 32 absent. The batch key then differs from the exact sum (the
/// core's, and the chain's) by a point of order 3, and the verdict is the same: the reduced pairing is
/// trivial on rE(Fp), which holds every point of order 3 (3 does not divide r).
#[test]
fn batch_sum_outside_g1_differs_by_an_order_3_point_and_keeps_the_verdict() {
    use blst::{
        blst_fp, blst_fp_from_bendian, blst_p1_affine_compress as compress_affine, blst_p1_affine_in_g1,
        blst_p1_affine_on_curve, blst_p1_is_inf,
    };
    let (spec, gvr, store, update, current) = substituted_base();
    // T = (0, 2): on the curve y^2 = x^3 + 4, of order 3, not in G1.
    let mut y = blst_fp::default();
    let mut two = [0u8; 48];
    two[47] = 2;
    // SAFETY: 48 bytes read.
    unsafe { blst_fp_from_bendian(&mut y, two.as_ptr()) };
    let t = blst_p1_affine { x: blst_fp::default(), y };
    // SAFETY: valid points.
    unsafe {
        assert!(blst_p1_affine_on_curve(&t));
        assert!(!blst_p1_affine_in_g1(&t));
    }
    let fake = fake_committee(512, 31);
    let mut c = fake.committee.clone();
    let mut k0 = blst_p1_affine::default();
    let mut acc = blst_p1::default();
    let mut k0t = [0u8; 48];
    // SAFETY: valid inputs and outputs; aliasing allowed.
    unsafe {
        assert_eq!(blst_p1_uncompress(&mut k0, c.pubkeys[0].as_ptr()), BLST_ERROR::BLST_SUCCESS);
        blst_p1_from_affine(&mut acc, &k0);
        let a: *mut blst_p1 = &mut acc;
        blst_p1_add_or_double_affine(a, a, &t);
        blst_p1_compress(k0t.as_mut_ptr(), &acc);
    }
    c.pubkeys[1] = neg(&c.pubkeys[0]);
    c.pubkeys[0] = k0t;
    for k in 1..16 {
        c.pubkeys[2 * k + 1] = neg(&c.pubkeys[2 * k]);
    }
    c.aggregate_pubkey = sum_keys(&c.pubkeys);
    let mut p0 = blst_p1_affine::default();
    // SAFETY: valid input and output.
    unsafe {
        assert_eq!(blst_p1_uncompress(&mut p0, c.pubkeys[0].as_ptr()), BLST_ERROR::BLST_SUCCESS);
        assert!(!blst_p1_affine_in_g1(&p0), "member 0 is outside G1");
    }
    let store = substitute(&store, current, &c);
    let bits: Vec<bool> = (0..512).map(|i| i >= 32).collect();
    let signing = ferrata_spectests::testkit::FakeCommittee { keys: fake.keys.clone(), committee: c.clone() };
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let neg_key = |job: &Job| -> [u8; 96] {
        let ferrata_core::steps::Phase::Miller { neg_key, .. } = &job.phase else { panic!("step 1 leaves the points") };
        *neg_key
    };
    for (label, message) in
        [("a valid signature", sync_committee_signing_root(&update, &ctx)), ("a wrong message", [7u8; 32])]
    {
        let mut u = update.clone();
        u.sync_aggregate.signature = sign(&signing, &bits, &message);
        u.sync_aggregate.bits = bits.clone();
        let cu = CompactUpdate::of(&u);
        let slot = u.signature_slot;
        let old = SignerWitness::build(&c, &bits).unwrap();
        let new = NonSigners::build(&c, &bits).unwrap();
        let core = steps::step1(&store, &cu, &old, slot, &spec, &gvr, 1).unwrap();
        let chain = residue_agg::step1(&store, &cu, &new, slot, &spec, &gvr, 1, Sum::Chain).unwrap();
        let batch = residue_agg::step1(&store, &cu, &new, slot, &spec, &gvr, 1, Sum::Batch).unwrap();
        assert_eq!(chain, core, "{label}: the chain is the core's exact sum");
        assert_ne!(neg_key(&batch), neg_key(&core), "{label}: the batch sum differs here");
        // The difference is a point of order 3.
        let (mut a, mut b) = (blst_p1_affine::default(), blst_p1_affine::default());
        let mut d = blst_p1::default();
        // SAFETY: the 96-byte serializations step 1 wrote; valid outputs; aliasing allowed.
        unsafe {
            assert_eq!(blst_p1_deserialize(&mut a, neg_key(&batch).as_ptr()), BLST_ERROR::BLST_SUCCESS);
            assert_eq!(blst_p1_deserialize(&mut b, neg_key(&core).as_ptr()), BLST_ERROR::BLST_SUCCESS);
            let mut minus_b = blst_p1::default();
            blst_p1_from_affine(&mut minus_b, &b);
            blst_p1_cneg(&mut minus_b, true);
            blst_p1_from_affine(&mut d, &a);
            let dp: *mut blst_p1 = &mut d;
            blst::blst_p1_add_or_double(dp, dp, &minus_b);
            assert!(!blst_p1_is_inf(&d), "{label}: a nonzero difference");
            let mut d_affine = blst_p1_affine::default();
            blst_p1_to_affine(&mut d_affine, &d);
            let mut three = d;
            let tp: *mut blst_p1 = &mut three;
            blst_p1_add_or_double_affine(tp, tp, &d_affine);
            blst_p1_add_or_double_affine(tp, tp, &d_affine);
            assert!(blst_p1_is_inf(&three), "{label}: the difference has order 3");
            let mut x = [0u8; 48];
            compress_affine(x.as_mut_ptr(), &d_affine);
            assert_eq!(x[1..], [0u8; 47], "{label}: the difference is (0, ±2)");
        }
        // The verdict (Miller loop, final exponentiation) is the same for both keys.
        let (mut jc, mut jb) = (core.clone(), batch.clone());
        jc.step2().unwrap();
        jb.step2().unwrap();
        let (vc, vb) = (jc.step3(), jb.step3());
        assert_eq!(vc, vb, "{label}: the same verdict");
        assert_eq!(vc.is_ok(), label == "a valid signature", "{label}");
    }
    println!(
        "batch sum outside G1: with an order-3 component in a committee's keys, the batch key differs from \
         the exact sum by (0, ±2); the verdict is the same (a valid signature accepted, a wrong message refused, \
         by both keys); with keys in G1 the sums are equal (the differential)"
    );
}
