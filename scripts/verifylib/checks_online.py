"""The online checks: read-only requests to public endpoints (Sepolia, Ethereum mainnet, our public LEZ test network,
the live site, Sepolia beacon APIs). Nothing here signs or sends a transaction. Most wrap the repository's own
re-check scripts, each already the re-check of an EVIDENCE row; the endpoints come from the command line
(--sepolia-rpc, --mainnet-rpc, --beacon, --lez) or scripts/pubnet-endpoints.json and
scripts/reproduce/deployed-programs.json."""
from __future__ import annotations

import json
import re
import os
import subprocess
import sys
import urllib.error
import urllib.request

from .core import CheckFailed, Skipped, check, fail_if


def _run(ctx, argv, timeout=1800, env=None):
    e = dict(os.environ)
    e.update(env or {})
    argv = [sys.executable if a == "python3" else a for a in argv]
    p = subprocess.run(argv, cwd=ctx.root, capture_output=True, text=True, timeout=timeout, env=e)
    out = [ln for ln in (p.stdout + "\n" + p.stderr).splitlines() if ln.strip()]
    return p.returncode, out


def _script(ctx, argv, ok_lines=4, unreadable=(2,), timeout=1800, env=None):
    """Run a re-check script; exit 0 passes, an exit status in `unreadable` (the endpoint could not be read) skips."""
    rc, out = _run(ctx, argv, timeout=timeout, env=env)
    if rc == 0:
        return out[-ok_lines:]
    if rc in unreadable:
        raise Skipped("an endpoint could not be read: " + (out[-1] if out else f"exit {rc}"))
    raise CheckFailed("\n".join([f"exit status {rc}"] + out[-25:]))


def _first_rpc(urls, chain_id: int):
    """The first JSON-RPC of `urls` that answers eth_chainId with `chain_id`."""
    for u in urls:
        try:
            got = _eth(u, "eth_chainId", [])
            if int(got, 16) == chain_id:
                return u
        except (OSError, ValueError, KeyError, urllib.error.URLError):
            continue
    raise Skipped(f"no JSON-RPC of chain {chain_id} answered ({', '.join(urls)})")


def _eth(url, method, params, timeout=30):
    if method not in ("eth_chainId", "eth_call", "eth_blockNumber", "eth_getCode"):
        raise ValueError(f"{method}: not a read this tool sends")
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json",
                                                          "User-Agent": "ferrata-verify/1"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        out = json.loads(r.read())
    if "error" in out:
        raise ValueError(str(out["error"]))
    return out["result"]


for _run_dir, _ev, _rows in (
        ("crown-live-3", (91, 92), ("RFP-022 F.1", "RFP-022 F.3", "RFP-022 F.4", "RFP-022 F.5", "RFP-022 F.6",
                                    "RFP-022 F.7", "RFP-022 R.10", "RFP-022 S.1")),
        ("crown-live-4", (141, 142), ("RFP-022 R.10", "RFP-022 R.14", "RFP-022 S.1")),
        ("crown-live-1", (41, 42), ("RFP-022 F.1", "RFP-022 F.7", "RFP-022 R.10", "RFP-022 Sec.2", "RFP-022 Sec.6"))):
    def _fn(ctx, _d=_run_dir):
        rpc = _first_rpc(ctx.sepolia_rpc, 11155111)
        return _script(ctx, ["python3", "-I", "scripts/verify-sepolia.py", f"e2e/runs/{_d}", "--rpc", rpc])
    check(id=f"sepolia-{_run_dir}", family="sepolia", mode="online", evidence=_ev, rows=_rows,
          title=f"The live round trip {_run_dir} on Sepolia, re-read from its committed records",
          what=f"every transaction of e2e/runs/{_run_dir} (deployment, deposits, checkpoint, releases): receipts, "
               "events, the vault's code and state, the seals and journals it verified",
          against="a public Sepolia JSON-RPC (scripts/pubnet-endpoints.json, or --sepolia-rpc), read-only",
          inputs=(f"e2e/runs/{_run_dir}", "scripts/verify-sepolia.py"))(_fn)


