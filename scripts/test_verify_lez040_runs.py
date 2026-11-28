#!/usr/bin/env python3
"""Tamper tests for verifylib.checks_lez040_runs (offline, standard library only).

All four checks pass on the repository and on minimal copies of just their declared inputs.  Each
subsequent case alters exactly one recorded or stated value and requires FAIL with a diagnostic
naming that difference.  Copies stay under .scratch/verify-lez040-runs-tests/.
"""
from __future__ import annotations

import base64
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import checks_lez040_runs  # noqa: E402 - registers the checks
from verifylib.core import Ctx, REGISTRY, run_check  # noqa: E402

SCRATCH = ROOT / ".scratch" / "verify-lez040-runs-tests"
STANDALONE = "lez040-run-standalone"
FOUR_KEYS = "lez040-run-four-keys"
VOTE_1 = "lez040-run-vote-1"
VOTE_2 = "lez040-run-vote-2"
CHECK_IDS = (STANDALONE, FOUR_KEYS, VOTE_1, VOTE_2)
VOTE_IDS = (VOTE_1, VOTE_2)
EVIDENCE_ROW = {cid: 303 for cid in CHECK_IDS}  # docs/EVIDENCE.md row 303 names the four
RUN = "e2e/runs/zone-v040-standalone-1"
FOUR = "e2e/runs/zone-v040-four-keys-1"
VOTE_RUNS = {VOTE_1: "e2e/runs/zone-v040-vote-1", VOTE_2: "e2e/runs/zone-v040-vote-2"}


