#!/usr/bin/env python3
"""scripts/verifylib/lez040.py (LEZ v0.4.0's blocks, transactions and receipts; the version probe) against the vectors
LEZ v0.4.0's own crates made (deploy/testnet/web/wallet/test/fixtures/lez-v040-vectors.json, generator
deploy/testnet/web/lez040-vectors), its read-only client, and a loopback stand-in for the probe. Nothing leaves 127.0.0.1.

    python3 -I scripts/test_lez040_decode.py
"""
from __future__ import annotations

import http.server
import json
import sys
import threading
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from verifylib import lez, lez040  # noqa: E402

VECTORS = json.loads((ROOT / "deploy/testnet/web/wallet/test/fixtures/lez-v040-vectors.json").read_text())


def h(s: str) -> bytes:
    return bytes.fromhex(s)


class Decode(unittest.TestCase):
    def test_blocks(self):
        receipts = [json.loads(r["json"]) for r in VECTORS["receipts"]]
        for b in VECTORS["blocks"]:
            d = lez040.decode_block(h(b["borsh"]))
            self.assertEqual(d["header"]["hash"].hex(), b["hash"], b["name"])
            self.assertEqual(d["computed_hash"].hex(), b.get("computed_hash", b["hash"]))
            self.assertTrue(d["hash_ok"])
            self.assertEqual([t["hash"].hex() for t in d["transactions"]], b.get("tx_hashes", []))
            self.assertEqual(d["reverted"], b.get("reverted", []))
            self.assertEqual(d["bedrock_status"], "Pending")
            for got, want in zip(d["receipts"], receipts):
                self.assertEqual(got["success"], want["success"])
                for k in ("gas_exec", "gas_stor", "fee_base", "tip", "refund"):
                    self.assertEqual(got[k], want["fee"][k], k)
            if b["name"] == "four-transactions":
                self.assertEqual([lez040.outcome(d, i) for i in range(4)], ["applied", "reverted", "applied", "system"])
                self.assertEqual(d["receipts"][1]["fee_base"], 1 << 64)

    def test_public_transactions(self):
        for v in VECTORS["public"]:
            tx = lez040.decode_transaction(h(v["lee_transaction"]))
            p = v["parts"]
            self.assertEqual(tx["kind"], "Public")
            self.assertEqual(tx["hash"].hex(), v["tx_hash"])
            self.assertEqual(tx["raw"].hex(), v["lee_transaction"])
            self.assertEqual([(a["account_id"].hex(), a["program_account_id"].hex(), a["authority"].lower())
                              for a in tx["public_context"]["actors"]],
                             [(a["actor"]["account"], a["actor"]["program"], a["authority"]) for a in p["actors"]])
            self.assertEqual([a.hex() for a in tx["public_context"]["authorized_accounts"]], p["authorized_accounts"])
            self.assertEqual(tx["root"]["message"].hex(), p["data"])
            self.assertEqual([[n["account"].hex(), str(n["nonce"])] for n in tx["nonces"]], p["nonces"])
            self.assertEqual([s.hex() for s in tx["signers"]], v["signer_ids"])
            if p["fee"] is None:
                self.assertIsNone(tx["fee"])
            else:
                self.assertEqual((tx["fee"]["payer"].hex(), str(tx["fee"]["gas_limit"]), str(tx["fee"]["tip"]),
                                  str(tx["fee"]["max_fee"])),
                                 (p["fee"]["payer"], p["fee"]["gas_limit"], p["fee"]["tip"], p["fee"]["max_fee"]))
            ev = [{"key": e["initial_key"].hex()} if e["kind"] == "key"
                  else {"pda": {"program": e["program"].hex(), "seed": e["seed"].hex()}}
                  for e in tx["admission_evidence"]]
            self.assertEqual(ev, p["evidence"])
            self.assertEqual([{"auth_data": e["auth_data"].hex(), "next": e["next"].hex()} for e in tx["entries"]],
                             v["witness_entries"])

    def test_private_transaction(self):
        for v in VECTORS["private"]:
            tx = lez040.decode_transaction(h(v["lee_transaction"]))  # finish(): every byte consumed
            self.assertEqual(tx["kind"], "PrivacyPreserving")
            self.assertEqual(tx["hash"].hex(), v["tx_hash"])
            ex = tx["execution"]
            self.assertEqual(len(tx["proof"]), 40)
            self.assertEqual([m["kind"] for m in ex["async_messages"]], ["Clear", "Sealed"])
            self.assertEqual([s["kind"] for s in ex["boundary"]],
                             ["PrivateToPublic", "PublicToPrivate", "EndPrivateSubtree", "EndPublicSubtree"])
            self.assertEqual(ex["proven_fee"]["tip"], 11)
            self.assertEqual(ex["validity"], {"blocks": {"from": 10, "to": 20}, "timestamps": {"from": None, "to": 99}})
            self.assertEqual([c["kind"] for c in ex["program_image_claims"]], ["Disclosed", "Undisclosed"])
            self.assertEqual(tx["public_context"]["promotions"], [0, 3])
            self.assertEqual(len(tx["public_context"]["private_signers"]), 1)

    def test_refusals(self):
        b = VECTORS["blocks"][0]
        raw = h(b["borsh"])
        with self.assertRaises(lez.DecodeError):
            lez040.decode_block(raw + b"\x00")
        altered = bytearray(raw)
        altered[40] ^= 1
        self.assertFalse(lez040.decode_block(bytes(altered))["hash_ok"])
        tx = bytearray(h(VECTORS["public"][0]["lee_transaction"]))
        tx[0] = 7
        with self.assertRaises(lez.DecodeError):
            lez040.decode_transaction(bytes(tx))
        with self.assertRaises(lez.DecodeError):
            lez040.decode_transaction(h(VECTORS["public"][0]["lee_transaction"]) + b"\x00")
        # An rc3 block does not read as v0.4.0's, nor a v0.4.0 block as rc3's.
        with self.assertRaises(lez.DecodeError):
            lez.decode_block(raw)


