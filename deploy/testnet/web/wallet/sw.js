// The wallet's service worker: an offline copy of exactly one build of the page. It keeps the files listed below, each
// checked against its SHA-256 before it is kept (a build that does not match, for example one caught half-deployed, is
// refused and the previous one stays), serves them from that copy, and fetches config.json from the network first (the
// copy only when offline). It never sees the wallet's keys and never touches the network calls: the RPC and the faucet
// are POSTs and pass straight through. A new build waits until the page asks for it (the "Reload" banner).
// BEGIN FILES (written by test/pwa-files.mjs; do not edit by hand)
const BUILD = 'a3d64d31f874c07c';
const FILES = {
  'app.js': '05b06cd680ec548b92c838352f6ee3da297c5746e2c044615f4371ac97d71707',
  'applock.js': 'a59bb7c8aa6978c1f9581599dc8c05f7df4598bc69ec62d9a66d406dce47eae4',
  'attest.js': '5fd1ee5d0f85bbf101ea0e4df618280e67d013ec609e50e3fdc92d966941b007',
  'bridge.js': 'e3662f72c7b066a03d87ddcb839ee7bf9cc6f0ef06733f181a5ed878dcc4309f',
  'claim.js': '6f993768720fd33506dc1f8507c78baf0a84a88498e68383a662e8bde17a2870',
  'eth.js': 'd3b8f296199680b6562c6ccb1df6109658802f8fbfe0e34e33716e7275f67cb2',
  'history.js': '18ddfc1394af1a037e1d6f93337a5925b33e01766406aa3b3adfdade95e6f311',
  'icons/apple-touch-icon.png': 'fdcf2fff62a99f4ceb4e501582a2a96c178d29deba682891d507c05dc588f3f0',
  'icons/icon-192.png': '053f64be2da029112675ce3a9d1eb560e018a2bb003c9abda697e91d28cda826',
  'icons/icon-512.png': '32b6be55c02d9e923fdf95104c5ccc609cc67c04c40f202afc18eca18d206958',
  'icons/icon.svg': '786437b05e3b1010a3fb14a2951ef00bc464b40dff40cb48104d3e4137d74b9f',
  'icons/maskable-512.png': '51472db5df63edeeaa3d1145254210a653b4db0779f67d3c3f382da0308f0bbc',
  'icons/maskable.svg': '272260e424f12d7d0272db2b3694194f5c5a644f6f9ac95f47345f5265a1c2b2',
  'icons/mono-512.png': '227054b028b807136b615d63aedaf3d7e1f812a4a597c6fd4b3b1a25c1763ddd',
  'icons/mono-96.png': '7ce71dd0cc5fe0f2811e86cde4de8bcb24d372eb7e80156365d9c603817649cf',
  'icons/mono.svg': 'c51c91efa3c7c7f6251d49cb5df0709ffb43f5bb3fff3d562a49b39fe6afeb95',
  'inbox.js': '5edbd5fe766c6c21f30d8c70429ff5323488adba9be95e6c98b27a4b2881aa87',
  'index.html': '77503c8f0ca2b9bae9ac1e99c5ef09ef6dec6cde74794eddfa611206649f563f',
  'keystore.js': '3f38a0d0384c21e05a0ed5413264d2fbba64bf0520ff4088bc69a04281a673e8',
  'lez.js': 'd278674401ed2aa47ad87d4ba1fa7846324dbfcc12911be0903e1bf407302c1e',
  'lez040.js': '42ee7df3ea722724a3f042b6b77e716b21b4bb58cb467f7cec2efcc82e97923c',
  'manifest.json': 'ae6f89d3122bcbfc787a90ffc90a14c7c8f351d7a6623a74df47d5e0c0a2e07f',
  'msg/client.js': 'db2b6985c0d7a53e3c6f2e104d2d5a6ed78836049a80885604f2a5c936ccf0f6',
  'msg/ferrata-msg.js': 'ff59d6f680f7e791b735a8e38a6cce017eeebe9f85db8626ae6bab60a4ea5805',
  'msg/vendor/crypto.js': '44a587ee8f6c403f8cbbc6ad0a157f8073f2e8860f0e814ffa49cbe7212e1f16',
  'net.js': 'cf2b9a3ebcfb39a41fe812b7347cc1544dde6a65185f5dafd348c230089faa8c',
  'notify.js': '4373171f6b07b9bc6d1b503573dff70ed7d89f6ae971a10cd8cd6734c6e72515',
  'private.js': 'b3da045726f7f676b7df7ca5495f211775e7094c991a565897682cc596cae2f0',
  'prove.js': '6456709309cc7d42b6ce095ac4a068e83a90236d7fac9b9063f67bfa2ef07a2d',
  'pwa.js': '22b8e74f9d222d9e3870086602935b4cdd9a60d9eb423acdea88eb9927198594',
  'qr.js': 'e101b5b57702ce6b16c3e7c89e43bb519700463d9af89b9eed6ccc3fc538dd33',
  'settings.js': '9784e80ac3680f6c61a1263565399503d457b9780103a4d142bbce36e31c597c',
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
