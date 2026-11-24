// Ferrata's accounts on LEZ, decoded: the attestation program's instance, job and record actor states (through
// idl/ferrata_attestation.idl.json), the light-client store inside an instance (`CompactStore`) and the header
// record inside a record (`HeaderRecord`), both in ferrata-core's strict encodings (crates/ferrata-core/src/
// witness.rs), the PDAs of both programs, and the beacon-chain arithmetic the status page needs.

import { Reader, DecodeError } from "./borsh.js";
import { sha256 } from "./sha256.js";
import { forPublicPda } from "./lez.js";
import { field, plain } from "./idl.js";
import { utf8, u64le, concat, hex } from "./util.js";

export const ATTESTATION_DOMAIN = "ferrata-lez/v1/";
export const BRIDGE_DOMAIN = "ferrata-lez/v1/bridge/";

const encodePart = (p) => (p instanceof Uint8Array ? p : typeof p === "bigint" || typeof p === "number" ? u64le(p) : utf8(String(p)));

/** `AccountId::for_public_pda(program, SHA-256(domain ‖ tag ‖ parts))`: Ferrata's PDA rule. */
export function ferrataPda(domain, program, tag, ...parts) {
  const seed = sha256(utf8(domain), utf8(tag), ...parts.map(encodePart));
  return forPublicPda(program, seed);
}

/** The attestation program's accounts: `instance(id)`, `job(id, job_id)`, `header(id, slot)`. */
export function attestationPda(program, tag, ...parts) {
  return ferrataPda(ATTESTATION_DOMAIN, program, tag, ...parts);
}

export function bridgePda(program, tag, ...parts) {
  return ferrataPda(BRIDGE_DOMAIN, program, tag, ...parts);
}

// ---- beacon headers ----------------------------------------------------------------------------------------

export const FORKS = ["Altair", "Capella", "Deneb", "Electra", "Gloas"];

function readBeaconHeader(r) {
  const raw = r.bytes(112);
  const dv = new DataView(raw.buffer, raw.byteOffset, 112);
  return {
    slot: dv.getBigUint64(0, true),
    proposer_index: dv.getBigUint64(8, true),
    parent_root: raw.subarray(16, 48),
    state_root: raw.subarray(48, 80),
    body_root: raw.subarray(80, 112),
    ssz: raw,
  };
}

/** `hash_tree_root(BeaconBlockHeader)`: the block root a beacon API serves for the header. */
export function beaconHeaderRoot(h) {
  const leaf = (v) => {
    const b = new Uint8Array(32);
    b.set(u64le(v));
    return b;
  };
  const z = new Uint8Array(32);
  const l = [leaf(h.slot), leaf(h.proposer_index), h.parent_root, h.state_root, h.body_root, z, z, z];
  const n1 = [sha256(l[0], l[1]), sha256(l[2], l[3]), sha256(l[4], l[5]), sha256(l[6], l[7])];
  return sha256(sha256(n1[0], n1[1]), sha256(n1[2], n1[3]));
}

// ---- CompactStore and HeaderRecord --------------------------------------------------------------------------

const summary = (r) => ({ pubkeys_root: r.bytes(32), aggregate_pubkey: r.bytes(48) });

/** `CompactStore::decode`: fork byte, finalized beacon header (SSZ), current committee summary, next flag [+ summary]. */
export function decodeCompactStore(bytes) {
  const r = new Reader(bytes);
  const f = r.u8();
  if (f >= FORKS.length) throw new DecodeError(`fork byte ${f}`);
  const finalized = readBeaconHeader(r);
  const current = summary(r);
  const flag = r.u8();
  if (flag > 1) throw new DecodeError(`flag ${flag}`);
  const next = flag ? summary(r) : null;
  r.finish("CompactStore");
  return { fork: FORKS[f], finalized, current, next, digest: sha256(bytes) };
}

/** `HeaderRecord::decode`: beacon header, execution root, layout byte, then the payload fields or the block hash. */
export function decodeHeaderRecord(bytes) {
  const r = new Reader(bytes);
  const beacon = readBeaconHeader(r);
  const execution_root = r.bytes(32);
  const layout = r.u8();
  let execution;
  if (layout === 0) execution = { layout: "none" };
  else if (layout === 1) {
    execution = {
      layout: "payload",
      block_number: r.u64(),
      block_hash: r.bytes(32),
      parent_hash: r.bytes(32),
      state_root: r.bytes(32),
      receipts_root: r.bytes(32),
      timestamp: r.u64(),
    };
  } else if (layout === 2) execution = { layout: "block hash", block_hash: r.bytes(32) };
  else throw new DecodeError(`record layout ${layout}`);
  r.finish("HeaderRecord");
  return { beacon, execution_root, execution, block_root: beaconHeaderRoot(beacon) };
}

// ---- the attestation program's actor states ----------------------------------------------------------------

/** The config's numbers as plain values (BigInts where the IDL says u64). */
export function configOf(v) {
  const p = plain(v);
  return p;
}

/**
 * Decode an attestation actor state. Returns {type, value, plain, store?, record?} or null when the bytes are none
 * of the three states. Try-all, as SPEL does; the store and record inside are decoded too.
 */
export function decodeAttestationState(idl, bytes) {
  const hits = idl.decodeAccount(bytes);
  for (const h of hits) {
    const out = { type: h.type, value: h.value, plain: plain(h.value) };
    try {
      if (h.type === "InstanceState") out.store = decodeCompactStore(field(h.value, "store").v);
      if (h.type === "RecordState") out.record = decodeHeaderRecord(field(h.value, "record").v);
    } catch (e) {
      out.inner_error = e.message;
    }
    if (!out.inner_error || hits.length === 1) return out;
  }
  return null;
}

export const decodeInstanceState = (idl, bytes) => {
  const d = decodeAttestationState(idl, bytes);
  return d && d.type === "InstanceState" ? d : null;
};
export const decodeRecordState = (idl, bytes) => {
  const d = decodeAttestationState(idl, bytes);
  return d && d.type === "RecordState" ? d : null;
};

// ---- beacon time ---------------------------------------------------------------------------------------------

/** Times and periods from an instance's `Config` (plain). */
export function beaconClock(config) {
  const spe = BigInt(config.slots_per_epoch);
  const epp = BigInt(config.epochs_per_sync_committee_period);
  const sps = BigInt(config.seconds_per_slot);
  const genesis = BigInt(config.genesis_time);
  return {
    slotsPerPeriod: spe * epp,
    epochOf: (slot) => BigInt(slot) / spe,
    periodOf: (slot) => BigInt(slot) / (spe * epp),
    timeOfSlotMs: (slot) => Number((genesis + BigInt(slot) * sps) * 1000n),
    slotAtMs: (ms) => (BigInt(Math.floor(ms / 1000)) - genesis) / sps,
    periodEndSlot: (slot) => (BigInt(slot) / (spe * epp) + 1n) * spe * epp,
  };
}

export const CHAIN_NAMES = { 1: "Ethereum mainnet", 11155111: "Sepolia", 17000: "Holešky", 560048: "Hoodi" };

export { hex, concat };
