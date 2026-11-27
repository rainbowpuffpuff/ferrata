#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_sec (offline, standard library only).

Each check runs on the repository, then on minimal copies holding only the files it reads (under $RUNNER_TEMP or
.scratch/, never /tmp), each with one alteration that must make it fail for the reason the alteration targets:
a verifying-key coordinate, a control id or a selector of groth16.py, the risc0-ethereum pin, one constant of the
installed risc0-ethereum sources, an Ethereum-side lockfile on the old zkVM, LEZ moved to 3.1 with R.8's row unchanged,
and R.8's row no longer naming what LEZ locks.
"""
from __future__ import annotations

import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_sec  # noqa: E402 - registers this module's checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = Path(os.environ.get("RUNNER_TEMP") or ROOT / ".scratch") / "verify-sec-tests"
SELECTORS, SPLIT = "r8-risc0-selectors", "r8-phase-split"
G16 = "scripts/verifylib/groth16.py"
SRC = checks_sec.RISC0_SRC


def _replace(path: Path, old: str, new: str, count: int = 1) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) < count:
        raise AssertionError(f"tamper target {path}: {old!r} found {text.count(old)} times, wanted {count}")
    path.write_text(text.replace(old, new, count), encoding="utf-8")


class SecChecks(unittest.TestCase):
    maxDiff = 4000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)

    def _run(self, cid: str, root: Path):
        return run_check(REGISTRY[cid], Ctx(root=root, cache=root / ".scratch" / "cache"))

    def _copy(self, cid: str, dest: Path, extra=()) -> Path:
        for rel in sorted(set(REGISTRY[cid].inputs) | set(extra)):
            src, dst = ROOT / rel, dest / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                shutil.copy2(src, dst)
        return dest

    def _fails(self, cid: str, mutate, contains: str, extra=()):
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
            root = self._copy(cid, Path(td), extra)
            ok = self._run(cid, root)
            self.assertEqual(ok.status, "PASS", f"{cid} on the unaltered copy:\n" + "\n".join(ok.lines))
            mutate(root)
            r = self._run(cid, root)
            self.assertEqual(r.status, "FAIL", f"{cid} should fail:\n" + "\n".join(r.lines))
            self.assertTrue(any(contains in ln for ln in r.lines), f"{cid}: no line names {contains!r}:\n" +
                            "\n".join(r.lines))

    # ---- the repository ---------------------------------------------------------------------------------------------

    def test_both_pass_on_the_repository(self):
        for cid in (SELECTORS, SPLIT):
            with self.subTest(check=cid):
                r = self._run(cid, ROOT)
                self.assertEqual(r.status, "PASS", "\n".join(r.lines))

    def test_the_derivation_is_risc0s(self):
        """Both releases' selectors come out of one derivation; a single bit of any input moves the selector."""
        from verifylib import groth16
        p = groth16.PARAMS["3.1.0"]
        self.assertEqual(checks_sec.selector(p), "e71e6620")
        self.assertEqual(checks_sec.selector(groth16.PARAMS["3.0.0"]), "73c457ba")
        for name, alter in (("control root", lambda q: q.update(control_root="00" + p["control_root"][2:])),
                            ("BN254 control id", lambda q: q.update(bn254_control_id=p["bn254_control_id"][:-2] + "00")),
                            ("alpha", lambda q: q.update(alpha=(p["alpha"][0] ^ 1, p["alpha"][1]))),
                            ("an IC point", lambda q: q.update(ic=p["ic"][:5] + [(p["ic"][5][0], p["ic"][5][1] ^ 1)])),
                            ("beta's x halves swapped", lambda q: q.update(beta=(p["beta"][0][::-1], p["beta"][1])))):
            with self.subTest(altered=name):
                q = dict(p)
                alter(q)
                self.assertNotEqual(checks_sec.selector(q), "e71e6620")

    # ---- r8-risc0-selectors ------------------------------------------------------------------------------------------

    def test_a_verifying_key_coordinate(self):
        # delta's first G2 x coordinate of 3.1.0, one digit changed: the selector no longer comes out
        self._fails(SELECTORS, lambda r: _replace(r / G16, "15776672394616990683767400715354533957990327248900631669473166016258279232588",
                                                  "15776672394616990683767400715354533957990327248900631669473166016258279232589"),
                    "3.1.0: the parameters in groth16.py give selector")

    def test_a_control_root(self):
        self._fails(SELECTORS, lambda r: _replace(r / G16, '"a54dc85ac99f851c', '"a54dc85ac99f851d'),
                    "3.0.0: the parameters in groth16.py give selector")

    def test_a_recorded_selector(self):
        self._fails(SELECTORS, lambda r: _replace(r / G16, '"selector": "e71e6620"', '"selector": "e71e6621"'),
                    "3.1.0: groth16.py records selector e71e6621")

    def test_the_library_pin(self):
        self._fails(SELECTORS, lambda r: _replace(r / checks_sec.LOCK, "risc0/risc0-ethereum@v3.1.0",
                                                  "risc0/risc0-ethereum@v3.0.0"),
                    "no risc0-ethereum@v3.1.0 pin")

    @unittest.skipUnless((ROOT / SRC / "Groth16Verifier.sol").exists(), "vault/lib not installed")
    def test_one_constant_of_the_installed_sources(self):
        extra = (f"{SRC}/Groth16Verifier.sol", f"{SRC}/ControlID.sol")
        self._fails(SELECTORS, lambda r: _replace(r / SRC / "Groth16Verifier.sol", "uint256 constant IC3y = ",
                                                  "uint256 constant IC3y = 1"),
                    "1 constants differ from groth16.py's 3.1.0 (IC3y)", extra=extra)

    # ---- r8-phase-split ----------------------------------------------------------------------------------------------

    def test_an_ethereum_side_prover_on_the_old_zkvm(self):
        def mutate(r):
            _replace(r / "gpu-prover/prover/Cargo.lock", 'name = "risc0-zkvm"\nversion = "3.1.0"',
                     'name = "risc0-zkvm"\nversion = "3.0.5"')
        self._fails(SPLIT, mutate, "gpu-prover/prover/Cargo.lock: risc0-zkvm 3.0.5; phase 1 wants risc0-zkvm 3.1.0")

    def test_lez_moved_but_the_row_did_not(self):
        def mutate(r):
            for rel in checks_sec.PHASE2:
                _replace(r / rel, 'name = "risc0-zkvm"\nversion = "3.0.5"', 'name = "risc0-zkvm"\nversion = "3.1.0"')
        self._fails(SPLIT, mutate, "but R.8's row says LEZ verifies private transactions with 3.0.5: the row is stale")

    def _move_lez(self, r, to="3.1.0"):
        for rel in checks_sec.PHASE2:
            _replace(r / rel, 'name = "risc0-zkvm"\nversion = "3.0.5"', f'name = "risc0-zkvm"\nversion = "{to}"')

    def _row(self, r, old, new):
        text = (r / checks_sec.STATUS).read_text()
        row = text.find("| R.8 |", text.find("## RFP-022:"))
        end = text.find("\n", row)
        line = text[row:end]
        self.assertIn(old, line)
        (r / checks_sec.STATUS).write_text(text[:row] + line.replace(old, new) + text[end:])

    def test_lez_moved_and_the_row_names_another_old_version(self):
        """Review round 2: the row names a 3.0.x other than 3.0.5 after LEZ moved; it passed before."""
        def mutate(r):
            self._move_lez(r)
            self._row(r, "risc0-zkvm 3.0.5", "risc0-zkvm 3.0.4")
        self._fails(SPLIT, mutate, "but R.8's row says LEZ verifies private transactions with 3.0.4: the row is stale")

    def test_lez_moved_and_the_row_names_neither(self):
        def mutate(r):
            self._move_lez(r)
            self._row(r, "risc0-zkvm 3.0.5", "risc0-zkvm 4.0.0")
        self._fails(SPLIT, mutate, "but R.8's row says LEZ verifies private transactions with 4.0.0: the row is stale")

    def test_the_row_drops_phase_2_while_lez_is_on_3_0(self):
        self._fails(SPLIT, lambda r: self._row(r, "phase 2", "phase two"),
                    "R.8's row does not say that LEZ is on risc0-zkvm 3.0.5 until phase 2")

    def test_the_row_says_phase_2_is_complete_while_lez_is_on_3_0(self):
        """Final review round: 'phase 2' elsewhere in the row passed; the LEZ sentence itself must say until phase 2."""
        self._fails(SPLIT, lambda r: self._row(r, "until phase 2 moves our LEZ side to LEZ v0.4.0",
                                               "forever; phase 2 is complete"),
                    "R.8's row does not say that LEZ is on risc0-zkvm 3.0.5 until phase 2")

    def test_lez_moved_and_the_row_follows_passes(self):
        """The row rewritten for LEZ on 3.1 (phase 2 done, no 3.0.x named) passes."""
        with tempfile.TemporaryDirectory(prefix="split-follows-", dir=SCRATCH) as td:
            root = self._copy(SPLIT, Path(td))
            self._move_lez(root)
            text = (root / checks_sec.STATUS).read_text()
            row = text.find("| R.8 |", text.find("## RFP-022:"))
            end = text.find("\n", row)
            line = text[row:end].replace("risc0-zkvm 3.0.5", "risc0-zkvm 3.1.0").replace("phase 2", "the LEZ tag")
            (root / checks_sec.STATUS).write_text(text[:row] + line + text[end:])
            r = self._run(SPLIT, root)
            self.assertEqual(r.status, "PASS", "\n".join(r.lines))

    def test_one_lez_linked_workspace_moved_alone(self):
        self._fails(SPLIT, lambda r: _replace(r / "cli/Cargo.lock", 'name = "risc0-zkvm"\nversion = "3.0.5"',
                                              'name = "risc0-zkvm"\nversion = "3.1.0"'),
                    "the LEZ-linked lockfiles disagree")

    def test_the_row_no_longer_says_what_lez_locks(self):
        def mutate(r):
            text = (r / checks_sec.STATUS).read_text()
            start = text.find("## RFP-022:")
            row = text.find("| R.8 |", start)
            end = text.find("\n", row)
            line = text[row:end]
            self.assertIn("risc0-zkvm 3.0.5", line)
            (r / checks_sec.STATUS).write_text(text[:row] + line.replace("risc0-zkvm 3.0.5", "risc0-zkvm 3.0.x") +
                                               text[end:])
        self._fails(SPLIT, mutate, "R.8's row does not say which risc0-zkvm LEZ verifies private transactions with")


if __name__ == "__main__":
    unittest.main(verbosity=2)
