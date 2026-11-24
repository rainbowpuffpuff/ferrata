// A small Markdown renderer for AGENTS.md: headings, paragraphs, lists, tables, fenced code, block quotes, inline code,
// bold, links and bare URLs. It builds DOM nodes (text through textContent), never HTML strings.

import { el } from "./ui.js";

const slug = (t) => t.toLowerCase().replace(/[`*]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

function inline(text) {
  const out = [];
  const re = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\[[^\]]+\]\([^)\s]+\))|(https?:\/\/[^\s)<>|]+[^\s)<>|.,;:])/g;
  let last = 0;
  let m;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const t = m[0];
    if (m[1]) out.push(el("code", {}, t.slice(1, -1)));
    else if (m[2]) out.push(el("strong", {}, inline(t.slice(2, -2))));
    else if (m[3]) {
      const [, label, url] = t.match(/^\[([^\]]+)\]\(([^)\s]+)\)$/);
      out.push(safeLink(url, inline(label)));
    } else out.push(safeLink(t, t));
    last = m.index + t.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function safeLink(url, children) {
  return /^(https?:\/\/|\/|#|\.\.?\/)/.test(url) && !/<this site>/.test(url) ? el("a", { href: url, rel: "noopener" }, children) : el("span", {}, children);
}

const splitRow = (line) => line.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());

export function renderMarkdown(src) {
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  const root = el("div", { class: "md" });
  const toc = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    const fenceM = line.match(/^```(\w*)\s*$/);
    if (fenceM) {
      const body = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) body.push(lines[i++]);
      i++;
      const code = el("code", { class: fenceM[1] ? `lang-${fenceM[1]}` : "" }, body.join("\n"));
      root.append(el("pre", {}, code));
      continue;
    }
    const h = line.match(/^(#{1,4})\s+(.*)$/);
    if (h) {
      const level = h[1].length;
      const id = slug(h[2]);
      root.append(el(`h${level}`, { id }, inline(h[2])));
      if (level === 2) toc.push([id, h[2].replace(/[`*]/g, "")]);
      i++;
      continue;
    }
    if (/^\|/.test(line) && i + 1 < lines.length && /^\|[\s:|-]+\|?\s*$/.test(lines[i + 1])) {
      const head = splitRow(line);
      i += 2;
      const rows = [];
      while (i < lines.length && /^\|/.test(lines[i])) rows.push(splitRow(lines[i++]));
      root.append(el("div", { class: "scroll" }, el("table", {}, el("thead", {}, el("tr", {}, head.map((c) => el("th", {}, inline(c))))), el("tbody", {}, rows.map((r) => el("tr", {}, r.map((c) => el("td", {}, inline(c)))))))));
      continue;
    }
    if (/^\s*(\d+\.|-)\s+/.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const list = el(ordered ? "ol" : "ul");
      while (i < lines.length && /^\s*(\d+\.|-)\s+/.test(lines[i])) {
        let item = lines[i].replace(/^\s*(\d+\.|-)\s+/, "");
        i++;
        while (i < lines.length && lines[i].trim() && /^\s{2,}\S/.test(lines[i]) && !/^\s*(\d+\.|-)\s+/.test(lines[i])) item += " " + lines[i++].trim();
        list.append(el("li", {}, inline(item)));
      }
      root.append(list);
      continue;
    }
    if (/^>\s?/.test(line)) {
      const body = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) body.push(lines[i++].replace(/^>\s?/, ""));
      root.append(el("blockquote", { class: "note" }, inline(body.join(" "))));
      continue;
    }
    const para = [line];
    i++;
    while (i < lines.length && lines[i].trim() && !/^(#{1,4}\s|```|\||\s*(\d+\.|-)\s+|>)/.test(lines[i])) para.push(lines[i++]);
    root.append(el("p", {}, inline(para.join(" "))));
  }
  return { root, toc };
}
