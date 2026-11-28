#!/usr/bin/env python3
"""Tests of checkpoint/attestor/check_zone_v040.py on made-up records (python3 -I checkpoint/attestor/test_check_zone_v040.py).
Temporary files go under this tree's ignored .scratch/ (never /tmp)."""
import copy
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
CHECK = os.path.join(HERE, "check_zone_v040.py")
V040 = "v0.4.0 a255b42c0fc80d1cb219454a3a1f5ec3b5d92115"

STANDALONE = {
    "lez": V040, "standalone_first_height": 2, "standalone_last_height": 14, "check_health": True,
    "wallet_storage_sealed": True, "test_account": "Public/3DpQ7crHm9mYi59LvGA2jPtXT929hRULYKuJBBam3jEr",
    "new_account": "Public/81XH42vBEN4XLLDttQvv2uapGtoUVBtVgPCZgX8YYQHS", "transfer_tx": "ab" * 32,
    "transfer_receipt": {"success": True, "fee": {"gas_exec": 1, "gas_stor": 2, "fee_base": "3", "tip": "0", "refund": "0"}},
    "transfer_block": 3, "private_enabled": True, "private_tx": "cd" * 32,
    "private_account": "Private/8sYxFr3Fn1hVQ7p2bNq3YhZQe1cVpTwEw4C3nGmAAAAA",
    "private_receipt": {"success": True, "fee": {"gas_exec": 9, "gas_stor": 9000}},
}
VOTE = {
    "lez": V040, "channel_keys": ["%064x" % i for i in range(1, 5)], "four_keys_after_s": 40,
    "posting_timeframe_before": 60, "posting_timeframe_voted": 90,
    "vote_1": "Included in block 7 1 of the 3 accredited keys these params need have voted for them",
    "vote_2": "Included in block 9 2 of the 3 accredited keys these params need have voted for them",
    "vote_3": "Included in block 11 The channel params are now ChannelParams { posting_timeframe: 90 }, by this vote",
    "bedrock_took_params": True, "unstake_enabled": True, "finality_depth": 35, "unstake_node_height": "100",
    "key_removed_node_height": "104", "release_node_height": "170", "release_paid": 149,
}


