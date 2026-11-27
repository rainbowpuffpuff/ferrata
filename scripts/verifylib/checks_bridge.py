"""RFP-022 bridge records: rederive their per-case verdicts and figures.

These are offline record checks, not reruns of the Rust/Solidity suites. Each check reads only through ctx.root,
reconciles generated summaries with their committed case data where those data exist, and checks the status row's
wording too. LIMIT lines identify aggregate or prose-only assertions that cannot be recomputed from public bytes.
"""
from __future__ import annotations

import hashlib
import json
import re
import statistics
from fractions import Fraction
from pathlib import Path

from .core import check, fail_if
from .rows import status_rows

STATUS = "docs/requirements-status.md"
B1 = "lez/results/b1"
PRIVACY = "docs/privacy"
LIVE = "e2e/runs/live-roundtrip-2026-11-24"
BRIDGE_IMAGE = "847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515"


def _text(ctx, rel: str) -> str:
    return ctx.path(rel).read_text(encoding="utf-8")


def _json(ctx, rel: str):
    return json.loads(_text(ctx, rel))


def _int(s: str) -> int:
    return int(s.replace(",", "").replace("**", "").strip())


def _expect(problems: list, condition: bool, message: str) -> None:
    if not condition:
        problems.append(message)


def _phrases(problems: list, text: str, where: str, *fragments: str) -> None:
    for fragment in fragments:
        _expect(problems, fragment in text, f"{where} no longer states {fragment!r}")


def _row(ctx, row_id: str) -> str:
    found = [r for r in status_rows(ctx.root) if r.rfp == "RFP-022" and r.id == row_id]
    if len(found) != 1:
        raise ValueError(f"expected one RFP-022 {row_id} row, found {len(found)}")
    return found[0].text


def _tables(text: str) -> list[dict]:
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


def _table(text: str, first_header: str) -> list[list[str]]:
    return next(t["rows"] for t in _tables(text) if t["header"] and t["header"][0] == first_header)


def _summary_tables(text: str) -> tuple[list[list[str]], dict[str, list[list[str]]]]:
    summary = _table(text, "#")
    ledger_rows = next(t["rows"] for t in _tables(text) if t["header"][:3] == ["transaction", "handler", "cycles"])
    ledger = {}
    for row in ledger_rows:
        ledger.setdefault(row[0], []).append(row)
    return summary, ledger


def _charges(problems: list, rel: str, text: str) -> list[list[str]]:
    """Reconcile a harness record's summary to its independent per-handler ledger."""
    summary, ledger = _summary_tables(text)
    for row in summary:
        n, case, handlers, gas, pct, _storage, _fee, result = row[:8]
        cycles = [_int(re.search(r"([\d,]+)\s*$", part).group(1)) for part in handlers.split("→")]
        _expect(problems, sum(cycles) == _int(gas), f"{rel} case {n}: handler sum {sum(cycles)} != gas {gas}")
        own = ledger.get(case, [])
        _expect(problems, [_int(r[2]) for r in own] == cycles, f"{rel} case {n}: summary/handler cycles differ")
        code = re.search(r"refused: exit (\d+)", result)
        if code:
            _expect(problems, own and own[-1][-1] == code.group(1), f"{rel} case {n}: refusing handler != {code.group(1)}")
        elif result == "applied":
            _expect(problems, own and all(r[-1] == "0" for r in own), f"{rel} case {n}: applied with a nonzero handler exit")
        _expect(problems, pct == f"{int(_int(gas) * 100 / 5_000_000 + 0.5)}%", f"{rel} case {n}: wrong 5M percentage")
    return summary


def _charge_records(ctx, problems: list, paths: tuple[str, ...]) -> dict[str, list[list[str]]]:
    return {rel: _charges(problems, rel, _text(ctx, rel)) for rel in paths}


def _summary_only_charges(problems: list, rel: str, text: str) -> list[list[str]]:
    """For a record with no separate handler ledger, still check the summary's own arithmetic and codes."""
    rows = _table(text, "#")
    for row in rows:
        cycles = [_int(re.search(r"([\d,]+)\s*$", part).group(1)) for part in row[2].split("→")]
        _expect(problems, sum(cycles) == _int(row[3]), f"{rel} case {row[0]}: handler sum != execution gas")
        _expect(problems, row[4] == f"{int(_int(row[3]) * 100 / 5_000_000 + 0.5)}%", f"{rel} case {row[0]}: wrong percentage")
    return rows


def _case(summary: list[list[str]], prefix: str) -> list[str]:
    matches = [r for r in summary if r[1].startswith(prefix)]
    if len(matches) != 1:
        raise ValueError(f"case {prefix!r}: {len(matches)} matches")
    return matches[0]


def _pct(value: float) -> str:
    return f"{value * 100:.1f}%"


def _check_privacy_run(ctx, problems: list, base: str, real: bool) -> dict:
    label = "real" if real else "dev"
    result = _json(ctx, f"{base}/generated/results.json")
    summary = _text(ctx, f"{base}/generated/summary.md")
    b6 = _text(ctx, f"{base}/generated/b6.md")
    _expect(problems, len(result["gate"]) == 8, f"{label}: eight gate cases expected")
    for name, verdict in result["gate"].items():
        should_pass = name != "claim, plaintext"
        _expect(problems, verdict["pass"] is should_pass, f"{label}: gate verdict changed for {name}")
        lo, hi = result["sizes"][name]
        _expect(problems, lo == hi and lo > 0, f"{label}: {name}: size range {lo}..{hi}")
        size_cell = f"{lo:,}-{hi:,}"
        escaped = re.escape(name)
        _expect(problems, re.search(rf"^\| {escaped} \| {re.escape(size_cell)} \| (yes|NO:)", summary, re.M) is not None,
                f"{label}: summary size/gate row differs for {name}")
    private_names = ("inbound, private (default)", "outbound, private (default)")
    for name in private_names:
        game = result["adversary"][name]
        sets = game["candidate_set_sizes"]
        _expect(problems, sum(n for _, n in sets) == 20 and game["exactly_identified"] == 0,
                f"{label}: {name}: not twenty anonymous records")
        _expect(problems, sets == [[20, 20]] and abs(game["success"] - game["chance"]) < 1e-15
                and abs(game["success"] - 0.05) < 1e-15,
                f"{label}: {name}: success is not chance with twenty full sets")
        _expect(problems, _pct(game["success"]) == "5.0%", f"{label}: {name}: displayed rate changed")
        _expect(problems, abs(game["without_derived_identifiers"] - 0.05) < 1e-15
                and abs(game["with_order_state"] - 0.05) < 1e-15, f"{label}: {name}: variant rates changed")
    for name in ("inbound, naive nullifier N = fid", "inbound, plaintext claim", "outbound, naive NOut = burnId"):
        game = result["adversary"][name]
        _expect(problems, game["candidate_set_sizes"] == [[1, 20]] and game["exactly_identified"] == 20
                and game["success"] == 1.0, f"{label}: {name}: all twenty singleton pairings no longer identified")
    lez = result["adversary"]["on LEZ: claim to burn (informational)"]
    if real:
        _expect(problems, lez["candidate_set_sizes"] == [[1, 20]] and lez["exactly_identified"] == 1
                and lez["success"] == 0.05, f"{label}: claim-to-burn singleton success changed")
    else:
        # The record's own partition: the burns in a candidate set of all 20 (each guessed at 1/20) and the others in
        # singleton sets, none of them right (exactly_identified 0), so the rate is (burns in full sets)/20 x 1/20:
        # 18 of 20, 4.5%, before ADR 0046; 19 of 20, 4.8% (displayed), since (the sets move with the build's ids).
        sizes = {size: count for size, count in lez["candidate_set_sizes"]}
        full = sizes.get(20, 0)
        derived = full / 20 * (1 / 20)
        _expect(problems, set(sizes) <= {1, 20} and full + sizes.get(1, 0) == 20 and lez["exactly_identified"] == 0
                and abs(lez["success"] - derived) < 1e-15,
                f"{label}: the claim-to-burn rate {lez['success']} is not {full}/20 x 1/20 from its candidate sets")
    # The adversary table row by row: each game's own six cells against its own JSON object, not a search of the
    # whole document (another game's cells would satisfy that).
    adversary_rows = _table(summary, "game")
    rows_by_name = {}
    for row in adversary_rows:
        rows_by_name.setdefault(row[0], []).append(row[1:])
    _expect(problems, set(rows_by_name) == set(result["adversary"]),
            f"{label}: the adversary table's games {sorted(rows_by_name)} are not the JSON's "
            f"{sorted(result['adversary'])}")
    for name, game in result["adversary"].items():
        fields = [game["success"], game["exactly_identified"], game["candidate_set_sizes"],
                  game["without_derived_identifiers"], game["with_order_state"], result["order_only"][name]]
        rendered = [f"{x} of 20" if isinstance(x, int) else "100.0%" if x == 1 else _pct(x)
                    if isinstance(x, float) else ", ".join(f"{size}: {count}" for size, count in x) for x in fields]
        own = rows_by_name.get(name, [])
        _expect(problems, len(own) == 1, f"{label}: {len(own)} adversary table rows for {name}")
        if len(own) == 1:
            _expect(problems, own[0] == rendered,
                    f"{label}: the {name} row {own[0]} is not its JSON's {rendered}")
    hidden = result["hidden"]
    searched = sum(v["values searched"] for v in hidden.values())
    _expect(problems, len(hidden) == 4 and searched == (320 if real else 680),
            f"{label}: hidden-value search count is {searched}")
    _expect(problems, all(v["found"] == 0 for v in hidden.values()), f"{label}: a hidden value was found")
    b6_rows = _table(b6, "what must stay hidden")
    _expect(problems, [(r[0], _int(r[1]), _int(r[2])) for r in b6_rows] ==
            [(name, values["values searched"], values["found"]) for name, values in hidden.items()],
            f"{label}: generated/b6.md hidden table != results.json")
    for step, fields in result["fields"].items():
        if step != "claim, plaintext":
            bad = [f["field"] for f in fields if f["class"] == "UNCLASSIFIED"]
            _expect(problems, not bad, f"{label}: {step}: unclassified public fields {bad}")
    recovery = result["recovery"]
    _expect(problems, len(recovery["per_user"]) == 20 and [u["user"] for u in recovery["per_user"]] == list(range(20)),
            f"{label}: per-user recovery does not hold twenty users")
    _expect(problems, all(u["deposits_found"] == u["burns_found"] == 1 and u["recipient_ok"] for u in recovery["per_user"]),
            f"{label}: per-user recovery case failed")
    _expect(problems, (recovery["unclaimed_found"], recovery["unclaimed_expected"], recovery["claimed_seen_as_claimed"],
                       recovery["unreleased_found"], recovery["unreleased_expected"], recovery["released_seen_as_released"],
                       recovery["foreign_items"], recovery["nout_matches_vault"], recovery["recipients_decrypted_correctly"],
                       recovery["rust_core"]) == (10, 10, 10, 10, 10, 10, 0, 20, 20, 20),
            f"{label}: recovery totals changed")
    _expect(problems, result["proof_roots_newest"] == [80, 80], f"{label}: newest-root actions changed")
    return {"result": result, "summary": summary, "hidden_total": searched}


@check(id="bridge-privacy", title="Priv.1–Priv.4/F.2/F.8/F.9/R.5: both 20-user privacy ledgers",
       family="record", mode="offline", tier="quick", rows=("RFP-022 Priv.1", "RFP-022 Priv.2", "RFP-022 Priv.3",
       "RFP-022 Priv.4", "RFP-022 F.2", "RFP-022 F.8", "RFP-022 F.9", "RFP-022 R.5"), evidence=(23, 89),
       what="both runs' eight gates, the seven adversary games' own summary rows against their JSON, all classified fields, hidden-value counts and per-user recovery; the real run's 40 LEZ and 20 release proofs",
       against="docs/privacy/{generated,real/generated}/{results.json,summary.md,b6.md}, real/proofs.json, both RESULTS.md and the RFP-022 status text",
       inputs=(STATUS, f"{PRIVACY}/RESULTS.md", f"{PRIVACY}/generated/results.json", f"{PRIVACY}/generated/summary.md",
               f"{PRIVACY}/generated/b6.md", f"{PRIVACY}/real/RESULTS.md", f"{PRIVACY}/real/generated/results.json",
               f"{PRIVACY}/real/generated/summary.md", f"{PRIVACY}/real/generated/b6.md", f"{PRIVACY}/real/proofs.json"))
