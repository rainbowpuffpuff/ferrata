// ferrata_names in JavaScript: resolve a name, reverse-resolve an account, check a name against the rules, and build
// the program's messages for a signer. EXPERIMENTAL, for the LEZ v0.4 test network. No dependencies: SHA-256 from
// WebCrypto (browsers, Node 18+), its own base58 and borsh. The wallet and the explorer import this file as it is.
//
//   import { Names, checkName } from './names.js';
//   const names = new Names({ rpc: 'https://HOST/rpc', program: '<base58 program account id>' });
//   await names.resolve('alice');   // { name, owner, target, receiver, registeredMs, expiresMs } or null
//   await names.reverse('<base58 account id>');  // 'alice' or null (checked against the forward record)
//
// The design, the rules and every refusal: lez/names-wire/src/lib.rs (the program) and names/README.md.

export const MIN_LEN = 3;
export const MAX_LEN = 32;
export const RESERVED = Object.freeze([
  'admin', 'administrator', 'bridge', 'explorer', 'faucet', 'ferrata', 'help', 'lez', 'logos', 'null', 'official',
  'root', 'security', 'sequencer', 'support', 'system', 'treasury', 'validator', 'wallet',
]);
export const MAX_NAMES = 5;
const DAY_MS = 86_400_000;
export const TERM_MS = 180 * DAY_MS;
export const GRACE_MS = 30 * DAY_MS;
export const CLOCK_SLACK_MS = 600_000;
// Commit and reveal: both valid only within COMMIT_SLACK_MS of their nowMs; a reveal from (commit nowMs +
// COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS) to (commit nowMs + MAX_REVEAL_WINDOW_MS).
export const COMMIT_SLACK_MS = 120_000;
export const MIN_REVEAL_DELAY_MS = 120_000;
export const MAX_REVEAL_WINDOW_MS = 86_400_000;
export const NPK_LEN = 32;
export const VPK_LEN = 1184;

/** The program's exit codes (240–254) and what each means to a user. */
export const CODES = Object.freeze({
  230: ['NO_COMMIT', 'commit first: this account has no pending commitment'],
  231: ['COMMIT_MISMATCH', 'the reveal does not match this account\'s commitment (name, salt or account differ)'],
  232: ['TOO_EARLY', 'too early: wait for the commitment\'s reveal window to open'],
  233: ['COMMIT_EXPIRED', 'the commitment has expired (24 h): commit again'],
  240: ['BAD_MESSAGE', 'the request is malformed (or its time is out of range, or a private key has the wrong size)'],
  241: ['BAD_STATE', 'stored state does not decode'],
  242: ['NOT_SIGNED', 'the account did not sign this request'],
  243: ['WRONG_SENDER', 'a message from the wrong sender'],
  244: ['WRONG_RECEIVER', 'a message at the wrong actor'],
  245: ['INVALID_NAME', 'names are 3-32 characters of a-z, 0-9 and -, not starting or ending with -, without --'],
  246: ['RESERVED_NAME', 'this name is reserved'],
  247: ['TAKEN', 'this name is taken'],
  248: ['NOT_OWNER', 'only the owner can do this'],
  249: ['EXPIRED', 'the name has expired: renew it first'],
  250: ['LAPSED', 'the grace period is over: the name is free for anyone to register'],
  251: ['TOO_MANY', `an account holds at most ${MAX_NAMES} names`],
  252: ['NOT_FOUND', 'this name is not registered'],
  253: ['SAME_OWNER', 'the account already owns this name'],
  254: ['NOT_TARGET', 'the name does not point at your account (primary name), or is not offered to you (accept)'],
});
export function describeCode(code) {
  const c = CODES[Number(code)];
  return c ? `${c[0]}: ${c[1]}` : `exit ${code}`;
}

