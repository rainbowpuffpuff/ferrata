// Offline process tests of ferrata-msg.mjs. Every RPC URL is a stand-in bound
// to 127.0.0.1; keygen and the usage cases never contact it or any other host.
// The profile is the committed Rust/JS state vector. The test builds the other
// recorded state shapes with its own small Borsh writer, not the CLI's client.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as M from '../ferrata-msg.js';
import * as rc3 from '../../wallet/lez.js';
import * as v040 from '../../wallet/lez040.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const CLI = join(HERE, '..', 'ferrata-msg.mjs');
const SCRATCH = join(HERE, '..', '..', '..', '..', 'target', 'msg-cli-test');
const PROGRAM = M.idToString(new Uint8Array(32).fill(7));
const B = M.idToString(new Uint8Array(32).fill(0x42));
const SK = '11'.repeat(32);

function scratch() {
  mkdirSync(dirname(SCRATCH), { recursive: true });
  const d = mkdtempSync(join(SCRATCH + '-'));
  writeFileSync(join(d, 'k.key'), SK + '\n', { mode: 0o600 });
  writeFileSync(join(d, 'bad.key'), 'not a key\n', { mode: 0o600 });
  return d;
}

function cli(args) {
  return new Promise((resolve) => {
    const env = { ...process.env, FERRATA_RPC: '', FERRATA_MSG_PROGRAM: '', FERRATA_MSG_LEGACY_PROGRAM: '', FERRATA_KEY_FILE: '' };
    const p = spawn(process.execPath, [CLI, ...args], { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '', err = '';
    p.stdout.on('data', (d) => { out += d; });
    p.stderr.on('data', (d) => { err += d; });
    p.on('close', (code) => resolve({ code, out, err }));
  });
}

/** A loopback JSON-RPC server. `answer(method, params)` supplies each recorded result. */
function stubRpc(answer) {
  const calls = [];
  const server = createServer((req, res) => {
    assert.equal(req.socket.remoteAddress, '127.0.0.1');
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      const { id, method, params } = JSON.parse(body);
      calls.push({ method, params });
      const result = answer(method, params);
      res.writeHead(200, { 'Content-Type': 'application/json', Connection: 'close' });
      res.end(JSON.stringify(result === undefined
        ? { jsonrpc: '2.0', id, error: { code: -32601, message: 'Method not found' } }
        : { jsonrpc: '2.0', id, result }));
    });
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({
    url: `http://127.0.0.1:${server.address().port}`, calls,
    close: () => new Promise((ok) => server.close(ok)),
  })));
}

const le = (n, bytes) => {
  const out = new Uint8Array(bytes);
  for (let i = 0; i < bytes; i++) { out[i] = Number(BigInt(n) & 255n); n = BigInt(n) >> 8n; }
  return out;
};
const bytes = (a) => Uint8Array.from(a);
const account = (hex) => bytes(Buffer.from(hex, 'hex'));
const actor = (hex) => ({ data: { actor_states: { [PROGRAM]: Array.from(bytes(Buffer.from(hex, 'hex'))) } } });

// One nonempty inbox, whose envelope has been deleted. Fields follow the IDL,
// written here independently: owner, nextN, and a vector of inbox entries.
function inboxWithMissingEnvelope(owner) {
  return Array.from(M.concat(
    owner, le(2, 8), le(1, 4), // owner, nextN, entries.len
    le(1, 8), new Uint8Array(32).fill(0x41), le(0, 8), // n, from, seq
    le(1_790_000_000_000n, 8), le(512, 4), Uint8Array.of(1), // atMs, len, contact
  ));
}

