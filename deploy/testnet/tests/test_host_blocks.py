#!/usr/bin/env python3
"""The host's block readers on LEZ v0.4.0's blocks (python3 -I deploy/testnet/tests/test_host_blocks.py).

deploy/testnet/host/bin/ferrata-status and ferrata-bridge-identity read a LEZ block's id, hash, time and Bedrock status
at fixed offsets of the sequencer's `getBlock` answer (base64 Borsh: block_id u64, prev_block_hash, hash, timestamp u64,
..., the BedrockStatus byte last). v0.4.0 keeps that header and the trailing status (its body carries receipts in place
of `reverted`, LEZ PR 1107). Each block a v0.4.0 sequencer served in a recorded run (e2e/runs/zone-v040-standalone-*/
blocks/block-<n>.json, written by checkpoint/attestor/zone-v040.sh standalone) is read by both scripts' own functions
here, with their RPC call answered from the record: the id must be <n>, the status one of LEZ's three, the time a time
of 2026 or later, and the two scripts must agree. Skipped when no such record exists.
"""
import base64
import glob
import importlib.machinery
import importlib.util
import json
import os
import struct
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
TESTNET = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(TESTNET))
RECORDS = sorted(glob.glob(os.path.join(REPO, "e2e", "runs", "zone-v040-standalone-*", "blocks", "block-*.json")))


def module(name):
    path = os.path.join(TESTNET, "host", "bin", name)
    loader = importlib.machinery.SourceFileLoader(name.replace("-", "_"), path)
    spec = importlib.util.spec_from_loader(loader.name, loader)
    m = importlib.util.module_from_spec(spec)
    loader.exec_module(m)
    return m


def record(path):
    with open(path) as f:
        return json.load(f)


@unittest.skipUnless(RECORDS, "no recorded v0.4.0 block (e2e/runs/zone-v040-standalone-*/blocks/)")
class V040Blocks(unittest.TestCase):
    def test_both_readers(self):
        status = module("ferrata-status")
        identity = module("ferrata-bridge-identity")
        for path in RECORDS:
            n = int(os.path.basename(path).removeprefix("block-").removesuffix(".json"))
            answer = record(path)
            self.assertIsInstance(answer, str, path)
            raw = base64.b64decode(answer)
            status.rpc = lambda method, params=None, timeout=10, answer=answer: answer if method == "getBlock" else None
            identity.rpc = lambda method, params=None, answer=answer: answer if method == "getBlock" else None
            a, b = status.block(n), identity.block(n)
            with self.subTest(block=n):
                self.assertEqual(a["id"], n)
                self.assertEqual(b["id"], n)
                self.assertEqual(a["hash"], b["hash"])
                self.assertRegex(a["hash"], r"^[0-9a-f]{64}$")
                self.assertNotEqual(a["hash"], "00" * 32)
                self.assertIn(a["status"], ("Pending", "Safe", "Finalized"))
                self.assertEqual(a["status"], b["status"])
                self.assertGreaterEqual(a["time_ms"], 1767225600000)  # 2026-01-01
                self.assertEqual(a["bytes"], len(raw))
                # the header is block_id, prev_block_hash, hash, timestamp: the previous hash sits between id and hash
                self.assertEqual(struct.unpack("<Q", raw[0:8])[0], n)


if __name__ == "__main__":
    unittest.main(verbosity=1)
