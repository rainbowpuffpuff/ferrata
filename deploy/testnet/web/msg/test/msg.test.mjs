// node --test deploy/testnet/web/msg/test/*.test.mjs   (node 20+). The library alone: keys, envelopes (v2's and v1's),
// borsh, PDAs, inbox listing, the preflight; and the borsh vectors (test/vectors-v2.json, test/vectors-v1.json) that
// lez/msg-v2-wire's and lez/msg-wire's Rust tests decode and re-encode from the other side.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as M from '../ferrata-msg.js';

const A = new Uint8Array(32).fill(0x41), B = new Uint8Array(32).fill(0x42), C = new Uint8Array(32).fill(0x43);
const secretB = new Uint8Array(32).fill(7);
const keysB = M.deriveKeys(secretB, 0);
const recB = M.publicRecord(keysB);
const load = (name) => JSON.parse(readFileSync(new URL(`./${name}`, import.meta.url)));
const v2 = load('vectors-v2.json');
const v1 = load('vectors-v1.json');
const unhex = (s) => Uint8Array.from(Buffer.from(s, 'hex'));
const hx = (b) => M.hex(b);

test('keys are deterministic, epoch-separated, and the right sizes', () => {
  const again = M.deriveKeys(secretB, 0);
  assert.deepEqual(again.x25519.publicKey, keysB.x25519.publicKey);
  assert.deepEqual(again.mlkem768.publicKey, keysB.mlkem768.publicKey);
  assert.notDeepEqual(M.deriveKeys(secretB, 1).x25519.publicKey, keysB.x25519.publicKey);
  assert.equal(recB.mlkem768.length, M.MLKEM768_PK_LEN);
  assert.ok(M.hasFingerprint(recB, M.fingerprint(recB)));
  assert.ok(!M.hasFingerprint(recB, M.fingerprint(M.publicRecord(M.deriveKeys(secretB, 1)))));
});

test('capacities: the numbers docs/design/messages.md states; a hybrid envelope never fits below 2048 B', () => {
  assert.equal(M.capacity(M.V_X25519, 512), 424);
  assert.equal(M.capacity(M.V_HYBRID, 2048), 872);
  assert.equal(M.capacity(M.V_HYBRID, 4096), 2920);
  assert.ok(M.capacity(M.V_HYBRID, 1024) < 0);
  assert.equal(M.bucketFor(M.V_HYBRID, 0), 2048);
  assert.equal(M.bucketFor(M.V_X25519, 425), 1024);
  assert.throws(() => M.bucketFor(M.V_HYBRID, 2921));
  assert.throws(() => M.seal({ sender: A, recipient: B, key: recB, text: 'x', bucket: 1024 }));
});

test('v2 round trip (versions 3 and 4); every envelope is exactly a bucket; 1 byte and 380 bytes look alike', () => {
  for (const text of ['h', 'x'.repeat(380), 'héllo ✉ 🔐', 'y'.repeat(2920)]) {
    const body = M.seal({ sender: A, recipient: B, key: recB, text, sentMs: 1234 });
    assert.ok(M.BUCKETS.includes(body.length));
    assert.equal(body[0], M.V_HYBRID);
    const o = M.open({ body, sender: A, recipient: B, keys: keysB });
    assert.equal(o.text, text);
    assert.equal(o.sentMs, 1234n);
  }
  const classic = { ...recB, mlkem768: null };
  const a = M.seal({ sender: A, recipient: B, key: classic, text: 'h' });
  const b = M.seal({ sender: A, recipient: B, key: classic, text: 'x'.repeat(380) });
  assert.equal(a.length, 512); assert.equal(b.length, 512); assert.equal(a[0], M.V_X25519);
  assert.equal(M.open({ body: b, sender: A, recipient: B, keys: keysB }).text, 'x'.repeat(380));
});

test('the HKDF info of a v2 envelope is 40 bytes; a v1 hybrid one was 1,253 (review finding L1)', () => {
  const body = M.seal({ sender: A, recipient: B, key: recB, text: 'pq' });
  const header = body.subarray(0, 1 + 4 + 32 + M.MLKEM768_CT_LEN + 24);
  assert.equal(M.envelopeInfo(M.V_HYBRID, header, recB.x25519, A, B).length, 40);
  assert.equal(M.envelopeInfo(M.LEGACY_HYBRID, header, recB.x25519, A, B).length, 1253);
});

test('a fresh ephemeral key per message: the same text twice gives unrelated envelopes', () => {
  const x = M.seal({ sender: A, recipient: B, key: recB, text: 'same', sentMs: 1 });
  const y = M.seal({ sender: A, recipient: B, key: recB, text: 'same', sentMs: 1 });
  assert.notDeepEqual(x.subarray(5, 37), y.subarray(5, 37));
  assert.notDeepEqual(x.subarray(-100), y.subarray(-100));
});

