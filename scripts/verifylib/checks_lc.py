"""RFP-021 light-client records: rederive their case counts and figures.

These are deliberately record checks, not test-runner wrappers.  A check parses the
committed case tables and, where present, the raw run ledger; it also reads the exact
status-row text so prose and record cannot drift independently.  Summary-only claims
are identified in the result lines rather than presented as replayed computations.
"""
from __future__ import annotations

import csv
import hashlib
import io
import json
import operator
import os
import re
import subprocess
from collections import Counter
from pathlib import Path

from .core import check, fail_if
from .rows import status_rows

STATUS = "docs/requirements-status.md"
LC = "lez/results/lc-security"


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _row(ctx, row_id: str) -> str:
    rows = [r for r in status_rows(ctx.root) if r.rfp == "RFP-021" and r.id == row_id]
    if len(rows) != 1:
        raise ValueError(f"expected one RFP-021 {row_id} status row, found {len(rows)}")
    return rows[0].text


def _expect(problems: list, condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def _phrases(problems: list, text: str, row_id: str, *fragments: str) -> None:
    for fragment in fragments:
        _expect(problems, fragment in text, f"RFP-021 {row_id} no longer states {fragment!r}")


def _int(s: str) -> int:
    return int(s.replace(",", "").strip())


def _tables(text: str) -> list[dict]:
    """Parse pipe tables, retaining their headers (case text may contain apostrophes)."""
    out = []
    lines = text.splitlines()
    i = 0
    while i + 1 < len(lines):
        if lines[i].startswith("|") and re.match(r"^\|(?:\s*:?-+:?\s*\|)+$", lines[i + 1]):
            cells = lambda line: [c.strip() for c in line.strip().strip("|").split("|")]
            header = cells(lines[i])
            i += 2
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                row = cells(lines[i])
                if len(row) == len(header):
                    rows.append(row)
                i += 1
            out.append({"header": header, "rows": rows})
        else:
            i += 1
    return out


def _case_rows(text: str) -> list[list[str]]:
    return [row for table in _tables(text) if table["header"][:3] == ["Case", "Expected", "Observed through the program"]
            for row in table["rows"]]


def _check_expected_observed(problems: list, rel: str, rows: list[list[str]]) -> list[int]:
    codes = []
    for row in rows:
        case, expected, observed = row[:3]
        if expected.startswith("applied"):
            _expect(problems, observed == "applied", f"{rel}: {case}: expected applied, observed {observed!r}")
        elif expected.startswith("refused"):
            _expect(problems, observed.startswith("refused"), f"{rel}: {case}: expected refusal, observed {observed!r}")
        else:
            m = re.match(r"^(\d+) ([A-Za-z0-9_]+)$", expected)
            _expect(problems, m is not None, f"{rel}: {case}: unrecognized Expected cell {expected!r}")
            if m:
                code, name = int(m.group(1)), m.group(2)
                codes.append(code)
                _expect(problems, observed.startswith(f"refused, exit {code} {name} ("),
                        f"{rel}: {case}: Expected {code} {name}, observed {observed!r}")
    return codes


def _summary_tables(text: str) -> tuple[list[list[str]], dict[str, list[list[str]]]]:
    """The harness's transaction table and its separate per-handler ledger."""
    tables = _tables(text)
    summary = next(t["rows"] for t in tables if t["header"][:3] == ["#", "transaction", "handlers run (cycles each)"])
    ledger_rows = next(t["rows"] for t in tables if t["header"][:3] == ["transaction", "handler", "cycles"])
    ledger = {}
    for row in ledger_rows:
        ledger.setdefault(row[0], []).append(row)
    return summary, ledger


def _check_charges(problems: list, rel: str, summary: list[list[str]], ledger: dict) -> None:
    for row in summary:
        n, case, handlers, gas, pct, _stor, _fee, result = row[:8]
        cycles = [_int(re.search(r"([\d,]+)\s*$", part).group(1)) for part in handlers.split("→")]

        if not result.startswith("reverted:"):
            _expect(problems, sum(cycles) == _int(gas),
                    f"{rel} row {n}: handler cycles {cycles} sum to {sum(cycles)}, execution gas {gas}")
        own = ledger.get(case, [])
        _expect(problems, [int(r[2].replace(",", "")) for r in own] == cycles,
                f"{rel} row {n}: summary cycles {cycles} != per-handler ledger")
        m = re.search(r"refused: exit (\d+)", result)
        if m:
            _expect(problems, bool(own) and own[-1][-1] == m.group(1),
                    f"{rel} row {n}: result exit {m.group(1)} != per-handler exits")
        elif result == "applied":
            _expect(problems, bool(own) and all(r[-1] == "0" for r in own),
                    f"{rel} row {n}: applied but per-handler exits are not all zero")
        target = re.fullmatch(r"(\d+)%", pct)
        _expect(problems, target is not None and int(target.group(1)) == int(_int(gas) * 100 / 5_000_000 + 0.5),
                f"{rel} row {n}: rounded whole-percent charge {pct} does not match {gas} / 5M")


@check(id="lc-f2-finalised-only", title="F.2: every non-finalised-header case and its refusal code",
       family="record", mode="offline", tier="quick", rows=("RFP-021 F.2",), evidence=(39, 151),
       what="F.2's expected/observed case cells, refusal-code sequence and cited header-request refusals",
       against="lez/results/lc-security/f2-finalised-only.md, stage5-attest-refusals.md, stage4-attest-refusals.md "
               "and RFP-021 F.2's status text",
       inputs=(STATUS, f"{LC}/f2-finalised-only.md", "lez/results/stage5-attest-refusals.md",
               "lez/results/stage4-attest-refusals.md"))
def lc_f2(ctx):
    problems = []
    rel = f"{LC}/f2-finalised-only.md"
    rows = _case_rows(_text(ctx, rel))
    codes = _check_expected_observed(problems, rel, rows)
    _expect(problems, len(rows) == 9, f"F.2: 9 cases expected, found {len(rows)}")
    _expect(problems, codes == [50, 26, 1, 26, 25, 25, 213, 110], f"F.2 refusal sequence changed: {codes}")
    stage5, _ = _summary_tables(_text(ctx, "lez/results/stage5-attest-refusals.md"))
    stage4, _ = _summary_tables(_text(ctx, "lez/results/stage4-attest-refusals.md"))
    s5codes = [int(m.group(1)) for r in stage5 if (m := re.search(r"refused: exit (\d+)", r[7]))]
    s4codes = [int(m.group(1)) for r in stage4 if (m := re.search(r"refused: exit (\d+)", r[7]))]
    _expect(problems, 215 in s5codes and s5codes.count(120) == 4, f"F.2 cited stage5 codes absent: {s5codes}")
    _expect(problems, 150 in s4codes and 151 in s4codes, "F.2 cited EIP-2935 codes 150/151 absent")
    _phrases(problems, _row(ctx, "F.2"), "F.2", "refused (50; 26, 1; 26; 25)", "(213, 110)",
             "(215, 120; 150, 151)")
    fail_if(problems, "F.2 record/status mismatch")
    return [f"9 cases: {len(codes)} refusals in order {codes}, one finalized-header control applied",
            "cited header-request tables also hold 215, four 120s, and EIP-2935 codes 150 and 151",
            "per-case verdict/code consistency and the exact status-row code groups checked"]


@check(id="lc-f4-comparisons", title="F.4: evaluate all 36 boundary predicates independently",
       family="record", mode="offline", tier="quick", rows=("RFP-021 F.4",), evidence=(151,),
       what="all six operators against constants and between facts, evaluated from the three recorded values",
       against="f4-comparisons.md's 36 expected/observed rows and RFP-021 F.4's 36-case / refusal-130 text",
       inputs=(STATUS, f"{LC}/f4-comparisons.md"))
def lc_f4(ctx):
    problems = []
    rel = f"{LC}/f4-comparisons.md"
    text = _text(ctx, rel)
    rows = _case_rows(text)
    values = [32_141_291, 19_981_261_002, 32_141_291]
    balance = 2_106_351_906_124_325_086_873_169
    _expect(problems, "2106351906124325086873169 wei" in text, "F.4: recorded balance changed")
    _expect(problems, "32141291 and 19981261002; fact 2 is the first again" in text,
            "F.4: recorded Transfer amounts/fact mapping changed")
    ops = {"Eq": operator.eq, "Ne": operator.ne, "Lt": operator.lt,
           "Le": operator.le, "Gt": operator.gt, "Ge": operator.ge}
    counts = Counter()
    for row in rows:
        case, expected, observed = row[:3]
        op_name = next((op for op in ops if f" {op} " in case), None)
        if op_name is None:
            problems.append(f"F.4: no comparison operator in {case!r}")
            continue
        relation = case.startswith("relation:")
        if relation:
            m = re.fullmatch(r"relation: amount of fact (\d+) \w+ amount of fact (\d+)", case)
            if not m:
                problems.append(f"F.4: unrecognized relation {case!r}")
                continue
            left, right = values[int(m.group(1))], values[int(m.group(2))]
        elif case.startswith("WETH's balance"):
            left = balance
            rhs = case.split(f" {op_name} ", 1)[1]
            delta = {"balance": 0, "balance+1": 1, "balance-1": -1}.get(rhs)
            if delta is None:
                problems.append(f"F.4: unrecognized right-hand side {rhs!r} in {case!r}")
                continue
            right = balance + delta
        elif case.startswith("Transfer amount"):
            left = values[0]
            rhs = case.split(f" {op_name} ", 1)[1]
            delta = {"amount": 0, "amount+1": 1, "amount-1": -1}.get(rhs)
            if delta is None:
                problems.append(f"F.4: unrecognized right-hand side {rhs!r} in {case!r}")
                continue
            right = values[0] + delta
        else:
            problems.append(f"F.4: unrecognized constant comparison {case!r}")
            continue
        holds = ops[op_name](left, right)
        wanted = "applied" if holds else "130 PredicateFailed"
        _expect(problems, expected == wanted, f"F.4: {case}: independently {wanted}, Expected says {expected}")
        wanted_observed = "applied" if holds else "refused, exit 130 PredicateFailed"
        _expect(problems, observed.startswith(wanted_observed),
                f"F.4: {case}: independently {wanted_observed}, observed {observed}")
        counts[(op_name, "relation" if relation else "constant", "applied" if holds else "refused")] += 1
    _expect(problems, len(rows) == 36, f"F.4: found {len(rows)} cases, not 36")
    for op_name in ops:
        _expect(problems, sum(v for (op, kind, _), v in counts.items() if op == op_name and kind == "constant") == 4,
                f"F.4: {op_name} does not have four constant cases")
        _expect(problems, sum(v for (op, kind, _), v in counts.items() if op == op_name and kind == "relation") == 2,
                f"F.4: {op_name} does not have two relation cases")
    applied = sum(v for (_, _, result), v in counts.items() if result == "applied")
    refused = sum(v for (_, _, result), v in counts.items() if result == "refused")
    _phrases(problems, _row(ctx, "F.4"), "F.4", "All six comparisons", "36 cases", "refused (130)")
    fail_if(problems, "F.4 evaluation mismatch")
    return [f"36 evaluated predicates: 24 constant + 12 relation; {applied} applied, {refused} refused with 130",
            "Eq/Ne/Lt/Le/Gt/Ge each independently checked at both sides of its boundary"]


@check(id="lc-f8-full-diff", title="F.8: full-diff counts, searches, charges and state-diff table",
       family="record", mode="offline", tier="quick", rows=("RFP-021 F.8",), evidence=(5, 73, 123, 152),
       what="transaction/proof byte figures, field/search counts, equal charges, two public-account diffs and control",
       against="f8-full-diff.md's per-field lists and state table and RFP-021 F.8's status text",
       inputs=(STATUS, f"{LC}/f8-full-diff.md"))
def lc_f8(ctx):
    problems = []
    text = _text(ctx, f"{LC}/f8-full-diff.md")
    searches = re.findall(r"^- ([AD]): (.+?) \((\d+) bytes\): absent$", text, re.M)
    _expect(problems, len(searches) == 13, f"F.8: found {len(searches)} absence checks, not 13")
    _expect(problems, sum(1 for who, _, _ in searches if who == "A") == 5, "F.8: A does not have five searches")
    _expect(problems, sum(1 for who, _, _ in searches if who == "D") == 8, "F.8: D does not have eight searches")
    hops = [int(m.group(1)) for who, what, _ in searches if who == "D" and (m := re.match(r"hop (\d+)'s", what))]
    _expect(problems, hops == [0, 1, 2], f"F.8: missing hop headers in absence searches: {hops}")
    repeated = {
        "aliases": _int(re.search(r"^- (\d+) alias\(es\)", text, re.M).group(1)),
        "notes": _int(re.search(r"^- (\d+) encrypted notes", text, re.M).group(1)),
        "dummies": _int(re.search(r"^- (\d+) dummy actions", text, re.M).group(1)),
        "recovery": _int(re.search(r"^- (\d+) recovery binding\(s\)", text, re.M).group(1)),
        "own_actions": _int(re.search(r"account's own (\d+) action\(s\)", text).group(1)),
        "own_commitments": _int(re.search(r"and (\d+) commitment\(s\), the states made", text).group(1)),
    }
    _expect(problems, repeated == {"aliases": 5, "notes": 8, "dummies": 6, "recovery": 1,
                                   "own_actions": 2, "own_commitments": 2}, f"F.8 repeated-proof counts: {repeated}")
    size = _int(re.search(r"Both are ([\d,]+) bytes", text).group(1))
    proof = _int(re.search(r"proof field ([\d,]+) bytes", text).group(1))
    different = _int(re.search(r"([\d,]+) of those bytes differ", text).group(1))
    _expect(problems, 0 < different < size, f"F.8: impossible diff {different} of {size}")
    _expect(problems, "two messages are byte-identical" in text, "F.8: normalized-message identity assertion changed")
    _expect(problems, "6 dummy actions" in text and "7 private actions" in text,
            "F.8: own plus six dummies no longer gives seven actions")
    charges = re.search(r"execution gas ([\d,]+), of which public part ([\d,]+) cycles; storage gas ([\d,]+); base fee ([\d,]+)", text)
    _expect(problems, charges is not None, "F.8: charge line changed")
    state_table = next((t for t in _tables(text) if t["header"] == ["Public account", "Before", "After (A)", "Same in D"]), None)
    _expect(problems, state_table is not None and len(state_table["rows"]) == 2,
            "F.8: expected two public-account state-diff rows")
    if state_table:
        for account, before, after, same_d in state_table["rows"]:
            _expect(problems, same_d == "yes", f"F.8: {account}: D differs")
            should_change = account.startswith("claim(pool")
            _expect(problems, (before != after) == should_change,
                    f"F.8: {account}: before/after change disagrees with its role")
    _expect(problems, "Events: 0 in each" in text, "F.8: nonzero or non-identical event summary")
    _expect(problems, "the comparison fails" in text and "34,165 and 39,265 bytes" in text,
            "F.8: calling control no longer fails with two different sizes")
    _phrases(problems, _row(ctx, "F.8"), "F.8", "two private claims", "three hops back", "two commitments",
             "events, charges and every public account's change identical", "the calling variant fails")
    fail_if(problems, "F.8 record/status mismatch")
    return [f"both messages {size:,} B; proof {proof:,} B; {different:,} B differ, {size - different:,} B normalized-identical",
            f"13 absence searches (A=5, D=8 including three hops); repeated-proof counts {repeated}",
            "equal A/D charges, zero events, two public state rows and the failing CallAttest control checked",
            "LIMIT: this Markdown holds no raw A/D bytes or per-byte diff, so byte identity and absence remain recorded assertions"]


@check(id="lc-u5-failure-modes", title="U.5: 22 failures with IDL names and messages",
       family="record", mode="offline", tier="quick", rows=("RFP-021 U.5",), evidence=(39, 56, 69, 70, 151, 200, 228),
       what="22 observed failure rows, each numeric code/name/message and the two LEZ validity-window refusals",
       against="u5-failure-modes.md, idl/ferrata_attestation.idl.json and RFP-021 U.5's status text",
       inputs=(STATUS, f"{LC}/u5-failure-modes.md", "idl/ferrata_attestation.idl.json"))
def lc_u5(ctx):
    problems = []
    rows = next(t["rows"] for t in _tables(_text(ctx, f"{LC}/u5-failure-modes.md"))
                if t["header"][:2] == ["Failure mode", "Case"])
    errors = {int(e["code"]): e for e in json.loads(_text(ctx, "idl/ferrata_attestation.idl.json"))["errors"]}
    codes = []
    lez = 0
    matched_messages = 0
    for mode, case, observed, message in rows:
        m = re.match(r"refused, exit (\d+) ([A-Za-z0-9_]+) ", observed)
        if m:
            code, name = int(m.group(1)), m.group(2)
            codes.append(code)
            error = errors.get(code)
            _expect(problems, error is not None and error["name"] == name,
                    f"U.5: {case}: {code} {name} not in IDL")
            if error:
                _expect(problems, error["msg"] == message, f"U.5: {case}: displayed message differs from IDL")
                matched_messages += error["msg"] == message
        else:
            lez += 1
            _expect(problems, 'refused by LEZ: "Execution outside of the validity window"' in observed
                    and 'LEZ\'s own text: "Execution outside of the validity window"' in message,
                    f"U.5: {case}: unrecognized non-IDL refusal")
    handoff = [int(re.search(r"exit (\d+)", r[2]).group(1)) for r in rows if r[0] == "committee handoff not verified"]
    _expect(problems, len(rows) == 22, f"U.5: found {len(rows)} cases, not 22")
    _expect(problems, handoff == [27, 45, 23], f"U.5: handoff codes {handoff}, expected [27, 45, 23]")
    _expect(problems, len({r[0] for r in rows}) == 8, "U.5: not all eight listed failure modes present")
    _expect(problems, codes.count(53) == 2 and lez == 2, f"U.5: stale cases are {codes.count(53)} code-53 + {lez} LEZ")
    _expect(problems, any("512 keys" in r[1] for r in rows if "foreign committee" in r[2]),
            "U.5: foreign-committee case's 512-key boundary changed")
    _phrases(problems, _row(ctx, "U.5"), "U.5", "22 cases", "27, 45 and 23", '"Execution outside of the validity window"',
             "code 53's message")
    fail_if(problems, "U.5 record/IDL/status mismatch")
    return [f"22 cases over {len({r[0] for r in rows})} modes: {len(codes)} exit-code rows + {lez} LEZ refusals",
            f"{matched_messages} IDL code/name/message triplets match; handoff codes {handoff}",
            "stale cases: two exit 53, one exit 217 and two LEZ validity-window refusals"]


@check(id="lc-r3-rotations", title="R.3: synthetic, mainnet and Sepolia rotation case matrices",
       family="record", mode="offline", tier="quick", rows=("RFP-021 R.3",), evidence=(151, 208),
       what="four synthetic rotations, five mainnet records at two times, and one Sepolia before/after rotation",
       against="r3-rotations.md's expected/observed tables and RFP-021 R.3's status text",
       inputs=(STATUS, f"{LC}/r3-rotations.md"))
def lc_r3(ctx):
    problems = []
    text = _text(ctx, f"{LC}/r3-rotations.md")
    rows = _case_rows(text)
    codes = _check_expected_observed(problems, f"{LC}/r3-rotations.md", rows)
    rotations = [int(m.group(1)) for r in rows if (m := re.match(r"after rotation (\d+) ", r[0]))]
    _expect(problems, rotations == [1, 2, 3, 4], f"R.3: synthetic rotations {rotations}")
    for row in rows:
        if row[0].startswith("after rotation"):
            _expect(problems, "event data byte-identical" in row[3] and "record bytes unchanged" in row[3],
                    f"R.3: {row[0]} lacks byte-identity/record-unchanged observation")
    mainnet = [r for r in rows if r[0].startswith("the reference consumer reads")]
    after = [r for r in mainnet if "after the four rotations" in r[0]]
    later = [r for r in mainnet if "ten years later" in r[0]]
    slots_after = {re.search(r"slot ([\d,]+)", r[0]).group(1) for r in after}
    slots_later = {re.search(r"slot ([\d,]+)", r[0]).group(1) for r in later}
    _expect(problems, len(after) == len(later) == 5 and slots_after == slots_later,
            f"R.3: mainnet matrix is {len(after)} + {len(later)} with unequal slots")
    sepolia = [r for r in rows if "period 1409" in r[0] or "period 1410" in r[0]]
    _expect(problems, len(sepolia) == 2 and "totalSupply_" in sepolia[0][0]
            and "period 1409" in sepolia[0][0] and sepolia[1][0].startswith("the same after"),
            "R.3: Sepolia before/after rows changed")
    _expect(problems, len(rows) == 19 and not codes, f"R.3: expected 19 applied/stale cases, found {len(rows)}")
    _phrases(problems, _row(ctx, "R.3"), "R.3", "four committee rotations", "ten years later",
             "five real mainnet records", "a real Sepolia record attested before and after a real rotation")
    fail_if(problems, "R.3 record/status mismatch")
    return [f"synthetic: four rotations, statements/record bytes unchanged, plus the ten-year attestation and stale update",
            f"mainnet: five distinct records read after four rotations and the same five ten years later",
            "Sepolia: one record attested before and after the 1409→1410 rotation",
            "LIMIT: the separate three-rotation CI row (EVIDENCE 208) has no case table in this record"]


@check(id="lc-vs-fabricated-forgeries", title="VS.1/VS.2: fabricated-chain and six-forgery verdicts",
       family="record", mode="offline", tier="quick", rows=("RFP-021 VS.1", "RFP-021 VS.2"), evidence=(1, 151),
       what="all expected/observed security cases, ordered code groups and the 512-key supermajority boundary",
       against="vs1-fabricated-chain.md, vs2-forgeries.md and RFP-021 VS.1/VS.2's status text",
       inputs=(STATUS, f"{LC}/vs1-fabricated-chain.md", f"{LC}/vs2-forgeries.md"))
def lc_vs(ctx):
    problems = []
    rel1, rel2 = f"{LC}/vs1-fabricated-chain.md", f"{LC}/vs2-forgeries.md"
    rows1 = _case_rows(_text(ctx, rel1))
    rows2 = _case_rows(_text(ctx, rel2))
    codes1 = _check_expected_observed(problems, rel1, rows1)
    codes2 = _check_expected_observed(problems, rel2, rows2)
    _expect(problems, len(rows1) == 12 and codes1 == [40, 40, 213, 110, 211, 213, 110],
            f"VS.1: cases/codes changed ({len(rows1)}, {codes1})")
    _expect(problems, len(rows2) == 19, f"VS.2: found {len(rows2)} cases, not 19")
    _expect(problems, codes2 == [40, 27, 45, 23, 40, 51, 213, 110, 25, 25, 110, 140, 211, 110, 110, 130, 131],
            f"VS.2: ordered refusal codes changed: {codes2}")
    threshold_rows = [r for r in rows2 if "of 512 members sign" in r[0]]
    _expect(problems, len(threshold_rows) == 2 and threshold_rows[0][0].startswith("341 ")
            and threshold_rows[1][0].startswith("342 ") and threshold_rows[1][2] == "applied",
            "VS.2: 341-refused / 342-applied boundary changed")
    threshold = (2 * 512 + 2) // 3
    _expect(problems, threshold == 342, "VS.2: arithmetic threshold is not 342")
    _phrases(problems, _row(ctx, "VS.1"), "VS.1", "refused (40)", "(213, 110)", "across instances (211)")
    _phrases(problems, _row(ctx, "VS.2"), "VS.2", "All six forgeries", "(40, 27, 45, 23)",
             "341 of 512 signers (51; 342 applies)", "(213, 110)", "(25, 110)", "(140; another instance 211)",
             "(110, 130, 131)")
    fail_if(problems, "VS.1/VS.2 record/status mismatch")
    return [f"VS.1: 12 rows, five controls applied and canonical/consumer refusals {codes1}",
            f"VS.2: six sections, 19 rows and 17 refusals; independent ceil(2*512/3) = {threshold}",
            "committee, non-finalized proof, tampered roots, chain/instance and uncovered-predicate code groups checked"]


R11_DIR = "lez/results/v0.4.0-rc3/r11-one-block"


@check(id="lc-r11-one-block", title="R.11: four recorded private reads in block 47 and one dropped control",
       family="record", mode="offline", tier="quick", rows=("RFP-021 R.11",), evidence=(152, 209, 217),
       what="four reader ledger rows, their gas totals/positions, JSON cross-check, storage cap and dropped second control",
       against="the rc3 run.tsv/r11-one-block.json/driver log, the in-process four-row table and RFP-021 R.11",
       inputs=(STATUS, f"{LC}/r11-one-block.md", f"{R11_DIR}/run.tsv", f"{R11_DIR}/r11-one-block.json",
               f"{R11_DIR}/README.md", f"{R11_DIR}/logs/driver.log"))
def lc_r11(ctx):
    problems = []
    run = list(csv.DictReader(io.StringIO(_text(ctx, f"{R11_DIR}/run.tsv")), delimiter="\t"))
    readers = [r for r in run if r["step"].startswith("reader ")]
    _expect(problems, len(readers) == 4, f"R.11: {len(readers)} reader rows, not four")
    for position, row in enumerate(readers):
        _expect(problems, row["status"] == "applied", f"R.11: reader {position} status {row['status']}")
        _expect(problems, row["block"] == "47" and row["position"] == str(position),
                f"R.11: reader {position} at {row['block']}:{row['position']}, not 47:{position}")
        _expect(problems, row["block_txs"] == "4", f"R.11: reader {position} block user-tx count changed")
        _expect(problems, (row["exec_gas"], row["stor_gas"], row["fee"]) == ("422032", "224063", "5168760"),
                f"R.11: reader {position} charges changed")
    exec_gas = sum(int(r["exec_gas"]) for r in readers)
    stor_gas = sum(int(r["stor_gas"]) for r in readers)
    _expect(problems, exec_gas == 1_688_128 and stor_gas == 896_252,
            f"R.11: reader sums are exec {exec_gas}, storage {stor_gas}")
    _expect(problems, all(int(r["block_exec_gas"]) == exec_gas and int(r["block_stor_gas"]) == stor_gas for r in readers),
            "R.11: recorded block totals differ from the four rows' sums")
    _expect(problems, len({r["block"] for r in run if r["status"] == "dropped"}) == 1,
            "R.11: more than one dropped entry")
    dropped = [r for r in run if r["status"] == "dropped"]
    _expect(problems, len(dropped) == 1 and dropped[0]["step"].startswith("control:")
            and "second transaction of the same snapshot" in dropped[0]["step"]
            and "Commitment" in dropped[0]["note"] and "already seen" in dropped[0]["note"],
            "R.11: dropped second control or its Commitment-already-seen reason changed")
    summary = json.loads(_text(ctx, f"{R11_DIR}/r11-one-block.json"))
    txs = summary["block"]["transactions"]
    _expect(problems, summary["readers"] == len(readers) and len(txs) == len(readers),
            "R.11: JSON reader count differs from run.tsv")
    for ledger, js in zip(readers, txs):
        for csv_key, json_key in (("block", "block"), ("position", "position"), ("tx_hash", "hash"),
                                  ("exec_gas", "exec_gas"), ("stor_gas", "stor_gas"), ("fee", "fee")):
            _expect(problems, ledger[csv_key] == js[json_key], f"R.11: JSON/CSV {json_key} mismatch for reader")
    _expect(problems, summary["block"]["record_unchanged"] is True
            and summary["block"]["commitments_in_tree"] == 7 * len(readers),
            "R.11: record-unchanged flag or 28 commitments changed")
    _expect(problems, summary["control_second"]["status"] == "dropped"
            and summary["control_second"]["hash"] == dropped[0]["tx_hash"],
            "R.11: JSON control-second differs from dropped CSV row")
    driver = _text(ctx, f"{R11_DIR}/logs/driver.log")
    cap_match = re.search(r"MAX_GAS_STOR (\d+)", driver)
    _expect(problems, cap_match is not None, "R.11: driver log has no MAX_GAS_STOR")
    cap = int(cap_match.group(1)) if cap_match else 0
    _expect(problems, cap == 1_000_000 and len(readers) * 224_063 <= cap < (len(readers) + 1) * 224_063,
            f"R.11: four/max inference fails for cap {cap}")
    inprocess = next(t["rows"] for t in _tables(_text(ctx, f"{LC}/r11-one-block.md"))
                     if t["header"][:2] == ["Transaction", "Settled"])
    _expect(problems, len(inprocess) == 4 and all(r[1] == "yes" for r in inprocess),
            "R.11: in-process table is not four settled rows")
    _expect(problems, "the second is dropped: `Invalid input: Nullifier already seen`" in _text(ctx, f"{LC}/r11-one-block.md"),
            "R.11: in-process dropped second transaction changed")
    for a, b in zip(inprocess, readers):
        _expect(problems, [_int(x) for x in re.findall(r"\d[\d,]*", a[2])] ==
                [int(b["exec_gas"]), int(b["stor_gas"]), int(b["fee"])],
                f"R.11: in-process and rc3 charges differ for {a[0]}")
        _expect(problems, "7 nullifiers spent, 7 commitments in the tree" in a[3],
                f"R.11: in-process outputs changed for {a[0]}")
    readme = _text(ctx, f"{R11_DIR}/README.md")
    _phrases(problems, readme, "R.11 record README", "896,252 storage gas", "storage cap",
             "control's second transaction")
    _phrases(problems, _row(ctx, "R.11"), "R.11", "four private attestations", "896,252 of LEZ's 1,000,000 storage gas",
             "one account's second transaction", "dropped")
    fail_if(problems, "R.11 record/status mismatch")
    return [f"run.tsv: four applied readers in block 47 positions 0–3; sums {exec_gas:,} exec / {stor_gas:,} storage",
            f"JSON and in-process four-row charges agree; 28 = 4*7 commitments; cap {cap:,} permits four, not five",
            "the control's second same-snapshot transaction is the sole dropped row (Commitment already seen)"]


@check(id="lc-r7-stall", title="R.7: stall attestations and 106 late-update lab ledger rows",
       family="record", mode="offline", tier="quick", rows=("RFP-021 R.7",), evidence=(21, 69, 70, 192, 200),
       what="three attestations at three times, 8.2-day conversion, 106/334 late jobs, periods and newest-first code 23",
       against="r7-stall.md/txt and the cited Sepolia jobs.tsv/update file, plus RFP-021 R.7's status text",
       inputs=(STATUS, "lez/results/r7-stall.md", "lez/results/r7-stall.txt",
               "lez/lab/results/record/net/sepolia/jobs.tsv",
               "lez/lab/results/record/net/sepolia/updates/00137.period.gloas.ssz"))
def lc_r7(ctx):
    problems = []
    output = _text(ctx, "lez/results/r7-stall.txt")
    attestations = re.findall(r"^(before the stall|during the stall|after the resume): (.+): applied, ([\d,]+) execution gas, "
                              r"(\d+) statement\(s\)$", output, re.M)
    by_phase = {}
    for phase, case, gas, statements in attestations:
        by_phase.setdefault(phase, {})[case] = (_int(gas), int(statements))
    _expect(problems, set(by_phase) == {"before the stall", "during the stall", "after the resume"}
            and all(len(v) == 3 for v in by_phase.values()), "R.7: not three attestations in each of three phases")
    if set(by_phase) == {"before the stall", "during the stall", "after the resume"}:
        _expect(problems, by_phase["before the stall"] == by_phase["during the stall"] == by_phase["after the resume"],
                "R.7: attestation cases/gas/statement counts differ across stall phases")
    stall = re.search(r"stall: R at slot (\d+); (\d+) updates recorded after it \(period updates of \{([^}]+)\}\); "
                      r"resume at slot (\d+) \((\d+) s = ([\d.]+) days", output)
    _expect(problems, stall is not None, "R.7: stall summary line changed")
    if not stall:
        fail_if(problems, "R.7 record/status mismatch")
    record_slot, claimed_updates, period_text, resume_slot, seconds, days = stall.groups()
    jobs = list(csv.DictReader(io.StringIO(_text(ctx, "lez/lab/results/record/net/sepolia/jobs.tsv")), delimiter="\t"))
    outstanding = [r for r in jobs if r["status"] == "applied" and r["finalized_slot"] != "-"
                   and int(r["finalized_slot"]) > int(record_slot)]
    _expect(problems, len(outstanding) == int(claimed_updates) == 106,
            f"R.7: lab has {len(outstanding)} applicable rows, output says {claimed_updates}, status needs 106")
    transactions = sum(int(r["transactions"]) for r in outstanding)
    _expect(problems, transactions == 334 and f"resume: {len(outstanding)} jobs applied ({transactions} transactions)" in output,
            f"R.7: late job transaction total {transactions}, not 334/output")
    periods = sorted({int(r["period"]) for r in outstanding if r["kind"] == "period" and r["adopted"] == "true"})
    output_periods = sorted(int(x) for x in period_text.split(","))
    _expect(problems, periods == output_periods == list(range(1410, 1418)),
            f"R.7: adopted/output periods differ: {periods} / {output_periods}")
    _expect(problems, all(r["advanced"] == "true" for r in outstanding), "R.7: an outstanding lab row did not advance")
    numeric_seconds = int(seconds)
    _expect(problems, f"{numeric_seconds / 86400:.1f}" == days == "8.2",
            f"R.7: {numeric_seconds}s is {numeric_seconds / 86400:.4f} days, output says {days}")
    _expect(problems, len(periods) == 8, "R.7: not eight committee-rotation periods")
    newest = max((r for r in outstanding if r["kind"] == "period"), key=lambda r: int(r["period"]))
    update_name = f"{int(newest['seq']):05d}.period.{newest['layout']}.ssz"
    _expect(problems, ctx.path(f"lez/lab/results/record/net/sepolia/updates/{update_name}").is_file(),
            f"R.7: newest period update {update_name} absent")
    _expect(problems, f"out of order: {update_name} refused at Open with exit Some(23)" in output,
            "R.7: newest-first case is not that period update refused with 23")
    markdown = _text(ctx, "lez/results/r7-stall.md")
    _phrases(problems, markdown, "R.7 record", "706,020 s (8.2 days)", "106 jobs applied (334 transactions)",
             "1410 to 1417", "exit 23 `PeriodSkipped`")
    _phrases(problems, _row(ctx, "R.7"), "R.7", "8.2 days", "three attestations", "eight committee rotations",
             "106 outstanding real Sepolia updates", "periods 1410–1417", "the newest sent first refused, 23")
    fail_if(problems, "R.7 record/lab-ledger/status mismatch")
    return [f"three identical attestation tuples before/during/after; {numeric_seconds:,}s = {numeric_seconds / 86400:.4f} days -> 8.2",
            f"jobs.tsv after slot {int(record_slot):,}: {len(outstanding)} applied jobs, {transactions} transactions, periods {periods}",
            f"newest period row seq={newest['seq']} maps to {update_name}; output refuses it first with 23",
            "LIMIT: the late resume itself is summary-only; jobs.tsv supplies the cited live lab per-update ledger, not 106 late-run logs"]


@check(id="lc-stage-refusals", title="R.8/F.3: stage refusal tables and 32 direct Attest refusals",
       family="record", mode="offline", tier="quick",
       rows=("RFP-021 R.8", "RFP-021 F.3", "RFP-021 P.7"), evidence=(3, 6, 22, 71, 72, 112, 170),
       what="summary-handler gas/exit reconciliation, duplicate/replay codes and direct-Attest refusal count",
       against="stage1-refusals.md, stage4-attest-refusals.md and RFP-021 R.8/F.3/P.7 status text",
       inputs=(STATUS, "lez/results/stage1-refusals.md", "lez/results/stage4-attest-refusals.md"))
def lc_stage_refusals(ctx):
    problems = []
    rel1, rel4 = "lez/results/stage1-refusals.md", "lez/results/stage4-attest-refusals.md"
    one, ledger1 = _summary_tables(_text(ctx, rel1))
    four, ledger4 = _summary_tables(_text(ctx, rel4))
    _check_charges(problems, rel1, one, ledger1)
    _check_charges(problems, rel4, four, ledger4)
    by_name1 = {r[1]: r for r in one}
    expected = {"duplicate Open (same job)": 207, "Finish replayed after the job applied": 205,
                "Open an applied update again": 52}
    for name, code in expected.items():
        row = by_name1.get(name)
        _expect(problems, row is not None and f"refused: exit {code} " in row[7],
                f"R.8: {name} no longer refuses with {code}")
    direct = [r for r in four if not r[1].startswith("consumer:")]
    _expect(problems, len(four) == 41 and len(direct) == 32,
            f"F.3: {len(direct)} direct Attest refusals among {len(four)}, not 32/41")
    _expect(problems, all("refused: exit " in r[7] for r in four), "stage4: a refusal-table row is not refused")
    forged = by_name1.get("forged signature: Open (passes)")
    _expect(problems, forged is not None and forged[3] == "2,195,028" and forged[7] == "applied",
            "P.7: forged-signature Open row 1 is not applied at 2,195,028 gas")
    _phrases(problems, _row(ctx, "R.8"), "R.8", "Duplicate Open (207)", "replayed Finish (205)", "applied update (52)")
    _phrases(problems, _row(ctx, "F.3"), "F.3", "32 refusals")
    _phrases(problems, _row(ctx, "P.7"), "P.7", "2,195,028 execution gas")
    fail_if(problems, "stage refusal record/status mismatch")
    return [f"stage1: {len(one)} rows; stage4: {len(four)} rows, {len(direct)} direct Attest + 9 consumer refusals",
            "every non-reverted execution-gas cell equals its handler-cycle sum and its separate handler ledger",
            "R.8 single rows hold 207/205/52; P.7 row 1's applied forged Open holds 2,195,028 gas",
            "not checked here: R.2 (state unchanged after a refusal, the same claim then applied); these tables hold no "
            "before/after states or retry, so this check does not name R.2"]


@check(id="lc-stage4-attest-costs", title="P.1/Soft.2: eight attestation costs and the four-fact call",
       family="record", mode="offline", tier="quick", rows=("RFP-021 P.1", "RFP-021 Soft.2"), evidence=(3, 71),
       what="min/max execution gas, 5M percentages, applied verdicts, handler ledger and four-fact 454,255-cycle row",
       against="stage4-attest.md's transaction and per-handler tables and RFP-021 P.1/Soft.2 status text",
       inputs=(STATUS, "lez/results/stage4-attest.md"))
def lc_stage4_costs(ctx):
    problems = []
    rel = "lez/results/stage4-attest.md"
    rows, ledger = _summary_tables(_text(ctx, rel))
    _check_charges(problems, rel, rows, ledger)
    _expect(problems, len(rows) == 8 and all(r[7] == "applied" for r in rows), "stage4: eight applied cases expected")
    charges = [_int(r[3]) for r in rows]
    low, high = min(charges), max(charges)
    _expect(problems, (low, high) == (97_485, 1_619_960), f"P.1: range is {low}..{high}")
    largest = next(r for r in rows if _int(r[3]) == high)
    soft = next((r for r in rows if r[1].startswith("four facts in one call")), None)
    _expect(problems, soft is not None and soft[3] == "454,255" and soft[2] == "Attest 454,255",
            "Soft.2: four-fact call no longer costs 454,255 in one Attest handler")
    _expect(problems, soft is not None and "account, two slots, a log" in soft[1],
            "Soft.2: four facts are not account + two slots + log")
    _phrases(problems, _row(ctx, "P.1"), "P.1", f"{low:,} to {high:,} cycles")
    share = high * 100 / 5_000_000
    stated = re.search(r"(at most|under|up to|about)?\s*(\d+(?:\.\d+)?)% of LEZ's 5M target", _row(ctx, "P.1"))
    if stated is None:
        problems.append("P.1: no share of LEZ's 5M target stated")
    else:
        word, x = stated.group(1), float(stated.group(2))
        if word in ("at most", "up to"):
            ok = x >= share
        elif word == "under":
            ok = x > share
        else:  # the share itself, or "about": the exact one rounded to the digits given
            digits = len(stated.group(2).split(".")[1]) if "." in stated.group(2) else 0
            ok = round(share, digits) == x
        _expect(problems, ok, f"P.1 says '{stated.group(0)}'; the largest case is {high:,} of 5,000,000 = "
                              f"{share:.4f}%: that is not what the row's words say")
    _phrases(problems, _row(ctx, "Soft.2"), "Soft.2", "Four facts under one record in one call, 454,255 cycles")
    fail_if(problems, "stage4 attestation-cost/status mismatch")
    return [f"eight applied cases; min {low:,}, max {high:,}; max is {share:.4f}% of 5M (the record's table rounds "
            "to whole percents), and P.1's stated share holds for it",
            "all eight summary gas cells equal their independent Attest handler rows; rounded gas*100/5M percentages match",
            "four facts (account + two slots + log) in one Attest call: 454,255 cycles"]


@check(id="lc-perf-state", title="P.7: account sizes and exact per-record state growth",
       family="record", mode="offline", tier="quick", rows=("RFP-021 P.7",), evidence=(170,),
       what="32-byte-key/account-size arithmetic, 453B slope over all four K rows and gas invariance across K",
       against="perf-state.md sections 1–3 and RFP-021 P.7's status text; annual cadence is marked summary-external",
       inputs=(STATUS, "lez/results/perf-state.md"))
def lc_perf_state(ctx):
    problems = []
    text = _text(ctx, "lez/results/perf-state.md")
    accounts = next(t["rows"] for t in _tables(text) if t["header"] == ["account", "actor state (B)", "borsh `Account` (B)", "with its 32-byte key (B)"])
    sizes = {r[0]: tuple(_int(x) for x in r[1:]) for r in accounts}
    _expect(problems, len(sizes) == 3, f"P.7: found {len(sizes)} account rows")
    for name, (actor, account, keyed) in sizes.items():
        _expect(problems, actor + 56 == account and account + 32 == keyed,
                f"P.7: {name}: {actor}+56 != {account} or +32 != {keyed}")
    expected_names = {"instance(id)": 593, "header(id, slot) record": 453,
                      "job(id, job_id), while open (built from Open's post-state)": 1110}
    _expect(problems, {name: sizes.get(name, (0, 0, 0))[2] for name in expected_names} == expected_names,
            f"P.7: keyed account sizes changed: {sizes}")
    sec2 = text.split("## 2.", 1)[1].split("## 3.", 1)[0]
    growth = next(t["rows"] for t in _tables(sec2) if t["header"][0] == "records added")
    slopes = set()
    previous = None
    statement_values = []
    for row in growth:
        records, state = _int(row[0]), _int(row[1])
        if previous is not None:
            numerator = state - previous[1]
            denominator = records - previous[0]
            _expect(problems, numerator == 453 * denominator,
                    f"P.7: growth {previous[0]}..{records} = {numerator/denominator} B/record, not 453")
            slopes.add(numerator // denominator)
        previous = (records, state)
        # Five statement groups: exec gas, storage gas and transitions.
        statement_values.append([_int(x) for x in row[3:]])
    _expect(problems, len(growth) == 4 and slopes == {453}, f"P.7: growth slopes {slopes}")
    _expect(problems, statement_values and all(v == statement_values[0] for v in statement_values),
            "P.7: a statement's execution/storage gas or transitions changes with K")
    sec3 = text.split("## 3.", 1)[1].split("## 4a.", 1)[0]
    updates = next(t["rows"] for t in _tables(sec3) if t["header"][0] == "records added")
    by_step = {}
    baselines = {r[0]: _int(r[1]) for r in growth}
    for records, before, step, exec_gas, stor_gas in updates:
        by_step.setdefault(step, set()).add((_int(exec_gas), _int(stor_gas)))
        _expect(problems, records in baselines and baselines[records] - _int(before) == 505,
                f"P.7: section2/3 serialized-state difference is not the 505B instance actor at K={records}")
    _expect(problems, len(updates) == 32 and all(len(v) == 1 for v in by_step.values()),
            "P.7: an update step's exec/storage gas changes with K")
    _expect(problems, "Every statement was charged the same execution and storage gas at every K (asserted)." in text
            and "Every step was charged the same execution and storage gas at every K (asserted)." in text,
            "P.7: asserted invariance summaries changed")
    _phrases(problems, _row(ctx, "P.7"), "P.7", "100,000 records", "record 453 B", "instance 593 B", "job 1,110 B",
             "4.12 MB and 20.58 MB", "10.09 MB")
    fail_if(problems, "perf-state record/status mismatch")
    return [f"three account rows satisfy actor+56=Account and Account+32=keyed: keyed sizes {expected_names}",
            f"serialized-state growth is exactly 453B/record across 0→1,000→10,000→100,000",
            "five statement groups and all 32 update-step rows are gas-invariant across K",
            "LIMIT: the 9,087-record cadence and the 4.12/20.58/10.09 MB projections are not in perf-state.md: perf-projections derives them again"]


def _ssz_files(root: Path) -> tuple[list[str], list[str]]:
    """The .ssz files of the tree as they are on disk (outside build and scratch directories, following the tamper
    tests' symlinks), and, when `root` is a git checkout's top level, only the tracked ones among them plus the
    tracked ones missing from disk. Returns (files, missing)."""
    skip = {".git", ".scratch", "target", "node_modules", "lib", "out", "cache"}
    found, seen = set(), set()
    for dirpath, dirnames, filenames in os.walk(root, followlinks=True):
        real = os.path.realpath(dirpath)
        if real in seen:  # a symlink loop, or a directory reached twice
            dirnames[:] = []
            continue
        seen.add(real)
        dirnames[:] = [d for d in dirnames if d not in skip]
        rel = Path(dirpath).relative_to(root)
        found.update(str(rel / f) if rel.parts else f for f in filenames if f.endswith(".ssz"))
    top = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=root, capture_output=True, text=True)
    if top.returncode == 0 and Path(top.stdout.strip()).resolve() == root.resolve():
        out = subprocess.run(["git", "ls-files", "-z", "--", "*.ssz"], cwd=root, capture_output=True, text=True).stdout
        tracked = {x for x in out.split("\0") if x}
        return sorted(found & tracked), sorted(tracked - found)
    return sorted(found), []


ORACLE_TXT = "crates/ferrata-spectests/results/helios-step-oracle-2026-11-24.txt"
ORACLE_MD = "crates/ferrata-spectests/HELIOS-DIFFERENTIAL.md"


def _random_totals(problems: list, output: str, seed: str) -> tuple[int, int, int, int]:
    marker = f"seeded step-path random oracle (seed {seed},"
    start = output.find(marker)
    _expect(problems, start >= 0, f"R.6: random oracle seed {seed} absent")
    if start < 0:
        return (0, 0, 0, 0)
    line_end = output.find("\n", start)
    headline = output[start:line_end]
    totals = tuple(int(x) for x in re.findall(r"(\d+) (?:total|refused by all|accepted at fixed clock|accepted at dynamic clock)", headline))
    block = output[line_end + 1:output.find("test ", line_end + 1)]
    rows = re.findall(r"^(?:electra|gloas)-[\w-]+ \| (\d+) \| (\d+) \| (\d+) \| (\d+)$", block, re.M)
    _expect(problems, len(rows) == 4, f"R.6: random seed {seed} does not have four class rows")
    sums = tuple(sum(int(row[i]) for row in rows) for i in range(4)) if rows else (0, 0, 0, 0)
    if len(totals) == 4:
        _expect(problems, sums == totals, f"R.6: random class sums {sums} != headline {totals}")
        for row in rows:
            _expect(problems, int(row[0]) == sum(int(x) for x in row[1:]), f"R.6: random class outcomes do not sum: {row}")
    return sums


@check(id="lc-helios-step-oracle", title="R.6: Helios step-oracle aggregate ledgers and suite arithmetic",
       family="record", mode="offline", tier="quick", rows=("RFP-021 R.6",), evidence=(1, 140, 153, 257),
       what="321/332 real-data counts, 33/96 vectors, 467-mutation split, random class sums and 1,400/11,400 totals",
       against="the 24 Nov run output, HELIOS-DIFFERENTIAL.md, actual .ssz inventory and RFP-021 R.6 status text",
       inputs=(STATUS, ORACLE_TXT, ORACLE_MD))
def lc_helios(ctx):
    problems = []
    output = _text(ctx, ORACLE_TXT)
    doc = _text(ctx, ORACLE_MD)
    ssz, missing = _ssz_files(ctx.root)
    _expect(problems, not missing, f"R.6: tracked .ssz files missing from the tree: {', '.join(missing[:3])}")
    boot_files = [p for p in ssz if p.rsplit("/", 1)[-1].endswith("bootstrap.ssz")]
    _expect(problems, len(ssz) == 332 and len(boot_files) == 13,
            f"R.6: actual SSZ inventory is {len(ssz)} files/{len(boot_files)} bootstrap files")
    real = re.search(r"real-update step-path oracle: (\d+) bootstraps, (\d+) updates \((\d+) mainnet, (\d+) Sepolia; "
                     r"(\d+) period, (\d+) finality; (\d+) unique update payloads by sha256; (\d+) applied by step path, "
                     r"(\d+) valid optimistic-only NotNewer;.*?; (\d+)/(\d+) repo \.ssz files covered\); (\d+) disagreements", output)
    _expect(problems, real is not None, "R.6: real-update summary not parseable")
    if real:
        vals = tuple(int(x) for x in real.groups())
        boots, updates, mainnet, sepolia, period, finality, unique, applied, not_newer, covered, total_files, disagreements = vals
        _expect(problems, updates == mainnet + sepolia == period + finality == applied + not_newer == 321,
                f"R.6: real-update partitions do not total 321: {vals}")
        _expect(problems, boots == len(boot_files) == 13 and covered == total_files == len(ssz) == 332,
                f"R.6: recorded/repo bootstrap and SSZ coverage differ: {vals}")
        _expect(problems, boots + updates == 334 and disagreements == 0, "R.6: real suite total/disagreements changed")
        _expect(problems, unique == 271, "R.6: unique-payload count changed")
    vectors = re.search(r"consensus-specs step-path oracle: (\d+) cases, (\d+) updates \((\d+) applied, (\d+) policy-refused, "
                        r"(\d+) in-place fork upgrades\); (\d+) disagreements", output)
    _expect(problems, vectors is not None, "R.6: vector summary not parseable")
    if vectors:
        cases, steps, applied, refused, upgrades, dis = map(int, vectors.groups())
        _expect(problems, (cases, steps, applied, refused, upgrades, dis) == (33, 111, 61, 50, 9, 0),
                f"R.6: vector summary changed: {vectors.groups()}")
        _expect(problems, applied + refused == steps, "R.6: vector outcomes do not sum")
    mutations = re.search(r"every-field step-path oracle: (\d+) update field/edge/wire mutations \((\d+) accepted by both Helios and step path\), "
                          r"(\d+) bootstrap/store/witness/job mutations; (\d+) disagreements", output)
    _expect(problems, mutations is not None, "R.6: field-mutation summary not parseable")
    if mutations:
        update_muts, accepted_both, extra, dis = map(int, mutations.groups())
        _expect(problems, (update_muts, accepted_both, extra, dis) == (467, 16, 35, 0),
                f"R.6: field-mutation summary changed: {mutations.groups()}")
    _phrases(problems, doc, "Helios differential record",
             "16 accepted by both, 439 refused by both, 4 valid optimistic-only", "8 #872 pins",
             "467 update mutations + 17 bootstrap/store differential + 18 step-path-only",
             "96 two-way compared (61 applied, 35 policy-refused); 15 post-`force_update` checked one way (0 applied, 15 refused)",
             "8 step-path-only refusals", "re-signed updates", "| 45 |", "1,400 cases", "11,400 cases",
             "11,392 step-path cases + 246 lab-record updates + 2,000 mainnet mutations")
    _expect(problems, 16 + 439 + 4 + 8 == 467 and 467 + 17 + 18 == 502, "R.6: mutation arithmetic changed")
    _expect(problems, 61 + 35 == 96 and 96 + 15 == 111 and 35 + 15 == 50, "R.6: two-way/forced vector arithmetic changed")
    ci = _random_totals(problems, output, "0x26112401")
    full = _random_totals(problems, output, "0x261124ff")
    _expect(problems, ci == (400, 397, 0, 3), f"R.6: CI random totals {ci}")
    _expect(problems, full == (10000, 9975, 1, 24), f"R.6: full random totals {full}")
    _expect(problems, "re-signed semantic edge oracle: 45 cases" in output, "R.6: 45 re-signed edges absent")
    old_lab = [int(m) for m in re.findall(r"^(?:mainnet|sepolia): (\d+) updates \(\d+ period, \d+ finality\)", output, re.M)]
    _expect(problems, len(old_lab) == 2 and sum(old_lab) == 246, f"R.6: legacy lab updates {old_lab}, not 246")
    region_text = output.split("region | mutations | refused by all three", 1)[-1].split("mutations accepted by both", 1)[0]
    region_rows = re.findall(r"^(?:[\w#][\w #-]*) \| (\d+) \| (\d+) \| (\d+) \| (\d+)$", region_text, re.M)
    _expect(problems, len(region_rows) == 14, f"R.6: expected 14 old-mutation region rows, found {len(region_rows)}")
    if len(region_rows) == 14:
        totals = tuple(sum(int(row[i]) for row in region_rows) for i in range(4))
        _expect(problems, totals == (2000, 1987, 13, 0), f"R.6: old region sums {totals}")
    ci_cases = 334 + 111 + 502 + 45 + ci[0] + 8
    recorded_cases = ci_cases - 8 + full[0]
    _expect(problems, ci_cases == 1400 and recorded_cases == 11392 and ci_cases + full[0] == 11400,
            f"R.6: suite totals CI={ci_cases}, recorded={recorded_cases}, current={ci_cases + full[0]}")
    _expect(problems, "finished in 16.03s" in output, "R.6: recorded CI duration changed")
    _expect(problems, "=== DONE: all Helios differential suites passed with 0 unexpected disagreements in 223 s ===" in output,
            "R.6: final zero-disagreement run summary absent")
    pin_rev = re.search(r"^helios_pin:.*rev ([0-9a-f]{40})$", output, re.M)
    pin_sha = re.search(r"^helios_src_sha:([0-9a-f]{64})$", output, re.M)
    _expect(problems, pin_rev is not None and pin_rev.group(1) == "2af9b41887e069144ee56cd710f3ddca195473f8",
            "R.6: Helios source revision changed")
    _expect(problems, pin_sha is not None and pin_sha.group(1) == "17be8945b6af08d5b07da5f84d734c09c9375d86f982713beaf1a9cdfa410a20",
            "R.6: Helios source-tree SHA-256 changed")
    _phrases(problems, doc, "Helios differential record", "blst 0.3.17", "bls12_381 0.8.0",
             "2af9b41887e069144ee56cd710f3ddca195473f8", pin_sha.group(1) if pin_sha else "missing")
    row = _row(ctx, "R.6")
    _phrases(problems, row, "R.6", "all 321 recorded real mainnet and Sepolia updates", "all 332 `.ssz` files",
             "33 official sync vectors", "96 updates compared", "467 single-field/edge/SSZ-framing mutations",
             "16 accepted by both, 439 refused by both, 4 optimistic-only step-refused, 8 #872 pins",
             "17 bootstrap and store mutations", "18 step-path-only wire", "8 step-path-only adoption", "45 re-signed semantic edges",
             "400 CI / 10,000 full", "1,400 CI cases", "11,400 in the full script", "ran 11,392, before the 8 key-root cases",
             "2,000 mainnet mutations", "0 unexpected disagreements")
    fail_if(problems, "Helios oracle record/status mismatch")
    return [f"SSZ inventory: {len(ssz)} files = {len(boot_files)} bootstraps + {len(ssz)-len(boot_files)} update files; run: 321 updates (multi-update files included)",
            "vectors: 33/111, with 96 two-way = 61 applied + 35 refused and 15 one-way; 467 = 16+439+4+8 and 502 = 467+17+18",
            f"random class sums CI {ci}; full {full}; legacy regions 2,000=1,987+13, lab 108+138=246",
            f"suite arithmetic: recorded {recorded_cases:,}; current CI {ci_cases:,}; current full {ci_cases+full[0]:,}; zero disagreements in run footer",
            "LIMIT: individual real updates, 468+ mutation verdicts and Lodestar cases are aggregate summaries here, not replayed case logs"]


@check(id="lc-fixture-hashes", title="Recorded chain fixtures: SHA-256 and directory completeness",
       family="hash", mode="offline", tier="quick",
       what="every SHA256SUMS entry re-hashed and every fixture file accounted for (provenance exceptions reported)",
       against="all manifests and files under crates/ferrata-spectests/fixtures/",
       inputs=("crates/ferrata-spectests/fixtures",))
def lc_fixture_hashes(ctx):
    problems = []
    base = ctx.path("crates/ferrata-spectests/fixtures")
    manifests = sorted(base.rglob("SHA256SUMS"))
    _expect(problems, len(manifests) == 12, f"fixtures: found {len(manifests)} SHA256SUMS files, not 12")
    listed_total = 0
    provenance_exceptions = []
    for manifest in manifests:
        seen = set()
        for lineno, line in enumerate(manifest.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                continue
            m = re.fullmatch(r"([0-9a-f]{64}) ([ *])(.+)", line)
            _expect(problems, m is not None, f"{manifest.relative_to(ctx.root)}:{lineno}: malformed SHA256SUMS line")
            if not m:
                continue
            digest, name = m.group(1), m.group(3)
            path = Path(name)
            _expect(problems, not path.is_absolute() and ".." not in path.parts,
                    f"{manifest}: unsafe path {name!r}")
            if path.is_absolute() or ".." in path.parts:
                continue
            _expect(problems, name not in seen, f"{manifest}: duplicate path {name!r}")
            if name in seen:
                continue
            seen.add(name)
            payload = manifest.parent / path
            if not payload.is_file():
                problems.append(f"{manifest}: listed file {name} absent")
                continue
            actual = hashlib.sha256(payload.read_bytes()).hexdigest()
            _expect(problems, actual == digest, f"{payload.relative_to(ctx.root)}: SHA-256 {actual} != {digest}")
            listed_total += 1
        actual = {p for p in manifest.parent.rglob("*") if p.is_file() and p.name != "SHA256SUMS"}
        listed_paths = {manifest.parent / name for name in seen}
        for unlisted in sorted(actual - listed_paths):
            if unlisted.parent == manifest.parent and unlisted.name == "SOURCE.md":
                provenance_exceptions.append(str(unlisted.relative_to(ctx.root)))
            else:
                problems.append(f"{unlisted.relative_to(ctx.root)}: file is absent from SHA256SUMS")
        for missing in sorted(listed_paths - actual):
            problems.append(f"{missing.relative_to(ctx.root)}: listed path is not a file")
    fail_if(problems, "fixture hash/completeness mismatch")
    return [f"{len(manifests)} manifests; {listed_total} listed files re-hashed with SHA-256; no unlisted chain-data files",
            f"completeness exception: {len(provenance_exceptions)} older manifests omit provenance-only SOURCE.md: "
            + ", ".join(p.rsplit("/", 2)[-2] for p in provenance_exceptions),
            "SOURCE.md is not chain data; newer manifests list it too. Any other extra/missing file fails"]
