// The app's pieces that run without a browser: the service worker's file list, the manifest and icons, LEZ's block
// hash recomputed in the page (on blocks of the public test network), the PIN wrap of the wallet's key, the QR codes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { ROOT, listing, swWith, servedFiles } from './pwa-files.mjs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import * as lez from '../lez.js';
import * as ks from '../keystore.js';
import * as lockm from '../applock.js';
import { parseScanned, receiveCode } from '../qr.js';
import { encodeQR, decodeQR } from '../vendor/qr.js';

const read = (f) => readFileSync(join(ROOT, f));
const shim = () => { const m = new Map(); return { getItem: (k) => (m.has(k) ? m.get(k) : null), setItem: (k, v) => m.set(k, String(v)), removeItem: (k) => m.delete(k) }; };

test('sw.js lists exactly the files on disk, with their SHA-256', () => {
  const sw = read('sw.js').toString();
  const l = listing();
  assert.equal(swWith(sw, l), sw, 'run node test/pwa-files.mjs');
  for (const f of ['index.html', 'app.js', 'manifest.json', 'vendor/noble.js', 'vendor/qr.js', 'icons/icon-192.png']) assert.ok(l.files[f], f);
  for (const f of Object.keys(l.files)) assert.ok(!f.startsWith('test/') && f !== 'config.json' && f !== 'README.md', f);
});

test('the manifest makes the page installable: names, scope, standalone, icons of the sizes they claim', () => {
  const m = JSON.parse(read('manifest.json'));
  assert.ok(m.name && m.short_name && m.short_name.length <= 12);
  assert.equal(m.display, 'standalone');
  assert.equal(m.scope, './');
  assert.ok(m.start_url.startsWith('./'));
  const png = (f) => { const b = read(f); assert.equal(b.subarray(1, 4).toString(), 'PNG', f); return [b.readUInt32BE(16), b.readUInt32BE(20)]; };
  for (const i of m.icons) {
    assert.ok(existsSync(join(ROOT, i.src)), i.src);
    if (i.type === 'image/png') { const [w, h] = png(i.src); assert.equal(`${w}x${h}`, i.sizes, i.src); }
  }
  const sizes = new Set(m.icons.filter((i) => (i.purpose ?? 'any') === 'any').map((i) => i.sizes));
  assert.ok(sizes.has('192x192') && sizes.has('512x512'));
  assert.ok(m.icons.some((i) => i.purpose === 'maskable'));
  for (const s of m.shortcuts) assert.ok(s.url.startsWith('./#'), s.url);
  const html = read('index.html').toString();
  assert.match(html, /<link rel="manifest" href="manifest.json">/);
  assert.match(html, /worker-src 'self'/);
  assert.deepEqual(png('icons/apple-touch-icon.png'), [180, 180]);
});

test('block hashes: recomputed from the contents equal the hash each public-network block carries', () => {
  const { blocks } = JSON.parse(read('test/fixtures/live-blocks.json'));
  for (const [id, raw] of Object.entries(blocks)) {
    const b = lez.decodeBlock(raw);
    assert.equal(b.header.id, Number(id));
    assert.ok(b.hashOk, `block ${id}`);
  }
  const bytes = lez.base64.decode(blocks['152']);
  bytes[200] ^= 1; // one bit of a transaction
  const t = lez.blockHashOf(bytes);
  assert.notEqual(t, lez.decodeBlock(blocks['152']).header.hash);
});

