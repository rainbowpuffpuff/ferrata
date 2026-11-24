#!/usr/bin/env python3
"""Check the build page: its counts equal the status document, every link it emits resolves, the bundle is fresh.

1. Counts. The requirement rows in index.html, counted by RFP and status, equal docs/requirements-status.md's own
   Counts table (read here independently of site/build.py); every requirement ID of docs/traceability.md appears
   exactly once (59 + 5 and 68 + 5 rows); the hero's "N of M" figures and build.json agree with them. The overview has
   one tile per row, linking to it, with the row's status as its class, its mark (the status's symbol) and its
   accessible name ("RFP-021 F.12, shown"); the legend's counts equal the tiles'.
2. Links. Every href and src of index.html, evidence.html and testers/index.html: in-page anchors exist; bundle files
   exist (a relative link is resolved from its page's directory); links into this repository name tracked files or
   directories at an existing line or heading; GitHub Actions links name real workflows; links into logos-co/rfp name
   the extracted commit and its files; Etherscan links name transactions and addresses of the recorded runs; the test
   network's paths are the ones its server serves; the report form is a tracked issue form; any other external URL is
   one the repository's own documents give. No http:// link, no inline script or style, no event-handler attribute.
   The build page links to the testers' page from its header and its hero; the testers' page loads no script.
3. Freshness. A build in memory equals the bundle on disk (deploy/testnet/web/build/).
4. Privacy. No e-mail address and no home-directory path in the bundle's text; no text or metadata chunk in its PNGs.
5. Sources. Every number of the testers' page names its source in a `<!-- source: PATH | TEXT -->` comment: PATH is
   tracked and holds TEXT (whitespace aside); the page has at least 30 such notes.
6. The rest: every row's status equals its row of docs/requirements-status.md; the proposal's payouts add up to its
   total; each status symbol has at least 3:1 contrast on its box in both themes (WCAG 1.4.11); `build.py --out`
   refuses a non-empty directory that is not a bundle and leaves files it did not write.

Usage: python3 -I site/test_site.py [--online] [--strict] [--skip-freshness]
  --online          also request every external URL once (HEAD, then GET), from this machine
  --strict          also fail on the generator's warnings (links in the documents that resolve to nothing)
  --skip-freshness  check a fresh build only, not the committed bundle (for CI: a push that changes a document then
                    fails nothing here; the bundle is rebuilt when it is deployed)
Standard library only.
"""

import html.parser
import importlib.util
import json
import pathlib
import posixpath
import re
import subprocess
import sys
import shutil
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "deploy" / "testnet" / "web" / "build"
REPO = "https://github.com/rainbowpuffpuff/ferrata"
TESTNET_PATHS = {"/explorer/", "/wallet/", "/agents", "/status", "/rpc", "/AGENTS.md", "/faucet", "/android/"}
STATUSES = ("shown", "in part", "not yet", "waits on Logos")
SKEY = {"shown": "shown", "in part": "part", "not yet": "notyet", "waits on Logos": "waits"}


