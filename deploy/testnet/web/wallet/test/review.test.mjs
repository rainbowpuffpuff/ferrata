// The independent review's findings on the wallet that unit tests can hold: amounts with a comma (L-W3), a
// transaction missing from its block (L-W2), EIP-55 (M-W3), the record past 200 KB, its iteration count and a second
// wallet over the first (L-W1, L-W6, L-W15), embedded trie nodes (L-W7), the Activity cap (L-W1).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as lez from '../lez.js';
import * as eth from '../eth.js';
import * as ks from '../keystore.js';
import * as history from '../history.js';
import { checkConfig, lezVersionText } from '../settings.js';
import { burnRecipient } from '../bridge.js';

const shim = () => { const m = new Map(); return { getItem: (k) => (m.has(k) ? m.get(k) : null), setItem: (k, v) => m.set(k, String(v)), removeItem: (k) => m.delete(k) }; };

test('settings: rc3 keeps its evidence-based LEZ version label; v0.4.0 and unreadable states are unchanged', () => {
  const rc3 = { version: 'v0.4.0-rc3', readOnly: false };
  assert.equal(lezVersionText(rc3, '631f0c96abcdef'), 'v0.4.0-rc2/rc3 (privacy circuit 631f0c96…)');
  assert.equal(lezVersionText(rc3, '01234567abcdef'), 'see the privacy circuit id');
  assert.equal(lezVersionText(rc3, null), 'see the privacy circuit id');
  assert.equal(lezVersionText({ version: 'v0.4.0', readOnly: false }, '631f0c96abcdef'), 'v0.4.0');
  const unreadable = { version: 'unknown', readOnly: true, accountsAs: 'v0.4.0', error: 'offline' };
  assert.equal(lezVersionText(unreadable, '631f0c96abcdef'), 'unreadable (accounts shown as on v0.4.0): offline');
  const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
  assert.match(app, /add\('LEZ version', lezVersionText\(net, ppc && lez\.programIdHex\(ppc\)\)\);/);
});

test('amounts: a point for decimals; a comma is refused, never guessed (L-W3)', () => {
  assert.equal(lez.parseUnits('1.5', 9), 1_500_000_000n);
  assert.equal(lez.parseUnits(' 1000 ', 9), 1_000_000_000_000n); // blanks around the amount are trimmed
  assert.equal(lez.parseUnits('0.000000001', 9), 1n);
  assert.throws(() => lez.parseUnits('1,5', 9), /no commas/);
  assert.throws(() => lez.parseUnits('1,000', 9), /no commas/);
  assert.throws(() => lez.parseUnits('0.0000000001', 9), /at most 9 decimals/);
  assert.throws(() => lez.parseUnits('0', 9), /more than zero/);
  assert.throws(() => lez.parseUnits('1e3', 9), /such as 1.5/);
});

test('outcome: applied, reverted, or missing from the block (never "applied" by default) (L-W2)', () => {
  const blk = { transactions: [{ hash: 'aa' }, { hash: 'bb' }], reverted: [1] };
  assert.equal(lez.outcomeIn(blk, 'aa'), 'applied');
  assert.equal(lez.outcomeIn(blk, 'bb'), 'reverted');
  assert.equal(lez.outcomeIn(blk, 'cc'), 'missing');
});

test('EIP-55: the EIP\'s own examples; a flipped letter is refused; no-checksum forms pass; the configured addresses pass (M-W3)', () => {
  const ok = ['0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed', '0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359',
    '0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB', '0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb',
    '0x52908400098527886E0F7030069857D2E4169EE7', '0x8617E340B3D01FA5F11F306F4090FD50E238070D',
    '0xde709f2102306220921060314715629080e2fb77', '0x27b1fdb04752bbc536007a920d24acb045561c26'];
  for (const a of ok) {
    assert.equal(eth.toChecksumAddress(a.toLowerCase()), a);
    assert.equal(lez.hex(eth.address(a)), a.slice(2).toLowerCase());
  }
  const flipped = '0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAeD';
  assert.throws(() => eth.address(flipped), /EIP-55 checksum/);
  assert.equal(eth.hasChecksum(flipped), true);
  assert.equal(eth.hasChecksum('0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed'), false);
  eth.address('0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed');
  eth.address('0x5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED');
  assert.throws(() => eth.address('0x5aaeb6053f3e94c9b9a09f33669435e7ef1bea'), /40 hex digits/);
  const cfg = JSON.parse(readFileSync(new URL('../config.json', import.meta.url)));
  for (const a of [cfg.bridge?.vault, cfg.bridge?.token?.address].filter(Boolean)) eth.address(a);
});

