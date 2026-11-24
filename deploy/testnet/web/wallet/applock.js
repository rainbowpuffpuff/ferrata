// App lock: unlock with a passkey (fingerprint, face or the device's screen lock) or a PIN instead of the passphrase.
// Both wrap the same AES key that the passphrase derives (keystore.js), so the wallet record itself does not change:
//
// - Passkey: WebAuthn's PRF extension. The authenticator (the phone's secure hardware, a password manager, a security
//   key) computes a secret from the passkey and a salt only after verifying the user; HKDF turns it into the key that
//   wraps the wallet's key. The secret never exists outside that moment and cannot be guessed offline. No server is
//   involved: nothing is registered anywhere, and the passkey only works on this site.
// - PIN: 6 to 12 digits through PBKDF2-SHA-256 (600,000 iterations). The page erases the PIN's wrap after 5 wrong PINs.
//   That limit is the page's own: someone who copies this browser's storage can try every PIN offline (about a minute on
//   one GPU for 6 digits, estimated from hashcat's PBKDF2-SHA-256 speed). The passphrase or a passkey resist that; a PIN
//   is a convenience against someone holding the phone.
//
// Stored (localStorage 'ferrata-wallet/v1/quick'): per method, its salt, the wrapped key (AES-256-GCM, bound to the
// method and to the wallet record's salt) and, for a passkey, its credential id. Never the key itself.
import * as ks from './keystore.js';

const QUICK = 'ferrata-wallet/v1/quick', PREFS = 'ferrata-wallet/v1/prefs';
const PIN_ITERATIONS = 600000, MAX_FAILS = 5;
const te = new TextEncoder();
const b64 = (u8) => btoa(String.fromCharCode(...new Uint8Array(u8)));
const unb64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const b64url = (u8) => b64(u8).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64url = (s) => unb64(s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4));
const random = (n) => crypto.getRandomValues(new Uint8Array(n));
const aad = (method, recSalt) => te.encode(`ferrata-wallet/quick/v1/${method}/${recSalt}`);

export function prefs() {
  try { return { lockHiddenS: -1, notify: false, ...JSON.parse(localStorage.getItem(PREFS) || '{}') }; } catch { return { lockHiddenS: -1, notify: false }; }
}
export function setPrefs(p) { localStorage.setItem(PREFS, JSON.stringify({ ...prefs(), ...p })); }

/** The quick-unlock record, if it belongs to the wallet record now in this browser. */
export function load(storage = globalThis.localStorage) {
  try {
    const q = JSON.parse(storage.getItem(QUICK) || 'null');
    return q && q.v === 1 && q.salt === ks.recordSalt(storage) ? q : null;
  } catch { return null; }
}
function save(q, storage = globalThis.localStorage) {
  if (!q.pin && !q.passkey) storage.removeItem(QUICK); else storage.setItem(QUICK, JSON.stringify(q));
}
/** Forget every quick-unlock method (a new or erased wallet). */
export function reset(storage = globalThis.localStorage) { storage.removeItem(QUICK); }
export function methods(storage = globalThis.localStorage) {
  const q = load(storage);
  return { pin: !!q?.pin, passkey: !!q?.passkey, fails: q?.pin?.fails ?? 0 };
}

