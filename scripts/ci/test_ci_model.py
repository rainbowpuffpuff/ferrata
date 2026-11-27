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
check("jobs: sequencer.yml's two jobs run on pushes to main (on_change)",
      jobs["sequencer-attest"].on_change and jobs["sequencer-roundtrip"].on_change)


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
check("trace: test runs crates/ferrata-spectests/tests/helios_step_oracle.rs (fast step-path oracle subset)",
      runs("test", "crates/ferrata-spectests/tests/helios_step_oracle.rs"))
check("trace: test does not run lez/harness/tests/public_path.rs (lez/ is its own workspace)",
      not runs("test", "lez/harness/tests/public_path.rs"))
check("trace: roundtrip runs cli/tests/roundtrip_b.rs (through e2e/ci/run.sh, in cli/)",
      runs("roundtrip", "cli/tests/roundtrip_b.rs"))
check("trace: roundtrip runs cli/tests/e2e.rs (through cli/tests/e2e.sh, which sets FERRATA_E2E)",
      runs("roundtrip", "cli/tests/e2e.rs"))
check("trace: roundtrip runs cli/src/privacy.rs's unit tests (the unfiltered --lib suite)",
      runs("roundtrip", "cli/src/privacy.rs"))
check("trace: vault runs vault/test/Migration.t.sol", runs("vault", "vault/test/Migration.t.sol"))
check("trace: vault runs VaultInvariantsB.t.sol (its invariants inherited)",
      runs("vault", "vault/test/invariant/VaultInvariantsB.t.sol"))
privacy_suite = [p for p in tracked if p.startswith("vault/test/") and p.endswith(".t.sol")
                 and "contract PrivacySuite" in (ROOT / p).read_text(encoding="utf-8")]
check("trace: vault leaves PrivacySuite out (--no-match-contract)",
      bool(privacy_suite) and not runs("vault", privacy_suite[0]))
check("trace: lez-harness runs lez/harness/tests/public_path.rs (through scripts/ci/lez-harness.sh --shard, in lez/)",
      runs("lez-harness", "lez/harness/tests/public_path.rs"))
check("trace: lez-harness runs lez/updater/tests/recorded.rs (updater-gate shard)",
      runs("lez-harness", "lez/updater/tests/recorded.rs"))
check("trace: lez-harness runs lez/updater/tests/refusals.rs (updater-privacy shard)",
      runs("lez-harness", "lez/updater/tests/refusals.rs"))
check("trace: lez-harness runs vault/test/PrivacySuite.t.sol (updater-privacy shard via scripts/privacy/run.sh)",
      runs("lez-harness", "vault/test/PrivacySuite.t.sol"))
check("trace: lez-harness does not run lez/harness/tests/residue_experimental.rs (feature residue-experimental)",
      not runs("lez-harness", "lez/harness/tests/residue_experimental.rs"))
check("jobs: lez-full.yml's lez-harness-full is scheduled/manual only (not on_change CI)",
      jobs["lez-harness-full"].workflow.name == "lez-full.yml"
      and not jobs["lez-harness-full"].on_change
      and runs("lez-harness-full", "lez/harness/tests/public_path.rs"))
check("trace: monitor runs monitor/tests/check.rs and monitor/tests/anvil-e2e.sh",
      runs("monitor", "monitor/tests/check.rs") and runs("monitor", "monitor/tests/anvil-e2e.sh"))
check("trace: roundtrip runs cli/tests/gasdrop.rs but not cli/tests/relayer_fork.rs (it reads Sepolia)",
      runs("roundtrip", "cli/tests/gasdrop.rs") and not runs("roundtrip", "cli/tests/relayer_fork.rs"))
check("trace: witness-independence runs the bridge core's tests (tests/inbound.rs)",
      runs("witness-independence", "bridge/crates/ferrata-bridge-core/tests/inbound.rs"))
