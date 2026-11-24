// ferrata-msg.js: encrypted wallet-to-wallet messages on the Ferrata LEZ v0.4 test network (prototype, unaudited).
// For the browser wallet and for node (the CLI, the tests). Design: docs/design/messages.md. Program: ferrata_msg
// (lez/guests/msg; its messages and state are lez/msg-wire/src/lib.rs, mirrored here byte for byte).
//
// Keys:      deriveKeys(accountSecret, epoch)  -> X25519 + ML-KEM-768 pairs, from the wallet's account secret
// Program:   encodePublish / encodeSend / encodeDelete / encodeClear / encodeContacts / encodeBlock / encodeSetPolicy
// State:     decodeMailbox(actorStateBytes)    -> the actor (account, ferrata_msg)'s published key, inbox, counters
// Envelope:  seal({...}) -> bucket-sized bytes; open({...}) -> { kind, sentMs, text }
// List:      readInbox(mailbox, recipientId, accountSecret) -> each message decrypted (or the reason it is not)
import { x25519, xchacha20poly1305, hkdf, sha256, ml_kem768, base58 } from './vendor/crypto.js';

const enc = new TextEncoder();
const dec = new TextDecoder('utf-8', { fatal: true });
const utf8 = (s) => enc.encode(s);

export const BUCKETS = [512, 1024, 2048, 4096];
export const V_X25519 = 1;
export const V_HYBRID = 2;
export const MLKEM768_PK_LEN = 1184;
export const MLKEM768_CT_LEN = 1088;
export const MAX_MESSAGES = 32;
export const MAX_INBOX_BYTES = 64 * 1024;
export const STRANGER_SLOTS = 8;
export const STRANGER_UNREAD = 2;
export const RATE_WINDOW_MS = 3_600_000;
export const RATE_MAX = 30;
export const CLOCK_SKEW_MS = 600_000;
const TAG = 16;
const PLAIN_HEADER = 11; // kind u8 | sent_ms u64 | len u16
const SALT = utf8('ferrata-msg/v1');

/** The program's exit codes (lez/msg-wire `code`). */
export const CODES = {
  0xa1: 'BAD_MESSAGE', 0xa2: 'BAD_STATE', 0xa3: 'NOT_AUTHORIZED', 0xa4: 'WRONG_SENDER', 0xa5: 'NO_KEY',
  0xa6: 'BAD_KEY', 0xa7: 'BLOCKED', 0xa8: 'NOT_A_CONTACT', 0xa9: 'BAD_SIZE', 0xaa: 'BAD_VERSION', 0xab: 'STALE_KEY',
  0xac: 'DOWNGRADE', 0xad: 'INBOX_FULL', 0xae: 'STRANGER_LIMIT', 0xaf: 'STRANGERS_FULL', 0xb0: 'RATE_LIMITED',
  0xb1: 'CLOCK_BACKWARDS', 0xb2: 'SELF_SEND', 0xb3: 'LIST_FULL', 0xb4: 'NOT_FOUND',
};

// ---------------------------------------------------------------- bytes and borsh
export function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
function le(v, bytes) {
  let n = BigInt(v);
  if (n < 0n || n >= 1n << BigInt(8 * bytes)) throw new Error(`value out of range for u${8 * bytes}`);
  const out = new Uint8Array(bytes);
  for (let i = 0; i < bytes; i++) { out[i] = Number(n & 0xffn); n >>= 8n; }
  return out;
}
const u8 = (v) => Uint8Array.of(v);
const u32 = (v) => le(v, 4);
const u64 = (v) => le(v, 8);
const vecBytes = (b) => concat(u32(b.length), b);
function id32(b) { if (!(b instanceof Uint8Array) || b.length !== 32) throw new Error('an account id is 32 bytes'); return b; }
const vecIds = (ids) => concat(u32(ids.length), ...ids.map(id32));