@check(id="sepolia-live-roundtrip", family="sepolia", mode="online", evidence=(250,),
       rows=("RFP-022 F.3", "RFP-022 F.5", "RFP-022 R.10", "RFP-022 S.1"),
       title="The first full round trip on our public LEZ test network, every fact read again from both chains",
       what="e2e/runs/live-roundtrip-2026-11-24/record.json: the deposit, the root import, the claim, the burn, "
            "the checkpoint, the release, and the links between them",
       against="Sepolia's public JSON-RPC and our LEZ network's public RPC, read-only: rebuilt field by field",
       inputs=("e2e/runs/live-roundtrip-2026-11-24", "scripts/verify-live-roundtrip.py"))
def _live_roundtrip(ctx):
    rpc = _first_rpc(ctx.sepolia_rpc, 11155111)
    return _script(ctx, ["python3", "-I", "scripts/verify-live-roundtrip.py", "--check", "--rpc", rpc,
                         "--lez-rpc", ctx.lez_url + "/rpc"], ok_lines=3)


ESTOP_30 = "0x9F9994Eb4Cb5200198FEfb470f8b50301662e696"  # RiscZeroVerifierEmergencyStop of Groth16 3.0.0 (mainnet)
ROUTER = "0x8EaB2D97Dfce405A1692a21b3ff3A172d593D319"


@check(id="mainnet-risc0-3.0-stopped", family="sepolia", mode="online", evidence=(149,),
       title="RISC Zero's Groth16 verifier 3.0.0 is stopped on Ethereum mainnet (its emergency stop paused)",
       what="the 3.0.0 emergency stop's paused() and the router's verifier for selector 0x73c457ba",
       against="Ethereum mainnet by eth_call (read-only), the addresses of risc0-ethereum v3.0.0's deployment.toml "
               "(e2e/runs/mainnet-risc0-verify/upstream/); EVIDENCE row 149's run of 18 November found every "
               "recorded seal accepted through this router, before the stop; since the stop the router refuses "
               "every 3.0.0 seal (scripts/verify-mainnet-risc0.py reports them as FAIL: EnforcedPause)",
       inputs=("e2e/runs/mainnet-risc0-verify/upstream/deployment.toml",))
def _mainnet_stopped(ctx):
    rpc = _first_rpc(ctx.mainnet_rpc, 1)
    block = _eth(rpc, "eth_blockNumber", [])
    paused = int(_eth(rpc, "eth_call", [{"to": ESTOP_30, "data": "0x5c975abb"}, block]), 16)
    got = _eth(rpc, "eth_call", [{"to": ROUTER, "data": "0x3cadf449" + "73c457ba" + "0" * 56}, block])
    via = "0x" + got[-40:]
    if via.lower() != ESTOP_30.lower():
        raise CheckFailed(f"the router's verifier for 0x73c457ba is {via}, not the 3.0.0 emergency stop {ESTOP_30}")
    if paused != 1:
        raise CheckFailed(f"the 3.0.0 emergency stop {ESTOP_30} is not paused at block {int(block, 16)}")
    return [f"at mainnet block {int(block, 16):,} ({rpc}): the router's verifier for 0x73c457ba is the 3.0.0 emergency "
            f"stop {ESTOP_30[:10]}…, and it is paused: every 3.0.0 seal is refused through the router"]


VAULTS = {"v3": "0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7", "live1": "0xfE3199fa2634413CbAC317371076Cae3EB2d9E54",
          "live3": "0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c", "live4": "0x80555Fe53CCcEf46951B23A3c75643a4C9283470"}
GLOBAL_CAP = "0x0eb2d6fe"  # globalCap(uint8) -> (capacity, refillPerSecond, available)


@check(id="sepolia-vaults-closed", family="sepolia", mode="online", evidence=(154,),
       rows=("RFP-022 R.8", "RFP-022 Sec.4"),
       title="The four RISC Zero 3.0 vaults on Sepolia stay closed: every deposit and release cap reads zero",
       what="globalCap(deposit) and globalCap(release) of the four vaults closed on 21 November 2026 "
            "(docs/security/risc0-groth16-advisory-2026-11-21/)",
       against="Sepolia at its latest block, eth_call (read-only): capacity, refill and available all zero",
       inputs=("docs/security/risc0-groth16-advisory-2026-11-21",))
