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

test('a published relayer: named, its amount, what it learns, the opt-out', () => {
  const l = gasDropLine(200, terms, 'https://n.example/relayer/gas-drop');
  assert.equal(l.state, 'on');
  for (const part of [terms.relayer, '200000000 native LEZ per deposit', 'private claim and burn', 'never which deposit', '--no-gas-drop', 'https://n.example/relayer/gas-drop']) {
    assert.ok(l.text.includes(part), part);
  }
});

test('none, paused, unreachable and malformed answers', () => {
  assert.equal(gasDropLine(404, { error: 'x' }, 'u').state, 'none');
  assert.match(gasDropLine(404, null, 'u').text, /own private account/);
  const p = gasDropLine(503, { error: 'the gas-drop relayer is paused: refilling' }, 'u');
  assert.equal(p.state, 'paused'); assert.match(p.text, /refilling/);
  assert.equal(gasDropLine(null, null, 'u').state, 'unreachable');
  assert.equal(gasDropLine(502, null, 'u').state, 'unreachable');
  assert.equal(gasDropLine(200, { relayer: 'not hex' }, 'u').state, 'none');
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
});
