// The wallet's page logic. Keys: made, encrypted and used only here (keystore.js, lez.js). Network: the configured
// same-origin endpoints only (config.json; the page's Content-Security-Policy allows nothing else).
import * as lez from './lez.js';
import * as ks from './keystore.js';
import * as history from './history.js';
import { mountProve } from './prove.js';
import { mountBridge } from './bridge.js';
import { mountPrivate } from './private.js';
import { mountInbox } from './inbox.js';
import { Names, checkName, describeCode } from './vendor/names.js';
import { mountPwa, renderPwa, keepStorage, standalone } from './pwa.js';
import { mountAppLock } from './applock.js';
import { mountScanner, parseScanned, receiveCode, renderQR } from './qr.js';
import { mountNotify } from './notify.js';
import { checkConfig } from './settings.js';

const $ = (id) => document.getElementById(id);
const show = (id, on = true) => { $(id).hidden = !on; };
// The network's settings. If they cannot be read (offline with no kept copy, the load cut short, or a file that parses
// but lacks rpc, token.decimals or token.symbol), the page says so and stops here, rather than failing with an
// unhandled error.
const cfg = await fetch('config.json', { cache: 'no-store' })
  .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
  .then(checkConfig)
  .catch((e) => {
    const p = document.createElement('p');
    p.className = 'msg bad'; p.dataset.testid = 'config-error';
    p.textContent = `This page could not read the network's settings (config.json: ${e.message}). Reload when online.`;
    document.querySelector('main')?.prepend(p);
    return new Promise(() => {}); // stop here: nothing below works without the settings
  });
const seq = new lez.Sequencer(cfg.rpc);
// Names (the names program, ferrata_names): on when the network publishes the program.
let names = cfg.names?.program ? new Names({ rpc: seq, program: cfg.names.program }) : null;
const NAMES_GAS = 200000n; // the names CLI's limit (names/README.md: a refused call costs at most this)
const DEC = BigInt(cfg.token.decimals), UNIT = 10n ** DEC, SYMBOL = cfg.token.symbol;
$('net-name').textContent = cfg.network;
// The shared-origin notice stays unless this page runs on the origin the host gives the wallet alone (config.json
// ownOrigin, e.g. "https://wallet.<host>"); the same files served on the shared origin keep the notice.
document.querySelectorAll('.shared-origin').forEach((e) => { e.hidden = typeof cfg.ownOrigin === 'string' && location.origin === cfg.ownOrigin; });
$('send-unit').textContent = SYMBOL;
$('recv-unit').textContent = SYMBOL;

// ------------------------------------------------------------------ amounts
export function fmt(v) {
  const n = lez.big(v), whole = n / UNIT, frac = (n % UNIT).toString().padStart(Number(DEC), '0').replace(/0+$/, '');
  return `${whole.toLocaleString('en-US')}${frac ? '.' + frac : ''} ${SYMBOL}`;
}
const parseAmount = (s) => lez.parseUnits(s, DEC); // a comma is refused: "1,5" and "1,000" are not guessed

// ------------------------------------------------------------------ session
const S = { key: null, salt: null, secret: null, seed: null, accounts: [], current: 0, idleTimer: null };
const acct = () => S.accounts[S.current];

function materialize() {
  S.seed = S.secret.mnemonic ? lez.seedOf(S.secret.mnemonic) : null;
  S.accounts = S.secret.accounts.map((a) => {
    const node = a.kind === 'seed' ? lez.deriveNode(S.seed, lez.parsePath(a.path)) : lez.rawKeyNode(a.sk);
    return { ...a, node, id: lez.idToString(node.accountId) };
  });
  S.current = Math.min(S.secret.current ?? 0, S.accounts.length - 1);
}
async function persist() { S.secret.current = S.current; await ks.seal(S.key, S.salt, S.secret); }