def _vaults_closed(ctx):
    rpc = _first_rpc(ctx.sepolia_rpc, 11155111)
    block = _eth(rpc, "eth_blockNumber", [])
    problems, lines = [], []
    for name, addr in VAULTS.items():
        for d, label in ((0, "deposit"), (1, "release")):
            data = GLOBAL_CAP + f"{d:064x}"
            r = _eth(rpc, "eth_call", [{"to": addr, "data": data}, block])
            words = [int(r[2 + 64 * i: 2 + 64 * (i + 1)], 16) for i in range(3)]
            if any(words):
                problems.append(f"{name} {addr}: globalCap({label}) = {words}, not zero")
        lines.append(f"{name} {addr[:10]}…: deposit and release caps zero")
    fail_if(problems, "the closed vaults")
    return [f"at Sepolia block {int(block, 16):,} ({rpc}):"] + lines


@check(id="lez-network-programs", family="lez-network", mode="online", evidence=(238, 248),
       rows=("RFP-021 S.1", "RFP-022 S.1"),
       title="The programs deployed on our public LEZ test network are the ones the repository rebuilds",
       what="each program's loader header on the network (the image id LEZ runs, immutable) and the network's "
            "/programs.json",
       against="scripts/reproduce/deployed-programs.json (the image ids and binaries the repository's reproducible "
               "build gives)", inputs=("scripts/reproduce/check_network.py", "scripts/reproduce/deployed-programs.json"))
def _network_programs(ctx):
    rc, out = _run(ctx, ["python3", "-I", "scripts/reproduce/check_network.py", "chain", "--network", ctx.lez_url])
    if rc == 0:
        return out[-12:]
    fails = [ln for ln in out if "FAIL" in ln]
    unlisted = [ln.split()[0] for ln in fails if "not in the list" in ln]
    if rc != 1 or len(unlisted) != len(fails):
        raise CheckFailed("\n".join([f"exit status {rc}"] + out[-25:]))
    # Deployed programs the reproduction list does not hold yet: each must be a build docs/image-ids.md records
    # (image id and binary SHA-256 both equal), and its loader header on chain must name that id, immutable.
    from . import lez
    ids = (ctx.root / "docs/image-ids.md").read_text()
    pub = json.loads(urllib.request.urlopen(urllib.request.Request(
        ctx.lez_url + "/programs.json", headers={"User-Agent": "ferrata-verify/1"}), timeout=30).read())["programs"]
    rpc = lez.Rpc(ctx.lez_url + "/rpc")
    loader = lez.b58encode(bytes([0xFE]) * 32)
    lines, problems = [], []
    for name in unlisted:
        p = pub.get(name) or {}
        img, sha = str(p.get("image_id", "")).removeprefix("0x"), str(p.get("binary_sha256", ""))
        row = next((ln for ln in ids.splitlines() if img and img in ln and sha and sha in ln), None)
        view = rpc.call("getAccountView", [{"account_id": p.get("program_account", ""), "program_account_id": loader}])
        hdr = bytes((view or {}).get("data", {}).get("actor_states", {}).get(loader, []))
        on_chain = len(hdr) == 66 and hdr[0] == 0 and hdr[1:33].hex() == img and hdr[65] == 1
        if row is None or not on_chain:
            problems.append(f"{name}: deployed, not in the list, and " + ("its header on chain is not an immutable "
                            "header naming its published id" if not on_chain else "no build of docs/image-ids.md "
                            "has its image id and binary SHA-256"))
        else:
            lines.append(f"{name}: deployed ({img[:16]}…, immutable on chain), equal to a build docs/image-ids.md "
                         "records; not yet in deployed-programs.json (its rebuild by scripts/reproduce/deployed.sh "
                         "is not recorded)")
    fail_if(problems, "the deployed programs")
    return [ln for ln in out if "ok" in ln.split()[-1:] or "programs ok" in ln][-12:] + lines


