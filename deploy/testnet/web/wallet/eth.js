// Ethereum reads for the Ferrata features: RLP, the block header rebuilt from JSON (checked against its hash), and a
// JSON-RPC client over the user's own browser wallet (EIP-1193, e.g. MetaMask) or a same-origin endpoint. The page
// never needs an Ethereum key for reads; deposits are signed by the user's browser wallet, never by this page.
import { keccak_256 } from './vendor/noble.js';
import { hex, unhex, concat } from './lez.js';

export const keccak = (b) => keccak_256(b);

// ------------------------------------------------------------------ RLP
function lenPrefix(len, short, long) {
  if (len < 56) return Uint8Array.of(short + len);
  const l = uintBytes(len);
  return concat(Uint8Array.of(long + l.length), l);
}
/** RLP of a byte string or a (nested) list of them. */
export function rlp(item) {
  if (item instanceof Uint8Array) {
    if (item.length === 1 && item[0] < 0x80) return item;
    return concat(lenPrefix(item.length, 0x80, 0xb7), item);
  }
  const body = concat(...item.map(rlp));
  return concat(lenPrefix(body.length, 0xc0, 0xf7), body);
}
/** An integer as RLP wants it: big-endian, no leading zero byte (zero is empty). */
export function uintBytes(v) {
  let n = BigInt(v);
  if (n === 0n) return new Uint8Array(0);
  let h = n.toString(16); if (h.length % 2) h = '0' + h;
  return unhex(h);
}

// The execution block header's fields in order; a field absent from the JSON ends the list (older forks).
// Same order as the repository's crates/ferrata-spectests/src/execkit.rs header_rlp (through Amsterdam's fields).
const HEADER = [['parentHash', 0], ['sha3Uncles', 0], ['miner', 0], ['stateRoot', 0], ['transactionsRoot', 0],
  ['receiptsRoot', 0], ['logsBloom', 0], ['difficulty', 1], ['number', 1], ['gasLimit', 1], ['gasUsed', 1],
  ['timestamp', 1], ['extraData', 0], ['mixHash', 0], ['nonce', 0], ['baseFeePerGas', 1], ['withdrawalsRoot', 0],
  ['blobGasUsed', 1], ['excessBlobGas', 1], ['parentBeaconBlockRoot', 0], ['requestsHash', 0],
  ['blockAccessListHash', 0], ['slotNumber', 1]];
/** The RLP header of an eth_getBlockBy* result; throws unless it hashes to the block's own hash. */
export function headerRlp(block) {
  const items = [];
  for (const [k, isInt] of HEADER) {
    if (block[k] === undefined || block[k] === null) break;
    items.push(isInt ? uintBytes(BigInt(block[k])) : unhex(block[k]));
  }
  const out = rlp(items);
  if (block.hash && hex(keccak(out)) !== block.hash.replace(/^0x/, '').toLowerCase()) {
    throw new Error('the block header rebuilt from the endpoint\'s JSON does not hash to the block hash (an unknown header field?)');
  }
  return out;
}

// ------------------------------------------------------------------ slots
const word = (v) => { const b = new Uint8Array(32); const x = typeof v === 'bigint' ? unhex(v.toString(16).padStart(64, '0')) : v; b.set(x, 32 - x.length); return b; };
export { word };
/** The storage slot of mapping entry `key` (an address or a 32-byte word) of a mapping declared at slot `index`. */
export function mappingSlot(key, index) { return keccak(concat(word(key), word(BigInt(index)))); }
/** EIP-55: the address with each hex letter upper-cased where keccak(lowercase hex) has a nibble of 8 or more. */
export function toChecksumAddress(s) {
  const h = String(s).trim().replace(/^0x/i, '').toLowerCase();
  if (!/^[0-9a-f]{40}$/.test(h)) throw new Error('an Ethereum address is 0x followed by 40 hex digits');
  const k = hex(keccak(new TextEncoder().encode(h)));
  return '0x' + [...h].map((c, i) => (parseInt(k[i], 16) >= 8 ? c.toUpperCase() : c)).join('');
}
/**
 * An Ethereum address as 20 bytes. Mixed-case input must carry a valid EIP-55 checksum (a typo in it is caught);
 * all-lowercase or all-uppercase input has none to check.
 */
export function address(s) {
  const t = String(s).trim();
  if (!/^0x[0-9a-fA-F]{40}$/.test(t)) throw new Error('an Ethereum address is 0x followed by 40 hex digits');
  const body = t.slice(2);
  if (body !== body.toLowerCase() && body !== body.toUpperCase() && toChecksumAddress(t) !== '0x' + body) {
    throw new Error(`${t} fails its EIP-55 checksum (mixed case that does not match): check it for a typo`);
  }
  return unhex(t);
}
/** Whether the address text carries an EIP-55 checksum (mixed case); address() has verified it if so. */
export const hasChecksum = (s) => { const b = String(s).trim().slice(2); return b !== b.toLowerCase() && b !== b.toUpperCase(); };