test('PIN unlock: wraps the passphrase\'s key, opens the same wallet, 5 wrong PINs erase it, a new wallet drops it', async () => {
  const st = shim();
  const secret = { v: 1, mnemonic: null, accounts: [{ kind: 'raw', sk: '11'.repeat(32), label: 'A' }] };
  await ks.create('correct horse 1', secret, st);
  await assert.rejects(lockm.setPin('wrong passphrase', '123456', st), /wrong passphrase/);
  await assert.rejects(lockm.setPin('correct horse 1', '12345', st), /6 to 12 digits/);
  await lockm.setPin('correct horse 1', '246810', st);
  assert.deepEqual(lockm.methods(st), { pin: true, passkey: false, fails: 0 });
  const raw = st.getItem('ferrata-wallet/v1/quick');
  assert.ok(!raw.includes('11'.repeat(32)));
  const o = await lockm.unlockPin('246810', st);
  assert.deepEqual(o.secret, secret);
  for (let i = 1; i <= 4; i++) await assert.rejects(lockm.unlockPin('000000', st), new RegExp(`${5 - i} tr`));
  assert.equal(lockm.methods(st).fails, 4);
  await assert.rejects(lockm.unlockPin('000000', st), /PIN unlock is now off/);
  assert.equal(lockm.methods(st).pin, false);
  await lockm.setPin('correct horse 1', '246810', st);
  await assert.rejects(ks.create('another one 2', secret, st), /already holds a wallet/); // never over a record (two tabs)
  ks.erase(st);
  await ks.create('another one 2', secret, st); // a new wallet record: the old wrap no longer applies
  assert.equal(lockm.methods(st).pin, false);
});

test('passkey wrap: the key from a PRF output opens only with that output', async () => {
  const salt = btoa('s'.repeat(16));
  const a = await lockm.prfKek(new Uint8Array(32).fill(7), salt), b = await lockm.prfKek(new Uint8Array(32).fill(8), salt);
  const iv = new Uint8Array(12), pt = new Uint8Array(32).fill(1);
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, a, pt);
  assert.deepEqual(new Uint8Array(await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, a, ct)), pt);
  await assert.rejects(crypto.subtle.decrypt({ name: 'AES-GCM', iv }, b, ct));
});

test('scanned codes: receive codes (with an amount), ids, keys and names; anything else refused', () => {
  const id = '9mFZ4yVq5b2cX1n3HjK8pQrStUvWxYz12345678', pk = 'ab'.repeat(32);
  assert.deepEqual(parseScanned(`lez:${id}?pk=${pk}`), { to: `lez:${id}?pk=${pk}`, amount: null });
  assert.deepEqual(parseScanned(` lez:${id}?pk=${pk.toUpperCase()}&amount=1.5 `), { to: `lez:${id}?pk=${pk}`, amount: '1.5' });
  const initial = '00' + 'ab'.repeat(32);
  assert.deepEqual(parseScanned(`lez:${id}?pk=${initial}`), { to: `lez:${id}?pk=${initial}`, amount: null });
  assert.equal(parseScanned(`lez:${id}?pk=${'ab'.repeat(32)}0`), null); // an odd 65 hex digits is neither version's key
  assert.deepEqual(parseScanned(`lez:${id}?pk=${pk}&amount=1.1234567891`), { to: `lez:${id}?pk=${pk}`, amount: null });
  assert.deepEqual(parseScanned(`lez:${id}`), { to: id, amount: null });
  assert.equal(parseScanned(`lez:${id}?pk=zz`), null);
  assert.deepEqual(parseScanned(id), { to: id, amount: null });
  assert.deepEqual(parseScanned('alice.lez'), { to: 'alice.lez', amount: null });
  assert.equal(parseScanned('https://example.com/pay'), null);
  assert.equal(receiveCode(id, pk, '2'), `lez:${id}?pk=${pk}&amount=2`);
});

test('QR: a receive code with an amount survives encoding and decoding (vendor/qr.js)', () => {
  const code = receiveCode('9mFZ4yVq5b2cX1n3HjK8pQrStUvWxYz12345678', 'cd'.repeat(32), '12.75');
  const m = encodeQR(code, 'raw', { ecc: 'medium', border: 4 });
  const s = 3, n = m.length * s, data = new Uint8ClampedArray(n * n * 4);
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    const v = m[Math.floor(y / s)][Math.floor(x / s)] ? 0 : 255, i = (y * n + x) * 4;
    data[i] = data[i + 1] = data[i + 2] = v; data[i + 3] = 255;
  }
  assert.equal(decodeQR({ width: n, height: n, data }), code);
});

test('the e2e server (test/serve.py) serves every file a deployment installs (review M-W4)', () => {
  const r = spawnSync('python3', ['-I', fileURLToPath(new URL('./serve.py', import.meta.url)), '--list'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const served = r.stdout.trim().split('\n').sort();
  assert.deepEqual(served, [...servedFiles(), 'config.json', 'sw.js'].sort());
});
