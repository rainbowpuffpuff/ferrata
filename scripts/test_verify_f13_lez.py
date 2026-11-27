#!/usr/bin/env python3
"""Offline tamper tests for the three ADR-0048/F.13 LEZ-fee record checks.

Each check runs on the repository's committed records, then on minimal copies containing only its declared
inputs.  One surgical alteration per copy must produce the named substantive mismatch.  All copies live
below this checkout's .scratch/, never the system temporary directory.
"""
from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_f13_lez  # noqa: E402,F401 - registers the checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-f13-lez-tests"
NEW_FEES = "e2e/both-halves/results/f13-lez/fees.md"
RT = "e2e/runs/f13-lezfee-1"
PROOFS = "lez/results/f13/prove-bridge-lezfee.md"
BOTH = "f13-lez-both-halves"
ROUNDTRIP = "f13-lez-roundtrip"
PROVE = "f13-lez-prove"


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def _replace_first(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    at = text.find(old)
    if at < 0:
        raise AssertionError(f"tamper target {path}: no {old!r}")
    path.write_text(text[:at] + new + text[at + len(old):], encoding="utf-8")


class F13LezVerifyTamperTests(unittest.TestCase):
    maxDiff = 4000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in (BOTH, ROUNDTRIP, PROVE)}
        if any(c.fn is None for c in cls.checks.values()):
            raise ValueError("the F.13-lez checks must be Python fn checks")

    def _copy(self, check, destination: Path) -> Path:
        """A minimal copy: only the check's declared inputs."""
        for rel in sorted(set(check.inputs)):
            src = ROOT / rel
            dst = destination / rel
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

    def test_01_repository_and_minimal_copies_pass(self):
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = run_check(check, Ctx(root=ROOT))
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")
                with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
                    copy = run_check(check, Ctx(root=self._copy(check, Path(td))))
                self.assertEqual(copy.status, "PASS", f"{cid} on its declared inputs: {copy.lines}")

    # ---- f13-lez-both-halves ---------------------------------------------------------------------------------------

    def test_02_both_halves_mint_fee_figure_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES,
            "mint [500000000000000, 5000000000000000, 50000000000000000]",
            "mint [500000000000000, 5000000000000001, 50000000000000000]"), "mint[1] is not 0.5%")

    def test_03_both_halves_claim_relay_fee_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "| 1 | 0 | 1000000000000000 | 6 | 5 |", "| 1 | 0 | 1000000000000001 | 6 | 5 |"),
            "registered relay fees")

    def test_04_both_halves_pre_claim_set_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "| 2 | 0 | 1000000000000000 | 6 | 4 |", "| 2 | 0 | 1000000000000000 | 5 | 4 |"),
            "pre-claim set")

    def test_05_both_halves_plaintext_set_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "| 3 | 1 | 1000000000000000 | 6 | 3 |", "| 3 | 1 | 1000000000000000 | 6 | 4 |"),
            "plaintext set")

    def test_06_both_halves_first_collection_amount_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES,
            "minted 15000000000000000 wei, the vault's `lezFees` then (15000000000000000: the three",
            "minted 15000000000000001 wei, the vault's `lezFees` then (15000000000000000: the three"),
            "the first collection")

    def test_07_both_halves_total_owed_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "`totalOwed` rose by 3000000000000000 wei over 3 releases",
            "`totalOwed` rose by 3000000000000001 wei over 3 releases"), "totalOwed")

    def test_08_both_halves_older_record_code_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "the earlier record's smaller total was then refused (247)",
            "the earlier record's smaller total was then refused (246)"), "older record")

    def test_09_both_halves_fees_counted_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "`fees(cid, ETH)` counts 21000000000000000 wei collected",
            "`fees(cid, ETH)` counts 21000000000000001 wei collected"), "both collections' sum")

    def test_10_both_halves_final_supply_fails(self):
        self._assert_fails(BOTH, lambda root: _replace(
            root / NEW_FEES, "the wrapped supply on LEZ, 3027000000000000000 wei, is the vault's escrow, 3027000000000000000 wei",
            "the wrapped supply on LEZ, 3027000000000000000 wei, is the vault's escrow, 3027000000000000001 wei"),
            "final wrapped supply")

    # ---- f13-lez-roundtrip ----------------------------------------------------------------------------------------

    def test_11_roundtrip_step_verdict_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/steps.tsv", "mint fees collected\tok\t", "mint fees collected\tFAILED\t"),
            "23 well-formed ok rows")

    def test_12_roundtrip_facts_image_id_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/facts.json",
            '"imageId": "0x9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321"',
            '"imageId": "0x9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa320"'),
            "image id: docs/image-ids.md has no")

    def test_13_roundtrip_facts_mint_fee_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/facts.json", '"mintFeeWei": "5000000000000"', '"mintFeeWei": "5000000000001"'),
            "CLI mint fee")

    def test_14_roundtrip_lez_fees_accrued_event_total_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/facts.json", '"lezFeesAccruedEventsWei": "5000000000000"',
            '"lezFeesAccruedEventsWei": "5000000000001"'), "is not the mint fee")

    def test_15_roundtrip_collection_amount_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/lez/17-collect-mint.json", '"amount":"5000000000000"',
            '"amount":"5000000000001"'), "collection 1: minted")

    def test_16_roundtrip_collection_ledger_reverted_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace_first(
            root / f"{RT}/run/lez/transactions.tsv", "false\tbridge\tCollectFees", "true\tbridge\tCollectFees"),
            "collection 1: the LEZ ledger does not show this CollectFees applied")

    def test_17_roundtrip_conservation_figures_fail(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/facts.json", '"escrowWei": "1011000000000000"', '"escrowWei": "1011000000000001"'),
            "conservation:")

    def test_18_roundtrip_recovery_destination_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/user/09c-recover.txt", '"account":0,"amount":"2000000000000000"',
            '"account":0,"amount":"2000000000000001"'), "destination delta")

    def test_19_roundtrip_release_amount_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/facts.json", '"amountWei": "1000000000000000"', '"amountWei": "1000000000000001"'),
            "release: amount")

    def test_20_roundtrip_same_record_refusal_code_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/lez/18-collect-again.json", '"exit":244', '"exit":245'),
            "the same collection again: not refused by the program with 244")

    def test_21_roundtrip_deposit_escrow_delta_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/facts.json", '"escrowDeltaWei": "1007000000000000"',
            '"escrowDeltaWei": "1007000000000001"'), "escrow delta")

    def test_21b_roundtrip_older_record_minting_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/lez/19b-collect-older.json", '"collectedAfter":"7000000000000"',
            '"collectedAfter":"7000000000001"'), "the older record: something was minted or counted")

    def test_21c_roundtrip_older_record_code_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/lez/19b-collect-older.json", '"exit":247', '"exit":244'),
            "the older record: not refused by the program with 247")

    def test_21d_roundtrip_fee_recipient_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/lez/19-collect-burn.json",
            '"feeRecipient":"0x15145aee2e6c9c57d2847b8ca2e100937f11ee76fdfd75fcb588488aa2064547"',
            '"feeRecipient":"0x15145aee2e6c9c57d2847b8ca2e100937f11ee76fdfd75fcb588488aa2064548"'),
            "collection 2: the fee recipient")

    def test_21e_roundtrip_burn_fee_record_slot_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace(
            root / f"{RT}/run/lez/19-collect-burn.json", '"recordSlot":11665664', '"recordSlot":11665600'),
            "does not read the record made after the release")

    def test_21f_roundtrip_refused_collection_ledger_fails(self):
        self._assert_fails(ROUNDTRIP, lambda root: _replace_first(
            root / f"{RT}/run/lez/transactions.tsv", "true\tbridge\tCollectFees", "false\tbridge\tCollectFees"),
            "the same collection again: the LEZ ledger does not show this CollectFees reverted")

    # ---- f13-lez-prove --------------------------------------------------------------------------------------------

    def test_22_prove_record_image_id_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS,
            "bridge image 9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321 (624204 ELF bytes)",
            "bridge image 9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa320 (624204 ELF bytes)"),
            "the record's bridge image differs")

    def test_23_prove_documented_image_id_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / "docs/image-ids.md",
            "| `0x9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321` |",
            "| `0x9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa320` |"),
            "the record's bridge image differs from docs/image-ids.md's")

    def test_24_prove_dev_mode_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "RISC0_DEV_MODE off; shape wallet", "RISC0_DEV_MODE on; shape wallet"),
            "RISC0_DEV_MODE is not off")

    def test_25_prove_claim_not_a_real_proof_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "private claim: REAL PROOF: execute_and_prove 298.2 s",
            "private claim: DRY RUN, NOT A PROOF (RISC0_DEV_MODE=1): execute_and_prove 298.2 s"),
            "prove private claim")

    def test_26_prove_burn_not_a_real_proof_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "private burn: REAL PROOF: execute_and_prove 252.1 s",
            "private burn: DRY RUN, NOT A PROOF (RISC0_DEV_MODE=1): execute_and_prove 252.1 s"),
            "prove private burn")

    def test_27_prove_claim_gas_figure_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "execution gas 556,655 (PRIVATE_VERIFY_GAS 409,764 + public part 146,891 cycles)",
            "execution gas 556,656 (PRIVATE_VERIFY_GAS 409,764 + public part 146,891 cycles)"),
            "execution gas is not PRIVATE_VERIFY_GAS")

    def test_28_prove_claim_byte_count_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "of its 256082 transaction bytes, none holds the destination",
            "of its 256083 transaction bytes, none holds the destination"), "no-private-inputs statement")

    def test_29_prove_claim_receipt_range_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "receipt at bytes 16,478..256,082 of 256,082",
            "receipt at bytes 16,479..256,082 of 256,082"), "receipt is not at the bytes its size gives")

    def test_30_prove_burn_transaction_bytes_fail(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "receipt (succinct InnerReceipt, borsh) 239046 bytes; transaction 254966 bytes",
            "receipt (succinct InnerReceipt, borsh) 239046 bytes; transaction 254967 bytes"),
            "written transaction size differs")

    def test_31_prove_pinned_image_fails(self):
        self._assert_fails(PROVE, lambda root: _replace(
            root / PROOFS, "FERRATA_EXPECT_BRIDGE_IMAGE_ID=9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321",
            "FERRATA_EXPECT_BRIDGE_IMAGE_ID=9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa320"),
            "the command does not pin the record's bridge image")


if __name__ == "__main__":
    unittest.main(verbosity=2)
