"""Tests of scripts/deny_all.py and the cargo-deny policy (review-fix-supply M-3, L-4, L-5):
`python3 -I scripts/test_deny_all.py`.

- When a workspace's graph does not resolve, only a missing path dependency beside the repository (outside it and
  outside cargo's home, with no git or registry source involved) or a Logos Core module's absent staged copies is
  SKIPPED; a git source that cannot be fetched (offline, a deleted revision), a lockfile --locked refuses, a missing
  path inside the repository or inside cargo's home all FAIL, and the run's exit status is 1. The stderr fixtures are
  cargo's own (1.93 / 1.98.1, recorded on the build box: review-fix-supply box C, fix-supply-2 box run 1), with the
  machine's paths replaced by the test's.
- The advisories baseline accepts an advisory only in the workspaces its line names; a line naming none is refused.
- Each workspace is checked with the deny.toml nearest to it (gpu-prover/deny.toml for gpu-prover/prover).
- The policy files: gpu-prover/deny.toml is the root policy plus exactly the RISC Zero prover's exceptions; NCSA is
  allowed for libfuzzer-sys only; every clarification names the version that every lockfile holding the crate has.
A fake `cargo` on PATH answers `cargo metadata` and `cargo deny` from per-workspace fixtures. Standard library only
(tomllib: Python 3.11 or later); files go to a temporary directory (TMPDIR).
"""

import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCRIPT = os.path.join(HERE, "deny_all.py")
_spec = importlib.util.spec_from_file_location("deny_all", SCRIPT)
da = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(da)

# cargo 1.98.1, a workspace member's path dependency in a source tree beside the repository that is not there
# (review-fix-supply box C, lez-v03; paths replaced).
MISSING_BESIDE = """error: failed to load manifest for workspace member `{root}/lez-v03/wire`
referenced by workspace at `{root}/lez-v03/Cargo.toml`

Caused by:
  failed to load manifest for dependency `lee_core`

Caused by:
  failed to read `{parent}/upstream/lez-v0.3.1/lee/state_machine/core/Cargo.toml`

Caused by:
  No such file or directory (os error 2)
"""
# The same, for a plain path dependency (idl/spel-check).
MISSING_DEP = """error: failed to load manifest for dependency `spel-framework-core`

Caused by:
  failed to read `{path}`

Caused by:
  No such file or directory (os error 2)
"""
# An older cargo's wording for a missing path source (no URL anywhere: still a path).
MISSING_OLD_WORDING = """error: failed to get `lee_core` as a dependency of package `wire v0.1.0 ({root}/lez-v03/wire)`

Caused by:
  failed to load source for dependency `lee_core`

Caused by:
  Unable to update {parent}/upstream/lez-v0.3.1/lee/state_machine/core

Caused by:
  failed to read `{parent}/upstream/lez-v0.3.1/lee/state_machine/core/Cargo.toml`

Caused by:
  No such file or directory (os error 2)
"""
# cargo 1.98.1, an empty cargo home and --offline: a git source that cannot be fetched (review-fix-supply box C, cli).
OFFLINE_GIT = """error: failed to load source for dependency `keccak`

Caused by:
  unable to update https://github.com/logos-blockchain/sponges?rev=3a56e99771beedf04946eab21a4a62adc2951377#3a56e997

Caused by:
  can't checkout from 'https://github.com/logos-blockchain/sponges': you are in the offline mode (--offline)
"""
# A pinned git revision that no longer exists upstream.
DELETED_REV = """error: failed to get `keccak` as a dependency of package `ferrata-cli v0.1.0 ({root}/cli)`

Caused by:
  failed to load source for dependency `keccak`

Caused by:
  Unable to update https://github.com/logos-blockchain/sponges?rev=3a56e99771beedf04946eab21a4a62adc2951377

Caused by:
  revspec '3a56e99771beedf04946eab21a4a62adc2951377' not found; class=Reference (4); code=NotFound (-3)
"""
# cargo 1.98.1, a manifest the lockfile does not match, with --locked (fix-supply-2 box run 1).
LOCKED_MISMATCH = """    Updating crates.io index
error: cannot update the lock file {root}/cli/Cargo.lock because --locked was passed to prevent this
help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
"""
# A Logos Core module whose staged copies are absent (review-fix-supply box C).
MODULE_UNSTAGED = """error: failed to load manifest for workspace member `{root}/modules/ferrata_attest_module/rust-lib/calls`
referenced by workspace at `{root}/modules/ferrata_attest_module/rust-lib/Cargo.toml`

Caused by:
  failed to load manifest for dependency `ferrata-advance`

Caused by:
  failed to read `{root}/modules/ferrata_attest_module/rust-lib/staged/lez/advance/Cargo.toml`

Caused by:
  No such file or directory (os error 2)
"""


