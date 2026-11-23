#!/usr/bin/env python3
"""vault/script/deploy_core.py (what vault/script/deploy_sepolia.py runs for the owner) end to end on a LOCAL anvil whose
chain id is Sepolia's, with throwaway keys made here: every refusal of its docstring (nothing sent, the deployer's nonce
unchanged), the --dry path, the signed path with its preflight, idempotence, a resume after a stop, a lost record, an
underfunded deployer stopped by the preflight, and an inherited environment that changes nothing. Never Sepolia, never
the Secret: the endpoint is 127.0.0.1, and scripts/pubnet.py lets no public endpoint answer for a loopback one.

    python3 -I scripts/test_deploy_sepolia.py        (anvil, cast and forge on PATH; vault/lib installed and built)

Records go to vault/deployments/fork-test-*.json (ignored by git), logs to a temporary directory.
"""

import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
VAULT = ROOT / "vault"
sys.path.insert(0, str(VAULT / "script"))
import deploy_core as core  # noqa: E402

PORT, FORK_PORT = 8546, 8547
RPC = f"http://127.0.0.1:{PORT}"
ZERO = "0x" + "0" * 40
OLD_RELEASE = "0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9"  # release, RISC Zero 3.0.5
OLD_CHECKPOINT_B_0_3_5 = "0xafbf402314e9498f3854f02d533e2a355d4692ca2b64b913bcb0232200c63bcf"
PARAMS = json.loads((VAULT / "results" / "fork-rehearsal-31" / "params.json").read_text())


def sh(*a: str, check: bool = True) -> str:
    p = subprocess.run(list(a), capture_output=True, text=True, env=core.base_env())
    if check and p.returncode:
        raise RuntimeError(f"{' '.join(a[:3])}: {p.stderr.strip()[-300:]}")
    return p.stdout.strip()


def label(text: str) -> str:
    return "0x" + sh("cast", "keccak", text)[-40:]


