"""The checks and the deployment that vault/script/deploy_sepolia.py runs in its Modal container, without Modal, so that
scripts/test_deploy_sepolia.py can run them end to end on a local anvil with a throwaway key.

    run(args, dry, rpc, frm, key, vault_dir=..., logs_root=..., deployer=..., commit=..., fork_port=...)

Before anything is sent (decision D1 (a) of docs/design/risc0-3.1-migration.md, the vault deployed by
vault/script/deploy.sh and DeployVault.s.sol):
  - the signer is `deployer`, the endpoint is chain 11155111, nothing of the deployer's is pending;
  - the programs are docs/image-ids.md's RISC Zero 3.1 ids; lezConfigId and genesisState are set; 1 <= tMin <= nMin;
  - a freeze authority that is none of 0x0, the deployer and the keyless default of the November vaults; one to three
    watchers, none of them 0x0, the deployer, the keyless default or the freeze authority, no two alike;
  - Δ (`delta`, seconds) not below rule (B)'s design wait, 3,600 s (0: that wait);
  - the vault lands on `expect_vault` (the address the LEZ configuration names): predicted from the deployer's nonce
    and refused unless equal; a contract already there with no record of this tool is refused too;
  - without --dry, a preflight first: the whole deployment on an anvil fork of the same endpoint (every transaction
    simulated on its state, as the deployer, with its real balance); nothing is sent unless it completes and reads back.
After it: every value the deployment promises read back from the chain (verifier, stop, vault, roles, timers, caps).
deploy.sh's environment is an allow-list (no inherited FERRATA_* or PUBNET_* variable, no Secret value).
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import threading
import time

CHAIN = "11155111"
# docs/image-ids.md, RISC Zero 3.1 (risc0-zkvm 3.1.0, risc0-build 3.0.7).
IDS_31 = {
    "checkpoint_b": "0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0",
    "checkpoint_b_0_3_5": "0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04",
    "checkpoint": "0x7c93425e9b6fedbd0d79092b3a0b6c164b9a6ee250b9e2e89b26a7fadc4497c1",
    "checkpoint_0_3_5": "0x9140ec7479debc9b40529b0805095d4cfdef93a1e0500c6687a1d7ec4d7c42de",
    "release": "0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd",
}
RULE_B_DELTA = 3600  # DeployVault.s.sol RULE_B_DELTA: rule (B)'s design wait, the least this tool deploys
ZERO = "0x" + "0" * 40
ETH = ZERO
UNLIMITED = 2**256 - 1
KEYLESS_LABEL = "ferrata testnet: freeze authority (no key)"  # DeployVault.s.sol's default freeze authority
ENV_KEEP = ("PATH", "HOME", "LANG", "LC_ALL", "TMPDIR", "USER", "FOUNDRY_DIR", "SVM_HOME", "RUSTUP_HOME",
            "CARGO_HOME", "XDG_CACHE_HOME", "XDG_CONFIG_HOME")


class Stop(Exception):
    """A refusal or a failure: nothing more is sent."""


def base_env() -> dict:
    env = {k: os.environ[k] for k in ENV_KEEP if k in os.environ}
    env["FOUNDRY_DISABLE_NIGHTLY_WARNING"] = "1"
    return env


def _sh(args: list[str], env: dict | None = None, check: bool = True, hide: str = "", cwd: str | None = None) -> str:
    p = subprocess.run(args, capture_output=True, text=True, env=env if env is not None else base_env(), cwd=cwd)
    if check and p.returncode:
        msg = (p.stderr.strip() or p.stdout.strip())[-500:]
        raise Stop(f"{args[0]} {args[1] if len(args) > 1 else ''} failed: " + (msg.replace(hide, "<rpc>") if hide else msg))
    return p.stdout.strip()


def _addr(a: str, what: str) -> str:
    if not re.fullmatch(r"0x[0-9a-fA-F]{40}", (a or "").strip()):
        raise Stop(f"{what}: not an address: {a!r}")
    return a.strip()


def _int(text: str) -> int:
    """A cast answer: a decimal, perhaps followed by cast's ' [1e18]' gloss."""
    return int(text.split()[0])


