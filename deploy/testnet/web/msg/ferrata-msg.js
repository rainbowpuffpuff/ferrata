// ferrata-msg.js: encrypted wallet-to-wallet messages on the Ferrata LEZ v0.4 test network (prototype, unaudited).
// For the browser wallet and for node (the CLI, the tests). Design: docs/design/messages.md. Program: ferrata_msg_v2
// (lez/guests/msg-v2; its messages, states and PDAs are lez/msg-v2-wire/src/lib.rs, mirrored here byte for byte).
// The first program, ferrata_msg (v1, live, immutable), is read only here: decodeLegacyMailbox, readLegacyInbox.
//
// Keys:      deriveKeys(accountSecret, epoch)   -> X25519 + ML-KEM-768 pairs, from the wallet's account secret
// Program:   encodePublish / encodeSend / encodeDelete / encodeClear / encodeContacts / encodeBlock / encodeSetPolicy
// Accounts:  inboxAccount(program, owner), envelopeAccount(program, recipient, sender, seq)
// State:     decodeProfile / decodeInbox / decodeEnvelope (the three actors of an account's messages)
// Envelope:  seal({...}) -> bucket-sized bytes; open({...}) -> { kind, sentMs, text }
// List:      openInbox({ inbox, envelopes, recipientId, accountSecret }) -> each message decrypted (or why not)
// Checks:    preflightSend({...}) -> the code the program would refuse a send with, or null; fingerprint(key)
import { x25519, xchacha20poly1305, hkdf, sha256, ml_kem768, base58 } from './vendor/crypto.js';

const enc = new TextEncoder();
const dec = new TextDecoder('utf-8', { fatal: true });
const utf8 = (s) => enc.encode(s);

export const BUCKETS = [512, 1024, 2048, 4096];
/** ferrata_msg_v2's envelope versions: X25519, and X25519 + ML-KEM-768; the HKDF transcript is hashed. */
export const V_X25519 = 3;
export const V_HYBRID = 4;
/** ferrata_msg's (v1) envelope versions, read only: the HKDF `info` carries the whole transcript. */
export const LEGACY_X25519 = 1;
export const LEGACY_HYBRID = 2;
export const HYBRID_MIN_BUCKET = 2048;
export const MLKEM768_PK_LEN = 1184;
export const MLKEM768_CT_LEN = 1088;
export const MAX_MESSAGES = 32;
export const MAX_INBOX_BYTES = 64 * 1024;
export const STRANGER_SLOTS = 8;
export const STRANGER_UNREAD = 1;
export const RATE_WINDOW_MS = 3_600_000;
export const RATE_MAX = 30;
export const CLOCK_SKEW_MS = 600_000;
export const MAX_LIST = 64;
const TAG = 16;
const PLAIN_HEADER = 11; // kind u8 | sent_ms u64 | len u16
const SALT = utf8('ferrata-msg/v1');
const PDA_DOMAIN = utf8('ferrata-msg/v2/');
const PDA_PREFIX = (() => { const b = new Uint8Array(32); b.set(utf8('/LEE/v0.2/AccountId/PDA/')); return b; })();