class Reader {
  constructor(b) { this.b = b; this.o = 0; }
  take(n) { if (this.o + n > this.b.length) throw new Error('borsh: out of data'); const s = this.b.subarray(this.o, this.o + n); this.o += n; return s; }
  u8() { return this.take(1)[0]; }
  uint(n) { const s = this.take(n); let v = 0n; for (let i = n - 1; i >= 0; i--) v = (v << 8n) | BigInt(s[i]); return v; }
  u32() { return Number(this.uint(4)); }
  u64() { return this.uint(8); }
  bytes() { return this.take(this.u32()); }
  vec(f) { const n = this.u32(); const out = []; for (let i = 0; i < n; i++) out.push(f(this)); return out; }
  option(f) { const t = this.u8(); if (t === 0) return null; if (t === 1) return f(this); throw new Error('borsh: bad option tag'); }
}

// ---------------------------------------------------------------- keys
/**
 * The messaging keys of an account at `epoch`, derived from its secret (the 32-byte key the account signs with), so
 * that they need no backup of their own: seed = HKDF-SHA256(secret, "ferrata-msg/v1", "keys" || epoch u32), 96 bytes;
 * X25519 secret = seed[0..32]; ML-KEM-768 = keygen(seed[32..96]) (FIPS 203 d || z).
 */
export function deriveKeys(accountSecret, epoch = 0) {
  if (accountSecret.length !== 32) throw new Error('an account secret is 32 bytes');
  const seed = hkdf(sha256, accountSecret, SALT, concat(utf8('keys'), u32(epoch)), 96);
  const xsk = seed.slice(0, 32);
  const kem = ml_kem768.keygen(seed.slice(32, 96));
  return {
    epoch,
    x25519: { secretKey: xsk, publicKey: x25519.getPublicKey(xsk) },
    mlkem768: { secretKey: kem.secretKey, publicKey: kem.publicKey },
  };
}
/** The record `Publish` puts on chain (`MsgKey`). `hybrid = false` publishes X25519 only. */
export function publicRecord(keys, { hybrid = true } = {}) {
  return { epoch: keys.epoch, x25519: keys.x25519.publicKey, mlkem768: hybrid ? keys.mlkem768.publicKey : null };
}
/** A short fingerprint of a published key, to compare out of band (first 10 bytes of its SHA-256, base58). */
export function fingerprint(record) {
  return base58.encode(sha256(encodeKey(record)).slice(0, 10));
}

// ---------------------------------------------------------------- the program's messages (MsgMessage, borsh)
function encodeKey(k) {
  if (k.x25519.length !== 32) throw new Error('an X25519 key is 32 bytes');
  if (k.mlkem768 && k.mlkem768.length !== MLKEM768_PK_LEN) throw new Error('an ML-KEM-768 key is 1184 bytes');
  return concat(u32(k.epoch), k.x25519, k.mlkem768 ? concat(u8(1), vecBytes(k.mlkem768)) : u8(0));
}
export const encodePublish = (record) => concat(u8(0), encodeKey(record));
export const POLICY = { open: 0, contactsOnly: 1 };
export const encodeSetPolicy = (policy) => concat(u8(1), u8(policy));
export const encodeContacts = (add = [], remove = []) => concat(u8(2), vecIds(add), vecIds(remove));
export const encodeBlock = (add = [], remove = []) => concat(u8(3), vecIds(add), vecIds(remove));
export const encodeSend = ({ to, nowMs, body }) => concat(u8(4), id32(to), u64(nowMs), vecBytes(body));
export const encodeDelete = (seqs) => concat(u8(6), u32(seqs.length), ...seqs.map(u64));
export const encodeClear = (upto) => concat(u8(7), u64(upto));

// ---------------------------------------------------------------- the actor's state (Mailbox, borsh)
export function decodeMailbox(bytes) {
  if (!bytes || bytes.length === 0) return null;
  const r = new Reader(bytes);
  const key = r.option((x) => ({ epoch: x.u32(), x25519: x.take(32), mlkem768: x.option((y) => y.bytes()) }));
  const policy = r.u8() === 1 ? 'contactsOnly' : 'open';
  const contacts = r.vec((x) => x.take(32));
  const blocked = r.vec((x) => x.take(32));
  const nextSeq = r.u64();
  const inbox = r.vec((x) => ({ seq: x.u64(), from: x.take(32), atMs: x.u64(), body: x.bytes() }));
  const m = { key, policy, contacts, blocked, nextSeq, inbox };
  m.windowStartMs = r.u64(); m.sentInWindow = r.u32(); m.lastSendMs = r.u64(); m.sentTotal = r.u64();
  if (r.o !== bytes.length) throw new Error('mailbox: trailing bytes');
  return m;
}

