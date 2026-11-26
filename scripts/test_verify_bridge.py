#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_bridge (offline, standard library only).

Every bridge record check is run against the real repository, then against minimal copies (under $RUNNER_TEMP or
.scratch/, never /tmp) with one case verdict or code, one table number, and one status-row figure altered.  Copies
contain only the files each check reads, at their original relative paths.

`bridge-import-claim-costs` fails on the repository as it is: RFP-022 P.2 states B1's harness import range as
231,635–237,140, but the B1 record's four applied `ImportRoot` cases run to 237,169 (vault B's 8-leaf import).  As
with `lc-stage4-attest-costs` in scripts/test_verify_lc.py, the tampered copies start from the row with that one
figure corrected, so that each tamper still has to be what fails; the check is run unfolded too, and must fail on
exactly that.

Two rows state no number (U.6, Priv.5); their status tamper alters the wording the check holds them to.
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

from verifylib import checks_bridge  # noqa: E402 - registers this module's checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = Path(os.environ.get("RUNNER_TEMP") or ROOT / ".scratch") / "verify-bridge-tests"
B1 = "lez/results/b1"
PRIVACY = "docs/privacy"
LIVE = "e2e/runs/live-roundtrip-2026-11-24"
HALVES = "e2e/both-halves/results"
TENANT = f"{B1}/tenant"
MINIAPP = "modules/ferrata_bridge_ui"

CHECK_IDS = (
    "bridge-privacy",
    "bridge-both-halves",
    "bridge-import-claim-costs",
    "bridge-vault-costs",
    "bridge-state-projections",
    "bridge-u8-refusals",
    "bridge-core-cases",
    "bridge-controls-depth-listing",
    "bridge-tenant-separation",
    "bridge-refusals-privacy",
    "bridge-r12-module-flows",
    "bridge-u6-private-default",
)

# (check id, relative path, exact old text, replacement).  These are single surgical edits, not regular
# expressions that could silently rewrite several cases.
VERDICT_OR_CODE = {
    "bridge-privacy": (f"{PRIVACY}/real/generated/summary.md",
                       "| claim, private (default) | 231,950-231,950 | yes |",
                       "| claim, private (default) | 231,950-231,950 | no |"),
    "bridge-both-halves": (f"{HALVES}/migration.md", "ImportRoot after it refused (242)",
                           "ImportRoot after it refused (243)"),
    "bridge-import-claim-costs": ("lez/results/bridge-inbound.md",
                                  "| 19 | Claim: the same again (replay) | bridge Claim 107,702 → bridge Issue 51,166 → "
                                  "bridge Admit 35,565 → bridge Spend 19,559 | 213,992 | 4% | 2,053 | 1,728,360 | "
                                  "refused: exit 250 (ALREADY_CLAIMED) |",
                                  "| 19 | Claim: the same again (replay) | bridge Claim 107,702 → bridge Issue 51,166 → "
                                  "bridge Admit 35,565 → bridge Spend 19,559 | 213,992 | 4% | 2,053 | 1,728,360 | "
                                  "refused: exit 251 (ALREADY_CLAIMED) |"),
    "bridge-vault-costs": (f"{LIVE}/record.json",
                           '   "block": 12196757,\n   "timestamp": 1795450260,\n   "status": 1,',
                           '   "block": 12196757,\n   "timestamp": 1795450260,\n   "status": 0,'),
    "bridge-state-projections": ("lez/results/perf-state-ethereum.txt",
                                 "Suite result: ok. 1 passed; 0 failed; 0 skipped",
                                 "Suite result: FAILED. 0 passed; 1 failed; 0 skipped"),
    "bridge-u8-refusals": ("e2e/runs/cli-u8/test.log",
                           "second claim: refused: LEZ bridge program exit code 250 ALREADY_CLAIMED",
                           "second claim: refused: LEZ bridge program exit code 251 ALREADY_CLAIMED"),
    "bridge-core-cases": (f"{B1}/bridge-inbound.md",
                           "| 19 | Claim: the same again (replay) | bridge Claim 107,796 → bridge Issue 52,567 → "
                           "bridge Admit 35,962 → bridge Spend 20,152 | 216,477 | 4% | 2,053 | 1,748,240 | "
                           "refused: exit 250 (ALREADY_CLAIMED) |",
                           "| 19 | Claim: the same again (replay) | bridge Claim 107,796 → bridge Issue 52,567 → "
                           "bridge Admit 35,962 → bridge Spend 20,152 | 216,477 | 4% | 2,053 | 1,748,240 | "
                           "refused: exit 251 (ALREADY_CLAIMED) |"),
    "bridge-controls-depth-listing": (f"{B1}/bridge-depth.md",
                                      "| 17 | SetDepth 216,000 (the maximum) by the admin | bridge SetDepth 12,913 → "
                                      "bridge SetDepth 32,060 | 44,973 | 1% | 679 | 365,216 | applied |",
                                      "| 17 | SetDepth 216,000 (the maximum) by the admin | bridge SetDepth 12,913 → "
                                      "bridge SetDepth 32,060 | 44,973 | 1% | 679 | 365,216 | "
                                      "refused: exit 240 (BAD_MESSAGE) |"),
    "bridge-tenant-separation": (f"{TENANT}/separation.txt",
                                 "| SetCap (global mints) | T's admin | refused, exit 253 (NOT_AUTHORIZED) | applied |",
                                 "| SetCap (global mints) | T's admin | refused, exit 252 (NOT_AUTHORIZED) | applied |"),
    "bridge-refusals-privacy": (f"{B1}/bridge-refusals.md", "refused before any proof (CLAIM_INVALID)",
                                "refused before any proof (ALREADY_CLAIMED)"),
    "bridge-r12-module-flows": ("modules/results/final-2026-11-13/check/bridge-e2e-dev.md",
                                "| devnet | two deposits to the user's private account /0 | yes |",
                                "| devnet | two deposits to the user's private account /0 | no |"),
    "bridge-u6-private-default": (f"{MINIAPP}/results/qmltest.txt",
                                  "PASS   : qmltestrunner::FerrataMiniApp::test_05_public_needs_an_explicit_action()",
                                  "FAIL!  : qmltestrunner::FerrataMiniApp::test_05_public_needs_an_explicit_action()"),
}

