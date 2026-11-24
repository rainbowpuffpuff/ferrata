// One small time-series line chart in SVG: one series per chart (never two y-scales), a 2px line, hairline solid
// grid, clean y ticks, optional labelled reference lines, a crosshair with a tooltip that snaps to the nearest point
// (pointer and keyboard), and the same values in a table for readers who do not hover. Text through textContent only.

import { el } from "./ui.js";

const NS = "http://www.w3.org/2000/svg";
const svg = (tag, attrs = {}) => {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
};

function niceStep(range, target = 4) {
  const raw = range / target;
  const mag = 10 ** Math.floor(Math.log10(raw || 1));
  const n = raw / mag;
  return (n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10) * mag;
}
const fmtTick = (v) => (Math.abs(v) >= 10000 ? `${Math.round(v / 1000)}K` : v.toLocaleString("en-US"));
const hhmm = (t) => new Date(t * 1000).toISOString().slice(11, 16);

/**
 * points: [{t (unix s), v (number or null)}], oldest first. Returns a figure element.
 * opts: {title, unit, ref: [{v, label}], height}
 */
export function lineChart(points, { title, unit = "", ref = [], height = 160 } = {}) {
  const pts = points.filter((p) => typeof p.v === "number" && Number.isFinite(p.v));
  const fig = el("figure", { class: "chart" }, el("figcaption", { class: "chart-title" }, title));
  if (pts.length < 2) {
    fig.append(el("p", { class: "muted small" }, "not enough history yet"));
    return fig;
  }
  const W = 560, H = height, L = 44, R = 12, T = 10, B = 22;
  const t0 = pts[0].t, t1 = pts[pts.length - 1].t;
  const vmax = Math.max(...pts.map((p) => p.v), ...ref.map((r) => r.v), 1);
  const step = niceStep(vmax);
  const top = Math.ceil(vmax / step) * step;
  const x = (t) => L + ((t - t0) / Math.max(1, t1 - t0)) * (W - L - R);
  const y = (v) => T + (1 - v / top) * (H - T - B);
  const s = svg("svg", { viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": `${title}: ${pts.length} points from ${hhmm(t0)} to ${hhmm(t1)} UTC`, class: "chart-svg" });
  for (let v = 0; v <= top + 1e-9; v += step) {
    s.append(svg("line", { x1: L, x2: W - R, y1: y(v), y2: y(v), class: "chart-grid" }));
    const tx = svg("text", { x: L - 6, y: y(v) + 4, "text-anchor": "end", class: "chart-tick" });
    tx.textContent = fmtTick(v);
    s.append(tx);
  }
  for (const k of [0, 0.5, 1]) {
    const t = t0 + k * (t1 - t0);
    const tx = svg("text", { x: x(t), y: H - 6, "text-anchor": k === 0 ? "start" : k === 1 ? "end" : "middle", class: "chart-tick" });
    tx.textContent = `${hhmm(t)} UTC`;
    s.append(tx);
  }
  for (const r of ref) {
    if (r.v > top) continue;
    s.append(svg("line", { x1: L, x2: W - R, y1: y(r.v), y2: y(r.v), class: "chart-ref" }));
    const tx = svg("text", { x: W - R, y: y(r.v) - 4, "text-anchor": "end", class: "chart-tick" });
    tx.textContent = r.label;
    s.append(tx);
  }
  // the line, broken where a point is missing
  let d = "";
  let pen = false;
  for (const p of points) {
    if (typeof p.v !== "number" || !Number.isFinite(p.v)) {
      pen = false;
      continue;
    }
    d += `${pen ? "L" : "M"}${x(p.t).toFixed(1)},${y(p.v).toFixed(1)}`;
    pen = true;
  }
  s.append(svg("path", { d, class: "chart-line" }));
  const last = pts[pts.length - 1];
  s.append(svg("circle", { cx: x(last.t), cy: y(last.v), r: 4, class: "chart-dot" }));
  // crosshair and tooltip
  const hair = svg("line", { y1: T, y2: H - B, class: "chart-hair", visibility: "hidden" });
  const mark = svg("circle", { r: 4, class: "chart-dot", visibility: "hidden" });
  s.append(hair, mark);
  const tip = el("div", { class: "chart-tip", hidden: true });
  const show = (i) => {
    const p = pts[Math.max(0, Math.min(pts.length - 1, i))];
    hair.setAttribute("x1", x(p.t));
    hair.setAttribute("x2", x(p.t));
    hair.setAttribute("visibility", "visible");
    mark.setAttribute("cx", x(p.t));
    mark.setAttribute("cy", y(p.v));
    mark.setAttribute("visibility", "visible");
    tip.replaceChildren(el("strong", {}, `${p.v.toLocaleString("en-US")}${unit ? " " + unit : ""}`), el("span", { class: "muted" }, ` · ${hhmm(p.t)} UTC`));
    tip.hidden = false;
    tip.style.left = `${(x(p.t) / W) * 100}%`;
    cur = i;
  };
  let cur = pts.length - 1;
  const nearest = (clientX) => {
    const box = s.getBoundingClientRect();
    const tx = t0 + ((((clientX - box.left) / box.width) * W - L) / (W - L - R)) * (t1 - t0);
    let best = 0;
    for (let i = 1; i < pts.length; i++) if (Math.abs(pts[i].t - tx) < Math.abs(pts[best].t - tx)) best = i;
    return best;
  };
  s.addEventListener("pointermove", (e) => show(nearest(e.clientX)));
  s.addEventListener("pointerleave", () => {
    hair.setAttribute("visibility", "hidden");
    mark.setAttribute("visibility", "hidden");
    tip.hidden = true;
  });
  const wrap = el("div", { class: "chart-wrap", tabindex: "0", "aria-label": `${title}; arrow keys move through the points` }, tip);
  wrap.prepend(s);
  wrap.addEventListener("focus", () => show(cur));
  wrap.addEventListener("blur", () => (tip.hidden = true));
  wrap.addEventListener("keydown", (e) => {
    if (e.key === "ArrowLeft") show(cur - 1);
    else if (e.key === "ArrowRight") show(cur + 1);
    else return;
    e.preventDefault();
  });
  const rows = pts.slice(-60).reverse();
  fig.append(wrap, el("details", { class: "small" }, el("summary", { class: "muted" }, `table (last ${rows.length} of ${pts.length})`),
    el("table", { class: "t" }, el("thead", {}, el("tr", {}, el("th", {}, "UTC"), el("th", { class: "right" }, unit || "value"))),
      el("tbody", {}, rows.map((p) => el("tr", {}, el("td", {}, new Date(p.t * 1000).toISOString().slice(0, 16).replace("T", " ")), el("td", { class: "right" }, p.v.toLocaleString("en-US"))))))));
  return fig;
}
