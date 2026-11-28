// Both LEZ wire versions through the messages client. Every sequencer answer is a stubbed fetch; no test opens a socket.
import test from 'node:test';
import assert from 'node:assert/strict';
import * as C from '../client.js';
import * as M from '../ferrata-msg.js';
import * as rc3 from '../../wallet/lez.js';
import * as v040 from '../../wallet/lez040.js';
import { connect } from '../../wallet/net.js';

const PROGRAM = new Uint8Array(32).fill(7);
const SEED = Uint8Array.from({ length: 64 }, (_, i) => i + 1);
const GATEWAY_ERROR = '{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}';

function rpc(t, answer) {
  const calls = [];
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    const request = JSON.parse(init.body);
    calls.push(request);
    const a = answer(request.method, request.params);
    if (a?.raw) return new Response(a.raw, { status: a.status, headers: { 'Content-Type': 'application/json' } });
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: request.id, ...(a.error ? { error: a.error } : { result: a.result }) }), {
      status: 200, headers: { 'Content-Type': 'application/json' },
    });
  });
  return calls;
}

function profileState(secret) {
  const k = M.publicRecord(M.deriveKeys(secret, 0), { hybrid: false });
  return new rc3.Writer().u8(1).u32(k.epoch).fixed(k.x25519, 32).u8(0) // Some(X25519-only key)
    .u8(0).u32(0).u32(0).u32(0).u64(0).u64(0).done(); // open, three empty lists, zero counters
}
const accountState = (state) => ({ data: { actor_states: { [M.idToString(PROGRAM)]: Array.from(state) } } });
const notFound = { error: { code: -32601, message: 'Method not found' } };
const feeState = { base_fee_exec: 1, base_fee_stor: 1 };

async function messagingFixture(t, version, receipt = true) {
  const calls = rpc(t, (method, params) => {
    if (method === 'getSequencerStatus') return version === 'v0.4.0' ? { result: { state: 'online', height: 8 } } : notFound;
    if (method === 'getAccount') return { result: params[0] === M.idToString(recipient.accountId) ? accountState(recipientProfile) : {} };
    if (method === 'getFeeState') return { result: feeState };
    if (method === 'getAccountsNonces') return { result: [3] };
    if (method === 'sendTransaction') { wire = params[0]; return { result: null }; }
    if (method === 'getTransaction') return { result: { block_id: 12 } };
    if (method === 'getTransactionReceipt') { receiptHash = params[0]; return { result: { success: receipt } }; }
    throw new Error(`unexpected RPC ${method}`);
  });
  const net = await connect('http://sequencer.test');
  const sender = net.deriveAccount(SEED, '/0'), recipient = net.deriveAccount(SEED, '/1');
  const recipientProfile = profileState(net.messagingSecretOf(recipient));
  let wire = null, receiptHash = null;
  return { net, sender, recipient, calls, sendOptions: { nowMs: 1_800_000_000_000n, sentMs: 1_800_000_000_000n },
    sentWire: () => wire, receiptHash: () => receiptHash };
}

test('v0.4.0 send: decoded public call, nonce-keyed witness, and a receipt outcome', async (t) => {
  const f = await messagingFixture(t, 'v0.4.0');
  const r = await C.send(f.net.seq, { node: f.sender, program: PROGRAM, to: f.recipient.accountId, text: 'hello v0.4.0', net: f.net, ...f.sendOptions });
  assert.deepEqual({ block: r.block, reverted: r.reverted }, { block: 12, reverted: false });
  const decoded = v040.decodeWire(f.sentWire());
  assert.equal(decoded.kind, 'public');
  assert.equal(decoded.hash, r.hash);
  const tx = decoded.tx;
  assert.equal(tx.data[0], 4); // ferrata_msg_v2 MsgMessage::Send
  assert.deepEqual(tx.to, { account: f.sender.accountId, program: PROGRAM });
  assert.deepEqual(tx.nonces, [[f.sender.accountId, 3n]]);
  assert.equal(tx.entries.length, 1);
  const [entry] = tx.entries;
  assert.equal(entry.auth_data.length, 96); // a nonzero nonce carries just signature and public key
  const sig = entry.auth_data.subarray(0, 64), pk = entry.auth_data.subarray(64);
  assert.deepEqual(pk, v040.signingKey(f.sender.secret, 3n).pk);
  assert.deepEqual(entry.next, v040.keyState(f.sender.secret, 4n));
  const keyed = v040.messageWithKeys(tx.messageBytes, tx.entries.map((e) => e.next), 'public');
  assert.ok(v040.verify(sig, keyed, pk));
  assert.equal(f.receiptHash(), r.hash);
  assert.equal(f.calls.filter((c) => c.method === 'getSequencerStatus').length, 1);
  assert.ok(!f.calls.some((c) => c.method === 'getBlock'), 'v0.4.0 reads its receipt, not rc3 block outcomes');
});

