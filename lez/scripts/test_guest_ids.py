#!/usr/bin/env python3
"""Offline tests of lez/scripts/guest-ids.sh (it builds every lez/ guest against a LEZ tag and writes their ids):
its dry run names every methods crate of lez/ that builds guests, and nothing else, and it refuses bad arguments.
Nothing is built.

    python3 -I lez/scripts/test_guest_ids.py
"""
import re
import subprocess
import unittest
from pathlib import Path

LEZ = Path(__file__).resolve().parents[1]
SCRIPT = LEZ / "scripts" / "guest-ids.sh"


def run(*args):
    return subprocess.run(["bash", str(SCRIPT), *args], capture_output=True, text=True)


def methods_crates() -> set:
    out = set()
    for manifest in LEZ.glob("methods*/Cargo.toml"):
        text = manifest.read_text()
        if "[package.metadata.risc0]" in text:
            out.add(re.search(r'^name = "([^"]+)"', text, re.M).group(1))
    return out


class GuestIds(unittest.TestCase):
    def test_dry_run_builds_every_methods_crate(self):
        p = run("--tag", "v0.4.0", "--out", "DIR", "--dry-run")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("tag: v0.4.0", p.stdout)
        listed = set(re.search(r"^methods: (.*)$", p.stdout, re.M).group(1).split())
        self.assertEqual(listed, methods_crates())
        self.assertGreaterEqual(len(listed), 7)
        command = re.search(r"^command: (.*)$", p.stdout, re.M).group(1)
        self.assertIn("cargo build --locked", command)
        for m in listed:
            self.assertIn(f"-p {m}", command)
        self.assertIn("CARGO_TARGET_DIR=DIR/target", command)

    def test_refusals(self):
        self.assertEqual(run("--tag", "v0.4.0").returncode, 2)
        self.assertEqual(run("--out", "DIR").returncode, 2)
        p = run("--tag", "v0.4.0", "--out", "DIR", "--bogus")
        self.assertEqual(p.returncode, 2)
        self.assertIn("unknown argument --bogus", p.stderr)


if __name__ == "__main__":
    unittest.main()
