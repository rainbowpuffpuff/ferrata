#!/usr/bin/env python3
"""Tests for scripts/ci/check-lez-shards.py and scripts/ci/lez-harness.sh (standard library only; run by CI's links job).

Verifies that the shard guard passes on the real map, refuses missing/duplicate/empty/feature-gated/bin/doc-test
gaps, and executes the real `--shard` loop in `scripts/ci/lez-harness.sh` against stubbed `cargo` and privacy
scripts (verifying exact command lists per shard, stdin isolation, failure propagation, and empty-plan refusal).

    python3 -I scripts/ci/test_lez_shards.py
"""

import copy
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("check_lez_shards", HERE / "check-lez-shards.py")
mod = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mod)

ROOT = mod.ROOT
REAL_MAP = json.loads(mod.DEFAULT_MAP.read_text(encoding="utf-8"))
npass = nfail = 0


def check(name, cond, detail=""):
    global npass, nfail
    if cond:
        npass += 1
        print(f"ok   {name}")
    else:
        nfail += 1
        print(f"FAIL {name}" + (f" ({detail})" if detail else ""))


def copy_pkg(src: pathlib.Path, dst: pathlib.Path):
    """Copy Cargo.toml, tests/ and src/ of a package (lightweight scratch copy)."""
    dst.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src / "Cargo.toml", dst / "Cargo.toml")
    shutil.copytree(src / "tests", dst / "tests")
    shutil.copytree(src / "src", dst / "src")


def run_check(map_data, harness_dir=mod.DEFAULT_HARNESS_DIR, updater_dir=mod.DEFAULT_UPDATER_DIR, workflow=None):
    with tempfile.TemporaryDirectory() as d:
        mp = pathlib.Path(d) / "shards.json"
        mp.write_text(json.dumps(map_data, indent=2), encoding="utf-8")
        return mod.check_shards(mp, harness_dir, updater_dir, workflow_path=workflow)


# 1. Real map passes
probs, summary = mod.check_shards()
check("real map passes guard", not probs, "; ".join(probs))
check("real map discovers 55 harness binaries (45 active + 10 feature-gated: names and names_review run in job `names`, the 3 messages tests in job `msg`) and 13 updater binaries",
      summary.get("harness_total") == 55 and summary.get("harness_feature_gated") == 10 and summary.get("updater_total") == 13,
      str(summary))

# 2. Scratch copy with an extra test file in lez/harness/tests/ -> fails
with tempfile.TemporaryDirectory() as d:
    h_copy = pathlib.Path(d) / "harness"
    copy_pkg(mod.DEFAULT_HARNESS_DIR, h_copy)
    (h_copy / "tests" / "extra_unsharded.rs").write_text("#[test]\nfn boom() {}\n", encoding="utf-8")
    probs, _ = run_check(REAL_MAP, harness_dir=h_copy)
    check("extra harness test file fails guard",
          any("extra_unsharded" in p and "not run by any shard" in p for p in probs),
          "; ".join(probs))

# 3. Helper subdir without main.rs is ignored; subdir with main.rs is discovered and fails if unsharded
with tempfile.TemporaryDirectory() as d:
    h_copy = pathlib.Path(d) / "harness"
    copy_pkg(mod.DEFAULT_HARNESS_DIR, h_copy)
    helper = h_copy / "tests" / "helper_common"
    helper.mkdir()
    (helper / "mod.rs").write_text("pub fn helper() {}\n", encoding="utf-8")
    probs, _ = run_check(REAL_MAP, harness_dir=h_copy)
    check("helper subdirectory (mod.rs, no main.rs) is ignored as a module", not probs, "; ".join(probs))

    sub_bin = h_copy / "tests" / "sub_bin"
    sub_bin.mkdir()
    (sub_bin / "main.rs").write_text("#[test]\nfn sub() {}\n", encoding="utf-8")
    probs, _ = run_check(REAL_MAP, harness_dir=h_copy)
    check("subdirectory with main.rs is discovered as a binary and fails if unsharded",
          any("sub_bin" in p and "not run by any shard" in p for p in probs),
          "; ".join(probs))

# 4. Missing entry in map -> fails
m = copy.deepcopy(REAL_MAP)
m["shards"]["harness-4"]["harness_tests"].remove("public_path")
probs, _ = run_check(m)
check("missing harness entry in map fails guard",
      any("public_path" in p and "not run by any shard" in p for p in probs),
      "; ".join(probs))

