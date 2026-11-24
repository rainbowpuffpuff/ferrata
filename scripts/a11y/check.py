#!/usr/bin/env python3
"""axe-core and a keyboard pass over every page of the test network site, in Chromium and Firefox.

    python3 -I scripts/a11y/check.py --site URL --out results.json [--pages /a/,/b/] [--cache DIR]

Needs Python Playwright with its Chromium and Firefox (`pip install playwright && playwright install chromium
firefox`; the record names the versions used: Playwright 1.58 here). axe-core is fetched once from the npm registry at the version pinned below and checked against the
registry's sha512 integrity before use. Each page is loaded, given a few seconds for its scripts, then axe-core runs
with the WCAG 2.0-2.2 A/AA and best-practice tags in: Chromium 1366 px light, Chromium 1366 px dark, Chromium 390 px
light, Firefox 1366 px light. The keyboard pass (Chromium, 1366 px) presses Tab up to 400 times and records each focus
stop. Writes one JSON record; prints one line per page.
"""
import argparse
import base64
import hashlib
import io
import json
import sys
import tarfile
import time
import urllib.error
import urllib.request
from pathlib import Path

AXE_VERSION = "4.14.1"
AXE_URL = f"https://registry.npmjs.org/axe-core/-/axe-core-{AXE_VERSION}.tgz"
AXE_SHA512 = "FHfa+lmNjQZfJz1q8wKqIDwC6DExcHt/pkjNQb09a3NPjCKg2rhu5qb2S4r76ASyFNNtuFooJpZe37OgudVGtQ=="
TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"]
OWNERS = {"/": "build page", "/evidence.html": "build page", "/network": "explorer",
          "/explorer/": "explorer", "/status/": "explorer", "/registry/": "explorer",
          "/agents/": "explorer", "/health": "the host", "/status.html": "the host (leftover)",
          "/wallet/": "wallet", "/basecamp/": "Basecamp page"}
PAGES = ["/", "/evidence.html", "/network", "/health", "/explorer/", "/status/", "/registry/", "/agents/", "/wallet/",
         "/status.html", "/basecamp/", "/trust/", "/bridge/", "/bridge/security/", "/bridge/privacy/", "/bridge/verify/", "/apps/",
         "/accessibility/"]
VARIANTS = [("chromium", 1366, "light"), ("chromium", 1366, "dark"), ("chromium", 390, "light"), ("firefox", 1366, "light")]

FOCUS_JS = """() => {
  const e = document.activeElement;
  if (!e || e === document.body) return null;
  const r = e.getBoundingClientRect(), s = getComputedStyle(e);
  const visible = r.width > 0 && r.height > 0 && s.visibility !== 'hidden';
  const ring = (s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) > 0) || (s.boxShadow && s.boxShadow !== 'none');
  const label = (e.getAttribute('aria-label') || e.innerText || e.value || e.getAttribute('title') || '').trim().slice(0, 40);
  let path = e.tagName.toLowerCase() + (e.id ? '#' + e.id : '');
  return {path, label, visible, ring, inView: r.bottom > 0 && r.right > 0, x: Math.round(r.x), y: Math.round(r.y)};
}"""


def axe_source(cache: Path) -> str:
    """axe.min.js from the pinned release; the cached tarball is checked against the registry's sha512 on every run."""
    f = cache / f"axe-core-{AXE_VERSION}.tgz"
    tgz = f.read_bytes() if f.exists() else None
    if tgz is None:
        req = urllib.request.Request(AXE_URL, headers={"User-Agent": "ferrata-a11y-check/1"})
        tgz = urllib.request.urlopen(req, timeout=60).read()
    got = base64.b64encode(hashlib.sha512(tgz).digest()).decode()
    if got != AXE_SHA512:
        raise SystemExit(f"axe-core {AXE_VERSION}: sha512 {got} is not the pinned {AXE_SHA512} ({f})")
    if not f.exists():
        cache.mkdir(parents=True, exist_ok=True)
        f.write_bytes(tgz)
    with tarfile.open(fileobj=io.BytesIO(tgz)) as t:
        return t.extractfile("package/axe.min.js").read().decode()


def settle(page, wait_s: float):
    try:
        page.wait_for_load_state("networkidle", timeout=12000)
    except Exception:  # noqa: BLE001  (pages that poll the chain never go idle)
        pass
    time.sleep(wait_s)


def run_axe(page, src: str):
    page.add_script_tag(content=src)
    res = page.evaluate("async (tags) => { const r = await axe.run(document, {runOnly: {type: 'tag', values: tags}, resultTypes: ['violations']}); "
                        "return r.violations.map(v => ({id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.length, "
                        "targets: v.nodes.slice(0, 3).map(n => n.target.join(' '))})); }", TAGS)
    return res


def keyboard(page, max_stops: int = 400):
    stops, seen, first = [], set(), None
    hidden = no_ring = 0
    trap = False
    # No click first: it would move the sequential focus starting point past a skip link.
    for i in range(max_stops):
        page.keyboard.press("Tab")
        f = page.evaluate(FOCUS_JS)
        if f is None:
            if stops:
                break  # focus left the document: one full cycle done
            continue
        key = (f["path"], f["label"], f["x"], f["y"])
        if first is None:
            first = f
        elif key == stops[0]:
            break
        if stops and key == stops[-1]:
            trap = True
            break
        if key in seen and len(stops) > 3:
            break
        seen.add(key)
        stops.append(key)
        hidden += not f["visible"]
        no_ring += f["visible"] and not f["ring"]
    skip = bool(first and "skip" in (first["label"] or "").lower())
    return {"stops": len(stops), "skip_link": skip, "first": (first or {}).get("label", ""),
            "hidden_stops": hidden, "focus_visible": "yes" if not no_ring else f"no ring on {no_ring}",
            "trap": "yes" if trap else "none"}