def bridge_privacy(ctx):
    problems = []
    dev = _check_privacy_run(ctx, problems, PRIVACY, False)
    real = _check_privacy_run(ctx, problems, f"{PRIVACY}/real", True)
    _expect(problems, real["hidden_total"] == 320 and dev["hidden_total"] == 680, "privacy run search totals changed")
    _phrases(problems, _text(ctx, f"{PRIVACY}/RESULTS.md"), "dev RESULTS", "Twenty fresh users", "looks for 680 values",
             "finds 0 of 680", "0 of 320 values found", "**5.0%**", "**100%**", "unclaimed deposits **10 of 10**")
    real_doc = _text(ctx, f"{PRIVACY}/real/RESULTS.md")
    _phrases(problems, real_doc, "real RESULTS", "20 of 20 deposit-and-claim pairs", "20 of 20 burn-and-release pairs",
             "40 of 40 private LEZ transactions", "20 of 20 releases", "chance (5.0%)", "hidden column is still 0 of 320")
    proofs = _json(ctx, f"{PRIVACY}/real/proofs.json")
    txs = proofs["lez_private_transactions"]
    releases = proofs["releases"]
    _expect(problems, len(txs) == 40 and [t["id"] for t in txs] == [f"lez:{kind}:private:{i:02d}" for kind in ("claim", "burn") for i in range(20)],
            "real: the 40 per-proof ids changed")
    _expect(problems, all(t["kind"] == "succinct" and t["seal_words"] == 55667 and t["claim_is_ok_of_journal"]
                          and not t["claim_is_pruned"] for t in txs), "real: a LEZ per-proof verdict changed")
    for kind in ("claim", "burn"):
        subset = [t for t in txs if t["id"].startswith(f"lez:{kind}:")]
        for key in ("proof_bytes", "tx_bytes"):
            _expect(problems, len({t[key] for t in subset}) == 1, f"real: {kind} {key} not constant across twenty")
        summary = real["result"]["proofs"]["steps"][f"{kind}, private (default)" if kind == "claim" else "burn, private holding"]
        _expect(problems, summary["records"] == summary["prove_s"]["n"] == 20, f"real: {kind} aggregate count changed")
        for key, field in (("proof_bytes", "proof_bytes"), ("tx_bytes", "tx_bytes")):
            _expect(problems, summary[key]["min"] == min(t[field] for t in subset)
                    and summary[key]["max"] == max(t[field] for t in subset), f"real: {kind} {key} summary differs")
        actual = [t["prove_s"] for t in subset]
        reported = summary["prove_s"]
        _expect(problems, abs(reported["sum"] - sum(actual)) < 1e-8 and reported["min"] == min(actual)
                and reported["median"] == statistics.median(actual) and reported["max"] == max(actual),
                f"real: {kind} timing summary differs")
        _phrases(problems, real_doc, "real RESULTS", f"**{min(actual):.1f}–{max(actual):.1f} s**, median {statistics.median(actual):.1f}")
    _expect(problems, len(releases) == 20 and [r["name"] for r in releases] == [f"release-{i:02d}" for i in range(20)],
            "real: release proof ids changed")
    seals = [bytes.fromhex(r["seal"][2:]) for r in releases]
    _expect(problems, len(set(seals)) == 20 and all(len(s) == 260 for s in seals), "real: Groth16 seal count/size changed")
    _expect(problems, all(r["succinct"]["user_cycles"] == 77736 and r["succinct"]["receipt_bytes"] == 223930
                          and set(r["verified"]) == {"groth16", "succinct"} for r in releases),
            "real: release per-proof size/verdict changed")
    _expect(problems, proofs["bridge_image_id"] == "ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd",
            "real: pre-B1 bridge id changed")
    _phrases(problems, _row(ctx, "Priv.1"), "Priv.1", "20 deposits", "chance (5.0%)", "docs/privacy/RESULTS.md",
             "docs/privacy/real/RESULTS.md", "no real-proof run of B1")
    _phrases(problems, _row(ctx, "Priv.2"), "Priv.2", "20 burns", "chance (5.0%)", "burn root's checkpoint mocked")
    _phrases(problems, _row(ctx, "Priv.3"), "Priv.3", "20 pairs each way", "0 of 320", "plaintext control fails")
    _phrases(problems, _row(ctx, "Priv.4"), "Priv.4", "0 of 320 hidden values over 20 round trips", "real proofs")
    _phrases(problems, _row(ctx, "F.2"), "F.2", "0 of 320", "calldata, events or storage")
    _phrases(problems, _row(ctx, "F.8"), "F.8", "20 users", "0.05", "naive variants 1.0")
    _phrases(problems, _row(ctx, "F.9"), "F.9", "10 of 10 unclaimed deposits and unreleased burns", "real proofs")
    _phrases(problems, _row(ctx, "R.5"), "R.5", "10 of 10 unclaimed deposits and unreleased burns", "real proofs")
    fail_if(problems, "privacy record/status mismatch")
    return [f"dev/real: 8 gate cases each; private inbound/outbound = 1/20 = 5.0%, naive controls = 100%; per-user recovery 20 records",
            f"zero hidden values found: real {real['hidden_total']} = 40+120+40+120; current dev {dev['hidden_total']} = 100+180+160+240",
            "real: 40 succinct LEZ transactions (20 claim + 20 burn), 20 verified releases with unique 260-B seals; proof tables re-aggregated",
            "LIMIT: candidate memberships and raw public bytes are not committed; searches/classifications are the recorded aggregates. Current dev is 0/680, so 0/320 describes the older dev run and the real run, not both current generated files"]


HALVES = "e2e/both-halves/results"
RANDOM_COLUMNS = ("steps", "deposits", "imports", "claims", "burns", "releases", "forgeries", "unminted", "unreleased", "checks")


def _random_runs(ctx, problems: list, rel: str) -> dict:
    text = _text(ctx, rel)
    rows = []
    for row in _table(text, "seed"):
        steps, deposits, imports, claims, capped, burns, releases, forgeries, unminted, unreleased, checks = row[1:]
        m = re.fullmatch(r"(\d+) \((\d+)\)", capped)
        _expect(problems, m is not None, f"{rel}: malformed capped cell {capped}")
        rows.append({"seed": int(row[0]), "steps": int(steps), "deposits": int(deposits), "imports": int(imports),
                     "claims": int(claims), "capped": int(m.group(1)) if m else -1,
                     "capped_later": int(m.group(2)) if m else -1, "burns": int(burns), "releases": int(releases),
                     "forgeries": int(forgeries), "unminted": int(unminted), "unreleased": int(unreleased),
                     "checks": int(checks)})
    _expect(problems, [r["seed"] for r in rows] == list(range(1, 9)), f"{rel}: seeds are not 1..8")
    for r in rows:
        _expect(problems, r["steps"] == 60, f"{rel}: seed {r['seed']} is not 60 steps")
        _expect(problems, r["unminted"] == r["deposits"] - r["claims"], f"{rel}: seed {r['seed']} unminted != deposits-claims")
        _expect(problems, r["unreleased"] == r["burns"] - r["releases"], f"{rel}: seed {r['seed']} unreleased != burns-releases")
        _expect(problems, 0 <= r["capped_later"] <= r["capped"], f"{rel}: seed {r['seed']} capped-later invalid")
    totals = {key: sum(r[key] for r in rows) for key in (*RANDOM_COLUMNS, "capped", "capped_later")}
    anonymity = []
    for row in _table(text.split("## Anonymity sets", 1)[1], "denomination (ETH)"):
        vals = [row[0]] + [int(x) for x in row[1:]]
        denomination, claims, small, med, large, ones, private_small, private_med, private_large, private_ones = vals
        _expect(problems, small <= med <= large and private_small <= private_med <= private_large,
                f"{rel}: {denomination}: anonymity extrema/median inconsistent")
        _expect(problems, 0 <= ones <= claims and 0 <= private_ones <= claims, f"{rel}: {denomination}: singleton count invalid")
        anonymity.append({"denomination": denomination, "claims": claims, "median": med, "private_median": private_med,
                          "ones": ones, "private_ones": private_ones})
    _expect(problems, sum(r["claims"] for r in anonymity) == totals["claims"],
            f"{rel}: denomination claims != seed-table claims")
    _expect(problems, len(anonymity) == 3, f"{rel}: expected three denominations")
    return {"totals": totals, "anonymity": anonymity, "text": text}


@check(id="bridge-both-halves", title="F.8/F.13/R.4/Sec.6/Sec.8/Sec.9: randomized, fee and migration records",
       family="record", mode="offline", tier="quick", rows=("RFP-022 F.8", "RFP-022 F.13", "RFP-022 R.4",
       "RFP-022 Sec.6", "RFP-022 Sec.8", "RFP-022 Sec.9"), evidence=(184, 186, 190, 191, 222),
       what="all 16 randomized seed rows' operation/end-position/invariant arithmetic and denomination totals, six fee claims and releases, and each migration/stop outcome",
       against="e2e/both-halves/results/{randomised,randomised-cap-20eth,fees,migration}.md and the six RFP-022 status rows",
       inputs=(STATUS, f"{HALVES}/randomised.md", f"{HALVES}/randomised-cap-20eth.md", f"{HALVES}/fees.md",
               f"{HALVES}/migration.md"))