test('the record seals and opens past 200 KB (chunked base64) (L-W1)', async () => {
  const st = shim();
  const secret = { v: 1, mnemonic: null, accounts: [], history: { x: { items: Array.from({ length: 4000 }, (_, i) => ({ hash: 'ab'.repeat(32), block: i, note: 'n'.repeat(60) })) } } };
  assert.ok(JSON.stringify(secret).length > 400000);
  await ks.create('correct horse 1', secret, st);
  const o = await ks.open('correct horse 1', st);
  assert.equal(o.secret.history.x.items.length, 4000);
});

test('a save keeps the record\'s own iteration count; a second wallet never overwrites the first (L-W6, L-W15)', async () => {
  const st = shim();
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const base = await crypto.subtle.importKey('raw', new TextEncoder().encode('older wallet 1'), 'PBKDF2', false, ['deriveKey']);
  const key = await crypto.subtle.deriveKey({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations: 1000 }, base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
  await ks.seal(key, salt, { v: 1, n: 1 }, st, 1000); // a record made when the count was 1000
  const o = await ks.open('older wallet 1', st);
  await ks.seal(o.key, o.salt, { v: 1, n: 2 }, st); // an ordinary save (persist)
  assert.equal(JSON.parse(st.getItem('ferrata-wallet/v1')).kdf.iterations, 1000);
  assert.deepEqual((await ks.open('older wallet 1', st)).secret, { v: 1, n: 2 });
  await assert.rejects(ks.create('another one 2', { v: 1 }, st), /already holds a wallet/);
  assert.deepEqual((await ks.open('older wallet 1', st)).secret, { v: 1, n: 2 });
});

// A trie whose branch and leaves are embedded (RLP under 32 bytes): extension(63 zero nibbles) → branch → leaves.
function smallTrie() {
  const leaf = (v) => [Uint8Array.of(0x20), Uint8Array.of(v)]; // even, empty remaining path
  const branch = Array.from({ length: 17 }, () => new Uint8Array(0));
  branch[0] = leaf(0x01); branch[1] = leaf(0x02);
  const hpExt = new Uint8Array(32); hpExt[0] = 0x10; // odd extension, its 63 nibbles all zero
  const ext = eth.rlp([hpExt, branch]);
  assert.ok(eth.rlp(branch).length < 32 && ext.length >= 32);
  const key = (last) => { const k = new Uint8Array(32); k[31] = last; return k; };
  return { root: eth.keccak(ext), proof: ['0x' + lez.hex(ext)], key };
}

test('embedded trie nodes are read in place; an appended forged node is never used (L-W7)', () => {
  const t = smallTrie();
  assert.deepEqual([...eth.verifyProof(t.root, t.key(0x00), t.proof)], [0x01]);
  assert.deepEqual([...eth.verifyProof(t.root, t.key(0x01), t.proof)], [0x02]);
  assert.equal(eth.verifyProof(t.root, t.key(0x02), t.proof), null); // empty branch slot: absent
  const other = new Uint8Array(32); other[0] = 0x10;
  assert.equal(eth.verifyProof(t.root, other, t.proof), null); // diverges inside the extension: absent
  const forged = '0x' + lez.hex(eth.rlp([Uint8Array.of(0x20), Uint8Array.of(0x99)]));
  assert.deepEqual([...eth.verifyProof(t.root, t.key(0x00), [...t.proof, forged])], [0x01]);
  const bad = '0x' + lez.hex(eth.rlp([new Uint8Array(32).fill(0x10), Array.from({ length: 17 }, () => new Uint8Array(0))]));
  assert.throws(() => eth.verifyProof(t.root, t.key(0x00), [bad]), /does not match its parent/);
});

test('Activity keeps the newest MAX_ITEMS per account, without duplicates (L-W1)', () => {
  const c = { S: { secret: {} } };
  const items = (from, n) => Array.from({ length: n }, (_, i) => ({ account: 'A', hash: (from + i).toString(16).padStart(64, '0'), dir: 'in', block: from + i }));
  history.keepItems(c, items(0, 400));
  history.keepItems(c, items(300, 400)); // 100 already kept, 300 new
  const st = c.S.secret.history.A;
  assert.equal(st.items.length, history.MAX_ITEMS);
  assert.equal(Math.min(...st.items.map((x) => x.block)), 700 - history.MAX_ITEMS);
  assert.equal(new Set(st.items.map((x) => x.hash)).size, st.items.length);
});

// The burn's review (M-W3): planBurn plans from the program's own state vectors (the IDL's Token#1: denominations
// 1, 10, 100; Holder#1: 5 of that token) and signs nothing; sendBurn signs exactly the plan, and refuses when the
// account, the counter or the holding changed since the review.
import * as cl from '../claim.js';
import * as br from '../bridge.js';
import { forPublicPda } from '../attest.js';
function burnStage() {
  const iv = JSON.parse(readFileSync(new URL('../../../../../idl/deployed/ferrata_bridge_b1.vectors.json', import.meta.url))).vectors;
  const state = (n) => lez.unhex(iv.find((x) => x.name === n).hex);
  const programId = '36iqVMFpfgHrwoeuVvZhXX3caM1JPBJh56PBw5A4H1rk', program = lez.idFromString(programId);
  const cid = Uint8Array.from({ length: 32 }, (_, i) => 164 + i), token = Uint8Array.from({ length: 20 }, (_, i) => 184 + i);
  const node = lez.rawKeyNode('11'.repeat(32)), a = { node, id: lez.idToString(node.accountId), label: 'A' };
  const st = new Map([[lez.idToString(forPublicPda(program, cl.seeds.token(cid, token))), state('BridgeState::Token#1')],
    [lez.idToString(forPublicPda(program, cl.seeds.holder(cid, node.accountId))), state('BridgeState::Holder#1')]]);
  const sent = [];
  const seq = {
    account: async (id) => { const b = st.get(lez.idToString(id)); return b ? { data: { actor_states: { [programId]: Array.from(b) } } } : null; },
    nonces: async () => [3], feeState: async () => ({ base_fee_exec: 1, base_fee_stor: 1 }), balance: async () => 10n ** 12n,
    send: async (wire) => { sent.push(wire); throw new Error('captured, not sent'); },
  };
  const bs = new Uint8Array(32).fill(9);
  const c = { cfg: { bridge: { program: programId, configId: '0x' + lez.hex(cid), vault: '0x' + '1b'.repeat(20), evmChainId: 11155111 } },
    seq, acct: () => a, fmt: String };
  return { c, a, st, sent, cid, token, program, keys: { kAuth: br.kAuthOf(bs), kView: br.kViewOf(bs) } };
}

test('burn review: the plan binds account, denomination, recipient and counter; sending signs exactly it (M-W3)', async () => {
  const t = burnStage(), recipient = eth.address('0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed');
  await assert.rejects(cl.planBurn(t.c, { ...t.keys, token: t.token, amount: 7n, recipient }), /not one of the token's denominations/);
  await assert.rejects(cl.planBurn(t.c, { ...t.keys, token: t.token, amount: 10n, recipient }), /holds 5 base units/);
  const plan = await cl.planBurn(t.c, { ...t.keys, token: t.token, amount: 1n, recipient });
  assert.equal(t.sent.length, 0); // planning signs and sends nothing
  assert.equal(plan.i, 0);
  const note = cl.burnNote({ ...t.keys, configId: t.cid, i: 0, chainId: 11155111n, vault: lez.unhex('1b'.repeat(20)), recipient, token: t.token, amount32: cl.u256word(1n) });
  assert.equal(lez.hex(plan.data), lez.hex(cl.encodeBurn({ configId: t.cid, note, token: t.token, amount32: cl.u256word(1n) })));
  assert.deepEqual(plan.actors.map((x) => lez.idToString(x.account)), [t.a.id,
    ...[cl.seeds.holder(t.cid, t.a.node.accountId), cl.seeds.token(t.cid, t.token), cl.seeds.config(t.cid), cl.seeds.burn(t.cid, note.r)].map((s) => lez.idToString(forPublicPda(t.program, s)))]);
  await assert.rejects(cl.sendBurn(t.c, plan, () => {}), /captured, not sent/);
  const tx = lez.decodeWire(t.sent[0]).tx;
  assert.equal(lez.hex(tx.data), lez.hex(plan.data));
  assert.equal(lez.idToString(tx.to.account), t.a.id);
  assert.equal(tx.fee.maxFee, plan.maxFee); // the fee cap the review showed
  assert.equal(tx.actors.length, 5);
  // Changed since the review: the selected account, the counter (another tab burned with it), the holding.
  const other = { ...t.a }; t.c.acct = () => other;
  await assert.rejects(cl.sendBurn(t.c, plan, () => {}), /account changed since the review/);
  t.c.acct = () => t.a;
  t.st.set(lez.idToString(forPublicPda(t.program, cl.seeds.burn(t.cid, note.r))), Uint8Array.of(4));
  await assert.rejects(cl.sendBurn(t.c, plan, () => {}), /counter 0 was used since the review/);
  const next = await cl.planBurn(t.c, { ...t.keys, token: t.token, amount: 1n, recipient });
  assert.equal(next.i, 1); // the next free counter
  t.st.delete(lez.idToString(forPublicPda(t.program, cl.seeds.holder(t.cid, t.a.node.accountId))));
  await assert.rejects(cl.sendBurn(t.c, next, () => {}), /holding changed since the review/);
  assert.equal(t.sent.length, 1);
});

// Round 3 of the review (new Lows).
test('settings: a config.json that parses but lacks rpc, token.decimals or token.symbol is refused with a reason', () => {
  const ok = { rpc: '/rpc', token: { symbol: 'LGO', decimals: 9 } };
  assert.equal(checkConfig(ok), ok);
  assert.ok(checkConfig(JSON.parse(readFileSync(new URL('../config.json', import.meta.url), 'utf8'))));
  assert.throws(() => checkConfig(null), /not a settings object/);
  assert.throws(() => checkConfig([]), /not a settings object/);
  assert.throws(() => checkConfig('x'), /not a settings object/);
  assert.throws(() => checkConfig({}), /rpc is missing/);
  assert.throws(() => checkConfig({ rpc: ' ' }), /rpc is missing/);
  assert.throws(() => checkConfig({ rpc: '/rpc' }), /token is missing/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: null }), /token is missing/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: { symbol: 'LGO' } }), /token.decimals/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: { symbol: 'LGO', decimals: '9' } }), /token.decimals/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: { symbol: 'LGO', decimals: 9.5 } }), /token.decimals/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: { symbol: 'LGO', decimals: -1 } }), /token.decimals/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: { decimals: 9 } }), /token.symbol is missing/);
  assert.throws(() => checkConfig({ rpc: '/rpc', token: { decimals: 9, symbol: '' } }), /token.symbol is missing/);
});

