//! The network session's sealed channel with the mini-app (`ferrata_bridge_ui`, its `qml/js/seal.js`).
//!
//! Logos Core's IPC carries a call's arguments in clear, and Qt's remote-objects debug logging
//! (`QT_LOGGING_RULES="qt.remoteobjects.debug=true"`, or `*.debug=true`) writes them to Basecamp's log file. So the
//! recovery phrase never crosses it in clear, and the session cannot be driven by whoever reads that log:
//!
//! 1. `seal_key`: the module makes a one-time X25519 key (held here at most ten minutes, used once) and gives the view
//!    its public half.
//! 2. The view makes its own key, hashed from the phrase (when there is one) and Qt's `Math.random`, and agrees with
//!    the module's: `shared = X25519(view secret, module key)`; both sides derive
//!    `prk = HMAC-SHA256("ferrata/basecamp-seal/v1", shared ‖ view key ‖ module key)` and from it the phrase's, the new
//!    words' and the calls' keys (`HMAC(prk, "phrase-enc")`, …).
//! 3. `open_network` carries the view's public key and the phrase sealed under them (encrypted with the HMAC-SHA256
//!    keystream `HMAC(k, be32(0)) ‖ HMAC(k, be32(1)) ‖ …`, then authenticated with `HMAC(k', ciphertext)`); a new
//!    phrase's words come back sealed the same way to the view.
//! 4. Every later call of the session carries `HMAC(call key, "ferrata/call/v1" ‖ be64(n) ‖ the call's JSON text)`,
//!    `n` never used before: only the view that opened the session can call it (session.rs).
//!
//! What a log of the IPC then holds: public keys, ciphertexts, MACs, and the calls' answers (account ids, positions,
//! balances), which are not secrets but are private. The module's one-time secrets and the derived keys are zeroized when
//! dropped (`x25519-dalek`'s `StaticSecret`, `zeroize`).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

/// What the view and the module agree on (the `scheme` of `seal_key`'s answer).
pub(crate) const SCHEME: &str = "x25519-hmac-sha256/v1";
const LABEL: &[u8] = b"ferrata/basecamp-seal/v1";
const CALL_LABEL: &[u8] = b"ferrata/call/v1";
/// How long a one-time key waits for its `open_network`, and how many wait at once.
const KEY_TTL: Duration = Duration::from_secs(600);
const KEYS_HELD: usize = 4;

type HmacSha256 = Hmac<Sha256>;

fn mac(key: &[u8], parts: &[&[u8]]) -> Zeroizing<[u8; 32]> {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    for p in parts {
        m.update(p);
    }
    Zeroizing::new(m.finalize().into_bytes().into())
}

/// The MAC compared in constant time.
fn verifies(key: &[u8], parts: &[&[u8]], tag: &[u8]) -> bool {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    for p in parts {
        m.update(p);
    }
    m.verify_slice(tag).is_ok()
}

/// `data` XOR the keystream of `key`, in place.
fn stream_xor(key: &[u8; 32], data: &mut [u8]) {
    for (i, chunk) in data.chunks_mut(32).enumerate() {
        let ks = mac(key, &[&(i as u32).to_be_bytes()]);
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
    }
}

/// The keys of one agreement, as `seal.js`'s `derive` makes them.
pub(crate) struct Keys {
    phrase_enc: Zeroizing<[u8; 32]>,
    phrase_mac: Zeroizing<[u8; 32]>,
    words_enc: Zeroizing<[u8; 32]>,
    words_mac: Zeroizing<[u8; 32]>,
    call: Zeroizing<[u8; 32]>,
}

pub(crate) fn derive(shared: &[u8; 32], view_pk: &[u8; 32], module_pk: &[u8; 32]) -> Keys {
    let prk = mac(LABEL, &[shared, view_pk, module_pk]);
    Keys {
        phrase_enc: mac(prk.as_ref(), &[b"phrase-enc"]),
        phrase_mac: mac(prk.as_ref(), &[b"phrase-mac"]),
        words_enc: mac(prk.as_ref(), &[b"words-enc"]),
        words_mac: mac(prk.as_ref(), &[b"words-mac"]),
        call: mac(prk.as_ref(), &[b"call-mac"]),
    }
}

