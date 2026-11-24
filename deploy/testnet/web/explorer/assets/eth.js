// Ethereum's encodings, for checking an attest request in the browser: Keccak-256, RLP, Merkle-Patricia proofs
// (state, storage and receipt tries), and the request's conditions. Independent of the attestation program's own code:
// a second implementation that must reach the same verdict.

import { hex } from "./util.js";

// ---- Keccak-256 (the original padding, as Ethereum uses) ------------------------------------------------------

const RC = new Uint32Array([
  0x00000001, 0x00000000, 0x00008082, 0x00000000, 0x0000808a, 0x80000000, 0x80008000, 0x80000000,
  0x0000808b, 0x00000000, 0x80000001, 0x00000000, 0x80008081, 0x80000000, 0x00008009, 0x80000000,
  0x0000008a, 0x00000000, 0x00000088, 0x00000000, 0x80008009, 0x00000000, 0x8000000a, 0x00000000,
  0x8000808b, 0x00000000, 0x0000008b, 0x80000000, 0x00008089, 0x80000000, 0x00008003, 0x80000000,
  0x00008002, 0x80000000, 0x00000080, 0x80000000, 0x0000800a, 0x00000000, 0x8000000a, 0x80000000,
  0x80008081, 0x80000000, 0x00008080, 0x80000000, 0x80000001, 0x00000000, 0x80008008, 0x80000000,
]);
// rotation offsets, lane index x + 5y
const ROT = [0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39, 41, 45, 15, 21, 8, 18, 2, 61, 56, 14];

function keccakF(s) {
  const C = new Uint32Array(10);
  const B = new Uint32Array(50);
  for (let round = 0; round < 24; round++) {
    for (let x = 0; x < 5; x++) {
      C[2 * x] = s[2 * x] ^ s[2 * x + 10] ^ s[2 * x + 20] ^ s[2 * x + 30] ^ s[2 * x + 40];
      C[2 * x + 1] = s[2 * x + 1] ^ s[2 * x + 11] ^ s[2 * x + 21] ^ s[2 * x + 31] ^ s[2 * x + 41];
    }
    for (let x = 0; x < 5; x++) {
      const x1 = (x + 1) % 5, x4 = (x + 4) % 5;
      const lo = C[2 * x1], hi = C[2 * x1 + 1];
      const dlo = C[2 * x4] ^ ((lo << 1) | (hi >>> 31));
      const dhi = C[2 * x4 + 1] ^ ((hi << 1) | (lo >>> 31));
      for (let y = 0; y < 25; y += 5) {
        s[2 * (x + y)] ^= dlo;
        s[2 * (x + y) + 1] ^= dhi;
      }
    }
    for (let x = 0; x < 5; x++) {
      for (let y = 0; y < 5; y++) {
        const i = x + 5 * y;
        const lo = s[2 * i], hi = s[2 * i + 1];
        const r = ROT[i];
        let nlo, nhi;
        if (r === 0) { nlo = lo; nhi = hi; }
        else if (r < 32) { nlo = (lo << r) | (hi >>> (32 - r)); nhi = (hi << r) | (lo >>> (32 - r)); }
        else if (r === 32) { nlo = hi; nhi = lo; }
        else { const m = r - 32; nlo = (hi << m) | (lo >>> (32 - m)); nhi = (lo << m) | (hi >>> (32 - m)); }
        const j = y + 5 * ((2 * x + 3 * y) % 5);
        B[2 * j] = nlo;
        B[2 * j + 1] = nhi;
      }
    }
    for (let y = 0; y < 25; y += 5) {
      for (let x = 0; x < 5; x++) {
        const i = x + y, i1 = ((x + 1) % 5) + y, i2 = ((x + 2) % 5) + y;
        s[2 * i] = B[2 * i] ^ (~B[2 * i1] & B[2 * i2]);
        s[2 * i + 1] = B[2 * i + 1] ^ (~B[2 * i1 + 1] & B[2 * i2 + 1]);
      }
    }
    s[0] ^= RC[2 * round];
    s[1] ^= RC[2 * round + 1];
  }
}

