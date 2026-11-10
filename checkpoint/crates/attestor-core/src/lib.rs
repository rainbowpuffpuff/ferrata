//! The core of `ferrata_attestor`, the signer that runs beside each LEZ sequencer under finality
//! rule (B) (SPEC-v3 D7 and D13; `docs/design/bridge.md` §8.3), without the Logos module packaging.
//!
//! Two signed statements, both Ed25519 over the whole tagged message (never over a bare digest):
//!
//! **Certificate.** A sequencer's channel key (Ed25519, the scheme of Logos Blockchain's channel
//! keys) signs, once per deployment, that an attestation key speaks for it:
//!
//! ```text
//! "ferrata/attest-key/v1"(21) ‖ evmChainId(32) ‖ vault(20) ‖ lezConfigId(32) ‖ channelId(32) ‖ attKey(32)   169 bytes
//! ```
//!
//! SPEC-v3 D13 writes `("ferrata/attest-key/v1", channelId, attKey)`; this binds the deployment
//! (chain, vault, configuration) as well, so a certificate cannot be carried to another vault or
//! configuration on the same channel. It carries no validity bound, as the specification has none:
//! a certificate dies when its channel key leaves the live key set, which the verifier checks
//! (the checkpoint program counts only channel keys of its live set). The channel key never signs
//! bridge data.
//!
//! **Final-tier attestation v2.** The attestation key signs its node's final tip:
//!
//! ```text
//! "ferrata/final-tier/v2"(21) ‖ evmChainId(32) ‖ vault(20) ‖ lezConfigId(32) ‖ channelId(32) ‖
//! keySetHash(32) ‖ tipId(u64, big-endian) ‖ tipHash(32) ‖ finalMsgId(32)                                    241 bytes
//! ```
//!
//! `keySetHash` is the channel's live key set as the attestor's node sees it, in SPEC-v3 D3's v2
//! encoding ([`key_set_hash`]); `finalMsgId` is the Logos Blockchain message id of the inscription
//! that carried the tip, signed for watchers and not checked by the checkpoint program.
//!
//! Verification is `verify_strict` (small-order keys and non-canonical signatures refused), as the
//! checkpoint program verifies channel keys' signatures. Integers in [`SignedAttestation::encode`]
//! (the checkpoint program's input) are little-endian like the rest of that input; in the signed
//! message `tipId` is big-endian, as in the checkpoint state `S`.
#![no_std]
#![forbid(unsafe_code)]

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use sha2::{Digest as _, Sha256};

/// A 32-byte hash, key, id or `uint256`/`bytes32` word.
pub type Hash = [u8; 32];
/// A 20-byte Ethereum address.
pub type Address = [u8; 20];

/// Domain of the certificate (SPEC-v3 D13).
pub const TAG_ATTEST_KEY: &[u8; 21] = b"ferrata/attest-key/v1";
/// Domain of the final-tier attestation, v2 (SPEC-v3 D13; v1 of D7 had no key-set hash).
pub const TAG_FINAL_TIER: &[u8; 21] = b"ferrata/final-tier/v2";
/// Domain of the key-set hash, v2 (SPEC-v3 D3: binds the posting timeframe).
pub const TAG_KEYSET: &[u8; 17] = b"ferrata/keyset/v2";

/// Tag ‖ evmChainId ‖ vault ‖ lezConfigId ‖ channelId: the prefix both messages share.
const PREFIX_LEN: usize = 21 + 32 + 20 + 32 + 32;
pub const CERTIFICATE_MESSAGE_LEN: usize = PREFIX_LEN + 32;
pub const ATTESTATION_MESSAGE_LEN: usize = PREFIX_LEN + 32 + 8 + 32 + 32;
/// channelKey ‖ attKey ‖ certificate ‖ tipId u64 LE ‖ tipHash ‖ finalMsgId ‖ signature.
pub const SIGNED_ATTESTATION_LEN: usize = 32 + 32 + 64 + 8 + 32 + 32 + 64;

/// The deployment an attestation key serves: the vault's chain and address, the LEZ
/// configuration it follows and that configuration's channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deployment {
    /// `uint256`, big-endian.
    pub evm_chain_id: Hash,
    pub vault: Address,
    pub lez_config_id: Hash,
    pub channel_id: Hash,
}

/// A final tip as the attestor reads it from its node's final tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinalTip {
    pub tip_id: u64,
    pub tip_hash: Hash,
    /// Message id of the inscription that carried the tip block.
    pub final_msg_id: Hash,
}

/// Which signature of a [`SignedAttestation`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// The channel key did not certify the attestation key for this deployment (or does not decode).
    Certificate,
    /// The attestation key did not sign this message (or does not decode, or is of small order).
    Attestation,
}

impl Deployment {
    fn prefix(&self, tag: &[u8; 21], out: &mut [u8]) {
        out[..21].copy_from_slice(tag);
        out[21..53].copy_from_slice(&self.evm_chain_id);
        out[53..73].copy_from_slice(&self.vault);
        out[73..105].copy_from_slice(&self.lez_config_id);
        out[105..PREFIX_LEN].copy_from_slice(&self.channel_id);
    }

    /// The certificate's signed bytes.
    pub fn certificate_message(&self, att_key: &Hash) -> [u8; CERTIFICATE_MESSAGE_LEN] {
        let mut m = [0u8; CERTIFICATE_MESSAGE_LEN];
        self.prefix(TAG_ATTEST_KEY, &mut m);
        m[PREFIX_LEN..].copy_from_slice(att_key);
        m
    }