// ---------------------------------------------------------------- the envelope
function headerLen(version) { return 1 + 4 + 32 + (version === V_HYBRID ? MLKEM768_CT_LEN : 0) + 24; }
/** The largest text (UTF-8 bytes) an envelope of `version` holds in `bucket`. */
export const capacity = (version, bucket) => bucket - headerLen(version) - TAG - PLAIN_HEADER;
export function bucketFor(version, textBytes) {
  const b = BUCKETS.find((n) => capacity(version, n) >= textBytes);
  if (!b) throw new Error(`text too long: ${textBytes} bytes, at most ${capacity(version, BUCKETS.at(-1))}`);
  return b;
}
function keyAndAad(header, ss, rx, sender, recipient) {
  const bind = concat(header, rx, id32(sender), id32(recipient));
  return { k: hkdf(sha256, ss, SALT, concat(utf8('envelope'), bind), 32), aad: concat(header, sender, recipient) };
}

/**
 * Encrypt `text` from `sender` to `recipient` (account ids) under the recipient's published `key` (as decodeMailbox
 * returns it). Hybrid (X25519 + ML-KEM-768) whenever the key has ML-KEM; a fresh ephemeral key per message; padded to
 * the smallest bucket that fits (or `bucket`, if given and large enough).
 */
export function seal({ sender, recipient, key, text, sentMs = Date.now(), kind = 0, bucket = null, rand = null }) {
  const random = rand || ((n) => globalThis.crypto.getRandomValues(new Uint8Array(n)));
  const version = key.mlkem768 ? V_HYBRID : V_X25519;
  const t = typeof text === 'string' ? utf8(text) : text;
  if (t.length > 0xffff) throw new Error('text too long');
  const size = bucket ?? bucketFor(version, t.length);
  if (!BUCKETS.includes(size) || capacity(version, size) < t.length) throw new Error('bad bucket');
  const esk = random(32);
  const epk = x25519.getPublicKey(esk);
  let ss = x25519.getSharedSecret(esk, key.x25519);
  let kemCt = new Uint8Array(0);
  if (version === V_HYBRID) {
    const e = ml_kem768.encapsulate(key.mlkem768, random(32));
    kemCt = e.cipherText;
    ss = concat(ss, e.sharedSecret);
  }
  const header = concat(u8(version), u32(key.epoch), epk, kemCt, random(24));
  const { k, aad } = keyAndAad(header, ss, key.x25519, sender, recipient);
  const plain = new Uint8Array(size - header.length - TAG);
  plain.set(concat(u8(kind), u64(sentMs), le(t.length, 2), t));
  const nonce = header.subarray(header.length - 24);
  return concat(header, xchacha20poly1305(k, nonce, aad).encrypt(plain));
}

/** Decrypt an envelope with the recipient's derived `keys` of the envelope's epoch. Throws if it does not open. */
export function open({ body, sender, recipient, keys }) {
  const version = body[0];
  if (version !== V_X25519 && version !== V_HYBRID) throw new Error(`unknown envelope version ${version}`);
  const epoch = Number(new Reader(body.subarray(1, 5)).u32());
  if (epoch !== keys.epoch) throw new Error(`envelope for key epoch ${epoch}, keys of epoch ${keys.epoch}`);
  const hl = headerLen(version);
  const header = body.subarray(0, hl);
  const epk = header.subarray(5, 37);
  let ss = x25519.getSharedSecret(keys.x25519.secretKey, epk);
  if (version === V_HYBRID) ss = concat(ss, ml_kem768.decapsulate(header.subarray(37, 37 + MLKEM768_CT_LEN), keys.mlkem768.secretKey));
  const { k, aad } = keyAndAad(header, ss, keys.x25519.publicKey, sender, recipient);
  const plain = xchacha20poly1305(k, header.subarray(hl - 24), aad).decrypt(body.subarray(hl));
  const r = new Reader(plain);
  const kind = r.u8();
  const sentMs = r.u64();
  const len = Number(r.uint(2));
  const text = dec.decode(r.take(len));
  if (plain.subarray(PLAIN_HEADER + len).some((b) => b !== 0)) throw new Error('padding is not zero');
  return { kind, sentMs, text, version, bucket: body.length };
}

