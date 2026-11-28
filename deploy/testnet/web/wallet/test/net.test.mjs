// Network selection with the sequencer stubbed: rc3 remains byte-for-byte on lez.js, while v0.4.0 uses account secrets,
// rotating witnesses and receipt outcomes. No test contacts a network.
import test from 'node:test';
import assert from 'node:assert/strict';
import * as rc3 from '../lez.js';
import * as v040 from '../lez040.js';
import { connect } from '../net.js';

const SEED = Uint8Array.from({ length: 64 }, (_, i) => i + 1);
const AUX = new Uint8Array(32).fill(9);

function rpc(t, answer) {
  return t.mock.method(globalThis, 'fetch', async (_url, init) => {
    const req = JSON.parse(init.body);
    const body = answer(req.method, req.params);
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...body }), {
      status: 200, headers: { 'Content-Type': 'application/json' },
    });
  });
}
const rc3Answer = () => ({ error: { code: -32601, message: 'Method not found' } });
const v040Answer = () => ({ result: { state: 'online', height: 7 } });

test('connect: v0.4.0\'s -31902 not-online answer selects v0.4.0, not an unreadable version', async (t) => {
  rpc(t, (method) => {
    assert.equal(method, 'getSequencerStatus');
    return { error: { code: -31902, message: 'Sequencer is not online' } };
  });
  const net = await connect('http://sequencer.test');
  assert.equal(net.version, 'v0.4.0');
  assert.equal(net.codec, v040);
  assert.equal(net.readOnly, false);
});

test('connect: an rc3 method-not-found answer selects lez.js and its ordinary transfer', async (t) => {
  rpc(t, rc3Answer);
  const net = await connect('http://sequencer.test');
  assert.equal(net.version, 'v0.4.0-rc3');
  assert.equal(net.codec, rc3);
  const from = net.deriveAccount(SEED, '/0'), to = net.deriveAccount(SEED, [1]);
  const key = net.admissionKeyOf(to);
  assert.deepEqual(key, to.pk);
  const tx = net.buildTransfer({ from, to: to.accountId, toKey: key, amount: 123n, nonce: 7n, gasLimit: 1000n, maxFee: 5000n, aux: AUX });
  const decoded = rc3.decodeWire(tx.wire);
  assert.equal(decoded.kind, 'public');
  assert.deepEqual(decoded.tx.evidence[0].key, to.pk);
  assert.ok(rc3.verify(decoded.tx.witnesses[0].sig, rc3.messageHash(decoded.tx.messageBytes), from.pk));
});

async function v040Transfer(nonce) {
  const net = await connect('http://sequencer.test');
  assert.equal(net.version, 'v0.4.0');
  assert.equal(net.codec, v040);
  const from = net.deriveAccount(SEED, '/0'), to = net.deriveAccount(SEED, '/1');
  const tx = net.buildTransfer({ from, to: to.accountId, toKey: net.admissionKeyOf(to), amount: 123n, nonce,
    gasLimit: 1000n, maxFee: 5000n, aux: AUX });
  const decoded = v040.decodeWire(tx.wire);
  const entry = decoded.tx.entries[0], initial = nonce === 0n ? 33 : 0;
  const signing = v040.signingKey(from.secret, nonce);
  assert.equal(entry.auth_data.length, initial + 96);
  assert.deepEqual(entry.auth_data.subarray(0, initial), nonce === 0n ? v040.initialKey(from.secret) : new Uint8Array());
  assert.deepEqual(entry.auth_data.subarray(initial + 64), signing.pk);
  assert.deepEqual(entry.next, v040.keyState(from.secret, nonce + 1n));
  assert.ok(v040.verify(entry.auth_data.subarray(initial, initial + 64), v040.unhex(tx.messageWithKeys), signing.pk));
}

test('connect: v0.4.0 nonce zero carries /0 initial-key evidence and signs with its nonce-zero key', async (t) => {
  rpc(t, v040Answer);
  await v040Transfer(0n);
});

