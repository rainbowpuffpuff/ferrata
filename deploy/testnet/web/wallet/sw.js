// The wallet's service worker: an offline copy of exactly one build of the page. It keeps the files listed below, each
// checked against its SHA-256 before it is kept (a build that does not match, for example one caught half-deployed, is
// refused and the previous one stays), serves them from that copy, and fetches config.json from the network first (the
// copy only when offline). It never sees the wallet's keys and never touches the network calls: the RPC and the faucet
// are POSTs and pass straight through. A new build waits until the page asks for it (the "Reload" banner).
// BEGIN FILES (written by test/pwa-files.mjs; do not edit by hand)
const BUILD = 'ea3e9b37f6a18882';
const FILES = {
  'app.js': '9eec8abbe2d76731a005ae72c06bc694b4ba38d6cf4839af06d07414e6b05484',
  'applock.js': 'a59bb7c8aa6978c1f9581599dc8c05f7df4598bc69ec62d9a66d406dce47eae4',
  'attest.js': '5fd1ee5d0f85bbf101ea0e4df618280e67d013ec609e50e3fdc92d966941b007',
  'bridge.js': 'd259b05812f9fd3c2bbacd7664c5068e3b76073e05633947f2583e225552cdb4',
  'claim.js': '32c3a495de7bfcef2deb60fb835ee6432b9bd6e7c81f52d5f4cb5d270609cb9e',
  'eth.js': 'd3b8f296199680b6562c6ccb1df6109658802f8fbfe0e34e33716e7275f67cb2',
  'history.js': '2a469a3b8b3db2e701e77c035c8e99577e23cf166f3a8fd7e726a0defb4cd9f4',
  'icons/apple-touch-icon.png': 'fdcf2fff62a99f4ceb4e501582a2a96c178d29deba682891d507c05dc588f3f0',
  'icons/icon-192.png': '053f64be2da029112675ce3a9d1eb560e018a2bb003c9abda697e91d28cda826',
  'icons/icon-512.png': '32b6be55c02d9e923fdf95104c5ccc609cc67c04c40f202afc18eca18d206958',
  'icons/icon.svg': '786437b05e3b1010a3fb14a2951ef00bc464b40dff40cb48104d3e4137d74b9f',
  'icons/maskable-512.png': '51472db5df63edeeaa3d1145254210a653b4db0779f67d3c3f382da0308f0bbc',
  'icons/maskable.svg': '272260e424f12d7d0272db2b3694194f5c5a644f6f9ac95f47345f5265a1c2b2',
  'icons/mono-512.png': '227054b028b807136b615d63aedaf3d7e1f812a4a597c6fd4b3b1a25c1763ddd',
  'icons/mono-96.png': '7ce71dd0cc5fe0f2811e86cde4de8bcb24d372eb7e80156365d9c603817649cf',
  'icons/mono.svg': 'c51c91efa3c7c7f6251d49cb5df0709ffb43f5bb3fff3d562a49b39fe6afeb95',
  'inbox.js': 'b44278ee6f5eac456bc8298e79ccf4ccd07215b004a5d8444d569efac901ad19',
  'index.html': '7d058586bfc16a1d55f4b212dbc81cab7e8134ef70ab04e5fc94b5c4d6c61f66',
  'keystore.js': '3f38a0d0384c21e05a0ed5413264d2fbba64bf0520ff4088bc69a04281a673e8',
  'lez.js': 'd278674401ed2aa47ad87d4ba1fa7846324dbfcc12911be0903e1bf407302c1e',
  'manifest.json': 'ae6f89d3122bcbfc787a90ffc90a14c7c8f351d7a6623a74df47d5e0c0a2e07f',
  'msg/client.js': 'd22aec6ce6537c48db99f1158bc8af676815d8b2c2e7873abfe38be71b44d17c',
  'msg/ferrata-msg.js': 'ff59d6f680f7e791b735a8e38a6cce017eeebe9f85db8626ae6bab60a4ea5805',
  'msg/vendor/crypto.js': '44a587ee8f6c403f8cbbc6ad0a157f8073f2e8860f0e814ffa49cbe7212e1f16',
  'notify.js': '879b9cec9e1fdfd787e6712821c272efadb66b80857d9b124b6f12c7e9477393',
  'private.js': 'a7e26a9ac145465de3381b9865370e70955914b5d50229c6059b824a136b5173',
  'prove.js': '6dc9dcdeab364d67d4e6e8e9fbf5186e42bdb7856b79281bf385e62de7d93856',
  'pwa.js': '22b8e74f9d222d9e3870086602935b4cdd9a60d9eb423acdea88eb9927198594',
  'qr.js': '4c5645f1201f537b7c083d7598dd263a31fa8d70e1665c206bc8905add158b9b',
  'settings.js': 'd992f06aebe7380281129fb73e3cda4b9a25248ca79ec2ca870602810df86c38',
  'style.css': '9d6ba21e0b4dd8a5c5584724c779d4c57fc5019a8c9b34d27ab411f9fb6eb7c4',
  'vendor/names.js': '40aea7543b30d387271915318d88e34d48800b28ea7c15a17b9a3505e82e40a2',
  'vendor/noble.js': '4350804c442ef6064d154f3bc1ad23d31243ec1c67e3fe5062f963cf776d143e',
  'vendor/qr.js': 'c2bca84ede724ae5ad56493c33b64768b8fd0e17fd52dc244613af4245186ee7',
};
// END FILES
const CACHE = `ferrata-wallet-${BUILD}`;
const CONFIG = 'ferrata-wallet-config';
const hex = (buf) => Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('');

