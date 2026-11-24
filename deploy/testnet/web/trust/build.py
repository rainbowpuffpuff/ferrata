#!/usr/bin/env python3
"""Build the trust pages of the test network site from src/ (page bodies) and the repository's JSON records.

    python3 -I deploy/testnet/web/trust/build.py          # writes the served tree beside this file
    python3 -I deploy/testnet/web/trust/build.py --check  # exit 1 if a committed page differs from a fresh build

Served tree (installed into the host's web root as is): trust/ bridge/ apps/ accessibility/. Every page gets the same
header, skip link, landmarks and a strict Content-Security-Policy meta (the host adds its own header too).
Inputs: src/*.html, scripts/reproduce/deployed-programs.json (-> apps/registry.json), the committed rebuild run
scripts/reproduce/runs/2026-11-23-deployed-testnet/ (-> apps/rebuild.json) and git (which source commits are public).
The manifest (bridge/verify/manifest.json) and the accessibility record (accessibility/results.json) are read by the
pages' own scripts, so the pages do not change when those records do.
"""
import html
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
SRC = HERE / "src"
NAV = [("/", "Build page"), ("/network", "Network"), ("/explorer/", "Explorer"), ("/wallet/", "Wallet"),
       ("/status/", "Status"), ("/trust/", "Trust"), ("/bridge/", "Bridge"), ("/apps/", "Apps"),
       ("/accessibility/", "Accessibility")]
CSP = ("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; "
       "connect-src 'self' https://raw.githubusercontent.com; base-uri 'none'; form-action 'none'; object-src 'none'")
REPO = "https://github.com/rainbowpuffpuff/ferrata"


def esc(s) -> str:
    return html.escape(str(s), quote=True)


def page(path: str, title: str, desc: str, body: str, script: str = "") -> str:
    depth = path.strip("/").count("/") + 1 if path.strip("/") else 0
    css = "../" * depth + "trust/trust.css"
    nav_section = "/" + path.strip("/").split("/")[0] + "/"
    items = []
    for href, label in NAV:
        cur = ' aria-current="page"' if href == nav_section else ""
        items.append(f'<li><a href="{href}"{cur}>{label}</a></li>')
    js = f'\n<script type="module" src="{esc(script)}"></script>' if script else ""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<meta name="referrer" content="no-referrer">
