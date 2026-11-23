// names.js against the program's own encodings: lez/results/names-vectors.json, written by the harness test
// `js_vectors` (lez/harness/tests/names.rs) from the Rust types the program uses. `node --test names/js/test/names.test.mjs`
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as N from '../names.js';

const V = JSON.parse(readFileSync(fileURLToPath(new URL('../../../lez/results/names-vectors.json', import.meta.url)), 'utf8'));
const A = V.account;
const SALT = N.unhex(V.salt);
const receiver = (fill) => ({ npk: new Uint8Array(N.NPK_LEN).fill(fill), vpk: new Uint8Array(N.VPK_LEN).fill(fill) });

test('seeds and name accounts are the program\'s', async () => {
  assert.equal(V.names.length, 14);
  for (const v of V.names) {
    assert.equal(N.skeleton(v.name), v.skeleton, v.name);
    assert.equal(N.hex(await N.nameSeed(v.name)), v.seed, v.name);
    assert.equal(N.hex(await N.nameAccountId(N.unhex(v.program), v.name)), v.account, `${v.programLabel} ${v.name}`);
    // and from the base58 form a user would paste
    assert.equal(N.hex(await N.nameAccountId(N.idString(N.unhex(v.program)), v.name)), v.account);
  }
  // the counter that numbers commitments
  assert.equal(N.hex(await N.sequenceSeed()), V.sequence.seed);
  assert.equal(N.hex(await N.sequenceAccountId(N.unhex(V.sequence.program))), V.sequence.account);
});

test('messages encode as the program decodes them', () => {
  const msgs = [
    { kind: 'Register', name: 'alice', nowMs: V.t0, target: A, salt: SALT },
    { kind: 'Register', name: 'alice', nowMs: V.t0, target: null, receiver: receiver(7), salt: SALT },
    { kind: 'Renew', name: 'alice', nowMs: V.t0 },
    { kind: 'Transfer', name: 'alice', to: A, nowMs: V.t0 },
    { kind: 'SetTarget', name: 'alice', target: A, receiver: null, nowMs: V.t0 },
    { kind: 'Release', name: 'alice' },
    { kind: 'SetPrimary', name: 'alice', nowMs: V.t0 },
    { kind: 'SetPrimary', name: null, nowMs: V.t0 },
    { kind: 'Accept', name: 'alice', nowMs: V.t0 },
    { kind: 'Commit', commitment: N.unhex(V.commitment), nowMs: V.t0 },
    { kind: 'Forget', names: ['alice', 'bob'] },
    { kind: 'Credit', name: 'alice' },
    { kind: 'Debit', name: 'alice' },
    { kind: 'Primary', name: 'alice' },
    { kind: 'Claim', name: 'alice', nowMs: V.t0, target: A, receiver: null, seq: 7 },
    { kind: 'Disown', name: 'alice' },
    { kind: 'Tick' },
    { kind: 'Sequenced', seq: 7 },
  ];
  assert.equal(msgs.length, V.messages.length);
  msgs.forEach((m, i) => assert.equal(N.hex(N.encodeMessage(m)), V.messages[i], m.kind));
});

test('records, user states and events decode', async () => {
  const r = N.decodeRecord(N.unhex(V.record));
  const a = N.idString(N.unhex(A));
  assert.deepEqual({ ...r, receiver: null }, { name: 'alice', owner: a, target: a, receiver: null, registeredMs: V.t0,
    expiresMs: V.t0 + N.TERM_MS, offer: a, commitSeq: 7 });
  assert.deepEqual(r.receiver, receiver(7));
  assert.equal(N.finalAt(r), V.t0 + N.CONTEST_MS);
  assert.deepEqual(N.decodeUser(N.unhex(V.user)), { names: ['alice', 'bob'], primary: 'alice',
    commit: { hash: N.unhex(V.commitment), seq: 7, notBeforeMs: V.t0 + 240_000, expiresMs: V.t0 + 3_600_000 } });
  assert.deepEqual(N.decodeUser(new Uint8Array(0)), { names: [], primary: null, commit: null });
  assert.equal(Object.keys(V.selectors).length, N.EVENT_KINDS.length);
  for (const [k, s] of Object.entries(V.selectors)) assert.equal(await N.eventKind(N.unhex(s)), k);
  assert.throws(() => N.decodeRecord(N.unhex(V.record + '00')), /trailing/);
});

test('the commitment is the program\'s', async () => {
  assert.equal(N.hex(await N.commitment('alice', A, SALT)), V.commitment);
  assert.notEqual(N.hex(await N.commitment('alice', N.idString(new Uint8Array(32).fill(9)), SALT)), V.commitment, 'binds the owner');
  assert.equal(N.COMMIT_SLACK_MS + N.MIN_REVEAL_DELAY_MS, 240_000);
});

