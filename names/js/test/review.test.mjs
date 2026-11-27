// The review's findings for the names client (23 November 2026) as tests of the behaviour the
// client must have; written first against the reviewed client, where they fail. `node --test names/js/test/review.test.mjs`
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as N from '../names.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const CLI = join(HERE, '..', 'cli.mjs');
const SCRATCH = join(HERE, '..', '..', '..', 'target', 'names-js-test');
const PROGRAM = N.idString(new Uint8Array(32).fill(7));
const SK = '11'.repeat(32);

/** A JSON-RPC stand-in for the sequencer: `states[accountB58]` is that account's actor state at PROGRAM. */
function stubRpc(states = {}) {
  const calls = [];
  const server = createServer((req, res) => {
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      const { id, method, params } = JSON.parse(body);
      calls.push(method);
      let result = null;
      if (method === 'getAccountView') {
        const s = states[params[0].account_id];
        result = s ? { nonce: 0, data: { actor_states: { [PROGRAM]: Array.from(s) } } } : null;
      } else if (method === 'getAccount') result = null;
      else if (method === 'getAccountsNonces') result = params[0].map(() => 0);
      else if (method === 'getFeeState') result = { base_fee_exec: 8, base_fee_stor: 8 };
      else if (method === 'getAccountBalance') result = '100000000000';
      else if (method === 'sendTransaction') result = 'sent';
      else if (method === 'getLastBlockId') result = 1;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ jsonrpc: '2.0', id, result }));
    });
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ url: `http://127.0.0.1:${server.address().port}`, calls, close: () => server.close() })));
}

