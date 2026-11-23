"""One command for the owner: a new FerrataVault on Sepolia against RISC Zero's Groth16 verifier 3.1.0 (RISC Zero 3.1
migration, phase 1, docs/design/risc0-3.1-migration.md), from a Modal container that holds the deployer key.

    MODAL_PROFILE=<profile> modal run vault/script/deploy_sepolia.py --params P.json --freeze-authority 0x... \
        --watchers 0x... --expect-vault 0x... --dry          # first: checks, then the whole deployment on a fork
    MODAL_PROFILE=<profile> modal run vault/script/deploy_sepolia.py --params P.json --freeze-authority 0x... \
        --watchers 0x... --expect-vault 0x...                # then: a preflight on a fork, then the same on Sepolia

The key: the Modal Secret `ferrata-sepolia-key` (FERRATA_KEYSTORE_JSON, FERRATA_KEYSTORE_PASSWORD, SEPOLIA_RPC_URL,
ETH_FROM), as e2e/crown/cloud/app.py uses it. Its values are taken out of the environment at once, written only to a
mode-700 directory for the signing run (deleted after), never printed and never passed to another process's
environment; --dry and the preflight do not write the key anywhere. The container runs script/deploy.sh (one transaction
per forge run, every signed transaction journaled before it leaves, scripts/pubnet.sh) with:
  - rule (B), gas rule eip8037 (Sepolia since Glamsterdam), capped defaults (script/DeployVault.s.sol);
  - decision D1 (a) of the migration plan: our own RiscZeroGroth16Verifier 3.1.0 (risc0-ethereum v3.1.0, vault/lib.lock;
    DeployVault refuses another one's code, VERSION() or SELECTOR()) behind RISC Zero's RiscZeroVerifierEmergencyStop,
    its owner renounced before the token registrations; a freeze authority and one to three watchers that have keys;
    no RISC Zero router;
  - FERRATA_EXPECT_VAULT = --expect-vault: the address the LEZ configuration names (its configId commits to it; P.json
    carries that configuration as `lezConfig`, and the tool refuses unless its hash is lezConfigId and it names
    Sepolia and --expect-vault: vault/script/params/sepolia-31.template.json). The
    vault is the deployer's fourth creation from its current nonce N (verifier N, stop N+1, library N+2, vault N+3, test
    token N+4); if anything else is sent from the deployer before the vault, its creation is refused, nothing sent.
Every check and refusal is vault/script/deploy_core.py's (its docstring lists them; scripts/test_deploy_sepolia.py runs
them, the signed path included, on a local anvil with a throwaway key). Explicit nonces: every sent transaction is held
to the next nonce of its start. Idempotent: run it again after a stop and it continues from the record, which lives in
the Volume `ferrata-deploy` (copied there from vault/deployments and committed every 15 s while deploy.sh runs: forge
may write only under vault/deployments, and a Volume mounted there is outside the project for forge), as does the
network journal (logs/).

--dry: the checks, then the whole deployment on an anvil fork of Sepolia, as the deployer (impersonated, with its real
balance), nothing sent to Sepolia: the addresses, the gas of every step and every value read back. Without --dry the
same runs first as a preflight, and nothing is sent unless it completes. Testing without the Secret (no key):
FERRATA_DEPLOY_NO_SECRET=1 with --deployer and --rpc; only --dry runs then.
"""

from __future__ import annotations

import json
import os
import pathlib
import sys

import modal

APP = "ferrata-deploy"
SECRET = "ferrata-sepolia-key"
DEPLOYER = "0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07"  # the deployer of every Ferrata vault on Sepolia (docs/sepolia.md)
# Foundry 1.3.6's release archive, checked by its sha256 (as scripts/reproduce/Dockerfile installs it): the image that
# holds the key runs no unpinned installer.
FOUNDRY_URL = "https://github.com/foundry-rs/foundry/releases/download/v1.3.6/foundry_v1.3.6_linux_amd64.tar.gz"
FOUNDRY_SHA256 = "01248d7ce267f5b7958bc9b331d068c6c2d8eeb182e58f0b556d7295f398e2ce"
HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1] if modal.is_local() else pathlib.Path("/repo")  # the image's copy
NO_SECRET = modal.is_local() and os.environ.get("FERRATA_DEPLOY_NO_SECRET") == "1"
VAULT_DIR = "/repo/vault"
STATE = "/state"  # the Volume: the records (copied to and from vault/deployments, where forge may write) and logs/

image = (
    modal.Image.debian_slim(python_version="3.12")
    .apt_install("curl", "ca-certificates", "git", "jq", "procps")
    .run_commands(
        f"mkdir -p /root/.foundry/bin && cd /root/.foundry && curl -sSfL -o foundry.tar.gz {FOUNDRY_URL}"
        f" && echo '{FOUNDRY_SHA256}  foundry.tar.gz' | sha256sum -c - && tar -xzf foundry.tar.gz -C bin"
        " && rm foundry.tar.gz && /root/.foundry/bin/forge --version")
    .env({"PATH": "/root/.foundry/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
          "FOUNDRY_DISABLE_NIGHTLY_WARNING": "1"})
    .add_local_dir(REPO / "vault", VAULT_DIR, copy=True,
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
    .add_local_file(HERE / "deploy_core.py", "/root/deploy_core.py")
)
app = modal.App(APP, image=image)
state = modal.Volume.from_name("ferrata-deploy", create_if_missing=True)
# Without the Secret (tests): an empty one in its place, so that the function has the same dependencies where it is
# defined and in the container (which hydrates them by id and never looks the Secret up by name).
secrets = [modal.Secret.from_dict({}) if NO_SECRET else modal.Secret.from_name(SECRET)]


@app.function(secrets=secrets, volumes={STATE: state}, timeout=3600, cpu=2.0, memory=4096, retries=0)
def deploy(a: dict, dry: bool) -> list[str]:
    sys.path.insert(0, "/root")
    import deploy_core as core

    # The Secret's values leave the environment at once (no child process inherits them).
    key_json = os.environ.pop("FERRATA_KEYSTORE_JSON", None)
    key_pw = os.environ.pop("FERRATA_KEYSTORE_PASSWORD", None)
    rpc = os.environ.pop("SEPOLIA_RPC_URL", None)
    frm = os.environ.pop("ETH_FROM", None)
    if key_json is None:
        if not dry:
            return ["STOP: no Secret: only --dry runs without the key"]
        rpc, frm, key = a["rpc"], a["deployer"], None
    else:
        key = None if dry else (key_json, key_pw)
    try:
        return core.run(a, dry, rpc, frm, key, vault_dir=VAULT_DIR, logs_root=f"{STATE}/logs", deployer=DEPLOYER,
                        state_dir=STATE, commit=state.commit)
    except core.Stop as e:
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
          else "DEPLOYING on Sepolia: checks, a preflight on a fork, then the deployment signed with the Secret's key")
    lines = deploy.remote(a, dry)
    if not lines or lines[-1].startswith("STOP"):
        raise SystemExit(1)