test('the rules match the program\'s (lez/names-wire tests)', () => {
  for (const ok of ['abc', 'alice', 'a-b', '0x1', '123', 'a1-b2-c3', 'abcdefghijklmnopqrstuvwxyz012345', 'bridget', 'helpful', 'rooted', 'walleting']) assert.equal(N.checkName(ok), null, ok);
  for (const v of ['faucet1', 'wa11et', 'ferrata-team', 'team-ferrata', '2faucet', '0fficial', 'support-desk', '1ez', 'vva11et', 'my-wallet-2']) assert.equal(N.checkName(v)?.code, 246, v);
  for (const bad of ['', 'ab', 'Alice', 'al ice', '-ab', 'ab-', 'a--b', 'alice.eth', 'ålice', 'аlice', 'abcdefghijklmnopqrstuvwxyz0123456', 'al_ice', 'a\u0000b']) {
    assert.equal(N.checkName(bad)?.code, 245, JSON.stringify(bad));
  }
  for (const r of N.RESERVED) assert.equal(N.checkName(r)?.code, 246, r);
  assert.equal(N.normalize('  Alice.LEZ '), 'alice');
});

test('status follows expiry and grace', () => {
  const r = { expiresMs: 1000 };
  assert.equal(N.status(r, 999), 'live');
  assert.equal(N.status(r, 1000), 'grace');
  assert.equal(N.status(r, 1000 + N.GRACE_MS), 'lapsed');
  assert.equal(N.status(null, 0), 'free');
});

test('base58 round trip, leading zeros', () => {
  for (const b of [new Uint8Array(32), Uint8Array.from({ length: 32 }, (_, i) => i), new Uint8Array(32).fill(255)]) {
    assert.deepEqual(N.base58.decode(N.base58.encode(b)), b);
  }
  assert.equal(N.base58.encode(new Uint8Array(32)), '11111111111111111111111111111111');
});

test('resolve, reverse and call against a stub RPC (getAccountView, then getAccount)', async () => {
  const program = N.idString(new Uint8Array(32).fill(7));
  const a = N.idString(N.unhex(A));
  const nameAcc = N.idString(await N.nameAccountId(program, 'alice'));
  const rec = Array.from(N.unhex(V.record)); // expires t0 + TERM
  const user = Array.from(N.unhex(V.user));
  const view = { [nameAcc]: rec, [a]: user };
  const calls = [];
  const rpc = { async call(m, p) {
    calls.push(m);
    if (m !== 'getAccountView') throw new Error(m);
    const s = view[p[0].account_id];
    return s ? { nonce: 0, data: { actor_states: { [program]: s } } } : null;
  } };
  const names = new N.Names({ rpc, program });
  const now = V.t0 + 1;
  assert.equal((await names.resolve('Alice', now)).owner, a);
  assert.equal(await names.resolve('alice', V.t0 + N.TERM_MS), null, 'expired');
  assert.equal(await names.reverse(a, now), 'alice');
  assert.equal(await names.reverse(N.idString(new Uint8Array(32).fill(9)), now), null);
  const c = await names.call(a, { kind: 'Transfer', name: 'alice', to: N.idString(new Uint8Array(32).fill(9)), nowMs: now });
  assert.equal(c.actors.length, 2, 'an offer touches the owner and the name only');
  assert.equal(c.evidence.length, 0, 'the name holds a record: no PDA evidence');
  const acc = await names.call(N.idString(new Uint8Array(32).fill(9)), { kind: 'Accept', name: 'alice', nowMs: now });
  assert.equal(acc.actors.length, 2, 'accepting needs no actor of the old owner (v2)');
  const fg = await names.call(a, { kind: 'Forget', names: ['alice', 'nobody'] });
  assert.equal(fg.actors.length, 3);
  assert.equal(fg.evidence.length, 1, 'evidence for the name account that holds nothing');
  await assert.rejects(names.call(a, { kind: 'Register', name: 'carol', nowMs: now }), /salt/);
  const fresh = await names.call(a, { kind: 'Register', name: 'carol', nowMs: now, salt: SALT });
  assert.equal(fresh.evidence.length, 1);
  assert.deepEqual(fresh.evidence[0].pda.seed, await N.nameSeed('carol'));
  await assert.rejects(names.call(a, { kind: 'Register', name: 'faucet' }), /reserved/);
  const cc = await names.commitCall(a, 'Dave', { salt: SALT, nowMs: now });
  assert.deepEqual([cc.name, cc.actors.length, cc.evidence.length, cc.revealFrom, cc.revealUntil], ['dave', 2, 1, now + 240_000, now + 3_600_000]);
  assert.equal(N.hex(cc.actors[1].account), N.hex(await N.sequenceAccountId(program)), 'the counter is declared');
  assert.equal(N.hex(cc.commitment), N.hex(await N.commitment('dave', a, SALT)));
  assert.ok(calls.every((m) => m === 'getAccountView'));
});