class RealBlocks(unittest.TestCase):
    """Blocks a real LEZ v0.4.0 sequencer served (LEZ's own standalone sequencer; the fixture's `source`)."""

    def test_standalone_blocks(self):
        import base64
        real = json.loads((ROOT / "deploy/testnet/web/wallet/test/fixtures/lez-v040-standalone-blocks.json").read_text())
        b2 = lez040.decode_block(base64.b64decode(real["blocks"]["2"]))
        self.assertTrue(b2["hash_ok"])
        self.assertEqual([lez040.outcome(b2, i) for i in range(len(b2["transactions"]))], ["system", "system"])
        b3 = lez040.decode_block(base64.b64decode(real["blocks"]["3"]))
        self.assertTrue(b3["hash_ok"])
        self.assertEqual(b3["transactions"][0]["hash"].hex(), real["facts"]["transfer_tx"])
        self.assertEqual(lez040.outcome(b3, 0), "applied")
        r = real["facts"]["transfer_receipt"]
        self.assertEqual({k: b3["receipts"][0][k] for k in r["fee"]}, r["fee"])
        self.assertEqual(lez.b58encode(b3["transactions"][0]["signers"][0]), real["facts"]["test_account"].removeprefix("Public/"))


class Presentation(unittest.TestCase):
    """lez-read's own reading of real v0.4.0 blocks: v0.4.0's system programs by name, their messages decoded."""

    def reader(self, version):
        import importlib.util
        spec = importlib.util.spec_from_file_location("lez_read_040", ROOT / "scripts/lez-read.py")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod, mod.Reader("http://127.0.0.1:9", 512, version)  # nothing listens there; no probe when forced

    def test_system_programs_of_a_real_block(self):
        import base64
        real = json.loads((ROOT / "deploy/testnet/web/wallet/test/fixtures/lez-v040-standalone-blocks.json").read_text())
        b2 = lez040.decode_block(base64.b64decode(real["blocks"]["2"]))
        # The ids derived here are the ones a real v0.4.0 sequencer's clock and fee transactions name.
        self.assertEqual([t["root"]["to"]["program_account_id"] for t in b2["transactions"]],
                         [lez040.system_program_id("fee"), lez040.system_program_id("clock")])
        _, r = self.reader("v0.4.0")
        calls = [r.describe_tx(t)["call"] for t in b2["transactions"]]
        self.assertEqual([(c["program_name"], c.get("instruction")) for c in calls], [("fee", "distribute"), ("clock", "tick")])
        self.assertEqual(calls[1]["args"]["block_id"], 2)

    def test_system_upgrader_is_its_own_fixed_account(self):
        self.assertIn("system_upgrader", lez040.SYSTEM_PROGRAMS)
        self.assertEqual(lez040.system_program_id("system_upgrader"), bytes([0xFD]) * 32)
        _, r = self.reader("v0.4.0")
        self.assertEqual(r.builtins[lez.b58encode(lez040.SYSTEM_UPGRADER)], "system upgrader")

    def test_an_rc3_fallback_reads_v040_bytes_as_v040(self):
        mod, r = self.reader("v0.4.0-rc3")
        r.sure = False  # as after a failed probe
        raw = bytes.fromhex(VECTORS["blocks"][0]["borsh"])
        b = r.decode("decode_block", raw)
        self.assertEqual(r.lez_version, "v0.4.0")
        self.assertEqual(b["reverted"], [1])
        _, forced = self.reader("v0.4.0-rc3")  # forced rc3 stays rc3: the bytes are refused
        with self.assertRaises(lez.DecodeError):
            forced.decode("decode_block", raw)


