// The block explorer: hash routes #/, #/blocks, #/block/N, #/tx/HASH, #/account/ID, #/programs, #/search/Q.

import { shell, el, kv, table, panel, loading, errorBox, badge, stat, accountLink, blockLink, txLink, hexSpan, copyButton, renderValue, setTitle, href, labels, loadIdl } from "./ui.js";
import { activityOf, activityItem } from "./activity.js";
import { features, featuresTable } from "./features.js";
import { NameService, normalizeName, wellFormedName, nameStatus, mailboxSummary } from "./names.js";
import { hex, b58, fmtInt, fmtTime, fmtAge, fmtBytes, shortHex, parseAccountId, fromHex, fmtNative } from "./util.js";
import { rootOf, signerAccounts, BUILTINS, builtinProgramId, isZero, NATIVE_TOKEN_PROGRAM_ID, PROGRAM_LOADER_ID } from "./lez.js";
import { Programs } from "./programs.js";
import { FORKS, beaconClock, attestationPda, CHAIN_NAMES } from "./ferrata.js";
import { plain } from "./idl.js";
import { checkAttest, factsPanel } from "./facts.js";

const { main, config, rpc } = await shell("explorer");
const programs = new Programs(rpc, config);
const SCAN = Number(config.scan_blocks || 300);

let generation = 0;
let leaving = false;
window.addEventListener("pagehide", () => (leaving = true));
window.addEventListener("beforeunload", () => (leaving = true));
// links of the form /explorer/?name=alice, ?tx=, ?block=, ?account= open the matching page
{
  const q = new URLSearchParams(location.search);
  const pick = [["name", "name"], ["tx", "tx"], ["block", "block"], ["account", "account"]].find(([k]) => q.get(k));
  if (pick && !location.hash) history.replaceState(null, "", `${location.pathname}${q.get("rpc") ? `?rpc=${encodeURIComponent(q.get("rpc"))}` : ""}#/${pick[1]}/${encodeURIComponent(q.get(pick[0]))}`);
}
window.addEventListener("hashchange", route);
queueMicrotask(route); // after the whole module is evaluated: the pages use its later declarations

function route() {
  const g = ++generation;
  const h = decodeURIComponent(location.hash.replace(/^#\/?/, ""));
  const [page, arg, ...rest] = h.split("/");
  const stale = () => g !== generation;
  const view = el("div");
  main.replaceChildren(searchBar(), view);
  let p;
  if (!page) p = home(view, stale);
  else if (page === "blocks") p = blocks(view, stale, arg);
  else if (page === "block") p = blockPage(view, stale, arg);
  else if (page === "tx") p = txPage(view, stale, arg);
  else if (page === "account") p = accountPage(view, stale, [arg, ...rest].join("/"));
  else if (page === "programs") p = programsPage(view, stale);
  else if (page === "search") p = search(view, stale, [arg, ...rest].join("/"));
  else if (page === "name") p = namePage(view, stale, arg);
  else if (page === "messages") p = messagesPage(view, stale);
  else if (page === "features") p = featuresPage(view, stale);
  else p = Promise.resolve(view.replaceChildren(errorBox(`no page ${page}`)));
  p.catch((e) => {
    if (stale() || leaving) return; // a page left, or the document unloading, cuts requests short: not an error
    view.replaceChildren(errorBox(e));
    console.error(e);
  });
}

function searchBar() {
  const input = el("input", { placeholder: "Block number, transaction hash, account id, or @name", "aria-label": "search", spellcheck: "false" });
  const form = el("form", { class: "search", onsubmit: (ev) => {
    ev.preventDefault();
    const q = input.value.trim();
    if (q) location.hash = `#/search/${encodeURIComponent(q)}`;
  } }, input, el("button", { class: "primary", type: "submit" }, "Search"));
  return form;
}

// ---- transactions in words -----------------------------------------------------------------------------------

const isSystem = (tx) => {
  if (tx.kind !== "Public" || tx.witnesses.length) return false;
  const p = hex(tx.execution.root.to.program_account_id);
  return p === hex(builtinProgramId("clock")) || p === hex(builtinProgramId("fee"));
};

async function describe(tx) {
  const root = rootOf(tx);
  if (!root) return { program: null, name: tx.kind === "PrivacyPreserving" ? "private call" : "—", decoded: null };
  const { info, decoded, error } = await programs.decodeMessage(root.to.program_account_id, root.message);
  return { program: root.to.program_account_id, info, name: decoded ? decoded.ins.name : error ? "undecodable" : `raw ${root.message.length} B`, decoded, error };
}

function statusBadge(block, index) {
  if (!block) return badge("—");
  return block.reverted.includes(index) ? badge("reverted", "bad") : badge("applied", "ok");
}

function bedrockBadge(status) {
  return status === "Finalized" ? badge("finalized", "ok") : status === "Safe" ? badge("safe", "info") : badge("pending", "warn");
}

async function txRows(entries) {
  const rows = [];
  for (const { tx, block, index } of entries) {
    const d = await describe(tx);
    const signer = signerAccounts(tx)[0];
    rows.push([
      txLink(tx.hash),
      blockLink(Number(block.header.block_id)),
      tx.kind === "Public" ? badge("public") : badge("private", "info"),
      d.program ? accountLink(d.program) : el("span", { class: "muted" }, "—"),
      el("span", { class: "callname" }, tx.kind === "Public" ? d.name : `private · ${d.name}`),
      signer ? accountLink(signer) : el("span", { class: "muted" }, tx.kind === "Public" ? "system" : "—"),
      statusBadge(block, index),
    ]);
  }
  return rows;
}

// ---- home --------------------------------------------------------------------------------------------------

const FEED_BLOCKS = Number(config.feed_blocks || 300);
const sessionFinality = []; // seconds from a block's time to the moment this page saw it Finalized
const seenPending = new Map(); // block id -> block time (ms) while Pending

function noteFinality(blocksInView) {
  for (const b of blocksInView) {
    const id = Number(b.header.block_id);
    if (b.bedrock_status !== "Finalized") seenPending.set(id, Number(b.header.timestamp));
    else if (seenPending.has(id)) {
      sessionFinality.push((Date.now() - seenPending.get(id)) / 1000);
      seenPending.delete(id);
    }
  }
}

const median = (xs) => {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

async function getJsonMaybe(path, init) {
  try {
    const r = await fetch(new URL(path, new URL("../", import.meta.url)), Object.assign({ cache: "no-store" }, init || {}));
    const body = await r.json().catch(() => null);
    return { ok: r.ok, status: r.status, body };
  } catch (e) {
    return { ok: false, status: 0, body: null, error: e.message };
  }
}

/** The genesis block's hash is this network's identity: a reset shows as a new genesis. */
async function chainIdentity() {
  const g = await rpc.block(1).catch(() => null);
  const second = await rpc.block(2).catch(() => null);
  if (!g) return null;
  const id = hex(g.header.hash);
  let previous = null;
  try {
    previous = JSON.parse(localStorage.getItem("ferrata-explorer-genesis") || "null");
    localStorage.setItem("ferrata-explorer-genesis", JSON.stringify({ id, seen: previous && previous.id === id ? previous.seen : Date.now() }));
  } catch {}
  return { id, started: second ? Number(second.header.timestamp) : null, reset: previous && previous.id !== id ? previous : null };
}

function resetNotice(chain) {
  if (!chain || !chain.reset) return null;
  return el("div", { class: "badbox" }, el("strong", {}, "This network was reset since your last visit. "),
    `Its genesis changed from ${shortHex(chain.reset.id, 8)} to ${shortHex(chain.id, 8)}: accounts, names, messages and balances from before are gone. `,
    "Get new test tokens below; keys still work, their accounts start empty.");
}

function statsHeader(ctx) {
  const { head, view, chain, hostHistory } = ctx;
  const last = view[view.length - 1];
  const gaps = [];
  for (let i = Math.max(1, view.length - 100); i < view.length; i++) if (view[i - 1].header.timestamp > 0n) gaps.push(Number(view[i].header.timestamp - view[i - 1].header.timestamp) / 1000);
  const med = median(gaps);
  const sorted = [...gaps].sort((a, b) => a - b);
  const p95 = sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))] : null;
  const fin = [...view].reverse().find((b) => b.bedrock_status === "Finalized");
  const behind = fin ? head - Number(fin.header.block_id) : null;
  const hostAges = (hostHistory || []).map((p) => p.final_age_s).filter((x) => typeof x === "number");
  const hostMed = median(hostAges);
  const sessMed = median(sessionFinality);
  const last100 = view.slice(-100);
  const user100 = last100.reduce((n, b) => n + b.transactions.filter((t) => !isSystem(t)).length, 0);
  const userAll = view.reduce((n, b) => n + b.transactions.filter((t) => !isSystem(t)).length, 0);
  const finalityText = sessMed !== null ? `${Math.round(sessMed)} s` : behind !== null && med ? `≈ ${Math.round(behind * med)} s` : "—";
  return el("div", { class: "stats stats-hero" },
    stat("Block height", `#${fmtInt(head)}`, last && last.hash_ok ? "head · hash recomputed ✓" : "head"),
    stat("Latest block", el("span", { "data-ts": String(Number(last.header.timestamp)) }, fmtAge(Number(last.header.timestamp))), fmtTime(Number(last.header.timestamp))),
    stat("Block time", med ? `${med.toFixed(1)} s` : "—", gaps.length ? `median of the last ${gaps.length} measured; 95% within ${p95.toFixed(1)} s` : ""),
    stat("Final on Logos Blockchain", finalityText,
      [fin ? `#${fmtInt(fin.header.block_id)} final, ${behind} behind the head` : "no final block in view",
        sessionFinality.length ? ` · seen here: ${sessionFinality.length} block(s)` : "",
        hostMed !== null ? ` · 24 h median age of the newest final block ${Math.round(hostMed)} s` : ""].join("")),
    stat("User transactions", fmtInt(user100), `last 100 blocks · ${fmtInt(userAll)} in the last ${view.length}`),
    chain ? stat("Chain id (genesis)", el("span", { class: "mono small", title: chain.id }, shortHex(chain.id, 5), copyButton(chain.id)), chain.started ? `since ${fmtTime(chain.started).slice(0, 16)} UTC` : "genesis block 1") : null,
  );
}

