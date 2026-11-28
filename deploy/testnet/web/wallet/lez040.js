// LEZ v0.4.0 (commit a255b42c), public side: the post-rc3 key tree, per-nonce key rotation, actor authorities,
// keyed BIP-340 witnesses, both transaction kinds, receipt-bearing blocks, and the sequencer JSON-RPC. The layouts
// below follow ../upstream/lez-v0.4.0; test/conformance040.test.mjs checks them against LEZ's own known answers and
// the vectors made by deploy/testnet/web/lez040-vectors.
import {
  hex, unhex, concat, utf8, equalBytes, compareBytes, Writer, Reader, randomBytes,
  idToString, idFromString, parseUnits, big, RpcError, programIdHex,
  nativeActor, NATIVE_TOKEN_PROGRAM_ID, pathString, parsePath, sign, verify,
  Sequencer as BaseSequencer,
} from './lez.js';
import { schnorr, sha256, sha512, hmac, base64 } from './vendor/noble.js';

export {
  hex, unhex, concat, utf8, equalBytes, compareBytes, Writer, Reader, randomBytes,
  idToString, idFromString, parseUnits, big, RpcError, programIdHex,
  nativeActor, NATIVE_TOKEN_PROGRAM_ID, pathString, parsePath, sign, verify, base64,
};

// ---------------------------------------------------------------- key tree and per-nonce keys
const pad32 = (s) => { const b = new Uint8Array(32); b.set(utf8(s)); return b; };
const MASTER_PUBLIC = utf8('/LEE-Keys/v1/Master/Public');
const CHILD_PUBLIC = utf8('/LEE-Keys/v1/Child/Public');
const DS_KEY_SEED = pad32('/LEE/v0.3/KeySeed/');
const DS_KEYGEN = pad32('/LEE/v0.3/KeyGen/Schnorr/');
const DS_KEY_COMMITMENT = pad32('/LEE/v0.3/KeyCommitment/Schnorr/');
const PUBLIC_ACCOUNT_PREFIX = pad32('/LEE/v0.3/AccountId/Public/');
const PUBLIC_SCHNORR_PREFIX = pad32('/LEE/v0.3/AccountId/Public/Schn/');
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;

/** lee/key_protocol/src/key_management/key_tree/keys_public.rs `ChildKeysPublic::root`. */
export function masterSecret(seed64) {
  if (seed64.length !== 64) throw new Error('a seed is 64 bytes');
  return hmac(sha512, MASTER_PUBLIC, seed64).slice(0, 32);
}

/** keys_public.rs `ChildKeysPublic::nth_child`: hardened, HMAC-SHA-512 child-secret derivation. */
export function childSecret(parentSecret, cci) {
  if (parentSecret.length !== 32) throw new Error('an account secret is 32 bytes');
  if (!Number.isInteger(cci) || cci < 0 || cci > 0xffffffff) throw new Error('chain index out of range');
  const index = new Uint8Array(4);
  new DataView(index.buffer).setUint32(0, cci, false);
  return hmac(sha512, parentSecret, concat(CHILD_PUBLIC, index)).slice(0, 32);
}

/** keys_public.rs `ChildKeysPublic::{derive_child,account_id}`: an account at a chain-index path. */
export function deriveNode(seed, path) {
  const secret = path.reduce(childSecret, masterSecret(seed));
  return { secret, path: [...path], accountId: publicAccountId(secret) };
}

/** signature/signing_keys.rs `key_seed`; keyType is 0 or 'schnorr', and nonce is a u128. */
export function keySeed(secret, keyType, nonce) {
  if (secret.length !== 32) throw new Error('an account secret is 32 bytes');
  const t = keyType === 'schnorr' ? 0 : keyType;
  if (t !== 0) throw new Error('unknown signing key type');
  return sha256(concat(DS_KEY_SEED, secret, Uint8Array.of(0), new Writer().u128(nonce).done()));
}

