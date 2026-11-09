//! Pure-Rust backend (zkcrypto/bls12_381), an independent implementation used as a
//! differential oracle for the `blst` backend. Its hash-to-curve sits behind the crate's
//! `experimental` feature; every signature in the spec vectors verifies under it.

use bls12_381::{
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    multi_miller_loop, G1Affine, G1Projective, G2Affine, G2Prepared, G2Projective, Gt,
};

use super::{check_shape, Aggregation, BlsBackend, DST};
use crate::{
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};

/// zkcrypto `bls12_381` with a chosen signing-key composition for [`BlsBackend::verify_checked`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Zkcrypto {
    pub aggregation: Aggregation,
}

impl Zkcrypto {
    /// A backend that composes the signing key by `aggregation`.
    pub const fn new(aggregation: Aggregation) -> Self {
        Self { aggregation }
    }
}

/// Decode a compressed G1 point with an on-curve check only.
fn uncompress(bytes: &BlsPubkey) -> Result<G1Affine, Error> {
    Option::from(G1Affine::from_compressed_unchecked(bytes)).ok_or(Error::PubkeyInvalid)
}

/// `KeyValidate`: a curve point, not the identity, in the prime-order subgroup.
/// A curve point that is not the identity (no subgroup check).
fn on_curve_not_identity(bytes: &BlsPubkey) -> Result<G1Affine, Error> {
    let point = uncompress(bytes)?;
    if bool::from(point.is_identity()) {
        return Err(Error::PubkeyInvalid);
    }
    Ok(point)
}

/// Sum the keys with `decode` and compare with `aggregate_pubkey`.
fn check_sum(committee: &SyncCommittee, decode: fn(&BlsPubkey) -> Result<G1Affine, Error>) -> Result<(), Error> {
    if committee.pubkeys.is_empty() {
        return Err(Error::CommitteeInvalid);
    }
    let total = sum(committee.pubkeys.iter(), decode).map_err(|_| Error::CommitteeInvalid)?;
    let aggregate = uncompress(&committee.aggregate_pubkey).map_err(|_| Error::CommitteeInvalid)?;
    if G1Affine::from(total) == aggregate {
        Ok(())
    } else {
        Err(Error::CommitteeInvalid)
    }
}

fn key_validate(bytes: &BlsPubkey) -> Result<G1Affine, Error> {
    let point: G1Affine = Option::from(G1Affine::from_compressed(bytes)).ok_or(Error::PubkeyInvalid)?;
    if bool::from(point.is_identity()) {
        return Err(Error::PubkeyInvalid);
    }
    Ok(point)
}

fn sum<'a>(
    mut keys: impl Iterator<Item = &'a BlsPubkey>,
    decode: fn(&BlsPubkey) -> Result<G1Affine, Error>,
) -> Result<G1Projective, Error> {
    keys.try_fold(G1Projective::identity(), |acc, key| Ok(acc + decode(key)?))
}

/// `e(−key, H(m)) · e(g1, signature) == 1`. `key` must not be the identity.
fn pairing_check(key: &G1Projective, signing_root: &Root, signature: &BlsSignature) -> Result<(), Error> {
    let sig: G2Affine = Option::from(G2Affine::from_compressed(signature)).ok_or(Error::SignatureMalformed)?;
    if bool::from(sig.is_identity()) {
        return Err(Error::SignatureInvalid);
    }
    let hashed = <G2Projective as HashToCurve<ExpandMsgXmd<sha2_09::Sha256>>>::hash_to_curve(signing_root, DST);
    let check = multi_miller_loop(&[
        (&G1Affine::from(-key), &G2Prepared::from(G2Affine::from(hashed))),
        (&G1Affine::generator(), &G2Prepared::from(sig)),
    ])
    .final_exponentiation();
    if check == Gt::identity() {
        Ok(())
    } else {
        Err(Error::SignatureInvalid)
    }
}

impl BlsBackend for Zkcrypto {
    fn fast_aggregate_verify(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        check_shape(committee, bits)?;
        let signers = committee.pubkeys.iter().zip(bits).filter(|(_, b)| **b).map(|(k, _)| k);
        let key = sum(signers, key_validate)?;
        if bool::from(key.is_identity()) {
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
        let signers = check_shape(committee, bits)?;
        let members = |want: bool| committee.pubkeys.iter().zip(bits).filter(move |(_, b)| **b == want).map(|(k, _)| k);
        let key = if self.aggregation.subtract(signers, bits.len()) {
            G1Projective::from(uncompress(&committee.aggregate_pubkey)?) - sum(members(false), uncompress)?
        } else {
            sum(members(true), uncompress)?
        };
        if bool::from(key.is_identity()) {
            return Err(Error::SigningKeyIdentity);
        }
        pairing_check(&key, signing_root, signature)
    }
}