function lock() {
  notifier?.stop();
  scanner?.stop();
  S.key = S.salt = S.secret = S.seed = null; S.accounts = [];
  clearTimeout(S.idleTimer);
  // Drop every reference to keys or plans the page holds (JavaScript cannot erase memory; the collector frees them).
  clearSendReview();
  mountPanels();
  for (const id of ['reveal-out']) { $(id).textContent = ''; $(id).hidden = true; }
  route();
  appLock.locked();
}
function touch() {
  clearTimeout(S.idleTimer);
  if (S.key) S.idleTimer = setTimeout(lock, Number($('autolock').value) * 60000);
}
['click', 'keydown', 'touchstart'].forEach((e) => document.addEventListener(e, touch, { passive: true }));

// ------------------------------------------------------------------ views
const VIEWS = ['v-welcome', 'v-create', 'v-import', 'v-unlock', 'v-main'];
function view(id) { VIEWS.forEach((v) => show(v, v === id)); show('lock', id === 'v-main'); }
function route() {
  if (!S.key) { view(ks.exists() ? 'v-unlock' : 'v-welcome'); if (ks.exists()) appLock.renderUnlock(); return; }
  view('v-main');
  const tab = (location.hash || '#home').slice(1);
  document.querySelectorAll('#tabs a').forEach((a) => a.classList.toggle('on', a.dataset.tab === tab));
  document.querySelectorAll('.tab').forEach((t) => { t.hidden = t.id !== 't-' + tab; });
  renderAccount();
  if (tab === 'activity') history.render(ctx());
  if (tab === 'inbox') { inboxPanel.render(); notifier.markSeen(); }
  if (tab === 'settings') { renderNetInfo(); appLock.renderSettings(); notifier.renderSettings(); renderPwa(); }
}
window.addEventListener('hashchange', route);
$('lock').onclick = lock;

// ------------------------------------------------------------------ onboarding
let pendingMnemonic = null;
$('go-create').onclick = () => {
  pendingMnemonic = lez.newMnemonic();
  const ol = $('mnemonic-words'); ol.textContent = '';
  for (const w of pendingMnemonic.split(' ')) { const li = document.createElement('li'); li.textContent = w; ol.append(li); }
  view('v-create');
};
$('go-import').onclick = () => view('v-import');
$('create-back').onclick = $('import-back').onclick = () => { pendingMnemonic = null; view('v-welcome'); };
async function startWith(secret, pass, errEl) {
  try {
    const { key, salt } = await ks.create(pass, secret);
    appLock.reset();
    Object.assign(S, { key, salt, secret });
    materialize(); location.hash = '#home'; route(); refresh(); notifier.start();
    keepStorage();
  } catch (e) { $(errEl).textContent = e.message; }
}
$('create-go').onclick = async () => {
  $('create-err').textContent = '';
  if (!$('saved-words').checked) { $('create-err').textContent = 'Write the 24 words down first, then tick the box.'; return; }
  if ($('create-pass').value !== $('create-pass2').value) { $('create-err').textContent = 'The two passphrases differ.'; return; }
  await startWith({ v: 1, mnemonic: pendingMnemonic, accounts: [{ kind: 'seed', path: '/0', label: 'Account 1' }] }, $('create-pass').value, 'create-err');
  pendingMnemonic = null; $('mnemonic-words').textContent = '';
};
document.querySelectorAll('[data-import]').forEach((b) => b.onclick = () => {
  document.querySelectorAll('[data-import]').forEach((x) => x.classList.toggle('on', x === b));
  show('import-phrase-box', b.dataset.import === 'phrase'); show('import-key-box', b.dataset.import === 'key');
});
$('import-go').onclick = async () => {
  $('import-err').textContent = '';
  try {
    let secret;
    if (!$('import-phrase-box').hidden) {
      const m = $('import-phrase').value.trim().toLowerCase().split(/\s+/).join(' ');
      if (!lez.isMnemonic(m)) throw new Error('These words are not a valid BIP-39 phrase.');
      secret = { v: 1, mnemonic: m, accounts: [{ kind: 'seed', path: '/0', label: 'Account 1' }] };
    } else {
      const sk = $('import-key').value.trim().replace(/^0x/, '');
      lez.rawKeyNode(sk);
      secret = { v: 1, mnemonic: null, accounts: [{ kind: 'raw', sk, label: 'Imported key' }] };
    }
    await startWith(secret, $('import-pass').value, 'import-err');
    $('import-phrase').value = $('import-key').value = '';
  } catch (e) { $('import-err').textContent = e.message; }
};
function opened(o) {
  Object.assign(S, o);
  materialize(); if (!location.hash) location.hash = '#home'; route(); refresh(); notifier.start();
  touch();
  if (standalone()) keepStorage();
}
$('unlock-go').onclick = async () => {
  $('unlock-err').textContent = '';
  $('unlock-go').disabled = true;
  try {
    const o = await ks.open($('unlock-pass').value);
    $('unlock-pass').value = '';
    opened(o);
  } catch (e) { $('unlock-err').textContent = e.message; } finally { $('unlock-go').disabled = false; }
};
$('unlock-pass').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('unlock-go').click(); });
$('erase-wallet').onclick = () => {
  if (confirm('Erase this wallet from this browser? Only your recovery phrase can bring it back.')) { ks.erase(); appLock.reset(); location.hash = ''; route(); }
};