CSP_HEADER = "script-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"  # the host's, for every page
TYPES = {".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".json": "application/json",
         ".svg": "image/svg+xml", ".png": "image/png"}


def overlay_server(site: str, root: Path):
    """A local server at 127.0.0.1 answering from ROOT where it has the file (pages not deployed yet, e.g. this commit's)
    and from SITE for everything else (GET relayed, POST /rpc forwarded): the pages as they will be served. Returns
    (base URL, server); the caller shuts it down."""
    import http.server
    import threading

    class H(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def send(self, code, body, ctype, extra=()):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Security-Policy", CSP_HEADER)
            for k, v in extra:
                self.send_header(k, v)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass  # the browser closed the page before the answer arrived

        def relay(self, data=None):
            req = urllib.request.Request(site + self.path, data=data, method="POST" if data is not None else "GET",
                                         headers={"User-Agent": "ferrata-a11y-check/1", "Content-Type": self.headers.get("Content-Type", "application/json")})
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    self.send(r.status, r.read(), r.headers.get("Content-Type", "application/octet-stream"))
            except urllib.error.HTTPError as e:
                self.send(e.code, e.read(), e.headers.get("Content-Type", "text/plain"))

        def do_GET(self):
            path = self.path.split("?")[0].split("#")[0]
            local = (root / path.lstrip("/")).resolve()
            if root.resolve() in local.parents or local == root.resolve():
                if local.is_dir() and not path.endswith("/"):
                    return self.send(308, b"", "text/plain", [("Location", path + "/")])
                if local.is_dir():
                    local = local / "index.html"
                if local.is_file():
                    return self.send(200, local.read_bytes(), TYPES.get(local.suffix, "application/octet-stream"))
            self.relay()

        def do_POST(self):
            self.relay(self.rfile.read(int(self.headers.get("Content-Length", "0"))))

    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return f"http://127.0.0.1:{srv.server_address[1]}", srv


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--site", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--pages")
    ap.add_argument("--cache", default=str(Path(__file__).resolve().parents[2] / "target" / "a11y"))
    ap.add_argument("--wait", type=float, default=3.0)
    ap.add_argument("--label", help="how the record names the site (e.g. when --site is a local copy)")
    ap.add_argument("--overlay", metavar="DIR", help="serve DIR's files locally over --site (pages not deployed yet)")
    a = ap.parse_args()
    from importlib.metadata import version
    from playwright.sync_api import sync_playwright
    src = axe_source(Path(a.cache))
    pages = a.pages.split(",") if a.pages else PAGES
    site = a.site.rstrip("/")
    srv = None
    if a.overlay:
        upstream = site
        site, srv = overlay_server(upstream, Path(a.overlay))
    rec = {p: {"path": p, "owner": OWNERS.get(p, "trust pages"), "runs": {}, "keyboard": {}} for p in pages}
    versions = {}
    with sync_playwright() as pw:
        for engine in ("chromium", "firefox"):
            browser = getattr(pw, engine).launch()
            versions[engine] = browser.version
            for _, width, scheme in [v for v in VARIANTS if v[0] == engine]:
                ctx = browser.new_context(viewport={"width": width, "height": 900}, color_scheme=scheme, bypass_csp=True)
                for p in pages:
                    page = ctx.new_page()
                    try:
                        page.goto(site + p, wait_until="load", timeout=30000)
                        settle(page, a.wait)
                        rec[p]["runs"][f"{engine}-{width}-{scheme}"] = run_axe(page, src)
                        if engine == "chromium" and width == 1366 and scheme == "light":
                            rec[p]["keyboard"] = keyboard(page)
                    except Exception as e:  # noqa: BLE001
                        rec[p]["runs"][f"{engine}-{width}-{scheme}"] = [{"id": "check-error", "impact": "unknown", "help": str(e)[:200], "nodes": 0, "targets": []}]
                    finally:
                        page.close()
                ctx.close()
            browser.close()
    out_pages = []
    for p in pages:
        merged = {}
        for run, vs in rec[p]["runs"].items():
            for v in vs:
                m = merged.setdefault(v["id"], {**v, "where": []})
                m["nodes"] = max(m["nodes"], v["nodes"])
                m["where"].append(run)
        rec[p]["violations"] = sorted(merged.values(), key=lambda v: ("critical serious moderate minor unknown".split().index(v["impact"] or "unknown"), v["id"]))
        out_pages.append({k: rec[p][k] for k in ("path", "owner", "violations", "keyboard")})
        kb = rec[p]["keyboard"]
        print(f"{p:20} {rec[p]['owner']:24} {len(merged):2} rules failed  " + ", ".join(f"{v['id']}({v['impact']},{v['nodes']})" for v in rec[p]["violations"])
              + f" | tab stops {kb.get('stops')} skip {kb.get('skip_link')} ring {kb.get('focus_visible')} trap {kb.get('trap')}")
    clean = sum(1 for x in out_pages if not x["violations"])
    res = {"when": time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime()),
           "tools": f"axe-core {AXE_VERSION} (sha512-pinned), Playwright {version('playwright')} with Chromium "
                    f"{versions.get('chromium')} and Firefox {versions.get('firefox')}",
           "site": a.label or site,
           "summary": f"{clean} of {len(out_pages)} pages pass every axe-core rule in all four runs; the table lists the rest.",
           "pages": out_pages}
    Path(a.out).write_text(json.dumps(res, indent=1) + "\n")
    if srv:
        srv.shutdown()
    return 0


if __name__ == "__main__":
    sys.exit(main())
