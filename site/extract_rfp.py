#!/usr/bin/env python3
"""Extract the numbered requirements of RFP-021 and RFP-022, word for word, into site/data/rfp-requirements.json.

The build page (site/build.py) shows each requirement as the RFP words it, beside the project's status for it. The
RFP texts live in Logos's public repository https://github.com/logos-co/rfp (MIT OR Apache-2.0); this script reads a
checkout of it and keeps only the "Scope of Work" lists (hard requirements by section, then the soft requirements),
with the commit they were read at, so the page never paraphrases a requirement.

Usage: python3 -I site/extract_rfp.py RFP_CHECKOUT [--out site/data/rfp-requirements.json]
       (RFP_CHECKOUT: a clone of logos-co/rfp; its HEAD commit is recorded)
Standard library only.
"""

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "site" / "data" / "rfp-requirements.json"

RFPS = {
    "RFP-021": "RFPs/RFP-021-ethereum-state-attestation.md",
    "RFP-022": "RFPs/RFP-022-wrapped-erc20.md",
}
# Section heading (without a leading "+ ") -> the row prefix used by docs/traceability.md.
PREFIX = {
    "Functionality": "F",
    "Usability": "U",
    "Reliability": "R",
    "Performance": "P",
    "Supportability": "S",
    "Verification Security": "VS",
    "Security": "Sec",
    "Privacy Preservation": "Priv",
    "Soft Requirements": "Soft",
}
EXPECTED = {
    "RFP-021": {"F": 15, "U": 6, "R": 12, "P": 9, "S": 11, "VS": 6, "Soft": 5},
    "RFP-022": {"F": 17, "U": 10, "R": 14, "P": 7, "S": 4, "Sec": 9, "Priv": 7, "Soft": 5},
}
ITEM = re.compile(r"^(\d+)\.\s+(.*)$")
HEADING = re.compile(r"^(#{2,4})\s+(.*?)\s*$")


def extract(text):
    """{row id: {"section", "n", "text", "line"}} from one RFP's markdown."""
    rows = {}
    lines = text.splitlines()
    in_scope = False
    section = None
    current = None

    def close():
        nonlocal current
        if current is not None:
            body = "\n".join(current["lines"]).strip()
            # Continuation lines are indented under the item; keep sub-list breaks, join wrapped prose.
            out = []
            for raw in body.splitlines():
                s = raw.strip()
                if not s:
                    out.append("")
                elif s.startswith(("- ", "* ")) or re.match(r"^\d+\.\s", s):
                    out.append(s)
                elif out and out[-1] != "":
                    out[-1] += " " + s
                else:
                    out.append(s)
            rows[current["id"]] = {
                "section": current["section"],
                "n": current["n"],
                "text": "\n".join(out).strip(),
                "line": current["line"],
            }
            current = None

    for number, line in enumerate(lines, 1):
        h = HEADING.match(line)
        if h:
            level, title = len(h.group(1)), h.group(2)
            if level == 2:
                close()
                in_scope = "Scope of Work" in title
                section = None
            elif in_scope and level == 3:
                close()
                section = "Soft Requirements" if title == "Soft Requirements" else None
            elif in_scope and level == 4:
                close()
                name = title.lstrip("+ ").strip()
                section = name if name in PREFIX else None
            continue
        if not in_scope or section is None:
            continue
        m = ITEM.match(line)
        if m:
            close()
            n = int(m.group(1))
            current = {"id": f"{PREFIX[section]}.{n}", "section": section, "n": n, "line": number, "lines": [m.group(2)]}
        elif current is not None:
            if line.startswith((" ", "\t")) or not line.strip():
                current["lines"].append(line)
            else:
                close()
    close()
    return rows


def main(argv):
    if not argv or argv[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    src = pathlib.Path(argv[0])
    out = OUT
    if "--out" in argv:
        out = pathlib.Path(argv[argv.index("--out") + 1])
    commit = subprocess.run(["git", "-C", str(src), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    listing = subprocess.run(["git", "-C", str(src), "ls-tree", "--name-only", "HEAD", "RFPs/"], capture_output=True, text=True,
                             check=True).stdout.split()
    data = {
        "source": {
            "repository": "https://github.com/logos-co/rfp",
            "commit": commit,
            "license": "MIT OR Apache-2.0 (the repository's LICENSE-MIT.txt and LICENSE-APACHEv2.txt)",
            "note": "Each requirement's text as the RFP words it, from the RFP's Scope of Work; extracted by site/extract_rfp.py",
            "files": sorted(listing),
        },
        "rfps": {},
    }
    problems = []
    for rfp, rel in RFPS.items():
        text = (src / rel).read_text(encoding="utf-8")
        title = next((line[2:].strip() for line in text.splitlines() if line.startswith("# ")), rfp)
        rows = extract(text)
        counts = {}
        for rid in rows:
            counts[rid.split(".")[0]] = counts.get(rid.split(".")[0], 0) + 1
        if counts != EXPECTED[rfp]:
            problems.append(f"{rfp}: counts {counts} != {EXPECTED[rfp]}")
        data["rfps"][rfp] = {"title": title, "file": rel, "rows": rows}
    if problems:
        for p in problems:
            print("ERROR:", p, file=sys.stderr)
        return 1
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    total = sum(len(r["rows"]) for r in data["rfps"].values())
    print(f"extract_rfp.py: {total} requirements from {commit[:12]} -> {out.relative_to(ROOT) if out.is_relative_to(ROOT) else out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
