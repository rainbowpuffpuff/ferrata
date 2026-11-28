#!/usr/bin/env python3
"""Tests of checkpoint/attestor/zone_inscriptions.py (python3 -I checkpoint/attestor/test_zone_inscriptions.py): made-up
blocks (signers, turns, other channels and opcodes left out) and the recorded rc1 zone (checkpoint/attestor/runs/zone).
Temporary files go under this tree's ignored .scratch/ (never /tmp)."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
TOOL = os.path.join(HERE, "zone_inscriptions.py")
CH = "01" * 32
K = ["%064x" % (i + 0xA0) for i in range(4)]


def load(path):
    with open(path) as f:
        return json.load(f)


def op(signer, channel=CH, opcode=0x11):
    return {"opcode": opcode, "payload": {"channel_id": channel, "inscription": "00", "parent": "00" * 32, "signer": signer}}


def block(slot, *ops):
    return {"header": {"slot": slot, "id": "%064x" % slot}, "transactions": [{"mantle_tx": {"ops": list(ops)}}]}


class Tool(unittest.TestCase):
    def setUp(self):
        os.makedirs(os.path.join(REPO, ".scratch"), exist_ok=True)
        self.d = tempfile.mkdtemp(prefix="test-zone-ins-", dir=os.path.join(REPO, ".scratch"))

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def zone(self, blocks, names=4):
        os.makedirs(os.path.join(self.d, "answers", "blocks"))
        with open(os.path.join(self.d, "answers", "blocks", "1-99.json"), "w") as f:
            json.dump(blocks, f)
        with open(os.path.join(self.d, "zone.log"), "w") as f:
            for i in range(names):
                f.write(f"[01:00:0{i}] seq-{i} set up: {K[i]}\n")
        p = subprocess.run([sys.executable, "-I", TOOL, self.d], capture_output=True, text=True)
        p2 = os.path.join(self.d, "inscriptions.json")
        out = load(p2) if os.path.exists(p2) else None
        return p.returncode, out

    def test_turns_and_counts(self):
        blocks = [block(10, op(K[0])), block(11, op(K[0])), block(14, op(K[1])),
                  block(12, op(K[0]), op(K[2], channel="02" * 32)),        # another channel: not counted
                  block(15, op(K[1], opcode=0x10)),                         # another opcode: not counted
                  block(20, op(K[3])), block(21, op(K[2])), block(22, op("ff" * 32))]
        rc, s = self.zone(blocks)
        self.assertEqual(rc, 0)
        self.assertEqual(s["nodeBlocks"], 8)
        self.assertEqual(s["inscriptions"], 7)
        self.assertEqual(s["perSequencer"], {"seq-0": 3, "seq-1": 1, "seq-3": 1, "seq-2": 1, "ff" * 8: 1})
        self.assertEqual([(t["sequencer"], t["inscriptions"], t["fromSlot"], t["toSlot"]) for t in s["turns"]],
                         [("seq-0", 3, 10, 12), ("seq-1", 1, 14, 14), ("seq-3", 1, 20, 20), ("seq-2", 1, 21, 21), ("ff" * 8, 1, 22, 22)])

    def test_every_shard_is_read(self):
        os.makedirs(os.path.join(self.d, "answers", "blocks"))
        for name, blocks in (("1-50.json", [block(10, op(K[0]))]), ("51-99.json", [block(60, op(K[1])), block(61, op(K[1]))])):
            with open(os.path.join(self.d, "answers", "blocks", name), "w") as f:
                json.dump(blocks, f)
        with open(os.path.join(self.d, "zone.log"), "w") as f:
            f.write(f"seq-0 set up: {K[0]}\nseq-1 set up: {K[1]}\n")
        p = subprocess.run([sys.executable, "-I", TOOL, self.d], capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        s = load(os.path.join(self.d, "inscriptions.json"))
        self.assertEqual((s["nodeBlocks"], s["inscriptions"], s["perSequencer"]), (3, 3, {"seq-0": 1, "seq-1": 2}))

    def test_overlapping_windows_count_a_block_once(self):
        os.makedirs(os.path.join(self.d, "answers", "blocks"))
        for name, blocks in (("1-60.json", [block(10, op(K[0])), block(60, op(K[1]))]),
                             ("60-99.json", [block(60, op(K[1])), block(61, op(K[1]))])):   # slot 60 in both windows
            with open(os.path.join(self.d, "answers", "blocks", name), "w") as f:
                json.dump(blocks, f)
        with open(os.path.join(self.d, "zone.log"), "w") as f:
            f.write(f"seq-0 set up: {K[0]}\nseq-1 set up: {K[1]}\n")
        p = subprocess.run([sys.executable, "-I", TOOL, self.d], capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        s = load(os.path.join(self.d, "inscriptions.json"))
        self.assertEqual((s["nodeBlocks"], s["inscriptions"], s["perSequencer"]), (3, 3, {"seq-0": 1, "seq-1": 2}))

    def test_no_inscription_is_a_failure(self):
        rc, s = self.zone([block(10, op(K[0], channel="02" * 32))])
        self.assertEqual(rc, 1)
        self.assertEqual(s["inscriptions"], 0)

    def test_recorded_rc1_zone(self):
        z = os.path.join(HERE, "runs", "zone")
        out = os.path.join(self.d, "rc1.json")
        p = subprocess.run([sys.executable, "-I", TOOL, z, "--out", out], capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        s = load(out)
        self.assertEqual(sorted(s["perSequencer"]), ["seq-0", "seq-1", "seq-2", "seq-3"])
        self.assertEqual(sum(s["perSequencer"].values()), s["inscriptions"])
        self.assertEqual(sum(t["inscriptions"] for t in s["turns"]), s["inscriptions"])
        self.assertTrue(all(t["fromSlot"] <= t["toSlot"] for t in s["turns"]))


if __name__ == "__main__":
    unittest.main(verbosity=1)
