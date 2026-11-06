//! `blst` backend (supranational/blst). RISC Zero publishes an accelerated fork of this library.
#![allow(unsafe_code)]

use blst::{
    blst_bendian_from_fp, blst_final_exp, blst_fp, blst_fp12, blst_fp12_is_one, blst_fp12_mul, blst_fp_from_bendian,
    blst_hash_to_g2, blst_miller_loop, blst_p1, blst_p1_add_or_double, blst_p1_add_or_double_affine, blst_p1_affine,
    blst_p1_affine_generator, blst_p1_affine_in_g1, blst_p1_affine_is_inf, blst_p1_cneg, blst_p1_from_affine,
    blst_p1_is_inf, blst_p1_to_affine, blst_p1_uncompress, blst_p2, blst_p2_affine, blst_p2_affine_in_g2,
    blst_p2_affine_is_inf, blst_p2_to_affine, blst_p2_uncompress, BLST_ERROR,
};

use super::{check_shape, Aggregation, BlsBackend, DST};
use crate::{
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};

/// Length of the canonical encoding of an Fp12 value: twelve Fp coefficients, 48 bytes each,
/// big-endian.
pub const FP12_BYTES: usize = 576;

/// `blst` with a chosen signing-key composition for [`BlsBackend::verify_checked`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Blst {
    pub aggregation: Aggregation,
}

impl Blst {
    /// A backend that composes the signing key by `aggregation`.
    pub const fn new(aggregation: Aggregation) -> Self {
        Self { aggregation }
    }

    /// First half of a verification split across transactions: for a committee that passed a
    /// committee check, everything up to and including the Miller loops. Returns the canonical
    /// encoding of the Miller-loop product, which a program stores until the second half.
    pub fn miller_half(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<[u8; FP12_BYTES], Error> {
        let key = self.checked_signing_key(committee, bits)?;
        Ok(encode_fp12(&miller(&key, signing_root, signature)?))
    }

    /// Second half: the final exponentiation and the check, from [`Blst::miller_half`]'s output.
    pub fn final_half(product: &[u8; FP12_BYTES]) -> Result<(), Error> {
        final_check(&decode_fp12(product))
    }

    /// The signing key for a committee that passed a committee check.
    fn checked_signing_key(&self, committee: &SyncCommittee, bits: &[bool]) -> Result<blst_p1, Error> {
        let signers = check_shape(committee, bits)?;
        let members = |want: bool| committee.pubkeys.iter().zip(bits).filter(move |(_, b)| **b == want).map(|(k, _)| k);
        let mut key = blst_p1::default();
        if self.aggregation.subtract(signers, bits.len()) {
            let aggregate = uncompress(&committee.aggregate_pubkey)?;
            let mut absent = blst_p1::default();
            accumulate(&mut absent, members(false), uncompress)?;
            let key_ptr: *mut blst_p1 = &mut key;
            // SAFETY: valid points; blst allows the output to alias the first input.
            unsafe {
                blst_p1_from_affine(key_ptr, &aggregate);
                blst_p1_cneg(&mut absent, true);
                blst_p1_add_or_double(key_ptr, key_ptr, &absent);
            }
        } else {
            accumulate(&mut key, members(true), uncompress)?;
        }
        if is_identity(&key) {
            return Err(Error::SigningKeyIdentity);
        }
        Ok(key)
    }
}

/// Decode a compressed G1 point with an on-curve check. blst also refuses the x = 0 points,
/// which are the points of order 3.
fn uncompress(bytes: &BlsPubkey) -> Result<blst_p1_affine, Error> {
    let mut out = blst_p1_affine::default();
    // SAFETY: `out` is a valid destination and `bytes` holds the 48 bytes blst reads.
    let rc = unsafe { blst_p1_uncompress(&mut out, bytes.as_ptr()) };
    if rc != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::PubkeyInvalid);
    }
    Ok(out)
}

/// A curve point that is not the identity (no subgroup check).
fn on_curve_not_identity(bytes: &BlsPubkey) -> Result<blst_p1_affine, Error> {
    let point = uncompress(bytes)?;
    // SAFETY: `point` is a valid affine point.
    if unsafe { blst_p1_affine_is_inf(&point) } {
        return Err(Error::PubkeyInvalid);
    }
    Ok(point)
}

/// `KeyValidate`: a curve point, not the identity, in the prime-order subgroup.
fn key_validate(bytes: &BlsPubkey) -> Result<blst_p1_affine, Error> {
    let point = on_curve_not_identity(bytes)?;
    // SAFETY: `point` is a valid affine point.
    if !unsafe { blst_p1_affine_in_g1(&point) } {
        return Err(Error::PubkeyInvalid);
    }
    Ok(point)
}

/// `acc += Σ decode(key)`.
fn accumulate<'a>(
    acc: &mut blst_p1,
    keys: impl Iterator<Item = &'a BlsPubkey>,
    decode: fn(&BlsPubkey) -> Result<blst_p1_affine, Error>,
) -> Result<(), Error> {
    for key in keys {
        let point = decode(key)?;
        let acc_ptr: *mut blst_p1 = acc;
        // SAFETY: blst allows the output to alias the first input; both point to `acc`.
        unsafe { blst_p1_add_or_double_affine(acc_ptr, acc_ptr, &point) };
    }
    Ok(())
}

