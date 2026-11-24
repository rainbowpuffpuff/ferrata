// The pages' shared shell (header, navigation, footer, network pill), the config, account labels and small DOM
// helpers. Everything from the chain is put in the page with textContent, never as HTML.

import { b58, hex, fmtInt, shortHex, fmtAge, parseAccountId } from "./util.js";
import { BUILTINS } from "./lez.js";
import { Rpc, rpcUrl } from "./rpc.js";
import { Idl } from "./idl.js";

/** The bundle's root URL (assets/ is one level below it), so pages work at / or under any prefix. */
export const ROOT = new URL("../", import.meta.url);
export const href = (path) => new URL(path, ROOT).pathname + (path.includes("#") ? "" : "");

let configPromise = null;
export function loadConfig() {
  if (!configPromise) {
    configPromise = fetch(new URL("config.json", ROOT), { cache: "no-cache" })
      .then((r) => (r.ok ? r.json() : {}))
      .catch(() => ({}))
      .then(async (cfg) => {
        const path = cfg.programs_json === undefined ? "programs.json" : cfg.programs_json;
        if (!path) return cfg;
        try {
          const r = await fetch(new URL(path, ROOT), { cache: "no-cache" });
          if (r.ok) mergeDeployment(cfg, await r.json());
        } catch {}
        return cfg;
      });
  }
  return configPromise;
}

const PRESET_CHAIN = { mainnet: 1, sepolia: 11155111, holesky: 17000, hoodi: 560048 };
const asB58 = (s) => {
  try {
    return b58(parseAccountId(String(s)));
  } catch {
    return null;
  }
};

/**
 * Fold the deployment's programs.json (deploy/testnet/deploy-programs.sh `publish`: programs by name with
 * `program_account` and `image_id`, instances by name with `instance_id`, `program`, `preset`) into the config:
 * accounts of the catalogue's programs (matched by image id) and the instances, unless the config names them already.
 */
export function mergeDeployment(cfg, dep) {
  if (!dep || typeof dep !== "object") return cfg;
  cfg.programs = cfg.programs || [];
  for (const [name, p] of Object.entries(dep.programs || {})) {
    const img = String(p.image_id || "").replace(/^0x/, "").toLowerCase();
    const account = asB58(p.program_account || p.account || "");
    if (!img || !account) continue;
    const known = cfg.programs.find((q) => q.image_id === img);
    if (known) known.account = known.account || account;
    else cfg.programs.push({ key: (cfg.known_images && cfg.known_images[img] && cfg.known_images[img].key) || name, name, image_id: img, account });
  }
  cfg.instances = cfg.instances || [];
  for (const [name, i] of Object.entries(dep.instances || {})) {
    const program = asB58(i.program || "");
    const id = String(i.instance_id || "").replace(/^0x/, "").toLowerCase();
    if (!program || !/^[0-9a-f]{64}$/.test(id)) continue;
    if (cfg.instances.some((x) => String(x.instance_id || "").toLowerCase() === id)) continue;
    const account = i.instance_account ? asB58(i.instance_account) : null;
    cfg.instances.push({ label: name.charAt(0).toUpperCase() + name.slice(1), program, instance_id: id, account: account || undefined, chain_id: i.chain_id || PRESET_CHAIN[i.preset] || undefined, from: "programs.json" });
  }
  return cfg;
}