async function rc3CallBytes(t, gateway) {
  let recipientProfile = null, recipientId = null, wire = null;
  const calls = rpc(t, (method, params) => {
    if (method === 'getSequencerStatus') {
      if (gateway) return { raw: GATEWAY_ERROR, status: 403 };
      return notFound;
    }
    if (method === 'getAccount') return { result: params[0] === M.idToString(recipientId) ? accountState(recipientProfile) : {} };
    if (method === 'getFeeState') return { result: feeState };
    if (method === 'getAccountsNonces') return { result: [3] };
    if (method === 'sendTransaction') { wire = params[0]; return { result: null }; }
    throw new Error(`unexpected RPC ${method}`);
  });
  t.mock.method(globalThis.crypto, 'getRandomValues', (b) => b.fill(9));
  const net = await connect(gateway ? 'http://gateway.test/rpc' : 'http://sequencer.test');
  assert.equal(net.version, 'v0.4.0-rc3');
  const node = net.deriveAccount(SEED, '/0'), recipient = net.deriveAccount(SEED, '/1');
  recipientId = recipient.accountId;
  recipientProfile = profileState(net.messagingSecretOf(recipient));
  const nowMs = 1_800_000_000_000n;
  const r = await C.send(net.seq, { node, program: PROGRAM, to: recipientId, text: 'hello rc3', nowMs, sentMs: nowMs, wait: false, net });
  const key = M.publicRecord(M.deriveKeys(net.messagingSecretOf(recipient), 0), { hybrid: false });
  const body = M.seal({ sender: node.accountId, recipient: recipientId, key, text: 'hello rc3', sentMs: nowMs });
  const accounts = [recipientId, M.inboxAccount(PROGRAM, recipientId), M.envelopeAccount(PROGRAM, recipientId, node.accountId, 0n)];
  const to = C.actorOf(node.accountId, PROGRAM), actors = [to, ...accounts.map((account) => C.actorOf(account, PROGRAM))];
  const data = M.encodeSend({ to: recipientId, nowMs, seq: 0n, body });
  const expected = rc3.buildCall({ payer: node, to, actors, data, nonce: 3n, gasLimit: C.gasFor('send'), maxFee: r.maxFee });
  assert.equal(wire, expected.wire, 'the selected rc3 path emits exactly the old message bytes');
  const decoded = rc3.decodeWire(wire);
  assert.equal(decoded.kind, 'public');
  assert.ok(rc3.verify(decoded.tx.witnesses[0].sig, rc3.messageHash(decoded.tx.messageBytes), node.pk));
  assert.equal(calls.filter((c) => c.method === 'getSequencerStatus').length, 1);
}

test('rc3 stand-in: message calls keep the original public transaction bytes', async (t) => {
  await rc3CallBytes(t, false);
});

test('the gateway exact HTTP 403 -32601 reply keeps the original rc3 transaction bytes', async (t) => {
  await rc3CallBytes(t, true);
});

test('an unknown version refuses before signing and sends no transaction', async (t) => {
  const calls = rpc(t, (method) => {
    if (method === 'getSequencerStatus') return { result: null };
    if (method === 'getFeeState') return { result: feeState };
    if (method === 'getAccountsNonces') return { result: [3] };
    throw new Error(`unexpected RPC ${method}`);
  });
  const net = await connect('http://unknown.test');
  assert.equal(net.version, 'unknown');
  const node = rc3.deriveNode(SEED, [0]);
  await assert.rejects(C.call(net.seq, { node, program: PROGRAM, data: Uint8Array.of(1, 0), gasLimit: 12_345n, net }), (e) => e.message === net.error);
  assert.match(net.error, /could not be read, so nothing is signed/);
  assert.ok(!calls.some((c) => c.method === 'sendTransaction'));
});
