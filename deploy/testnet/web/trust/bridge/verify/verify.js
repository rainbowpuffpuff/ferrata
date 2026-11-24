// The verify page's quick check: every file of manifest.json fetched from this host and hashed here, and the served
// manifest compared with the copy committed on GitHub. A convenience: a compromised host can change this file too.
const GITHUB = "https://raw.githubusercontent.com/rainbowpuffpuff/ferrata/main/deploy/testnet/web/trust/bridge/verify/manifest.json";
const out = document.getElementById("out");
const btn = document.getElementById("run");

const hex = (buf) => Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, "0")).join("");
const sha = async (buf) => hex(await crypto.subtle.digest("SHA-256", buf));

function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  for (const k of kids) e.append(k);
  return e;
}
const badge = (cls, text) => el("span", { class: `badge ${cls}` }, text);

async function run() {
  btn.disabled = true;
  out.replaceChildren(el("p", {}, "Checking…"));
  const r = await fetch("manifest.json", { cache: "no-store" });
  const servedBuf = await r.arrayBuffer();
  const served = JSON.parse(new TextDecoder().decode(servedBuf));
  const servedHash = await sha(servedBuf);
  let gh;
  try {
    const g = await fetch(GITHUB, { cache: "no-store" });
    gh = g.ok ? await sha(await g.arrayBuffer()) : `not on GitHub's main yet (HTTP ${g.status})`;
  } catch (e) {
    gh = `GitHub unreachable (${e.message})`;
  }
  const rows = [];
  let ok = 0;
  for (const f of served.files) {
    let cell;
    try {
      const res = await fetch(f.path, { cache: "no-store" });
      const h = await sha(await res.arrayBuffer());
      if (h === f.sha256) { ok++; cell = badge("ok", "equal"); } else cell = badge("bad", `differs: ${h.slice(0, 12)}…`);
    } catch (e) {
      cell = badge("warn", `unread: ${e.message}`);
    }
    rows.push(el("tr", {}, el("th", { scope: "row" }, el("code", {}, f.path)), el("td", {}, el("code", {}, f.commit.slice(0, 8))), el("td", {}, cell)));
  }
  const same = gh === servedHash;
  out.replaceChildren(
    el("p", {}, badge(ok === served.files.length ? "ok" : "bad", `${ok} of ${served.files.length} files equal the served manifest`)),
    el("p", {}, "Served manifest SHA-256: ", el("code", {}, servedHash)),
    el("p", {}, "GitHub main's copy: ", /^[0-9a-f]{64}$/.test(gh) ? el("code", {}, gh) : gh, " ",
      /^[0-9a-f]{64}$/.test(gh) ? badge(same ? "ok" : "bad", same ? "equal" : "differs") : badge("warn", "not compared")),
    el("div", { class: "table-wrap", tabindex: "0", role: "region", "aria-label": "Each listed file as served" }, el("table", {}, el("caption", {}, "Each listed file as this host served it to this browser"),
      el("thead", {}, el("tr", {}, el("th", { scope: "col" }, "Path"), el("th", { scope: "col" }, "Commit"), el("th", { scope: "col" }, "SHA-256 here"))),
      el("tbody", {}, ...rows))),
  );
  btn.disabled = false;
}
btn.addEventListener("click", () => run().catch((e) => { out.replaceChildren(el("p", {}, badge("bad", `check failed: ${e.message}`))); btn.disabled = false; }));

// The manifest's summary, rendered on load (the page itself cannot hold it: its own hash is in the manifest).
async function summary() {
  const box = document.getElementById("summary");
  const m = await (await fetch("manifest.json", { cache: "no-store" })).json();
  const groups = new Map();
  for (const f of m.files) {
    const k = `${f.branch} ${f.commit}`;
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(f);
  }
  const repo = "https://github.com/rainbowpuffpuff/ferrata/commit/";
  const rows = [...groups.entries()].sort().map(([k, fs]) => {
    const [branch, commit] = k.split(" ");
    return el("tr", {}, el("th", { scope: "row" }, el("code", {}, branch)),
      el("td", {}, el("a", { href: repo + commit }, el("code", {}, commit.slice(0, 8)))),
      el("td", {}, String(fs.length)),
      el("td", {}, fs[0].public === true ? badge("ok", "public") : fs[0].public === false ? badge("warn", "not public yet") : "—"),
      el("td", { class: "small" }, fs.slice(0, 6).map((f) => f.path).join(", ") + (fs.length > 6 ? " …" : "")));
  });
  const list = (items, f) => el("ul", { class: "small" }, ...(items.length ? items.map((x) => el("li", {}, ...f(x))) : [el("li", {}, "none")]));
  box.replaceChildren(
    el("p", {}, `Made ${m.made}${m.made_how ? ` from ${m.made_how}` : ""}: ${m.files.length} files from the repository, ${(m.generated || []).length} generated on the host, ${(m.unmatched || []).length} matched to no committed file.`),
    el("div", { class: "table-wrap", tabindex: "0", role: "region", "aria-label": "Served files by commit" }, el("table", {}, el("caption", {}, "Served files by the commit they come from"),
      el("thead", {}, el("tr", {}, ...["Branch", "Commit", "Files", "Commit public", "Examples"].map((h) => el("th", { scope: "col" }, h)))),
      el("tbody", {}, ...rows))),
    el("p", { class: "small muted" }, `A commit is public when a branch of the public repository holds it (checked ${(m.public_checked || {}).utc || "when the manifest was made"}). A file at a commit that is not public matches the manifest, but nobody outside the project can check it against its source until the commit is published.`),
    el("h3", {}, "Generated on the host (listed, not hashed)"),
    list(m.generated || [], (g) => [el("code", {}, g.path), `: ${g.why}`]),
    el("h3", {}, "Served, but matched to no committed file"),
    list(m.unmatched || [], (u) => [el("code", {}, u.path), ` (SHA-256 ${u.sha256.slice(0, 16)}…)`]),
  );
}
summary().catch((e) => { document.getElementById("summary").replaceChildren(el("p", {}, badge("warn", `could not read manifest.json: ${e.message}`))); });
