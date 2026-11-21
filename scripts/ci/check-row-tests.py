#!/usr/bin/env python3
"""Every hard requirement of RFP-021 and RFP-022 has at least one named test (RFP-022 Reliability #11).

Reads the hard row IDs from docs/traceability.md (the matrices of both RFPs, soft rows excluded) and
the row-to-test map scripts/ci/row-tests.toml, and checks that:

  - the matrices hold 59 hard rows for RFP-021 and 68 for RFP-022, as traceability.md's header states;
  - every hard row has at least one entry in the map;
  - every entry names a path tracked by git, unless the entry lives on another branch (`branch` set),
    in which case it is printed as pending and not required to exist here. A path that exists on disk
    but is not yet tracked (this check's own files, before their first commit) is accepted with a
    warning: on a fresh checkout - what CI runs - only tracked files exist, so there the two are the
    same test;
  - every entry's `ci` names a job defined by a workflow under .github/workflows/, or is "box" /
    "local" for what runs only on the build box or by hand (a "box" entry must carry its `run`
    command).

Prints a summary: rows per RFP, rows with a CI-run test, rows with box or local tests only (listed),
and the pending entries (listed). Exits 0 when everything checks out, 1 otherwise.

Standard library only. Usage: python3 -I scripts/ci/check-row-tests.py
"""

import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
TRACEABILITY = ROOT / "docs" / "traceability.md"
ROW_TESTS = ROOT / "scripts" / "ci" / "row-tests.toml"
WORKFLOWS = ROOT / ".github" / "workflows"

# The hard rows of each RFP, as traceability.md's header counts them (soft rows excluded).
EXPECTED = {"RFP-021": 59, "RFP-022": 68}
PREFIXES = {
    "RFP-021": {"F", "U", "R", "P", "S", "VS"},
    "RFP-022": {"F", "U", "R", "P", "S", "Sec", "Priv"},
}
ROW_ID = re.compile(r"^\|\s*(VS|Sec|Priv|F|U|R|P|S)\.(\d+)\s*\|")
HEADING = re.compile(r"^(\#{2,3})\s*(.*?)\s*$")
JOB = re.compile(r"^  ([A-Za-z0-9][A-Za-z0-9_-]*):\s*$")


def hard_rows():
    """The hard row IDs of both RFPs, from traceability.md's matrices (soft rows excluded)."""
    rows = {rfp: [] for rfp in EXPECTED}
    rfp = None
    section = None
    in_soft = False
    for line in TRACEABILITY.read_text(encoding="utf-8").splitlines():
        heading = HEADING.match(line)
        if heading:
            level, text = len(heading.group(1)), heading.group(2)
            if level == 2:
                rfp = next((r for r in EXPECTED if text.startswith(r + ":")), None)
                section, in_soft = None, False
            elif level == 3:
                section, in_soft = text, text == "Soft requirements"
            continue
        if rfp is None or section is None or in_soft:
            continue
        match = ROW_ID.match(line)
        if match and match.group(1) in PREFIXES[rfp]:
            rows[rfp].append(f"{rfp} {match.group(1)}.{match.group(2)}")
    return rows


def workflow_jobs():
    """The job names every workflow under .github/workflows/ defines."""
    jobs = set()
    for path in sorted(WORKFLOWS.glob("*.yml")):
        in_jobs = False
        for line in path.read_text(encoding="utf-8").splitlines():
            if re.match(r"^\S", line):
                in_jobs = line == "jobs:"
                continue
            if in_jobs:
                match = JOB.match(line)
                if match:
                    jobs.add(match.group(1))
    return jobs


def tracked_paths():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z"], capture_output=True, check=True
    ).stdout.decode("utf-8", "surrogateescape")
    return {name for name in out.split("\0") if name}