function cli(args, env = {}) {
  return new Promise((resolve) => {
    const p = spawn(process.execPath, [CLI, ...args], { env: { ...process.env, FERRATA_LEZ_JS: '', ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '', err = '';
    p.stdout.on('data', (d) => { out += d; });
    p.stderr.on('data', (d) => { err += d; });
    p.on('close', (code) => resolve({ code, out, err }));
  });
}

function scratch() {
  mkdirSync(dirname(SCRATCH), { recursive: true });
  const d = mkdtempSync(join(SCRATCH + '-'));
  writeFileSync(join(d, 'k.key'), SK + '\n', { mode: 0o600 });
  return d;
}

// L16: the CLI signs with the wallet's lez.js on this branch alone (its default path, no FERRATA_LEZ_JS).
test('L16: the CLI signs a commit on this branch alone', async () => {
  const rpc = await stubRpc();
  const d = scratch();
  try {
    const r = await cli(['--rpc', rpc.url, '--program', PROGRAM, 'commit', 'alice', '--key', join(d, 'k.key'), '--state', join(d, 'alice.commit.json'), '--dry-run']);
    assert.equal(r.code, 0, r.err);
    const o = JSON.parse(r.out);
    assert.equal(o.dryRun, true);
    assert.ok(!rpc.calls.includes('sendTransaction'));
    // The transaction as LEZ reads it: signed by the key's account, the root at user(signer), the counter declared with
    // its PDA evidence (it holds no state yet), and a valid BIP-340 signature over the message hash.
    const lez = await import(join(HERE, '..', '..', '..', 'deploy', 'testnet', 'web', 'wallet', 'lez.js'));
    const w = lez.decodeWire(o.wire);
    assert.equal(w.kind, 'public');
    const me = lez.rawKeyNode(SK);
    assert.equal(N.hex(w.tx.to.account), N.hex(me.accountId));
    assert.equal(N.hex(w.tx.to.program), N.hex(N.accountId(PROGRAM)));
    const seqAccount = await N.sequenceAccountId(PROGRAM);
    assert.ok(w.tx.actors.some((x) => N.hex(x.account) === N.hex(seqAccount)), 'the sequence actor is declared');
    assert.deepEqual(w.tx.evidence.map((e) => N.hex(e.pda.seed)), [N.hex(await N.sequenceSeed())]);
    assert.equal(w.tx.data[0], 7, 'Commit is variant 7');
    assert.equal(w.tx.witnesses.length, 1);
    assert.ok(lez.verify(w.tx.witnesses[0].sig, lez.messageHash(w.tx.messageBytes), me.pk), 'the signature verifies');
  } finally { rpc.close(); rmSync(d, { recursive: true, force: true }); }
});

// M1: no reveal is sent before its window (no --force), so none is refused on chain for being early.
test('M1: an early reveal is refused locally, even with --force', async () => {
  const rpc = await stubRpc();
  const d = scratch();
  try {
    const now = Date.now();
    writeFileSync(join(d, 'alice.commit.json'), JSON.stringify({ name: 'alice', signer: 'x', salt: '5a'.repeat(32), nowMs: now,
      revealFrom: now + 240_000, revealUntil: now + 3_600_000, target: null, receiverKeys: null }));
    const r = await cli(['--rpc', rpc.url, '--program', PROGRAM, 'reveal', 'alice', '--key', join(d, 'k.key'), '--state', join(d, 'alice.commit.json'), '--force', '--no-wait']);
    assert.notEqual(r.code, 0);
    assert.match(r.err, /too early/);
    assert.ok(!rpc.calls.includes('sendTransaction'), 'nothing sent');
  } finally { rpc.close(); rmSync(d, { recursive: true, force: true }); }
});

// M1: before a reveal is sent, everything that would refuse it on chain is checked.
test('M1: preflightReveal finds what would refuse the reveal', async () => {
  const program = new Uint8Array(32).fill(7);
  const me = N.idString(new Uint8Array(32).fill(1));
  const salt = new Uint8Array(32).fill(0x5a);
  const names = new N.Names({ rpc: { call: async () => null }, program });
  const p = await names.preflightReveal(me, { name: 'alice', salt, nowMs: Date.now() });
  assert.equal(p.ok, false);
  assert.ok(p.problems.some((x) => x.code === 230), JSON.stringify(p.problems));
});

// L5: two RPCs that disagree about a name are not trusted.
test('L5: resolve refuses when the RPCs disagree', async () => {
  const program = N.idString(new Uint8Array(32).fill(7));
  const stub = (bytes) => ({ call: async (m) => (m === 'getAccountView' ? { data: { actor_states: { [program]: bytes } } } : null) });
  const names = new N.Names({ rpc: [stub([1, 2, 3]), stub([1, 2, 4])], program });
  await assert.rejects(names.resolve('alice'), /disagree/);
});

// L12: look-alikes share one address; variants of reserved names are reserved.
test('L12: look-alikes share a skeleton and variants of reserved names are refused', async () => {
  assert.equal(N.skeleton('a1ice'), 'alice');
  assert.equal(N.skeleton('rnallory'), 'mallory');
  assert.equal(N.skeleton('vvallet'), 'wallet');
  const p = N.idString(new Uint8Array(32).fill(7));
  assert.equal(N.hex(await N.nameAccountId(p, 'a1ice')), N.hex(await N.nameAccountId(p, 'alice')));
  for (const v of ['faucet1', 'wa11et', 'ferrata-team', 'team-ferrata', '2faucet', '0fficial', 'support-desk']) {
    assert.equal(N.checkName(v)?.code, 246, v);
  }
  for (const okName of ['alice', 'bridget', 'helpful', 'rooted', 'walleting']) assert.equal(N.checkName(okName), null, okName);
});

// M4/L15: every code the program has is described.
test('L15: every exit code is described', () => {
  for (const c of [230, 231, 232, 233, 234, 235, 236, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254]) {
    assert.ok(N.CODES[c], `code ${c}`);
  }
});

// Round 2 of the review (REPORT-round2.md).

// N1: every action is sent with a gas limit that covers its worst case measured in the harness
// (lez/results/names-worst.md: 32-byte names of six shapes, the largest records, full lists, Forget of 1 to 5 names).
test('N1: each action\'s gas limit covers its measured worst case, and the CLI sends with it', async () => {
  const table = readFileSync(join(HERE, '..', '..', '..', 'lez', 'results', 'names-worst.md'), 'utf8');
  const rows = table.split('\n').filter((l) => /^\| [A-Z]/.test(l)).map((l) => l.split('|').map((c) => c.trim()));
  assert.equal(rows.length, 13, 'eight actions and Forget of 1 to 5 names');
  for (const c of rows) {
    const [action, n, cycles] = [c[1], Number(c[2]), Number(c[5].replace(/,/g, ''))];
    const msg = action === 'Forget' ? { kind: 'Forget', names: Array(n).fill('x') } : { kind: action };
    assert.ok(cycles <= N.gasFor(msg), `${action} (${n}): ${cycles} > ${N.gasFor(msg)}`);
    assert.equal(Number(c[6].replace(/,/g, '')), N.gasFor(msg), `${action} (${n}): the table's limit is gasFor's`);
  }
  // the CLI's own Forget before a reveal and `forget` send one Forget of up to five names: 267,216 cycles in the review
  assert.ok(N.gasFor({ kind: 'Forget', names: Array(5).fill('x') }) > 267_216);
  const rpc = await stubRpc();
  const d = scratch();
  try {
    const r = await cli(['--rpc', rpc.url, '--program', PROGRAM, 'commit', 'alice', '--key', join(d, 'k.key'), '--state', join(d, 's.json'), '--dry-run']);
    assert.equal(r.code, 0, r.err);
    assert.equal(JSON.parse(r.out).gasLimit, N.gasFor({ kind: 'Commit' }));
  } finally { rpc.close(); rmSync(d, { recursive: true, force: true }); }
});

// The names analogue of N3: one of the program's own accounts (a name's record, the counter) read as an account.
test('a program account has no name, and is not an account\'s state', async () => {
  const program = N.idString(new Uint8Array(32).fill(7));
  const V = JSON.parse(readFileSync(join(HERE, '..', '..', '..', 'lez', 'results', 'names-vectors.json'), 'utf8'));
  const states = { record: Array.from(N.unhex(V.record)), counter: [7, 0, 0, 0, 0, 0, 0, 0] };
  for (const [what, bytes] of Object.entries(states)) {
    const rpc = { call: async (m) => (m === 'getAccountView' ? { data: { actor_states: { [program]: bytes } } } : null) };
    const names = new N.Names({ rpc, program });
    const pda = N.idString(new Uint8Array(32).fill(3));
    assert.equal(await names.reverse(pda), null, what);
    await assert.rejects(names.user(pda), N.NotAnAccount, what);
  }
});

// G15: a complete register send path, offline. The little reader is independent
// of names.js's encoder: the signed bytes captured by the RPC stand-in must
// decode to the intended Commit and Register instructions. No production
// encoder is used to construct the expected instruction bytes.
const u64le = (n) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(n), true); return b; };
const u32leTest = (n) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; };
class InstructionReader {
  constructor(b) { this.b = b; this.o = 0; this.v = new DataView(b.buffer, b.byteOffset, b.byteLength); }
  take(n) { assert.ok(this.o + n <= this.b.length, 'truncated instruction'); const s = this.b.slice(this.o, this.o + n); this.o += n; return s; }
  u8() { return this.take(1)[0]; }
  u32() { const n = this.v.getUint32(this.o, true); this.take(4); return n; }
  u64() { const n = this.v.getBigUint64(this.o, true); this.take(8); return n; }
  str() { return new TextDecoder().decode(this.take(this.u32())); }
  end() { assert.equal(this.o, this.b.length, 'trailing instruction bytes'); }
}

