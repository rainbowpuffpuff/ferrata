// /accessibility/: the results table, read from results.json (the record scripts/a11y/check.py wrote), so this page's
// markup does not change when the record does.
function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  for (const k of kids) e.append(k);
  return e;
}
const badge = (cls, text) => el("span", { class: `badge ${cls}` }, text);

async function render() {
  const box = document.getElementById("summary");
  const r = await (await fetch("results.json", { cache: "no-store" })).json();
  const rows = r.pages.map((p) => {
    const v = p.violations || [];
    const serious = v.some((x) => x.impact === "critical" || x.impact === "serious");
    const kb = p.keyboard || {};
    return el("tr", {},
      el("th", { scope: "row" }, el("a", { href: p.path }, p.path)),
      el("td", {}, p.owner || ""),
      el("td", {}, v.length ? badge(serious ? "bad" : "warn", `${v.length} rule${v.length === 1 ? "" : "s"} failed`) : badge("ok", "0 violations")),
      el("td", { class: "small" }, v.length ? v.map((x) => `${x.id} (${x.impact}, ${x.nodes})`).join("; ") : "—"),
      el("td", { class: "small" }, `${kb.stops ?? "?"} tab stops; skip link ${kb.skip_link ? "yes" : "no"}; focus visible ${kb.focus_visible ?? "?"}; traps ${kb.trap ?? "?"}`));
  });
  box.replaceChildren(
    el("p", {}, r.summary),
    el("p", { class: "small muted" }, `Run ${r.when} against ${r.site || "the site"} with ${r.tools}.`),
    el("div", { class: "table-wrap", tabindex: "0", role: "region", "aria-label": "Results by page" },
      el("table", {}, el("caption", {}, "Every page: axe-core rules failed in any of the four runs, and the keyboard pass"),
        el("thead", {}, el("tr", {}, ...["Page", "Owner (branch)", "axe-core", "Rules failed (impact, elements)", "Keyboard"].map((h) => el("th", { scope: "col" }, h)))),
        el("tbody", {}, ...rows))));
}
render().catch((e) => { document.getElementById("summary").replaceChildren(el("p", {}, badge("warn", `could not read results.json: ${e.message}`))); });