/** signature/schnorr.rs `Schnorr::keygen`: first valid scalar among its 256 hash candidates. */
export function schnorrKeygen(seed32) {
  if (seed32.length !== 32) throw new Error('a key-generation seed is 32 bytes');
  for (let counter = 0; counter <= 255; counter++) {
    const sk = sha256(concat(DS_KEYGEN, seed32, Uint8Array.of(counter)));
    const scalar = BigInt('0x' + hex(sk));
    if (scalar !== 0n && scalar < N) return { sk, pk: schnorr.getPublicKey(sk) };
  }
  throw new Error('one of 256 candidates must be a valid scalar');
}

/** signature/signing_keys.rs `signing_key::<Schnorr>`. */
export function signingKey(secret, nonce) { return schnorrKeygen(keySeed(secret, 'schnorr', nonce)); }

/** signature/schnorr.rs `Schnorr::commit`: the bunkered public-key commitment. */
export function keyCommitment(pk) {
  if (pk.length !== 32) throw new Error('a public key is 32 bytes');
  return sha256(concat(DS_KEY_COMMITMENT, pk));
}

/** lee/state_machine/core/src/signing_key.rs `SigningKeyState` borsh (Schnorr byte 0, commitment). */
export function keyState(secret, nonce) { return concat(Uint8Array.of(0), keyCommitment(signingKey(secret, nonce).pk)); }

/** signature/signing_keys.rs `initial_key`: the nonce-zero Schnorr key state. */
export function initialKey(secret) { return keyState(secret, 0n); }

/** lee/state_machine/core/src/account.rs `AccountId::for_initial_key`, including its nested hash. */
export function accountIdForInitialKey(keyState33) {
  if (keyState33.length !== 33 || keyState33[0] !== 0) throw new Error('not a Schnorr initial key state');
  const keyHash = sha256(concat(PUBLIC_SCHNORR_PREFIX, keyState33.subarray(1)));
  return sha256(concat(PUBLIC_ACCOUNT_PREFIX, keyHash));
}

/** signature/signing_keys.rs `public_account_id`: an account id is that of its nonce-zero key. */
export function publicAccountId(secret) { return accountIdForInitialKey(initialKey(secret)); }

// ---------------------------------------------------------------- messages and keyed signing hashes
const DOMAINS = {
  public: { prefix: pad32('/LEE/v0.3/Message/Public/'), keyed: pad32('/LEE/v0.3/KeyedMessage/Public/') },
  privacy: { prefix: pad32('/LEE/v0.3/Message/Privacy/'), keyed: pad32('/LEE/v0.3/KeyedMessage/Privacy/') },
};

/** transaction_message.rs `MessageDomain` and public/privacy message.rs: select one domain pair. */
function domainOf(domain) {
  if (domain === 'private' || domain === 'privacy-preserving') domain = 'privacy';
  const d = DOMAINS[domain];
  if (!d) throw new Error("message domain must be 'public' or 'privacy-preserving'");
  return d;
}

/** core/src/account.rs `Actor` ordering: account id, then program account id. */
function cmpActor(a, b) { return compareBytes(a.account, b.account) || compareBytes(a.program, b.program); }

/** core/src/execution_state.rs `PublicExecutionContext`: borsh set/map ordering and deduplication. */
function uniqSorted(items, cmp) {
  const sorted = [...items].sort(cmp);
  return sorted.filter((x, i) => i === 0 || cmp(sorted[i - 1], x) !== 0);
}

/** transaction_message.rs `actor_map` / `nonce_map`: a key given twice with another value is refused. */
function refuseConflicts(items, keyOf, valueOf, what) {
  const seen = new Map();
  for (const it of items) {
    const k = keyOf(it), v = valueOf(it);
    if (seen.has(k) && seen.get(k) !== v) throw new Error(`refused before signing: ${what}`);
    seen.set(k, v);
  }
}

/** core/src/execution_state.rs `Delivery` and messages: write one actor's two account ids. */
function writeActor(w, a) { return w.fixed(a.account, 32).fixed(a.program, 32); }

/** core/src/execution_state.rs `Authority`: None is 0 and Signer is 1. */
function authorityByte(authority) {
  if (authority === 'none') return 0;
  if (authority === 'signer') return 1;
  throw new Error("actor authority must be 'none' or 'signer'");
}