test('tampering, another sender, another recipient, another key, a too-small envelope: none opens', () => {
  const body = M.seal({ sender: A, recipient: B, key: recB, text: 'secret' });
  for (const i of [6, 40, 1100, body.length - 1]) {
    const t = body.slice(); t[i] ^= 1;
    assert.throws(() => M.open({ body: t, sender: A, recipient: B, keys: keysB }), `byte ${i}`);
  }
  assert.throws(() => M.open({ body, sender: B, recipient: B, keys: keysB }));
  assert.throws(() => M.open({ body, sender: A, recipient: A, keys: keysB }));
  assert.throws(() => M.open({ body, sender: A, recipient: B, keys: M.deriveKeys(new Uint8Array(32).fill(8), 0) }));
  const junk = new Uint8Array(512); junk[0] = M.V_HYBRID;
  assert.throws(() => M.open({ body: junk, sender: A, recipient: B, keys: keysB }), /cannot hold/);
});

test('hybrid: the X25519 half alone does not open it (the ML-KEM secret is needed)', () => {
  const body = M.seal({ sender: A, recipient: B, key: recB, text: 'pq' });
  const wrongKem = { ...keysB, mlkem768: M.deriveKeys(new Uint8Array(32).fill(9), 0).mlkem768 };
  assert.throws(() => M.open({ body, sender: A, recipient: B, keys: wrongKem }));
});

test('v2 messages match the vectors the Rust side checks; Delete sorts and deduplicates', () => {
  const m = v2.messages;
  assert.equal(hx(M.encodePublish(recB)), m.publish);
  assert.equal(hx(M.encodeSetPolicy(M.POLICY.contactsOnly)), m.set_policy_contacts_only);
  assert.equal(hx(M.encodeContacts([A], [B])), m.contacts_add_a_remove_b);
  assert.equal(hx(M.encodeBlock([B], [])), m.block_b);
  assert.equal(hx(M.encodeSend({ to: B, nowMs: 1790000000000n, seq: 5n, body: Uint8Array.from([3, 0, 0, 0, 0, 9]) })), m.send);
  assert.equal(hx(M.encodeDelete([5n, 3n, 5n])), m.delete_3_5);
  assert.equal(hx(M.encodeClear(9n)), m.clear_9);
  assert.throws(() => M.encodeDelete([]));
  assert.throws(() => M.encodeDelete(Array.from({ length: 33 }, (_, i) => i)));
});

test('a list edit names at most 64 ids each way (review N2: the gas limit grows with them, up to 64 + 64)', () => {
  const ids = (n, v) => Array.from({ length: n }, (_, i) => Uint8Array.from({ length: 32 }, () => (i + v) & 0xff));
  assert.equal(M.encodeBlock(ids(64, 1), ids(64, 2)).length, 1 + 4 + 64 * 32 + 4 + 64 * 32);
  assert.throws(() => M.encodeBlock(ids(65, 1), []), /at most 64/);
  assert.throws(() => M.encodeContacts([], ids(65, 1)), /at most 64/);
  const full = ids(64, 1);
  assert.equal(M.preflightEdit(full, ids(1, 200), []), 'LIST_FULL');
  assert.equal(M.preflightEdit(full, ids(1, 200), full.slice(0, 1)), null, 'one out, one in');
  assert.equal(M.preflightEdit(full, full.slice(0, 64), ids(64, 100)), null, 're-adding present ids adds nothing');
});

test('v2 states decode; the inbox opens through its envelope; the PDAs are the program\'s', () => {
  const p = M.decodeProfile(unhex(v2.states.profile));
  assert.deepEqual([p.key.epoch, p.policy, p.contacts.length, p.sentAt, p.lastSendMs, p.sentTotal], [0, 'contactsOnly', 1, [1790000000000n], 1790000000000n, 3n]);
  const ix = M.decodeInbox(unhex(v2.states.inbox));
  assert.deepEqual([ix.nextN, ix.entries.length, ix.entries[0].n, ix.entries[0].contact], [2n, 1, 1n, true]);
  const env = M.decodeEnvelope(unhex(v2.states.envelope));
  const list = M.openInbox({ inbox: ix, envelopes: new Map([['1', env.body]]), recipientId: B, accountSecret: secretB });
  assert.equal(list[0].text, v2.states.text);
  assert.equal(list[0].from, M.idToString(A));
  assert.equal(M.openInbox({ inbox: ix, envelopes: new Map(), recipientId: B, accountSecret: secretB })[0].error, 'envelope not found');
  const P = unhex(v2.pdas.program);
  assert.equal(hx(M.inboxAccount(P, B)), v2.pdas.inbox_b);
  assert.equal(hx(M.envelopeAccount(P, B, A, 0n)), v2.pdas.envelope_b_a_0);
  assert.equal(hx(M.envelopeAccount(P, B, A, 7n)), v2.pdas.envelope_b_a_7);
  assert.equal(M.decodeProfile(new Uint8Array(0)).sentTotal, 0n);
  assert.equal(M.decodeInbox(new Uint8Array(0)), null);
  assert.equal(M.decodeEnvelope(new Uint8Array(0)), null);
});

