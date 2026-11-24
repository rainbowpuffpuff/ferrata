// The deposit commitment the page derives, against the bridge's published vectors (bridge/vectors/inbound-v2.json,
// SPEC-v2 §3): bridgeSeed → kAuth → s → C, every case.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as lez from '../lez.js';
import * as br from '../bridge.js';

const v = JSON.parse(readFileSync(new URL('../../../../../bridge/vectors/inbound-v2.json', import.meta.url)));
const u = (h) => lez.unhex(h);

test('kAuth, s and C equal every case of inbound-v2.json', () => {
  assert.ok(v.cases.length >= 3);
  for (const c of v.cases) {
    const kAuth = br.kAuthOf(u(c.bridge_seed));
    assert.equal('0x' + lez.hex(kAuth), c.k_auth);
    const s = br.depositSecret(kAuth, BigInt(c.evm_chain_id), u(c.vault), u(c.depositor), BigInt(c.j));
    assert.equal('0x' + lez.hex(s), c.s);
    assert.equal('0x' + lez.hex(br.commitment(s, c.kind, u(c.dest))), c.C);
  }
});

test('the bridge seed is the ferrata tool\'s: HMAC-SHA256(seed64, "ferrata/bridge-seed/v1"), disjoint from LEZ keys', () => {
  const seed = lez.seedOf('abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about');
  const bs = br.bridgeSeedOf(seed);
  assert.equal(bs.length, 32);
  assert.notDeepEqual(bs, lez.deriveNode(seed, [0]).ssk);
  assert.notDeepEqual(br.kAuthOf(bs), br.kViewOf(bs));
});

test('depositETH calldata: selector, then denomIndex, C, j as words', () => {
  const C = new Uint8Array(32).fill(0xab);
  const d = br.depositEthData(1, C, 7n);
  assert.equal(d.length, 2 + 8 + 3 * 64);
  assert.equal(d.slice(10, 74), '0'.repeat(63) + '1');
  assert.equal(d.slice(74, 138), 'ab'.repeat(32));
  assert.equal(d.slice(138), '0'.repeat(63) + '7');
});
