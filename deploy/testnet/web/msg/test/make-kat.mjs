// Writes test/kat.json with OpenSSL (3.5 or later, for ML-KEM-768) and node:crypto, and nothing of ferrata-msg.js or
// @noble: the account's messaging keys at epochs 0 and 1 (HKDF here, X25519 and ML-KEM-768 key generation from the seed
// by OpenSSL), two OpenSSL encapsulations to the epoch-0 key, and four v2 envelopes sealed by test/xcrypto.mjs with
// fixed inputs. test/independent.test.mjs checks the library against these without OpenSSL.
//   node deploy/testnet/web/msg/test/make-kat.mjs        (scratch: a temporary directory under the repository's target/)
import { execFileSync } from 'node:child_process';
import { writeFileSync, readFileSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as X from './xcrypto.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const base = join(here, '../../../../../target');
mkdirSync(base, { recursive: true });
const work = mkdtempSync(join(base, 'msg-kat-'));
let nfile = 0;
const f = (name) => join(work, `${nfile++}-${name}`);
const ossl = (args) => execFileSync('openssl', args, { stdio: ['ignore', 'pipe', 'pipe'] });
const version = ossl(['version']).toString().trim();
function kemFromSeed(seed64) {
  const priv = f('mlkem.pem');
  ossl(['genpkey', '-algorithm', 'ML-KEM-768', '-pkeyopt', `hexseed:${X.hex(seed64)}`, '-out', priv]);
  const pubDer = f('pub.der');
  ossl(['pkey', '-in', priv, '-pubout', '-outform', 'DER', '-out', pubDer]);
  const der = new Uint8Array(readFileSync(pubDer));
  return der.subarray(der.length - 1184);
}
function kemEncap(pubRaw) {
  const spki = X.cat(X.H('308204b2300b0609608648016503040402038204a100'), pubRaw);
  const pf = f('pub.der'), ctf = f('ct.bin'), ssf = f('ss.bin');
  writeFileSync(pf, spki);
  ossl(['pkeyutl', '-encap', '-pubin', '-keyform', 'DER', '-inkey', pf, '-out', ctf, '-secret', ssf]);
  return { ct: new Uint8Array(readFileSync(ctf)), ss: new Uint8Array(readFileSync(ssf)) };
}

const secret = new Uint8Array(32).fill(7);
const keys = [0, 1].map((epoch) => {
  const seed = X.hkdf(secret, X.SALT, X.cat(Buffer.from('keys'), X.u32(epoch)), 96);
  return { epoch, x25519_secret: X.hex(seed.subarray(0, 32)), x25519_public: X.hex(X.xPubOf(seed.subarray(0, 32))), mlkem768_public: X.hex(kemFromSeed(seed.subarray(32, 96))) };
});
const ek0 = X.H(keys[0].mlkem768_public);
const encaps = [kemEncap(ek0), kemEncap(ek0)].map((e) => ({ ct: X.hex(e.ct), ss: X.hex(e.ss) }));
const A = new Uint8Array(32).fill(0x41), B = new Uint8Array(32).fill(0x42);
const rx = X.H(keys[0].x25519_public);
const fill = (n, v) => new Uint8Array(n).fill(v);
const envelopes = [[true, 2048, 0], [true, 4096, 1], [false, 512, null], [false, 1024, null]].map(([hybrid, bucket, k], i) => {
  const text = `independent v2 ${hybrid ? 'hybrid' : 'X25519'} envelope in ${bucket} B`;
  const kem = hybrid ? { ct: X.H(encaps[k].ct), ss: X.H(encaps[k].ss) } : null;
  const body = X.independentSeal({ hybrid, epoch: 0, rx, sender: A, recipient: B, text, kind: 1, sentMs: 42 + i, bucket, esk: fill(32, 0x30 + i), nonce: fill(24, 0x50 + i), kem });
  return { hybrid, bucket, text, sent_ms: 42 + i, body: X.hex(body) };
});
const out = {
  note: `written by test/make-kat.mjs with ${version} and node ${process.version}; checked by test/independent.test.mjs`,
  secret: X.hex(secret), sender: X.hex(A), recipient: X.hex(B), keys, encapsulations: encaps, envelopes,
};
writeFileSync(join(here, 'kat.json'), JSON.stringify(out, null, 1) + '\n');
rmSync(work, { recursive: true, force: true });
console.log(`kat.json written (${version})`);
