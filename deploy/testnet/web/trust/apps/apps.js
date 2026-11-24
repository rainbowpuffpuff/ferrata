// /apps/: each deployed program's loader header read from this network and compared with the registry (the rebuilt
// ids), its reverse name record looked up, and each app's files hashed against the site manifest.
const A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
function b58(bytes) {
  let n = 0n;
  for (const b of bytes) n = n * 256n + BigInt(b);
  let s = "";
  while (n > 0n) { s = A[Number(n % 58n)] + s; n /= 58n; }
  for (const b of bytes) { if (b !== 0) break; s = "1" + s; }
  return s;
}
const LOADER = b58(new Uint8Array(32).fill(0xfe));
const hex = (u8) => Array.from(u8, (b) => b.toString(16).padStart(2, "0")).join("");

function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  for (const k of kids) e.append(k);
  return e;
}
const badge = (cls, text) => el("span", { class: `badge ${cls}` }, text);

async function rpc(method, params) {
  const r = await fetch("/rpc", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }) });
  const j = await r.json();
  if (j.error) throw new Error(j.error.message || JSON.stringify(j.error));
  return j.result;
}

async function header(account) {
  const v = await rpc("getAccountView", [{ account_id: account, program_account_id: LOADER }]);
  const data = v && v.data && v.data.actor_states && v.data.actor_states[LOADER];
  if (!data) throw new Error("no loader header at that account");
  const u8 = Uint8Array.from(data);
  if (u8.length !== 66 || u8[0] !== 0) throw new Error(`not a loader header (${u8.length} bytes)`);
  return { image: "0x" + hex(u8.slice(1, 33)), immutable: u8[65] === 1 };
}

async function programs(reg, live) {
  const status = document.getElementById("chain-status");
  let names = null;
  const namesAcct = live.ferrata_names && live.ferrata_names.program_account;
  if (namesAcct) {
    try {
      const mod = await import("/names/names.js");
      names = new mod.Names({ rpc: "/rpc", program: namesAcct });
    } catch { names = null; }
  }
  let fresh = 0;
  await Promise.all(reg.programs.map(async (p) => {
    const row = document.querySelector(`tr[data-program="${p.name}"]`);
    const cell = row.querySelector('[data-col="chain"]');
    const ncell = row.querySelector('[data-col="name"]');
    const lp = live[p.name];
    if (!lp) { cell.replaceChildren(badge("warn", "not in /programs.json")); ncell.replaceChildren("—"); return; }
    try {
      const h = await header(lp.program_account);
      const pub = (lp.image_id || "").toLowerCase();
      let b;
      if (h.image === p.image_id && pub === p.image_id && h.immutable) { b = badge("ok", "fresh"); fresh++; }
      else if (h.image === pub && h.image !== p.image_id) b = badge("warn", "stale");
      else b = badge("bad", "mismatch");
      cell.replaceChildren(b, el("br"), el("span", { class: "small" }, "header ", el("code", { title: h.image }, h.image.slice(0, 12) + "…"), h.immutable ? ", immutable" : ", MUTABLE"),
        el("br"), el("span", { class: "small" }, "account ", el("a", { href: `/explorer/?account=${lp.program_account}` }, el("code", {}, lp.program_account.slice(0, 8) + "…"))));
    } catch (e) {
      cell.replaceChildren(badge("warn", "could not read"), el("br"), el("span", { class: "small" }, e.message));
    }
    if (!names) { ncell.replaceChildren(el("span", { class: "muted small" }, "names program not reachable")); return; }
    try {
      const n = await names.reverse(lp.program_account);
      ncell.replaceChildren(n ? el("code", {}, typeof n === "string" ? n : (n.name || JSON.stringify(n))) : el("span", { class: "muted small" }, "none registered"));
    } catch (e) {
      ncell.replaceChildren(el("span", { class: "muted small" }, `lookup failed: ${e.message}`));
    }
  }));
  status.textContent = `${fresh} of ${reg.programs.length} programs fresh: their loader headers on this network name the rebuilt image ids and are immutable (read ${new Date().toISOString().slice(0, 19)}Z).`;
  const extra = Object.keys(live).filter((k) => !reg.programs.some((p) => p.name === k));
  if (extra.length) {
    document.getElementById("extra").replaceChildren(el("div", { class: "note warn", role: "note" },
      el("p", {}, el("strong", {}, "Deployed but not in the registry: "), extra.join(", "), ". No reproduction is recorded for them yet.")));
  }
}

async function apps() {
  const m = await (await fetch("/bridge/verify/manifest.json", { cache: "no-store" })).json();
  const covered = new Set();
  const rows = [...document.querySelectorAll("#apps-table tr[data-prefix]")];
  const pick = (row) => {
    const prefixes = row.dataset.prefix.split(" ");
    return m.files.filter((f) => prefixes.some((p) => (p.endsWith("/") ? f.path.startsWith(p) : f.path === p)));
  };
  for (const row of rows) for (const f of pick(row)) covered.add(f.path);
  const rest = document.querySelector("#apps-table tr[data-rest]");
  const groups = rows.map((row) => [row, pick(row)]);
  if (rest) groups.push([rest, m.files.filter((f) => !covered.has(f.path))]);
  for (const [row, files] of groups) {
    const cell = row.querySelector(".files");
    if (!files.length) { cell.replaceChildren(badge("warn", "not in the manifest")); continue; }
    let ok = 0;
    for (const f of files) {
      try {
        const buf = await (await fetch(f.path, { cache: "no-store" })).arrayBuffer();
        if (hex(new Uint8Array(await crypto.subtle.digest("SHA-256", buf))) === f.sha256) ok++;
      } catch { /* counted as not equal */ }
    }
    const commits = [...new Set(files.map((f) => f.commit.slice(0, 8)))];
    const unpublished = files.filter((f) => f.public === false).length;
    cell.replaceChildren(badge(ok === files.length ? "ok" : "bad", `${ok} of ${files.length} equal`), el("br"),
      el("span", { class: "small" }, "commit ", ...commits.flatMap((c, i) => [i ? ", " : "", el("code", {}, c)])),
      ...(unpublished ? [el("br"), badge("warn", `${unpublished} at commits not public yet`)] : []),
      ...(row.dataset.rest ? [el("br"), el("span", { class: "small" }, files.map((f) => f.path).join(", "))] : []));
  }
}

const reg = await (await fetch("registry.json", { cache: "no-store" })).json();
const live = (await (await fetch("/programs.json", { cache: "no-store" })).json()).programs || {};
await Promise.all([programs(reg, live), apps().catch(() => {})]);
