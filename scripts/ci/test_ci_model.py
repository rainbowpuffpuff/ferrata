#!/usr/bin/env python3
"""The tests of scripts/ci/ci_model.py and scripts/ci/check-row-tests.py: that the checks see what CI runs,
and refuse a claim that it runs what it does not. Standard library only; run by CI's links job.

    python3 -I scripts/ci/test_ci_model.py
"""

import os
import pathlib
import re
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import ci_model  # noqa: E402

ROOT = ci_model.ROOT
npass = nfail = 0


def check(name, cond):
    global npass, nfail
    if cond:
        npass += 1
        print(f"ok   {name}")
    else:
        nfail += 1
        print(f"FAIL {name}")


# ── The YAML subset ────────────────────────────────────────────────────────────────────────────────
doc = ci_model.load_yaml("""\
name: x   # a comment
on:
  push:
    branches: [main, 'release/*']
  workflow_dispatch:
env:
  A: "1"
jobs:
  one:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4 # TODO: pin
      - name: a block
        run: |
          echo "a: b" # not a comment of the YAML
          cd sub && cargo test --test t
        working-directory: dir
      - run: >
          folded
          text
      - run: 'single ''quoted'' line'
""")
check("yaml: a trailing comment dropped from a plain scalar", doc["name"] == "x")
check("yaml: flow sequence with a quoted item", doc["on"]["push"]["branches"] == ["main", "release/*"])
check("yaml: an empty mapping value", doc["on"]["workflow_dispatch"] is None)
steps = doc["jobs"]["one"]["steps"]
check("yaml: comment after a uses value", steps[0]["uses"] == "actions/checkout@v4")
check("yaml: a literal block keeps its lines and its '#'",
      steps[1]["run"] == 'echo "a: b" # not a comment of the YAML\ncd sub && cargo test --test t\n')
check("yaml: a step's keys after the block", steps[1]["working-directory"] == "dir")
check("yaml: a folded block", steps[2]["run"] == "folded text\n")
check("yaml: a single-quoted scalar", steps[3]["run"] == "single 'quoted' line")

# ── The workflows as CI reads them ─────────────────────────────────────────────────────────────────
jobs = ci_model.jobs()
tracked = ci_model.tracked_paths()
check("jobs: ci.yml's test job runs on push and pull requests", jobs["test"].on_change)
check("jobs: sequencer.yml's job is manual only", not jobs["sequencer-attest"].on_change)


def runs(job, path):
    for cmd in ci_model.trace(jobs[job], tracked):
        if path.endswith(".rs"):
            if ci_model.cargo_runs(cmd, path)[0]:
                return True
        elif path.endswith(".sol"):
            if ci_model.forge_runs(cmd, path)[0]:
                return True
        elif ci_model.invokes(cmd, path, tracked):
            return True
    return False


check("trace: test runs crates/ferrata-spectests/tests/steps.rs (cargo test --workspace at the root)",
      runs("test", "crates/ferrata-spectests/tests/steps.rs"))
check("trace: test does not run lez/harness/tests/public_path.rs (lez/ is its own workspace)",
      not runs("test", "lez/harness/tests/public_path.rs"))
check("trace: roundtrip runs cli/tests/roundtrip_b.rs (through e2e/ci/run.sh, in cli/)",
      runs("roundtrip", "cli/tests/roundtrip_b.rs"))
check("trace: roundtrip runs cli/tests/e2e.rs (through cli/tests/e2e.sh, which sets FERRATA_E2E)",
      runs("roundtrip", "cli/tests/e2e.rs"))
check("trace: roundtrip does not run cli/src/privacy.rs's unit tests (only --test targets)",
      not runs("roundtrip", "cli/src/privacy.rs"))
check("trace: vault runs vault/test/Migration.t.sol", runs("vault", "vault/test/Migration.t.sol"))
check("trace: vault runs VaultInvariantsB.t.sol (its invariants inherited)",
      runs("vault", "vault/test/invariant/VaultInvariantsB.t.sol"))
privacy_suite = [p for p in tracked if p.startswith("vault/test/") and p.endswith(".t.sol")
                 and "contract PrivacySuite" in (ROOT / p).read_text(encoding="utf-8")]