check("trace: links runs scripts/ci/check-unaudited.sh", runs("links", "scripts/ci/check-unaudited.sh"))
check("trace: links does not run scripts/ci/sequencer-attest.sh", not runs("links", "scripts/ci/sequencer-attest.sh"))
check("trace: sequencer-attest runs scripts/ci/sequencer-attest.sh; sequencer-roundtrip runs scripts/ci/sequencer-roundtrip.sh",
      runs("sequencer-attest", "scripts/ci/sequencer-attest.sh")
      and runs("sequencer-roundtrip", "scripts/ci/sequencer-roundtrip.sh"))

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
# M1: bin targets and doc-tests are distinguished from --lib
BIN_TEST = "#[test]\nfn bin_unit_test_added() { assert!(false); }\n"
DOC_TEST = "/// ```\n/// assert!(false);\n/// ```\npub fn doc_added() {}\n"
TEXT_DOC = "//! ```text\n//! not rust\n//! ```\npub fn plain() {}\n"
check("cargo: --lib does not run #[test] in src/main.rs (ferrata-updater)",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--lib"], "lez", {}, "t"),
                              "lez/updater/src/main.rs", text=BIN_TEST)[0])
check("cargo: --bins and --bin ferrata-updater do run #[test] in src/main.rs",
      ci_model.cargo_runs(C(["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--bins"], "lez", {}, "t"),
                          "lez/updater/src/main.rs", text=BIN_TEST)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--bin", "ferrata-updater"], "lez", {}, "t"),
                              "lez/updater/src/main.rs", text=BIN_TEST)[0])
check("cargo: --lib does not run #[test] in src/bin/tables.rs; --bins does",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--lib"], "lez", {}, "t"),
                              "lez/harness/src/bin/tables.rs", text=BIN_TEST)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--bins"], "lez", {}, "t"),
                              "lez/harness/src/bin/tables.rs", text=BIN_TEST)[0])
check("cargo: --lib does not run doc-tests in src/lib.rs; --doc and unsharded cargo test do",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--lib"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=DOC_TEST)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=DOC_TEST)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=DOC_TEST)[0])
check("cargo: --doc ignores non-Rust ```text blocks and does not run #[test]-only files",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=TEXT_DOC)[0]
      and not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                                  "lez/harness/src/lib.rs", text=BIN_TEST)[0])
check("cargo: --doc honours name filters and --skip",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc", "--", "zz_no_such_doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=DOC_TEST)[0]
      and not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc", "--", "--skip", "doc_added"], "lez", {}, "t"),
                                  "lez/harness/src/lib.rs", text=DOC_TEST)[0])
# N3: ~~~ fences, /** */ / /*! */ block docs, and literal #[doc = "..."] attributes
TILDE_DOC = "/// ~~~rust\n/// assert!(false);\n/// ~~~\npub fn tilde() {}\n"
TILDE_TEXT = "/// ~~~text\n/// not rust\n/// ~~~\npub fn plain_tilde() {}\n"
BLOCK_OUTER_DOC = "/**\n * ```\n * assert!(false);\n * ```\n */\npub fn block_doc() {}\n"
BLOCK_INNER_DOC = "/*!\n * ~~~\n * assert!(false);\n * ~~~\n */\n"
BLOCK_TEXT_DOC = "/**\n * ```text\n * not rust\n * ```\n */\npub fn block_text() {}\n"
ATTR_DOC = '#[doc = "```\\nassert!(false);\\n```"]\npub fn attr_doc() {}\n'
ATTR_SPLIT_DOC = '#[doc = "```"]\n#[doc = "assert!(false);"]\n#[doc = "```"]\npub fn attr_split() {}\n'
ATTR_RAW_DOC = '#![doc = r#"\n~~~rust\nassert!(false);\n~~~\n"#]\n'
NON_DOC_FORMS = (
    "//// ```\n//// assert!(false);\n//// ```\n"
    "/* ```\nassert!(false);\n``` */\n"
    "/*** ```\nassert!(false);\n``` */\n"
    'const S: &str = "/// ```\\n/// assert!(false);\\n/// ```";\n'
    'const R: &str = r#"/**\n```\nassert!(false);\n```\n*/"#;\n'
)
check("cargo: --doc detects ~~~ fences and /** */ / /*! */ block doc comments (and ignores non-Rust text fences)",
      ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                          "lez/harness/src/lib.rs", text=TILDE_DOC)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=BLOCK_OUTER_DOC)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=BLOCK_INNER_DOC)[0]
      and not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                                  "lez/harness/src/lib.rs", text=TILDE_TEXT)[0]
      and not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                                  "lez/harness/src/lib.rs", text=BLOCK_TEXT_DOC)[0])