test('keygen reads the local key, derives the public record, and sends nothing', async () => {
  const rpc = await stubRpc(() => { throw new Error('keygen reached the RPC'); });
  const d = scratch();
  try {
    const r = await cli(['keygen', '--key', join(d, 'k.key'), '--program', PROGRAM, '--epoch', '0']);
    assert.equal(r.code, 0, r.err);
    assert.match(r.out, /^account\s+\S+/m);
    assert.match(r.out, /^epoch\s+0$/m);
    assert.match(r.out, /^x25519\s+[0-9a-f]{64}$/m);
    assert.match(r.out, /^ml-kem-768\s+1184 bytes, sha256 [0-9a-f]{64}$/m);
    assert.match(r.out, /^fingerprint\s+\S+/m);
    assert.deepEqual(rpc.calls, [], 'keygen makes no JSON-RPC call');
  } finally { await rpc.close(); rmSync(d, { recursive: true, force: true }); }
});

test('--lez-version reports the selected codec; v0.4.0 keygen and the unknown-version refusal follow it', async () => {
  const d = scratch();
  const phrase = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
  writeFileSync(join(d, 'phrase.key'), phrase + '\n', { mode: 0o600 });
  const answers = [
    { answer: () => undefined, version: 'v0.4.0-rc3' },
    { answer: () => ({ state: 'online', height: 8 }), version: 'v0.4.0' },
    { answer: () => null, version: 'unknown' },
  ];
  try {
    for (const a of answers) {
      const rpc = await stubRpc((method) => {
        assert.equal(method, 'getSequencerStatus');
        return a.answer();
      });
      try {
        const r = await cli(['--lez-version', '--rpc', rpc.url]);
        assert.deepEqual({ code: r.code, out: r.out, err: r.err }, { code: 0, out: `${a.version}\n`, err: '' });
        assert.equal(rpc.calls.length, 1);
        if (a.version === 'v0.4.0') {
          const key = await cli(['keygen', '--key', join(d, 'k.key'), '--rpc', rpc.url]);
          assert.equal(key.code, 0, key.err);
          assert.match(key.out, new RegExp(`^account\\s+${v040.idToString(v040.publicAccountId(v040.unhex(SK)))}$`, 'm'));
          const fromPhrase = await cli(['keygen', '--key', join(d, 'phrase.key'), '--path', '/2', '--rpc', rpc.url]);
          assert.equal(fromPhrase.code, 0, fromPhrase.err);
          const expected = v040.deriveNode(rc3.seedOf(phrase), [2]);
          assert.match(fromPhrase.out, new RegExp(`^account\\s+${v040.idToString(expected.accountId)}$`, 'm'));
        }
        if (a.version === 'unknown') {
          const refused = await cli(['publish', '--key', join(d, 'k.key'), '--rpc', rpc.url, '--program', PROGRAM]);
          assert.equal(refused.code, 3);
          assert.match(refused.err, /the network's LEZ version could not be read, so nothing is signed/);
          assert.ok(!rpc.calls.some((x) => x.method === 'sendTransaction'));
        }
      } finally { await rpc.close(); }
    }
  } finally { rmSync(d, { recursive: true, force: true }); }
});

test('argument and local-input refusals exit 2 without contacting an RPC', async () => {
  const d = scratch();
  const key = join(d, 'k.key');
  const auth = ['--rpc', 'http://127.0.0.1:1', '--program', PROGRAM, '--key', key];
  const cases = [
    { args: ['--rpc'], error: /--rpc needs a value/ },
    { args: ['unknown', ...auth], error: /unknown command unknown/ },
    { args: ['keygen'], error: /no key/ },
    { args: ['keygen', '--key', join(d, 'bad.key')], error: /neither 64 hex digits nor a BIP-39 mnemonic/ },
    { args: ['status', B], error: /no RPC/ },
    { args: ['status', B, '--rpc', 'http://127.0.0.1:1'], error: /no program id/ },
    { args: ['send', B, ...auth], error: /usage: ferrata-msg send/ },
    { args: ['delete', ...auth], error: /usage: ferrata-msg delete/ },
    { args: ['clear', ...auth], error: /usage: ferrata-msg clear/ },
    ...['contact', 'uncontact', 'block', 'unblock'].flatMap((cmd) => [
      { args: [cmd, ...auth], error: new RegExp(`usage: ferrata-msg ${cmd} ACCOUNT`) },
    ]),
    { args: ['policy', ...auth], error: /usage: ferrata-msg policy/ },
    { args: ['policy', 'nobody', ...auth], error: /usage: ferrata-msg policy/ },
  ];
  try {
    for (const c of cases) {
      const r = await cli(c.args);
      assert.equal(r.code, 2, `${c.args.join(' ')}: ${r.err}`);
      assert.match(r.err, c.error, c.args.join(' '));
    }
  } finally { rmSync(d, { recursive: true, force: true }); }
});

test('status prints the recorded profile and asks only the two account reads', async () => {
  const vectors = JSON.parse(readFileSync(join(HERE, 'vectors-v2.json'), 'utf8'));
  const profile = actor(vectors.states.profile);
  const rpc = await stubRpc((method, params) => {
    if (method === 'getSequencerStatus') return undefined; // rc3's method-not-found reply
    assert.equal(method, 'getAccount');
    // The profile belongs to B in the committed vector. B has no inbox here.
    return params[0] === B ? profile : { data: { actor_states: {} } };
  });
  try {
    const r = await cli(['status', B, '--rpc', rpc.url, '--program', PROGRAM]);
    assert.equal(r.code, 0, r.err);
    assert.match(r.out, new RegExp(`^account\\s+${B}$`, 'm'));
    assert.match(r.out, /^key\s+epoch 0, hybrid X25519 \+ ML-KEM-768, fingerprint /m);
    assert.match(r.out, /^policy\s+contactsOnly; 1 contacts, 0 blocked$/m);
    assert.match(r.out, /^inbox\s+none \(no key published\)$/m);
    assert.match(r.out, /^sent\s+3 in all, 1 declared within the last hour/m);
    assert.deepEqual(rpc.calls.map((x) => x.method), ['getSequencerStatus', 'getAccount', 'getAccount']);
    assert.equal(rpc.calls[1].params[0], B, 'the profile read names the requested account');
    assert.ok(!rpc.calls.some((x) => x.method === 'sendTransaction'));
  } finally { await rpc.close(); }
});

test('inbox decodes its recorded entry, reports a missing envelope, and sends nothing', async () => {
  const lez = await import('../../wallet/lez.js');
  const node = lez.rawKeyNode(SK);
  const inboxId = M.inboxAccount(M.idFromString(PROGRAM), node.accountId);
  const inbox = { data: { actor_states: { [PROGRAM]: inboxWithMissingEnvelope(node.accountId) } } };
  const rpc = await stubRpc((method, params) => {
    if (method === 'getSequencerStatus') return undefined; // rc3's method-not-found reply
    assert.equal(method, 'getAccount');
    return params[0] === M.idToString(inboxId) ? inbox : { data: { actor_states: {} } };
  });
  const d = scratch();
  try {
    const r = await cli(['inbox', '--rpc', rpc.url, '--program', PROGRAM, '--key', join(d, 'k.key')]);
    assert.equal(r.code, 0, r.err);
    assert.match(r.out, /^#1 2026-09-21T14:13:20Z from \S+ \(contact\) \(512 B, X25519\): cannot open: envelope not found$/m);
    assert.match(r.out, /^1 of 32 messages, 512 of 65536 bytes; free them with `ferrata-msg clear 1`$/m);
    assert.deepEqual(rpc.calls.map((x) => x.method), ['getSequencerStatus', 'getAccount', 'getAccount'], 'version probe, inbox then its envelope');
    assert.equal(rpc.calls[1].params[0], M.idToString(inboxId));
    assert.equal(rpc.calls[2].params[0], M.idToString(M.envelopeAccount(M.idFromString(PROGRAM), node.accountId, new Uint8Array(32).fill(0x41), 0n)));
    assert.ok(!rpc.calls.some((x) => x.method === 'sendTransaction'));
  } finally { await rpc.close(); rmSync(d, { recursive: true, force: true }); }
});