test('connect: v0.4.0 nonce three signs with the nonce-three key and rotates to four', async (t) => {
  rpc(t, v040Answer);
  await v040Transfer(3n);
});

test('admission keys: both valid forms and plain-words refusal of the other version', async (t) => {
  let fetch = rpc(t, rc3Answer);
  const oldNet = await connect('http://sequencer.test');
  const oldNode = oldNet.deriveAccount(SEED, [0]);
  const oldParsed = oldNet.parseAdmissionKey(rc3.hex(oldNet.admissionKeyOf(oldNode)));
  assert.deepEqual(oldParsed, { id: oldNode.accountId, key: oldNode.pk });
  fetch.mock.restore();

  fetch = rpc(t, v040Answer);
  const newNet = await connect('http://sequencer.test');
  const newNode = newNet.deriveAccount(SEED, [0]);
  const initial = newNet.admissionKeyOf(newNode);
  assert.equal(initial.length, 33);
  assert.equal(initial[0], 0);
  assert.deepEqual(newNet.parseAdmissionKey(v040.hex(initial)), { id: newNode.accountId, key: initial });
  assert.throws(() => newNet.parseAdmissionKey(rc3.hex(oldNode.pk)), /rc3.*v0\.4\.0 network needs.*initial key state/);
  assert.throws(() => oldNet.parseAdmissionKey(v040.hex(initial)), /v0\.4\.0.*rc3 network needs.*public key/);
});

test('importAccount: rc3 imports a raw signing key; v0.4.0 imports its account secret', async (t) => {
  let fetch = rpc(t, rc3Answer);
  const oldNet = await connect('http://sequencer.test');
  assert.deepEqual(oldNet.importAccount('11'.repeat(32)), rc3.rawKeyNode('11'.repeat(32)));
  fetch.mock.restore();
  rpc(t, v040Answer);
  const newNet = await connect('http://sequencer.test');
  const account = newNet.importAccount('07'.repeat(32));
  assert.deepEqual(account.accountId, v040.publicAccountId(account.secret));
  assert.deepEqual(newNet.admissionKeyOf(account), v040.initialKey(account.secret));
});

