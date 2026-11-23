#!/usr/bin/env python3
"""Every count names/README.md and the names row of docs/EVIDENCE.md cite, recomputed from the committed records
(and, with --run, from the checks themselves). Standard library only.

    python3 -I names/e2e/claims.py          # from the committed files
    python3 -I names/e2e/claims.py --run    # also run the IDL check and the JS tests here (Node 22.7+ / 20.19+)

The exit codes: the program's (lez/names-wire/src/lib.rs `pub mod code`) against those the harness tables record as
reached (`refused: exit N` in lez/results/names-*.md, the rc1 run; names/e2e/results/harness-rc3/, the rc3 run)."""
import pathlib, re, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
RC1 = sorted((ROOT / "lez/results").glob("names-*.md"))
RC3_DIR = ROOT / "names/e2e/results/harness-rc3"
E2E = ROOT / "names/e2e/results/local-rc3-v2"

src = (ROOT / "lez/names-wire/src/lib.rs").read_text()
codes = {int(v): n for n, v in re.findall(r"\n\s+([A-Z_]+) = (\d+);", src[src.index("pub mod code {"):])}

def reached(files):
    return {int(c) for f in files for c in re.findall(r"refused: exit (\d+)", f.read_text())}

def table_rows(f):  # a results table without its title and preamble
    return [l for l in f.read_text().splitlines() if l.startswith("|")]

out = []
def say(k, v):
    out.append((k, v))
    print(f"{k}: {v}")

r1, r3 = reached(RC1), reached(sorted(RC3_DIR.glob("names-*.md")))
say("program exit codes", len(codes))
say("reached on rc1 (harness tables)", f"{len(r1 & set(codes))} of {len(codes)}; not reached: "
    + (", ".join(f"{c} {codes[c]}" for c in sorted(set(codes) - r1)) or "none"))
say("reached on rc3 (harness tables)", f"{len(r3 & set(codes))} of {len(codes)}")
same = all(table_rows(f) == table_rows(RC3_DIR / f.name) for f in RC1 if (RC3_DIR / f.name).exists())
say("rc1 and rc3 tables identical", f"{same} ({len(RC1)} tables)")
for tag, d in [("rc1", ROOT / "names/e2e/results/harness-rc1"), ("rc3", RC3_DIR)]:
    summary = (d / "summary.txt").read_text() if (d / "summary.txt").exists() else ""
    passed = [int(n) for n in re.findall(r"test result: \w+\. (\d+) passed", summary)]
    failed = [int(n) for n in re.findall(r"test result: \w+\. \d+ passed; (\d+) failed", summary)]
    say(f"{tag} harness tests (names, names_review)", f"{' + '.join(map(str, passed))} passed, {sum(failed)} failed")
before = ROOT / "names/e2e/results/review-before"
if before.exists():
    h = (before / "harness.log").read_text()
    m = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", h)
    say("before the fixes, the review's harness tests (v1)", f"{m.group(1)} passed, {m.group(2)} failed")
    j = (before / "js.log").read_text()
    say("before the fixes, the review's JS tests", f"{re.search(r'# pass (\d+)', j).group(1)} passed, {re.search(r'# fail (\d+)', j).group(1)} failed")
# The worst case of each action (lez/results/names-worst.md) against the clients' limits (names/js/names.js GAS).
js = (ROOT / "names/js/names.js").read_text()
gas_block = js[js.index("export const GAS = Object.freeze({"):]
gas_block = gas_block[: gas_block.index("});")]
GAS = {k: int(v.replace("_", "")) for k, v in re.findall(r"\n\s+(\w+): ([\d_]+),", gas_block)}
def gas_for(action, n):
    return GAS["ForgetBase"] + GAS["ForgetPerName"] * n if action == "Forget" else GAS[action]
worst_rows = []
for row in table_rows(ROOT / "lez/results/names-worst.md"):
    c = [x.strip() for x in row.strip("|").split("|")]
    if c[0] in ("action", "---") or not c[1].isdigit():
        continue
    worst_rows.append((c[0], int(c[1]), int(c[4].replace(",", "")), int(c[5].replace(",", ""))))
top = max(worst_rows, key=lambda r: r[2])
say("worst cases measured (actions; Forget of 1 to 5 names)", len(worst_rows))
say("the costliest action's worst case", f"{top[0]} of {top[1]} names: {top[2]:,} cycles")
say("every worst case fits the clients' limit (names.js GAS)", all(cyc <= gas_for(a, n) and lim == gas_for(a, n) for a, n, cyc, lim in worst_rows))
n = lambda x: int(x.replace(",", ""))
G, S, F = [], [], []
for f in RC1:
    for row in table_rows(f):
        if "refused: exit" not in row:
            continue
        c = [x.strip() for x in row.strip("|").split("|")]
        if f.name == "names-review.md":
            G.append(n(c[1])); F.append(n(c[2]))
        else:
            G.append(n(c[3])); S.append(n(c[5])); F.append(n(c[6]))
say("refusals (coded) in the tables", f"{len(G)}: execution gas {min(G):,} – {max(G):,}, storage gas {min(S):,} – {max(S):,}, base fee {min(F):,} – {max(F):,}")
if E2E.exists():
    say("rc3 deployment transactions (3 segments + header)", (E2E / "deploy.txt").read_text().count("Transaction hash"))
    checks = (E2E / "checks.txt").read_text().splitlines()
    say("rc3 sequencer e2e checks", f"{sum(l.startswith('ok ') for l in checks)} ok, {sum(l.startswith('FAIL') for l in checks)} failed")
    fees = re.findall(r"^\| (.+?) \| (\d+) \|$", (E2E / "fees.md").read_text(), re.M)
    for label, fee in fees:
        say(f"rc3 fee: {label}", f"{int(fee):,}")
if "--run" in sys.argv:
    idl = subprocess.run([sys.executable, "-I", str(ROOT / "idl/tools/check_names_idl.py")], capture_output=True, text=True)
    say("IDL checks", f"{idl.stdout.count(chr(10) + 'ok ') + idl.stdout.startswith('ok ')} ok, {idl.stdout.count('FAIL')} failed")
    for t in ["names.test.mjs", "review.test.mjs"]:
        js = subprocess.run(["node", "--test", str(ROOT / "names/js/test" / t)], capture_output=True, text=True)
        p = re.search(r"# pass (\d+)", js.stdout); fl = re.search(r"# fail (\d+)", js.stdout)
        say(f"JS {t}", f"{p.group(1) if p else '?'} passed, {fl.group(1) if fl else '?'} failed")