# 5. Duplicate binary across two shards -> fails
m = copy.deepcopy(REAL_MAP)
m["shards"]["harness-1"]["harness_tests"].append("public_path")
probs, _ = run_check(m)
check("duplicate harness binary fails guard",
      any("public_path" in p and "more than once" in p for p in probs),
      "; ".join(probs))

# 6. Nonexistent binary in map -> fails
m = copy.deepcopy(REAL_MAP)
m["shards"]["harness-1"]["harness_tests"].append("does_not_exist")
probs, _ = run_check(m)
check("nonexistent binary in map fails guard",
      any("does_not_exist" in p and "does not exist" in p for p in probs),
      "; ".join(probs))

# 7. privacy_suite listed in harness_tests as well as privacy: true -> fails (duplicate)
m = copy.deepcopy(REAL_MAP)
m["shards"]["harness-2"]["harness_tests"].append("privacy_suite")
probs, _ = run_check(m)
check("privacy_suite in harness_tests alongside privacy: true fails as duplicate",
      any("privacy_suite" in p and "more than once" in p for p in probs),
      "; ".join(probs))

# 8. Feature-gated binary whose #![cfg(feature = ...)] is removed -> fails
with tempfile.TemporaryDirectory() as d:
    h_copy = pathlib.Path(d) / "harness"
    copy_pkg(mod.DEFAULT_HARNESS_DIR, h_copy)
    p = h_copy / "tests" / "statements.rs"
    p.write_text(p.read_text(encoding="utf-8").replace('#![cfg(feature = "statements")]', ""), encoding="utf-8")
    probs, _ = run_check(REAL_MAP, harness_dir=h_copy)
    check("removing #![cfg(feature)] from feature-gated binary fails guard",
          any("statements" in p and "gate features" in p for p in probs),
          "; ".join(probs))

# 9. Split mismatch (exact vs skip differ) -> fails
m = copy.deepcopy(REAL_MAP)
m["shards"]["updater-privacy"]["updater_splits"][0]["names"].pop()
probs, _ = run_check(m)
check("split exact/skip mismatch fails guard",
      any("exact/skip mismatch" in p for p in probs),
      "; ".join(probs))

# 10. Split naming a nonexistent test function -> fails
m = copy.deepcopy(REAL_MAP)
m["shards"]["updater-gate"]["updater_splits"][0]["names"].append("no_such_test_fn")
m["shards"]["updater-privacy"]["updater_splits"][0]["names"].append("no_such_test_fn")
probs, _ = run_check(m)
check("split with nonexistent test name fails guard",
      any("no_such_test_fn" in p and "nonexistent test" in p for p in probs),
      "; ".join(probs))

# 11. Missing updater binary and missing updater_lib -> fails
m = copy.deepcopy(REAL_MAP)
m["shards"]["updater-privacy"]["updater_tests"].remove("refusals")
m["shards"]["updater-privacy"]["updater_lib"] = False
probs, _ = run_check(m)
check("missing updater binary fails guard",
      any("refusals" in p and "not run by any shard" in p for p in probs),
      "; ".join(probs))
check("missing updater_lib fails guard when src/ has unit tests",
      any("updater_lib" in p for p in probs),
      "; ".join(probs))

# 11b. M1: bin unit tests (--bins) and doc-tests (--doc) are required and planned
m = copy.deepcopy(REAL_MAP)
m["shards"]["harness-1"]["harness_bins"] = False
m["shards"]["harness-1"]["harness_doc"] = False
m["shards"]["updater-privacy"]["updater_bins"] = False
m["shards"]["updater-privacy"]["updater_doc"] = False
with tempfile.TemporaryDirectory() as d:
    h_copy = pathlib.Path(d) / "harness"
    u_copy = pathlib.Path(d) / "updater"
    copy_pkg(mod.DEFAULT_HARNESS_DIR, h_copy)
    copy_pkg(mod.DEFAULT_UPDATER_DIR, u_copy)
    with open(h_copy / "src" / "bin" / "tables.rs", "a", encoding="utf-8") as fh:
        fh.write("\n#[test]\nfn bin_unit_test_added() { assert!(false); }\n")
    with open(u_copy / "src" / "main.rs", "a", encoding="utf-8") as fh:
        fh.write("\n#[test]\nfn bin_unit_test_added() { assert!(false); }\n")
    with open(h_copy / "src" / "lib.rs", "a", encoding="utf-8") as fh:
        fh.write("\n/// ```\n/// assert!(false);\n/// ```\npub fn doc_added() {}\n")
    probs, _ = run_check(m, harness_dir=h_copy, updater_dir=u_copy)
    check("missing harness_bins fails guard (including with bin #[test] added)",
          any("harness_bins" in p and "has tests" in p for p in probs),
          "; ".join(probs))
    check("missing updater_bins fails guard (including with src/main.rs #[test] added)",
          any("updater_bins" in p and "has tests" in p for p in probs),
          "; ".join(probs))
    check("missing harness_doc fails guard (including with lib doc-test added)",
          any("harness_doc" in p and "has tests" in p for p in probs),
          "; ".join(probs))
    check("missing updater_doc fails guard",
          any("updater_doc" in p for p in probs),
          "; ".join(probs))