/**
 * transaction_message.rs `TransactionMessage`, public_transaction/message.rs `PublicExecution`, and
 * core/src/execution_state.rs `PublicExecutionContext`: the complete common public-context borsh layout.
 * `authorized_accounts` is derived from the sorted nonce map; the fee follows the nonces in v0.4.0.
 */
export function encodeMessage(m) {
  const w = new Writer();
  // LEZ's actor_map and nonce_map refuse a key given twice: an identical repeat is dropped here (the map's bytes are
  // the same), a conflicting one (another authority, another nonce) is refused before anything is signed.
  refuseConflicts(m.actors ?? [], (a) => hex(a.actor.account) + hex(a.actor.program), (a) => a.authority, 'an actor is declared twice with different authorities');
  refuseConflicts(m.nonces ?? [], (n) => hex(n[0]), (n) => String(big(n[1])), 'an account is given two different nonces');
  const actors = uniqSorted(m.actors ?? [], (a, b) => cmpActor(a.actor, b.actor));
  w.u32(actors.length);
  for (const a of actors) { writeActor(w, a.actor); w.u8(authorityByte(a.authority)); }
  const nonces = uniqSorted(m.nonces ?? [], (a, b) => compareBytes(a[0], b[0]));
  w.u32(nonces.length);
  for (const [id] of nonces) w.fixed(id, 32);
  const promotions = [...new Set((m.promotions ?? []).map(big))].sort((a, b) => a < b ? -1 : a > b ? 1 : 0);
  w.u32(promotions.length);
  for (const p of promotions) w.u64(p);
  const privateSigners = uniqSorted(m.privateSigners ?? [], cmpActor);
  w.u32(privateSigners.length);
  for (const a of privateSigners) writeActor(w, a);
  writeActor(w, m.to);
  w.bytes(m.data);
  w.u32(nonces.length);
  for (const [id, nonce] of nonces) w.fixed(id, 32).u128(nonce);
  if (m.fee) w.u8(1).fixed(m.fee.payer, 32).u64(m.fee.gasLimit).u64(m.fee.tip).u128(m.fee.maxFee);
  else w.u8(0);
  w.u32((m.evidence ?? []).length);
  for (const e of m.evidence ?? []) {
    if (e.key) w.u8(0).fixed(e.key, 33);                             // transaction_message.rs PublicAccountEvidence::Key
    else if (e.pda) w.u8(1).fixed(e.pda.program, 32).fixed(e.pda.seed, 32); // PublicAccountEvidence::Pda
    else throw new Error('unknown admission evidence');
  }
  return w.done();
}

/** transaction_message.rs `TransactionMessage::hash_under` and both message.rs `PREFIX` constants. */
export function messageHash(bytes, domain) { return sha256(concat(domainOf(domain).prefix, bytes)); }

/** transaction_message.rs `hash_with_keys`: KEYED_PREFIX || ordinary message hash || 33-byte next states. */
export function messageWithKeys(bytes, nextKeys, domain) {
  return sha256(concat(domainOf(domain).keyed, messageHash(bytes, domain), ...nextKeys.map((k) => {
    if (k.length !== 33) throw new Error('a signing key state is 33 bytes');
    return k;
  })));
}

// ---------------------------------------------------------------- witnesses and public transactions
/** signature/signing_keys.rs `Signer::new`: its sort id, current key, optional initial state, and next state. */
function signerState(s) {
  const nonce = big(s.nonce);
  if (nonce === (1n << 128n) - 1n) throw new Error('the next public-account nonce overflows u128');
  return {
    ...s, nonce, accountId: publicAccountId(s.secret), current: signingKey(s.secret, nonce),
    next: keyState(s.secret, nonce + 1n),
  };
}

/**
 * public_transaction/witness_set.rs `sign_entries` and signature/signing_keys.rs `Signer::{auth_data,next}`.
 * One entry per signer in account-id order, all signatures over the same hash of every next key.
 */
export function signEntries(signers, messageBytes, domain, aux) {
  const sorted = signers.map(signerState).sort((a, b) => compareBytes(a.accountId, b.accountId));
  const h = messageWithKeys(messageBytes, sorted.map((s) => s.next), domain);
  return sorted.map((s, i) => {
    const a = aux instanceof Uint8Array || aux === undefined ? aux : aux[i];
    const authData = concat(s.nonce === 0n ? initialKey(s.secret) : new Uint8Array(),
      sign(s.current.sk, h, a), s.current.pk);
    return { auth_data: authData, next: s.next };
  });
}

