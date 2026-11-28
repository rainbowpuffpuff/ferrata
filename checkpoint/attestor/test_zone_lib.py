#!/usr/bin/env python3
"""Tests of checkpoint/attestor/zone-lib.sh: what the zone scripts need to know of each LEZ tag (rc1, rc3, v0.4.0).

    python3 -I checkpoint/attestor/test_zone_lib.py

Each test sources zone-lib.sh in a fresh bash and calls one function on files written into a temporary directory
under this tree's ignored .scratch/ (never /tmp). Where LEZ's trees are beside the repository (upstream/lez-<tag>,
scripts/fetch-upstream.sh), the tag facts are also checked against them; without them those checks are skipped.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
LIB = os.path.join(HERE, "zone-lib.sh")
REPO = os.path.dirname(os.path.dirname(HERE))
UPSTREAM = os.path.join(os.path.dirname(REPO), "upstream")
SCRATCH = os.path.join(REPO, ".scratch")


def zl(script, *args, check=True):
    """Run `script` (bash) after sourcing zone-lib.sh; $1.. are args. Returns (exit status, stdout)."""
    p = subprocess.run(["bash", "-c", f'set -euo pipefail; . "{LIB}"; ' + script, "zl", *args],
                       capture_output=True, text=True)
    if check and p.returncode != 0:
        raise AssertionError(f"{script!r} {args} exited {p.returncode}: {p.stderr}")
    return p.returncode, p.stdout


def read(path):
    with open(path) as f:
        return f.read()


def load(path):
    with open(path) as f:
        return json.load(f)


def lez_tree(tag):
    d = os.path.join(UPSTREAM, f"lez-{tag}")
    return d if os.path.isfile(os.path.join(d, "Cargo.toml")) else None


class Tmp(unittest.TestCase):
    def setUp(self):
        os.makedirs(SCRATCH, exist_ok=True)
        self.d = tempfile.mkdtemp(prefix="test-zone-lib-", dir=SCRATCH)

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def write(self, name, text):
        p = os.path.join(self.d, name)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(text)
        return p


class Tags(unittest.TestCase):
    def test_commits(self):
        want = {"v0.4.0-rc1": "1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21",
                "v0.4.0-rc2": "e3edf47402a82557103ca719b30d0bd2684338d1",
                "v0.4.0-rc3": "bafd07be0701398f9bc435f73ab05e9a7936c301",
                "v0.4.0": "a255b42c0fc80d1cb219454a3a1f5ec3b5d92115"}
        for tag, commit in want.items():
            self.assertEqual(zl('zl_lez_commit "$1"', tag)[1].strip(), commit)
            self.assertEqual(zl('zl_known_tag "$1"', tag)[0], 0)
        for bad in ("v0.4.0-rc4", "v0.4.1", "", "v0.4.0 "):
            self.assertNotEqual(zl('zl_lez_commit "$1"', bad, check=False)[0], 0, bad)
            self.assertNotEqual(zl('zl_known_tag "$1"', bad, check=False)[0], 0, bad)

    def test_commits_agree_with_lez_select(self):
        # lez/scripts/lez-select.sh pins the same commits for the tags it knows
        sel = read(os.path.join(REPO, "lez", "scripts", "lez-select.sh"))
        for tag in ("v0.4.0-rc1", "v0.4.0-rc2", "v0.4.0-rc3", "v0.4.0"):
            line = next((l for l in sel.splitlines() if l.strip().startswith(f"{tag})")), None)
            if line is None:
                continue  # the selector does not know this tag yet (v0.4.0 before P1's change)
            self.assertIn(zl('zl_lez_commit "$1"', tag)[1].strip(), line, tag)

    def test_only_v040_is_v040(self):
        self.assertEqual(zl('zl_is_v040 v0.4.0')[0], 0)
        for t in ("v0.4.0-rc3", "v0.4.0-rc1", "v0.4.0-rc4", ""):
            self.assertNotEqual(zl('zl_is_v040 "$1"', t, check=False)[0], 0, t)

    def test_r0vm_and_bedrock(self):
        self.assertEqual(zl("zl_r0vm_version v0.4.0")[1].strip(), "3.1.0")
        self.assertEqual(zl("zl_r0vm_version v0.4.0-rc3")[1].strip(), "3.0.5")
        self.assertEqual(zl("zl_r0vm_version v0.4.0-rc1")[1].strip(), "3.0.5")
        self.assertEqual(zl("zl_bedrock_release v0.4.0")[1].strip(), "0.4.0-rc.2")
        self.assertEqual(zl("zl_bedrock_release v0.4.0-rc3")[1].strip(), "0.3.5-rc.1")
        self.assertNotEqual(zl("zl_bedrock_release v9", check=False)[0], 0)

    def test_r0vm_agrees_with_lockfiles(self):
        seen = 0
        for tag in ("v0.4.0-rc3", "v0.4.0"):
            t = lez_tree(tag)
            if not t:
                continue
            lock = read(os.path.join(t, "Cargo.lock")).split("[[package]]")
            zkvm = [b for b in lock if '\nname = "risc0-zkvm"\n' in b]
            self.assertEqual(len(zkvm), 1, tag)
            version = zkvm[0].split('version = "')[1].split('"')[0]
            self.assertEqual(zl('zl_r0vm_version "$1"', tag)[1].strip(), version, tag)
            seen += 1
        if not seen:
            self.skipTest("no LEZ tree beside the repository")

    def test_bedrock_release_agrees_with_v040_lockfile(self):
        t = lez_tree("v0.4.0")
        if not t:
            self.skipTest("upstream/lez-v0.4.0 is not beside the repository")
        lock = read(os.path.join(t, "Cargo.lock"))
        self.assertIn("logos-blockchain.git?tag=0.4.0-rc.2#", lock)
        self.assertIn('logos-zone-sdk.git?tag=v0.2.0#', lock)


class Files(Tmp):
    def test_era_schema(self):
        v1 = self.write("v1.yaml", "eras:\n  0: !V1\n    x: 1\n")
        v2 = self.write("v2.yaml", "eras:\n  0: !V2\n    x: 1\n")
        none = self.write("none.yaml", "eras: {}\n")
        self.assertEqual(zl('zl_era_schema "$1"', v1)[1].strip(), "V1")
        self.assertEqual(zl('zl_era_schema "$1"', v2)[1].strip(), "V2")
        self.assertEqual(zl('zl_era_schema "$1"', none)[1].strip(), "")
        commented = self.write("c.yaml", "eras:\n    0: !V2   # the era of 0.4.0-rc.2\n")
        self.assertEqual(zl('zl_era_schema "$1"', commented)[1].strip(), "V2")
        self.assertEqual(zl('zl_era_schema "$1"', self.write("d.yaml", "eras:\n  10: !V2\n"))[1].strip(), "")
        for tag, era in (("v0.4.0-rc3", "V1"), ("v0.4.0", "V2")):
            t = lez_tree(tag)
            if t:
                self.assertEqual(zl('zl_era_schema "$1"', os.path.join(t, "bedrock", "deployment-settings.yaml"))[1].strip(), era, tag)

    def test_tag_of_tree(self):
        d = os.path.join(self.d, "upstream", "lez-v0.4.0")
        os.makedirs(d)
        self.assertEqual(zl('zl_tag_of_tree "$1"', d)[1].strip(), "v0.4.0")
        self.assertEqual(zl('zl_tag_of_tree "$1/"', d)[1].strip(), "v0.4.0")
        cfg = os.path.join(self.d, "bundle", "lez-config")
        self.write("bundle/lez-config/lez-tag.txt", "v0.4.0-rc3 bafd07be0701398f9bc435f73ab05e9a7936c301\n")
        self.assertEqual(zl('zl_tag_of_tree "$1"', cfg)[1].strip(), "v0.4.0-rc3")
        self.assertNotEqual(zl('zl_tag_of_tree "$1"', self.d, check=False)[0], 0)
        # a directory named for the tree's commit (spike/upstream/lez-a255b42c), and one of an unknown commit
        for name, tag in (("lez-a255b42c", "v0.4.0"), ("lez-bafd07be0701398f9bc435f73ab05e9a7936c301", "v0.4.0-rc3"),
                          ("lez-1806e873", "v0.4.0-rc1"), ("lez-0123abcd", "0123abcd"), ("lez-v0.3.1", "v0.3.1")):
            c = os.path.join(self.d, "upstream", name)
            os.makedirs(c, exist_ok=True)
            self.assertEqual(zl('zl_tag_of_tree "$1"', c)[1].strip(), tag, name)

    def test_find_r0vm(self):
        h1, h2 = os.path.join(self.d, "home1"), os.path.join(self.d, "home2")
        for h, v in ((h1, "3.0.5"), (h2, "3.1.0"), (h2, "3.0.5")):
            p = os.path.join(h, "extensions", f"v{v}-cargo-risczero-x86_64-unknown-linux-gnu")
            os.makedirs(p, exist_ok=True)
            with open(os.path.join(p, "r0vm"), "w") as f:
                f.write("#!/bin/sh\n")
            os.chmod(os.path.join(p, "r0vm"), 0o755)
        self.assertTrue(zl('zl_find_r0vm 3.1.0 "$1" "$2"', h1, h2)[1].strip().startswith(h2 + "/extensions/v3.1.0-"))
        self.assertTrue(zl('zl_find_r0vm 3.0.5 "$1" "$2"', h1, h2)[1].strip().startswith(h1 + "/"))  # the first home wins
        self.assertNotEqual(zl('zl_find_r0vm 3.2.0 "$1" "$2"', h1, h2, check=False)[0], 0)
        self.assertNotEqual(zl('zl_find_r0vm 3.1.0 "$1"', h1, check=False)[0], 0)
        # a file that is not executable is not an r0vm
        h3 = os.path.join(self.d, "home3")
        p = os.path.join(h3, "extensions", "v3.1.0-cargo-risczero-x86_64-unknown-linux-gnu")
        os.makedirs(p)
        with open(os.path.join(p, "r0vm"), "w") as f:
            f.write("not a program")
        os.chmod(os.path.join(p, "r0vm"), 0o644)
        self.assertNotEqual(zl('zl_find_r0vm 3.1.0 "$1"', h3, check=False)[0], 0)

    def test_node_loopback(self):
        text = ("network:\n  backend:\n    swarm:\n      node_key: ab\n"
                "api:\n  backend:\n    listen_address: 0.0.0.0:18080\n    cors_origins: []\n"
                "    # The compose service name\n    allowed_hosts: [logos-blockchain-node-0]\n    timeout: 30s\n")
        p = self.write("node-config.yaml", text)
        zl('zl_node_loopback "$1" 18555', p)
        out = read(p)
        self.assertIn("    listen_address: 127.0.0.1:18555\n", out)
        self.assertIn("    swarm:\n      host: 127.0.0.1\n      node_key: ab\n", out)
        # allowed_hosts stays as upstream has it (an IP there stops 0.4.0-rc.2's node at start-up)
        self.assertIn("    allowed_hosts: [logos-blockchain-node-0]\n", out)
        # rc3's configuration has no allowed_hosts: nothing is added
        p3 = self.write("node-config-rc3.yaml", text.replace("    allowed_hosts: [logos-blockchain-node-0]\n", ""))
        zl('zl_node_loopback "$1" 18080', p3)
        self.assertNotIn("allowed_hosts", read(p3))
        for tag in ("v0.4.0-rc3", "v0.4.0"):
            t = lez_tree(tag)
            if not t:
                continue
            q = os.path.join(self.d, f"node-{tag}.yaml")
            shutil.copy(os.path.join(t, "bedrock", "node-config.yaml"), q)
            zl('zl_node_loopback "$1" 18080', q)
            out = read(q)
            self.assertIn("listen_address: 127.0.0.1:18080", out, tag)
            self.assertNotIn("0.0.0.0", out, tag)
            self.assertIn("      host: 127.0.0.1\n", out, tag)
            if tag == "v0.4.0":
                self.assertIn("allowed_hosts: [logos-blockchain-node-0]", out)
                self.assertNotIn("127.0.0.1, localhost", out)


class Logs(Tmp):
    def test_log_ts(self):
        env_logger = "[2026-11-28T01:02:03Z INFO  sequencer_core::block_production] Block production scheduler started"
        tracing = "2026-11-28T01:02:03.456789Z  INFO sequencer_core::block_production: Block production scheduler started"
        coloured = "\x1b[2m2026-11-28T01:02:03.456789Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2msequencer_core\x1b[0m\x1b[2m:\x1b[0m started"
        for line in (env_logger, tracing, coloured):
            self.assertEqual(zl('zl_log_ts "$1"', line)[1].strip(), "2026-11-28T01:02:03", line)
        for line in ("no time here", "  2026-11-28T01:02:03Z indented", "[2026-11-28 01:02:03 INFO] other format", ""):
            self.assertEqual(zl('zl_log_ts "$1"', line)[1].strip(), "", line)

    def test_first_marker_ts(self):
        log = self.write("seq.log", "\n".join([
            "zone-bridge start 0.1",
            "[2026-11-28T01:00:00Z INFO  x] Block production scheduler started",
            "zone-bridge start 0.2",
            "2026-11-28T01:05:00.1Z  INFO x: something else",
            "\x1b[2m2026-11-28T01:05:07.250Z\x1b[0m  INFO sequencer_core: Block production scheduler started",
            "2026-11-28T01:06:00Z  INFO x: Block production scheduler started",
            ""]))
        pat = "Block production scheduler started"
        self.assertEqual(zl('zl_first_marker_ts "$1" "$2" "$3"', log, "zone-bridge start 0.2", pat)[1].strip(),
                         "2026-11-28T01:05:07")
        # the marker is matched as a whole line: start 0.1's own line is not found again after start 0.2
        self.assertEqual(zl('zl_first_marker_ts "$1" "$2" "$3"', log, "zone-bridge start 0.1", pat)[1].strip(),
                         "2026-11-28T01:00:00")
        rc, out = zl('zl_first_marker_ts "$1" "$2" "$3"', log, "zone-bridge start 0.3", pat, check=False)
        self.assertEqual(out.strip(), "")
        rc, out = zl('zl_first_marker_ts "$1" "$2" "$3"', log, "zone-bridge start 0.2", "never logged", check=False)
        self.assertEqual(out.strip(), "")


class Committee(unittest.TestCase):
    def test_threshold(self):
        want = {1: 2, 2: 2, 3: 2, 4: 3, 5: 4, 6: 4, 7: 5, 8: 6, 9: 6, 10: 7}
        for n, t in want.items():
            self.assertEqual(int(zl("zl_threshold $1", str(n))[1]), t, n)


DEVNET_V040 = {
    "home": ".", "bedrock_config": {"channel_id": "01" * 32, "node_url": "http://logos-blockchain-node-0:18080"},
    "gossip": {"listen_addr": "/ip4/0.0.0.0/udp/3001/quic-v1", "bootstrap_peers": []},
    "genesis": {"channel_params": {"minimum_sequencer_stake": 149, "posting_timeframe": 300, "posting_timeout": 25,
                                   "max_block_size": 1048576, "finality_depth": 35},
                "actions": [{"stake_sequencer": {"sequencer_key": [1, 2, 3]}},
                            {"supply_account": {"recipient": {"identified": {"key": "00" + "ab" * 32}}, "balance": 7}}]},
}
LOCAL_V040 = {"genesis": {"actions": [
    {"supply_account": {"recipient": {"identified": {"key": "00" + "ab" * 32}}, "balance": 7}},
    {"supply_account": {"recipient": {"identified": {"key": "00" + "cd" * 32}}, "balance": 9}}]}}


class SequencerConfig(Tmp):
    def cfg(self, devnet, local, tag, i=2, npeers=4, check=True):
        src = self.write("devnet.json", json.dumps(devnet))
        loc = self.write("local.json", json.dumps(local)) if local is not None else "-"
        dst = os.path.join(self.d, f"cfg-{i}.json")
        rc, _ = zl('zl_seq_cfg "$@"', src, loc, dst, "http://127.0.0.1:18080", str(i), str(npeers), tag, check=check)
        return rc, (load(dst) if rc == 0 else None)

    def test_v040_config(self):
        _, c = self.cfg(DEVNET_V040, LOCAL_V040, "v0.4.0")
        self.assertEqual(c["bedrock_config"]["node_url"], "http://127.0.0.1:18080")
        self.assertEqual(c["gossip"]["listen_addr"], "/ip4/127.0.0.1/udp/19102/quic-v1")
        self.assertEqual(c["gossip"]["bootstrap_peers"],
                         [f"/ip4/127.0.0.1/udp/{p}/quic-v1" for p in (19100, 19101, 19103)])
        self.assertEqual(c["metrics_listen_address"], "127.0.0.1:19202")
        # local.json's second account added once, the duplicate not
        keys = [a["supply_account"]["recipient"]["identified"]["key"] for a in c["genesis"]["actions"] if "supply_account" in a]
        self.assertEqual(keys, ["00" + "ab" * 32, "00" + "cd" * 32])
        self.assertEqual(c["genesis"]["channel_params"]["finality_depth"], 35)

    def test_no_local(self):
        _, c = self.cfg(DEVNET_V040, None, "v0.4.0", i=0, npeers=5)
        self.assertEqual(len(c["genesis"]["actions"]), 2)
        self.assertEqual(len(c["gossip"]["bootstrap_peers"]), 4)

    def test_tag_mismatch_refused(self):
        rc, _ = self.cfg(DEVNET_V040, LOCAL_V040, "v0.4.0-rc3", check=False)
        self.assertNotEqual(rc, 0)
        rc3 = json.loads(json.dumps(DEVNET_V040))
        params = rc3["genesis"]["channel_params"]
        del params["finality_depth"]
        params["exit_delay"] = 100
        self.assertNotEqual(self.cfg(rc3, LOCAL_V040, "v0.4.0", check=False)[0], 0)
        self.assertEqual(self.cfg(rc3, LOCAL_V040, "v0.4.0-rc3")[0], 0)
        # before v0.4.0 the genesis must carry exit_delay (rc1, rc2 and rc3's devnets all do)
        neither = json.loads(json.dumps(rc3))
        del neither["genesis"]["channel_params"]["exit_delay"]
        for tag in ("v0.4.0-rc3", "v0.4.0-rc1", "v0.4.0"):
            self.assertNotEqual(self.cfg(neither, LOCAL_V040, tag, check=False)[0], 0, tag)

    def test_only_public_accounts_added(self):
        bad = {"genesis": {"actions": [{"stake_sequencer": {"sequencer_key": [9]}}]}}
        self.assertNotEqual(self.cfg(DEVNET_V040, bad, "v0.4.0", check=False)[0], 0)

    def test_upstream_devnet_configs(self):
        seen = 0
        for tag in ("v0.4.0-rc3", "v0.4.0"):
            t = lez_tree(tag)
            if not t:
                continue
            src = os.path.join(t, "lez", "configs", "docker-all-in-one", "devnet", "sequencer_config.json")
            loc = os.path.join(t, "lez", "sequencer", "service", "networks", "local.json")
            dst = os.path.join(self.d, f"{tag}.json")
            zl('zl_seq_cfg "$@"', src, loc, dst, "http://127.0.0.1:18080", "1", "4", tag)
            other = "v0.4.0-rc3" if tag == "v0.4.0" else "v0.4.0"
            self.assertNotEqual(zl('zl_seq_cfg "$@"', src, loc, dst, "x", "1", "4", other, check=False)[0], 0, tag)
            seen += 1
        if not seen:
            self.skipTest("no LEZ tree beside the repository")


class Channel(unittest.TestCase):
    def summary(self, text):
        p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_channel_summary'], input=text, capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout.strip()

    def test_summary(self):
        self.assertEqual(self.summary(json.dumps({"accredited_keys": ["a", "b", "c", "d"], "tip_sequencer": "ab", "tip_slot": 0})),
                         "4 ab 0")
        self.assertEqual(self.summary(json.dumps({"accredited_keys": []})), "0 - -")
        self.assertEqual(self.summary(json.dumps({"accredited_keys": ["a"], "tip_sequencer": 0, "tip_slot": 12})), "1 0 12")
        self.assertEqual(self.summary("{}"), "- - -")
        self.assertEqual(self.summary("not json"), "- - -")


class WalletAndOperator(Tmp):
    def test_published_secrets(self):
        j = self.write("Justfile", "x:\n    just run-wallet account import public --secret " + "ab" * 32 + "\n"
                       "    just run-wallet account import public --private-key " + "cd" * 32 + "\n"
                       "    just run-wallet account import public --secret " + "ef" * 31 + "\n")
        self.assertEqual(zl('zl_published_secrets "$1"', j)[1].split(), ["ab" * 32])
        t = lez_tree("v0.4.0")
        if t:
            got = zl('zl_published_secrets "$1"', os.path.join(t, "Justfile"))[1].split()
            self.assertEqual(len(got), 2)
            self.assertTrue(all(len(x) == 64 for x in got))

    def test_wallet_config(self):
        src = self.write("wallet_config.json", '{\n    "sequencer": {\n        "sequencer_addr": "http://127.0.0.1:3040" \n    },\n    "x": 1\n}\n')
        dst = os.path.join(self.d, "out.json")
        zl('zl_wallet_config "$1" "$2" "$3"', src, dst, "http://127.0.0.1:19390")
        self.assertEqual(load(dst)["sequencer"]["sequencer_addr"], "http://127.0.0.1:19390")
        bad = self.write("bad.json", '{"sequencer": {"addr": "x"}}')
        self.assertNotEqual(zl('zl_wallet_config "$1" "$2" "$3"', bad, dst, "http://127.0.0.1:1", check=False)[0], 0)
        t = lez_tree("v0.4.0")
        if t:
            zl('zl_wallet_config "$1" "$2" "$3"', os.path.join(t, "lez", "wallet", "configs", "debug", "wallet_config.json"), dst, "http://127.0.0.1:19300")
            self.assertEqual(load(dst)["sequencer"]["sequencer_addr"], "http://127.0.0.1:19300")

    def test_public_id_and_tx_hash(self):
        out = ("Persistent storage not found\nImported public account Public/3DpQ7crHm9mYi59LvGA2jPtXT929hRULYKuJBBam3jEr\n"
               "Generated new account with account_id Public/81XH42vBEN4XLLDttQvv2uapGtoUVBtVgPCZgX8YYQHS at path /0\n")
        p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_public_id'], input=out, capture_output=True, text=True)
        self.assertEqual(p.stdout.strip(), "Public/3DpQ7crHm9mYi59LvGA2jPtXT929hRULYKuJBBam3jEr")
        p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_public_id'], input="Private/abc only\n", capture_output=True, text=True)
        self.assertEqual(p.stdout.strip(), "")
        # no id under set -e -o pipefail: an empty value for the caller's own check, not a silent stop of the script
        p = subprocess.run(["bash", "-c", f'set -euo pipefail; . "{LIB}"; x="$(printf "an error\\n" | zl_public_id)"; echo "reached [$x]"'],
                           capture_output=True, text=True)
        self.assertEqual((p.returncode, p.stdout.strip()), (0, "reached []"), p.stderr)
        txt = "Staking 149 from Public/x into Public/y: 0x" + "1f" * 32 + "\nIncluded in block 12\nnext " + "2e" * 32 + "\n"
        p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_tx_hash'], input=txt, capture_output=True, text=True)
        self.assertEqual(p.stdout.strip(), "2e" * 32)
        p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_tx_hash'], input="nothing\n", capture_output=True, text=True)
        self.assertEqual(p.stdout.strip(), "")
        # LEZ's wallet names the hash, then prints the transaction's data (other hashes) after it
        wallet = ("Transaction hash is " + "2e" * 32 + "\nTransaction is included in block 12\nTransaction data is "
                  "PrivacyPreserving(... commitment: " + "7a" * 32 + " nullifier: " + "9c" * 32 + " ...)\n")
        p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_tx_hash'], input=wallet, capture_output=True, text=True)
        self.assertEqual(p.stdout.strip(), "2e" * 32)

    def test_storage_sealed(self):
        sealed = self.write("s1.json", json.dumps({"version": 1, "kdf": {"salt": "00", "memory_kib": 1, "iterations": 1, "parallelism": 1},
                                                   "nonce": "00" * 12, "ciphertext": "ab" * 40}))
        plain = self.write("s2.json", json.dumps({"key_chain": {}, "accounts": []}))
        empty = self.write("s3.json", json.dumps({"version": 1, "kdf": {}, "nonce": "", "ciphertext": ""}))
        junk = self.write("s4.json", "not json")
        self.assertEqual(zl('zl_storage_sealed "$1"', sealed)[0], 0)
        for f in (plain, empty, junk, os.path.join(self.d, "missing.json")):
            self.assertNotEqual(zl('zl_storage_sealed "$1"', f, check=False)[0], 0, f)

    def test_no_founding_stakes(self):
        src = self.write("devnet.json", json.dumps(DEVNET_V040))
        dst = os.path.join(self.d, "nostakes.json")
        zl('zl_no_founding_stakes "$1" "$2"', src, dst)
        acts = load(dst)["genesis"]["actions"]
        self.assertEqual(acts, [a for a in DEVNET_V040["genesis"]["actions"] if "stake_sequencer" not in a])
        self.assertEqual(load(dst)["genesis"]["channel_params"], DEVNET_V040["genesis"]["channel_params"])
        # a configuration with no founding stakes is refused (the step would be a no-op the caller relies on)
        self.assertNotEqual(zl('zl_no_founding_stakes "$1" "$2"', dst, os.path.join(self.d, "x.json"), check=False)[0], 0)
        t = lez_tree("v0.4.0")
        if t:
            zl('zl_no_founding_stakes "$1" "$2"', os.path.join(t, "lez", "configs", "docker-all-in-one", "devnet", "sequencer_config.json"), dst)
            self.assertFalse(any("stake_sequencer" in a for a in load(dst)["genesis"]["actions"]))
            self.assertTrue(any("supply_account" in a for a in load(dst)["genesis"]["actions"]))

    def test_channel_param(self):
        def param(text, name):
            p = subprocess.run(["bash", "-c", f'. "{LIB}"; zl_channel_param "$1"', "zl", name], input=text, capture_output=True, text=True)
            self.assertEqual(p.returncode, 0, p.stderr)
            return p.stdout.strip()
        top = json.dumps({"accredited_keys": ["a"], "posting_timeframe": 60, "posting_timeout": 25})
        nested = json.dumps({"accredited_keys": ["a"], "params": {"posting_timeframe": 90}})
        self.assertEqual(param(top, "posting_timeframe"), "60")
        self.assertEqual(param(nested, "posting_timeframe"), "90")
        self.assertEqual(param(top, "configuration_threshold"), "")
        self.assertEqual(param("junk", "posting_timeframe"), "")


if __name__ == "__main__":
    unittest.main(verbosity=1)
