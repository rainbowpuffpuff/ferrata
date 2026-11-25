// The bridge tab's fee-payer line (RFP-022 U.5, Priv.7): the network's gas-drop relayer, read from the RPC's origin as
// the `ferrata` tool reads it, named with what it learns and the opt-out; the other answers say what happens instead.
import test from 'node:test';
import assert from 'node:assert/strict';
import { gasDropUrl, gasDropLine, showGasDrop } from '../bridge.js';

const terms = { relayer: '0x' + '11'.repeat(32), imageId: '0x' + '89'.repeat(32), amount: '200000000', covers: ['claim', 'burn'] };

test('the terms are read at the origin of the page\'s RPC', () => {
  assert.equal(gasDropUrl('/rpc', 'https://ferrata.example/wallet/'), 'https://ferrata.example/relayer/gas-drop');
  assert.equal(gasDropUrl('https://rpc.example/', 'https://ferrata.example/wallet/'), 'https://rpc.example/relayer/gas-drop');
});

test('a published relayer: named, its amount, what it learns, who is paid the relay fee, the opt-out', () => {
  const l = gasDropLine(200, terms, 'https://n.example/relayer/gas-drop');
  assert.equal(l.state, 'on');
  for (const part of [terms.relayer, '200000000 native LEZ per deposit', 'private claim and burn', 'never which deposit',
    'relay fee', 'paid to it', '--no-gas-drop', 'https://n.example/relayer/gas-drop']) {
    assert.ok(l.text.includes(part), part);
  }
});

test('only 404 and 410 say "no relayer"; 429, other errors and malformed answers say the tool stops', () => {
  for (const s of [404, 410]) {
    assert.equal(gasDropLine(s, { error: 'x' }, 'u').state, 'none');
    assert.match(gasDropLine(s, null, 'u').text, /own private account/);
  }
  const p = gasDropLine(503, { error: 'the gas-drop relayer is paused: refilling' }, 'u');
  assert.equal(p.state, 'paused'); assert.match(p.text, /refilling/);
  const l = gasDropLine(429, { error: 'too many gas-drop reads' }, 'u');
  assert.equal(l.state, 'limited'); assert.match(l.text, /stops a private claim/);
  for (const [s, t] of [[null, null], [502, null], [503, { error: 'the terms cannot be read now' }], [403, {}], [405, null],
    [200, { relayer: 'not hex', amount: '1' }], [200, { relayer: terms.relayer }], [200, null]]) {
    const u = gasDropLine(s, t, 'u');
    assert.equal(u.state, 'unreachable', `${s} ${JSON.stringify(t)}`);
    assert.match(u.text, /stops a private claim/);
    assert.doesNotMatch(u.text, /publishes no gas-drop relayer/);
  }
});

test('showGasDrop fills the node from the network', async () => {
  const node = { textContent: '', dataset: {} };
  const seen = [];
  const fake = async (url) => { seen.push(url); return { status: 200, json: async () => terms }; };
  const l = await showGasDrop(node, '/rpc', 'https://n.example/wallet/', fake);
  assert.deepEqual(seen, ['https://n.example/relayer/gas-drop']);
  assert.equal(l.state, 'on'); assert.equal(node.dataset.state, 'on'); assert.ok(node.textContent.includes(terms.relayer));
  const down = await showGasDrop(node, '/rpc', 'https://n.example/wallet/', async () => { throw new Error('offline'); });
  assert.equal(down.state, 'unreachable'); assert.equal(node.dataset.state, 'unreachable');
  const busy = await showGasDrop(node, '/rpc', 'https://n.example/wallet/', async () => ({ status: 429, json: async () => ({}) }));
  assert.equal(busy.state, 'limited');
});
