#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_lc (offline, standard library only).

Every record check is run against the real repository, then against minimal copies (under $RUNNER_TEMP or .scratch/)
with one case verdict/code, one table number, and one status-row number altered.
The hash family separately flips a fixture byte and a manifest hex digit.  Copies contain
only the files each check reads, at their original relative paths.
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

from verifylib import checks_lc  # noqa: E402 - registers this module's checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = Path(os.environ.get("RUNNER_TEMP") or ROOT / ".scratch") / "verify-lc-tests"
LC = "lez/results/lc-security"
R11 = "lez/results/v0.4.0-rc3/r11-one-block"
ORACLE = "crates/ferrata-spectests/results/helios-step-oracle-2026-11-24.txt"
FIXTURES = "crates/ferrata-spectests/fixtures"

CHECK_IDS = (
    "lc-f2-finalised-only",
    "lc-f4-comparisons",
    "lc-f8-full-diff",
    "lc-u5-failure-modes",
    "lc-r3-rotations",
    "lc-vs-fabricated-forgeries",
    "lc-r11-one-block",
    "lc-r7-stall",
    "lc-stage-refusals",
    "lc-stage4-attest-costs",
    "lc-perf-state",
    "lc-helios-step-oracle",
)

# (check id, relative path, exact old text, replacement).  These are single surgical
# edits, not regular expressions that could silently rewrite several cases.
VERDICT_OR_CODE = {
    "lc-f2-finalised-only": (f"{LC}/f2-finalised-only.md", "refused, exit 50 NotFinalized", "refused, exit 51 NotFinalized"),
    "lc-f4-comparisons": (f"{LC}/f4-comparisons.md", "WETH's balance Eq balance+1 | 130 PredicateFailed | refused, exit 130", "WETH's balance Eq balance+1 | 130 PredicateFailed | refused, exit 131"),
    "lc-f8-full-diff": (f"{LC}/f8-full-diff.md", "| the fee inbox | nonce Nonce(0), 56 bytes of data | nonce Nonce(0), 56 bytes of data | yes |", "| the fee inbox | nonce Nonce(0), 56 bytes of data | nonce Nonce(0), 56 bytes of data | no |"),
    "lc-u5-failure-modes": (f"{LC}/u5-failure-modes.md", "refused, exit 27 NextCommitteeBranchInvalid", "refused, exit 28 NextCommitteeBranchInvalid"),
    "lc-r3-rotations": (f"{LC}/r3-rotations.md", "after rotation 1 (committee 1 current, committee 2 handed off) | applied, the same statements | applied |", "after rotation 1 (committee 1 current, committee 2 handed off) | applied, the same statements | refused |"),
    "lc-vs-fabricated-forgeries": (f"{LC}/vs2-forgeries.md", "an update signed by 512 keys of a committee never handed off from the checkpoint | 40 SignatureInvalid | refused, exit 40", "an update signed by 512 keys of a committee never handed off from the checkpoint | 40 SignatureInvalid | refused, exit 41"),
    "lc-r11-one-block": (f"{R11}/run.tsv", "reader 0: AttestPrivately of tx 0 log 0, proven against the snapshot\t387d309981238b6831f955971d82efad14a6e8216bef6b06724ca22ac0d385c3\t47\t0\tapplied", "reader 0: AttestPrivately of tx 0 log 0, proven against the snapshot\t387d309981238b6831f955971d82efad14a6e8216bef6b06724ca22ac0d385c3\t47\t0\trefused"),
    "lc-r7-stall": ("lez/results/r7-stall.txt", "out of order: 00137.period.gloas.ssz refused at Open with exit Some(23)", "out of order: 00137.period.gloas.ssz refused at Open with exit Some(24)"),
    "lc-stage-refusals": ("lez/results/stage1-refusals.md", "duplicate Open (same job) | Open 2,171,611 → JobOpen 16,529 | 2,188,140 | 44% | 3,525 | 17,533,320 | refused: exit 207", "duplicate Open (same job) | Open 2,171,611 → JobOpen 16,529 | 2,188,140 | 44% | 3,525 | 17,533,320 | refused: exit 208"),
    "lc-stage4-attest-costs": ("lez/results/stage4-attest.md", "| 1 | account: the WETH contract (nonce, balance, storage root, code hash) | Attest 168,909 | 168,909 | 3% | 4,424 | 1,386,664 | applied |", "| 1 | account: the WETH contract (nonce, balance, storage root, code hash) | Attest 168,909 | 168,909 | 3% | 4,424 | 1,386,664 | refused |"),
    "lc-perf-state": ("lez/results/perf-state.md", "Every statement was charged the same execution and storage gas at every K (asserted).", "Every statement was charged the same execution and storage gas at every K (not asserted)."),
    "lc-helios-step-oracle": (ORACLE, "1 valid optimistic-only NotNewer", "1 valid optimistic-only NotFinalized"),
}