/** ferrata_msg_v2's exit codes (lez/msg-v2-wire `code`; the first twenty are v1's too). */
export const CODES = {
  0xa1: 'BAD_MESSAGE', 0xa2: 'BAD_STATE', 0xa3: 'NOT_AUTHORIZED', 0xa4: 'WRONG_SENDER', 0xa5: 'NO_KEY',
  0xa6: 'BAD_KEY', 0xa7: 'BLOCKED', 0xa8: 'NOT_A_CONTACT', 0xa9: 'BAD_SIZE', 0xaa: 'BAD_VERSION', 0xab: 'STALE_KEY',
  0xac: 'DOWNGRADE', 0xad: 'INBOX_FULL', 0xae: 'STRANGER_LIMIT', 0xaf: 'STRANGERS_FULL', 0xb0: 'RATE_LIMITED',
  0xb1: 'CLOCK_BACKWARDS', 0xb2: 'SELF_SEND', 0xb3: 'LIST_FULL', 0xb4: 'NOT_FOUND', 0xb5: 'STALE_SEQ',
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
const sameId = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
const has = (list, id) => list.some((x) => sameId(x, id));

class Reader {
  constructor(b) { this.b = b; this.o = 0; }
  take(n) { if (this.o + n > this.b.length) throw new Error('borsh: out of data'); const s = this.b.subarray(this.o, this.o + n); this.o += n; return s; }
  u8() { return this.take(1)[0]; }
  uint(n) { const s = this.take(n); let v = 0n; for (let i = n - 1; i >= 0; i--) v = (v << 8n) | BigInt(s[i]); return v; }
  u32() { return Number(this.uint(4)); }
  u64() { return this.uint(8); }
  bool() { const v = this.u8(); if (v > 1) throw new Error('borsh: bad bool'); return v === 1; }
  bytes() { return this.take(this.u32()); }
  vec(f) { const n = this.u32(); const out = []; for (let i = 0; i < n; i++) out.push(f(this)); return out; }
  option(f) { const t = this.u8(); if (t === 0) return null; if (t === 1) return f(this); throw new Error('borsh: bad option tag'); }
  end(what) { if (this.o !== this.b.length) throw new Error(`${what}: trailing bytes`); }
}

// ---------------------------------------------------------------- keys
/**
 * The messaging keys of an account at `epoch`, derived from its secret (the 32-byte key the account signs with), so
 * that they need no backup of their own: seed = HKDF-SHA256(secret, "ferrata-msg/v1", "keys" || epoch u32), 96 bytes;
 * X25519 secret = seed[0..32]; ML-KEM-768 = keygen(seed[32..96]) (FIPS 203 d || z). Every epoch derives from the one
 * secret: rotating bounds a leaked epoch key, not a leaked wallet.
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
/** Whether `record` has the fingerprint `expected` (as `fingerprint` prints it). */
export function hasFingerprint(record, expected) {
  return fingerprint(record) === String(expected).trim();
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
/** A list edit names at most 64 ids each way: more additions can never fit a list of 64, more removals change nothing. */
function listEdit(tag, add, remove) {
  if (add.length > MAX_LIST || remove.length > MAX_LIST) throw new Error(`an edit adds and removes at most ${MAX_LIST} ids each`);
  return concat(u8(tag), vecIds(add), vecIds(remove));
}
/**
 * LIST_FULL if the list would hold more than 64 ids after the edit (removals first, then additions not yet present), as
 * the program refuses it; else null. Checking it first matters: a refused edit's comparisons are not bounded by the
 * client's gas limit (`gasFor('edit', ids)` covers edits that apply).
 */
export function preflightEdit(list, add = [], remove = []) {
  const kept = list.filter((x) => !has(remove, x));
  for (const a of add) if (!has(kept, a)) kept.push(a);
  return kept.length > MAX_LIST ? 'LIST_FULL' : null;
}
export const encodeContacts = (add = [], remove = []) => listEdit(2, add, remove);
export const encodeBlock = (add = [], remove = []) => listEdit(3, add, remove);
/** `Send { to, now_ms, seq, body }`: `seq` is the sender's send counter (its profile's `sentTotal`). */
export const encodeSend = ({ to, nowMs, seq, body }) => concat(u8(4), id32(to), u64(nowMs), u64(seq), vecBytes(body));
/** `Delete { ns }`: message numbers, sorted and deduplicated here (the program wants them strictly ascending). */
export function encodeDelete(ns) {
  const s = [...new Set(ns.map(BigInt))].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  if (s.length === 0 || s.length > MAX_MESSAGES) throw new Error(`delete takes 1 to ${MAX_MESSAGES} message numbers`);
  return concat(u8(5), u32(s.length), ...s.map(u64));
}
export const encodeClear = (upto) => concat(u8(6), u64(upto));

// ---------------------------------------------------------------- accounts (LEZ PDAs)
function seed(tag, ...parts) { return sha256(concat(PDA_DOMAIN, utf8(tag), ...parts)); }
/** LEZ v0.4's `AccountId::for_public_pda(program, seed)`. */
export const pda = (program, s) => sha256(concat(PDA_PREFIX, id32(program), s));
export const inboxAccount = (program, owner) => pda(program, seed('inbox', id32(owner)));
export const envelopeAccount = (program, recipient, sender, seqNo) => pda(program, seed('envelope', id32(recipient), id32(sender), u64(seqNo)));

// ---------------------------------------------------------------- the actors' states (borsh)
const readKey = (x) => ({ epoch: x.u32(), x25519: x.take(32), mlkem768: x.option((y) => y.bytes()) });
/** The profile actor `(account, program)`; an empty state is the empty profile. */
export function decodeProfile(bytes) {
  if (!bytes || bytes.length === 0) {
    return { key: null, policy: 'open', contacts: [], blocked: [], sentAt: [], lastSendMs: 0n, sentTotal: 0n };
  }
  const r = new Reader(bytes);
  const p = {
    key: r.option(readKey), policy: r.u8() === 1 ? 'contactsOnly' : 'open',
    contacts: r.vec((x) => x.take(32)), blocked: r.vec((x) => x.take(32)), sentAt: r.vec((x) => x.u64()),
    lastSendMs: r.u64(), sentTotal: r.u64(),
  };
  r.end('profile');
  return p;
}
/** The inbox actor; null if it does not exist (no key published yet). */
export function decodeInbox(bytes) {
  if (!bytes || bytes.length === 0) return null;
  const r = new Reader(bytes);
  const i = {
    owner: r.take(32), nextN: r.u64(),
    entries: r.vec((x) => ({ n: x.u64(), from: x.take(32), seq: x.u64(), atMs: x.u64(), len: x.u32(), contact: x.bool() })),
  };
  r.end('inbox');
  return i;
}
/** An envelope actor; null once deleted. */
export function decodeEnvelope(bytes) {
  if (!bytes || bytes.length === 0) return null;
  const r = new Reader(bytes);
  const e = { recipient: r.take(32), from: r.take(32), seq: r.u64(), atMs: r.u64(), body: r.bytes() };
  r.end('envelope');
  return e;
}
/** ferrata_msg (v1)'s single actor state: key, policy, lists, inbox, counters. */
export function decodeLegacyMailbox(bytes) {
  if (!bytes || bytes.length === 0) return null;
  const r = new Reader(bytes);
  const key = r.option(readKey);
  const policy = r.u8() === 1 ? 'contactsOnly' : 'open';
  const contacts = r.vec((x) => x.take(32));
  const blocked = r.vec((x) => x.take(32));
  const nextSeq = r.u64();
  const inbox = r.vec((x) => ({ seq: x.u64(), from: x.take(32), atMs: x.u64(), body: x.bytes() }));
  const m = { key, policy, contacts, blocked, nextSeq, inbox };
  m.windowStartMs = r.u64(); m.sentInWindow = r.u32(); m.lastSendMs = r.u64(); m.sentTotal = r.u64();
  r.end('mailbox');
  return m;
}

// ---------------------------------------------------------------- the envelope
const isHybrid = (v) => v === V_HYBRID || v === LEGACY_HYBRID;
const isLegacy = (v) => v === LEGACY_X25519 || v === LEGACY_HYBRID;
function headerLen(version) { return 1 + 4 + 32 + (isHybrid(version) ? MLKEM768_CT_LEN : 0) + 24; }
/** The largest text (UTF-8 bytes) an envelope of `version` holds in `bucket` (negative: it does not fit at all). */
export const capacity = (version, bucket) => bucket - headerLen(version) - TAG - PLAIN_HEADER;
export function bucketFor(version, textBytes) {
  const b = BUCKETS.find((n) => capacity(version, n) >= textBytes);
  if (!b) throw new Error(`text too long: ${textBytes} bytes, at most ${capacity(version, BUCKETS.at(-1))}`);
  return b;
}
/**
 * The key and the AEAD's associated data. HKDF-SHA256(ikm = X25519 secret [|| ML-KEM secret], salt, info): v2's info is
 * "envelope" || SHA-256(header || rx || sender || recipient), 40 bytes (any HKDF takes it; review finding L1); v1's was
 * the 1,149-byte header itself, over OpenSSL's 1,024-byte limit.
 */
export function envelopeInfo(version, header, rx, sender, recipient) {
  const transcript = concat(header, rx, id32(sender), id32(recipient));
  return concat(utf8('envelope'), isLegacy(version) ? transcript : sha256(transcript));
}
function keyAndAad(version, header, ss, rx, sender, recipient) {
  const info = envelopeInfo(version, header, rx, sender, recipient);
  return { k: hkdf(sha256, ss, SALT, info, 32), aad: concat(header, sender, recipient) };
}

/**
 * Encrypt `text` from `sender` to `recipient` (account ids) under the recipient's published `key` (as decodeProfile
 * returns it). Hybrid (X25519 + ML-KEM-768) whenever the key has ML-KEM; a fresh ephemeral key per message; padded to
 * the smallest bucket that fits (or `bucket`, if given and large enough). `legacy: true` makes a v1 envelope.
 */
export function seal({ sender, recipient, key, text, sentMs = Date.now(), kind = 0, bucket = null, rand = null, legacy = false }) {
  const random = rand || ((n) => globalThis.crypto.getRandomValues(new Uint8Array(n)));
  const hybrid = !!key.mlkem768;
  const version = legacy ? (hybrid ? LEGACY_HYBRID : LEGACY_X25519) : (hybrid ? V_HYBRID : V_X25519);
  const t = typeof text === 'string' ? utf8(text) : text;
  if (t.length > 0xffff) throw new Error('text too long');
  const size = bucket ?? bucketFor(version, t.length);
  if (!BUCKETS.includes(size) || capacity(version, size) < t.length) throw new Error('bad bucket');
  const esk = random(32);
  const epk = x25519.getPublicKey(esk);
  let ss = x25519.getSharedSecret(esk, key.x25519);
  let kemCt = new Uint8Array(0);
  if (hybrid) {
    const e = ml_kem768.encapsulate(key.mlkem768, random(32));
    kemCt = e.cipherText;
    ss = concat(ss, e.sharedSecret);
  }
  const header = concat(u8(version), u32(key.epoch), epk, kemCt, random(24));
  const { k, aad } = keyAndAad(version, header, ss, key.x25519, sender, recipient);
  const plain = new Uint8Array(size - header.length - TAG);
  plain.set(concat(u8(kind), u64(sentMs), le(t.length, 2), t));
  const nonce = header.subarray(header.length - 24);
  return concat(header, xchacha20poly1305(k, nonce, aad).encrypt(plain));
}

/** Decrypt an envelope (v2's or v1's) with the recipient's derived `keys` of the envelope's epoch. Throws if it does not open. */
export function open({ body, sender, recipient, keys }) {
  const version = body[0];
  if (![V_X25519, V_HYBRID, LEGACY_X25519, LEGACY_HYBRID].includes(version)) throw new Error(`unknown envelope version ${version}`);
  const hl = headerLen(version);
  if (body.length < hl + TAG + PLAIN_HEADER) throw new Error(`a ${body.length}-byte envelope cannot hold a version ${version} header`);
  const epoch = Number(new Reader(body.subarray(1, 5)).u32());
  if (epoch !== keys.epoch) throw new Error(`envelope for key epoch ${epoch}, keys of epoch ${keys.epoch}`);
  const header = body.subarray(0, hl);
  const epk = header.subarray(5, 37);
  let ss = x25519.getSharedSecret(keys.x25519.secretKey, epk);
  if (isHybrid(version)) ss = concat(ss, ml_kem768.decapsulate(header.subarray(37, 37 + MLKEM768_CT_LEN), keys.mlkem768.secretKey));
  const { k, aad } = keyAndAad(version, header, ss, keys.x25519.publicKey, sender, recipient);
  const plain = xchacha20poly1305(k, header.subarray(hl - 24), aad).decrypt(body.subarray(hl));
  const r = new Reader(plain);
  const kind = r.u8();
  const sentMs = r.u64();
  const len = Number(r.uint(2));
  const text = dec.decode(r.take(len));
  if (plain.subarray(PLAIN_HEADER + len).some((b) => b !== 0)) throw new Error('padding is not zero');
  return { kind, sentMs, text, version, bucket: body.length };
}

function opener(recipientId, accountSecret) {
  const keysByEpoch = new Map();
  return (from, body) => {
    const epoch = Number(new Reader(body.subarray(1, 5)).u32());
    if (!keysByEpoch.has(epoch)) keysByEpoch.set(epoch, deriveKeys(accountSecret, epoch));
    return open({ body, sender: from, recipient: recipientId, keys: keysByEpoch.get(epoch) });
  };
}

/**
 * Every message of a v2 inbox, decrypted with keys derived from `accountSecret` (each envelope's epoch). `envelopes`
 * maps an entry's `n` (a string) to its envelope's body. Returns [{ n, from (base58), fromId, seq, atMs, contact,
 * bucket, version, text | error }], oldest first.
 */
export function openInbox({ inbox, envelopes, recipientId, accountSecret }) {
  if (!inbox) return [];
  const openOne = opener(recipientId, accountSecret);
  return inbox.entries.map((e) => {
    const out = { n: e.n, from: base58.encode(e.from), fromId: e.from, seq: e.seq, atMs: e.atMs, contact: e.contact, bucket: e.len };
    const body = envelopes.get(String(e.n));
    if (!body) return { ...out, error: 'envelope not found' };
    try {
      const o = openOne(e.from, body);
      return { ...out, version: o.version, text: o.text, sentMs: o.sentMs, kind: o.kind };
    } catch (err) {
      return { ...out, version: body[0], error: String(err.message || err) };
    }
  });
}

/** Every message of a v1 (legacy) mailbox, decrypted. Returns [{ seq, from, fromId, atMs, bucket, version, text | error }]. */
export function readLegacyInbox(mailbox, recipientId, accountSecret) {
  if (!mailbox) return [];
  const openOne = opener(recipientId, accountSecret);
  return mailbox.inbox.map((m) => {
    const out = { seq: m.seq, from: base58.encode(m.from), fromId: m.from, atMs: m.atMs, bucket: m.body.length, version: m.body[0] };
    try {
      const o = openOne(m.from, m.body);
      return { ...out, text: o.text, sentMs: o.sentMs, kind: o.kind };
    } catch (err) {
      return { ...out, error: String(err.message || err) };
    }
  });
}

export const idToString = (id) => base58.encode(id);
export function idFromString(s) { const b = base58.decode(s.trim()); if (b.length !== 32) throw new Error('an account id is 32 bytes in base58'); return b; }
export { hex, sha256 };

// ---------------------------------------------------------------- preflight (the program's rules, before paying a fee)
function bodyCode(body) {
  if (!BUCKETS.includes(body.length)) return 'BAD_SIZE';
  if (body[0] !== V_X25519 && body[0] !== V_HYBRID) return 'BAD_VERSION';
  if (body[0] === V_HYBRID && body.length < HYBRID_MIN_BUCKET) return 'BAD_SIZE';
  return null;
}
/**
 * The exit code ferrata_msg_v2 would answer `Send { to: recipientId, nowMs, seq, body }` from `senderId` with, given the
 * sender's profile, the recipient's profile and the recipient's inbox (decodeProfile / decodeInbox), or null if it
 * would deliver. Mirrors lez/msg-v2-wire `handle`, in its order.
 */
export function preflightSend({ senderId, recipientId, senderProfile, recipientProfile, recipientInbox, body, nowMs, seq }) {
  const now = BigInt(nowMs);
  const sp = senderProfile ?? decodeProfile(null);
  const rp = recipientProfile ?? decodeProfile(null);
  // Send, at the sender's profile.
  if (sameId(senderId, recipientId)) return 'SELF_SEND';
  if (BigInt(seq) !== sp.sentTotal) return 'STALE_SEQ';
  const b = bodyCode(body);
  if (b) return b;
  if (now < sp.lastSendMs) return 'CLOCK_BACKWARDS';
  if (sp.sentAt.filter((t) => now - t < BigInt(RATE_WINDOW_MS)).length >= RATE_MAX) return 'RATE_LIMITED';
  // Deliver, at the recipient's profile.
  if (!rp.key) return 'NO_KEY';
  if (has(rp.blocked, senderId)) return 'BLOCKED';
  const contact = has(rp.contacts, senderId);
  if (rp.policy === 'contactsOnly' && !contact) return 'NOT_A_CONTACT';
  if (body[0] === V_HYBRID && !rp.key.mlkem768) return 'BAD_VERSION';
  if (body[0] === V_X25519 && rp.key.mlkem768) return 'DOWNGRADE';
  if (new Reader(body.subarray(1, 5)).u32() !== rp.key.epoch) return 'STALE_KEY';
  // Admit, at the recipient's inbox (it exists once a key is published).
  const entries = recipientInbox?.entries ?? [];
  const bytes = entries.reduce((n, e) => n + e.len, 0);
  if (entries.length >= MAX_MESSAGES || bytes + body.length > MAX_INBOX_BYTES) return 'INBOX_FULL';
  if (!contact) {
    if (entries.filter((e) => !e.contact && sameId(e.from, senderId)).length >= STRANGER_UNREAD) return 'STRANGER_LIMIT';
    if (entries.filter((e) => !e.contact).length >= STRANGER_SLOTS) return 'STRANGERS_FULL';
  }
  return null;
}
