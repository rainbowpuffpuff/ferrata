#!/usr/bin/env python3
"""Compare check.sh's replies and write REPORT/summary.md.

    python3 -I modules/scripts/summary.py REPORT_DIR STEP_FAILURES

For each call: the native reply must meet the request's expectations (requests.py,
bridge_requests.py, daemon_requests.py), and the reply through Logos Core (dev, and portable when it
ran) must equal the native reply. Then the end-to-end runs: e2e.sh's checks per flavour,
bridge_e2e.sh's per flavour (required when BRIDGE_E2E is set and not 0), and the daemons' runs (attestor-e2e.sh, updater-e2e.sh, monitor/tests/anvil-e2e.sh: one PASS/FAIL line per
check in their summary.md), natively and per flavour. MODULES (the environment) names the modules
that were built (default all). Exits 1 if any check fails.
"""

import json
import os
import pathlib
import sys

ALL = "ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_updater_module"
MODULES = set((os.environ.get("MODULES") or ALL).split())
DAEMONS = {"attestor": "ferrata_attestor_module", "updater": "ferrata_updater_module", "monitor": "ferrata_monitor_module"}


def load(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def get(reply, dotted):
    value = reply
    for key in dotted.split("."):
        if isinstance(value, list) and key.isdigit() and int(key) < len(value):
            value = value[int(key)]
        elif isinstance(value, dict) and key in value:
            value = value[key]
        else:
            return "<missing>"
    return value


def main():
    rep = pathlib.Path(sys.argv[1])
    step_failures = int(sys.argv[2])
    calls = [{"name": "version", "method": "version"}] + (load(rep / "requests/calls.json") or [])
    calls = [c for c in calls if c.get("module", "ferrata_attest_module") in MODULES]
    expect = load(rep / "requests/expect.json") or {}
    flavours = [f for f in ("dev", "portable") if (rep / "replies" / f).is_dir()]
    timings = {}
    for f in flavours:
        rows = (rep / "replies" / f / "timings.tsv").read_text().splitlines()[1:] if (rep / "replies" / f / "timings.tsv").exists() else []
        timings[f] = {r.split("\t")[0]: r.split("\t")[1:] for r in rows}

    failures = 0
    lines = ["# check.sh summary", "", "## Steps", "", (rep / "steps.md").read_text(), "## Calls", ""]
    head = "| call | module | method | native verdict | code | expectations |"
    rule = "|---|---|---|---|---:|---|"
    for f in flavours:
        head += f" {f}: equal to native | {f}: seconds |"
        rule += "---|---:|"
    lines += [head, rule]
    for call in calls:
        name = call["name"]
        native = load(rep / "replies/native" / f"{name}.json")
        if native is None:
            failures += 1
            lines.append(f"| {name} | {call.get('module', 'ferrata_attest_module')} | {call['method']} | no native reply | | |")
            continue
        missed = [f"{k}={get(native, k)!r}" for k, v in expect.get(name, {}).items() if get(native, k) != v]
        failures += bool(missed)
        verdict = native.get("verdict", native.get("module", ""))
        code = native.get("code", get(native, "bootstrap.code") if native.get("bootstrap") else "")
        row = f"| {name} | {call.get('module', 'ferrata_attest_module')} | {call['method']} | {verdict} | {code} | {'met' if not missed else 'MISSED: ' + ', '.join(missed)} |"
        for f in flavours:
            reply = load(rep / "replies" / f / f"{name}.json")
            same = reply == native
            failures += not same
            seconds, rc = (timings[f].get(name) or ["", ""])[:2]
            row += f" {'yes' if same else 'NO (exit ' + rc + ')'} | {seconds} |"
        lines.append(row)

    # e2e.sh through Logos Core (check.sh runs it in each flavour's daemon).
    for f in flavours if {"ferrata_attest_module", "ferrata_bridge_module"} <= MODULES and os.environ.get("E2E", "1") != "0" else []:
        e2e = load(rep / f"e2e-{f}" / "e2e.json")
        if e2e is None:
            lines += ["", f"## End to end through Logos Core ({f})", "", "not run or no result", ""]
            failures += 1
            continue
        failures += e2e["failed"]
        lines += ["", f"## End to end through Logos Core ({f}): {e2e['checks']} checks, {e2e['failed']} failed", ""]
        lines += ["| group | check | passed | detail |", "|---|---|---|---|"]
        lines += [f"| {r['group']} | {r['check']} | {r['passed']} | {r['detail']} |" for r in e2e["rows"]]

    # bridge_e2e.sh through Logos Core (check.sh with BRIDGE_E2E=1): required only then.
    for f in flavours if "ferrata_bridge_module" in MODULES else []:
        b = load(rep / f"bridge-e2e-{f}" / "bridge_e2e.json")
        if b is None:
            if os.environ.get("BRIDGE_E2E", "0") != "0":
                lines += ["", f"## Bridge module end to end through Logos Core ({f})", "", "not run or no result", ""]
                failures += 1
            continue
        failures += b["failed"]
        lines += ["", f"## Bridge module end to end through Logos Core ({f}, the mini-app devnet): {b['checks']} checks, "
                      f"{b['failed']} failed", ""]
        lines += ["| group | check | passed | detail |", "|---|---|---|---|"]
        lines += [f"| {r['group']} | {r['check']} | {r['passed']} | {r['detail']} |" for r in b["rows"]]

    # The daemons' jobs: natively, then through each flavour's Logos Core (when check.sh ran them there).
    ran_in = ["native"] + [f for f in flavours if f in (os.environ.get("DAEMONS_E2E_IN") or "dev portable").split()]
    if os.environ.get("E2E", "1") != "0":
        for f in ran_in:
            for name, module in DAEMONS.items():
                if module not in MODULES:
                    continue
                path = rep / f"daemons-{f}" / name / "summary.md"
                try:
                    text = path.read_text().splitlines()
                except OSError:
                    failures += 1
                    lines += ["", f"## {name} ({f}): not run or no summary", ""]
                    continue
                passed = [l for l in text if l.startswith("- PASS ")]
                failed = [l for l in text if l.startswith("- FAIL ")]
                failures += len(failed) + (0 if passed or failed else 1)
                lines += ["", f"## {name} ({f}): {len(passed)} passed, {len(failed)} failed", ""] + passed + failed

    lines += ["", "## Replies (native)", ""]
    for call in calls:
        native = load(rep / "replies/native" / f"{call['name']}.json")
        if call["name"] == "version" or (native and native.get("verdict") != "invalid_request"):
            text = json.dumps(native)
            lines.append(f"- `{call['name']}`: `{text if len(text) < 600 else text[:600] + ' …'}`")
    lines += ["", f"Step failures: {step_failures}. Check failures: {failures}.", ""]
    (rep / "summary.md").write_text("\n".join(lines))
    print("\n".join(lines[:4 + len(calls) + 12]))
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
