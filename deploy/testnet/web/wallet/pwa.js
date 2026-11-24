// The wallet as an installable app. sw.js keeps an offline copy of exactly this build (each file checked against its
// SHA-256 before it is kept); this module registers it, shows when a new build is ready, offers installation (Chrome,
// Edge, Samsung Internet and Android: the browser's own prompt; iPhone and iPad: Share, then Add to Home Screen) and
// asks the browser to keep this origin's storage, where the encrypted wallet lives, out of automatic clean-ups.
const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(typeof k === 'string' ? document.createTextNode(k) : k);
  return e;
};

if (document.referrer.startsWith('android-app://')) sessionStorage.setItem('ferrata-android-app', '1');
export const inAndroidApp = () => sessionStorage.getItem('ferrata-android-app') === '1';
export const standalone = () => inAndroidApp() || navigator.standalone === true ||
  matchMedia('(display-mode: standalone)').matches || matchMedia('(display-mode: fullscreen)').matches;
const isIOS = () => /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

const P = { deferred: null, reg: null, build: null, files: 0, swError: null, persisted: null, installed: false, updateAsked: false };
export const pwaState = P;

export function mountPwa({ onOpen } = {}) {
  window.addEventListener('beforeinstallprompt', (e) => { e.preventDefault(); P.deferred = e; render(); });
  window.addEventListener('appinstalled', () => { P.deferred = null; P.installed = true; render(); });
  $('install').onclick = install;
  $('update-go').onclick = () => { P.updateAsked = true; P.reg?.waiting?.postMessage({ type: 'SKIP_WAITING' }); };
  const offline = () => { if (!navigator.onLine) { $('net-status').textContent = 'offline'; $('net-status').className = 'pill bad'; } };
  window.addEventListener('offline', offline); offline();
  if ('serviceWorker' in navigator) {
    register();
    navigator.serviceWorker.addEventListener('message', (e) => {
      if (e.data?.type === 'OPEN' && onOpen) onOpen(new URL(e.data.url).hash);
    });
  } else P.swError = 'this browser has no service workers';
  navigator.storage?.persisted?.().then((p) => { P.persisted = p; render(); }).catch(() => {});
  render();
}

async function register() {
  try {
    P.reg = await navigator.serviceWorker.register('sw.js', { scope: './', updateViaCache: 'none' });
  } catch (e) { P.swError = e.message; render(); return; }
  const reg = P.reg;
  const waiting = () => { $('update-banner').hidden = !(reg.waiting && navigator.serviceWorker.controller); };
  waiting();
  reg.addEventListener('updatefound', () => {
    const w = reg.installing;
    w?.addEventListener('statechange', () => { if (w.state === 'installed') waiting(); });
  });
  navigator.serviceWorker.addEventListener('controllerchange', () => {
    if (P.updateAsked) { P.updateAsked = false; location.reload(); return; }
    askBuild();
  });
  // An installed app can stay open for days: look for a new build whenever it comes back to the screen.
  document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') reg.update().catch(() => {}); });
  await navigator.serviceWorker.ready;
  askBuild();
}

function askBuild() {
  const sw = navigator.serviceWorker.controller;
  if (!sw) return;
  const ch = new MessageChannel();
  ch.port1.onmessage = (e) => { P.build = e.data?.build ?? null; P.files = e.data?.files ?? 0; render(); };
  sw.postMessage({ type: 'BUILD' }, [ch.port2]);
}

async function install() {
  if (!P.deferred) return;
  const e = P.deferred; P.deferred = null;
  e.prompt();
  const choice = await e.userChoice.catch(() => null);
  if (choice?.outcome === 'accepted') P.installed = true;
  render();
}

/** Asks the browser to keep this origin's storage (the encrypted wallet) out of automatic clean-ups. */
export async function keepStorage() {
  try { P.persisted = (await navigator.storage?.persisted?.()) || (await navigator.storage?.persist?.()) || false; } catch { P.persisted = false; }
  render();
  return P.persisted;
}

function render() {
  $('install').hidden = !P.deferred || standalone();
  const box = $('app-settings');
  if (!box) return;
  box.textContent = '';
  const line = (...k) => box.append(el('p', { class: 'fine' }, ...k));
  if (standalone()) line(inAndroidApp() ? 'Running as the Android app (the same page, in Chrome\'s engine).' : 'Running as an installed app.');
  else if (P.installed) line('Installed: open it from your home screen or app list.');
  else if (P.deferred) {
    const b = el('button', { class: 'primary', 'data-testid': 'install-more', text: 'Install the wallet as an app' });
    b.onclick = install;
    box.append(b);
  } else if (isIOS()) line('On iPhone or iPad: tap Share, then “Add to Home Screen”. The app opens full screen and keeps working offline.');
  else line('To install: your browser\'s menu, “Install app” or “Add to Home screen”.');
  const a = el('a', { href: '../android/', text: 'Android app (APK)' });
  line(a, ': the same wallet as an Android app, with its checksum and signing key published.');
  const sw = P.swError ? `Offline copy: not available (${P.swError}).`
    : P.build ? `Offline copy: build ${P.build}, ${P.files} files, each checked against its SHA-256 before it was kept.`
      : 'Offline copy: being prepared…';
  line(el('span', { 'data-testid': 'sw-state', text: sw }));
  const keep = el('span', { 'data-testid': 'storage-state', text: P.persisted ? 'Storage: kept (the browser will not clear it on its own).'
    : 'Storage: the browser may clear it when space runs low; keep your recovery phrase written down.' });
  line(keep);
  if (!P.persisted && navigator.storage?.persist) {
    const k = el('button', { class: 'small', 'data-testid': 'storage-keep', text: 'Ask to keep it' });
    k.onclick = keepStorage;
    box.append(k);
  }
}
export const renderPwa = render;