// ------------------------------------------------------------------ accounts
function renderAccount() {
  const sel = $('account-select');
  sel.textContent = '';
  S.accounts.forEach((a, i) => {
    const o = document.createElement('option');
    o.value = i; o.textContent = `${a.label} · ${a.id.slice(0, 6)}…${a.id.slice(-4)}`; o.selected = i === S.current;
    sel.append(o);
  });
  $('add-account').hidden = !S.seed;
  const a = acct(); if (!a) return;
  $('home-id').textContent = a.id;
  renderName(a);
  const pkHex = lez.hex(a.node.pk);
  $('recv-id').textContent = a.id; $('recv-pk').textContent = pkHex;
  $('recv-code').textContent = `lez:${a.id}?pk=${pkHex}`;
  renderReceiveQR();
  $('recv-cli').textContent = `wallet auth-transfer send --to Public/${a.id} … (or --to-pk ${pkHex} for a first transfer)`;
}
$('account-select').onchange = async (e) => { clearSendReview(); S.current = Number(e.target.value); await persist(); route(); refresh(); };
$('add-account').onclick = async () => {
  const used = S.secret.accounts.filter((a) => a.kind === 'seed').map((a) => lez.parsePath(a.path)[0]);
  const next = used.length ? Math.max(...used) + 1 : 0;
  S.secret.accounts.push({ kind: 'seed', path: `/${next}`, label: `Account ${S.secret.accounts.length + 1}` });
  S.secret.current = S.secret.accounts.length - 1;
  materialize(); await persist(); route(); refresh();
};
document.addEventListener('click', (e) => {
  const c = e.target.closest('.copy');
  if (c && navigator.clipboard) navigator.clipboard.writeText(c.textContent).catch(() => {});
});

