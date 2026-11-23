#!/usr/bin/env python3
"""Every hard requirement of RFP-021 and RFP-022 mapped to its tests, and which of them CI really runs
(RFP-022 Reliability #11: "Every hard requirement has at least one corresponding test").

Reads the hard row IDs from docs/traceability.md (the matrices of both RFPs, soft rows excluded) and the
row-to-test map scripts/ci/row-tests.toml, where every entry has a `kind`:

  - "test": something that runs and fails when the requirement breaks: a Rust test file, a Foundry test
    file, a test script, or a workflow whose job is the test. Its `ci` is the CI job that runs it, or
    "box" / "local" for what runs only on the build box or by hand, then with its `run` command.
  - "record": the record of a run (a results file, a log, recorded data).
  - "document": a document (a README, a guide, the specification).

It checks that the matrices hold 59 and 68 hard rows; that every hard row has an entry; that every path
is tracked by git (an entry with `branch` lives on another branch and is reported as pending); that every
kind is one of the three, every test path is something that can run, every box or local test has its
command; and, for every test whose `ci` names a job, that the job really runs it: the job's `run:` steps
and the shell scripts they call are read (scripts/ci/ci_model.py), and the file must be selected by one of
the commands - a `cargo test` whose directory, packages, targets, features, name filters and environment
select it (a test file that skips itself or fails unless an environment variable is set needs the command
to set it), a `forge test` whose project and filters select it, an invocation of the script itself, or, for
a workflow file, the job being defined in it. A command that cannot fail the job does not count: its step
or job under an `if:` that is not always true or with `continue-on-error`, its failure masked by `|| X`,
run with `&` or as a condition, or after the script has exited (ci_model.py). A job that runs only when
started by hand counts as manual, not as CI. The model is static and does not catch every way of not running a
test (a job needing a skipped job, triggers that match nothing, `--ignored` or `--list`, `exec`, an `exit` in a
function or compound command, an `exit 0` trap, a masked subshell, other `cfg` forms): scripts/ci/README.md,
"What it does not catch", lists them.

The summary gives, per RFP, the rows with a test CI runs, and the others by what they have instead (a test
in a manual workflow; tests on the box or by hand; records and documents only); `--list` prints every
row's status. Exits 1 on any problem above. With `--require-ci RFP` (RFP-021, RFP-022 or all) it also
exits 1 while any hard row of that RFP lacks a test CI runs: the bar milestone B4 sets for RFP-022 R.11
(docs/traceability.md, "one named test per hard requirement, passing in CI").

Standard library only. Usage: python3 -I scripts/ci/check-row-tests.py [--list] [--require-ci RFP] [--map FILE]
[--traceability FILE] (--map and --traceability read other copies of the map and of the matrices:
scripts/ci/test_ci_model.py uses them).
"""

import pathlib
import re
import sys
import tomllib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import ci_model  # noqa: E402

ROOT = ci_model.ROOT
TRACEABILITY = ROOT / "docs" / "traceability.md"
ROW_TESTS = ROOT / "scripts" / "ci" / "row-tests.toml"

EXPECTED = {"RFP-021": 59, "RFP-022": 68}
PREFIXES = {
    "RFP-021": {"F", "U", "R", "P", "S", "VS"},
    "RFP-022": {"F", "U", "R", "P", "S", "Sec", "Priv"},
}
ROW_ID = re.compile(r"^\|\s*(VS|Sec|Priv|F|U|R|P|S)\.(\d+)\s*\|")
HEADING = re.compile(r"^(\#{2,3})\s*(.*?)\s*$")
KINDS = ("test", "record", "document")
RUNNABLE = re.compile(r"\.(rs|sol|sh|py|mjs|js)$|^\.github/workflows/[^/]+\.yml$")
MUST_BE_TEST = re.compile(r"\.t\.sol$|(^|/)tests/[^/]+\.rs$")


def hard_rows(traceability=TRACEABILITY):
    """The hard row IDs of both RFPs, from traceability.md's matrices (soft rows excluded)."""
    rows = {rfp: [] for rfp in EXPECTED}
    rfp = section = None
    in_soft = False
    for line in traceability.read_text(encoding="utf-8").splitlines():
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