export function keccak256(data) {
  const rate = 136;
  const s = new Uint32Array(50);
  const blocks = Math.floor(data.length / rate) + 1;
  const padded = new Uint8Array(blocks * rate);
  padded.set(data);
  padded[data.length] ^= 0x01;
  padded[padded.length - 1] ^= 0x80;
  const dv = new DataView(padded.buffer);
  for (let b = 0; b < blocks; b++) {
    for (let i = 0; i < rate / 4; i++) s[i] ^= dv.getUint32(b * rate + i * 4, true);
    keccakF(s);
  }
  const out = new Uint8Array(32);
  const ov = new DataView(out.buffer);
  for (let i = 0; i < 8; i++) ov.setUint32(i * 4, s[i], true);
  return out;
}

// ---- RLP --------------------------------------------------------------------------------------------------------

export class RlpError extends Error {}

function rlpItem(b, pos) {
  if (pos >= b.length) throw new RlpError("RLP: out of bytes");
  const p = b[pos];
  const be = (at, n) => {
    if (n > 6 || at + n > b.length) throw new RlpError("RLP: bad length");
    let v = 0;
    for (let i = 0; i < n; i++) v = v * 256 + b[at + i];
    return v;
  };
  if (p < 0x80) return { item: b.subarray(pos, pos + 1), end: pos + 1 };
  if (p <= 0xb7) {
    const n = p - 0x80;
    if (pos + 1 + n > b.length) throw new RlpError("RLP: string past the end");
    return { item: b.subarray(pos + 1, pos + 1 + n), end: pos + 1 + n };
  }
  if (p <= 0xbf) {
    const ll = p - 0xb7;
    const n = be(pos + 1, ll);
    const at = pos + 1 + ll;
    if (at + n > b.length) throw new RlpError("RLP: string past the end");
    return { item: b.subarray(at, at + n), end: at + n };
  }
  let at, n;
  if (p <= 0xf7) {
    n = p - 0xc0;
    at = pos + 1;
  } else {
    const ll = p - 0xf7;
    n = be(pos + 1, ll);
    at = pos + 1 + ll;
  }
  if (at + n > b.length) throw new RlpError("RLP: list past the end");
  const items = [];
  let q = at;
  while (q < at + n) {
    const r = rlpItem(b, q);
    items.push(r.item);
    q = r.end;
  }
  if (q !== at + n) throw new RlpError("RLP: list length");
  items.raw = b.subarray(pos, at + n);
  return { item: items, end: at + n };
}

export function rlpDecode(b) {
  const r = rlpItem(b, 0);
  if (r.end !== b.length) throw new RlpError("RLP: trailing bytes");
  return r.item;
}

export function rlpEncodeInt(n) {
  // only what the receipt trie's keys need: a non-negative integer
  let x = BigInt(n);
  const bytes = [];
  while (x > 0n) {
    bytes.unshift(Number(x & 0xffn));
    x >>= 8n;
  }
  if (bytes.length === 1 && bytes[0] < 0x80) return Uint8Array.from(bytes);
  return Uint8Array.from([0x80 + bytes.length, ...bytes]);
}

export const toBig = (bytes) => bytes.reduce((a, b) => (a << 8n) | BigInt(b), 0n);
export const word = (bytes) => {
  const w = new Uint8Array(32);
  w.set(bytes.subarray(Math.max(0, bytes.length - 32)), 32 - Math.min(32, bytes.length));
  return w;
};

// ---- Merkle-Patricia proofs ---------------------------------------------------------------------------------------

function nibbles(key) {
  const out = new Uint8Array(key.length * 2);
  key.forEach((b, i) => {
    out[2 * i] = b >> 4;
    out[2 * i + 1] = b & 15;
  });
  return out;
}

function hexPrefix(path) {
  const flag = path[0] >> 4;
  const n = nibbles(path);
  return { leaf: flag >= 2, nibbles: flag & 1 ? n.subarray(1) : n.subarray(2) };
}

const eqBytes = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);

/**
 * Verify an eth_getProof-style proof (RLP nodes from the root) of `key` (the trie key, already hashed where the trie
 * hashes it) under `root`. Returns the value's bytes, or null for a proof of absence; throws when the proof is bad.
 */