check("cargo: --doc detects literal #[doc = \"...\"] (single, split, and raw-string) doc-tests",
      ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                          "lez/harness/src/lib.rs", text=ATTR_DOC)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=ATTR_SPLIT_DOC)[0]
      and ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=ATTR_RAW_DOC)[0])
check("cargo: --doc ignores non-doc comments (////, /* */, /*** */) and doc-like text inside string literals",
      not ci_model.cargo_runs(C(["cargo", "test", "-p", "harness", "--doc"], "lez", {}, "t"),
                              "lez/harness/src/lib.rs", text=NON_DOC_FORMS)[0])
check("trace: lez-harness runs bin unit tests and lib doc-tests when present",
      any(ci_model.cargo_runs(c, "lez/updater/src/main.rs", text=BIN_TEST)[0] for c in ci_model.trace(jobs["lez-harness"], tracked))
      and any(ci_model.cargo_runs(c, "lez/harness/src/bin/tables.rs", text=BIN_TEST)[0] for c in ci_model.trace(jobs["lez-harness"], tracked))
      and any(ci_model.cargo_runs(c, "lez/harness/src/lib.rs", text=DOC_TEST)[0] for c in ci_model.trace(jobs["lez-harness"], tracked)))
# L4: --exact matches full test path, not substring
UPD_REC = ["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked", "--test", "recorded"]
check("cargo: --exact matches exact test name and refuses substring prefix",
      ci_model.cargo_runs(C(UPD_REC + ["--", "--exact", "gate_two_honest_one_adversary"], "lez", {"RISC0_DEV_MODE": "1"}, "t"),
                          "lez/updater/tests/recorded.rs")[0]
      and not ci_model.cargo_runs(C(UPD_REC + ["--", "--exact", "gate_two"], "lez", {"RISC0_DEV_MODE": "1"}, "t"),
                                  "lez/updater/tests/recorded.rs")[0])
check("cargo: --exact --skip ignores non-exact prefix and skips exact match",
      ci_model.cargo_runs(C(UPD_REC + ["--", "--exact", "--skip", "gate_two"], "lez", {"RISC0_DEV_MODE": "1"}, "t"),
                          "lez/updater/tests/recorded.rs")[0]
      and not ci_model.cargo_runs(C(UPD_REC + ["--", "--exact", "--skip", "gate_two_honest_one_adversary"], "lez", {"RISC0_DEV_MODE": "1"}, "t"),
                                  "lez/updater/tests/recorded.rs")[0])
# L3: ci_model trace for lez-harness matches check-lez-shards.py build_shard_plan for every shard
shard_mod = ci_model._shard_planner()
shard_map = __import__("json").loads((HERE / "lez-harness-shards.json").read_text(encoding="utf-8"))
planned_cargos = []
for sname in shard_map["shards"]:
    for step in shard_mod.build_shard_plan(shard_map, sname, build_only=False):
        if step["argv"][0] == "cargo":
            planned_cargos.append((step["cwd"], step["argv"]))
traced_cargos = [
    (c.cwd, c.argv) for c in ci_model.trace(jobs["lez-harness"], tracked)
    if c.argv and c.argv[0] == "cargo" and "--no-run" not in c.argv
    and "scripts/privacy/run.sh" not in c.origin
]
check("trace: lez-harness traced cargo commands equal check-lez-shards.py build_shard_plan across all shards",
      traced_cargos == planned_cargos)
FG = ci_model.feature_gates
check("features: #![cfg(feature)] gates the whole file", FG('#![cfg(feature = "a")]\n#[test]\nfn x() {}\n', set()) == ("a", set(), 0))
check("features: ... unless the feature is on", FG('#![cfg(feature = "a")]\n#[test]\nfn x() {}\n', {"a"}) == (None, set(), 1))
check("features: #[cfg(feature)] on one test of two",
      FG('#[cfg(feature = "b")]\n#[test]\nfn x() {}\n#[test]\nfn y() {}\n', set()) == (None, {"b"}, 1))
check("features: a gated module's tests, braces counted",
      FG('#[cfg(feature = "c")]\nmod t {\n    #[test]\n    fn x() { if true {} }\n}\n', set()) == (None, {"c"}, 0))