m_dup = copy.deepcopy(REAL_MAP)
m_dup["shards"]["harness-2"]["harness_bins"] = True
m_dup["shards"]["harness-2"]["harness_doc"] = True
probs, _ = run_check(m_dup)
check("duplicate harness_bins and harness_doc fail guard",
      any("harness_bins" in p and "more than one shard" in p for p in probs)
      and any("harness_doc" in p and "more than one shard" in p for p in probs),
      "; ".join(probs))

h1_build = mod.build_shard_plan(REAL_MAP, "harness-1", build_only=True)
h1_run = mod.build_shard_plan(REAL_MAP, "harness-1", build_only=False)
up_build = mod.build_shard_plan(REAL_MAP, "updater-privacy", build_only=True)
up_run = mod.build_shard_plan(REAL_MAP, "updater-privacy", build_only=False)
check("build_shard_plan emits --bins and separate --doc in run mode, and never --doc with --no-run",
      "--bins" in h1_build[0]["argv"] and "--doc" not in h1_build[0]["argv"]
      and "--bins" in h1_run[0]["argv"] and "--doc" in h1_run[1]["argv"]
      and any("--bins" in c["argv"] and "--doc" not in c["argv"] for c in up_build)
      and any("--bins" in c["argv"] for c in up_run)
      and any("--doc" in c["argv"] for c in up_run),
      f"h1_build={h1_build} h1_run={h1_run}")

# 11c. L1: empty shard refused by both guard and planner
m_empty = copy.deepcopy(REAL_MAP)
m_empty["shards"]["empty-shard"] = {}
probs, _ = run_check(m_empty)
check("empty shard {} fails guard",
      any("empty-shard" in p and "empty shard" in p for p in probs),
      "; ".join(probs))
empty_plan_failed = False
try:
    mod.build_shard_plan(m_empty, "empty-shard")
except ValueError:
    empty_plan_failed = True
check("empty shard {} raises ValueError in build_shard_plan", empty_plan_failed)

# 11d. Extra updater test file (e.g. a new tests/*.rs on another branch) fails guard
with tempfile.TemporaryDirectory() as d:
    u_copy = pathlib.Path(d) / "updater"
    copy_pkg(mod.DEFAULT_UPDATER_DIR, u_copy)
    (u_copy / "tests" / "extra_updater.rs").write_text("#[test]\nfn t() {}\n", encoding="utf-8")
    probs, _ = run_check(REAL_MAP, updater_dir=u_copy)
    check("extra updater test file fails guard",
          any("extra_updater" in p and "not run by any shard" in p for p in probs),
          "; ".join(probs))

# 12. autotests = false + explicit [[test]] in Cargo.toml
with tempfile.TemporaryDirectory() as d:
    pkg = pathlib.Path(d) / "pkg"
    (pkg / "tests").mkdir(parents=True)
    (pkg / "src").mkdir()
    (pkg / "Cargo.toml").write_text(
        '[package]\nname = "demo"\nversion = "0.1.0"\nautotests = false\n\n'
        '[[test]]\nname = "explicit_one"\npath = "tests/custom.rs"\n',
        encoding="utf-8",
    )
    (pkg / "tests" / "custom.rs").write_text("#[test]\nfn ok() {}\n", encoding="utf-8")
    (pkg / "tests" / "ignored_by_autotests.rs").write_text("#[test]\nfn nope() {}\n", encoding="utf-8")
    bins = mod.discover_test_binaries(pkg, set())
    check("autotests = false discovers only explicit [[test]] entries",
          set(bins.keys()) == {"explicit_one"} and bins["explicit_one"]["rel_path"] == "tests/custom.rs",
          str(list(bins.keys())))