/** public_transaction/witness_set.rs `WitnessEntry` and transaction.rs `PublicTransaction` borsh. */
export function encodePublicTransaction(messageBytes, entries) {
  const w = new Writer().raw(messageBytes).u32(entries.length);
  for (const e of entries) w.bytes(e.auth_data).fixed(e.next, 33);
  return w.done();
}

/** lez/common/src/transaction.rs `LeeTransaction::Public`: tag 0, wire base64, and inner-tx SHA-256. */
export function wrapPublic(txBytes) {
  return { wire: base64.encode(concat(Uint8Array.of(0), txBytes)), hash: hex(sha256(txBytes)), size: txBytes.length + 1 };
}

/** lee/state_machine/core/src/native_token.rs `Message::Transfer` (variant 0, destination, u128 amount). */
export function transferData(to, amount) { return new Writer().u8(0).fixed(to, 32).u128(amount).done(); }

/**
 * lez/wallet/src/program_facades/native_token_transfer/mod.rs `credit_destination` and
 * lez/wallet/src/lib.rs `send_tx_with_pre_check`/`send_public`: confirmed for two public accounts, no outstanding
 * async delivery, and a normal owned sender: root = sender's native actor, sender authority Signer, recipient None,
 * only sender's nonce/signature; an absent, non-signing public recipient supplies its initial-key evidence.
 * Not derivable from this synchronous API: the wallet's RPC state checks, fee measurement, private/deferred-delivery
 * branch, keycard credentials, and whether the recipient is actually absent. `toKey` is the caller's declaration of
 * that last fact. This builder intentionally supplies the already measured fee fields and only the public branch.
 */
export function buildTransfer({ from, to, toKey = null, amount, nonce, gasLimit, tip = 0n, maxFee, aux }) {
  if (toKey && !equalBytes(accountIdForInitialKey(toKey), to)) throw new Error('the initial key is not the recipient account\'s');
  if (!equalBytes(from.accountId, publicAccountId(from.secret))) throw new Error('the sender secret is not the sender account\'s');
  if (equalBytes(from.accountId, to)) throw new Error('sender and recipient are the same account');
  const messageBytes = encodeMessage({
    actors: [
      { actor: nativeActor(from.accountId), authority: 'signer' },
      { actor: nativeActor(to), authority: 'none' },
    ],
    to: nativeActor(from.accountId), data: transferData(to, amount),
    nonces: [[from.accountId, nonce]],
    fee: { payer: from.accountId, gasLimit, tip, maxFee },
    evidence: toKey ? [{ key: toKey }] : [],
  });
  const entries = signEntries([{ secret: from.secret, nonce }], messageBytes, 'public', aux);
  const h = messageHash(messageBytes, 'public');
  const keyed = messageWithKeys(messageBytes, entries.map((e) => e.next), 'public');
  const tx = encodePublicTransaction(messageBytes, entries);
  return { ...wrapPublic(tx), messageHash: hex(h), messageWithKeys: hex(keyed), messageBytes, entries, txBytes: tx };
}

/**
 * lez/wallet/src/lib.rs `send_public`: one external fee payer may co-sign without gaining actor authority.
 * `actors` accepts {actor, authority} entries; a bare actor defaults to Signer for the payer and None otherwise.
 */
export function buildCall({ payer, to, actors = [to], data, nonce, gasLimit, tip = 0n, maxFee, evidence = [], aux }) {
  if (payer.secret && !equalBytes(payer.accountId, publicAccountId(payer.secret))) throw new Error('the payer secret is not the payer account\'s');
  const declared = actors.map((a) => a.actor ? { ...a } : {
    actor: a, authority: equalBytes(a.account, payer.accountId) ? 'signer' : 'none',
  });
  const messageBytes = encodeMessage({ actors: declared, to, data, nonces: [[payer.accountId, nonce]],
    fee: { payer: payer.accountId, gasLimit, tip, maxFee }, evidence });
  const entries = signEntries([{ secret: payer.secret, nonce }], messageBytes, 'public', aux);
  const tx = encodePublicTransaction(messageBytes, entries);
  return { ...wrapPublic(tx), messageHash: hex(messageHash(messageBytes, 'public')),
    messageWithKeys: hex(messageWithKeys(messageBytes, entries.map((e) => e.next), 'public')),
    messageBytes, entries, txBytes: tx };
}