class SkipReason(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.parent = os.path.realpath(self.tmp.name)
        self.root = os.path.join(self.parent, "ferrata")
        os.makedirs(self.root)
        self.env = dict(os.environ)
        os.environ["CARGO_HOME"] = os.path.join(self.parent, "cargo-home")

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self.env)
        self.tmp.cleanup()

    def reason(self, text, ws="lez-v03", **kw):
        return da.skip_reason(text.format(root=self.root, parent=self.parent, **kw), self.root, ws)

    def test_missing_tree_beside_the_repository_is_skipped(self):
        r = self.reason(MISSING_BESIDE)
        self.assertIsNotNone(r)
        self.assertIn("beside the repository", r)
        self.assertIn("upstream/lez-v0.3.1", r)
        self.assertIsNotNone(self.reason(MISSING_DEP, ws="idl/spel-check",
                                         path=f"{self.parent}/upstream/spel/spel-framework-core/Cargo.toml"))
        self.assertIsNotNone(self.reason(MISSING_OLD_WORDING))

    def test_tree_present_is_not_skipped(self):
        p = os.path.join(self.parent, "upstream/lez-v0.3.1/lee/state_machine/core")
        os.makedirs(p)
        open(os.path.join(p, "Cargo.toml"), "w").close()
        self.assertIsNone(self.reason(MISSING_BESIDE))

    def test_fetch_and_lock_failures_are_not_skipped(self):
        for text in (OFFLINE_GIT, DELETED_REV, LOCKED_MISMATCH):
            self.assertIsNone(self.reason(text, ws="cli"), text[:60])
        # A missing path beside the repository does not excuse a git source failing in the same run.
        self.assertIsNone(self.reason(MISSING_BESIDE + OFFLINE_GIT))

    def test_missing_path_inside_the_repository_or_cargo_home_is_not_skipped(self):
        self.assertIsNone(self.reason(MISSING_DEP, ws=".", path=f"{self.root}/crates/ferrata-spectests/Cargo.toml"))
        git_checkout = f"{os.environ['CARGO_HOME']}/git/checkouts/sponges-1/3a56e99/keccak/Cargo.toml"
        self.assertIsNone(self.reason(MISSING_DEP, ws="cli", path=git_checkout))
        self.assertIsNone(self.reason(MISSING_DEP, ws="cli", path="/nonexistent-elsewhere/x/Cargo.toml"))

    def test_module_staged_sources(self):
        ws = "modules/ferrata_attest_module/rust-lib"
        r = self.reason(MODULE_UNSTAGED, ws=ws)
        self.assertIsNotNone(r)
        self.assertIn("staged sources", r)
        # The same path for a workspace that is not a module's library, or a path outside the staging dirs: no skip.
        self.assertIsNone(self.reason(MODULE_UNSTAGED, ws="lez"))
        other = f"{self.root}/modules/ferrata_attest_module/rust-lib/calls-extra/Cargo.toml"
        self.assertIsNone(self.reason(MISSING_DEP, ws=ws, path=other))