check("features: a test after a gated module is not gated",
      FG('#[cfg(feature = "c")]\nmod t {\n    #[test]\n    fn x() {}\n}\n#[test]\nfn y() {}\n', set()) == (None, {"c"}, 1))

# ── Silent non-runs and masked failures: what the static model must not count ────────────────────────
# Each check below fails under one of the checker mutants a review found surviving (feature-file-only,
# no-ignore-check, sh-skip-filter, sh-or-true, sh-no-devmode, sh-exit-early, yml-build-only, yml-step-if-false,
# yml-continue-on-error): synthetic jobs and texts, the real lez-harness script where a job calls it.
PUBLIC = "lez/harness/tests/public_path.rs"
LEZ_CMD = ["cargo", "test", "-p", "harness", "--locked"]


def synthetic_job(steps, **keys):
    return ci_model.Job(ROOT / ".github" / "workflows" / "ci.yml", "synthetic",
                        {"runs-on": "ubuntu-latest", "steps": steps, **keys}, {"on": {"push": None}})


def job_runs(job, path):
    for cmd in ci_model.trace(job, tracked):
        if (ci_model.cargo_runs(cmd, path)[0] if path.endswith(".rs") else ci_model.invokes(cmd, path, tracked)):
            return True
    return False


SCRIPT_STEP = {"name": "suites", "run": "scripts/ci/lez-harness.sh"}
check("guard: lez-harness.sh with no argument runs the harness's tests (the baseline of the checks below)",
      job_runs(synthetic_job([SCRIPT_STEP]), PUBLIC))
check("guard: a job whose only step is `lez-harness.sh build` runs no test (the script exits after its build)",
      not job_runs(synthetic_job([{"run": "scripts/ci/lez-harness.sh build"}]), PUBLIC))
check("guard: a step with `if: ${{ false }}` counts for nothing",
      not job_runs(synthetic_job([{**SCRIPT_STEP, "if": "${{ false }}"}]), PUBLIC))
check("guard: a step with `if: always()` still counts", job_runs(synthetic_job([{**SCRIPT_STEP, "if": "always()"}]), PUBLIC))
check("guard: a step with `continue-on-error: true` counts for nothing",
      not job_runs(synthetic_job([{**SCRIPT_STEP, "continue-on-error": True}]), PUBLIC))
check("guard: a job with `continue-on-error: true` counts for nothing",
      not job_runs(synthetic_job([SCRIPT_STEP], **{"continue-on-error": True}), PUBLIC))
check("guard: a job under an `if:` counts for nothing",
      not job_runs(synthetic_job([SCRIPT_STEP], **{"if": "github.ref == 'refs/heads/main'"}), PUBLIC))
check("guard: a job with `if: github.event_name == 'push' || github.event_name == 'pull_request'` counts for nothing (no skipped jobs allowed in ci.yml)",
      not job_runs(ci_model.Job(ROOT / ".github" / "workflows" / "ci.yml", "s",
                                {"runs-on": "ubuntu-latest", "steps": [SCRIPT_STEP],
                                 "if": "github.event_name == 'push' || github.event_name == 'pull_request'"},
                                {"on": {"push": None, "pull_request": None, "schedule": None}}), PUBLIC))
check("guard: lez-harness.sh --shard harness-4 runs public_path; --shard harness-1 does not",
      job_runs(synthetic_job([{"run": "scripts/ci/lez-harness.sh --shard harness-4"}]), PUBLIC)
      and not job_runs(synthetic_job([{"run": "scripts/ci/lez-harness.sh --shard harness-1"}]), PUBLIC))
check("guard: lez-harness.sh --shard harness-4 build / --dry-run runs no test",
      not job_runs(synthetic_job([{"run": "scripts/ci/lez-harness.sh --shard harness-4 build"}]), PUBLIC)
      and not job_runs(synthetic_job([{"run": "scripts/ci/lez-harness.sh --shard harness-4 --dry-run"}]), PUBLIC))
check("guard: lez-harness.sh --shard harness-4 && echo ok || true masks failure",
      not job_runs(synthetic_job([{"run": "scripts/ci/lez-harness.sh --shard harness-4 && echo ok || true"}]), PUBLIC))