class Check(unittest.TestCase):
    def setUp(self):
        os.makedirs(os.path.join(REPO, ".scratch"), exist_ok=True)
        self.d = tempfile.mkdtemp(prefix="test-check-zone-v040-", dir=os.path.join(REPO, ".scratch"))

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def run_on(self, **runs):
        for name, facts in runs.items():
            os.makedirs(os.path.join(self.d, name), exist_ok=True)
            with open(os.path.join(self.d, name, "facts.json"), "w") as f:
                json.dump(facts, f)
        p = subprocess.run([sys.executable, "-I", CHECK, self.d], capture_output=True, text=True)
        return p.returncode, p.stdout

    def test_good_runs_pass(self):
        rc, out = self.run_on(standalone=STANDALONE, vote=VOTE)
        self.assertEqual(rc, 0, out)
        self.assertIn("standalone/facts.json: ok", out)
        self.assertIn("vote/facts.json: ok", out)

    def test_no_records(self):
        p = subprocess.run([sys.executable, "-I", CHECK, self.d], capture_output=True, text=True)
        self.assertEqual(p.returncode, 2)

    def bad(self, base, name, **change):
        f = copy.deepcopy(base)
        for k, v in change.items():
            if v is None:
                f.pop(k, None)
            else:
                f[k] = v
        rc, out = self.run_on(**{name: f})
        self.assertEqual(rc, 1, (change, out))
        return out

    def test_standalone_failures(self):
        self.bad(STANDALONE, "s", lez="v0.4.0-rc3 bafd07be0701398f9bc435f73ab05e9a7936c301")
        self.bad(STANDALONE, "s", standalone_first_height=1)
        self.bad(STANDALONE, "s", standalone_last_height=2)
        self.bad(STANDALONE, "s", check_health=False)
        self.bad(STANDALONE, "s", wallet_storage_sealed=False)
        self.bad(STANDALONE, "s", transfer_tx="0x" + "ab" * 32)
        self.bad(STANDALONE, "s", transfer_receipt={"success": False, "fee": {}})
        self.bad(STANDALONE, "s", transfer_receipt=None)
        self.bad(STANDALONE, "s", new_account="Private/abc")
        self.bad(STANDALONE, "s", program_deploy={"header": "Public/x", "segments": 0, "image_id": "0x" + "00" * 32})
        self.bad(STANDALONE, "s", program_deploy={"header": "Public/x", "segments": 2, "image_id": "nope"})
        self.bad(STANDALONE, "s", program_deploy="deployed")      # not a record: reported, not a crash
        rc, out = self.run_on(s=dict(STANDALONE, program_deploy={"header": "Public/x", "segments": 2, "image_id": "0x" + "0a" * 32}))
        self.assertEqual(rc, 0, out)
        self.bad(STANDALONE, "s", private_receipt={"success": False})
        self.bad(STANDALONE, "s", private_account="Public/x")
        self.bad(STANDALONE, "s", private_tx="zz")
        # the private transfer runs by default: its record is required unless the run says it was off
        self.bad(STANDALONE, "s", private_tx=None, private_account=None, private_receipt=None)
        self.bad(STANDALONE, "s", private_enabled=None, private_tx=None, private_account=None, private_receipt=None)
        rc, out = self.run_on(s=dict({k: v for k, v in STANDALONE.items() if not k.startswith("private")}, private_enabled=False))
        self.assertEqual(rc, 0, out)
        # the transfer's block: a height (the recorded run of 28 Nov wrote it as a string of digits)
        self.bad(STANDALONE, "s", transfer_block=None)
        self.bad(STANDALONE, "s", transfer_block="")
        self.bad(STANDALONE, "s", transfer_block=1)
        rc, out = self.run_on(s=dict(STANDALONE, transfer_block="3"))
        self.assertEqual(rc, 0, out)

    def test_vote_failures(self):
        self.bad(VOTE, "v", channel_keys=VOTE["channel_keys"][:3])
        self.bad(VOTE, "v", channel_keys=[VOTE["channel_keys"][0]] * 4)
        self.bad(VOTE, "v", four_keys_after_s=None)
        self.bad(VOTE, "v", posting_timeframe_voted=61)
        self.bad(VOTE, "v", vote_2="The channel params are now ChannelParams {..}")
        self.bad(VOTE, "v", vote_3="2 of the 3 accredited keys these params need have voted for them")
        self.bad(VOTE, "v", bedrock_took_params=False)

    def test_unstake_finality(self):
        ok = VOTE
        rc, out = self.run_on(v=ok)
        self.assertEqual(rc, 0, out)
        self.bad(ok, "v", release_node_height="134")            # 34 blocks after the unstake: too soon for depth 35
        self.bad(ok, "v", release_paid=0)
        self.bad(ok, "v", release_paid=None)                  # an unstake that never paid has no release_paid at all
        self.bad(ok, "v", key_removed_node_height="")
        self.bad(ok, "v", key_removed_node_height="180")         # the removal seen after the release
        self.bad(ok, "v", unstake_node_height=None)
        self.bad(ok, "v", finality_depth="35")
        # LEZ's rule: 2 x 35 - 1 = 69 blocks after the key's removal (the removing configuration final), less 3 for the
        # polls, measured from the removal as seen (h1), not from the unstake request (h0)
        rc, out = self.run_on(v=dict(ok, unstake_node_height="100", key_removed_node_height="110", release_node_height="176"))
        self.assertEqual(rc, 0, out)
        self.bad(ok, "v", unstake_node_height="100", key_removed_node_height="110", release_node_height="175")
        self.bad(ok, "v", unstake_node_height="100", key_removed_node_height="104", release_node_height="136")   # 32 < 35
        self.bad(ok, "v", unstake_node_height="100", key_removed_node_height="104", release_node_height="139")   # depth only
        self.bad(ok, "v", unstake_node_height="100", key_removed_node_height="160", release_node_height="166")   # 66 from h0

    def test_unstake_is_required_unless_off(self):
        gone = dict(finality_depth=None, unstake_node_height=None, key_removed_node_height=None, release_node_height=None,
                    release_paid=None)
        out = self.bad(VOTE, "v", **gone)
        self.assertIn("has no record", out)
        self.bad(VOTE, "v", unstake_enabled=None, **gone)       # a run of before the flag (zone-v040-vote-1)
        rc, out = self.run_on(v=dict({k: v for k, v in VOTE.items() if k not in gone}, unstake_enabled=False))
        self.assertEqual(rc, 0, out)

    def test_bedrock_channel_after_each_vote(self):
        ok = dict(VOTE, channel_posting_timeframe_after_vote_1="60", channel_posting_timeframe_after_vote_2="60",
                  channel_posting_timeframe_after_vote_3="90")
        rc, out = self.run_on(v=ok)
        self.assertEqual(rc, 0, out)
        self.bad(ok, "v", channel_posting_timeframe_after_vote_2="90")   # changed before the third vote
        self.bad(ok, "v", channel_posting_timeframe_after_vote_1="90")

    def test_unknown_run(self):
        self.bad({"lez": V040}, "x")


if __name__ == "__main__":
    unittest.main(verbosity=1)