FAKE_CARGO = """#!{python}
# fake cargo for scripts/test_deny_all.py: answers from fixture.json in the working directory
import json, os, sys
fx = json.load(open("fixture.json"))
args = sys.argv[1:]
if args[:1] == ["metadata"]:
    m = fx.get("metadata", {{"rc": 0}})
    sys.stderr.write(m.get("stderr", ""))
    if m.get("rc", 0) == 0:
        print("{{}}")
    sys.exit(m.get("rc", 0))
if args[:1] == ["deny"]:
    with open(os.environ["FAKE_DENY_LOG"], "a") as f:
        f.write(json.dumps({{"cwd": os.getcwd(), "config": args[args.index("--config") + 1]}}) + "\\n")
    d = fx.get("deny", {{}})
    for line in d.get("lines", []):
        sys.stderr.write(json.dumps(line) + "\\n")
    sys.stderr.write(json.dumps({{"type": "summary", "fields": {{}}}}) + "\\n")
    sys.exit(d.get("rc", 0))
sys.exit("fake cargo: unexpected arguments " + " ".join(args))
"""


def advisory(adv_id, crate="foo", version="1.0.0"):
    return {"type": "diagnostic", "fields": {"severity": "error", "code": "vulnerability", "advisory": {"id": adv_id},
                                             "graphs": [{"Krate": {"name": crate, "version": version}}]}}