async function feedItems(view, from = 0) {
  const items = [];
  for (let i = view.length - 1; i >= from; i--) {
    const b = view[i];
    for (let j = b.transactions.length - 1; j >= 0; j--) {
      const tx = b.transactions[j];
      if (isSystem(tx)) continue;
      try {
        const a = await activityOf(tx, b, j, programs, config);
        if (a) items.push({ a, tx, b });
      } catch (e) {
        console.warn("activity", e);
      }
      if (items.length >= 40) return items;
    }
  }
  return items;
}

function faucetBox() {
  const terms = el("p", { class: "muted small" }, "reading the faucet's terms…");
  const account = el("input", { placeholder: "account id (base58) or @name", "aria-label": "faucet account", spellcheck: "false" });
  const pk = el("input", { placeholder: "public key, 64 hex (only for an account never used)", "aria-label": "faucet public key", spellcheck: "false" });
  const out = el("div", { class: "small", "aria-live": "polite" });
  const btn = el("button", { class: "primary", type: "submit" }, "Request test tokens");
  const form = el("form", { class: "faucet", onsubmit: async (ev) => {
    ev.preventDefault();
    out.replaceChildren("sending the request…");
    btn.disabled = true;
    try {
      let target = account.value.trim();
      if (!target) throw new Error("enter an account id or a name");
      const n = normalizeName(target);
      if (wellFormedName(n) && !/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(target)) {
        const ns = await nameService();
        const rec = ns ? await ns.record(n) : null;
        if (!rec || rec.missing || nameStatus(rec) !== "live" || !rec.target) throw new Error(`@${n} does not resolve to an account`);
        target = b58(rec.target);
      } else target = b58(parseAccountId(target));
      const body = { account: target };
      if (pk.value.trim()) body.public_key = pk.value.trim().replace(/^0x/, "");
      const r = await getJsonMaybe(config.faucet_path || "faucet", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
      if (r.ok && r.body && r.body.ok) out.replaceChildren(badge("sent", "ok"), " ", r.body.hash ? ["transaction ", txLink(r.body.hash)] : "", ` to `, accountLink(parseAccountId(target)));
      else out.replaceChildren(badge("refused", "bad"), " ", (r.body && r.body.error) || `HTTP ${r.status}`);
    } catch (e) {
      out.replaceChildren(badge("not sent", "warn"), " ", e.message);
    } finally {
      btn.disabled = false;
    }
  } }, account, pk, btn);
  getJsonMaybe(config.faucet_path || "faucet").then((r) => {
    if (!r.ok || !r.body) return terms.replaceChildren("the faucet does not answer here");
    const b = r.body;
    const lim = b.limits || {};
    terms.replaceChildren(el("strong", {}, fmtNative(b.amount || 0, config.native)), ` per request · once per account every ${Math.round((lim.per_account_seconds || 86400) / 3600)} h · ${lim.per_ip_per_day || "?"} per IP a day · faucet holds `, fmtNative(b.balance || 0, config.native), ...(b.faucet_account ? [" (", accountLink(parseAccountId(b.faucet_account)), ")"] : []));
  });
  return panel(el("h2", {}, "Faucet"), terms, form, out, el("p", { class: "muted small" }, "Test tokens only. A never-used account needs its public key (the wallet sends it for you). Never paste a private key."));
}

function usePanel() {
  const repo = config.repo ? `${config.repo}/blob/main/` : null;
  return panel(el("h2", {}, "Use it"),
    el("ul", { class: "links" },
      config.wallet ? el("li", {}, el("a", { href: config.wallet }, "Web wallet"), ": accounts, tokens, names, encrypted messages, Ethereum proofs") : null,
      repo ? el("li", {}, el("a", { href: `${repo}modules/ferrata_bridge_ui/README.md` }, "Basecamp mini-app"), ": the bridge in Logos Basecamp (build, load, the round trip it runs)") : null,
      el("li", {}, el("a", { href: href("agents/") }, "AGENTS.md"), ": every RPC method, scripts, rules"),
      el("li", {}, el("a", { href: "#/messages" }, "Messages"), " · ", el("a", { href: "#/features" }, "LEZ features here"), " · ", el("a", { href: href("status/") }, "Ethereum light client"))));
}

async function home(view, stale) {
  setTitle("Explorer");
  view.replaceChildren(loading());
  const head = await rpc.lastBlockId();
  const [blocksInView, chain, fee, ids, dead, hostStatus, hostHistory] = await Promise.all([
    rpc.blockRange(Math.max(1, head - FEED_BLOCKS + 1), head),
    chainIdentity(),
    rpc.feeState().catch(() => null),
    rpc.programIds().catch(() => null),
    rpc.call("getCrossZoneDeadLetters").catch(() => null),
    getJsonMaybe(config.status_api || "status/status.json").then((r) => (r.ok ? r.body : null)),
    getJsonMaybe(config.status_history || "status/history.json").then((r) => (r.ok && Array.isArray(r.body) ? r.body : null)),
  ]);
  if (stale()) return;
  let view_ = blocksInView;
  let lastHead = head;
  noteFinality(view_);
  const statsBox = el("div");
  const feed = el("ul", { class: "feed" });
  const featBox = el("div");
  const blocksBox = el("div");
  const renderStats = () => statsBox.replaceChildren(statsHeader({ head: lastHead, view: view_, chain, hostHistory }));
  const renderBlocks = () => blocksBox.replaceChildren(table(["Block", "Age", ">User txs", ">Size", "Bedrock", "Hash"], [...view_].reverse().slice(0, 10).map((b) => [
    blockLink(Number(b.header.block_id)),
    el("span", { "data-ts": String(Number(b.header.timestamp)), title: fmtTime(Number(b.header.timestamp)) }, fmtAge(Number(b.header.timestamp))),
    fmtInt(b.transactions.filter((t) => !isSystem(t)).length),
    fmtBytes(b.size),
    bedrockBadge(b.bedrock_status),
    b.hash_ok ? badge("✓", "ok") : badge("✗", "bad"),
  ])));
  const renderFeatures = () => {
    const list = features({ view: view_, head: lastHead, fee, ids, deadLetters: dead, status: hostStatus, config });
    featBox.replaceChildren(el("ul", { class: "featlist" }, list.map((f) => el("li", {}, f.state === "on" ? badge("on", "ok") : f.state === "partial" ? badge("partly", "warn") : badge("off"), " ", f.name))), el("p", { class: "small" }, el("a", { href: "#/features" }, "each with its evidence →")));
  };
  renderStats();
  renderBlocks();
  renderFeatures();
  const first = await feedItems(view_);
  if (stale()) return;
  feed.replaceChildren(...(first.length ? first.map(({ a, tx, b }) => activityItem(a, tx, b)) : [el("li", { class: "muted" }, `no user transaction in the last ${view_.length} blocks`)]));
  const liveDot = el("span", { class: "badge info" }, "live · every 10 s");
  view.replaceChildren(
    resetNotice(chain) || "",
    statsBox,
    el("div", { class: "grid home" },
      panel(el("h2", {}, "Latest activity ", liveDot), el("p", { class: "muted small" }, "Decoded from the blocks: transfers and the faucet, names, encrypted messages (who, to whom, how large; never the content), the Ethereum light client and its attestations, the bridge, program deployments."), feed),
      el("div", {}, faucetBox(), panel(el("h2", {}, "LEZ v0.4 on this network"), featBox), usePanel())),
    panel(el("h2", {}, "Latest blocks ", el("a", { class: "small", href: "#/blocks" }, "all →")), blocksBox),
    el("p", { class: "muted small" }, "Every block's hash is recomputed in your browser from its contents (SHA-256 over LEZ's block encoding). The sequencer's RPC reports no receipts: an outcome is read from its block's reverted list."),
  );
  annotateNames(view);
  // refresh: new blocks every 10 s, ages every second; stops when the page changes
  const tickAges = setInterval(() => {
    if (stale()) return clearInterval(tickAges);
    for (const s of view.querySelectorAll("[data-ts]")) s.textContent = fmtAge(Number(s.dataset.ts));
  }, 1000);
  const refresh = setInterval(async () => {
    if (stale()) return clearInterval(refresh);
    try {
      const h = await rpc.lastBlockId();
      if (h <= lastHead) return;
      // the tail of the view is re-read too: a block's Bedrock status changes after it is made
      const from = Math.max(1, Math.min(lastHead - 5, h) + 1);
      const fresh = await rpc.blockRange(from, h);
      if (stale()) return;
      const byId = new Map(view_.map((b) => [Number(b.header.block_id), b]));
      for (const b of fresh) byId.set(Number(b.header.block_id), b);
      view_ = [...byId.values()].sort((x, y) => Number(x.header.block_id - y.header.block_id)).slice(-FEED_BLOCKS);
      noteFinality(fresh);
      const newOnes = fresh.filter((b) => Number(b.header.block_id) > lastHead);
      lastHead = h;
      const items = await feedItems(newOnes);
      if (stale()) return;
      if (items.length) {
        if (feed.firstChild && feed.firstChild.classList && !feed.firstChild.classList.contains("act")) feed.replaceChildren();
        for (const { a, tx, b } of items.reverse()) feed.prepend(activityItem(a, tx, b, true));
        while (feed.children.length > 60) feed.lastChild.remove();
        annotateNames(feed);
      }
      renderStats();
      renderBlocks();
      renderFeatures();
    } catch (e) {
      console.warn("refresh", e);
    }
  }, 10000);
}

// ---- names, messages, features pages ---------------------------------------------------------------------

let nameServicePromise = null;
function nameService() {
  if (!nameServicePromise) {
    const p = (config.programs || []).find((q) => q.key === "names" && q.account);
    nameServicePromise = p ? loadIdl(p.idl || "idl/ferrata_names.idl.json").then((idl) => new NameService(rpc, parseAccountId(p.account), idl)).catch(() => null) : Promise.resolve(null);
  }
  return nameServicePromise;
}

/** Put each resolved name beside its account link: `@alice`. A few lookups at a time; cached for the page. */
async function annotateNames(container) {
  const ns = await nameService();
  if (!ns) return;
  const links = [...container.querySelectorAll("a[data-acct]:not([data-named])")];
  const queue = links.filter((a) => !labels.get(parseAccountId(a.dataset.acct)) || labels.get(parseAccountId(a.dataset.acct)).kind === "known");
  let i = 0;
  const worker = async () => {
    while (i < queue.length) {
      const a = queue[i++];
      a.dataset.named = "1";
      const name = await ns.reverse(parseAccountId(a.dataset.acct));
      if (name) {
        a.textContent = `@${name}`;
        a.classList.remove("mono");
        a.title = `${a.dataset.acct} (name @${name})`;
      }
    }
  };
  await Promise.all([worker(), worker(), worker()]);
}

async function namePage(view, stale, arg) {
  const n = normalizeName(arg || "");
  setTitle(`@${n}`);
  view.replaceChildren(loading());
  const ns = await nameService();
  if (!ns) {
    view.replaceChildren(el("h1", {}, `@${n}`), el("div", { class: "warnbox" }, "The names program is not deployed on this network (or not in its programs.json)."));
    return;
  }
  if (!wellFormedName(n)) {
    view.replaceChildren(el("h1", {}, `@${n}`), el("div", { class: "warnbox" }, "Not a valid name: 3-32 characters of a-z, 0-9 and -, not starting or ending with -, without --."));
    return;
  }
  const rec = await ns.record(n);
  if (stale()) return;
  const parts = [el("h1", {}, `@${n} `, rec.missing ? badge("free", "info") : badge(nameStatus(rec), nameStatus(rec) === "live" ? "ok" : "warn"))];
  if (rec.missing) {
    parts.push(panel(kv([["Name account", accountLink(rec.account, { short: false })], ["Status", "not registered (or released): anyone may register it"]]), config.wallet ? el("p", {}, "Register it in the ", el("a", { href: config.wallet }, "web wallet"), " (commit, then reveal after 4 minutes).") : null));
  } else {
    const primary = rec.target ? await ns.reverse(rec.target) : null;
    parts.push(panel(kv([
      ["Resolves to", rec.target ? el("span", {}, accountLink(rec.target, { short: false }), primary === n ? el("span", { class: "muted small" }, " (and this is that account's primary name)") : "") : el("span", { class: "muted" }, "no public target")],
      ["Private receiver", rec.receiver ? "set: payments to the name can go to a private account" : "none"],
      ["Owner", accountLink(rec.owner, { short: false })],
      ["Registered", fmtTime(rec.registered_ms)],
      ["Expires", `${fmtTime(rec.expires_ms)} (${fmtAge(rec.expires_ms)}); 30 days of grace after`],
      rec.offer ? ["Offered to", accountLink(rec.offer, { short: false })] : null,
      ["Name account", accountLink(rec.account, { short: false })],
    ])));
  }
  // its transactions among recent blocks
  const head = await rpc.lastBlockId();
  const recent = await rpc.blockRange(Math.max(1, head - SCAN + 1), head);
  const items = [];
  for (let i = recent.length - 1; i >= 0; i--) {
    for (let j = recent[i].transactions.length - 1; j >= 0; j--) {
      const tx = recent[i].transactions[j];
      const r = rootOf(tx);
      if (!r) continue;
      const d = await programs.decodeMessage(r.to.program_account_id, r.message);
      if (!d.decoded || !d.info || d.info.key !== "names") continue;
      const nm = d.decoded.args.find(([k]) => k === "name");
      const val = nm && (nm[1].k === "opt" ? nm[1].v && nm[1].v.v : nm[1].v);
      if (val !== n) continue;
      const a = await activityOf(tx, recent[i], j, programs, config);
      if (a) items.push(activityItem(a, tx, recent[i]));
    }
  }
  if (stale()) return;
  parts.push(panel(el("h2", {}, "Its recent transactions"), items.length ? el("ul", { class: "feed" }, items) : el("p", { class: "muted" }, `none in the last ${recent.length} blocks (a commit does not name the name)`)));
  view.replaceChildren(...parts);
  for (const s of view.querySelectorAll("[data-ts]")) s.textContent = fmtAge(Number(s.dataset.ts));
  annotateNames(view);
}

async function messagesPage(view, stale) {
  setTitle("Messages");
  view.replaceChildren(loading());
  const head = await rpc.lastBlockId();
  const recent = await rpc.blockRange(Math.max(1, head - FEED_BLOCKS + 1), head);
  const rows = [];
  for (let i = recent.length - 1; i >= 0; i--) {
    for (let j = recent[i].transactions.length - 1; j >= 0; j--) {
      const tx = recent[i].transactions[j];
      const r = rootOf(tx);
      if (!r || r.message[0] !== 4) continue;
      const d = await programs.decodeMessage(r.to.program_account_id, r.message);
      if (!d.decoded || !d.info || d.info.key !== "msg" || d.decoded.ins.name !== "send") continue;
      const to = d.decoded.args.find(([k]) => k === "to")[1].v;
      const body = d.decoded.args.find(([k]) => k === "body")[1].v;
      rows.push([
        el("span", { "data-ts": String(Number(recent[i].header.timestamp)), title: fmtTime(Number(recent[i].header.timestamp)) }, fmtAge(Number(recent[i].header.timestamp))),
        accountLink(signerAccounts(tx)[0]),
        accountLink(to),
        fmtBytes(body.length),
        recent[i].reverted.includes(j) ? badge("refused", "bad") : badge("delivered", "ok"),
        txLink(tx.hash),
      ]);
    }
  }
  if (stale()) return;
  view.replaceChildren(
    el("h1", {}, "Encrypted messages"),
    el("div", { class: "note" }, "What the chain shows of a message: who sent it, to whom, when, and the size of its envelope (padded to 512, 1,024, 2,048 or 4,096 bytes). The content is encrypted to the recipient (X25519, or X25519 + ML-KEM-768, and XChaCha20-Poly1305) and this page never shows it."),
    panel(table(["When", "From", "To", ">Envelope", "Outcome", "Transaction"], rows, { empty: `no message in the last ${recent.length} blocks` })),
  );
  annotateNames(view);
}

async function featuresPage(view, stale) {
  setTitle("LEZ features on this network");
  view.replaceChildren(loading());
  const head = await rpc.lastBlockId();
  const [recent, fee, ids, dead, hostStatus] = await Promise.all([
    rpc.blockRange(Math.max(1, head - FEED_BLOCKS + 1), head),
    rpc.feeState().catch(() => null),
    rpc.programIds().catch(() => null),
    rpc.call("getCrossZoneDeadLetters").catch(() => null),
    getJsonMaybe(config.status_api || "status/status.json").then((r) => (r.ok ? r.body : null)),
  ]);
  if (stale()) return;
  view.replaceChildren(
    el("h1", {}, "LEZ v0.4 on this network"),
    el("p", { class: "muted" }, `What LEZ v0.4.0-rc3 does here, read live from the last ${recent.length} blocks and the RPC; each row links its evidence on this chain and in LEZ's source at that tag.`),
    panel(featuresTable(features({ view: recent, head, fee, ids, deadLetters: dead, status: hostStatus, config }))),
  );
}

// ---- blocks ------------------------------------------------------------------------------------------------

async function blocks(view, stale, beforeArg) {
  setTitle("Blocks");
  view.replaceChildren(loading());
  const head = await rpc.lastBlockId();
  const top = beforeArg ? Math.min(head, Number(beforeArg)) : head;
  const from = Math.max(1, top - 49);
  const list = await rpc.blockRange(from, top);
  if (stale()) return;
  const rows = [...list].reverse().map((b) => [
    blockLink(Number(b.header.block_id)),
    el("span", { class: "mono", title: hex(b.header.hash) }, shortHex(hex(b.header.hash), 8)),
    fmtTime(Number(b.header.timestamp)),
    fmtInt(b.transactions.length),
    fmtInt(b.transactions.filter((t) => !isSystem(t)).length),
    b.reverted.length ? badge(`${b.reverted.length} reverted`, "warn") : "",
    fmtBytes(b.size),
    bedrockBadge(b.bedrock_status),
    b.hash_ok ? badge("✓", "ok") : badge("✗", "bad"),
  ]);
  const nav = el("p", {},
    top < head ? el("a", { href: `#/blocks/${Math.min(head, top + 50)}` }, "← newer") : null, " ",
    from > 1 ? el("a", { href: `#/blocks/${from - 1}` }, "older →") : null);
  view.replaceChildren(el("h1", {}, "Blocks"), panel(table(["Block", "Hash", "Time (UTC)", ">Txs", ">User", "Reverted", ">Size", "Bedrock", "Hash ✓"], rows)), nav);
}

async function blockPage(view, stale, arg) {
  const id = Number(arg);
  setTitle(`Block ${id}`);
  view.replaceChildren(loading());
  const [b, head] = await Promise.all([rpc.block(id), rpc.lastBlockId()]);
  if (stale()) return;
  if (!b) {
    view.replaceChildren(el("h1", {}, `Block #${fmtInt(id)}`), el("div", { class: "warnbox" }, `The sequencer has no block ${id} (head is #${fmtInt(head)}).`));
    return;
  }
  const entries = b.transactions.map((tx, index) => ({ tx, block: b, index }));
  const rows = await txRows(entries);
  rows.forEach((r, i) => r.unshift(String(i)));
  if (stale()) return;
  view.replaceChildren(
    el("h1", {}, `Block #${fmtInt(id)} `, bedrockBadge(b.bedrock_status)),
    panel(kv([
      ["Hash", hexSpan(b.header.hash)],
      ["Hash check", b.hash_ok ? badge("recomputed in this browser: equal", "ok") : el("span", {}, badge("recomputed hash differs", "bad"), " ", hexSpan(b.computed_hash))],
      ["Previous", id > 1 ? el("span", {}, el("a", { href: `#/block/${id - 1}`, class: "mono" }, hex(b.header.prev_block_hash))) : hexSpan(b.header.prev_block_hash)],
      ["Time", `${fmtTime(Number(b.header.timestamp))} (${fmtAge(Number(b.header.timestamp))})`],
      ["Bedrock status", el("span", {}, b.bedrock_status, " ", el("span", { class: "muted small" }, "(as the sequencer reports it; outside the block hash)"))],
      ["Transactions", `${b.transactions.length} (${b.transactions.filter((t) => !isSystem(t)).length} user)`],
      ["Reverted", b.reverted.length ? b.reverted.join(", ") : "none"],
      ["Size", fmtBytes(b.size)],
    ])),
    el("p", {}, id > 1 ? el("a", { href: `#/block/${id - 1}` }, "← previous") : null, " ", id < head ? el("a", { href: `#/block/${id + 1}` }, "next →") : null),
    panel(el("h2", {}, "Transactions"), table(["#", "Hash", "Block", "Kind", "Program", "Call", "Signer", "Status"], rows)),
  );
}

// ---- transaction -------------------------------------------------------------------------------------------

async function txPage(view, stale, hashArg) {
  setTitle("Transaction");
  view.replaceChildren(loading());
  const h = String(hashArg || "").replace(/^0x/, "").toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(h)) throw new Error("a transaction hash is 64 hex digits");
  const found = await rpc.transaction(h);
  if (stale()) return;
  if (!found) {
    view.replaceChildren(el("h1", {}, "Transaction"), el("div", { class: "warnbox" }, `The sequencer knows no transaction ${h}. It answers only for transactions included in a block; one still in the mempool, dropped, or refused at submission is not found.`));
    return;
  }
  const { tx, blockId } = found;
  const block = await rpc.block(blockId);
  const index = block ? block.transactions.findIndex((t) => hex(t.hash) === h) : -1;
  const d = await describe(tx);
  if (stale()) return;
  const parts = [];
  parts.push(el("h1", {}, tx.kind === "Public" ? "Public transaction " : "Private transaction ", index >= 0 ? statusBadge(block, index) : null));
  if (isSystem(tx)) parts.push(el("div", { class: "note" }, "A system transaction: the sequencer adds one to every block (the clock's tick, the fee distribution). Two of them with the same contents have the same hash, so the same hash can recur in other blocks; the RPC names one."));
  parts.push(panel(kv([
    ["Hash", hexSpan(tx.hash)],
    ["Block", el("span", {}, blockLink(blockId), index >= 0 ? ` · position ${index}` : "", block ? ` · ${fmtTime(Number(block.header.timestamp))}` : "")],
    ["Status", index >= 0 ? el("span", {}, statusBadge(block, index), " ", el("span", { class: "muted small" }, block.reverted.includes(index) ? "charged, its effects reverted (listed in the block's reverted indices)" : "included and not in the block's reverted list")) : "—"],
    ["Bedrock", block ? bedrockBadge(block.bedrock_status) : "—"],
    ["Kind", tx.kind === "Public" ? "public" : "privacy-preserving (only the public parts are shown)"],
    ["Size", fmtBytes(tx.size)],
  ])));

  const root = rootOf(tx);
  if (root) parts.push(callPanel(tx.kind === "Public" ? "Call" : "Public root call", root, d));
  if (root && d.decoded && d.info && d.info.key === "attestation" && d.decoded.ins.name === "attest") {
    try {
      parts.push(factsPanel(await checkAttest(rpc, d.info, root, d.decoded.args), index >= 0 && block.reverted.includes(index)));
    } catch (e) {
      parts.push(errorBox(e));
    }
  }
  if (tx.kind === "Public" && tx.execution.fee) {
    const f = tx.execution.fee;
    parts.push(panel(el("h2", {}, "Fee declaration"), kv([["Payer", accountLink(f.payer, { short: false })], ["Gas limit", fmtInt(f.gas_limit)], ["Tip", fmtInt(f.tip)], ["Max fee", `${fmtNative(f.max_fee, config.native)}`]])));
  }
  if (tx.kind === "PrivacyPreserving") parts.push(...privateParts(tx));
  parts.push(signersPanel(tx));
  parts.push(contextPanel(tx));
  view.replaceChildren(...parts);
}