test('v1 (legacy, live): a mailbox the Rust side encoded decodes and its hybrid message opens', () => {
  const m = M.decodeLegacyMailbox(unhex(v1.mailbox.hex));
  assert.equal(m.key.epoch, 0);
  assert.deepEqual(m.key.x25519, keysB.x25519.publicKey);
  assert.equal(m.inbox.length, 1);
  const list = M.readLegacyInbox(m, B, secretB);
  assert.equal(list[0].version, M.LEGACY_HYBRID);
  assert.equal(list[0].text, v1.mailbox.text);
  const legacy = M.seal({ sender: A, recipient: B, key: recB, text: 'old format', legacy: true });
  assert.equal(legacy[0], M.LEGACY_HYBRID);
  assert.equal(M.open({ body: legacy, sender: A, recipient: B, keys: keysB }).text, 'old format');
});

test('preflight: the program\'s code for every rule it checks, in its order', () => {
  const NOW = 1_790_000_000_000n;
  const empty = M.decodeProfile(null);
  const prof = (o = {}) => ({ ...empty, ...o });
  const rec = (o = {}) => prof({ key: recB, ...o });
  const inbox = (entries = []) => ({ owner: B, nextN: BigInt(entries.length), entries });
  const ok = M.seal({ sender: A, recipient: B, key: recB, text: 'hi' });
  const base = { senderId: A, recipientId: B, senderProfile: prof(), recipientProfile: rec(), recipientInbox: inbox(), body: ok, nowMs: NOW, seq: 0n };
  const pre = (o) => M.preflightSend({ ...base, ...o });
  assert.equal(pre({}), null);
  assert.equal(pre({ recipientId: A }), 'SELF_SEND');
  assert.equal(pre({ seq: 1n }), 'STALE_SEQ');
  assert.equal(pre({ body: new Uint8Array(2000) }), 'BAD_SIZE');
  const small = new Uint8Array(1024); small[0] = M.V_HYBRID;
  assert.equal(pre({ body: small }), 'BAD_SIZE');
  const v7 = new Uint8Array(2048); v7[0] = 7;
  assert.equal(pre({ body: v7 }), 'BAD_VERSION');
  assert.equal(pre({ senderProfile: prof({ lastSendMs: NOW + 1n }) }), 'CLOCK_BACKWARDS');
  const recent = Array.from({ length: 30 }, (_, i) => NOW - 3_599_000n + BigInt(i));
  assert.equal(pre({ senderProfile: prof({ sentAt: recent, lastSendMs: recent.at(-1), sentTotal: 0n }) }), 'RATE_LIMITED');
  assert.equal(pre({ senderProfile: prof({ sentAt: recent.map((t) => t - 2_000n), lastSendMs: NOW - 1n }) }), null, 'the oldest send has left the hour');
  assert.equal(pre({ recipientProfile: empty }), 'NO_KEY');
  assert.equal(pre({ recipientProfile: rec({ blocked: [A] }) }), 'BLOCKED');
  assert.equal(pre({ recipientProfile: rec({ policy: 'contactsOnly' }) }), 'NOT_A_CONTACT');
  assert.equal(pre({ recipientProfile: rec({ key: { ...recB, mlkem768: null } }) }), 'BAD_VERSION');
  const classic = M.seal({ sender: A, recipient: B, key: { ...recB, mlkem768: null }, text: 'hi' });
  assert.equal(pre({ body: classic }), 'DOWNGRADE');
  assert.equal(pre({ recipientProfile: rec({ key: { ...recB, epoch: 1 } }) }), 'STALE_KEY');
  const entry = (n, from, contact, len = 2048) => ({ n: BigInt(n), from, seq: 0n, atMs: NOW, len, contact });
  assert.equal(pre({ recipientInbox: inbox(Array.from({ length: 32 }, (_, i) => entry(i, C, true, 512))) }), 'INBOX_FULL');
  assert.equal(pre({ recipientInbox: inbox(Array.from({ length: 16 }, (_, i) => entry(i, C, true, 4096))) }), 'INBOX_FULL');
  assert.equal(pre({ recipientInbox: inbox([entry(0, A, false)]) }), 'STRANGER_LIMIT');
  const strangers = Array.from({ length: 8 }, (_, i) => entry(i, new Uint8Array(32).fill(0x60 + i), false, 512));
  assert.equal(pre({ recipientInbox: inbox(strangers) }), 'STRANGERS_FULL');
  assert.equal(pre({ recipientInbox: inbox(strangers), recipientProfile: rec({ contacts: [A] }) }), null, 'a contact gets through');
});