def load_build():
    spec = importlib.util.spec_from_file_location("site_build", ROOT / "site" / "build.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class Page(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids, self.links, self.imgs, self.rows, self.problems = [], [], [], [], []
        self.scripts, self.tiles = [], []
        self._tile = False
        self.req_tiles, self.legend, self.uses = [], [], []
        self._in_tile = None
        self._in_legend = None

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.append(a["id"])
        for k in a:
            if k.startswith("on"):
                self.problems.append(f"<{tag}> has an event-handler attribute {k}")
        if "style" in a:
            self.problems.append(f"<{tag}> has an inline style attribute")
        if tag == "a" and "href" in a:
            self.links.append(a["href"])
        if tag in ("img", "script", "link") and ("src" in a or "href" in a):
            self.links.append(a.get("src") or a.get("href"))
        if tag == "img":
            self.imgs.append(a)
        if tag == "script":
            self.scripts.append(a)
        if tag == "tr" and "data-status" in a and "data-rfp" in a:
            self.rows.append(a)
        if tag == "p" and "sum-value" in (a.get("class") or "").split():
            self._tile = True
            self.tiles.append("")
        if tag == "use":
            self.uses.append(a.get("href", ""))
            if self._in_tile is not None:
                self._in_tile["marks"].append(a.get("href", ""))
            if self._in_legend is not None:
                self._in_legend["marks"].append(a.get("href", ""))
        if tag == "a" and "tile" in (a.get("class") or "").split():
            self._in_tile = {"attrs": a, "text": "", "marks": []}
            self.req_tiles.append(self._in_tile)
        if tag == "span" and (a.get("class") or "") == "lg" and "data-hl" in a:
            self._in_legend = {"hl": a["data-hl"], "text": "", "marks": []}
            self.legend.append(self._in_legend)

    def handle_endtag(self, tag):
        if tag == "p":
            self._tile = False
        if tag == "a":
            self._in_tile = None
        if tag == "li":
            self._in_legend = None

    def handle_data(self, data):
        if self._tile:
            self.tiles[-1] += data
        if self._in_tile is not None:
            self._in_tile["text"] += data
        if self._in_legend is not None:
            self._in_legend["text"] += data


def doc_counts():
    """docs/requirements-status.md's Counts table and its rows, read without site/build.py."""
    text = (ROOT / "docs" / "requirements-status.md").read_text(encoding="utf-8")
    counts = {}
    for m in re.finditer(r"^\| (RFP-02[12]) \| (\d+) \| (\d+) \| (\d+) \| (\d+) \| \*\*(\d+)\*\* \|$", text, re.M):
        counts[m.group(1)] = dict(zip(list(STATUSES) + ["total"], map(int, m.groups()[1:])))
    return counts


def traceability_ids():
    ids, rfp = [], None
    for line in (ROOT / "docs" / "traceability.md").read_text(encoding="utf-8").splitlines():
        m = re.match(r"^## (RFP-02[12]):", line)
        if m:
            rfp = m.group(1)
            continue
        if line.startswith("## "):
            rfp = None
        m = re.match(r"^\| ((?:VS|Sec|Priv|Soft|F|U|R|P|S)\.\d+) \|", line)
        if rfp and m:
            ids.append(f"{rfp.lower()}-{m.group(1).lower().replace('.', '-')}")
    return ids


def main(argv):
    online, strict, fresh = "--online" in argv, "--strict" in argv, "--skip-freshness" not in argv
    problems = []
    build = load_build()
    files, model, site = build.build()
    repo = site.repo

    # 3. Freshness
    for name, data in (files.items() if fresh else ()):
        disk = BUNDLE / name
        if not disk.is_file() or disk.read_bytes() != data:
            problems.append(f"freshness: {name} differs from a fresh build (run python3 -I site/build.py)")
    for p in (BUNDLE.rglob("*") if fresh else ()):
        if p.is_file() and p.relative_to(BUNDLE).as_posix() not in files:
            problems.append(f"freshness: {p.relative_to(BUNDLE)} is not built by site/build.py")

    pages = {}
    for name in ("index.html", "evidence.html", "testers/index.html"):
        pg = Page()
        pg.feed(files[name].decode("utf-8"))
        pages[name] = pg
        problems += [f"{name}: {p}" for p in pg.problems]
        dup = {i for i in pg.ids if pg.ids.count(i) > 1}
        if dup:
            problems.append(f"{name}: duplicate ids {sorted(dup)[:5]}")
        for s in pg.scripts:
            if "src" not in s and s.get("type") != "application/json":
                problems.append(f"{name}: an inline script")
        text = files[name].decode("utf-8")
        if 'http-equiv="Content-Security-Policy"' not in text:
            problems.append(f"{name}: no Content-Security-Policy")
        if not re.search(r'<html lang="en">', text):
            problems.append(f"{name}: no lang")
        for img in pg.imgs:
            if not img.get("alt") and img.get("alt") != "":
                problems.append(f"{name}: an image without alt: {img.get('src')}")
            if img.get("src", "").startswith(("img/", "assets/")) and not (img.get("width") and img.get("height")):
                problems.append(f"{name}: an image without its size: {img.get('src')}")

    # The testers' page: linked from the build page's header and hero; complete without JavaScript (no script at all);
    # each number's source holds it
    testers = files["testers/index.html"].decode("utf-8")
    head, _, rest = files["index.html"].decode("utf-8").partition('<main id="main">')
    hero = rest.split("</section>", 1)[0]
    if 'href="testers/"' not in head or 'href="testers/"' not in hero:
        problems.append("index.html: the header and the hero do not both link to testers/")
    if "<script" in testers:
        problems.append("testers/index.html: a script (the page must work without JavaScript)")
    notes = build.source_notes(site, testers)
    problems += [f"testers/index.html: source {p}: {why}: {s!r}" for p, s, why in notes if why]
    if len(notes) < 30:
        problems.append(f"testers/index.html: {len(notes)} source notes, fewer than 30")
    for fake, why in (("<!-- source: no/such/file.txt | 1 -->", "not a tracked file"),
                      ("<!-- source: README.md | 987654321 seconds -->", "no longer holds")):
        got = build.source_notes(site, fake)
        if not got or not got[0][2] or why not in got[0][2]:
            problems.append(f"self-check: the source note {fake!r} gave {got!r}, expected a refusal naming {why!r}")

    # 1. Counts
    idx = pages["index.html"]
    want = doc_counts()
    if set(want) != {"RFP-021", "RFP-022"}:
        problems.append(f"counts: the Counts table of requirements-status.md was not read ({want})")
    got = {}
    soft = {}
    for r in idx.rows:
        bucket = soft if r.get("data-section") == "Soft" else got
        bucket.setdefault(r["data-rfp"], {s: 0 for s in STATUSES})
        status = next(s for s in STATUSES if SKEY[s] == r["data-status"])
        bucket[r["data-rfp"]][status] += 1
    for rfp, w in want.items():
        g = got.get(rfp, {})
        if any(g.get(s, 0) != w[s] for s in STATUSES) or sum(g.values()) != w["total"]:
            problems.append(f"counts: {rfp} in the page {g}, in requirements-status.md {w}")
    if {k: sum(v.values()) for k, v in soft.items()} != {"RFP-021": 5, "RFP-022": 5}:
        problems.append(f"counts: soft rows {soft}")
    doc_status = {}
    rfp = section = None
    for line in (ROOT / "docs" / "requirements-status.md").read_text(encoding="utf-8").splitlines():
        m = re.match(r"^## (RFP-02[12]):", line)
        if m:
            rfp = m.group(1)
            continue
        m = re.match(r"^\| ((?:VS|Sec|Priv|Soft|F|U|R|P|S)\.\d+) \| (shown|in part|not yet|waits on Logos) \|", line)
        if rfp and m:
            doc_status[f"{rfp.lower()}-{m.group(1).lower().replace('.', '-')}"] = SKEY[m.group(2)]
    for r in idx.rows:
        if doc_status.get(r["id"]) != r["data-status"]:
            problems.append(f"counts: {r['id']} shows {r['data-status']}, the document says {doc_status.get(r['id'])}")
    row_ids = [r["id"] for r in idx.rows]
    trace = traceability_ids()
    if sorted(row_ids) != sorted(trace) or len(set(row_ids)) != len(row_ids):
        missing = sorted(set(trace) - set(row_ids))
        extra = sorted(set(row_ids) - set(trace))
        problems.append(f"counts: rows differ from traceability.md: missing {missing[:6]}, extra {extra[:6]}, "
                        f"{len(row_ids)} rows for {len(trace)} IDs")
    tiles = [re.sub(r"\s+", " ", t).strip() for t in idx.tiles]
    for rfp, tile in zip(("RFP-021", "RFP-022"), tiles):
        m = re.match(r"^(\d+) of (\d+) hard requirements shown$", tile)
        if not m or int(m.group(1)) != want[rfp]["shown"] or int(m.group(2)) != want[rfp]["total"]:
            problems.append(f"counts: the {rfp} tile says {tile!r}, the document {want[rfp]['shown']} of {want[rfp]['total']}")
    # The overview: one tile per row, its link, status, mark and name; the legend's counts
    by_row = {r["id"]: r for r in idx.rows}
    seen = set()
    tile_counts = {k: 0 for k in SKEY.values()}
    for t in idx.req_tiles:
        a = t["attrs"]
        req, href = a.get("data-req", ""), a.get("href", "")
        m = re.match(r"^(RFP-02[12])-((?:VS|Sec|Priv|Soft|F|U|R|P|S)\.\d+)$", req)
        if not m:
            problems.append(f"overview: a tile with data-req {req!r}")
            continue
        row_id = f"{m.group(1).lower()}-{m.group(2).lower().replace('.', '-')}"
        if req in seen:
            problems.append(f"overview: two tiles for {req}")
        seen.add(req)
        if href != "#" + row_id or row_id not in by_row:
            problems.append(f"overview: {req} links to {href!r}, not its row")
            continue
        status = by_row[row_id]["data-status"]
        classes = (a.get("class") or "").split()
        if f"t-{status}" not in classes:
            problems.append(f"overview: {req} has classes {classes}, its row's status is {status}")
        if t["marks"] != [f"#g-{status}"]:
            problems.append(f"overview: {req}'s mark is {t['marks']}, expected the symbol #g-{status}")
        word = next(w for w, k in SKEY.items() if k == status)
        name = re.sub(r"\s+", " ", t["text"]).strip()
        if name != f"{m.group(1)} {m.group(2)}, {word}":
            problems.append(f"overview: {req}'s accessible name is {name!r}")
        tile_counts[status] += 1
    if seen != {f"{r['data-rfp']}-{rid_from(r['id'])}" for r in idx.rows}:
        problems.append(f"overview: {len(seen)} tiles for {len(idx.rows)} rows")
    for lg in idx.legend:
        n = re.search(r"(\d+)\s*$", lg["text"].strip())
        if not n or int(n.group(1)) != tile_counts.get(lg["hl"], -1) or lg["marks"] != [f"#g-{lg['hl']}"]:
            problems.append(f"overview: legend {lg['hl']} says {lg['text'].strip()!r} with {lg['marks']}, the tiles {tile_counts.get(lg['hl'])}")
    if len(idx.legend) != 4:
        problems.append(f"overview: {len(idx.legend)} legend entries")
    for name, pg in pages.items():
        for use in set(pg.uses):
            if not use.startswith("#") or use[1:] not in pg.ids:
                problems.append(f"{name}: <use href={use!r}> names no symbol of the page")

    # The footer names the commit the page was built from and the commit of its statuses
    want_status = subprocess.run(["git", "-C", str(ROOT), "log", "-1", "--format=%H", "--", "docs/requirements-status.md"],
                                 capture_output=True, text=True, check=True).stdout.strip()
    text = files["index.html"].decode("utf-8")
    if f'data-statuses-from="{want_status}"' not in text or f'data-built-from="{model["stamp"]["commit"]}"' not in text:
        problems.append(f"footer: does not name the build's commit and the statuses' commit {want_status[:8]}")

    bj = json.loads(files["build.json"])
    for rfp in want:
        if bj["hard"][rfp] != want[rfp]:
            problems.append(f"counts: build.json {rfp} {bj['hard'][rfp]} != {want[rfp]}")

    # 2. Links
    proposal = json.loads((ROOT / "site" / "data" / "proposal-257.json").read_text(encoding="utf-8"))
    rfp_src = json.loads((ROOT / "site" / "data" / "rfp-requirements.json").read_text(encoding="utf-8"))["source"]
    comment_urls = {u["url"] for u in proposal["updates"]} | {proposal["url"]}
    txs, addrs = set(), set()
    for f in repo.files:
        if re.match(r"^e2e/runs/[^/]+/manifest\.json$", f):
            for stage in json.loads(repo.text(f)).get("stages", {}).values():
                txs.update(t["hash"].lower() for t in stage)
        if re.match(r"^e2e/runs/[^/]+/deploy\.json$", f):
            d = json.loads(repo.text(f))
            addrs.update(str(v).lower() for v in d.values() if re.match(r"^0x[0-9a-fA-F]{40}$", str(v)))
    workflows = {p.name for p in (ROOT / ".github" / "workflows").glob("*.yml")}
    # The documents an external URL may come from: the repository's own (outside site/ and the bundle) and the RFP text
    # the page quotes (site/data/rfp-requirements.json, from logos-co/rfp)
    sources = "\n".join([repo.text(p) for p in repo.files if p.endswith(".md") and not p.startswith(("site/", "deploy/"))]
                         + [repo.text("site/data/rfp-requirements.json")])
    external = set()
    checked = 0
    for name, pg in pages.items():
        ids = set(pg.ids)
        for href in pg.links:
            checked += 1
            err = check_link(href, name, ids, pages, files, repo, rfp_src, comment_urls, txs, addrs, workflows, sources, external)
            if err:
                problems.append(f"{name}: {href}: {err}")
    # L11: a link pinned to a commit names a file whose content there is the content the page was built from
    pinned = sorted({(c, p) for name, pg in pages.items() for href in pg.links
                     for c, p in re.findall(rf"^{re.escape(REPO)}/blob/([0-9a-f]{{40}})/([^#]+)", href)})
    if pinned:
        query = "\n".join([f"{c}:{p}" for c, p in pinned] + [f"HEAD:{p}" for c, p in pinned]) + "\n"
        oids = subprocess.run(["git", "-C", str(ROOT), "cat-file", "--batch-check=%(objectname)"], input=query,
                              capture_output=True, text=True, check=True).stdout.split("\n")
        for i, (c, p) in enumerate(pinned):
            if oids[i] != oids[i + len(pinned)] or "missing" in oids[i]:
                problems.append(f"links: {p} at {c[:8]} is not the file the page was built from")

    if online:
        for url in sorted(external):
            try:
                req = urllib.request.Request(url, method="HEAD", headers={"User-Agent": "ferrata-site-check"})
                code = urllib.request.urlopen(req, timeout=20).status
            except Exception:
                try:
                    code = urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "ferrata-site-check"}),
                                                  timeout=20).status
                except Exception as e:
                    problems.append(f"online: {url}: {e}")
                    continue
            if code >= 400:
                problems.append(f"online: {url}: HTTP {code}")

    # 4. Privacy
    for name, data in files.items():
        if not name.endswith((".html", ".json", ".js", ".css")):
            continue
        text = data.decode("utf-8")
        for m in re.finditer(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+\.[A-Za-z]{2,}", text):
            if not re.search(r"\.(png|svg|jpg|rs|md)$", m.group(0)):
                problems.append(f"privacy: {name}: an e-mail-like string {m.group(0)!r}")
        if re.search(r"/home/[a-z]|/Users/[A-Za-z]", text):
            problems.append(f"privacy: {name}: a home-directory path")

    for name, data in files.items():
        if name.endswith(".png"):
            pos, chunks = 8, []
            while pos + 8 <= len(data):
                size, kind = int.from_bytes(data[pos:pos + 4], "big"), data[pos + 4:pos + 8].decode("latin-1")
                chunks.append(kind)
                pos += 12 + size
            extra = set(chunks) - {"IHDR", "PLTE", "IDAT", "IEND", "tRNS", "sRGB", "gAMA", "cHRM", "pHYs", "sBIT", "bKGD"}
            if extra:
                problems.append(f"privacy: {name}: metadata chunks {sorted(extra)}")

    # The proposal's payouts add up to its total (site/snapshot_proposal.py's copy of the filed table)
    pays, total = [], None
    for line in proposal["sections"]["milestones"].splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")] if line.startswith("|") else []
        if len(cells) == 5 and cells[4].replace("*", "").startswith("$"):
            value = int(re.sub(r"[^\d]", "", cells[4]))
            if "Total" in cells[1]:
                total = value
            else:
                pays.append(value)
    if total is None or sum(pays) != total or f"${total:,}" not in proposal["bid"]:
        problems.append(f"proposal: payouts {pays} add to {sum(pays)}, the total row says {total}")

    # Contrast of each status symbol on its box (WCAG 1.4.11: 3:1 for graphics), light and dark
    css = files["assets/site.css"].decode("utf-8")
    dark = css.split("@media (prefers-color-scheme: dark)", 1)[1].split("\n}\n", 1)[0]
    light = css.split(":root {", 1)[1].split("\n}\n", 1)[0]

    def lum(hexv):
        rgb = [int(hexv[i:i + 2], 16) / 255 for i in (1, 3, 5)]
        rgb = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb]
        return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]

    for theme, block in (("light", light), ("dark", dark)):
        var = dict(re.findall(r"--([a-z-]+):\s*(#[0-9a-fA-F]{6})", block))
        for k in ("shown", "part", "waits", "notyet"):
            box, glyph = var.get(f"c-{k}"), var.get(f"g-{k}")
            if not box or not glyph:
                problems.append(f"contrast: {theme}: --c-{k} or --g-{k} missing")
                continue
            a, b = sorted((lum(box), lum(glyph)), reverse=True)
            if (a + 0.05) / (b + 0.05) < 3:
                problems.append(f"contrast: {theme}: the {k} symbol {glyph} on {box} is {(a + 0.05) / (b + 0.05):.2f}:1")

    # build.py --out: refuses a non-empty directory that holds no build.json; keeps files it did not write
    scratch = pathlib.Path(tempfile.mkdtemp(prefix=".test-out-", dir=ROOT / "site"))
    try:
        canary = scratch / "canary.txt"
        canary.write_text("not the bundle's")
        if build.main(["--out", str(scratch)]) == 0 or not canary.exists():
            problems.append("build.py --out: wrote into a non-empty directory without build.json")
        canary.unlink()
        if build.main(["--out", str(scratch)]) != 0:
            problems.append("build.py --out: an empty directory was refused")
        canary.write_text("written beside the bundle")
        if build.main(["--out", str(scratch)]) != 0 or not canary.exists():
            problems.append("build.py --out: a file it did not write was removed")
    finally:
        shutil.rmtree(scratch)

    # The link checker refuses what it must refuse (a check that passes everything checks nothing).
    bad = {
        "#no-such-id": "no such id", "evidence.html#e999999": "no id", "img/none.png": "not in the bundle",
        f"{REPO}/blob/main/no/such/file.md": "not a tracked file", f"{REPO}/blob/main/README.md#L999999": "past",
        f"{REPO}/blob/main/README.md#no-such-heading-here": "no such heading", f"{REPO}/tree/main/no-such-dir": "directory",
        f"https://github.com/logos-co/rfp/blob/{'0' * 40}/RFPs/x.md": "extracted commit",
        "https://sepolia.etherscan.io/tx/0x" + "ab" * 32: "recorded transaction", "http://example.com/": "plain http",
        "https://example.invalid/page": "no document", f"{REPO}/actions/workflows/none.yml": "no such workflow",
        "/elsewhere": "unrecognised", "../outside.html": "outside the bundle", "testers/#no-such-id": "no id",
        f"{REPO}/issues/new?template=none.yml": "no such issue form",
    }
    for href, why in bad.items():
        err = check_link(href, "index.html", set(idx.ids), pages, files, repo, rfp_src, comment_urls, txs, addrs, workflows,
                         sources, set())
        if not err or why not in err:
            problems.append(f"self-check: {href} gave {err!r}, expected a refusal naming {why!r}")

    if strict and site.warnings:
        problems += [f"warning: {w}" for w in site.warnings]
    print(f"test_site.py: {len(idx.req_tiles)} overview tiles; {len(idx.rows)} requirement rows; RFP-021 {want['RFP-021']['shown']} of {want['RFP-021']['total']}, "
          f"RFP-022 {want['RFP-022']['shown']} of {want['RFP-022']['total']} shown; {checked} links checked "
          f"({len(external)} other external URLs, each given by the repository's documents); {len(files)} files")
    for w in site.warnings:
        print(f"  generator warning: {w}")
    if problems:
        for p in problems:
            print(f"FAIL {p}", file=sys.stderr)
        print(f"test_site.py: {len(problems)} problem(s)", file=sys.stderr)
        return 1
    print("test_site.py: ok")
    return 0