// ------------------------------------------------------------------ names
function renderName(a) {
  const box = $('names-box');
  box.hidden = !names;
  if (!names) return;
  $('home-name').textContent = '…';
  names.reverse(a.id).then((n) => { if (acct() === a) $('home-name').textContent = n ? `${n}.lez` : 'none yet'; })
    .catch(() => { $('home-name').textContent = 'unreadable'; });
}
// Claiming is two transactions (the names program's commit-reveal): Commit hides the name behind sha256(owner, salt, name);
// Register reveals it a few minutes later. The salt waits in the encrypted wallet record between the two.
async function namesTx(a, call, label) {
  const out = $('name-msg');
  const [info, fs] = await Promise.all([account(a.node.accountId), seq.feeState()]);
  a.chain = info; feeState = fs;
  const args = { payer: a.node, to: call.to, actors: call.actors, data: call.data, evidence: call.evidence ?? [], nonce: info.nonce, gasLimit: NAMES_GAS };
  const { maxFee } = lez.feeFor({ gasLimit: NAMES_GAS, size: lez.buildCall({ ...args, maxFee: 0n }).size, feeState: fs });
  if (info.balance < maxFee) throw new Error(`${label} needs up to ${fmt(maxFee)} in fees; use the faucet first.`);
  const tx = lez.buildCall({ ...args, maxFee });
  const hash = await seq.send(tx.wire);
  out.textContent = `${label}: sent ${hash}; waiting for a block…`;
  const res = await waitApplied(a, hash, info.nonce);
  if (res.cls !== 'ok') throw new Error(`${label} refused by the names program: ${res.text}`);
  return res;
}
function pendingCommit(a) { return (S.secret.nameCommits ?? []).find((x) => x.account === a.id) ?? null; }
function renderCommit(a) {
  const p = pendingCommit(a), btn = $('name-claim');
  if (!p) { btn.textContent = 'Claim'; return; }
  // The reveal's nowMs is the page's clock less 30 s (inside the block's clock slack), and must reach revealFrom.
  const left = p.revealFrom + 31000 - Date.now();
  btn.textContent = left > 0 ? `Reveal ${p.name} in ${Math.ceil(left / 1000)} s` : `Reveal ${p.name}`;
  $('name-input').value = p.name;
}
setInterval(() => { if (S.key && acct()) renderCommit(acct()); }, 1000);
$('name-claim').onclick = async () => {
  const out = $('name-msg'); out.className = 'msg'; out.textContent = '';
  try {
    const a = acct();
    const p = pendingCommit(a);
    if (!p) {
      const name = $('name-input').value.trim().toLowerCase().replace(/\.lez$/, '');
      const bad = checkName(name); if (bad) throw new Error(bad.reason ?? describeCode(bad.code));
      const c = await names.commitCall(a.id, name);
      (S.secret.nameCommits ??= []).push({ account: a.id, name: c.name, salt: lez.hex(c.salt), nowMs: c.nowMs, revealFrom: c.revealFrom, revealUntil: c.revealUntil });
      await persist();
      try { await namesTx(a, c, 'Commit'); } catch (e) {
        S.secret.nameCommits = S.secret.nameCommits.filter((x) => x.salt !== lez.hex(c.salt)); await persist(); throw e;
      }
      out.textContent = `Committed to ${c.name} (hidden until the reveal). The reveal opens at ${new Date(c.revealFrom + 30000).toLocaleTimeString()}; this page keeps the secret salt.`;
      out.className = 'msg ok';
    } else {
      const nowMs = Date.now() - 30000;
      if (nowMs < p.revealFrom) throw new Error(`The reveal opens at ${new Date(p.revealFrom + 30000).toLocaleTimeString()}.`);
      if (Date.now() >= p.revealUntil) { S.secret.nameCommits = S.secret.nameCommits.filter((x) => x !== p); await persist(); throw new Error('The reveal window has passed: commit again.'); }
      const call = await names.call(a.id, { kind: 'Register', name: p.name, salt: lez.unhex(p.salt), nowMs });
      const res = await namesTx(a, call, 'Reveal');
      S.secret.nameCommits = S.secret.nameCommits.filter((x) => x !== p); await persist();
      // Registering does not make a name the account's displayed (primary) name: SetPrimary does, once.
      const u = await names.user(a.id).catch(() => null);
      if (!u?.primary) await namesTx(a, await names.call(a.id, { kind: 'SetPrimary', name: p.name }), 'Set as this account\'s name');
      out.textContent = `${p.name}.lez is yours and shown as this account's name (${res.text})`; out.className = 'msg ok';
    }
    renderName(a); renderCommit(a);
  } catch (e) { out.textContent = e.message; out.className = 'msg bad'; }
};

