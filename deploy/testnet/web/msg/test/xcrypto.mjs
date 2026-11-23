// An implementation of ferrata_msg_v2's envelope that shares no code with ferrata-msg.js or its vendored @noble
// packages: X25519, HKDF-SHA256 and ChaCha20-Poly1305 from node:crypto (OpenSSL), HChaCha20 written out here (checked
// against draft-irtf-cfrg-xchacha-03's vector by the test). ML-KEM-768 is not in node:crypto: the test takes OpenSSL's
// values from test/kat.json (written by test/make-kat.mjs) and checks the library's ML-KEM against them first.
// Adapted from the independent review's cross-check (23 November 2026).
import crypto from 'node:crypto';

export const H = (s) => Uint8Array.from(Buffer.from(s.replace(/[\s:]/g, ''), 'hex'));
export const hex = (b) => Buffer.from(b).toString('hex');
export const cat = (...p) => Uint8Array.from(Buffer.concat(p.map((x) => Buffer.from(x))));
export const eq = (a, b) => Buffer.from(a).equals(Buffer.from(b));
export const u32 = (v) => { const b = Buffer.alloc(4); b.writeUInt32LE(v); return b; };
export const u64 = (v) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(v)); return b; };
export const SALT = Buffer.from('ferrata-msg/v1');

export function hchacha20(key, n16) {
  const s = new Uint32Array(16);
  s[0] = 0x61707865; s[1] = 0x3320646e; s[2] = 0x79622d32; s[3] = 0x6b206574;
  const kv = new DataView(key.buffer, key.byteOffset, 32), nv = new DataView(n16.buffer, n16.byteOffset, 16);
  for (let i = 0; i < 8; i++) s[4 + i] = kv.getUint32(4 * i, true);
  for (let i = 0; i < 4; i++) s[12 + i] = nv.getUint32(4 * i, true);
  const r = (v, c) => ((v << c) | (v >>> (32 - c))) >>> 0;
  const q = (a, b, c, d) => {
    s[a] = (s[a] + s[b]) >>> 0; s[d] = r(s[d] ^ s[a], 16); s[c] = (s[c] + s[d]) >>> 0; s[b] = r(s[b] ^ s[c], 12);
    s[a] = (s[a] + s[b]) >>> 0; s[d] = r(s[d] ^ s[a], 8); s[c] = (s[c] + s[d]) >>> 0; s[b] = r(s[b] ^ s[c], 7);
  };
  for (let i = 0; i < 10; i++) { q(0, 4, 8, 12); q(1, 5, 9, 13); q(2, 6, 10, 14); q(3, 7, 11, 15); q(0, 5, 10, 15); q(1, 6, 11, 12); q(2, 7, 8, 13); q(3, 4, 9, 14); }
  const out = new Uint8Array(32), ov = new DataView(out.buffer);
  [0, 1, 2, 3, 12, 13, 14, 15].forEach((w, i) => ov.setUint32(4 * i, s[w], true));
  return out;
}
const xc = (key, nonce24) => ({ sub: hchacha20(key, nonce24.subarray(0, 16)), n12: cat(new Uint8Array(4), nonce24.subarray(16, 24)) });
export function xOpen(key, nonce24, aad, ctTag) {
  const { sub, n12 } = xc(key, nonce24);
  const d = crypto.createDecipheriv('chacha20-poly1305', sub, n12, { authTagLength: 16 });
  d.setAAD(aad, { plaintextLength: ctTag.length - 16 });
  d.setAuthTag(ctTag.subarray(ctTag.length - 16));
  return cat(d.update(ctTag.subarray(0, ctTag.length - 16)), d.final());
}
export function xSeal(key, nonce24, aad, pt) {
  const { sub, n12 } = xc(key, nonce24);
  const c = crypto.createCipheriv('chacha20-poly1305', sub, n12, { authTagLength: 16 });
  c.setAAD(aad, { plaintextLength: pt.length });
  return cat(c.update(pt), c.final(), c.getAuthTag());
}
const X_PRIV = H('302e020100300506032b656e04220420'), X_PUB = H('302a300506032b656e032100');
const xPriv = (sk) => crypto.createPrivateKey({ key: Buffer.from(cat(X_PRIV, sk)), format: 'der', type: 'pkcs8' });
export const xPubOf = (sk) => new Uint8Array(crypto.createPublicKey(xPriv(sk)).export({ format: 'der', type: 'spki' })).subarray(12);
export const xDH = (sk, pk) => new Uint8Array(crypto.diffieHellman({ privateKey: xPriv(sk), publicKey: crypto.createPublicKey({ key: Buffer.from(cat(X_PUB, pk)), format: 'der', type: 'spki' }) }));
/** node:crypto's HKDF (OpenSSL's): it refuses an info longer than 1,024 bytes. */
export const hkdf = (ikm, salt, info, n) => new Uint8Array(crypto.hkdfSync('sha256', ikm, salt, info, n));
const sha256 = (b) => new Uint8Array(crypto.createHash('sha256').update(b).digest());

