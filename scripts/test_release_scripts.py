"""Tests of the release scripts' provenance checks (review-code-ops L3, M1): `python3 -I scripts/test_release_scripts.py`.

- `scripts/release.sh commit`: "+dirty" for a modified tracked file and for an untracked one (cargo and the build box
  use both), not for an ignored one; FERRATA_COMMIT wins; "unknown" outside git.
- `scripts/reproduce-image-ids.sh build`: refuses an upstream LEZ tree that is missing, not a clone, or at another commit
  than v0.4.0-rc1's, before anything is built.
- `scripts/release.sh build`: a corrupted cached native archive stops it before anything is built (run with a PATH
  that has no cargo or forge, so nothing heavy could start even if the check failed open).
- `scripts/release.sh verify`: without r0vm it says the image ids were not recomputed; --strict fails.
Standard library only; bash and git needed; files go to a temporary directory (TMPDIR).
"""

import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
LEZ_COMMIT = "1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21"


def minimal_path(t, extra=()):
    """A PATH with only the commands the scripts' checks use: no cargo, forge or r0vm."""
    b = os.path.join(t, "minbin")
    os.makedirs(b, exist_ok=True)
    for c in ("bash", "sh", "git", "python3", "sha256sum", "date", "cat", "tee", "mkdir", "dirname", "cut", "grep", "sed",
              "find", "sort", "xargs", "tail", "head", "ls", "tr", "wc", "env", "rm", "cp", *extra):
        p = shutil.which(c)
        if p and not os.path.exists(os.path.join(b, c)):
            os.symlink(p, os.path.join(b, c))
    return b


def git(cwd, *args):
    return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false",
                           *args], cwd=cwd, capture_output=True, text=True, check=True).stdout.strip()


