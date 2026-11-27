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


# ---- EVIDENCE rows 275-281: the gate runs at A3's size, F.13 under ADR 0046, F.7's ERC-20 and rotation runs, and the
# tools' B1 layout; with RFP-021 R.9, R.10 and RFP-022 F.7, F.13, which cite them ---------------------------------------

GATE_100 = "lez/results/perf-gate-sequencer-100.md"
GATE_JOIN = "lez/results/perf-gate-sequencer-join.md"
GATE_OVERLAP = "lez/updater/results/gate-overlap.txt"
F13_FEES, F13_FEES_B1 = "e2e/both-halves/results/f13/fees.md", "e2e/both-halves/results/fees.md"
F13_RT_STEPS, F13_PROOFS = "e2e/runs/f13-roundtrip-1/run/steps.tsv", "lez/results/f13/prove-bridge.md"
F7_ERC20, F7_ROTATION = "e2e/runs/zone-bridge-rc3-erc20", "e2e/runs/zone-bridge-rc3-rotation"


def _gate_runs(text: str) -> dict:
    """Each run of a gate record (the generator's sections, which test_perf_records.py holds to the raw journals): A3's
    ratio, honest-1's share, the abandoned jobs, all gas over the completed jobs' and the latency median."""
    runs = {}
    for name, body in re.findall(r"^## Run `(gate-g\d+)`\n(.*?)(?=^## Run `|\Z)", text, re.S | re.M):
        grab = lambda pattern: (re.search(pattern, body) or [None, None])[1]
        runs[name] = {"ratio": grab(r"\*\*R\.9:\*\* the honest daemons' execution gas on refused steps / on applied "
                                    r"steps = [\d,]+ / [\d,]+ = \*\*([\d.]+)\*\*"),
                      "share": grab(r"\*\*R\.10:\*\* honest-1's share of the honest daemons' fees = \*\*([\d.]+)\*\*"),
                      "abandoned": grab(r"completed, \*\*(\d+) abandoned\*\*"),
                      "completed": grab(r"Jobs: \d+ with an applied step, (\d+) completed"),
                      "all": grab(r"the updates cost \*\*([\d.]+)\*\* times"),
                      "latency": grab(r"revealed finality updates covered \([^)]*\): median \*\*(\d+) s\*\*"),
                      "covered": grab(r"(\d+ of \d+) revealed finality updates covered")}
    return runs


def _overlap(text: str) -> dict:
    """gate-overlap.txt: each gate's honest gas besides the completed jobs' over theirs, by its join-or-wait setting."""
    out = {}
    for block in text.split("\n\n"):
        setting = re.search(r"honest join_wait_rounds\t([^\t\n]+)(?:\t([^\n]+))?", block)
        besides = re.search(r"besides the completed jobs' / the completed jobs'\t([\d.]+)", block)
        if setting and besides:
            key = setting[1] + (f" ({setting[2]})" if setting[2] else "")
            if key in out:
                raise ValueError(f"{GATE_OVERLAP}: two gates named {key}")
            out[key] = float(besides[1])
    return out


def _span(values: list, unit: str = "") -> str:
    lo, hi = min(values), max(values)
    return f"{lo}{unit}" if lo == hi else f"{lo}–{hi}{unit}"


@check(id="evidence-rows-final", family="docs", mode="offline", evidence=(275, 276, 277, 278, 279, 280, 281),
       rows=("RFP-021 R.9", "RFP-021 R.10", "RFP-022 F.7", "RFP-022 F.13"),
       title="EVIDENCE rows 275-281 and RFP-021 R.9, R.10, RFP-022 F.7, F.13: every figure as the records give it",
       what="the gate runs' A3 ratios, fee shares, abandoned jobs, gas over the completed jobs' and latency medians "
            "(rows 275, 276; R.9, R.10), the in-process overlap gates (row 276, R.9), F.13's anonymity sets, round "
            "trip steps and proving times (rows 277, 278; F.13), F.7's ERC-20 payments and rotation threshold (rows "
            "279, 280; F.7), and the tests row 281 names",
       against="the gate records' generated sections (test_perf_records.py holds them to the runs' journals), "
               "gate-overlap.txt (CI's recorded.rs), the F.13 both-halves tables and round trip steps, the real-proof "
               "record, the F.7 runs' vault checks and checkpoint attestor records (f7-* re-derive them), and the test "
               "sources",
       inputs=(EVIDENCE_DOC, STATUS_DOC, GATE_100, GATE_JOIN, GATE_OVERLAP, F13_FEES, F13_FEES_B1, F13_RT_STEPS,
               F13_PROOFS, f"{F7_ERC20}/vault-checks.json", f"{F7_ROTATION}/checkpoint/attestor.json",
               "cli/wallet/src/deployed.rs", "cli/src/lez.rs",
               "modules/ferrata_bridge_module/rust-lib/calls/tests/network_session.rs"))
