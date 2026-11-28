#!/usr/bin/env python3
"""The test network gateway's JSON-RPC methods per LEZ tag (python3 -I deploy/testnet/tests/test_gateway_tag.py).

deploy/testnet/host/bin/ferrata-gateway serves rc3's read methods unchanged when FERRATA_LEZ_TAG is unset or rc3; on
LEZ v0.4.0 (FERRATA_LEZ_TAG=v0.4.0, which install.sh writes into /etc/ferrata/gateway.env) it also serves
getTransactionReceipt and getSequencerStatus (the v0.4.0 clients read receipts and probe the second to tell v0.4.0 from
rc3) and no longer getCrossZoneDeadLetters (cross-zone messaging is gone at v0.4.0). Each case loads the gateway in a
fresh interpreter with only that variable set and asks its own `cost_of` whether a request is served.
"""
import json
import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(os.path.dirname(HERE), "host", "bin", "ferrata-gateway")
INSTALL = os.path.join(os.path.dirname(HERE), "install.sh")

RC3_READS = {
    "checkHealth", "getFeeState", "getBlock", "getBlockRange", "getLastBlockId", "getAccountBalance",
    "getTransaction", "getAccountsNonces", "getAccountView", "getPublications", "getMessagePath",
    "getRecoveryBinding", "getProofsAndRoot", "getAccount", "getProgramIds", "getChannelId",
    "getCrossZoneDeadLetters",
}

PROBE = r"""
import importlib.machinery, importlib.util, json, sys
loader = importlib.machinery.SourceFileLoader("gateway_under_test", sys.argv[1])
spec = importlib.util.spec_from_loader(loader.name, loader)
gw = importlib.util.module_from_spec(spec)
loader.exec_module(gw)
served = {m: gw.cost_of([{"jsonrpc": "2.0", "id": 1, "method": m, "params": []}])[2] is None for m in json.loads(sys.argv[2])}
keys = {k: bool(gw.PK_RE.match(k)) for k in ("ab" * 32, "00" + "ab" * 32, "01" + "ab" * 32, "00" + "ab" * 31)}
print(json.dumps({"reads": sorted(gw.READ_METHODS), "served": served, "keys": keys, "form": gw.PK_FORM}))
"""


def gateway(tag, record=None):
    """The gateway's methods and faucet key form for FERRATA_LEZ_TAG=TAG on a host whose tag record holds RECORD
    (None: no record, as on today's rc3 host)."""
    env = {k: v for k, v in os.environ.items() if k != "FERRATA_LEZ_TAG"}
    if tag is not None:
        env["FERRATA_LEZ_TAG"] = tag
    scratch = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(HERE))), ".scratch")
    os.makedirs(scratch, exist_ok=True)
    env["FERRATA_LEZ_TAG_FILE"] = os.path.join(scratch, "test-gateway-tag-record-%d" % os.getpid())
    if record is None:
        if os.path.exists(env["FERRATA_LEZ_TAG_FILE"]):
            os.unlink(env["FERRATA_LEZ_TAG_FILE"])
    else:
        with open(env["FERRATA_LEZ_TAG_FILE"], "w") as f:
            f.write(record + "\n")
    methods = sorted(RC3_READS | {"getTransactionReceipt", "getSequencerStatus", "requeueCrossZoneDeadLetter",
                                  "submitSystemApproval", "sendTransaction"})
    p = subprocess.run([sys.executable, "-I", "-c", PROBE, GW, json.dumps(methods)], capture_output=True, text=True,
                       env=env, timeout=30)
    if p.returncode != 0:
        raise AssertionError(p.stderr)
    return json.loads(p.stdout)


class Methods(unittest.TestCase):
    def test_rc3_unchanged(self):
        for tag in (None, "v0.4.0-rc3"):
            g = gateway(tag)
            self.assertEqual(set(g["reads"]), RC3_READS, tag)
            self.assertFalse(g["served"]["getTransactionReceipt"], tag)
            self.assertFalse(g["served"]["getSequencerStatus"], tag)
            self.assertTrue(g["served"]["getCrossZoneDeadLetters"], tag)

    def test_v040_reads(self):
        g = gateway("v0.4.0")
        self.assertEqual(set(g["reads"]), (RC3_READS - {"getCrossZoneDeadLetters"}) | {"getTransactionReceipt", "getSequencerStatus"})
        self.assertTrue(g["served"]["getTransactionReceipt"])
        self.assertTrue(g["served"]["getSequencerStatus"])
        self.assertFalse(g["served"]["getCrossZoneDeadLetters"])

    def test_never_operator_methods(self):
        for tag in (None, "v0.4.0"):
            g = gateway(tag)
            self.assertFalse(g["served"]["requeueCrossZoneDeadLetter"], tag)
            self.assertFalse(g["served"]["submitSystemApproval"], tag)
            self.assertTrue(g["served"]["sendTransaction"], tag)

    def test_faucet_key_form(self):
        # a public account never used is named by its public key on rc3 (64 hex) and by its typed initial key on v0.4.0
        # (LEZ PR 1135: 00 and the 64-hex key, as the v0.4.0 web wallet sends it)
        for tag in (None, "v0.4.0-rc3"):
            g = gateway(tag)
            self.assertEqual(g["keys"], {"ab" * 32: True, "00" + "ab" * 32: False, "01" + "ab" * 32: False,
                                         "00" + "ab" * 31: True}, tag)          # (64 hex that happen to start 00)
            self.assertIn("64 hex digits", g["form"])
        g = gateway("v0.4.0")
        self.assertEqual(g["keys"], {"ab" * 32: False, "00" + "ab" * 32: True, "01" + "ab" * 32: False, "00" + "ab" * 31: False})
        self.assertIn("typed initial key", g["form"])

    def test_host_record_without_gateway_env(self):
        # a v0.4.0 host's record serves the gateway even when its unit carries no FERRATA_LEZ_TAG
        g = gateway(None, record="v0.4.0")
        self.assertTrue(g["served"]["getTransactionReceipt"])
        self.assertIn("typed initial key", g["form"])

    def test_install_writes_the_tag_for_the_gateway(self):
        with open(INSTALL) as f:
            text = f.read()
        self.assertIn("echo FERRATA_LEZ_TAG=v0.4.0 >> /etc/ferrata/gateway.env", text)
        self.assertIn("sed -i 's/^FERRATA_LEZ_TAG=.*/FERRATA_LEZ_TAG=v0.4.0/' /etc/ferrata/gateway.env", text)
        with open(os.path.join(os.path.dirname(HERE), "host", "systemd", "ferrata-gateway.service")) as f:
            self.assertIn("EnvironmentFile=-/etc/ferrata/gateway.env", f.read())


if __name__ == "__main__":
    unittest.main(verbosity=1)
