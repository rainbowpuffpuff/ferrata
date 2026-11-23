// The wallet's secrets at rest: one record in the browser's localStorage, encrypted with a key derived from the
// user's passphrase (WebCrypto: PBKDF2-HMAC-SHA-256, 600,000 iterations, a random 16-byte salt; AES-256-GCM with a
// random 12-byte nonce, the record's header as associated data). The derived key is non-extractable and lives only in
// this page's memory while the wallet is unlocked. Nothing here is ever sent anywhere.
const STORE_KEY = 'ferrata-wallet/v1';
const ITERATIONS = 600000;
const te = new TextEncoder(), td = new TextDecoder();
const b64 = (u8) => btoa(String.fromCharCode(...u8));
const unb64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

async function deriveKey(passphrase, salt, iterations) {
  const base = await crypto.subtle.importKey('raw', te.encode(passphrase.normalize('NFKC')), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, base,
    { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}
const header = (r) => te.encode(JSON.stringify({ v: r.v, kdf: r.kdf, cipher: r.cipher }));

export function exists(storage = globalThis.localStorage) { return !!storage.getItem(STORE_KEY); }

export async function seal(key, salt, secret, storage = globalThis.localStorage) {
  const r = { v: 1, kdf: { name: 'PBKDF2-SHA256', iterations: ITERATIONS, salt: b64(salt) }, cipher: { name: 'AES-256-GCM' } };
  const iv = crypto.getRandomValues(new Uint8Array(12));
  r.cipher.iv = b64(iv);
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: header(r) }, key, te.encode(JSON.stringify(secret)));
  r.ciphertext = b64(new Uint8Array(ct));
  storage.setItem(STORE_KEY, JSON.stringify(r));
}

export async function create(passphrase, secret, storage = globalThis.localStorage) {
  if (passphrase.length < 8) throw new Error('choose a passphrase of at least 8 characters');
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const key = await deriveKey(passphrase, salt, ITERATIONS);
  await seal(key, salt, secret, storage);
  return { key, salt };
}

export async function open(passphrase, storage = globalThis.localStorage) {
  const raw = storage.getItem(STORE_KEY);
  if (!raw) throw new Error('no wallet in this browser');
  const r = JSON.parse(raw);
  if (r.v !== 1 || r.kdf?.name !== 'PBKDF2-SHA256' || r.cipher?.name !== 'AES-256-GCM') throw new Error('unknown wallet format');
  const salt = unb64(r.kdf.salt);
  const key = await deriveKey(passphrase, salt, r.kdf.iterations);
  let pt;
  try {
    pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64(r.cipher.iv), additionalData: header(r) }, key, unb64(r.ciphertext));
  } catch { throw new Error('wrong passphrase'); }
  return { key, salt, secret: JSON.parse(td.decode(pt)) };
}

/** Deletes the record. The secrets are gone unless the user kept the recovery phrase or the keys. */
export function erase(storage = globalThis.localStorage) { storage.removeItem(STORE_KEY); }
export function exportRecord(storage = globalThis.localStorage) { return storage.getItem(STORE_KEY); }