def bridge_both_halves(ctx):
    problems = []
    random = _random_runs(ctx, problems, f"{HALVES}/randomised.md")
    capped = _random_runs(ctx, problems, f"{HALVES}/randomised-cap-20eth.md")
    expected = {"steps": 480, "deposits": 141, "imports": 44, "claims": 81, "burns": 59, "releases": 25,
                "forgeries": 93, "unminted": 60, "unreleased": 34, "checks": 496}
    _expect(problems, random["totals"] == {**expected, "capped": 0, "capped_later": 0},
            f"randomised totals changed: {random['totals']}")
    # The capped rerun's own totals: EVIDENCE 186 states 30 claims refused and 18 of them claimed later.
    _expect(problems, capped["totals"]["capped"] == 30 and capped["totals"]["capped_later"] == 18
            and capped["totals"]["claims"] == 63 and capped["totals"]["deposits"] == 142,
            f"capped totals changed: {capped['totals']}")
    _expect(problems, [(r["denomination"], r["median"], r["private_median"], r["ones"], r["private_ones"])
                       for r in random["anonymity"]] == [("0.1", 2, 4, 8, 2), ("1", 2, 4, 10, 5), ("10", 2, 3, 9, 3)],
            f"F.8 anonymity table changed: {random['anonymity']}")
    _expect(problems, [(r["denomination"], r["median"], r["private_median"], r["ones"], r["private_ones"])
                       for r in capped["anonymity"]] == [("0.1", 3, 4, 9, 3), ("1", 2, 4, 8, 5), ("10", 2, 2, 3, 3)],
            f"F.8 capped anonymity table changed: {capped['anonymity']}")
    _expect(problems, sum(r["ones"] for r in random["anonymity"]) == 27
            and sum(r["private_ones"] for r in random["anonymity"]) == 10, "F.8 singleton totals changed")
    _expect(problems, sum(r["ones"] for r in capped["anonymity"]) == 20
            and sum(r["private_ones"] for r in capped["anonymity"]) == 11, "F.8 capped singleton totals changed")
    _phrases(problems, random["text"], "randomised", "27 of 81", "10 of 81", "57 of the 81", "279 s in all", BRIDGE_IMAGE)
    _phrases(problems, capped["text"], "capped randomised", "20 of 63", "11 of 63", "39 of the 63", "SetCap` by the admin",
             "ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e")

    fees = _text(ctx, f"{HALVES}/fees.md")
    schedule = re.search(r"Schedule 1 announced at (\d+), in force at (\d+) \(seven days and 1 s later\), from leaf (\d+): "
                         r"mint \[(\d+), (\d+), (\d+)\], burn \[(\d+), (\d+), (\d+)\], checkpoint \[(\d+), (\d+), (\d+)\]", fees)
    _expect(problems, schedule is not None, "fees: schedule headline changed")
    if schedule:
        announced, effective, first_leaf, *fee_values = map(int, schedule.groups())
        _expect(problems, effective - announced == 7 * 86400 + 1 and first_leaf == 3, "fees: notice/range arithmetic changed")
        mint, burn, checkpoint_fee = fee_values[:3], fee_values[3:6], fee_values[6:9]
        _expect(problems, all(0 <= f <= d // 100 for f, d in zip(mint + burn + checkpoint_fee, [10**17, 10**18, 10**19] * 3)),
                "fees: a schedule fee exceeds 1%")
    else:
        mint = burn = checkpoint_fee = []
    claims = _table(fees.split("## Claims", 1)[1].split("## Releases", 1)[0], "leaf")
    _expect(problems, len(claims) == 6, "fees: six claim cases expected")
    for row in claims:
        leaf, sched, mint_fee, relay, before, at_claim = int(row[0]), int(row[1]), int(row[2]), int(row[3]), int(row[4]), int(row[5])
        _expect(problems, leaf == len([r for r in claims if int(r[0]) < leaf]) and sched == (1 if leaf >= 3 else 0),
                f"fees: leaf/schedule changed at {leaf}")
        _expect(problems, mint_fee == (5_000_000_000_000_000 if sched else 0) and relay == 1_000_000_000_000_000,
                f"fees: leaf {leaf} fee changed")
        _expect(problems, before == 3 and at_claim == 3 - leaf % 3, f"fees: leaf {leaf} anonymity split changed")
    if schedule:
        # The release's arithmetic from the schedule the headline states: 1 ETH less its burn and checkpoint fees.
        amount = 10**18
        received = amount - burn[1] - checkpoint_fee[1]
        owed = 3 * (burn[1] + checkpoint_fee[1])
        _expect(problems, received == 997_000_000_000_000_000 and owed == 9_000_000_000_000_000,
                f"fees: release amount/owed arithmetic changed: {received}/{owed}")
        _phrases(problems, fees, "fees", str(received), f"totalOwed` rose by {owed}", "Invariant checks: 12",
                 "Forgeries refused: 7")

    migration = _text(ctx, f"{HALVES}/migration.md")
    _phrases(problems, migration, "migration", "A's imported count stays 2", "ImportRoot after it refused (242)",
             "after Sunset: d1 claimed", "sunset checkpoint finalised (burns 2, imported 2)", "burn 0 released on the retired vault",
             "d2 and d3 refunded to their depositor (the whole escrow)", "refunds of d0 and d1 refused", "second refunds refused",
             "B: deposit d0 imported and claimed", "the proven release of burn 1 and the refund of d4 refused", "B goes on (d1 claimed)",
             "Invariant checks: A 6 times, B 2 times", "Forgeries refused: 9")
    for row, fragments in {
            "F.8": ("medians 2, 2, 2 and 27 sets of 1 among 81", "medians 4, 4, 3 and 10 sets of 1"),
            "F.13": ("over both halves every claim and release", "capped at 1%", "fee change a claim shows its schedule's range"),
            "R.4": ("Every later act only delays", "an imported deposit claimed after `Sunset`", "a burn released under the sunset checkpoint"),
            "Sec.6": ("eight seeds of 60 steps: 141 deposits, 81 claims, 59 burns, 25 releases; 496 checks; 93 forgeries refused",),
            "Sec.8": ("two never-imported deposits refunded whole", "9 forgeries refused", "a new vault and configuration taking a deposit, claim and burn"),
            "Sec.9": ("Over both halves the stop on one vault refuses a proven release and a refund while another configuration's vault goes on",),
    }.items():
        _phrases(problems, _row(ctx, row), row, *fragments)
    fail_if(problems, "both-halves record/status mismatch")
    return [f"8 seeds × 60 steps: {expected['deposits']} deposits, 44 imports, 81 claims, 59 burns, 25 releases; 496 checks, 93 forgeries; capped rerun (B1's first build, 30 capped, 18 later) independently summed",
            "F.8: 33+28+20 claims, medians 2/2/2 and 27 singleton sets as run, 4/4/3 and 10 all-private; capped rerun 33+22+8, medians 3/2/2 and 20; fee split 3→2→1 and 997e15 recipient / 9e15 owed",
            "migration: the record's own totals are A 6 + B 2 invariant checks and 9 forgeries refused; every named outcome (old-import claim, sunset release, two refunds, stopped release/refund, independent new vault) still stated",
            "LIMIT: seed rows hold aggregate per-seed counts, not each of 480 steps; medians come from the three denomination summaries because individual anonymity sets are absent; migration.md is a narrative of positions in flight, not a per-case table"]


def _applied_imports(rows: list) -> list:
    """Both programs' applied `ImportRoot` summary rows: every root import the harness settled."""
    return [r for r in rows if r[1].startswith("ImportRoot") and r[7] == "applied"]


def _stated_figure(text: str) -> tuple:
    """`10.25M` / `102k` / `934,029` as (value in its own unit, decimals, unit size)."""
    digits, unit = text[:-1], {"M": 1e6, "k": 1e3}.get(text[-1], 1)
    if text[-1] not in ("M", "k"):
        digits = text
    decimals = len(digits.split(".")[1]) if "." in digits else 0
    return float(digits), decimals, unit


@check(id="bridge-import-claim-costs", title="P.1/P.2: two inbound gas ledgers and exact import amortisation",
       family="record", mode="offline", tier="quick", rows=("RFP-022 P.1", "RFP-022 P.2"), evidence=(8, 27, 91, 173),
       what="all 64 summary rows against per-handler cycles and exits, both programs' applied-import ranges against P.2's stated ones, per-new-deposit quotients and hourly public/proven-path totals; two live ImportRoot cases",
       against="lez/results/{,b1/}bridge-inbound.md, perf-state.md §4a, perf-tracking.md and perf-proofs.md's measured inputs, docs/performance.md §P.2, crown-live-3/session-b records and RFP-022 P.1/P.2",
       inputs=(STATUS, "lez/results/bridge-inbound.md", f"{B1}/bridge-inbound.md", "lez/results/perf-state.md",
               "lez/results/perf-tracking.md", "lez/results/perf-proofs.md", "docs/performance.md",
               "e2e/runs/crown-live-3/lez/session-b/session-b.json",
               "e2e/runs/crown-live-3/lez/session-b/logs/driver.log"))
def bridge_import_claim_costs(ctx):
    problems = []
    paths = ("lez/results/bridge-inbound.md", f"{B1}/bridge-inbound.md")
    ledgers = _charge_records(ctx, problems, paths)
    for rel, rows in ledgers.items():
        _expect(problems, len(rows) == 32, f"{rel}: 32 cases expected")
        claim = _case(rows, "Claim: the shared vector's deposit")
        wanted = 265_694 if rel.startswith(B1) else 261_671
        _expect(problems, _int(claim[3]) == wanted and claim[7] == "applied", f"P.1: {rel} claim gas/verdict changed")
        expected_parts = ([107_796, 52_567, 35_962, 31_769, 18_800, 18_800] if rel.startswith(B1)
                          else [107_702, 51_166, 35_565, 30_916, 18_161, 18_161])
        parts = [_int(re.search(r"([\d,]+)\s*$", p).group(1)) for p in claim[2].split("→")]
        _expect(problems, parts == expected_parts and sum(parts) == wanted, f"P.1: claim handler breakdown changed: {parts}")
        _expect(problems, wanted < 10_000_000, "P.1: claim exceeds 10M limit")
        for name, code in (("Claim: the same again", "250"), ("Claim: the same deposit under root(A, 8)", "250")):
            _expect(problems, f"refused: exit {code} " in _case(rows, name)[7], f"{rel}: {name} code changed")
    roots_a = [_int(ledgers[rel][i][3]) for rel in paths for i in (6, 7, 13)]
    _expect(problems, roots_a[:3] == [230_731, 236_224, 236_224] and roots_a[3:] == [231_635, 237_140, 237_140],
            f"P.2: A import sequence changed {roots_a}")
    old_b, new_b = _int(ledgers[paths[0]][14][3]), _int(ledgers[paths[1]][14][3])
    _expect(problems, (old_b, new_b) == (236_202, 237_169), "P.2: configuration-B import costs changed")
    # P.2's stated harness ranges, held to every applied import of the two records (configuration A and B).
    p2 = _row(ctx, "P.2")
    stated = {"deployed": re.search(r"measured \((\d[\d,]*)\s*[–-]\s*(\d[\d,]*) execution gas in the harness for the deployed", p2),
              "B1": re.search(r"; (\d[\d,]*)\s*[–-]\s*(\d[\d,]*) on B1's", p2)}
    for (name, m), rel in zip(stated.items(), paths):
        applied = [_int(r[3]) for r in _applied_imports(ledgers[rel])]
        _expect(problems, m is not None, f"P.2: the {name} harness range is no longer stated")
        _expect(problems, len(applied) == 4, f"P.2: {rel}: {len(applied)} applied ImportRoot cases, not four")
        if m is None or len(applied) != 4:
            continue
        lo, hi = _int(m.group(1)), _int(m.group(2))
        _expect(problems, (lo, hi) == (min(applied), max(applied)),
                f"P.2 states {name}'s harness import range as {lo:,}–{hi:,}, but {rel}'s four applied ImportRoot "
                f"cases run {min(applied):,}–{max(applied):,} ({', '.join(f'{a:,}' for a in applied)})")
    imports = _table(_text(ctx, "lez/results/perf-state.md").split("## 4a.", 1)[-1].split("## 4.", 1)[0], "import")
    for row in imports:
        gas, newly, quotient = _int(row[3]), int(row[2]), row[5]
        expected_q = None if newly == 0 else int(Fraction(gas, newly) + Fraction(1, 2))
        _expect(problems, (newly == 0 and "none new" in quotient) or (newly and _int(quotient) == expected_q),
                f"P.2: per-new-deposit quotient changed: {row}")
    # §4a is the deployed program's own measurement: its four import costs are the harness record's four applied
    # imports, in the same order (A n=5, A n=8, A n=6 lagging, B n=8).
    section4a = [_int(r[3]) for r in imports]
    _expect(problems, section4a == [_int(r[3]) for r in _applied_imports(ledgers[paths[0]])],
            f"P.2: perf-state.md §4a's import costs {section4a} are not the deployed harness record's applied imports "
            f"{[_int(r[3]) for r in _applied_imports(ledgers[paths[0]])]}")
    session = _json(ctx, "e2e/runs/crown-live-3/lez/session-b/session-b.json")
    live_imports = [s for s in session["steps"] if s["step"].startswith("bridge ImportRoot")]
    _expect(problems, len(live_imports) == 2 and [s["execGas"] for s in live_imports] == ["432182", "432182"]
            and all(s["status"] == "applied" and s["exit"] == "" for s in live_imports), "P.2: live import cases changed")
    driver = _text(ctx, "e2e/runs/crown-live-3/lez/session-b/logs/driver.log")
    for step in live_imports:
        _expect(problems, f"gas    {step['execGas']} block" in driver and step["tx"] in driver, "P.2: driver log != session JSON")
    performance = _text(ctx, "docs/performance.md")
    # The hourly figures' measured inputs, read from their records, not retyped: perf-tracking.md's mainnet medians
    # (a finality update and a period's handoff) and perf-proofs.md's two settled proven-path transactions.
    per_update = _table(_text(ctx, "lez/results/perf-tracking.md").split("## Per update", 1)[1].split("## Per day", 1)[0],
                        "chain")
    medians = {(r[0], r[1]): _int(r[4]) for r in per_update}
    finality_median, handoff_median = medians.get(("mainnet", "finality")), medians.get(("mainnet", "period"))
    _expect(problems, isinstance(finality_median, int) and finality_median > 0
            and isinstance(handoff_median, int) and handoff_median > 0,
            f"P.2: perf-tracking.md's mainnet medians unreadable: {medians}")
    proofs_doc = _text(ctx, "lez/results/perf-proofs.md")
    settled = {label: re.search(rf"^\| {label} \| .*\| (\d[\d,]*) / \d[\d,]* then \1 / \d[\d,]* \| 0 \|", proofs_doc, re.M)
               for label in ("1", "handoff-1")}
    _expect(problems, all(settled.values()), "P.2: perf-proofs.md's settled proven-path rows unreadable")
    if not (finality_median and handoff_median and all(settled.values())):
        fail_if(problems, "inbound cost/status mismatch")
    proven_update, proven_handoff = (_int(settled["1"].group(1)), _int(settled["handoff-1"].group(1)))
    live_import = _int(live_imports[0]["execGas"])
    period_s = 8192 * 12  # a sync-committee period, exactly; the handoff's share of an hour
    public_hour = finality_median + Fraction(handoff_median * 3600, period_s) + live_import
    proven_hour = proven_update + Fraction(proven_handoff * 3600, period_s) + live_import
    rounded = lambda x: int(x + Fraction(1, 2))
    cadence = re.search(r"(\d[\d.]*M) / (\d[\d.]*M) / (\d+k) at 1 / 10 / 100 deposits an hour, "
                        r"(\d[\d,]*) / (\d[\d,]*) / (\d[\d,]*) on the proven path", p2)
    _expect(problems, cadence is not None, "P.2: the per-deposit cadence figures are no longer stated")
    if cadence:
        public_said = [_stated_figure(x) for x in cadence.groups()[:3]]
        proven_said = [_int(x) for x in cadence.groups()[3:]]
        _expect(problems, [rounded(proven_hour / n) for n in (1, 10, 100)] == proven_said,
                f"P.2: proven-path cadence {proven_said} != the derived {[rounded(proven_hour / n) for n in (1, 10, 100)]} "
                f"(hourly {float(proven_hour):.3f})")
        derived_public = [round(float(public_hour) / n / fig[2], fig[1]) for fig, n in zip(public_said, (1, 10, 100))]
        _expect(problems, derived_public == [fig[0] for fig in public_said],
                f"P.2: public-path cadence {[fig[0] for fig in public_said]} not supported: the derived hourly "
                f"{float(public_hour):.1f} gives {derived_public}")
    _phrases(problems, performance, "performance P.2", "235.5M execution gas a day", "16,549", "934,029, 93,403 and 9,340",
             "432,182 per deposit at one deposit an hour, 43,218 at ten, 4,322 at a hundred")
    _phrases(problems, _row(ctx, "P.1"), "P.1", "265,694 execution gas", "`Claim` 107,796", "`Issue` 52,567",
             "`Admit` 35,962", "`Spend` 31,769", "two `Mint`s of 18,800", "261,671", "10,000,000")
    # P.2's ranges and cadence figures are held to their records above, not pinned as phrases: while the row's stated
    # B1 maximum (237,140) is below its record's vault-B import (237,169), the range comparison is what fails.
    _phrases(problems, p2, "P.2", "432,182")
    fail_if(problems, "inbound cost/status mismatch")
    return [f"32+32 ledger rows reconciled; claim row 18 = 265,694 (B1) / 261,671 (deployed), each exactly its six handler cycles",
            f"A imports deployed/B1: {roots_a[:3]} / {roots_a[3:]}; B's {old_b:,} / {new_b:,}; two live imports 432,182; per-deposit integer quotients recomputed",
            f"hourly cadence from the records' own inputs (finality {finality_median:,}, handoff {handoff_median:,}, "
            f"proven {proven_update:,}/{proven_handoff:,}, import {live_import:,}; period {period_s:,} s): public "
            f"{float(public_hour):.1f}, proven {float(proven_hour):.1f} an hour",
            f"LIMIT: live cost inputs are their session summary, not a replay; P.2's stated B1 harness maximum is held to "
            f"the record's four applied imports, and fails while the row says 237,140 but the B1 record's vault-B import "
            f"costs {new_b:,}"]


@check(id="bridge-vault-costs", title="P.3: real-verifier local gas and all five live-network receipts",
       family="record", mode="offline", tier="quick", rows=("RFP-022 P.3",), evidence=(13, 25, 91, 250),
       what="eight local real-proof operations and anvil receipts, crown-live-3's 13-receipt subtotal/checkpoint sum, and the rc3 vault's five named gas receipts and statuses",
       against="vault/GAS.md, live-roundtrip-2026-11-24/{record.json,README.md} and RFP-022 P.3",
       inputs=(STATUS, "vault/GAS.md", f"{LIVE}/record.json", f"{LIVE}/README.md"))
def bridge_vault_costs(ctx):
    problems = []
    gas = _text(ctx, "vault/GAS.md")
    local = gas.split("## With the real verifier", 1)[1].split("Creations, from", 1)[0]
    rows = _table(local, "Operation")
    _expect(problems, len(rows) == 8, "GAS.md: eight real-verifier operations expected")
    allowed_differences = {"`registerToken`, three denominations: ETH / the test token ³": 24,
                           "`finalize` with a real checkpoint seal that echoes the beacon ⁴": 12}
    for operation, forge, fork in rows:
        a, c = (([_int(x) for x in forge.split("/")], [_int(x) for x in fork.split("/")]) if "/" in forge
                else ([_int(forge)], [_int(fork)]))
        _expect(problems, len(a) == len(c) and all(f - s in (allowed_differences.get(operation, 0), -allowed_differences.get(operation, 0))
                                                   for f, s in zip(a, c)), f"GAS.md: unexplained forge/fork delta for {operation}: {a}/{c}")
    crown = gas.split("| Nonce | Transaction | **Sepolia receipt (crown-live-3)**", 1)[1].split("A rule (B) checkpoint", 1)[0]
    crown_rows = _table("| Nonce | Transaction | **Sepolia receipt (crown-live-3)**" + crown, "Nonce")
    numbered = [r for r in crown_rows if r[0].isdigit()]
    _expect(problems, len(numbered) == 13 and [int(r[0]) for r in numbered] == list(range(23, 36)), "GAS.md: crown nonces changed")
    crown_total = sum(_int(r[2]) for r in numbered)
    _expect(problems, crown_total == 74_947_202 and f"**{crown_total:,}**" in crown, f"GAS.md: crown receipt total is {crown_total}")
    previous = [_int(r[4]) for r in numbered]
    _expect(problems, sum(previous) == 74_947_322, "GAS.md: crown-live-1 subtotal changed")
    by_nonce = {int(r[0]): r for r in numbered}
    submit = _int(next(r[2] for n, r in by_nonce.items() if "submitCandidate" in r[1]))
    finalise = _int(next(r[2] for n, r in by_nonce.items() if "finalizeAttested" in r[1]))
    _expect(problems, submit + finalise == 1_573_698 and f"{submit:,} + {finalise:,} = **{submit + finalise:,}**" in gas,
            "GAS.md: rule-B checkpoint sum changed")
    record = _json(ctx, f"{LIVE}/record.json")
    steps = record["sepolia"]
    expected = {"deposit": 614_222, "submitCandidate": 729_373, "finalizeAttested": 844_349,
                "release": 732_686, "claimOwed": 44_728}
    _expect(problems, [s["step"] for s in steps[:5]] == list(expected), "P.3: live step sequence changed")
    for step in steps[:5]:
        name = step["step"]
        _expect(problems, step["status"] == 1 and step["gas_used"] == expected[name] and step["to"] == record["vault"],
                f"P.3: {name}: receipt status/gas/to changed")
    _expect(problems, [s["nonce"] for s in steps[1:5]] == [0, 1, 2, 3], "P.3: live relayer nonces changed")
    readme = _text(ctx, f"{LIVE}/README.md")
    for name, value in expected.items():
        _expect(problems, f"gas {value:,}" in readme, f"P.3: README lacks {name} gas {value:,}")
    # The live release (0.001 ETH) pays the recipient 0.000997, leaves the 0.000002 relay fee in escrow and owes the
    # 0.000001 checkpoint fee; the fee claim then clears what is owed. All five amounts are the record's own reads.
    release = next(s for s in steps if s["step"] == "release")
    escrow_out = release["vault_before"]["escrowed(ETH)"] - release["vault_after"]["escrowed(ETH)"]
    recipient_paid = release["vault_after"]["balance(recipient)"] - release["vault_before"]["balance(recipient)"]
    _expect(problems, escrow_out == 1_000_000_000_000_000 and recipient_paid == 997_000_000_000_000
            and release["vault_after"]["escrowed(ETH)"] == 2_000_000_000_000
            and release["vault_after"]["totalOwed(ETH)"] == 1_000_000_000_000
            and release["vault_after"]["owed(relayer, ETH)"] == 1_000_000_000_000
            and release["vault_before"]["nullifierUsed(NOut)"] is False and release["vault_after"]["nullifierUsed(NOut)"] is True,
            "P.3: live release escrow/owed/nullifier transition changed")
    _phrases(problems, readme, "live README", "relay fee 0.000002 to the relayer", "`OwedCredited` 0.000001",
             "the recipient's balance **+0.000997 ETH**", "`escrowed(ETH)` 0.001002 -> 0.000002")
    claim = next(s for s in steps if s["step"] == "claimOwed")
    _expect(problems, claim["vault_after"]["totalOwed(ETH)"] == 0 and claim["vault_after"]["owed(relayer, ETH)"] == 0
            and claim["vault_before"]["balance(vault)"] - claim["vault_after"]["balance(vault)"] == 1_000_000_000_000,
            "P.3: the fee claim did not clear the owed checkpoint fee")
    _phrases(problems, _row(ctx, "P.3"), "P.3", "614,222 / 729,373 / 844,349 / 732,686 gas", "fee claim 44,728")
    fail_if(problems, "vault gas/live receipt/status mismatch")
    return [f"GAS.md: 8 real-proof local operations forge/anvil reconciled (only documented 24/12-gas calldata deltas); 13 crown receipts sum {crown_total:,}",
            f"live vault receipts: deposit 614,222 / candidate 729,373 / finalise 844,349 / release 732,686 / claimOwed 44,728; statuses 1, relayer nonces 0–3",
            "release: 0.001002 -> 0.000002 ETH escrowed (0.000997 paid out, the 0.000002 relay fee kept), 0.000001 ETH owed and then claimed; nullifier false -> true; crown checkpoint 729,349+844,349=1,573,698",
            "LIMIT: these are receipt and document cross-checks, not a chain replay or gas estimation; scripts/verify-live-roundtrip.py separately verifies chain linkage"]


@check(id="bridge-state-projections", title="P.7: measured bridge state slopes and 1M/10M projections",
       family="record", mode="offline", tier="quick", rows=("RFP-022 P.7",), evidence=(170, 176),
       what="nullifier/burn slopes, holder/key arithmetic, vault per-operation slots and derived decimal MB/GB projections",
       against="lez/results/{perf-state.md,perf-state-ethereum.txt,perf-projections.md} and RFP-022 P.7",
       inputs=(STATUS, "lez/results/perf-state.md", "lez/results/perf-state-ethereum.txt", "lez/results/perf-projections.md"))
def bridge_state_projections(ctx):
    problems = []
    state = _text(ctx, "lez/results/perf-state.md")
    growth = _table(state.split("## 5.", 1)[1], "records added")
    nullifier_slopes, burn_slopes = set(), set()
    previous = None
    rates = []
    for row in growth:
        n, nullifier, nullifier_rate, burn, burn_rate = int(_int(row[0])), _int(row[1]), float(row[2]), _int(row[3]), float(row[4])
        _expect(problems, (nullifier_rate, burn_rate) == (165.0, 273.0), f"P.7: displayed per-record rates changed at {n}")
        if previous:
            dn = n - previous[0]
            nullifier_slopes.add(Fraction(nullifier - previous[1], dn))
            burn_slopes.add(Fraction(burn - previous[2], dn))
        previous = (n, nullifier, burn)
        rates.append((n, nullifier, burn))
    _expect(problems, len(growth) == 3 and nullifier_slopes == {165} and burn_slopes == {273},
            f"P.7: LEZ slopes changed: {nullifier_slopes}/{burn_slopes}")
    claims = [r for r in _table(state.split("## 4.", 1)[1].split("## 5.", 1)[0], "transaction") if r[0].startswith("public claim")]
    grew = [_int(r[4]) for r in claims]
    _expect(problems, len(grew) == 2 and grew[0] - 165 == 3 * (grew[1] - 165) and grew[1] - 165 == 162,
            f"P.7: holder growth is not 3×162 and 1×162: {grew}")
    # The root row's own arithmetic, both measurements of it: LEZ's account layout is actor state + 56 = borsh
    # Account (the light-client record check's own rule), and the account with its 32-B key is the 205 B stated.
    root = re.search(r"root\(cid, \d+\) \| (\d+) \| (\d+) \|", state)
    root_actor, root_account = (int(root.group(1)), int(root.group(2))) if root else (0, 0)
    root_total = root_account + 32
    _expect(problems, root is not None and root_actor + 56 == root_account and root_total == 205,
            f"P.7: the root row is not actor state {root_actor} + 56 = borsh Account {root_account}, "
            f"+ 32 = account and key 205 B")
    burn = [r for r in _table(state.split("## 4.", 1)[1].split("## 5.", 1)[0], "transaction") if r[0].startswith("public burn")]
    _expect(problems, len(burn) == 2 and all(_int(r[2]) + 56 == _int(r[3]) and _int(r[3]) + 32 == _int(r[4]) == 273
                                            for r in burn), "P.7: burn sizes changed")
    eth = _text(ctx, "lez/results/perf-state-ethereum.txt")
    slots = {m.group(1): (int(m.group(2)), int(m.group(3))) for m in re.finditer(r"GROWTH\t(.*?)\tops.*\n\s+(\d+) (\d+) \d+", eth)}
    expected_slots = {"releases, 25 more (total)": (25, 25), "deposits by 25 new addresses (total)": (25, 28),
                      "deposits by the same 25 addresses again (total)": (25, 1), "deposit, another address's first": (1, 1)}
    _expect(problems, all(slots.get(k) == v for k, v in expected_slots.items()), f"P.7: vault growth cases changed: {slots}")
    _expect(problems, "Suite result: ok. 1 passed; 0 failed; 0 skipped" in eth, "P.7: Foundry growth suite verdict changed")

    mb = lambda b: f"{b / 1e6:.2f} MB" if b < 1e9 else f"{b / 1e9:.2f} GB"
    projections = _text(ctx, "lez/results/perf-projections.md")
    for n in (1_000_000, 10_000_000):
        _expect(problems, mb(n * 165) in projections and mb(n * 273) in projections
                and f"| {n:,} |" in projections and f"{n:,} ({mb(n * 64)} of keys and values)" in projections,
                f"P.7: 1M/10M projection cells changed for {n}")
    holder = grew[1] - 165
    _expect(problems, f"{mb(1_000_000 * holder)} when" in projections and f"{mb(2_000_000 * holder)} if" in projections
            and f"{mb(10_000_000 * holder)} and {mb(20_000_000 * holder)}" in projections, "P.7: holder projections changed")
    _expect(problems, mb(8766 * root_total) == "1.80 MB"
            and f"8,766 a year at one an hour: {mb(8766 * root_total)}" in projections,
            "P.7: root annual projection changed")
    _phrases(problems, projections, "perf-projections", "nullifier 165 B", "burn record 273 B", "frontier is 32 slots",
             "1,000,000 + at most 32 / at most 32", "10,000,000 + at most 32 / at most 32", "162 to 324 B")
    _phrases(problems, _row(ctx, "P.7"), "P.7", "nullifier 165 B", "burn record 273 B", f"root {root_total:,} B",
             "162 B per new holder",
             "one slot per release and per new depositor", "nullifiers 165 MB / 1.65 GB", "holders 162–324 MB at 1M claims")
    fail_if(problems, "state projection record/status mismatch")
    return [f"three LEZ growth rows: exact deltas 165 B/nullifier and 273 B/burn; holder deltas 486=3×162 and 162; "
            f"root {root_actor}+56={root_account}, +32={root_total} B",
            "vault: releases 25/25 one slot each; 25 new depositors 28 slots (25+3 frontier), repeat 25/1 frontier only; suite verdict ok",
            f"1M/10M: nullifiers {mb(1e6*165)}/{mb(1e7*165)}, burns {mb(1e6*273)}/{mb(1e7*273)}, holders {mb(1e6*holder)}–{mb(2e6*holder)} at 1M; 8,766 roots = {mb(8766*root_total)}/year",
            "LIMIT: MB/GB are decimal units as in the generator; the 68-B added holding and 32-slot frontier bound are documented bounds, not additional growth runs"]


@check(id="bridge-u8-refusals", title="U.8: CLI/freeze refusal cases and mini-app verdicts",
       family="record", mode="offline", tier="quick", rows=("RFP-022 U.8",), evidence=(62, 85, 202, 221, 233),
       what="the log's source/code/refusal cases, the nine required freeze modes (vault deposits/releases whole and "
       "per token, bridge mints/burns per token and whole, and the private path) each anchored to its own raw-log "
       "refusal, and the 12 tool + 7 mini-app mutant results (only the no-mutant control fails nothing)",
       against="e2e/runs/{cli-u8,cli-freezes}/{README.md,test.log}, modules/ferrata_bridge_ui/results/qmltest.txt and RFP-022 U.8",
       inputs=(STATUS, "e2e/runs/cli-u8/README.md", "e2e/runs/cli-u8/test.log", "e2e/runs/cli-freezes/README.md",
               "e2e/runs/cli-freezes/test.log", "modules/ferrata_bridge_ui/results/qmltest.txt"))
def bridge_u8(ctx):
    problems = []
    log = _text(ctx, "e2e/runs/cli-u8/test.log")
    expected = {
        "NOT FINAL (claim, no root imported)": "exit code 245 NOT_FOUND",
        "NOT FINAL (claim, before its root is imported)": "exit code 248 LEAF_NOT_IMPORTED",
        "NOT FINAL (claim, its root's import inside the depth)": "exit code 248 LEAF_NOT_IMPORTED",
        "VERIFICATION (claim, plaintext path of a private-kind deposit)": "exit code 252 WRONG_DESTINATION",
        "VERIFICATION (claim, opening redirected)": "exit code 249 CLAIM_INVALID",
        "NOT FINAL (release, its checkpoint a candidate)": "vault error BurnRootNotAccepted()",
        "VERIFICATION (release, proof rejected by the verifier)": "vault error VerificationFailed()",
        "UNSUPPORTED TOKEN (deposit)": "vault error TokenNotRegistered()",
        "INVALID AMOUNT (deposit)": "vault error InvalidDenominationIndex()",
        "CAP (deposit, eth_call)": "vault error TokenDepositCapExceeded()",
        "second claim": "exit code 250 ALREADY_CLAIMED",
        "burn 1": "exit code 254 INSUFFICIENT_BALANCE",
        "burn 0.5": "exit code 251 NOT_REGISTERED",
        "release in the delay": "vault error ReleaseDelayNotElapsed()",
    }
    for case, code in expected.items():
        matches = [line for line in log.splitlines() if case in line and "refused:" in line and code in line]
        _expect(problems, len(matches) == 1, f"U.8: {case}: {len(matches)} matching {code} refusals")
        if case == "NOT FINAL (claim, its root's import inside the depth)":
            _expect(problems, matches and "64 beacon slots (about 13 min)" in matches[0], "U.8: depth case no longer names 64 slots")
        _expect(problems, matches and ("checked before sending" in matches[0] or "eth_call" in matches[0]),
                f"U.8: {case}: pre-send source missing")
    _expect(problems, len({f"exit code {n}" for n in (245, 248, 249, 250, 251, 252, 254)} -
                       set(re.findall(r"exit code \d+", log))) == 0, "U.8: program-code set changed")
    _phrases(problems, log, "U.8 log", "test names_nothing_catches_every_form ... ok",
             "test deposit_scan_claim_burn_release_and_recovery ... ok", "test result: ok. 2 passed; 0 failed", "finished in 55.05s")
    freeze_log = _text(ctx, "e2e/runs/cli-freezes/test.log")
    freeze_readme = _text(ctx, "e2e/runs/cli-freezes/README.md")
    freeze_rows = _table(freeze_readme.split("## What the tool says", 1)[1], "Freeze")
    # The nine required freeze modes as an explicit matrix, each row anchored to its own refusal line in the raw
    # log (its FREEZE marker, error code and command path), not nine arbitrary distinct labels: the vault's four
    # (deposits/releases x whole vault/token) and the bridge's four (mints/burns x token/whole configuration),
    # plus the private path of the bridge's whole-configuration mint freeze, refused before its proof.
    freeze_matrix = (
        # (the table row's Freeze cell, its Command cell, the raw log's FREEZE line, the code it refuses with)
        ("vault deposits, whole vault (`setDepositsFrozen`)", "`ferrata deposit` (the binary)",
         "FREEZE (vault, deposit): error: refused: vault error DepositsAreFrozen()", "DepositsAreFrozen"),
        ("vault deposits of ETH (`setTokenDepositsFrozen`)", "`ferrata deposit`",
         "FREEZE (vault, deposit): error: refused: vault error TokenDepositsAreFrozen()", "TokenDepositsAreFrozen"),
        ("bridge mints of ETH (`Freeze`, token)", "`claim` (plaintext path)",
         "FREEZE (LEZ, claim, mints of this token):", "242 FROZEN"),
        ("bridge mints, whole configuration (`Freeze`, no token)", "`claim` (plaintext path)",
         "FREEZE (LEZ, claim, mints of the whole configuration):", "242 FROZEN"),
        ("bridge burns of ETH", "`burn` (public holding)",
         "FREEZE (LEZ, burn, burns of this token):", "242 FROZEN"),
        ("bridge burns, whole configuration", "`burn`",
         "FREEZE (LEZ, burn, burns of the whole configuration):", "242 FROZEN"),
        ("vault releases, whole vault (`setReleasesFrozen`)", "`release` (self-sent)",
         "FREEZE (vault, release): refused: vault error ReleasesAreFrozen()", "ReleasesAreFrozen"),
        ("vault releases of ETH (`setTokenReleasesFrozen`)", "`release`",
         "FREEZE (vault, release): refused: vault error TokenReleasesAreFrozen()", "TokenReleasesAreFrozen"),
        ("bridge mints, whole configuration", "`claim` (private path, before its proof)",
         "FREEZE (LEZ, private claim):", "242 FROZEN"),
    )
    freeze_by_label = {r[0]: r for r in freeze_rows}
    _expect(problems, len(freeze_rows) == 9 and len(freeze_by_label) == 9,
            f"U.8: {len(freeze_rows)} freeze rows, {len(freeze_by_label)} distinct, nine expected")
    _expect(problems, set(freeze_by_label) == {row[0] for row in freeze_matrix},
            f"U.8: the freeze table's rows {sorted(freeze_by_label)} are not the nine required modes "
            f"{sorted(row[0] for row in freeze_matrix)}")
    for label, command, anchor, code in freeze_matrix:
        row = freeze_by_label.get(label)
        _expect(problems, row is not None, f"U.8: no freeze table row {label!r}")
        if row is None:
            continue
        _expect(problems, row[1] == command, f"U.8: the {label} row's command is {row[1]!r}, not {command!r}")
        _expect(problems, code in row[2] or (code == "242 FROZEN" and "…" in row[2]),
                f"U.8: freeze table code changed for {label}")
        matches = [ln for ln in freeze_log.splitlines() if ln.startswith(anchor) and code in ln]
        _expect(problems, len(matches) == 1,
                f"U.8: {label}: {len(matches)} raw-log refusals start {anchor!r} with {code}, one expected")
        _expect(problems, matches and "checked before sending" in matches[0],
                f"U.8: {label}: its raw-log refusal is not a pre-send check")
    _expect(problems, "test result: ok. 1 passed; 0 failed" in freeze_log and "finished in 34.47s" in freeze_log,
            "U.8: freeze test verdict changed")
    qml = _text(ctx, "modules/ferrata_bridge_ui/results/qmltest.txt")
    _expect(problems, len(re.findall(r"^PASS   :", qml, re.M)) == 27 and "Totals: 27 passed, 0 failed" in qml and not re.search(r"^FAIL!", qml, re.M),
            "U.8: mini-app 27-case verdict changed")
    for code in ("245 NOT_FOUND", "250 ALREADY_CLAIMED", "TokenNotRegistered", "InvalidDenominationIndex",
                 "TokenDepositCapExceeded", "249 CLAIM_INVALID", "252 WRONG_DESTINATION", "BurnRootNotAccepted",
                 "VerificationFailed", "242 FROZEN"):
        _expect(problems, f"U8 {code} " in qml, f"U.8: mini-app U8 banner code {code} absent")
    u8 = _text(ctx, "e2e/runs/cli-u8/README.md")
    tool_mutants = _table(u8.split("## Each check fails", 1)[1].split("Mini-app:", 1)[0], "Mutant")
    _expect(problems, len(tool_mutants) == 12 and all(r[2].startswith("fails") for r in tool_mutants),
            "U.8: twelve failing tool-mutant results changed")
    mini_mutants = _table(u8.split("Mini-app:", 1)[1], "Mutant")
    # The seven mini-app mutant results: F0, the no-mutant control, is the only one allowed to fail nothing;
    # each Q0-Q5 must fail the tests its `Must break` cell names, and those names must be the run's own tests.
    qml_names = re.findall(r"^PASS   : qmltestrunner::FerrataMiniApp::(\w+)\(\)", qml, re.M)

    def _named_test(name):
        """The qmltest function a mutant's `Must break` cell names (`test_09`, shorthand `06b`): the one function
        equal to it or continuing it before another word character, or None when that is not exactly one."""
        hits = [f for f in qml_names if f == name or f.startswith(name + "_")]
        return hits[0] if len(hits) == 1 else None

    def _expected_failures(cell):
        """The test names the cell names: `test_09`, `06b` shorthand and `09b`–`09f` ranges, expanded."""
        names = []
        tokens = [(m.group(1), m.start(), m.end()) for m in re.finditer(r"`([^`]+)`", cell)]
        i = 0
        while i < len(tokens):
            tok, _, end = tokens[i]
            name = tok if tok.startswith("test_") else "test_" + tok
            if i + 1 < len(tokens) and cell[end:tokens[i + 1][1]].strip() == "–":
                after = tokens[i + 1][0]
                after = after if after.startswith("test_") else "test_" + after
                stem, last = name[:-1], name[-1]
                if after[:-1] == stem and last.isalpha() and "a" <= last <= after[-1] <= "z":
                    names += [stem + chr(c) for c in range(ord(last), ord(after[-1]) + 1)]
                    i += 2  # the range's end is consumed with its start
                    continue
            names.append(name)
            i += 1
        return names

    _expect(problems, len(mini_mutants) == 7
            and [r[0].split()[0] for r in mini_mutants] == [f"Q{i}" for i in range(6)] + ["F0"],
            "U.8: the seven mini-app mutant rows are not Q0-Q5 then F0")
    for mid, must_break, result in mini_mutants:
        m = re.match(r"(\d+) passed, (\d+) failed", result)
        _expect(problems, m is not None, f"U.8: mini-app mutant {mid}: unreadable result {result!r}")
        if m is None:
            continue
        passed, failed = int(m.group(1)), int(m.group(2))
        _expect(problems, passed + failed == len(qml_names),
                f"U.8: mini-app mutant {mid}: {passed} passed + {failed} failed, the run has {len(qml_names)} tests")
        if mid.startswith("F0"):
            _expect(problems, failed == 0 and must_break == "nothing",
                    f"U.8: the no-mutant control F0 broke {failed} tests ({must_break!r})")
        else:
            named = _expected_failures(must_break)
            _expect(problems, failed > 0, f"U.8: mutant {mid} broke nothing ({result!r})")
            _expect(problems, len(named) == failed,
                    f"U.8: mutant {mid}: {must_break!r} names {len(named)} tests, {failed} failed")
            _expect(problems, all(_named_test(n) for n in named),
                    f"U.8: mutant {mid}: {must_break!r} names tests the 27-case run does not have: {named}")
    _phrases(problems, u8, "U.8 README", "31 named forms fail", "names_nothing_catches_every_form", "every U.08 refusal of the sequence",
             "unmoved", "the signer's nonce is unchanged")
    _phrases(problems, _row(ctx, "U.8"), "U.8", "9 of 9", "252 by the plaintext path", "249 on a redirected opening",
             "245 before any import", "248 before the claim's root's", "inbound depth",
             "`BurnRootNotAccepted`", "`VerificationFailed`", "Mutants break each check", "all seven modes")
    fail_if(problems, "U.8 record/status mismatch")
    return [f"{len(expected)} named CLI cases rechecked with pre-send source/code; the nine required freeze modes as their own table rows and raw-log FREEZE lines; CLI verdict 2 passed, freeze verdict 1 passed",
            "mini-app: 27 PASS, 0 FAIL; all seven U8 modes' banner codes present; 12 tool mutants fail their checks; "
            "Q0-Q5 each fail the tests their rows name (the run's own), F0 alone fails nothing",
            "LIMIT: names-nothing/value and unchanged nonces are the tests' recorded assertions; raw per-mutant logs are not committed beside README's summaries"]


@check(id="bridge-core-cases", title="F.3–F.6/F.15/R.1–R.3: inbound, burn, tag and private-path cases",
       family="record", mode="offline", tier="quick", rows=("RFP-022 F.3", "RFP-022 F.4", "RFP-022 F.5", "RFP-022 F.6",
       "RFP-022 F.15", "RFP-022 R.1", "RFP-022 R.2", "RFP-022 R.3"), evidence=(8, 220),
       what="all inbound/burn/tag summary-versus-handler cases, repeat/root/refusal codes, private claim/burn outputs, tag state assertions and corresponding BRIDGE-RESULTS tables",
       against="lez/results/b1/{bridge-inbound,bridge-burn,bridge-burn-tag,bridge-private-claim}.md, lez/BRIDGE-RESULTS.md §§2–5 and the status rows",
       inputs=(STATUS, f"{B1}/bridge-inbound.md", f"{B1}/bridge-burn.md", f"{B1}/bridge-burn-tag.md",
               f"{B1}/bridge-private-claim.md", "lez/BRIDGE-RESULTS.md"))
def bridge_core(ctx):
    problems = []
    paths = (f"{B1}/bridge-inbound.md", f"{B1}/bridge-burn.md")
    ledgers = _charge_records(ctx, problems, paths)
    inbound, burns = (ledgers[p] for p in paths)
    tags = _summary_only_charges(problems, f"{B1}/bridge-burn-tag.md", _text(ctx, f"{B1}/bridge-burn-tag.md"))
    _expect(problems, [len(inbound), len(burns), len(tags)] == [32, 13, 8], "core: case counts changed")
    for prefix in ("Claim: the same again", "Claim: the same deposit under root(A, 8)"):
        _expect(problems, "refused: exit 250 (ALREADY_CLAIMED)" in _case(inbound, prefix)[7], f"F.3/R.3: {prefix} code changed")
    _expect(problems, "refused: exit 246 (PAIRING_MISMATCH)" in _case(inbound, "Init A'")[7]
            and "refused: exit 110 (ProofNodeMismatch)" in _case(inbound, "ImportRoot A with vault B's proofs")[7],
            "F.15: Init pairing/foreign-proof codes changed")
    _expect(problems, _case(inbound, "Claim: the shared vector")[7] == "applied"
            and _case(inbound, "Claim: vault B's deposit")[7] == "applied", "F.3: control claims changed")
    for prefix in ("Burn: the same note again",):
        _expect(problems, "refused: exit 244 (ALREADY_WRITTEN)" in _case(burns, prefix)[7], "R.2/R.3: repeated note code changed")
    for i in (1, 2, 3, 6):
        _expect(problems, "refused: exit 244 (ALREADY_WRITTEN)" in tags[i][7], f"R.2/R.3: tag case {i+1} code changed")
    tag_text = _text(ctx, f"{B1}/bridge-burn-tag.md")
    _phrases(problems, tag_text, "burn-tag", "B's holding whole, A's debited once", "code 244); nothing sent",
             "settled reverted (Program exited with non-zero code 244 after 20873 cycles)", "the fee paid, the 1 ETH holding whole",
             "counters [0, 1, 2, 4], 4 distinct NOut", "burn(cid, r) holds the first note")
    private = _text(ctx, f"{B1}/bridge-private-claim.md")
    exits = _table(private, "handler execution")
    _expect(problems, len(exits) == 2 and all(r[-1] == "0" for r in exits), "F.4: private transition exits changed")
    _expect(problems, "| 556,901 | 224,063 | 6,247,712 | 8,937 | 4,525 |" in private, "F.4: private claim charge/size changed")
    absent = re.findall(r"^- (.+?) \(\d+ bytes\): absent$", private, re.M)
    siblings = [x for x in absent if x.startswith("path sibling")]
    _expect(problems, len(siblings) == 32 and len(absent) == 38, f"F.4/Priv: private search has {len(absent)} absences/{len(siblings)} siblings")
    _phrases(problems, private, "private claim", "N (public by design): present", "configId (public by design): present",
             "the root read (public by design): present")
    burn_text = _text(ctx, f"{B1}/bridge-burn.md")
    _phrases(problems, burn_text, "private burn", "Burn at the private holder", "| 0 |", "execution gas 515,479, storage gas 224,063",
             "private burn | PrivateToPublic | 185 | Burn | yes")
    _expect(problems, "| Burn: the same note again | PublicRoot | 185 | Burn | no (reverted) |" in burn_text,
            "F.5/R.3: checkpoint no longer ignores reverted duplicate burn")
    results = _text(ctx, "lez/BRIDGE-RESULTS.md")
    _phrases(problems, results, "BRIDGE-RESULTS", "the same claim again (replay)", "250 `ALREADY_CLAIMED`",
             "the same deposit under a second root", "246 `PAIRING_MISMATCH`", "110 `ProofNodeMismatch`",
             "the same note again (the holder still holds 9 ETH)", "244 `ALREADY_WRITTEN`",
             "private burn of one denomination", "private claim, private destination")
    for row, fragments in {
            "F.3": ("repeat and second-root claims refused (error 250)", "Mints only on a claim under a root"),
            "F.4": ("Mints to public and private accounts",), "F.5": ("both live burns", "Burns write the note once"),
            "F.6": ("Burns from public and private holdings",), "F.15": ("Init` refuses another configuration (246)", "foreign proofs (110)"),
            "R.1": ("Refused claims exit with codes and spend nothing",), "R.2": ("a second note with a used tag refused",),
            "R.3": ("Second claim (250)", "same note or another (244)", "replayed release refused"),
    }.items():
        _phrases(problems, _row(ctx, row), row, *fragments)
    fail_if(problems, "bridge core case/status mismatch")
    return ["inbound 32, burn 13, burn-tag 8 cases: all cycle sums/5M percentages/final handler exits reconciled; repeat/root claims 250, Init 246/110, repeated tags 244",
            "private claim: both transitions exit 0; 32 path siblings + six other private inputs absent, public N/configId/root present; private burn delivery collected once",
            "BRIDGE-RESULTS §§2–5 still state the corresponding cases; tag record holds whole refused holdings and four distinct recovered NOut",
            "LIMIT: harness records prove dev-mode behaviour only; live Sepolia/applied-sequencer parts of F.3–F.6 and vault replay/nullifier state of R.2/R.3 are outside these tables"]


@check(id="bridge-controls-depth-listing", title="F.10–F.12/F.14/Sec.4–Sec.5: 115 control/delay cases",
       family="record", mode="offline", tier="quick", rows=("RFP-022 F.10", "RFP-022 F.11", "RFP-022 F.12",
       "RFP-022 F.14", "RFP-022 Sec.4", "RFP-022 Sec.5"), evidence=(8, 28, 188, 189),
       what="78 controls, 22 depth and 15 delisting cases with their handler ledgers; maximum/boundary depth arithmetic, listing monotonicity, token/global caps and freezes",
       against="lez/results/b1/{bridge-controls,bridge-depth,bridge-delist}.md, BRIDGE-RESULTS.md §8/§12 and the six status rows",
       inputs=(STATUS, f"{B1}/bridge-controls.md", f"{B1}/bridge-depth.md", f"{B1}/bridge-delist.md", "lez/BRIDGE-RESULTS.md"))
def bridge_controls(ctx):
    problems = []
    paths = (f"{B1}/bridge-controls.md", f"{B1}/bridge-depth.md", f"{B1}/bridge-delist.md")
    ledgers = _charge_records(ctx, problems, paths)
    controls, depth, listing = (ledgers[p] for p in paths)
    _expect(problems, [len(controls), len(depth), len(listing)] == [78, 22, 15], "controls: case counts changed")
    _expect(problems, "applied" in _case(controls, "SetCap (admin): ETH (A) mints")[7]
            and "applied" in _case(controls, "SetCap (admin): global mints of B at the floor")[7]
            and "applied" in _case(controls, "SetCap (admin): ETH (A) burns 1 ETH")[7], "F.11: per-token/global SetCap cases changed")
    for prefix in ("Claim leaf 2 (1.001 ETH) with 0.94", "Claim B's 1 ETH (1.001 ETH", "Burn 1 ETH (A) again: the burn bucket"):
        _expect(problems, "refused: exit 255 (CAPPED)" in _case(controls, prefix)[7], f"Sec.4/F.11: {prefix} cap refusal changed")
    for prefix in ("Claim B's 1 ETH while its mints are frozen", "Claim B's 1 ETH under the global mint freeze",
                   "Burn 1 ETH (A) while its burns are frozen", "Burn 1 ETH (A) under the global burn freeze"):
        _expect(problems, "refused: exit 242 (FROZEN)" in _case(controls, prefix)[7], f"F.14/Sec.5: {prefix} freeze refusal changed")
    _expect(problems, "applied" in _case(controls, "Claim B's 1 ETH after Sunset")[7]
            and "applied" in _case(controls, "Burn 1 ETH (B) after Sunset")[7]
            and "refused: exit 242 (FROZEN)" in _case(controls, "ImportRoot B after Sunset")[7],
            "Sunset control cases changed (R.4's own check is bridge-both-halves)")
    for prefix, code in (("SetDepth 65 by the freeze authority", "253"), ("SetDepth 65 by an outsider", "253"),
                         ("SetDepth 216,001 by the admin", "240"), ("ImportRoot under block 102", "242")):
        _expect(problems, f"refused: exit {code} " in _case(depth, prefix)[7], f"F.12: {prefix} code changed")
    _expect(problems, _case(depth, "SetDepth 216,000")[7] == "applied"
            and _case(depth, "SetDepth 0")[7] == "applied", "F.12: max/default SetDepth changed")
    _expect(problems, "refused: exit 242" in _case(depth, "ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 65 needed")[7]
            and _case(depth, "ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed")[7] == "applied",
            "F.12: 64/65 boundary changed")
    # F.12's boundary, from the record's own words: block 101 sits at slot 9,000,032 and the newest record the
    # configuration's clock read at slot 9,000,096, so an import under it is 64 slots behind the tip.
    depth_text = _text(ctx, f"{B1}/bridge-depth.md")
    block101 = re.search(r"under block 101 \(slot (\d[\d,]*)\), depth 65", depth_text)
    tip = re.search(r"Tick A: a record at slot (\d[\d,]*)", depth_text)
    _expect(problems, block101 is not None and tip is not None
            and _int(tip.group(1)) - _int(block101.group(1)) == 64,
            "F.12: the tip-to-block-101 distance is no longer the 64 slots the boundary cases name")
    for prefix in ("Claim of leaf 2 under root(A, 5)", "Claim of leaf 6 under root(A, 8)"):
        _expect(problems, "applied" in _case(depth, prefix)[7] and "depth now 216,000" in _case(depth, prefix)[1],
                "F.12: older-root claim invalidated by depth change")
    for prefix in ("AttestListing ETH under the same record again", "AttestListing ETH: the slot holds 2",
                   "AttestListing ETH under the delisting's record"):
        _expect(problems, "refused: exit 247 (IMPORT_MISMATCH)" in _case(listing, prefix)[7], f"F.10: {prefix} code changed")
    for prefix in ("Claim of leaf 2 (1 ETH), after the delisting", "Burn of 1 ETH by the payer, after the delisting",
                   "Transfer of 1 ETH from the payer, after the delisting", "AttestListing ETH: relisted"):
        _expect(problems, _case(listing, prefix)[7] == "applied", f"F.10: {prefix} no longer applies")
    results = _text(ctx, "lez/BRIDGE-RESULTS.md")
    _phrases(problems, results, "BRIDGE-RESULTS", "255 `CAPPED`", "242 `FROZEN`", "SetCap` by the admin",
             "a record 10 s later", "**stalled light client**", "a block stamped **a year ahead**", "a header that does not hash")
    for row, fragments in {
            "F.10": ("AttestListing`", "never undone by an older record", "claims of earlier deposits, burns, transfers and releases still apply"),
            "F.11": ("Per-token and global buckets", "attested Ethereum time"), "F.12": ("admin only (253)", "at most 216,000 slots (240)", "default 0", "held until", "242"),
            "F.14": ("All 32 freeze combinations on the vault", "per token or globally (242)"), "Sec.4": ("Buckets bound the value at risk on both sides",),
            "Sec.5": ("Each half freezes by its own authority",),
    }.items():
        _phrases(problems, _row(ctx, row), row, *fragments)
    fail_if(problems, "bridge control/depth/listing record/status mismatch")
    return ["78 controls + 22 depth + 15 delisting: cycle sums, final codes and percentages reconciled; per-token/global caps 255, freezes 242, Sunset import refusal and continued claim/burn",
            "depth: 9,000,096-9,000,032=64, so 65 refuses and 64 applies; 216,001 refuses 240, 216,000 applies; older-root claims still apply",
            "listing: older/same/unexpected states refuse 247; claim, burn, transfer and relisting apply; BRIDGE-RESULTS §8/§12 case assertions retained",
            "LIMIT: these records cover the LEZ half; the vault's 32 freeze combinations, 99-deposit default-cap case and outbound deployment delay defaults have no committed per-case tables here"]


TENANT = f"{B1}/tenant"


@check(id="bridge-tenant-separation", title="F.16: configuration-isolation message matrix and gas invariance",
       family="record", mode="offline", tier="quick", rows=("RFP-022 F.16",), evidence=(8, 136, 183),
       what="12 privileged cross-configuration refusals, 15 internal-message refusals/controls, rogue-instance outcomes and 0-to-12,098 foreign-holding gas/state/cycle invariance",
       against="lez/results/b1/tenant/{separation,r1,r2}.txt and RFP-022 F.16",
       inputs=(STATUS, f"{TENANT}/separation.txt", f"{TENANT}/r1.txt", f"{TENANT}/r2.txt"))
def bridge_tenant(ctx):
    problems = []
    separation = _text(ctx, f"{TENANT}/separation.txt")
    privileged = _table(separation.split("## T's roles", 1)[1].split("## Internal messages", 1)[0], "message")
    _expect(problems, len(privileged) == 13, f"F.16: privileged matrix has {len(privileged)} rows, not 12 plus one execution-rule row")
    for row in privileged[:-1]:
        _expect(problems, row[2] == "refused, exit 253 (NOT_AUTHORIZED)", f"F.16: privileged {row[0]} outcome changed")
    _expect(problems, sum(r[2] == "refused, exit 253 (NOT_AUTHORIZED)" for r in privileged) == 12,
            "F.16: not exactly twelve privileged 253 refusals")
    _expect(problems, privileged[-1][0].startswith("ImportRoot") and "Program violated execution rules" in privileged[-1][2],
            "F.16: cross-record ImportRoot execution-rule case changed")
    for prefix in ("SetCap (global mints)", "SetCap (ETH burns)", "SetDepth 216,000", "Sunset"):
        matches = [r for r in privileged if r[0] == prefix]
        _expect(problems, len(matches) == 1 and matches[0][3] == "applied", f"F.16: {prefix} own-configuration control changed")
    _expect(problems, "A's configuration and ETH token state after every attempt: identical to before" in separation,
            "F.16: unchanged-state assertion missing")
    internal = _table(separation.split("## Internal messages", 1)[1].split("## Every transaction", 1)[0], "delivered (the program's executor)")
    _expect(problems, len(internal) == 20 and sum(r[1] == "243" for r in internal) == 15,
            f"F.16: internal matrix is not 15 refusals among 20: {len(internal)}")
    controls = [r for r in internal if "control" in r[0]]
    _expect(problems, len(controls) == 5, "F.16: five internal control rows expected")
    _expect(problems, all((r[1] == "0") == ("no token state" not in r[0] and "no configuration state" not in r[0]) for r in controls),
            "F.16: internal control exits changed")
    every = _table(separation.split("## Every transaction", 1)[1], "transaction")
    for row in every:
        if "naming A" in row[0] and "ImportRoot" not in row[0]:
            _expect(problems, row[1] == "refused, exit 253 (NOT_AUTHORIZED)", f"F.16: ledger disagrees for {row[0]}")
    rogue = _text(ctx, f"{TENANT}/r1.txt")
    _phrases(problems, rogue, "tenant r1", "B: Claim of A's deposit (leaf 2, public kind) with its s | applied",
             "A: Claim of the same deposit (leaf 2) by its owner | applied", "A: the same claim again | refused, exit 250",
             "burn(configId(B), r): written; burn(configId(A), r): empty", "nullifier of the private-kind deposit (leaf 5): unspent under A and under B")
    _expect(problems, "| 10 | A: Claim of the same deposit" in rogue and "| 11 | A: the same claim again" in rogue,
            "F.16: detailed rogue-instance ledger changed")
    r2 = _text(ctx, f"{TENANT}/r2.txt")
    claim_rows = _table(r2.split("## A claim", 1)[1].split("## The owner's burn", 1)[0], "k")
    burn_rows = _table(r2.split("## The owner's burn", 1)[1].split("## holder(A, dest)", 1)[0], "k")
    for name, rows, cycle in (("claim", claim_rows, "18800"), ("burn", burn_rows, "28270")):
        _expect(problems, min(int(r[0]) for r in rows) == 0 and max(int(r[0]) for r in rows) == 12_098,
                f"F.16: {name} k range changed")
        first = rows[0]
        for k, state_bytes, outcome, gas, fee, cycles in rows:
            k_int, state_int = int(k), int(state_bytes)
            _expect(problems, state_int == 74 * k_int, f"F.16: {name} k={k}: state {state_int} != 74*k")
            _expect(problems, (outcome, gas, fee, cycles) == ("applied", first[3], first[4], cycle),
                    f"F.16: {name} k={k}: outcome/gas/fee/cycles changed")
        for a, b in zip(rows, rows[1:]):
            _expect(problems, int(b[1]) - int(a[1]) == 74 * (int(b[0]) - int(a[0])),
                    f"F.16: {name}: non-linear state growth")
    _expect(problems, claim_rows[0][3] == "261772" and burn_rows[0][3] == "156678", "F.16: k=0 gas changed")
    _phrases(problems, r2, "tenant r2", "Every claim and every burn above: the same gas, fee and cycles as with k = 0",
             "Execution gas limit of a transaction: 10000000")
    _phrases(problems, _row(ctx, "F.16"), "F.16", "strictly apart", "same gas at 0 and 12,098",
             "12 privileged and 15 internal messages", "release binds the vault's `lezConfigId`")
    fail_if(problems, "tenant-separation record/status mismatch")
    return ["T's roles: 12 naming-A privileged calls refuse 253 with their naming-T controls applying where sent; 20 internal rows = 15 code-243 refusals + five non-243 controls",
            "rogue configuration: A's owner claim applies after B's, A replay refuses 250, B burn does not write A, private leaf unspent under both",
            "4 claims + 3 burns over k=0..12,098: state bytes exactly 74*k and outcome/gas/fee/target cycles unchanged (261,772 and 156,678 gas)",
            "LIMIT: r1/r2 are settled ledgers and post-state assertions, not independently decoded account bytes; the release-side lezConfigId binding belongs to the cited vault tests"]


REFUSALS = f"{B1}/bridge-refusals.md"


@check(id="bridge-refusals-privacy", title="Priv.5: refused, repeated and capped claims publish nothing more",
       family="record", mode="offline", tier="quick", rows=("RFP-022 Priv.5",), evidence=(185,),
       what="the private path's three refusals (blocks and public accounts each 0), the plaintext path's four cases with their exit codes, and the deposit fields each publishes",
       against="lez/results/b1/bridge-refusals.md's two case tables, idl/ferrata_bridge.idl.json's codes and RFP-022 Priv.5",
       inputs=(STATUS, REFUSALS, "idl/ferrata_bridge.idl.json"))
def bridge_refusals_privacy(ctx):
    problems = []
    text = _text(ctx, REFUSALS)
    _expect(problems, BRIDGE_IMAGE in text, "Priv.5: the record no longer names B1's bridge image")
    codes = {e["name"]: e["code"] for e in _json(ctx, "idl/ferrata_bridge.idl.json")["errors"]}
    private = _table(text.split("## The private claim", 1)[1].split("## The plaintext claim", 1)[0], "claim")
    _expect(problems, len(private) == 4, f"Priv.5: {len(private)} private-path cases, not four")
    by_outcome = {}
    for case, outcome, blocks, changed in private:
        m = re.fullmatch(r"refused before any proof \(([A-Z_]+)\)", outcome)
        _expect(problems, m is not None or outcome == "applied", f"Priv.5: private outcome {outcome!r} unreadable")
        if m:
            by_outcome[case.split(":")[0]] = (m.group(1), blocks, changed)
    _expect(problems, set(by_outcome) == {"rejected", "repeated", "capped"},
            f"Priv.5: the three refusals changed: {sorted(by_outcome)}")
    # EVIDENCE 185: the three refusals are 249, 250 and 255, and each adds 0 blocks and 0 public accounts.
    for case, (name, blocks, changed) in by_outcome.items():
        _expect(problems, codes.get(name) == {"rejected": 249, "repeated": 250, "capped": 255}[case],
                f"Priv.5: {case} refuses with {name}, IDL code {codes.get(name)}")
        _expect(problems, blocks == "0" and changed == "0",
                f"Priv.5: {case} adds {blocks} blocks and changes {changed} public accounts")
    applied = [r for r in private if r[1] == "applied"]
    _expect(problems, len(applied) == 1 and applied[0][2] == "1" and "(a mint)" in applied[0][3],
            "Priv.5: the control claim no longer applies with one block and a mint")
    plaintext = _table(text.split("## The plaintext claim", 1)[1], "claim")
    _expect(problems, len(plaintext) == 4, f"Priv.5: {len(plaintext)} plaintext cases, not four")
    sizes = {r[2] for r in plaintext}
    _expect(problems, len(sizes) == 1 and next(iter(sizes)).isdigit(),
            f"Priv.5: the plaintext claims' transaction sizes are no longer one constant: {sorted(sizes)}")
    for case, exit_code, _size, gas_exec, fields in plaintext:
        kind = case.split(":")[0]
        _expect(problems, gas_exec.isdigit() and _int(gas_exec) > 0, f"Priv.5: {case}: unreadable gas {gas_exec!r}")
        if kind in by_outcome:  # the same three refusals on the plaintext path, with the IDL's code
            _expect(problems, exit_code == str(codes[by_outcome[kind][0]]),
                    f"Priv.5: {case}: plaintext exit {exit_code} != the IDL's {by_outcome[kind][0]}")
        _expect(problems, fields in ("the destination", "s, the destination"),
                f"Priv.5: {case}: unexpected deposit fields {fields!r}")
        _expect(problems, (kind != "rejected") == (fields == "s, the destination"),
                f"Priv.5: {case} publishes {fields!r}")
    _expect(problems, any(r[1] == "0 (applied)" and r[4] == "s, the destination" for r in plaintext),
            "Priv.5: the applied plaintext claim no longer publishes s and the destination")
    _phrases(problems, text, "bridge-refusals", "No refusal of the private path adds a block, a fee or any public state",
             "none names a deposit", "A plaintext claim carries its witness, so every plaintext claim, applied or refused, names its deposit",
             "A refusal names no more than the claim that applies")
    _phrases(problems, _row(ctx, "Priv.5"), "Priv.5", "A rejected, a repeated and a capped claim",
             "each is refused before any proof exists", "no block, no fee, no public account changed",
             "publishes no more than an applied plaintext claim", "the client's refusals name no deposit")
    fail_if(problems, "refusal-privacy record/status mismatch")
    return ["private path: rejected/repeated/capped refuse before any proof (249, 250, 255 by the IDL) with 0 blocks and 0 public accounts each; the control applies with one block",
            f"plaintext path: four cases, exit codes {[r[1] for r in plaintext]}, one constant transaction size "
            f"({plaintext[0][2]} bytes); the applied, repeated and capped claims publish s and the destination, the rejected one only the destination",
            "the record's two narratives (nothing added, nothing named; a refusal names no more than the applied claim) still stated",
            "LIMIT: the vault's half of Priv.5 (refused releases and deposits reverting with their four-byte selector) is "
            "RefusalPrivacyTest, a Foundry test with no committed per-case record; nor is the client's own refusal wording recorded here"]


MODULE_FLOWS = ("modules/results/complete-2026-11-12/bridge-e2e-native/bridge_e2e.md",
                "modules/results/final-2026-11-13/check/bridge-e2e-dev.md",
                "modules/results/final-2026-11-13/check/bridge-e2e-portable.md")
REAL_PROOF = "modules/results/complete-2026-11-12/real-proof"


@check(id="bridge-r12-module-flows", title="R.12: the core module's 24-check flows and its one real release proof",
       family="record", mode="offline", tier="quick", rows=("RFP-022 R.12",), evidence=(93, 103),
       what="each record's 24 per-check rows (native, and in each of the two Logos Cores), their groups and ok verdicts, and the release proven for real through Logos Core",
       against="modules/results/{complete-2026-11-12,final-2026-11-13}/ bridge-e2e records, real-proof/{reply.json,receipt-succinct.json,receipt-succinct.sha256}, the user guide's §13 and RFP-022 R.12",
       inputs=(STATUS, *MODULE_FLOWS, f"{REAL_PROOF}/reply.json", f"{REAL_PROOF}/receipt-succinct.json",
             f"{REAL_PROOF}/receipt-succinct.sha256", "modules/results/final-2026-11-13/README.md",
             "docs/guides/user-guide.md"))
def bridge_r12_module_flows(ctx):
    problems = []
    groups = {"devnet", "import_root", "registry", "claim (private)", "release (the wallet's rule)", "prove_release",
              "vault", "status"}
    for rel in MODULE_FLOWS:
        text = _text(ctx, rel)
        rows = _table(text, "group")
        _expect(problems, len(rows) == 24, f"R.12: {rel}: {len(rows)} check rows, not 24")
        _expect(problems, all(r[2] == "yes" for r in rows), f"R.12: {rel}: a check is no longer ok")
        _expect(problems, {r[0] for r in rows} == groups, f"R.12: {rel}: the flow's groups changed")
        _expect(problems, text.count("24 of 24 checks passed.") == 1, f"R.12: {rel}: the 24-of-24 verdict line changed")
    reply = _json(ctx, f"{REAL_PROOF}/reply.json")
    _expect(problems, reply["dev_mode"] is False and reply["verdict"] == "proven" and reply["image_id_matches"] is True,
            "R.12: the release proven through Logos Core is no longer real/matching")
    receipt = ctx.path(f"{REAL_PROOF}/receipt-succinct.json").read_bytes()
    digest = hashlib.sha256(receipt).hexdigest()
    sums = [ln.strip() for ln in _text(ctx, f"{REAL_PROOF}/receipt-succinct.sha256").splitlines() if ln.strip()]
    _expect(problems, len(sums) == 2 and sums[0] == digest and sums[1].isdigit() and int(sums[1]) == len(receipt),
            f"R.12: the receipt's committed digest/size differs: {sums} vs {digest}/{len(receipt)}")
    _expect(problems, reply["receipt_sha256"] == "0x" + digest and reply["receipt_bytes"] == len(receipt),
            "R.12: the module's reply no longer names the committed receipt")
    _phrases(problems, _text(ctx, "modules/results/final-2026-11-13/README.md"), "final record",
             "`bridge_e2e.sh` 24/24 in each Core", "e2e.sh` 43/43")
    guide = _text(ctx, "docs/guides/user-guide.md").split("## 13.", 1)[1]
    # The guide's method count is read from the guide, not retyped here: both of its statements of it, and they
    # must agree, so a count edit cannot leave this check's own words (or one of the two) stale.
    counts = [re.search(r"(\w+) methods a Logos ui module or a client calls", guide),
              re.search(r"The (\w+) methods natively", guide)]
    _expect(problems, all(counts), "R.12: the user guide's §13 no longer states the module's method count twice")
    if all(counts):
        _expect(problems, counts[0].group(1) == counts[1].group(1),
                f"R.12: the guide's method counts disagree: {counts[0].group(1)} and {counts[1].group(1)}")
    method_count = counts[0].group(1) if counts[0] else "an unstated number of"
    _phrases(problems, guide, "user guide §13", "`ferrata_bridge_module` is the bridge's core module",
             "logosctl call ferrata_bridge_module", "24 of 24 checks", "one release proven",
             "for real through Logos Core", "recovery and the registry", "**Messages** of the bridge program",
             "**Derivations**")
    _phrases(problems, _row(ctx, "R.12"), "R.12", "in journey order", "with `logosctl` commands",
             "the messages and derivations below it", "24 of 24", "one release proven for real",
             "the mini-app's round trip through it on a running sequencer and inside Basecamp")
    fail_if(problems, "module-flow record/status mismatch")
    return ["three bridge-e2e records (native and dev/portable in each Logos Core): 24 check rows each, every one ok, "
            "over the eight groups of both flows, recovery and status; each ends in its own 24-of-24 line",
            f"the release proven for real through Logos Core: reply dev_mode false, verdict {reply['verdict']!r}, "
            f"image id matching, and the receipt re-hashed ({digest[:16]}…, {len(receipt):,} B) equal to the committed "
            f"digest and the module's own reply",
            f"user guide §13 still states the {method_count} methods, the logosctl commands, 24 of 24 and the one real release proof",
            "LIMIT: the guide's wording and the Basecamp/mini-app runs the row also cites (EVIDENCE 125, 126) are not "
            "re-derived here; the real-proof record is the module's own reply and receipt, not a chain read"]


MINIAPP = "modules/ferrata_bridge_ui"


@check(id="bridge-u6-private-default", title="U.6: the private path preselected, every public choice acknowledged",
       family="record", mode="offline", tier="quick", rows=("RFP-022 U.6",), evidence=(62, 85),
       what="the mini-app record's per-test verdicts (private by default; the public destination, plaintext claim and public burn each refused until acknowledged) and the tool's documented defaults and acknowledgement flags",
       against=f"{MINIAPP}/RESULTS.md with {MINIAPP}/results/qmltest.txt, cli/README.md's defaults and refusal table, and RFP-022 U.6",
       inputs=(STATUS, f"{MINIAPP}/RESULTS.md", f"{MINIAPP}/results/qmltest.txt", "cli/README.md"))
def bridge_u6_private_default(ctx):
    problems = []
    qml = _text(ctx, f"{MINIAPP}/results/qmltest.txt")
    verdicts = {name: verdict for verdict, name in
                re.findall(r"^(PASS|FAIL!) *: qmltestrunner::FerrataMiniApp::(\w+)\(\)", qml, re.M)}
    _expect(problems, "Totals: 27 passed, 0 failed" in qml and not re.search(r"^FAIL!", qml, re.M),
            "U.6: the mini-app's 27-case verdict changed")
    # U.6's own cases: the private default at each step, and each public choice refused until acknowledged.
    for name in ("test_04_deposit_private_by_default", "test_05_public_needs_an_explicit_action",
                 "test_08_plaintext_claim", "test_10_private_burn_on_defaults", "test_11b_public_burn_and_checkpoint"):
        _expect(problems, verdicts.get(name) == "PASS", f"U.6: the mini-app's {name} is {verdicts.get(name)!r}")
    results = _text(ctx, f"{MINIAPP}/RESULTS.md")
    _phrases(problems, results, "mini-app RESULTS", "ran the view's 27 test functions", "**27 passed in 40.5 s**",
             "private destination, random delay and relayer pre-selected",
             "a public destination disables the deposit until acknowledged",
             "plaintext only after its acknowledgement", "a public burn is refused until acknowledged",
             "a public burn after its acknowledgement debits the holder")
    readme = _text(ctx, "cli/README.md")
    _phrases(problems, readme, "cli README", "The private choice is preselected everywhere; each weaker one is an\n"
             "option, and a public path also needs its own acknowledgement", "--i-accept-public-account",
             "--i-accept-plaintext-links-deposit", "a public path without its acknowledgement",
             "refusing a public destination without", "refusing the plaintext path", "refusing to burn from a public account")
    _phrases(problems, _row(ctx, "U.6"), "U.6", "Tool and mini-app preselect the private path",
             "each public choice needs its own acknowledgement")
    fail_if(problems, "U.6 record/status mismatch")
    return ["mini-app: 27 of 27 test functions pass; the five U.6 cases named (private deposit by default, public "
            "destination needing an explicit action, plaintext claim, private burn on defaults, public burn) each PASS",
            "tool: cli/README.md's defaults table still preselects the private path everywhere and names the three "
            "acknowledgement flags; its refusal table still names the three refusals without them",
            "LIMIT: the tool's and the mini-app's refusals are exercised by cli/tests/e2e.rs and box-test.sh (toolchain "
            "tests); the committed record here is the mini-app's qmltest.txt and RESULTS.md, and the tool's half of the "
            "row rests on its documented defaults"]
