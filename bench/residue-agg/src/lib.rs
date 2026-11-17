//! A cheaper composition of a light-client update's signing key, for the bench only.
//!
//! **Experimental and unaudited. No deployed program uses this code**: the light-client program
//! and every other guest with an image id compose the key with `ferrata_core::steps`, unchanged.
//!
//! The core composes the signing key as `aggregate − Σ non-signers` (`steps.rs`, `signing_key`):
//! for each member whose bit is clear, its key arrives uncompressed with a Merkle branch of depth 9
//! to the committee's key root; the core decodes the key (a canonical encoding of a curve point,
//! not the identity), re-compresses it with blst, hashes its SSZ leaf, checks the branch and adds
//! the point with one mixed addition. This crate composes the same key from the same committee
//! summary with the same checks on every key, in less work:
//!
//! 1. one Merkle **multiproof** for all the non-signers ([`multiproof_root`]): an interior node on
//!    the paths of several non-signers is hashed once, and the witness carries each sibling the
//!    paths need once, in a fixed order; the leaf positions come from the bitfield;
//! 2. the compressed key read off the canonical uncompressed bytes blst has just accepted
//!    ([`compressed`]: x's bytes, the compression flag, the sign flag iff `y > (p − 1)/2`) instead
//!    of blst's conversion out of Montgomery form;
//! 3. the non-signers' points summed by blst's own batch affine addition (`blst_p1s_add`,
//!    Montgomery's trick: one inversion per round of pairwise additions) instead of one mixed
//!    addition each ([`Sum::Batch`]; [`Sum::Chain`] keeps the core's additions, for comparison);
//! 4. SHA-256 of each 64-byte message (a leaf, a node) as two compressions of a word-aligned
//!    buffer ([`hash64`]), without the streaming hasher's buffering.
//!
//! [`step1`] is `ferrata_core::steps::step1` with [`signing_key`] in place of the core's; the
//! job it returns is the core's `Job`, and the rest (`Job::step2`, the final check,
//! `steps::apply`) is the core's code, unchanged. `bench/RESIDUE.md` §9 gives the argument that
//! the composition accepts exactly the witnesses the core's does (up to SHA-256 collisions) and
//! yields the same key, and the tests.
#![allow(unsafe_code)]

use blst::{
    blst_hash_to_g2, blst_p1, blst_p1_add_or_double, blst_p1_add_or_double_affine, blst_p1_affine,
    blst_p1_affine_is_inf, blst_p1_affine_serialize, blst_p1_cneg, blst_p1_deserialize, blst_p1_from_affine,
    blst_p1_is_inf, blst_p1_to_affine, blst_p1_uncompress, blst_p1s_add, blst_p2, blst_p2_affine,
    blst_p2_affine_in_g2, blst_p2_affine_is_inf, blst_p2_affine_serialize, blst_p2_to_affine, blst_p2_uncompress,
    BLST_ERROR,
};
use ferrata_core::{
    bls::DST,
    fork::ChainSpec,
    steps::{
        apply_core_compact, update_advances, validate_compact, Job, PendingApply, Phase, SigningCommittee,
        StepError,
    },
    types::{BlsPubkey, BlsSignature},
    witness::{
        key_branch_depth, AdoptionProgress, CommitteeSummary, CompactStore, CompactUpdate, HeaderRecord,
        UncompressedPubkey,
    },
    Error, Root,
};
use sha2::digest::generic_array::{typenum::U64, GenericArray};

// ---------------------------------------------------------------------------------------------
// SHA-256 of 64 bytes
// ---------------------------------------------------------------------------------------------

