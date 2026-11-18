#!/usr/bin/env python3
"""Tests of the stand-in attestor (standin.py), standard library only:  python3 -I lez-v03-return/test_standin.py

The LEZ node is a local stub that serves LEZ v0.3.1's `getLastBlockId`, `getBlock` and `getAccount` answers: REAL
blocks of the public LEZ testnet (fixtures/, read on 18 November 2026: 83000 and 83001 Finalized, 83002 and 83003
Pending) and a bridge account whose shard holds a burn accumulator computed natively. Nothing here talks to a network.
"""
import base64
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import standin as s  # noqa: E402

PROGRAM = s.b58encode(s.sha256(b"test: bridge program"))
ACCOUNT = s.b58encode(s.sha256(b"test: bridge accumulator account"))
CONFIG = s.sha256(b"test: LEZ bridge configuration")
VAULT = "0x" + "ab" * 20
VERIFIER = "0x" + "cd" * 20
LAYOUT = {"program": PROGRAM, "account": ACCOUNT, "burn_count": {"offset": 0, "length": 8},
          "burn_root": {"offset": 8, "length": 32}, "burn_frontier": {"offset": 40, "length": 1024},
          "imported_leaf_count": {"offset": 1064, "length": 8}, "config_id": {"offset": 1072, "length": 32}}


def jl(path: str):
    with open(path) as f:
        return json.load(f)


def real_block(block_id: int) -> str:
    return jl(os.path.join(HERE, "fixtures", f"testnet-block-{block_id}.json"))["result"]


def public_tx(program: bytes, shards: list = (), data: bytes = b"") -> bytes:
    """borsh of a v0.3.1 public transaction: tag 0, Message {program_account_id, shard_selectors, nonces,
    instruction_data, fee: None}, WitnessSet {} (standin.parse_block reads it back)."""
    return (b"\x00" + program + len(shards).to_bytes(4, "little") + b"".join(a + p for a, p in shards)
            + bytes(4) + len(data).to_bytes(4, "little") + data + b"\x00" + bytes(4))


def made_block(block_id: int, prev: bytes, status: int, body: bytes = bytes(4)) -> str:
    head = block_id.to_bytes(8, "little") + prev
    ts = (1_794_885_220_602 + block_id).to_bytes(8, "little")
    digest = s.sha256(s.BLOCK_PREFIX + head + ts + body)
    return base64.b64encode(head + digest + ts + body + bytes([status])).decode()


class Node:
    """What the stub answers; tests change it."""

    def __init__(self):
        self.blocks = {i: real_block(i) for i in (83000, 83001, 83002)}
        self.head = 83002
        self.shard = b""
        self.accounts = {}
        self.calls = []

    def set_accumulator(self, leaves: list, imported: int = 2, config: bytes = CONFIG, root: bytes | None = None):
        self.shard = (len(leaves).to_bytes(8, "little") + (root or s.tree_root(leaves))
                      + b"".join(s.tree_frontier(leaves)) + imported.to_bytes(8, "little") + config)

    def answer(self, method: str, params: list):
        self.calls.append(method)
        if method == "getLastBlockId":
            return self.head
        if method == "getBlock":
            return self.blocks.get(params[0])
        if method == "getAccount":
            shards = {PROGRAM: list(self.shard)} if params[0] == ACCOUNT else {
                k: list(v) for k, v in self.accounts.get(params[0], {}).items()}
            return {"nonce": 0, "data": {"shards": shards}}
        raise AssertionError(f"the tool called {method}")


NODE = Node()


class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "result": NODE.answer(req["method"], req["params"])}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


class StandInTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.rpc = f"http://127.0.0.1:{cls.server.server_address[1]}"
        cls.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR") or HERE)
        cls.dir = cls.tmp.name
        cls.keys = os.path.join(cls.dir, "keys")
        cls.signers = json.loads(cls.tool("keygen", "--dir", cls.keys).stdout)["signers"]
        s.write(os.path.join(cls.dir, "identity.json"), {"chainId": 11155111, "vault": VAULT, "configId": s.hx(CONFIG)})
        s.write(os.path.join(cls.dir, "layout.json"), LAYOUT)
        cls.tool("native-burns", "--identity", cls.path("identity.json"), "--recipient", "0x" + "11" * 20,
                 "--out", cls.dir)
        burns = jl(cls.path("burns.json"))
        cls.leaves = [s.unhex(b["leaf"], 32) for b in burns["burns"]]
        cls.root = burns["burn_root"]

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.tmp.cleanup()

    @classmethod
    def path(cls, name: str) -> str:
        return os.path.join(cls.dir, name)

    @staticmethod
    def tool(*args, ok: bool = True):
        p = subprocess.run([sys.executable, "-I", os.path.join(HERE, "standin.py"), *args], capture_output=True,
                           text=True, timeout=120)
        if ok and p.returncode != 0:
            raise AssertionError(p.stderr[-600:])
        return p

    def setUp(self):
        NODE.__init__()
        NODE.set_accumulator(self.leaves)

    def checkpoint(self, *extra, ok: bool = True, layout: bool = True, block: int | None = 83001):
        args = ["checkpoint", "--identity", self.path("identity.json"), "--burns", self.path("burns.json"),
                "--parent-state", "0x" + "22" * 32, "--signers", ",".join(self.signers), "--threshold", "2",
                "--rpc", self.rpc, "--out", self.path("checkpoint.json"), *extra]
        if layout:
            args += ["--layout", self.path("layout.json")]
        if block is not None:
            args += ["--block", str(block)]
        return self.tool(*args, ok=ok)

    # ---- the selftest and the real blocks ---------------------------------------------------------------------------

    def test_selftest(self):
        self.assertIn("ok", self.tool("selftest").stdout)

    def test_real_testnet_blocks_hash_and_link(self):
        blocks = [s.decode_block(base64.b64decode(real_block(i))) for i in (83000, 83001, 83002)]
        for b in blocks:
            self.assertEqual(b["recomputed_hash"], b["hash"], "v0.3.1's block hash, recomputed from the contents")
        self.assertEqual(blocks[1]["prev"], blocks[0]["hash"])
        self.assertEqual(blocks[2]["prev"], blocks[1]["hash"])
        self.assertEqual([b["status"] for b in blocks], ["Finalized", "Finalized", "Pending"])

    def test_the_newest_final_block_is_found(self):
        # A chain of twelve blocks, seven of them final: the search returns block 7.
        NODE.blocks, prev = {}, bytes(32)
        for i in range(1, 13):
            NODE.blocks[i] = made_block(i, prev, 2 if i <= 7 else 0)
            prev = s.decode_block(base64.b64decode(NODE.blocks[i]))["hash"]
        NODE.head = 12
        lez = s.Lez(self.rpc)
        self.assertEqual(s.newest_final(lez)["id"], 7)
        self.assertEqual(s.final_tip(lez, None)["id"], 7)
        with self.assertRaises(SystemExit):
            s.final_tip(lez, 8)
        self.assertEqual(s.final_tip(lez, 8, show_only=True)["status"], "Pending")

    # ---- the accumulator against a natively computed root ----------------------------------------------------------

    def test_a_matching_accumulator_at_a_final_block_is_attested(self):
        # The state was read at head 83001: the tip is that block, which the node reports Finalized.
        NODE.head = 83001
        out = json.loads(self.checkpoint(block=None).stdout)
        self.assertEqual(out["burnRoot"], self.root)
        self.assertEqual(out["burnCount"], 2)
        self.assertEqual(out["tipId"], 83001)
        self.assertTrue(out["final_on_node"])
        record = jl(self.path("checkpoint.json"))
        self.assertTrue(record["source"]["accumulator_checked_against_node"])
        self.assertEqual(record["checkpoint"]["importedLeafCount"], 2)
        self.assertEqual(s.hx(s.checkpoint_journal(record["checkpoint"])), record["journal"])
        self.assertEqual(len(s.unhex(record["journal"])), 17 * 32)
        self.assertNotIn("sendTransaction", NODE.calls)

    def test_another_root_count_frontier_or_configuration_on_the_node_is_refused(self):
        NODE.head = 83001
        NODE.set_accumulator(self.leaves, root=s.sha256(b"another root"))
        self.assertIn("burn root", self.checkpoint(block=None, ok=False).stderr)
        NODE.set_accumulator(self.leaves[:1])
        self.assertIn("holds 1 burns", self.checkpoint(block=None, ok=False).stderr)
        NODE.set_accumulator(self.leaves + [s.sha256(b"a burn the wallet does not know")])
        self.assertIn("holds 3 burns", self.checkpoint(block=None, ok=False).stderr)
        NODE.set_accumulator(self.leaves, config=s.sha256(b"another configuration"))
        self.assertIn("configuration", self.checkpoint(block=None, ok=False).stderr)
        NODE.set_accumulator(self.leaves)
        NODE.shard = NODE.shard[:40] + s.sha256(b"x") + NODE.shard[72:]
        self.assertIn("frontier", self.checkpoint(block=None, ok=False).stderr)
        NODE.set_accumulator(self.leaves, imported=5)
        self.assertIn("--imported differs", self.checkpoint("--imported", "2", block=None, ok=False).stderr)

    def test_a_layout_that_reads_past_the_shard_is_refused(self):
        NODE.head = 83001
        NODE.shard = NODE.shard[:100]
        self.assertIn("the layout reads", self.checkpoint(block=None, ok=False).stderr)

    def test_a_tip_away_from_the_read_needs_a_layout_that_says_the_bridge_changes_only_in_root_calls(self):
        # The test layout does not say it: a tip below the head the state was read at is refused, named or not.
        NODE.head = 83002
        self.assertIn("changes_only_in_root_calls", self.checkpoint(block=None, ok=False).stderr)
        self.assertIn("changes_only_in_root_calls", self.checkpoint(block=83001, ok=False).stderr)

    # ---- lez-v03-bridge's own layout (borsh of its BridgeState), derived from the program id and the configuration ---

    def bridge_state(self, leaves: list, imported: int, deposit_root: bytes, variant: int = 5) -> str:
        """The stub holds `burns(cid)` and `root(cid, imported)` as lez-v03-bridge's core crate encodes them."""
        program = s.b58decode(PROGRAM)
        burns = bytes([variant]) + CONFIG + len(leaves).to_bytes(8, "little") + (32).to_bytes(4, "little") \
            + b"".join(s.tree_frontier(leaves))
        root = bytes([1]) + CONFIG + (11155111).to_bytes(8, "little") + s.unhex(VAULT, 20) \
            + imported.to_bytes(8, "little") + deposit_root + (11_563_328).to_bytes(8, "little") \
            + (12_119_428).to_bytes(8, "little")
        NODE.accounts = {
            s.b58encode(s.bridge_pda(program, b"burns", CONFIG)): {PROGRAM: burns},
            s.b58encode(s.bridge_pda(program, b"root", CONFIG, imported.to_bytes(8, "little"))): {PROGRAM: root}}
        self.tool("layout", "--program", PROGRAM, "--config-id", s.hx(CONFIG), "--imported", str(imported), "--out",
                  self.path("bridge-layout.json"))
        return self.path("bridge-layout.json")

    def bridge_checkpoint(self, layout: str, *extra, ok: bool = True):
        return self.tool("checkpoint", "--identity", self.path("identity.json"), "--burns", self.path("burns.json"),
                         "--parent-state", "0x" + "22" * 32, "--signers", ",".join(self.signers), "--threshold", "2",
                         "--rpc", self.rpc, "--layout", layout, "--out", self.path("checkpoint.json"), *extra, ok=ok)

    def test_the_bridges_own_layout_is_read_and_the_imported_root_compared_with_the_vaults(self):
        NODE.head = 83001
        deposit_root = s.sha256(b"test: the vault's deposit root at two leaves")
        layout = self.bridge_state(self.leaves, 2, deposit_root)
        out = json.loads(self.bridge_checkpoint(layout, "--deposit-root", s.hx(deposit_root)).stdout)
        self.assertEqual(out["burnRoot"], self.root)
        record = jl(self.path("checkpoint.json"))
        self.assertEqual(record["checkpoint"]["importedLeafCount"], 2)
        self.assertTrue(record["source"]["imported_root_is_the_vaults"])
        other = s.hx(s.sha256(b"another deposit root"))
        self.assertIn("not the vault's", self.bridge_checkpoint(layout, "--deposit-root", other, ok=False).stderr)

    def test_the_bridges_layout_refuses_another_state_kind_count_or_missing_account(self):
        NODE.head = 83001
        layout = self.bridge_state(self.leaves, 2, bytes(32), variant=4)
        self.assertIn("not laid out", self.bridge_checkpoint(layout, ok=False).stderr)
        layout = self.bridge_state(self.leaves[:1], 2, bytes(32))
        self.assertIn("holds 1 burns", self.bridge_checkpoint(layout, ok=False).stderr)
        layout = self.bridge_state(self.leaves, 2, bytes(32))
        NODE.accounts = {}  # nothing deployed at the derived addresses
        self.assertIn("has 0 bytes", self.bridge_checkpoint(layout, ok=False).stderr)

    # ---- a Finalized tip below the head (the blocks between are read) ---------------------------------------------

    def test_a_final_tip_below_the_head_is_attested_when_no_block_between_touches_the_bridge(self):
        # The real testnet blocks: 83001 Finalized, 83002 and 83003 Pending (11 public transactions of other programs).
        NODE.blocks[83003], NODE.head = real_block(83003), 83003
        layout = self.bridge_state(self.leaves, 2, bytes(32))
        out = json.loads(self.bridge_checkpoint(layout).stdout)
        self.assertEqual((out["tipId"], out["accumulator_read_at_head"], out["blocks_between"]), (83001, 83003, 2))
        src = jl(self.path("checkpoint.json"))["source"]
        self.assertTrue(src["final_on_node"] and src["accumulator_checked_against_node"])
        self.assertEqual([b["id"] for b in src["blocks_between_tip_and_read"]], [83002, 83003])
        self.assertEqual([b["transactions"] for b in src["blocks_between_tip_and_read"]], [2, 9])
        self.assertEqual(src["tip"]["hash"], s.hx(s.decode_block(base64.b64decode(real_block(83001)))["hash"]))
        # Named, the same block is taken the same way; the record is signed.
        self.bridge_checkpoint(layout, "--block", "83001")
        self.assertEqual(jl(self.path("checkpoint.json"))["checkpoint"]["tipId"], 83001)
        self.tool("sign", "--checkpoint", self.path("checkpoint.json"), "--verifier", VERIFIER, "--keys", self.keys,
                  "--out", self.path("candidate.json"))

    def pending_after_83001(self, body: bytes) -> None:
        h83001 = s.decode_block(base64.b64decode(real_block(83001)))["hash"]
        NODE.blocks[83002], NODE.head = made_block(83002, h83001, 0, body), 83002

    def test_a_bridge_transaction_between_the_tip_and_the_read_is_refused(self):
        layout = self.bridge_state(self.leaves, 2, bytes(32))
        program, burns_account = s.b58decode(PROGRAM), s.b58decode(jl(layout)["account"])
        other = s.sha256(b"test: another program")
        for what, tx in (("its root call", public_tx(program)),
                         ("a shard of the accumulator", public_tx(other, [(burns_account, other)])),
                         ("a shard under the bridge program", public_tx(other, [(s.sha256(b"x"), program)]))):
            self.pending_after_83001((2).to_bytes(4, "little") + public_tx(other) + tx)
            self.assertIn("carries a transaction of the bridge program", self.bridge_checkpoint(layout, ok=False).stderr,
                          what)
        # Another program's transaction alone: the tip below the head is taken.
        self.pending_after_83001((1).to_bytes(4, "little") + public_tx(other))
        self.assertEqual(json.loads(self.bridge_checkpoint(layout).stdout)["tipId"], 83001)

    def test_a_transaction_the_tool_does_not_read_between_the_tip_and_the_read_is_refused(self):
        # A privacy-preserving transaction (tag 1) could carry a private burn: it is not parsed, so it refuses.
        layout = self.bridge_state(self.leaves, 2, bytes(32))
        self.pending_after_83001((1).to_bytes(4, "little") + b"\x01" + bytes(64))
        self.assertIn("does not read", self.bridge_checkpoint(layout, ok=False).stderr)

    def test_a_head_replaced_during_the_read_is_not_taken(self):
        layout = self.bridge_state(self.leaves, 2, bytes(32))
        self.pending_after_83001((1).to_bytes(4, "little") + public_tx(s.sha256(b"first")))
        answer, replaced = NODE.answer, []

        def replacing(method, params):
            if method == "getAccount" and not replaced:
                replaced.append(1)  # another block 83002 (as the testnet's 83006 was replaced on 18 November)
                self.pending_after_83001((1).to_bytes(4, "little") + public_tx(s.sha256(b"second")))
            return answer(method, params)

        NODE.answer = replacing
        self.addCleanup(lambda: NODE.__dict__.pop("answer", None))
        self.bridge_checkpoint(layout)
        src = jl(self.path("checkpoint.json"))["source"]
        second = s.decode_block(base64.b64decode(NODE.blocks[83002]))["hash"]
        self.assertEqual(src["accumulator_read_at_hash"], s.hx(second), "read again under the block that stayed")

    # ---- finality ---------------------------------------------------------------------------------------------------

    def test_a_pending_tip_is_refused(self):
        # The real block 83002 is Pending on the public node: it is no tip.
        self.assertIn("83002 is Pending", self.checkpoint(block=83002, ok=False).stderr)

    def test_there_is_no_way_to_attest_a_pending_tip(self):
        # Named outright it is refused as well, and no flag lifts the refusal.
        self.assertIn("83002 is Pending", self.checkpoint(block=83002, ok=False).stderr)
        p = self.checkpoint("--allow-pending", block=83002, ok=False)
        self.assertIn("unrecognized arguments", p.stderr)
        p = self.checkpoint("--tip-id", "83002", block=None, ok=False)
        self.assertIn("unrecognized arguments", p.stderr)
        # `tip` only shows it.
        shown = json.loads(self.tool("tip", "--rpc", self.rpc, "--block", "83002").stdout)
        self.assertEqual(shown["tip"]["status"], "Pending")

    def test_the_wallets_burns_from_its_seed_are_the_drivers_notes(self):
        out = os.path.join(self.dir, "wallet")
        args = ["wallet-burns", "--identity", self.path("identity.json"), "--seed-text",
                "ferrata/lez-v03-bridge: testnet user bridge seed (test value)", "--recipient", "0x" + "11" * 20,
                "--out", out]
        self.tool(*args)
        burns = jl(os.path.join(out, "burns.json"))
        notes = jl(os.path.join(out, "burn-notes.json"))
        self.assertEqual([n["kind"] for n in notes], ["public", "private"])
        # The bridge driver's record of what it sent: {"notes": {step: {r, memo, mac, c_out, hash}}}.
        sent = {"notes": {f"burn-{n['kind']}": {"r": b["r"], "c_out": b["c_out"]} for n, b in zip(notes, burns["burns"])}}
        s.write(self.path("driver.json"), sent)
        self.assertTrue(json.loads(self.tool(*args, "--check", self.path("driver.json")).stdout)["checked"])
        sent["notes"]["burn-public"]["r"] = s.hx(s.sha256(b"another tag"))
        s.write(self.path("driver.json"), sent)
        self.assertIn("not the notes", self.tool(*args, "--check", self.path("driver.json"), ok=False).stderr)
        # The same wallet under the rehearsal's seed text is `native-burns`.
        self.tool("wallet-burns", "--identity", self.path("identity.json"), "--seed-text", s.REHEARSAL_SEED_TEXT,
                  "--recipient", "0x" + "11" * 20, "--out", out)
        self.assertEqual(jl(os.path.join(out, "burns.json"))["burn_root"], self.root)

    def test_a_block_whose_hash_is_not_its_contents_is_refused(self):
        NODE.head = 83001
        raw = bytearray(base64.b64decode(real_block(83001)))
        raw[100] ^= 1  # a byte of a transaction
        NODE.blocks[83001] = base64.b64encode(bytes(raw)).decode()
        self.assertIn("hash recomputed", self.checkpoint(block=None, ok=False).stderr)

    def test_a_block_that_does_not_link_to_its_parent_is_refused(self):
        NODE.head = 83001
        NODE.blocks[83000] = made_block(83000, bytes(32), 2)
        self.assertIn("does not link", self.checkpoint(block=None, ok=False).stderr)

    def test_a_head_that_moves_during_the_read_is_read_again(self):
        NODE.head = 83001
        answer, moved = NODE.answer, []

        def moving(method, params):
            if method == "getAccount" and not moved:
                moved.append(1)
                NODE.head = 83002  # a block lands between the two reads of the head
                NODE.blocks[83002] = made_block(83002, s.decode_block(base64.b64decode(real_block(83001)))["hash"], 2)
            return answer(method, params)

        NODE.answer = moving
        self.addCleanup(lambda: NODE.__dict__.pop("answer", None))
        out = json.loads(self.checkpoint(block=None).stdout)
        self.assertEqual(out["tipId"], 83002, "the state is attested at the head it was read under")

    def test_only_reads_and_only_https_or_local(self):
        with self.assertRaises(SystemExit):
            s.Lez("http://example.org")
        with self.assertRaises(SystemExit):
            s.Lez(self.rpc).call("sendTransaction", [])

    # ---- signing ----------------------------------------------------------------------------------------------------

    def signed(self, *extra):
        NODE.head = 83001
        self.checkpoint(block=None)
        self.tool("sign", "--checkpoint", self.path("checkpoint.json"), "--verifier", VERIFIER, "--keys", self.keys,
                  "--out", self.path("candidate.json"), *extra)
        return jl(self.path("candidate.json"))

    def test_a_threshold_signs_and_the_seal_recovers_to_the_set(self):
        cand = self.signed()
        self.assertEqual(len(s.unhex(cand["seal"])), 130)
        got = [s.hx(a) for a in s.seal_signers(cand)]
        self.assertEqual(got, cand["signedBy"])
        self.assertEqual(got, sorted(got))
        self.assertTrue(set(got) <= set(self.signers))
        self.assertEqual(self.tool("check-seal", "--candidate", self.path("candidate.json")).returncode, 0)
        digest = s.attestation_digest(11155111, s.unhex(VERIFIER, 20), s.unhex(cand["journalDigest"], 32))
        self.assertEqual(s.hx(digest), cand["attestationDigest"])

    def test_one_signature_is_below_the_threshold(self):
        self.signed("--count", "1")
        self.assertEqual(self.tool("check-seal", "--candidate", self.path("candidate.json"), ok=False).returncode, 1)

    def test_a_seal_is_for_one_verifier_chain_and_journal(self):
        cand = self.signed()
        for change in ({"verifier": "0x" + "ee" * 20}, {"chainId": 17000},
                       {"journal": cand["journal"][:-2] + "03"}):
            got = {s.hx(a) for a in s.seal_signers(dict(cand, **change))}
            self.assertFalse(got & set(self.signers), f"{list(change)}: the signers still recover")

    def test_key_files_are_private_and_not_overwritten(self):
        for name in os.listdir(self.keys):
            self.assertEqual(os.stat(os.path.join(self.keys, name)).st_mode & 0o777, 0o600)
        self.assertIn("already holds keys", self.tool("keygen", "--dir", self.keys, ok=False).stderr)
        loose = os.path.join(self.dir, "loose")
        self.tool("keygen", "--dir", loose, "--n", "1")
        os.chmod(os.path.join(loose, "signer-0.key"), 0o644)
        self.assertIn("mode 600", self.tool("signers", "--dir", loose, ok=False).stderr)

    def test_the_genesis_state_is_the_empty_state_at_the_tip(self):
        NODE.head = 83001
        out = json.loads(self.tool("genesis", "--rpc", self.rpc, "--signers", ",".join(self.signers), "--block",
                                   "83001").stdout)
        keys = s.signer_words(self.signers)
        want = s.lez_state(83001, s.unhex(out["tip"]["hash"], 32), s.key_set_hash(keys), bytes(32), 0,
                           [bytes(32)] * 32, 0, False)
        self.assertEqual(out["genesisState"], s.hx(want))
        # A block the node does not report Finalized is no genesis.
        NODE.head = 83002
        p = self.tool("genesis", "--rpc", self.rpc, "--signers", ",".join(self.signers), "--block", "83002", ok=False)
        self.assertIn("83002 is Pending", p.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
