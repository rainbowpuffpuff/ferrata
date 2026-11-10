//! The attestor core: message layout, signing and verification, every field bound, and the golden
//! vectors in `vectors/attestor-v1.json` (checked independently by
//! `checkpoint/scripts/check-attestor-vectors.py`, a pure-Python RFC 8032 implementation).
//!
//! `REGEN_ATTESTOR_VECTORS=1 cargo test -p attestor-core` rewrites the vectors file.

use attestor_core::*;
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{json, Value};

fn h(x: &[u8]) -> String {
    hex::encode(x)
}

fn uint256(x: u64) -> Hash {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&x.to_be_bytes());
    w
}

fn deployment() -> Deployment {
    Deployment {
        evm_chain_id: uint256(11_155_111),
        vault: [0x5a; 20],
        lez_config_id: [0xc0; 32],
        channel_id: [0xc4; 32],
    }
}

fn tip() -> FinalTip {
    FinalTip { tip_id: 361, tip_hash: [0x7a; 32], final_msg_id: [0x5e; 32] }
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn signed(channel: &SigningKey, att: &SigningKey, d: &Deployment, ksh: &Hash, tip: FinalTip) -> SignedAttestation {
    let att_pub = att.verifying_key().to_bytes();
    let certificate = sign_certificate(channel, d, &att_pub);
    SignedAttestation::new(att, channel.verifying_key().to_bytes(), certificate, d, ksh, tip)
}

/// One vector: a key set of `channel_seeds`, the first of which certifies `att_seed`, which
/// attests `tip`.
fn vector(d: &Deployment, channel_seeds: &[u8], att_seed: u8, timeframe: u32, tip: FinalTip) -> Value {
    let channels: Vec<SigningKey> = channel_seeds.iter().map(|s| key(*s)).collect();
    let keys: Vec<Hash> = channels.iter().map(|k| k.verifying_key().to_bytes()).collect();
    let ksh = key_set_hash(&keys, timeframe);
    let att = key(att_seed);
    let a = signed(&channels[0], &att, d, &ksh, tip);
    assert_eq!(a.verify(d, &ksh), Ok(()));
    json!({
        "deployment": {"evm_chain_id": h(&d.evm_chain_id), "vault": h(&d.vault),
                       "lez_config_id": h(&d.lez_config_id), "channel_id": h(&d.channel_id)},
        "channel_key_seed": h(&[channel_seeds[0]; 32]),
        "channel_key": h(&keys[0]),
        "att_key_seed": h(&[att_seed; 32]),
        "att_key": h(&a.att_key),
        "key_set": keys.iter().map(|k| h(k)).collect::<Vec<_>>(),
        "posting_timeframe": timeframe,
        "key_set_hash": h(&ksh),
        "certificate_message": h(&d.certificate_message(&a.att_key)),
        "certificate": h(&a.certificate),
        "tip": {"tip_id": tip.tip_id, "tip_hash": h(&tip.tip_hash), "final_msg_id": h(&tip.final_msg_id)},
        "attestation_message": h(&d.attestation_message(&ksh, &tip)),
        "signature": h(&a.signature),
        "signed_attestation": h(&a.encode()),
    })
}

#[test]
fn vectors() {
    let d2 =
        Deployment { evm_chain_id: uint256(1), vault: [0x11; 20], lez_config_id: [0x22; 32], channel_id: [0x33; 32] };
    let tip2 = FinalTip { tip_id: 0x0102_0304_0506_0708, tip_hash: [0x44; 32], final_msg_id: [0x55; 32] };
    let all = json!({
        "description": "ferrata_attestor core: certificate (ferrata/attest-key/v1, binding the deployment) and final-tier attestation v2 (ferrata/final-tier/v2), Ed25519 (RFC 8032) over the whole tagged message; keys from 32-byte seeds; key_set_hash is SPEC-v3 D3's v2 encoding; signed_attestation is the checkpoint program's 264-byte input entry.",
        "vectors": [
            vector(&deployment(), &[1, 2, 3, 4], 0xa1, 30, tip()),
            vector(&d2, &[9], 0xb2, 90, tip2),
        ],
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/attestor-v1.json");
    if std::env::var_os("REGEN_ATTESTOR_VECTORS").is_some() {
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/vectors")).unwrap();
        std::fs::write(path, serde_json::to_string_pretty(&all).unwrap() + "\n").unwrap();
    }
    let recorded: Value = serde_json::from_slice(&std::fs::read(path).expect("vectors file")).unwrap();
    assert_eq!(recorded, all);
}

#[test]
fn message_layout() {
    let d = deployment();
    let att = [0xab; 32];
    let c = d.certificate_message(&att);
    assert_eq!(c.len(), 169);
    assert_eq!(&c[..21], b"ferrata/attest-key/v1");
    assert_eq!(&c[21..53], &d.evm_chain_id);
    assert_eq!(&c[53..73], &d.vault);
    assert_eq!(&c[73..105], &d.lez_config_id);
    assert_eq!(&c[105..137], &d.channel_id);
    assert_eq!(&c[137..], &att);

    let ksh = [0x99; 32];
    let m = d.attestation_message(&ksh, &tip());
    assert_eq!(m.len(), 241);
    assert_eq!(&m[..21], b"ferrata/final-tier/v2");
    assert_eq!(&m[21..137], &c[21..137]);
    assert_eq!(&m[137..169], &ksh);
    assert_eq!(&m[169..177], &361u64.to_be_bytes());
    assert_eq!(&m[177..209], &tip().tip_hash);
    assert_eq!(&m[209..], &tip().final_msg_id);
}

#[test]
fn every_field_is_bound() {
    let d = deployment();
    let ksh = [0x99; 32];
    let a = signed(&key(1), &key(0xa1), &d, &ksh, tip());
    assert_eq!(a.verify(&d, &ksh), Ok(()));

    // The deployment: a certificate (checked first) for another chain, vault, configuration or
    // channel does not hold.
    let mut other = d;
    other.evm_chain_id[31] ^= 1;
    assert_eq!(a.verify(&other, &ksh), Err(VerifyError::Certificate));
    let mut other = d;
    other.vault[0] ^= 1;
    assert_eq!(a.verify(&other, &ksh), Err(VerifyError::Certificate));
    let mut other = d;
    other.lez_config_id[0] ^= 1;
    assert_eq!(a.verify(&other, &ksh), Err(VerifyError::Certificate));
    let mut other = d;
    other.channel_id[0] ^= 1;
    assert_eq!(a.verify(&other, &ksh), Err(VerifyError::Certificate));

    // The certificate binds both keys.
    let mut b = a;
    b.channel_key = key(2).verifying_key().to_bytes();
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Certificate));
    let mut b = a;
    b.att_key = key(0xa2).verifying_key().to_bytes();
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Certificate));
    let mut b = a;
    b.certificate[5] ^= 1;
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Certificate));

    // The attestation binds the key set and the tip.
    let mut wrong = ksh;
    wrong[0] ^= 1;
    assert_eq!(a.verify(&d, &wrong), Err(VerifyError::Attestation));
    let mut b = a;
    b.tip.tip_id += 1;
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Attestation));
    let mut b = a;
    b.tip.tip_hash[3] ^= 1;
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Attestation));
    let mut b = a;
    b.tip.final_msg_id[3] ^= 1;
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Attestation));
    let mut b = a;
    b.signature[7] ^= 1;
    assert_eq!(b.verify(&d, &ksh), Err(VerifyError::Attestation));
}