// ---------------------------------------------------------------- names
/** What a user typed, as the program expects it: trimmed, lower case, an optional `.lez` suffix dropped. */
export function normalize(input) {
  return String(input).trim().toLowerCase().replace(/\.lez$/, '');
}
/** Whether `name` follows the rules (a reserved name does). */
export function wellFormed(name) {
  return typeof name === 'string' && name.length >= MIN_LEN && name.length <= MAX_LEN && /^[a-z0-9-]+$/.test(name)
    && !name.startsWith('-') && !name.endsWith('-') && !name.includes('--');
}
/** null if `name` can be registered, else `{ code, reason }` (the program's own refusal). */
export function checkName(name) {
  if (!wellFormed(name)) return { code: 245, reason: CODES[245][1] };
  if (RESERVED.includes(name)) return { code: 246, reason: CODES[246][1] };
  return null;
}

// ---------------------------------------------------------------- bytes
const enc = new TextEncoder();
const dec = new TextDecoder('utf-8', { fatal: true });
export function hex(b) { return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(''); }
export function unhex(s) {
  const t = String(s).replace(/^0x/, '');
  if (t.length % 2 || /[^0-9a-fA-F]/.test(t)) throw new Error('not hex');
  return Uint8Array.from(t.match(/../g) ?? [], (x) => parseInt(x, 16));
}
function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
// WebCrypto: global in browsers and Node 19+; Node 18 has it under node:crypto.
const subtle = async () => globalThis.crypto?.subtle ?? (await import('node:crypto')).webcrypto.subtle;
export async function sha256(bytes) { return new Uint8Array(await (await subtle()).digest('SHA-256', bytes)); }

const B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
export const base58 = {
  encode(bytes) {
    let n = 0n;
    for (const b of bytes) n = n * 256n + BigInt(b);
    let s = '';
    while (n > 0n) { s = B58[Number(n % 58n)] + s; n /= 58n; }
    for (const b of bytes) { if (b !== 0) break; s = '1' + s; }
    return s;
  },
  decode(s) {
    let n = 0n;
    for (const c of s) {
      const i = B58.indexOf(c);
      if (i < 0) throw new Error('not base58');
      n = n * 58n + BigInt(i);
    }
    const out = [];
    while (n > 0n) { out.unshift(Number(n % 256n)); n /= 256n; }
    for (const c of s) { if (c !== '1') break; out.unshift(0); }
    return Uint8Array.from(out);
  },
};
/** An account id from base58 (LEZ's form), hex, or bytes. */
export function accountId(x) {
  if (x instanceof Uint8Array) { if (x.length !== 32) throw new Error('an account id is 32 bytes'); return x; }
  const s = String(x).replace(/^Public\//, '');
  const b = /^(0x)?[0-9a-fA-F]{64}$/.test(s) ? unhex(s) : base58.decode(s);
  if (b.length !== 32) throw new Error(`not an account id: ${s}`);
  return b;
}
export const idString = (b) => base58.encode(b);

class W {
  constructor() { this.parts = []; }
  u8(v) { this.parts.push(Uint8Array.of(v)); return this; }
  u32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); this.parts.push(b); return this; }
  u64(v) { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), true); this.parts.push(b); return this; }
  fixed(b, n) { if (b.length !== n) throw new Error(`expected ${n} bytes`); this.parts.push(b); return this; }
  bytes(b) { return this.u32(b.length).fixed(b, b.length); }
  str(s) { return this.bytes(enc.encode(s)); }
  opt(v, f) { if (v === null || v === undefined) return this.u8(0); this.u8(1); f(v); return this; }
  done() { return concat(...this.parts); }
}
class R {
  constructor(b) { this.b = b; this.o = 0; this.v = new DataView(b.buffer, b.byteOffset, b.byteLength); }
  take(n) { if (this.o + n > this.b.length) throw new Error('truncated'); const s = this.b.slice(this.o, this.o + n); this.o += n; return s; }
  u8() { return this.take(1)[0]; }
  u32() { const v = this.v.getUint32(this.o, true); this.take(4); return v; }
  u64() { const v = this.v.getBigUint64(this.o, true); this.take(8); return Number(v); }
  bytes() { return this.take(this.u32()); }
  str() { return dec.decode(this.bytes()); }
  opt(f) { const t = this.u8(); if (t === 0) return null; if (t !== 1) throw new Error('bad option'); return f(); }
  end() { if (this.o !== this.b.length) throw new Error('trailing bytes'); }
}

