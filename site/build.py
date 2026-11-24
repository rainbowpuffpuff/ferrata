#!/usr/bin/env python3
"""Build the bid's public build page from the repository, so it never drifts from the bid.

Reads docs/requirements-status.md (every row's status and reason), docs/traceability.md (each row's milestone),
docs/EVIDENCE.md (the evidence rows), scripts/ci/row-tests.toml with scripts/ci/check-row-tests.py's model of the
workflows (which tests CI really runs), site/data/rfp-requirements.json (the RFPs' own words), site/data/proposal-257.json
(the filed milestones, budget and timeline), site/content.toml (which runs and screenshots to feature) and the run
records it names, and site/testers.html (the testers' start page, written by hand: each of its numbers names the file it
comes from, which must still hold it); writes a static site to deploy/testnet/web/build/ (index.html, evidence.html,
testers/index.html, assets, images, build.json). No server code. At run time the page requests, besides its own files: github.com (four workflow badge
images), api.github.com (each workflow's latest run on main and the proposal's state: five calls, cached five
minutes), and its own host (/rpc, /status/status.json or /status.json, /status/programs.json or /programs.json,
/faucet).

The build is deterministic: it stamps the last commit that changed anything but the bundle, not the time of the
build, so `python3 -I site/test_site.py` can rebuild in memory and compare with the committed bundle.

Usage: python3 -I site/build.py [--out DIR] [--check]
  --check  build in memory and exit 1 if the bundle on disk differs (nothing is written)
Standard library only.
"""

import hashlib
import html
import importlib.util
import json
import math
import pathlib
import posixpath
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
OUT_REL = "deploy/testnet/web/build"
STATUSES = ("shown", "in part", "not yet", "waits on Logos")
SKEY = {"shown": "shown", "in part": "part", "not yet": "notyet", "waits on Logos": "waits"}
RFPS = ("RFP-021", "RFP-022")
RFP_NAMES = {"RFP-021": "Trustless Ethereum State Attestation", "RFP-022": "Private wrapped ERC-20 and ETH bridge"}
SECTION_ORDER = {
    "RFP-021": ["Functionality", "Usability", "Reliability", "Performance", "Supportability", "Verification Security"],
    "RFP-022": ["Functionality", "Usability", "Reliability", "Performance", "Supportability", "Security",
                "Privacy Preservation"],
}
MILESTONES = ("M0", "A1", "A2", "A3", "A4", "B1", "B2", "B3", "B4", "R")
TESTERS_SRC = "site/testers.html"          # the testers' start page's body, written by hand (its numbers cite sources)
TESTERS_DIR = "testers"                    # served at /testers/
ISSUE_FORM = ".github/ISSUE_TEMPLATE/tester-report.yml"
SOURCE_NOTE = re.compile(r"<!-- source: (\S+) \| (.+?) -->", re.S)
SEPOLIA = 11155111
ROW_ID = re.compile(r"^(VS|Sec|Priv|Soft|F|U|R|P|S)\.(\d+)$")
h = html.escape


def git(*args):
    return subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True, check=True).stdout


class Repo:
    """The tracked files, their line counts and their headings' GitHub anchors."""

    def __init__(self):
        if git("rev-parse", "--is-shallow-repository").strip() == "true":
            raise SystemExit("build.py: a shallow clone has no history for the footer's commits and the pinned links; "
                             "fetch the whole history (actions/checkout: fetch-depth: 0)")
        self.files = set(git("ls-files", "-z").split("\0")) - {""}
        self._pins = None
        self.dirs = {posixpath.dirname(f) for f in self.files}
        for d in list(self.dirs):
            while d:
                d = posixpath.dirname(d)
                self.dirs.add(d)
        self._lines, self._anchors = {}, {}

    def text(self, path):
        return (ROOT / path).read_text(encoding="utf-8")

    def pin(self, path):
        """The newest commit that changed `path` and holds it as HEAD does (None when there is none)."""
        if self._pins is None:
            head = {}
            for line in git("ls-tree", "-r", "HEAD").splitlines():
                meta, name = line.split("\t", 1)
                head[name] = meta.split()[2]
            first, cur = {}, None
            out = subprocess.run(["git", "-C", str(ROOT), "-c", "core.quotePath=false", "log", "--format=%x00%H", "--name-only",
                                  "--no-merges", "HEAD"], capture_output=True, text=True, check=True).stdout
            for line in out.splitlines():
                if line.startswith("\0"):
                    cur = line[1:]
                elif line and line not in first and line in head:
                    first[line] = cur
            queries = [f"{c}:{n}" for n, c in first.items()]
            oids = subprocess.run(["git", "-C", str(ROOT), "cat-file", "--batch-check=%(objectname)"],
                                  input="\n".join(queries) + "\n", capture_output=True, text=True, check=True).stdout.split()
            self._pins = {q.split(":", 1)[1]: q.split(":", 1)[0] for q, oid in zip(queries, oids) if oid == head[q.split(":", 1)[1]]}
        return self._pins.get(path)

    def lines(self, path):
        if path not in self._lines:
            self._lines[path] = len((ROOT / path).read_bytes().splitlines())
        return self._lines[path]

    def anchors(self, path):
        if path not in self._anchors:
            seen, out = {}, set()
            in_fence = False
            for line in self.text(path).splitlines():
                if line.lstrip().startswith("```"):
                    in_fence = not in_fence
                    continue
                m = re.match(r"^#{1,6}\s+(.*?)\s*#*\s*$", line)
                if m and not in_fence:
                    slug = github_slug(m.group(1))
                    n = seen.get(slug, 0)
                    seen[slug] = n + 1
                    out.add(slug if n == 0 else f"{slug}-{n}")
            self._anchors[path] = out
        return self._anchors[path]


def github_slug(heading):
    """GitHub's anchor for a markdown heading (github-slugger): lower case, markup and punctuation dropped."""
    text = re.sub(r"`([^`]*)`", r"\1", heading)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"[*~]", "", text)
    text = text.lower()
    text = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE)
    return text.replace(" ", "-")


def split_cells(line):
    """The cells of a markdown table row; a | inside a code span or escaped as \\| does not split."""
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|") and not s.endswith("\\|"):
        s = s[:-1]
    cells, buf, in_code = [], [], 0
    i = 0
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s) and s[i + 1] == "|":
            buf.append("|")
            i += 2
            continue
        if c == "`":
            j = i
            while j < len(s) and s[j] == "`":
                j += 1
            ticks = j - i
            if in_code == 0:
                in_code = ticks
            elif in_code == ticks:
                in_code = 0
            buf.append(s[i:j])
            i = j
            continue
        if c == "|" and not in_code:
            cells.append("".join(buf).strip())
            buf = []
        else:
            buf.append(c)
        i += 1
    cells.append("".join(buf).strip())
    return cells


def first_sentence(text, limit=260):
    """The first sentence of plain or markdown text (a period followed by a space and a capital or end), its
    markup balanced when the sentence ends inside bold or code."""
    t = text.strip()
    s = t
    for m in re.finditer(r"\.(?=\s+[A-Z(`\"*]|\s*$)", t):
        cand = t[:m.end()]
        if cand.count("(") <= cand.count(")") and cand.count("[") <= cand.count("]") and len(cand) >= 25:
            s = cand
            break
    if len(s) > limit:
        cut = s[:limit].rsplit(" ", 1)[0]
        s = cut.rstrip(",;:") + " …"
    if len(re.findall(r"(?<!`)`(?!`)", s)) % 2:
        s = s.replace(" …", "") + "` …" if s.endswith(" …") else s + "`"
    if s.count("**") % 2:
        s = s[:-2] + "** …" if s.endswith(" …") else s + "**"
    return s


def split_first(text, limit):
    """(the first sentence for display, the rest of the text): the whole text as the rest when the sentence was cut."""
    first = first_sentence(text, limit)
    if first.endswith(" …"):
        return first, text.strip()
    core = first.rstrip("`*")
    t = text.strip()
    rest = t[len(core):].strip() if t.startswith(core) else t
    return first, rest


def plain(md):
    """Markdown inline -> plain text (for titles, tooltips and aria labels)."""
    t = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", md)
    t = re.sub(r"`+([^`]+)`+", r"\1", t)
    t = t.replace("**", "")
    return re.sub(r"\s+", " ", t).strip()


class Md:
    """Inline markdown -> HTML. Repository paths become links to GitHub; every emitted link is recorded."""

    TOKEN = re.compile(
        r"(?P<tick>`+)(?P<code>.+?)(?P=tick)"
        r"|\[(?P<ltext>[^\]]+)\]\((?P<lurl>[^)\s]+)\)"
        r"|\*\*(?P<bold>.+?)\*\*"
        r"|(?P<url>https?://[^\s)<>`\"]+[^\s)<>`\".,;:])"
    )
    PATH = re.compile(r"^(?P<path>[A-Za-z0-9_.@+\-][A-Za-z0-9_.@+\-/]*?)(?::(?P<l1>\d+)(?:-(?P<l2>\d+))?)?$")

    def __init__(self, site, doc, upstream=None):
        self.site, self.doc = site, doc
        self.base = posixpath.dirname(doc) if doc else ""
        self.upstream = upstream  # (repository URL, commit, directory, files) for text quoted from another repository

    def repo_url(self, path, anchor=None):
        """GitHub URL of a tracked file or directory, or None. Anchors are kept only when they exist."""
        repo = self.site.repo
        path = path.rstrip("/") if path not in ("", "/") else ""
        if path in repo.files:
            url = f"{self.site.gh}/blob/{repo.pin(path) or self.site.ref}/{path}"
            if anchor:
                m = re.match(r"^L(\d+)(?:-L(\d+))?$", anchor)
                if m:
                    if int(m.group(2) or m.group(1)) <= repo.lines(path):
                        url += "#" + anchor
                    else:
                        self.site.warn(f"{self.doc}: {path}#{anchor}: past the file's end")
                elif path.endswith(".md"):
                    if anchor in repo.anchors(path):
                        url += "#" + anchor
                    else:
                        self.site.warn(f"{self.doc}: {path}#{anchor}: no such heading")
            return url
        if path in repo.dirs:
            return f"{self.site.gh}/tree/{self.site.ref}/{path}" if path else self.site.gh
        return None

    def link_target(self, url):
        if re.match(r"^https?://", url):
            return url
        if url.startswith("mailto:"):
            return None
        if self.upstream:
            repo, commit, base, files = self.upstream
            path, _, anchor = url.partition("#")
            resolved = posixpath.normpath(posixpath.join(base, path))
            if resolved in files:
                return f"{repo}/blob/{commit}/{resolved}" + (f"#{anchor}" if anchor else "")
            self.site.warn(f"upstream text: link to {url!r} names no file of {repo} at {commit[:8]}")
            return None
        path, _, anchor = url.partition("#")
        if not path:
            return self.repo_url(self.doc, anchor or None) if self.doc else None
        resolved = posixpath.normpath(posixpath.join(self.base, path)) if self.base else posixpath.normpath(path)
        target = self.repo_url(resolved, anchor or None)
        if target is None:
            self.site.warn(f"{self.doc}: link to {url!r} resolves to no tracked path")
        return target

    def code(self, body):
        inner = body.strip()
        m = self.PATH.match(inner)
        if m and ("/" in m.group("path") or m.group("path") in self.site.repo.files):
            path = m.group("path")
            anchor = None
            if m.group("l1"):
                anchor = f"L{m.group('l1')}" + (f"-L{m.group('l2')}" if m.group("l2") else "")
            url = self.repo_url(path, anchor)
            if url:
                return f'<a class="path" href="{h(url)}"><code>{h(body)}</code></a>'
        return f"<code>{h(body)}</code>"

    def text(self, s, evidence=False):
        out = h(s, quote=False)
        if evidence:
            def ev(m):
                n = int(m.group(0))
                if n in self.site.evidence:
                    return f'<a class="ev" href="evidence.html#e{n}" data-ev="{n}">{n}</a>'
                return m.group(0)

            def run(m):
                return f'<a href="{self.site.gh}/actions/runs/{m.group(0)}">{m.group(0)}</a>'

            out = re.sub(r"(?<![\w.,-])\d{10,12}(?![\w])", run, out)
            out = re.sub(r"(?<![\w.,/#§–—-])\d{1,3}(?![\w%–—]|,\d|\s(?:Nov|Dec|Oct))", ev, out)
        return out

    def render(self, md, evidence=False, links=True):
        parts, pos = [], 0
        for m in self.TOKEN.finditer(md):
            parts.append(self.text(md[pos:m.start()], evidence))
            pos = m.end()
            if m.group("tick"):
                parts.append(self.code(m.group("code")) if links else f"<code>{h(m.group('code'))}</code>")
            elif m.group("ltext") is not None:
                inner = self.render(m.group("ltext"), links=False)
                target = self.link_target(m.group("lurl")) if links else None
                parts.append(f'<a href="{h(target)}">{inner}</a>' if target else inner)
            elif m.group("bold") is not None:
                parts.append(f"<strong>{self.render(m.group('bold'), evidence, links)}</strong>")
            elif m.group("url"):
                url = m.group("url")
                parts.append(f'<a href="{h(url)}">{h(url)}</a>' if links else h(url))
        parts.append(self.text(md[pos:], evidence))
        return "".join(parts)

    def block(self, md):
        """Paragraphs and bullet lists (one level) -> HTML."""
        out, para, items = [], [], []

        def flush():
            if para:
                out.append(f"<p>{self.render(' '.join(para))}</p>")
                para.clear()
            if items:
                out.append("<ul>" + "".join(f"<li>{self.render(i)}</li>" for i in items) + "</ul>")
                items.clear()

        for line in md.splitlines():
            s = line.strip()
            if not s:
                flush()
            elif re.match(r"^[-*]\s+", s):
                if para:
                    flush()
                items.append(re.sub(r"^[-*]\s+", "", s))
            elif items and line.startswith((" ", "\t")):
                items[-1] += " " + s
            else:
                if items:
                    flush()
                para.append(s)
        flush()
        return "".join(out)