def lez_text(text):
    return synthetic_job([{"working-directory": "lez", "run": text}])


check("guard: `cargo test ... || true` masks the test's failure",
      not job_runs(lez_text("RISC0_DEV_MODE=1 cargo test -p harness --locked || true"), PUBLIC))
check("guard: `cargo test ... && echo ok || true` masks it too (the chain before `||`)",
      not job_runs(lez_text("RISC0_DEV_MODE=1 cargo test -p harness --locked && echo ok || true"), PUBLIC))
check("guard: `cargo test ... || exit 1` keeps the failure",
      job_runs(lez_text("RISC0_DEV_MODE=1 cargo test -p harness --locked || exit 1"), PUBLIC))
check("guard: a cargo test run as an `if` condition counts for nothing",
      not job_runs(lez_text("if RISC0_DEV_MODE=1 cargo test -p harness --locked; then echo ok; fi"), PUBLIC))
check("guard: a cargo test run in the background (&) counts for nothing",
      not job_runs(lez_text("RISC0_DEV_MODE=1 cargo test -p harness --locked &\nwait"), PUBLIC))
check("guard: an unconditional `exit 0` before the test",
      not job_runs(lez_text("exit 0\nRISC0_DEV_MODE=1 cargo test -p harness --locked"), PUBLIC))
check("guard: an `exit` inside an `if` block does not stop the trace",
      job_runs(lez_text('if [ -n "$NO_SUCH_VARIABLE" ]; then exit 0; fi\nRISC0_DEV_MODE=1 cargo test -p harness --locked'),
               PUBLIC))
check("guard: an `exit` inside a function body does not stop the trace",
      job_runs(lez_text("die() {\n  exit 1\n}\nRISC0_DEV_MODE=1 cargo test -p harness --locked"), PUBLIC))
check("cargo: `-- --skip P` where P is in a test's path leaves the file out",
      not ci_model.cargo_runs(C(LEZ_CMD + ["--", "--skip", "a"], "lez", {}, "t"), PUBLIC)[0])
check("cargo: `-- --skip P` matching none of the file's tests keeps it",
      ci_model.cargo_runs(C(LEZ_CMD + ["--", "--skip", "zz_no_such_test"], "lez", {}, "t"), PUBLIC)[0])
check("cargo: a file that asserts RISC0_DEV_MODE (attest_private.rs) needs the command to set it",
      not ci_model.cargo_runs(C(LEZ_CMD, "lez", {}, "t"), "lez/harness/tests/attest_private.rs")[0]
      and ci_model.cargo_runs(C(LEZ_CMD, "lez", {"RISC0_DEV_MODE": "1"}, "t"), "lez/harness/tests/attest_private.rs")[0])
MODULE_GATED = '#[cfg(feature = "x")]\nmod t {\n    #[test]\n    fn a() {}\n}\n'
TEST_GATED = '#[cfg(feature = "x")]\n#[test]\nfn a() {}\n'
check("features: cargo_runs refuses a file whose every test is in a gated module",
      not ci_model.cargo_runs(C(LEZ_CMD, "lez", {}, "t"), PUBLIC, text=MODULE_GATED)[0])
check("features: cargo_runs refuses a file whose every test is gated",
      not ci_model.cargo_runs(C(LEZ_CMD, "lez", {}, "t"), PUBLIC, text=TEST_GATED)[0])
check("features: ... and accepts it once the command enables the feature",
      ci_model.cargo_runs(C(LEZ_CMD + ["--features", "x"], "lez", {}, "t"), PUBLIC, text=TEST_GATED)[0])
IGNORED = "#[test]\n#[ignore]\nfn a() {}\n"
check("cargo: a file whose every test is #[ignore]d is not run",
      not ci_model.cargo_runs(C(LEZ_CMD, "lez", {}, "t"), PUBLIC, text=IGNORED)[0])
check("cargo: ... unless the harness is given --ignored",
      ci_model.cargo_runs(C(LEZ_CMD + ["--", "--ignored"], "lez", {}, "t"), PUBLIC, text=IGNORED)[0])
check("guard: `cargo test ... | tee log` in a default-shell step (bash -e, no pipefail) counts for nothing",
      not job_runs(lez_text("RISC0_DEV_MODE=1 cargo test -p harness --locked | tee log"), PUBLIC))