function callPanel(title, root, d) {
  const progName = d.info && d.info.name ? d.info.name : null;
  let body;
  if (d.decoded && d.info && d.info.key === "msg") {
    // an encrypted message's envelope is shown by its size only
    d.decoded.args = d.decoded.args.map(([n, v]) => (n === "body" && v.k === "bytes" ? [n, { k: "str", v: `${v.v.length} bytes of encrypted envelope (not shown)` }] : [n, v]));
  }
  if (d.decoded) {
    body = el("div", {},
      el("p", {}, el("strong", { class: "mono" }, d.decoded.ins.name), d.decoded.ins.docs ? el("span", { class: "muted" }, ` — ${d.decoded.ins.docs[0]}`) : null),
      d.decoded.args.length ? kv(d.decoded.args.map(([n, v]) => [n, renderValue(v, 1)])) : el("p", { class: "muted" }, "no arguments"));
  } else {
    body = el("div", {},
      el("p", { class: "muted" }, d.error ? `The message does not decode with ${progName || "this program"}'s IDL: ${d.error}` : "No IDL for this program; the message's bytes:"),
      el("div", { class: "hexbox" }, hex(root.message.subarray(0, 8192))));
  }
  return panel(el("h2", {}, title),
    kv([
      ["Program", el("span", {}, accountLink(root.to.program_account_id, { short: false }), progName ? null : el("span", { class: "muted small" }, " (unrecognised)"))],
      ["Actor account", accountLink(root.to.account_id, { short: false })],
      ["Message", `${fmtInt(root.message.length)} bytes`],
    ]),
    el("h3", {}, "Decoded"), body);
}

