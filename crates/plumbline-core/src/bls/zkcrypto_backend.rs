//! Pure-Rust backend (zkcrypto/bls12_381), an independent implementation used as a
//! differential oracle for the `blst` backend.

use bls12_381::{
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    multi_miller_loop, G1Affine, G1Projective, G2Affine, G2Prepared, G2Projective, Gt,
};

use super::{check_shape, Aggregation, BlsBackend, DST};
use crate::{
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};

/// zkcrypto `bls12_381` with a chosen signing-key composition.
#[derive(Clone, Copy, Debug, Default)]
pub struct Zkcrypto {
    pub aggregation: Aggregation,
}

impl Zkcrypto {
    /// A backend that always uses `aggregation`.
    pub const fn new(aggregation: Aggregation) -> Self {
        Self { aggregation }
    }
}

/// Decode a compressed G1 point with an on-curve check (no subgroup check).
fn decode_g1(bytes: &BlsPubkey) -> Result<G1Affine, Error> {
    Option::from(G1Affine::from_compressed_unchecked(bytes)).ok_or(Error::PubkeyInvalid)
}

fn sum<'a>(mut keys: impl Iterator<Item = &'a BlsPubkey>) -> Result<G1Projective, Error> {
    keys.try_fold(G1Projective::identity(), |acc, key| Ok(acc + decode_g1(key)?))
}

impl BlsBackend for Zkcrypto {
    fn verify_sync_aggregate(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error> {
        let signers = check_shape(committee, bits)?;
        let selected =
            |want: bool| committee.pubkeys.iter().zip(bits).filter(move |(_, b)| **b == want).map(|(k, _)| k);
        let key = if self.aggregation.subtract(signers, bits.len()) {
            G1Projective::from(decode_g1(&committee.aggregate_pubkey)?) - sum(selected(false))?
        } else {
            sum(selected(true))?
        };
        if bool::from(key.is_identity()) {
            return Err(Error::SigningKeyIdentity);
        }
        let key = G1Affine::from(key);
        if !bool::from(key.is_torsion_free()) {
            return Err(Error::SigningKeyNotInGroup);
        }
        let sig: G2Affine = Option::from(G2Affine::from_compressed(signature)).ok_or(Error::SignatureMalformed)?;
        let message = <G2Projective as HashToCurve<ExpandMsgXmd<sha2_09::Sha256>>>::hash_to_curve(signing_root, DST);
        // e(key, H(m)) · e(−g1, sig) == 1
        let check = multi_miller_loop(&[
            (&key, &G2Prepared::from(G2Affine::from(message))),
            (&-G1Affine::generator(), &G2Prepared::from(sig)),
        ])
        .final_exponentiation();
        if check == Gt::identity() {
            Ok(())
        } else {
            Err(Error::SignatureInvalid)
        }
    }

    fn check_committee(&self, committee: &SyncCommittee) -> Result<(), Error> {
        let sum = sum(committee.pubkeys.iter()).map_err(|_| Error::CommitteeInvalid)?;
        let aggregate = decode_g1(&committee.aggregate_pubkey).map_err(|_| Error::CommitteeInvalid)?;
        if G1Affine::from(sum) == aggregate {
            Ok(())
        } else {
            Err(Error::CommitteeInvalid)
        }
    }
}
