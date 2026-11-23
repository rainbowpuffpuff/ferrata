"""One command for the owner: a new FerrataVault on Sepolia against RISC Zero's Groth16 verifier 3.1.0 (RISC Zero 3.1
migration, phase 1, docs/design/risc0-3.1-migration.md), from a Modal container that holds the deployer key.

    MODAL_PROFILE=<profile> modal run vault/script/deploy_sepolia.py --params P.json --freeze-authority 0x... \
        --watchers 0x... --expect-vault 0x... --dry          # first: checks, then the whole deployment on a fork
    MODAL_PROFILE=<profile> modal run vault/script/deploy_sepolia.py --params P.json --freeze-authority 0x... \
        --watchers 0x... --expect-vault 0x...                # then: the same on Sepolia, signed with the key

The key: the Modal Secret `ferrata-sepolia-key` (FERRATA_KEYSTORE_JSON, FERRATA_KEYSTORE_PASSWORD, SEPOLIA_RPC_URL,
ETH_FROM), as e2e/crown/cloud/app.py uses it. It never leaves the container and is never printed; --dry does not write it
anywhere. The container runs script/deploy.sh (one transaction per forge run, every signed transaction journaled before it
leaves, scripts/pubnet.sh) with:
  - rule (B), gas rule eip8037 (Sepolia since Glamsterdam), capped defaults (script/DeployVault.s.sol);
  - decision D1 (a) of the migration plan: our own RiscZeroGroth16Verifier 3.1.0 (risc0-ethereum v3.1.0, vault/lib.lock;
    DeployVault refuses any other VERSION()/SELECTOR()) behind RISC Zero's RiscZeroVerifierEmergencyStop, its owner
    renounced at the end; a freeze authority and at least one watcher that have keys (--freeze-authority, --watchers;
    the keyless default of the November vaults is refused); no RISC Zero router;
  - FERRATA_EXPECT_VAULT = --expect-vault: the address the LEZ configuration names (its configId commits to it). The
    vault is the deployer's fourth creation from its current nonce N (verifier N, stop N+1, library N+2, vault N+3, test
    token N+4); if anything else is sent from the deployer before the vault, its creation is refused and nothing more is
    sent.
Checks before anything: ETH_FROM is DEPLOYER below, the endpoint is chain 11155111, no transaction of the deployer is
pending (pending nonce = latest), the program ids of --params are docs/image-ids.md's RISC Zero 3.1 ids, the record of an
earlier start (the Volume `ferrata-deploy`) is of the same deployment. Explicit nonces: every sent transaction is held to
the next nonce of the sequence that started at N. Idempotent: run it again after a stop and it continues from the record.

--dry: the same checks, then the whole deployment on an anvil fork of Sepolia at the latest block, as the deployer
(impersonated, with its real balance), nothing sent to Sepolia: the addresses, the gas of every step and the cost at
the fork's base fee. Testing without the Secret (no key): FERRATA_DEPLOY_NO_SECRET=1 with --deployer and --rpc; only
--dry runs then.
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import shutil
import subprocess
import time

import modal

APP = "ferrata-deploy"
SECRET = "ferrata-sepolia-key"
DEPLOYER = "0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07"  # the deployer of every Ferrata vault on Sepolia (docs/sepolia.md)
CHAIN = "11155111"
FOUNDRY = "nightly-a4d66e8e348b9763208f715a753c82d0a4ba5b63"  # Foundry 1.3.6, as the Sepolia runs
# docs/image-ids.md, RISC Zero 3.1 (risc0-zkvm 3.1.0, risc0-build 3.0.7).
IDS_31 = {
    "checkpoint_b": "0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0",
    "checkpoint_b_0_3_5": "0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04",
    "checkpoint": "0x7c93425e9b6fedbd0d79092b3a0b6c164b9a6ee250b9e2e89b26a7fadc4497c1",
    "checkpoint_0_3_5": "0x9140ec7479debc9b40529b0805095d4cfdef93a1e0500c6687a1d7ec4d7c42de",
    "release": "0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd",
}
REPO = pathlib.Path(__file__).resolve().parents[2] if modal.is_local() else pathlib.Path("/repo")  # the image's copy
NO_SECRET = modal.is_local() and os.environ.get("FERRATA_DEPLOY_NO_SECRET") == "1"

image = (
    modal.Image.debian_slim(python_version="3.12")
    .apt_install("curl", "ca-certificates", "git", "jq", "procps")
    .run_commands("curl -L https://foundry.paradigm.xyz | bash", f"/root/.foundry/bin/foundryup --install {FOUNDRY}")
    .env({"PATH": "/root/.foundry/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
          "FOUNDRY_DISABLE_NIGHTLY_WARNING": "1"})
    .add_local_dir(REPO / "vault", "/repo/vault", copy=True,
                   ignore=["lib", "out", "cache", "broadcast", "deployments", "results", "**/__pycache__"])
    .add_local_file(REPO / "scripts" / "pubnet.sh", "/repo/scripts/pubnet.sh", copy=True)
    .add_local_file(REPO / "scripts" / "pubnet.py", "/repo/scripts/pubnet.py", copy=True)
    .add_local_file(REPO / "scripts" / "pubnet-endpoints.json", "/repo/scripts/pubnet-endpoints.json", copy=True)
    .add_local_file(REPO / "scripts" / "vault_libs.py", "/repo/scripts/vault_libs.py", copy=True)
    # The pinned libraries (vault/lib.lock, checked by content) and the build, once, into the image.
    .run_commands(
        "cd /repo/vault && forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0"
        " risc0/risc0-ethereum@v3.1.0 > /dev/null 2>&1 && python3 -I /repo/scripts/vault_libs.py check",
        "cd /repo/vault && FOUNDRY_OFFLINE=false forge build > /dev/null && ls out/FerrataVault.sol/FerrataVault.json",
    )
)
app = modal.App(APP, image=image)
state = modal.Volume.from_name("ferrata-deploy", create_if_missing=True)
# Without the Secret (tests): an empty one in its place, so that the function has the same dependencies where it is
# defined and in the container (which hydrates them by id and never looks the Secret up by name).
secrets = [modal.Secret.from_dict({}) if NO_SECRET else modal.Secret.from_name(SECRET)]


class Stop(Exception):
    pass


def _sh(args: list[str], env: dict | None = None, check: bool = True, hide: str = "") -> str:
    p = subprocess.run(args, capture_output=True, text=True, env=env)
    if check and p.returncode:
        msg = (p.stderr.strip() or p.stdout.strip())[-500:]
        raise Stop(f"{args[0]} {args[1] if len(args) > 1 else ''} failed: " + (msg.replace(hide, "<rpc>") if hide else msg))
    return p.stdout.strip()


def _addr(a: str) -> str:
    if not re.fullmatch(r"0x[0-9a-fA-F]{40}", a or ""):
        raise Stop(f"not an address: {a!r}")
    return a


def _deploy(a: dict, dry: bool, rpc: str, frm: str, key: tuple[str, str] | None) -> list[str]:
    out: list[str] = []

    def say(line: str) -> None:
        print(line, flush=True)
        out.append(line)

    def cast(*x: str) -> str:
        return _sh(["cast", *x], hide=rpc)

    # ---- checks, nothing sent -----------------------------------------------------------------------------------
    if frm.lower() != DEPLOYER.lower():
        raise Stop(f"ETH_FROM is {frm}, not the vault deployer {DEPLOYER}")
    chain = cast("chain-id", "--rpc-url", rpc)
    if chain != CHAIN:
        raise Stop(f"the endpoint is chain {chain}, not Sepolia ({CHAIN})")
    p = a["params"]
    b, r, aa = p.get("checkpointImageIdB", ""), p.get("releaseImageId", ""), p.get("checkpointImageIdA", "")
    layout = {IDS_31["checkpoint_b"]: "Logos Blockchain d480efc6 (LEZ v0.4.0-rc1)",
              IDS_31["checkpoint_b_0_3_5"]: "Logos Blockchain 0.3.5-rc.1 (LEZ v0.4.0-rc3)"}.get(b.lower())
    if not layout:
        raise Stop(f"checkpointImageIdB {b} is not a RISC Zero 3.1 checkpoint_b (docs/image-ids.md)")
    if r.lower() != IDS_31["release"]:
        raise Stop(f"releaseImageId {r} is not the RISC Zero 3.1 release program {IDS_31['release']}")
    if aa.lower() not in (IDS_31["checkpoint"], IDS_31["checkpoint_0_3_5"]):
        raise Stop(f"checkpointImageIdA {aa} is not a RISC Zero 3.1 checkpoint program (unused under rule B, but recorded)")
    for k in ("lezConfigId", "genesisState"):
        if not re.fullmatch(r"0x[0-9a-fA-F]{64}", p.get(k, "")) or int(p[k], 16) == 0:
            raise Stop(f"params: {k} must be the LEZ side's (32 bytes, not zero)")
    freeze, watchers = _addr(a["freeze_authority"]), [_addr(w) for w in a["watchers"].split(",") if w]
    if not 1 <= len(watchers) <= 3:
        raise Stop("decision D1 (a): one to three watchers (--watchers)")
    keyless = "0x" + cast("keccak", "ferrata testnet: freeze authority (no key)")[-40:]
    if freeze.lower() in (frm.lower(), keyless.lower()):
        raise Stop("decision D1 (a): a freeze authority with a key holder, not the deployer and not the keyless default")
    expect = _addr(a["expect_vault"])
    say(f"chain {chain}; deployer {frm}; balance {cast('balance', '--ether', '--rpc-url', rpc, frm)} ETH")
    say(f"programs (RISC Zero 3.1): checkpoint_b {b} ({layout}), release {r}; lezConfigId {p['lezConfigId']}, "
        f"genesisState {p['genesisState']}, nMin {p['nMin']}, tMin {p['tMin']}")
    say(f"D1 (a): freeze authority {freeze}, watchers {', '.join(watchers)}; the stop's owner renounced at the end")
    latest = int(cast("nonce", "--block", "latest", "--rpc-url", rpc, frm))
    pending = int(cast("nonce", "--block", "pending", "--rpc-url", rpc, frm))
    if pending != latest:
        raise Stop(f"the deployer has a pending transaction (latest nonce {latest}, pending {pending}): wait for it")
    name = a["name"]
    record = pathlib.Path("/state/deployments") / f"{name}.json"
    rec = json.loads(record.read_text()) if record.exists() else None
    if rec:
        if rec.get("deployer", "").lower() != frm.lower() or str(rec.get("chainId")) != CHAIN:
            raise Stop(f"the record {name} is of another deployer or chain")
        if rec.get("vault") not in (None, "0x0000000000000000000000000000000000000000") and rec["vault"].lower() != expect.lower():
            raise Stop(f"the record's vault {rec['vault']} is not --expect-vault {expect}")
        say(f"resuming the record {name}{' (on the fork)' if dry else ''}: last step {rec.get('lastStep')}, "
            f"complete {rec.get('complete')}")
    else:
        names = ["Groth16 verifier 3.1.0", "emergency stop", "library FerrataRegistry", "vault", "test token"]
        predicted = [cast("compute-address", frm, "--nonce", str(latest + i)).split()[-1] for i in range(5)]
        for i, (n, x) in enumerate(zip(names, predicted)):
            say(f"  nonce {latest + i}: {n} at {x}")
        if predicted[3].lower() != expect.lower():
            raise Stop(f"from nonce {latest} the vault would be at {predicted[3]}, not --expect-vault {expect}: the LEZ "
                       "configuration must name the address the deployer's nonce gives (or the nonce has moved)")
    # ---- the deployment: on a fork (--dry) or on Sepolia --------------------------------------------------------
    # The record lives in the Volume; DeployVault reads and writes it under vault/deployments (forge's fs_permissions),
    # so it is copied in before and out after. The network journal (scripts/pubnet.sh) is written in the Volume itself.
    os.chdir("/repo/vault")
    os.makedirs("deployments", exist_ok=True)
    if rec:
        shutil.copy(record, f"deployments/{name}.json")
    logs = f"/state/logs/{name}" if not dry else f"/root/dry/logs/{name}"
    os.makedirs(logs, exist_ok=True)
    pathlib.Path(f"deployments/{name}.params.json").write_text(json.dumps(p))  # forge may read ./deployments
    env = {k: v for k, v in os.environ.items() if not k.startswith(("FERRATA_KEYSTORE", "SEPOLIA_RPC"))}
    env.update(FERRATA_MODE="B", FERRATA_GAS_RULE="eip8037", FERRATA_PARAMS=f"deployments/{name}.params.json", ETH_FROM=frm,
               FERRATA_FREEZE_AUTHORITY=freeze, FERRATA_WATCHERS=",".join(watchers), FERRATA_EXPECT_VAULT=expect,
               FERRATA_OUT=f"deployments/{name}.json", FERRATA_LOG_DIR=logs)
    if a.get("delta"):
        env["FERRATA_DELTA"] = str(int(a["delta"]))
    anvil = None
    target_rpc = rpc
    if dry:
        # Five blocks behind the head: a load-balanced endpoint may not serve the head yet ("block not found").
        at = int(cast("block-number", "--rpc-url", rpc)) - 5
        anvil = subprocess.Popen(["anvil", "--fork-url", rpc, "--fork-block-number", str(at), "--port", "8545", "--silent"],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        target_rpc = "http://127.0.0.1:8545"
        for _ in range(90):
            if subprocess.run(["cast", "chain-id", "--rpc-url", target_rpc], capture_output=True).returncode == 0:
                break
            time.sleep(1)
        _sh(["cast", "rpc", "--rpc-url", target_rpc, "anvil_impersonateAccount", frm])
        say(f"--dry: an anvil fork of Sepolia at block {_sh(['cast', 'block-number', '--rpc-url', target_rpc])}; "
            "the deployer impersonated with its real balance; nothing is sent to Sepolia")
        env.update(FERRATA_RPC_URL=target_rpc, FERRATA_UNLOCKED="1", PUBNET_LAG="0")
    else:
        kd = pathlib.Path("/root/k")
        kd.mkdir(mode=0o700, exist_ok=True)
        (kd / "key.json").write_text(key[0])
        (kd / "pw").write_text(key[1])
        os.chmod(kd / "key.json", 0o600)
        os.chmod(kd / "pw", 0o600)
        env.update(FERRATA_RPC_URL=rpc, ETH_KEYSTORE=str(kd / "key.json"), ETH_PASSWORD=str(kd / "pw"))
    try:
        t0 = time.time()
        p = subprocess.run(["bash", "script/deploy.sh"], capture_output=True, text=True, env=env)
        log = (p.stdout + p.stderr).replace(rpc, "<rpc>")
        pathlib.Path(f"{logs}/deploy-{int(t0)}.txt").write_text(log)
        sent = [l for l in log.splitlines() if l.startswith("FERRATA_SENT")]
        for l in sent:
            say("  " + l)
        if p.returncode:
            say("deploy.sh stopped (exit %d); its last lines:" % p.returncode)
            for l in log.strip().splitlines()[-12:]:
                say("    " + l)
            raise Stop("not complete: look at the lines above, then run the same command again (it resumes)")
        # Explicit nonces: the transactions sent by this start, in order, from `latest`.
        for i, l in enumerate(sent):
            h = re.search(r"0x[0-9a-fA-F]{64}", l).group(0)
            n = int(_sh(["cast", "tx", h, "nonce", "--rpc-url", target_rpc], hide=rpc))
            if n != latest + i:
                raise Stop(f"transaction {h} has nonce {n}, not {latest + i}")
        rec = json.loads(pathlib.Path(f"deployments/{name}.json").read_text())
        say(f"complete in {time.time() - t0:.0f} s; record {name} ({'fork only, discarded' if dry else 'Volume ferrata-deploy'})")
        gas = sum(int(re.search(r"used (\d+)", l).group(1)) for l in sent if re.search(r"used (\d+)", l))
        if dry and gas:
            fee = int(_sh(["cast", "base-fee", "--rpc-url", target_rpc]))
            say(f"gas used on the fork {gas:,} (anvil's accounting; Sepolia's node prices state creation by EIP-8037 and "
                f"estimates more, the vault's creation about 39.5 M: deploy.sh sends each step with the node's own "
                f"estimate x 1.25); at the fork's base fee {fee:,} wei: {gas * fee:,} wei")
    finally:
        if anvil:
            anvil.kill()
        shutil.rmtree("/root/k", ignore_errors=True)
        if not dry:
            if os.path.exists(f"deployments/{name}.json"):
                record.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(f"deployments/{name}.json", record)
            state.commit()
    # ---- what to verify ------------------------------------------------------------------------------------------
    say("addresses:")
    for k in ("groth16Verifier", "emergencyStop", "registry", "vault", "token"):
        say(f"  {k:16} {rec[k]}")
    say("no RISC Zero router: the vault calls the stop, the stop calls the verifier (decision D1 (a))")
    if not dry:
        say("verify on https://sepolia.etherscan.io (Read Contract) and with the repository's scripts:")
        say(f"  {rec['groth16Verifier']}: VERSION() \"3.1.0\", SELECTOR() 0xe71e6620 (risc0-ethereum v3.1.0, ff30da77)")
        say(f"  {rec['emergencyStop']}: owner() 0x0, paused() false, verifier() {rec['groth16Verifier']}")
        say(f"  {rec['vault']}: verifier() {rec['emergencyStop']}, checkpointImageId() {b}, releaseImageId() {r},")
        say(f"      lezConfigId() {p['lezConfigId']}, genesisState() {p['genesisState']}, mode() 1,")
        say(f"      freezeAuthority() {freeze}, admin() {frm}; globalCap(0) and globalCap(1) finite")
        say("  sources: vault/script/sourcify.sh (exact match); python3 -I scripts/verify-sepolia.py (the live checks)")
    return out


@app.function(secrets=secrets, volumes={"/state": state}, timeout=3600, cpu=2.0, memory=4096, retries=0)
def deploy(a: dict, dry: bool) -> list[str]:
    env = os.environ
    if "FERRATA_KEYSTORE_JSON" not in env:
        if not dry:
            raise Stop("no Secret: only --dry runs without the key")
        rpc, frm, key = a["rpc"], a["deployer"], None
    else:
        rpc, frm = env["SEPOLIA_RPC_URL"], env["ETH_FROM"]
        key = None if dry else (env["FERRATA_KEYSTORE_JSON"], env["FERRATA_KEYSTORE_PASSWORD"])
    try:
        return _deploy(a, dry, rpc, frm, key)
    except Stop as e:
        print(f"STOP: {e}", flush=True)
        return [f"STOP: {e}"]


@app.local_entrypoint()
def main(params: str, freeze_authority: str, watchers: str, expect_vault: str, dry: bool = False,
         name: str = "sepolia-31-B", delta: int = 0, deployer: str = "", rpc: str = ""):
    if NO_SECRET and not dry:
        raise SystemExit("FERRATA_DEPLOY_NO_SECRET=1 runs --dry only")
    a = {"params": json.loads(pathlib.Path(params).read_text()), "freeze_authority": freeze_authority,
         "watchers": watchers, "expect_vault": expect_vault, "name": name, "delta": delta,
         "deployer": deployer or DEPLOYER, "rpc": rpc}
    print("DRY RUN: checks, then the deployment on an anvil fork of Sepolia; nothing is sent" if dry
          else "DEPLOYING on Sepolia, signing with the deployer key of the Secret")
    lines = deploy.remote(a, dry)
    if lines and lines[-1].startswith("STOP"):
        raise SystemExit(1)
