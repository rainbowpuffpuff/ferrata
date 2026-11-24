// The claim core against the bridge's published vectors (bridge/vectors/inbound-v2.json): every case's leaf, fid and
// N; the 'claim' case's 5-leaf tree (root, the path of leaf 2) and its nullifier from the wallet's own derivation of s.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as lez from '../lez.js';
import * as br from '../bridge.js';
import * as cl from '../claim.js';

const v = JSON.parse(readFileSync(new URL('../../../../../bridge/vectors/inbound-v2.json', import.meta.url)));
const u = (h) => lez.unhex(h);
const H = (b) => '0x' + lez.hex(b);

test('leaf, fid and N equal every case', () => {
  for (const c of v.cases) {
    const leaf = cl.depositLeaf(BigInt(c.leaf_index), u(c.depositor), u(c.token), u(c.amount), u(c.C));
    assert.equal(H(leaf), c.leaf);
    const fid = cl.fidOf(BigInt(c.evm_chain_id), u(c.vault), BigInt(c.leaf_index), leaf);
    assert.equal(H(fid), c.fid);
    assert.equal(H(cl.nullifierOf(u(c.s), fid)), c.N);
  }
});

test('the claim case: the 5-leaf root, the path of leaf 2, s from the bridge seed, N', () => {
  const c = v.claim;
  const leaves = c.leaves.map(u);
  const { root, path } = cl.treeOf(leaves, c.leaf_index);
  assert.equal(H(root), c.root);
  assert.deepEqual(path.map(H), c.path.siblings);
  const s = br.depositSecret(br.kAuthOf(u(c.bridge_seed)), BigInt(c.evm_chain_id), u(c.vault), u(c.depositor), BigInt(c.j));
  assert.equal(H(s), c.s);
  const fid = cl.fidOf(BigInt(c.evm_chain_id), u(c.vault), c.leaf_index, leaves[c.leaf_index]);
  assert.equal(H(cl.nullifierOf(s, fid)), c.N);
  const msg = cl.encodeClaim({ configId: new Uint8Array(32), leafCount: 5n, witness: { depositor: u(c.depositor), token: u(c.token),
    amount: u(c.amount), leafIndex: BigInt(c.leaf_index), path, s, kind: c.kind, dest: u(c.dest) },
  range: { index: 0n, fromLeaf: 0n, untilLeaf: 5n } });
  assert.equal(msg.length, 1 + 32 + 8 + 20 + 20 + 32 + 8 + 4 + 32 * 32 + 32 + 1 + 32 + 24 + 1);
  assert.equal(msg[0], 5);
});

test('an empty tree is zero[32]; one leaf at index 0 with zero siblings', () => {
  assert.deepEqual(cl.treeOf([]).root, cl.zeros()[32]);
  const leaf = new Uint8Array(32).fill(7);
  const t = cl.treeOf([leaf], 0);
  assert.deepEqual(t.path, cl.zeros().slice(0, 32));
});

// The program's own borsh (idl/vectors/ferrata_bridge.vectors.json, written by lez/bridge-wire/tests/idl_vectors.rs from
// the Rust types): each Claim, ImportRoot and ImportFees(None) vector decoded with the IDL's layout and encoded again by
// the page's encoders gives the same bytes.
const iv = JSON.parse(readFileSync(new URL('../../../../../idl/vectors/ferrata_bridge.vectors.json', import.meta.url))).vectors;
const vecs = (prefix) => iv.filter((x) => x.name.startsWith(prefix));
const vv = (r) => r.vec((x) => x.bytes());

test('Claim vectors: decoded with the IDL layout, re-encoded identically', () => {
  const list = vecs('BridgeMessage::Claim#');
  assert.ok(list.length >= 1);
  for (const x of list) {
    const b = lez.unhex(x.hex), r = new lez.Reader(b);
    assert.equal(r.u8(), 5);
    const configId = r.take(32), leafCount = r.u64();
    const w = { depositor: r.take(20), token: r.take(20), amount: r.take(32), leafIndex: r.u64(), path: r.vec((y) => y.take(32)), s: r.take(32), kind: r.u8(), dest: r.take(32) };
    const range = { index: r.u64(), fromLeaf: r.u64(), untilLeaf: r.u64() };
    const relayer = r.option((y) => y.take(32));
    assert.ok(r.end(), x.name + ': trailing bytes');
    assert.deepEqual(cl.encodeClaim({ configId, leafCount, witness: w, range, relayer }), b, x.name);
  }
});