RECORD_NUMBER = {
    "bridge-privacy": (f"{PRIVACY}/real/generated/b6.md", "| deposit hides the LEZ destination | 40 | 0 |",
                      "| deposit hides the LEZ destination | 41 | 0 |"),
    "bridge-both-halves": (f"{HALVES}/randomised.md",
                           "| 1 | 60 | 19 | 3 | 12 | 0 (0) | 6 | 1 | 15 | 7 | 5 | 62 |",
                           "| 1 | 60 | 20 | 3 | 12 | 0 (0) | 6 | 1 | 15 | 7 | 5 | 62 |"),
    "bridge-import-claim-costs": ("lez/results/perf-state.md",
                                  "| ImportRoot A n=5 | 5 | 5 | 230,731 | 1,895 | 46,146 |",
                                  "| ImportRoot A n=5 | 5 | 5 | 230,732 | 1,895 | 46,146 |"),
    "bridge-vault-costs": ("vault/GAS.md", "| 30 | depositETH, the tree's first (public kind) | 559,652 | | 559,652 |",
                           "| 30 | depositETH, the tree's first (public kind) | 559,653 | | 559,652 |"),
    "bridge-state-projections": ("lez/results/perf-state.md",
                                 "| 1,000 | 3,562,355 | 165.00 | 3,670,355 | 273.00 |",
                                 "| 1,000 | 3,562,356 | 165.00 | 3,670,355 | 273.00 |"),
    "bridge-u8-refusals": (f"{MINIAPP}/results/qmltest.txt", "Totals: 27 passed, 0 failed", "Totals: 26 passed, 0 failed"),
    "bridge-core-cases": (f"{B1}/bridge-private-claim.md", "| 556,901 | 224,063 | 6,247,712 | 8,937 | 4,525 |",
                          "| 556,902 | 224,063 | 6,247,712 | 8,937 | 4,525 |"),
    "bridge-controls-depth-listing": (f"{B1}/bridge-depth.md",
                                      "| 11 | Tick A: a record at slot 9,000,096 | bridge Tick 34,956 → Read 12,268 → "
                                      "bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |",
                                      "| 11 | Tick A: a record at slot 9,000,097 | bridge Tick 34,956 → Read 12,268 → "
                                      "bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |"),
    "bridge-tenant-separation": (f"{TENANT}/r2.txt", "| 0 | 0 | applied | 261772 | 2110600 | 18800 |",
                                 "| 0 | 0 | applied | 261773 | 2110600 | 18800 |"),
    "bridge-refusals-privacy": (f"{B1}/bridge-refusals.md", "| leaf 6, its owner's claim | applied | 1 | (a mint) |",
                                "| leaf 6, its owner's claim | applied | 2 | (a mint) |"),
    "bridge-r12-module-flows": ("modules/results/final-2026-11-13/check/bridge-e2e-dev.md",
                                "24 of 24 checks passed.", "23 of 24 checks passed."),
    "bridge-u6-private-default": (f"{MINIAPP}/RESULTS.md", "**27 passed in 40.5 s**", "**28 passed in 40.5 s**"),
}