/** v2's key: HKDF(ss, salt, "envelope" || SHA-256(header || rx || sender || recipient)). */
export const envelopeKey = (ss, header, rx, sender, recipient) => hkdf(ss, SALT, cat(Buffer.from('envelope'), sha256(cat(header, rx, sender, recipient))), 32);
/** v1's key: HKDF(ss, salt, "envelope" || header || rx || sender || recipient): over OpenSSL's limit for a hybrid header. */
export const legacyEnvelopeKey = (ss, header, rx, sender, recipient) => hkdf(ss, SALT, cat(Buffer.from('envelope'), header, rx, sender, recipient), 32);

/** Open a v2 envelope; `kemDecap(ct)` gives the ML-KEM shared secret (from OpenSSL's values or a checked implementation). */
export function independentOpen({ body, xsk, rx, sender, recipient, kemDecap, keyOf = envelopeKey }) {
  const v = body[0];
  const hybrid = v === 4 || v === 2;
  const hl = 1 + 4 + 32 + (hybrid ? 1088 : 0) + 24;
  const header = body.subarray(0, hl), epk = header.subarray(5, 37);
  let ss = xDH(xsk, epk);
  if (hybrid) ss = cat(ss, kemDecap(header.subarray(37, 37 + 1088)));
  const k = keyOf(ss, header, rx, sender, recipient);
  const pt = xOpen(k, header.subarray(hl - 24), cat(header, sender, recipient), body.subarray(hl));
  const len = pt[9] | (pt[10] << 8);
  return { kind: pt[0], sentMs: Buffer.from(pt.subarray(1, 9)).readBigUInt64LE(), text: Buffer.from(pt.subarray(11, 11 + len)).toString('utf8'), zeros: pt.subarray(11 + len).every((x) => x === 0) };
}

/** Seal a v2 envelope with fixed inputs: esk, nonce, and (hybrid) an ML-KEM ciphertext and its shared secret. */
export function independentSeal({ hybrid, epoch, rx, sender, recipient, text, kind, sentMs, bucket, esk, nonce, kem }) {
  const epk = xPubOf(esk);
  let ss = xDH(esk, rx), kemCt = new Uint8Array(0);
  if (hybrid) { kemCt = kem.ct; ss = cat(ss, kem.ss); }
  const header = cat(Uint8Array.of(hybrid ? 4 : 3), u32(epoch), epk, kemCt, nonce);
  const k = envelopeKey(ss, header, rx, sender, recipient);
  const t = Buffer.from(text, 'utf8');
  const pt = new Uint8Array(bucket - header.length - 16);
  pt.set(cat(Uint8Array.of(kind), u64(sentMs), Uint8Array.of(t.length & 0xff, t.length >> 8), t));
  return cat(header, xSeal(k, nonce, cat(header, sender, recipient), pt));
}