def _replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"tamper target {path}: expected one {old!r}, found {text.count(old)}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class Lez040RunVerifyTamperTests(unittest.TestCase):
    maxDiff = 5000

    @classmethod
    def setUpClass(cls):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        cls.checks = {cid: REGISTRY[cid] for cid in CHECK_IDS}

    def _copy(self, cid: str, destination: Path) -> Path:
        for rel in sorted(set(self.checks[cid].inputs)):
            source = ROOT / rel
            target = destination / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                shutil.copy2(source, target)
        return destination

    def _run(self, cid: str, root: Path = ROOT):
        return run_check(self.checks[cid], Ctx(root=root, git_dir=ROOT))

    def _altered(self, cid: str, edit, contains: str):
        with tempfile.TemporaryDirectory(prefix=cid + "-", dir=SCRATCH) as temporary:
            root = self._copy(cid, Path(temporary))
            edit(root)
            result = self._run(cid, root)
        self.assertEqual(result.status, "FAIL", f"{cid} did not detect the alteration: {result.lines}")
        self.assertTrue(any(contains.casefold() in line.casefold() for line in result.lines),
                        f"{cid}: no failure line names {contains!r}: {result.lines}")

    def _replace_and_fail(self, cid: str, rel: str, old: str, new: str, contains: str):
        self._altered(cid, lambda root: _replace(root / rel, old, new), contains)

    def test_01_all_checks_pass_on_the_real_tree(self):
        for cid in CHECK_IDS:
            with self.subTest(check=cid):
                result = self._run(cid)
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")
                if cid == STANDALONE:
                    self.assertTrue(any("p = 8" in line for line in result.lines), result.lines)

    def test_02_minimal_input_copies_also_pass(self):
        for cid in CHECK_IDS:
            with self.subTest(check=cid), tempfile.TemporaryDirectory(prefix=cid + "-clean-", dir=SCRATCH) as temporary:
                result = self._run(cid, self._copy(cid, Path(temporary)))
                self.assertEqual(result.status, "PASS", f"{cid}: {result.lines}")

    def test_03_metadata_and_declared_inputs(self):
        self.assertEqual(set(self.checks), set(CHECK_IDS))
        for cid, registered in self.checks.items():
            with self.subTest(check=cid):
                self.assertEqual(registered.family, "record")
                self.assertEqual(registered.mode, "offline")
                self.assertEqual(registered.rows, ())
                self.assertEqual(registered.evidence, (EVIDENCE_ROW[cid],))  # verify.py evidence:N selects it
                self.assertTrue(registered.title and registered.what and registered.against)
                self.assertTrue(registered.inputs)
                self.assertIsNotNone(registered.fn)
                for rel in registered.inputs:
                    path = Path(rel)
                    self.assertFalse(path.is_absolute(), rel)
                    self.assertNotIn("..", path.parts, rel)
                    self.assertTrue((ROOT / rel).exists(), f"{cid}: missing input {rel}")

    def test_10_a_decoded_byte_of_block_3_fails(self):
        def edit(root: Path) -> None:
            path = root / RUN / "blocks/block-3.json"
            raw = bytearray(base64.b64decode(json.loads(path.read_text(encoding="utf-8")), validate=True))
            raw[100] ^= 1  # transaction material: still decodable, but no longer the recorded block hash
            path.write_text(json.dumps(base64.b64encode(raw).decode("ascii")) + "\n", encoding="utf-8")
        self._altered(STANDALONE, edit, "recomputed block hash")

    def test_11_transfer_receipt_storage_gas_fails(self):
        self._replace_and_fail(STANDALONE, f"{RUN}/facts.json", '"gas_stor": 621', '"gas_stor": 622',
                               "transfer_receipt")

    def test_12_private_receipt_base_fee_fails(self):
        self._replace_and_fail(STANDALONE, f"{RUN}/facts.json", '"fee_base": 5163400', '"fee_base": 5163401',
                               "private fee_base")

    def test_13_a_readme_table_number_fails(self):
        self._replace_and_fail(STANDALONE, f"{RUN}/README.md", "base fee 4,968", "base fee 4,969", "public receipt")

    def test_14_the_ferrata_rebate_image_id_fails(self):
        old = "eddda0baf6daac70d4baf3aa2446b8907578f9f863817ff1353d05511eb18712"
        new = "fddda0baf6daac70d4baf3aa2446b8907578f9f863817ff1353d05511eb18712"
        self._replace_and_fail(STANDALONE, f"{RUN}/facts.json", old, new, "ferrata_rebate image_id")

    def test_15_a_private_wallet_log_line_fails(self):
        self._replace_and_fail(STANDALONE, f"{RUN}/logs/private.log",
                               "Transaction hash is 431ee48182ad222280eea791636f868b224f24346201e72b319bb955ce83b590",
                               "Transaction hash is 531ee48182ad222280eea791636f868b224f24346201e72b319bb955ce83b590",
                               "wallet transaction hash")

    def test_16_storage_gas_must_be_the_serialized_length(self):
        """Only the length identity can catch this: the decoded transfer one byte longer than its receipt's storage gas
        (everything else as recorded). The decoder is wrapped, not the records, so no other check trips first."""
        from unittest import mock
        from verifylib import lez040
        real = lez040.decode_block
        transfer = json.loads((ROOT / RUN / "facts.json").read_text(encoding="utf-8"))["transfer_tx"]

        def longer(raw):
            block = real(raw)
            for tx in block["transactions"]:
                if tx["hash"].hex() == transfer:
                    tx["raw"] = tx["raw"] + b"\x00"
            return block

        with mock.patch.object(checks_lez040_runs.lez040, "decode_block", longer):
            result = self._run(STANDALONE)
        self.assertEqual(result.status, "FAIL", result.lines)
        self.assertTrue(any("serialized transaction length" in line for line in result.lines), result.lines)

    def test_17_the_public_fee_must_be_a_whole_price_times_gas(self):
        """A public fee of 4,969 for 621 gas, forged alike in the decoded block, facts.json, run.log and the README:
        only the whole-price identity can catch it."""
        from unittest import mock
        from verifylib import lez040
        real = lez040.decode_block
        transfer = json.loads((ROOT / RUN / "facts.json").read_text(encoding="utf-8"))["transfer_tx"]

        def forged(raw):
            block = real(raw)
            for i, tx in enumerate(block["transactions"]):
                if tx["hash"].hex() == transfer and i < len(block["receipts"]):
                    block["receipts"][i] = {**block["receipts"][i], "fee_base": 4969}
            return block

        def edit(root: Path) -> None:
            _replace(root / RUN / "facts.json", '"fee_base": 4968', '"fee_base": 4969')
            _replace(root / RUN / "run.log", '"fee_base":4968', '"fee_base":4969')
            _replace(root / RUN / "README.md", "base fee 4,968", "base fee 4,969")

        with mock.patch.object(checks_lez040_runs.lez040, "decode_block", forged):
            self._altered(STANDALONE, edit, "whole-number price")

    def test_20_one_inscription_removed_from_the_node_answers_fails(self):
        def edit(root: Path) -> None:
            path = root / FOUR / "answers/blocks/2584723-2584999.json"
            blocks = json.loads(path.read_text(encoding="utf-8"))
            for block in blocks:
                for transaction in block.get("transactions") or []:
                    mantle = transaction.get("mantle_tx") or {}
                    for index, operation in enumerate(mantle.get("ops") or []):
                        payload = operation.get("payload") or {}
                        if (operation.get("opcode") == 0x11 and
                                str(payload.get("channel_id", "")).removeprefix("0x") == "01" * 32):
                            del mantle["ops"][index]
                            path.write_text(json.dumps(blocks), encoding="utf-8")
                            return
            raise AssertionError("no channel inscription found to remove")
        self._altered(FOUR_KEYS, edit, "channel inscriptions counted from answers")

    def test_25_an_inscribed_lez_block_byte_fails(self):
        """One byte inside one inscribed LEZ block: the counts by signer and turn stay; only the v0.4.0 reading of the
        inscribed blocks (hash, chain) can see it."""
        def edit(root: Path) -> None:
            path = root / FOUR / "answers/blocks/2584723-2584999.json"
            blocks = json.loads(path.read_text(encoding="utf-8"))
            for block in blocks:
                for transaction in block.get("transactions") or []:
                    for operation in (transaction.get("mantle_tx") or {}).get("ops") or []:
                        payload = operation.get("payload") or {}
                        if operation.get("opcode") == 0x11 and payload.get("channel_id") == "01" * 32 and len(
                                payload.get("inscription", "")) > 400:
                            raw = bytearray(bytes.fromhex(payload["inscription"]))
                            raw[150] ^= 1
                            payload["inscription"] = raw.hex()
                            path.write_text(json.dumps(blocks), encoding="utf-8")
                            return
            raise AssertionError("no inscription to alter")
        self._altered(FOUR_KEYS, edit, "inscribed LEZ block")

    def test_26_checkpoint_test_data_differs(self):
        def edit(root: Path) -> None:
            path = root / "checkpoint/crates/checkpoint-core/tests/data/zone-v040-four-keys-1/blocks.hex"
            lines = path.read_text(encoding="utf-8").splitlines()
            lines[5] = lines[5][:-2] + ("00" if lines[5][-2:] != "00" else "01")
            path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        self._altered(FOUR_KEYS, edit, "checkpoint-core's v0.4.0 test data")

    def test_21_the_readmes_107_fails(self):
        self._replace_and_fail(FOUR_KEYS, f"{FOUR}/README.md", "107 channel inscriptions", "106 channel inscriptions",
                               "inscription/node/per-sequencer counts and turns")

    def test_22_a_per_sequencer_count_fails(self):
        self._replace_and_fail(FOUR_KEYS, f"{FOUR}/inscriptions.json", '"seq-0": 38', '"seq-0": 37',
                               "perSequencer counted from answers")

    def test_23_one_byte_of_deployment_settings_fails(self):
        self._replace_and_fail(FOUR_KEYS, f"{FOUR}/deployment-settings.yaml", "num_blend_layers: 1",
                               "num_blend_layers: 2", "deployment-settings.yaml SHA-256")

    def test_24_the_attestor_refusal_fails(self):
        self._replace_and_fail(FOUR_KEYS, f"{FOUR}/attest-run.txt", "NotStrictlyOrdered", "StrictlyOrdered",
                               "rc3 parser refusal")

    def _both_vote_runs(self, edit, contains: str):
        for cid in VOTE_IDS:
            with self.subTest(check=cid):
                self._altered(cid, edit(cid), contains)

    def test_30_the_third_votes_posting_timeframe_fails(self):
        def edit(cid):
            return lambda root: _replace(
                root / VOTE_RUNS[cid] / "logs/vote.log",
                "The channel params are now ChannelParams { minimum_sequencer_stake: 149, posting_timeframe: 90",
                "The channel params are now ChannelParams { minimum_sequencer_stake: 149, posting_timeframe: 91")
        self._both_vote_runs(edit, "third vote's resulting ChannelParams")

    def test_31_the_channel_after_votes_posting_timeframe_fails(self):
        def edit(cid):
            return lambda root: _replace(root / VOTE_RUNS[cid] / "channel-after-vote.json",
                                         '"posting_timeframe":90', '"posting_timeframe":91')
        self._both_vote_runs(edit, "posting_timeframe after third vote")

    def test_32_one_of_the_four_channel_keys_removed_fails(self):
        def edit(cid):
            def remove(root):
                path = root / VOTE_RUNS[cid] / "channel-four-keys.json"
                channel = json.loads(path.read_text(encoding="utf-8"))
                channel["accredited_keys"].pop()
                path.write_text(json.dumps(channel), encoding="utf-8")
            return remove
        self._both_vote_runs(edit, "four accredited setup keys")

    def test_33_a_stake_of_150_fails(self):
        def edit(cid):
            def first_stake(root):
                path = root / VOTE_RUNS[cid] / "logs/stake.log"
                lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
                self.assertTrue(lines[0].startswith("Staking 149 from"), lines[0])
                lines[0] = lines[0].replace("Staking 149 from", "Staking 150 from", 1)
                path.write_text("".join(lines), encoding="utf-8")
            return first_stake
        self._both_vote_runs(edit, "three stakes of 149")

    def test_34_vote_1s_unstake_refusal_text_fails(self):
        self._replace_and_fail(VOTE_1, f"{VOTE_RUNS[VOTE_1]}/logs/unstake.log",
                               "neither a public account nor a bound private address",
                               "neither an existing public account nor a bound private address", "unstake refusal")

    def test_35_vote_2s_paid_height_in_facts_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/facts.json",
                               '"release_node_height": "254"', '"release_node_height": "253"', "paid height")

    def test_36_vote_2s_readme_69_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/README.md",
                               "The release came 69 node blocks", "The release came 68 node blocks",
                               "README's distance")

    def test_37_raw_release_amount_controls_the_balance_after(self):
        """149 -> 148 in unstake.log also changes the independently derived 1,000 -> 1,149 balance statement."""
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/logs/unstake.log",
                               "Releasing 149 to", "Releasing 148 to", "destination balance after raw release")

    def test_38_the_readmes_destination_balance_after_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/README.md", "1,000 → 1,149", "1,000 → 1,150",
                               "destination balance after, paid height and seconds")

    def test_39_genesis_finality_depth_fails(self):
        def edit(cid):
            return lambda root: _replace(root / VOTE_RUNS[cid] / "logs/seq-0.log",
                                         '"finality_depth":35', '"finality_depth":36')
        self._both_vote_runs(edit, "genesis finality_depth")

    def test_40_an_exit_delay_in_genesis_fails(self):
        def edit(cid):
            return lambda root: _replace(root / VOTE_RUNS[cid] / "logs/seq-0.log",
                                         '"finality_depth":35}', '"finality_depth":35,"exit_delay":0}')
        self._both_vote_runs(edit, "exit_delay is present")

    def test_41_vote_1s_first_vote_block_in_the_readme_fails(self):
        self._replace_and_fail(VOTE_1, f"{VOTE_RUNS[VOTE_1]}/README.md", "block 36:", "block 37:",
                               "first two vote blocks")

    def test_42_vote_2s_unstaked_key_in_the_sequencer_log_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/logs/seq-3.log", "public_key=12b87983", "public_key=22b87983",
                               "unstaked key")

    def test_43_vote_2s_landing_height_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/facts.json",
                               '"unstake_node_height": "185"', '"unstake_node_height": "186"',
                               "unstake and removal heights")

    def test_44_vote_2s_removal_height_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/facts.json",
                               '"key_removed_node_height": "185"', '"key_removed_node_height": "184"',
                               "release distance")

    def test_45_vote_2s_paid_seconds_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/facts.json", '"release_after_s": 306',
                               '"release_after_s": 307', "landing/removal and paid seconds")

    def test_46_vote_2s_destination_balance_before_fails(self):
        self._replace_and_fail(VOTE_2, f"{VOTE_RUNS[VOTE_2]}/facts.json", '"release_destination_before": 1000',
                               '"release_destination_before": 1001', "destination balance before funding")

    def test_47_the_recorded_runs_own_checker(self):
        """vote-2's checker must pass it; vote-1 stopped short, so its checker must report that gap (exit 1) and only it."""
        self._replace_and_fail(VOTE_2, "checkpoint/attestor/check_zone_v040.py",
                               "return 1 if bad else 0", "return 1", "exit status 1")
        self._replace_and_fail(VOTE_1, "checkpoint/attestor/check_zone_v040.py",
                               "return 1 if bad else 0", "return 0", "expected exit 1 reporting only")

    def test_48_no_alteration_means_no_failure(self):
        """Mutation-test control: the same minimal-copy path with no altered record stays PASS."""
        for cid in VOTE_IDS:
            with self.subTest(check=cid), tempfile.TemporaryDirectory(prefix=cid + "-control-", dir=SCRATCH) as temporary:
                result = self._run(cid, self._copy(cid, Path(temporary)))
                self.assertEqual(result.status, "PASS", result.lines)
                self.assertFalse(any("differ" in line or "absent" in line for line in result.lines), result.lines)


