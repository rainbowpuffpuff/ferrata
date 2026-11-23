// ferrata_names v2 in JavaScript: resolve a name, reverse-resolve an account, check a name against the rules, check a
// reveal before sending it, and build the program's messages for a signer. EXPERIMENTAL, for the LEZ v0.4 test network.
// No dependencies: SHA-256 from WebCrypto (browsers, Node 18+), its own base58 and borsh. The wallet and the explorer
// import this file as it is. v2 is a new program (a new image id and address): v1's records are not read here.
//
//   import { Names, checkName } from './names.js';
//   const names = new Names({ rpc: 'https://HOST/rpc', program: '<base58 program account id>' });
//   await names.resolve('alice');   // { name, owner, target, receiver, registeredMs, expiresMs, ... } or null
//   await names.reverse('<base58 account id>');  // 'alice' or null (checked against the forward record)
//
// The RPC is trusted for what it returns (LEZ's RPC serves public state without proofs): give `rpc` as a list of
// independent endpoints to refuse any answer they disagree on. Show the resolved account before sending to it.
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
export const MAX_REVEAL_WINDOW_MS = 3_600_000;
// A registration can be taken by a reveal of an older commitment until registeredMs + CONTEST_MS.
export const CONTEST_MS = MAX_REVEAL_WINDOW_MS;
export const NPK_LEN = 32;
export const VPK_LEN = 1184;

/**
 * The execution gas each action is sent with (CLI and wallet: `gasFor`): the worst case measured for the action
 * (lez/results/names-worst.md: 32-byte names of six shapes, records with a private receiver and an offer, full lists,
 * a 32-byte primary name, a pending commitment, a reveal that displaces a registration) with about 25 % more, rounded
 * up. lez/harness/tests/names.rs (`worst_cases_fit_the_clients_gas`) reads this table and fails if a worst case does
 * not fit. A transaction that runs out of gas is charged its whole limit; the limit also sets the fee reserve.
 */
export const GAS = Object.freeze({
  Commit: 60_000, //      worst measured 42,098
  Register: 190_000, //   148,966 (the reveal that displaces a registration)
  Renew: 70_000, //       54,534
  Transfer: 70_000, //    55,775
  SetTarget: 100_000, //  74,078
  Accept: 90_000, //      70,802
  SetPrimary: 90_000, //  67,177
  Release: 90_000, //     68,527
  ForgetBase: 20_000, //  Forget of n names: 70,473 / 128,500 / 186,102 / 243,549 / 300,583 for n = 1..5
  ForgetPerName: 72_000,
});
/** The gas limit to send `msg` with (see GAS). */
export function gasFor(msg) {
  if (msg.kind === 'Forget') return GAS.ForgetBase + GAS.ForgetPerName * msg.names.length;
  const g = GAS[msg.kind];
  if (!g) throw new Error(`no gas limit for ${msg.kind}`);
  return g;
}