# 13. Dry-run CLI via lez-harness.sh works without RISC0_HOME_LEZ and prints cargo commands for every shard
with tempfile.TemporaryDirectory() as empty_home:
    dry_env = {k: v for k, v in os.environ.items() if k not in ("RISC0_HOME_LEZ", "RISC0_HOME")}
    dry_env["HOME"] = empty_home
    p = subprocess.run(
        ["bash", str(HERE / "lez-harness.sh"), "--dry-run"],
        capture_output=True, text=True, env=dry_env,
    )
    check("lez-harness.sh --dry-run exits 0 without RISC0_HOME_LEZ", p.returncode == 0, p.stderr)
    check("lez-harness.sh --dry-run prints all 6 shards",
          all(f"# shard: {s}" in p.stdout for s in REAL_MAP["shards"]),
          p.stdout[:200])
    p_b = subprocess.run(
        ["bash", str(HERE / "lez-harness.sh"), "--dry-run", "--shard", "updater-privacy", "build"],
        capture_output=True, text=True, env=dry_env,
    )
    check("lez-harness.sh --dry-run --shard updater-privacy build exits 0 with --no-run",
          p_b.returncode == 0 and "--no-run" in p_b.stdout and "privacy_suite" in p_b.stdout,
          p_b.stdout + p_b.stderr)
    p_empty = subprocess.run(
        ["bash", str(HERE / "lez-harness.sh"), "--shard", ""],
        capture_output=True, text=True, env=dry_env,
    )
    check("lez-harness.sh --shard '' exits 2 instead of falling through to unsharded",
          p_empty.returncode == 2, f"rc={p_empty.returncode} {p_empty.stderr}")

# 14. Workflow matrix mismatch in ci.yml -> fails
real_ci = mod.DEFAULT_WORKFLOW.read_text(encoding="utf-8")
with tempfile.TemporaryDirectory() as d:
    wf = pathlib.Path(d) / "ci.yml"
    wf.write_text(real_ci.replace(", updater-privacy]", "]"), encoding="utf-8")
    probs, _ = run_check(REAL_MAP, workflow=wf)
    check("ci.yml matrix missing a shard fails guard",
          any("differ from shard map" in p for p in probs),
          "; ".join(probs))

    wf.write_text(real_ci.replace("shard: [harness-1", "shard: [harness-1, harness-1"), encoding="utf-8")
    probs, _ = run_check(REAL_MAP, workflow=wf)
    check("ci.yml matrix duplicate shard fails guard",
          any("duplicate shard" in p for p in probs),
          "; ".join(probs))

