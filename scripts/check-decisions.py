#!/usr/bin/env python3
"""The design decision log (docs/decisions/) is complete and in step with the repository.

RFP-021 Supportability #11 and RFP-022 Supportability #4 ask for a log "recording each major design decision and
choice: the context, the options considered, the option chosen, and the rationale and trade-offs", "updated as
decisions are made and current at each milestone delivery". This check fails (exit 1, one line per finding) if:

* a record is not `docs/decisions/NNNN-slug.md` with numbers 0001, 0002, ... without a gap, or its title is not
  `# ADR NNNN: ...` with its own number;
* a record lacks its Date, Status or Requirements line, or a status is not one of the README's (accepted, design
  only, superseded by ADR n naming an existing record);
* a record lacks one of the sections Context, Options considered (at least two options), Decision, Rationale and
  trade-offs, Sources, or one is empty;
* a source names a file the repository does not track, or an EVIDENCE row that docs/EVIDENCE.md does not have;
* a requirement ID (F.1, Sec.8, ...) is not a row of docs/traceability.md for the RFP named before it;
* the README's index does not link every record exactly once, or its status cell differs from the record's;
* a change D1, D2, ... of the bridge specification (docs/design/spec/bridge-spec-v3-changes.md) is cited by no record.

    python3 -I scripts/check-decisions.py
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEC = ROOT / "docs" / "decisions"
SECTIONS = ["Context", "Options considered", "Decision", "Rationale and trade-offs", "Sources"]
STATUS = re.compile(r"^(accepted\b.*|design only\b.*|superseded by ADR (\d{4})\b.*)$")
ROW_ID = re.compile(r"^(F|U|R|P|S|VS|Sec|Priv|Soft)\.\d+$")


def tracked():
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files"], capture_output=True, text=True, check=True).stdout
    files = set(out.split("\n")) - {""}
    dirs = {str(pathlib.PurePosixPath(f).parent) for f in files}
    while True:
        more = {str(pathlib.PurePosixPath(d).parent) for d in dirs} - dirs
        if not more:
            break
        dirs |= more
    return files, dirs


def trace_ids():
    ids, rfp = {"RFP-021": set(), "RFP-022": set()}, None
    for line in (ROOT / "docs" / "traceability.md").read_text().splitlines():
        m = re.match(r"^## (RFP-02[12])\b", line)
        if m:
            rfp = m.group(1)
        m = re.match(r"^\| ((?:F|U|R|P|S|VS|Sec|Priv|Soft)\.\d+) \|", line)
        if m and rfp:
            ids[rfp].add(m.group(1))
    return ids


def evidence_rows():
    return {int(m.group(1)) for m in re.finditer(r"^\| (\d+) \|", (ROOT / "docs" / "EVIDENCE.md").read_text(), re.M)}


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


def main():
    findings = []
    files, dirs = tracked()
    ids = trace_ids()
    ev = evidence_rows()
    records = sorted(p for p in DEC.glob("*.md") if p.name != "README.md")
    numbers, status_of, text_of = [], {}, {}
    for p in records:
        m = re.match(r"^(\d{4})-[a-z0-9-]+\.md$", p.name)
        if not m:
            findings.append(f"{p.name}: not NNNN-slug.md")
            continue
        n = m.group(1)
        numbers.append(int(n))
        text = p.read_text()
        text_of[n] = text
        first = text.splitlines()[0] if text else ""
        if not re.match(rf"^# ADR {n}: \S", first):
            findings.append(f"{p.name}: the title is not '# ADR {n}: ...'")
        meta = {}
        for key in ("Date", "Status", "Requirements"):
            mm = re.search(rf"^- \*\*{key}:\*\* (.+)$", text, re.M)
            if not mm:
                findings.append(f"{p.name}: no '- **{key}:**' line")
            else:
                meta[key] = mm.group(1).strip()
        st = meta.get("Status", "")
        sm = STATUS.match(st)
        if st and not sm:
            findings.append(f"{p.name}: status '{st}' is not accepted / design only / superseded by ADR n")
        elif sm and sm.group(2) and not list(DEC.glob(sm.group(2) + "-*.md")):
            findings.append(f"{p.name}: superseded by ADR {sm.group(2)}, which does not exist")
        status_of[n] = st
        # Requirement IDs: each bare ID belongs to the RFP named last before it.
        rfp = None
        for tok in re.split(r"[,;\s]+", meta.get("Requirements", "")):
            tok = tok.strip("()")
            if tok in ("RFP-021", "RFP-022"):
                rfp = tok
            elif ROW_ID.match(tok):
                if rfp is None:
                    findings.append(f"{p.name}: requirement {tok} without an RFP before it")
                elif tok not in ids[rfp]:
                    findings.append(f"{p.name}: {rfp} {tok} is not a row of docs/traceability.md")
        sec = sections(text)
        for s in SECTIONS:
            if s not in sec:
                findings.append(f"{p.name}: no '## {s}' section")
            elif not sec[s]:
                findings.append(f"{p.name}: section '{s}' is empty")
        order = [s for s in sec if s in SECTIONS]
        if order != [s for s in SECTIONS if s in sec]:
            findings.append(f"{p.name}: sections out of order: {order}")
        opts = sec.get("Options considered", "")
        n_opts = len(re.findall(r"^(?:\d+\.|-) ", opts, re.M))
        if n_opts < 2:
            findings.append(f"{p.name}: {n_opts} option(s) considered; at least two")
        srcs = [l[2:].strip() for l in sec.get("Sources", "").splitlines() if l.startswith("- ")]
        if not srcs:
            findings.append(f"{p.name}: no source listed")
        for src in srcs:
            if src.startswith("EVIDENCE row"):
                for r in re.findall(r"\d+", src.split("(")[0]):
                    if int(r) not in ev:
                        findings.append(f"{p.name}: EVIDENCE row {r} does not exist")
                continue
            path = re.split(r"[\s,;]", src.replace("`", ""), maxsplit=1)[0].rstrip(":").split("#")[0]
            if "/" in path or re.search(r"\.(md|rs|sol|sh|py|toml|json)$", path):
                if path.rstrip("/") not in files and path.rstrip("/") not in dirs:
                    findings.append(f"{p.name}: source {path} is not a tracked file")
    if numbers != list(range(1, len(numbers) + 1)):
        findings.append(f"record numbers are not 1..{len(numbers)} without a gap: {numbers}")
    # The index.
    readme = (DEC / "README.md").read_text()
    linked = {}
    for line in readme.splitlines():
        m = re.match(r"^\| \[(\d{4})\]\((\d{4}-[a-z0-9-]+\.md)\) \|(.*)\|\s*$", line)
        if not m:
            continue
        n, target, rest = m.groups()
        linked[n] = linked.get(n, 0) + 1
        if not (DEC / target).exists() or not target.startswith(n):
            findings.append(f"README: ADR {n} links to {target}, which is not its record")
        cells = [c.strip() for c in rest.split("|")]
        if len(cells) >= 3 and n in status_of and cells[2] != status_of[n]:
            findings.append(f"README: ADR {n}'s status '{cells[2]}' differs from the record's '{status_of[n]}'")
    for n in text_of:
        if linked.get(n, 0) != 1:
            findings.append(f"README: ADR {n} is linked {linked.get(n, 0)} times in the index")
    # Currency: every change of the bridge specification v3 is recorded.
    spec = (ROOT / "docs" / "design" / "spec" / "bridge-spec-v3-changes.md").read_text()
    changes = sorted({int(d) for d in re.findall(r"^#+ .*?\bD(\d+)\b", spec, re.M)})
    alltext = "\n".join(text_of.values())
    for d in changes:
        if not re.search(rf"\bD{d}\b", alltext):
            findings.append(f"bridge-spec-v3-changes.md D{d} is cited by no record")
    for f in findings:
        print(f)
    print(f"check-decisions: {len(text_of)} records, {len(changes)} specification changes, {len(findings)} findings")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