check("trace: vault leaves PrivacySuite out (--no-match-contract)",
      bool(privacy_suite) and not runs("vault", privacy_suite[0]))
check("trace: lez-harness runs lez/harness/tests/public_path.rs (through scripts/ci/lez-harness.sh, in lez/)",
      runs("lez-harness", "lez/harness/tests/public_path.rs"))
check("trace: lez-harness runs lez/updater/tests/recorded.rs (cargo test -p ferrata-updater --features harness)",
      runs("lez-harness", "lez/updater/tests/recorded.rs"))
check("trace: lez-harness does not run lez/harness/tests/residue_experimental.rs (feature residue-experimental)",
      not runs("lez-harness", "lez/harness/tests/residue_experimental.rs"))
check("trace: monitor runs monitor/tests/check.rs and monitor/tests/anvil-e2e.sh",
      runs("monitor", "monitor/tests/check.rs") and runs("monitor", "monitor/tests/anvil-e2e.sh"))
check("trace: roundtrip runs cli/tests/gasdrop.rs but not cli/tests/relayer_fork.rs (it reads Sepolia)",
      runs("roundtrip", "cli/tests/gasdrop.rs") and not runs("roundtrip", "cli/tests/relayer_fork.rs"))
check("trace: witness-independence runs the bridge core's tests (tests/inbound.rs)",
      runs("witness-independence", "bridge/crates/ferrata-bridge-core/tests/inbound.rs"))
check("trace: links runs scripts/ci/check-unaudited.sh", runs("links", "scripts/ci/check-unaudited.sh"))
check("trace: links does not run scripts/ci/sequencer-attest.sh", not runs("links", "scripts/ci/sequencer-attest.sh"))

# ── cargo and forge selection on commands made up here ─────────────────────────────────────────────
C = ci_model.Cmd
e2e = "cli/tests/e2e.rs"
check("cargo: --no-run runs nothing",
      not ci_model.cargo_runs(C(["cargo", "test", "--test", "e2e", "--no-run"], "cli", {"FERRATA_E2E": "1"}, "t"), e2e)[0])
check("cargo: the test's own gate unset (FERRATA_E2E)",
      not ci_model.cargo_runs(C(["cargo", "test", "--test", "e2e"], "cli", {}, "t"), e2e)[0])
check("cargo: the gate set",
      ci_model.cargo_runs(C(["cargo", "test", "--test", "e2e"], "cli", {"FERRATA_E2E": "1"}, "t"), e2e)[0])
check("cargo: a name filter matching nothing in the file",
      not ci_model.cargo_runs(C(["cargo", "test", "--test", "e2e", "--", "zz_no_such_test"], "cli", {"FERRATA_E2E": "1"}, "t"), e2e)[0])
check("cargo: another package selected",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "ferrata-core"], ".", {}, "t"), "crates/ferrata-spectests/tests/steps.rs")[0])
check("cargo: --lib leaves integration tests out",
      not ci_model.cargo_runs(C(["cargo", "test", "--workspace", "--lib"], ".", {}, "t"), "crates/ferrata-spectests/tests/steps.rs")[0])
check("cargo: an unknown directory proves nothing",
      not ci_model.cargo_runs(C(["cargo", "test"], None, {}, "t"), "crates/ferrata-spectests/tests/steps.rs")[0])
check("forge: --match-path leaves other files out",
      not ci_model.forge_runs(C(["forge", "test", "--match-path", "test/Migration*.t.sol"], "vault", {}, "t"), "vault/test/Refund.t.sol")[0])
check("forge: --match-path keeps its own",
      ci_model.forge_runs(C(["forge", "test", "--match-path", "test/Migration*.t.sol"], "vault", {}, "t"), "vault/test/MigrationFailsafe.t.sol")[0])
check("forge: --match-test matching no function of the file",
      not ci_model.forge_runs(C(["forge", "test", "--match-test", "zz_no_such_test"], "vault", {}, "t"), "vault/test/Refund.t.sol")[0])