class Runs:
    """Whether a job runs a path, with the reasons the commands of the job gave when it does not."""

    def __init__(self, tracked):
        self.jobs = ci_model.jobs()
        self.tracked = tracked
        self.traces = {}

    def trace(self, job):
        if job not in self.traces:
            self.traces[job] = ci_model.trace(self.jobs[job], self.tracked)
        return self.traces[job]

    def check(self, job, path):
        if path.startswith(".github/workflows/"):
            ok = self.jobs[job].workflow.relative_to(ROOT).as_posix() == path
            return ok, "" if ok else f"job {job} is not defined in {path}"
        reasons = []
        for cmd in self.trace(job):
            if path.endswith(".rs"):
                ok, why = ci_model.cargo_runs(cmd, path)
                if why not in ("not cargo", "not cargo test"):
                    reasons.append(f"`{' '.join(cmd.argv)}` in {cmd.cwd}: {why}")
            elif path.endswith(".sol"):
                ok, why = ci_model.forge_runs(cmd, path)
                if why != "not forge test":
                    reasons.append(f"`{' '.join(cmd.argv)}` in {cmd.cwd}: {why}")
            else:
                ok = ci_model.invokes(cmd, path, self.tracked)
            if ok:
                return True, ""
        if not reasons:
            reasons.append("no command of the job runs it" if not path.endswith((".rs", ".sol")) else
                           f"no {'cargo' if path.endswith('.rs') else 'forge'} test command in the job")
        return False, "; ".join(dict.fromkeys(reasons))


