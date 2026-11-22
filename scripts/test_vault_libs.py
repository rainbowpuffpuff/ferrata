"""Tests of scripts/vault_libs.py (review-code-ops L6): `python3 -I scripts/test_vault_libs.py`.

A library tree as pinned passes; one changed byte, an added file or a missing library fails; .git is left out of the
hash; a linked directory is part of the hash (its target changed: fails) and a link leading outside the library is
refused (review-fix-supply L-6); the repository's vault/lib.lock parses. Standard library only; files go to a
temporary directory (TMPDIR).
"""

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "vault_libs.py")
_spec = importlib.util.spec_from_file_location("vault_libs", SCRIPT)
vl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vl)


class VaultLibs(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.lib = os.path.join(self.tmp.name, "lib")
        for rel, data in (("forge-std/src/Test.sol", "contract T {}\n"), ("forge-std/README.md", "fs\n"),
                          ("risc0-ethereum/contracts/src/Verifier.sol", "contract V {}\n")):
            p = os.path.join(self.lib, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w") as f:
                f.write(data)
        self.lock = os.path.join(self.tmp.name, "lib.lock")
        with open(self.lock, "w") as f:
            f.write("# test\n")
            for d, spec in (("forge-std", "foundry-rs/forge-std@v1"), ("risc0-ethereum", "risc0/risc0-ethereum@v3")):
                f.write(f"{d} {spec} {vl.tree_hash(os.path.join(self.lib, d))[0]}\n")

    def tearDown(self):
        self.tmp.cleanup()

    def check(self):
        return subprocess.run([sys.executable, "-I", SCRIPT, "check", "--lib", self.lib, "--lock", self.lock],
                              capture_output=True, text=True)

    def test_as_pinned(self):
        r = self.check()
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout.count("as pinned"), 2)

    def test_changed_byte(self):
        with open(os.path.join(self.lib, "risc0-ethereum/contracts/src/Verifier.sol"), "w") as f:
            f.write("contract W {}\n")
        r = self.check()
        self.assertEqual(r.returncode, 1)
        self.assertIn("risc0-ethereum (risc0/risc0-ethereum@v3): tree hash", r.stdout)

    def test_linked_directory(self):
        top = os.path.join(self.lib, "forge-std")
        os.makedirs(os.path.join(top, "alt-a"))
        os.makedirs(os.path.join(top, "alt-b"))
        os.symlink("alt-a", os.path.join(top, "lib-link"))
        h1 = vl.tree_hash(top)[0]
        os.unlink(os.path.join(top, "lib-link"))
        os.symlink("alt-b", os.path.join(top, "lib-link"))           # the link's target changed, no file did
        self.assertNotEqual(vl.tree_hash(top)[0], h1)

    def test_link_leaving_the_library(self):
        outside = os.path.join(self.tmp.name, "outside")
        os.makedirs(outside)
        os.symlink(outside, os.path.join(self.lib, "forge-std", "src", "escape"))
        r = self.check()
        self.assertEqual(r.returncode, 1)
        self.assertIn("escape -> " + outside + " leads outside the library", r.stdout)
        os.unlink(os.path.join(self.lib, "forge-std", "src", "escape"))
        os.symlink("../../../outside/x.sol", os.path.join(self.lib, "forge-std", "src", "Up.sol"))
        self.assertIn("leads outside the library", self.check().stdout)

    def test_added_file(self):
        with open(os.path.join(self.lib, "forge-std/src/Extra.sol"), "w") as f:
            f.write("x\n")
        self.assertEqual(self.check().returncode, 1)

    def test_missing_library(self):
        import shutil
        shutil.rmtree(os.path.join(self.lib, "forge-std"))
        r = self.check()
        self.assertEqual(r.returncode, 1)
        self.assertIn("forge-std missing", r.stdout)

    def test_git_metadata_ignored(self):
        os.makedirs(os.path.join(self.lib, "forge-std/.git"))
        with open(os.path.join(self.lib, "forge-std/.git/HEAD"), "w") as f:
            f.write("ref: refs/heads/x\n")
        self.assertEqual(self.check().returncode, 0)

    def test_repository_lock(self):
        lock = vl.read_lock()
        self.assertEqual([d for d, _, _ in lock], ["forge-std", "openzeppelin-contracts", "risc0-ethereum"])
        for _, spec, h in lock:
            self.assertRegex(h, r"^[0-9a-f]{64}$")
            self.assertIn("@v", spec)


if __name__ == "__main__":
    unittest.main()