// ---------------------------------------------------------------- addresses
const SEED_DOMAIN = enc.encode('ferrata-names/v1/name/');
const COMMIT_DOMAIN = enc.encode('ferrata-names/v1/commit/');
/** `commitment(name, owner, salt)`: sha256("ferrata-names/v1/commit/" || owner || salt || name). */
export async function commitment(name, owner, salt) {
  if (salt.length !== 32) throw new Error('a salt is 32 bytes');
  return sha256(concat(COMMIT_DOMAIN, accountId(owner), salt, enc.encode(name)));
}
/** A fresh 32-byte salt (keep it until the reveal: without it the commitment cannot be revealed). */
export function newSalt() { const b = new Uint8Array(32); globalThis.crypto.getRandomValues(b); return b; }
const PDA_PREFIX = enc.encode('/LEE/v0.2/AccountId/PDA/\0\0\0\0\0\0\0\0'); // lee_core AccountId::for_public_pda
/** `name_seed(name)`: sha256("ferrata-names/v1/name/" || name). */
export async function nameSeed(name) { return sha256(concat(SEED_DOMAIN, enc.encode(name))); }
/** The account id of `name(n)` for the program at `program`. */
export async function nameAccountId(program, name) {
  return sha256(concat(PDA_PREFIX, accountId(program), await nameSeed(name)));
}

// ---------------------------------------------------------------- messages (NamesMessage, borsh)
function writeReceiver(w, r) { w.fixed(r.npk, NPK_LEN).bytes(r.vpk); }
/**
 * `msg`: { kind: 'Register', name, nowMs, target?, receiver? } | { kind: 'Renew', name, nowMs }
 *      | { kind: 'Transfer', name, to, nowMs } | { kind: 'SetTarget', name, target?, receiver?, nowMs }
 *      | { kind: 'Release', name } | { kind: 'SetPrimary', name?, nowMs } | { kind: 'Accept', name, nowMs }
 *      | { kind: 'Commit', commitment, nowMs }   (Register is the reveal: it also carries `salt`, 32 bytes)
 * `Transfer` offers the name to `to`, who takes it with `Accept`; `Transfer` to the owner withdraws the offer.
 * Accounts as base58, hex or bytes; `receiver`: { npk: Uint8Array(32), vpk: Uint8Array(1184) }.
 */
export function encodeMessage(msg) {
  const w = new W();
  const target = (t) => w.fixed(accountId(t), 32);
  switch (msg.kind) {
    case 'Register': w.u8(0).str(msg.name).u64(msg.nowMs).opt(msg.target, target).opt(msg.receiver, (r) => writeReceiver(w, r)).fixed(msg.salt, 32); break;
    case 'Renew': w.u8(1).str(msg.name).u64(msg.nowMs); break;
    case 'Transfer': w.u8(2).str(msg.name).fixed(accountId(msg.to), 32).u64(msg.nowMs); break;
    case 'SetTarget': w.u8(3).str(msg.name).opt(msg.target, target).opt(msg.receiver, (r) => writeReceiver(w, r)).u64(msg.nowMs); break;
    case 'Release': w.u8(4).str(msg.name); break;
    case 'SetPrimary': w.u8(5).opt(msg.name, (n) => w.str(n)).u64(msg.nowMs); break;
    case 'Accept': w.u8(9).str(msg.name).u64(msg.nowMs); break;
    case 'Commit': w.u8(10).fixed(msg.commitment, 32).u64(msg.nowMs); break;
    default: throw new Error(`unknown message ${msg.kind}`);
  }
  return w.done();
}