async function wrap(kek, raw, method, recSalt) {
  const iv = random(12);
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: aad(method, recSalt) }, kek, raw);
  return { iv: b64(iv), ct: b64(ct) };
}
async function unwrap(kek, w, method, recSalt) {
  return new Uint8Array(await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64(w.iv), additionalData: aad(method, recSalt) }, kek, unb64(w.ct)));
}
async function pinKek(pin, salt, iterations) {
  const base = await crypto.subtle.importKey('raw', te.encode(pin), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}
/** HKDF-SHA-256 of the passkey's PRF output (salt: the wallet record's salt) into the wrapping key. */
export async function prfKek(prfOutput, recSalt) {
  const base = await crypto.subtle.importKey('raw', prfOutput, 'HKDF', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt: unb64(recSalt), info: te.encode('ferrata-wallet/quick/v1/passkey') },
    base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}

// ------------------------------------------------------------------ PIN
export function checkPin(pin) { if (!/^\d{6,12}$/.test(pin)) throw new Error('A PIN is 6 to 12 digits.'); }
export async function setPin(passphrase, pin, storage = globalThis.localStorage) {
  checkPin(pin);
  const raw = await ks.deriveRaw(passphrase, storage);
  try {
    const recSalt = ks.recordSalt(storage), salt = random(16);
    const w = await wrap(await pinKek(pin, salt, PIN_ITERATIONS), raw, 'pin', recSalt);
    const q = load(storage) ?? { v: 1, salt: recSalt };
    q.pin = { iterations: PIN_ITERATIONS, salt: b64(salt), ...w, fails: 0 };
    save(q, storage);
  } finally { raw.fill(0); }
}
export async function unlockPin(pin, storage = globalThis.localStorage) {
  const q = load(storage);
  if (!q?.pin) throw new Error('PIN unlock is not set up in this browser.');
  let raw;
  try {
    raw = await unwrap(await pinKek(pin, unb64(q.pin.salt), q.pin.iterations), q.pin, 'pin', q.salt);
  } catch {
    q.pin.fails = (q.pin.fails ?? 0) + 1;
    if (q.pin.fails >= MAX_FAILS) {
      delete q.pin; save(q, storage);
      throw new Error(`${MAX_FAILS} wrong PINs: PIN unlock is now off. Unlock with your passphrase.`);
    }
    save(q, storage);
    const left = MAX_FAILS - q.pin.fails;
    throw new Error(`Wrong PIN (${left} ${left === 1 ? 'try' : 'tries'} left before the PIN is erased).`);
  }
  try {
    q.pin.fails = 0; save(q, storage);
    return await ks.openWithKey(await ks.importRaw(raw), storage);
  } finally { raw.fill(0); }
}
export function removePin(storage = globalThis.localStorage) { const q = load(storage); if (q) { delete q.pin; save(q, storage); } }

// ------------------------------------------------------------------ passkey (WebAuthn PRF)
/** true / false when the browser says whether its passkeys have PRF; null when it does not say (then: try). */
export async function prfSupport() {
  if (!globalThis.PublicKeyCredential || !navigator.credentials?.create) return false;
  try {
    const caps = await PublicKeyCredential.getClientCapabilities?.();
    if (caps && 'extension:prf' in caps) return !!caps['extension:prf'];
  } catch { /* not told */ }
  return null;
}
async function prfGet(credId, prfSalt) {
  const a = await navigator.credentials.get({ publicKey: {
    challenge: random(32), rpId: location.hostname, allowCredentials: [{ type: 'public-key', id: credId }],
    userVerification: 'required', timeout: 120000, extensions: { prf: { eval: { first: prfSalt } } },
  } });
  const r = a?.getClientExtensionResults?.().prf?.results?.first;
  if (!r) throw new Error('The passkey answered without its PRF secret, so it cannot unlock the wallet on this device.');
  return new Uint8Array(r);
}
function forget(credId) {
  try { PublicKeyCredential.signalUnknownCredential?.({ rpId: location.hostname, credentialId: b64url(credId) })?.catch?.(() => {}); } catch { /* optional */ }
}
export async function setPasskey(passphrase, storage = globalThis.localStorage) {
  if ((await prfSupport()) === false) throw new Error('This browser\'s passkeys cannot derive an unlock key (no PRF extension). Use a PIN, or the passphrase.');
  const raw = await ks.deriveRaw(passphrase, storage);
  try {
    const recSalt = ks.recordSalt(storage), prfSalt = random(32);
    const day = new Date().toISOString().slice(0, 10);
    const cred = await navigator.credentials.create({ publicKey: {
      rp: { name: 'Ferrata wallet', id: location.hostname },
      user: { id: random(16), name: `Ferrata wallet ${day}`, displayName: `Ferrata wallet (test network, ${day})` },
      challenge: random(32),
      pubKeyCredParams: [{ type: 'public-key', alg: -7 }, { type: 'public-key', alg: -8 }, { type: 'public-key', alg: -257 }],
      authenticatorSelection: { userVerification: 'required', residentKey: 'preferred' },
      hints: ['client-device'], attestation: 'none', timeout: 120000,
      extensions: { prf: { eval: { first: prfSalt } } },
    } });
    const ext = cred.getClientExtensionResults?.() ?? {};
    if (!ext.prf || ext.prf.enabled === false) {
      forget(cred.rawId);
      throw new Error('This passkey cannot derive an unlock key (its authenticator has no PRF). Nothing was saved; use a PIN or the passphrase.');
    }
    const first = ext.prf.results?.first ? new Uint8Array(ext.prf.results.first) : await prfGet(cred.rawId, prfSalt);
    const w = await wrap(await prfKek(first, recSalt), raw, 'passkey', recSalt);
    first.fill(0);
    const q = load(storage) ?? { v: 1, salt: recSalt };
    q.passkey = { credId: b64url(cred.rawId), prfSalt: b64(prfSalt), ...w, created: Date.now() };
    save(q, storage);
  } finally { raw.fill(0); }
}
export async function unlockPasskey(storage = globalThis.localStorage) {
  const q = load(storage);
  if (!q?.passkey) throw new Error('Passkey unlock is not set up in this browser.');
  const first = await prfGet(unb64url(q.passkey.credId), unb64(q.passkey.prfSalt));
  let raw;
  try { raw = await unwrap(await prfKek(first, q.salt), q.passkey, 'passkey', q.salt); } catch {
    throw new Error('That passkey does not open this wallet.');
  } finally { first.fill(0); }
  try { return await ks.openWithKey(await ks.importRaw(raw), storage); } finally { raw.fill(0); }
}
export function removePasskey(storage = globalThis.localStorage) {
  const q = load(storage);
  if (!q?.passkey) return;
  forget(unb64url(q.passkey.credId));
  delete q.passkey; save(q, storage);
}

// ------------------------------------------------------------------ the page
const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(typeof k === 'string' ? document.createTextNode(k) : k);
  return e;
};

