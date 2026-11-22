#!/usr/bin/env python3
"""The design decision log (docs/decisions/) is complete, sourced and in step with the repository.

RFP-021 Supportability #11 and RFP-022 Supportability #4 ask for a log "recording each major design decision and
choice: the context, the options considered, the option chosen, and the rationale and trade-offs", "updated as
decisions are made and current at each milestone delivery". This check fails (exit 1, one line per finding) if:

* a record is not `docs/decisions/NNNN-slug.md` with numbers 0001, 0002, ... without a gap, or its title is not
  `# ADR NNNN: ...` with its own number;
* a record lacks its Date (with a day, month and year), Status or Requirements line, or a status is not one of the
  README's (accepted ..., design only, superseded by ADR n naming an existing record);
* a record lacks one of the sections Context, Options considered, Decision, Rationale and trade-offs, Sources, or one
  is empty, or Context or the rationale says too little to be one (fewer than 12 words), or the decision fewer than 2;
* there are fewer than two options, an option says fewer than four words, or an option other than the chosen one (the
  last) does not say where it is recorded (`*Recorded:*`, `*Recorded later:*`), which RFP clause rules it out
  (`*Ruled out:*`), or that it is `*Reconstructed for contrast*`; a recorded option must name a source that resolves,
  a ruled-out one an RFP requirement that exists; "records no alternative" is not an option;
* a source, or a path anywhere in a record, names a file the repository does not track (a path with a slash under
  any top-level directory, or under an unknown one when it ends in a file extension; in the text, a bare file name
  such as `FerrataVault.sol` that no tracked file has; a full stop after a path is not part of it), a line (`path:N`, `path:N-M`) past the end of its file, a
  section (§n) its Markdown file has no heading for or a § of a file that is not Markdown, a quoted phrase its file
  does not contain (a quote with "…", or quotes joined by one, must have its parts in order, each within 300
  characters of the one before), a
  `name` its file does not contain as a whole word, an EVIDENCE row that docs/EVIDENCE.md does not have, or a change
  Dn the bridge specification does not have;
* a record names a git branch anywhere ("branch" or "branches", except a chain's or a proof's: canonical, Merkle,
  finality, execution, proof), or links to GitHub at a ref that is not a commit (`/tree/`, `/blob/`, `/commits/`,
  `/compare/` with a name): a record describes only work in this tree;
* a requirement is not an ID of docs/traceability.md (F.1, Sec.8, Soft.3, ...) under the RFP named before it;
* a record or the README names an ADR (`ADR 0029`, `ADRs 0003 and 0011`, `ADRs 0027–0031`) that is not in the log;
* the README's index does not link every record exactly once, or one of its cells (title, date, status,
  requirements) differs from the record's;
* a change D1, D2, ... of the bridge specification (docs/design/spec/bridge-spec-v3-changes.md) is cited in no
  record's Sources.

It checks form and references, not truth: a false statement in a record's own words passes, as does a line number
that exists but says something else, a quote taken out of its context, or a source that exists but is beside the
point. Those are for the reviewer.

    python3 -I scripts/check-decisions.py

The checks are `check(repo)`, on a `Repo` read from disk here; `scripts/test_check_decisions.py` runs them on records
built in memory.
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SECTIONS = ["Context", "Options considered", "Decision", "Rationale and trade-offs", "Sources"]
STATUS = re.compile(r"^(accepted\b.*|design only\b.*|superseded by ADR (\d{4})\b.*)$")
ROW_ID = re.compile(r"^(F|U|R|P|S|VS|Sec|Priv|Soft)\.\d+$")
MONTHS = "January|February|March|April|May|June|July|August|September|October|November|December"
DATE = re.compile(rf"\b\d{{1,2}}(?:[–-]\d{{1,2}})? (?:{MONTHS}) \d{{4}}\b")
MARK = re.compile(r"\*(Recorded|Recorded later|Ruled out):\*|\*Reconstructed for contrast\*")
PATH = re.compile(r"(?<![\w./-])((?:[\w.-]+/)+(?:[\w.-]+)?|[\w.-]+\.(?:md|rs|sol|sh|py|toml|json))(?![\w-])")
SKIP_PATH = re.compile(r"^(https?:|[\d.]+$|v\d)|\.\.\.")
FILE_EXT = re.compile(r"\.(?:md|rs|sol|sh|py|toml|json|ya?ml|txt|tsv|csv|js|ts|lock|nix|html)$")
LINES = re.compile(r":(\d+)(?:[–-](\d+))?(?!\w)")
CHAIN_BRANCH = {"canonical", "merkle", "finality", "execution", "proof"}
GIT_REF = re.compile(r"https?://github\.com/[^/\s]+/[^/\s]+/(?:tree|blob|commits?|compare)/([^/\s)>\]#]+)")
NEAR = 300
ADR_REF = re.compile(r"\bADRs? (\d{4}(?:(?:, | and | or |[–-]| to )\d{4})*)")


def plain(text):
    """Text as compared with a quote: no Markdown emphasis or code marks, one space for any run of white space."""
    return re.sub(r"\s+", " ", re.sub(r"[*`_]", "", text))


class Repo:
    """What the checks read: the records, the index, the tracked files and the files they cite."""

    def __init__(self, records, readme, files, trace_ids, evidence_rows, spec, read):
        self.records = records          # name -> text
        self.readme = readme
        self.files = files              # tracked paths
        self.dirs = set()
        for f in files:
            d = pathlib.PurePosixPath(f).parent
            while str(d) not in ("", "."):
                self.dirs.add(str(d))
                d = d.parent
        self.trace_ids = trace_ids      # {"RFP-021": set(), "RFP-022": set()}
        self.evidence_rows = evidence_rows
        self.spec = spec                # bridge-spec-v3-changes.md
        self.read = read                # path -> text of a tracked file
        self.tops = {f.split("/")[0] for f in files if "/" in f}
        self.basenames = {}
        for f in files:
            self.basenames.setdefault(f.rsplit("/", 1)[-1], []).append(f)


def sections(text):
    out, cur = {}, None
    for line in text.splitlines():
        m = re.match(r"^## (.+?)\s*$", line)
        if m:
            cur = m.group(1)
            out[cur] = []
        elif cur is not None:
            out[cur].append(line)
    return {k: "\n".join(v).strip() for k, v in out.items()}


def words(text):
    return len(re.findall(r"[A-Za-z0-9`]+", text))


def requirement_findings(where, value, repo):
    out, rfp = [], None
    value = re.sub(r"\([^)]*\)", " ", value)
    for tok in re.split(r"[,;\s]+", value):
        if not tok:
            continue
        if tok in ("RFP-021", "RFP-022"):
            rfp = tok
        elif ROW_ID.match(tok):
            if rfp is None:
                out.append(f"{where}: requirement {tok} without an RFP before it")
            elif tok not in repo.trace_ids[rfp]:
                out.append(f"{where}: {rfp} {tok} is not a row of docs/traceability.md")
        else:
            out.append(f"{where}: '{tok}' is not a requirement ID (F.1, Sec.8, Soft.3, ...)")
    return out


def adr_findings(where, text, repo):
    """Every ADR the text names (one, a list, or a range) is a record of the log."""
    out = []
    for m in ADR_REF.finditer(text):
        nums, seps = re.findall(r"\d{4}", m.group(1)), re.findall(r"\d{4}(, | and | or |[–-]| to )", m.group(1))
        named = {int(nums[0])}
        for k, sep in enumerate(seps):
            a, b = int(nums[k]), int(nums[k + 1])
            named |= set(range(a, b + 1)) if sep.strip() in ("–", "-", "to") else {b}
        for n in sorted(named):
            if not any(r.startswith(f"{n:04d}-") for r in repo.records):
                out.append(f"{where}: names ADR {n:04d}, which is not in the log")
    return out


def path_ok(p, repo):
    return p.rstrip("/") in repo.files or p.rstrip("/") in repo.dirs


def paths_in(text, repo):
    """The repository paths a text names, with the lines it gives after a colon (or None): tokens with a slash under a
    top-level directory, or under any directory when they end in a file extension, and file names. A full stop after a
    path ends the sentence; it is not part of the path."""
    out, flat = [], text.replace("`", " ")
    for m in PATH.finditer(flat):
        p = m.group(1)
        if p.endswith(".") and not p.endswith(".."):
            p = p[:-1]
        if SKIP_PATH.search(p) or ("/" in p and p.split("/")[0] not in repo.tops and not FILE_EXT.search(p)):
            continue
        lm = LINES.match(flat, m.end(1))
        lines = tuple(int(x) for x in lm.groups() if x) if lm else None
        out.append((m.start(1), p, lines))
    return out


def line_findings(where, path, lines, repo):
    if not lines or path.rstrip("/") not in repo.files:
        return []
    n = len(repo.read(path).splitlines())
    if max(lines) > n or min(lines) < 1 or (len(lines) == 2 and lines[0] > lines[1]):
        return [f"{where}: {path}:{'-'.join(map(str, lines))} is not within its {n} lines"]
    return []


def branch_findings(where, text):
    """A git branch named in the text (a chain's or a proof's branch is not one)."""
    out = []
    for m in re.finditer(r"\bbranch(?:es)?\b", text, re.I):
        before = re.search(r"([\w'’]+)[^\w'’]*$", text[:m.start()])
        if before and before.group(1).lower() in CHAIN_BRANCH:
            continue
        lo = max(0, m.start() - 30)
        out.append(f"{where}: names a branch ('…{plain(text[lo:m.end() + 30])}…'): a record describes only work in "
                   f"this tree")
    for m in GIT_REF.finditer(text):
        if not re.fullmatch(r"[0-9a-f]{7,40}", m.group(1)):
            out.append(f"{where}: links to GitHub at '{m.group(1)}', not at a commit: a record describes only work in "
                       f"this tree")
    return out


def quote_found(q, text):
    """The quote's parts (split at "…") in the text, in order, each within NEAR characters of the one before."""
    parts = [x.strip() for x in plain(q).split("…") if x.strip()]
    hay = plain(text)
    starts = [m.start() for m in re.finditer(re.escape(parts[0]), hay)] if parts else []
    for s in starts:
        end, ok = s + len(parts[0]), True
        for x in parts[1:]:
            k = hay.find(x, end)
            if k < 0 or k - end > NEAR:
                ok = False
                break
            end = k + len(x)
        if ok:
            return True
    return not parts