// ---------------------------------------------------------------- state
function readReceiver(r) { return { npk: r.take(NPK_LEN), vpk: r.bytes() }; }
/** `NameRecord` (the state of `name(n)`); accounts as base58. */
export function decodeRecord(bytes) {
  const r = new R(bytes);
  const out = {
    name: r.str(), owner: idString(r.take(32)), target: r.opt(() => idString(r.take(32))), receiver: r.opt(() => readReceiver(r)),
    registeredMs: r.u64(), expiresMs: r.u64(), offer: r.opt(() => idString(r.take(32))),
  };
  r.end();
  return out;
}
/** `UserState` (the state of `user(a)`). */
export function decodeUser(bytes) {
  if (!bytes || bytes.length === 0) return { owned: 0, primary: null, commit: null };
  const r = new R(bytes);
  const out = { owned: r.u8(), primary: r.opt(() => r.str()),
    commit: r.opt(() => ({ hash: r.take(32), notBeforeMs: r.u64(), expiresMs: r.u64() })) };
  r.end();
  return out;
}
export const EVENT_KINDS = ['Registered', 'Renewed', 'Offered', 'Transferred', 'Updated', 'Released'];
/** The kind of a `ProgramEvent` of this program from its 8-byte selector, or null. */
export async function eventKind(selector) {
  const s = hex(selector);
  for (const k of EVENT_KINDS) if (hex((await sha256(enc.encode(`ferrata_names::${k}`))).slice(0, 8)) === s) return k;
  return null;
}
/** `NameEvent` (a `ProgramEvent`'s data). */
export function decodeEvent(bytes) {
  const r = new R(bytes);
  const out = { name: r.str(), owner: r.opt(() => idString(r.take(32))), target: r.opt(() => idString(r.take(32))), expiresMs: r.u64() };
  r.end();
  return out;
}

/** A record's status at `nowMs`: 'live', 'grace' (expired; only the owner may renew) or 'lapsed' (free to register). */
export function status(record, nowMs = Date.now()) {
  if (!record) return 'free';
  if (nowMs < record.expiresMs) return 'live';
  if (nowMs < record.expiresMs + GRACE_MS) return 'grace';
  return 'lapsed';
}

/** One actor's state from `getAccount` / `getAccountView` JSON (bytes as an array, base64 or hex). */
export function actorStateOf(account, program) {
  if (!account) return new Uint8Array(0);
  const states = account?.data?.actor_states ?? account?.actor_states ?? {};
  const key = idString(accountId(program));
  let v = Array.isArray(states) ? (states.find((e) => e[0] === key) ?? [])[1] : states[key];
  if (v === undefined || v === null) return new Uint8Array(0);
  if (v && typeof v === 'object' && !Array.isArray(v) && 'data' in v) v = v.data;
  if (Array.isArray(v)) return Uint8Array.from(v);
  if (typeof v === 'string') {
    if (/^(0x)?[0-9a-fA-F]*$/.test(v) && v.length % 2 === 0) return unhex(v);
    return Uint8Array.from(atob(v), (c) => c.charCodeAt(0));
  }
  throw new Error('unknown actor state encoding');
}

// ---------------------------------------------------------------- the client
export class Names {
  /**
   * `rpc`: the sequencer's JSON-RPC URL, or an object with `call(method, params)` (lez.js's `Sequencer`).
   * `program`: the names program's account id (base58).
   */
  constructor({ rpc, program, fetch: f = globalThis.fetch }) {
    this.program = accountId(program);
    this.rpc = typeof rpc === 'string' ? { call: (m, p) => jsonRpc(f, rpc, m, p) } : rpc;
  }

  /** The state of `Actor(account, program)`: one actor's view if the sequencer has it, else the whole account. */
  async actorState(account) {
    const id = idString(accountId(account));
    try {
      const v = await this.rpc.call('getAccountView', [{ account_id: id, program_account_id: idString(this.program) }]);
      return actorStateOf(v, this.program);
    } catch (e) {
      if (!/method not found|-32601/i.test(String(e?.message ?? e))) throw e;
      return actorStateOf(await this.rpc.call('getAccount', [id]), this.program);
    }
  }