// ---------------------------------------------------------------- decoding: both transaction kinds and blocks
/** core/src/account.rs `Actor` borsh. */
function readActor(r) { return { account: r.take(32), program: r.take(32) }; }

/** core/src/execution_state.rs `Authority` borsh and its refusal of an unknown discriminant. */
function readAuthority(r) {
  const t = r.u8();
  if (t === 0) return 'none';
  if (t === 1) return 'signer';
  throw new Error('unknown actor authority');
}

/** core/src/execution_state.rs `PublicExecutionContext` with its actor/authority map. */
function readContext(r) {
  return {
    actors: r.vec((x) => ({ actor: readActor(x), authority: readAuthority(x) })),
    authorizedAccounts: r.vec((x) => x.take(32)),
    promotions: r.vec((x) => x.u64()),
    privateSigners: r.vec(readActor),
  };
}

/** transaction_message.rs `PublicAccountEvidence` (a v0.4.0 key is 33 bytes, not a raw public key). */
function readEvidence(r) {
  const t = r.u8();
  if (t === 0) {
    const key = r.take(33);
    if (key[0] !== 0) throw new Error('unknown signing key type');
    return { key };
  }
  if (t === 1) return { pda: { program: r.take(32), seed: r.take(32) } };
  throw new Error('unknown admission evidence');
}

/** public_transaction/witness_set.rs `WitnessEntry`: variable auth_data then a SigningKeyState. */
function readWitnessEntry(r) {
  const authData = r.bytes();
  const type = r.u8();
  if (type !== 0) throw new Error('unknown signing key type');
  return { auth_data: authData, next: concat(Uint8Array.of(type), r.take(32)) };
}

/** transaction_message.rs `FeeDeclaration`: payer, u64 gas/tip, u128 maximum fee. */
function readFee(r) {
  return r.option((x) => ({ payer: x.take(32), gasLimit: x.u64(), tip: x.u64(), maxFee: x.u128() }));
}

/** public_transaction/message.rs `Message` and transaction.rs `PublicTransaction` borsh. */
function readPublicTx(r) {
  const start = r.o;
  const context = readContext(r);
  const to = readActor(r), data = r.bytes();
  const nonces = r.vec((x) => [x.take(32), x.u128()]);
  const fee = readFee(r);
  const evidence = r.vec(readEvidence);
  const messageEnd = r.o;
  const entries = r.vec(readWitnessEntry);
  return { ...context, to, data, nonces, fee, evidence, entries,
    messageBytes: r.b.subarray(start, messageEnd), bytes: r.b.subarray(start, r.o) };
}

/** core/src/encryption/mod.rs `EncryptedNote`: two byte vectors (ciphertext and ephemeral public key). */
function readNote(r) { return { ciphertext: r.bytes(), epk: r.bytes() }; }

/** core/src/program/mod.rs `MessageEnvelope<Actor>`. */
function readEnvelope(r) { return { from: readActor(r), to: readActor(r), message: r.bytes() }; }

/** core/src/execution_state.rs `Delivery`: envelope, grants, and PDA seeds. */
function readDelivery(r) {
  return { envelope: readEnvelope(r), grants: r.vec(readActor), pdaSeeds: r.vec((x) => x.take(32)) };
}

/** core/src/execution_state.rs `BoundaryStep`, including both subtree-end variants. */
function readBoundary(r) {
  return r.vec((x) => {
    const t = x.u8();
    if (t === 0) return { privateToPublic: readDelivery(x) };
    if (t === 1) return { publicToPrivate: readDelivery(x) };
    if (t === 2 || t === 3) return { end: t === 2 ? 'private' : 'public' };
    throw new Error('unknown boundary step');
  });
}

/** core/src/circuit_io.rs `PrivateAction`: nullifier, roots, commitment, and post-state note. */
function readAction(r) {
  return { nullifier: r.take(32), root: r.take(32), commitment: r.take(32), encryptedPostState: readNote(r) };
}

