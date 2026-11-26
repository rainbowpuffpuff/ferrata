"""The requirement rows (docs/requirements-status.md), the EVIDENCE rows (docs/EVIDENCE.md) and the row-to-test map
(scripts/ci/row-tests.toml), as scripts/verify.py reads them."""
from __future__ import annotations

import dataclasses
import re
import tomllib
from pathlib import Path

STATUSES = ("shown", "in part", "not yet", "waits on Logos")


@dataclasses.dataclass
class Row:
    rfp: str  # "RFP-021"
    id: str  # "F.4"
    section: str
    status: str
    soft: bool
    evidence: tuple  # EVIDENCE row numbers the status row cites
    text: str

    @property
    def key(self) -> str:
        return f"{self.rfp} {self.id}"


def _evidence_numbers(cell: str) -> tuple:
    """Row numbers in an Evidence cell: bare numbers and ranges (`43–48`), not those inside backticks (paths, runs)."""
    cell = re.sub(r"`[^`]*`", " ", cell)
    cell = re.sub(r"GitHub runs?[^;|]*", " ", cell)
    cell = re.sub(r"§\s*[\d.]+", " ", cell)
    out = []
    for a, b in re.findall(r"(?<![\w.])(\d{1,3})(?:\s*[–-]\s*(\d{1,3}))?(?![\w.])", cell):
        lo = int(a)
        hi = int(b) if b else lo
        if 0 < lo <= hi < 1000 and hi - lo < 50:
            out.extend(range(lo, hi + 1))
    return tuple(dict.fromkeys(out))


def status_rows(root: Path) -> list:
    rows = []
    rfp = section = None
    for line in (root / "docs/requirements-status.md").read_text().splitlines():
        if line.startswith("## RFP-021"):
            rfp = "RFP-021"
        elif line.startswith("## RFP-022"):
            rfp = "RFP-022"
        elif line.startswith("### "):
            section = line[4:].strip()
        elif rfp and section and line.startswith("| ") and not line.startswith("| ID") and not line.startswith("|---"):
            cells = [c.strip() for c in line.strip().strip("|").split(" | ")]
            if len(cells) < 4 or cells[1] not in STATUSES:
                continue
            rows.append(Row(rfp, cells[0], section, cells[1], section.startswith("Soft"),
                            _evidence_numbers(cells[-1]), cells[2]))
    return rows


@dataclasses.dataclass
class EvidenceRow:
    n: int
    what: str
    where: str
    proofs: str
    recheck: str
    details: str


def evidence_rows(root: Path) -> dict:
    out = {}
    for line in (root / "docs/EVIDENCE.md").read_text().splitlines():
        m = re.match(r"^\| (\d+) \| (.*)$", line)
        if not m:
            continue
        cells = [c.strip() for c in ("| " + m.group(2)).strip().strip("|").split(" | ")]
        if len(cells) < 6:
            continue
        what = re.sub(r"\*\*", "", cells[0])
        out[int(m.group(1))] = EvidenceRow(int(m.group(1)), what, cells[2], cells[3], cells[4], cells[5])
    return out


def row_tests(root: Path) -> list:
    with open(root / "scripts/ci/row-tests.toml", "rb") as fh:
        return tomllib.load(fh).get("row", [])


def normalise_row(text: str, rows: list) -> list:
    """`RFP-021 F.4`, `RFP-021:F.4`, `021:F.4`, `021/F.4`, `21-F.4` and (when only one RFP has it) `F.4`."""
    t = text.strip()
    m = re.match(r"^(?:RFP[-_ ]?)?0?(21|22)\s*[:/ _-]\s*([A-Za-z]+\.\d+)$", t, re.I)
    want_id = None
    want_rfp = None
    if m:
        want_rfp = "RFP-0" + m.group(1)
        want_id = m.group(2)
    elif re.match(r"^[A-Za-z]+\.\d+$", t):
        want_id = t
    else:
        return []
    out = []
    for r in rows:
        if r.id.lower() == want_id.lower() and (want_rfp is None or r.rfp == want_rfp):
            out.append(r)
    return out