class Site:
    def __init__(self):
        self.repo = Repo()
        self.content = tomllib.loads((SITE / "content.toml").read_text(encoding="utf-8"))
        self.gh = self.content["site"]["repo"]
        self.ref = self.content["site"]["ref"]
        self.warnings = []
        self.css_flex, self.css_grid, self.css_fit = set(), set(), set()
        self.evidence = {}
        self.images = {}

    def warn(self, msg):
        if msg not in self.warnings:
            self.warnings.append(msg)

    def md(self, doc):
        return Md(self, doc)

    def src(self, path):
        """GitHub URL of a tracked path, which must exist."""
        url = Md(self, None).repo_url(path)
        if url is None:
            raise SystemExit(f"build.py: {path} is not tracked")
        return url


# ---------------------------------------------------------------------------------------------------------- model

def parse_doc_rows(text, columns):
    """Rows of the per-RFP matrices of a status-style document: (rfp, section, cells)."""
    rfp = section = None
    out = []
    for line in text.splitlines():
        m = re.match(r"^##\s+(RFP-02[12]):", line)
        if m:
            rfp, section = m.group(1), None
            continue
        if re.match(r"^##\s", line):
            rfp, section = None, None
            continue
        m = re.match(r"^###\s+(.*?)\s*$", line)
        if m and rfp:
            section = m.group(1)
            continue
        if rfp and section and line.startswith("|"):
            cells = split_cells(line)
            if ROW_ID.match(cells[0]) and len(cells) >= columns:
                out.append((rfp, section, cells))
    return out


def load_requirements(site):
    status_doc = "docs/requirements-status.md"
    text = site.repo.text(status_doc)
    rfp_words = json.loads((SITE / "data" / "rfp-requirements.json").read_text(encoding="utf-8"))
    trace = {}
    for rfp, section, cells in parse_doc_rows(site.repo.text("docs/traceability.md"), 7):
        trace[(rfp, cells[0])] = {"paraphrase": cells[1], "milestone": cells[-1]}
    rows = []
    for rfp, section, cells in parse_doc_rows(text, 4):
        rid, status, why, evidence = cells[:4]
        if status not in STATUSES:
            raise SystemExit(f"build.py: {rfp} {rid}: unknown status {status!r}")
        soft = section == "Soft requirements"
        words = rfp_words["rfps"][rfp]["rows"].get(rid)
        if words is None:
            raise SystemExit(f"build.py: {rfp} {rid}: not in site/data/rfp-requirements.json")
        t = trace.get((rfp, rid))
        if t is None:
            raise SystemExit(f"build.py: {rfp} {rid}: not in docs/traceability.md")
        ms = milestones_of(t["milestone"])
        rows.append({
            "rfp": rfp, "section": "Soft" if soft else section, "id": rid, "soft": soft, "status": status,
            "why": why, "evidence": evidence, "words": words["text"], "words_line": words["line"],
            "milestone_raw": t["milestone"], "milestones": ms, "tests": [], "category": "soft" if soft else "none",
            "note": "",
        })
    counts_doc = parse_counts(text)
    return rows, counts_doc, rfp_words


def milestones_of(raw):
    head = re.sub(r"\(.*?\)", "", raw)
    found = []
    for m in re.finditer(r"\b([AB])(\d)\s*[–-]\s*(?:[AB])?(\d)\b", head):
        for k in range(int(m.group(2)), int(m.group(3)) + 1):
            found.append(f"{m.group(1)}{k}")
    head = re.sub(r"\b[AB]\d\s*[–-]\s*[AB]?\d\b", "", head)
    found += re.findall(r"\b(M0|A[1-4]|B[1-4]|R)\b", head)
    return [m for m in MILESTONES if m in found]


def parse_counts(text):
    """The document's own Counts table: {rfp: {status: n, 'total': n}}."""
    out = {}
    block = text.split("## Counts", 1)[1].split("##", 1)[0]
    for line in block.splitlines():
        cells = split_cells(line) if line.startswith("|") else []
        if cells and cells[0] in RFPS and len(cells) == 6:
            nums = [int(c.strip("*")) for c in cells[1:]]
            out[cells[0]] = dict(zip(list(STATUSES) + ["total"], nums))
    return out


def load_row_tests(site, rows):
    spec = importlib.util.spec_from_file_location("check_row_tests", ROOT / "scripts" / "ci" / "check-row-tests.py")
    crt = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(crt)
    runs = crt.Runs(crt.ci_model.tracked_paths())
    with open(ROOT / "scripts" / "ci" / "row-tests.toml", "rb") as f:
        entries = tomllib.load(f).get("row", [])
    by_id = {f"{r['rfp']} {r['id']}": r for r in rows if not r["soft"]}
    for block in entries:
        row = by_id.get(block.get("id"))
        if row is None:
            raise SystemExit(f"build.py: row-tests.toml names {block.get('id')!r}, not a hard row")
        if block.get("note"):
            row["note"] = block["note"]
        for t in block.get("tests", []):
            path, kind, ci = t.get("path"), t.get("kind"), t.get("ci")
            entry = {"path": path, "kind": kind, "ci": ci, "run": t.get("run", ""), "where": kind, "workflow": ""}
            if block.get("branch"):
                entry["where"] = "pending"
            elif kind == "test":
                if ci in ("box", "local"):
                    entry["where"] = ci
                elif ci in runs.jobs:
                    ok, why = runs.check(ci, path)
                    if not ok:
                        raise SystemExit(f"build.py: {block['id']}: {path}: job {ci} does not run it: {why}")
                    entry["where"] = "ci" if runs.jobs[ci].on_change else "manual"
                    entry["workflow"] = runs.jobs[ci].workflow.relative_to(ROOT).as_posix()
                else:
                    raise SystemExit(f"build.py: {block['id']}: {path}: unknown job {ci!r}")
            row["tests"].append(entry)
    for row in by_id.values():
        wheres = {t["where"] for t in row["tests"]}
        row["category"] = ("ci" if "ci" in wheres else "manual" if "manual" in wheres else
                           "box" if wheres & {"box", "local"} else "records" if wheres & {"record", "document"} else
                           "pending" if "pending" in wheres else "none")


def load_evidence(site):
    text = site.repo.text("docs/EVIDENCE.md")
    header = None
    for line in text.splitlines():
        if not line.startswith("|"):
            header = None if not line.strip() else header
            continue
        cells = split_cells(line)
        if cells[0] == "#":
            header = cells
            continue
        if header and re.match(r"^\d+$", cells[0]) and len(cells) == len(header):
            n = int(cells[0])
            row = dict(zip(["n", "what", "numbers", "where", "proofs", "recheck", "details"], cells))
            what = row["what"]
            m = re.match(r"^\*\*(.+?)\*\*", what)
            title = plain(m.group(1)) if m else plain(first_sentence(what, 200))
            row["title"] = title.rstrip(":")
            site.evidence[n] = row
    if not site.evidence:
        raise SystemExit("build.py: no evidence rows read from docs/EVIDENCE.md")
    intro = text.split("\n## ", 1)[0].split("\n", 2)[2]
    return intro.strip()


def load_proposal(site):
    p = json.loads((SITE / "data" / "proposal-257.json").read_text(encoding="utf-8"))
    ms_md = p["sections"]["milestones"]
    table, gates, paras = [], {}, []
    current = None
    for line in ms_md.splitlines():
        if line.startswith("|"):
            cells = split_cells(line)
            if cells and not set(cells[0]) <= set("-: ") and cells[0] != "#":
                table.append(cells)
            continue
        m = re.match(r"^\*\*(M0|A[1-4]|B[1-4]|R)\.\*\*\s*(.*)$", line)
        if m:
            current = m.group(1)
            gates[current] = [m.group(2)] if m.group(2) else []
            continue
        if line.startswith("The done gates below"):
            paras.append(line)
            current = None
            continue
        if current and line.strip():
            gates[current].append(line)
        elif not line.strip():
            if current and current != "M0":
                current = None
        elif not current:
            paras.append(line)
    p["table"], p["gates"], p["paras"] = table, {k: "\n".join(v) for k, v in gates.items()}, paras
    return p


def load_security(site):
    path = "docs/security/risc0-groth16-advisory-2026-11-21/README.md"
    text = site.repo.text(path)
    lines = text.splitlines()
    title = lines[0].lstrip("# ").strip()
    quote = " ".join(l[1:].strip() for l in lines if l.startswith(">"))
    body = text.split("\n\n")
    paras = [b for b in body if b and not b.startswith(("#", ">"))]
    corr = text.split("## Correction, 23 November 2026", 1)[1].split("\n## ", 1)[0].strip().split("\n\n")
    return {"path": path, "title": title, "quote": quote, "paras": paras[:2], "correction": corr[:2]}