function privateParts(tx) {
  const e = tx.execution;
  const out = [];
  out.push(el("div", { class: "note" }, "A privacy-preserving transaction: the accounts it spends and creates, the amounts and the recipients are encrypted or committed; what the chain sees is below. The proof (", fmtBytes(tx.proof.length), ") shows the private execution followed the programs it names."));
  out.push(panel(el("h2", {}, `Private actions (${e.private_actions.length})`), table(["#", "Nullifier", "Commitment", "Under root", ">Note"], e.private_actions.map((a, i) => [
    String(i), el("span", { class: "mono", title: hex(a.nullifier) }, shortHex(hex(a.nullifier), 8)), el("span", { class: "mono", title: hex(a.commitment) }, shortHex(hex(a.commitment), 8)), el("span", { class: "mono", title: hex(a.root) }, shortHex(hex(a.root), 8)), fmtBytes(a.encrypted_post_state.ciphertext.length),
  ]))));
  if (e.boundary.length) {
    out.push(panel(el("h2", {}, `Boundary between private and public (${e.boundary.length})`), table(["#", "Step", "From", "To", ">Message"], e.boundary.map((s, i) => [
      String(i), s.kind, s.delivery ? accountLink(s.delivery.envelope.from.account_id) : "", s.delivery ? el("span", {}, accountLink(s.delivery.envelope.to.account_id), " @ ", accountLink(s.delivery.envelope.to.program_account_id)) : "", s.delivery ? `${s.delivery.envelope.message.length} B` : "",
    ]))));
  }
  if (e.async_messages.length) out.push(panel(el("h2", {}, `Sealed messages (${e.async_messages.length})`), table(["#", "Commitment", ">Note"], e.async_messages.map((m, i) => [String(i), el("span", { class: "mono" }, hex(m.commitment)), fmtBytes(m.note.ciphertext.length)]))));
  if (e.recovery_bindings.length) out.push(panel(el("h2", {}, `Recovery bindings (${e.recovery_bindings.length})`), table(["#", "Address", ">Note"], e.recovery_bindings.map((m, i) => [String(i), accountLink(m.address), fmtBytes(m.note.ciphertext.length)]))));
  const w = e.validity;
  const win = (x, f) => (x.from === null && x.to === null ? "unbounded" : `${x.from === null ? "−∞" : f(x.from)} … ${x.to === null ? "∞" : f(x.to)}`);
  out.push(panel(el("h2", {}, "Validity, images, fee"), kv([
    ["Blocks", win(w.blocks, (v) => `#${fmtInt(v)}`)],
    ["Timestamps", win(w.timestamps, (v) => fmtTime(Number(v)))],
    ["Program images", e.program_image_claims.length ? el("div", {}, e.program_image_claims.map((c) => el("div", {}, c.kind === "Disclosed" ? [accountLink(c.account_id), " runs image ", el("span", { class: "mono" }, shortHex(hex(c.image_id), 10))] : ["undisclosed, under root ", el("span", { class: "mono" }, shortHex(hex(c.root), 10))]))) : "none"],
    ["Fee", e.fee ? el("span", {}, "refund to ", e.fee.refund_to ? accountLink(e.fee.refund_to) : "—", e.fee.fallback ? " · with a fallback action" : "") : "none"],
    ["Proof", fmtBytes(tx.proof.length)],
  ])));
  return out;
}