export function mptGet(root, key, proof) {
  const path = nibbles(key);
  let idx = 0;
  let expect = root; // a 32-byte hash, or an inline node (array)
  let i = 0;
  while (true) {
    let node;
    if (Array.isArray(expect)) node = expect;
    else {
      if (i >= proof.length) throw new RlpError("proof ends before the key's node");
      const enc = proof[i++];
      if (!eqBytes(keccak256(enc), expect)) throw new RlpError(`node ${i - 1} does not hash to its reference`);
      node = rlpDecode(enc);
    }
    if (!Array.isArray(node)) throw new RlpError("a node is not a list");
    if (node.length === 17) {
      if (idx === path.length) return node[16].length ? node[16] : null;
      const child = node[path[idx++]];
      if (Array.isArray(child)) {
        expect = child;
        continue;
      }
      if (child.length === 0) return null;
      if (child.length !== 32) throw new RlpError("a branch reference is not a hash");
      expect = child;
      continue;
    }
    if (node.length === 2) {
      const hp = hexPrefix(node[0]);
      const rest = path.subarray(idx);
      if (hp.leaf) return eqBytes(hp.nibbles, rest) ? node[1] : null;
      if (rest.length < hp.nibbles.length || !eqBytes(hp.nibbles, rest.subarray(0, hp.nibbles.length))) return null;
      idx += hp.nibbles.length;
      const child = node[1];
      if (Array.isArray(child)) expect = child;
      else if (child.length === 32) expect = child;
      else throw new RlpError("an extension reference is not a hash");
      continue;
    }
    throw new RlpError(`a node of ${node.length} items`);
  }
}

// ---- the request, checked ------------------------------------------------------------------------------------------

/** An execution header's fields from its RLP. */
export function executionHeader(rlp) {
  const h = rlpDecode(rlp);
  return { hash: keccak256(rlp), parent_hash: h[0], state_root: h[3], receipts_root: h[5], number: toBig(h[8]), timestamp: toBig(h[11]) };
}

const CMP = { Eq: (a, b) => a === b, Ne: (a, b) => a !== b, Lt: (a, b) => a < b, Le: (a, b) => a <= b, Gt: (a, b) => a > b, Ge: (a, b) => a >= b };
const CMP_TEXT = { Eq: "=", Ne: "≠", Lt: "<", Le: "≤", Gt: ">", Ge: "≥" };

function conditionResult(fieldWord, cond) {
  const lhs = toBig(fieldWord);
  const rhs = toBig(cond.rhs);
  return CMP[cond.cmp](lhs, rhs);
}

/**
 * Check a decoded attest request (plain JS: see `requestFromIdl`) against roots: either the request's own header
 * (whose Keccak must equal `record.block_hash`) or the record's payload roots. Returns {header, facts, ok, notes}.
 */