/** core/src/program/mod.rs `ValidityWindow<T>` for u64 block ids or timestamps. */
function readWindow(r) { return { from: r.option((x) => x.u64()), to: r.option((x) => x.u64()) }; }

/** core/src/circuit_io.rs `FeeFallback`: separate ciphertext vector, not an EncryptedNote. */
function readFallback(r) {
  return { nullifier: r.take(32), root: r.take(32), commitment: r.take(32),
    ciphertext: r.bytes(), boundary: readBoundary(r) };
}

/**
 * core/src/circuit_io.rs `ProvenExecution`, program/mod.rs `Publication`, sealing.rs `SealedMessage`, and
 * recovery.rs `RecoveryBinding`: all fields, including clear publications and the v0.4.0 proven-fee tip.
 */
function readProvenExecution(r) {
  return {
    boundary: readBoundary(r),
    asyncMessages: r.vec((x) => {
      const t = x.u8();
      if (t === 0) return { clear: readEnvelope(x) };
      if (t === 1) return { sealed: { commitment: x.take(32), note: readNote(x) } };
      throw new Error('unknown publication kind');
    }),
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
    provenFee: r.option((x) => ({ refundTo: x.option((y) => y.take(32)), tip: x.u64(),
      fallback: x.option(readFallback) })),
  };
}

/** privacy_preserving_transaction/message.rs `Message`, witness_set.rs `WitnessSet`, and circuit/mod.rs `Proof`. */
function readPrivateTx(r) {
  const start = r.o;
  const context = readContext(r);
  const execution = readProvenExecution(r);
  const nonces = r.vec((x) => [x.take(32), x.u128()]);
  const fee = readFee(r);
  const evidence = r.vec(readEvidence);
  const messageEnd = r.o;
  const entries = r.vec(readWitnessEntry);
  const proof = r.bytes();
  return { ...context, execution, nonces, fee, evidence, entries, proof, proofSize: proof.length,
    messageBytes: r.b.subarray(start, messageEnd), bytes: r.b.subarray(start, r.o) };
}

/** lez/common/src/transaction.rs `LeeTransaction`: Public is tag 0, PrivacyPreserving tag 1. */
function readLeeTransaction(r) {
  const t = r.u8();
  if (t === 0) { const tx = readPublicTx(r); return { kind: 'public', tx, hash: hex(sha256(tx.bytes)) }; }
  if (t === 1) { const tx = readPrivateTx(r); return { kind: 'private', tx, hash: hex(sha256(tx.bytes)) }; }
  throw new Error('unknown transaction kind');
}

/** lee/state_machine/src/encoding/public_transaction.rs `PublicTransaction::from_bytes`, with no trailing bytes. */
export function decodePublicTransaction(bytes) {
  const r = new Reader(bytes), tx = readPublicTx(r);
  if (!r.end()) throw new Error('trailing bytes after a public transaction');
  return tx;
}

/** lez/common/src/transaction.rs `LeeTransaction` borsh-base64 deserialization, consuming every byte. */
export function decodeWire(b64) {
  const r = new Reader(base64.decode(b64)), d = readLeeTransaction(r);
  if (!r.end()) throw new Error('trailing bytes after a transaction');
  return d;
}

const STATUS = ['Pending', 'Safe', 'Finalized'];
const BLOCK_PREFIX = pad32('/LEE/v0.3/Message/Block/');

/**
 * lez/common/src/block.rs `HashableBlockData::compute_hash`: SHA-256(PREFIX || borsh(hashable data)). In a served
 * Block those are bytes 0..40 and 72..(end-1): the stored hash (40..72) and Bedrock status are the only omissions.
 */
export function blockHashOf(bytes) {
  if (bytes.length < 89) throw new Error('a block is too short');
  return hex(sha256(concat(BLOCK_PREFIX, bytes.subarray(0, 40), bytes.subarray(72, bytes.length - 1))));
}

