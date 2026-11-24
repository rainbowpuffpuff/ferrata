// The wallet's secrets at rest: one record in the browser's localStorage, encrypted with a key derived from the
// user's passphrase (WebCrypto: PBKDF2-HMAC-SHA-256, 600,000 iterations, a random 16-byte salt; AES-256-GCM with a
// random 12-byte nonce, the record's header as associated data). The derived key is non-extractable and lives only in
// this page's memory while the wallet is unlocked. Nothing here is ever sent anywhere.
const STORE_KEY = 'ferrata-wallet/v1';
const ITERATIONS = 600000;
const te = new TextEncoder(), td = new TextDecoder();
// In 32 KiB slices: String.fromCharCode(...u8) on the whole record overflows the call stack past ~125-200 KB.
const b64 = (u8) => { let s = ''; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode(...u8.subarray(i, i + 0x8000)); return btoa(s); };
const unb64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

async function deriveKey(passphrase, salt, iterations) {
  const base = await crypto.subtle.importKey('raw', te.encode(passphrase.normalize('NFKC')), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, base,
    { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}
const header = (r) => te.encode(JSON.stringify({ v: r.v, kdf: r.kdf, cipher: r.cipher }));

export function exists(storage = globalThis.localStorage) { return !!storage.getItem(STORE_KEY); }

/**
 * Encrypts `secret` under `key` (derived from the passphrase with `salt`). The iteration count written is the one the
 * key was derived with: `iterations` when given (a new record), else the stored record's own (same salt), else today's.
 */
export async function seal(key, salt, secret, storage = globalThis.localStorage, iterations = null) {
  let it = iterations;
  if (it === null) {
    try { const prev = JSON.parse(storage.getItem(STORE_KEY) ?? 'null'); if (prev?.kdf?.salt === b64(salt)) it = prev.kdf.iterations; } catch { /* none */ }
  }
  const r = { v: 1, kdf: { name: 'PBKDF2-SHA256', iterations: it ?? ITERATIONS, salt: b64(salt) }, cipher: { name: 'AES-256-GCM' } };
  const iv = crypto.getRandomValues(new Uint8Array(12));
  r.cipher.iv = b64(iv);
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: header(r) }, key, te.encode(JSON.stringify(secret)));
  r.ciphertext = b64(new Uint8Array(ct));
  storage.setItem(STORE_KEY, JSON.stringify(r));
}

/** A new record. Refused when this browser already holds one (another tab may have made it): erase it first. */
export async function create(passphrase, secret, storage = globalThis.localStorage) {
  if (passphrase.length < 8) throw new Error('choose a passphrase of at least 8 characters');
  if (exists(storage)) throw new Error('This browser already holds a wallet (made in another tab?): unlock it, or erase it first.');
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const key = await deriveKey(passphrase, salt, ITERATIONS);
  await seal(key, salt, secret, storage, ITERATIONS);
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

// Quick unlock (applock.js): the same AES key as raw bytes, so that a PIN or a passkey can wrap it. PBKDF2's first 256
// bits are the key `deriveKey` makes, so the record's format does not change.
function record(storage) {
  const raw = storage.getItem(STORE_KEY);
  if (!raw) throw new Error('no wallet in this browser');
  const r = JSON.parse(raw);
  if (r.v !== 1 || r.kdf?.name !== 'PBKDF2-SHA256' || r.cipher?.name !== 'AES-256-GCM') throw new Error('unknown wallet format');
  return r;
}
/** The record's salt (base64): a quick-unlock wrap names the record it belongs to by it. */
export function recordSalt(storage = globalThis.localStorage) { return storage.getItem(STORE_KEY) ? record(storage).kdf.salt : null; }
/** The record's AES key as 32 raw bytes, from the passphrase. Checked: throws 'wrong passphrase' if it does not open. */
export async function deriveRaw(passphrase, storage = globalThis.localStorage) {
  const r = record(storage);
  const base = await crypto.subtle.importKey('raw', te.encode(passphrase.normalize('NFKC')), 'PBKDF2', false, ['deriveBits']);
  const raw = new Uint8Array(await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt: unb64(r.kdf.salt), iterations: r.kdf.iterations }, base, 256));
  await openWithKey(await importRaw(raw), storage);
  return raw;
}
export function importRaw(raw) { return crypto.subtle.importKey('raw', raw, { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']); }
/** Opens the record with an AES key already derived (from the passphrase, a PIN's wrap or a passkey's). */
export async function openWithKey(key, storage = globalThis.localStorage) {
  const r = record(storage);
  let pt;
  try {
    pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64(r.cipher.iv), additionalData: header(r) }, key, unb64(r.ciphertext));
  } catch { throw new Error('wrong passphrase'); }
  return { key, salt: unb64(r.kdf.salt), secret: JSON.parse(td.decode(pt)) };
}

/** Deletes the record. The secrets are gone unless the user kept the recovery phrase or the keys. */
export function erase(storage = globalThis.localStorage) { storage.removeItem(STORE_KEY); }
export function exportRecord(storage = globalThis.localStorage) { return storage.getItem(STORE_KEY); }