/// SHA-256's initial state.
const IV: [u32; 8] =
    [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

/// The padding block of a 64-byte message: `0x80`, zeros, the length in bits (512) big-endian.
const PAD64: [u8; 64] = {
    let mut b = [0u8; 64];
    b[0] = 0x80;
    b[62] = 0x02;
    b
};

/// The message and its padding block, 128 bytes at a word-aligned address: RISC Zero's SHA-256
/// accelerator reads words, and its `sha2` patch copies unaligned blocks into a new vector first.
#[repr(C, align(4))]
struct Blocks([u8; 128]);

impl Blocks {
    /// The padding block in place, the message to be written into the first 64 bytes.
    fn new() -> Self {
        let mut b = Blocks([0u8; 128]);
        b.0[64..].copy_from_slice(&PAD64);
        b
    }

    /// SHA-256 of the 64-byte message in the first block: the compression function on the
    /// message, then on the padding block (`sha2::compress256`; in the guest, RISC Zero's
    /// accelerator, one call for both blocks).
    fn digest(&self) -> Root {
        // SAFETY: `GenericArray<u8, U64>` has the layout of `[u8; 64]` (generic-array's own
        // `from_slice` casts the same way), so the 128 bytes are two of them.
        let blocks: &[GenericArray<u8, U64>] =
            unsafe { core::slice::from_raw_parts(self.0.as_ptr().cast::<GenericArray<u8, U64>>(), 2) };
        let mut state = IV;
        sha2::compress256(&mut state, blocks);
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(4).zip(state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// SHA-256 of a 64-byte message.
pub fn hash64(message: &[u8; 64]) -> Root {
    let mut b = Blocks::new();
    b.0[..64].copy_from_slice(message);
    b.digest()
}

/// `hash(left ‖ right)`, the Merkle node function.
pub fn node(left: &Root, right: &Root) -> Root {
    let mut b = Blocks::new();
    b.0[..32].copy_from_slice(left);
    b.0[32..64].copy_from_slice(right);
    b.digest()
}

/// The SSZ leaf of a compressed key: `hash(pubkey ‖ 16 zero bytes)` (`witness::pubkey_leaf`).
pub fn leaf(pk: &BlsPubkey) -> Root {
    let mut b = Blocks::new();
    b.0[..48].copy_from_slice(pk);
    b.digest()
}

// ---------------------------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------------------------

/// `(p − 1)/2`, big-endian (`tests/agg.rs` re-derives it from p).
pub const P_HALF: [u8; 48] = [
    0x0d, 0x00, 0x88, 0xf5, 0x1c, 0xbf, 0xf3, 0x4d, 0x25, 0x8d, 0xd3, 0xdb, 0x21, 0xa5, 0xd6, 0x6b, 0xb2, 0x3b,
    0xa5, 0xc2, 0x79, 0xc2, 0x89, 0x5f, 0xb3, 0x98, 0x69, 0x50, 0x7b, 0x58, 0x7b, 0x12, 0x0f, 0x55, 0xff, 0xff,
    0x58, 0xa9, 0xff, 0xff, 0xdc, 0xff, 0x7f, 0xff, 0xff, 0xff, 0xd5, 0x55,
];

/// The compressed encoding (`blst_p1_affine_compress`, the ZCash format) of a key given in the
/// canonical uncompressed encoding: x's 48 bytes with the compression flag (`0x80`) and, iff
/// `y > (p − 1)/2`, the sign flag (`0x20`). Only meaningful for bytes [`key_point`] accepted: every
/// flag clear and x, y below p, so x's top three bits are free and the byte order of y is its
/// numeric order.
pub fn compressed(key: &UncompressedPubkey) -> BlsPubkey {
    let mut out: BlsPubkey = key[..48].try_into().expect("48 bytes");
    let y: &[u8; 48] = key[48..].try_into().expect("48 bytes");
    out[0] |= 0x80 | if *y > P_HALF { 0x20 } else { 0 };
    out
}

/// A non-signer's key: exactly the keys the core's `witness_key` accepts (flags clear; blst's
/// canonical decoding of a point on the curve, which refuses x = 0; not the identity).
pub fn key_point(key: &UncompressedPubkey) -> Result<blst_p1_affine, StepError> {
    // The core refuses 0x80 as malformed and the identity's 0x40 encoding as the identity; blst
    // refuses every other flagged encoding. With the flags clear, blst reads x ‖ y.
    if key[0] & 0xe0 != 0 {
        return Err(StepError::Witness);
    }
    let mut point = blst_p1_affine::default();
    // SAFETY: `point` is a valid destination and `key` holds the 96 bytes blst reads.
    if unsafe { blst_p1_deserialize(&mut point, key.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(StepError::Witness);
    }
    // Kept although the flags rule it out (blst never decodes x ‖ y to the identity).
    // SAFETY: `point` is a valid affine point.
    if unsafe { blst_p1_affine_is_inf(&point) } {
        return Err(StepError::Witness);
    }
    Ok(point)
}

// ---------------------------------------------------------------------------------------------
// The multiproof
// ---------------------------------------------------------------------------------------------

/// The root of a binary SHA-256 tree of depth `depth`, from the leaves at `leaves` (index, value;
/// ascending, distinct, below `2^depth`, at least one) and `siblings`, or `None`.
///
/// Level by level from the leaves up, each known node is paired with its sibling: the sibling's
/// computed value if the sibling is known too (it is then never read from the proof), else the
/// next entry of `siblings`. So `siblings` holds, in that order, exactly the nodes off the
/// leaves' paths that the paths need. `None` if the leaves break the rules, if `siblings` runs
/// out, or if any of it is left over.
pub fn multiproof_root(leaves: &[(u32, Root)], siblings: &[Root], depth: usize) -> Option<Root> {
    if leaves.is_empty() || depth >= 32 {
        return None;
    }
    let ordered = leaves.windows(2).all(|w| w[0].0 < w[1].0);
    if !ordered || leaves[leaves.len() - 1].0 >> depth != 0 {
        return None;
    }
    let mut layer = leaves.to_vec();
    let mut next = Vec::with_capacity(layer.len());
    let mut used = 0usize;
    for _ in 0..depth {
        next.clear();
        let mut i = 0;
        while i < layer.len() {
            let (index, value) = layer[i];
            let parent = if index & 1 == 0 && i + 1 < layer.len() && layer[i + 1].0 == index + 1 {
                i += 2;
                node(&value, &layer[i - 1].1)
            } else {
                let sibling = siblings.get(used)?;
                used += 1;
                i += 1;
                if index & 1 == 0 {
                    node(&value, sibling)
                } else {
                    node(sibling, &value)
                }
            };
            next.push((index >> 1, parent));
        }
        core::mem::swap(&mut layer, &mut next);
    }
    // After `depth` levels every index is 0, and the merging leaves one node.
    (used == siblings.len() && layer.len() == 1).then_some(layer[0].1)
}

/// The siblings [`multiproof_root`] reads for the leaves at `indices` (ascending, distinct), from
/// the tree's layers (`layers[0]` the leaves padded to a power of two, the last the root): host side.
pub fn multiproof_siblings(layers: &[Vec<Root>], indices: &[usize]) -> Vec<Root> {
    let mut known = indices.to_vec();
    let mut out = Vec::new();
    for layer in &layers[..layers.len() - 1] {
        let mut next = Vec::with_capacity(known.len());
        let mut i = 0;
        while i < known.len() {
            let index = known[i];
            if index & 1 == 0 && i + 1 < known.len() && known[i + 1] == index + 1 {
                i += 2;
            } else {
                out.push(layer[index ^ 1]);
                i += 1;
            }
            next.push(index >> 1);
        }
        known = next;
    }
    out
}

/// Every layer of the tree of `leaves` padded with zero chunks to `width` (a power of two).
pub fn layers(leaves: &[Root], width: usize) -> Vec<Vec<Root>> {
    assert!(width.is_power_of_two() && leaves.len() <= width);
    let mut layer = leaves.to_vec();
    layer.resize(width, [0u8; 32]);
    let mut out = vec![layer];
    while out[out.len() - 1].len() > 1 {
        let next = out[out.len() - 1].chunks_exact(2).map(|p| node(&p[0], &p[1])).collect();
        out.push(next);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The witness
// ---------------------------------------------------------------------------------------------

/// The non-signers of one update: for each member whose bit is clear, in index order, its key
/// uncompressed; and one multiproof of all their leaves to the committee's key root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NonSigners {
    pub keys: Vec<UncompressedPubkey>,
    pub siblings: Vec<Root>,
}

impl NonSigners {
    /// Strict encoding: a `u16` key count, the keys, a `u16` sibling count, the siblings.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 96 * self.keys.len() + 32 * self.siblings.len());
        out.extend_from_slice(&u16::try_from(self.keys.len()).expect("fewer than 2^16 keys").to_le_bytes());
        for k in &self.keys {
            out.extend_from_slice(k);
        }
        out.extend_from_slice(&u16::try_from(self.siblings.len()).expect("fewer than 2^16 nodes").to_le_bytes());
        for s in &self.siblings {
            out.extend_from_slice(s);
        }
        out
    }

    /// Strict decoding for a committee of `n`: at most `n` keys and `n` siblings, the input consumed
    /// exactly.
    pub fn decode(b: &[u8], n: usize) -> Result<Self, Error> {
        let count = |at: usize| -> Result<usize, Error> {
            let c = b.get(at..at + 2).ok_or(Error::Malformed)?;
            Ok(u16::from_le_bytes([c[0], c[1]]) as usize)
        };
        let keys = count(0)?;
        if keys > n {
            return Err(Error::Malformed);
        }
        let keys_end = 2 + 96 * keys;
        let siblings = count(keys_end)?;
        if siblings > n || b.len() != keys_end + 2 + 32 * siblings {
            return Err(Error::Malformed);
        }
        Ok(Self {
            keys: b[2..keys_end].chunks_exact(96).map(|c| c.try_into().expect("96 bytes")).collect(),
            siblings: b[keys_end + 2..].chunks_exact(32).map(|c| c.try_into().expect("32 bytes")).collect(),
        })
    }

    /// The witness for the members of `committee` whose bit in `bits` is clear (host side).
    pub fn build(committee: &ferrata_core::types::SyncCommittee, bits: &[bool]) -> Result<Self, Error> {
        let n = committee.pubkeys.len();
        if bits.len() != n {
            return Err(Error::Malformed);
        }
        let leaves: Vec<Root> = committee.pubkeys.iter().map(leaf).collect();
        let tree = layers(&leaves, n.next_power_of_two());
        let absent: Vec<usize> = bits.iter().enumerate().filter(|(_, b)| !**b).map(|(i, _)| i).collect();
        let keys = absent
            .iter()
            .map(|&i| ferrata_core::witness::uncompress_key(&committee.pubkeys[i]))
            .collect::<Result<Vec<_>, _>>()?;
        let siblings = if absent.is_empty() { Vec::new() } else { multiproof_siblings(&tree, &absent) };
        Ok(Self { keys, siblings })
    }
}

// ---------------------------------------------------------------------------------------------
// The signing key
// ---------------------------------------------------------------------------------------------

/// How the non-signers' points are summed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sum {
    /// blst's batch affine addition (`blst_p1s_add`).
    Batch,
    /// One mixed addition per point (`blst_p1_add_or_double_affine`), as the core.
    Chain,
}

/// `aggregate − Σ non-signers`, every non-signer's key checked against the committee's key root
/// through one multiproof. The same refusals as the core's composition: [`Error::PubkeyInvalid`]
/// for an aggregate that does not decode, [`StepError::Witness`] for a witness that does not fit
/// (count, a key's encoding, the multiproof), [`Error::SigningKeyIdentity`].
pub fn signing_key(
    committee: &CommitteeSummary,
    bits: &[bool],
    witness: &NonSigners,
    sum: Sum,
) -> Result<blst_p1, StepError> {
    let depth = key_branch_depth(bits.len());
    let absent: Vec<u32> = bits.iter().enumerate().filter(|(_, b)| !**b).map(|(i, _)| i as u32).collect();
    if witness.keys.len() != absent.len() {
        return Err(StepError::Witness);
    }
    let mut aggregate = blst_p1_affine::default();
    // SAFETY: valid destination; the key holds the 48 bytes blst reads.
    if unsafe { blst_p1_uncompress(&mut aggregate, committee.aggregate_pubkey.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::PubkeyInvalid.into());
    }
    let mut points = Vec::with_capacity(absent.len());
    let mut leaves = Vec::with_capacity(absent.len());
    for (&index, key) in absent.iter().zip(&witness.keys) {
        points.push(key_point(key)?);
        leaves.push((index, leaf(&compressed(key))));
    }
    if absent.is_empty() {
        if !witness.siblings.is_empty() {
            return Err(StepError::Witness);
        }
    } else if multiproof_root(&leaves, &witness.siblings, depth) != Some(committee.pubkeys_root) {
        return Err(StepError::Witness);
    }
    let mut absent_sum = blst_p1::default();
    match sum {
        Sum::Batch if !points.is_empty() => {
            // A contiguous array: the first pointer, then null (blst's own bindings do the same).
            let ptrs: [*const blst_p1_affine; 2] = [points.as_ptr(), core::ptr::null()];
            // SAFETY: `points` holds `points.len()` valid affine points, contiguous.
            unsafe { blst_p1s_add(&mut absent_sum, ptrs.as_ptr(), points.len()) };
        }
        Sum::Batch => {}
        Sum::Chain => {
            for p in &points {
                let acc: *mut blst_p1 = &mut absent_sum;
                // SAFETY: blst allows the output to alias the first input.
                unsafe { blst_p1_add_or_double_affine(acc, acc, p) };
            }
        }
    }
    let mut key = blst_p1::default();
    let key_ptr: *mut blst_p1 = &mut key;
    // SAFETY: valid points; blst allows the output to alias the first input.
    unsafe {
        blst_p1_from_affine(key_ptr, &aggregate);
        blst_p1_cneg(&mut absent_sum, true);
        blst_p1_add_or_double(key_ptr, key_ptr, &absent_sum);
    }
    // SAFETY: `key` is a valid point.
    if unsafe { blst_p1_is_inf(&key) } {
        return Err(Error::SigningKeyIdentity.into());
    }
    Ok(key)
}

// ---------------------------------------------------------------------------------------------
// Step 1, as the core's, with this composition
// ---------------------------------------------------------------------------------------------

/// `ferrata_core::steps::step1` (at 510f05cf, `steps.rs:550-602`) with [`signing_key`] in place
/// of the core's `signing_key`; nothing else differs.
#[allow(clippy::too_many_arguments)]
pub fn step1(
    store: &CompactStore,
    update: &CompactUpdate,
    signers: &NonSigners,
    current_slot: u64,
    spec: &ChainSpec,
    genesis_validators_root: &Root,
    parts: u8,
    sum: Sum,
) -> Result<Job, StepError> {
    // The policy: finality updates only, signed by two thirds of the committee.
    if !update.is_finality_update() {
        return Err(Error::NotFinalized.into());
    }
    let n = spec.preset.sync_committee_size as u64;
    if update.sync_aggregate.participants() * 3 < n * 2 {
        return Err(Error::BelowSupermajority.into());
    }
    let (signer, signing_root) = validate_compact(store, update, current_slot, spec, genesis_validators_root)?;
    if !update_advances(store, update, spec) {
        return Err(Error::NotNewer.into());
    }
    let (_, outcome) = apply_core_compact(store, &update.finalized_header.beacon, update.next_sync_committee, spec)?;

    // The signature, as `verify_checked` with the subtractive composition: key, then signature.
    let committee = match signer {
        SigningCommittee::Current => &store.current,
        SigningCommittee::Next => store.next.as_ref().ok_or(Error::CommitteeUnknown)?,
    };
    let key = signing_key(committee, &update.sync_aggregate.bits, signers, sum)?;
    let signature = decode_signature(&update.sync_aggregate.signature)?;
    let (hm, neg_key) = hash_and_negate(&signing_root, &key);

    let fork = store.fork;
    let adoption = if outcome.adopted {
        let next = update.next_sync_committee.ok_or(Error::CommitteeUnknown)?;
        Some(AdoptionProgress::new(next, parts))
    } else {
        None
    };
    Ok(Job {
        store_digest: store.digest(),
        update_digest: update.digest(fork),
        phase: Phase::Miller { hm, neg_key, signature: serialize_g2(&signature) },
        adoption,
        pending: PendingApply::Update {
            finalized: update.finalized_header.beacon.clone(),
            next: update.next_sync_committee,
            record: HeaderRecord::of(&update.finalized_header, fork, spec),
        },
    })
}

/// The signature as a point of G2, refused as in `verify_checked` (a copy of the core's
/// `decode_signature`, `steps.rs:669-685`).
fn decode_signature(signature: &BlsSignature) -> Result<blst_p2_affine, StepError> {
    let mut sig = blst_p2_affine::default();
    // SAFETY: valid destination; `signature` holds the 96 bytes blst reads.
    if unsafe { blst_p2_uncompress(&mut sig, signature.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::SignatureMalformed.into());
    }
    // SAFETY: `sig` is a valid affine point.
    if !unsafe { blst_p2_affine_in_g2(&sig) } {
        return Err(Error::SignatureMalformed.into());
    }
    // The identity signature verifies only under the identity key, which is refused earlier.
    // SAFETY: `sig` is a valid affine point.
    if unsafe { blst_p2_affine_is_inf(&sig) } {
        return Err(Error::SignatureInvalid.into());
    }
    Ok(sig)
}

/// H(m) on G2 and −key, both affine and serialized uncompressed (a copy of the core's
/// `hash_and_negate`, `steps.rs:688-713`).
fn hash_and_negate(signing_root: &Root, key: &blst_p1) -> ([u8; 192], [u8; 96]) {
    let mut hashed = blst_p2::default();
    let mut hashed_affine = blst_p2_affine::default();
    let mut neg_key = *key;
    let mut neg_key_affine = blst_p1_affine::default();
    let (mut hm, mut nk) = ([0u8; 192], [0u8; 96]);
    // SAFETY: every pointer refers to a live, initialised value of the type blst expects, and the
    // lengths passed are those of the slices pointed to.
    unsafe {
        blst_hash_to_g2(
            &mut hashed,
            signing_root.as_ptr(),
            signing_root.len(),
            DST.as_ptr(),
            DST.len(),
            core::ptr::null(),
            0,
        );
        blst_p2_to_affine(&mut hashed_affine, &hashed);
        blst_p1_cneg(&mut neg_key, true);
        blst_p1_to_affine(&mut neg_key_affine, &neg_key);
        blst_p2_affine_serialize(hm.as_mut_ptr(), &hashed_affine);
        blst_p1_affine_serialize(nk.as_mut_ptr(), &neg_key_affine);
    }
    (hm, nk)
}

fn serialize_g2(p: &blst_p2_affine) -> [u8; 192] {
    let mut out = [0u8; 192];
    // SAFETY: `out` has the 192 bytes blst writes.
    unsafe { blst_p2_affine_serialize(out.as_mut_ptr(), p) };
    out
}

/// The signing key as the job carries it (−key, uncompressed): for comparing compositions.
pub fn neg_key_bytes(key: &blst_p1) -> [u8; 96] {
    let mut neg = *key;
    let mut affine = blst_p1_affine::default();
    let mut out = [0u8; 96];
    // SAFETY: valid source and destinations.
    unsafe {
        blst_p1_cneg(&mut neg, true);
        blst_p1_to_affine(&mut affine, &neg);
        blst_p1_affine_serialize(out.as_mut_ptr(), &affine);
    }
    out
}
