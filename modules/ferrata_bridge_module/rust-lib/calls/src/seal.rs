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
    // and Date.now fixed), opened and answered here.
    const MODULE_PK: &str = "132c442be010fbd57e72603328aa76e71fccc1503aae219327d14d9c9993f472";
    const VIEW_KEY: &str = "1db374d9f0d91bc172bbce9216043a64a4b9a1dbf899f25749e5fbf7eb8c4a57";
    const PHRASE_CT: &str = "9816ae171b0e755a9e76002e81cfbc60937c2d2176dbd7c78f8621738d36b8cf2c39ceaa7c43975308a6ae880eb644c74b2346b7e610c56f4a26c5b9c9fafa38a546bfa5e7ea45f21aba0745539906a21ec2776e99f66f45a414147764";
    const PHRASE_MAC: &str = "ec1692184be31a1465340439d41fc2f7df4a43495ddd7541e748703e7032f37e";
    const WORDS_CT: &str = "31f12c4c5a24a630c67530260c48bc83992716e7f5643c2fb747fccb81571b25d27ba0f7edc2765d6676eac50fc415fb1bb4dbf9088c8478587a04e09859bd2e720efb40082706d2ecc557";
    const WORDS_MAC: &str = "fcd904c95f237d4ffee966aaeb5de586d842ecacfd005a2f7a5080f11fa95b96";
    const CALL7_MAC: &str = "eb7c11840968d2c10311a5a8456536ccba9db8bdd679edb944b93471574a2534";

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