@check(id="lez-network-head", family="lez-network", mode="online", evidence=(242,), rows=("RFP-021 S.7",),
       title="The test network's head block and each light-client instance against a public beacon API",
       what="the head block (its hash recomputed from its bytes) and each instance's finalized Ethereum header, "
            "read from the instance's account and hashed to its block root",
       against="a public Sepolia beacon API's root at the same slot (the providers the network's config.json names)",
       inputs=("deploy/testnet/web/explorer/agents/ferrata_check.py",))
def _network_head(ctx):
    return _script(ctx, ["python3", "-I", "deploy/testnet/web/explorer/agents/ferrata_check.py", ctx.lez_url + "/"],
                   ok_lines=8)


@check(id="site-manifest-online", family="site", mode="online", evidence=(239,),
       title="The live site serves the files the repository holds",
       what="the manifest the host serves and every file it lists, hashed here",
       against="the committed manifest (deploy/testnet/web/trust/bridge/verify/manifest.json) and the git blobs it "
               "names", needs=("python3", "git", "network"), inputs=("scripts/site-manifest.py",))
def _site_manifest(ctx):
    return _script(ctx, ["python3", "-I", "scripts/site-manifest.py", "verify", "--site", ctx.lez_url, "--no-second"],
                   ok_lines=4, unreadable=())


@check(id="updater-live-sepolia-beacon", family="beacon", mode="online", evidence=(99, 104),
       rows=("RFP-021 U.3", "RFP-021 R.4", "RFP-021 R.5"),
       title="The updater's 8 h live run on LEZ sequencers: its record re-checked, every applied root against Sepolia",
       what="lez/updater/results/live-sepolia (zone-2): the journal against the manifest, the jobs' steps in order, "
            "the instance at the end; every applied job's finalized root",
       against="a public Sepolia beacon API: the canonical block at each slot",
       inputs=("lez/updater/results/live-sepolia",))
def _updater_beacon(ctx):
    for b in ctx.beacon:
        rc, out = _run(ctx, ["python3", "-I", "lez/updater/results/live-sepolia/check.py",
                             "lez/updater/results/live-sepolia/zone-2", "--beacon", b], timeout=1800)
        if rc == 0:
            return out[-4:]
        if any("Traceback" in ln for ln in out) or rc not in (1, 2):
            continue
        raise CheckFailed("\n".join([f"exit status {rc} ({b})"] + out[-20:]))
    raise Skipped(f"no beacon API answered ({', '.join(ctx.beacon)})")


CATALOG_INDEX = "https://raw.githubusercontent.com/landclimber/ferrata-modules/refs/heads/main/index/index.json"


def _release_rows(text: str) -> dict:
    """(module, version) -> (rootHash, sha256) from releases/RELEASE-INTEGRITY.md's tables."""
    out = {}
    for ln in text.splitlines():
        m = re.match(r"^\| `(ferrata_[a-z_]+)` \| `(\d+\.\d+\.\d+)` \| https://github\.com/landclimber/", ln)
        if not m:
            continue
        hexes = re.findall(r"`([0-9a-f]{64})`", ln)
        if len(hexes) >= 2:
            out[(m.group(1), m.group(2))] = (hexes[-2], hexes[-1])
    return out


@check(id="catalog-index-online", family="signature", mode="online", evidence=(253, 262),
       rows=("RFP-021 S.9", "RFP-021 S.10", "RFP-022 U.10", "RFP-022 S.3"),
       title="The published module catalog's index names the packages the release record gives",
       what="every package version of landclimber/ferrata-modules' index/index.json: its rootHash and sha256",
       against="releases/RELEASE-INTEGRITY.md's tables (each row the release sidecar's values, recorded at "
               "publication); every recorded release still offered", inputs=("releases/RELEASE-INTEGRITY.md",))