def rid_from(row_id):
    """'rfp-021-f-12' -> 'F.12' (the ID as the tiles carry it)."""
    rest = row_id.split("-", 2)[2]
    prefix, num = rest.rsplit("-", 1)
    return {"f": "F", "u": "U", "r": "R", "p": "P", "s": "S", "vs": "VS", "sec": "Sec", "priv": "Priv", "soft": "Soft"}[prefix] + "." + num


def check_link(href, page, ids, pages, files, repo, rfp_src, comment_urls, txs, addrs, workflows, sources, external):
    if href.startswith("#"):
        return None if href[1:] in ids else "no such id on the page"
    if href.startswith("http://"):
        return "plain http"
    if href in TESTNET_PATHS:
        return None
    if not re.match(r"^[a-z]+:", href) and not href.startswith("/"):
        path, _, frag = href.partition("#")
        target = posixpath.normpath(posixpath.join(posixpath.dirname(page), path)) if path else page
        if target == ".." or target.startswith("../"):
            return "outside the bundle"
        if target == ".":
            target = "index.html"
        elif path.endswith("/"):
            target += "/index.html"
        if target in pages:
            return None if not frag or frag in pages[target].ids else f"no id {frag} in {target}"
        return None if target in files else "not in the bundle"
    m = re.match(rf"^{re.escape(REPO)}/(blob|tree)/(?:main|[0-9a-f]{{40}})/([^#]+?)(?:#(.+))?$", href)
    if m:
        kind, path, anchor = m.groups()
        if kind == "blob" and path not in repo.files:
            return "not a tracked file"
        if kind == "tree" and path not in repo.dirs:
            return "not a tracked directory"
        if anchor:
            lm = re.match(r"^L(\d+)(?:-L(\d+))?$", anchor)
            if lm:
                if int(lm.group(2) or lm.group(1)) > repo.lines(path):
                    return "line past the file's end"
            elif path.endswith(".md") and anchor not in repo.anchors(path):
                return "no such heading"
        return None
    if re.match(rf"^{re.escape(REPO)}/actions/runs/\d{{8,12}}$", href):
        return None
    m = re.match(rf"^{re.escape(REPO)}/actions/workflows/([\w.-]+\.yml)(?:/badge\.svg\?branch=main)?$", href)
    if m:
        return None if m.group(1) in workflows else "no such workflow"
    m = re.match(rf"^{re.escape(REPO)}/commit/([0-9a-f]{{40}})$", href)
    if m:
        ok = subprocess.run(["git", "-C", str(ROOT), "cat-file", "-e", m.group(1) + "^{commit}"], capture_output=True).returncode == 0
        return None if ok else "no such commit"
    if href in (REPO, "https://github.com/rainbowpuffpuff", "https://github.com/logos-co/rfp", REPO + "/security/advisories/new"):
        return None
    m = re.match(rf"^{re.escape(REPO)}/issues/new\?template=([\w.-]+\.yml)$", href)
    if m:
        return None if f".github/ISSUE_TEMPLATE/{m.group(1)}" in repo.files else "no such issue form"
    m = re.match(r"^https://github\.com/logos-co/rfp/blob/([0-9a-f]{40})/(RFPs/[^#]+)(?:#L(\d+))?$", href)
    if m:
        if m.group(1) != rfp_src["commit"]:
            return "not the extracted commit"
        return None if m.group(2) in rfp_src["files"] else "no such RFP file at that commit"
    if href in comment_urls:
        return None
    m = re.match(r"^https://sepolia\.etherscan\.io/(tx|address)/(0x[0-9a-fA-F]+)$", href)
    if m and href in sources:
        return None
    if m:
        if m.group(1) == "tx":
            return None if re.match(r"^0x[0-9a-f]{64}$", m.group(2)) and m.group(2) in txs else "not a recorded transaction"
        return None if m.group(2).lower() in addrs else "not a recorded address"
    if href.startswith("https://"):
        external.add(href)
        return None if href in sources else "an external URL no document of the repository gives"
    return "unrecognised link"


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