// ------------------------------------------------------------------ chain reads
let feeState = null, lastHead = null;
async function account(id) {
  const [bal, nonces, acc] = await Promise.all([seq.balance(id), seq.nonces([id]), seq.account(id)]);
  return { balance: lez.big(bal), nonce: lez.big(nonces[0]), exists: acc !== null && acc !== undefined };
}
// One refresh at a time per account: a refresh started for another account (before a switch) must not stand in for
// this one (on a network with real latency that left the sender's state unset; found on the public test network).
let refreshing = null;
async function refresh() {
  if (!S.key) return undefined;
  const a = acct();
  if (refreshing && refreshing.id === a.id) return refreshing.p;
  const p = (async () => {
    try {
      const [info, head, fs] = await Promise.all([account(a.node.accountId), seq.lastBlockId(), seq.feeState()]);
      feeState = fs; lastHead = head; a.chain = info;
      $('balance').textContent = fmt(info.balance);
      $('balance-atomic').textContent = `${info.balance.toLocaleString('en-US')} atomic units`;
      $('home-nonce').textContent = info.nonce.toString();
      $('send-avail').textContent = `Available: ${fmt(info.balance)}`;
      $('net-status').textContent = `block ${head}`; $('net-status').className = 'pill ok';
    } catch (e) {
      $('net-status').textContent = 'RPC unreachable'; $('net-status').className = 'pill bad';
      console.warn(e);
    }
  })();
  refreshing = { id: a.id, p };
  try { return await p; } finally { if (refreshing && refreshing.p === p) refreshing = null; }
}
// The follower (notify.js) refreshes as soon as a new block touches these accounts; this catches anything else.
setInterval(() => { if (S.key && document.visibilityState === 'visible') refresh(); }, Number(cfg.pollMs ?? 30000));
async function renderNetInfo() {
  const dl = $('net-info'); dl.textContent = '';
  const add = (k, v) => { const dt = document.createElement('dt'); dt.textContent = k; const dd = document.createElement('dd'); dd.textContent = v; dl.append(dt, dd); };
  add('RPC', new URL(cfg.rpc, location.href).href);
  try {
    const [head, fs, ids, ch] = await Promise.all([seq.lastBlockId(), seq.feeState(), seq.programIds(), seq.channelId().catch(() => null)]);
    add('Head block', String(head));
    add('Base fees', `execution ${fs.base_fee_exec}, storage ${fs.base_fee_stor} per unit`);
    if (ch) add('Channel (zone id)', typeof ch === 'string' ? ch : JSON.stringify(ch));
    const ppc = ids && (ids.privacy_preserving_circuit || ids.PRIVACY_PRESERVING_CIRCUIT);
    if (ppc) add('Privacy circuit', lez.programIdHex(ppc));
    add('LEZ version', ppc && lez.programIdHex(ppc).startsWith('631f0c96') ? 'v0.4.0-rc2/rc3 (privacy circuit 631f0c96…)' : 'see the privacy circuit id');
  } catch (e) { add('Error', e.message); }
}

// ------------------------------------------------------------------ faucet
$('faucet').onclick = async () => {
  const a = acct(), msg = $('faucet-msg');
  if (!cfg.faucet) { msg.textContent = 'This network has no faucet configured.'; msg.className = 'msg warn'; return; }
  $('faucet').disabled = true; msg.className = 'msg'; msg.textContent = 'Asking the faucet…';
  try {
    const before = a.chain?.balance ?? 0n;
    // The test network's gateway: {account} (+ public_key, which LEZ needs to credit an account the first time).
    const r = await fetch(cfg.faucet, { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ account: a.id, account_id: a.id, public_key: lez.hex(a.node.pk) }) });
    const j = await r.json().catch(() => ({}));
    if (!r.ok || j.ok === false) throw new Error(j.error ? `Faucet: ${j.error}${j.detail ? ' (' + j.detail + ')' : ''}` : `faucet answered HTTP ${r.status}`);
    const h = j.hash || j.tx_hash;
    msg.textContent = `Sent${j.amount ? ' ' + fmt(j.amount) : ''}${h ? ' in ' + h.slice(0, 12) + '…' : ''}; waiting for the block…`;
    for (let i = 0; i < 60; i++) {
      await new Promise((res) => setTimeout(res, 2000));
      await refresh();
      if ((a.chain?.balance ?? 0n) > before) { msg.textContent = `Received: your balance is ${fmt(a.chain.balance)}.`; msg.className = 'msg ok'; return; }
    }
    msg.textContent = 'The faucet sent the coins, but they have not arrived after two minutes. Check Activity later.'; msg.className = 'msg warn';
  } catch (e) { msg.textContent = e.message; msg.className = 'msg bad'; } finally { $('faucet').disabled = false; }
};