class ClientsLiveRun(unittest.TestCase):
    """lez040-run-clients: the clients' run on LEZ v0.4.0's standalone sequencer (cli/results/v040-standalone-1),
    re-checked by scripts/test_lez040_decode.py's LiveRun on copies of its declared inputs."""

    CID = "lez040-run-clients"
    RUN = "cli/results/v040-standalone-1"

    def _copy(self, destination: Path) -> Path:
        for rel in REGISTRY[self.CID].inputs:
            source, target = ROOT / rel, destination / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            (shutil.copytree if source.is_dir() else shutil.copy2)(source, target)
        return destination

    def _result(self, edit=None):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="clients-", dir=SCRATCH) as temporary:
            root = self._copy(Path(temporary))
            if edit:
                edit(root)
            return run_check(REGISTRY[self.CID], Ctx(root=root, git_dir=ROOT))

    def assertFailsWith(self, edit):
        r = self._result(edit)
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("FAILED" in ln or "Error" in ln for ln in r.lines), r.lines)

    def test_passes_on_the_tree_and_a_copy(self):
        self.assertEqual(run_check(REGISTRY[self.CID], Ctx(root=ROOT)).status, "PASS")
        r = self._result()
        self.assertEqual(r.status, "PASS", r.lines)

    def test_a_block_byte(self):
        def edit(root):
            path = root / self.RUN / "blocks/block-4.json"
            raw = bytearray(base64.b64decode(json.loads(path.read_text(encoding="utf-8"))))
            raw[100] ^= 1
            path.write_text(json.dumps(base64.b64encode(raw).decode("ascii")), encoding="utf-8")
        self.assertFailsWith(edit)

    def test_a_recorded_fee(self):
        self.assertFailsWith(lambda root: _replace(root / self.RUN / "facts.json", '"fee_base": 4432', '"fee_base": 4433'))

    def test_lez_reads_outcome(self):
        self.assertFailsWith(lambda root: _replace(root / self.RUN / "lez-read-block-3.json", '"outcome": "applied"',
                                                   '"outcome": "reverted"'))