# 15. M3 + L1 + L2: real execution test of lez-harness.sh's --shard loop with stubbed cargo & privacy scripts
with tempfile.TemporaryDirectory() as td:
    base = pathlib.Path(td).resolve()
    stub_repo = base / "repo"
    stub_bin = base / "bin"
    empty_home = base / "home"
    empty_risc0 = base / "risc0-lez"
    priv_out = base / "privacy-out"
    for p in (stub_bin, empty_home, empty_risc0, stub_repo / "scripts" / "ci",
              stub_repo / "scripts" / "privacy", stub_repo / ".github" / "workflows"):
        p.mkdir(parents=True, exist_ok=True)

    for fn in ("check-lez-shards.py", "ci_model.py", "lez-harness-shards.json", "lez-harness.sh"):
        shutil.copy2(HERE / fn, stub_repo / "scripts" / "ci" / fn)
    shutil.copy2(mod.DEFAULT_WORKFLOW, stub_repo / ".github" / "workflows" / "ci.yml")
    copy_pkg(mod.DEFAULT_HARNESS_DIR, stub_repo / "lez" / "harness")
    copy_pkg(mod.DEFAULT_UPDATER_DIR, stub_repo / "lez" / "updater")

    # Stub cargo, scripts/privacy/run.sh, and scripts/privacy/check-results.py.
    # Each drains stdin to prove commands in the loop cannot consume remaining plan lines (L2).
    cargo_stub = stub_bin / "cargo"
    cargo_stub.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "cat >/dev/null\n"
        'python3 -I - "$STUB_LOG" "$PWD" "${RISC0_DEV_MODE:-0}" "cargo" "$@" <<\'PY\'\n'
        "import json, os, sys\n"
        "log, cwd, dev, *argv = sys.argv[1:]\n"
        'with open(log, "a", encoding="utf-8") as fh:\n'
        '    fh.write(json.dumps({"cwd": os.path.realpath(cwd), "dev": dev, "argv": argv}) + "\\n")\n'
        "PY\n"
        'if [ -n "${STUB_FAIL_SUBSTR:-}" ] && [[ "$*" == *"$STUB_FAIL_SUBSTR"* ]]; then\n'
        '  exit "${STUB_FAIL_CODE:-101}"\n'
        "fi\n",
        encoding="utf-8",
    )
    cargo_stub.chmod(0o755)

    run_stub = stub_repo / "scripts" / "privacy" / "run.sh"
    run_stub.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "cat >/dev/null\n"
        'python3 -I - "$STUB_LOG" "$PWD" "${RISC0_DEV_MODE:-0}" "bash" "$0" "$@" <<\'PY\'\n'
        "import json, os, sys\n"
        "log, cwd, dev, *argv = sys.argv[1:]\n"
        'with open(log, "a", encoding="utf-8") as fh:\n'
        '    fh.write(json.dumps({"cwd": os.path.realpath(cwd), "dev": dev, "argv": argv}) + "\\n")\n'
        "PY\n",
        encoding="utf-8",
    )
    run_stub.chmod(0o755)

    gate_stub = stub_repo / "scripts" / "privacy" / "check-results.py"
    gate_stub.write_text(
        "import json, os, sys\n"
        "sys.stdin.read()\n"
        'with open(os.environ["STUB_LOG"], "a", encoding="utf-8") as fh:\n'
        '    fh.write(json.dumps({"cwd": os.path.realpath(os.getcwd()), '
        '"dev": os.environ.get("RISC0_DEV_MODE", "0"), '
        '"argv": ["python3", "-I", sys.argv[0], *sys.argv[1:]]}) + "\\n")\n',
        encoding="utf-8",
    )
    shutil.copy2(gate_stub, stub_repo / "scripts" / "privacy" / "controls.py")

    def run_stub_shard(extra_args, extra_env=None):
        log_file = base / "calls.jsonl"
        if log_file.exists():
            log_file.unlink()
        env = {k: v for k, v in os.environ.items() if k not in ("RISC0_HOME", "RISC0_HOME_LEZ", "STUB_FAIL_SUBSTR", "STUB_FAIL_CODE")}
        env.update({
            "HOME": str(empty_home),
            "PATH": f"{stub_bin}:{env.get('PATH', '')}",
            "RISC0_HOME_LEZ": str(empty_risc0),
            "FERRATA_PRIVACY_OUT": str(priv_out),
            "FERRATA_TEST_THREADS": "4",
            "STUB_LOG": str(log_file),
        })
        if extra_env:
            env.update(extra_env)
        proc = subprocess.run(
            ["bash", str(stub_repo / "scripts" / "ci" / "lez-harness.sh"), *extra_args],
            capture_output=True, text=True, env=env,
        )
        logged = []
        if log_file.exists():
            for line in log_file.read_text(encoding="utf-8").splitlines():
                if line.strip():
                    logged.append(json.loads(line))
        return proc, logged

    def expected_from_tsv(shard_name, build_only=False):
        cmd = [
            sys.executable, "-I", str(stub_repo / "scripts" / "ci" / "check-lez-shards.py"),
            "--emit-tsv", shard_name, "--threads", "4",
            "--privacy-out", str(priv_out), "--repo-prefix", f"{stub_repo}/",
        ]
        if build_only:
            cmd.append("--build-only")
        p = subprocess.run(cmd, capture_output=True, text=True, check=True)
        want = []
        for line in p.stdout.splitlines():
            if not line.strip():
                continue
            fields = line.split("\t")
            want.append({
                "cwd": os.path.realpath(stub_repo / fields[0]),
                "dev": fields[1],
                "argv": fields[2:],
            })
        return want

    for shard_name in REAL_MAP["shards"]:
        p_run, logged_run = run_stub_shard(["--shard", shard_name])
        want_run = expected_from_tsv(shard_name, build_only=False)
        check(f"stub loop: --shard {shard_name} executes exact --emit-tsv commands (with stdin drained)",
              p_run.returncode == 0 and logged_run == want_run and f"passed shard {shard_name}" in p_run.stdout,
              f"rc={p_run.returncode} stderr={p_run.stderr} got={logged_run} want={want_run}")

        p_bld, logged_bld = run_stub_shard(["--shard", shard_name, "build"])
        want_bld = expected_from_tsv(shard_name, build_only=True)
        check(f"stub loop: --shard {shard_name} build executes exact --emit-tsv --build-only commands",
              p_bld.returncode == 0 and logged_bld == want_bld and f"built shard {shard_name}" in p_bld.stdout,
              f"rc={p_bld.returncode} stderr={p_bld.stderr} got={logged_bld} want={want_bld}")

    # Failure propagation: first command failing aborts shard with cargo's exit status and skips remaining steps
    p_fail1, logged_fail1 = run_stub_shard(
        ["--shard", "updater-privacy"],
        {"STUB_FAIL_SUBSTR": "--bins", "STUB_FAIL_CODE": "101"},
    )
    check("stub loop: failing first cargo command fails shard with exit 101 and skips later steps",
          p_fail1.returncode == 101 and len(logged_fail1) == 1,
          f"rc={p_fail1.returncode} calls={len(logged_fail1)}")

    # Failure propagation: third command (--test recorded) failing skips privacy scripts
    p_fail3, logged_fail3 = run_stub_shard(
        ["--shard", "updater-privacy"],
        {"STUB_FAIL_SUBSTR": "--test recorded", "STUB_FAIL_CODE": "101"},
    )
    check("stub loop: failing split cargo command exits 101 and skips privacy suite and gate",
          p_fail3.returncode == 101 and len(logged_fail3) == 3
          and not any("privacy/run.sh" in " ".join(c["argv"]) for c in logged_fail3),
          f"rc={p_fail3.returncode} calls={logged_fail3}")

    # Unknown shard and repeated --shard
    p_unk, logged_unk = run_stub_shard(["--shard", "no-such-shard"])
    check("stub loop: unknown shard exits non-zero and runs no commands",
          p_unk.returncode != 0 and len(logged_unk) == 0,
          f"rc={p_unk.returncode} calls={logged_unk}")

    p_rep, logged_rep = run_stub_shard(["--shard", "harness-1", "--shard", "harness-2"])
    check("stub loop: repeated --shard exits 2 and runs no commands",
          p_rep.returncode == 2 and len(logged_rep) == 0,
          f"rc={p_rep.returncode} stderr={p_rep.stderr}")

    # L1 defense-in-depth in lez-harness.sh: an empty plan from check-lez-shards.py fails the shard
    empty_repo = base / "empty_plan_repo"
    (empty_repo / "scripts" / "ci").mkdir(parents=True)
    shutil.copy2(HERE / "lez-harness.sh", empty_repo / "scripts" / "ci" / "lez-harness.sh")
    (empty_repo / "scripts" / "ci" / "check-lez-shards.py").write_text("import sys\nsys.exit(0)\n", encoding="utf-8")
    env_ep = {k: v for k, v in os.environ.items() if k not in ("RISC0_HOME", "RISC0_HOME_LEZ")}
    env_ep.update({"HOME": str(empty_home), "RISC0_HOME_LEZ": str(empty_risc0)})
    p_ep = subprocess.run(
        ["bash", str(empty_repo / "scripts" / "ci" / "lez-harness.sh"), "--shard", "harness-1"],
        capture_output=True, text=True, env=env_ep,
    )
    check("stub loop: empty plan fails lez-harness.sh instead of printing passed shard",
          p_ep.returncode != 0 and "produced no commands" in p_ep.stderr and "passed shard" not in p_ep.stdout,
          f"rc={p_ep.returncode} out={p_ep.stdout} err={p_ep.stderr}")

    # N2: step-count check in lez-harness.sh refuses a dropped/malformed plan line (fails if check is removed)
    short_repo = base / "short_plan_repo"
    (short_repo / "scripts" / "ci").mkdir(parents=True)
    shutil.copy2(HERE / "lez-harness.sh", short_repo / "scripts" / "ci" / "lez-harness.sh")
    (short_repo / "scripts" / "ci" / "check-lez-shards.py").write_text(
        'import sys\nif "--emit-tsv" in sys.argv:\n    print("lez\\t1")\nsys.exit(0)\n',
        encoding="utf-8",
    )
    p_sp = subprocess.run(
        ["bash", str(short_repo / "scripts" / "ci" / "lez-harness.sh"), "--shard", "harness-1"],
        capture_output=True, text=True, env=env_ep,
    )
    check("stub loop: malformed 2-field plan line triggers step-count mismatch (exit 1, executed 0 of 1)",
          p_sp.returncode == 1 and "executed 0 of 1" in p_sp.stderr and "passed shard" not in p_sp.stdout,
          f"rc={p_sp.returncode} out={p_sp.stdout} err={p_sp.stderr}")

print()
print(f"test_lez_shards.py: {npass} passed, {nfail} failed")
sys.exit(1 if nfail else 0)