  /** The record of `name` whatever its status (null if never registered or released). */
  async record(name) {
    const s = await this.actorState(await nameAccountId(this.program, name));
    return s.length ? decodeRecord(s) : null;
  }

  /** What `name` resolves to at `nowMs`: its record while live, else null. Accepts what a user typed (`Alice.lez`). */
  async resolve(input, nowMs = Date.now()) {
    const name = normalize(input);
    if (!wellFormed(name)) return null;
    const r = await this.record(name);
    return r && status(r, nowMs) === 'live' ? r : null;
  }

  /** `account`'s name: its primary name, only if that name is live and points back at `account`. */
  async reverse(account, nowMs = Date.now()) {
    const id = idString(accountId(account));
    const user = decodeUser(await this.actorState(id));
    if (!user.primary) return null;
    const r = await this.resolve(user.primary, nowMs);
    return r && r.target === id ? user.primary : null;
  }

  /**
   * Step 1 of registering: the call that commits to `name` for `signer`, and what the reveal needs later.
   * Keep `salt` until the reveal. `revealFrom` / `revealUntil`: the reveal's nowMs must be in [revealFrom, revealUntil).
   */
  async commitCall(signer, name, { salt = newSalt(), nowMs = Date.now() - 30_000 } = {}) {
    const n = normalize(name);
    const bad = checkName(n);
    if (bad) throw new Error(`${n}: ${bad.reason}`);
    const c = await commitment(n, signer, salt);
    const call = await this.call(signer, { kind: 'Commit', commitment: c, nowMs });
    return { ...call, name: n, salt, commitment: c, nowMs,
      revealFrom: nowMs + COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS, revealUntil: nowMs + MAX_REVEAL_WINDOW_MS };
  }

  /** The account's own state: how many names it holds, its primary name as stored (unchecked), its pending commit. */
  async user(account) { return decodeUser(await this.actorState(account)); }

  /**
   * The call `signer` (base58) signs for `msg` (see encodeMessage; `nowMs` defaults to now less 30 s, so that the
   * block's time is after it and within CLOCK_SLACK_MS): `{ to, actors, data, evidence }` for lez.js's `buildCall`
   * (actors as `{ account, program }` bytes; evidence for a name account that holds no record yet).
   */
  async call(signer, msg) {
    const p = this.program;
    const me = { account: accountId(signer), program: p };
    const m = { ...msg };
    if (m.kind !== 'Release' && m.nowMs === undefined) m.nowMs = Date.now() - 30_000;
    if (m.name !== undefined && m.name !== null) m.name = normalize(m.name);
    if (m.kind === 'Register') {
      const bad = checkName(m.name);
      if (bad) throw new Error(`${m.name}: ${bad.reason}`);
      if (m.target === undefined) m.target = signer;
      if (!m.salt || m.salt.length !== 32) throw new Error('a reveal needs the 32-byte salt of its commitment');
    }
    if (m.kind === 'Commit' && m.commitment?.length !== 32) throw new Error('a commitment is 32 bytes');
    const actors = [me];
    const evidence = [];
    if (m.name) {
      const at = await nameAccountId(p, m.name);
      actors.push({ account: at, program: p });
      const record = await this.record(m.name);
      if (!record) evidence.push({ pda: { program: p, seed: await nameSeed(m.name) } });
      // A lapsed name leaves its old owner's count: their actor is in the transaction.
      if (m.kind === 'Register' && record && record.owner !== idString(me.account)) actors.push({ account: accountId(record.owner), program: p });
      // Accepting debits the old owner.
      if (m.kind === 'Accept' && record) actors.push({ account: accountId(record.owner), program: p });
    }
    return { to: me, actors, data: encodeMessage(m), evidence, message: m };
  }
}

async function jsonRpc(f, url, method, params = []) {
  const res = await f(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
  if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`);
  const j = await res.json();
  if (j.error) throw new Error(`${method}: ${j.error.message} (${j.error.code})`);
  return j.result;
}