def ref_findings(where, ref, repo, need_source):
    """A reference (a source bullet, or the text after an option's mark): each path it names, and what it says of the
    path up to the next one: its sections (§n), quoted headings or phrases, `names`, and changes Dn."""
    out = []
    rows = re.search(r"\bEVIDENCE rows? ([\d–, -]+)", ref)
    if rows:
        for r in re.findall(r"\d+", rows.group(1)):
            if int(r) not in repo.evidence_rows:
                out.append(f"{where}: EVIDENCE row {r} does not exist")
    rfp = re.search(r"\bRFP-02[12] ((?:F|U|R|P|S|VS|Sec|Priv|Soft)\.\d+)", ref)
    if rfp:
        out += requirement_findings(where, rfp.group(0), repo)
    paths = paths_in(ref, repo)
    if need_source and not paths and not rows and not rfp:
        out.append(f"{where}: names no source ('{ref[:60]}')")
    for k, (at, path, lines) in enumerate(paths):
        if not path_ok(path, repo):
            out.append(f"{where}: {path} is not a tracked file")
            continue
        out += line_findings(where, path, lines, repo)
        if path.rstrip("/") not in repo.files:
            continue
        seg = ref[at + len(path): paths[k + 1][0] if k + 1 < len(paths) else len(ref)]
        seg = re.sub(r'"\s*…\s*"', " … ", seg)  # "a" … "b" is one quote with a gap
        quotes = re.findall(r'"([^"]{4,})"', seg)
        bare = re.sub(r'"[^"]*"', " ", seg)
        text = repo.read(path)
        if path.endswith(".md"):
            heads = [l for l in text.splitlines() if l.startswith("#")]
            for sec in re.findall(r"§\s?(\d+(?:\.\d+)*)", bare):
                if not any(re.match(rf"^#{{1,6}}\s+§?{re.escape(sec)}(?:\.|\s)", h) for h in heads):
                    out.append(f"{where}: {path} has no §{sec}")
        elif "§" in bare:
            out.append(f"{where}: a § of {path}, which is not Markdown (only Markdown headings are numbered)")
        if path == "docs/design/spec/bridge-spec-v3-changes.md":
            for d in re.findall(r"\bD(\d+)\b", bare):
                if not re.search(rf"^#+ D{d}\.", text, re.M):
                    out.append(f"{where}: the bridge specification has no change D{d}")
        for q in quotes:
            if not quote_found(q, text):
                out.append(f'{where}: {path} does not contain "{q[:50]}"' + (" (its parts in order, near)" if "…" in q else ""))
        for name in re.findall(r"`([A-Za-z_][A-Za-z0-9_]{3,})`", bare):
            if not re.search(rf"(?<![A-Za-z0-9_]){re.escape(name)}(?![A-Za-z0-9_])", text):
                out.append(f"{where}: {path} does not contain `{name}` (as a whole word)")
    return out