function signersPanel(tx) {
  const signers = signerAccounts(tx);
  const nonce = new Map(tx.nonces.map((n) => [hex(n.account), n.nonce]));
  return panel(el("h2", {}, `Signers (${signers.length})`), table(["Account", "Public key", ">Nonce"], signers.map((a, i) => [
    accountLink(a, { short: false }), el("span", { class: "mono small", title: hex(tx.witnesses[i].public_key) }, shortHex(hex(tx.witnesses[i].public_key), 8)), nonce.has(hex(a)) ? fmtInt(nonce.get(hex(a))) : "—",
  ]), { empty: "none: a system transaction (the sequencer's own) or a private one whose spends are proven, not signed" }));
}

function contextPanel(tx) {
  const c = tx.public_context;
  return panel(el("h2", {}, "Public context"),
    table(["Actor account", "Program"], c.actors.map((a) => [accountLink(a.account_id, { short: false }), accountLink(a.program_account_id)]), { empty: "no public actors" }),
    kv([
      ["Authorized", c.authorized_accounts.length ? el("div", {}, c.authorized_accounts.map((a) => el("div", {}, accountLink(a, { short: false })))) : "none"],
      c.promotions.length ? ["Promotions", c.promotions.map(String).join(", ")] : null,
      tx.admission_evidence.length ? ["Admission evidence", el("div", {}, tx.admission_evidence.map((e) => el("div", { class: "small" }, e.kind === "key" ? ["key ", el("span", { class: "mono" }, shortHex(hex(e.public_key), 8))] : ["PDA of ", accountLink(e.program), " seed ", el("span", { class: "mono" }, shortHex(hex(e.seed), 8))])))] : null,
    ]));
}