def _catalog_index(ctx):
    want = _release_rows((ctx.root / "releases/RELEASE-INTEGRITY.md").read_text())
    if not want:
        raise CheckFailed("releases/RELEASE-INTEGRITY.md holds no release rows")
    try:
        req = urllib.request.Request(CATALOG_INDEX, headers={"User-Agent": "ferrata-verify/1"})
        idx = json.loads(urllib.request.urlopen(req, timeout=30).read())
    except (urllib.error.URLError, TimeoutError, ValueError) as e:
        raise Skipped(f"the catalog's index could not be read: {e}") from e
    got = {}
    for p in idx.get("packages", []):
        for v in p.get("versions", []):
            ref = v.get("publisherRef", "")
            ver = ref.rsplit("-v", 1)[-1] if "-v" in ref else v.get("version", "")
            got[(p["name"], ver)] = (v.get("rootHash", ""), v.get("sha256", ""))
    problems = []
    for key, (root, sha) in sorted(want.items()):
        g = got.get(key)
        if g is None:
            problems.append(f"{key[0]} {key[1]}: in the release record, not in the published index")
        elif g != (root, sha):
            problems.append(f"{key[0]} {key[1]}: the index says rootHash {g[0][:12]}…, sha256 {g[1][:12]}…; the "
                            f"record {root[:12]}…, {sha[:12]}…")
    extra = sorted(set(got) - set(want))
    fail_if(problems, "the published catalog")
    return [f"{len(want)} of {len(want)} recorded releases in the published index with the same rootHash and sha256 "
            f"({len(got)} versions published" + (f"; not in the record: {', '.join(f'{a} {b}' for a, b in extra)})"
                                                 if extra else ")")]


@check(id="gasdrop-terms-online", family="lez-network", mode="online", evidence=(258,),
       rows=("RFP-022 U.5", "RFP-022 Priv.7"),
       title="The test network publishes its gas-drop terms, and its guest is the one the repository builds",
       what="the gateway's /relayer/gas-drop terms (the relayer's LEZ account, the gas-drop guest's image id, the "
            "native LEZ a drop gives, its inbox)",
       against="docs/image-ids.md's gas-drop guest row; the relayer's balance on the network (at least one drop)",
       inputs=("docs/image-ids.md",))
def _gasdrop_terms(ctx):
    try:
        req = urllib.request.Request(ctx.lez_url + "/relayer/gas-drop", headers={"User-Agent": "ferrata-verify/1",
                                                                                "Accept": "application/json"})
        terms = json.loads(urllib.request.urlopen(req, timeout=30).read())
    except urllib.error.HTTPError as e:
        raise CheckFailed(f"{ctx.lez_url}/relayer/gas-drop answered HTTP {e.code}") from e
    except (urllib.error.URLError, TimeoutError, ValueError) as e:
        raise Skipped(f"the gateway could not be read: {e}") from e
    from . import lez
    img = str(terms.get("imageId", "")).lower().removeprefix("0x")
    rows = [ln for ln in (ctx.root / "docs/image-ids.md").read_text().splitlines()
            if ln.startswith("|") and img and img in ln.lower()]
    if not rows or not any("gasdrop" in ln.lower() or "gas-drop" in ln.lower() or "gas drop" in ln.lower()
                           for ln in rows):
        raise CheckFailed(f"the published gas-drop guest {img[:16]}… is not a gas-drop row of docs/image-ids.md")
    relayer = lez.b58encode(bytes.fromhex(str(terms.get("relayer", "")).removeprefix("0x")))
    bal = lez.Rpc(ctx.lez_url + "/rpc", budget_s=60).call("getAccountBalance", [relayer])
    if not isinstance(bal, int) or bal < int(terms.get("amount", 0)):
        raise CheckFailed(f"the relayer's account {relayer} holds {bal}, less than one drop ({terms.get('amount')})")
    return [f"terms: relayer {relayer[:10]}…, guest {img[:16]}… (docs/image-ids.md: a gas-drop row), "
            f"{terms.get('amount')} native units a drop, covers {', '.join(terms.get('covers', []))}",
            f"the relayer's account holds {bal:,} native units ({bal // int(terms.get('amount') or 1):,} drops)"]


@check(id="faucet-terms-online", family="lez-network", mode="online", evidence=(243, 248),
       title="The test network's faucet publishes its terms, and its account holds what it says on chain",
       what="the gateway's GET /faucet: its account, its balance, the grant and its limits",
       against="the account's balance as the network's RPC serves it (getAccountBalance), at least the published "
               "balance's order and many grants", inputs=())