class Anvil:
    def __init__(self, port: int, chain: int):
        self.url = f"http://127.0.0.1:{port}"
        self.p = subprocess.Popen(["anvil", "--chain-id", str(chain), "--port", str(port), "--silent"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=core.base_env())
        for _ in range(120):
            if subprocess.run(["cast", "chain-id", "--rpc-url", self.url], capture_output=True,
                              env=core.base_env()).returncode == 0:
                break
            time.sleep(0.25)
        else:
            raise RuntimeError("anvil did not start")
        self.mine(32)  # the tool forks five blocks behind the head

    def mine(self, n: int = 16) -> None:
        sh("cast", "rpc", "--rpc-url", self.url, "anvil_mine", hex(n))

    def stop(self) -> None:
        self.p.kill()
        self.p.wait()


class DeployToolTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for tool in ("anvil", "cast", "forge"):
            if not shutil.which(tool, path=core.base_env().get("PATH")):
                raise unittest.SkipTest(f"{tool} is not on PATH")
        if not (VAULT / "out" / "FerrataVault.sol" / "FerrataVault.json").exists():
            raise unittest.SkipTest("vault is not built (cd vault && forge build)")
        cls.tmp = tempfile.mkdtemp(prefix="test-deploy-")
        cls.chain = Anvil(PORT, 11155111)
        cls.keys = {k: cls.new_key(k) for k in ("main", "poor", "resume", "env")}
        for k in ("main", "resume", "env"):
            sh("cast", "rpc", "--rpc-url", RPC, "anvil_setBalance", cls.keys[k][0], hex(100 * 10**18))
        # "poor" keeps a balance of 0: anvil's base fee falls with empty blocks, so only nothing at all surely fails.
        cls.chain.mine()
        cls.F, cls.W, cls.W2 = label("test: freeze authority"), label("test: watcher"), label("test: watcher 2")
        cls.keyless = label(core.KEYLESS_LABEL)

    @classmethod
    def tearDownClass(cls):
        cls.chain.stop()
        for f in (VAULT / "deployments").glob("fork-test-*"):
            f.unlink()
        shutil.rmtree(cls.tmp, ignore_errors=True)

    @classmethod
    def new_key(cls, name: str) -> tuple[str, tuple[str, str]]:
        d = os.path.join(cls.tmp, f"key-{name}")
        os.makedirs(d)
        out = sh("cast", "wallet", "new", d, "--unsafe-password", f"pw-{name}")
        addr = re.search(r"0x[0-9a-fA-F]{40}", out).group(0)
        (path,) = [os.path.join(d, f) for f in os.listdir(d)]
        return addr, (pathlib.Path(path).read_text(), f"pw-{name}")

    def nonce(self, who: str) -> int:
        return int(sh("cast", "nonce", "--rpc-url", RPC, who))

    def predicted(self, who: str, k: int = 3) -> str:
        return sh("cast", "compute-address", who, "--nonce", str(self.nonce(who) + k)).split()[-1]

    def run_tool(self, who: str, dry: bool, name: str, **kw) -> list[str]:
        frm, key = self.keys[who]
        a = {"params": dict(PARAMS), "freeze_authority": self.F, "watchers": self.W, "expect_vault": self.predicted(frm),
             "name": name, "delta": 0}
        a.update(kw)
        return core.run(a, dry, RPC, frm, None if dry else key, vault_dir=str(VAULT), logs_root=os.path.join(self.tmp, "logs"),
                        deployer=frm, fork_port=FORK_PORT)

    def refused(self, who: str, fragment: str, **kw) -> None:
        frm = self.keys[who][0]
        n0, vault = self.nonce(frm), self.predicted(frm)
        with self.assertRaises(core.Stop) as ctx:
            self.run_tool(who, False, kw.pop("name", "fork-test-refused"), **kw)
        self.assertIn(fragment, str(ctx.exception))
        self.assertEqual(self.nonce(frm), n0, "nothing sent")
        self.assertEqual(sh("cast", "code", "--rpc-url", RPC, vault), "0x")

    # ---- refusals (decision D1 (a), the wait, the programs, the address) ------------------------------------------
    def test_01_refusals(self):
        F, W, frm = self.F, self.W, self.keys["main"][0]
        p = lambda **o: {**PARAMS, **o}  # noqa: E731
        cases = [
            ({"watchers": ZERO}, "a watcher cannot be 0x0"),
            ({"watchers": frm}, "a watcher cannot be the deployer"),
            ({"watchers": self.keyless}, "a watcher cannot be the keyless default"),
            ({"watchers": F}, "a watcher cannot be the freeze authority"),
            ({"watchers": f"{W},{W.lower()}"}, "the same watcher twice"),
            ({"watchers": ""}, "one to three watchers"),
            ({"watchers": ",".join(label(f"w{i}") for i in range(4))}, "one to three watchers"),
            ({"freeze_authority": ZERO}, "freeze authority cannot be 0x0"),
            ({"freeze_authority": frm}, "freeze authority cannot be the deployer"),
            ({"freeze_authority": self.keyless}, "freeze authority cannot be the keyless default"),
            ({"delta": 1}, "below rule (B)'s design wait"),
            ({"delta": 3599}, "below rule (B)'s design wait"),
            ({"params": p(checkpointImageIdB=OLD_CHECKPOINT_B_0_3_5)}, "is not a RISC Zero 3.1 checkpoint_b"),
            ({"params": p(releaseImageId=OLD_RELEASE)}, "is not the RISC Zero 3.1 release program"),
            ({"params": p(lezConfigId="0x" + "00" * 32)}, "lezConfigId must be the LEZ side's"),
            ({"params": p(nMin=0, tMin=0)}, "need 1 <= tMin <= nMin"),
            ({"params": p(nMin=1, tMin=2)}, "need 1 <= tMin <= nMin"),
            ({"expect_vault": "0x000000000000000000000000000000000000dEaD"}, "not --expect-vault"),
            ({"name": "fork-dry-x"}, "--name"),
            ({"name": "../x"}, "--name"),
        ]
        for kw, fragment in cases:
            with self.subTest(kw=list(kw), fragment=fragment):
                self.refused("main", fragment, **kw)
        # Another signer than the deployer; an endpoint of another chain.
        with self.assertRaises(core.Stop) as ctx:
            core.run({"params": dict(PARAMS)}, True, RPC, self.keys["env"][0], None, vault_dir=str(VAULT),
                     logs_root=self.tmp, deployer=frm)
        self.assertIn("not the vault deployer", str(ctx.exception))
        other = Anvil(PORT + 10, 31337)
        try:
            with self.assertRaises(core.Stop) as ctx:
                core.run({"params": dict(PARAMS)}, True, other.url, frm, None, vault_dir=str(VAULT), logs_root=self.tmp,
                         deployer=frm)
            self.assertIn("not Sepolia", str(ctx.exception))
        finally:
            other.stop()

    # ---- --dry: the whole deployment on a fork, nothing on the chain -----------------------------------------------
    def test_02_dry(self):
        frm = self.keys["main"][0]
        n0, vault = self.nonce(frm), self.predicted(frm)
        out = self.run_tool("main", True, "fork-test-main")
        self.assertTrue(any("--dry: complete" in line for line in out))
        self.assertTrue(any("--dry: 9 of 9 checks hold" in line for line in out))
        self.assertEqual(self.nonce(frm), n0)
        self.assertEqual(sh("cast", "code", "--rpc-url", RPC, vault), "0x")
        self.assertFalse((VAULT / "deployments" / "fork-test-main.json").exists())

    # ---- an underfunded deployer: the preflight stops it before anything is sent ------------------------------------
    def test_03_preflight_stops_an_underfunded_deployer(self):
        frm = self.keys["poor"][0]
        n0 = self.nonce(frm)
        with self.assertRaises(core.Stop) as ctx:
            self.run_tool("poor", False, "fork-test-poor")
        self.assertIn("preflight", str(ctx.exception))
        self.assertEqual(self.nonce(frm), n0, "the preflight's failure sent nothing")

    # ---- the signed path, end to end; then the same command again; then a lost record -------------------------------
    def test_04_signed_then_again_then_lost_record(self):
        frm = self.keys["main"][0]
        n0, vault = self.nonce(frm), self.predicted(frm)
        out = self.run_tool("main", False, "fork-test-main", watchers=f"{self.W},{self.W2}")
        text = "\n".join(out)
        self.assertIn("preflight: complete", text)
        self.assertIn("deployment: 9 of 9 checks hold", text)
        self.assertIn("script/sourcify.sh", text)  # the verification list, printed to the end
        steps = [line.split()[1] for line in out if line.strip().startswith("FERRATA_SENT")]
        self.assertEqual(len(steps), 16, "eight on the preflight's fork, eight on the chain")
        self.assertEqual(steps[-8:], ["groth16Verifier", "emergencyStop", "registry", "vault", "token", "renounceStopOwner",
                                      "registerETH", "registerToken"], "the stop renounced before any registration")
        self.assertEqual(self.nonce(frm), n0 + 8)
        self.assertNotEqual(sh("cast", "code", "--rpc-url", RPC, vault), "0x")
        rec = json.loads((VAULT / "deployments" / "fork-test-main.json").read_text())
        self.assertTrue(rec["complete"])
        self.assertIs(rec["uncapped"], False)
        self.assertEqual(rec["vault"].lower(), vault.lower())
        # The same command again: it reads the record, sends nothing. (The tool forks five blocks behind the head and
        # refuses while the deployer's last transactions are newer than that: blocks pass first.)
        self.chain.mine()
        out = self.run_tool("main", False, "fork-test-main", watchers=f"{self.W},{self.W2}", expect_vault=vault)
        self.assertIn("deployment: complete in", "\n".join(out))
        self.assertEqual(self.nonce(frm), n0 + 8)
        # The record lost: refused, nothing sent (a second vault would land elsewhere).
        (VAULT / "deployments" / "fork-test-main.json").unlink()
        with self.assertRaises(core.Stop) as ctx:
            self.run_tool("main", False, "fork-test-main", expect_vault=vault)
        self.assertIn("already at --expect-vault", str(ctx.exception))
        self.assertEqual(self.nonce(frm), n0 + 8)

    # ---- a stop part way (deploy.sh run directly with a gas cap under the library's estimate), then the tool resumes --
    def test_05_resume_after_a_stop(self):
        frm, (key_json, pw) = self.keys["resume"]
        n0, vault = self.nonce(frm), self.predicted(frm)
        kd = os.path.join(self.tmp, "resume-key")
        os.makedirs(kd)
        pathlib.Path(kd, "key.json").write_text(key_json)
        pathlib.Path(kd, "pw").write_text(pw)
        (VAULT / "deployments" / "fork-test-resume.params.json").write_text(json.dumps(PARAMS))
        env = core.base_env()
        env.update(FERRATA_MODE="B", FERRATA_GAS_RULE="eip8037", FERRATA_PARAMS="deployments/fork-test-resume.params.json",
                   ETH_FROM=frm, FERRATA_FREEZE_AUTHORITY=self.F, FERRATA_WATCHERS=self.W, FERRATA_EXPECT_VAULT=vault,
                   FERRATA_OUT="deployments/fork-test-resume.json", FERRATA_LOG_DIR=os.path.join(self.tmp, "resume-logs"),
                   FERRATA_RPC_URL=RPC, ETH_KEYSTORE=os.path.join(kd, "key.json"), ETH_PASSWORD=os.path.join(kd, "pw"),
                   FERRATA_TX_GAS_CAP="2000000")
        p = subprocess.run(["bash", "script/deploy.sh"], cwd=VAULT, env=env, capture_output=True, text=True)
        self.assertNotEqual(p.returncode, 0, "stopped by the gas cap")
        self.assertEqual(self.nonce(frm), n0 + 2, "the verifier and the stop sent, then the refusal")
        self.chain.mine()
        out = self.run_tool("resume", False, "fork-test-resume", expect_vault=vault)
        text = "\n".join(out)
        self.assertIn("resuming the record fork-test-resume: last step emergencyStop", text)
        self.assertIn("deployment: 9 of 9 checks hold", text)
        self.assertEqual(self.nonce(frm), n0 + 8)
        self.assertNotEqual(sh("cast", "code", "--rpc-url", RPC, vault), "0x")

    # ---- an inherited environment changes nothing (deploy.sh's environment is an allow-list) -----------------------
    def test_06_inherited_environment_is_ignored(self):
        frm = self.keys["env"][0]
        planted = {"FERRATA_UNCAPPED": "1", "FERRATA_GROTH16_VERIFIER": label("test: planted verifier"),
                   "FERRATA_RELEASE_DELAY": "1", "PUBNET_SEPOLIA_RPC": "https://example.invalid"}
        os.environ.update(planted)
        try:
            out = self.run_tool("env", True, "fork-test-env")
        finally:
            for k in planted:
                os.environ.pop(k, None)
        text = "\n".join(out)
        self.assertIn("--dry: 9 of 9 checks hold", text)  # capped, our own verifier 3.1.0
        self.assertIn(f"groth16Verifier  {sh('cast', 'compute-address', frm, '--nonce', str(self.nonce(frm))).split()[-1]}",
                      text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