const idlCache = new Map();
export function loadIdl(path) {
  if (!idlCache.has(path)) {
    idlCache.set(
      path,
      fetch(new URL(path, ROOT)).then((r) => {
        if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`);
        return r.json().then((j) => new Idl(j));
      }),
    );
  }
  return idlCache.get(path);
}

// ---- DOM ----------------------------------------------------------------------------------------------------

export function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on") && typeof v === "function") e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}

export const badge = (text, kind = "") => el("span", { class: `badge ${kind}` }, text);

export function copyButton(text) {
  return el("button", {
    class: "copy",
    title: "copy",
    onclick: (ev) => {
      ev.preventDefault();
      navigator.clipboard && navigator.clipboard.writeText(text);
      ev.target.textContent = "✓";
      setTimeout(() => (ev.target.textContent = "⧉"), 900);
    },
  }, "⧉");
}

export function kv(rows) {
  const dl = el("dl", { class: "kv" });
  for (const r of rows) {
    if (!r) continue;
    const [k, v] = r;
    dl.append(el("dt", {}, k), el("dd", {}, v));
  }
  return dl;
}

export function table(headers, rows, { empty = "nothing yet" } = {}) {
  const t = el("table", { class: "t" });
  t.append(el("thead", {}, el("tr", {}, headers.map((h) => el("th", { class: h.startsWith(">") ? "right" : "" }, h.replace(/^>/, ""))))));
  const tb = el("tbody");
  if (!rows.length) tb.append(el("tr", {}, el("td", { colspan: headers.length, class: "muted" }, empty)));
  for (const r of rows) tb.append(el("tr", {}, r.map((c, i) => el("td", { class: headers[i] && headers[i].startsWith(">") ? "right" : "" }, c))));
  t.append(tb);
  return el("div", { class: "scroll" }, t);
}

export const panel = (...children) => el("div", { class: "panel" }, ...children);
export const loading = (text = "loading…") => el("div", { class: "loading" }, text);
export const errorBox = (e) => el("div", { class: "badbox" }, `Error: ${e && e.message ? e.message : e}`);

export function stat(label, value, sub) {
  return el("div", { class: "stat" }, el("div", { class: "label" }, label), el("div", { class: "value" }, value), sub ? el("div", { class: "sub" }, sub) : null);
}

// ---- labels -------------------------------------------------------------------------------------------------

/** Names of accounts the pages know: LEZ's built-in programs, Ferrata's programs and instances from config.json. */
export class Labels {
  constructor() {
    this.map = new Map();
    for (const [h, b] of BUILTINS) this.map.set(h, { name: b.name, kind: "builtin" });
  }
  set(bytesOrB58, name, kind = "ferrata") {
    const h = typeof bytesOrB58 === "string" ? hexOfB58(bytesOrB58) : hex(bytesOrB58);
    if (h) this.map.set(h, { name, kind });
  }
  get(bytes) {
    return this.map.get(hex(bytes));
  }
}

import { fromB58 } from "./util.js";
function hexOfB58(s) {
  try {
    return hex(fromB58(s));
  } catch {
    return null;
  }
}

export const labels = new Labels();

/** A link to an account's page, labelled when the account is known. */
export function accountLink(bytes, { short = true } = {}) {
  const id = b58(bytes);
  const l = labels.get(bytes);
  const a = el("a", { href: `${href("explorer/")}#/account/${id}`, title: id, class: l ? "" : "mono", "data-acct": id }, l ? l.name : short ? shortHex(id, 6) : id);
  return a;
}

export function blockLink(id) {
  return el("a", { href: `${href("explorer/")}#/block/${id}` }, `#${fmtInt(id)}`);
}

export function txLink(hashBytesOrHex, { short = true } = {}) {
  const h = typeof hashBytesOrHex === "string" ? hashBytesOrHex : hex(hashBytesOrHex);
  return el("a", { href: `${href("explorer/")}#/tx/${h}`, class: "mono", title: h }, short ? shortHex(h, 8) : h);
}

export function hexSpan(bytes, { short = false, copy = true } = {}) {
  const h = hex(bytes);
  return el("span", {}, el("span", { class: "mono", title: h }, short ? shortHex(h, 10) : h), copy ? copyButton(h) : null);
}

// ---- the shell ----------------------------------------------------------------------------------------------

const NAV = [
  ["home", "Overview", ""],
  ["explorer", "Explorer", "explorer/"],
  ["status", "Attestation status", "status/"],
  ["registry", "Programs & apps", "registry/"],
  ["agents", "For agents", "agents/"],
];

/**
 * Mount the header and footer; returns {main, config, rpc}. `active` is the nav key. The network pill polls the
 * head every 10 s.
 */
export async function shell(active) {
  const config = await loadConfig();
  for (const p of config.programs || []) if (p.account) labels.set(p.account, p.name, "ferrata");
  for (const i of config.instances || []) if (i.account) labels.set(i.account, `${i.label} instance`, "ferrata");
  for (const [acct, name] of Object.entries(config.accounts || {})) labels.set(acct, name, "known");

  const rpc = new Rpc(rpcUrl(config), { rangeLimit: config.range_limit });
  const pill = el("div", { class: "netpill" }, el("span", { class: "dot" }), el("span", {}, "connecting…"));
  // the landing page may be served at another path than the root (config.landing, e.g. "network" beside a front page)
  const nav = el("nav", { class: "main" }, NAV.map(([k, label, path]) => el("a", { href: href(k === "home" ? config.landing || path : path), class: k === active ? "active" : "" }, label)));
  if (config.wallet) nav.append(el("a", { href: config.wallet }, "Wallet"));
  if (config.faucet) nav.append(el("a", { href: config.faucet }, "Faucet"));
  const header = el("header", { class: "top" }, el("div", { class: "wrap" }, el("a", { class: "brand", href: href(config.landing || "") }, "Ferrata ", el("span", {}, "LEZ"), " test network"), nav, pill));
  const main = el("main");
  const footer = el(
    "footer",
    { class: "bottom" },
    el("div", {}, "Test network only: its tokens have no value, and it may be reset. Never paste a private key or seed phrase into any page or agent."),
    el("div", {}, "RPC: ", el("code", {}, rpc.url), config.repo ? [" · source: ", el("a", { href: config.repo }, config.repo.replace(/^https:\/\//, ""))] : null),
  );
  document.body.replaceChildren(header, main, footer);

  const tick = async () => {
    try {
      const id = await rpc.lastBlockId();
      const b = await rpc.block(id);
      const age = b ? fmtAge(Number(b.header.timestamp)) : "";
      const stale = b && Date.now() - Number(b.header.timestamp) > 120000;
      pill.replaceChildren(el("span", { class: `dot ${stale ? "warn" : "ok"}` }), el("span", {}, `block #${fmtInt(id)} · ${age}`));
    } catch (e) {
      pill.replaceChildren(el("span", { class: "dot bad" }), el("span", { title: e.message }, "RPC unreachable"));
    }
  };
  tick();
  setInterval(tick, 10000);
  return { main, config, rpc };
}

// ---- decoded values (IDL trees) ---------------------------------------------------------------------------------

/** Render a decoded IDL value: structs and variants as nested key/value lists, account ids as links. */
export function renderValue(v, depth = 0) {
  if (v === null || v === undefined) return el("span", { class: "muted" }, "None");
  switch (v.k) {
    case "int":
      return el("span", { class: "mono" }, typeof v.v === "bigint" ? fmtInt(v.v) : String(v.v));
    case "bool":
      return el("span", { class: "mono" }, String(v.v));
    case "str":
      return el("span", {}, v.v);
    case "acct":
      return accountLink(v.v, { short: false });
    case "bytes":
      if (v.v.length === 0) return el("span", { class: "muted" }, "(empty)");
      if (v.v.length <= 64) return hexSpan(v.v);
      return el("details", {}, el("summary", { class: "muted" }, `${v.v.length} bytes`), el("div", { class: "hexbox" }, hex(v.v.subarray(0, 4096)) + (v.v.length > 4096 ? "…" : "")));
    case "opt":
      return v.v === null ? el("span", { class: "muted" }, "None") : renderValue(v.v, depth);
    case "list": {
      if (!v.items.length) return el("span", { class: "muted" }, "[]");
      const shown = v.items.slice(0, 50);
      const box = el("div", { class: "tree" }, shown.map((it, i) => el("div", {}, el("span", { class: "tname" }, `[${i}] `), renderValue(it, depth + 1))));
      if (v.items.length > shown.length) box.append(el("div", { class: "muted small" }, `… ${v.items.length - shown.length} more`));
      return box;
    }
    case "struct":
      return el("div", { class: depth ? "tree" : "" }, el("div", { class: "tname" }, v.name), kv(v.fields.map(([n, f]) => [n, renderValue(f, depth + 1)])));
    case "enum":
      if (!v.fields.length) return el("span", {}, el("span", { class: "tname" }, `${v.name}::`), v.variant);
      return el("div", { class: depth ? "tree" : "" }, el("div", {}, el("span", { class: "tname" }, `${v.name}::`), el("strong", {}, v.variant)), kv(v.fields.map(([n, f]) => [n, renderValue(f, depth + 1)])));
    default:
      return el("span", {}, String(v));
  }
}

export function setTitle(t) {
  document.title = t ? `${t} · Ferrata LEZ test network` : "Ferrata LEZ test network";
}