check("guard: ... with `shell: bash` (pipefail) it counts",
      job_runs(synthetic_job([{"working-directory": "lez", "shell": "bash",
                               "run": "RISC0_DEV_MODE=1 cargo test -p harness --locked | tee log"}]), PUBLIC))
check("guard: ... and with `set -o pipefail` first",
      job_runs(lez_text("set -o pipefail\nRISC0_DEV_MODE=1 cargo test -p harness --locked | tee log"), PUBLIC))
check("guard: a custom shell without -e: a test that is not the last command counts for nothing",
      not job_runs(synthetic_job([{"working-directory": "lez", "shell": "bash {0}",
                                   "run": "RISC0_DEV_MODE=1 cargo test -p harness --locked\necho done"}]), PUBLIC))
check("guard: ... and counts once the text sets -e",
      job_runs(synthetic_job([{"working-directory": "lez", "shell": "bash {0}",
                               "run": "set -e\nRISC0_DEV_MODE=1 cargo test -p harness --locked\necho done"}]), PUBLIC))
LH = jobs["lez-harness"]
last = LH.steps[-1] if LH.steps else {}
check("jobs: no job in ci.yml has a job-level if: (none skipped on push/PR/dispatch)",
      all("if" not in j.spec for j in jobs.values() if j.workflow.name == "ci.yml"))


def _eval_gha_expr(value, ctx):
    """Evaluate a small GitHub Actions expression subset (==, !=, &&, ||, format(), github.*)."""
    if not isinstance(value, str):
        return value

    def eval_atom(tok):
        tok = tok.strip()
        if tok == "true":
            return True
        if tok == "false":
            return False
        if len(tok) >= 2 and tok[0] == "'" and tok[-1] == "'":
            return tok[1:-1].replace("''", "'")
        m_fmt = re.fullmatch(r"format\(\s*'((?:''|[^'])*)'\s*,\s*(.+)\)", tok)
        if m_fmt:
            fmt = m_fmt.group(1).replace("''", "'")
            args = [eval_atom(a) for a in m_fmt.group(2).split(",")]
            return fmt.format(*args)
        if tok in ctx:
            return ctx[tok]
        raise AssertionError(f"unsupported expression token in concurrency test: {tok!r}")

    def eval_cmp(expr):
        for op in ("==", "!="):
            if op in expr:
                lhs, rhs = expr.split(op, 1)
                lv, rv = eval_atom(lhs), eval_atom(rhs)
                return (lv == rv) if op == "==" else (lv != rv)
        return eval_atom(expr)

    def eval_and(expr):
        res = True
        for part in expr.split("&&"):
            res = eval_cmp(part)
            if not res:
                return res
        return res

    def eval_or(expr):
        res = False
        for part in expr.split("||"):
            res = eval_and(part)
            if res:
                return res
        return res

    m_whole = re.fullmatch(r"\$\{\{(.*)\}\}", value.strip())
    if m_whole:
        return eval_or(m_whole.group(1))
    return re.sub(r"\$\{\{(.*?)\}\}", lambda m: str(eval_or(m.group(1))), value)


