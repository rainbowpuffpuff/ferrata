#!/usr/bin/env python3
"""Tests for scripts/ci/check-lez-shards.py (standard library only; run by CI's links job).

Verifies that the shard guard passes on the real map and refuses:
  - an extra unsharded test file under tests/
  - an extra tests/<dir>/main.rs binary, while ignoring helper modules like tests/helper_common/mod.rs
  - a missing entry from the map
  - a duplicate binary run by two shards
  - a map entry naming a nonexistent binary
  - privacy_suite listed in harness_tests alongside privacy: true
  - a feature-gated entry whose #![cfg(feature = ...)] was removed
  - a split binary with mismatched exact/skip sets, a typo in a test name, or a duplicate test name
  - autotests = false with explicit [[test]] entries in Cargo.toml

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
check("real map discovers 49 harness binaries (44 active + 5 feature-gated) and 11 updater binaries",
      summary.get("harness_total") == 49 and summary.get("harness_feature_gated") == 5 and summary.get("updater_total") == 11,
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

print()
print(f"test_lez_shards.py: {npass} passed, {nfail} failed")
sys.exit(1 if nfail else 0)
