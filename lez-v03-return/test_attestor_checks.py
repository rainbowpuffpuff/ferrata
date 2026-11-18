#!/usr/bin/env python3
"""The independent review's tests of the stand-in attestor (18 November 2026), kept here and turned around: each
counterexample it found (a signature without the bridge's state compared, a tip given by hand) is now a refusal, and
its cross-check of the accumulator's layout against the bridge port's own recorded run stays as it was.

    python3 -I lez-v03-return/test_attestor_checks.py

Standard library only, no network: the stub LEZ node and the real testnet blocks of test_standin.py, and the record
of lez-v03-bridge's local run on a v0.3.1 sequencer (lez-v03-bridge/results/local/).
"""
import base64
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import standin as s  # noqa: E402
import test_standin as t  # noqa: E402

PORT_RUN = os.path.join(HERE, "..", "lez-v03-bridge", "results", "local")
FORK_1 = os.path.join(HERE, "..", "e2e", "runs", "testnet-return-fork-1", "checkpoint", "checkpoint.json")


class LayoutMatchesThePort(unittest.TestCase):
    """The accumulator account and offsets standin.py derives match what the port wrote on a v0.3.1 sequencer."""

    def test_the_derived_layout_reads_the_ports_recorded_state(self):
        run = t.jl(os.path.join(PORT_RUN, "run.json"))
        st = t.jl(os.path.join(PORT_RUN, "state.json"))
        program, cid = s.b58decode(run["bridge"]), bytes.fromhex(run["config_id"])
        lay = s.bridge_layout(program, cid, 2)
        self.assertEqual(lay["account"], st["accounts"]["burns"]["id"], "burns(cid)")
        root_account = lay["imported_leaf_count"]["account"]
        self.assertEqual(root_account, st["accounts"]["root_2"]["id"], "root(cid, 2)")
        burns = bytes.fromhex(st["accounts"]["burns"]["shard"])
        root2 = bytes.fromhex(st["accounts"]["root_2"]["shard"])
        for e in lay["expect"]:
            data = root2 if e.get("account") == root_account else burns
            want = bytes.fromhex(e["hex"])
            self.assertEqual(data[e["offset"]:e["offset"] + len(want)], want, e["what"])
        self.assertEqual(len(burns), 45 + 32 * 32)
        self.assertEqual(burns[lay["config_id"]["offset"]:][:32], cid)
        count = int.from_bytes(burns[33:41], "little")
        self.assertEqual(count, st["burn_tree"]["count"])
        frontier = [burns[45 + 32 * h:77 + 32 * h] for h in range(s.TREE_DEPTH)]
        self.assertEqual(s.root_from_frontier(count, frontier).hex(), st["burn_tree"]["root"])
        self.assertEqual(int.from_bytes(root2[61:69], "little"), 2, "imported_leaf_count at 61")
        # The leaves recomputed from the port's burn notes, in the order they were sent, give the port's tree.
        notes = sorted(run["notes"].values(), key=lambda n: n["sent"])
        leaves = [s.burn_leaf(cid, i, bytes.fromhex(n["token"]), bytes.fromhex(n["amount"]), bytes.fromhex(n["c_out"]),
                              bytes.fromhex(n["r"])) for i, n in enumerate(notes)]
        self.assertEqual(s.tree_root(leaves).hex(), st["burn_tree"]["root"])
        self.assertEqual(s.tree_frontier(leaves), frontier)


class NothingUncheckedIsSigned(t.StandInTest):
    """The review's two counterexamples, now refused (review findings A1 and A2)."""

    def sign(self, checkpoint: str, ok: bool = False):
        return self.tool("sign", "--checkpoint", checkpoint, "--verifier", t.VERIFIER, "--keys", self.keys,
                         "--out", self.path("candidate-x.json"), ok=ok)

    def test_without_a_layout_nothing_is_checkpointed_and_burns_the_node_does_not_hold_are_refused(self):
        # The node's bridge holds NO burn; burns.json lists two.
        t.NODE.head = 83001
        t.NODE.set_accumulator([])
        p = self.checkpoint("--imported", "2", layout=False, block=83001, ok=False)
        self.assertIn("required: --layout", p.stderr)
        self.assertIn("holds 0 burns", self.checkpoint("--imported", "2", block=83001, ok=False).stderr)
        self.assertNotIn("sendTransaction", t.NODE.calls)

    def test_sign_refuses_a_checkpoint_whose_source_lacks_either_check(self):
        t.NODE.head = 83001
        self.checkpoint(block=None)
        good = t.jl(self.path("checkpoint.json"))
        self.sign(self.path("checkpoint.json"), ok=True)
        for change, why in ((("accumulator_checked_against_node", False), "both checks"),
                            (("final_on_node", False), "both checks"), (("kind", "given on the command line"),
                                                                        "both checks")):
            bad = json.loads(json.dumps(good))
            bad["source"][change[0]] = change[1]
            s.write(self.path("checkpoint-bad.json"), bad)
            self.assertIn(why, self.sign(self.path("checkpoint-bad.json")).stderr, change)
        bad = json.loads(json.dumps(good))
        del bad["source"]
        s.write(self.path("checkpoint-bad.json"), bad)
        self.assertIn("both checks", self.sign(self.path("checkpoint-bad.json")).stderr)
        bad = json.loads(json.dumps(good))
        bad["checkpoint"]["tipId"] += 1
        bad["journal"] = s.hx(s.checkpoint_journal(bad["checkpoint"]))
        s.write(self.path("checkpoint-bad.json"), bad)
        self.assertIn("not the tip its source checked", self.sign(self.path("checkpoint-bad.json")).stderr)

    def test_the_first_rehearsals_checkpoint_is_one_sign_now_refuses(self):
        # testnet-return-fork-1 was cut before this check: its source records the accumulator as not compared.
        cp = t.jl(FORK_1)
        self.assertFalse(cp["source"]["accumulator_checked_against_node"])
        self.assertIn("both checks", self.sign(FORK_1).stderr)

    def test_a_tip_given_by_hand_is_no_longer_an_option(self):
        pending = s.decode_block(base64.b64decode(t.real_block(83002)))
        self.assertEqual(pending["status"], "Pending")
        p = self.tool("checkpoint", "--identity", self.path("identity.json"), "--burns", self.path("burns.json"),
                      "--parent-state", "0x" + "22" * 32, "--signers", ",".join(self.signers), "--threshold", "2",
                      "--tip-id", "83002", "--tip-hash", s.hx(pending["hash"]), "--imported", "2",
                      "--out", self.path("checkpoint-pending.json"), ok=False)
        self.assertNotEqual(p.returncode, 0)
        self.assertFalse(os.path.exists(self.path("checkpoint-pending.json")))
        # Through the node the same tip is refused, as before.
        self.assertIn("83002 is Pending", self.checkpoint(block=83002, ok=False).stderr)


def load_tests(loader, tests, pattern):
    suite = unittest.TestSuite()
    suite.addTests(loader.loadTestsFromTestCase(LayoutMatchesThePort))
    for name in loader.getTestCaseNames(NothingUncheckedIsSigned):
        if name not in loader.getTestCaseNames(t.StandInTest):
            suite.addTest(NothingUncheckedIsSigned(name))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
