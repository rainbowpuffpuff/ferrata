#!/usr/bin/env python3
"""Offline tamper tests for the two new RFP-022 F.7 run-record checks.

Each check passes on the repository and on minimal copies of its declared inputs.  One surgical alteration per copy
must produce a substantive comparison failure.  Temp directories are always below this checkout's .scratch/.
"""
from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_f7  # noqa: E402,F401 - registers the two checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-f7-tests"
ERC20 = "e2e/runs/zone-bridge-rc3-erc20"
ROTATION = "e2e/runs/zone-bridge-rc3-rotation"
ERC20_ID = "f7-zone-bridge-rc3-erc20"
ROTATION_ID = "f7-zone-bridge-rc3-rotation"


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class F7VerifyTamperTests(unittest.TestCase):
    maxDiff = 5000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in (ERC20_ID, ROTATION_ID)}
        # Registered with the row they back and the EVIDENCE row of their run (the fold of 27 November 2026).
        expected = {ERC20_ID: (279,), ROTATION_ID: (280,)}
        if any(c.fn is None or tuple(c.rows) != ("RFP-022 F.7",) or tuple(c.evidence) != expected[cid]
               for cid, c in cls.checks.items()):
            raise ValueError("F.7 checks must be Python quick checks naming RFP-022 F.7 and their run's EVIDENCE row")

    def _copy(self, check, destination: Path) -> Path:
        for rel in sorted(set(check.inputs)):
            src, dst = ROOT / rel, destination / rel
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
        return destination

    def _assert_fails(self, cid: str, path: str, old: str, new: str, contains: str) -> None:
        check = self.checks[cid]
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
            root = self._copy(check, Path(td))
            _replace(root / path, old, new)
            result = run_check(check, Ctx(root=root))
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect tampering: {result.lines}")
        self.assertTrue(any(contains in line for line in result.lines),
                        f"{cid}: no failure line names {contains!r}: {result.lines}")

    def test_01_unaltered_repository_passes(self):
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = run_check(check, Ctx(root=ROOT))
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")

    def test_02_erc20_paid_amount_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/vault-checks.json",
                           '"nullifier": "0xcdd5b0c9a7191f52623487f21722c58aea6896ecd090101e89c4a3afcf4f4263",\n      "paidWei": "99900000"',
                           '"nullifier": "0xcdd5b0c9a7191f52623487f21722c58aea6896ecd090101e89c4a3afcf4f4263",\n      "paidWei": "99900001"', "paid amount")

    def test_03_rotation_paid_amount_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/vault-checks.json",
                           '"nullifier": "0xf9ea7f723c93551ee7fc8d17b132c0b7ec4a89d4a3dcf008d83fedb1acacbd8e",\n      "paidWei": "999000000000000"',
                           '"nullifier": "0xf9ea7f723c93551ee7fc8d17b132c0b7ec4a89d4a3dcf008d83fedb1acacbd8e",\n      "paidWei": "999000000000001"', "paid amount")

    def test_04_erc20_guest_verdict_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/checkpoint/exec.json", '"journal_equals_native":true',
                           '"journal_equals_native":false', "journal_equals_native verdict")

    def test_05_rotation_guest_verdict_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/checkpoint/exec.json", '"journal_equals_native":true',
                           '"journal_equals_native":false', "journal_equals_native verdict")

    def test_06_erc20_attestation_count_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/checkpoint/zone-25-143.json", '"attestations": 3,',
                           '"attestations": 4,', "accepted attestation count")

    def test_07_rotation_attestation_count_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/checkpoint/zone-32-176.json", '"attestations": 4,',
                           '"attestations": 5,', "accepted attestation count")

    def test_08_erc20_native_refusal_error_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/checkpoint/refusals/native.txt", "refuse-outsider: refused: Core(NotAccredited)",
                           "refuse-outsider: refused: Core(DuplicateAttestor)", "refusal outsider: native.txt")

    def test_09_rotation_guest_refusal_error_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/checkpoint/refusals/guest.txt",
                           "refuse-repeated: native refused: Core(DuplicateAttestor); guest checkpoint refused: Core(DuplicateAttestor)",
                           "refuse-repeated: native refused: Core(DuplicateAttestor); guest checkpoint refused: Core(NotAccredited)",
                           "refusal repeated: guest/native output")

    def test_10_erc20_replay_error_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/vault-checks.json",
                           '"what": "release-0",\n      "transaction": "0x813ee5e4fc65418205d0693372dc86a908ed439acf6b8167853fb4321fc004b2",\n      "refused": "NullifierAlreadyUsed"',
                           '"what": "release-0",\n      "transaction": "0x813ee5e4fc65418205d0693372dc86a908ed439acf6b8167853fb4321fc004b2",\n      "refused": "ParentNotFinal"', "vault replay release-0")

    def test_11_rotation_prover_resume_verdict_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/prover/calls.tsv", "\t1\ttrue\ttrue\t193.8",
                           "\t1\tfalse\ttrue\t193.8", "attempts/ok/resumed")

    def test_12_erc20_readme_cycle_count_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/README.md", "22,449,087 user cycles", "22,449,088 user cycles", "README:")

    def test_13_rotation_readme_release_amount_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/README.md", "999,000,000,000,000 wei", "999,000,000,000,001 wei", "README:")

    def test_14_rotation_signer_indices_fail(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/checkpoint/zone-32-176.json", '"signer_indices": [\n        0,\n        1,\n        2\n',
                           '"signer_indices": [\n        0,\n        1,\n        3\n', "rotation: signer indices")

    def test_15_rotation_fixture_drops_an_earlier_key_fails(self):
        old = ('    "0x99273f3e47ddce773ac19173cde1fa5b11588e3b285228f0da3d33abb68166e3",\n'
               '    "0xfce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096"')
        new = ('    "0x99273f3e47ddce773ac19173cde1fa5b11588e3b285228f0da3d33abb68166e3",\n'
               '    "0x0000000000000000000000000000000000000000000000000000000000000000"')
        self._assert_fails(ROTATION_ID, "vault/test/real/fixtures/zone-bridge-rc3-rotation.json", old, new,
                           "rotation fixture")

    def test_16_erc20_schedule_fee_fails(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/identity.json", '"checkpointFees": [\n      100000,',
                           '"checkpointFees": [\n      100001,', "checkpoint fees")

    def test_17_rotation_burn_count_fails(self):
        self._assert_fails(ROTATION_ID, f"{ROTATION}/checkpoint/burns.json", '"burn_count": 2,', '"burn_count": 3,',
                           "burns: expected two")

    def test_18_erc20_readme_prover_seconds_fail(self):
        self._assert_fails(ERC20_ID, f"{ERC20}/README.md", "232.4 s", "232.5 s", "README:")


if __name__ == "__main__":
    unittest.main(verbosity=2)