/** lez/common/src/block.rs `TransactionReceipt` and `FeeReceipt`; every numeric field stays a bigint. */
function readReceipt(r) {
  const success = r.u8();
  if (success > 1) throw new Error('invalid receipt success boolean');
  return { success: success === 1, fee: {
    gasExec: r.u64(), gasStor: r.u64(), feeBase: r.u128(), tip: r.u128(), refund: r.u128(),
  } };
}

/** lez/common/src/block.rs `Block`: header, body transactions/receipts, and Bedrock status. */
export function decodeBlock(b64) {
  const bytes = base64.decode(b64), r = new Reader(bytes);
  const header = { id: r.u64(), prevHash: hex(r.take(32)), hash: hex(r.take(32)), timestamp: r.u64() };
  const transactions = r.vec(readLeeTransaction);
  const receipts = r.vec(readReceipt);
  const statusTag = r.u8();
  if (statusTag >= STATUS.length) throw new Error('unknown Bedrock status');
  if (!r.end()) throw new Error('trailing bytes after a block');
  const computedHash = blockHashOf(bytes);
  const reverted = receipts.flatMap((receipt, i) => receipt.success ? [] : [i]);
  return { header, transactions, receipts, reverted, status: STATUS[statusTag], computedHash,
    hashOk: computedHash === header.hash, bytes };
}

/** core/src/native_token.rs `Message::Transfer`: recognize only an exact native Transfer payload. */
export function nativeTransferOf(tx) {
  if (!equalBytes(tx.to.program, NATIVE_TOKEN_PROGRAM_ID) || tx.data.length !== 49 || tx.data[0] !== 0) return null;
  const r = new Reader(tx.data); r.u8();
  return { from: tx.to.account, to: r.take(32), amount: r.u128() };
}

/** lez/common/src/block.rs `BlockBody::is_reverted`; trailing system transactions have no receipt. */
export function outcomeIn(block, hash) {
  const pos = block.transactions.findIndex((d) => d.hash === hash);
  if (pos < 0) return 'missing';
  if (pos >= block.receipts.length) return 'system';
  return block.receipts[pos].success ? 'applied' : 'reverted';
}

// ---------------------------------------------------------------- JSON integers and sequencer RPC
/** lez/sequencer/service/rpc/src/lib.rs JSON responses: JSON whitespace, for the small lossless parser. */
function jsonSpace(text, i) {
  while (text[i] === ' ' || text[i] === '\t' || text[i] === '\n' || text[i] === '\r') i++;
  return i;
}

/** JSON string grammar: delegate escape and Unicode semantics to JSON.parse on the exact token. */
function jsonString(text, i) {
  if (text[i] !== '"') throw new SyntaxError('expected a JSON string');
  for (let j = i + 1; j < text.length; j++) {
    if (text[j] === '"') return { value: JSON.parse(text.slice(i, j + 1)), i: j + 1 };
    if (text[j] === '\\') j++;
    else if (text.charCodeAt(j) < 0x20) throw new SyntaxError('control character in a JSON string');
  }
  throw new SyntaxError('unterminated JSON string');
}

const JSON_NUMBER = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;

/** JSON values: convert only plain integer tokens with more than 15 digits to bigint (no copy of the rest). */
function jsonValue(text, start) {
  const i = jsonSpace(text, start);
  if (text.startsWith('true', i)) return { value: true, i: i + 4 };
  if (text.startsWith('false', i)) return { value: false, i: i + 5 };
  if (text.startsWith('null', i)) return { value: null, i: i + 4 };
  if (text[i] === '"') return jsonString(text, i);
  if (text[i] === '[') {
    const out = []; let p = jsonSpace(text, i + 1);
    if (text[p] === ']') return { value: out, i: p + 1 };
    for (;;) {
      const v = jsonValue(text, p); out.push(v.value); p = jsonSpace(text, v.i);
      if (text[p] === ']') return { value: out, i: p + 1 };
      if (text[p] !== ',') throw new SyntaxError('expected comma or closing bracket');
      p = jsonSpace(text, p + 1);
    }
  }
  if (text[i] === '{') {
    const out = {}; let p = jsonSpace(text, i + 1);
    if (text[p] === '}') return { value: out, i: p + 1 };
    for (;;) {
      const k = jsonString(text, p); p = jsonSpace(text, k.i);
      if (text[p] !== ':') throw new SyntaxError('expected colon');
      const v = jsonValue(text, p + 1);
      Object.defineProperty(out, k.value, { value: v.value, writable: true, enumerable: true, configurable: true });
      p = jsonSpace(text, v.i);
      if (text[p] === '}') return { value: out, i: p + 1 };
      if (text[p] !== ',') throw new SyntaxError('expected comma or closing brace');
      p = jsonSpace(text, p + 1);
    }
  }
  JSON_NUMBER.lastIndex = i;
  const match = JSON_NUMBER.exec(text);
  if (!match) throw new SyntaxError('invalid JSON value');
  const token = match[0], integer = /^-?\d+$/.test(token);
  const digits = integer ? token.replace(/^-/, '').length : 0;
  return { value: integer && digits > 15 ? BigInt(token) : Number(token), i: i + token.length };
}