const digest = async (res) => hex(await crypto.subtle.digest('SHA-256', await res.clone().arrayBuffer()));

// A file the previous build already holds with the same hash is copied, not downloaded again.
async function fromOlder(url, sha) {
  for (const k of await caches.keys()) {
    if (!k.startsWith('ferrata-wallet-') || k === CACHE || k === CONFIG) continue;
    const hit = await (await caches.open(k)).match(url);
    if (hit && await digest(hit) === sha) return hit;
  }
  return null;
}

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    const kept = new Set((await cache.keys()).map((r) => r.url));
    await Promise.all(Object.entries(FILES).map(async ([name, sha]) => {
      const url = new URL(name, self.location).href;
      if (kept.has(url)) return;
      const res = (await fromOlder(url, sha)) ?? await fetch(url, { cache: 'no-cache' });
      if (!res.ok) throw new Error(`${name}: HTTP ${res.status}`);
      const got = await digest(res);
      if (got !== sha) throw new Error(`${name}: SHA-256 ${got}, this build lists ${sha}`);
      await cache.put(url, res);
    }));
    // The host's config.json (not part of the build): kept too, so that the page opens offline from the first visit.
    const cfg = await fetch(new URL('config.json', self.location).href, { cache: 'no-cache' }).catch(() => null);
    if (cfg?.ok) await (await caches.open(CONFIG)).put('config.json', cfg);
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    for (const k of await caches.keys()) if (k.startsWith('ferrata-wallet-') && k !== CACHE && k !== CONFIG) await caches.delete(k);
    await self.clients.claim();
  })());
});

self.addEventListener('message', (event) => {
  if (event.data?.type === 'SKIP_WAITING') self.skipWaiting();
  if (event.data?.type === 'BUILD') event.ports[0]?.postMessage({ build: BUILD, files: Object.keys(FILES).length });
});

async function configFirst(request) {
  const cache = await caches.open(CONFIG);
  try {
    // Always the network's current copy (no HTTP cache either): the kept copy is for offline use only.
    const res = await fetch(request.url, { cache: 'no-store', credentials: 'same-origin' });
    if (res.ok) await cache.put('config.json', res.clone());
    return res;
  } catch (e) {
    const kept = await cache.match('config.json');
    if (kept) return kept;
    throw e;
  }
}

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);
  const scope = new URL(self.registration.scope);
  if (url.origin !== scope.origin || !url.pathname.startsWith(scope.pathname)) return;
  let name = url.pathname.slice(scope.pathname.length);
  if (name === '' || (req.mode === 'navigate' && name === 'index.html')) name = 'index.html';
  if (name === 'config.json') { event.respondWith(configFirst(req)); return; }
  if (!Object.hasOwn(FILES, name)) return;
  event.respondWith(caches.open(CACHE).then((c) => c.match(new URL(name, self.location).href)).then((hit) => hit || fetch(req)));
});

// A notification the page showed (notify.js): open the wallet, or bring it forward, at the place it names.
self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const target = new URL(event.notification.data?.url || './', self.registration.scope).href;
  event.waitUntil((async () => {
    const all = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
    const open = all.find((c) => c.url.startsWith(self.registration.scope));
    if (open) { await open.focus(); open.postMessage({ type: 'OPEN', url: target }); return; }
    await self.clients.openWindow(target);
  })());
});
