// The attestation request the page builds, byte for byte against the Attest messages the repository's own tool
// (ferrata-attest, run of 21 November 2026: docs/guides/runs/2026-11-21-lez/attest-messages/) sent for Sepolia's USDC
// slot 11 at block 12,105,936, from the same recorded responses (crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as lez from '../lez.js';
import * as eth from '../eth.js';
import * as at from '../attest.js';

const repo = new URL('../../../../../', import.meta.url);
const read = (p) => readFileSync(new URL(p, repo));
const fx = (f) => JSON.parse(read('crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10/' + f)).result;
const recorded = (n) => lez.unhex(read(`docs/guides/runs/2026-11-21-lez/attest-messages/message-${n}.hex`).toString().trim());

const block = fx('block.json');
const proof = fx('proof_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_12105936.json');
const SLOT = eth.word(11n);

test('the header rebuilt from eth_getBlockByHash JSON is the recorded header.rlp and hashes to the block', () => {
  const h = eth.headerRlp(block);
  assert.deepEqual(h, new Uint8Array(read('crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10/header.rlp')));
  assert.equal('0x' + lez.hex(eth.keccak(h)), block.hash);
  assert.throws(() => eth.headerRlp({ ...block, gasUsed: '0x1' }));
});

function request(conditions) {
  const rec = recorded(5);
  return {
    instanceId: rec.subarray(1, 33), chainId: 11155111, header: eth.headerRlp(block),
    accounts: [{ address: lez.unhex(proof.address), proof: proof.accountProof, conditions: [],
      slots: [{ slot: SLOT, proof: proof.storageProof[0].proof, conditions }] }],
  };
}

test('slot 11, no predicate: equal to the recorded message 5 (and 6, its send)', () => {
  const m = at.encodeAttest(request([]));
  assert.deepEqual(m, recorded(5));
  assert.deepEqual(m, recorded(6));
});

test('slot 11 with the predicate value == 0x01: equal to the recorded message 7 (and 8)', () => {
  const m = at.encodeAttest(request([{ field: 'Value', cmp: 'Eq', rhs: eth.word(1n) }]));
  assert.deepEqual(m, recorded(7));
  assert.deepEqual(m, recorded(8));
});

test('the PDAs and statement ids are the documented derivations', () => {
  const id = recorded(5).subarray(1, 33), program = new Uint8Array(32).fill(7);
  const seed = lez.concat(new TextEncoder().encode('ferrata-lez/v1/header'), id, new lez.Writer().u64(11549664n).done());
  assert.deepEqual(at.headerSeed(id, 11549664), (await_sha(seed)));
  assert.equal(at.headerAccount(program, id, 11549664).length, 32);
  const v = lez.unhex(proof.storageProof[0].value.replace(/^0x/, '').padStart(64, '0'));
  assert.equal(lez.hex(v), '0000000000000000000000000000000000000000000000009513ef76177584ee');
  assert.equal(at.storageStatementId(11155111, 12105936, lez.unhex(proof.address), SLOT, v).length, 32);
});
import { sha256 } from '../vendor/noble.js';
function await_sha(b) { return sha256(b); }

test('the page\'s own proof check: the account and slot 11 hold under the block\'s state root', () => {
  const acc = eth.provenAccount(lez.unhex(block.stateRoot), lez.unhex(proof.address), proof.accountProof);
  assert.equal(lez.hex(acc.storageRoot), proof.storageHash.slice(2));
  assert.equal(acc.nonce, BigInt(proof.nonce));
  const v = eth.provenSlot(acc.storageRoot, SLOT, proof.storageProof[0].proof);
  assert.equal(lez.hex(v), '0000000000000000000000000000000000000000000000009513ef76177584ee');
  const bad = [...proof.storageProof[0].proof]; bad[1] = bad[1].slice(0, -2) + (bad[1].endsWith('00') ? '01' : '00');
  assert.throws(() => eth.provenSlot(acc.storageRoot, SLOT, bad));
  assert.throws(() => eth.provenAccount(lez.unhex(block.parentHash), lez.unhex(proof.address), proof.accountProof));
  const other = fx('proof_0xca11bde05977b3631167028862be2a173976ca11_12105936.json');
  const a2 = eth.provenAccount(lez.unhex(block.stateRoot), lez.unhex(other.address), other.accountProof);
  assert.equal(lez.hex(a2.codeHash), other.codeHash.slice(2));
});