class Commit(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.r = os.path.join(self.tmp.name, "tree")
        os.makedirs(os.path.join(self.r, "scripts"))
        shutil.copy(os.path.join(HERE, "release.sh"), os.path.join(self.r, "scripts", "release.sh"))
        with open(os.path.join(self.r, ".gitignore"), "w") as f:
            f.write("target/\n")
        git(self.r, "init", "-q")
        git(self.r, "add", ".")
        git(self.r, "commit", "-q", "-m", "x")
        self.head = git(self.r, "rev-parse", "HEAD")

    def tearDown(self):
        self.tmp.cleanup()

    def commit(self, env=None):
        e = {k: v for k, v in os.environ.items() if k != "FERRATA_COMMIT"}
        e.update(env or {})
        return subprocess.run(["bash", os.path.join(self.r, "scripts", "release.sh"), "commit"], capture_output=True,
                              text=True, env=e).stdout.strip()

    def test_clean_modified_untracked_ignored(self):
        self.assertEqual(self.commit(), self.head)
        with open(os.path.join(self.r, ".gitignore"), "a") as f:
            f.write("# changed\n")
        self.assertEqual(self.commit(), self.head + "+dirty")
        git(self.r, "checkout", "--", ".gitignore")
        os.makedirs(os.path.join(self.r, "src", "bin"))
        with open(os.path.join(self.r, "src", "bin", "extra.rs"), "w") as f:
            f.write("fn main() {}\n")          # untracked: cargo would build it
        self.assertEqual(self.commit(), self.head + "+dirty")
        shutil.rmtree(os.path.join(self.r, "src"))
        os.makedirs(os.path.join(self.r, "target"))
        with open(os.path.join(self.r, "target", "x"), "w") as f:
            f.write("ignored\n")
        self.assertEqual(self.commit(), self.head)

    def test_ferrata_commit_and_no_git(self):
        self.assertEqual(self.commit({"FERRATA_COMMIT": "abc+dirty"}), "abc+dirty")
        shutil.rmtree(os.path.join(self.r, ".git"))
        self.assertEqual(self.commit({"GIT_CEILING_DIRECTORIES": self.tmp.name}), "unknown")


class ReproduceUpstream(unittest.TestCase):
    """reproduce-image-ids.sh build in a copy laid out as tree/ beside upstream/ and risc0-home-lez/."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = self.t = self.tmp.name
        os.makedirs(os.path.join(t, "tree", "scripts"))
        shutil.copy(os.path.join(HERE, "reproduce-image-ids.sh"), os.path.join(t, "tree", "scripts"))
        os.makedirs(os.path.join(t, "risc0-home-lez"))
        self.path = minimal_path(t)

    def tearDown(self):
        self.tmp.cleanup()

    def build(self):
        return subprocess.run(["bash", os.path.join(self.t, "tree", "scripts", "reproduce-image-ids.sh"), "build", "--out",
                               os.path.join(self.t, "out")], capture_output=True, text=True,
                              env={"PATH": self.path, "HOME": self.t, "GIT_CEILING_DIRECTORIES": self.t})

    def test_missing_upstream(self):
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("missing", r.stdout)

    def test_upstream_not_a_clone(self):
        os.makedirs(os.path.join(self.t, "upstream", "lez-v0.4.0-rc1"))
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("cannot read the commit", r.stdout)

    def test_upstream_at_another_commit(self):
        u = os.path.join(self.t, "upstream", "lez-v0.4.0-rc1")
        os.makedirs(u)
        git(u, "init", "-q")
        with open(os.path.join(u, "README.md"), "w") as f:
            f.write("not LEZ v0.4.0-rc1\n")
        git(u, "add", ".")
        git(u, "commit", "-q", "-m", "x")
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn(f"is at {git(u, 'rev-parse', 'HEAD')}, not {LEZ_COMMIT}", r.stdout)
        self.assertFalse(os.path.exists(os.path.join(self.t, "out", "logs", "lez-methods.log")))  # nothing was built


class ReleaseBuildNative(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.t = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def test_corrupted_cached_archive_stops_the_build(self):
        spec = importlib.util.spec_from_file_location("fetch_native", os.path.join(HERE, "fetch_native.py"))
        fn = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fn)
        pins = fn.load_pins(fn.PINS)
        cache = os.path.join(self.t, "native")
        arc = fn.archive_path(cache, pins["archives"][0])
        os.makedirs(os.path.dirname(arc))
        with open(arc, "wb") as f:              # the pinned size, other bytes: a replaced asset in the cache
            f.write(b"\0" * pins["archives"][0]["bytes"])
        out = os.path.join(self.t, "rel")
        r = subprocess.run(["bash", os.path.join(HERE, "release.sh"), "build", out], capture_output=True, text=True,
                           env={"PATH": minimal_path(self.t), "HOME": self.t, "FERRATA_NATIVE_DIR": cache,
                                "FERRATA_COMMIT": "test"}, timeout=120)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("corrupted or was replaced; it was not used", r.stderr)
        with open(os.path.join(out, "build", "logs", "steps.txt")) as f:
            steps = f.read()
        self.assertIn("native archives FAILED", steps)
        for log in ("guests.log", "vault.log", "cli.log"):     # nothing was started
            self.assertFalse(os.path.exists(os.path.join(out, "build", "logs", log)), log)


class VerifyWithoutR0vm(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        spec = importlib.util.spec_from_file_location("trm", os.path.join(HERE, "test_release_manifest.py"))
        trm = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(trm)
        rel = trm.Release("test_complete_release")
        rel.tmp, rel.out = self.tmp, self.tmp.name
        rel.d = os.path.join(rel.out, "ferrata-9.9.9")
        rel.layout(rel.d)
        rel.tar = rel.sources_tar(os.path.join(rel.out, "ferrata-9.9.9-third-party-sources.tar"))
        self.assertEqual(rel.make().returncode, 0)
        self.d = rel.d
        files = sorted(os.path.relpath(os.path.join(dp, f), self.d) for dp, _, fs in os.walk(self.d) for f in fs)
        with open(os.path.join(self.d, "SHA256SUMS"), "w") as out:
            for f in files:
                with open(os.path.join(self.d, f), "rb") as g:
                    out.write(f"{hashlib.sha256(g.read()).hexdigest()}  {f}\n")

    def tearDown(self):
        self.tmp.cleanup()

    def verify(self, *extra):
        return subprocess.run(["bash", os.path.join(HERE, "release.sh"), "verify", self.d, *extra], capture_output=True,
                              text=True, env={"PATH": minimal_path(self.tmp.name), "HOME": self.tmp.name})

    def test_says_ids_not_recomputed(self):
        r = self.verify()
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("image ids NOT recomputed", r.stdout)

    def test_strict_fails(self):
        r = self.verify("--strict")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("r0vm not found", r.stderr)


if __name__ == "__main__":
    unittest.main()
