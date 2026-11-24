//! LEZ v0.4.0-rc3's public side for the network session (`net.rs`), ported from the web wallet's
//! `deploy/testnet/web/wallet/lez.js`, which is checked against LEZ's own pinned vectors and against transactions LEZ's
//! wallet CLI signed (`test/conformance.test.mjs`): the recovery phrase (BIP-39, LEZ's wallet's `to_seed("")`), the
//! public key tree (`/LEE-Keys/v1/Master/Public`, the tweak), account ids, the private account ids (key_protocol's
//! private tree and ML-KEM-768, as `cli/src/seed.rs` derives them with LEZ's crates), the public transaction's borsh,
//! its signing hash, BIP-340 signatures and the wire form `sendTransaction` takes.
//!
//! Secrets (the seed, the signing keys) are zeroized when dropped, have no `Debug` and never leave this module.

use hmac::{Hmac, Mac};
use k256::elliptic_curve::{ff::PrimeField, sec1::ToEncodedPoint};
use sha2::{Digest, Sha256, Sha512};
use zeroize::Zeroizing;

type HmacSha512 = Hmac<Sha512>;
type HmacSha256 = Hmac<Sha256>;

pub(crate) type Id = [u8; 32];

fn hmac512(key: &[u8], msg: &[u8]) -> Zeroizing<[u8; 64]> {
    let mut m = HmacSha512::new_from_slice(key).expect("HMAC takes any key length");
    m.update(msg);
    let mut out = Zeroizing::new([0u8; 64]);
    out.copy_from_slice(&m.finalize().into_bytes());
    out
}

pub(crate) fn hmac256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    m.update(msg);
    m.finalize().into_bytes().into()
}

pub(crate) fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// A label padded with zeros to 32 bytes (LEZ's domain prefixes).
fn pad32(s: &str) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[..s.len()].copy_from_slice(s.as_bytes());
    b
}

const MESSAGE_PUBLIC_PREFIX: &str = "/LEE/v0.3/Message/Public/";
const ACCOUNT_ID_PUBLIC_PREFIX: &str = "/LEE/v0.3/AccountId/Public/";
const ACCOUNT_ID_PRIVATE_PREFIX: &str = "/LEE/v0.3/AccountId/Private/";

// ---------------------------------------------------------------------------------------------- the phrase

/// The seed of a recovery phrase as LEZ's wallet makes it: BIP-39 English, any valid length, `to_seed("")`. The words
/// are normalized as the web wallet does (trimmed, lowercased, single spaces). The error never names a word.
#[cfg(test)]
thread_local! {
    /// Seeds derived on this thread (net.rs tests: a refused open derives none).
    pub(crate) static SEEDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn seed_of_phrase(phrase: &str) -> Result<Zeroizing<[u8; 64]>, String> {
    #[cfg(test)]
    SEEDS.with(|c| c.set(c.get() + 1));
    const BAD: &str = "not a valid BIP-39 English recovery phrase (a word, the word count or the checksum is wrong)";
    // The English word list is ASCII: anything else is refused before a copy is made. The words are normalized into
    // one buffer sized once, so that no reallocation leaves a part of the phrase behind; it and the parsed mnemonic
    // (bip39's `zeroize`) are wiped when dropped.
    if !phrase.is_ascii() {
        return Err(BAD.into());
    }
    let mut words = Zeroizing::new(String::with_capacity(phrase.len()));
    for w in phrase.split_ascii_whitespace() {
        if !words.is_empty() {
            words.push(' ');
        }
        for c in w.chars() {
            words.push(c.to_ascii_lowercase());
        }
    }
    let m = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &words).map_err(|_| BAD.to_string())?;
    Ok(Zeroizing::new(m.to_seed_normalized("")))
}