// ------------------------------------------------------------------ JSON-RPC
export class Eth {
  /** `source`: an EIP-1193 provider (window.ethereum) or a same-origin URL path such as "/eth". */
  constructor(source) { this.source = source; this.id = 0; }
  async request(method, params = []) {
    if (typeof this.source === 'string') {
      const r = await fetch(this.source, { method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: ++this.id, method, params }) });
      const j = await r.json();
      if (j.error) throw new Error(`${method}: ${j.error.message}`);
      return j.result;
    }
    return this.source.request({ method, params });
  }
  async chainId() { return Number(BigInt(await this.request('eth_chainId'))); }
  blockByHash(h) { return this.request('eth_getBlockByHash', [h, false]); }
  blockByNumber(n) { return this.request('eth_getBlockByNumber', [typeof n === 'string' ? n : '0x' + n.toString(16), false]); }
  proof(addr, slots, blockNumber) {
    return this.request('eth_getProof', [addr, slots, '0x' + BigInt(blockNumber).toString(16)]);
  }
}
export function injected() { return typeof window !== 'undefined' && window.ethereum ? window.ethereum : null; }

// ------------------------------------------------------------------ Merkle-Patricia proofs (the page's own check)
/** RLP decode: returns a Uint8Array or a nested array of them. */
export function unrlp(b) {
  const [v, n] = dec(b, 0);
  if (n !== b.length) throw new Error('rlp: trailing bytes');
  return v;
  function len(o, k) { let x = 0; for (let i = 0; i < k; i++) x = x * 256 + b[o + i]; return x; }
  function dec(_, o) {
    const p = b[o];
    if (p < 0x80) return [b.subarray(o, o + 1), o + 1];
    if (p < 0xb8) return [b.subarray(o + 1, o + 1 + p - 0x80), o + 1 + p - 0x80];
    if (p < 0xc0) { const k = p - 0xb7, l = len(o + 1, k); return [b.subarray(o + 1 + k, o + 1 + k + l), o + 1 + k + l]; }
    let end, s;
    if (p < 0xf8) { s = o + 1; end = s + p - 0xc0; } else { const k = p - 0xf7; s = o + 1 + k; end = s + len(o + 1, k); }
    const out = [];
    while (s < end) { const [v, n] = dec(b, s); out.push(v); s = n; }
    if (s !== end) throw new Error('rlp: list length');
    return [out, end];
  }
}
const nibbles = (b) => { const out = []; for (const x of b) out.push(x >> 4, x & 15); return out; };
/**
 * The value an eth_getProof proof proves for `key` (already hashed: keccak(address) or keccak(slot)) under `root`;
 * null when it proves absence. Throws when the proof does not hold.
 */
export function verifyProof(root, key, proofHex) {
  const path = nibbles(key);
  let i = 0, p = 0, want = root, node = null;
  // `node` is the next node when it was embedded in its parent (its RLP is under 32 bytes, so it has no proof element of
  // its own); otherwise the next proof element is taken and must hash to `want`.
  for (;;) {
    if (node === null) {
      if (p >= proofHex.length) throw new Error('proof: incomplete');
      const raw = unhex(proofHex[p++]);
      if (hex(keccak(raw)) !== hex(want)) throw new Error('proof: a node does not match its parent');
      node = unrlp(raw);
    }
    const n = node; node = null;
    if (!Array.isArray(n)) throw new Error('proof: a node is not a list');
    let next;
    if (n.length === 17) {
      if (i === path.length) return n[16].length ? n[16] : null;
      next = n[path[i++]];
      if (next instanceof Uint8Array && !next.length) return null;
    } else if (n.length === 2) {
      const hp = nibbles(n[0]), flag = hp[0], isLeaf = flag >= 2;
      const part = hp.slice(flag % 2 === 1 ? 1 : 2);
      const rest = path.slice(i, i + part.length);
      if (part.length > path.length - i || part.some((x, k) => x !== rest[k])) return null; // diverges: absent
      i += part.length;
      if (isLeaf) { if (i !== path.length) return null; return n[1]; }
      next = n[1];
    } else throw new Error('proof: a node is neither a branch nor a leaf or extension');
    if (next instanceof Uint8Array) {
      if (next.length !== 32) throw new Error('proof: a reference is neither a 32-byte hash nor an embedded node');
      want = next;
    } else node = next; // embedded: decoded in place
  }
}
/** An account's fields proven under a state root, or null if it is absent. */
export function provenAccount(stateRoot, addr, accountProof) {
  const v = verifyProof(stateRoot, keccak(addr), accountProof);
  if (!v) return null;
  const [nonce, balance, storageRoot, codeHash] = unrlp(v);
  const big = (x) => (x.length ? BigInt('0x' + hex(x)) : 0n);
  return { nonce: big(nonce), balance: big(balance), storageRoot, codeHash };
}
/** A storage slot's value proven under an account's storage root (zero when absent), as a 32-byte word. */
export function provenSlot(storageRoot, slot, storageProof) {
  const v = verifyProof(storageRoot, keccak(slot), storageProof);
  if (!v) return new Uint8Array(32);
  const inner = unrlp(v);
  return word(inner);
}