class Run:
    def __init__(self, a: dict, rpc: str, frm: str, key: tuple[str, str] | None, *, vault_dir: str, logs_root: str,
                 deployer: str, commit=None, fork_port: int = 8545):
        self.a, self.rpc, self.frm, self.key = a, rpc, frm, key
        self.vault_dir, self.logs_root, self.deployer = vault_dir, logs_root, deployer
        self.commit, self.fork_port = commit, fork_port
        self.out: list[str] = []

    def say(self, line: str) -> None:
        print(line, flush=True)
        self.out.append(line)

    def cast(self, *x: str, rpc: str | None = None) -> str:
        return _sh(["cast", *x, *(["--rpc-url", rpc] if rpc else [])], hide=self.rpc)

    # ---- checks ---------------------------------------------------------------------------------------------------
    def check(self) -> None:
        a, frm, rpc = self.a, self.frm, self.rpc
        if frm.lower() != self.deployer.lower():
            raise Stop(f"ETH_FROM is {frm}, not the vault deployer {self.deployer}")
        chain = self.cast("chain-id", rpc=rpc)
        if chain != CHAIN:
            raise Stop(f"the endpoint is chain {chain}, not Sepolia ({CHAIN})")
        p = self.params = dict(a["params"])
        b, r, ca = (str(p.get(k, "")).lower() for k in ("checkpointImageIdB", "releaseImageId", "checkpointImageIdA"))
        self.layout = {IDS_31["checkpoint_b"]: "Logos Blockchain d480efc6 (LEZ v0.4.0-rc1)",
                       IDS_31["checkpoint_b_0_3_5"]: "Logos Blockchain 0.3.5-rc.1 (LEZ v0.4.0-rc3)"}.get(b)
        if not self.layout:
            raise Stop(f"checkpointImageIdB {b} is not a RISC Zero 3.1 checkpoint_b (docs/image-ids.md)")
        if r != IDS_31["release"]:
            raise Stop(f"releaseImageId {r} is not the RISC Zero 3.1 release program {IDS_31['release']}")
        if ca not in (IDS_31["checkpoint"], IDS_31["checkpoint_0_3_5"]):
            raise Stop(f"checkpointImageIdA {ca} is not a RISC Zero 3.1 checkpoint program (unused under rule B, recorded)")
        for k in ("lezConfigId", "genesisState"):
            if not re.fullmatch(r"0x[0-9a-fA-F]{64}", str(p.get(k, ""))) or int(p[k], 16) == 0:
                raise Stop(f"params: {k} must be the LEZ side's (32 bytes, not zero)")
        n, t = p.get("nMin"), p.get("tMin")
        if not (isinstance(n, int) and isinstance(t, int) and 1 <= t <= n):
            raise Stop(f"params: nMin {n!r}, tMin {t!r}: need 1 <= tMin <= nMin (the zone's key set and threshold)")
        keyless = "0x" + self.cast("keccak", KEYLESS_LABEL)[-40:]
        self.freeze = _addr(a.get("freeze_authority", ""), "--freeze-authority")
        refused = {ZERO: "0x0", frm.lower(): "the deployer", keyless.lower(): "the keyless default freeze authority"}
        if self.freeze.lower() in refused:
            raise Stop(f"decision D1 (a): the freeze authority cannot be {refused[self.freeze.lower()]}: a key someone holds")
        self.watchers = [_addr(w, "--watchers") for w in str(a.get("watchers", "")).split(",") if w.strip()]
        if not 1 <= len(self.watchers) <= 3:
            raise Stop("decision D1 (a): one to three watchers (--watchers, comma-separated)")
        refused[self.freeze.lower()] = "the freeze authority"
        for w in self.watchers:
            if w.lower() in refused:
                raise Stop(f"decision D1 (a): a watcher cannot be {refused[w.lower()]} ({w})")
        if len({w.lower() for w in self.watchers}) != len(self.watchers):
            raise Stop("decision D1 (a): the same watcher twice")
        self.delta = int(a.get("delta") or 0)
        if self.delta and self.delta < RULE_B_DELTA:
            raise Stop(f"--delta {self.delta} s is below rule (B)'s design wait, {RULE_B_DELTA} s (watchers need the time)")
        self.expect = _addr(a.get("expect_vault", ""), "--expect-vault")
        self.name = str(a.get("name", ""))
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}", self.name) or self.name.startswith("fork-dry-"):
            raise Stop(f"--name {self.name!r}: letters, digits, '.', '_', '-' (and not fork-dry-*)")
        self.say(f"chain {chain}; deployer {frm}; balance {self.cast('balance', '--ether', frm, rpc=rpc)} ETH")
        self.say(f"programs (RISC Zero 3.1): checkpoint_b {b} ({self.layout}), release {r}; lezConfigId "
                 f"{p['lezConfigId']}, genesisState {p['genesisState']}, nMin {n}, tMin {t}")
        self.say(f"D1 (a): freeze authority {self.freeze}, watchers {', '.join(self.watchers)}; Δ "
                 f"{self.delta or RULE_B_DELTA} s; the stop's owner renounced before the token registrations")
        self.latest = int(self.cast("nonce", "--block", "latest", frm, rpc=rpc))
        pending = int(self.cast("nonce", "--block", "pending", frm, rpc=rpc))
        if pending != self.latest:
            raise Stop(f"the deployer has a pending transaction (latest nonce {self.latest}, pending {pending}): wait")
        self.record = pathlib.Path(self.vault_dir, "deployments", f"{self.name}.json")
        rec = json.loads(self.record.read_text()) if self.record.exists() else None
        if rec:
            if rec.get("deployer", "").lower() != frm.lower() or str(rec.get("chainId")) != CHAIN:
                raise Stop(f"the record {self.name} is of another deployer or chain")
            if rec.get("vault") not in (None, ZERO) and rec["vault"].lower() != self.expect.lower():
                raise Stop(f"the record's vault {rec['vault']} is not --expect-vault {self.expect}")
            self.say(f"resuming the record {self.name}: last step {rec.get('lastStep')}, complete {rec.get('complete')}")
        else:
            if self.cast("code", self.expect, rpc=rpc) not in ("0x", ""):
                raise Stop(f"a contract is already at --expect-vault {self.expect} and there is no record {self.name}: "
                           "find the record (the Volume) before anything else is sent")
            names = ["Groth16 verifier 3.1.0", "emergency stop", "library FerrataRegistry", "vault", "test token"]
            predicted = [self.cast("compute-address", frm, "--nonce", str(self.latest + i)).split()[-1] for i in range(5)]
            for i, (n_, x) in enumerate(zip(names, predicted)):
                self.say(f"  nonce {self.latest + i}: {n_} at {x}")
            if predicted[3].lower() != self.expect.lower():
                raise Stop(f"from nonce {self.latest} the vault would be at {predicted[3]}, not --expect-vault "
                           f"{self.expect}: the LEZ configuration must name the address the deployer's nonce gives")

    # ---- one attempt: on a fork (dry, or the preflight) or on the endpoint -----------------------------------------
    def attempt(self, dry: bool, label: str) -> dict:
        rec_name = f"fork-dry-{self.name}" if dry else self.name
        deps = pathlib.Path(self.vault_dir, "deployments")
        deps.mkdir(parents=True, exist_ok=True)
        rec_path = deps / f"{rec_name}.json"
        if dry:  # the fork resumes exactly as the endpoint would: from the record of this name, if there is one
            rec_path.unlink(missing_ok=True)
            if self.record.exists():
                shutil.copy(self.record, rec_path)
        logs = pathlib.Path(self.logs_root, rec_name, time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
        logs.mkdir(parents=True, exist_ok=True)
        (deps / f"{rec_name}.params.json").write_text(json.dumps(self.params))  # forge may read ./deployments
        env = base_env()
        env.update(FERRATA_MODE="B", FERRATA_GAS_RULE="eip8037", FERRATA_PARAMS=f"deployments/{rec_name}.params.json",
                   ETH_FROM=self.frm, FERRATA_FREEZE_AUTHORITY=self.freeze, FERRATA_WATCHERS=",".join(self.watchers),
                   FERRATA_EXPECT_VAULT=self.expect, FERRATA_OUT=f"deployments/{rec_name}.json",
                   FERRATA_LOG_DIR=str(logs), FERRATA_GAS_MARGIN_PCT="125")
        if self.delta:
            env["FERRATA_DELTA"] = str(self.delta)
        anvil, keydir, stop_commits, committer = None, None, threading.Event(), None
        target = self.rpc
        try:
            if dry:
                # Five blocks behind the head: a load-balanced endpoint may not serve the head yet ("block not found").
                at = max(int(self.cast("block-number", rpc=self.rpc)) - 5, 0)
                anvil = subprocess.Popen(["anvil", "--fork-url", self.rpc, "--fork-block-number", str(at), "--port",
                                          str(self.fork_port), "--silent"], stdout=subprocess.DEVNULL,
                                         stderr=subprocess.DEVNULL, env=base_env())
                target = f"http://127.0.0.1:{self.fork_port}"
                for _ in range(120):
                    if subprocess.run(["cast", "chain-id", "--rpc-url", target], capture_output=True,
                                      env=base_env()).returncode == 0:
                        break
                    time.sleep(0.5)
                else:
                    raise Stop("the fork did not start")
                _sh(["cast", "rpc", "--rpc-url", target, "anvil_impersonateAccount", self.frm])
                if int(self.cast("nonce", self.frm, rpc=target)) != self.latest:
                    raise Stop(f"{label}: the deployer's nonce at block {at} is not its latest {self.latest} (a "
                               "transaction in the last blocks): wait a minute and run the same command again")
                self.say(f"{label}: an anvil fork of the endpoint at block {at}, the deployer impersonated with its real "
                         "balance; nothing is sent to the endpoint")
                env.update(FERRATA_RPC_URL=target, FERRATA_UNLOCKED="1", PUBNET_LAG="0")
            else:
                keydir = tempfile.mkdtemp(prefix="k-")
                os.chmod(keydir, 0o700)
                for fname, data in (("key.json", self.key[0]), ("pw", self.key[1])):
                    fd = os.open(os.path.join(keydir, fname), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                    with os.fdopen(fd, "w") as f:
                        f.write(data)
                env.update(FERRATA_RPC_URL=self.rpc, ETH_KEYSTORE=os.path.join(keydir, "key.json"),
                           ETH_PASSWORD=os.path.join(keydir, "pw"))
                self.say(f"{label}: signing with the deployer key; every transaction journaled before it leaves")
                if self.commit:  # the record and the journal reach the Volume while deploy.sh runs, not only at the end
                    def loop():
                        while not stop_commits.wait(15):
                            try:
                                self.commit()
                            except Exception as e:  # noqa: BLE001 (a failed background commit is retried)
                                print(f"(background commit failed: {e})", flush=True)
                    committer = threading.Thread(target=loop, daemon=True)
                    committer.start()
            n0 = int(self.cast("nonce", self.frm, rpc=target))
            t0 = time.time()
            proc = subprocess.run(["bash", "script/deploy.sh"], cwd=self.vault_dir, env=env, capture_output=True, text=True)
            log = (proc.stdout + proc.stderr).replace(self.rpc, "<rpc>")
            (logs / "deploy.txt").write_text(log)
            sent = [line for line in log.splitlines() if line.startswith("FERRATA_SENT")]
            for line in sent:
                self.say("  " + line)
            warnings = sorted({w.strip() for f in logs.glob("deploy-*.log")
                               for w in f.read_text(errors="replace").splitlines() if "warning:" in w})
            for w in warnings:
                self.say("  " + w)
            if proc.returncode:
                self.say(f"deploy.sh stopped (exit {proc.returncode}); its last lines:")
                for line in log.strip().splitlines()[-12:]:
                    self.say("    " + line)
                raise Stop(f"{label}: not complete; nothing more is sent. Look at the lines above, then run the same "
                           "command again (it resumes from the record)")
            for i, line in enumerate(sent):  # explicit nonces: this attempt's transactions, in order, from n0
                h = re.search(r"0x[0-9a-fA-F]{64}", line).group(0)
                n = int(_sh(["cast", "tx", h, "nonce", "--rpc-url", target], hide=self.rpc))
                if n != n0 + i:
                    raise Stop(f"{label}: transaction {h} has nonce {n}, not {n0 + i}")
            rec = json.loads(rec_path.read_text())
            if rec.get("uncapped") is not False or not rec.get("complete"):
                raise Stop(f"{label}: the record says uncapped={rec.get('uncapped')}, complete={rec.get('complete')}")
            self.say(f"{label}: complete in {time.time() - t0:.0f} s, {len(sent)} transaction(s) by this start")
            self.read_back(rec, target, label)
            return rec
        finally:
            stop_commits.set()
            if committer:
                committer.join(timeout=30)
            if anvil:
                anvil.kill()
                anvil.wait()
            if keydir:
                shutil.rmtree(keydir, ignore_errors=True)
            if dry:
                rec_path.unlink(missing_ok=True)
                (deps / f"{rec_name}.params.json").unlink(missing_ok=True)
            if self.commit and not dry:
                self.commit()

    # ---- what the chain holds ----------------------------------------------------------------------------------
    def read_back(self, rec: dict, rpc: str, label: str) -> None:
        g, s, v = rec["groth16Verifier"], rec["emergencyStop"], rec["vault"]
        c = lambda addr, sig, *args: self.cast("call", addr, sig, *args, rpc=rpc)  # noqa: E731
        got = {
            "verifier VERSION": c(g, "VERSION()(string)").strip('"'),
            "verifier SELECTOR": c(g, "SELECTOR()(bytes4)"),
            "stop owner": c(s, "owner()(address)"),
            "stop paused": c(s, "paused()(bool)"),
            "stop verifier": c(s, "verifier()(address)"),
            "vault verifier": c(v, "verifier()(address)"),
            "checkpointImageId": c(v, "checkpointImageId()(bytes32)"),
            "releaseImageId": c(v, "releaseImageId()(bytes32)"),
            "lezConfigId": c(v, "lezConfigId()(bytes32)"),
            "genesisState": c(v, "genesisState()(bytes32)"),
            "mode": c(v, "mode()(uint8)"),
            "admin": c(v, "admin()(address)"),
            "freezeAuthority": c(v, "freezeAuthority()(address)"),
            "watchers": [c(v, "watchers(uint256)(address)", str(i)) for i in range(3)],
            "nMin, tMin": (_int(c(v, "nMin()(uint32)")), _int(c(v, "tMin()(uint32)"))),
            "delta, deltaFloor": (_int(c(v, "delta()(uint256)")), _int(c(v, "deltaFloor()(uint256)"))),
            "revokeWindow": _int(c(v, "revokeWindow()(uint256)")),
            "maxFreeze": _int(c(v, "maxFreeze()(uint256)")),
            "minReleaseDelay": _int(c(v, "minReleaseDelay()(uint256)")),
            "releaseCapFloor": _int(c(v, "releaseCapFloor()(uint256)")),
            "feeRecipient": c(v, "feeRecipient()(address)"),
            "globalCap deposit": [_int(x) for x in c(v, "globalCap(uint8)(uint256,uint256,uint256)", "0").splitlines()],
            "globalCap release": [_int(x) for x in c(v, "globalCap(uint8)(uint256,uint256,uint256)", "1").splitlines()],
            "ETH cap deposit": [_int(x) for x in c(v, "tokenCap(address,uint8)(uint256,uint256,uint256)", ETH, "0").splitlines()],
            "ETH cap release": [_int(x) for x in c(v, "tokenCap(address,uint8)(uint256,uint256,uint256)", ETH, "1").splitlines()],
        }
        p, low = self.params, str.lower
        want_watchers = [w.lower() for w in self.watchers] + [ZERO] * (3 - len(self.watchers))
        checks = [
            ("verifier 3.1.0, selector 0xe71e6620", got["verifier VERSION"] == "3.1.0" and got["verifier SELECTOR"] == "0xe71e6620"),
            ("stop: owner renounced, not paused, wraps the verifier",
             got["stop owner"] == ZERO and got["stop paused"] == "false" and low(got["stop verifier"]) == low(g)),
            ("vault -> stop", low(got["vault verifier"]) == low(s)),
            ("programs", low(got["checkpointImageId"]) == low(p["checkpointImageIdB"]) and low(got["releaseImageId"]) == low(p["releaseImageId"])),
            ("configuration and genesis", low(got["lezConfigId"]) == low(p["lezConfigId"]) and low(got["genesisState"]) == low(p["genesisState"])),
            ("rule (B), nMin/tMin", got["mode"] == "1" and got["nMin, tMin"] == (p["nMin"], p["tMin"])),
            ("admin = deployer, freeze authority, watchers",
             low(got["admin"]) == low(self.frm) and low(got["freezeAuthority"]) == low(self.freeze)
             and [low(w) for w in got["watchers"]] == want_watchers),
            ("Δ", got["delta, deltaFloor"] == ((self.delta or RULE_B_DELTA),) * 2),
            ("capped", all(cap[0] != UNLIMITED for k, cap in got.items() if "cap" in k.lower() and isinstance(cap, list))
             and got["releaseCapFloor"] > 0),
        ]
        self.say(f"{label}: read back from the chain:")
        for k, val in got.items():
            self.say(f"  {k:22} {val}")
        bad = [name for name, ok in checks if not ok]
        if bad:
            raise Stop(f"{label}: read back, not as deployed: {', '.join(bad)}")
        self.say(f"{label}: {len(checks)} of {len(checks)} checks hold")

    def verify_list(self, rec: dict) -> None:
        n = self.name
        self.say("addresses:")
        for k in ("groth16Verifier", "emergencyStop", "registry", "vault", "token"):
            self.say(f"  {k:16} {rec[k]}")
        self.say("no RISC Zero router: the vault calls the stop, the stop calls the verifier (decision D1 (a))")
        self.say("to verify (from a checkout of this tree, vault/):")
        self.say(f"  the record:  modal volume get ferrata-deploy {n}.json deployments/{n}.json")
        self.say(f"  Sourcify:    FERRATA_FIXTURE=deployments/{n}.json script/sourcify.sh risc0   (RISC Zero's sources)")
        self.say(f"               FERRATA_FIXTURE=deployments/{n}.json FERRATA_PUBLISH_SOURCES=yes script/sourcify.sh vault"
                 "   (publishes this repository's vault sources)")
        self.say(f"               FERRATA_FIXTURE=deployments/{n}.json script/sourcify.sh status")
        self.say("  Etherscan (https://sepolia.etherscan.io): each address above; once Sourcify holds the sources, "
                 "Read Contract shows the values read back above")


def run(a: dict, dry: bool, rpc: str, frm: str, key: tuple[str, str] | None, *, vault_dir: str, logs_root: str,
        deployer: str, commit=None, fork_port: int = 8545) -> list[str]:
    """The checks, then --dry: the deployment on a fork; else the preflight on a fork, then the deployment."""
    r = Run(a, rpc, frm, key, vault_dir=vault_dir, logs_root=logs_root, deployer=deployer, commit=commit,
            fork_port=fork_port)
    r.check()
    if dry:
        rec = r.attempt(True, "--dry")
    else:
        if not key:
            raise Stop("no key: only --dry runs without one")
        r.attempt(True, "preflight")
        rec = r.attempt(False, "deployment")
    r.verify_list(rec)
    return r.out