// ---- account -----------------------------------------------------------------------------------------------

async function accountPage(view, stale, idArg) {
  setTitle("Account");
  view.replaceChildren(loading());
  const id = parseAccountId(idArg);
  const acct = await rpc.account(id);
  if (stale()) return;
  const label = labels.get(id);
  const parts = [el("h1", {}, label ? label.name : "Account")];
  const states = acct ? acct.states : [];
  const native = states.find((s) => isZero(s.program));
  let balance = 0n;
  if (native) for (let i = native.data.length - 1; i >= 0; i--) balance = (balance << 8n) | BigInt(native.data[i]);
  parts.push(panel(kv([
    ["Account id", el("span", {}, el("span", { class: "mono" }, b58(id)), copyButton(b58(id)))],
    ["Hex", el("span", { class: "mono small muted" }, hex(id))],
    ["Nonce", acct ? fmtInt(acct.nonce) : "—"],
    ["Native balance", acct ? el("span", {}, fmtNative(balance, config.native), config.native ? el("span", { class: "muted small" }, ` (${fmtInt(balance)} units)`) : null) : "—"],
    ["Actor states", acct ? String(states.length) : "none"],
  ])));
  if (!acct) parts.push(el("div", { class: "note" }, "No public state: the account was never written, or it is a private account (private accounts live as commitments, not as public state)."));
  view.replaceChildren(...parts);
  for (const s of states) {
    if (isZero(s.program)) continue;
    const box = panel(el("h2", {}, "State at ", accountLink(s.program, { short: false })), loading());
    view.append(box);
    renderState(box, id, s).catch((e) => box.append(errorBox(e)));
  }
  const recent = panel(el("h2", {}, "Recent transactions"), loading(`scanning the last ${SCAN} blocks…`));
  view.append(recent);
  const rows = await scanAccount(id, stale);
  if (stale()) return;
  recent.replaceChildren(el("h2", {}, "Recent transactions"), el("p", { class: "muted small" }, `Transactions in the last ${SCAN} blocks that sign for, pay from, call or list this account (public parts only; LEZ's RPC has no index).`), table(["Hash", "Block", "Kind", "Program", "Call", "Signer", "Status"], await txRows(rows)));
}

