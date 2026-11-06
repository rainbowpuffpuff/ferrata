//! `blst` backend (supranational/blst), the library RISC Zero accelerates.
#![allow(unsafe_code)]

use blst::{
    blst_core_verify_pk_in_g1, blst_p1, blst_p1_add_or_double, blst_p1_add_or_double_affine, blst_p1_affine,
    blst_p1_cneg, blst_p1_from_affine, blst_p1_in_g1, blst_p1_is_inf, blst_p1_to_affine, blst_p1_uncompress,
    blst_p2_affine, blst_p2_affine_in_g2, blst_p2_uncompress, BLST_ERROR,
};

use super::{check_shape, Aggregation, BlsBackend, DST};
use crate::{
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};

/// `blst` with a chosen signing-key composition.
#[derive(Clone, Copy, Debug, Default)]
pub struct Blst {
    pub aggregation: Aggregation,
}

impl Blst {
    /// A backend that always uses `aggregation`.
    pub const fn new(aggregation: Aggregation) -> Self {
        Self { aggregation }
    }
}

/// Decode a compressed G1 point with an on-curve check (no subgroup check).
fn decode_g1(bytes: &BlsPubkey) -> Result<blst_p1_affine, Error> {
    let mut out = blst_p1_affine::default();
    // SAFETY: `out` is a valid destination and `bytes` holds the 48 bytes blst reads.
    let rc = unsafe { blst_p1_uncompress(&mut out, bytes.as_ptr()) };
    if rc != BLST_ERROR::BLST_SUCCESS {
        return Err(Error::PubkeyInvalid);
    }
    Ok(out)
}

/// `acc += Σ keys[i]` over the selected keys.
fn accumulate<'a>(acc: &mut blst_p1, keys: impl Iterator<Item = &'a BlsPubkey>) -> Result<(), Error> {
    for key in keys {
        let point = decode_g1(key)?;
        let acc_ptr: *mut blst_p1 = acc;
        // SAFETY: blst allows the output to alias the first input; both point to `acc`.
        unsafe { blst_p1_add_or_double_affine(acc_ptr, acc_ptr, &point) };
    }
    Ok(())
}

impl BlsBackend for Blst {
    fn verify_sync_aggregate(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        let signers = check_shape(committee, bits)?;
        let n = bits.len();
        let selected =
            |want: bool| committee.pubkeys.iter().zip(bits).filter(move |(_, b)| **b == want).map(|(k, _)| k);

        // The all-zero Jacobian point is the identity.
        let mut key = blst_p1::default();
        if self.aggregation.subtract(signers, n) {
            let aggregate = decode_g1(&committee.aggregate_pubkey)?;
            let mut absent = blst_p1::default();
            accumulate(&mut absent, selected(false))?;
            let key_ptr: *mut blst_p1 = &mut key;
            // SAFETY: valid points; blst allows the output to alias the first input.
            unsafe {
                blst_p1_from_affine(key_ptr, &aggregate);
                blst_p1_cneg(&mut absent, true);
                blst_p1_add_or_double(key_ptr, key_ptr, &absent);
            }
        } else {
            accumulate(&mut key, selected(true))?;
        }

        // KeyValidate on the aggregate: not the identity, and in the prime-order subgroup.
        // SAFETY: `key` is a valid point.
        if unsafe { blst_p1_is_inf(&key) } {
            return Err(Error::SigningKeyIdentity);
        }
        // SAFETY: `key` is a valid point.
        if !unsafe { blst_p1_in_g1(&key) } {
            return Err(Error::SigningKeyNotInGroup);
        }
        let mut key_affine = blst_p1_affine::default();
        // SAFETY: valid destination and source.
        unsafe { blst_p1_to_affine(&mut key_affine, &key) };

        let mut sig = blst_p2_affine::default();
        // SAFETY: `sig` is a valid destination and `signature` holds the 96 bytes blst reads.
        if unsafe { blst_p2_uncompress(&mut sig, signature.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
            return Err(Error::SignatureMalformed);
        }
        // SAFETY: `sig` is a valid point.
        if !unsafe { blst_p2_affine_in_g2(&sig) } {
            return Err(Error::SignatureMalformed);
        }
        // SAFETY: all pointers are valid for the lengths given; no augmentation.
        let rc = unsafe {
            blst_core_verify_pk_in_g1(
                &key_affine,
                &sig,
                true,
                signing_root.as_ptr(),
                signing_root.len(),
                DST.as_ptr(),
                DST.len(),
                core::ptr::null(),
                0,
            )
        };
        if rc == BLST_ERROR::BLST_SUCCESS {
            Ok(())
        } else {
            Err(Error::SignatureInvalid)
        }
    }

    fn check_committee(&self, committee: &SyncCommittee) -> Result<(), Error> {
        let mut sum = blst_p1::default();
        accumulate(&mut sum, committee.pubkeys.iter()).map_err(|_| Error::CommitteeInvalid)?;
        let aggregate = decode_g1(&committee.aggregate_pubkey).map_err(|_| Error::CommitteeInvalid)?;
        let mut sum_affine = blst_p1_affine::default();
        // SAFETY: valid destination and source.
        unsafe { blst_p1_to_affine(&mut sum_affine, &sum) };
        if sum_affine == aggregate {
            Ok(())
        } else {
            Err(Error::CommitteeInvalid)
        }
    }
}