check("cargo: a name filter matching the module path of a file under src/ (cargo test --lib relayer)",
      ci_model.cargo_runs(C(["cargo", "test", "--lib", "relayer"], "cli", {}, "t"), "cli/src/relayer.rs")[0])
check("cargo: a filter naming the module path, an inner module and a function",
      ci_model.cargo_runs(C(["cargo", "test", "--lib", "relayer::tests::temporary_failures"], "cli", {}, "t"),
                          "cli/src/relayer.rs")[0])
check("cargo: a filter matching another module's path does not select this file",
      not ci_model.cargo_runs(C(["cargo", "test", "--lib", "privacy"], "cli", {}, "t"), "cli/src/relayer.rs")[0])
check("trace: roundtrip runs cli/src/relayer.rs's tests (cargo test --lib relayer in cli/)",
      runs("roundtrip", "cli/src/relayer.rs"))
check("cargo: a file compiled only with a feature the command leaves off (lez/updater/tests/recorded.rs)",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "ferrata-updater"], "lez", {}, "t"),
                              "lez/updater/tests/recorded.rs")[0])
check("cargo: the same file with the feature on",
      ci_model.cargo_runs(C(["cargo", "test", "-p", "ferrata-updater", "--features", "harness"], "lez", {}, "t"),
                          "lez/updater/tests/recorded.rs")[0])
FG = ci_model.feature_gates
check("features: #![cfg(feature)] gates the whole file", FG('#![cfg(feature = "a")]\n#[test]\nfn x() {}\n', set()) == ("a", set(), 0))
check("features: ... unless the feature is on", FG('#![cfg(feature = "a")]\n#[test]\nfn x() {}\n', {"a"}) == (None, set(), 1))
check("features: #[cfg(feature)] on one test of two",
      FG('#[cfg(feature = "b")]\n#[test]\nfn x() {}\n#[test]\nfn y() {}\n', set()) == (None, {"b"}, 1))
check("features: a gated module's tests, braces counted",
      FG('#[cfg(feature = "c")]\nmod t {\n    #[test]\n    fn x() { if true {} }\n}\n', set()) == (None, {"c"}, 0))
check("features: a test after a gated module is not gated",
      FG('#[cfg(feature = "c")]\nmod t {\n    #[test]\n    fn x() {}\n}\n#[test]\nfn y() {}\n', set()) == (None, {"c"}, 1))

# ── check-row-tests.py refuses false claims ────────────────────────────────────────────────────────
checker = HERE / "check-row-tests.py"
real = (HERE / "row-tests.toml").read_text(encoding="utf-8")


def run_checker(text, *args, traceability=None):
    with tempfile.TemporaryDirectory() as d:
        path = pathlib.Path(d) / "row-tests.toml"
        path.write_text(text, encoding="utf-8")
        extra = []
        if traceability is not None:
            (pathlib.Path(d) / "traceability.md").write_text(traceability, encoding="utf-8")
            extra = ["--traceability", str(pathlib.Path(d) / "traceability.md")]
        p = subprocess.run([sys.executable, "-I", str(checker), "--map", str(path), *extra, *args],
                           capture_output=True, text=True)
        return p.returncode, p.stdout + p.stderr


rc, out = run_checker(real)
check("checker: the real map is consistent (exit 0)", rc == 0)
claim = '{ path = "vault/test/Refund.t.sol", kind = "test", ci = "vault" }'
if claim in real:
    rc, out = run_checker(real.replace(claim, '{ path = "vault/test/Refund.t.sol", kind = "test", ci = "test" }', 1))
    check("checker: a Foundry test claimed for the Rust job is refused", rc == 1 and "job test does not run it" in out)
    rc, out = run_checker(real.replace(claim, '{ path = "vault/test/Refund.t.sol", kind = "record" }', 1))
    check("checker: a test file listed as a record is refused", rc == 1 and "a test file listed as a record" in out)
else:
    check("checker: the map still names vault/test/Refund.t.sol for the vault job", False)
rc, out = run_checker(real.replace('kind = "document"', 'kind = "doc"', 1))
check("checker: an unknown kind is refused", rc == 1 and "is none of test, record, document" in out)