def load_not_built(site):
    text = site.repo.text("README.md")
    sec = text.split("## Specified but not built", 1)[1].split("\n## ", 1)[0].strip()
    return sec


def load_runs(site):
    runs = []
    for run in site.content.get("run", []):
        r = dict(run)
        for n in r.get("evidence", []):
            if n not in site.evidence:
                raise SystemExit(f"build.py: run {r['id']}: no evidence row {n}")
        for label in r.get("labels", []):
            if re.search(r"proof|fake|dev.?mode", label, re.I):
                raise SystemExit(f"build.py: run {r['id']}: the label {label!r} speaks of proofs; the Proofs cell of its "
                                 "evidence row gives that label")
        for key in ("record", "txs", "deploy", "stages", "lez_txs"):
            if key in r and r[key] not in site.repo.files:
                raise SystemExit(f"build.py: run {r['id']}: {r[key]} is not tracked")
        if "txs" in r:
            man = json.loads(site.repo.text(r["txs"]))
            if man.get("chainId") != SEPOLIA:
                raise SystemExit(f"build.py: {r['txs']}: not Sepolia")
            r["tx_rows"] = []
            for stage, txs in man["stages"].items():
                for tx in txs:
                    if not re.match(r"^0x[0-9a-fA-F]{64}$", tx["hash"]):
                        raise SystemExit(f"build.py: {r['txs']}: bad hash {tx['hash']!r}")
                    r["tx_rows"].append({
                        "stage": stage, "hash": tx["hash"].lower(), "block": int(tx["blockNumber"], 16),
                        "ok": tx.get("status") == "0x1", "gas": int(tx["gasUsed"], 16),
                    })
        if "deploy" in r:
            dep = json.loads(site.repo.text(r["deploy"]))
            r["contracts"] = [(k, dep[k]) for k in ("vault", "registry", "token", "emergencyStop", "groth16Verifier")
                              if re.match(r"^0x[0-9a-fA-F]{40}$", str(dep.get(k, "")))]
        if "stages" in r:
            r["stage_rows"] = stage_table(site.repo.text(r["stages"]))
            if not r["stage_rows"]:
                raise SystemExit(f"build.py: {r['stages']}: no '| Stage | Result | Time |' table")
        if "lez_txs" in r:
            lines = site.repo.text(r["lez_txs"]).splitlines()
            head = lines[0].split("\t")
            r["lez_rows"] = [dict(zip(head, l.split("\t"))) for l in lines[1:] if l.strip()]
        runs.append(r)
    return runs


def proofs_label(cell):
    """The proofs label of a run, from its evidence row's Proofs cell: real, dev mode or none, as the cell says."""
    t = plain(cell).lower()
    if t.startswith("real"):
        return "proofs: real" + ("; stand-ins named below" if re.search(r"stand-?in|mock|warp", t) else "")
    if t.startswith(("dev mode", "dev-mode")) or (t.startswith("none") and "dev mode" in t):
        return "proofs: dev mode (not real)"
    if t.startswith("none"):
        return "proofs: none"
    return "proofs: " + first_sentence(plain(cell), 60)


def stage_table(text):
    rows, on = [], False
    for line in text.splitlines():
        if re.match(r"^\|\s*Stage\s*\|\s*Result\s*\|\s*Time\s*\|", line):
            on = True
            continue
        if on:
            if not line.startswith("|"):
                break
            cells = split_cells(line)
            if set("".join(cells)) <= set("-: "):
                continue
            rows.append(cells[:3] + [""] * (3 - len(cells[:3])))
    return rows


# ----------------------------------------------------------------------------------------------------- rendering

ICONS = """<svg class="defs" aria-hidden="true" focusable="false"><defs>
<symbol id="g-shown" viewBox="0 0 16 16"><path d="M3.6 8.4l2.9 2.9 5.9-6.3" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"/></symbol>
<symbol id="g-part" viewBox="0 0 16 16"><path d="M4 8h8" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/></symbol>
<symbol id="g-notyet" viewBox="0 0 16 16"><path d="M4.6 4.6l6.8 6.8M11.4 4.6l-6.8 6.8" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round"/></symbol>
<symbol id="g-waits" viewBox="0 0 16 16"><path d="M4.5 2.5h7M4.5 13.5h7M5.4 2.7c0 2.6 5.2 3 5.2 5.3s-5.2 2.7-5.2 5.3M10.6 2.7c0 2.6-5.2 3-5.2 5.3s5.2 2.7 5.2 5.3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></symbol>
<symbol id="i-copy" viewBox="0 0 16 16"><rect x="5" y="5" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M3 11V3a1 1 0 0 1 1-1h7" fill="none" stroke="currentColor" stroke-width="1.4"/></symbol>
</defs></svg>"""


def box(status):
    """The status mark: a box in the status colour holding the status symbol (check, dash, cross, hourglass)."""
    k = SKEY[status]
    return (f'<span class="box b-{k}" aria-hidden="true"><svg viewBox="0 0 16 16" focusable="false">'
            f'<use href="#g-{k}"/></svg></span>')


def chip(status):
    return f'<span class="st st-{SKEY[status]}">{box(status)}{h(status)}</span>'


def short_hash(x, keep=10):
    return x if len(x) <= 2 * keep + 1 else f"{x[:keep]}…{x[-6:]}"


def flex(site, n):
    site.css_flex.add(n)
    return f"g{n}"


# A count inside its part of a bar needs this much of the part's width, in px at a 16 px root: the part's left
# padding (7), the label's padding (2 x .3rem) and a margin, plus each digit (bold, at .82rem, rounded up).
LABEL_PX, DIGIT_PX = 20, 9


def label_fit(site, n, share, parts):
    """The class that shows count n inside its part once the bar is wide enough: fitN, for a container query on the
    bar's width (N rem); None when no bar on the page is ever that wide."""
    need = (LABEL_PX + DIGIT_PX * len(str(n))) / share + 2 * (parts - 1)
    rem = math.ceil(need / 16)
    if rem > 80:
        return None
    site.css_fit.add(rem)
    return f"fit{rem}"


def seg_bar(site, label, counts, total, compact=False):
    """A segmented bar (one whole split into the four statuses), its legend with every count, and its accessible name.
    In the tall bar a part also carries its count, shown only while the bar is wide enough for it (label_fit)."""
    parts, legend = [], []
    aria = f"{label}: " + ", ".join(f"{counts[s]} {s}" for s in STATUSES if counts[s]) + f", of {total}"
    present = [s for s in STATUSES if counts[s]]
    for s in present:
        n = counts[s]
        share = n / total
        tip = f"{n} {s} · {share:.0%} of {total}"
        fit = None if compact else label_fit(site, n, share, len(present))
        parts.append(
            f'<span class="seg-part st-{SKEY[s]} {flex(site, n)}{" " + fit if fit else ""}" tabindex="0" role="img" '
            f'aria-label="{h(tip)}" data-tip="{h(tip)}">{f"<span class=seg-n>{n}</span>" if fit else ""}</span>')
        legend.append(f'<li>{box(s)}{h(s)} <b>{n}</b></li>')
    return (f'<div class="seg{" seg-thin" if compact else ""}" role="group" aria-label="{h(aria)}">{"".join(parts)}</div>'
            f'<ul class="legend{" legend-compact" if compact else ""}" aria-hidden="true">{"".join(legend)}</ul>')


def count_rows(rows):
    c = {s: 0 for s in STATUSES}
    for r in rows:
        c[r["status"]] += 1
    return c


def page(site, title, body, desc, extra_head="", root="", script=True):
    """A page of the bundle; `root` leads from the page's directory to the bundle's (testers/ -> "../"). Without
    `script` the page loads no script at all (and needs none)."""
    csp = ("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https://github.com "
           "https://api.github.com; connect-src 'self' https://api.github.com; base-uri 'none'; form-action 'none'; "
           "object-src 'none'")
    js = f'<script src="{root}assets/site.js" defer></script>\n' if script else ""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="{csp}">