impl Keys {
    /// The phrase the view sealed (its bytes, zeroized when dropped); a refusal names nothing of it.
    pub(crate) fn open_phrase(&self, ct: &[u8], tag: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        if !verifies(self.phrase_mac.as_ref(), &[ct], tag) {
            return Err("the sealed phrase does not verify (sealed to another key, or changed on the way): open the session again".into());
        }
        let mut out = Zeroizing::new(ct.to_vec());
        stream_xor(&self.phrase_enc, &mut out);
        Ok(out)
    }

    /// A new phrase's words sealed to the view: (ciphertext, MAC). The buffer is the words' only copy here, overwritten
    /// in place by the ciphertext.
    pub(crate) fn seal_words(&self, words: &[u8]) -> (Vec<u8>, [u8; 32]) {
        let mut ct = words.to_vec();
        stream_xor(&self.words_enc, &mut ct);
        let tag = *mac(self.words_mac.as_ref(), &[&ct]);
        (ct, tag)
    }

    /// Whether call `n`'s MAC holds for its JSON text.
    pub(crate) fn verify_call(&self, n: u64, body: &str, tag: &[u8]) -> bool {
        verifies(self.call.as_ref(), &[CALL_LABEL, &n.to_be_bytes(), body.as_bytes()], tag)
    }

    /// Call `n`'s MAC, as the view makes it (for the native client: tests and the `call` example).
    pub(crate) fn mac_call(&self, n: u64, body: &str) -> [u8; 32] {
        *mac(self.call.as_ref(), &[CALL_LABEL, &n.to_be_bytes(), body.as_bytes()])
    }

    /// The view's side of the phrase (for the native client): the ciphertext and its MAC.
    pub(crate) fn seal_phrase(&self, phrase: &[u8]) -> (Vec<u8>, [u8; 32]) {
        let mut ct = phrase.to_vec();
        stream_xor(&self.phrase_enc, &mut ct);
        let tag = *mac(self.phrase_mac.as_ref(), &[&ct]);
        (ct, tag)
    }

    /// The view's side of the words (for the native client).
    pub(crate) fn open_words(&self, ct: &[u8], tag: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        if !verifies(self.words_mac.as_ref(), &[ct], tag) {
            return Err("the sealed words do not verify".into());
        }
        let mut out = Zeroizing::new(ct.to_vec());
        stream_xor(&self.words_enc, &mut out);
        Ok(out)
    }
}

struct OneTime {
    id: u64,
    secret: StaticSecret,
    public: [u8; 32],
    made: Instant,
}

static ONE_TIME: Mutex<(u64, Vec<OneTime>)> = Mutex::new((1, Vec::new()));

/// `seal_key`: a new one-time key, `(its id, its public half)`. At most [`KEYS_HELD`] wait at once (the oldest goes).
pub(crate) fn new_key() -> Result<(u64, [u8; 32]), String> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(bytes.as_mut()).map_err(|e| format!("no randomness from the operating system: {e}"))?;
    let secret = StaticSecret::from(*bytes);
    let public = PublicKey::from(&secret).to_bytes();
    let mut g = ONE_TIME.lock().unwrap_or_else(|p| p.into_inner());
    g.1.retain(|k| k.made.elapsed() < KEY_TTL);
    while g.1.len() >= KEYS_HELD {
        g.1.remove(0);
    }
    let id = g.0;
    g.0 += 1;
    g.1.push(OneTime { id, secret, public, made: Instant::now() });
    Ok((id, public))
}

/// Agrees one-time key `id` (used up here, whatever happens next) with the view's public key: the session's keys and
/// the module's public half.
pub(crate) fn agree(id: u64, view_pk: &[u8; 32]) -> Result<(Keys, [u8; 32]), String> {
    let k = {
        let mut g = ONE_TIME.lock().unwrap_or_else(|p| p.into_inner());
        let i = g.1.iter().position(|k| k.id == id).ok_or("the module's one-time key is unknown or used: ask seal_key for another")?;
        g.1.remove(i)
    };
    if k.made.elapsed() >= KEY_TTL {
        return Err("the module's one-time key expired: ask seal_key for another".into());
    }
    let shared = k.secret.diffie_hellman(&PublicKey::from(*view_pk));
    if !shared.was_contributory() {
        return Err("the view's key is not usable (a low-order point): refused".into());
    }
    Ok((derive(shared.as_bytes(), view_pk, &k.public), k.public))
}