ci_doc = ci_model.load_yaml((ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8"))
ci_conc = ci_doc.get("concurrency") or {}
m1_ctx = {"github.ref": "refs/heads/main", "github.run_id": "1001", "github.sha": "aaaa"}
m2_ctx = {"github.ref": "refs/heads/main", "github.run_id": "1002", "github.sha": "bbbb"}
b1_ctx = {"github.ref": "refs/heads/feat", "github.run_id": "2001", "github.sha": "cccc"}
b2_ctx = {"github.ref": "refs/heads/feat", "github.run_id": "2002", "github.sha": "dddd"}
pr1_ctx = {"github.ref": "refs/pull/42/merge", "github.run_id": "3001", "github.sha": "eeee"}
pr2_ctx = {"github.ref": "refs/pull/42/merge", "github.run_id": "3002", "github.sha": "ffff"}
g_m1, g_m2 = _eval_gha_expr(ci_conc.get("group"), m1_ctx), _eval_gha_expr(ci_conc.get("group"), m2_ctx)
g_b1, g_b2 = _eval_gha_expr(ci_conc.get("group"), b1_ctx), _eval_gha_expr(ci_conc.get("group"), b2_ctx)
g_p1, g_p2 = _eval_gha_expr(ci_conc.get("group"), pr1_ctx), _eval_gha_expr(ci_conc.get("group"), pr2_ctx)
cip_m = _eval_gha_expr(ci_conc.get("cancel-in-progress"), m1_ctx)
cip_b = _eval_gha_expr(ci_conc.get("cancel-in-progress"), b1_ctx)
cip_p = _eval_gha_expr(ci_conc.get("cancel-in-progress"), pr1_ctx)
check("jobs: ci.yml concurrency isolates each run on main (unique group, cancel-in-progress false) and cancels superseded runs elsewhere",
      bool(g_m1) and bool(g_m2) and g_m1 != g_m2 and cip_m is False
      and bool(g_b1) and g_b1 == g_b2 and cip_b is True
      and bool(g_p1) and g_p1 == g_p2 and g_p1 != g_b1 and cip_p is True)
check("jobs: lez-harness is an unconditional matrix job running scripts/ci/lez-harness.sh --shard \"$SHARD\" via env",
      last.get("run") == 'scripts/ci/lez-harness.sh --shard "$SHARD"'
      and (last.get("env") or {}).get("SHARD") == "${{ matrix.shard }}"
      and "if" not in last and "continue-on-error" not in last
      and "if" not in LH.spec and "continue-on-error" not in LH.spec and LH.on_change
      and ci_model.step_guard(LH, last) is None
      and bool(((LH.spec.get("strategy") or {}).get("matrix") or {}).get("shard")))
LHF = jobs["lez-harness-full"]
lhf_last = LHF.steps[-1] if LHF.steps else {}
check("jobs: lez-harness-full runs unsharded in lez-full.yml on schedule and workflow_dispatch with no if:",
      LHF.workflow.name == "lez-full.yml"
      and lhf_last.get("run") == "scripts/ci/lez-harness.sh"
      and "if" not in LHF.spec and "if" not in lhf_last and "continue-on-error" not in lhf_last
      and not LHF.on_change
      and "schedule" in LHF.triggers and "workflow_dispatch" in LHF.triggers
      and ci_model.step_guard(LHF, lhf_last) is None)

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
# The real map's counts, as docs/EVIDENCE.md row 162 states them: update these lines with the map (a row that gains a
# test CI runs moves both numbers).
rc, out = run_checker(real, "--list")
check("checker: the real map, RFP-022: 66 of 68 hard rows with a test CI runs",
      "RFP-022: 68 hard rows; 66 with a test CI runs; 2 without one" in out)
check("checker: the real map, RFP-021: 56 of 59 hard rows with a test CI runs",
      "RFP-021: 59 hard rows; 56 with a test CI runs; 3 without one" in out)
listed = re.findall(r"^    (RFP-02[12] (?:VS|Sec|Priv|F|U|R|P|S)\.\d+): (\S+)$", out, re.M)
rows022 = [rid for rid, _ in listed if rid.startswith("RFP-022 ")]
check("checker: --list gives every hard row its status", len(listed) == 59 + 68 and len(rows022) == 68)
rc, out = run_checker(real, "--require-ci", "RFP-022")
check("checker: --require-ci RFP-022 exits 1 on the real map and counts the rows without a test CI runs",
      rc == 1 and "--require-ci RFP-022: 2 hard row(s) without a test CI runs" in out)

# Synthetic maps: the real one with a test the vault job runs added to some RFP-022 rows (a row may have
# several [[row]] blocks; the checker joins them).
IN_CI = '{ path = "vault/test/Refund.t.sol", kind = "test", ci = "vault" }'
IN_MANUAL_JOB = '{ path = "scripts/ci/lez-harness.sh", kind = "test", ci = "lez-harness-full" }'


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
IN_FULL_JOB = '{ path = "lez/harness/tests/public_path.rs", kind = "test", ci = "lez-harness-full" }'
rc, out = run_checker(with_tests(with_tests(real, others), ["RFP-022 S.1"], IN_FULL_JOB), "--require-ci", "RFP-022")
check("checker: a test in lez-full.yml (lez-harness-full) is counted as manual, not as CI",
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
