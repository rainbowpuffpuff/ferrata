#!/usr/bin/env python3
"""Offline tamper tests for the two ADR-0046/F.13 record checks.

Each check runs on the repository, then on minimal copies containing only its declared
inputs.  One surgical alteration per copy must produce the named substantive mismatch.
All copies live below this checkout's .scratch/, never the system temporary directory.
"""
from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_f13  # noqa: E402,F401 - registers the checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-f13-tests"
NEW_FEES = "e2e/both-halves/results/f13/fees.md"
RT = "e2e/runs/f13-roundtrip-1"
BOTH = "f13-both-halves-fees"
ROUNDTRIP = "f13-roundtrip-record"


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class F13VerifyTamperTests(unittest.TestCase):
    maxDiff = 4000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in (BOTH, ROUNDTRIP)}
        if any(c.fn is None for c in cls.checks.values()):
            raise ValueError("both F.13 checks must be Python fn checks")

    def _copy(self, check, destination: Path) -> Path:
        for rel in sorted(set(check.inputs)):
            src, dst = ROOT / rel, destination / rel
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
        return destination

    def _assert_fails(self, cid: str, mutate, contains: str) -> None:
        check = self.checks[cid]
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
            root = self._copy(check, Path(td))
            mutate(root)
            result = run_check(check, Ctx(root=root))
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect tampering: {result.lines}")
        self.assertTrue(any(contains in line for line in result.lines),
                        f"{cid}: no failure line names {contains!r}: {result.lines}")

    def test_01_unaltered_repository_passes(self):
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = run_check(check, Ctx(root=ROOT))
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")

    def test_02_both_halves_claim_relay_amount_fails(self):
        old = "| 1 | 0 | 1000000000000000 | 6 | 5 |"
        new = "| 1 | 0 | 1000000000000001 | 6 | 5 |"
        self._assert_fails(BOTH, lambda root: _replace(root / NEW_FEES, old, new), "relay fees")

    def test_03_both_halves_release_amount_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "Each recipient received 997000000000000000 wei", "Each recipient received 997000000000000001 wei"),
            "recipient release arithmetic")

    def test_04_both_halves_plaintext_set_fails(self):
        old = "| 3 | 1 | 1000000000000000 | 6 | 3 |"
        new = "| 3 | 1 | 1000000000000000 | 6 | 4 |"
        self._assert_fails(BOTH, lambda root: _replace(root / NEW_FEES, old, new), "plaintext set")

    def test_05_both_halves_no_mint_fee_statement_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "mints the same fixed relay fee and no mint fee", "mints the same fixed relay fee and a mint fee"),
            "no-mint-fee")

    def test_06_both_halves_notice_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "in force at 1796345771 (seven days and 1 s later)", "in force at 1796345772 (seven days and 1 s later)"),
            "notice is")

    def test_07_roundtrip_raw_claim_amount_fails(self):
        rel = f"{RT}/run/user/09c-recover.txt"
        old = '"account":0,"amount":"2000000000000000"'
        new = '"account":0,"amount":"2000000000000001"'
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "destination delta")

    def test_08_roundtrip_facts_escrow_delta_fails(self):
        rel = f"{RT}/run/facts.json"
        old = '"mintFeeWei": "0",\n      "escrowDeltaWei": "1002000000000000"'
        new = '"mintFeeWei": "0",\n      "escrowDeltaWei": "1002000000000001"'
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "escrow delta")

    def test_09_roundtrip_facts_owed_delta_fails(self):
        rel = f"{RT}/run/facts.json"
        old = '"feeRecipientCreditDeltaWei": "5000000000000"'
        new = '"feeRecipientCreditDeltaWei": "5000000000001"'
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "owed delta")

    def test_10_roundtrip_step_verdict_fails(self):
        rel = f"{RT}/run/steps.tsv"
        old = "claim of leaf 1 (schedule 1)\tok\t"
        new = "claim of leaf 1 (schedule 1)\tFAILED\t"
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "17 well-formed ok rows")

    def test_11_roundtrip_f13_doc_release_figure_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/F13.md", "the release paid 997,000,000,000,000 wei", "the release paid 998,000,000,000,000 wei"),
            "F13.md")

    def test_12_roundtrip_raw_deposit_mint_fee_fails(self):
        rel = f"{RT}/run/user/04-deposit.txt"
        old = "mint fee      0 ETH (0 base units, 18 decimals)"
        new = "mint fee      0 ETH (1 base units, 18 decimals)"
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "CLI mint fee")

    def test_13_roundtrip_step_owed_delta_fails(self):
        rel = f"{RT}/run/steps.tsv"
        old = "credited 5000000000000 wei at the deposit (OwedCredited)"
        new = "credited 5000000000001 wei at the deposit (OwedCredited)"
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "fee-recipient credit")

    def test_14_roundtrip_documented_image_id_fails(self):
        old = "| `0xffb47f6eaec6ef13f49505e5e80209f73d38ea53792e77656994f8069dde7675` |"
        new = "| `0xffb47f6eaec6ef13f49505e5e80209f73d38ea53792e77656994f8069dde7676` |"
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / "docs/image-ids.md", old, new), "image id")

    def test_15_roundtrip_owed_credited_summary_fails(self):
        rel = f"{RT}/run/facts.json"
        old = '"owedCreditedEventsWei": "5000000000000"'
        new = '"owedCreditedEventsWei": "5000000000001"'
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "OwedCredited summary")

    def test_16_roundtrip_lez_claim_verdict_fails(self):
        rel = f"{RT}/run/lez/transactions.tsv"
        old = "ec31a6ad3cce52d57ee2cd3bd74b297108ca7a097b5e093dce879063767011d2\tpublic\tfalse\tbridge\tClaim"
        new = "ec31a6ad3cce52d57ee2cd3bd74b297108ca7a097b5e093dce879063767011d2\tpublic\ttrue\tbridge\tClaim"
        self._assert_fails(ROUNDTRIP, lambda root: _replace(root / rel, old, new), "LEZ ledger")


if __name__ == "__main__":
    unittest.main(verbosity=2)