def check(repo):
    findings, numbers, meta_of, text_of = [], [], {}, {}
    for name in sorted(repo.records):
        m = re.match(r"^(\d{4})-[a-z0-9-]+\.md$", name)
        if not m:
            findings.append(f"{name}: not NNNN-slug.md")
            continue
        n = m.group(1)
        numbers.append(int(n))
        text = repo.records[name]
        text_of[n] = text
        first = text.splitlines()[0] if text else ""
        tm = re.match(rf"^# ADR {n}: (\S.*)$", first)
        if not tm:
            findings.append(f"{name}: the title is not '# ADR {n}: ...'")
        meta = {"Title": tm.group(1).strip() if tm else ""}
        for key in ("Date", "Status", "Requirements"):
            mm = re.search(rf"^- \*\*{key}:\*\* (.+)$", text, re.M)
            if not mm:
                findings.append(f"{name}: no '- **{key}:**' line")
            else:
                meta[key] = mm.group(1).strip()
        meta_of[n] = meta
        if meta.get("Date") and not DATE.search(meta["Date"]):
            findings.append(f"{name}: the date '{meta['Date']}' names no day, month and year")
        st = meta.get("Status", "")
        sm = STATUS.match(st)
        if st and not sm:
            findings.append(f"{name}: status '{st}' is not accepted / design only / superseded by ADR n")
        elif sm and sm.group(2) and not any(r.startswith(sm.group(2) + "-") for r in repo.records):
            findings.append(f"{name}: superseded by ADR {sm.group(2)}, which does not exist")
        if re.search(r"\bbranch\b", st):
            findings.append(f"{name}: the status cites a branch: a record describes only work in this tree")
        findings += requirement_findings(name, meta.get("Requirements", ""), repo)
        findings += adr_findings(name, text, repo)
        sec = sections(text)
        for s in SECTIONS:
            if s not in sec:
                findings.append(f"{name}: no '## {s}' section")
            elif not sec[s]:
                findings.append(f"{name}: section '{s}' is empty")
        order = [s for s in sec if s in SECTIONS]
        if order != [s for s in SECTIONS if s in sec]:
            findings.append(f"{name}: sections out of order: {order}")
        for s, least in (("Context", 12), ("Rationale and trade-offs", 12), ("Decision", 2)):
            if sec.get(s) and words(sec[s]) < least:
                findings.append(f"{name}: section '{s}' says too little ({words(sec[s])} words)")
        # Options: numbered items, each of four words or more; every one but the chosen (last) says where it stands.
        opts = re.split(r"^\d+\. ", sec.get("Options considered", ""), flags=re.M)[1:]
        if len(opts) < 2:
            findings.append(f"{name}: {len(opts)} option(s) considered; at least two")
        for i, opt in enumerate(opts, 1):
            if words(MARK.split(opt)[0]) < 4:
                findings.append(f"{name}: option {i} says too little")
            if re.search(r"records no alternative", opt, re.I):
                findings.append(f"{name}: option {i} says the source records no alternative: drop it or mark it")
            mk = MARK.search(opt)
            if i < len(opts) and not mk:
                findings.append(f"{name}: option {i} is neither recorded, ruled out nor marked reconstructed")
            elif mk and mk.group(1) in ("Recorded", "Recorded later"):
                findings += ref_findings(f"{name} option {i}", opt[mk.end():], repo, need_source=True)
            elif mk and mk.group(1) == "Ruled out":
                if not re.search(r"\bRFP-02[12] (?:F|U|R|P|S|VS|Sec|Priv|Soft)\.\d+", opt[mk.end():]):
                    findings.append(f"{name}: option {i} is ruled out by no RFP requirement")
                findings += ref_findings(f"{name} option {i}", opt[mk.end():], repo, need_source=False)
        # Sources: every reference resolves.
        srcs = [l[2:].strip() for l in sec.get("Sources", "").splitlines() if l.startswith("- ")]
        if not srcs:
            findings.append(f"{name}: no source listed")
        for src in srcs:
            findings += ref_findings(f"{name} source", src, repo, need_source=True)
        # Any other path the record names; a branch or a link to one anywhere but in the status (checked above).
        body = "\n".join(v for k, v in sec.items() if k not in ("Sources", "Options considered"))
        for _, p, lines in paths_in(body, repo):
            if "/" in p and not path_ok(p, repo):
                findings.append(f"{name}: names {p}, which is not a tracked file")
            elif "/" not in p and not path_ok(p, repo) and p not in repo.basenames:
                findings.append(f"{name}: names {p}, which no tracked file is called")
            elif "/" in p or len(repo.basenames.get(p, [])) == 1:
                findings += line_findings(name, p if "/" in p else repo.basenames[p][0], lines, repo)
        findings += branch_findings(name, re.sub(r"^- \*\*Status:\*\*.*$", "", text, flags=re.M))
    if numbers != list(range(1, len(numbers) + 1)):
        findings.append(f"record numbers are not 1..{len(numbers)} without a gap: {numbers}")
    # The index: every record once, every cell equal to the record's; every ADR it names in the log.
    findings += adr_findings("README", repo.readme, repo)
    linked = {}
    for line in repo.readme.splitlines():
        m = re.match(r"^\| \[(\d{4})\]\((\d{4}-[a-z0-9-]+\.md)\) \|(.*)\|\s*$", line)
        if not m:
            continue
        n, target, rest = m.groups()
        linked[n] = linked.get(n, 0) + 1
        if target not in repo.records or not target.startswith(n):
            findings.append(f"README: ADR {n} links to {target}, which is not its record")
        cells = [c.strip() for c in rest.split("|")]
        if n in meta_of:
            for key, cell in zip(("Title", "Date", "Status", "Requirements"), cells):
                if cell != meta_of[n].get(key, ""):
                    findings.append(f"README: ADR {n}'s {key.lower()} '{cell}' differs from the record's "
                                    f"'{meta_of[n].get(key, '')}'")
            if len(cells) != 4:
                findings.append(f"README: ADR {n}'s row has {len(cells)} cells after the link, not 4")
    for n in text_of:
        if linked.get(n, 0) != 1:
            findings.append(f"README: ADR {n} is linked {linked.get(n, 0)} times in the index")
    # Currency: every change of the bridge specification v3 is cited in some record's Sources.
    changes = sorted({int(d) for d in re.findall(r"^#+ .*?\bD(\d+)\b", repo.spec, re.M)})
    cited = set()
    for text in text_of.values():
        for src in sections(text).get("Sources", "").splitlines():
            if "bridge-spec-v3-changes.md" in src:
                cited |= {int(d) for d in re.findall(r"\bD(\d+)\b", src)}
    for d in changes:
        if d not in cited:
            findings.append(f"bridge-spec-v3-changes.md D{d} is cited in no record's Sources")
    return findings, len(text_of), len(changes)