/**
 * Wires the unlock view and the settings. `onOpened(opened)`: the page's own unlock (keystore.open's result);
 * `lock()`: the page's lock; `unlocked()`: whether the wallet is open now.
 */
export function mountAppLock({ onOpened, lock, unlocked }) {
  let passkeyOnly = false;
  const quickErr = (m) => { $('quick-err').textContent = m; };
  async function viaPasskey() {
    quickErr(''); $('unlock-passkey').disabled = true;
    try { onOpened(await unlockPasskey()); } catch (e) { quickErr(e.name === 'NotAllowedError' ? 'The passkey prompt was closed or timed out.' : e.message); }
    finally { $('unlock-passkey').disabled = false; }
  }
  async function viaPin() {
    quickErr(''); $('unlock-pin-go').disabled = true;
    try { const o = await unlockPin($('unlock-pin').value); $('unlock-pin').value = ''; onOpened(o); }
    catch (e) { $('unlock-pin').value = ''; quickErr(e.message); renderUnlock(); }
    finally { $('unlock-pin-go').disabled = false; }
  }
  $('unlock-passkey').onclick = viaPasskey;
  $('unlock-pin-go').onclick = viaPin;
  $('unlock-pin').addEventListener('keydown', (e) => { if (e.key === 'Enter') viaPin(); });
  $('use-pass').onclick = (e) => { e.preventDefault(); passkeyOnly = true; renderUnlock(); $('unlock-pass').focus(); };

  function renderUnlock() {
    const m = methods();
    const quick = (m.pin || m.passkey) && !passkeyOnly;
    $('quick-unlock').hidden = !quick;
    $('pass-unlock').hidden = !!quick;
    $('unlock-passkey').hidden = !m.passkey;
    $('unlock-pin-box').hidden = !m.pin;
  }

  // Lock when the app has been off the screen for longer than the chosen time (checked when it comes back, so it does
  // not depend on timers running in the background, which phones stop).
  let hiddenAt = 0;
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') { hiddenAt = Date.now(); return; }
    const s = prefs().lockHiddenS;
    if (hiddenAt && s >= 0 && unlocked() && Date.now() - hiddenAt >= s * 1000) lock();
    hiddenAt = 0;
  });

  function renderSettings() {
    const box = $('lock-settings');
    box.textContent = '';
    const m = methods();
    const status = el('p', { 'data-testid': 'lock-status' }, 'Unlock with: passphrase',
      m.passkey ? ' · passkey (fingerprint, face or screen lock)' : '', m.pin ? ' · PIN' : '');
    const pass = el('input', { type: 'password', autocomplete: 'current-password', 'data-testid': 'lock-pass' });
    const out = el('p', { id: 'lock-msg', class: 'msg', role: 'status', 'data-testid': 'lock-msg' });
    const done = (t) => { pass.value = ''; renderSettings(); $('lock-msg').textContent = t; $('lock-msg').className = 'msg ok'; };
    const fail = (e) => { out.textContent = e.name === 'NotAllowedError' ? 'The passkey prompt was closed or timed out.' : e.message; out.className = 'msg bad'; };
    box.append(status, el('label', {}, 'Your passphrase (to turn a method on)', pass));

    const pk = el('button', { 'data-testid': m.passkey ? 'passkey-off' : 'passkey-on', text: m.passkey ? 'Turn off passkey unlock' : 'Turn on fingerprint / face unlock (passkey)' });
    pk.onclick = async () => {
      out.textContent = ''; pk.disabled = true;
      try {
        if (m.passkey) { removePasskey(); done('Passkey unlock is off. (The passkey may stay in your password manager; you can delete it there.)'); }
        else { await setPasskey(pass.value); setDefaultHide(); done('Passkey unlock is on: next time, unlock with your fingerprint, face or screen lock.'); }
      } catch (e) { fail(e); } finally { pk.disabled = false; }
    };
    const pin1 = el('input', { type: 'password', inputmode: 'numeric', autocomplete: 'off', maxlength: '12', 'data-testid': 'pin-new' });
    const pin2 = el('input', { type: 'password', inputmode: 'numeric', autocomplete: 'off', maxlength: '12', 'data-testid': 'pin-again' });
    const pinBtn = el('button', { 'data-testid': 'pin-set', text: m.pin ? 'Change PIN' : 'Set a PIN' });
    pinBtn.onclick = async () => {
      out.textContent = ''; pinBtn.disabled = true;
      try {
        checkPin(pin1.value);
        if (pin1.value !== pin2.value) throw new Error('The two PINs differ.');
        await setPin(pass.value, pin1.value); setDefaultHide();
        done('PIN unlock is on.');
      } catch (e) { fail(e); } finally { pinBtn.disabled = false; pin1.value = pin2.value = ''; }
    };
    const pinOff = el('button', { class: 'ghost small', 'data-testid': 'pin-off', text: 'Turn off PIN unlock' });
    pinOff.onclick = () => { removePin(); done('PIN unlock is off.'); };
    pinOff.hidden = !m.pin;

    const sel = el('select', { 'data-testid': 'lock-hidden' });
    for (const [v, t] of [[-1, 'never (only after idle time)'], [0, 'right away'], [60, 'after 1 minute'], [300, 'after 5 minutes']]) {
      const o = el('option', { value: String(v), text: t }); if (prefs().lockHiddenS === v) o.selected = true; sel.append(o);
    }
    sel.onchange = () => setPrefs({ lockHiddenS: Number(sel.value) });

    box.append(el('div', { class: 'row' }, pk),
      el('div', { class: 'row' }, el('label', { class: 'grow' }, 'New PIN (6 to 12 digits)', pin1), el('label', { class: 'grow' }, 'Again', pin2)),
      el('div', { class: 'row' }, pinBtn, pinOff), out,
      el('label', { class: 'inline' }, 'Lock when the app leaves the screen: ', sel),
      el('div', { class: 'privacy' }, el('b', { text: 'Passkey: ' }),
        'your fingerprint, face or screen lock releases a secret from the passkey (WebAuthn\'s PRF extension) that unwraps the wallet\'s key; it cannot be guessed offline, and no server is involved. Needs a passkey provider with PRF (recent Android with Google Password Manager, iOS 18 and later, Windows Hello, most security keys). ',
        el('b', { text: 'PIN: ' }),
        'erased after 5 wrong tries, but that limit is this page\'s own: whoever copies this browser\'s storage can try every 6-digit PIN offline in about a minute. Use it against someone picking up your phone, not against malware.'));
  }
  // With a quick unlock set, lock one minute after the app leaves the screen unless the user chose otherwise.
  function setDefaultHide() { const p = JSON.parse(localStorage.getItem(PREFS) || '{}'); if (p.lockHiddenS === undefined) setPrefs({ lockHiddenS: 60 }); }

  // Each lock starts again from the quick methods ("Use the passphrase instead" holds for one unlock only).
  const locked = () => { passkeyOnly = false; renderUnlock(); };
  return { renderUnlock, renderSettings, locked, reset: () => { reset(); passkeyOnly = false; } };
}
