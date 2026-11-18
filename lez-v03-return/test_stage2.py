#!/usr/bin/env python3
"""The stage 2 measurement, offline (STAGE2.md §1):  python3 -I lez-v03-return/test_stage2.py

Over the committed fixtures: four real LEZ v0.3.1 blocks (83000 to 83003) and the transactions that inscribed them on
the public Logos Blockchain testnet. No network.
"""
import json
import os
import shutil
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import stage2  # noqa: E402

FIXTURES = os.path.join(HERE, "fixtures")


class Stage2Test(unittest.TestCase):
    def test_ed25519_rfc8032_vector(self):
        # RFC 8032 section 7.1, test 2 (a one-byte message).
        key = bytes.fromhex("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c")
        sig = bytes.fromhex("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
                            "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00")
        self.assertTrue(stage2.ed25519_verify(key, bytes.fromhex("72"), sig))
        self.assertFalse(stage2.ed25519_verify(key, bytes.fromhex("73"), sig))
        self.assertFalse(stage2.ed25519_verify(key, bytes.fromhex("72"), sig[:63] + bytes([sig[63] ^ 1])))

    def test_the_real_inscriptions_check_out(self):
        lines = stage2.check(FIXTURES)
        self.assertEqual(len(lines), 1 + len(stage2.BLOCKS))
        self.assertIn("4 accredited keys", lines[0])
        self.assertIn("t(n) = 3 of 4", lines[0])
        self.assertIn("9 public transactions (6 with a fee, 3 system)", lines[-1])

    def changed(self, name: str, edit) -> str:
        """A copy of the fixtures with one file edited."""
        base = os.environ.get("TMPDIR") or HERE
        os.makedirs(base, exist_ok=True)
        copy = tempfile.mkdtemp(dir=base)
        self.addCleanup(shutil.rmtree, copy)
        for f in os.listdir(FIXTURES):
            shutil.copy(os.path.join(FIXTURES, f), copy)
        path = os.path.join(copy, name)
        with open(path) as f:
            value = json.load(f)
        edit(value)
        with open(path, "w") as f:
            json.dump(value, f)
        return copy

    def test_a_changed_inscription_parent_signer_or_key_set_is_noticed(self):
        def flip_payload(tx):
            c = stage2.inscription_of(tx)["content"]
            c["inscription"] = c["inscription"][:200] + ("0" if c["inscription"][200] != "0" else "1") + c["inscription"][201:]

        def other_parent(tx):
            stage2.inscription_of(tx)["content"]["parent"] = "11" * 32

        def other_signer(tx):
            stage2.inscription_of(tx)["content"]["signer"] = \
                "a12da31c0df1e8cbd2416822b80864411d102355e049edc56609b076312259aa"

        for edit in (flip_payload, other_parent, other_signer):
            with self.assertRaises(AssertionError, msg=edit.__name__):
                stage2.check(self.changed("l1-inscription-83001.json", edit))

        def drop_key(op):
            op["content"]["keys"] = op["content"]["keys"][:3]  # the signing key is the fourth

        with self.assertRaises(AssertionError):
            stage2.check(self.changed("l1-channel-config.json", drop_key))

    def test_a_block_that_is_not_the_inscribed_one_is_noticed(self):
        def other_block(reply):
            import base64
            raw = bytearray(base64.b64decode(reply["result"]))
            raw[100] ^= 1
            reply["result"] = base64.b64encode(bytes(raw)).decode()

        with self.assertRaises(AssertionError):
            stage2.check(self.changed("testnet-block-83002.json", other_block))


if __name__ == "__main__":
    unittest.main(verbosity=2)