/// Sum the keys with `decode` and compare with `aggregate_pubkey`.
fn check_sum(committee: &SyncCommittee, decode: fn(&BlsPubkey) -> Result<blst_p1_affine, Error>) -> Result<(), Error> {
    if committee.pubkeys.is_empty() {
        return Err(Error::CommitteeInvalid);
    }
    let mut sum = blst_p1::default();
    accumulate(&mut sum, committee.pubkeys.iter(), decode).map_err(|_| Error::CommitteeInvalid)?;
    let aggregate = uncompress(&committee.aggregate_pubkey).map_err(|_| Error::CommitteeInvalid)?;
    let mut sum_affine = blst_p1_affine::default();
    // SAFETY: valid destination and source.
    unsafe { blst_p1_to_affine(&mut sum_affine, &sum) };
    if sum_affine == aggregate {
        Ok(())
    } else {
        Err(Error::CommitteeInvalid)
    }
}

/// The Miller-loop product `ML(H(m), −key) · ML(signature, g1)`. `key` must not be the identity.
fn miller(key: &blst_p1, signing_root: &Root, signature: &BlsSignature) -> Result<blst_fp12, Error> {
    let mut sig = blst_p2_affine::default();
    // SAFETY: `sig` is a valid destination and `signature` holds the 96 bytes blst reads.
    if unsafe { blst_p2_uncompress(&mut sig, signature.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::SignatureMalformed);
    }
    // SAFETY: `sig` is a valid affine point.
    if !unsafe { blst_p2_affine_in_g2(&sig) } {
        return Err(Error::SignatureMalformed);
    }
    // The identity signature verifies only under the identity key, which is refused earlier.
    // SAFETY: `sig` is a valid affine point.
    if unsafe { blst_p2_affine_is_inf(&sig) } {
        return Err(Error::SignatureInvalid);
    }
    let mut hashed = blst_p2::default();
    let mut hashed_affine = blst_p2_affine::default();
    let mut neg_key = *key;
    let mut neg_key_affine = blst_p1_affine::default();
    let (mut lhs, mut rhs, mut product) = (blst_fp12::default(), blst_fp12::default(), blst_fp12::default());
    // SAFETY: every pointer refers to a live, initialised value of the type blst expects, and
    // the lengths passed are those of the slices pointed to.
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
        blst_miller_loop(&mut lhs, &hashed_affine, &neg_key_affine);
        blst_miller_loop(&mut rhs, &sig, blst_p1_affine_generator());
        blst_fp12_mul(&mut product, &lhs, &rhs);
    }
    Ok(product)
}

/// Final exponentiation, and the check that the result is one.
fn final_check(product: &blst_fp12) -> Result<(), Error> {
    let mut result = blst_fp12::default();
    // SAFETY: valid destination and source.
    unsafe { blst_final_exp(&mut result, product) };
    // SAFETY: `result` is a valid Fp12 element.
    if unsafe { blst_fp12_is_one(&result) } {
        Ok(())
    } else {
        Err(Error::SignatureInvalid)
    }
}

/// `e(key, H(m)) == e(g1, signature)`, as `e(−key, H(m)) · e(g1, signature) == 1`.
fn pairing_check(key: &blst_p1, signing_root: &Root, signature: &BlsSignature) -> Result<(), Error> {
    final_check(&miller(key, signing_root, signature)?)
}

/// The twelve Fp coefficients of an Fp12 value, in a fixed order.
fn coefficients(f: &blst_fp12) -> impl Iterator<Item = &blst_fp> {
    f.fp6.iter().flat_map(|fp6| fp6.fp2.iter()).flat_map(|fp2| fp2.fp.iter())
}

fn encode_fp12(f: &blst_fp12) -> [u8; FP12_BYTES] {
    let mut out = [0u8; FP12_BYTES];
    for (chunk, fp) in out.chunks_exact_mut(48).zip(coefficients(f)) {
        // SAFETY: `chunk` has the 48 bytes blst writes; `fp` is a valid field element.
        unsafe { blst_bendian_from_fp(chunk.as_mut_ptr(), fp) };
    }
    out
}

fn decode_fp12(bytes: &[u8; FP12_BYTES]) -> blst_fp12 {
    let mut f = blst_fp12::default();
    let slots = f.fp6.iter_mut().flat_map(|fp6| fp6.fp2.iter_mut()).flat_map(|fp2| fp2.fp.iter_mut());
    for (fp, chunk) in slots.zip(bytes.chunks_exact(48)) {
        // SAFETY: `chunk` holds the 48 bytes blst reads; `fp` is a valid destination.
        unsafe { blst_fp_from_bendian(fp, chunk.as_ptr()) };
    }
    f
}

fn is_identity(point: &blst_p1) -> bool {
    // SAFETY: `point` is a valid point.
    unsafe { blst_p1_is_inf(point) }
}

impl BlsBackend for Blst {
    fn fast_aggregate_verify(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        check_shape(committee, bits)?;
        let signers = committee.pubkeys.iter().zip(bits).filter(|(_, b)| **b).map(|(k, _)| k);
        let mut key = blst_p1::default(); // the all-zero Jacobian point is the identity
        accumulate(&mut key, signers, key_validate)?;
        if is_identity(&key) {
            return Err(Error::SigningKeyIdentity);
        }
        pairing_check(&key, signing_root, signature)
    }

    fn check_committee(&self, committee: &SyncCommittee) -> Result<(), Error> {
        check_sum(committee, key_validate)
    }

    fn check_committee_onchain(&self, committee: &SyncCommittee) -> Result<(), Error> {
        check_sum(committee, on_curve_not_identity)
    }

    fn verify_checked(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        let key = self.checked_signing_key(committee, bits)?;
        pairing_check(&key, signing_root, signature)
    }
}