RECORD_NUMBER = {
    "lc-f2-finalised-only": (f"{LC}/f2-finalised-only.md", "an update without finality (optimistic) | 50 NotFinalized", "an update without finality (optimistic) | 51 NotFinalized"),
    "lc-f4-comparisons": (f"{LC}/f4-comparisons.md", "| relation: amount of fact 0 Eq amount of fact 1 | 130", "| relation: amount of fact 0 Eq amount of fact 9 | 130"),
    "lc-f8-full-diff": (f"{LC}/f8-full-diff.md", "| the fee inbox | nonce Nonce(0), 56 bytes of data | nonce Nonce(0), 56 bytes", "| the fee inbox | nonce Nonce(0), 57 bytes of data | nonce Nonce(0), 56 bytes"),
    "lc-u5-failure-modes": (f"{LC}/u5-failure-modes.md", "an update signed by 512 keys of a committee never handed off", "an update signed by 513 keys of a committee never handed off"),
    "lc-r3-rotations": (f"{LC}/r3-rotations.md", "after rotation 4 (committee 4 current, committee 5 handed off)", "after rotation 5 (committee 4 current, committee 5 handed off)"),
    "lc-vs-fabricated-forgeries": (f"{LC}/vs2-forgeries.md", "| 341 of 512 members sign |", "| 340 of 512 members sign |"),
    "lc-r11-one-block": (f"{R11}/run.tsv", "\t387d309981238b6831f955971d82efad14a6e8216bef6b06724ca22ac0d385c3\t47\t0\tapplied\t-\t422032\t224063", "\t387d309981238b6831f955971d82efad14a6e8216bef6b06724ca22ac0d385c3\t47\t0\tapplied\t-\t422032\t224064"),
    "lc-r7-stall": ("lez/results/r7-stall.txt", "(706020 s = 8.2 days", "(7060200 s = 8.2 days"),
    "lc-stage-refusals": ("lez/results/stage1-refusals.md", "duplicate Open (same job) | Open 2,171,611 → JobOpen 16,529 | 2,188,140", "duplicate Open (same job) | Open 2,171,611 → JobOpen 16,529 | 2,188,141"),
    "lc-stage4-attest-costs": ("lez/results/stage4-attest.md", "account: the WETH contract (nonce, balance, storage root, code hash) | Attest 168,909 | 168,909", "account: the WETH contract (nonce, balance, storage root, code hash) | Attest 168,909 | 168,910"),
    "lc-perf-state": ("lez/results/perf-state.md", "| header(id, slot) record | 365 | 421 | 453 |", "| header(id, slot) record | 365 | 421 | 454 |"),
    "lc-helios-step-oracle": (ORACLE, "real-update step-path oracle: 13 bootstraps, 321 updates", "real-update step-path oracle: 13 bootstraps, 320 updates"),
}

# check id: (requirement row, old number-bearing fragment, altered fragment)
STATUS_NUMBER = {
    "lc-f2-finalised-only": ("F.2", "refused (50; 26, 1; 26; 25)", "refused (51; 26, 1; 26; 25)"),
    "lc-f4-comparisons": ("F.4", "36 cases", "35 cases"),
    "lc-f8-full-diff": ("F.8", "two private claims", "three private claims"),
    "lc-u5-failure-modes": ("U.5", "22 cases", "21 cases"),
    "lc-r3-rotations": ("R.3", "four committee rotations", "five committee rotations"),
    "lc-vs-fabricated-forgeries": ("VS.2", "341 of 512 signers", "340 of 512 signers"),
    "lc-r11-one-block": ("R.11", "four private attestations", "three private attestations"),
    "lc-r7-stall": ("R.7", "8.2 days", "8.3 days"),
    "lc-stage-refusals": ("R.8", "Duplicate Open (207)", "Duplicate Open (208)"),
    "lc-stage4-attest-costs": ("P.1", "97,485 to 1,619,960", "97,486 to 1,619,960"),
    "lc-perf-state": ("P.7", "record 453 B", "record 454 B"),
    "lc-helios-step-oracle": ("R.6", "all 321 recorded real", "all 320 recorded real"),
}


# RFP-021 P.1 says "at most 32%" of the 5M target; the largest case is 32.3992% (review round 1, M10), so
# lc-stage4-attest-costs fails on the repository until ROWS-PROPOSED's P.1 is folded. Its tampered copies start from
# the folded wording, so that each tamper still has to be what fails.
P1_OLD, P1_FOLDED = "at most 32% of LEZ's 5M target", "at most 32.4% of LEZ's 5M target"