test('G15: register sends a signed Commit then the intended signed Register', async () => {
  const lez = await import(join(HERE, '..', '..', '..', 'deploy', 'testnet', 'web', 'wallet', 'lez.js'));
  const me = lez.rawKeyNode(SK);
  const myId = lez.idToString(me.accountId);
  const sent = [];
  let userState = null;
  const calls = [];
  const server = createServer((req, res) => {
    assert.equal(req.socket.remoteAddress, '127.0.0.1');
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      const { id, method, params } = JSON.parse(body);
      calls.push(method);
      let result = null;
      if (method === 'getAccountView') {
        result = params[0].account_id === myId && userState
          ? { nonce: 0, data: { actor_states: { [PROGRAM]: Array.from(userState) } } }
          : null;
      } else if (method === 'getAccountsNonces') result = params[0].map(() => 0);
      else if (method === 'getFeeState') result = { base_fee_exec: 8, base_fee_stor: 8 };
      else if (method === 'getAccountBalance') result = '100000000000';
      else if (method === 'sendTransaction') {
        const w = lez.decodeWire(params[0]);
        sent.push(w);
        if (sent.length === 1) {
          // UserState { names: [], primary: None, commit: Some(…) } for the
          // reveal's preflight, constructed here independently of names.js.
          const r = new InstructionReader(w.tx.data);
          assert.equal(r.u8(), 7);
          const commitment = r.take(32), nowMs = r.u64();
          r.end();
          userState = concatTest(u32leTest(0), Uint8Array.of(0, 1), commitment, u64le(1),
            u64le(nowMs + 240_000n), u64le(nowMs + 3_600_000n));
        }
        result = 'sent';
      }
      res.writeHead(200, { 'Content-Type': 'application/json', Connection: 'close' });
      res.end(JSON.stringify({ jsonrpc: '2.0', id, result }));
    });
  });
  const url = await new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(`http://127.0.0.1:${server.address().port}`)));
  const d = scratch();
  try {
    const state = join(d, 'alice.commit.json');
    const r = await cli(['--rpc', url, '--program', PROGRAM, 'register', 'alice', '--key', join(d, 'k.key'), '--state', state, '--no-wait'],
      { FERRATA_NAMES_TEST_COMMIT_OFFSET_MS: '-300000' });
    assert.equal(r.code, 0, r.err);
    assert.equal(sent.length, 2, 'both the commit and the reveal reached the stand-in');
    assert.deepEqual(calls.filter((m) => m === 'sendTransaction'), ['sendTransaction', 'sendTransaction']);
    assert.match(r.out, /"action": "Commit"/);
    assert.match(r.out, /"action": "Register"/);

    const st = JSON.parse(readFileSync(state, 'utf8'));
    const salt = N.unhex(st.salt);
    const commit = new InstructionReader(sent[0].tx.data);
    assert.equal(commit.u8(), 7, 'Commit is variant 7');
    const { createHash } = await import('node:crypto');
    const domain = new TextEncoder().encode('ferrata-names/v2/commit/');
    const expectedCommitment = createHash('sha256').update(concatTest(domain, me.accountId, salt, new TextEncoder().encode('alice'))).digest();
    assert.deepEqual(commit.take(32), Uint8Array.from(expectedCommitment), 'independent SHA-256 commitment over owner, salt and name');
    const committedAt = commit.u64();
    commit.end();

    const reveal = new InstructionReader(sent[1].tx.data);
    assert.equal(reveal.u8(), 0, 'Register is variant 0');
    assert.equal(reveal.str(), 'alice');
    assert.ok(reveal.u64() >= committedAt, 'the reveal carries its own execution time');
    assert.equal(reveal.u8(), 1, 'the default target is present');
    assert.equal(N.hex(reveal.take(32)), N.hex(me.accountId), 'the target is the signer');
    assert.equal(reveal.u8(), 0, 'no private receiver was requested');
    assert.deepEqual(reveal.take(32), salt, 'the reveal opens exactly the saved salt');
    reveal.end();
    for (const [i, w] of sent.entries()) {
      assert.equal(w.kind, 'public');
      assert.equal(N.hex(w.tx.to.account), N.hex(me.accountId));
      assert.equal(N.hex(w.tx.to.program), N.hex(N.accountId(PROGRAM)));
      assert.equal(w.tx.witnesses.length, 1);
      assert.ok(lez.verify(w.tx.witnesses[0].sig, lez.messageHash(w.tx.messageBytes), me.pk), `signature ${i + 1} verifies`);
    }
  } finally { await new Promise((ok) => server.close(ok)); rmSync(d, { recursive: true, force: true }); }
});

test('G15: write-command and register refusals send nothing', async () => {
  const d = scratch();
  const auth = ['--rpc', 'http://127.0.0.1:1', '--program', PROGRAM];
  const key = ['--key', join(d, 'k.key')];
  const cases = [
    { args: [...auth, 'register', ...key], code: 2, error: /register NAME/ },
    { args: [...auth, 'renew', ...key], code: 2, error: /renew NAME/ },
    { args: [...auth, 'transfer', 'alice', ...key], code: 2, error: /transfer NAME TO/ },
    { args: [...auth, 'renew', 'alice'], code: 2, error: /signed command needs --key/ },
    { args: [...auth, 'register', 'wallet', ...key, '--state', join(d, 'reserved.json'), '--no-wait'], code: 1, error: /reserved/i },
  ];
  try {
    for (const c of cases) {
      const r = await cli(c.args);
      assert.equal(r.code, c.code, `${c.args.join(' ')}: ${r.err}`);
      assert.match(r.err, c.error, c.args.join(' '));
    }
  } finally { rmSync(d, { recursive: true, force: true }); }
});

function concatTest(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