    /// The final-tier attestation's signed bytes.
    pub fn attestation_message(&self, key_set_hash: &Hash, tip: &FinalTip) -> [u8; ATTESTATION_MESSAGE_LEN] {
        let mut m = [0u8; ATTESTATION_MESSAGE_LEN];
        self.prefix(TAG_FINAL_TIER, &mut m);
        m[PREFIX_LEN..PREFIX_LEN + 32].copy_from_slice(key_set_hash);
        m[PREFIX_LEN + 32..PREFIX_LEN + 40].copy_from_slice(&tip.tip_id.to_be_bytes());
        m[PREFIX_LEN + 40..PREFIX_LEN + 72].copy_from_slice(&tip.tip_hash);
        m[PREFIX_LEN + 72..].copy_from_slice(&tip.final_msg_id);
        m
    }
}

/// Ed25519 `verify_strict` of `signature` by `key` over `message`.
pub fn verify_strict(key: &Hash, message: &[u8], signature: &[u8; 64]) -> bool {
    VerifyingKey::from_bytes(key).is_ok_and(|k| k.verify_strict(message, &Signature::from_bytes(signature)).is_ok())
}

/// The channel key's certificate of `att_key` for `deployment`.
pub fn sign_certificate(channel_key: &SigningKey, deployment: &Deployment, att_key: &Hash) -> [u8; 64] {
    channel_key.sign(&deployment.certificate_message(att_key)).to_bytes()
}

pub fn verify_certificate(channel_key: &Hash, deployment: &Deployment, att_key: &Hash, signature: &[u8; 64]) -> bool {
    verify_strict(channel_key, &deployment.certificate_message(att_key), signature)
}

/// The attestation key's signature of `tip` under the live key set `key_set_hash`.
pub fn sign_attestation(
    att_key: &SigningKey,
    deployment: &Deployment,
    key_set_hash: &Hash,
    tip: &FinalTip,
) -> [u8; 64] {
    att_key.sign(&deployment.attestation_message(key_set_hash, tip)).to_bytes()
}

pub fn verify_attestation(
    att_key: &Hash,
    deployment: &Deployment,
    key_set_hash: &Hash,
    tip: &FinalTip,
    signature: &[u8; 64],
) -> bool {
    verify_strict(att_key, &deployment.attestation_message(key_set_hash, tip), signature)
}

/// `keySetHash = H("ferrata/keyset/v2" ‖ n:u32 ‖ postingTimeframe:u32 ‖ key_1 ‖ … ‖ key_n)`, big-endian
/// integers, keys in `accredited_keys` order (SPEC-v3 D3). The checkpoint state uses this function.
pub fn key_set_hash(keys: &[Hash], posting_timeframe: u32) -> Hash {
    let n = u32::try_from(keys.len()).unwrap_or(u32::MAX);
    let mut h = Sha256::new();
    h.update(TAG_KEYSET);
    h.update(n.to_be_bytes());
    h.update(posting_timeframe.to_be_bytes());
    for key in keys {
        h.update(key);
    }
    h.finalize().into()
}

/// What an attestor publishes and a prover collects: an attestation with the certificate of its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedAttestation {
    /// The sequencer's channel key, which must be in the live key set.
    pub channel_key: Hash,
    pub att_key: Hash,
    /// The channel key's signature of [`Deployment::certificate_message`]`(att_key)`.
    pub certificate: [u8; 64],
    pub tip: FinalTip,
    /// The attestation key's signature of [`Deployment::attestation_message`].
    pub signature: [u8; 64],
}

fn array<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes[at..at + N]);
    out
}

impl SignedAttestation {
    /// Signs `tip` with `att_key` and attaches the certificate `channel_key` gave that key.
    pub fn new(
        att_key: &SigningKey,
        channel_key: Hash,
        certificate: [u8; 64],
        deployment: &Deployment,
        key_set_hash: &Hash,
        tip: FinalTip,
    ) -> Self {
        let signature = sign_attestation(att_key, deployment, key_set_hash, &tip);
        Self { channel_key, att_key: att_key.verifying_key().to_bytes(), certificate, tip, signature }
    }

    /// Both signatures, certificate first. Whether `channel_key` belongs to the live key set, and
    /// whether the tip is the one a proof needs, is the verifier's to check.
    pub fn verify(&self, deployment: &Deployment, key_set_hash: &Hash) -> Result<(), VerifyError> {
        if !verify_certificate(&self.channel_key, deployment, &self.att_key, &self.certificate) {
            return Err(VerifyError::Certificate);
        }
        if !verify_attestation(&self.att_key, deployment, key_set_hash, &self.tip, &self.signature) {
            return Err(VerifyError::Attestation);
        }
        Ok(())
    }

    pub fn encode(&self) -> [u8; SIGNED_ATTESTATION_LEN] {
        let mut out = [0u8; SIGNED_ATTESTATION_LEN];
        out[..32].copy_from_slice(&self.channel_key);
        out[32..64].copy_from_slice(&self.att_key);
        out[64..128].copy_from_slice(&self.certificate);
        out[128..136].copy_from_slice(&self.tip.tip_id.to_le_bytes());
        out[136..168].copy_from_slice(&self.tip.tip_hash);
        out[168..200].copy_from_slice(&self.tip.final_msg_id);
        out[200..].copy_from_slice(&self.signature);
        out
    }

    pub fn decode(bytes: &[u8; SIGNED_ATTESTATION_LEN]) -> Self {
        Self {
            channel_key: array(bytes, 0),
            att_key: array(bytes, 32),
            certificate: array(bytes, 64),
            tip: FinalTip {
                tip_id: u64::from_le_bytes(array(bytes, 128)),
                tip_hash: array(bytes, 136),
                final_msg_id: array(bytes, 168),
            },
            signature: array(bytes, 200),
        }
    }
}