export function checkRequest(req, record) {
  const notes = [];
  let stateRoot = null, receiptsRoot = null, number = null, hdr = null;
  const rex = record ? record.execution : null;
  if (req.header) {
    hdr = executionHeader(req.header);
    stateRoot = hdr.state_root;
    receiptsRoot = hdr.receipts_root;
    number = hdr.number;
    if (rex && rex.block_hash) {
      hdr.binds = eqBytes(hdr.hash, rex.block_hash);
      if (!hdr.binds) notes.push("the request's header does not hash to the record's execution block hash");
    }
  } else if (rex && rex.state_root) {
    stateRoot = rex.state_root;
    receiptsRoot = rex.receipts_root;
    number = rex.block_number;
  }
  const facts = [];
  if (req.hops.length) notes.push(`${req.hops.length} history hop(s): not re-checked in the browser`);
  if (req.leaves.length) notes.push(`${req.leaves.length} accumulator leaf/leaves: not re-checked in the browser`);
  if (!stateRoot) {
    notes.push("no execution roots to check against (a Gloas record needs the request's header)");
    return { header: hdr, number, facts, ok: false, notes };
  }
  for (const a of req.accounts) {
    const f = { kind: "account", address: a.address, conditions: [], slots: [] };
    try {
      const v = mptGet(stateRoot, keccak256(a.address), a.proof);
      if (v === null) f.absent = true;
      else {
        const acc = rlpDecode(v);
        f.nonce = toBig(acc[0]);
        f.balance = toBig(acc[1]);
        f.storage_root = acc[2];
        f.code_hash = acc[3];
      }
      f.proof_ok = true;
    } catch (e) {
      f.proof_ok = false;
      f.error = e.message;
    }
    const accWord = (field) => {
      if (f.absent) return new Uint8Array(32);
      if (field === "Nonce") return word(bigBytes(f.nonce));
      if (field === "Balance") return word(bigBytes(f.balance));
      if (field === "StorageRoot") return f.storage_root;
      if (field === "CodeHash") return f.code_hash;
      return null;
    };
    for (const c of a.conditions) f.conditions.push(evalCond(c, accWord(c.field)));
    for (const s of a.slots) {
      const sf = { slot: s.slot, conditions: [] };
      try {
        if (!f.proof_ok) throw new Error("the account proof failed");
        const sr = f.absent ? keccak256(Uint8Array.of(0x80)) : f.storage_root;
        const v = mptGet(sr, keccak256(s.slot), s.proof);
        sf.value = v === null ? new Uint8Array(32) : word(rlpDecode(v));
        sf.proof_ok = true;
      } catch (e) {
        sf.proof_ok = false;
        sf.error = e.message;
      }
      for (const c of s.conditions) sf.conditions.push(evalCond(c, sf.value || null));
      f.slots.push(sf);
    }
    facts.push(f);
  }
  for (const l of req.logs) {
    const f = { kind: "log", tx_index: l.tx_index, log_index: l.log_index, conditions: [] };
    try {
      const v = mptGet(receiptsRoot, rlpEncodeInt(l.tx_index), l.proof);
      if (v === null) throw new Error("no receipt at that index");
      const body = v[0] < 0x80 ? v.subarray(1) : v; // typed receipt: drop the type byte
      const receipt = rlpDecode(body);
      const logs = receipt[3];
      const log = logs[Number(l.log_index)];
      if (!log) throw new Error("no log at that index");
      f.address = log[0];
      f.topics = log[1];
      f.data = log[2];
      f.proof_ok = true;
    } catch (e) {
      f.proof_ok = false;
      f.error = e.message;
    }
    const logWord = (field, arg) => {
      if (!f.proof_ok) return null;
      if (field === "Emitter") return word(f.address);
      if (field === "Topic") return f.topics[arg] ? word(f.topics[arg]) : null;
      if (field === "DataWord") {
        const w = f.data.subarray(32 * arg, 32 * arg + 32);
        return w.length === 32 ? w : null;
      }
      return null;
    };
    for (const c of l.conditions) f.conditions.push(evalCond(c, logWord(c.field, c.arg)));
    facts.push(f);
  }
  const all = facts.flatMap((f) => [f.proof_ok, ...f.conditions.map((c) => c.holds), ...(f.slots || []).flatMap((s) => [s.proof_ok, ...s.conditions.map((c) => c.holds)])]);
  const ok = (hdr ? hdr.binds !== false : true) && all.every((x) => x === true);
  return { header: hdr, number, facts, ok, notes };
}

function bigBytes(x) {
  const out = [];
  let v = BigInt(x);
  while (v > 0n) {
    out.unshift(Number(v & 0xffn));
    v >>= 8n;
  }
  return Uint8Array.from(out);
}

function evalCond(c, fieldWord) {
  const text = `${c.field}${c.arg !== undefined ? `(${c.arg})` : ""} ${CMP_TEXT[c.cmp]} 0x${hex(c.rhs).replace(/^0+(?=.)/, "")}`;
  if (!fieldWord) return { text, holds: false, why: "field unavailable" };
  return { text, holds: conditionResult(fieldWord, c), lhs: fieldWord };
}

/** The attest request decoded by the IDL, as plain JS with byte arrays. */
export function requestFromIdl(v) {
  const f = (x, n) => x.fields.find(([k]) => k === n)[1];
  const bytesList = (x) => x.items.map((i) => i.v);
  const cond = (c) => {
    const fld = f(c, "field");
    return { field: fld.variant, arg: fld.fields.length ? Number(fld.fields[0][1].v) : undefined, cmp: f(c, "cmp").variant, rhs: f(c, "rhs").v };
  };
  const hdr = f(v, "header");
  return {
    instance_id: f(v, "instance_id").v,
    chain_id: f(v, "chain_id").v,
    header: hdr.v ? hdr.v.v : null,
    hops: f(v, "hops").items,
    accounts: f(v, "accounts").items.map((a) => ({
      address: f(a, "address").v,
      proof: bytesList(f(a, "proof")),
      conditions: f(a, "conditions").items.map(cond),
      slots: f(a, "slots").items.map((s) => ({ slot: f(s, "slot").v, proof: bytesList(f(s, "proof")), conditions: f(s, "conditions").items.map(cond) })),
    })),
    logs: f(v, "logs").items.map((l) => ({ tx_index: f(l, "tx_index").v, log_index: f(l, "log_index").v, proof: bytesList(f(l, "proof")), conditions: f(l, "conditions").items.map(cond) })),
    leaves: f(v, "leaves").items,
    relations: f(v, "relations").items,
  };
}
