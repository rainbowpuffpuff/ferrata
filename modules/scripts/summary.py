#!/usr/bin/env python3
"""Compare check.sh's replies and write REPORT/summary.md.

    python3 -I modules/scripts/summary.py REPORT_DIR STEP_FAILURES

For each call: the native reply must meet the request's expectations (requests.py), and the reply
through Logos Core (dev, and portable when it ran) must equal the native reply. Exits 1 if any
check fails.
"""

import json
import pathlib
import sys


def load(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def get(reply, dotted):
    value = reply
    for key in dotted.split("."):
        if not isinstance(value, dict) or key not in value:
            return "<missing>"
        value = value[key]
    return value


def main():
    rep = pathlib.Path(sys.argv[1])
    step_failures = int(sys.argv[2])
    calls = [{"name": "version", "method": "version"}] + (load(rep / "requests/calls.json") or [])
    expect = load(rep / "requests/expect.json") or {}
    flavours = [f for f in ("dev", "portable") if (rep / "replies" / f).is_dir()]
    timings = {}
    for f in flavours:
        rows = (rep / "replies" / f / "timings.tsv").read_text().splitlines()[1:] if (rep / "replies" / f / "timings.tsv").exists() else []
        timings[f] = {r.split("\t")[0]: r.split("\t")[1:] for r in rows}

    failures = 0
    lines = ["# check.sh summary", "", "## Steps", "", (rep / "steps.md").read_text(), "## Calls", ""]
    head = "| call | method | native verdict | code | expectations |"
    rule = "|---|---|---|---:|---|"
    for f in flavours:
        head += f" {f}: equal to native | {f}: seconds |"
        rule += "---|---:|"
    lines += [head, rule]
    for call in calls:
        name = call["name"]
        native = load(rep / "replies/native" / f"{name}.json")
        if native is None:
            failures += 1
            lines.append(f"| {name} | {call['method']} | no native reply | | |")
            continue
        missed = [f"{k}={get(native, k)!r}" for k, v in expect.get(name, {}).items() if get(native, k) != v]
        failures += bool(missed)
        verdict = native.get("verdict", native.get("module", ""))
        code = native.get("code", get(native, "bootstrap.code") if native.get("bootstrap") else "")
        row = f"| {name} | {call['method']} | {verdict} | {code} | {'met' if not missed else 'MISSED: ' + ', '.join(missed)} |"
        for f in flavours:
            reply = load(rep / "replies" / f / f"{name}.json")
            same = reply == native
            failures += not same
            seconds, rc = (timings[f].get(name) or ["", ""])[:2]
            row += f" {'yes' if same else 'NO (exit ' + rc + ')'} | {seconds} |"
        lines.append(row)

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
