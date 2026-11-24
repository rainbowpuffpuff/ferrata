// Ferrata's attestation program (RFP-021) from the browser: its PDAs, its accounts' layouts and the borsh of its
// `Attest` request (idl/ferrata_attestation.idl.json; lez/wire/src/lib.rs; crates/ferrata-core/src/attest.rs).
// test/attest.test.mjs checks the request byte for byte against messages the repository's own tool recorded.
import { sha256 } from './vendor/noble.js';
import { Writer, Reader, concat, utf8, unhex, hex } from './lez.js';

const DOMAIN = utf8('ferrata-lez/v1/');
const PDA_PREFIX = (() => { const b = new Uint8Array(32); b.set(utf8('/LEE/v0.2/AccountId/PDA/')); return b; })();
const le64 = (v) => new Writer().u64(v).done();
const be64 = (v) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), false); return b; };

export const taggedHash = (tag, ...parts) => sha256(concat(DOMAIN, utf8(tag), ...parts));
/** LEZ's `AccountId::for_public_pda(program, seed)`. */
export const forPublicPda = (program, seed) => sha256(concat(PDA_PREFIX, program, seed));
export const instanceSeed = (id) => taggedHash('instance', id);
export const headerSeed = (id, slot) => taggedHash('header', id, le64(slot));
export const instanceAccount = (program, id) => forPublicPda(program, instanceSeed(id));
export const headerAccount = (program, id, slot) => forPublicPda(program, headerSeed(id, slot));

// ------------------------------------------------------------------ the program's accounts
const CONFIG_LEN = 8 + 4 + 8 + 8 + 8 + 36 + 72 + 32 + 8 + 8 + 1 + 1 + 8; // Config: fixed width, 202 bytes
const BEACON_HEADER_LEN = 112; // SSZ BeaconBlockHeader: slot, proposer, parent, state, body roots
/** InstanceState: id, config, active, store (CompactStore: fork byte, then the finalized beacon header), ... */
export function decodeInstance(bytes) {
  const r = new Reader(bytes);
  const id = r.take(32);
  const config = r.take(CONFIG_LEN);
  const chainId = Number(new Reader(config).u64());
  const active = r.u8() === 1;
  const store = r.bytes();
  const applied = r.u64();
  const rootId = r.take(32), parentId = r.take(32);
  const finalizedSlot = store.length >= 9 ? Number(new Reader(store.subarray(1, 9)).u64()) : null;
  return { id, chainId, active, applied, rootId, parentId, finalizedSlot, storeFork: store[0] };
}
/** RecordState: instance id, chain id, the HeaderRecord's encoding, root id. */
export function decodeRecord(bytes) {
  const r = new Reader(bytes);
  const instanceId = r.take(32), chainId = Number(r.u64()), rec = r.bytes(), rootId = r.take(32);
  const q = new Reader(rec);
  const beacon = q.take(BEACON_HEADER_LEN);
  const slot = Number(new Reader(beacon.subarray(0, 8)).u64());
  const executionRoot = q.take(32);
  const kind = q.u8();
  let execution = { kind: 'none' };
  if (kind === 1) {
    execution = { kind: 'payload', blockNumber: Number(q.u64()), blockHash: q.take(32), parentHash: q.take(32),
      stateRoot: q.take(32), receiptsRoot: q.take(32), timestamp: Number(q.u64()) };
  } else if (kind === 2) execution = { kind: 'blockHash', blockHash: q.take(32) };
  return { instanceId, chainId, slot, executionRoot, execution, rootId };
}

// ------------------------------------------------------------------ the Attest request
export const FIELD = { Nonce: 0, Balance: 1, StorageRoot: 2, CodeHash: 3, Value: 4, Emitter: 5, Topic: 6, DataWord: 7 };
export const CMP = { Eq: 0, Ne: 1, Lt: 2, Le: 3, Gt: 4, Ge: 5 };
const nodes = (w, list) => { w.u32(list.length); for (const n of list) w.bytes(typeof n === 'string' ? unhex(n) : n); };
function condition(w, c) {
  w.u8(FIELD[c.field]);
  if (c.field === 'Topic') w.u8(c.index);
  if (c.field === 'DataWord') w.u32(c.index);
  w.u8(CMP[c.cmp]).fixed(c.rhs, 32);
}
const conditions = (w, list = []) => { w.u32(list.length); for (const c of list) condition(w, c); };
/**
 * `Message::Attest { request }` (variant 7). req = { instanceId, chainId, header: bytes | null,
 *   accounts: [{ address, proof: [hex], conditions, slots: [{ slot, proof: [hex], conditions }] }],
 *   logs: [{ txIndex, logIndex, proof: [hex], conditions }] }
 */
export function encodeAttest(req) {
  const w = new Writer().u8(7).fixed(req.instanceId, 32).u64(req.chainId);
  if (req.header) w.u8(1).bytes(req.header); else w.u8(0);
  w.u32(0); // hops: none (the record's own block)
  const accounts = req.accounts ?? [];
  w.u32(accounts.length);
  for (const a of accounts) {
    w.fixed(a.address, 20); nodes(w, a.proof); conditions(w, a.conditions);
    w.u32((a.slots ?? []).length);
    for (const s of a.slots ?? []) { w.fixed(s.slot, 32); nodes(w, s.proof); conditions(w, s.conditions); }
  }
  const logs = req.logs ?? [];
  w.u32(logs.length);
  for (const l of logs) { w.u64(l.txIndex).u64(l.logIndex); nodes(w, l.proof); conditions(w, l.conditions); }
  w.u32(0); // leaves
  w.u32(0); // relations
  return w.done();
}

// ------------------------------------------------------------------ statement identifiers (attest.rs)
const STATEMENT = utf8('ferrata/statement/v1');
/** `storage_id_at_block`: SHA-256(tag ‖ 0x12 ‖ chain_id ‖ block_number ‖ address ‖ slot ‖ value). */
export const storageStatementId = (chainId, blockNumber, address, slot, value) =>
  sha256(concat(STATEMENT, Uint8Array.of(0x12), be64(chainId), be64(blockNumber), address, slot, value));
/** `account_id_at_block`: SHA-256(tag ‖ 0x11 ‖ chain_id ‖ block_number ‖ address ‖ nonce ‖ balance ‖ storage_root ‖ code_hash). */
export const accountStatementId = (chainId, blockNumber, address, a) =>
  sha256(concat(STATEMENT, Uint8Array.of(0x11), be64(chainId), be64(blockNumber), address, be64(a.nonce), a.balance, a.storageRoot, a.codeHash));

/** The account fields eth_getProof reports, as the statement hashes them. */
export function accountOf(proof) {
  const w32 = (v) => { const x = BigInt(v); return unhex(x.toString(16).padStart(64, '0')); };
  return { nonce: BigInt(proof.nonce), balance: w32(proof.balance), storageRoot: unhex(proof.storageHash), codeHash: unhex(proof.codeHash) };
}
export const ATTEST_GAS = 2000000n; // the day-one kit's limit for an Attest (lez/updater/sequencer/src/day1.rs)
export { hex };