/// A new 24-word phrase from 256 bits of the operating system's randomness (as LEZ's wallet makes one), built in one
/// buffer sized once (24 words of at most 8 letters, and their spaces) and wiped when dropped.
pub(crate) fn new_phrase() -> Result<Zeroizing<String>, String> {
    let mut entropy = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(entropy.as_mut()).map_err(|e| format!("no randomness from the operating system: {e}"))?;
    let m = bip39::Mnemonic::from_entropy_in(bip39::Language::English, entropy.as_ref()).map_err(|e| e.to_string())?;
    let mut out = Zeroizing::new(String::with_capacity(24 * 9));
    for w in m.words() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    debug_assert!(out.len() <= 24 * 9);
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- public keys

fn scalar(b: &[u8; 32]) -> Option<k256::Scalar> {
    Option::from(k256::Scalar::from_repr((*b).into()))
}

fn scalar_bytes(s: &k256::Scalar) -> [u8; 32] {
    s.to_repr().into()
}

/// A non-zero canonical scalar (a valid secp256k1 secret key).
fn check_secret(b: &[u8; 32]) -> Result<k256::Scalar, String> {
    match scalar(b) {
        Some(s) if !bool::from(s.is_zero()) => Ok(s),
        _ => Err("an invalid secp256k1 key in LEZ's key tree".into()),
    }
}

/// LEZ's `PrivateKey::tweak`: `sk + SHA-256(compressed public key of sk) mod n`.
pub(crate) fn tweak(sk: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, String> {
    let s = check_secret(sk)?;
    let secret = k256::SecretKey::from_bytes(&(*sk).into()).map_err(|_| "an invalid secp256k1 key".to_string())?;
    let compressed = secret.public_key().to_encoded_point(true);
    let h = sha256(&[compressed.as_bytes()]);
    let t = scalar(&h).ok_or("the tweak is not a canonical scalar")?;
    let out = s + t;
    if bool::from(out.is_zero()) {
        return Err("the tweaked key is zero".into());
    }
    Ok(Zeroizing::new(scalar_bytes(&out)))
}

/// The x-only public key of a signing key (LEZ's `PublicKey::new_from_private_key`).
pub(crate) fn public_key_of(ssk: &[u8; 32]) -> Result<[u8; 32], String> {
    let k = k256::schnorr::SigningKey::from_bytes(ssk).map_err(|_| "an invalid signing key".to_string())?;
    Ok(k.verifying_key().to_bytes().into())
}

/// LEZ's `AccountId::from(&PublicKey)`.
pub(crate) fn account_id_of(pk: &[u8; 32]) -> Id {
    sha256(&[&pad32(ACCOUNT_ID_PUBLIC_PREFIX), pk])
}

/// One public account of the phrase: `/index`, its signing key (tweaked), public key and account id.
pub(crate) struct PublicAccount {
    pub index: u32,
    ssk: Zeroizing<[u8; 32]>,
    pub pk: [u8; 32],
    pub id: Id,
}

impl PublicAccount {
    /// BIP-340 over `hash32` with the operating system's randomness as `aux`.
    pub(crate) fn sign(&self, hash32: &[u8; 32]) -> Result<[u8; 64], String> {
        let mut aux = [0u8; 32];
        getrandom::getrandom(&mut aux).map_err(|e| format!("no randomness from the operating system: {e}"))?;
        sign(&self.ssk, hash32, &aux)
    }
}

/// Public accounts `/0` … `/(n-1)` of a seed, as LEZ's wallet creates them (`ChildKeysPublic::nth_child` of the root).
pub(crate) fn public_accounts(seed: &[u8; 64], n: u32) -> Result<Vec<PublicAccount>, String> {
    let h = hmac512(b"/LEE-Keys/v1/Master/Public", seed);
    let mut sk = Zeroizing::new([0u8; 32]);
    sk.copy_from_slice(&h[..32]);
    let master = check_secret(&sk)?;
    let cc = &h[32..];
    let mut out = Vec::new();
    for index in 0..n {
        let mut data = Zeroizing::new([0u8; 37]);
        data[1..33].copy_from_slice(sk.as_ref());
        data[33..].copy_from_slice(&index.to_be_bytes());
        let c = hmac512(cc, data.as_ref());
        let mut left = Zeroizing::new([0u8; 32]);
        left.copy_from_slice(&c[..32]);
        let child = scalar(&left).ok_or("a child key is not a canonical scalar")? + master;
        if bool::from(child.is_zero()) {
            return Err("a child key is zero".into());
        }
        let child_sk = Zeroizing::new(scalar_bytes(&child));
        let ssk = tweak(&child_sk)?;
        let pk = public_key_of(&ssk)?;
        out.push(PublicAccount { index, pk, id: account_id_of(&pk), ssk });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- private account ids

struct PrivateNode {
    ccc: Zeroizing<[u8; 32]>,
    nsk: Zeroizing<[u8; 32]>,
    d: Zeroizing<[u8; 32]>,
    z: Zeroizing<[u8; 32]>,
    id: Id,
}

fn private_node(ssk: &[u8], ccc: &[u8], cci: Option<u32>) -> PrivateNode {
    let idx = cci.unwrap_or(0).to_be_bytes();
    let ask = Zeroizing::new(sha256(&[b"/LEE-Keys/v1/Authorization/Secret", ssk, &idx]));
    let msg = Zeroizing::new([b"/LEE-Keys/v1/Viewing/Secret".as_slice(), ssk, &idx].concat());
    let vseed = hmac512(b"/LEE-Keys/v1/Viewing/Seed", &msg);
    let nsk = Zeroizing::new(sha256(&[b"/LEE-Keys/v1/Nullifier/Secret", ask.as_ref()]));
    let npk = sha256(&[b"/LEE-Keys/v1/Nullifier/Public", nsk.as_ref()]);
    // The viewing key: ML-KEM-768 from d ‖ z (FIPS 203), as key_protocol's `ViewingPublicKey::from`.
    let vpk = {
        use ml_kem::{Kem, KeyExport as _, MlKem768};
        let mut seed = Zeroizing::new([0u8; 64]);
        seed.copy_from_slice(vseed.as_ref());
        let dk = <MlKem768 as Kem>::DecapsulationKey::from_seed(ml_kem::Seed::from(*seed));
        dk.encapsulation_key().to_bytes().to_vec()
    };
    let mut d = Zeroizing::new([0u8; 32]);
    let mut z = Zeroizing::new([0u8; 32]);
    d.copy_from_slice(&vseed[..32]);
    z.copy_from_slice(&vseed[32..]);
    let mut c = Zeroizing::new([0u8; 32]);
    c.copy_from_slice(ccc);
    PrivateNode { ccc: c, nsk, d, z, id: sha256(&[&pad32(ACCOUNT_ID_PRIVATE_PREFIX), &npk, &vpk]) }
}

/// Private account ids `/0` … `/(n-1)` of a seed, as LEZ's wallet creates them (`ChildKeysPrivate::nth_child` of the
/// root). Only the ids leave this function: spending a private account needs a proof this module does not make.
pub(crate) fn private_account_ids(seed: &[u8; 64], n: u32) -> Vec<Id> {
    let h = hmac512(b"/LEE-Keys/v1/Master/Private", seed);
    let root = private_node(&h[..32], &h[32..], None);
    (0..n)
        .map(|i| {
            let pt = Zeroizing::new(sha256(&[b"/LEE-Keys/v1/Parent/Private", root.nsk.as_ref(), root.d.as_ref(), root.z.as_ref()]));
            let msg = Zeroizing::new([b"/LEE-Keys/v1/Seed/Private".as_slice(), pt.as_ref(), &i.to_be_bytes()].concat());
            let c = hmac512(root.ccc.as_ref(), &msg);
            private_node(&c[..32], &c[32..], Some(i)).id
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------- the public transaction

/// An account as a program sees it: (account id, program id).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Actor {
    pub account: Id,
    pub program: Id,
}

/// LEZ's admission evidence of a public account the message names.
#[derive(Clone)]
pub(crate) enum Evidence {
    /// A never-used account named with its public key (the web wallet's transfers); the bridge's calls name PDAs only.
    #[allow(dead_code)]
    Key([u8; 32]),
    Pda { program: Id, seed: [u8; 32] },
}

pub(crate) struct Fee {
    pub payer: Id,
    pub gas_limit: u64,
    pub tip: u64,
    pub max_fee: u128,
}

/// `TransactionMessage<PublicExecution>` (lee/state_machine, LEZ rc3).
pub(crate) struct Message {
    pub actors: Vec<Actor>,
    pub signers: Vec<Id>,
    pub to: Actor,
    pub data: Vec<u8>,
    pub fee: Option<Fee>,
    pub nonces: Vec<(Id, u128)>,
    pub evidence: Vec<Evidence>,
}

fn u32le(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&(v as u32).to_le_bytes());
}

/// The message's borsh, LEZ's layout: actors and signers sorted and deduplicated, no promotions, the root call, the
/// fee, the nonces sorted by account, the evidence in order.
pub(crate) fn encode_message(m: &Message) -> Vec<u8> {
    let mut out = Vec::new();
    let mut actors = m.actors.clone();
    actors.sort();
    actors.dedup();
    u32le(&mut out, actors.len());
    for a in &actors {
        out.extend_from_slice(&a.account);
        out.extend_from_slice(&a.program);
    }
    let mut signers = m.signers.clone();
    signers.sort();
    signers.dedup();
    u32le(&mut out, signers.len());
    for s in &signers {
        out.extend_from_slice(s);
    }
    u32le(&mut out, 0);
    out.extend_from_slice(&m.to.account);
    out.extend_from_slice(&m.to.program);
    u32le(&mut out, m.data.len());
    out.extend_from_slice(&m.data);
    match &m.fee {
        Some(f) => {
            out.push(1);
            out.extend_from_slice(&f.payer);
            out.extend_from_slice(&f.gas_limit.to_le_bytes());
            out.extend_from_slice(&f.tip.to_le_bytes());
            out.extend_from_slice(&f.max_fee.to_le_bytes());
        }
        None => out.push(0),
    }
    let mut nonces = m.nonces.clone();
    nonces.sort_by(|a, b| a.0.cmp(&b.0));
    u32le(&mut out, nonces.len());
    for (id, n) in &nonces {
        out.extend_from_slice(id);
        out.extend_from_slice(&n.to_le_bytes());
    }
    u32le(&mut out, m.evidence.len());
    for e in &m.evidence {
        match e {
            Evidence::Key(k) => {
                out.push(0);
                out.extend_from_slice(k);
            }
            Evidence::Pda { program, seed } => {
                out.push(1);
                out.extend_from_slice(program);
                out.extend_from_slice(seed);
            }
        }
    }
    out
}

pub(crate) fn message_hash(message: &[u8]) -> [u8; 32] {
    sha256(&[&pad32(MESSAGE_PUBLIC_PREFIX), message])
}

/// BIP-340 (LEZ's `Signature::new_with_aux_random`).
pub(crate) fn sign(ssk: &[u8; 32], hash32: &[u8; 32], aux: &[u8; 32]) -> Result<[u8; 64], String> {
    let k = k256::schnorr::SigningKey::from_bytes(ssk).map_err(|_| "an invalid signing key".to_string())?;
    let sig = k.sign_raw(hash32, aux).map_err(|e| format!("signing: {e}"))?;
    Ok(sig.to_bytes())
}

/// `PublicTransaction`'s borsh: the message, then the witness set's (signature, public key) pairs.
pub(crate) fn encode_transaction(message: &[u8], witnesses: &[([u8; 64], [u8; 32])]) -> Vec<u8> {
    let mut out = message.to_vec();
    u32le(&mut out, witnesses.len());
    for (sig, pk) in witnesses {
        out.extend_from_slice(sig);
        out.extend_from_slice(pk);
    }
    out
}

/// `LeeTransaction::Public(tx)` as `sendTransaction` takes it (base64 of borsh), and the hash LEZ names it by.
pub(crate) fn wire(tx: &[u8]) -> (String, String) {
    use base64::Engine as _;
    let mut b = Vec::with_capacity(tx.len() + 1);
    b.push(0);
    b.extend_from_slice(tx);
    (base64::engine::general_purpose::STANDARD.encode(&b), crate::hex(&sha256(&[tx]))[2..].to_string())
}

/// The fee fields of a public transaction of `size` bytes (the day-one kit's rule the web wallet uses): what the
/// sequencer reserves at the current base fees, and a `max_fee` with headroom (the larger of 64 and 8 x the base).
pub(crate) fn fee_for(gas_limit: u64, size: usize, base_exec: u128, base_stor: u128) -> (u128, u128) {
    let at = |e: u128, s: u128| gas_limit as u128 * e + size as u128 * s;
    let reserve = at(base_exec, base_stor);
    let max_fee = at((base_exec * 8).max(64), (base_stor * 8).max(64));
    (reserve, max_fee)
}

pub(crate) fn id_string(id: &Id) -> String {
    bs58::encode(id).into_string()
}

pub(crate) fn id_from_string(s: &str) -> Result<Id, String> {
    let t = s.trim().trim_start_matches("Public/");
    let v = bs58::decode(t).into_vec().map_err(|_| format!("{t:?} is not a base58 LEZ account id"))?;
    v.try_into().map_err(|_| format!("{t:?} is not a 32-byte LEZ account id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h32(s: &str) -> [u8; 32] {
        crate::unhex(s).unwrap().try_into().unwrap()
    }

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // LEZ's own vectors (lee/key_protocol keys_public.rs, signature/private_key.rs), as lez.js's conformance test pins them.
    #[test]
    fn lez_master_and_tweak_vectors() {
        let seed: [u8; 64] = [
            88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173, 134, 61, 109, 34, 103, 121, 39,
            237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87, 22, 227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61,
            203, 246, 121, 214, 6, 187, 148, 92, 44, 253, 210, 37,
        ];
        let h = hmac512(b"/LEE-Keys/v1/Master/Public", &seed);
        assert_eq!(&h[..32], &[142, 140, 44, 81, 255, 159, 131, 163, 210, 67, 198, 176, 43, 243, 163, 35, 242, 200, 232, 99, 69,
                               240, 63, 16, 33, 104, 8, 152, 243, 153, 180, 169]);
        let ssk = tweak(&h[..32].try_into().unwrap()).unwrap();
        assert_eq!(*ssk, [241, 47, 167, 208, 182, 77, 106, 158, 182, 41, 17, 3, 91, 229, 165, 35, 90, 33, 145, 202, 246, 65, 127,
                          65, 124, 240, 165, 152, 127, 50, 60, 198]);
        assert_eq!(public_key_of(&ssk).unwrap(), [43, 138, 92, 79, 223, 49, 90, 162, 205, 76, 143, 151, 96, 77, 10, 85, 179, 208,
                                                  244, 71, 251, 191, 237, 226, 120, 247, 194, 57, 117, 180, 96, 65]);
        assert_eq!(*tweak(&[1u8; 32]).unwrap(), [242, 210, 33, 19, 65, 108, 136, 176, 179, 128, 110, 210, 107, 193, 168, 112, 206,
                                                 171, 86, 238, 131, 10, 39, 36, 44, 39, 246, 20, 46, 193, 204, 66]);
        assert!(tweak(&[0u8; 32]).is_err());
        assert!(tweak(&[0xff; 32]).is_err());
    }

    // BIP-340's own vectors (the ones LEZ tests Signature::new_with_aux_random with).
    #[test]
    fn bip340_vectors() {
        let v = [
            ("0000000000000000000000000000000000000000000000000000000000000003", "F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9",
             "0000000000000000000000000000000000000000000000000000000000000000", "0000000000000000000000000000000000000000000000000000000000000000",
             "E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0"),
            ("C90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B14E5C9", "DD308AFEC5777E13121FA72B9CC1B7CC0139715309B086C960E18FD969774EB8",
             "C87AA53824B4D7AE2EB035A2B5BBBCCC080E76CDC6D1692C4B0B62D798E6D906", "7E2D58D8B3BCDF1ABADEC7829054F90DDA9805AAB56C77333024B9D0A508B75C",
             "5831AAEED7B44BB74E5EAB94BA9D4294C49BCF2A60728D8B4C200F50DD313C1BAB745879A5AD954A72C45A91C3A51D3C7ADEA98D82F8481E0E1E03674A6F3FB7"),
        ];
        for (sk, pk, aux, msg, sig) in v {
            let s = sign(&h32(sk), &h32(msg), &h32(aux)).unwrap();
            assert_eq!(crate::hex(&s)[2..].to_uppercase(), sig);
            assert_eq!(crate::hex(&public_key_of(&h32(sk)).unwrap())[2..].to_uppercase(), pk);
        }
    }

    // The test phrase's accounts as lez.js derives them (spike's vectors run; the same ids are in cli/'s runs, made with
    // LEZ's key_protocol), and its bridge seed as cli/src/seed.rs derives it.
    #[test]
    fn the_test_phrase_accounts() {
        let seed = seed_of_phrase(&format!("  {}\n", PHRASE.to_uppercase())).unwrap();
        assert_eq!(crate::hex(seed.as_ref()), "0x5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4");
        assert_eq!(crate::hex(&hmac256(seed.as_ref(), crate::BRIDGE_SEED_LABEL)), "0x43bff369e1a90a093a1955430058273ad3a5cfd943c8e6684bc33fe245dec340");
        let p = public_accounts(&seed, 8).unwrap();
        assert_eq!(id_string(&p[0].id), "AKRYNTU8c4MZ4fSr5FHcn6BGE6T4dfRCzn1ziQCNKFBu");
        assert_eq!(crate::hex(&p[0].pk), "0xc8730d3953a94b608d67710f3c7cbb2f0694665ae60af5c7584253f49346b231");
        assert_eq!(id_string(&p[1].id), "DzN4xv4Z3MZhEACzb8nENnocedCNBKEEiUekVCEMNJqH");
        assert_eq!(id_string(&p[7].id), "AZAYsXu7brfbyupSZG2n1VbtzRoZgAVAcBdKfZQjMahv");
        let q = private_account_ids(&seed, 8);
        assert_eq!(crate::hex(&q[0]), "0xd0029aa57943164511b1565faea8a7b0f0df439cc613c9ee5fc64366ac66fe85");
        assert_eq!(crate::hex(&q[1]), "0x57f5ae1946491d89030fd11f7bba203811fff0018b8b081b4473ca81a99552f2");
        assert_eq!(crate::hex(&q[7]), "0xa22d9876d2808ddaa5cb47cefd90004e7ff7efff1c2c65d2de978de76cde7bf3");
        assert!(seed_of_phrase("abandon abandon").is_err());
        assert!(seed_of_phrase(&PHRASE.replace("about", "abandon")).is_err());
        let w = new_phrase().unwrap();
        assert_eq!(w.split(' ').count(), 24);
        assert!(seed_of_phrase(&w).is_ok());
    }

    // A signed call, byte for byte as lez.js's buildCall makes it (the vectors run: payer /0 of the test phrase).
    #[test]
    fn a_public_call_as_lez_js_signs_it() {
        let seed = seed_of_phrase(PHRASE).unwrap();
        let payer = &public_accounts(&seed, 1).unwrap()[0];
        let prog = [7u8; 32];
        let own = Actor { account: payer.id, program: prog };
        let m = Message {
            actors: vec![own, Actor { account: [9; 32], program: prog }, Actor { account: [3; 32], program: prog }],
            signers: vec![payer.id],
            to: own,
            data: vec![5, 1, 2, 3],
            fee: Some(Fee { payer: payer.id, gas_limit: 1_000_000, tip: 0, max_fee: 123_456_789 }),
            nonces: vec![(payer.id, 3)],
            evidence: vec![Evidence::Pda { program: prog, seed: [1; 32] }, Evidence::Pda { program: prog, seed: [2; 32] }],
        };
        let bytes = encode_message(&m);
        let h = message_hash(&bytes);
        assert_eq!(crate::hex(&h), "0x8416b404b79bbc797517412bfcc37638f16c191c0567d75b99694ed8baace9a6");
        let sig = sign(&payer.ssk, &h, &[0u8; 32]).unwrap();
        let tx = encode_transaction(&bytes, &[(sig, payer.pk)]);
        let (w, hash) = wire(&tx);
        assert_eq!(hash, "963d16d6f619ede4daf18149c3f702dcf4778dfd6922f1cfbeb4bde6ecdb0392");
        assert!(w.starts_with("AAMAAAADAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwcHBwcH"));
        assert!(w.ends_with("hzDTlTqUtgjWdxDzx8uy8GlGZa5gr1x1hCU/STRrIx"));
        assert_eq!(fee_for(1_000_000, 500, 8, 8), (8_004_000, 64_032_000));
        assert_eq!(id_from_string("Public/AKRYNTU8c4MZ4fSr5FHcn6BGE6T4dfRCzn1ziQCNKFBu").unwrap(), payer.id);
    }
}