test('connect: an answer naming no version is read-only and every signing path refuses plainly', async (t) => {
  rpc(t, () => ({ result: null }));
  const net = await connect('http://sequencer.test');
  assert.equal(net.readOnly, true);
  assert.equal(net.codec, null);
  assert.match(net.error, /the network's LEZ version could not be read, so nothing is signed:/);
  assert.throws(() => net.buildTransfer({}), /could not be read, so nothing is signed/);
  assert.throws(() => net.buildCall({}), /could not be read, so nothing is signed/);
  assert.throws(() => net.deriveAccount(SEED, [0]), /could not be read, so nothing is signed/);
});

test('outcome: v0.4.0 receipts distinguish applied, reverted and missing', async (t) => {
  rpc(t, (method, params) => {
    if (method === 'getSequencerStatus') return v040Answer();
    assert.equal(method, 'getTransactionReceipt');
    return { result: params[0] === 'unknown' ? null : { success: params[0] === 'good' } };
  });
  const net = await connect('http://sequencer.test');
  assert.equal(await net.outcome('good'), 'applied');
  assert.equal(await net.outcome('bad'), 'reverted');
  assert.equal(await net.outcome('unknown'), 'missing');
});

test('outcome: rc3 reads the transaction position in the block reverted list', async (t) => {
  rpc(t, rc3Answer);
  const net = await connect('http://sequencer.test');
  const block = { transactions: [{ hash: 'good' }, { hash: 'bad' }], reverted: [1] };
  assert.equal(await net.outcome('good', block), 'applied');
  assert.equal(await net.outcome('bad', block), 'reverted');
  assert.equal(await net.outcome('unknown', block), 'missing');
});

// The deployed rc3 network's gateway (deploy/testnet/host/bin/ferrata-gateway) answers a method it does not forward
// with HTTP 403 and a JSON-RPC -32601 body: the page must read that as rc3 and keep signing (review P4 r1, High 2).
function rawRpc(t, status, body) {
  return t.mock.method(globalThis, 'fetch', async () => new Response(body, { status, headers: { 'Content-Type': 'application/json' } }));
}

test('connect: the rc3 gateway\'s HTTP 403 + -32601 for getSequencerStatus is rc3, and the page still signs', async (t) => {
  rawRpc(t, 403, '{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}');
  const net = await connect('http://gateway.test/rpc');
  assert.equal(net.readOnly, false);
  assert.equal(net.version, 'v0.4.0-rc3');
  assert.equal(net.codec, rc3);
  const from = net.deriveAccount(SEED, '/0'), to = net.deriveAccount(SEED, [1]);
  const tx = net.buildTransfer({ from, to: to.accountId, toKey: net.admissionKeyOf(to), amount: 1n, nonce: 0n, gasLimit: 1000n, maxFee: 5000n, aux: AUX });
  assert.equal(rc3.decodeWire(tx.wire).kind, 'public');
});

test('connect: on rc3 the page\'s short version-probe timeout leaves rc3\'s calls at lez.js\'s 15 s, as before the port', async (t) => {
  rawRpc(t, 403, '{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}');
  const net = await connect('http://gateway.test/rpc', { timeoutMs: 8000 });
  assert.equal(net.version, 'v0.4.0-rc3');
  assert.equal(net.seq.timeoutMs, 15000);
});

test('connect: any other answer names no version: another JSON-RPC error, an HTTP error page, a 200 without JSON', async (t) => {
  for (const [status, body] of [
    [200, '{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"Internal error"}}'],
    [403, '{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"forbidden"}}'],
    [500, '<html>Internal Server Error</html>'],
    [200, 'not json'],
  ]) {
    const f = rawRpc(t, status, body);
    const net = await connect('http://gateway.test/rpc');
    assert.equal(net.readOnly, true, `${status} ${body}`);
    assert.throws(() => net.buildTransfer({}), /could not be read, so nothing is signed/);
    f.mock.restore();
  }
});

// The page's options (app.js): a failed probe must not lock the user out (offline PWA, a rate limit, a sequencer restart).
function memoryStorage(init = {}) {
  const m = new Map(Object.entries(init));
  return { getItem: (k) => (m.has(k) ? m.get(k) : null), setItem: (k, v) => { m.set(k, String(v)); }, m };
}
const PAGE_URL = 'http://page.test/rpc';

test('page: offline, accounts follow the fallback version and nothing is signed until a re-read finds that version', async (t) => {
  const storage = memoryStorage();
  let online = false;
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    if (!online) throw new TypeError('Failed to fetch');
    const req = JSON.parse(init.body);
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...rc3Answer() }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  });
  const net = await connect(PAGE_URL, { storage, fallback: 'v0.4.0-rc3' });
  assert.deepEqual({ version: net.version, readOnly: net.readOnly, accountsAs: net.accountsAs, is040: net.is040 },
    { version: 'unknown', readOnly: true, accountsAs: 'v0.4.0-rc3', is040: false });
  const from = net.deriveAccount(SEED, '/0'), to = net.deriveAccount(SEED, [1]);
  assert.deepEqual(from.accountId, rc3.deriveNode(SEED, [0]).accountId); // unlock, receive code, recovery views work
  assert.deepEqual(net.admissionKeyOf(to), to.pk);
  assert.equal(net.accountSecretHex(from), rc3.hex(from.ssk));
  const args = { from, to: to.accountId, toKey: to.pk, amount: 1n, nonce: 0n, gasLimit: 1000n, maxFee: 5000n, aux: AUX };
  assert.throws(() => net.buildTransfer(args), /could not be read, so nothing is signed: Failed to fetch/);
  await assert.rejects(net.outcome('00'.repeat(32)), /could not be read/);
  assert.equal(await net.recheck(), false); // still offline
  assert.equal(net.readOnly, true);
  online = true;
  assert.equal(await net.recheck(), true);
  assert.deepEqual({ version: net.version, readOnly: net.readOnly, error: net.error }, { version: 'v0.4.0-rc3', readOnly: false, error: null });
  const tx = net.buildTransfer(args);
  assert.equal(tx.wire, rc3.buildTransfer(args).wire);
  assert.equal(storage.getItem(`ferrata.lezVersion ${PAGE_URL}`), 'v0.4.0-rc3');
});

