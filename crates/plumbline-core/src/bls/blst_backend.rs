//! `blst` backend (supranational/blst). RISC Zero publishes an accelerated fork of this library.
#![allow(unsafe_code)]

use blst::{
    blst_final_exp, blst_fp12, blst_fp12_is_one, blst_fp12_mul, blst_hash_to_g2, blst_miller_loop, blst_p1,
    blst_p1_add_or_double, blst_p1_add_or_double_affine, blst_p1_affine, blst_p1_affine_generator,
    blst_p1_affine_in_g1, blst_p1_affine_is_inf, blst_p1_cneg, blst_p1_from_affine, blst_p1_is_inf, blst_p1_to_affine,
    blst_p1_uncompress, blst_p2, blst_p2_affine, blst_p2_affine_in_g2, blst_p2_affine_is_inf, blst_p2_to_affine,
    blst_p2_uncompress, BLST_ERROR,
};

use super::{check_shape, Aggregation, BlsBackend, DST};
use crate::{
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};

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

/// `KeyValidate`: a curve point, not the identity, in the prime-order subgroup.
fn key_validate(bytes: &BlsPubkey) -> Result<blst_p1_affine, Error> {
    let point = uncompress(bytes)?;
    // SAFETY: `point` is a valid affine point.
    if unsafe { blst_p1_affine_is_inf(&point) || !blst_p1_affine_in_g1(&point) } {
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

/// `e(key, H(m)) == e(g1, signature)`, computed as `e(−key, H(m)) · e(g1, signature) == 1`: two
/// Miller loops, then one final exponentiation. `key` must not be the identity.
fn pairing_check(key: &blst_p1, signing_root: &Root, signature: &BlsSignature) -> Result<(), Error> {
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
    let (mut lhs, mut rhs, mut product, mut result) =
        (blst_fp12::default(), blst_fp12::default(), blst_fp12::default(), blst_fp12::default());
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
        // The Miller loops: the seam where a multi-transaction verification can split.
        blst_miller_loop(&mut lhs, &hashed_affine, &neg_key_affine);
        blst_miller_loop(&mut rhs, &sig, blst_p1_affine_generator());
        blst_fp12_mul(&mut product, &lhs, &rhs);
        blst_final_exp(&mut result, &product);
    }
    // SAFETY: `result` is a valid Fp12 element.
    if unsafe { blst_fp12_is_one(&result) } {
        Ok(())
    } else {
        Err(Error::SignatureInvalid)
    }
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
        if committee.pubkeys.is_empty() {
            return Err(Error::CommitteeInvalid);
        }
        let mut sum = blst_p1::default();
        accumulate(&mut sum, committee.pubkeys.iter(), key_validate).map_err(|_| Error::CommitteeInvalid)?;
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

    fn verify_checked(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
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
        pairing_check(&key, signing_root, signature)
    }
}
