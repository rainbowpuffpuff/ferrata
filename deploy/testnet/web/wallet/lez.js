// LEZ v0.4.0-rc3, public side, for the browser (and node, for the tests): keys as LEZ's own wallet derives them,
// account ids, the borsh encoding of a public transaction, its signing hash, BIP-340 signatures, and the sequencer's
// JSON-RPC. Every layout here is LEZ's (spike/upstream/lez-v0.4.0-rc3); test/conformance.test.mjs checks it against
// LEZ's own pinned test vectors and against transactions LEZ's wallet CLI signed.
import {
  schnorr, secp256k1, sha256, sha512, hmac, base58, base64,
  generateMnemonic, mnemonicToSeedSync, validateMnemonic, englishWordlist, ml_kem768,
} from './vendor/noble.js';

// ---------------------------------------------------------------- bytes
const enc = new TextEncoder();
export const utf8 = (s) => enc.encode(s);
export function hex(bytes) { return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join(''); }
export function unhex(s) {
  const t = s.startsWith('0x') ? s.slice(2) : s;
  if (t.length % 2 || /[^0-9a-fA-F]/.test(t)) throw new Error('not a hex string');
  const out = new Uint8Array(t.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(t.slice(2 * i, 2 * i + 2), 16);
  return out;
}
export function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
export function equalBytes(a, b) { return a.length === b.length && a.every((x, i) => x === b[i]); }
export function compareBytes(a, b) {
  for (let i = 0; i < Math.min(a.length, b.length); i++) if (a[i] !== b[i]) return a[i] - b[i];
  return a.length - b.length;
}
const toBig = (b) => BigInt('0x' + (hex(b) || '0'));
function toBytes32(n) { return unhex(n.toString(16).padStart(64, '0')); }
export function randomBytes(n) { const b = new Uint8Array(n); globalThis.crypto.getRandomValues(b); return b; }

// ---------------------------------------------------------------- borsh (the subset LEZ's public types use)
export class Writer {
  constructor() { this.parts = []; }
  raw(b) { this.parts.push(b instanceof Uint8Array ? b : Uint8Array.from(b)); return this; }
  u8(v) { return this.raw([v]); }
  uint(v, bytes) { // little-endian, `bytes` wide, from a number or bigint
    let n = BigInt(v);
    if (n < 0n || n >= 1n << BigInt(8 * bytes)) throw new Error(`value out of range for u${8 * bytes}`);
    const out = new Uint8Array(bytes);
    for (let i = 0; i < bytes; i++) { out[i] = Number(n & 0xffn); n >>= 8n; }
    return this.raw(out);
  }
  u32(v) { return this.uint(v, 4); }
  u64(v) { return this.uint(v, 8); }
  u128(v) { return this.uint(v, 16); }
  fixed(b, len) { if (b.length !== len) throw new Error(`expected ${len} bytes, got ${b.length}`); return this.raw(b); }
  bytes(b) { return this.u32(b.length).raw(b); } // Vec<u8>
  done() { return concat(...this.parts); }
}

export class Reader {
  constructor(bytes) { this.b = bytes; this.o = 0; }
  take(n) { if (this.o + n > this.b.length) throw new Error('borsh: out of data'); const s = this.b.subarray(this.o, this.o + n); this.o += n; return s; }
  u8() { return this.take(1)[0]; }
  uint(bytes) { const s = this.take(bytes); let n = 0n; for (let i = bytes - 1; i >= 0; i--) n = (n << 8n) | BigInt(s[i]); return n; }
  u32() { return Number(this.uint(4)); }
  u64() { return this.uint(8); }
  u128() { return this.uint(16); }
  bytes() { return this.take(this.u32()); }
  vec(f) { const n = this.u32(); const out = []; for (let i = 0; i < n; i++) out.push(f(this)); return out; }
  option(f) { const t = this.u8(); if (t === 0) return null; if (t === 1) return f(this); throw new Error('borsh: bad option tag'); }
  end() { return this.o === this.b.length; }
}

// ---------------------------------------------------------------- domains (lee/state_machine at rc3)
const pad32 = (s) => { const b = new Uint8Array(32); b.set(utf8(s)); return b; };
const MESSAGE_PUBLIC_PREFIX = pad32('/LEE/v0.3/Message/Public/');      // public_transaction/message.rs
const ACCOUNT_ID_PUBLIC_PREFIX = pad32('/LEE/v0.3/AccountId/Public/'); // signature/public_key.rs
const MASTER_PUBLIC_KEY = utf8('/LEE-Keys/v1/Master/Public');           // key_protocol keys_public.rs
export const NATIVE_TOKEN_PROGRAM_ID = new Uint8Array(32);              // lee_core native_token.rs
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n; // secp256k1 order

// ---------------------------------------------------------------- keys
function scalarFromRepr(b) { const v = toBig(b); if (v >= N) throw new Error('not a canonical scalar'); return v; }
function checkSecret(b) { const v = toBig(b); if (v === 0n || v >= N) throw new Error('invalid private key'); return b; }

/** LEZ's `PrivateKey::tweak`: sk + SHA-256(compressed public key of sk) mod n. */
export function tweak(sk) {
  checkSecret(sk);
  const h = sha256(secp256k1.getPublicKey(sk, true));
  const s = (toBig(sk) + scalarFromRepr(h)) % N;
  if (s === 0n) throw new Error('invalid private key');
  return toBytes32(s);
}

/** The x-only public key of a (tweaked) signing key, as LEZ's `PublicKey::new_from_private_key`. */
export function publicKeyOf(ssk) { return schnorr.getPublicKey(checkSecret(ssk)); }

/** LEZ's `AccountId::from(&PublicKey)`. */
export function accountIdOf(pk) {
  if (pk.length !== 32) throw new Error('a public key is 32 bytes');
  return sha256(concat(ACCOUNT_ID_PUBLIC_PREFIX, pk));
}

function node(sk, cc, path) {
  const ssk = tweak(sk);
  const pk = publicKeyOf(ssk);
  return { sk, cc, path, ssk, pk, accountId: accountIdOf(pk) };
}
/** `ChildKeysPublic::root`. */
export function masterNode(seed) {
  if (seed.length !== 64) throw new Error('a seed is 64 bytes');
  const h = hmac(sha512, MASTER_PUBLIC_KEY, seed);
  return node(checkSecret(h.slice(0, 32)), h.slice(32), []);
}
/** `ChildKeysPublic::nth_child`. */
export function childNode(parent, i) {
  const data = new Uint8Array(37);
  data.set(parent.sk, 1);
  new DataView(data.buffer).setUint32(33, i, false);
  const h = hmac(sha512, parent.cc, data);
  const s = (scalarFromRepr(h.slice(0, 32)) + toBig(parent.sk)) % N;
  return node(checkSecret(toBytes32(s)), h.slice(32), [...parent.path, i]);
}
/** The public account at chain index /i/j/... of a seed (LEZ's CLI: the first public account is /0). */
export function deriveNode(seed, path) { return path.reduce(childNode, masterNode(seed)); }
export function pathString(path) { return '/' + path.join('/'); }
export function parsePath(s) {
  if (!/^\/(\d+(\/\d+)*)?$/.test(s)) throw new Error('a chain index looks like /0 or /0/3');
  return s === '/' ? [] : s.slice(1).split('/').map((x) => { const v = Number(x); if (v > 0xffffffff) throw new Error('index too large'); return v; });
}

export function newMnemonic() { return generateMnemonic(englishWordlist, 256); } // 24 words, as LEZ's wallet
export function isMnemonic(m) { return validateMnemonic(m.trim().toLowerCase().split(/\s+/).join(' '), englishWordlist); }
/** LEZ's wallet: BIP-39 `to_seed("")` (storage/mod.rs). */
export function seedOf(mnemonic) {
  const m = mnemonic.trim().toLowerCase().split(/\s+/).join(' ');
  if (!validateMnemonic(m, englishWordlist)) throw new Error('not a valid BIP-39 English mnemonic');
  return mnemonicToSeedSync(m, '');
}
/** A raw key as LEZ's wallet imports one (`PrivateKey` hex): it signs as itself, no tweak. */
export function rawKeyNode(skHex) {
  const ssk = checkSecret(unhex(skHex.trim()));
  if (ssk.length !== 32) throw new Error('a private key is 32 bytes (64 hex digits)');
  const pk = publicKeyOf(ssk);
  return { ssk, pk, accountId: accountIdOf(pk), path: null };
}

// ---------------------------------------------------------------- account ids and actors
export const idToString = (id) => base58.encode(id);
export function idFromString(s) {
  let t = s.trim();
  if (t.startsWith('Public/')) t = t.slice(7);
  if (t.startsWith('Private/')) throw new Error('a private account cannot receive a public transfer here');
  const b = base58.decode(t);
  if (b.length !== 32) throw new Error('an account id is 32 bytes in base58');
  return b;
}
export const nativeActor = (id) => ({ account: id, program: NATIVE_TOKEN_PROGRAM_ID });
function cmpActor(a, b) { return compareBytes(a.account, b.account) || compareBytes(a.program, b.program); }
function uniqSorted(items, cmp) {
  const s = [...items].sort(cmp);
  return s.filter((x, i) => i === 0 || cmp(s[i - 1], x) !== 0);
}

// ---------------------------------------------------------------- the public transaction
/**
 * `TransactionMessage<PublicExecution>` (lee/state_machine/src/transaction_message.rs, public_transaction/message.rs).
 * m = { actors: [actor], signers: [id], to: actor, data: bytes, fee: {payer, gasLimit, tip, maxFee} | null,
 *       nonces: [[id, nonce]], evidence: [{key: pk32}] }
 */
export function encodeMessage(m) {
  const w = new Writer();
  const actors = uniqSorted(m.actors, cmpActor);
  w.u32(actors.length);
  for (const a of actors) w.fixed(a.account, 32).fixed(a.program, 32);
  const signers = uniqSorted(m.signers, compareBytes);
  w.u32(signers.length);
  for (const s of signers) w.fixed(s, 32);
  w.u32(0); // promotions: a public transaction selects none
  w.fixed(m.to.account, 32).fixed(m.to.program, 32).bytes(m.data);
  if (m.fee) {
    w.u8(1).fixed(m.fee.payer, 32).u64(m.fee.gasLimit).u64(m.fee.tip).u128(m.fee.maxFee);
  } else w.u8(0);
  const nonces = [...m.nonces].sort((a, b) => compareBytes(a[0], b[0]));
  w.u32(nonces.length);
  for (const [id, n] of nonces) w.fixed(id, 32).u128(n);
  w.u32(m.evidence.length);
  for (const e of m.evidence) {
    if (e.key) w.u8(0).fixed(e.key, 32);                                  // PublicAccountEvidence::Key
    else w.u8(1).fixed(e.pda.program, 32).fixed(e.pda.seed, 32);          // PublicAccountEvidence::Pda
  }
  return w.done();
}
export function messageHash(messageBytes) { return sha256(concat(MESSAGE_PUBLIC_PREFIX, messageBytes)); }

/** `Signature::new`: BIP-340 over the 32-byte message hash, fresh aux randomness unless given. */
export function sign(ssk, hash32, aux = randomBytes(32)) { return schnorr.sign(hash32, ssk, aux); }
export function verify(sig, hash32, pk) { return schnorr.verify(sig, hash32, pk); }

/** `PublicTransaction` borsh (message, then the witness set's (signature, public key) pairs). */
export function encodePublicTransaction(messageBytes, witnesses) {
  const w = new Writer().raw(messageBytes).u32(witnesses.length);
  for (const { sig, pk } of witnesses) w.fixed(sig, 64).fixed(pk, 32);
  return w.done();
}
/** `LeeTransaction::Public(tx)` as `sendTransaction` takes it: base64 of borsh. Hash: SHA-256 of the PublicTransaction. */
export function wrapPublic(txBytes) { return { wire: base64.encode(concat(Uint8Array.of(0), txBytes)), hash: hex(sha256(txBytes)), size: txBytes.length + 1 }; }

/** native_token::Message::Transfer { to, amount } (variant 0). */
export function transferData(to, amount) { return new Writer().u8(0).fixed(to, 32).u128(amount).done(); }

/**
 * A signed native transfer from `from` (a node with ssk, pk, accountId) to `to` (an account id), at `nonce`.
 * `toKey`: the recipient's public key when the recipient account does not exist yet (LEZ's admission evidence).
 */
export function buildTransfer({ from, to, toKey = null, amount, nonce, gasLimit, tip = 0n, maxFee, aux }) {
  if (toKey && !equalBytes(accountIdOf(toKey), to)) throw new Error('the public key is not the recipient account\'s');
  if (equalBytes(from.accountId, to)) throw new Error('sender and recipient are the same account');
  const m = {
    actors: [nativeActor(from.accountId), nativeActor(to)],
    signers: [from.accountId],
    to: nativeActor(from.accountId),
    data: transferData(to, amount),
    fee: { payer: from.accountId, gasLimit, tip, maxFee },
    nonces: [[from.accountId, nonce]],
    evidence: toKey ? [{ key: toKey }] : [],
  };
  const messageBytes = encodeMessage(m);
  const h = messageHash(messageBytes);
  const sig = sign(from.ssk, h, aux);
  const tx = encodePublicTransaction(messageBytes, [{ sig, pk: from.pk }]);
  return { ...wrapPublic(tx), messageHash: hex(h), messageBytes, txBytes: tx };
}

/**
 * A signed call of a program at `to` (an actor), paid by `payer` (a node), which co-signs without being an actor (as
 * LEZ's wallet does for a payer outside the message's accounts). `actors`: the public actors the call touches.
 */
export function buildCall({ payer, to, actors = [to], data, nonce, gasLimit, tip = 0n, maxFee, evidence = [], aux }) {
  const messageBytes = encodeMessage({ actors, signers: [payer.accountId], to, data,
    fee: { payer: payer.accountId, gasLimit, tip, maxFee }, nonces: [[payer.accountId, nonce]], evidence });
  const h = messageHash(messageBytes);
  const tx = encodePublicTransaction(messageBytes, [{ sig: sign(payer.ssk, h, aux), pk: payer.pk }]);
  return { ...wrapPublic(tx), messageHash: hex(h), messageBytes, txBytes: tx };
}

/** An account's actor state at `program` from getAccount's JSON (bytes as a JSON array, or base64). */
export function actorStateOf(account, program) {
  const states = account?.data?.actor_states ?? {};
  const v = states[idToString(program)];
  if (v === undefined || v === null) return null;
  if (Array.isArray(v)) return Uint8Array.from(v);
  if (typeof v === 'string') return base64.decode(v);
  throw new Error('unknown actor state encoding');
}

// ---------------------------------------------------------------- decoding (for history: LEZ's public transactions)
function readActor(r) { return { account: r.take(32), program: r.take(32) }; }
// The privacy-preserving transaction (lee privacy_preserving_transaction/, lee_core circuit_io.rs): read in full so
// that a block's later transactions can be found (borsh puts no length before a vector's elements).
const readNote = (r) => ({ ciphertext: r.bytes(), epk: r.bytes() });             // EncryptedNote
const readEnvelope = (r) => ({ from: readActor(r), to: readActor(r), message: r.bytes() });
function readDelivery(r) {
  return { envelope: readEnvelope(r), inherited: r.vec((x) => x.take(32)), inheritsEntry: r.u8() !== 0, pdaSeeds: r.vec((x) => x.take(32)) };
}
function readBoundary(r) {
  return r.vec((x) => {
    const t = x.u8();
    if (t === 0) return { privateToPublic: readDelivery(x) };
    if (t === 1) return { publicToPrivate: readDelivery(x) };
    if (t === 2 || t === 3) return { end: t === 2 ? 'private' : 'public' };
    throw new Error('unknown boundary step');
  });
}
const readAction = (r) => ({ nullifier: r.take(32), root: r.take(32), commitment: r.take(32), note: readNote(r) });
const readWindow = (r) => ({ from: r.option((x) => x.u64()), to: r.option((x) => x.u64()) });
function readProvenExecution(r) {
  return {
    boundary: readBoundary(r),
    asyncMessages: r.vec((x) => ({ commitment: x.take(32), note: readNote(x) })),
    recoveryBindings: r.vec((x) => ({ address: x.take(32), note: readNote(x) })),
    publicRoot: r.option((x) => ({ to: readActor(x), message: x.bytes() })),
    privateActions: r.vec(readAction),
    validity: { blocks: readWindow(r), timestamps: readWindow(r) },
    imageClaims: r.vec((x) => {
      const t = x.u8();
      if (t === 0) return { disclosed: { account: x.take(32), image: x.take(32) } };
      if (t === 1) return { undisclosed: x.take(32) };
      throw new Error('unknown program image claim');
    }),
    fee: r.option((x) => ({ refundTo: x.option((y) => y.take(32)),
      fallback: x.option((y) => ({ action: readAction(y), boundary: readBoundary(y) })) })),
  };
}
function readEvidence(r) {
  const t = r.u8();
  if (t === 0) return { key: r.take(32) };
  if (t === 1) return { pda: { program: r.take(32), seed: r.take(32) } };
  throw new Error('unknown admission evidence');
}
function readContext(r) { return { actors: r.vec(readActor), signers: r.vec((x) => x.take(32)), promotions: r.vec((x) => x.u64()) }; }
function readPublicTx(r) {
  const start = r.o;
  const c = readContext(r);
  const to = readActor(r), data = r.bytes();
  const fee = r.option((x) => ({ payer: x.take(32), gasLimit: x.u64(), tip: x.u64(), maxFee: x.u128() }));
  const nonces = r.vec((x) => [x.take(32), x.u128()]);
  const evidence = r.vec(readEvidence);
  const messageEnd = r.o;
  const witnesses = r.vec((x) => ({ sig: x.take(64), pk: x.take(32) }));
  return { ...c, to, data, fee, nonces, evidence, witnesses, messageBytes: r.b.subarray(start, messageEnd), bytes: r.b.subarray(start, r.o) };
}
function readPrivateTx(r) {
  const start = r.o;
  const c = readContext(r);
  const execution = readProvenExecution(r);
  const nonces = r.vec((x) => [x.take(32), x.u128()]);
  const evidence = r.vec(readEvidence);
  const witnesses = r.vec((x) => ({ sig: x.take(64), pk: x.take(32) }));
  const proof = r.bytes();
  return { ...c, execution, nonces, evidence, witnesses, proofSize: proof.length, bytes: r.b.subarray(start, r.o) };
}
function readLeeTransaction(r) {
  const t = r.u8();
  if (t === 0) { const tx = readPublicTx(r); return { kind: 'public', tx, hash: hex(sha256(tx.bytes)) }; }
  if (t === 1) { const tx = readPrivateTx(r); return { kind: 'private', tx, hash: hex(sha256(tx.bytes)) }; }
  throw new Error('unknown transaction kind');
}
export function decodePublicTransaction(bytes) {
  const r = new Reader(bytes); const tx = readPublicTx(r);
  if (!r.end()) throw new Error('trailing bytes after a public transaction');
  return tx;
}
/** A LeeTransaction from its base64 wire form: { kind: 'public' | 'private', tx, hash }. */
export function decodeWire(b64) {
  const r = new Reader(base64.decode(b64)); const d = readLeeTransaction(r);
  if (!r.end()) throw new Error('trailing bytes after a transaction');
  return d;
}
const STATUS = ['Pending', 'Safe', 'Finalized'];
const BLOCK_PREFIX = concat(utf8('/LEE/v0.3/Message/Block/'), new Uint8Array(8));
/**
 * LEZ rc3's block hash recomputed from the block's own bytes (`HashableBlockData::compute_hash`, block.rs): SHA-256 of
 * the 32-byte prefix and the borsh of (id, previous hash, timestamp, transactions, reverted), which is the served
 * block without its own hash field (bytes 40..72) and without its Bedrock status (the last byte).
 */
export function blockHashOf(bytes) {
  return hex(sha256(concat(BLOCK_PREFIX, bytes.subarray(0, 40), bytes.subarray(72, bytes.length - 1))));
}
/**
 * A block as getBlock / getBlockRange serve it (base64 of borsh `Block`, lez/common/src/block.rs at rc3). `hashOk`: its
 * hash recomputed from its contents equals the hash it carries.
 */
export function decodeBlock(b64) {
  const bytes = base64.decode(b64);
  const r = new Reader(bytes);
  const header = { id: Number(r.u64()), prevHash: hex(r.take(32)), hash: hex(r.take(32)), timestamp: Number(r.u64()) };
  const transactions = r.vec(readLeeTransaction);
  const reverted = r.vec((x) => Number(x.u64()));
  const status = STATUS[r.u8()] ?? 'unknown';
  if (!r.end()) throw new Error('trailing bytes after a block');
  const computedHash = blockHashOf(bytes);
  return { header, transactions, reverted, status, computedHash, hashOk: computedHash === header.hash };
}

/** The native transfer a public transaction makes, if it is one. */
export function nativeTransferOf(tx) {
  if (!equalBytes(tx.to.program, NATIVE_TOKEN_PROGRAM_ID) || tx.data.length !== 49 || tx.data[0] !== 0) return null;
  const r = new Reader(tx.data); r.u8();
  return { from: tx.to.account, to: r.take(32), amount: r.u128() };
}

// ---------------------------------------------------------------- the sequencer's JSON-RPC
export class RpcError extends Error {
  constructor(method, error) { super(`${method}: ${error.message}${error.data ? ' (' + JSON.stringify(error.data) + ')' : ''}`); this.code = error.code; this.data = error.data; }
}
export class Sequencer {
  constructor(url, { timeoutMs = 15000 } = {}) { this.url = url; this.timeoutMs = timeoutMs; this.id = 0; }
  // A public gateway limits requests per IP (HTTP 429, Retry-After) and may answer 502/503 while the sequencer restarts:
  // the call waits and tries again (at most 4 times). Nothing reached the sequencer on such an answer, so a resend of
  // sendTransaction is safe; a transaction that did reach it keeps its hash and nonce, so LEZ applies it once.
  async call(method, params = []) {
    for (let attempt = 0; ; attempt++) {
      const ctl = new AbortController();
      const t = setTimeout(() => ctl.abort(), this.timeoutMs);
      let res;
      try {
        res = await fetch(this.url, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, signal: ctl.signal,
          body: JSON.stringify({ jsonrpc: '2.0', id: ++this.id, method, params }),
        });
        if ((res.status === 429 || res.status === 502 || res.status === 503) && attempt < 4) {
          const ra = Number(res.headers.get('Retry-After'));
          await new Promise((r) => setTimeout(r, Math.min(10000, (ra > 0 ? ra * 1000 : 0) || 600 * 2 ** attempt)));
          continue;
        }
        if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`);
        const j = await res.json();
        if (j.error) throw new RpcError(method, j.error);
        return j.result;
      } finally { clearTimeout(t); }
    }
  }
  lastBlockId() { return this.call('getLastBlockId'); }
  feeState() { return this.call('getFeeState'); }
  account(id) { return this.call('getAccount', [idToString(id)]); }
  balance(id) { return this.call('getAccountBalance', [idToString(id)]); }
  nonces(ids) { return this.call('getAccountsNonces', [ids.map(idToString)]); }
  block(n) { return this.call('getBlock', [n]); }
  blockRange(a, b) { return this.call('getBlockRange', [a, b]); }
  transaction(hash) { return this.call('getTransaction', [hash]); }
  programIds() { return this.call('getProgramIds'); }
  channelId() { return this.call('getChannelId'); }
  send(wire) { return this.call('sendTransaction', [wire]); }
}

// JSON numbers above 2^53 lose precision: read a u128 that may come as a number or a string.
/**
 * An amount typed by a person, in units of `decimals` (e.g. "1.5"), as a bigint of atomic units. A comma is refused:
 * "1,5" (a decimal comma) and "1,000" (a thousands separator) read differently, so neither is guessed.
 */
export function parseUnits(s, decimals) {
  const D = Number(decimals), trimmed = String(s).trim();
  // A space (or any blank, such as a narrow no-break space) inside the amount is refused, as a comma is: "1 5" is not
  // read as 15, nor "1 000" as 1000.
  if (/\s/.test(trimmed)) throw new Error('write the amount without spaces (1500, or 1.5 with a point)');
  const t = trimmed.replace(/_/g, '');
  if (t.includes(',')) throw new Error('use a point for decimals (1.5) and no commas');
  if (!/^\d+(\.\d+)?$/.test(t)) throw new Error('enter an amount such as 1.5');
  const [w, f = ''] = t.split('.');
  if (f.length > D) throw new Error(`at most ${D} decimals`);
  const v = BigInt(w) * 10n ** BigInt(D) + BigInt((f + '0'.repeat(D)).slice(0, D) || '0');
  if (v <= 0n) throw new Error('the amount must be more than zero');
  return v;
}
/** A transaction's outcome in a decoded block: 'applied', 'reverted', or 'missing' (the block does not list it). */
export function outcomeIn(block, hash) {
  const pos = block.transactions.findIndex((d) => d.hash === hash);
  if (pos < 0) return 'missing';
  return block.reverted.includes(pos) ? 'reverted' : 'applied';
}
export function big(v) { return typeof v === 'bigint' ? v : BigInt(typeof v === 'number' ? Math.round(v) : String(v)); }

/** Fee fields for a public transaction of `size` bytes: reserve at the head, and a max_fee with headroom. */
export function feeFor({ gasLimit, size, tip = 0n, feeState, headroom = 8n }) {
  const be = BigInt(feeState.base_fee_exec), bs = BigInt(feeState.base_fee_stor);
  const at = (e, s) => BigInt(gasLimit) * e + BigInt(size) * s + tip;
  const reserve = at(be, bs);
  // The day-one kit's rule (repo lez/testnet04/NOTES.md §3): price max_fee at the larger of 64 and headroom x base.
  const ce = be * headroom > 64n ? be * headroom : 64n;
  const cs = bs * headroom > 64n ? bs * headroom : 64n;
  return { reserve, maxFee: at(ce, cs) };
}

/** A RISC Zero image id as LEZ's RPC serves it ([u32; 8]) in the usual hex form (each word little-endian). */
export function programIdHex(words) {
  if (typeof words === 'string') return words.replace(/^0x/, '');
  return words.map((w) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, Number(w), true); return hex(b); }).join('');
}

// ---------------------------------------------------------------- private account keys (key_protocol keys_private.rs)
// The keys of LEZ's private accounts from the same phrase: the spending key tree under /LEE-Keys/v1/Master/Private, the
// authorization and nullifier keys, the ML-KEM-768 viewing key (FIPS 203 from d ‖ z), and the private account id. The
// page derives and shows them; spending a private account needs a proof of LEZ's privacy circuit (private.js).
const be32 = (i) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, i, false); return b; };
const PRIVATE_ACCOUNT_ID_PREFIX = pad32('/LEE/v0.3/AccountId/Private/');
function privateNode(ssk, ccc, cci) {
  const idx = be32(cci ?? 0);
  const ask = sha256(concat(utf8('/LEE-Keys/v1/Authorization/Secret'), ssk, idx));
  const vseed = hmac(sha512, utf8('/LEE-Keys/v1/Viewing/Seed'), concat(utf8('/LEE-Keys/v1/Viewing/Secret'), ssk, idx));
  const d = vseed.slice(0, 32), z = vseed.slice(32);
  const nsk = sha256(concat(utf8('/LEE-Keys/v1/Nullifier/Secret'), ask));
  const npk = sha256(concat(utf8('/LEE-Keys/v1/Nullifier/Public'), nsk));
  const vpk = ml_kem768.keygen(concat(d, z)).publicKey;
  return { ssk, ccc, cci, ask, nsk, npk, d, z, vpk, accountId: sha256(concat(PRIVATE_ACCOUNT_ID_PREFIX, npk, vpk)) };
}
/** `ChildKeysPrivate::root`. */
export function privateMaster(seed) {
  const h = hmac(sha512, utf8('/LEE-Keys/v1/Master/Private'), seed);
  return privateNode(h.slice(0, 32), h.slice(32), null);
}
/** `ChildKeysPrivate::nth_child`. */
export function privateChild(parent, cci) {
  const pt = sha256(concat(utf8('/LEE-Keys/v1/Parent/Private'), parent.nsk, parent.d, parent.z));
  const h = hmac(sha512, parent.ccc, concat(utf8('/LEE-Keys/v1/Seed/Private'), pt, be32(cci)));
  return privateNode(h.slice(0, 32), h.slice(32), cci);
}
/** The private account at /i of a phrase's seed, as LEZ's wallet creates it (the first is /0). */
export const derivePrivate = (seed, path) => path.reduce(privateChild, privateMaster(seed));

/** Re-exported for the messages client (msg/client.js reads base64 actor states with it). */
export { base64 };