#[test]
fn certificate_is_not_an_attestation_and_v1_is_not_v2() {
    // Domain separation: the channel key's certificate cannot be read as an attestation, and a
    // signature over another domain (D7's v1 tag) does not verify as v2.
    let d = deployment();
    let ksh = [0x99; 32];
    let att = key(0xa1);
    let mut m = d.attestation_message(&ksh, &tip());
    m[..21].copy_from_slice(b"ferrata/final-tier/v1");
    let sig = att.sign(&m).to_bytes();
    assert!(!verify_attestation(&att.verifying_key().to_bytes(), &d, &ksh, &tip(), &sig));
    let cert = sign_certificate(&key(1), &d, &att.verifying_key().to_bytes());
    assert!(!verify_attestation(&key(1).verifying_key().to_bytes(), &d, &ksh, &tip(), &cert));
}

#[test]
fn small_order_attestation_key_is_refused() {
    // The identity point (an encoding of small order): verify_strict refuses it whatever the
    // signature, even with a valid certificate.
    let d = deployment();
    let ksh = [0x99; 32];
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let certificate = sign_certificate(&key(1), &d, &identity);
    let a = SignedAttestation {
        channel_key: key(1).verifying_key().to_bytes(),
        att_key: identity,
        certificate,
        tip: tip(),
        signature: [0u8; 64],
    };
    assert_eq!(a.verify(&d, &ksh), Err(VerifyError::Attestation));
}

#[test]
fn encoding_round_trips() {
    let d = deployment();
    let a = signed(&key(1), &key(0xa1), &d, &[0x99; 32], FinalTip { tip_id: u64::MAX - 5, ..tip() });
    let bytes = a.encode();
    assert_eq!(bytes.len(), SIGNED_ATTESTATION_LEN);
    assert_eq!(SignedAttestation::decode(&bytes), a);
    assert_eq!(&bytes[128..136], &(u64::MAX - 5).to_le_bytes());
}