test('page: the version last read from the RPC is used offline; a re-read finding another version refuses until a reload', async (t) => {
  const storage = memoryStorage({ [`ferrata.lezVersion ${PAGE_URL}`]: 'v0.4.0' });
  let answer = null;
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    if (!answer) return new Response('rate limited', { status: 429, headers: { 'Retry-After': '0' } });
    const req = JSON.parse(init.body);
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...answer() }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  });
  t.mock.method(globalThis, 'setTimeout', (fn) => { fn(); return 0; }); // the 429 back-off without waiting
  const net = await connect(PAGE_URL, { storage, fallback: 'v0.4.0-rc3' });
  assert.deepEqual({ version: net.version, accountsAs: net.accountsAs, is040: net.is040 }, { version: 'unknown', accountsAs: 'v0.4.0', is040: true });
  const from = net.deriveAccount(SEED, '/0');
  assert.deepEqual(from.accountId, v040.deriveNode(SEED, [0]).accountId);
  assert.deepEqual(net.admissionKeyOf(from), v040.initialKey(from.secret));
  answer = rc3Answer; // the network now answers as an rc3 sequencer
  assert.equal(await net.recheck(), false);
  assert.equal(net.mismatch, true);
  assert.match(net.error, /^refused before anything was signed or sent: the network runs LEZ v0\.4\.0-rc3, but this page's accounts follow LEZ v0\.4\.0; reload the page$/);
  assert.throws(() => net.buildTransfer({ from, to: from.accountId, amount: 1n, nonce: 0n, gasLimit: 1000n, maxFee: 1n }), /reload the page/);
  assert.equal(storage.getItem(`ferrata.lezVersion ${PAGE_URL}`), 'v0.4.0-rc3'); // the next load uses what was read
});

test('page: a confirmed probe keeps its version for the next load; the CLI (no options) still refuses accounts on a failed probe', async (t) => {
  const storage = memoryStorage();
  rpc(t, v040Answer);
  const net = await connect(PAGE_URL, { storage, fallback: 'v0.4.0-rc3' });
  assert.deepEqual({ version: net.version, readOnly: net.readOnly, accountsAs: net.accountsAs }, { version: 'v0.4.0', readOnly: false, accountsAs: 'v0.4.0' });
  assert.equal(await net.recheck(), true);
  assert.equal(storage.getItem(`ferrata.lezVersion ${PAGE_URL}`), 'v0.4.0');
  t.mock.method(globalThis, 'fetch', async () => { throw new TypeError('Failed to fetch'); });
  const cli = await connect(PAGE_URL);
  assert.equal(cli.accountsAs, null);
  assert.throws(() => cli.deriveAccount(SEED, '/0'), /could not be read, so nothing is signed/);
});