test('ImportRoot vectors: decoded with the IDL layout, re-encoded identically', () => {
  for (const x of vecs('BridgeMessage::ImportRoot#')) {
    const b = lez.unhex(x.hex), r = new lez.Reader(b);
    assert.equal(r.u8(), 1);
    const m = { configId: r.take(32), recordSlot: r.u64(), root: r.take(32), leafCount: r.u64(), account: vv(r), rootSlot: vv(r), countSlot: vv(r) };
    assert.ok(r.end(), x.name);
    assert.deepEqual(cl.encodeImportRoot(m), b, x.name);
  }
});

test('ImportFees vectors with no schedule: re-encoded identically', () => {
  let checked = 0;
  for (const x of vecs('BridgeMessage::ImportFees#')) {
    const b = lez.unhex(x.hex), r = new lez.Reader(b);
    assert.equal(r.u8(), 12);
    const configId = r.take(32), token = r.take(20), index = r.u64();
    if (r.u8() !== 0) continue; // Some(schedule): not built by the page
    const recordSlot = r.u64(), account = vv(r), slots = r.vec(vv);
    assert.ok(r.end(), x.name);
    assert.deepEqual(cl.encodeImportFeesAbsent({ configId, token, index, recordSlot, account, slots }), b, x.name);
    checked++;
  }
  assert.ok(checked >= 1, 'an ImportFees(None) vector');
});

test('Root and Token state vectors decode to their fields', () => {
  for (const x of iv.filter((y) => y.type === 'BridgeState' && /^BridgeState::(Root|Token)/.test(y.name))) {
    const s = cl.decodeBridgeState(lez.unhex(x.hex));
    assert.ok(s.kind === 'root' || s.kind === 'token', x.name);
    if (s.kind === 'root') assert.match(x.debug, new RegExp('leaf_count: ' + s.leafCount + '\\b'));
    if (s.kind === 'token') assert.match(x.debug, new RegExp('known_until: ' + s.knownUntil + '\\b'));
  }
});

// The public burn: the note against bridge/vectors/outbound-v2.json (every case), the message against the IDL vector.
const ov = JSON.parse(readFileSync(new URL('../../../../../bridge/vectors/outbound-v2.json', import.meta.url)));
test('burn note (r, sOut, memo, COut, mac, NOut) equals every outbound-v2 case, from the bridge seed', () => {
  for (const c of ov.cases) {
    const bs = u(c.bridge_seed), kAuth = br.kAuthOf(bs), kView = br.kViewOf(bs);
    assert.equal(H(kAuth), c.k_auth);
    assert.equal(H(kView), c.k_view);
    const n = cl.burnNote({ kAuth, kView, configId: u(c.config_id), i: BigInt(c.i), chainId: BigInt(c.evm_chain_id), vault: u(c.vault),
      recipient: u(c.recipient), token: u(c.token), amount32: u(c.amount) });
    assert.equal(H(n.r), c.r);
    assert.equal(H(n.sOut), c.s_out);
    assert.equal(H(n.memo), c.memo);
    assert.equal(H(n.cOut), c.C_out);
    assert.equal(H(n.mac), c.mac);
    assert.equal(H(n.nOut), c.N_out);
  }
});

test('BridgeMessage::Burn equals the IDL vector byte for byte', () => {
  const x = iv.find((t) => t.name === 'BridgeMessage::Burn#1');
  const b = u(x.hex);
  const f = (o, n) => b.slice(o, o + n);
  const note = { r: f(33, 32), memo: f(65, 20), mac: f(85, 16), cOut: f(153, 32) };
  assert.equal(lez.hex(cl.encodeBurn({ configId: f(1, 32), note, token: f(101, 20), amount32: f(121, 32) })), x.hex);
  assert.equal(b.length, 185);
  assert.deepEqual([...f(1, 2), ...note.r.slice(0, 2), ...note.memo.slice(0, 2), ...note.mac.slice(0, 2), ...f(101, 2), ...f(121, 2), ...note.cOut.slice(0, 2)],
    [164, 165, 178, 179, 181, 182, 160, 161, 184, 185, 192, 193, 199, 200]); // the fields' first bytes in the vector's debug form
});