class HarnessRecord(unittest.TestCase):
    """lez040-run-harness: docs/port-v0.4.0/harness-a7a45cb6/'s README held to its test lines."""
    CID = "lez040-run-harness"
    RUN = "docs/port-v0.4.0/harness-a7a45cb6"

    def _result(self, edit=None):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="harness-", dir=SCRATCH) as temporary:
            root = Path(temporary)
            for rel in REGISTRY[self.CID].inputs:
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / rel, root / rel)
            if edit:
                edit(root)
            return run_check(REGISTRY[self.CID], Ctx(root=root, git_dir=ROOT))

    def assertFails(self, edit, needle):
        r = self._result(edit)
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any(needle in ln for ln in r.lines), r.lines)

    def test_passes(self):
        self.assertEqual(self._result().status, "PASS")

    def test_a_shard_row(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "README.md", "| harness-2 | 33 | 4 |", "| harness-2 | 34 | 3 |"),
                         "| harness-2 | 33 | 4 |")

    def test_a_failure_turned_ok(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "tests.txt", "test f8_two_deposits_full_diff ... FAILED",
                                               "test f8_two_deposits_full_diff ... ok"), "harness-4")

    def test_a_failed_test_unnamed(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "README.md", "`registry_fees_caps_freezes_and_sunset`",
                                               "`registry_fees`"), "registry_fees_caps_freezes_and_sunset")

    def test_the_title(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "README.md", "140 of 153 tests pass, 13 fail",
                                               "141 of 153 tests pass, 12 fail"), "title")

    def test_updater_gate_failed(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "updater-gate.txt",
                                               "test gate_poll_speeds_share_the_cost ... ok",
                                               "test gate_poll_speeds_share_the_cost ... FAILED"), "updater-gate.txt")

    def test_updater_privacy_counts(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "README.md", "(105, then the remaining 9",
                                               "(106, then the remaining 9"), "105 and 9")

    def test_privacy_failure_unnamed(self):
        self.assertFails(lambda root: _replace(root / self.RUN / "updater-privacy.txt",
                                               "test privacy_suite_lez_half ... FAILED", "test privacy_suite_eth_half ... FAILED"),
                         "privacy_suite_eth_half")


