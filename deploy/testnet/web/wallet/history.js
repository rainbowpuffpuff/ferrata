// Activity: LEZ's RPC keeps no per-account index, so the page reads blocks (getBlockRange, 50 at a time) and keeps the
// public transactions that touch this wallet's accounts. What it learns stays in the encrypted wallet record. Each
// transaction this page signs is also journalled (its bytes, hash and nonce) before it is sent, so it is shown while
// pending and can be sent again byte for byte if the network rewinds.
const CHUNK = 50; // the public gateway serves getBlockRange spans of at most 50
const FIRST_SCAN = 1000;
/** Activity keeps the newest MAX_ITEMS transfers per account (older ones are read again by "Scan further back"). */
export const MAX_ITEMS = 500;
/** Adds `added` to the account stores (no duplicates) and keeps each store's newest MAX_ITEMS. */
export function keepItems(c, added) {
  const touched = new Set();
  for (const it of added) {
    const st = store(c, it.account);
    if (!st.items.some((x) => x.hash === it.hash && x.dir === it.dir)) { st.items.push(it); touched.add(st); }
  }
  for (const st of touched) if (st.items.length > MAX_ITEMS) st.items = st.items.sort((x, y) => y.block - x.block).slice(0, MAX_ITEMS);
}

function store(c, id) {
  const h = (c.S.secret.history ??= {});
  return (h[id] ??= { from: null, to: null, items: [], sent: [] });
}

export function remember(c, id, entry) {
  const st = store(c, id);
  st.sent = st.sent.filter((x) => x.hash !== entry.hash).concat([entry]).slice(-50);
  c.persist();
}

function classify(c, d, blockId, time, reverted) {
  if (d.kind !== 'public') return [];
  const t = c.lez.nativeTransferOf(d.tx);
  if (!t) return [];
  const out = [];
  for (const a of c.S.accounts) {
    const mine = a.node.accountId;
    const isFrom = c.lez.equalBytes(t.from, mine), isTo = c.lez.equalBytes(t.to, mine);
    if (!isFrom && !isTo) continue;
    out.push({ account: a.id, hash: d.hash, block: blockId, time, dir: isFrom ? 'out' : 'in',
      other: c.lez.idToString(isFrom ? t.to : t.from), amount: t.amount.toString(), reverted,
      nonce: d.tx.nonces.find(([k]) => c.lez.equalBytes(k, t.from))?.[1]?.toString() ?? null });
  }
  return out;
}

async function scanRange(c, a, b) {
  const found = [];
  for (let s = a; s <= b; s += CHUNK) {
    const e = Math.min(b, s + CHUNK - 1);
    const blocks = await c.seq.blockRange(s, e);
    for (const raw of blocks) {
      const blk = c.lez.decodeBlock(raw);
      blk.transactions.forEach((d, i) => {
        found.push(...classify(c, d, blk.header.id, blk.header.timestamp || null, blk.reverted.includes(i)));
      });
    }
  }
  return found;
}

let scanning = null;
/** Brings every account's history up to the head; `back`: also scan that many older blocks. */
export async function sync(c, back = 0) {
  if (scanning) return scanning;
  scanning = (async () => {
    const head = Number(await c.seq.lastBlockId());
    // One scan serves all accounts: the window is shared.
    const h = (c.S.secret.history ??= {});
    const win = (h._window ??= { from: null, to: null });
    let added = [];
    if (win.to === null) {
      const from = Math.max(1, head - FIRST_SCAN + 1);
      added = await scanRange(c, from, head);
      win.from = from; win.to = head;
    } else if (head > win.to) {
      added = await scanRange(c, win.to + 1, head);
      win.to = head;
    }
    if (back > 0 && win.from > 1) {
      const from = Math.max(1, win.from - back);
      added = added.concat(await scanRange(c, from, win.from - 1));
      win.from = from;
    }
    keepItems(c, added);
    await c.persist();
    return win;
  })();
  try { return await scanning; } finally { scanning = null; }
}

export async function render(c, back = 0) {
  const status = document.getElementById('act-status'), list = document.getElementById('act-list');
  status.textContent = 'Reading blocks…';
  let win;
  try { win = await sync(c, back); } catch (e) { status.textContent = 'Could not read blocks: ' + e.message; return; }
  const a = c.acct(), st = store(c, a.id);
  status.textContent = `Blocks ${win.from} to ${win.to} read${win.from > 1 ? ' (older ones: “Scan further back”)' : ' (from genesis)'}.`;
  const rows = [...st.items].sort((x, y) => y.block - x.block);
  const seen = new Set(rows.map((r) => r.hash));
  const pending = st.sent.filter((s) => !seen.has(s.hash));
  list.textContent = '';
  for (const p of pending) {
    const li = document.createElement('li');
    li.dataset.testid = 'act-pending';
    li.innerHTML = '<span></span><span class="amt out"></span><span class="sub"></span>';
    li.children[0].textContent = `Sent, not in a block yet (nonce ${p.nonce})`;
    li.children[1].textContent = '−' + c.fmt(p.amount);
    li.children[2].textContent = `to ${p.to} · ${p.hash}`;
    list.append(li);
  }
  for (const r of rows) {
    const li = document.createElement('li');
    li.dataset.testid = 'act-item';
    li.innerHTML = '<span></span><span class="amt"></span><span class="sub"></span>';
    li.children[0].textContent = `${r.dir === 'in' ? 'Received' : 'Sent'} · block ${r.block}${r.time ? ' · ' + new Date(r.time).toLocaleString() : ''}${r.reverted ? ' · reverted (fee charged, nothing moved)' : ''}`;
    li.children[1].textContent = (r.reverted ? '±' : r.dir === 'in' ? '+' : '−') + c.fmt(r.amount);
    li.children[1].classList.add(r.dir);
    li.children[2].textContent = `${r.dir === 'in' ? 'from' : 'to'} ${r.other} · ${r.hash}`;
    if (c.names) c.names.reverse(r.other).then((n) => { if (n) li.children[2].textContent = `${r.dir === 'in' ? 'from' : 'to'} ${n}.lez (${r.other}) · ${r.hash}`; }).catch(() => {});
    list.append(li);
  }
  if (!rows.length && !pending.length) {
    const li = document.createElement('li'); li.textContent = 'Nothing yet in the blocks read.'; list.append(li);
  }
  document.getElementById('act-more').onclick = () => render(c, 1000);
}