/**
 * lez/sequencer/service/rpc/src/lib.rs `getTransactionReceipt` (FeeReceipt contains u128): preserve integer tokens
 * above 15 decimal digits. Floats, exponents, short integers, strings, booleans and null remain ordinary JS values.
 */
export function parseJsonBig(text) {
  const start = jsonSpace(text, 0), v = jsonValue(text, start), end = jsonSpace(text, v.i);
  if (end !== text.length) throw new SyntaxError('trailing characters after JSON');
  return v.value;
}

/** A sequencer with v0.4.0's receipt/status methods; inherited wire/account APIs remain those of lez.js. */
export class Sequencer extends BaseSequencer {
  /** lez/sequencer/service/rpc/src/lib.rs RPC transport, parsing response integers losslessly. */
  async call(method, params = []) {
    for (let attempt = 0; ; attempt++) {
      const ctl = new AbortController(), timer = setTimeout(() => ctl.abort(), this.timeoutMs);
      try {
        const res = await fetch(this.url, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, signal: ctl.signal,
          body: JSON.stringify({ jsonrpc: '2.0', id: ++this.id, method, params }),
        });
        if ((res.status === 429 || res.status === 502 || res.status === 503) && attempt < 4) {
          const ra = Number(res.headers.get('Retry-After'));
          await new Promise((resolve) => setTimeout(resolve, Math.min(10000, (ra > 0 ? ra * 1000 : 0) || 600 * 2 ** attempt)));
          continue;
        }
        // A JSON-RPC error is read whatever the HTTP status: the test network's gateway answers a method it does not
        // forward (getSequencerStatus on rc3) with HTTP 403 and error -32601, which detectLezVersion must see.
        const text = await res.text();
        let j = null;
        try { j = parseJsonBig(text); } catch { j = null; }
        if (j && typeof j === 'object' && j.error) throw new RpcError(method, j.error);
        if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`);
        if (!j || typeof j !== 'object' || !('result' in j)) throw new Error(`${method}: not a JSON-RPC answer`);
        return j.result;
      } finally { clearTimeout(timer); }
    }
  }
  /** rpc/src/lib.rs `get_transaction_receipt`: null for an unknown hash or a receipt-less system transaction. */
  receipt(hash) { return this.call('getTransactionReceipt', [hash]); }
  /** rpc/src/lib.rs `get_sequencer_status`: Bootstrapping or Online state. */
  status() { return this.call('getSequencerStatus'); }
}

/**
 * rpc/src/lib.rs `getSequencerStatus`: its presence distinguishes v0.4.0 from rc3's method-not-found response (the
 * test network's gateway answers a method it does not forward the same way). v0.4.0's -31902 says its sequencer is
 * not online yet. Its status is tagged by `state` (lez/sequencer/service/protocol/src/lib.rs); any other answer
 * (a stand-in's null) names no version and throws.
 */
export async function detectLezVersion(seq) {
  let status;
  try { status = await seq.status(); }
  catch (error) {
    if (error instanceof RpcError && error.code === -31902) return 'v0.4.0';
    if (error instanceof RpcError && error.code === -32601) return 'pre-v0.4.0';
    throw error;
  }
  if (status && (status.state === 'online' || status.state === 'bootstrapping')) return 'v0.4.0';
  throw new Error('getSequencerStatus answered without a sequencer status: the network\'s LEZ version is unknown');
}