class CliEndToEnd(unittest.TestCase):
    CID = "lez040-run-cli-e2e"
    RUN = "cli/results/v040-e2e-1"

    def _result(self, edit=None):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="cli-e2e-", dir=SCRATCH) as temporary:
            root = Path(temporary)
            for rel in REGISTRY[self.CID].inputs:
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / rel, root / rel)
            if edit:
                edit(root)
            return run_check(REGISTRY[self.CID], Ctx(root=root, git_dir=ROOT))

    def test_passes(self):
        self.assertEqual(self._result().status, "PASS")

    def test_the_round_trip_failed(self):
        r = self._result(lambda root: _replace(root / self.RUN / "e2e.txt",
                         "test deposit_scan_claim_burn_release_and_recovery ... ok",
                         "test deposit_scan_claim_burn_release_and_recovery ... FAILED"))
        self.assertEqual(r.status, "FAIL", r.lines)

    def test_the_readmes_seconds(self):
        r = self._result(lambda root: _replace(root / self.RUN / "README.md", "passed in 87 s", "passed in 86 s"))
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("87 s" in ln for ln in r.lines), r.lines)

    def test_run_3_failed(self):
        r = self._result(lambda root: _replace(root / self.RUN / "e2e-3.txt",
                         "test deposit_scan_claim_burn_release_and_recovery ... ok",
                         "test deposit_scan_claim_burn_release_and_recovery ... FAILED"))
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("e2e-3.txt" in ln for ln in r.lines), r.lines)

    def test_run_3_seconds(self):
        r = self._result(lambda root: _replace(root / self.RUN / "README.md", "(2 in 50.6 s)", "(2 in 50.7 s)"))
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("50.6 s" in ln for ln in r.lines), r.lines)

    def test_run_3_names_a_test_its_log_lacks(self):
        r = self._result(lambda root: _replace(root / self.RUN / "README.md",
                         "`deposit_scan_claim_burn_release_and_recovery` and `names_nothing_catches_every_form` passed (2",
                         "`deposit_scan_claim_burn_release_and_recovery` and `a_test_never_run` passed (2"))
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("run 3 names" in ln for ln in r.lines), r.lines)

    def test_run_2_names_a_test_its_log_lacks(self):
        r = self._result(lambda root: _replace(root / self.RUN / "README.md", "and `names_nothing_catches_every_form`: deposits",
                                               "and `another_test`: deposits"))
        self.assertEqual(r.status, "FAIL", r.lines)
        self.assertTrue(any("run 2 names" in ln for ln in r.lines), r.lines)

    def test_attest_faults_count(self):
        r = self._result(lambda root: _replace(root / self.RUN / "README.md", "`attest_faults` 3 passed",
                                               "`attest_faults` 4 passed"))
        self.assertEqual(r.status, "FAIL", r.lines)


if __name__ == "__main__":
    unittest.main(verbosity=2)
