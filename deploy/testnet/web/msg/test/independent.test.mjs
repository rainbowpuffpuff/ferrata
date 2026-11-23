// The library against an implementation that shares none of its code (test/xcrypto.mjs: node:crypto and a hand
// HChaCha20) and against OpenSSL's ML-KEM-768 values (test/kat.json, written by test/make-kat.mjs): review findings L1
// (the HKDF info must fit any HKDF: node's, OpenSSL's, refuse more than 1,024 bytes) and L3 (no independent test of the
// crypto was in the repository). Runs without OpenSSL.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ml_kem768 } from '../vendor/crypto.js';
import * as M from '../ferrata-msg.js';
import * as X from './xcrypto.mjs';

const kat = JSON.parse(readFileSync(new URL('./kat.json', import.meta.url)));
const secret = X.H(kat.secret), A = X.H(kat.sender), B = X.H(kat.recipient);
const keys0 = M.deriveKeys(secret, 0);

test('HChaCha20 and XChaCha20-Poly1305 match draft-irtf-cfrg-xchacha-03 (sections 2.2.1 and A.3.1)', () => {
  const sub = X.hchacha20(X.H('000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f'), X.H('000000090000004a0000000031415927'));
  assert.equal(X.hex(sub), '82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc');
  const pt = Buffer.from("Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.");
  const ct = X.xSeal(X.H('808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f'), X.H('404142434445464748494a4b4c4d4e4f5051525354555657'), X.H('50515253c0c1c2c3c4c5c6c7'), pt);
  assert.equal(X.hex(ct.subarray(-16)), 'c0875924c1c7987947deafd8780acf49');
});

test('keys: the library derives the X25519 keys node:crypto derives and the ML-KEM-768 keys OpenSSL derives', () => {
  for (const k of kat.keys) {
    const lib = M.deriveKeys(secret, k.epoch);
    assert.equal(X.hex(lib.x25519.secretKey), k.x25519_secret, `epoch ${k.epoch} X25519 secret`);
    assert.equal(X.hex(lib.x25519.publicKey), k.x25519_public, `epoch ${k.epoch} X25519 public`);
    assert.equal(X.hex(lib.mlkem768.publicKey), k.mlkem768_public, `epoch ${k.epoch} ML-KEM-768 public`);
  }
});

test('ML-KEM-768: the library decapsulates OpenSSL\'s encapsulations to the same shared secrets', () => {
  for (const e of kat.encapsulations) {
    assert.equal(X.hex(ml_kem768.decapsulate(X.H(e.ct), keys0.mlkem768.secretKey)), e.ss);
  }
});

test('envelopes sealed independently (node:crypto, OpenSSL\'s ML-KEM) open in the library', () => {
  for (const e of kat.envelopes) {
    const o = M.open({ body: X.H(e.body), sender: A, recipient: B, keys: keys0 });
    assert.deepEqual([o.text, o.kind, o.sentMs, o.bucket], [e.text, 1, BigInt(e.sent_ms), e.bucket], e.text);
  }
});

test('envelopes the library seals open independently, and only for their own sender', () => {
  // The independent side needs an ML-KEM decapsulation: the library's, checked against OpenSSL's above.
  const kemDecap = (ct) => ml_kem768.decapsulate(ct, keys0.mlkem768.secretKey);
  for (const [hybrid, text] of [[true, 'from the library, hybrid'], [true, 'z'.repeat(2900)], [false, 'from the library, X25519 only']]) {
    const key = M.publicRecord(keys0, { hybrid });
    const body = M.seal({ sender: A, recipient: B, key, text, sentMs: 1790000000123, kind: 3 });
    const o = X.independentOpen({ body, xsk: keys0.x25519.secretKey, rx: keys0.x25519.publicKey, sender: A, recipient: B, kemDecap });
    assert.deepEqual([o.text, o.kind, o.sentMs, o.zeros], [text, 3, 1790000000123n, true]);
    assert.throws(() => X.independentOpen({ body, xsk: keys0.x25519.secretKey, rx: keys0.x25519.publicKey, sender: B, recipient: B, kemDecap }));
  }
});

test('L1: a v1 hybrid envelope (the live program\'s format) cannot be opened with an OpenSSL HKDF; a v2 one can', () => {
  const kemDecap = (ct) => ml_kem768.decapsulate(ct, keys0.mlkem768.secretKey);
  const legacy = M.seal({ sender: A, recipient: B, key: M.publicRecord(keys0), text: 'v1 format', legacy: true });
  assert.throws(
    () => X.independentOpen({ body: legacy, xsk: keys0.x25519.secretKey, rx: keys0.x25519.publicKey, sender: A, recipient: B, kemDecap, keyOf: X.legacyEnvelopeKey }),
    /info|length|range/i,
  );
  const current = M.seal({ sender: A, recipient: B, key: M.publicRecord(keys0), text: 'v2 format' });
  const o = X.independentOpen({ body: current, xsk: keys0.x25519.secretKey, rx: keys0.x25519.publicKey, sender: A, recipient: B, kemDecap });
  assert.equal(o.text, 'v2 format');
});