test('page: another version, once read, is final until a reload; a later read of the expected one signs nothing', async (t) => {
  const storage = memoryStorage({ [`ferrata.lezVersion ${PAGE_URL}`]: 'v0.4.0-rc3' });
  let answer = null, probes = 0;
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    const req = JSON.parse(init.body);
    if (req.method === 'getSequencerStatus') probes++;
    if (!answer) throw new TypeError('Failed to fetch');
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...answer() }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  });
  const net = await connect(PAGE_URL, { storage, fallback: 'v0.4.0-rc3' });
  const from = net.deriveAccount(SEED, '/0'), to = net.deriveAccount(SEED, [1]);
  const args = { from, to: to.accountId, toKey: to.pk, amount: 1n, nonce: 0n, gasLimit: 1000n, maxFee: 5000n, aux: AUX };
  answer = v040Answer; // a v0.4.0 sequencer answers (the accounts shown are rc3's)
  assert.equal(await net.recheck(), false);
  assert.equal(net.mismatch, true);
  answer = rc3Answer; // then the expected version again (another sequencer behind the same URL)
  const before = probes;
  assert.equal(await net.recheck(), false);
  assert.equal(probes, before, 'no read after a mismatch: it is final');
  assert.deepEqual({ readOnly: net.readOnly, version: net.version }, { readOnly: true, version: 'unknown' });
  assert.throws(() => net.buildTransfer(args), /reload the page/);
  await assert.rejects(net.outcome('00'.repeat(32)), /reload the page/);
});

test('page: re-reads asked for together share one read (the timer, the online event and a send)', async (t) => {
  const storage = memoryStorage();
  let answer = null, probes = 0, release;
  const gate = new Promise((r) => { release = r; });
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    const req = JSON.parse(init.body);
    if (!answer) throw new TypeError('Failed to fetch');
    if (req.method === 'getSequencerStatus') { probes++; await gate; }
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...answer() }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  });
  const net = await connect(PAGE_URL, { storage, fallback: 'v0.4.0-rc3' });
  answer = v040Answer; // the other version: both callers must see the refusal, from one read
  const a = net.recheck(), b = net.recheck(), c = net.recheck();
  release();
  assert.deepEqual(await Promise.all([a, b, c]), [false, false, false]);
  assert.equal(probes, 1);
  assert.equal(net.mismatch, true);
  assert.equal(net.readOnly, true);
});

test('page: a page confirmed at load reads the version again before each signature; a move to another LEZ is final', async (t) => {
  const storage = memoryStorage();
  let answer = rc3Answer, probes = 0, fail = false;
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    const req = JSON.parse(init.body);
    if (req.method === 'getSequencerStatus') probes++;
    if (fail) throw new TypeError('Failed to fetch');
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...answer() }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  });
  const net = await connect(PAGE_URL, { storage, fallback: 'v0.4.0-rc3' });
  assert.deepEqual({ version: net.version, readOnly: net.readOnly }, { version: 'v0.4.0-rc3', readOnly: false });
  const from = net.deriveAccount(SEED, '/0'), to = net.deriveAccount(SEED, [1]);
  const args = { from, to: to.accountId, toKey: to.pk, amount: 1n, nonce: 0n, gasLimit: 1000n, maxFee: 5000n, aux: AUX };
  assert.equal(await net.recheck(), true);
  assert.equal(probes, 2, 'one read at load, one before the signature');
  net.buildTransfer(args); // still the same LEZ: it signs
  fail = true; // a failed read refuses that signature only
  assert.equal(await net.recheck(), false);
  assert.equal(net.readOnly, false);
  assert.match(net.error, /could not be read, so nothing is signed: Failed to fetch/);
  fail = false;
  assert.equal(await net.recheck(), true);
  assert.equal(net.error, null);
  answer = v040Answer; // the network moved to LEZ v0.4.0 while the page was open
  assert.equal(await net.recheck(), false);
  assert.deepEqual({ mismatch: net.mismatch, readOnly: net.readOnly, version: net.version }, { mismatch: true, readOnly: true, version: 'unknown' });
  assert.match(net.error, /^refused before anything was signed or sent: the network runs LEZ v0\.4\.0, but this page's accounts follow LEZ v0\.4\.0-rc3; reload the page$/);
  assert.throws(() => net.buildTransfer(args), /reload the page/);
  answer = rc3Answer;
  const before = probes;
  assert.equal(await net.recheck(), false);
  assert.equal(probes, before, 'final: no read after a mismatch');
  assert.equal(storage.getItem(`ferrata.lezVersion ${PAGE_URL}`), 'v0.4.0', 'the next load follows what was read');
});