class LiveRun(unittest.TestCase):
    """cli/results/v040-standalone-1: the clients against LEZ v0.4.0's standalone sequencer (cli/v040/live.sh)."""

    def test_recorded_blocks_hold_the_recorded_transfers(self):
        import base64
        run = ROOT / "cli/results/v040-standalone-1"
        facts = json.loads((run / "facts.json").read_text())
        self.assertTrue(facts["ok"] and facts["wallet_ok"])
        # Two transfers through the wallet's codec, a third through the page's network layer (wallet/net.js).
        self.assertEqual([s["nonce"] for s in facts["sends"]], ["0", "1", "2"])
        for s in facts["sends"]:
            b = lez040.decode_block(base64.b64decode(json.loads((run / "blocks" / f"block-{s['block']}.json").read_text())))
            self.assertTrue(b["hash_ok"])
            i = [t["hash"].hex() for t in b["transactions"]].index(s["hash"])
            self.assertEqual(lez040.outcome(b, i), "applied")
            self.assertEqual({k: b["receipts"][i][k] for k in s["receipt"]["fee"]}, s["receipt"]["fee"])
            # The witness carries the initial key at nonce 0 only (LEZ #1135): 33 + 64 + 32 bytes, then 64 + 32.
            self.assertEqual(len(b["transactions"][i]["entries"][0]["auth_data"]), 129 if s["nonce"] == "0" else 96)
            read = json.loads((run / f"lez-read-block-{s['block']}.json").read_text())
            self.assertEqual((read["lez"], read["hash_ok"]), ("v0.4.0", True))
            self.assertEqual(read["transactions"][i]["outcome"], "applied")


class Client(unittest.TestCase):
    def test_reads_only(self):
        self.assertEqual(lez040.Rpc040.ALLOWED - lez.Rpc.ALLOWED, {"getSequencerStatus", "getTransactionReceipt"})
        self.assertFalse([m for m in lez040.Rpc040.ALLOWED if m.startswith(("send", "submit", "requeue"))])
        rpc = lez040.Rpc040("http://127.0.0.1:9/rpc")  # nothing listens; refused before any request
        for m in ("sendTransaction", "submitSystemApproval", "requeueCrossZoneDeadLetter"):
            with self.assertRaises(lez.RpcError):
                rpc.call(m, ["00"])
        self.assertEqual(rpc.requests, 0)

    def test_classify(self):
        self.assertEqual(lez040.classify(200, b'{"jsonrpc":"2.0","id":1,"result":{"state":"online","height":1,"isOurTurn":true}}'), "v0.4.0")
        self.assertEqual(lez040.classify(200, b'{"jsonrpc":"2.0","id":1,"result":{"state":"bootstrapping","targetSlot":null,"replayedUpToSlot":null,"height":null}}'), "v0.4.0")
        self.assertEqual(lez040.classify(200, b'{"jsonrpc":"2.0","id":1,"error":{"code":-31902,"message":"x"}}'), "v0.4.0")
        self.assertEqual(lez040.classify(200, b'{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}'),
                         "before v0.4.0")
        self.assertEqual(lez040.classify(403, b'{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"not served"}}'),
                         "before v0.4.0")
        # scripts/test_verify.py's stand-in answers every method with null: no version, so lez-read falls back to rc3.
        for status, body in ((502, b"<html>"), (200, b'{"jsonrpc":"2.0","id":1,"error":{"code":-32603}}'),
                             (500, b'{"result":1}'), (200, b'{"jsonrpc":"2.0","id":1,"result":null}')):
            with self.assertRaises(lez.RpcError):
                lez040.classify(status, body)

    def test_probe_on_loopback(self):
        answers = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                assert req["method"] == "getSequencerStatus"
                status, body = answers.pop(0)
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *a):
                pass

        srv = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        url = f"http://127.0.0.1:{srv.server_address[1]}/rpc"
        try:
            answers.append((200, b'{"jsonrpc":"2.0","id":1,"result":{"state":"online","height":3,"isOurTurn":false}}'))
            self.assertEqual(lez040.probe(url), "v0.4.0")
            answers.append((403, b'{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}'))
            self.assertEqual(lez040.probe(url), "before v0.4.0")
            answers.append((502, b"<html>bad gateway</html>"))
            with self.assertRaises(lez.RpcError):
                lez040.probe(url)
        finally:
            srv.shutdown()
            srv.server_close()


if __name__ == "__main__":
    unittest.main(verbosity=1)