def load(root=ROOT):
    out = subprocess.run(["git", "-C", str(root), "ls-files"], capture_output=True, text=True, check=True).stdout
    files = set(out.split("\n")) - {""}
    ids, rfp = {"RFP-021": set(), "RFP-022": set()}, None
    for line in (root / "docs" / "traceability.md").read_text().splitlines():
        m = re.match(r"^## (RFP-02[12])\b", line)
        if m:
            rfp = m.group(1)
        m = re.match(r"^\| ((?:F|U|R|P|S|VS|Sec|Priv|Soft)\.\d+) \|", line)
        if m and rfp:
            ids[rfp].add(m.group(1))
    ev = {int(m.group(1)) for m in re.finditer(r"^\| (\d+) \|", (root / "docs" / "EVIDENCE.md").read_text(), re.M)}
    dec = root / "docs" / "decisions"
    records = {p.name: p.read_text() for p in sorted(dec.glob("*.md")) if p.name != "README.md"}
    spec = (root / "docs" / "design" / "spec" / "bridge-spec-v3-changes.md").read_text()

    def read(path):
        try:
            return (root / path).read_text(errors="replace")
        except (IsADirectoryError, FileNotFoundError):
            return ""

    return Repo(records, (dec / "README.md").read_text(), files, ids, ev, spec, read)


def main():
    findings, n_records, n_changes = check(load())
    for f in findings:
        print(f)
    print(f"check-decisions: {n_records} records, {n_changes} specification changes, {len(findings)} findings")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
