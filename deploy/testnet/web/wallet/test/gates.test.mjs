// The network's LEZ version is read again right before every signature (net.js `watched`, app.js `versionConfirmed`):
// nothing may be awaited between that read and the signature, or the network could move to another LEZ in between.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as C from '../msg/client.js';
import * as MC from '../../msg/client.js';

const src = (f) => readFileSync(new URL(`../${f}`, import.meta.url), 'utf8');

/** In `body`, the gate's last occurrence before the first `build` comes after every `await` but its own. */
function gatedRightBefore(body, gate, build, what) {
  const b = body.indexOf(build);
  assert.ok(b > 0, `${what}: ${build} found`);
  const g = body.lastIndexOf(gate, b);
  assert.ok(g >= 0, `${what}: ${gate} before ${build}`);
  const between = body.slice(g + gate.length, b);
  assert.ok(!/\bawait\b/.test(between), `${what}: nothing awaited between the version read and the signature:\n${between}`);
  const before = body.slice(0, g);
  return before;
}
const fn = (text, start, end) => { const i = text.indexOf(start); assert.ok(i >= 0, start); const j = end ? text.indexOf(end, i + start.length) : text.length; return text.slice(i, j < 0 ? text.length : j); };

test('the messages client reads the version after its own reads and refuses before building', async () => {
  const order = [];
  const seq = {
    feeState: async () => { order.push('fee'); return { base_fee_exec: 1, base_fee_stor: 1 }; },
    nonces: async () => { order.push('nonces'); return [5]; },
    send: async () => { order.push('send'); return 'h'; },
  };
  const net = { error: 'refused: another LEZ', buildCall: () => { order.push('build'); return { wire: '00', size: 100, hash: 'h' }; } };
  const args = { node: { accountId: new Uint8Array(32) }, program: new Uint8Array(32).fill(7), data: Uint8Array.of(1), gasLimit: 1000n, net };
  await C.call(seq, { ...args, gate: async () => { order.push('gate'); return true; } });
  assert.deepEqual(order, ['fee', 'nonces', 'gate', 'build', 'build', 'send']);
  order.length = 0;
  await assert.rejects(C.call(seq, { ...args, gate: async () => { order.push('gate'); return false; } }), /refused: another LEZ/);
  assert.deepEqual(order, ['fee', 'nonces', 'gate'], 'nothing built or sent after a refused read');
});

test('the messages library\'s signing helpers pass the gate on (the CLI gives net.recheck)', async () => {
  const order = [];
  const seq = {
    feeState: async () => { order.push('fee'); return { base_fee_exec: 1, base_fee_stor: 1 }; },
    nonces: async () => { order.push('nonces'); return [5]; },
    send: async () => { order.push('send'); return 'h'; },
  };
  const net = { error: 'refused: another LEZ', buildCall: () => { order.push('build'); return { wire: '00', size: 100, hash: 'h' }; } };
  const o = { node: { accountId: new Uint8Array(32) }, program: new Uint8Array(32).fill(7), policy: 'open', wait: false, net };
  await assert.rejects(MC.setPolicy(seq, { ...o, gate: async () => { order.push('gate'); return false; } }), /refused: another LEZ/);
  assert.deepEqual(order, ['fee', 'nonces', 'gate']);
  const cli = readFileSync(new URL('../../msg/ferrata-msg.mjs', import.meta.url), 'utf8');
  assert.equal((cli.match(/wait, net, gate \}/g) ?? []).length, 6, 'every signed command of the CLI passes the gate');
  assert.match(cli, /const gate = net \? \(\) => net\.recheck\(\) : null;/);
});

test('every page signature reads the version right before it, after the reads it depends on', () => {
  const app = src('app.js'), claim = src('claim.js'), prove = src('prove.js'), bridge = src('bridge.js'), inbox = src('inbox.js');
  const names = fn(app, 'async function namesTx', '\n}\n');
  assert.match(gatedRightBefore(names, 'versionConfirmed()', 'net.buildCall(', 'names'), /await Promise\.all/);
  const review = fn(app, "$('send-review').onclick", "\n};\n");
  assert.match(gatedRightBefore(review, 'versionConfirmed()', 'net.buildTransfer(', 'send review'), /resolveRecipient[\s\S]*await Promise\.all[\s\S]*seq\.feeState\(\)/);
  const go = fn(app, "$('send-go').onclick", "\n};\n");
  gatedRightBefore(go, 'versionConfirmed()', 'net.buildTransfer(', 'send');
  const faucet = fn(app, "$('faucet').onclick", "\n};\n");
  gatedRightBefore(faucet, 'versionConfirmed()', 'net.admissionKeyOf(', 'faucet');
  const sender = fn(claim, 'function sender(', '\n}\n');
  assert.match(gatedRightBefore(sender, 'c.versionConfirmed()', 'net.buildCall(', 'claims and burns'), /c\.seq\.balance\(/);
  gatedRightBefore(fn(claim, 'export async function planBurn', '\n}\n'), 'c.versionConfirmed()', 'net.buildCall(', 'burn review');
  assert.match(gatedRightBefore(fn(prove, 'send.onclick', 'catch (e)'), 'c.versionConfirmed()', 'net.buildCall(', 'attestation'), /await c\.refresh\(\)/);
  assert.match(inbox, /C\.call\(c\.seq, \{[^}]*gate: c\.versionConfirmed/, 'messages: the gate goes into C.call');
  const dReview = fn(bridge, 'review.onclick', 'go.onclick');
  assert.match(gatedRightBefore(dReview, 'depositVersionGate(c)', 'commitment(', 'deposit review'), /await connectWallet\(\)/);
  const dGo = fn(bridge, 'go.onclick', 'catch (e) { fail(e)');
  assert.match(gatedRightBefore(dGo, 'depositVersionGate(c)', "await P.request({ method: 'eth_sendTransaction'", 'deposit'), /readVault\([\s\S]*sendAndWait\(p\.approve/);
  assert.match(gatedRightBefore(dGo, 'depositVersionGate(c)', 'await sendAndWait(p.approve', 'approval'), /readVault\(/);
});
