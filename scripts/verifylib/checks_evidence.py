"""The figures EVIDENCE rows 271-274 and the rows citing them state, held to the values the checks behind them
recompute (offline).

`run-record` holds lez/updater/results/public-network-run/summary.json to the transaction list and chain.json;
`seals-v3.0` and `seals-v3.1` verify every recorded seal; `rc3-roundtrip-record` runs CI's check stage on the rc3
round trip's steps; the registry is verify.py's own. This check closes the last link: what docs/EVIDENCE.md rows
271-274, RFP-021 S.1, RFP-022 S.1, R.10 and R.11 and docs/VERIFY.md say about them, figure by figure.
"""
from __future__ import annotations

import csv
import io
import json
import re
import subprocess
import sys

from .core import REGISTRY, check, fail_if

EVIDENCE_DOC = "docs/EVIDENCE.md"
STATUS_DOC = "docs/requirements-status.md"
VERIFY_DOC = "docs/VERIFY.md"
RUN_SUMMARY = "lez/updater/results/public-network-run/summary.json"
RUN_INSTANCE = "3pQmc7V8CuUZNmYeqUYTrnc1NgExoQvHw5VfKzpkaVFH"  # the Sepolia instance the network's updater keeps
RT_STEPS = "e2e/runs/rc3-roundtrip-1/run/steps.tsv"


def _evidence_row(text: str, n: int) -> str | None:
    return next((line for line in text.splitlines() if line.startswith(f"| {n} | ")), None)


def _status_row(text: str, rfp: str, rid: str) -> str | None:
    current = None
    for line in text.splitlines():
        if line.startswith("## RFP-02"):
            current = line[3:10]
        elif current == rfp and line.startswith(f"| {rid} | "):
            return line
    return None


def _duration(seconds: float) -> str:
    """As the documents write a duration under an hour: "6 min 20 s", "20 min 40 s"."""
    s = int(round(seconds))
    return f"{s // 60} min {s % 60} s" if s >= 60 else f"{s} s"


def _states(problems: list, where: str, line: str | None, fragments) -> None:
    """Each fragment in the line as a whole figure: no digit, comma or point just before it, nor a digit, a thousands
    group or a decimal part just after it when it ends with a digit (so "672 updates" is not found in "1672 updates",
    "0 over" not in "10 over", "25" not in "25,776", "25,776" not in "25,776.5"; a sentence's full stop is fine)."""
    if line is None:
        problems.append(f"{where}: not found")
        return
    for fragment in fragments:
        pattern = r"(?<![\d,.])" + re.escape(fragment) + (r"(?!\d|[.,]\d)" if fragment[-1:].isdigit() else "")
        if not re.search(pattern, line):
            problems.append(f"{where}: does not state {fragment!r}")


@check(id="evidence-rows-integ", family="docs", mode="offline", evidence=(271, 272, 273, 274),
       title="EVIDENCE rows 271-274 and the rows citing them: every figure as the checks behind them recompute it",
       what="the offline figures of EVIDENCE rows 271 (the unattended run: those summary.json holds; the chain-read "
            "ones, 670 of 670 headers canonical and the counter on chain, are run-record-beacon's and "
            "run-record-counters'), 272 (verify.py's registry and coverage), 273 (the seals verified locally and those "
            "not verifiable here; the online package count is basecamp-packages-online's) and 274 (CI's round trip on "
            "rc3), of RFP-021 S.1, RFP-022 S.1, R.10 and R.11 (with check-row-tests.py's CI counts), and "
            "docs/VERIFY.md's counts of named tests",
       against="the run record's summary.json (which run-record holds to the chain data), the recorded seals the seal "
               "checks verify, the rc3 round trip's steps.tsv (which rc3-roundtrip-record checks with CI's check stage), "
               "and the registry and --coverage of this verify.py",
       inputs=(EVIDENCE_DOC, STATUS_DOC, VERIFY_DOC, RUN_SUMMARY, RT_STEPS, "scripts/verify-mainnet-risc0.py",
               "e2e/runs", "vault/test", "bridge/artifacts", "lez/results", "checkpoint/artifacts", "bench/report",
               "checkpoint/realblocks", "checkpoint/attestor/runs", "scripts/ci/row-tests.toml",
               "scripts/ci/check-row-tests.py"))