// ------------------------------------------------------------------ send
async function resolveRecipient(s) {
  const t = s.trim();
  if (names && /^[a-zA-Z0-9-]{3,32}(\.lez)?$/.test(t) && !/^[1-9A-HJ-NP-Za-km-z]{43,44}$/.test(t)) {
    const r = await names.resolve(t);
    if (!r) throw new Error(`No live name "${t}" on this network.`);
    return { id: lez.idFromString(r.target), pk: null, name: r.name };
  }
  return parseRecipient(t);
}
function parseRecipient(s) {
  const t = s.trim();
  const code = t.match(/^lez:([1-9A-HJ-NP-Za-km-z]{32,44})\?pk=([0-9a-fA-F]{64})$/);
  if (code) {
    const id = lez.idFromString(code[1]), pk = lez.unhex(code[2]);
    if (!lez.equalBytes(lez.accountIdOf(pk), id)) throw new Error('This receive code is damaged: its key does not match its account id.');
    return { id, pk };
  }
  if (/^(0x)?[0-9a-fA-F]{64}$/.test(t)) { const pk = lez.unhex(t); return { id: lez.accountIdOf(pk), pk }; }
  try { return { id: lez.idFromString(t), pk: null }; } catch (e) {
    if (/private/i.test(e.message)) throw e;
    throw new Error('That is not an account id, a public key (64 hex digits) or a receive code (lez:…?pk=…).');
  }
}
let pending = null;
// A review stands only for the form it was made from: any change to it (or to the account) drops the review.
function clearSendReview() { pending = null; show('send-confirm', false); $('send-summary').textContent = ''; }
for (const id of ['send-to', 'send-amount']) $(id).addEventListener('input', () => { if (pending) clearSendReview(); });
$('send-review').onclick = async () => {
  $('send-err').textContent = ''; $('send-result').textContent = ''; show('send-confirm', false);
  try {
    const a = acct();
    const to = await resolveRecipient($('send-to').value);
    const amount = parseAmount($('send-amount').value);
    if (lez.equalBytes(to.id, a.node.accountId)) throw new Error('That is this account.');
    const [info, fs, toAcc] = await Promise.all([account(a.node.accountId), seq.feeState(), seq.account(to.id)]);
    a.chain = info; feeState = fs;
    const toExists = toAcc !== null && toAcc !== undefined;
    if (!toExists && !to.pk) throw new Error('This account has never been used on LEZ, so the network needs its public key once: ask the recipient for their receive code (Receive tab), or paste their public key.');
    const gasLimit = BigInt(cfg.gasLimitTransfer);
    const probe = lez.buildTransfer({ from: a.node, to: to.id, toKey: toExists ? null : to.pk, amount, nonce: a.chain.nonce, gasLimit, maxFee: 0n });
    if (!feeState) feeState = await seq.feeState();
    const { reserve, maxFee } = lez.feeFor({ gasLimit, size: probe.size, feeState });
    if (a.chain.balance < amount + maxFee) throw new Error(`Not enough: ${fmt(amount)} plus at most ${fmt(maxFee)} in fees is more than your ${fmt(a.chain.balance)}.`);
    pending = { to, toKey: toExists ? null : to.pk, amount, gasLimit, maxFee, reserve, nonce: a.chain.nonce, from: a };
    const dl = $('send-summary'); dl.textContent = '';
    const add = (k, v) => { const dt = document.createElement('dt'); dt.textContent = k; const dd = document.createElement('dd'); dd.textContent = v; dl.append(dt, dd); };
    add('From', `${a.label} (${a.id})`); add('To', `${to.name ? to.name + ' → ' : ''}${lez.idToString(to.id)}`); add('Amount', fmt(amount));
    add('Fee now', `about ${fmt(reserve)} (gas limit ${gasLimit}, ${probe.size} bytes at base fees ${feeState.base_fee_exec}/${feeState.base_fee_stor})`);
    add('Fee at most', `${fmt(maxFee)} (unused gas is refunded)`);
    add('Nonce', pending.nonce.toString());
    if (pending.toKey) add('First credit', 'the recipient\'s public key goes with it (LEZ\'s admission evidence)');
    show('send-confirm');
  } catch (e) { $('send-err').textContent = e.message; }
};
$('send-cancel').onclick = clearSendReview;
$('send-max').onclick = async () => {
  clearSendReview();
  await refresh();
  const a = acct(); if (!a.chain || !feeState) return;
  const { maxFee } = lez.feeFor({ gasLimit: BigInt(cfg.gasLimitTransfer), size: 300, feeState });
  const v = a.chain.balance > maxFee ? a.chain.balance - maxFee : 0n;
  $('send-amount').value = (v / UNIT).toString() + (v % UNIT ? '.' + (v % UNIT).toString().padStart(Number(DEC), '0').replace(/0+$/, '') : '');
};
$('send-go').onclick = async () => {
  if (!pending) return;
  const p = pending; pending = null;
  show('send-confirm', false);
  const out = $('send-result'); out.className = 'msg'; out.textContent = 'Signing and sending…';
  try {
    const tx = lez.buildTransfer({ from: p.from.node, to: p.to.id, toKey: p.toKey, amount: p.amount, nonce: p.nonce, gasLimit: p.gasLimit, maxFee: p.maxFee });
    history.remember(ctx(), p.from.id, { hash: tx.hash, wire: tx.wire, nonce: p.nonce.toString(), to: lez.idToString(p.to.id), amount: p.amount.toString(), sent: Date.now() });
    const h = await seq.send(tx.wire);
    const hash = typeof h === 'string' ? h.replace(/^0x/, '') : tx.hash;
    if (hash !== tx.hash) console.warn(`the sequencer named the transaction ${hash}; this page computed ${tx.hash}`);
    out.textContent = `Sent: ${hash}. Waiting for a block…`;
    const res = await waitApplied(p.from, hash, p.nonce);
    out.textContent = `${res.text} Transaction ${hash}.`; out.className = 'msg ' + res.cls;
    $('send-amount').value = '';
  } catch (e) { out.textContent = e.message; out.className = 'msg bad'; }
};
// LEZ rc3's RPC has no receipts, but its blocks list the charged transactions whose action reverted: the page reads
// the block that holds the transaction and reports applied or reverted from it.
async function waitApplied(a, hash, nonce) {
  for (let i = 0; i < 90; i++) {
    await new Promise((r) => setTimeout(r, 2000));
    const t = await seq.transaction(hash).catch(() => null);
    if (!t) continue;
    const blockId = Number(Array.isArray(t) ? t[1] : t.block_id);
    const raw = await seq.block(blockId);
    const blk = lez.decodeBlock(raw);
    const o = lez.outcomeIn(blk, hash);
    await refresh();
    if (o === 'reverted') {
      return { text: `In block ${blockId}, but it reverted: the fee was charged and nothing moved (nonce now ${a.chain?.nonce}).`, cls: 'bad' };
    }
    if (o === 'missing') {
      return { text: `The RPC places it in block ${blockId}, but that block does not list it: outcome unknown (nonce now ${a.chain?.nonce}). Check Activity before sending again.`, cls: 'warn' };
    }
    return { text: `In block ${blockId}, applied; the nonce moved to ${a.chain?.nonce}. Balance: ${fmt(a.chain?.balance ?? 0n)}.`, cls: 'ok' };
  }
  return { text: 'Not in a block after three minutes. It may still be in the pool; Activity will show it when it lands.', cls: 'warn' };
}