<meta name="description" content="{esc(desc)}">
<title>{esc(title)} · Ferrata LEZ test network</title>
<link rel="stylesheet" href="{css}">{js}
</head>
<body>
<a class="skip" href="#main">Skip to the content</a>
<header class="site"><div class="wrap">
<a class="brand" href="/">Ferrata LEZ test network</a>
<nav aria-label="Site"><ul>{''.join(items)}</ul></nav>
</div></header>
<main id="main" class="wrap" tabindex="-1">
{body.strip()}
</main>
<footer class="site"><div class="wrap">
<p>This page is static and its source is in the public repository: <a href="{REPO}/tree/main/deploy/testnet/web/trust">deploy/testnet/web/trust/</a>. Every file this host serves is listed with its SHA-256 and commit in <a href="/bridge/verify/">the site manifest</a>; check it from a clone with <code>python3 -I scripts/site-manifest.py verify</code>. No cookies, no analytics, no third-party scripts.</p>
</div></footer>
</body>
</html>
"""


def focusable_regions(body: str) -> str:
    """Wide tables and code blocks scroll sideways on a phone: each gets a tab stop and a name (WCAG 2.1.1; axe's
    scrollable-region-focusable)."""
    def wrap(m):
        cap = re.search(r"<caption>(.*?)</caption>", body[m.end():m.end() + 600], re.S)
        name = re.sub(r"<[^>]+>", "", cap.group(1)).strip() if cap else "Table"
        return f'<div class="table-wrap" tabindex="0" role="region" aria-label="{esc(name)}">'
    body = re.sub(r'<div class="table-wrap">', wrap, body)
    return body.replace("<pre>", '<pre tabindex="0">')


def short(h: str, n: int = 10) -> str:
    return h[:n] + "…" if len(h) > n else h


def apps_tables(reg: dict, rebuild: dict) -> str:
    src = {s["id"]: {**s, "public": public_commit(s["commit"])} for s in reg["sources"]}
    rb = {r["name"]: r for r in rebuild.get("programs", [])}
    rows = []
    for p in reg["programs"]:
        s = src[p["source"]]
        r = rb.get(p["name"], {})
        if r.get("ok") and not s["commit"]:
            rebuilt = (f'<span class="badge warn">rebuilt {esc(rebuild.get("when", "").split(",")[0])} from a source '
                       'not in public history</span>')
        elif r.get("ok"):
            rebuilt = f'<span class="badge ok">rebuilt: equal</span><br><span class="small">{esc(rebuild.get("when", ""))}</span>'
        elif r:
            rebuilt = '<span class="badge bad">rebuilt: differs</span>'
        else:
            rebuilt = '<span class="badge warn">not rebuilt yet</span>'
        idl = (f'<a href="{esc(p["idl"])}">{esc(p["idl"].rsplit("/", 1)[-1])}</a>' if p["idl"] else
               '<span class="muted">none published</span>')
        if not s["commit"]:
            commit = '<span class="badge warn">source not in public history</span>'
        elif s.get("public"):
            commit = f'<a href="{REPO}/tree/{s["commit"]}/{esc(p["path"])}"><code>{s["commit"][:8]}</code></a>'
        else:
            commit = f'<code>{s["commit"][:8]}</code> <span class="badge warn">not public yet</span>'
        rows.append(
            f'<tr id="p-{esc(p["name"])}" data-program="{esc(p["name"])}"><th scope="row">{esc(p["title"])}<br>'
            f'<code class="small">{esc(p["name"])}</code></th>'
            f'<td>{esc(p["what"])} <span class="muted small">({esc(p["rfp"])})</span></td>'
            f'<td><code title="{esc(p["image_id"])}">{esc(short(p["image_id"], 14))}</code><br>'
            f'<span class="small">binary <code title="{esc(p["binary_sha256"])}">{esc(short(p["binary_sha256"], 10))}</code></span></td>'
            f'<td>{commit} <code class="small">{esc(p["path"])}</code><br><span class="small muted">{esc(s["where"])}; LEZ {esc(s["lez_tag"])}</span></td>'
            f'<td>{idl}</td><td>{rebuilt}</td>'
            f'<td class="live" data-col="chain"><span class="muted small">needs JavaScript: reads the loader header</span></td>'
            f'<td class="live" data-col="name"><span class="muted small">needs JavaScript</span></td></tr>')
    return "\n".join(rows)


RUN = ROOT / "scripts/reproduce/runs/2026-11-23-deployed-testnet"


def rebuild_record() -> dict:
    """apps/rebuild.json, derived from the committed run (result.json, steps.txt, compare.txt): never written by hand."""
    res = json.loads((RUN / "result.json").read_text())
    steps = (RUN / "steps.txt").read_text().split("\n")
    t0, t1 = steps[0].split()[0], [x for x in steps if x.strip()][-1].split()[0]
    checked = next((x for x in (RUN / "compare.txt").read_text().splitlines() if x.startswith("checked ")), "")
    progs = [{"name": p["name"], "ok": bool(p.get("ok")) and p.get("rebuilt") == p["recorded"]
              and p.get("rebuilt_sha256") == p["binary_sha256"] and p.get("r0vm") == "= id"
              and p.get("chain_image_id") == p["recorded"] and p.get("immutable") is True} for p in res["programs"]]
    n = sum(x["ok"] for x in progs)
    return {"record": str(RUN.relative_to(ROOT)) + "/",
            "when": f"build 23 November 2026, {t0}–{t1} UTC (steps.txt); chain read {checked.split(';')[0].removeprefix('checked ')} (compare.txt)",
            "summary": (f"{n} of {len(progs)} deployed programs rebuilt from their source commits to the recorded image id and "
                        f"binary SHA-256, r0vm agreeing, each equal to its immutable loader header on this network"
                        + ("" if not res.get("not_in_list") else f"; deployed but not rebuilt: {', '.join(res['not_in_list'])}")
                        + "."),
            "programs": progs}


def public_commit(commit: str) -> bool:
    """Whether a remote-tracking ref of this clone holds the commit (anyone outside can then check against it)."""
    import subprocess
    r = subprocess.run(["git", "-C", str(ROOT), "for-each-ref", "--contains", commit, "--format=%(refname)", "refs/remotes"],
                       capture_output=True, text=True)
    return r.returncode == 0 and bool(r.stdout.strip())


def build() -> dict:
    out = {}
    reg = json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text())
    out["apps/registry.json"] = json.dumps(reg, indent=1) + "\n"
    rebuild = rebuild_record()
    out["apps/rebuild.json"] = json.dumps(rebuild, indent=1) + "\n"
    subs = {
        "{{APPS_ROWS}}": apps_tables(reg, rebuild),
        "{{REBUILD_WHEN}}": esc(rebuild.get("when", "not yet")),
        "{{REBUILD_SUMMARY}}": esc(rebuild.get("summary", "The rebuild has not been recorded yet.")),
    }
    for f in sorted(SRC.glob("*.html")):
        text = f.read_text()
        m = re.match(r"<!--\s*path:\s*(\S+)\s*\|\s*title:\s*(.+?)\s*\|\s*desc:\s*(.+?)\s*(?:\|\s*script:\s*(\S+)\s*)?-->\n", text)
        if not m:
            raise SystemExit(f"{f}: first line must be <!-- path: … | title: … | desc: … [| script: …] -->")
        path, title, desc, script = m.groups()
        body = text[m.end():]
        for k, v in subs.items():
            body = body.replace(k, v)
        left = re.findall(r"\{\{[A-Z_]+\}\}", body)
        if left:
            raise SystemExit(f"{f}: unknown placeholders {left}")
        out[path.strip("/") + "/index.html"] = page(path, title, desc, focusable_regions(body), script or "")
    return out


def main() -> int:
    check = "--check" in sys.argv
    out = build()
    bad = 0
    for rel, text in out.items():
        p = HERE / rel
        if check:
            if not p.exists() or p.read_text() != text:
                print(f"stale: {rel}")
                bad += 1
        else:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
    print(("checked " if check else "wrote ") + f"{len(out)} files" + (f", {bad} stale" if bad else ""))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
