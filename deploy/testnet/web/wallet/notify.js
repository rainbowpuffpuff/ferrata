// New messages and incoming transfers without a push service. While the wallet is open and unlocked, the page follows
// the chain itself: it asks for the newest block id every 15 s while on screen (every 60 s off screen, as far as the
// system lets a page run), reads each new block once, recomputes its hash and checks its link to the block before, and
// picks out what concerns this wallet's accounts: a transfer to one of them, or a message to one of them through the
// messages program. Then: a badge on Inbox, a line on screen, and, if allowed, a notification from this page. No push
// service (Google's, Apple's) is told who you are or when something arrived; when the app is closed nothing runs, and
// unread messages are counted again at the next unlock.
import * as C from './msg/client.js';
import { prefs, setPrefs } from './applock.js';

const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...kids) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) { if (k === 'text') e.textContent = v; else e.setAttribute(k, v); }
  for (const k of kids) e.append(typeof k === 'string' ? document.createTextNode(k) : k);
  return e;
};
const CHUNK = 50, MAX_GAP = 200;

export function mountNotify(c, { onInboxChanged }) {
  const ON_MS = Number(c.cfg.followMs ?? 15000), OFF_MS = Number(c.cfg.followHiddenMs ?? 60000);
  const msgProgram = c.cfg.msg?.program ? c.lez.idFromString(c.cfg.msg.program) : null;
  const stats = { blocks: 0, hashOk: 0, links: 0, linksOk: 0, bad: [], from: null, to: null, error: null, last: 0 };
  let follow = null, timer = 0, running = false, unread = new Map();

  const mine = () => c.S.accounts.map((a) => ({ a, id: a.node.accountId }));
  const owner = (id) => mine().find((x) => c.lez.equalBytes(x.id, id))?.a ?? null;

  function toast(text, cls = 'ok') {
    const t = $('toast');
    t.textContent = text; t.className = `toast ${cls}`; t.hidden = false;
    clearTimeout(t._h); t._h = setTimeout(() => { t.hidden = true; }, 6000);
  }
  async function notifySystem(title, body, url) {
    if (!prefs().notify || globalThis.Notification?.permission !== 'granted') return false;
    const reg = await navigator.serviceWorker?.getRegistration?.().catch(() => null);
    if (!reg?.showNotification) return false;
    await reg.showNotification(title, { body, tag: url, icon: 'icons/icon-192.png', badge: 'icons/mono-96.png', data: { url } }).catch(() => {});
    return true;
  }
  function badge() {
    const a = c.acct(), n = a ? unread.get(a.id) ?? 0 : 0;
    const b = $('inbox-badge');
    b.textContent = n > 99 ? '99+' : String(n); b.hidden = n === 0;
  }
  // Per account, the mailbox's next sequence number when the user last had the inbox on screen (messages are numbered
  // from 0): a message at or above it is unread.
  const nextOf = (id) => c.S.secret?.inboxNext?.[id];
  async function countUnread(a) {
    if (!msgProgram) return 0;
    const box = await C.mailbox(c.seq, a.node.accountId, msgProgram);
    const next = Number(box?.nextSeq ?? 0);
    const seen = nextOf(a.id);
    if (seen === undefined) { (c.S.secret.inboxNext ??= {})[a.id] = next; await c.persist(); unread.set(a.id, 0); return 0; }
    const n = (box?.inbox ?? []).filter((m) => Number(m.seq) >= seen).length;
    unread.set(a.id, n);
    return n;
  }
  /** The user has the inbox on screen: what is there now counts as read. */
  async function markSeen() {
    const a = c.acct();
    if (!a || !msgProgram || !c.S.key) return;
    try {
      const box = await C.mailbox(c.seq, a.node.accountId, msgProgram);
      (c.S.secret.inboxNext ??= {})[a.id] = Number(box?.nextSeq ?? 0);
      unread.set(a.id, 0); badge(); await c.persist();
    } catch { /* next time */ }
  }

  async function inspect(b) {
    const got = { transfers: [], messages: new Set() };
    b.transactions.forEach((d, i) => {
      if (d.kind !== 'public' || b.reverted.includes(i)) return;
      const t = c.lez.nativeTransferOf(d.tx);
      if (t) {
        const to = owner(t.to);
        if (to && !owner(t.from)) got.transfers.push({ a: to, amount: t.amount, from: c.lez.idToString(t.from) });
        return;
      }
      if (!msgProgram || !c.lez.equalBytes(d.tx.to.program, msgProgram)) return;
      const signer = (d.tx.signers ?? d.tx.authorizedAccounts ?? []).find((s) => owner(s));
      for (const entry of d.tx.actors) {
        const act = entry.actor ?? entry; // v0.4.0 adds authority around each actor
        const a = owner(act.account);
        if (a && c.lez.equalBytes(act.program, msgProgram) && !signer) got.messages.add(a);
      }
    });
    for (const x of got.transfers) {
      const text = `Received ${c.fmt(x.amount)} on ${x.a.label}`;
      toast(text);
      await notifySystem('Ferrata wallet', text, './#activity');
    }
    if (got.transfers.length) c.refresh();
    for (const a of got.messages) {
      const n = await countUnread(a).catch(() => 0);
      badge();
      const text = `New encrypted message on ${a.label}`;
      toast(n > 1 ? `${text} (${n} unread)` : text);
      await notifySystem('Ferrata wallet', text, './#inbox');
      onInboxChanged?.(a);
    }
  }

  async function poll() {
    const head = Number(await c.seq.lastBlockId());
    stats.last = Date.now();
    if (!follow) { follow = { id: head, hash: null }; return; }
    if (head <= follow.id) return;
    let from = follow.id + 1;
    if (head - from + 1 > MAX_GAP) { from = head - MAX_GAP + 1; follow.hash = null; }
    for (let s = from; s <= head; s += CHUNK) {
      const e = Math.min(head, s + CHUNK - 1);
      for (const raw of await c.seq.blockRange(s, e)) {
        const b = (c.net ? c.net.codec : c.lez).decodeBlock(raw);
        const id = Number(b.header.id); // v0.4.0 keeps RPC/Borsh u64 values lossless as bigint
        stats.blocks++;
        if (b.hashOk) stats.hashOk++; else stats.bad.push(`#${id}: hash`);
        if (follow.hash && id === follow.id + 1) {
          stats.links++;
          if (b.header.prevHash === follow.hash) stats.linksOk++; else stats.bad.push(`#${id}: link`);
        }
        stats.from ??= id; stats.to = id;
        follow = { id, hash: b.computedHash };
        if (!c.S.key) return;
        await inspect(b);
      }
    }
  }
  function schedule() {
    clearTimeout(timer);
    if (!c.S.key) return;
    timer = setTimeout(step, document.visibilityState === 'visible' ? ON_MS : OFF_MS);
  }
  async function step() {
    if (running || !c.S.key) return;
    running = true;
    try { await poll(); stats.error = null; } catch (e) { stats.error = e.message; } finally { running = false; schedule(); }
    if (location.hash === '#settings') renderChecks();
  }
  document.addEventListener('visibilitychange', () => { if (c.S.key && document.visibilityState === 'visible') { clearTimeout(timer); step(); } });

  /** On unlock: count unread messages per account, then follow from the head. */
  async function start() {
    follow = null; unread = new Map();
    for (const { a } of mine()) await countUnread(a).catch(() => 0);
    badge();
    const n = [...unread.values()].reduce((x, y) => x + y, 0);
    if (n) toast(`${n} unread encrypted message${n > 1 ? 's' : ''}`);
    step();
  }
  function stop() { clearTimeout(timer); follow = null; unread = new Map(); badge(); }

  function renderChecks() {
    const box = $('checks');
    if (!box) return;
    box.textContent = '';
    const p = (...k) => box.append(el('p', { class: 'fine' }, ...k));
    if (!stats.blocks) p(el('span', { 'data-testid': 'checks-blocks', text: 'No new block read yet in this session (the page starts at the newest block when unlocked).' }));
    else {
      p(el('span', { 'data-testid': 'checks-blocks', text: `This session: ${stats.blocks} new blocks read (#${stats.from} to #${stats.to}); ${stats.hashOk} of ${stats.blocks} hashes recomputed from their contents and equal; ${stats.linksOk} of ${stats.links} links to the previous block checked.` }));
      if (stats.bad.length) p(el('b', { class: 'bad', text: `Mismatch: ${stats.bad.slice(0, 5).join(', ')}` }));
    }
    if (stats.error) p(`Last read failed: ${stats.error}`);
    p('What this does not check: who produced a block, or that the chain under it is final (a light client of the Logos chain would). The page trusts this network\'s RPC for account state, as LEZ\'s own wallet does; it checks transaction outcomes in the block itself, receive codes (key against account id) and Ethereum facts (Merkle proofs against the attested block hash, Prove tab).');
  }

  function renderSettings() {
    const box = $('notify-settings');
    box.textContent = '';
    const has = 'Notification' in globalThis;
    const perm = has ? Notification.permission : 'unavailable';
    const cb = el('input', { type: 'checkbox', 'data-testid': 'notify-on' });
    cb.checked = !!prefs().notify && perm === 'granted';
    cb.disabled = !has || perm === 'denied';
    cb.onchange = async () => {
      if (cb.checked && Notification.permission !== 'granted') {
        const r = await Notification.requestPermission().catch(() => 'denied');
        if (r !== 'granted') cb.checked = false;
      }
      setPrefs({ notify: cb.checked });
      renderSettings();
    };
    box.append(el('label', { class: 'check' }, cb, ' Notify me of new messages and incoming transfers'),
      el('p', { class: 'fine', 'data-testid': 'notify-state', text: !has ? 'This browser has no notifications here (on iPhone and iPad: install the app to the Home Screen first, iOS 16.4 or later).'
        : perm === 'denied' ? 'Notifications are blocked for this site: allow them in the browser\'s site settings.'
          : `Permission: ${perm}. Checks run every ${ON_MS / 1000} s while the wallet is open (every ${OFF_MS / 1000} s in the background, where the system allows).` }),
      el('div', { class: 'privacy' }, el('b', { text: 'No push service: ' }),
        'the page itself asks this network for new blocks while it is open; nobody else learns when you receive something. When the app is closed nothing runs: you see what arrived when you next open it. Notifications say only "new encrypted message" or the amount received, never a message\'s text.'));
    renderChecks();
  }

  return { start, stop, markSeen, renderSettings, renderChecks, stats, unread };
}
