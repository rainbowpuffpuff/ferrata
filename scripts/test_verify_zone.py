#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_zone (offline, standard library only).

Every check runs against the real repository, then against a minimal copy containing only its
named inputs.  One raw record value, one stated README figure and one figure as the documents
state it (EVIDENCE row 266, the status rows citing it) are altered independently for each check; all such
copies must FAIL.  Copies stay under .scratch/, never the system tempdir.
"""
from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_zone  # noqa: E402 - registers this module's checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-zone-tests"
RUN = "e2e/runs/zone-bridge-rc3-multi"
README = f"{RUN}/README.md"

CHECK_IDS = (
    "zone-rc3-multi-inscriptions",
    "zone-rc3-multi-checkpoint",
    "zone-rc3-multi-refusals",
    "zone-rc3-multi-proofs",
    "zone-rc3-multi-anvil",
    "zone-rc3-multi-fork",
    "zone-rc3-multi-resources",
)

# (relative path, unique old text, replacement): one record value changed surgically.
RECORD_TAMPERS = {
    "zone-rc3-multi-inscriptions": (
        f"{RUN}/checkpoint/realblocks-crown.txt", "6462ecf1… 46", "6462ecf1… 45"),
    "zone-rc3-multi-checkpoint": (
        f"{RUN}/checkpoint/exec.json", '"user_cycles":22149862', '"user_cycles":22149863'),
    "zone-rc3-multi-refusals": (
        f"{RUN}/checkpoint/refusals/native.txt", "Core(NotAccredited)", "Core(Accredited)"),
    # A byte after the four-byte selector keeps the record well-formed enough to reach Groth16.
    "zone-rc3-multi-proofs": (
        f"{RUN}/checkpoint/receipts/seal.hex", "e71e662019bf2e64", "e71e662018bf2e64"),
    "zone-rc3-multi-anvil": (
        f"{RUN}/vault-checks.json", '"refused": "ParentNotFinal"', '"refused": "ParentIsFinal"'),
    "zone-rc3-multi-fork": (
        f"{RUN}/fork-rehearsal-31/transcript.txt", "gas 375245", "gas 375246"),
    "zone-rc3-multi-resources": (
        f"{RUN}/zone/attestor-0/resources.tsv",
        "2026-11-26T09:41:34Z\t0.21\t0.08\t0.09\t18460\t2639089\t0",
        "2026-11-26T09:41:34Z\t0.21\t0.08\t0.09\t18460\t2639090\t0"),
}

# A stated figure/text in the run README, changed without touching the raw records.
README_TAMPERS = {
    "zone-rc3-multi-inscriptions": ("46, 46, 38 and 35", "46, 46, 38 and 34"),
    "zone-rc3-multi-checkpoint": ("22,149,862 user cycles", "22,149,863 user cycles"),
    "zone-rc3-multi-refusals": ("`NotAccredited`", "`Accredited`"),
    # The README omits the three durations; seal size is a stated proof figure it does contain.
    "zone-rc3-multi-proofs": ("260-byte Groth16 seal", "259-byte Groth16 seal"),
    "zone-rc3-multi-anvil": ("999,000,000,000,000 wei each", "999,000,000,000,001 wei each"),
    "zone-rc3-multi-fork": ("375,245 gas", "375,246 gas"),
    # Likewise, the resource summaries are absent; the key range states how many attestors ran.
    "zone-rc3-multi-resources": ("zone/attestor-{0..3}/", "zone/attestor-{0..2}/"),
}


# A figure as the documents state it (EVIDENCE row 266, the status rows that cite it), changed in the document alone.
DOC_TAMPERS = {
    "zone-rc3-multi-inscriptions": ("docs/EVIDENCE.md", "by 4 keys (46/46/38/35)", "by 4 keys (46/46/38/36)"),
    "zone-rc3-multi-checkpoint": ("docs/EVIDENCE.md", "22,149,862 user cycles", "22,149,863 user cycles"),
    "zone-rc3-multi-refusals": ("docs/requirements-status.md", "one attestor twice and a stale round refused",
                                "one attestor twice and a stale tip refused"),
    "zone-rc3-multi-proofs": ("docs/requirements-status.md", "killed (SIGKILL) 8 s after it started",
                              "killed (SIGKILL) 9 s after it started"),
    "zone-rc3-multi-anvil": ("docs/EVIDENCE.md", "each release paid 999,000,000,000,000 wei once",
                             "each release paid 999,000,000,000,001 wei once"),
    "zone-rc3-multi-fork": ("docs/EVIDENCE.md", "checkpoint 375,245 gas", "checkpoint 375,246 gas"),
    "zone-rc3-multi-resources": ("docs/requirements-status.md", "0.17 CPU s and 0.19 s wall",
                                 "0.17 CPU s and 0.2 s wall"),
}

ROWS = {
    "zone-rc3-multi-inscriptions": ("RFP-022 F.7",),
    "zone-rc3-multi-checkpoint": ("RFP-022 F.7",),
    "zone-rc3-multi-refusals": ("RFP-022 F.7",),
    "zone-rc3-multi-proofs": ("RFP-022 F.7", "RFP-022 R.6"),
    "zone-rc3-multi-anvil": ("RFP-022 F.7", "RFP-022 Sec.6"),
    "zone-rc3-multi-fork": ("RFP-022 F.7",),
    "zone-rc3-multi-resources": ("RFP-022 P.6",),
}


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class ZoneBridgeVerifyTamperTests(unittest.TestCase):
    maxDiff = 5000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in CHECK_IDS}
        if any(check.fn is None for check in cls.checks.values()):
            raise ValueError("all zone record checks must be Python fn checks")

    def _copy(self, check, destination: Path) -> Path:
        for rel in sorted(set(check.inputs)):
            source = ROOT / rel
            target = destination / rel
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
        return destination

    def _assert_fails(self, cid: str, spec) -> None:
        rel, old, new = spec
        check = self.checks[cid]
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as temporary:
            root = self._copy(check, Path(temporary))
            _replace(root / rel, old, new)
            result = run_check(check, Ctx(root=root))
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect tampering: {result.lines}")
        self.assertTrue(result.lines)

    def test_01_every_check_passes_on_unaltered_repository(self):
        results = {}
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = run_check(check, Ctx(root=ROOT))
                results[cid] = result
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")
        self.assertTrue(any("LIMIT:" in line and "343,581" in line
                            for line in results["zone-rc3-multi-anvil"].lines))
        self.assertTrue(any("LIMIT:" in line and "249.7" in line
                            for line in results["zone-rc3-multi-proofs"].lines))
        self.assertTrue(any("LIMIT:" in line and "35,714" not in line and "README" in line
                            for line in results["zone-rc3-multi-resources"].lines))

    def test_02_raw_record_tamper_fails_every_check(self):
        for cid, spec in RECORD_TAMPERS.items():
            with self.subTest(check=cid, path=spec[0]):
                self._assert_fails(cid, spec)

    def test_03_readme_figure_tamper_fails_every_check(self):
        for cid, (old, new) in README_TAMPERS.items():
            with self.subTest(check=cid):
                self._assert_fails(cid, (README, old, new))

    def test_05_document_figure_tamper_fails_every_check(self):
        """The figures the documents state are the records': a document's figure changed alone fails its check."""
        for cid, spec in DOC_TAMPERS.items():
            with self.subTest(check=cid, path=spec[0]):
                self._assert_fails(cid, spec)

    def test_06_the_kill_time_is_read_from_the_prover_summary(self):
        """The documents' "killed 8 s after it started" is the summary's two clock times, 10:08:36 and 10:08:44."""
        self._assert_fails("zone-rc3-multi-proofs", (f"{RUN}/prover/summary.txt", "[10:08:44] client 1 killed",
                                                     "[10:08:45] client 1 killed"))

    def test_07_a_missing_document_row_fails(self):
        """EVIDENCE row 266 renumbered away, or the P.6 status row gone: the checks that read them fail."""
        self._assert_fails("zone-rc3-multi-anvil", ("docs/EVIDENCE.md", "| 266 | ", "| 299 | "))
        self._assert_fails("zone-rc3-multi-resources", ("docs/requirements-status.md", "| P.6 | in part | Provers,",
                                                        "| P.6x | in part | Provers,"))

    def test_04_ids_inputs_and_evidence_metadata(self):
        self.assertEqual(set(self.checks), set(CHECK_IDS))
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                self.assertTrue(cid.startswith("zone-rc3-multi-"))
                self.assertEqual(check.evidence, (266,))
                self.assertEqual(check.rows, ROWS[cid])
                self.assertEqual(check.mode, "offline")
                self.assertEqual(check.tier, "quick")
                self.assertTrue(check.title and check.what and check.against)
                self.assertTrue(check.inputs)
                for rel in check.inputs:
                    path = Path(rel)
                    self.assertFalse(path.is_absolute(), rel)
                    self.assertNotIn("..", path.parts, rel)
                    self.assertTrue((ROOT / rel).exists(), f"{cid}: missing input {rel}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
