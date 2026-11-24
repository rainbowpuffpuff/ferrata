// /agents/: AGENTS.md rendered, with its raw form one click away.

import { shell, el, panel, setTitle, href, errorBox, copyButton } from "./ui.js";
import { renderMarkdown } from "./md.js";

const { main, config, rpc } = await shell("agents");
setTitle("For AI agents");

const raw = new URL("AGENTS.md", new URL("../", import.meta.url));
try {
  const r = await fetch(raw, { cache: "no-cache" });
  if (!r.ok) throw new Error(`AGENTS.md: HTTP ${r.status}`);
  let text = await r.text();
  // the page knows where it is served: show this site's endpoints where the file names placeholders
  const here = new URL(href(""), location.href).href;
  text = text.replaceAll("https://<this site>/", here).replaceAll("https://{{HOST}}/", here);
  const { root, toc } = renderMarkdown(text);
  main.replaceChildren(
    el("div", { class: "note" }, "This page is ", el("a", { href: raw.pathname }, "/AGENTS.md"), " rendered. Give an agent the raw file: ", el("code", {}, new URL(raw.pathname, location.href).href), copyButton(new URL(raw.pathname, location.href).href)),
    el("div", { class: "grid" }, panel(el("strong", {}, "Contents: "), ...toc.flatMap(([id, t], k) => [k ? " · " : "", el("a", { href: `#${id}` }, t)]))),
    panel(root),
  );
} catch (e) {
  main.replaceChildren(errorBox(e));
}