# check id: (requirement row, old fragment, altered fragment).  U.6 and Priv.5 state no number, so their tamper
# alters the wording the check holds the row to.
STATUS_NUMBER = {
    "bridge-privacy": ("Priv.1", "chance (5.0%)", "chance (5.1%)"),
    "bridge-both-halves": ("Sec.6", "141 deposits", "142 deposits"),
    "bridge-import-claim-costs": ("P.1", "265,694 execution gas", "265,695 execution gas"),
    "bridge-vault-costs": ("P.3", "732,686 gas", "732,687 gas"),
    "bridge-state-projections": ("P.7", "nullifier 165 B", "nullifier 166 B"),
    "bridge-u8-refusals": ("U.8", "9 of 9", "8 of 9"),
    "bridge-core-cases": ("F.3", "error 250", "error 251"),
    "bridge-controls-depth-listing": ("F.12", "at most 216,000 slots (240)", "at most 216,001 slots (240)"),
    "bridge-tenant-separation": ("F.16", "same gas at 0 and 12,098", "same gas at 0 and 12,099"),
    "bridge-refusals-privacy": ("Priv.5", "no block, no fee, no public account changed", "no block and no fee changed"),
    "bridge-r12-module-flows": ("R.12", "24 of 24", "23 of 24"),
    "bridge-u6-private-default": ("U.6", "each public choice needs its own acknowledgement",
                                  "each public choice needs no acknowledgement"),
}


# RFP-022 P.2 states B1's harness import range as 231,635–237,140, but the B1 record's vault-B import costs 237,169,
# so bridge-import-claim-costs fails on the repository until the row is corrected. Its tampered copies start from the
# corrected row, so that each tamper still has to be what fails.
P2_OLD, P2_FOLDED = "231,635–237,140 on B1's", "231,635–237,169 on B1's"


def _fold_p2(root: Path) -> None:
    path = root / "docs/requirements-status.md"
    text = path.read_text(encoding="utf-8")
    if P2_OLD in text:
        path.write_text(text.replace(P2_OLD, P2_FOLDED, 1), encoding="utf-8")


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def _status_tamper(root: Path, row_id: str, old: str, new: str) -> None:
    path = root / "docs/requirements-status.md"
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.startswith("## RFP-022"))
    indices = [i for i in range(start, len(lines)) if lines[i].startswith(f"| {row_id} |")]
    if len(indices) != 1:
        raise AssertionError(f"status row {row_id}: {len(indices)} matches")
    i = indices[0]
    if lines[i].count(old) != 1:
        raise AssertionError(f"status row {row_id}: expected one {old!r}")
    lines[i] = lines[i].replace(old, new, 1)
    path.write_text("".join(lines), encoding="utf-8")


class BridgeVerifyTamperTests(unittest.TestCase):
    maxDiff = 4000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in CHECK_IDS}
        if any(c.fn is None for c in cls.checks.values()):
            raise ValueError("all bridge checks must be Python fn checks")

    def _copy(self, check, destination: Path) -> Path:
        for rel in sorted(set(check.inputs)):
            src, dst = ROOT / rel, destination / rel
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
        if check.id == "bridge-import-claim-costs":
            _fold_p2(destination)
        return destination

    def _run(self, check, root: Path):
        return run_check(check, Ctx(root=root))

    def _tampered_result(self, cid: str, mutate) -> tuple:
        check = self.checks[cid]
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
            root = self._copy(check, Path(td))
            mutate(root)
            return run_check(check, Ctx(root=root))

    def _assert_fails(self, cid: str, mutate) -> None:
        result = self._tampered_result(cid, mutate)
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect tampering: {result.lines}")
        self.assertTrue(result.lines)

    def test_01_unaltered_repository_passes(self):
        p2_unfolded = P2_OLD in (ROOT / "docs/requirements-status.md").read_text(encoding="utf-8")
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = self._run(check, ROOT)
                if cid == "bridge-import-claim-costs" and p2_unfolded:
                    # The row's stated B1 range does not cover its record's vault-B import: the check must fail on it.
                    self.assertEqual(result.status, "FAIL", f"{cid}: {result.lines}")
                    self.assertTrue(any("231,635–237,140" in ln for ln in result.lines), result.lines)
                    with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
                        folded = run_check(check, Ctx(root=self._copy(check, Path(td))))
                    self.assertEqual(folded.status, "PASS", f"{cid} with P.2 folded: {folded.lines}")
                    continue
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")

    def test_02_case_verdict_or_code_tamper_fails(self):
        for cid, spec in VERDICT_OR_CODE.items():
            with self.subTest(check=cid):
                rel, old, new = spec
                self._assert_fails(cid, lambda root, rel=rel, old=old, new=new: _replace(root / rel, old, new))

    def test_03_record_table_number_tamper_fails(self):
        for cid, spec in RECORD_NUMBER.items():
            with self.subTest(check=cid):
                rel, old, new = spec
                self._assert_fails(cid, lambda root, rel=rel, old=old, new=new: _replace(root / rel, old, new))

    def test_04_status_row_number_tamper_fails(self):
        for cid, spec in STATUS_NUMBER.items():
            with self.subTest(check=cid):
                row, old, new = spec
                self._assert_fails(cid, lambda root, row=row, old=old, new=new: _status_tamper(root, row, old, new))


if __name__ == "__main__":
    unittest.main(verbosity=2)