test('amounts: a space between digits is refused, as a comma is ("1 5" is not 15)', () => {
  for (const t of ['1 5', '1 000', '1\t5', '1\u00a0000', '1\u202f000', '0. 5']) assert.throws(() => lez.parseUnits(t, 9), /without spaces/, JSON.stringify(t));
  assert.equal(lez.parseUnits(' 15 ', 9), 15_000_000_000n);
  assert.equal(lez.parseUnits('1_000', 9), 1_000_000_000_000n); // an underscore stays a digit separator
});

test('burn recipient: says where it came from; EIP-55 only when it was checked; the zero address refused', () => {
  const mixed = '0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed', lower = mixed.toLowerCase();
  assert.equal(burnRecipient(mixed, lower).how, 'typed; EIP-55 checksum checked');
  assert.equal(burnRecipient(` ${lower} `, null).how, 'typed without a checksum: compare every character');
  const fromWallet = burnRecipient('', lower);
  assert.match(fromWallet.how, /^from your browser wallet/);
  assert.doesNotMatch(fromWallet.how, /EIP-55|checksum/);
  assert.equal(fromWallet.to, lower);
  assert.equal(lez.hex(fromWallet.bytes), lower.slice(2));
  const zero = '0x' + '0'.repeat(40);
  assert.throws(() => burnRecipient(zero, lower), /cannot be the recipient/);
  assert.throws(() => burnRecipient('', zero), /cannot be the recipient/);
  assert.throws(() => burnRecipient('0x' + '0'.repeat(39), lower), /0x followed by 40 hex digits/);
  assert.throws(() => burnRecipient(mixed.replace('a', 'A'), lower), /EIP-55 checksum/);
  assert.throws(() => burnRecipient('', ''), /connect your browser wallet/);
});
