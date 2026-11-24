// LEZ's private key tree in the page against LEZ rc3's own pinned vectors (key_protocol keys_private.rs
// master_key_generation): ssk, ccc, ask, nsk, npk and the 1,184-byte ML-KEM-768 viewing key, read from LEZ's source.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as lez from '../lez.js';

const file = fileURLToPath(new URL('../../../../../../upstream/lez-v0.4.0-rc3/lee/key_protocol/src/key_management/key_tree/keys_private.rs', import.meta.url));
const arrays = (src, name) => {
  const i = src.indexOf(`let ${name}`);
  const eq = src.indexOf('=', i);
  const open = src.indexOf('[', eq), close = src.indexOf(']', open);
  return Uint8Array.from(src.slice(open + 1, close).split(',').map((x) => x.trim()).filter(Boolean).map(Number));
};

test('private master keys equal LEZ keys_private.rs master_key_generation', { skip: !existsSync(file) }, () => {
  const src = readFileSync(file, 'utf8');
  const body = src.slice(src.indexOf('fn master_key_generation'));
  const seedSrc = src.slice(src.indexOf('const SEED'));
  const SEED = Uint8Array.from(seedSrc.slice(seedSrc.indexOf('[', seedSrc.indexOf('=')) + 1, seedSrc.indexOf(']', seedSrc.indexOf('='))).split(',').map((x) => x.trim()).filter(Boolean).map(Number));
  assert.equal(SEED.length, 64);
  const k = lez.privateMaster(SEED);
  assert.deepEqual(k.ssk, arrays(body, 'expected_ssk'));
  assert.deepEqual(k.ccc, arrays(body, 'expected_ccc'));
  assert.deepEqual(k.ask, arrays(body, 'expected_ask'));
  assert.deepEqual(k.nsk, arrays(body, 'expected_nsk'));
  assert.deepEqual(k.npk, arrays(body, 'expected_npk'));
  const vpk = arrays(body, 'expected_vpk');
  assert.equal(vpk.length, 1184);
  assert.deepEqual(k.vpk, vpk);
  const child = lez.privateChild(k, 0);
  assert.equal(child.vpk.length, 1184);
  assert.notDeepEqual(child.accountId, k.accountId);
});