/**
 * Every message of a recipient's inbox, decrypted with keys derived from `accountSecret` (each envelope's epoch).
 * Returns [{ seq, from (base58), atMs, bucket, version, text | error }], oldest first.
 */
export function readInbox(mailbox, recipientId, accountSecret) {
  if (!mailbox) return [];
  const keysByEpoch = new Map();
  return mailbox.inbox.map((m) => {
    const out = { seq: m.seq, from: base58.encode(m.from), fromId: m.from, atMs: m.atMs, bucket: m.body.length, version: m.body[0] };
    try {
      const epoch = Number(new Reader(m.body.subarray(1, 5)).u32());
      if (!keysByEpoch.has(epoch)) keysByEpoch.set(epoch, deriveKeys(accountSecret, epoch));
      const o = open({ body: m.body, sender: m.from, recipient: recipientId, keys: keysByEpoch.get(epoch) });
      return { ...out, text: o.text, sentMs: o.sentMs, kind: o.kind };
    } catch (e) {
      return { ...out, error: String(e.message || e) };
    }
  });
}

export const idToString = (id) => base58.encode(id);
export function idFromString(s) { const b = base58.decode(s.trim()); if (b.length !== 32) throw new Error('an account id is 32 bytes in base58'); return b; }
export { hex, sha256 };

// ---------------------------------------------------------------- preflight (the program's rules, before paying a fee)
const sameId = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
const has = (list, id) => list.some((x) => sameId(x, id));
/**
 * The exit code the program would answer `Send { to: recipient, nowMs, body }` from `sender` with, given both actors'
 * current state (decodeMailbox; null if empty), or null if it would deliver. Mirrors lez/msg-wire `handle`.
 */
export function preflightSend({ senderId, recipientId, sender, recipient, body, nowMs }) {
  const now = BigInt(nowMs);
  if (sameId(senderId, recipientId)) return 'SELF_SEND';
  if (!BUCKETS.includes(body.length)) return 'BAD_SIZE';
  if (body[0] !== V_X25519 && body[0] !== V_HYBRID) return 'BAD_VERSION';
  if (sender) {
    if (now < sender.lastSendMs) return 'CLOCK_BACKWARDS';
    const fresh = now >= sender.windowStartMs + BigInt(RATE_WINDOW_MS);
    if (!fresh && sender.sentInWindow >= RATE_MAX) return 'RATE_LIMITED';
  }
  if (!recipient || !recipient.key) return 'NO_KEY';
  if (has(recipient.blocked, senderId)) return 'BLOCKED';
  const contact = has(recipient.contacts, senderId);
  if (recipient.policy === 'contactsOnly' && !contact) return 'NOT_A_CONTACT';
  if (body[0] === V_HYBRID && !recipient.key.mlkem768) return 'BAD_VERSION';
  if (body[0] === V_X25519 && recipient.key.mlkem768) return 'DOWNGRADE';
  if (new Reader(body.subarray(1, 5)).u32() !== recipient.key.epoch) return 'STALE_KEY';
  const bytes = recipient.inbox.reduce((n, m) => n + m.body.length, 0);
  if (recipient.inbox.length >= MAX_MESSAGES || bytes + body.length > MAX_INBOX_BYTES) return 'INBOX_FULL';
  if (!contact) {
    if (recipient.inbox.filter((m) => sameId(m.from, senderId)).length >= STRANGER_UNREAD) return 'STRANGER_LIMIT';
    if (recipient.inbox.filter((m) => !has(recipient.contacts, m.from)).length >= STRANGER_SLOTS) return 'STRANGERS_FULL';
  }
  return null;
}