def evidence_rows_final(ctx):
    problems, lines = [], []
    evidence = ctx.path(EVIDENCE_DOC).read_text(encoding="utf-8")
    status = ctx.path(STATUS_DOC).read_text(encoding="utf-8")
    read = lambda rel: ctx.path(rel).read_text(encoding="utf-8")

    # Rows 275, 276; R.9, R.10: the gate runs
    before, after = _gate_runs(read(GATE_100)), _gate_runs(read(GATE_JOIN))
    names_before, names_after = ["gate-g6", "gate-g7"], ["gate-g8", "gate-g9", "gate-g10", "gate-g11"]
    if sorted(before) != names_before or sorted(after, key=lambda n: int(n[6:])) != names_after or any(
            None in r.values() for r in [*before.values(), *after.values()]):
        problems.append(f"the gate records' runs did not parse: {before} {after}")
    else:
        b, a = [before[n] for n in names_before], [after[n] for n in names_after]
        f = lambda runs, key, sep=" / ": sep.join(r[key] for r in runs)
        _states(problems, "EVIDENCE row 275", _evidence_row(evidence, 275), (
            f"A3's ratio honest refused / applied {f(b, 'ratio')}", f"honest-1's fee share {f(b, 'share')}",
            f"latency median {f(b, 'latency')} s", f"all gas {f(b, 'all')} times the completed jobs'",
            *(f"{r['covered']} finality updates covered" for r in b[:1])))
        _states(problems, "EVIDENCE row 275", _evidence_row(evidence, 275),
                (f"jobs completed / abandoned {b[0]['completed']} / {b[0]['abandoned']} and {b[1]['completed']} / "
                 f"{b[1]['abandoned']}",))
        _states(problems, "EVIDENCE row 276", _evidence_row(evidence, 276), (
            f"jobs abandoned {f(a, 'abandoned')}", f"all gas {f(a, 'all')} times the completed jobs'",
            f"A3's ratio {f(a, 'ratio')}", f"honest-1's share {f(a, 'share')}", f"latency median {f(a, 'latency')} s"))
        overlap = _overlap(read(GATE_OVERLAP))
        with_policy, without, alone = overlap.get("None"), overlap.get("Some(0)"), overlap.get("None (honest-1 alone)")
        if with_policy is None or without is None or alone != 0:
            problems.append(f"{GATE_OVERLAP}: no gate with and without join or wait ({overlap})")
        else:
            pct = lambda v: f"{100 * v:.1f}%"
            _states(problems, "EVIDENCE row 276", _evidence_row(evidence, 276),
                    (f"{pct(with_policy)} with the policy, {pct(without)} without",))
            _states(problems, "RFP-021 R.9", _status_row(status, "RFP-021", "R.9"),
                    (f"{pct(with_policy)} of the completed jobs' gas besides", f"{pct(without)} without the policy"))
        _states(problems, "RFP-021 R.9", _status_row(status, "RFP-021", "R.9"), (
            f"A3's ratio {b[0]['ratio']} and {b[1]['ratio']}", f"{b[0]['abandoned']} and {b[1]['abandoned']} jobs abandoned",
            f"all gas {b[0]['all']} and {b[1]['all']} times", f"all gas {_span([r['all'] for r in a])} times",
            f"A3's ratio {_span([r['ratio'] for r in a])}", "275, 276"))
        if sum(int(r["abandoned"]) for r in a) != 1:
            problems.append("RFP-021 R.9 says one job abandoned in four runs; the records say otherwise")
        w8 = [before["gate-g6"], after["gate-g8"], after["gate-g10"]]
        w30 = [before["gate-g7"], after["gate-g9"], after["gate-g11"]]
        _states(problems, "RFP-021 R.10", _status_row(status, "RFP-021", "R.10"), (
            f"{b[0]['share']} and {b[1]['share']} without join or wait", f"{_span([r['share'] for r in a])} with it",
            f"latency median {_span([r['latency'] for r in w8], ' s')} at window 8",
            f"{_span([r['latency'] for r in w30], ' s')} at window 30",
            f"({_span([r['share'] for r in b + a])};", "275, 276"))
        if not all(float(r["share"]) > 0.5 for r in b + a):
            problems.append("RFP-021 R.10 says honest-1 paid more than half in all six runs; the records say otherwise")
        lines.append(f"rows 275-276, RFP-021 R.9, R.10: A3's ratio {f(b + a, 'ratio')}, honest-1's share "
                     f"{f(b + a, 'share')}, abandoned {f(b + a, 'abandoned')} ({GATE_100}, {GATE_JOIN}, {GATE_OVERLAP})")

    # Rows 277, 278; F.13
    def set_before(rel):
        table = read(rel).split("## Claims", 1)[-1].split("## Releases", 1)[0]
        sets = [int(m) for m in re.findall(r"^\| \d+ \| \d+ \|(?: [^|]+ \|)* (\d+) \| \d+ \|$", table, re.M)]
        return sets
    new_sets, old_sets = set_before(F13_FEES), set_before(F13_FEES_B1)
    if len(new_sets) != 6 or len(old_sets) != 6:
        problems.append(f"the both-halves fee tables did not parse: {new_sets} {old_sets}")
    else:
        lo, hi = new_sets[:3], new_sets[3:]
        _states(problems, "RFP-022 F.13", _status_row(status, "RFP-022", "F.13"), (
            f"keep one anonymity set ({lo[0]} and {hi[0]}; {old_sets[0]} and {old_sets[3]} before ADR 0046)",))
        _states(problems, "EVIDENCE row 277", _evidence_row(evidence, 277), (f"the set is {lo[0]} on both sides",))
        if len(set(new_sets)) != 1:
            problems.append(f"{F13_FEES}: the sets before any claim differ across the change: {new_sets}")
    steps = [r for r in csv.reader(io.StringIO(read(F13_RT_STEPS)), delimiter="\t") if r]
    ok = sum(1 for r in steps if len(r) > 1 and r[1] == "ok")
    proofs = read(F13_PROOFS)
    claim_s = (re.search(r"private claim: REAL PROOF: execute_and_prove ([\d.]+) s", proofs) or [None, "?"])[1]
    burn_s = (re.search(r"private burn: REAL PROOF: execute_and_prove ([\d.]+) s", proofs) or [None, "?"])[1]
    _states(problems, "RFP-022 F.13", _status_row(status, "RFP-022", "F.13"),
            (f"({ok} of {len(steps)} steps)", f"{claim_s} s and {burn_s} s", "277, 278, 281"))
    _states(problems, "EVIDENCE row 278", _evidence_row(evidence, 278),
            (f"{ok} of {len(steps)} steps ok", f"the private claim {claim_s} s", f"the private burn {burn_s} s"))
    lines.append(f"rows 277-278, RFP-022 F.13: sets {new_sets} (before ADR 0046 {old_sets}); {ok} of {len(steps)} steps; "
                 f"real proofs {claim_s} s and {burn_s} s")

    # Rows 279, 280; F.7
    checks = json.loads(read(f"{F7_ERC20}/vault-checks.json"))
    paid = sorted({int(r["paidWei"]) for r in checks["releases"]})
    attestor = json.loads(read(f"{F7_ROTATION}/checkpoint/attestor.json"))
    n, t = attestor.get("n"), attestor.get("t")
    words = {4: "four", 5: "five"}
    if len(paid) != 1 or len(checks["releases"]) != 2 or n not in words or t not in words:
        problems.append(f"the F.7 runs' records did not parse: paid {paid}, n {n}, t {t}")
    else:
        _states(problems, "RFP-022 F.7", _status_row(status, "RFP-022", "F.7"), (
            f"each released once for {paid[0]:,} base units", f"{words[t]} of {words[n]} keys", "279, 280"))
        _states(problems, "EVIDENCE row 279", _evidence_row(evidence, 279), (f"{paid[0]:,} base units each",))
        _states(problems, "EVIDENCE row 280", _evidence_row(evidence, 280), (f"(n = {n}, t = {t})",))
        lines.append(f"rows 279-280, RFP-022 F.7: two ERC-20 releases of {paid[0]:,} base units; rotation n = {n}, t = {t}")

    # Row 281: the tests it names exist where it says
    for rel, name in (("cli/wallet/src/deployed.rs", "fn b1s_frozen_claim_vectors_are_this_trees_claims_with_the_range"),
                      ("cli/wallet/src/deployed.rs", "fn the_loader_header_names_the_image_in_both_lez_layouts"),
                      ("cli/src/lez.rs", "fn the_claim_takes_the_layout_of_the_program_the_zone_runs"),
                      ("cli/src/lez.rs", "fn a_binary_of_another_program_is_refused_before_proving"),
                      ("modules/ferrata_bridge_module/rust-lib/calls/tests/network_session.rs",
                       'claim_approval["program"], "ferrata_bridge_b1"')):
        if name not in read(rel):
            problems.append(f"EVIDENCE row 281: {rel} has no {name}")
    _states(problems, "EVIDENCE row 281", _evidence_row(evidence, 281), ("B1's three frozen claim vectors",))
    lines.append("row 281: the B1-layout tests it names are in cli/wallet/src/deployed.rs, cli/src/lez.rs and the "
                 "module's network_session.rs (their toolchain runs are CI's roundtrip and modules-resolve jobs)")
    fail_if(problems, "EVIDENCE rows 275-281 and the rows citing them")
    return lines