/** The program's exit codes (230–236, 240–254) and what each means to a user. */
export const CODES = Object.freeze({
  230: ['NO_COMMIT', 'commit first: this account has no pending commitment'],
  231: ['COMMIT_MISMATCH', 'the reveal does not match this account\'s commitment (name, salt or account differ)'],
  232: ['TOO_EARLY', 'too early: wait for the commitment\'s reveal window to open'],
  233: ['COMMIT_EXPIRED', 'the commitment has expired (1 h): commit again'],
  234: ['CONFUSABLE', 'a look-alike of this name is registered (0/o, 1/l, rn/m, vv/w read the same)'],
  235: ['STILL_HELD', 'this name is still yours: release it instead of forgetting it'],
  236: ['NOT_HELD', 'this name is not in your list'],
  240: ['BAD_MESSAGE', 'the request is malformed (or its time is out of range, or a private key has the wrong size)'],
  241: ['BAD_STATE', 'stored state does not decode'],
  242: ['NOT_SIGNED', 'the account did not sign this request'],
  243: ['WRONG_SENDER', 'a message from the wrong sender'],
  244: ['WRONG_RECEIVER', 'a message at the wrong actor'],
  245: ['INVALID_NAME', 'names are 3-32 characters of a-z, 0-9 and -, not starting or ending with -, without --'],
  246: ['RESERVED_NAME', 'this name is reserved, or a variant of a reserved name'],
  247: ['TAKEN', 'this name is taken'],
  248: ['NOT_OWNER', 'only the owner can do this'],
  249: ['EXPIRED', 'the name has expired (or would by the time the request lands): renew it first'],
  250: ['LAPSED', 'the grace period is over (or would be by the time the request lands): the name is free for anyone to register'],
  251: ['TOO_MANY', `an account holds at most ${MAX_NAMES} names (forget the ones that left you)`],
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
/** What a name looks like, for uniqueness: 0 → o, 1 → l, then rn → m, then vv → w (the program's `skeleton`). */
export function skeleton(name) {
  return String(name).replace(/0/g, 'o').replace(/1/g, 'l').replaceAll('rn', 'm').replaceAll('vv', 'w');
}
const RESERVED_SKELETONS = new Set(RESERVED.map(skeleton));
/** A reserved name or a variant: a `-`-separated part with a reserved skeleton, as it is or without its edge digits. */
export function isReserved(name) {
  return String(name).split('-').some((part) => {
    const core = part.replace(/^[0-9]+|[0-9]+$/g, '');
    return RESERVED_SKELETONS.has(skeleton(part)) || (core !== '' && RESERVED_SKELETONS.has(skeleton(core)));
  });
}
/** null if `name` can be registered, else `{ code, reason }` (the program's own refusal). */
export function checkName(name) {
  if (!wellFormed(name)) return { code: 245, reason: CODES[245][1] };
  if (isReserved(name)) return { code: 246, reason: CODES[246][1] };
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
const SEED_DOMAIN = enc.encode('ferrata-names/v2/name/');
const COMMIT_DOMAIN = enc.encode('ferrata-names/v2/commit/');
const SEQUENCE_DOMAIN = enc.encode('ferrata-names/v2/sequence');
/** `commitment(name, owner, salt)`: sha256("ferrata-names/v2/commit/" || owner || salt || name). */
export async function commitment(name, owner, salt) {
  if (salt.length !== 32) throw new Error('a salt is 32 bytes');
  return sha256(concat(COMMIT_DOMAIN, accountId(owner), salt, enc.encode(name)));
}
/** A fresh 32-byte salt (keep it until the reveal: without it the commitment cannot be revealed). */
export function newSalt() { const b = new Uint8Array(32); globalThis.crypto.getRandomValues(b); return b; }
const PDA_PREFIX = enc.encode('/LEE/v0.2/AccountId/PDA/\0\0\0\0\0\0\0\0'); // lee_core AccountId::for_public_pda
/** `name_seed(name)`: sha256("ferrata-names/v2/name/" || skeleton(name)): look-alikes share it. */
export async function nameSeed(name) { return sha256(concat(SEED_DOMAIN, enc.encode(skeleton(name)))); }
/** The account id of `name(n)` for the program at `program`. */
export async function nameAccountId(program, name) {
  return sha256(concat(PDA_PREFIX, accountId(program), await nameSeed(name)));
}
/** `sequence_seed()`: sha256("ferrata-names/v2/sequence"); the counter that numbers commitments. */
export async function sequenceSeed() { return sha256(SEQUENCE_DOMAIN); }
export async function sequenceAccountId(program) {
  return sha256(concat(PDA_PREFIX, accountId(program), await sequenceSeed()));
}

// ---------------------------------------------------------------- messages (NamesMessage, borsh)
function writeReceiver(w, r) { w.fixed(r.npk, NPK_LEN).bytes(r.vpk); }
/**
 * `msg`: { kind: 'Register', name, nowMs, target?, receiver?, salt } (the reveal) | { kind: 'Renew', name, nowMs }
 *      | { kind: 'Transfer', name, to, nowMs } | { kind: 'SetTarget', name, target?, receiver?, nowMs }
 *      | { kind: 'Release', name } | { kind: 'SetPrimary', name?, nowMs } | { kind: 'Accept', name, nowMs }
 *      | { kind: 'Commit', commitment, nowMs } | { kind: 'Forget', names }
 * `Transfer` offers the name to `to`, who takes it with `Accept`; `Transfer` to the owner withdraws the offer.
 * `Forget` drops names that left the signer without it (accepted by another, registered after they lapsed, or taken
 * by an older commitment). The program's own messages (Credit … Sequenced) encode too, for tests.
 * Accounts as base58, hex or bytes; `receiver`: { npk: Uint8Array(32), vpk: Uint8Array(1184) }.
 */
export function encodeMessage(msg) {
  const w = new W();
  const target = (t) => w.fixed(accountId(t), 32);
  const receiver = (r) => writeReceiver(w, r);
  switch (msg.kind) {
    case 'Register': w.u8(0).str(msg.name).u64(msg.nowMs).opt(msg.target, target).opt(msg.receiver, receiver).fixed(msg.salt, 32); break;
    case 'Renew': w.u8(1).str(msg.name).u64(msg.nowMs); break;
    case 'Transfer': w.u8(2).str(msg.name).fixed(accountId(msg.to), 32).u64(msg.nowMs); break;
    case 'SetTarget': w.u8(3).str(msg.name).opt(msg.target, target).opt(msg.receiver, receiver).u64(msg.nowMs); break;
    case 'Release': w.u8(4).str(msg.name); break;
    case 'SetPrimary': w.u8(5).opt(msg.name, (n) => w.str(n)).u64(msg.nowMs); break;
    case 'Accept': w.u8(6).str(msg.name).u64(msg.nowMs); break;
    case 'Commit': w.u8(7).fixed(msg.commitment, 32).u64(msg.nowMs); break;
    case 'Forget': w.u8(8).u32(msg.names.length); for (const n of msg.names) w.str(n); break;
    case 'Credit': w.u8(9).str(msg.name); break;
    case 'Debit': w.u8(10).str(msg.name); break;
    case 'Primary': w.u8(11).str(msg.name); break;
    case 'Claim': w.u8(12).str(msg.name).u64(msg.nowMs).opt(msg.target, target).opt(msg.receiver, receiver).u64(msg.seq); break;
    case 'Disown': w.u8(13).str(msg.name); break;
    case 'Tick': w.u8(14); break;
    case 'Sequenced': w.u8(15).u64(msg.seq); break;
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
    registeredMs: r.u64(), expiresMs: r.u64(), offer: r.opt(() => idString(r.take(32))), commitSeq: r.u64(),
  };
  r.end();
  return out;
}
/** A state at an account under the names program that is not an account's own (a name's record, the counter). */
export class NotAnAccount extends Error {}
/**
 * `UserState` (the state of `user(a)`): the names it holds (and those that left it until it forgets them). Throws
 * `NotAnAccount` for bytes that are not one (the state of a name's or the counter's account read as an account's).
 */
export function decodeUser(bytes) {
  if (!bytes || bytes.length === 0) return { names: [], primary: null, commit: null };
  try {
    const r = new R(bytes);
    const n = r.u32();
    if (n > MAX_NAMES) throw new Error('too many names');
    const names = [];
    for (let i = 0; i < n; i++) names.push(r.str());
    const out = { names, primary: r.opt(() => r.str()),
      commit: r.opt(() => ({ hash: r.take(32), seq: r.u64(), notBeforeMs: r.u64(), expiresMs: r.u64() })) };
    r.end();
    return out;
  } catch (e) {
    throw new NotAnAccount(`not an account's state under the names program (a name's or the counter's account?): ${e.message}`);
  }
}
export const EVENT_KINDS = ['Registered', 'Displaced', 'Renewed', 'Offered', 'Transferred', 'Updated', 'Released'];
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
/** From when no older commitment can take the name; before it, a registration is provisional. */
export function finalAt(record) { return record.registeredMs + CONTEST_MS; }

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
   * `rpc`: the sequencer's JSON-RPC URL, an object with `call(method, params)` (lez.js's `Sequencer`), or a list of
   * them: then every read goes to each, and an answer they disagree on is refused.
   * `program`: the names program's account id (base58).
   */
  constructor({ rpc, program, fetch: f = globalThis.fetch }) {
    this.program = accountId(program);
    const one = (x) => (typeof x === 'string' ? { call: (m, p) => jsonRpc(f, x, m, p) } : x);
    this.rpcs = (Array.isArray(rpc) ? rpc : [rpc]).map(one);
    if (this.rpcs.length === 0) throw new Error('no RPC');
    this.rpc = this.rpcs[0];
  }

  /** The state of `Actor(account, program)` from every RPC (one actor's view, else the whole account); must agree. */
  async actorState(account) {
    const id = idString(accountId(account));
    const read = async (rpc) => {
      try {
        return actorStateOf(await rpc.call('getAccountView', [{ account_id: id, program_account_id: idString(this.program) }]), this.program);
      } catch (e) {
        if (!/method not found|-32601/i.test(String(e?.message ?? e))) throw e;
        return actorStateOf(await rpc.call('getAccount', [id]), this.program);
      }
    };
    const states = await Promise.all(this.rpcs.map(read));
    if (states.some((s) => hex(s) !== hex(states[0]))) throw new Error(`the RPCs disagree about ${id}: not trusting either`);
    return states[0];
  }

  /** The record at `name`'s address (the name itself or a look-alike of it), whatever its status; null if none. */
  async record(name) {
    const s = await this.actorState(await nameAccountId(this.program, name));
    return s.length ? decodeRecord(s) : null;
  }

  /** What `name` resolves to at `nowMs`: its record while live, else null. Accepts what a user typed (`Alice.lez`). */
  async resolve(input, nowMs = Date.now()) {
    const name = normalize(input);
    if (!wellFormed(name)) return null;
    const r = await this.record(name);
    return r && r.name === name && status(r, nowMs) === 'live' ? r : null;
  }

  /**
   * `account`'s name: its primary name, only if that name is live and points back at `account`. One of the program's
   * own accounts (a name's or the counter's: its state there is not an account's) has none.
   */
  async reverse(account, nowMs = Date.now()) {
    const id = idString(accountId(account));
    let user;
    try { user = decodeUser(await this.actorState(id)); } catch (e) { if (e instanceof NotAnAccount) return null; throw e; }
    if (!user.primary) return null;
    const r = await this.resolve(user.primary, nowMs);
    return r && r.target === id ? user.primary : null;
  }

  /**
   * The account's own state: the names in its list, its primary name as stored (unchecked), its pending commit.
   * Throws `NotAnAccount` for one of the program's own accounts (a name's or the counter's).
   */
  async user(account) { return decodeUser(await this.actorState(account)); }

  /** The names in `account`'s list that are no longer its own: what `Forget` takes. */
  async lostNames(account) {
    const id = idString(accountId(account));
    const lost = [];
    for (const n of (await this.user(id)).names) {
      const r = await this.record(n);
      if (!(r && r.name === n && r.owner === id)) lost.push(n);
    }
    return lost;
  }

  /**
   * Step 1 of registering: the call that commits to `name` for `signer`, and what the reveal needs later.
   * Keep `salt` until the reveal. The reveal's nowMs must be in [revealFrom, revealUntil - COMMIT_SLACK_MS].
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

  /**
   * Everything that would refuse `signer`'s reveal of `name` with `salt` at `nowMs`, checked before it is sent: a reveal
   * that is refused or reverts publishes the name. `{ ok, problems: [{code, reason}], lost }`, `lost` the names
   * `Forget` would free when the list is full.
   */
  async preflightReveal(signer, { name, salt, nowMs = Date.now() - 30_000 }) {
    const id = idString(accountId(signer));
    const n = normalize(name);
    const problems = [];
    const add = (code, extra = '') => problems.push({ code, reason: CODES[code][1] + extra });
    const bad = checkName(n);
    if (bad) { problems.push(bad); return { ok: false, problems, lost: [] }; }
    const user = await this.user(id);
    const c = user.commit;
    if (!c) add(230);
    else {
      if (hex(c.hash) !== hex(await commitment(n, id, salt))) add(231);
      if (nowMs < c.notBeforeMs) add(232, ` (opens in ${Math.ceil((c.notBeforeMs - nowMs) / 1000)} s)`);
      if (nowMs + COMMIT_SLACK_MS > c.expiresMs) add(233);
    }
    const r = await this.record(n);
    if (r) {
      const lapsed = nowMs >= r.expiresMs + GRACE_MS;
      const older = c && c.seq > 0 && c.seq < r.commitSeq && nowMs + COMMIT_SLACK_MS <= finalAt(r);
      if (!lapsed && !older) add(r.name === n ? 247 : 234);
    }
    let lost = [];
    if (!user.names.includes(n) && user.names.length + 1 > MAX_NAMES) {
      lost = await this.lostNames(id);
      if (user.names.length - lost.length + 1 > MAX_NAMES) add(251);
    }
    return { ok: problems.length === 0, problems, lost };
  }

  /**
   * The call `signer` (base58) signs for `msg` (see encodeMessage; `nowMs` defaults to now less 30 s, so that the
   * block's time is after it and within its window): `{ to, actors, data, evidence }` for lez.js's `buildCall`
   * (actors as `{ account, program }` bytes; PDA evidence for a name or counter account that holds no state yet).
   * No transaction needs the actor of a name's previous owner.
   */
  async call(signer, msg) {
    const p = this.program;
    const me = { account: accountId(signer), program: p };
    const m = { ...msg };
    if (!['Release', 'Forget'].includes(m.kind) && m.nowMs === undefined) m.nowMs = Date.now() - 30_000;
    if (m.name !== undefined && m.name !== null) m.name = normalize(m.name);
    if (m.kind === 'Register') {
      const bad = checkName(m.name);
      if (bad) throw new Error(`${m.name}: ${bad.reason}`);
      if (m.target === undefined) m.target = signer;
      if (!m.salt || m.salt.length !== 32) throw new Error('a reveal needs the 32-byte salt of its commitment');
    }
    if (m.kind === 'Commit' && m.commitment?.length !== 32) throw new Error('a commitment is 32 bytes');
    if (m.kind === 'Forget') {
      m.names = m.names.map(normalize);
      if (m.names.length === 0 || m.names.length > MAX_NAMES) throw new Error(`forget 1 to ${MAX_NAMES} names`);
    }
    const actors = [me];
    const evidence = [];
    const pda = async (account, seed) => {
      if (actors.some((a) => hex(a.account) === hex(account))) return;
      actors.push({ account, program: p });
      if ((await this.actorState(account)).length === 0) evidence.push({ pda: { program: p, seed } });
    };
    if (m.kind === 'Commit') await pda(await sequenceAccountId(p), await sequenceSeed());
    const names = m.kind === 'Forget' ? m.names : (m.name ? [m.name] : []);
    for (const n of names) await pda(await nameAccountId(p, n), await nameSeed(n));
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
