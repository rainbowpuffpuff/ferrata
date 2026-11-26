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
FIXTURE = "vault/test/real/fixtures/zone-bridge-rc3-multi.json"
KEY_6462 = "6462ecf103d4074bbdbed04e115233548af773d62506b30ef25a2501af597fac"  # an accredited channel key, the attestor of zone/attestor-2

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

    def test_08_the_prover_wall_time_is_the_records(self):
        """The prover's own wall time (92.7 s for the checkpoint) is read from its done record, apart from the wait."""
        check = self.checks["zone-rc3-multi-proofs"]
        with tempfile.TemporaryDirectory(prefix="wall-", dir=SCRATCH) as temporary:
            root = self._copy(check, Path(temporary))
            # the done record and the receipt's copy altered together, so only the wall-time comparison can see it
            for rel in (f"{RUN}/prover/done/0-checkpoint.json", f"{RUN}/checkpoint/receipts/prover.json"):
                text = (root / rel).read_text(encoding="utf-8")
                self.assertEqual(text.count("92.67559051513672"), 1, rel)
                (root / rel).write_text(text.replace("92.67559051513672", "93.67559051513672"), encoding="utf-8")
            result = run_check(check, Ctx(root=root))
        self.assertEqual(result.status, "FAIL", result.lines)
        self.assertTrue(any("wall time" in line for line in result.lines), result.lines)
        self._assert_fails("zone-rc3-multi-proofs", ("docs/EVIDENCE.md", "92.7 s for the checkpoint",
                                                     "249.7 s for the checkpoint, on the GPU,"))

    def test_09_the_done_record_wait_is_checked(self):
        """The done record's wait (249.7 s), altered with the receipt's copy, fails though calls.tsv is untouched."""
        check = self.checks["zone-rc3-multi-proofs"]
        with tempfile.TemporaryDirectory(prefix="wait-", dir=SCRATCH) as temporary:
            root = self._copy(check, Path(temporary))
            for rel in (f"{RUN}/prover/done/0-checkpoint.json", f"{RUN}/checkpoint/receipts/prover.json"):
                text = (root / rel).read_text(encoding="utf-8")
                self.assertEqual(text.count('"seconds": 249.7'), 1, rel)
                (root / rel).write_text(text.replace('"seconds": 249.7', '"seconds": 249.8'), encoding="utf-8")
            result = run_check(check, Ctx(root=root))
        self.assertEqual(result.status, "FAIL", result.lines)
        self.assertTrue(any("client's wait" in line for line in result.lines), result.lines)

    def test_07_a_missing_document_row_fails(self):
        """EVIDENCE row 266 renumbered away, or the P.6 status row gone: the checks that read them fail."""
        self._assert_fails("zone-rc3-multi-anvil", ("docs/EVIDENCE.md", "| 266 | ", "| 299 | "))
        self._assert_fails("zone-rc3-multi-resources", ("docs/requirements-status.md", "| P.6 | in part | Provers,",
                                                        "| P.6x | in part | Provers,"))

    def _assert_fails_multi(self, cid: str, edits, contains: str | None = None) -> None:
        """Several files altered together (rel, old, new, count): what one record alone would not show."""
        check = self.checks[cid]
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as temporary:
            root = self._copy(check, Path(temporary))
            for rel, old, new, count in edits:
                text = (root / rel).read_text(encoding="utf-8")
                self.assertEqual(text.count(old), count, f"tamper target {rel}: {old[:60]!r}")
                (root / rel).write_text(text.replace(old, new), encoding="utf-8")
            result = run_check(check, Ctx(root=root))
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect tampering: {result.lines}")
        if contains:
            self.assertTrue(any(contains in line for line in result.lines), result.lines)

    def test_10_review_round_1_experiments(self):
        """The integration's review round 1 (H1, M1-M4, L1): each of these altered records passed before its fix."""
        outsider = "1" * 64
        cases = {
            # H1: an attestation and its final.json moved to a key outside the channel, the key sets left alone
            "zone-rc3-multi-checkpoint": [(f"{RUN}/checkpoint/attestor.json", '"channel_key": "0x%s"' % KEY_6462,
                                           '"channel_key": "0x%s"' % outsider, 1),
                                          (f"{RUN}/zone/attestor-2/final.json", '"channel_key": "%s"' % KEY_6462,
                                           '"channel_key": "%s"' % outsider, 1)],
            # H1: the outsider case's key set and attestations swapped, so the "outsider" is an accredited key
            "zone-rc3-multi-refusals": [(f"{RUN}/checkpoint/refusals/zone-28-164-refuse-outsider.json", "f1564eb3", "@@", 1),
                                        (f"{RUN}/checkpoint/refusals/zone-28-164-refuse-outsider.json", "90a548e0",
                                         "f1564eb3", 1),
                                        (f"{RUN}/checkpoint/refusals/zone-28-164-refuse-outsider.json", "@@", "90a548e0",
                                         1)],
            # M1: block 3's parent no longer block 2's message
            "zone-rc3-multi-inscriptions": [(f"{RUN}/checkpoint/blocks.tsv", "\t4af4cf304867724d3e3aec24ab06fec266b18ecfe5bad519224acde4d9dc8f9a\t-", "\t" + "0" * 64 + "\t-", 1)],
            # M2: the checkpoint's answer event a failure
            "zone-rc3-multi-proofs": [(f"{RUN}/prover/events.jsonl",
                                       '"name": "0-checkpoint", "kind": "checkpoint-b", "ok": true',
                                       '"name": "0-checkpoint", "kind": "checkpoint-b", "ok": false', 1)],
            # M3: both crown requests with a relay fee their seals do not prove
            "zone-rc3-multi-anvil": [(f"{RUN}/crown.json", '"relayFee": "0x' + "0" * 64 + '"',
                                      '"relayFee": "0x' + "0" * 63 + '1"', 2)],
            # M4: both parameter files with other floors
            "zone-rc3-multi-fork": [(f"{RUN}/params.json", '"nMin": 4', '"nMin": 5', 1),
                                    (f"{RUN}/params.json", '"tMin": 3', '"tMin": 4', 1),
                                    (f"{RUN}/fork-rehearsal-31/params.json", '"nMin": 4', '"nMin": 5', 1),
                                    (f"{RUN}/fork-rehearsal-31/params.json", '"tMin": 3', '"tMin": 4', 1)],
            # L1: a round's timestamp moved onto the next one's
            "zone-rc3-multi-resources": [(f"{RUN}/zone/attestor-0/resources.tsv", "2026-11-26T09:41:34Z",
                                          "2026-11-26T09:42:34Z", 1)],
        }
        for cid, edits in cases.items():
            with self.subTest(check=cid):
                self._assert_fails_multi(cid, edits)

    def test_11_guards_alone(self):
        """Guards that other comparisons used to mask (review round 1, the surviving code mutants)."""
        seal = (ROOT / RUN / "checkpoint/receipts/seal.hex").read_text(encoding="utf-8").strip().removeprefix("0x")
        forged = seal[:200] + ("0" if seal[200] != "0" else "1") + seal[201:]
        # the checkpoint's seal altered everywhere the check reads it: only the Groth16 verification can tell
        self._assert_fails_multi("zone-rc3-multi-proofs", [
            (f"{RUN}/checkpoint/receipts/seal.hex", seal, forged, 1),
            (f"{RUN}/prover/done/0-checkpoint.json", seal, forged, 1),
            (f"{RUN}/prover/requests/done/0-checkpoint.json", seal, forged, 1),
            (FIXTURE, seal, forged, 1)], "Groth16")
        self._assert_fails_multi("zone-rc3-multi-checkpoint", [
            (f"{RUN}/checkpoint/exec.json", '"journal_equals_native":true', '"journal_equals_native":false', 1)],
            "native journal")
        self._assert_fails_multi("zone-rc3-multi-anvil", [
            (f"{RUN}/vault-checks.json", '"finalState": "0x', '"finalState": "0x00', 1)], "final state")

    def test_12_signatures_and_counts_alone(self):
        """An attestation's signature altered in both records that carry it, the outsider case's outsider made
        accredited, and the fork's floors alone changed: each caught by its own comparison."""
        import json as _json
        attestor = _json.loads((ROOT / RUN / "checkpoint/attestor.json").read_text(encoding="utf-8"))
        entry = attestor["attestations"][0]
        sig = entry["signature"].removeprefix("0x")
        bad = sig[:-2] + ("00" if sig[-2:] != "00" else "01")
        final = next(i for i in range(4) if sig in (ROOT / RUN / f"zone/attestor-{i}/final.json").read_text())
        self._assert_fails_multi("zone-rc3-multi-checkpoint", [
            (f"{RUN}/checkpoint/attestor.json", sig, bad, 1),
            (f"{RUN}/zone/attestor-{final}/final.json", sig, bad, 2)], "does not verify")  # its signature and encoding
        outsider = _json.loads((ROOT / RUN / "checkpoint/refusals/zone-28-164-refuse-outsider.json").read_text())
        live = [k.lower().removeprefix("0x") for k in outsider["keys"]]
        out_key = next(a["channel_key"] for a in outsider["attestations"]
                       if a["channel_key"].lower().removeprefix("0x") not in live)
        self._assert_fails_multi("zone-rc3-multi-refusals", [
            (f"{RUN}/checkpoint/refusals/zone-28-164-refuse-outsider.json", out_key, live[1], 1)],
            "outside the channel's set")
        self._assert_fails_multi("zone-rc3-multi-fork", [(f"{RUN}/fork-rehearsal-31/params.json", '"tMin": 3', '"tMin": 2', 1)],
                                 "tMin")
        # the collector's recorded key-set hash, which the signatures are verified under once recomputed
        self._assert_fails_multi("zone-rc3-multi-checkpoint", [
            (f"{RUN}/checkpoint/attestor.json", "a300aec501d6fa09b60964a4e85ca7fe91cd4d21f2689199176441254dd03469",
             "b300aec501d6fa09b60964a4e85ca7fe91cd4d21f2689199176441254dd03469", 1)], "key-set hash")

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