/// The view's side of an agreement (for the native client: tests and the `call` example): its key pair from
/// `secret`, agreed with the module's public half.
pub(crate) fn view_side(secret: &[u8; 32], module_pk: &[u8; 32]) -> (Keys, [u8; 32]) {
    let s = StaticSecret::from(*secret);
    let view_pk = PublicKey::from(&s).to_bytes();
    let shared = s.diffie_hellman(&PublicKey::from(*module_pk));
    (derive(shared.as_bytes(), &view_pk, module_pk), view_pk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    // The vector of modules/ferrata_bridge_ui/tests/seal-test.mjs: made by seal.js in the view's way (its Math.random
    // and Date.now fixed; the module's entropy 0x24 x 32), opened and answered here.
    const MODULE_PK: &str = "132c442be010fbd57e72603328aa76e71fccc1503aae219327d14d9c9993f472";
    const VIEW_KEY: &str = "ff854c15857d81eed2aff488fc21159467d9237bd12eec0640a5b28b397a3637";
    const PHRASE_CT: &str = "e8b88ff3a680ffa6b61317cb4856be235024974e70904d84b8da6ac7f4fa427ce5de0bb4e3c5dca4b37bebf7e757617cc7e1558622eb04acaabd5dc23d7a9df291ac33bf00007ccc8661b7b5ae76723c8cbb63076ddfb26cefdfc837ba";
    const PHRASE_MAC: &str = "34c69a20aad6445d026f3e83730e800ec1e8dfc753052779d24edc078fbfe3d3";
    const WORDS_CT: &str = "faf6875fa16f670447b59cfd944404dffa6445a1ede7f95073775582fbca4ab4bc06367b7ff973e2665b44f9f4d887a9281fa5bf9dcfcfd7ec482f688ee7290fffdbce9a3fed0f81d0f301";
    const WORDS_MAC: &str = "3fa1d584f8993ea258cdbc250428f52481d25a9ec3fb818a7edcc798c5855035";
    const CALL7_MAC: &str = "9e2db54380de70a44661f2dc1d8bb9f0e3ad1580c4fc57e8165dbcc383e75fe7";

    #[test]
    fn the_views_sealed_phrase_opens_in_the_module() {
        let module_secret = [0x42u8; 32];
        let module_pk = x25519_dalek::x25519(module_secret, x25519_dalek::X25519_BASEPOINT_BYTES);
        assert_eq!(hex(&module_pk), MODULE_PK);
        let view_pk: [u8; 32] = unhex(VIEW_KEY).try_into().unwrap();
        let k = derive(&x25519_dalek::x25519(module_secret, view_pk), &view_pk, &module_pk);
        let phrase = k.open_phrase(&unhex(PHRASE_CT), &unhex(PHRASE_MAC)).unwrap();
        assert_eq!(&phrase[..], b"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about");
        let (ct, tag) = k.seal_words(b"legal winner thank year wave sausage worth useful legal winner thank yellow");
        assert_eq!((hex(&ct), hex(&tag)), (WORDS_CT.to_string(), WORDS_MAC.to_string()));
        let body = r#"{"method":"info","params":{}}"#;
        assert!(k.verify_call(7, body, &unhex(CALL7_MAC)));
        assert!(!k.verify_call(8, body, &unhex(CALL7_MAC)), "another counter");
        assert!(!k.verify_call(7, r#"{"method":"burn","params":{}}"#, &unhex(CALL7_MAC)), "another call");
        // A changed ciphertext is refused, and the refusal names nothing of the phrase.
        let mut bad = unhex(PHRASE_CT);
        bad[3] ^= 1;
        let e = k.open_phrase(&bad, &unhex(PHRASE_MAC)).unwrap_err();
        assert!(e.contains("does not verify") && !e.contains("abandon"), "{e}");
    }

    #[test]
    fn a_one_time_key_is_used_once() {
        let (id, module_pk) = new_key().unwrap();
        let (view, view_pk) = view_side(&[7u8; 32], &module_pk);
        let (keys, pk) = agree(id, &view_pk).unwrap();
        assert_eq!(pk, module_pk);
        assert!(keys.verify_call(1, "x", &view.mac_call(1, "x")), "both sides hold the same call key");
        assert!(agree(id, &view_pk).err().unwrap().contains("unknown or used"));
        // A low-order point (all zeros) is refused, and uses the key up.
        let (id2, _) = new_key().unwrap();
        assert!(agree(id2, &[0u8; 32]).err().unwrap().contains("low-order"));
        assert!(agree(id2, &view_pk).is_err());
    }
}