class Run(unittest.TestCase):
    """deny_all.py end to end in a tree of fixture workspaces, with the fake cargo."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.parent = os.path.realpath(self.tmp.name)
        self.root = os.path.join(self.parent, "tree")
        bin_dir = os.path.join(self.parent, "bin")
        os.makedirs(bin_dir)
        with open(os.path.join(bin_dir, "cargo"), "w") as f:
            f.write(FAKE_CARGO.format(python=sys.executable))
        os.chmod(os.path.join(bin_dir, "cargo"), 0o755)
        self.deny_log = os.path.join(self.parent, "deny.log")
        self.env = {"PATH": bin_dir + os.pathsep + "/usr/bin:/bin", "HOME": self.parent,
                    "CARGO_HOME": os.path.join(self.parent, "cargo-home"), "FAKE_DENY_LOG": self.deny_log}
        os.makedirs(self.root)
        open(os.path.join(self.root, "deny.toml"), "w").close()

    def tearDown(self):
        self.tmp.cleanup()

    def workspace(self, ws, fixture):
        d = os.path.join(self.root, ws)
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, "Cargo.lock"), "w").close()
        open(os.path.join(d, "Cargo.toml"), "w").close()
        text = json.dumps(fixture)
        with open(os.path.join(d, "fixture.json"), "w") as f:
            f.write(text.replace("{root}", self.root).replace("{parent}", self.parent))

    def run_all(self, *args):
        return subprocess.run([sys.executable, "-I", SCRIPT, "--root", self.root, *args], capture_output=True, text=True,
                              env=self.env, timeout=60)

    def test_skip_passes_fetch_failure_fails(self):
        self.workspace("lez-v03", {"metadata": {"rc": 101, "stderr": MISSING_BESIDE}})
        self.workspace("ok", {})
        r = self.run_all("--advisories", "off")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"SKIPPED lez-v03 .*beside the repository")
        self.assertIn("1 passed, 0 failed, 1 skipped", r.stdout)
        r = self.run_all("--advisories", "off", "--require-all")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.workspace("cli", {"metadata": {"rc": 101, "stderr": OFFLINE_GIT}})
        r = self.run_all("--advisories", "off")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"FAIL +cli .*does not resolve with --locked")
        self.assertIn("1 passed, 1 failed, 1 skipped", r.stdout)

    def test_lock_mismatch_and_deleted_revision_fail(self):
        self.workspace("cli", {"metadata": {"rc": 101, "stderr": LOCKED_MISMATCH}})
        self.workspace("bridge", {"metadata": {"rc": 101, "stderr": DELETED_REV}})
        r = self.run_all("--advisories", "off")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("0 passed, 2 failed, 0 skipped", r.stdout)

    def test_baseline_is_per_workspace(self):
        self.workspace("a", {"deny": {"rc": 1, "lines": [advisory("RUSTSEC-2099-0001")]}})
        self.workspace("b", {"deny": {"rc": 1, "lines": [advisory("RUSTSEC-2099-0001")]}})
        base = os.path.join(self.parent, "baseline.txt")
        with open(base, "w") as f:
            f.write("# test\nRUSTSEC-2099-0001 a\n")
        r = self.run_all("--advisories", "baseline", "--baseline", base)
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"PASS +a ")
        self.assertRegex(r.stdout, r"FAIL +b .*does not accept here: RUSTSEC-2099-0001")
        with open(base, "w") as f:
            f.write("RUSTSEC-2099-0001 a b  # both\n")
        r = self.run_all("--advisories", "baseline", "--baseline", base)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        with open(base, "w") as f:
            f.write("RUSTSEC-2099-0001  # 2 workspace(s): a, b\n")      # the old format: no workspaces
        r = self.run_all("--advisories", "baseline", "--baseline", base)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("names no workspace", r.stderr)
        out = os.path.join(self.parent, "written.txt")
        r = self.run_all("--advisories", "report", "--write-baseline", out)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(da.read_baseline(out), {"RUSTSEC-2099-0001": {"a", "b"}})

    def test_nearest_policy(self):
        self.workspace("gpu-prover/prover", {})
        self.workspace("cli", {})
        open(os.path.join(self.root, "gpu-prover", "deny.toml"), "w").close()
        r = self.run_all("--advisories", "off")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        with open(self.deny_log) as f:
            used = {os.path.relpath(j["cwd"], self.root): os.path.relpath(j["config"], self.root)
                    for j in map(json.loads, f)}
        self.assertEqual(used, {"gpu-prover/prover": "gpu-prover/deny.toml", "cli": "deny.toml"})


class Policy(unittest.TestCase):
    def load(self, rel):
        with open(os.path.join(ROOT, rel), "rb") as f:
            return tomllib.load(f)

    def test_gpu_prover_policy_is_the_root_policy_plus_the_prover_exceptions(self):
        root, gpu = self.load("deny.toml"), self.load("gpu-prover/deny.toml")
        er, eg = root["licenses"].pop("exceptions"), gpu["licenses"].pop("exceptions")
        self.assertEqual(root, gpu)
        self.assertTrue(all(e in eg for e in er))
        extra = [e for e in eg if e not in er]
        self.assertEqual(sorted(e["crate"] for e in extra),
                         ["downloader", "malachite", "malachite-base", "malachite-float", "malachite-nz", "malachite-q"])
        self.assertFalse(any(e["crate"].startswith(("malachite", "downloader")) for e in er))

    def test_ncsa_only_for_libfuzzer(self):
        p = self.load("deny.toml")["licenses"]
        self.assertNotIn("NCSA", p["allow"])
        self.assertIn({"allow": ["NCSA"], "crate": "libfuzzer-sys"}, p["exceptions"])

    def test_clarifications_name_the_locked_versions(self):
        clar = self.load("deny.toml")["licenses"]["clarify"]
        self.assertEqual(len(clar), 5)
        locks = subprocess.run(["git", "-C", ROOT, "ls-files", "--", "*Cargo.lock"], capture_output=True,
                               text=True).stdout.split()
        self.assertTrue(locks, "no tracked Cargo.lock (run in a git checkout)")
        for c in clar:
            name, _, version = c["crate"].partition("@")
            self.assertTrue(version, f"{c['crate']}: no version")
            seen = set()
            for lock in locks:
                with open(os.path.join(ROOT, lock), encoding="utf-8") as f:
                    seen.update(re.findall(rf'name = "{re.escape(name)}"\nversion = "([^"]+)"', f.read()))
            self.assertEqual(seen, {version}, f"{name}: lockfiles hold {sorted(seen)}, deny.toml clarifies {version}")


if __name__ == "__main__":
    unittest.main()
