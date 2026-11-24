// The Android page's own checks, run in the visitor's browser against this host: the APK's SHA-256 and size against
// apk.json; Digital Asset Links naming the APK's signing certificate; the newest blocks of the chain, each hash
// recomputed from its contents and each link to the block before checked (the wallet's own lez.js); the wallet's
// security headers; every file of the wallet against the SHA-256 its service worker lists.
import { decodeBlock } from '/wallet/lez.js';

const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(typeof k === 'string' ? document.createTextNode(k) : k);
  return e;
};
const hex = (buf) => Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('');
const sha256 = async (buf) => hex(await crypto.subtle.digest('SHA-256', buf));
const mb = (n) => `${(n / 1048576).toFixed(2)} MB (${n.toLocaleString('en-US')} bytes)`;

function check(name) {
  const li = el('li', { class: 'pending', 'data-testid': 'check' }, el('span', { class: 'mark', text: '…' }), el('b', { text: name }), el('span', { class: 'detail', text: ' running…' }));
  $('checks').append(li);
  return (ok, detail) => {
    li.className = ok ? 'ok' : 'bad';
    li.querySelector('.mark').textContent = ok ? '✓' : '✗';
    li.querySelector('.detail').textContent = ' ' + detail;
  };
}
async function rpc(method, params = []) {
  const r = await fetch('/rpc', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
  const j = await r.json();
  if (j.error) throw new Error(`${method}: ${j.error.message}`);
  return j.result;
}

let apk = null;
async function facts() {
  apk = await fetch('apk.json', { cache: 'no-cache' }).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  const dl = $('facts');
  dl.textContent = '';
  if (!apk) { dl.append(el('dt', { text: 'Not published yet' }), el('dd', { text: 'apk.json is missing on this host.' })); return; }
  const add = (k, v, cls) => dl.append(el('dt', { text: k }), el('dd', cls ? { class: cls } : {}, v));
  add('File', apk.file);
  add('Version', `${apk.versionName} (code ${apk.versionCode})`);
  add('Size', mb(apk.size));
  add('SHA-256', apk.sha256, 'mono');
  add('Signing certificate SHA-256', apk.certSha256, 'mono');
  add('Package', apk.package);
  add('Android', `${apk.minSdk} or later (API level); built for ${apk.targetSdk}`);
  add('Permissions', apk.permissions.length ? apk.permissions.map((p) => p.replace('android.permission.', '')).join(', ') : 'none');
  add('Signature schemes', Object.entries(apk.signatureSchemes ?? {}).filter(([, v]) => v === 'true').map(([k]) => k).join(', ') || 'see apksigner');
  add('Two builds equal', apk.reproducible ? `yes: the unsigned APK rebuilt from a second generated project is byte-identical (${apk.unsignedSha256.slice(0, 16)}…)` : 'no');
  add('Built', `${apk.built} from commit ${apk.commit}`);
  $('dl').href = apk.file;
  $('dl').setAttribute('download', apk.file);
  $('dl-size').textContent = mb(apk.size);
  document.querySelectorAll('.apk-name').forEach((s) => { s.textContent = apk.file; });
  $('cmp-size').textContent = `${mb(apk.size)}; nothing runs when closed`;
}

$('check-apk').onclick = async () => {
  const out = $('check-apk-out');
  if (!apk) { out.textContent = 'No apk.json to check against.'; return; }
  out.className = 'msg'; out.textContent = `Downloading ${mb(apk.size)} and hashing it here…`;
  try {
    const buf = await fetch(apk.file, { cache: 'no-store' }).then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.arrayBuffer(); });
    const h = await sha256(buf);
    const ok = h === apk.sha256 && buf.byteLength === apk.size;
    out.className = ok ? 'msg ok' : 'msg bad';
    out.textContent = ok ? `✓ The file this host serves has SHA-256 ${h} and ${buf.byteLength.toLocaleString('en-US')} bytes, as published.`
      : `✗ Got SHA-256 ${h} and ${buf.byteLength} bytes; published: ${apk.sha256}, ${apk.size} bytes.`;
  } catch (e) { out.className = 'msg bad'; out.textContent = `Could not check: ${e.message}`; }
};