// ------------------------------------------------------------------ backup
async function checkPass() {
  const p = $('reveal-pass').value; $('reveal-pass').value = '';
  await ks.open(p); // throws on a wrong passphrase
}
$('reveal-phrase').onclick = async () => {
  const out = $('reveal-out');
  try {
    await checkPass();
    out.textContent = S.secret.mnemonic || 'This wallet was imported from a private key: it has no recovery phrase.';
  } catch (e) { out.textContent = e.message; }
  out.hidden = false; setTimeout(() => { out.textContent = ''; out.hidden = true; }, 60000);
};
$('reveal-key').onclick = async () => {
  const out = $('reveal-out');
  try {
    await checkPass();
    const a = acct();
    out.textContent = a.kind === 'raw' ? a.sk : `${lez.hex(a.node.ssk)}\n(the signing key LEZ's wallet derives at ${a.path}; it signs this account's transactions)`;
  } catch (e) { out.textContent = e.message; }
  out.hidden = false; setTimeout(() => { out.textContent = ''; out.hidden = true; }, 60000);
};

// ------------------------------------------------------------------ the Ferrata panels
function ctx() { return { cfg, seq, lez, S, acct, fmt, parseAmount, refresh, feeState: () => feeState, persist, names }; }
let inboxPanel = null;
/** (Re)builds the panels that keep state of their own (plans, reviews): at start, on lock, when config.json changes. */
function mountPanels() {
  mountProve($('prove-body'), ctx());
  mountBridge($('bridge-body'), ctx());
  inboxPanel = mountInbox($('inbox-body'), ctx());
}
mountPanels();
mountPrivate($('private-body'), ctx());