def _faucet_terms(ctx):
    from . import lez
    try:
        req = urllib.request.Request(ctx.lez_url + "/faucet", headers={"User-Agent": "ferrata-verify/1",
                                                                      "Accept": "application/json"})
        terms = json.loads(urllib.request.urlopen(req, timeout=30).read())
    except urllib.error.HTTPError as e:
        raise CheckFailed(f"{ctx.lez_url}/faucet answered HTTP {e.code}") from e
    except (urllib.error.URLError, TimeoutError, ValueError) as e:
        raise Skipped(f"the gateway could not be read: {e}") from e
    acct, grant = terms.get("faucet_account"), int(terms.get("amount", 0))
    bal = lez.Rpc(ctx.lez_url + "/rpc", budget_s=60).call("getAccountBalance", [acct])
    if not isinstance(bal, int) or grant <= 0 or bal < grant:
        raise CheckFailed(f"the faucet {acct} holds {bal} on chain; a grant is {grant}")
    return [f"faucet {acct[:10]}…: a grant of {grant:,} native units (1 LGO = 10^9), limits {terms.get('limits')}",
            f"its balance on chain {bal:,} ({bal // grant:,} grants); published {int(terms.get('balance', 0)):,}"]


@check(id="sepolia-31-vault", family="sepolia", mode="online", evidence=(232,), rows=("RFP-022 R.8",),
       title="The RISC Zero 3.1 vault of our test network, deployed on Sepolia, read back against its record",
       what="vault/deployments/sepolia-31-B.json and vault/script/params/sepolia-31.json: every address the "
            "deployer's creation at its nonce, the verifier 3.1.0, the emergency stop, the programs, configuration, "
            "roles, delta, floors and caps", needs=("bash", "cast", "network"),
       against="Sepolia by cast call (read-only), through vault/deployments/sepolia-31-B.check.sh",
       inputs=("vault/deployments/sepolia-31-B.json", "vault/deployments/sepolia-31-B.check.sh"))
def _sepolia_31(ctx):
    # Two of its reads are at past blocks (the vault's code before and at its creation block): an endpoint that
    # prunes history cannot answer them, so the endpoints are tried in turn until one serves them.
    history = ("pruned", "missing trie node", "header not found", "historical state", "is not available")
    tried = []
    for rpc in ctx.sepolia_rpc:
        rc, out = _run(ctx, ["bash", "vault/deployments/sepolia-31-B.check.sh", "."], env={"RPC": rpc}, timeout=900)
        fails = [ln for ln in out if ln.startswith("FAIL")]
        if rc == 0 and not fails:
            return [f"{sum(1 for ln in out if ln.startswith('ok'))} reads equal the record ({rpc})"] + out[-1:]
        pruned = any(h in ln for ln in fails for h in history)
        if pruned and all(any(h in ln for h in history) or "has code at block" in ln for ln in fails):
            tried.append(rpc)
            continue
        raise CheckFailed("\n".join([f"exit status {rc} ({rpc})"] + (fails or out)[-20:]))
    raise Skipped(f"no Sepolia endpoint served the past blocks the record names ({', '.join(tried)}): give an "
                  "archive endpoint with --sepolia-rpc")


@check(id="sepolia-verifiers", family="sepolia", mode="online", evidence=(154, 232), rows=("RFP-022 R.8",),
       title="Each Sepolia vault's Groth16 verifier: 3.1.0, or 3.0.0 only behind closed caps",
       what="the verifier behind each vault's emergency stop (its SELECTOR) for the four 3.0 vaults and the 3.1 vault",
       against="vault/script/check-verifier.sh's rule: 3.1.0 ok; 3.0.0 (which accepts forged claims) ok only while "
               "the vault's deposit and release caps both read 0", needs=("bash", "cast", "network"),
       inputs=("vault/script/check-verifier.sh",))
def _sepolia_verifiers(ctx):
    rpc = _first_rpc(ctx.sepolia_rpc, 11155111)
    lines, problems = [], []
    for name, addr in list(VAULTS.items()) + [("test-network", "0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787")]:
        rc, out = _run(ctx, ["bash", "vault/script/check-verifier.sh", addr, rpc], timeout=300)
        (lines if rc == 0 else problems).append(f"{name} {addr[:10]}…: " + (out[-1] if out else f"exit {rc}"))
    fail_if(problems, "the vaults' verifiers")
    return lines