async function assetLinks() {
  const done = check('Android\'s link check (Digital Asset Links)');
  try {
    const r = await fetch('/.well-known/assetlinks.json', { cache: 'no-cache' });
    const j = await r.json();
    const t = j.find((s) => s.relation?.includes('delegate_permission/common.handle_all_urls') && s.target?.namespace === 'android_app');
    const ok = !!(apk && t && t.target.package_name === apk.package && t.target.sha256_cert_fingerprints?.includes(apk.certSha256));
    done(ok, ok ? `this site names ${apk.package} signed by ${apk.certSha256.slice(0, 23)}…, so Android opens it full screen.`
      : `assetlinks.json does not name this APK's package and certificate (HTTP ${r.status}).`);
  } catch (e) { done(false, e.message); }
}

async function chain() {
  const done = check('The chain, block by block');
  try {
    const head = Number(await rpc('getLastBlockId'));
    const from = Math.max(1, head - 19);
    const blocks = (await rpc('getBlockRange', [from, head])).map(decodeBlock);
    let hashes = 0, links = 0, linked = 0;
    blocks.forEach((b, i) => {
      if (b.hashOk) hashes++;
      if (i > 0) { links++; if (b.header.prevHash === blocks[i - 1].computedHash) linked++; }
    });
    const last = blocks.at(-1), age = Math.round((Date.now() - last.header.timestamp) / 1000);
    const ok = hashes === blocks.length && linked === links && blocks.length > 0;
    done(ok, `${hashes} of ${blocks.length} block hashes recomputed from their contents and equal, ${linked} of ${links} links to the previous block checked (blocks #${from} to #${last.header.id}; the newest is ${age} s old, ${last.status}).`);
  } catch (e) { done(false, e.message); }
}

async function headers() {
  const done = check('The wallet\'s security headers');
  try {
    const r = await fetch('/wallet/', { method: 'HEAD', cache: 'no-store' });
    const csp = r.headers.get('content-security-policy') || '';
    const need = ["script-src 'self'", "connect-src 'self'", "default-src 'none'", "frame-ancestors 'none'"];
    const missing = need.filter((x) => !csp.includes(x));
    done(!missing.length, missing.length ? `missing: ${missing.join(', ')}` : 'scripts and connections from this site only, nothing else loaded, not embeddable in another page.');
  } catch (e) { done(false, e.message); }
}

$('check-files').onclick = async () => {
  $('check-files').disabled = true;
  const done = check('Every file of the wallet against its service worker\'s list');
  try {
    const sw = await fetch('/wallet/sw.js', { cache: 'no-store' }).then((r) => r.text());
    const build = sw.match(/const BUILD = '([0-9a-f]+)'/)?.[1];
    const files = [...sw.matchAll(/^\s+'([^']+)': '([0-9a-f]{64})',$/gm)].map((m) => [m[1], m[2]]);
    let ok = 0; const bad = [];
    for (const [f, h] of files) {
      const got = await sha256(await fetch(`/wallet/${f}`, { cache: 'no-store' }).then((r) => r.arrayBuffer()));
      if (got === h) ok++; else bad.push(f);
    }
    done(files.length > 0 && !bad.length, `${ok} of ${files.length} files equal the SHA-256 that build ${build} lists${bad.length ? '; differ: ' + bad.join(', ') : ''}.`);
  } catch (e) { done(false, e.message); }
};

async function measured() {
  const t = $('measured');
  const m = await fetch('measure.json', { cache: 'no-cache' }).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  t.textContent = '';
  if (!m) { t.append(el('tr', {}, el('td', { text: 'Not measured yet.' }))); return; }
  for (const row of m.rows) t.append(el('tr', {}, el('th', { text: row[0] }), el('td', { text: row[1] }), el('td', { class: 'fine', text: row[2] ?? '' })));
  $('measured-note').textContent = m.note ?? '';
}

await facts();
await Promise.all([assetLinks(), chain(), headers(), measured()]);