// The network's settings can change while the page (or the installed app) stays open: config.json is read again from
// the network (never a cached copy) every minute and whenever the page comes back to the front, and the panels that
// depend on it are rebuilt in place when it changed. No hard reload needed.
let cfgText = JSON.stringify(cfg);
async function recheckConfig() {
  try {
    const fresh = await fetch('config.json', { cache: 'no-store' }).then((r) => (r.ok ? r.json() : null));
    if (!fresh || JSON.stringify(fresh) === cfgText) return;
    checkConfig(fresh); // a changed file that lacks a field the page reads is not taken
    cfgText = JSON.stringify(fresh);
    for (const k of Object.keys(cfg)) delete cfg[k];
    Object.assign(cfg, fresh);
    names = cfg.names?.program ? new Names({ rpc: seq, program: cfg.names.program }) : null;
    $('net-name').textContent = cfg.network;
    mountPanels();
    if (S.key) route();
  } catch { /* offline, or incomplete settings: keep the settings in use */ }
}
setInterval(recheckConfig, 60000);
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') recheckConfig(); });

// ------------------------------------------------------------------ the app: install, lock, QR, notifications
const notifier = mountNotify(ctx(), { onInboxChanged: () => { if (location.hash === '#inbox') { inboxPanel.render(); notifier.markSeen(); } } });
const appLock = mountAppLock({ onOpened: opened, lock, unlocked: () => !!S.key });
mountPwa({ onOpen: (hash) => { if (hash) location.hash = hash; } });
function renderReceiveQR() {
  const a = acct(); if (!a) return;
  const raw = $('recv-amount').value.trim();
  let amount = '';
  $('recv-err').textContent = '';
  if (raw) {
    try { const v = parseAmount(raw); amount = fmt(v).split(' ')[0].replace(/,/g, ''); } catch (e) { $('recv-err').textContent = e.message; }
  }
  const code = receiveCode(a.id, lez.hex(a.node.pk), amount);
  renderQR($('recv-qr'), code, amount ? `Receive code asking for ${amount} ${SYMBOL}` : 'Receive code').catch((e) => { $('recv-err').textContent = e.message; });
}
$('recv-amount').oninput = renderReceiveQR;
$('recv-copy').onclick = () => navigator.clipboard?.writeText($('recv-qr').dataset.text || $('recv-code').textContent).catch(() => {});
$('recv-share').hidden = !navigator.share;
$('recv-share').onclick = () => navigator.share?.({ title: 'My LEZ receive code', text: $('recv-qr').dataset.text || $('recv-code').textContent }).catch(() => {});
const scanSay = (t, cls = '') => { $('scan-msg').textContent = t; $('scan-msg').className = `msg ${cls}`; };
const scanner = mountScanner({ button: $('send-scan'), file: $('send-scan-file'), say: scanSay, onText: (text) => {
  const r = parseScanned(text, Number(DEC));
  if (!r) { scanSay('That QR code is not a LEZ receive code, account id, public key or name.', 'bad'); return; }
  $('send-to').value = r.to;
  if (r.amount) $('send-amount').value = r.amount;
  clearSendReview();
  scanSay(r.amount ? `Read: ${r.to.slice(0, 18)}… asking for ${r.amount} ${SYMBOL}. Check it, then Review.` : `Read: ${r.to.slice(0, 18)}…. Check it, then Review.`, 'ok');
} });

route();
seq.lastBlockId().then((h) => { $('net-status').textContent = `block ${h}`; $('net-status').className = 'pill ok'; })
  .catch(() => { $('net-status').textContent = 'RPC unreachable'; $('net-status').className = 'pill bad'; });