# ── R.11's numbers and gate: what counts as a test CI runs ──────────────────────────────────────────
# The real map's counts, as docs/rows/coverage.md states them: update these lines with the map (a row that gains a
# test CI runs moves both numbers).
rc, out = run_checker(real, "--list")
check("checker: the real map, RFP-022: 60 of 68 hard rows with a test CI runs",
      "RFP-022: 68 hard rows; 60 with a test CI runs; 8 without one" in out)
check("checker: the real map, RFP-021: 49 of 59 hard rows with a test CI runs",
      "RFP-021: 59 hard rows; 49 with a test CI runs; 10 without one" in out)
listed = re.findall(r"^    (RFP-02[12] (?:VS|Sec|Priv|F|U|R|P|S)\.\d+): (\S+)$", out, re.M)
rows022 = [rid for rid, _ in listed if rid.startswith("RFP-022 ")]
check("checker: --list gives every hard row its status", len(listed) == 59 + 68 and len(rows022) == 68)
rc, out = run_checker(real, "--require-ci", "RFP-022")
check("checker: --require-ci RFP-022 exits 1 on the real map and counts the rows without a test CI runs",
      rc == 1 and "--require-ci RFP-022: 8 hard row(s) without a test CI runs" in out)

# Synthetic maps: the real one with a test the vault job runs added to some RFP-022 rows (a row may have
# several [[row]] blocks; the checker joins them).
IN_CI = '{ path = "vault/test/Refund.t.sol", kind = "test", ci = "vault" }'
IN_MANUAL_JOB = '{ path = "scripts/ci/sequencer-attest.sh", kind = "test", ci = "sequencer-attest" }'


def with_tests(text, ids, test=IN_CI):
    return text + "".join(f'\n[[row]]\nid = "{rid}"\ntests = [ {test} ]\n' for rid in ids)


rc, out = run_checker(with_tests(real, rows022), "--require-ci", "RFP-022")
check("checker: --require-ci RFP-022 exits 0 once every RFP-022 row has a test CI runs",
      rc == 0 and "RFP-022: 68 hard rows; 68 with a test CI runs; 0 without one" in out
      and "--require-ci RFP-022: every hard row has a test CI runs" in out)
# S.1 has a test on the build box only in the real map (lez/testnet04/day1.sh sends to a public LEZ testnet)
others = [rid for rid in rows022 if rid != "RFP-022 S.1"]
rc, out = run_checker(with_tests(real, others), "--require-ci", "RFP-022")
check("checker: a row whose tests run on the box only is not counted as CI, and the gate names it",
      rc == 1 and "RFP-022: 68 hard rows; 67 with a test CI runs; 1 without one" in out
      and "tests run on the build box or by hand only (1): S.1" in out
      and "--require-ci RFP-022: 1 hard row(s) without a test CI runs" in out)
rc, out = run_checker(with_tests(with_tests(real, others), ["RFP-022 S.1"], IN_MANUAL_JOB), "--require-ci", "RFP-022")
check("checker: a test in a job started by hand only is counted as manual, not as CI",
      rc == 1 and "RFP-022: 68 hard rows; 67 with a test CI runs; 1 without one" in out
      and "a test in a workflow started by hand only (1): S.1" in out)
# The matrices must hold 59 and 68 hard rows: one RFP-022 row taken out of a copy of traceability.md.
lines, rfp, dropped = [], None, False
for line in (ROOT / "docs" / "traceability.md").read_text(encoding="utf-8").splitlines(keepends=True):
    if line.startswith("## "):
        rfp = line[3:10]
    if rfp == "RFP-022" and not dropped and re.match(r"^\|\s*F\.4\s*\|", line):
        dropped = True
        continue
    lines.append(line)
rc, out = run_checker(real, traceability="".join(lines))
check("checker: a matrix holding 67 hard rows for RFP-022 is refused",
      dropped and rc == 1 and "traceability.md holds 67 hard rows for RFP-022, expected 68" in out)

print()
print(f"test_ci_model.py: {npass} passed, {nfail} failed")
sys.exit(1 if nfail else 0)