def main():
    rows = hard_rows()
    problems = []

    known = {rfp: set(ids) for rfp, ids in rows.items()}
    for rfp, ids in rows.items():
        if len(ids) != EXPECTED[rfp]:
            problems.append(
                f"traceability.md holds {len(ids)} hard rows for {rfp}, expected {EXPECTED[rfp]}"
            )
        if len(ids) != len(set(ids)):
            dupes = sorted({i for i in ids if ids.count(i) > 1})
            problems.append(f"{rfp}: duplicate hard row IDs parsed from traceability.md: {dupes}")

    jobs = workflow_jobs()
    allowed_ci = jobs | {"box", "local"}
    tracked = tracked_paths()
    untracked_here = []

    with open(ROW_TESTS, "rb") as handle:
        entries = tomllib.load(handle).get("row", [])

    blocks = {}
    for block in entries:
        rid = block.get("id")
        if not isinstance(rid, str) or " " not in rid:
            problems.append(f"row-tests.toml: an entry has no readable id: {block!r}")
            continue
        rfp = rid.split(" ", 1)[0]
        if rfp not in EXPECTED:
            problems.append(f"row-tests.toml: {rid}: no such RFP")
            continue
        if rid not in known[rfp]:
            problems.append(f"row-tests.toml: {rid}: not a hard row of {rfp} (typo, or a soft row)")
            continue
        branch = block.get("branch")
        if branch is not None and not isinstance(branch, str):
            problems.append(f"row-tests.toml: {rid}: branch must be a string")
        tests = block.get("tests")
        if not isinstance(tests, list) or not tests:
            problems.append(f"row-tests.toml: {rid}: no tests")
            continue
        for test in tests:
            path, ci = test.get("path"), test.get("ci")
            if not isinstance(path, str) or not path:
                problems.append(f"row-tests.toml: {rid}: a test has no path")
                continue
            if branch is None and path not in tracked:
                if (ROOT / path).is_file():
                    untracked_here.append(f"{rid}: {path}")
                else:
                    problems.append(f"row-tests.toml: {rid}: {path} is not tracked by git")
            if ci not in allowed_ci:
                defined = ", ".join(sorted(jobs)) or "none"
                problems.append(
                    f"row-tests.toml: {rid}: ci {ci!r} of {path} is no workflow job ({defined}) "
                    "and not box/local"
                )
            if ci == "box" and not isinstance(test.get("run"), str):
                problems.append(f"row-tests.toml: {rid}: {path} runs on the box but names no run command")
        blocks.setdefault(rid, []).append(block)

    missing = sorted(
        rid for rfp, ids in known.items() for rid in sorted(set(ids)) if rid not in blocks
    )
    for rid in missing:
        problems.append(f"row-tests.toml: hard row {rid} has no entry")

    # The summary.
    print("Row-to-test map (docs/traceability.md against scripts/ci/row-tests.toml)")
    for rfp in sorted(EXPECTED):
        ids = sorted(set(rows[rfp]))
        in_ci = [
            rid
            for rid in ids
            if any(
                test.get("ci") in jobs
                for block in blocks.get(rid, [])
                if not block.get("branch")
                for test in block["tests"]
            )
        ]
        box_local_only = [
            rid
            for rid in ids
            if rid in blocks
            and rid not in in_ci
            and all(
                test.get("ci") in {"box", "local"}
                for block in blocks[rid]
                if not block.get("branch")
                for test in block["tests"]
            )
        ]
        pending = [
            f"{rid} -> {test['path']} ({block['branch']})"
            for rid in ids
            for block in blocks.get(rid, [])
            if block.get("branch")
            for test in block["tests"]
        ]
        print()
        print(f"{rfp}: {len(ids)} hard rows, {len(in_ci)} with a CI-run test")
        print(f"  rows with box or local tests only ({len(box_local_only)}):")
        for rid in box_local_only:
            print(f"    {rid}")
        print(f"  pending entries, on another branch ({len(pending)}):")
        for line in pending:
            print(f"    {line}")
    if untracked_here:
        print()
        print(f"  paths not tracked by git yet, present on disk ({len(untracked_here)}):")
        for line in untracked_here:
            print(f"    {line}")
        print("    (accepted here; on a fresh checkout only tracked files exist)")

    sys.stdout.flush()
    if problems:
        print()
        for problem in problems:
            print(f"ERROR: {problem}", file=sys.stderr)
        print(f"check-row-tests.py: {len(problems)} problem(s)", file=sys.stderr)
        return 1
    print()
    print("check-row-tests.py: every hard row of both RFPs has at least one named test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