def main(argv):
    list_all = "--list" in argv
    row_tests = ROW_TESTS
    if "--map" in argv:
        i = argv.index("--map")
        row_tests = pathlib.Path(argv[i + 1]) if i + 1 < len(argv) else None
        if row_tests is None:
            print("usage: check-row-tests.py [--list] [--require-ci RFP] [--map FILE] [--traceability FILE]", file=sys.stderr)
            return 2
    traceability = TRACEABILITY
    if "--traceability" in argv:
        i = argv.index("--traceability")
        if i + 1 >= len(argv):
            print("usage: check-row-tests.py [--list] [--require-ci RFP] [--map FILE] [--traceability FILE]", file=sys.stderr)
            return 2
        traceability = pathlib.Path(argv[i + 1])
    require = None
    if "--require-ci" in argv:
        i = argv.index("--require-ci")
        require = argv[i + 1] if i + 1 < len(argv) else ""
        if require not in ("RFP-021", "RFP-022", "all"):
            print("usage: check-row-tests.py [--list] [--require-ci RFP-021|RFP-022|all]", file=sys.stderr)
            return 2

    rows = hard_rows(traceability)
    problems = []
    known = {rfp: set(ids) for rfp, ids in rows.items()}
    for rfp, ids in rows.items():
        if len(ids) != EXPECTED[rfp]:
            problems.append(f"traceability.md holds {len(ids)} hard rows for {rfp}, expected {EXPECTED[rfp]}")
        if len(ids) != len(set(ids)):
            problems.append(f"{rfp}: duplicate hard row IDs in traceability.md: {sorted({i for i in ids if ids.count(i) > 1})}")

    tracked = ci_model.tracked_paths()
    runs = Runs(tracked)
    with open(row_tests, "rb") as handle:
        entries = tomllib.load(handle).get("row", [])

    # Per row: the statuses of its entries.
    status = {}
    pending = {}
    for block in entries:
        rid = block.get("id")
        if not isinstance(rid, str) or " " not in rid:
            problems.append(f"row-tests.toml: an entry has no readable id: {block!r}")
            continue
        rfp = rid.split(" ", 1)[0]
        if rfp not in EXPECTED or rid not in known[rfp]:
            problems.append(f"row-tests.toml: {rid}: not a hard row (typo, or a soft row)")
            continue
        branch = block.get("branch")
        tests = block.get("tests")
        if not isinstance(tests, list) or not tests:
            problems.append(f"row-tests.toml: {rid}: no tests")
            continue
        status.setdefault(rid, set())
        for test in tests:
            path, kind, ci = test.get("path"), test.get("kind"), test.get("ci")
            where = f"row-tests.toml: {rid}: {path}"
            if not isinstance(path, str) or not path:
                problems.append(f"row-tests.toml: {rid}: an entry has no path")
                continue
            if kind not in KINDS:
                problems.append(f"{where}: kind {kind!r} is none of {', '.join(KINDS)}")
                continue
            if branch:
                pending.setdefault(rid, []).append(f"{path} ({kind}{', ci ' + ci if ci else ''}; on {branch})")
                status[rid].add("pending")
                continue
            if path not in tracked:
                problems.append(f"{where}: not tracked by git")
                continue
            if kind != "test":
                if ci is not None:
                    problems.append(f"{where}: a {kind} has no `ci` (only a test runs somewhere)")
                if MUST_BE_TEST.search(path):
                    problems.append(f"{where}: a test file listed as a {kind}; say kind = \"test\" and where it runs")
                status[rid].add(kind)
                continue
            if not RUNNABLE.search(path):
                problems.append(f"{where}: kind test, but not a test file, test script or workflow")
                continue
            if ci in ("box", "local"):
                if not isinstance(test.get("run"), str) or not test["run"].strip():
                    problems.append(f"{where}: a {ci} test names no `run` command")
                status[rid].add(ci)
                continue
            if ci not in runs.jobs:
                problems.append(f"{where}: ci {ci!r} is no job of .github/workflows/ ({', '.join(sorted(runs.jobs))}) and not box/local")
                continue
            ok, why = runs.check(ci, path)
            if not ok:
                problems.append(f"{where}: job {ci} does not run it: {why}")
                continue
            status[rid].add("ci" if runs.jobs[ci].on_change else "manual")

    missing = sorted(rid for rfp in known for rid in known[rfp] if rid not in status)
    for rid in missing:
        problems.append(f"row-tests.toml: hard row {rid} has no entry")

    def category(rid):
        s = status.get(rid, set())
        if "ci" in s:
            return "ci"
        if "manual" in s:
            return "manual"
        if s & {"box", "local"}:
            return "box/local"
        if s & {"record", "document"}:
            return "records/documents"
        if "pending" in s:
            return "pending"
        return "none"

    labels = {
        "manual": "a test in a workflow started by hand only",
        "box/local": "tests run on the build box or by hand only",
        "records/documents": "records and documents only, no test",
        "pending": "entries on another branch only",
        "none": "no entry",
    }
    print("Row-to-test map (docs/traceability.md against scripts/ci/row-tests.toml)")
    lacking = {}
    for rfp in sorted(EXPECTED):
        ids = sorted(set(rows[rfp]), key=lambda r: (r.split()[1].split(".")[0], int(r.split(".")[-1])))
        cats = {rid: category(rid) for rid in ids}
        in_ci = [rid for rid in ids if cats[rid] == "ci"]
        lacking[rfp] = [rid for rid in ids if cats[rid] != "ci"]
        print()
        print(f"{rfp}: {len(ids)} hard rows; {len(in_ci)} with a test CI runs; {len(lacking[rfp])} without one")
        for cat in ("manual", "box/local", "records/documents", "pending", "none"):
            these = [rid.split(" ", 1)[1] for rid in ids if cats[rid] == cat]
            if these:
                print(f"  {labels[cat]} ({len(these)}): {', '.join(these)}")
        pend = [f"{rid.split(' ', 1)[1]} -> {p}" for rid in ids for p in pending.get(rid, [])]
        if pend:
            print(f"  pending entries, on another branch ({len(pend)}):")
            for line in pend:
                print(f"    {line}")
        if list_all:
            for rid in ids:
                print(f"    {rid}: {cats[rid]}")

    sys.stdout.flush()
    if problems:
        print()
        for problem in problems:
            print(f"ERROR: {problem}", file=sys.stderr)
        print(f"check-row-tests.py: {len(problems)} problem(s)", file=sys.stderr)
        return 1
    print()
    print("check-row-tests.py: the map is consistent: every hard row has an entry, every path is tracked, "
          "every test a job is said to run is run by it")
    if require:
        short = [rid for rfp in (EXPECTED if require == "all" else [require]) for rid in lacking[rfp]]
        if short:
            print(f"check-row-tests.py --require-ci {require}: {len(short)} hard row(s) without a test CI runs "
                  "(listed above)", file=sys.stderr)
            return 1
        print(f"check-row-tests.py --require-ci {require}: every hard row has a test CI runs")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