def evidence_rows_integ(ctx):
    from . import catalog, checks_seals  # imported here: catalog imports this module
    problems, lines = [], []
    evidence = ctx.path(EVIDENCE_DOC).read_text(encoding="utf-8")
    status = ctx.path(STATUS_DOC).read_text(encoding="utf-8")

    # Row 271 and the S.1 rows: the run record's summary
    summary = json.loads(ctx.path(RUN_SUMMARY).read_text(encoding="utf-8"))
    run = summary["instances"][RUN_INSTANCE]
    updates = run["updates_applied"]
    intervals = run["intervals"]
    adoptions = run["committee_adoptions"]
    periods = [a["period"] for a in adoptions]
    days = sorted(run["updates_per_utc_day"].items())
    full_days = {n for _, n in days[1:-1]}  # the first and the last day are partial
    pauses = [_duration(g["seconds"]) for g in summary["chain"]["block_gaps_over_60s"]]
    as_of = summary["as_of"]["block"]
    longest = _duration(intervals["longest"]["seconds"])
    _states(problems, "EVIDENCE row 271", _evidence_row(evidence, 271), (
        f"as of LEZ block {as_of:,}", f"{updates} updates applied, every one by the updater's account",
        f"median interval {_duration(intervals['median_s'])}", f"longest {longest}",
        f"{len(intervals['over_1h'])} over 1 h",
        f"{len(adoptions)} committee adoptions (periods {periods[0]}–{periods[-1]})",
        f"{len(run['period_crossings'])} period crossings", f"{len(run['refused'])} refusals",
        f"participation {run['participants']['min']}–{run['participants']['max']} of 512",
        f"three pauses of block production over 60 s ({', '.join(pauses)})" if len(pauses) == 3 else "pauses: not 3",
        *((f"{full_days.pop()} updates on each full UTC day",) if len(full_days) == 1 else ("full days differ",))))
    first, last = run["first_update"], run["last_update"]
    lag = run["lag_behind_finality_s"]
    open_now = [j for j in run["jobs_opened_not_finished"] if j.get("open_at_as_of")]
    _states(problems, "EVIDENCE row 271", _evidence_row(evidence, 271), (
        f"from block {first['block']:,} (", f"to block {last['block']:,} (",
        f"each update's LEZ block {_duration(lag['min'])} to", f"(median {_duration(lag['median'])})",
        f"{len(open_now)} open at the as-of block",
        f"the instance's counter {updates + 1} = 1 + {updates}"))
    none_over = "none over 1 h" if not intervals["over_1h"] else "over 1 h: some"
    _states(problems, "RFP-021 S.1", _status_row(status, "RFP-021", "S.1"), (
        f"{updates} updates to LEZ block {as_of:,}", f"the longest interval {longest}", none_over,
        f"{len(adoptions)} committee adoptions, {len(run['refused'])} refusals", "(row 271)"))
    _states(problems, "RFP-022 S.1", _status_row(status, "RFP-022", "S.1"), (
        f"{updates} updates to 26 Nov 2026", "none more than 1 h apart" if not intervals["over_1h"] else "-",
        "(row 271)"))
    lines.append(f"row 271, RFP-021 S.1, RFP-022 S.1: {updates} updates, median {_duration(intervals['median_s'])}, "
                 f"longest {longest}, {len(adoptions)} adoptions, {len(run['refused'])} refusals, as of block {as_of:,} "
                 f"({RUN_SUMMARY})")

    # Row 273 and the rows citing it: the recorded seals the seal checks verify
    counts, unverifiable = {}, {}
    for release in ("3.0.0", "3.1.0"):
        # counted as _seal_check counts what it verifies: one entry per (seal, image id, journal), and for 3.1.0 the
        # live round trip's proofs the catalog does not already hold
        entries, nojournal, unmapped = checks_seals.recorded_seals(ctx.root, release)
        if release == "3.1.0":
            live = checks_seals._live_proofs(ctx.root)
            seen = {e["seal"] for e in entries}
            entries = entries + [e for e in live if e["seal"] not in seen]
            unmapped = {k: v for k, v in unmapped.items() if bytes.fromhex(k) not in {e["seal"] for e in live}}
        counts[release] = len(entries)
        unverifiable[release] = len(nojournal) + len(unmapped)
    _states(problems, "EVIDENCE row 273", _evidence_row(evidence, 273), (
        f"{counts['3.0.0']} of {counts['3.0.0']} seals of 3.0.0 and {counts['3.1.0']} of {counts['3.1.0']} of 3.1.0",
        f"{unverifiable['3.0.0']} more of 3.0.0 and {unverifiable['3.1.0']} of 3.1.0 are recorded without a journal"))
    for rid in ("F.7", "R.10"):
        row = _status_row(status, "RFP-022", rid)
        if row is None or "273" not in row.rsplit(" | ", 1)[-1]:
            problems.append(f"RFP-022 {rid}: its Evidence cell does not cite row 273")
    lines.append(f"row 273: {counts['3.0.0']} seals of 3.0.0 and {counts['3.1.0']} of 3.1.0 recorded with their image "
                 "ids and journals (seals-v3.0 and seals-v3.1 verify each)")

    # Row 274 and R.10: the rc3 round trip's steps
    steps = list(csv.reader(io.StringIO(ctx.path(RT_STEPS).read_text(encoding="utf-8")), delimiter="\t"))
    ok = sum(1 for row in steps if len(row) > 1 and row[1] == "ok")
    _states(problems, "EVIDENCE row 274", _evidence_row(evidence, 274), (f"{ok} of {len(steps)} steps ok",))
    _states(problems, "RFP-022 R.10", _status_row(status, "RFP-022", "R.10"), (f"{ok} of {len(steps)} steps (row 274",))
    lines.append(f"row 274, RFP-022 R.10: {ok} of {len(steps)} steps ok ({RT_STEPS})")

    # Row 272, R.11 and VERIFY.md: the registry and its coverage
    rows = catalog.load(ctx.root)  # every check registered once; the requirement rows of ctx.root
    cov = catalog.coverage(ctx.root, rows)
    checks = list(REGISTRY.values())
    quick = sum(1 for c in checks if c.mode == "offline" and c.tier == "quick")
    online = sum(1 for c in checks if c.mode == "online")
    tests = [c for c in checks if c.family == "test"]
    by_hand = sum(1 for c in tests if c.title.endswith("[by hand]"))
    no_ci = sum(1 for c in tests if "(not in CI:" in c.title)
    hard = [v for v in cov.values() if not v["soft"]]
    with_check = sum(1 for v in hard if v["quick_offline"] or v["online"] or v["tests"])
    shown = {rfp: [v for v in hard if v["rfp"] == rfp and v["status"] == "shown"] for rfp in ("RFP-021", "RFP-022")}
    a, b = shown["RFP-021"], shown["RFP-022"]
    _states(problems, "EVIDENCE row 272", _evidence_row(evidence, 272), (
        f"{len(checks)} checks: {quick} quick offline, {online} online (read-only), {len(tests)} named tests",
        f"({by_hand} of them recipes it lists and never runs", f"{no_ci} run on no CI job",
        f"at least one check ({with_check} of {len(hard)})",
        f"RFP-021 {sum(bool(v['quick_offline']) for v in a)} of {len(a)} have a quick offline check and "
        f"{sum(bool(v['online']) for v in a)} an online one",
        f"RFP-022 {sum(bool(v['quick_offline']) for v in b)} of {len(b)} and {sum(bool(v['online']) for v in b)};"))
    _states(problems, "RFP-022 R.11", _status_row(status, "RFP-022", "R.11"), (
        f"(`--coverage`: {with_check} of {len(hard)})", "(row 272)"))
    # check-row-tests.py reads git and walks the tree; a tamper test's copy (symlinks, no git: ctx.git_dir names the
    # checkout it mirrors) runs it in that checkout. The figures it gives come from the tree, not from the documents
    # those tests alter; verify.py and CI run it on the tree itself.
    where = getattr(ctx, "git_dir", None) or ctx.root
    out = subprocess.run([sys.executable, "-I", "scripts/ci/check-row-tests.py"], cwd=where, capture_output=True,
                         text=True, timeout=300).stdout
    in_ci = {m[0]: (int(m[1]), int(m[2])) for m in re.findall(r"(RFP-02[12]): (\d+) hard rows; (\d+) with a test CI runs",
                                                              out)}
    if set(in_ci) != {"RFP-021", "RFP-022"}:
        problems.append(f"scripts/ci/check-row-tests.py: no count of the rows with a test CI runs ({out[-200:]!r})")
    else:
        (h21, c21), (h22, c22) = in_ci["RFP-021"], in_ci["RFP-022"]
        _states(problems, "RFP-022 R.11", _status_row(status, "RFP-022", "R.11"), (
            f"{c22} of the {h22} run in CI ({c21} of {h21} RFP-021 rows have a test CI runs)",))
    verify_doc = ctx.path(VERIFY_DOC).read_text(encoding="utf-8")
    _states(problems, VERIFY_DOC, verify_doc, (f"{no_ci} run only on a", f"{by_hand} are recipes"))
    lines.append(f"row 272, RFP-022 R.11, {VERIFY_DOC}: {len(checks)} checks ({quick} quick offline, {online} online, "
                 f"{len(tests)} named tests, {by_hand} recipes, {no_ci} not in CI); {with_check} of {len(hard)} hard rows "
                 "with a check")
    fail_if(problems, "EVIDENCE rows 271-274 and the rows citing them")
    return lines
