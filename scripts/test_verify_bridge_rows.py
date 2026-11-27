#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_bridge_rows (offline, standard library only).

Each check runs against the real repository and minimal copies under .scratch/ (never /tmp), then must fail after:
(1) one case verdict changes, (2) one record value changes and (3) the status row's stated number or fact changes.
Copies contain only the files the check reads, at their original relative paths.
"""
from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_bridge_rows  # noqa: E402,F401 - registers this module's checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-bridge-row-tests"
CATALOG = "modules/catalog/install-test-landclimber-2026-11-25"
DEPLOYER_RUN = "e2e/runs/seqnet-second-deployer-2"
CLI3 = "e2e/runs/crown-live-3"
CLI_RUN = "e2e/runs/seqnet-cli-1"

CHECK_IDS = (
    "bridge-f1-live-deposits",
    "bridge-u10-catalog-install",
    "bridge-s3-release-action",
    "bridge-r13-cli-operator",
    "bridge-soft3-release-delay",
    "bridge-r14-deployer-walkthrough",
)

VERDICT = {
    "bridge-f1-live-deposits": (
        f"{CLI3}/manifest.json",
        '"hash": "0x32b188db031993f23358ca29c22b2bc1d3fabb638dc02e80bbe737bbd39cdec6",\n'
        '        "blockNumber": "0xb8ed7d",\n        "status": "0x1",',
        '"hash": "0x32b188db031993f23358ca29c22b2bc1d3fabb638dc02e80bbe737bbd39cdec6",\n'
        '        "blockNumber": "0xb8ed7d",\n        "status": "0x0",'),
    "bridge-u10-catalog-install": (
        f"{CATALOG}/steps.tsv", "install-ferrata_bridge_module\tok\t0\t4448\t1\t",
        "install-ferrata_bridge_module\tok\t0\t4448\t0\t"),
    "bridge-s3-release-action": (
        f"{CATALOG}/integrity-commands.log",
        "setup: success, build (darwin-arm64): success, build (linux-amd64): success, build (linux-arm64): success, "
        "already-published: skipped, release: success",
        "setup: success, build (darwin-arm64): success, build (linux-amd64): failure, build (linux-arm64): success, "
        "already-published: skipped, release: success"),
    "bridge-r13-cli-operator": (
        "monitor/results/anvil-e2e/summary.md", "- PASS S1 honest candidate still pending after 3 polls",
        "- FAIL S1 honest candidate still pending after 3 polls"),
    "bridge-soft3-release-delay": (
        f"{CLI_RUN}/user/INDEX", "15-release-held\texit 1\t", "15-release-held\texit 0\t"),
    "bridge-r14-deployer-walkthrough": (
        f"{DEPLOYER_RUN}/checks.txt", "PASS vault admin (handed over):", "FAIL vault admin (handed over):"),
}

RECORD_VALUE = {
    "bridge-f1-live-deposits": (f"{CLI3}/manifest.json", '"gasUsed": "0x88a24"', '"gasUsed": "0x88a25"'),
    "bridge-u10-catalog-install": (f"{CATALOG}/read-back-checks.txt", "index: 7 packages", "index: 8 packages"),
    "bridge-s3-release-action": (f"{CATALOG}/read-back-checks.txt", "index: 8 versions", "index: 9 versions"),
    "bridge-r13-cli-operator": ("e2e/runs/relayer-fork/results.json", '"gasUsed": 375281', '"gasUsed": 375282'),
    "bridge-soft3-release-delay": (
        f"{CLI_RUN}/user/15-release-held.txt", "first opens at Ethereum time 1794564852, in 121 s",
        "first opens at Ethereum time 1794564853, in 121 s"),
    "bridge-r14-deployer-walkthrough": (
        f"{DEPLOYER_RUN}/checks.txt", "delta (the candidate's wait, s): 5400", "delta (the candidate's wait, s): 5401"),
}

STATUS_FACT = {
    "bridge-f1-live-deposits": ("F.1", "ETH deposited live on Sepolia", "ETH never deposited live on Sepolia"),
    "bridge-u10-catalog-install": ("U.10", "seven packages, eight versions", "seven packages, nine versions"),
    "bridge-s3-release-action": ("S.3", "45543214624 and 45543219299", "45543214624 and 45543219298"),
    "bridge-r13-cli-operator": ("R.13", "each component run", "no component run"),
    "bridge-soft3-release-delay": (
        "Soft.3", "the tool's early release is refused, then sent", "the tool's early release is sent without refusal"),
    "bridge-r14-deployer-walkthrough": ("R.14", "29 of 29 settings PASS", "28 of 29 settings PASS"),
}


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


class BridgeRowVerifyTamperTests(unittest.TestCase):
    maxDiff = 4000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in CHECK_IDS}
        if any(c.fn is None for c in cls.checks.values()):
            raise ValueError("all bridge-row checks must be Python fn checks")

    def _copy(self, check, destination: Path) -> Path:
        for rel in sorted(set(check.inputs)):
            src, dst = ROOT / rel, destination / rel
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
        return destination

    def _tampered_result(self, cid: str, mutate):
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
        for cid, check in self.checks.items():
            with self.subTest(check=cid):
                result = run_check(check, Ctx(root=ROOT))
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")

    def test_02_case_verdict_tamper_fails(self):
        for cid, spec in VERDICT.items():
            with self.subTest(check=cid):
                rel, old, new = spec
                self._assert_fails(cid, lambda root, rel=rel, old=old, new=new: _replace(root / rel, old, new))

    def test_03_record_value_tamper_fails(self):
        for cid, spec in RECORD_VALUE.items():
            with self.subTest(check=cid):
                rel, old, new = spec
                self._assert_fails(cid, lambda root, rel=rel, old=old, new=new: _replace(root / rel, old, new))

    def test_04_status_number_or_fact_tamper_fails(self):
        for cid, spec in STATUS_FACT.items():
            with self.subTest(check=cid):
                row, old, new = spec
                self._assert_fails(cid, lambda root, row=row, old=old, new=new: _status_tamper(root, row, old, new))


if __name__ == "__main__":
    unittest.main(verbosity=2)
