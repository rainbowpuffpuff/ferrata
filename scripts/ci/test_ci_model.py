#!/usr/bin/env python3
"""The tests of scripts/ci/ci_model.py and scripts/ci/check-row-tests.py: that the checks see what CI runs,
and refuse a claim that it runs what it does not. Standard library only; run by CI's links job.

    python3 -I scripts/ci/test_ci_model.py
"""

import os
import pathlib
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

# ── check-row-tests.py refuses false claims ────────────────────────────────────────────────────────
checker = HERE / "check-row-tests.py"
real = (HERE / "row-tests.toml").read_text(encoding="utf-8")


def run_checker(text, *args):
    with tempfile.TemporaryDirectory() as d:
        path = pathlib.Path(d) / "row-tests.toml"
        path.write_text(text, encoding="utf-8")
        p = subprocess.run([sys.executable, "-I", str(checker), "--map", str(path), *args],
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
rc, out = run_checker(real, "--require-ci", "RFP-022")
check("checker: --require-ci exits 1 while a row lacks a CI test, and says how many",
      (rc == 1 and "without a test CI runs" in out) or (rc == 0 and "every hard row has a test CI runs" in out))

print()
print(f"test_ci_model.py: {npass} passed, {nfail} failed")
sys.exit(1 if nfail else 0)