<meta name="referrer" content="strict-origin-when-cross-origin">
<title>{h(title)}</title>
<meta name="description" content="{h(desc)}">
<meta property="og:title" content="{h(title)}">
<meta property="og:description" content="{h(desc)}">
<meta name="color-scheme" content="light dark">
<link rel="icon" href="{root}assets/logo.svg" type="image/svg+xml">
<link rel="stylesheet" href="{root}assets/site.css">
{extra_head}</head>
<body>
{ICONS if script else ""}
{body}
{js}</body>
</html>
"""


def render_index(site, model):
    c = site.content
    rows, counts_by, stamp = model["rows"], model["counts"], model["stamp"]
    md = site.md("docs/requirements-status.md")
    parts = []
    nav = [("what", "What it is"), ("live", "Live"), ("requirements", "Requirements"), ("runs", "What runs"),
           ("milestones", "Milestones"), ("security", "Security"), ("try", "Try it"), ("not-done", "Not done yet")]
    parts.append('<a class="skip" href="#main">Skip to content</a>')
    parts.append('<header class="top"><div class="wrap top-in"><a class="brand" href="#main" aria-label="Ferrata, top of the page">'
                 '<img src="assets/logo.svg" alt="" width="28" height="28"><span>Ferrata</span></a>'
                 '<nav aria-label="Sections"><ul>' +
                 "".join(f'<li><a href="#{k}">{h(v)}</a></li>' for k, v in nav) + "</ul></nav>"
                 f'<a class="top-cta" href="{TESTERS_DIR}/">Testers<span class="top-cta-more">: start here</span></a>'
                 "</div></header>")
    parts.append('<main id="main">')

    # Hero
    hard = {rfp: [r for r in rows if r["rfp"] == rfp and not r["soft"]] for rfp in RFPS}
    soft = {rfp: [r for r in rows if r["rfp"] == rfp and r["soft"]] for rfp in RFPS}
    p = model["proposal"]
    parts.append('<section class="hero wrap" aria-labelledby="h-top">')
    parts.append(f'<p class="eyebrow">{h(c["site"]["bid"])}</p>')
    parts.append(f'<h1 id="h-top">Ferrata <span class="sub">{h(c["site"]["tagline"])}</span></h1>')
    parts.append('<ul class="chips" aria-label="Status at a glance">')
    parts.append(f'<li><a class="chip" id="chip-proposal" href="{h(p["url"])}"><span class="dot dot-open" aria-hidden="true"></span>'
                 f'Proposal <b data-live="issue-state">{h(p["state"].lower())}</b> · logos-co/rfp #257</a></li>')
    parts.append('<li><span class="chip"><span class="dot dot-rc" aria-hidden="true"></span>Runs on LEZ v0.4.0-rc3 · zones we run</span></li>')
    parts.append('<li><a class="chip" href="#live-net"><span class="dot" data-live="net-dot" aria-hidden="true"></span>'
                 'Test network <b data-live="net-chip">status loads with JavaScript</b></a></li>')
    parts.append('<li><a class="chip" href="#live-ci"><span class="dot" data-live="ci-dot" aria-hidden="true"></span>'
                 'CI on main <b data-live="ci-chip">see below</b></a></li>')
    parts.append('<li><a class="chip chip-warn" href="#security"><span class="dot dot-warn" aria-hidden="true"></span>Not audited</a></li>')
    parts.append("</ul>")
    parts.append(f'<p class="testers-cta"><a href="{TESTERS_DIR}/">Testers: start here</a> <span>What to try on the test '
                 'network, in order, what each step should show, and how to report what you find.</span></p>')

    # One card per RFP: its name, the count shown, the bar of its four statuses and their counts (rows aligned
    # across the two cards)
    parts.append('<div class="sums">')
    for rfp in RFPS:
        cnt = count_rows(hard[rfp])
        total = len(hard[rfp])
        slug = rfp.lower()
        parts.append(f'<div class="sum" role="group" aria-labelledby="sum-{slug}">'
                     f'<p class="sum-name" id="sum-{slug}"><b>{rfp}</b> {h(RFP_NAMES[rfp])}</p>'
                     f'<p class="sum-value"><b>{cnt["shown"]}</b><span> of {total} hard requirements shown</span></p>'
                     + seg_bar(site, f"{rfp} hard requirements", cnt, total) + "</div>")
    parts.append("</div>")
    softline = "; ".join(
        f"{rfp}: " + ", ".join(f"{n} {s}" for s, n in count_rows(soft[rfp]).items() if n) for rfp in RFPS)
    parts.append(f'<p class="soft">Soft requirements, counted apart ({len(soft["RFP-021"])} for each RFP): {h(softline)}.</p>')
    definition = model["definition"]
    parts.append(f'<details class="def"><summary>What “shown” means here: strict on purpose</summary>{md.block(definition)}'
                 f'<p>Source: <a href="{h(site.src("docs/requirements-status.md"))}">docs/requirements-status.md</a>, '
                 f'one row per requirement, read by this page’s generator at commit <code>{h(stamp["short"])}</code>.</p></details>')
    parts.append("</section>")

    # What it is
    ci_counts = {rfp: sum(1 for r in hard[rfp] if r["category"] == "ci") for rfp in RFPS}
    rfpsrc = model["rfp_words"]["source"]
    rfp_links = " and ".join(
        f'<a href="{h(rfpsrc["repository"])}/blob/{h(rfpsrc["commit"])}/{h(model["rfp_words"]["rfps"][r]["file"])}">{r}</a>'
        for r in RFPS)
    what = [
        ("What", "A LEZ program checks Ethereum's sync-committee signatures itself and keeps "
                 "a write-once record of each finalized header; any LEZ program proves an account, a storage slot or a log "
                 "against it. The bridge escrows ERC-20s and ETH in an Ethereum vault and mints on LEZ against an attested "
                 "deposit; the vault releases against a LEZ burn proven to it."),
        ("Answers", f"{rfp_links}, one codebase, tested against each other end to end."),
        ("Proposal", f'<a href="{h(p["url"])}">logos-co/rfp #257</a>, filed {h(p["created"][:10])}; '
                     f'<span data-live="issue-line">{h(p["state"].lower())} (as read {h(p["read_utc"][:10])})</span>.'),
        ("Ask", f'<q>{md.render(p["bid"])}</q> (<a href="{h(p["url"])}">the proposal</a>)'),
        ("Platform", h(c["site"]["platform"]) + "; the Ethereum side on Sepolia and local EVMs."),
        ("Tests in CI", f"CI runs a test for {ci_counts['RFP-021']} of {len(hard['RFP-021'])} RFP-021 and "
                        f"{ci_counts['RFP-022']} of {len(hard['RFP-022'])} RFP-022 hard requirements "
                        f'(<a href="{h(site.src("scripts/ci/row-tests.toml"))}">the map</a>, checked against the workflows).'),
        ("Proofs", model["risc0_html"] + ' <a href="#security">The security note</a>.'),
        ("Audit", "None. " + " ".join(f'{r["rfp"]} {h(r["id"])} is not yet shown: {md.render(first_sentence(r["why"], 200))}'
                                       for r in rows if r["status"] == "not yet")),
        ("Code", f'<a href="{h(site.gh)}">{h(site.gh.removeprefix("https://"))}</a>, MIT OR Apache-2.0.'),
        ("Team", f'One author (GitHub <a href="https://github.com/rainbowpuffpuff">rainbowpuffpuff</a>) working with AI coding agents: '
                 f'<a href="{h(site.src("docs/how-it-was-built.md"))}">how it was built</a>.'),
    ]
    parts.append('<section class="wrap band" id="what" aria-labelledby="h-what"><h2 id="h-what">What it is</h2><dl class="kv">')
    for k, v in what:
        parts.append(f"<div><dt>{h(k)}</dt><dd>{v}</dd></div>")
    parts.append("</dl></section>")

    # Live
    t = c["testnet"]
    parts.append('<section class="wrap band" id="live" aria-labelledby="h-live"><h2 id="h-live">Live</h2>'
                 '<p class="lede">Read by your browser when the page loads: CI from GitHub’s public API, the test network '
                 'from this host. Nothing here is cached by the page’s build.</p><div class="live-grid">')
    parts.append('<article class="card" id="live-ci" aria-labelledby="h-ci"><h3 id="h-ci">CI on <code>main</code></h3>'
                 '<ul class="ci" data-live="ci-list">' +
                 "".join(f'<li><a href="{h(site.gh)}/actions/workflows/{h(w)}"><img src="{h(site.gh)}/actions/workflows/{h(w)}/badge.svg?branch=main" '
                         f'alt="{h(w)} on main" height="20" loading="lazy"></a></li>' for w in c["site"]["workflows"]) +
                 '</ul><p class="small" data-live="ci-note">Badges from GitHub; with JavaScript, each workflow’s latest run on main.</p>'
                 + model["r12_html"] + '</article>')
    parts.append('<article class="card" id="live-net" aria-labelledby="h-net"><h3 id="h-net">The test network</h3>'
                 '<dl class="kv kv-tight" data-live="net">'
                 '<div><dt>Network</dt><dd data-live="net-name">read from the host</dd></div>'
                 '<div><dt>Status</dt><dd data-live="net-status">not read yet</dd></div>'
                 '<div><dt>LEZ block height</dt><dd data-live="net-height">–</dd></div>'
                 '<div><dt>Last block</dt><dd data-live="net-block">–</dd></div>'
                 '<div><dt>Sepolia light client</dt><dd data-live="net-lc">–</dd></div>'
                 '<div><dt>Behind Sepolia</dt><dd data-live="net-lag">–</dd></div>'
                 '</dl><p class="small">'
                 f'<a href="{h(t["explorer"])}">Explorer</a> · <a href="{h(t["wallet"])}">Wallet</a> · '
                 f'<a href="{h(t["agents"])}">For agents</a> · <a href="{h(t["status"])}">Status page</a></p>'
                 '<p class="small">A LEZ network we run for the bid, not LEZ testnet 0.4 (not released).</p></article>')
    parts.append('<article class="card" id="live-facts" aria-labelledby="h-facts"><h3 id="h-facts">Latest attested Ethereum facts</h3>'
                 '<div data-live="facts"><p class="small">Read from the test network’s status when it lists them.</p></div></article>')
    parts.append("</div></section>")

    # Requirements
    parts.append(render_requirements(site, model))

    # What runs today
    parts.append(render_runs(site, model))

    # Milestones
    parts.append(render_milestones(site, model))

    # Security
    parts.append(render_security(site, model))

    # Try it
    parts.append('<section class="wrap band" id="try" aria-labelledby="h-try"><h2 id="h-try">Try it</h2>'
                 '<p class="lede">Commands as the guides give them. Building from source needs Rust and, for the LEZ side, '
                 'RISC Zero’s toolchain (<a href="' + h(site.src("docs/guides/quickstart.md")) + '">quickstart</a>).</p><div class="try-grid">')
    for tr in c.get("try", []):
        lines = "\n".join(tr["lines"])
        doc = tr.get("doc")
        parts.append('<article class="card try"><h3>' + h(tr["title"]) + "</h3>"
                     f'<div class="cmd"><pre><code data-rpc>{h(lines)}</code></pre>'
                     '<button type="button" class="copy" hidden aria-label="Copy the commands"><svg aria-hidden="true"><use href="#i-copy"/></svg></button></div>'
                     + (f'<p class="small">{h(tr["note"])}</p>' if tr.get("note") else "")
                     + (f'<p class="small"><a href="{h(site.src(doc))}">{h(doc)}</a></p>' if doc else "") + "</article>")
    faucet = t.get("faucet")
    if faucet:
        parts.append('<article class="card try" id="faucet"><h3>Funds on the test network: the faucet</h3>'
                     f'<p class="small" data-live="faucet-terms">One transfer of the test network’s native token to a LEZ '
                     f'public account, limited per account and per address; its terms: <a href="{h(faucet)}">{h(faucet)}</a>.</p>'
                     f'<div class="cmd"><pre><code data-rpc>curl -s -X POST {{origin}}{h(faucet)} -H \'content-type: application/json\' '
                     '-d \'{"account": "&lt;your LEZ public account id&gt;", "public_key": "&lt;its 64-hex public key, for an account never used&gt;"}\''
                     '</code></pre><button type="button" class="copy" hidden '
                     'aria-label="Copy the command"><svg aria-hidden="true"><use href="#i-copy"/></svg></button></div>'
                     '<form class="faucet" id="faucet-form" hidden><label>Your LEZ public account id '
                     '<input name="account" required autocomplete="off" spellcheck="false" maxlength="100"></label>'
                     '<label>Its public key (64 hex digits; needed when the account has never been used) '
                     '<input name="public_key" autocomplete="off" spellcheck="false" maxlength="66" pattern="(0x)?[0-9a-fA-F]{64}"></label>'
                     '<button type="submit">Request funds</button><output aria-live="polite"></output></form>'
                     "<p class=\"small\">The test network is ours, for the bid; its tokens have no value.</p></article>")
    parts.append("</div></section>")

    # Not done yet
    parts.append(render_not_done(site, model))

    # Updates
    parts.append('<section class="wrap band" id="updates" aria-labelledby="h-upd"><h2 id="h-upd">Updates posted on the proposal</h2><ol class="updates">')
    for u in reversed(p["updates"]):
        parts.append(f'<li><time datetime="{h(u["created"])}">{h(u["created"][:10])}</time> <a href="{h(u["url"])}">{h(u["title"])}</a></li>')
    parts.append("</ol></section>")

    parts.append("</main>")
    parts.append(render_footer(site, model))
    parts.append('<dialog id="req-dialog" class="sheet" aria-labelledby="rq-title"><div class="sheet-head">'
                 '<h2 id="rq-title">Requirement</h2><div class="sheet-nav">'
                 '<button type="button" class="nav" data-step="-1" aria-label="Previous requirement">‹</button>'
                 '<button type="button" class="nav" data-step="1" aria-label="Next requirement">›</button>'
                 '<button type="button" class="close" data-close aria-label="Close">×</button></div></div>'
                 '<div id="rq-body" class="sheet-body"></div></dialog>')
    parts.append('<dialog id="ev-dialog" aria-labelledby="ev-title"><form method="dialog"><button class="close" aria-label="Close">×</button></form>'
                 '<div id="ev-body"></div></dialog><div id="tip" role="tooltip" hidden></div>')
    title = "Ferrata · the build: RFP-021 and RFP-022, requirement by requirement"
    desc = (f"Ferrata's bid for Logos RFP-021 and RFP-022: {count_rows(hard['RFP-021'])['shown']} of {len(hard['RFP-021'])} and "
            f"{count_rows(hard['RFP-022'])['shown']} of {len(hard['RFP-022'])} hard requirements shown, each with its test or run.")
    cfg = dict(c["testnet"])
    cfg.update({"repo": site.gh, "api_repo": c["site"]["api_repo"], "issue_api": c["site"]["issue_api"],
                "workflows": c["site"]["workflows"]})
    cfg_json = json.dumps(cfg, separators=(",", ":")).replace("<", "\\u003c")
    return page(site, title, "\n".join(parts), desc,
                f'<script type="application/json" id="site-config">{cfg_json}</script>\n'
                '<noscript><link rel="stylesheet" href="assets/nojs.css"></noscript>\n')


def render_overview(site, model):
    """One tile per requirement, grouped by RFP and section, each with its ID and its status mark (colour and symbol)."""
    rows = model["rows"]
    total = count_rows(rows)
    out = ['<div class="ov" id="overview">'
           '<div class="ov-bar"><ul class="ov-legend" aria-label="Statuses, with the number of requirements in each">']
    for s in STATUSES:
        out.append(f'<li><span class="lg" data-hl="{SKEY[s]}">{box(s)}<span class="lg-name">{h(s)}</span> '
                   f'<b class="lg-n">{total[s]}</b></span></li>')
    out.append('</ul><button type="button" class="btn" id="view-toggle" aria-controls="req-full" aria-expanded="false" hidden>'
               'Show the full table</button></div><div class="ov-grid" id="ov-grid">')
    for rfp in RFPS:
        mine = [r for r in rows if r["rfp"] == rfp]
        hard = [r for r in mine if not r["soft"]]
        hc, sc = count_rows(hard), count_rows([r for r in mine if r["soft"]])
        slug = rfp.lower()
        summary = (f'{hc["shown"]} of {len(hard)} hard shown' + "".join(f" · {hc[s]} {s}" for s in STATUSES[1:] if hc[s]) +
                   " · soft: " + ", ".join(f"{n} {s}" for s, n in sc.items() if n))
        out.append(f'<section class="ov-rfp" aria-labelledby="ovh-{slug}"><h3 id="ovh-{slug}">{rfp} '
                   f'<span>{h(RFP_NAMES[rfp])}</span></h3><p class="ov-sum">{h(summary)}</p>')
        for sec in SECTION_ORDER[rfp] + ["Soft"]:
            these = [r for r in mine if r["section"] == sec]
            if not these:
                continue
            prefix = these[0]["id"].split(".")[0]
            lab = f"ovl-{slug}-{prefix.lower()}"
            out.append(f'<div class="ov-sec"><span class="ov-lab" id="{lab}"><b>{h(prefix)}</b> '
                       f'{h("Soft requirements" if sec == "Soft" else sec)}</span><ul class="tiles" aria-labelledby="{lab}">')
            for r in these:
                k = SKEY[r["status"]]
                anchor = f'{rfp.lower()}-{r["id"].lower().replace(".", "-")}'
                num = r["id"].split(".", 1)[1]
                out.append(f'<li><a class="tile t-{k}" href="#{anchor}" data-req="{rfp}-{h(r["id"])}" aria-haspopup="dialog" '
                           f'title="{rfp} {h(r["id"])}: {h(r["status"])}">{box(r["status"])}<span class="vh">{rfp} {h(prefix)}.</span>'
                           f'<span class="tn">{h(num)}</span><span class="vh">, {h(r["status"])}</span></a></li>')
            out.append("</ul></div>")
        out.append("</section>")
    out.append("</div></div>")
    return "".join(out)


def render_requirements(site, model):
    rows = model["rows"]
    md = site.md("docs/requirements-status.md")
    out = ['<section class="wrap band" id="requirements" aria-labelledby="h-req"><h2 id="h-req">Every requirement</h2>'
           '<p class="lede lede-wide">One tile per requirement, by RFP and section. Open one for the RFP’s words, the strict '
           'status and its reason, the tests (and whether CI runs them) and the milestone; or show the full table.</p>']
    out.append(render_overview(site, model))
    # Section table (the bars' table view)
    out.append('<details class="by-section"><summary>Counts by section</summary><div class="scroll"><table class="num">'
               '<caption>Hard requirements by section, from docs/requirements-status.md</caption><thead><tr><th scope="col">RFP</th>'
               '<th scope="col">Section</th>' + "".join(f'<th scope="col">{h(s)}</th>' for s in STATUSES) +
               '<th scope="col">total</th></tr></thead><tbody>')
    for rfp in RFPS:
        for sec in SECTION_ORDER[rfp]:
            these = [r for r in rows if r["rfp"] == rfp and r["section"] == sec]
            cnt = count_rows(these)
            out.append(f'<tr><th scope="row">{rfp}</th><td class="txt">{h(sec)}</td>' +
                       "".join(f"<td>{cnt[s]}</td>" for s in STATUSES) + f"<td>{len(these)}</td></tr>")
    out.append("</tbody></table></div></details>")

    # The full table, with its filters (hidden behind "Show the full table" when JavaScript runs)
    out.append('<div class="req-full" id="req-full" hidden>')
    sections = []
    for rfp in RFPS:
        for s in SECTION_ORDER[rfp] + ["Soft"]:
            if s not in sections:
                sections.append(s)
    out.append('<form class="filters" id="filters" aria-label="Filter the requirements" hidden>'
               '<label>RFP <select name="rfp"><option value="">both</option><option>RFP-021</option><option>RFP-022</option></select></label>'
               '<label>Status <select name="status"><option value="">any</option>' +
               "".join(f'<option value="{SKEY[s]}">{h(s)}</option>' for s in STATUSES) + "</select></label>"
               '<label>Section <select name="section"><option value="">any</option>' +
               "".join(f'<option value="{h(s)}">{h(s)}</option>' for s in sections) + "</select></label>"
               '<label>CI <select name="ci"><option value="">any</option><option value="ci">a test CI runs</option>'
               '<option value="not">no test CI runs</option></select></label>'
               '<label>Milestone <select name="ms"><option value="">any</option>' +
               "".join(f"<option>{m}</option>" for m in MILESTONES) + "</select></label>"
               '<label class="grow">Search <input type="search" name="q" placeholder="words, ids, paths" autocomplete="off"></label>'
               '<button type="reset">Clear</button>'
               '<output id="req-count" aria-live="polite"></output></form>')
    out.append('<div class="scroll req-wrap"><table class="req" id="req-table"><caption class="vh">Requirements of RFP-021 and RFP-022 with status, '
               'reason, tests and milestone</caption><thead><tr><th scope="col">Requirement</th><th scope="col">Status and why</th>'
               '<th scope="col">Shown by</th><th scope="col">Milestone</th></tr></thead><tbody>')
    rsrc = model["rfp_words"]["source"]
    where_label = {"ci": "CI", "manual": "CI, started by hand", "box": "build box", "local": "by hand", "record": "record",
                   "document": "document", "pending": "on another branch"}
    up = Md(site, None, (rsrc["repository"], rsrc["commit"], "RFPs", set(rsrc["files"])))
    cat_label = {"ci": "a test CI runs", "manual": "a test in a workflow started by hand",
                 "box": "tests on the build box or by hand only", "records": "records and documents only",
                 "pending": "entries on another branch only", "none": "no entry", "soft": "soft requirement: evidence only"}
    for r in rows:
        anchor = f'{r["rfp"].lower()}-{r["id"].lower().replace(".", "-")}'
        rfile = model["rfp_words"]["rfps"][r["rfp"]]["file"]
        words_html = up.render(r["words"].split("\n")[0]) + "".join(
            f"<br>{up.render(l)}" for l in r["words"].split("\n")[1:] if l.strip())
        why_first, rest = split_first(r["why"], 300)
        ci_attr = "ci" if r["category"] == "ci" else ("soft" if r["soft"] else "not")
        kinds = {}
        for t in r["tests"]:
            kinds[t["kind"]] = kinds.get(t["kind"], 0) + 1
        tests_html = f'<p class="cisum"><span class="tag tag-{h(r["category"])}">{h(cat_label[r["category"]])}</span></p>'
        tests_html += f'<p class="evid"><span class="lbl">Evidence</span> {md.render(r["evidence"], evidence=True)}</p>'
        if r["tests"]:
            items = []
            for t in r["tests"]:
                url = site.src(t["path"]) if t["where"] != "pending" else None
                p_html = f'<a class="path" href="{h(url)}"><code>{h(t["path"])}</code></a>' if url else f"<code>{h(t['path'])}</code>"
                tag = where_label.get(t["where"], t["where"])
                if t["where"] in ("ci", "manual") and t.get("ci"):
                    tag_html = f'<a class="tag tag-{t["where"]}" href="{h(site.src(t["workflow"]))}">{h(tag)} · {h(t["ci"])}</a>'
                else:
                    tag_html = f'<span class="tag tag-{h(t["where"])}">{h(tag)}</span>'
                run = f'<span class="runcmd">run: <code>{h(t["run"])}</code></span>' if t.get("run") else ""
                items.append(f"<li>{tag_html} {p_html}{run}</li>")
            summary = ", ".join(f"{n} {k}{'s' if n > 1 else ''}" for k, n in kinds.items())
            tests_html += (f'<details class="tests-d"><summary>{h(summary)}</summary><ul class="tests">{"".join(items)}</ul>'
                           + (f'<p class="note">{md.render(r["note"])}</p>' if r["note"] else "") + "</details>")
        ms = ", ".join(r["milestones"]) or r["milestone_raw"].strip() or "—"
        extra = re.sub(r"^[^(]*", "", r["milestone_raw"]).strip() if r["milestones"] else ""
        out.append(
            f'<tr id="{anchor}" data-rfp="{r["rfp"]}" data-status="{SKEY[r["status"]]}" data-section="{h(r["section"])}" '
            f'data-ci="{ci_attr}" data-ms="{" ".join(r["milestones"])}">'
            f'<th scope="row" data-label="Requirement"><a class="rid" href="#{anchor}">{r["rfp"]} {h(r["id"])}</a>'
            f'<span class="sec">{h(r["section"])}{" · soft" if r["soft"] else ""}</span>'
            f'<div class="words">{words_html}</div>'
            f'<a class="small" href="{h(rsrc["repository"])}/blob/{h(rsrc["commit"])}/{h(rfile)}#L{r["words_line"]}">the RFP’s text</a></th>'
            f'<td data-label="Status and why">{chip(r["status"])}<p class="why">{md.render(why_first)}</p>'
            + (f'<details class="more"{" data-full" if rest == r["why"].strip() else ""}><summary>The full reason</summary>'
               f'<p>{md.render(rest)}</p></details>' if rest else "")
            + f'</td><td data-label="Shown by">{tests_html}</td>'
            f'<td data-label="Milestone"><span class="ms">{h(ms)}</span>{f"<span class=small>{h(extra)}</span>" if extra else ""}</td></tr>')
    out.append("</tbody></table></div>")
    out.append(f'<p class="small">Statuses and reasons: <a href="{h(site.src("docs/requirements-status.md"))}">docs/requirements-status.md</a>; '
               f'tests: <a href="{h(site.src("scripts/ci/row-tests.toml"))}">scripts/ci/row-tests.toml</a>, whose CI jobs are checked '
               f'against the workflows by <a href="{h(site.src("scripts/ci/check-row-tests.py"))}">check-row-tests.py</a>; milestones: '
               f'<a href="{h(site.src("docs/traceability.md"))}">docs/traceability.md</a>; the RFPs’ words: '
               f'<a href="{h(rsrc["repository"])}">logos-co/rfp</a> at <code>{h(rsrc["commit"][:8])}</code> (MIT OR Apache-2.0). '
               f'Evidence numbers open the row of <a href="evidence.html">the evidence index</a>.</p></div></section>')
    return "".join(out)


def render_runs(site, model):
    out = ['<section class="wrap band" id="runs" aria-labelledby="h-runs"><h2 id="h-runs">What runs today</h2>'
           '<p class="lede">Recorded runs, each with where it ran and whether its proofs were real, as its evidence row says. '
           'LEZ testnet 0.4 does not exist yet, so every LEZ v0.4 run is on a zone we ran. ' + model["risc0_html"] +
           ' (<a href="#security">security note</a>).</p>']
    shots = site.content.get("shot", [])
    for r in model["runs"]:
        ev = [site.evidence[n] for n in r.get("evidence", [])]
        mdv = site.md("docs/EVIDENCE.md")
        out.append(f'<article class="run" id="run-{h(r["id"])}" aria-labelledby="h-run-{h(r["id"])}">')
        out.append(f'<header><h3 id="h-run-{h(r["id"])}">{h(r["title"])}</h3>'
                   + (f'<time datetime="{h(r["date"])}">{h(r["date"])}</time>' if r.get("date") else "") + "</header>")
        derived = [proofs_label(ev[0]["proofs"])] if ev else []
        out.append('<ul class="labels">' + "".join(f"<li>{h(l)}</li>" for l in r.get("labels", []) + derived) + "</ul>")
        if ev:
            e = ev[0]
            out.append('<dl class="kv kv-tight">'
                       f'<div><dt>Shows</dt><dd>{mdv.render(first_sentence(e["what"], 420))}</dd></div>'
                       f'<div><dt>Numbers</dt><dd>{mdv.render(first_sentence(e["numbers"], 520))}</dd></div>'
                       f'<div><dt>Where it ran</dt><dd>{mdv.render(e["where"])}</dd></div>'
                       f'<div><dt>Proofs</dt><dd>{mdv.render(e["proofs"])}</dd></div>'
                       "</dl>")
            out.append('<p class="small">Evidence ' + ", ".join(
                f'<a class="ev" href="evidence.html#e{x["n"]}" data-ev="{x["n"]}">row {x["n"]}</a>' for x in ev) + " (the full rows)"
                + (f' · record <a href="{h(site.src(r["record"]))}"><code>{h(posixpath.dirname(r["record"]) + "/")}</code></a>' if r.get("record") else "")
                + "</p>")
        if r.get("tx_rows"):
            out.append('<div class="scroll"><table class="tx stack stack-row"><caption>Sepolia transactions of the round trip '
                       '(the deployment’s are in the record)</caption><thead><tr><th scope="col">Step</th><th scope="col">Transaction</th>'
                       '<th scope="col">Block</th><th scope="col">Gas used</th><th scope="col">Result</th></tr></thead><tbody>')
            for t in r["tx_rows"]:
                out.append(f'<tr><th scope="row">{h(t["stage"])}</th><td data-label="Transaction"><a class="hash" '
                           f'href="https://sepolia.etherscan.io/tx/{t["hash"]}" title="{t["hash"]}"><code>{short_hash(t["hash"])}</code></a></td>'
                           f'<td class="n" data-label="Block">{t["block"]:,}</td><td class="n" data-label="Gas used">{t["gas"]:,}</td>'
                           f'<td data-label="Result">{"success" if t["ok"] else "failed"}</td></tr>')
            out.append("</tbody></table></div>")
        if r.get("contracts"):
            out.append('<p class="small contracts">Contracts: ' + " · ".join(
                f'{h(k)} <a class="hash" href="https://sepolia.etherscan.io/address/{h(v)}"><code>{h(short_hash(v, 8))}</code></a>'
                for k, v in r["contracts"]) + "</p>")
        if r.get("stage_rows"):
            mds = site.md(r["stages"])
            out.append('<details class="stages"><summary>Every stage with its measured time</summary><div class="scroll"><table>'
                       '<thead><tr><th scope="col">Stage</th><th scope="col">Result</th><th scope="col">Time</th></tr></thead><tbody>')
            for s in r["stage_rows"]:
                out.append(f'<tr><th scope="row">{mds.render(s[0])}</th><td>{mds.render(first_sentence(s[1], 300))}</td>'
                           f'<td class="n">{mds.render(s[2])}</td></tr>')
            out.append("</tbody></table></div></details>")
        if r.get("lez_rows"):
            out.append('<details class="stages"><summary>The LEZ transactions</summary><div class="scroll"><table class="tx">'
                       '<thead><tr><th scope="col">#</th><th scope="col">Transaction</th><th scope="col">Hash</th><th scope="col">Block</th></tr></thead><tbody>')
            for x in r["lez_rows"]:
                out.append(f'<tr><td class="n">{h(x.get("seq", ""))}</td><td>{h(x.get("label", ""))}</td>'
                           f'<td><code title="{h(x.get("tx_hash", ""))}">{h(short_hash(x.get("tx_hash", "")))}</code></td>'
                           f'<td class="n">{h(x.get("block", ""))}</td></tr>')
            out.append("</tbody></table></div>"
                       + (f'<p class="small">Look them up on <a href="{h(r["explorer"])}">the public LEZ testnet’s explorer</a>.</p>' if r.get("explorer") else "")
                       + "</details>")
        if r.get("recheck"):
            out.append(f'<div class="cmd"><pre><code>{h(r["recheck"])}</code></pre><button type="button" class="copy" hidden '
                       'aria-label="Copy the command"><svg aria-hidden="true"><use href="#i-copy"/></svg></button></div>')
        if r.get("gallery"):
            out.append(render_gallery(site, [s for s in shots if s.get("gallery") == r["gallery"]], r["record"]))
        out.append("</article>")
    # The CLI walkthrough
    cli = site.content.get("cli")
    if cli:
        out.append(f'<article class="run" id="run-cli" aria-labelledby="h-run-cli"><header><h3 id="h-run-cli">{site.md(None).render(cli["title"])}</h3></header>'
                   '<p>The attestation CLI’s own output, as recorded: a status read, a storage slot attested and applied, '
                   'the same request with a condition that does not hold (refused with its code), and an ERC-20 '
                   '<code>Transfer</code> log attested. Each command is a dry run first; with <code>--send --yes</code> it '
                   'sends the same signed bytes and checks the zone’s receipt against the dry run.</p>')
        for step in cli["steps"]:
            text = site.repo.text(step).rstrip("\n")
            name = posixpath.basename(step)
            out.append(f'<figure class="term"><figcaption><a href="{h(site.src(step))}"><code>{h(name)}</code></a></figcaption>'
                       f'<pre tabindex="0"><code>{h(text)}</code></pre></figure>')
        ev = cli.get("evidence", [])
        out.append('<p class="small">Evidence ' + ", ".join(
            f'<a class="ev" href="evidence.html#e{n}" data-ev="{n}">row {n}</a>' for n in ev) +
            f' · record <a href="{h(site.src(cli["record"]))}"><code>{h(posixpath.dirname(cli["record"]) + "/")}</code></a></p></article>')
    others = [s for s in shots if s.get("gallery") not in {r.get("gallery") for r in model["runs"]}]
    if others:
        out.append('<article class="run" id="run-screens" aria-labelledby="h-run-screens"><header><h3 id="h-run-screens">More screens</h3></header>'
                   + render_gallery(site, others, None) + "</article>")
    out.append("</section>")
    return "".join(out)


def render_gallery(site, shots, record):
    out = ['<ul class="gallery">']
    for s in shots:
        path = s["path"]
        if path not in site.repo.files:
            raise SystemExit(f"build.py: screenshot {path} is not tracked")
        data = (ROOT / path).read_bytes()
        ext = posixpath.splitext(path)[1].lower()
        name = hashlib.sha256(data).hexdigest()[:12] + ext
        site.images[f"img/{name}"] = data
        w, hgt = image_size(data, ext)
        src_rec = s.get("record") or record
        out.append(f'<li><figure><a href="img/{name}"><img src="img/{name}" alt="{h(s["caption"])}" width="{w}" height="{hgt}" '
                   f'loading="lazy" decoding="async"></a><figcaption>{h(s["caption"])}'
                   + (f' <a class="small" href="{h(site.src(src_rec))}">(the run)</a>' if src_rec else "")
                   + "</figcaption></figure></li>")
    out.append("</ul>")
    return "".join(out)


def image_size(data, ext):
    if ext == ".png" and data[:8] == b"\x89PNG\r\n\x1a\n":
        return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
    if ext in (".jpg", ".jpeg"):
        i = 2
        while i < len(data):
            marker, length = data[i + 1], int.from_bytes(data[i + 2:i + 4], "big")
            if marker in (0xC0, 0xC1, 0xC2):
                return int.from_bytes(data[i + 7:i + 9], "big"), int.from_bytes(data[i + 5:i + 7], "big")
            i += 2 + length
    raise SystemExit("build.py: unknown image format")


def weeks_label(start, n):
    return f"week {start}" if n == 1 else f"weeks {start}–{start + n - 1}"


def render_milestones(site, model):
    p = model["proposal"]
    rows = model["rows"]
    md = site.md(None)
    out = ['<section class="wrap band" id="milestones" aria-labelledby="h-ms"><h2 id="h-ms">Milestones, budget and timeline</h2>'
           f'<p class="lede">As filed in <a href="{h(p["url"])}">the proposal</a> (read {h(p["read_utc"][:10])}). No grant is '
           'awarded yet, so no milestone has started or been paid; each milestone’s state below is the status of the requirements '
           'it delivers, from the table above.</p>']
    for para in p["paras"]:
        out.append(f"<p>{md.render(para)}</p>")
    # Timeline: weeks from signing, in the filed order, then float, then R after the audit reports.
    week = 1
    cells = []
    for row in p["table"]:
        key, name, rfp, weeks, pay = (row + [""] * 5)[:5]
        if key in MILESTONES and key != "R":
            n = int(weeks)
            site.css_grid.add((week, n))
            cells.append(f'<li class="tl-{h(key[0])} gs{week}-{n}"><b>{h(key)}</b><span>{h(name)}</span>'
                         f'<small class="tl-wk">{weeks_label(week, n)}</small></li>')
            week += n
    total_weeks = 14
    if week <= total_weeks:
        site.css_grid.add((week, total_weeks - week + 1))
        cells.append(f'<li class="tl-float gs{week}-{total_weeks - week + 1}"><b>float</b><span>{total_weeks - week + 1} weeks</span>'
                     f'<small class="tl-wk">{weeks_label(week, total_weeks - week + 1)}</small></li>')
    out.append('<figure class="timeline"><figcaption>Weeks from signing (the filed order; 11 paid build weeks in 14 calendar '
               'weeks, then R, a week after the audit reports)</figcaption><ol class="tl-weeks" aria-hidden="true">' +
               "".join(f"<li>{i}</li>" for i in range(1, total_weeks + 1)) + '</ol><ol class="tl">' + "".join(cells) +
               '</ol></figure>')
    out.append('<div class="scroll"><table class="ms-table stack"><caption>The milestones as filed, with the state of the requirements each delivers</caption>'
               '<thead><tr><th scope="col">#</th><th scope="col">Milestone and done gate (abridged, as filed)</th><th scope="col">RFP</th>'
               '<th scope="col">Weeks</th><th scope="col">Payout</th><th scope="col">Its requirements today</th></tr></thead><tbody>')
    for row in p["table"]:
        key, name, rfp, weeks, pay = (row + [""] * 5)[:5]
        gate = p["gates"].get(key, "")
        mine = [r for r in rows if key and key in r["milestones"]]
        if mine:
            cnt = count_rows(mine)
            state = (seg_bar(site, f"{key}: its {len(mine)} requirements", cnt, len(mine), compact=True) +
                     f'<p class="small"><a href="#requirements" data-filter="ms={h(key)}">Its {len(mine)} requirements</a></p>')
        else:
            state = '<p class="small">—</p>'
        today = ""
        for n in site.content.get("milestone_evidence", {}).get(key, []):
            e = site.evidence[n]
            mde = site.md("docs/EVIDENCE.md")
            today += (f'<p class="today"><span class="lbl">Today</span> {mde.render(first_sentence(e["numbers"], 220))} '
                      f'(<a class="ev" href="evidence.html#e{n}" data-ev="{n}">evidence row {n}</a>)</p>')
        out.append(f'<tr><th scope="row">{h(key) or ""}</th><td><b>{md.render(name)}</b>'
                   + (f'<div class="gate">{md.block(gate)}</div>' if gate else "") + today +
                   f'</td><td class="inl" data-label="RFP">{h(rfp)}</td><td class="n inl" data-label="Weeks">{md.render(weeks)}</td>'
                   f'<td class="n inl" data-label="Payout">{md.render(pay)}</td><td data-label="Its requirements today">{state}</td></tr>')
    out.append("</tbody></table></div>")
    out.append(f'<div class="two"><article class="card"><h3>Budget</h3><p class="small">As filed in <a href="{h(p["url"])}">the '
               f'proposal</a> (its sections and tables):</p><blockquote>{md.block(p["sections"]["budget"])}{md.block(p["bid"])}'
               '</blockquote></article>'
               f'<article class="card"><h3>Risks, as filed</h3>{render_md_table(md, p["sections"]["risks"])}</article></div>')
    out.append(f'<article class="card"><h3>After delivery, as filed</h3>{md.block(p["sections"]["post_delivery"])}</article>')
    out.append("</section>")
    return "".join(out)


def render_md_table(md, text):
    lines = [l for l in text.splitlines() if l.startswith("|")]
    if len(lines) < 2:
        return md.block(text)
    head = split_cells(lines[0])
    body = [split_cells(l) for l in lines[2:]]
    return ('<div class="scroll"><table><thead><tr>' + "".join(f'<th scope="col">{md.render(c)}</th>' for c in head) +
            "</tr></thead><tbody>" + "".join("<tr>" + "".join(f"<td>{md.render(c)}</td>" for c in r) + "</tr>" for r in body) +
            "</tbody></table></div>")


def render_security(site, model):
    s = model["security"]
    md = site.md(s["path"])
    review = site.content["review"]
    note = site.src(s["path"])
    out = ['<section class="wrap band" id="security" aria-labelledby="h-sec"><h2 id="h-sec">Security</h2>'
           f'<article class="card sec-note"><h3><a href="{h(note)}">{h(s["title"])}</a></h3>']
    out.append(f'<blockquote class="correction"><p>{md.render(s["quote"])}</p></blockquote>')
    for para in s["paras"]:
        out.append(f"<p>{md.render(' '.join(para.split()))}</p>")
    out.append(f'<details><summary>The correction of 23 November 2026, as written</summary>'
               + "".join(f"<p>{md.render(' '.join(c.split()))}</p>" for c in s["correction"])
               + f'<p><a href="{h(note)}#correction-23-november-2026">The whole correction</a></p></details>')
    out.append("</article>")
    out.append('<div class="two">')
    sec_md = site.md("SECURITY.md")
    out.append('<article class="card"><h3>Not audited</h3>'
               f'<blockquote><p>{sec_md.render(model["security_says"])}</p></blockquote>'
               f'<p class="small">(<a href="{h(site.src("SECURITY.md"))}">SECURITY.md</a>)</p><ul>' +
               "".join(f'<li><a href="#rfp-{r["rfp"][-3:]}-{r["id"].lower().replace(".", "-")}">{r["rfp"]} {h(r["id"])}</a> is not '
                       f'yet shown: {md.render(r["why"])}</li>' for r in model["rows"] if r["status"] == "not yet") +
               "</ul></article>")
    rv_md = site.md(review["doc"])
    out.append(f'<article class="card"><h3>Review</h3><blockquote><p>{rv_md.render(model["review_says"])}</p></blockquote>'
               f'<p class="small">(<a href="{h(site.src(review["doc"]))}#reviews-by-agents">How it was built</a>, “Reviews by '
               f'agents”, which lists each review; the agent reviews are kept in <a href="{h(site.src("docs/reviews"))}">'
               'docs/reviews/</a>.)</p></article>')
    out.append("</div></section>")
    return "".join(out)


def render_not_done(site, model):
    rows = model["rows"]
    md = site.md("docs/requirements-status.md")
    blocked = [r for r in rows if r["status"] in ("not yet", "waits on Logos")]
    part = [r for r in rows if r["status"] == "in part"]
    out = ['<section class="wrap band" id="not-done" aria-labelledby="h-nd"><h2 id="h-nd">Not done yet</h2>'
           '<p class="lede">The requirements that wait on Logos (an upstream release or LEZ testnet 0.4) or on an audit, '
           'with what each needs, as their rows say.</p><div class="scroll"><table class="nd stack"><thead><tr>'
           '<th scope="col">Requirement</th><th scope="col">Status</th><th scope="col">What it needs</th><th scope="col">Shown instead, today</th>'
           '</tr></thead><tbody>']
    for r in blocked:
        anchor = f'{r["rfp"].lower()}-{r["id"].lower().replace(".", "-")}'
        why = r["why"].strip()
        need, _, rest = why.partition("Shown instead:")
        need, rest = need.strip(), rest.strip()
        out.append(f'<tr><th scope="row"><a href="#{anchor}">{r["rfp"]} {h(r["id"])}</a><span class="small">{Md(site, None, (model["rfp_words"]["source"]["repository"], model["rfp_words"]["source"]["commit"], "RFPs", set(model["rfp_words"]["source"]["files"]))).render(first_sentence(r["words"], 160))}</span></th>'
                   f'<td>{chip(r["status"])}</td><td data-label="What it needs">{md.render(need)}</td>'
                   f'<td data-label="Shown instead, today">{md.render(rest) if rest else "—"}</td></tr>')
    out.append("</tbody></table></div>")
    out.append(f'<p>And {len(part)} requirements are <a href="#requirements" data-filter="status=part">in part</a>: each row '
               'says which clause is shown and which is not.</p>')
    out.append(f'<article class="card"><h3>Specified but not built</h3>{site.md("README.md").block(model["not_built"])}</article></section>')
    return "".join(out)


def render_footer(site, model):
    st = model["stamp"]
    return (f'<footer class="foot"><div class="wrap"><p>Built by <a href="{h(site.src("site/build.py"))}"><code>site/build.py</code></a> '
            f'from the repository at commit <code data-built-from="{h(st["commit"])}">{h(st["short"])}</code> ({h(st["date"])})'
            f'{" with uncommitted changes" if st["dirty"] else ""}; the requirement statuses are those of commit '
            f'<a href="{h(site.gh)}/commit/{h(st["status_commit"])}"><code data-statuses-from="{h(st["status_commit"])}">'
            f'{h(st["status_commit"][:8])}</code></a> ({h(st["status_date"])}), the last change to '
            f'<code>docs/requirements-status.md</code>. Rebuild: '
            '<code>python3 -I site/build.py</code>; check: <code>python3 -I site/test_site.py</code>.</p>'
            '<p>No cookies, no analytics. Besides its own files the page requests: github.com (four workflow badges), '
            'api.github.com (each workflow’s latest run on main and the proposal’s state, five calls cached for five minutes), '
            'and this host (<code>/rpc</code>, <code>/status/status.json</code>, <code>/status/programs.json</code>, '
            '<code>/faucet</code>); nothing else.</p>'
            f'<p>Code and text MIT OR Apache-2.0. The RFPs’ words are quoted from <a href="https://github.com/logos-co/rfp">logos-co/rfp</a> '
            f'under its MIT licence (<a href="{h(site.src("site/data/LICENSE-logos-co-rfp.txt"))}">the notice</a>).</p>'
            '</div></footer>')


def render_evidence(site, model):
    md = site.md("docs/EVIDENCE.md")
    out = ['<a class="skip" href="#main">Skip to content</a><header class="top"><div class="wrap top-in"><a class="brand" href="index.html">'
           '<img src="assets/logo.svg" alt="" width="28" height="28"><span>Ferrata</span></a><nav aria-label="Pages"><ul>'
           '<li><a href="index.html#requirements">Requirements</a></li><li><a href="index.html#runs">What runs</a></li></ul></nav></div></header>'
           '<main id="main" class="wrap band"><h1>The evidence index</h1>'
           f'<p class="lede">Every row of <a href="{h(site.src("docs/EVIDENCE.md"))}">docs/EVIDENCE.md</a>, as it stands at '
           f'<code>{h(model["stamp"]["short"])}</code>: what it shows, its numbers, where it ran, whether its proofs were real, how to '
           're-check it and the file with the details.</p>']
    out.append(f'<details class="def"><summary>How to read it (the index’s own introduction)</summary>{md.block(model["evidence_intro"])}</details>')
    for n in sorted(site.evidence):
        e = site.evidence[n]
        out.append(f'<article class="evrow" id="e{n}" aria-labelledby="eh{n}"><h2 id="eh{n}"><a href="#e{n}">Row {n}</a> '
                   f'<span>{h(e["title"])}</span></h2><dl class="kv kv-tight">')
        for key, label in (("what", "What it shows"), ("numbers", "Numbers"), ("where", "Where it ran"), ("proofs", "Proofs"),
                           ("recheck", "Re-check"), ("details", "Details")):
            out.append(f"<div><dt>{label}</dt><dd>{md.render(e[key])}</dd></div>")
        out.append("</dl></article>")
    out.append("</main>")
    out.append(render_footer(site, model))
    return page(site, "Ferrata · the evidence index", "\n".join(out), "Every evidence row of Ferrata's bid for Logos RFP-021 and RFP-022.")


def source_notes(site, text):
    """The `<!-- source: PATH | TEXT -->` notes of a page and, for each, whether PATH is tracked and holds TEXT
    (runs of whitespace count as one space): [(path, text, problem or None)]."""
    out = []
    for path, snippet in SOURCE_NOTE.findall(text):
        squash = lambda s: re.sub(r"\s+", " ", s).strip()  # noqa: E731
        if path not in site.repo.files:
            out.append((path, snippet, "not a tracked file"))
        elif squash(snippet) not in squash(site.repo.text(path)):
            out.append((path, snippet, "the file no longer holds this text"))
        else:
            out.append((path, snippet, None))
    return out


def render_testers(site, model):
    """The testers' start page (/testers/): site/testers.html in the build page's header, footer and style. No script:
    it is complete without JavaScript."""
    if ISSUE_FORM not in site.repo.files:
        raise SystemExit(f"build.py: {ISSUE_FORM} is not tracked (the page links to it)")
    text = re.sub(r"<!--#.*?-->\n?", "", site.repo.text(TESTERS_SRC), flags=re.S)
    bad = [f"{p}: {why}: {s!r}" for p, s, why in source_notes(site, text) if why]
    if bad:
        raise SystemExit(f"build.py: {TESTERS_SRC}: a number's source does not hold it any more (update the page):\n  "
                         + "\n  ".join(bad))

    def src(m):
        url = site.md(None).repo_url(m.group(1), m.group(2))
        if url is None:
            raise SystemExit(f"build.py: {TESTERS_SRC}: {m.group(1)} is not tracked")
        return h(url)

    body = re.sub(r"\{\{src:([^}#]+)(?:#([^}]+))?\}\}", src, text)
    for key, url in (("{{report}}", f"{site.gh}/issues/new?template={posixpath.basename(ISSUE_FORM)}"),
                     ("{{security}}", f"{site.gh}/security/advisories/new")):
        body = body.replace(key, h(url))
    left = re.findall(r"\{\{[^}]*\}\}", body)
    if left:
        raise SystemExit(f"build.py: {TESTERS_SRC}: unknown placeholders {left}")
    nav = [("steps", "What to try"), ("bridge-times", "Bridge times"), ("limits", "Limits"), ("safety", "Safety"),
           ("report", "Report")]
    st = model["stamp"]
    header = ('<a class="skip" href="#main">Skip to content</a><header class="top"><div class="wrap top-in">'
              '<a class="brand" href="../" aria-label="Ferrata, the build page"><img src="../assets/logo.svg" alt="" '
              'width="28" height="28"><span>Ferrata</span></a><nav aria-label="Sections"><ul>'
              + "".join(f'<li><a href="#{k}">{h(v)}</a></li>' for k, v in nav) + "</ul></nav></div></header>")
    footer = (f'<footer class="foot"><div class="wrap"><p>This page is <a href="{h(site.src(TESTERS_SRC))}"><code>{TESTERS_SRC}'
              f'</code></a>, built by <code>site/build.py</code> from the repository at commit <code data-built-from='
              f'"{h(st["commit"])}">{h(st["short"])}</code> ({h(st["date"])}){" with uncommitted changes" if st["dirty"] else ""}. '
              'Each number on it names its source in an HTML comment (view the page’s source); '
              '<code>site/test_site.py</code> checks that each source still holds it.</p>'
              '<p>No scripts, no cookies, no analytics: the page loads its stylesheet and logo from this host, nothing '
              'else. The report form is on GitHub.</p><p>Code and text MIT OR Apache-2.0.</p></div></footer>')
    desc = ("Testers of Ferrata's LEZ test network: what to try, in order, what each step should show, the bridge's "
            "waiting times, the limits, safety, and how to report.")
    return page(site, "Testers: start here · Ferrata’s LEZ test network", header + '<main id="main">\n' + body.strip()
                + "\n</main>\n" + footer, desc, root="../", script=False)


def evidence_json(site):
    md = site.md("docs/EVIDENCE.md")
    out = {}
    for n, e in site.evidence.items():
        out[str(n)] = {
            "title": e["title"],
            "where": md.render(e["where"]),
            "proofs": md.render(e["proofs"]),
            "recheck": md.render(e["recheck"]),
        }
    return json.dumps(out, ensure_ascii=False, separators=(",", ":")) + "\n"


def generated_css(site):
    lines = ["/* generated by site/build.py: the proportions of this build's bars and timeline, and the bar widths from which "
             "each count fits inside its part */"]
    for n in sorted(site.css_flex):
        lines.append(f".g{n}{{flex-grow:{n}}}")
    for start, span in sorted(site.css_grid):
        lines.append(f".gs{start}-{span}{{grid-column:{start} / span {span}}}")
    for rem in sorted(site.css_fit):
        lines.append(f"@container (min-width: {rem}rem) {{ .fit{rem} > .seg-n {{ display: inline-block }} }}")
    return "\n".join(lines) + "\n"


def stamp():
    commit = git("log", "-1", "--format=%H", "--", ".", f":(exclude){OUT_REL}").strip()
    date = git("log", "-1", "--format=%cs", commit).strip()
    dirty = bool(git("status", "--porcelain", "--", ".", f":(exclude){OUT_REL}").strip())
    status_commit = git("log", "-1", "--format=%H", "--", "docs/requirements-status.md").strip()
    status_date = git("log", "-1", "--format=%cs", status_commit).strip()
    return {"commit": commit, "short": commit[:8], "date": date, "dirty": dirty, "status_commit": status_commit,
            "status_date": status_date}


def build():
    site = Site()
    evidence_intro = load_evidence(site)
    rows, counts_doc, rfp_words = load_requirements(site)
    load_row_tests(site, rows)
    status_text = site.repo.text("docs/requirements-status.md")
    definition = status_text.split("\n\n", 2)[1].strip() if status_text.count("\n\n") >= 2 else ""
    intro = re.sub(r"\s+", " ", evidence_intro)
    m = re.search(r"\*\*Since [^*]+:\*\*\s*(.+?\.)(?=\s+[A-Z])", intro)
    risc0 = m.group(1) if m else ""
    if "RISC Zero" not in risc0:
        raise SystemExit("build.py: EVIDENCE.md's introduction no longer states which RISC Zero its proofs use; update the reading")
    later = sorted(n for n, e in site.evidence.items() if re.search(r"RISC Zero 3\.[1-9]", e["title"]))
    ev_md = site.md("docs/EVIDENCE.md")
    risc0_html = ("The evidence index: “" + ev_md.render(risc0) + "”" + (
        " Later rows record RISC Zero 3.1 work: " + ", ".join(f'<a class="ev" href="evidence.html#e{n}" data-ev="{n}">row {n}</a>'
                                                              for n in later) + "." if later else ""))
    r12 = next(r for r in rows if r["rfp"] == "RFP-021" and r["id"] == "R.12")
    r12_html = (f'<p class="small"><a href="#rfp-021-r-12">RFP-021 R.12</a> asks for CI green on the default branch: '
                f'{chip(r12["status"])} {site.md("docs/requirements-status.md").render(first_sentence(r12["why"], 240))}</p>')
    sec_text = site.repo.text("SECURITY.md").split("\n\n")
    security_says = re.sub(r"\s+", " ", next(b for b in sec_text if b.strip() and not b.startswith("#")))
    hb = site.repo.text("docs/how-it-was-built.md")
    rb = hb.split("## Reviews by agents", 1)[1].strip().split("\n\n")[0]
    review_says = re.sub(r"\s+", " ", rb)
    model = {
        "risc0_html": risc0_html, "r12_html": r12_html, "security_says": security_says, "review_says": review_says,
        "rows": rows, "counts": counts_doc, "rfp_words": rfp_words, "stamp": stamp(),
        "proposal": load_proposal(site), "security": load_security(site), "not_built": load_not_built(site),
        "runs": load_runs(site), "evidence_intro": evidence_intro, "definition": definition,
    }
    # The document's own counts must equal its rows.
    for rfp in RFPS:
        hard = [r for r in rows if r["rfp"] == rfp and not r["soft"]]
        got = count_rows(hard)
        want = counts_doc.get(rfp)
        if not want or any(got[s] != want[s] for s in STATUSES) or len(hard) != want["total"]:
            raise SystemExit(f"build.py: {rfp}: rows count {got} ({len(hard)}) but the Counts table says {want}")
    files = {}
    files["index.html"] = render_index(site, model).encode()
    files["evidence.html"] = render_evidence(site, model).encode()
    files["evidence.json"] = evidence_json(site).encode()
    files[f"{TESTERS_DIR}/index.html"] = render_testers(site, model).encode()
    css = (SITE / "assets" / "site.css").read_text(encoding="utf-8") + "\n" + generated_css(site)
    files["assets/site.css"] = css.encode()
    for name in ("site.js", "logo.svg", "nojs.css"):
        files[f"assets/{name}"] = (SITE / "assets" / name).read_bytes()
    for name, data in site.images.items():
        files[name] = data
    hard_counts = {rfp: count_rows([r for r in rows if r["rfp"] == rfp and not r["soft"]]) for rfp in RFPS}
    files["build.json"] = (json.dumps({
        "commit": model["stamp"]["commit"], "date": model["stamp"]["date"], "dirty": model["stamp"]["dirty"],
        "statuses_from": model["stamp"]["status_commit"],
        "hard": {rfp: dict(hard_counts[rfp], total=sum(hard_counts[rfp].values())) for rfp in RFPS},
        "soft": {rfp: count_rows([r for r in rows if r["rfp"] == rfp and r["soft"]]) for rfp in RFPS},
        "ci": {rfp: sum(1 for r in rows if r["rfp"] == rfp and r["category"] == "ci") for rfp in RFPS},
        "rows": len(rows), "evidence_rows": len(site.evidence), "warnings": site.warnings,
        "files": sorted(set(files) | {"build.json"}),
    }, indent=1) + "\n").encode()
    return files, model, site


LEGACY_FILES = ["index.html", "evidence.html", "evidence.json", "build.json", "assets/site.css", "assets/site.js",
                "assets/logo.svg", "assets/nojs.css"]


def main(argv):
    out = ROOT / OUT_REL
    if "--out" in argv:
        out = pathlib.Path(argv[argv.index("--out") + 1]).resolve()
    files, model, site = build()
    if "--check" in argv:
        stale = [n for n, d in files.items() if not (out / n).is_file() or (out / n).read_bytes() != d]
        extra = [p.relative_to(out).as_posix() for p in out.rglob("*") if p.is_file() and p.relative_to(out).as_posix() not in files]
        for n in stale:
            print(f"stale: {n}", file=sys.stderr)
        for n in extra:
            print(f"not built by site/build.py: {n}", file=sys.stderr)
        if stale or extra:
            print("build.py --check: the bundle differs from a fresh build; run python3 -I site/build.py", file=sys.stderr)
            return 1
        print(f"build.py --check: the bundle in {out.relative_to(ROOT) if out.is_relative_to(ROOT) else out} is fresh ({len(files)} files)")
        return 0
    previous = []
    if out.exists():
        if not out.is_dir():
            print(f"build.py: {out} is not a directory", file=sys.stderr)
            return 2
        marker = out / "build.json"
        if any(out.iterdir()) and not marker.is_file():
            print(f"build.py: refusing to write into {out}: it is not empty and holds no build.json (not a bundle)",
                  file=sys.stderr)
            return 2
        if marker.is_file():
            try:
                old = json.loads(marker.read_text(encoding="utf-8"))
            except ValueError:
                print(f"build.py: refusing to write into {out}: its build.json is not a bundle's", file=sys.stderr)
                return 2
            previous = old.get("files") or LEGACY_FILES + sorted(
                q.relative_to(out).as_posix() for q in (out / "img").glob("*") if re.match(r"^[0-9a-f]{12}\.(png|jpe?g)$", q.name))
    for name in previous:
        if name not in files and re.match(r"^[A-Za-z0-9_./-]+$", name) and ".." not in name.split("/"):
            stale = out / name
            if stale.is_file():
                stale.unlink()
    for name, data in files.items():
        dest = out / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        if not dest.exists() or dest.read_bytes() != data:
            dest.write_bytes(data)
    size = sum(len(d) for d in files.values())
    print(f"build.py: {len(files)} files, {size / 1024:.0f} KiB -> {out.relative_to(ROOT) if out.is_relative_to(ROOT) else out}; "
          f"{len(model['rows'])} rows, {len(site.evidence)} evidence rows, {len(site.warnings)} warnings")
    for w in site.warnings:
        print(f"  warning: {w}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
