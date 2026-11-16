"""Tests of scripts/m0.py's checks 2 (the gas drop) and 4 (the real proofs on the newest LEZ tag):
`python3 -I scripts/test_m0.py`.

Standard library only; nothing is written to disk. The summaries are built here in the shapes
lez/scripts/gas-drop-summary.py and lez/scripts/real-proofs-summary.py write.
"""

import copy
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("m0", os.path.join(HERE, "m0.py"))
m0 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m0)


def item(key, passed=True, status="applied", bedrock="Finalized"):
    return {"key": key, "what": f"private {key}", "session": "s", "manifest": "s/run.tsv", "found": True,
            "step": f"{key} (real proof)", "status": status, "block": 10, "bedrock_status": bedrock,
            "final_after_s": 20.0, "prove_and_submit_s": 400.0, "tx_bytes": 250_000, "receipt_bytes": 230_000,
            "real_proof": True, "passed": passed}


def summary(tag="v0.4.0-rc3", dev_mode=False):
    return {"tag": tag, "sessions": {"s": {"tag": tag, "sequencer_dev_mode": dev_mode}},
            "fake_receipt": {"status": "dropped", "note": ""},
            "items": [item(k) for k in m0.REAL_PROOF_ITEMS], "passed": 4}


class RealProofs(unittest.TestCase):
    def test_all_four_on_the_newest_tag_is_go(self):
        verdict, value, rows = m0.real_proofs_verdict(summary(), "v0.4.0-rc3")
        self.assertEqual(verdict, "GO")
        self.assertIn("4 of 4", value)
        self.assertEqual(sum(1 for r in rows if r[2] == "pass"), 4)

    def test_one_missing_is_no_go(self):
        for key in m0.REAL_PROOF_ITEMS:
            s = summary()
            s["items"] = [i for i in s["items"] if i["key"] != key]
            verdict, value, _ = m0.real_proofs_verdict(s, "v0.4.0-rc3")
            self.assertTrue(verdict.startswith("NO-GO"), key)
            self.assertIn(key, value)

    def test_not_passed_is_no_go(self):
        s = summary()
        s["items"][3] = item("burn", passed=False, status="dropped")
        verdict, value, _ = m0.real_proofs_verdict(s, "v0.4.0-rc3")
        self.assertTrue(verdict.startswith("NO-GO"))
        self.assertIn("3 of 4", value)

    def test_another_tag_is_no_go(self):
        verdict, value, _ = m0.real_proofs_verdict(summary(tag="v0.4.0-rc2"), "v0.4.0-rc3")
        self.assertTrue(verdict.startswith("NO-GO"))
        self.assertIn("v0.4.0-rc2", value)

    def test_a_dev_mode_sequencer_is_no_go(self):
        for dev in (True, None):
            verdict, _, _ = m0.real_proofs_verdict(summary(dev_mode=dev), "v0.4.0-rc3")
            self.assertTrue(verdict.startswith("NO-GO"), dev)

    def test_a_newer_tag_without_a_record_is_no_go(self):
        facts = copy.deepcopy(m0.FACTS)
        try:
            m0.FACTS["lez_newest_tag"] = ("v9.9.9-test", "test")
            c = m0.check_real_proofs(online=False)
            self.assertTrue(c["verdict"].startswith("NO-GO"))
            self.assertIn("no real-proof record", c["value"])
        finally:
            m0.FACTS.clear()
            m0.FACTS.update(facts)


def gas_drop(tag="v0.4.0-rc3", dev_mode=False, failed=(), claim_status="applied", bedrock="Finalized", real=True):
    checks = {k: {"passed": k not in failed, "read": f"{k} as read"} for k in m0.GAS_DROP_CHECKS}
    return {"tag": tag, "session": {"tag": tag, "sequencer_dev_mode": dev_mode}, "checks": checks,
            "passed": not failed, "failed": list(failed),
            "claim": {"found": True, "status": claim_status, "block": 120, "bedrock_status": bedrock, "real_proof": real,
                      "prove_and_submit_s": 280.0, "tx_bytes": 252_000, "receipt_bytes": 237_000, "fee": "6600000"},
            "accounting": {"credit": "20000000", "feeCharged": "6600000", "creditEqualsNotePlusRefundPlusFee": True}}


class GasDrop(unittest.TestCase):
    def test_a_passing_sweep_on_the_newest_tag_is_go(self):
        verdict, value, rows = m0.gas_drop_verdict(gas_drop(), "v0.4.0-rc3")
        self.assertEqual(verdict, "GO")
        self.assertIn("swept", value)
        self.assertEqual(sum(1 for r in rows if r[2] == "pass"), len(m0.GAS_DROP_CHECKS))
        self.assertNotIn(m0.GAS_DROP_OTHERWISE, rows)

    def test_each_failed_check_is_no_go_with_the_fallback(self):
        for key in m0.GAS_DROP_CHECKS:
            verdict, value, rows = m0.gas_drop_verdict(gas_drop(failed=(key,)), "v0.4.0-rc3")
            self.assertTrue(verdict.startswith("NO-GO"), key)
            self.assertIn(key, value)
            self.assertIn(m0.GAS_DROP_OTHERWISE, rows)

    def test_nonzero_balances_before_or_an_unrefused_control_are_no_go(self):
        for key in ("zero_before", "negative"):
            verdict, _, _ = m0.gas_drop_verdict(gas_drop(failed=(key,)), "v0.4.0-rc3")
            self.assertTrue(verdict.startswith("NO-GO"), key)

    def test_a_dev_mode_or_unknown_sequencer_is_no_go(self):
        for dev in (True, None):
            verdict, value, _ = m0.gas_drop_verdict(gas_drop(dev_mode=dev), "v0.4.0-rc3")
            self.assertTrue(verdict.startswith("NO-GO"), dev)
            self.assertIn("RISC0_DEV_MODE", value)

    def test_another_tag_is_no_go(self):
        verdict, value, _ = m0.gas_drop_verdict(gas_drop(tag="v0.4.0-rc2"), "v0.4.0-rc3")
        self.assertTrue(verdict.startswith("NO-GO"))
        self.assertIn("v0.4.0-rc2", value)

    def test_a_claim_not_real_applied_and_final_is_no_go(self):
        for kw in ({"real": False}, {"claim_status": "dropped"}, {"bedrock": "Pending"}):
            verdict, _, _ = m0.gas_drop_verdict(gas_drop(**kw), "v0.4.0-rc3")
            self.assertTrue(verdict.startswith("NO-GO"), kw)

    def test_no_record_on_the_newest_tag_is_open(self):
        facts = copy.deepcopy(m0.FACTS)
        try:
            m0.FACTS["lez_newest_tag"] = ("v9.9.9-test", "test")
            c = m0.check_gas_drop()
            self.assertTrue(c["verdict"].startswith("OPEN"))
            self.assertIn("no gas-drop record for v9.9.9-test", c["value"])
            self.assertIn(m0.GAS_DROP_OTHERWISE, c["rows"])
        finally:
            m0.FACTS.clear()
            m0.FACTS.update(facts)


if __name__ == "__main__":
    unittest.main()