async function scanAccount(id, stale) {
  const head = await rpc.lastBlockId();
  const list = await rpc.blockRange(Math.max(1, head - SCAN + 1), head);
  const h = hex(id);
  const out = [];
  for (const b of list) {
    if (stale()) return out;
    b.transactions.forEach((tx, index) => {
      if (isSystem(tx)) return;
      const touched = [
        ...tx.public_context.actors.flatMap((a) => [a.account_id, a.program_account_id]),
        ...tx.public_context.authorized_accounts,
        ...tx.nonces.map((n) => n.account),
        ...signerAccounts(tx),
      ];
      const root = rootOf(tx);
      if (root) touched.push(root.to.account_id, root.to.program_account_id);
      if (tx.kind === "Public" && tx.execution.fee) touched.push(tx.execution.fee.payer);
      if (touched.some((t) => hex(t) === h)) out.push({ tx, block: b, index });
    });
  }
  return out.reverse().slice(0, 50);
}

async function renderState(box, accountId, s) {
  const d = await programs.decodeState(s.program, s.data);
  const head = box.firstChild;
  const body = [];
  if (d.kind === "loader-header") {
    const img = d.image_id;
    body.push(kv([
      ["Program", d.known ? el("strong", {}, d.known.name) : el("span", { class: "muted" }, "an image this page does not know")],
      ["Image id", el("span", {}, el("span", { class: "mono" }, `0x${img}`), copyButton(img))],
      ["First segment", accountLink(d.value.fields[1][1].v, { short: false })],
      ["Immutable", String(d.value.fields[2][1].v)],
      d.known && d.known.idl ? ["IDL", el("a", { href: href(d.known.idl) }, d.known.idl)] : null,
      d.known && d.known.source ? ["Source", el("a", { href: d.known.source }, d.known.source.replace(/^https:\/\/github.com\//, ""))] : null,
    ]));
    body.push(el("p", { class: "small muted" }, "This account is a program: LEZ's loader keeps its header here. Compare the image id with a reproducible build (docs/image-ids.md)."));
  } else if (d.kind === "loader-segment") {
    const code = d.value.fields[0][1].v;
    body.push(kv([["Program segment", `${fmtBytes(code.length)} of RISC Zero program`], ["Next segment", d.value.fields[1][1].v ? accountLink(d.value.fields[1][1].v.v) : "none (head)"]]));
  } else if (d.kind === "attestation") {
    body.push(attestationView(accountId, s.program, d));
  } else if (d.kind === "idl" && d.info.key === "msg" && d.type === "Mailbox") {
    body.push(mailboxView(d, s));
  } else if (d.kind === "idl" && d.info.key === "names") {
    body.push(await namesView(accountId, d));
  } else if (d.kind === "idl") {
    body.push(el("p", {}, badge(d.type, "info"), " decoded with ", el("a", { href: href(d.info.idlPath) }, d.info.idlPath)), renderValue(d.value));
  } else {
    body.push(el("p", { class: "muted" }, `${s.data.length} bytes`), el("div", { class: "hexbox" }, hex(s.data.subarray(0, 4096))));
  }
  box.replaceChildren(head, ...body);
}

function mailboxView(d, s) {
  const m = mailboxSummary(d.info.idl, s.data);
  return el("div", {},
    el("p", {}, badge("encrypted inbox", "accent"), " ", m.key ? `messaging key published (epoch ${m.key.epoch}, ${m.key.hybrid ? "X25519 + ML-KEM-768" : "X25519"})` : "no messaging key published", " · inbox ", el("strong", {}, m.policy === "ContactsOnly" ? "contacts only" : "open"), ` · ${m.contacts} contact(s), ${m.blocked} blocked · ${m.sent_total} sent in all`),
    table(["#", "From", "When", ">Envelope"], m.inbox.slice().reverse().map((x) => [String(x.seq), accountLink(x.from), el("span", { title: fmtTime(x.at_ms) }, fmtAge(x.at_ms)), fmtBytes(x.size)]), { empty: "no message waiting" }),
    el("p", { class: "muted small" }, "Messages are encrypted to this account; the explorer shows who sent each, when and its size, never its content."));
}

async function namesView(accountId, d) {
  if (d.type === "NameRecord") {
    const n = d.plain.name;
    return el("div", {}, el("p", {}, badge("name record", "accent"), " this account is the name ", el("a", { href: `#/name/${n}` }, `@${n}`)), renderValue(d.value));
  }
  const p = d.plain;
  const ns = await nameService();
  const verified = ns ? await ns.reverse(accountId) : null;
  return el("div", {},
    el("p", {}, badge("names", "accent"), " ", verified ? ["name ", el("a", { href: `#/name/${verified}` }, `@${verified}`), el("span", { class: "muted small" }, " (its primary name, live and pointing back here)")] : p.primary ? ["primary name set to ", el("a", { href: `#/name/${p.primary}` }, `@${p.primary}`), el("span", { class: "muted small" }, " (not pointing back here, or not live)")] : "no primary name"),
    kv([["Names owned", String(p.owned)], ["Pending commitment", p.commit ? `yes (reveal from ${fmtTime(Number(p.commit.not_before_ms))})` : "none"]]));
}

function attestationView(accountId, program, d) {
  const p = d.plain;
  if (d.type === "InstanceState") {
    const st = d.store;
    const cfg = p.config;
    const clock = beaconClock(cfg);
    const slot = st ? st.finalized.slot : null;
    const recordAcct = st ? attestationPda(program, "header", fromHex(p.id), slot) : null;
    return el("div", {},
      el("p", {}, badge("light-client instance", "accent"), " ", p.active ? badge("active", "ok") : badge("not yet active", "warn"), " ", el("a", { href: `${href("status/")}#${b58(accountId)}` }, "live status →")),
      kv([
        ["Instance id", el("span", { class: "mono" }, p.id)],
        ["Chain", `${CHAIN_NAMES[cfg.chain_id] || "chain"} (${cfg.chain_id})`],
        ["Light-client layout", st ? st.fork : "?"],
        ["Finalized slot", st ? el("span", {}, fmtInt(slot), ` · epoch ${fmtInt(clock.epochOf(slot))} · period ${fmtInt(clock.periodOf(slot))} · ${fmtTime(clock.timeOfSlotMs(slot))} (${fmtAge(clock.timeOfSlotMs(slot))})`) : "?"],
        ["Finalized block root", st ? hexSpan(beaconRoot(st)) : "?"],
        ["Its record", recordAcct ? accountLink(recordAcct, { short: false }) : "—"],
        ["Current committee", st ? el("span", { class: "mono small" }, hex(st.current.pubkeys_root)) : "?"],
        ["Next committee", st ? (st.next ? badge("known", "ok") : badge("unknown", "warn")) : "?"],
        ["Jobs applied", fmtInt(p.applied)],
        ["Root id", el("span", { class: "mono small" }, p.root_id)],
        ["Parent", /^0+$/.test(p.parent_id) ? "none (a root instance)" : el("span", { class: "mono small" }, p.parent_id)],
        ["Store digest", st ? el("span", { class: "mono small" }, hex(st.digest)) : "?"],
      ]),
      el("details", {}, el("summary", {}, "Configuration"), renderValue(d.value.fields.find(([n]) => n === "config")[1], 1)));
  }
  if (d.type === "RecordState") {
    const r = d.record;
    const ex = r ? r.execution : null;
    return el("div", {},
      el("p", {}, badge("attested header record", "accent"), " written once by the instance's own finality update"),
      kv([
        ["Instance", el("span", { class: "mono small" }, p.instance_id)],
        ["Chain id", String(p.chain_id)],
        ["Beacon slot", r ? fmtInt(r.beacon.slot) : "?"],
        ["Beacon block root", r ? hexSpan(r.block_root) : "?"],
        ["Beacon state root", r ? hexSpan(r.beacon.state_root) : "?"],
        ["Execution", ex ? ex.layout : "?"],
        ex && ex.block_number !== undefined ? ["Block number", fmtInt(ex.block_number)] : null,
        ex && ex.block_hash ? ["Execution block hash", hexSpan(ex.block_hash)] : null,
        ex && ex.state_root ? ["Execution state root", hexSpan(ex.state_root)] : null,
        ex && ex.timestamp !== undefined ? ["Execution time", fmtTime(Number(ex.timestamp) * 1000)] : null,
        ["Root id", el("span", { class: "mono small" }, p.root_id)],
      ]),
      r ? el("p", { class: "small muted" }, "Check it: the beacon block root above must equal a public beacon API's root for slot ", fmtInt(r.beacon.slot), " (the status page does this for you).") : null);
  }
  return el("div", {}, el("p", {}, badge(d.type, "info")), renderValue(d.value));
}

import { beaconHeaderRoot } from "./ferrata.js";
const beaconRoot = (st) => beaconHeaderRoot(st.finalized);

// ---- programs ----------------------------------------------------------------------------------------------

async function programsPage(view, stale) {
  setTitle("Programs");
  view.replaceChildren(loading());
  const ids = await rpc.programIds().catch(() => ({}));
  if (stale()) return;
  const builtin = Object.entries(ids).map(([name, img]) => {
    const acct = name === "authenticated_transfer" || name === "native_token" ? NATIVE_TOKEN_PROGRAM_ID : builtinProgramId(name);
    return [el("strong", {}, name), BUILTINS.has(hex(acct)) ? accountLink(acct) : el("span", { class: "muted" }, "—"), el("span", { class: "mono small", title: img }, shortHex(img, 10))];
  });
  const ferrata = (config.programs || []).map((p) => [el("strong", {}, p.name), p.account ? accountLink(fromB58Safe(p.account)) : el("span", { class: "muted" }, "not deployed yet"), el("span", { class: "mono small" }, p.image_id ? shortHex(p.image_id.replace(/^0x/, ""), 10) : "—"), p.idl ? el("a", { href: href(p.idl) }, "IDL") : ""]);
  view.replaceChildren(
    el("h1", {}, "Programs"),
    panel(el("h2", {}, "Ferrata's programs"), table(["Program", "Account", "Image id", "IDL"], ferrata, { empty: "none configured" }), el("p", { class: "small" }, el("a", { href: href("registry/") }, "Program registry and app directory →"))),
    panel(el("h2", {}, "Built into this LEZ (getProgramIds)"), table(["Name", "Account", "Image id"], builtin)),
  );
}

function fromB58Safe(s) {
  try {
    return parseAccountId(s);
  } catch {
    return new Uint8Array(32);
  }
}

// ---- search ------------------------------------------------------------------------------------------------

async function search(view, stale, q) {
  view.replaceChildren(loading(`looking up ${q}…`));
  const t = q.trim().replace(/^#/, "");
  if (t.startsWith("@") || (wellFormedName(normalizeName(t)) && !/^\d+$/.test(t) && t.length <= 32 && !/[A-Z]/.test(t))) {
    location.replace(`#/name/${normalizeName(t)}`);
    return;
  }
  if (/^\d+$/.test(t)) {
    location.replace(`#/block/${Number(t)}`);
    return;
  }
  if (/^(0x)?[0-9a-fA-F]{64}$/.test(t)) {
    const h = t.replace(/^0x/, "").toLowerCase();
    const found = await rpc.transaction(h).catch(() => null);
    if (stale()) return;
    if (found) {
      location.replace(`#/tx/${h}`);
      return;
    }
    // a block hash among recent blocks?
    const head = await rpc.lastBlockId();
    const list = await rpc.blockRange(Math.max(1, head - SCAN + 1), head);
    const b = list.find((x) => hex(x.header.hash) === h);
    if (b) {
      location.replace(`#/block/${b.header.block_id}`);
      return;
    }
    // else an account id in hex
    location.replace(`#/account/${b58(fromHex(h))}`);
    return;
  }
  try {
    const id = parseAccountId(t);
    location.replace(`#/account/${b58(id)}`);
  } catch {
    view.replaceChildren(el("div", { class: "warnbox" }, `Nothing matches “${q}”: try a block number, a 64-hex-digit transaction hash, or a base58 account id.`));
  }
}