def _fold_p1(root: Path) -> None:
    path = root / "docs/requirements-status.md"
    text = path.read_text(encoding="utf-8")
    if P1_OLD in text:
        path.write_text(text.replace(P1_OLD, P1_FOLDED, 1), encoding="utf-8")


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def _status_tamper(root: Path, row_id: str, old: str, new: str) -> None:
    path = root / "docs/requirements-status.md"
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.startswith("## RFP-021"))
    end = next(i for i, line in enumerate(lines) if line.startswith("## RFP-022"))
    indices = [i for i in range(start, end) if lines[i].startswith(f"| {row_id} |")]
    if len(indices) != 1:
        raise AssertionError(f"status row {row_id}: {len(indices)} matches")
    i = indices[0]
    if lines[i].count(old) != 1:
        raise AssertionError(f"status row {row_id}: expected one {old!r}")
    lines[i] = lines[i].replace(old, new, 1)
    path.write_text("".join(lines), encoding="utf-8")


class LightClientVerifyTamperTests(unittest.TestCase):
    maxDiff = 4000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in (*CHECK_IDS, "lc-fixture-hashes")}
        if any(c.fn is None for c in cls.checks.values()):
            raise ValueError("all light-client checks must be Python fn checks")

    def _copy(self, check, destination: Path) -> Path:
        inputs = set(check.inputs)
        if check.id == "lc-helios-step-oracle":
            # The checker inventories every repository .ssz, but reads no other file there.
            inputs.update(checks_lc._ssz_files(ROOT)[0])
        for rel in sorted(inputs):
            src = ROOT / rel
            dst = destination / rel
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
        if check.id == "lc-stage4-attest-costs":
            _fold_p1(destination)
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
        p1_unfolded = P1_OLD in (ROOT / "docs/requirements-status.md").read_text(encoding="utf-8")
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = self._run(check, ROOT)
                if cid == "lc-stage4-attest-costs" and p1_unfolded:
                    self.assertEqual(result.status, "FAIL", f"{cid}: {result.lines}")
                    self.assertTrue(any("P.1 says 'at most 32%" in ln for ln in result.lines), result.lines)
                    with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as td:
                        folded = run_check(check, Ctx(root=self._copy(check, Path(td))))
                    self.assertEqual(folded.status, "PASS", f"{cid} with P.1 folded: {folded.lines}")
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

    def test_05_fixture_byte_tamper_fails(self):
        rel = f"{FIXTURES}/mainnet-attest-2026-11-10/bootstrap.ssz"
        def flip(root):
            path = root / rel
            data = bytearray(path.read_bytes())
            self.assertGreater(len(data), 0)
            data[0] ^= 1
            path.write_bytes(data)
        self._assert_fails("lc-fixture-hashes", flip)

    def test_06_manifest_hex_digit_tamper_fails(self):
        rel = f"{FIXTURES}/mainnet-attest-2026-11-10/SHA256SUMS"
        def flip(root):
            path = root / rel
            text = path.read_text(encoding="utf-8")
            replacement = "1" if text[0] == "0" else "0"
            path.write_text(replacement + text[1:], encoding="utf-8")
        self._assert_fails("lc-fixture-hashes", flip)

    def test_07_unlisted_chain_data_file_fails(self):
        rel = f"{FIXTURES}/mainnet-attest-2026-11-10/unlisted.json"
        self._assert_fails("lc-fixture-hashes", lambda root: (root / rel).write_text("{}", encoding="utf-8"))

    def test_08_ssz_deleted_or_added_fails(self):
        """Review round 1, M3: the inventory is the tree on disk, in a copy as in a checkout."""
        rel = "lez/lab/results/record/net/mainnet/updates/00002.finality.electra.ssz"
        self._assert_fails("lc-helios-step-oracle", lambda root: (root / rel).unlink())
        self._assert_fails("lc-helios-step-oracle",
                           lambda root: (root / rel).with_name("99999.finality.electra.ssz").write_bytes(b"\0"))

    def test_09_tracked_ssz_missing_in_a_checkout(self):
        import subprocess
        with tempfile.TemporaryDirectory(prefix="ssz-git-", dir=SCRATCH) as td:
            root = Path(td)
            (root / "d").mkdir()
            for name in ("a.ssz", "d/b.ssz"):
                (root / name).write_bytes(b"\0")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "a.ssz", "d/b.ssz"], cwd=root, check=True)
            (root / "d/b.ssz").unlink()
            (root / "untracked.ssz").write_bytes(b"\0")
            self.assertEqual(checks_lc._ssz_files(root), (["a.ssz"], ["d/b.ssz"]))

    def test_10_p1_share_wording(self):
        """The share of the 5M target P.1 states must hold for the largest case, 32.3992%."""
        cases = {"at most 32%": "FAIL", "at most 32.4%": "PASS", "under 33%": "PASS", "under 32.3%": "FAIL",
                 "32.4%": "PASS", "about 32%": "PASS", "32.5%": "FAIL", "up to 32.3992%": "PASS"}
        for words, want in cases.items():
            with self.subTest(words=words):
                result = self._tampered_result("lc-stage4-attest-costs", lambda root, w=words: _status_tamper(
                    root, "P.1", P1_FOLDED, f"{w} of LEZ's 5M target"))
                self.assertEqual(result.status, want, result.lines)


if __name__ == "__main__":
    unittest.main(verbosity=2)
