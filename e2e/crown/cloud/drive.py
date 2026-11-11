#!/usr/bin/env python3
"""The crown run's stage machine (e2e/crown/README.md), run by the cloud runner (e2e/crown/cloud/app.py) from the
uploaded tree:  drive.py <run> live|fork

Every stage first asks the chain or the Volume whether it is already done, so a run stopped anywhere (`e2e/crown.sh
stop`, a timeout, a crash) continues with `start`; `stages.done` lists what finished. Stages, in order:

  lez-a             LEZ session A (lez/crown/session.sh a): a new devnet, the programs, the Sepolia instance; T0.
                    Never once the vault's deployment has begun.
  identity          deployer nonce N (latest = pending), vault = CREATE(deployer, N+2) with no code yet, configId
                    (crown-driver config-id), genesisState (identity.py), the two deposit commitments; identity.json,
                    params.json, crown.json. Computed again at every start until the deployment has begun.
  deploy            e2e/crown/sepolia.sh deploy (v5, mode B, eip8037, node-priced gas, verifier reused)
  deposit0, capture1, deposit1, capture2
                    sepolia.sh deposit j; then (live) the deposit's block finalized and capture.py with the vault's
                    ETH registry and fee slots, or (fork) lez/crown/capture-anvil.py; the capture's proofs checked
  lez-b             LEZ session B (session.sh b): the captures, the bridge flow, the claims and burns (two real
                    private proofs); never again once generation 2 is saved
  checkpoint-input  realblocks-crown over the node's final blocks (attestor, checkpoint_b input, burns)
  checkpoint-exec   ferrata-e2e exec: the guest's journal = the native one
  checkpoint-prove  gpu_prove (one L4); after CROWN_GPU_WAIT_S without a GPU, the CPU here (ferrata-e2e prove)
  checkpoint-wrap   p254 here, groth16_wrap, groth16, verify: the seal for submitCandidate
  submit            sepolia.sh submit
  releases-prove    crown-release, then per burn a CPU proof and the same wrap (during W)
  wait              until the candidate's wait W is over (fork: evm_increaseTime, labelled)
  finalize          sepolia.sh finalize (finalizeAttested)
  release0, release1  after the release delay: sepolia.sh release i
  verify            e2e/crown/verify.py over the run: DONE

Environment (set by app.py): CROWN_INPUTS, CROWN_LEZ_VOL, CROWN_CLOUD, CROWN_E2E, CROWN_REALBLOCKS, CROWN_APP; live:
SEPOLIA_RPC_URL, ETH_FROM, ETH_KEYSTORE, ETH_PASSWORD; fork: CROWN_FORK_UPSTREAM, FERRATA_UNLOCKED=1.
"""
import hashlib
import io
import json
import os
import shutil
import signal
import subprocess
import sys
import tarfile
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
RUN_NAME, MODE = sys.argv[1], sys.argv[2]
RUN = f"{REPO}/e2e/runs/{RUN_NAME}"
INPUTS = os.environ.get("CROWN_INPUTS", "/vol/inputs")
VOL_LEZ = os.environ.get("CROWN_LEZ_VOL", f"/vol/lez/{RUN_NAME}")  # lez_session's alone; read here only at a start
LEZ = f"{RUN}/lez"  # the sessions' records as lez_session returned them (without the channel key)
CLOUD = os.environ.get("CROWN_CLOUD", f"/vol/cloud/{RUN_NAME}")
KEY = f"{CLOUD}/secret/channel_signing_key"  # the devnet's channel key (attestor input): on the Volume, never fetched
E2E = os.environ.get("CROWN_E2E", f"{REPO}/bridge/target/release/ferrata-e2e")
RB = os.environ.get("CROWN_REALBLOCKS", f"{REPO}/checkpoint/target/release/realblocks-crown")
APP = os.environ.get("CROWN_APP", "ferrata-crown")
BUNDLE_DIR = "/root/crown-bundle"
PROG = f"{RUN}/programs"
CHAIN_ID = 11155111
VERIFIER = "0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0"  # RISC Zero's Groth16 verifier of sepolia-live-2 (reused)
DELTA = 3600
# The final programs (path-independent builds, docs/image-ids.md; the R0BF files of scripts/reproduce-image-ids.sh build)
IDS = {"checkpoint_b": "0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e",
       "checkpoint": "0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84",
       "segment": "0x11e888e94b9520b08477ba5c5c29cc46bdffc7b1e7c5fe822eb0776f40c54592",
       "release": "0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9"}
ZERO32 = "0x" + "00" * 32
GPU_WAIT_S = int(os.environ.get("CROWN_GPU_WAIT_S", "7200"))
CAPTURE_ATTEMPTS = 12
STAGES = ["lez-a", "identity", "deploy", "deposit0", "capture1", "deposit1", "capture2", "lez-b", "checkpoint-input",
          "checkpoint-exec", "checkpoint-prove", "checkpoint-wrap", "submit", "releases-prove", "wait", "finalize",
          "release0", "release1", "verify"]
LOCAL_RPC = "http://127.0.0.1:8545"


class Stop(Exception):
    pass


def utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def scrub(text: str) -> str:
    for k in ("SEPOLIA_RPC_URL", "CROWN_FORK_UPSTREAM"):
        v = os.environ.get(k)
        if v and v != LOCAL_RPC:
            text = text.replace(v, "<endpoint>")
    return text


def say(msg: str) -> None:
    line = f"[{utc()}] {scrub(msg)}"
    print(line, flush=True)
    with open(f"{RUN}/crown.log", "a") as f:
        f.write(line + "\n")


def die(msg: str):
    raise Stop(msg)


def jload(path: str):
    with open(path) as f:
        return json.load(f)


def jsave(path: str, obj) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path + ".part", "w") as f:
        json.dump(obj, f, indent=1)
        f.write("\n")
    os.replace(path + ".part", path)


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def run(cmd: list, log: str | None = None, env: dict | None = None, cwd: str | None = None, check: bool = True,
        timeout: int | None = None) -> int:
    """A command, its output appended to <run>/logs/<log> (or crown.log); check: a failure stops the stage."""
    path = f"{RUN}/logs/{log}" if log else f"{RUN}/crown.log"
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "a") as out:
        out.write(f"[{utc()}] $ {scrub(' '.join(str(c) for c in cmd))}\n")
        out.flush()
        try:
            rc = subprocess.run([str(c) for c in cmd], stdout=out, stderr=subprocess.STDOUT, env=env or os.environ,
                                cwd=cwd or REPO, timeout=timeout).returncode
        except subprocess.TimeoutExpired:
            rc = 124
    if check and rc != 0:
        tail = open(path, errors="replace").read().splitlines()[-12:]
        die(f"{os.path.basename(str(cmd[0]))} exited {rc} (logs/{log or 'crown.log'}):\n    " + "\n    ".join(scrub(l) for l in tail))
    return rc


def out(cmd: list, env: dict | None = None, timeout: int = 300) -> str:
    p = subprocess.run([str(c) for c in cmd], capture_output=True, text=True, env=env or os.environ, cwd=REPO, timeout=timeout)
    if p.returncode != 0:
        die(f"{os.path.basename(str(cmd[0]))} {cmd[1] if len(cmd) > 1 else ''} failed: {scrub(p.stderr.strip()[-400:])}")
    return p.stdout.strip()


# ---- the chain ------------------------------------------------------------------------------------------------------
def rpc_env() -> dict:
    return {**os.environ, "ETH_RPC_URL": os.environ["SEPOLIA_RPC_URL"]}


def cast(*a: str) -> str:
    return out(["cast", *a], env=rpc_env(), timeout=120)


def call(sig: str, *a: str) -> str:
    return cast("call", identity()["vault"], sig, *a)


def call_until(want: str, sig: str, *a: str, tries: int = 8, gap: int = 10) -> bool:
    """Reads until the answer is `want`: a load-balanced endpoint can answer from a node behind the block holding the
    transaction just sent (crown-live-1, 11 Nov 2026: release 0's nullifier read false right after its receipt said
    success, and true on publicnode a minute later)."""
    for k in range(tries):
        if call(sig, *a) == want:
            return True
        if k + 1 < tries:
            time.sleep(gap)
    return False


def has_code(addr: str) -> bool:
    return cast("code", addr) not in ("0x", "")


def head_time() -> int:
    return int(cast("block", "latest", "--field", "timestamp"))


def first_int(text: str) -> int:
    return int(text.split()[0], 0)


def pass_time(until: int, what: str) -> None:
    """Until block time >= until: on a fork by evm_increaseTime (recorded), live by waiting for Sepolia's blocks."""
    now = head_time()
    if now >= until:
        return
    if MODE == "fork":
        cast("rpc", "evm_increaseTime", str(until - now + 1))
        cast("rpc", "evm_mine")
        rj = runj()
        rj.setdefault("warped", []).append({"what": what, "seconds": until - now + 1, "at": utc()})
        jsave(f"{RUN}/run.json", rj)
        say(f"{what}: FORK ONLY, block time moved {until - now + 1} s forward with evm_increaseTime")
        return
    say(f"{what}: waiting {until - now} s (until block time {until})")
    while head_time() < until:
        time.sleep(min(60, max(5, until - head_time())))


# ---- the run's own files ------------------------------------------------------------------------------------------
def runj() -> dict:
    return jload(f"{RUN}/run.json")


def identity() -> dict:
    return jload(f"{RUN}/identity.json")


def done_list() -> list:
    p = f"{RUN}/stages.done"
    return [l.split()[0] for l in open(p).read().splitlines() if l.strip()] if os.path.isfile(p) else []


def mark(stage: str, note: str = "") -> None:
    if stage not in done_list():
        with open(f"{RUN}/stages.done", "a") as f:
            f.write(f"{stage} {utc()} {note}".rstrip() + "\n")


def state(stage: str, note: str = "") -> None:
    jsave(f"{RUN}/state.json", {"stage": stage, "note": note, "updated_at": utc(), "done": done_list()})


def deploy_record() -> str:
    return f"{REPO}/vault/deployments/crown-{RUN_NAME}.json"


def deploy_started() -> bool:
    return os.path.isfile(deploy_record()) or os.path.isfile(f"{RUN}/deploy.json")


def session_a() -> dict:
    return jload(f"{LEZ}/session-a/session-a.json")


def session_b() -> dict:
    return jload(f"{LEZ}/session-b/session-b.json")




def sepolia(*a: str, log: str) -> None:
    env = {**os.environ}
    run(["bash", f"{REPO}/e2e/crown/sepolia.sh", RUN, *a], log=log, env=env)


# ---- inputs -----------------------------------------------------------------------------------------------------
def prepare_inputs() -> None:
    """The run's own copies of the LEZ bundle and the R0BF programs (a later `setup` does not change a started run);
    their hashes and image ids checked; the bundle's binaries unpacked for crown-driver's offline commands."""
    if not os.path.isfile(f"{RUN}/bundle.json"):
        jsave(f"{RUN}/bundle.json", {**jload(f"{INPUTS}/bundle.json"), "copied_at": utc()})
    want = jload(f"{RUN}/bundle.json")["sha256"]
    # lez_session copies the bundle into lez/<run>/ at session A; until then the Volume's inputs/ holds the same bytes
    src = f"{VOL_LEZ}/bundle.tar.zst" if os.path.isfile(f"{VOL_LEZ}/bundle.tar.zst") else f"{INPUTS}/bundle.tar.zst"
    got = sha256(src)
    if got != want:
        die(f"{src} has sha256 {got}, the run's bundle is {want} (a later `setup` changed inputs/?)")
    shutil.rmtree(BUNDLE_DIR, ignore_errors=True)
    os.makedirs(BUNDLE_DIR)
    run(["bash", "-c", f"zstd -q -d -c {src} | tar -C {BUNDLE_DIR} -xf - && cd {BUNDLE_DIR}"
                       " && sha256sum -c --quiet SHA256SUMS && chmod +x bin/* node/*"], log="inputs.log")
    if not os.path.isdir(PROG):
        shutil.copytree(f"{INPUTS}/programs", PROG + ".part")
        os.replace(PROG + ".part", PROG)
    lines = out([E2E, "image-id"] + [f"{PROG}/{p}.bin" for p in IDS]).splitlines()
    got_ids = {os.path.basename(x["program"])[:-4]: x["image_id"] for x in (json.loads(l) for l in lines if l.startswith("{"))}
    if got_ids != IDS:
        die(f"the R0BF programs' image ids {got_ids} are not the crown build's {IDS}")
    say(f"inputs: LEZ bundle sha256 {got[:16]}..., programs {', '.join(f'{k} {v[:10]}...' for k, v in got_ids.items())}")


def driver(*a: str) -> str:
    return out([f"{BUNDLE_DIR}/bin/crown-driver", *a])


def lez_chain() -> str:
    return "stand-in" if MODE == "fork" else "sepolia"


# ---- fork mode: anvil ---------------------------------------------------------------------------------------------
ANVIL = None


def start_anvil() -> None:
    """anvil forking Sepolia at a block fixed at the run's first start; its state dumped into the run directory every
    30 s (and on exit) and loaded again at the next start, so a stopped rehearsal continues."""
    global ANVIL
    # The upstream must serve state at the fork block for as long as the rehearsal runs: anvil reads every storage slot
    # it has not seen (also of contracts made on the fork, once their state was reloaded) at the fork block. A node that
    # prunes after 128 blocks fails ~25 min in ("failed to get storage", crown-rehearsal-2); publicnode served state
    # 2,000 blocks old (11 Nov 2026). CROWN_FORK_ARCHIVE overrides it.
    up = os.environ.get("CROWN_FORK_ARCHIVE") or "https://ethereum-sepolia-rpc.publicnode.com"
    rj = runj()
    if not rj.get("fork_block"):
        head = int(out(["cast", "block-number"], env={**os.environ, "ETH_RPC_URL": up}))
        rj["fork_upstream"] = up
        rj["fork_block"] = head - 5  # five behind the head: a load-balanced endpoint may lag (anvil then exits)
        jsave(f"{RUN}/run.json", rj)
    os.makedirs(f"{RUN}/anvil", exist_ok=True)
    statef = f"{RUN}/anvil/state.json"
    loaded = "loaded from the run's last dump" if os.path.isfile(statef) and os.path.getsize(statef) > 0 else "new"
    for attempt in (1, 2, 3):
        log = open(f"{RUN}/logs/anvil-{attempt}.log", "a")
        cmd = ["anvil", "--fork-url", up, "--fork-block-number", str(rj["fork_block"]), "--auto-impersonate",
               "--host", "127.0.0.1", "--port", "8545", "--state", statef, "--state-interval", "30", "--retries", "10"]
        ANVIL = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
        for _ in range(90):
            time.sleep(2)
            c = subprocess.run(["cast", "chain-id", "--rpc-url", LOCAL_RPC], capture_output=True, text=True)
            if c.returncode == 0 and c.stdout.strip() == str(CHAIN_ID):
                os.environ["SEPOLIA_RPC_URL"] = LOCAL_RPC
                say(f"anvil: FORK of Sepolia at block {rj['fork_block']} (state {loaded}), head {cast('block-number')},"
                    " every sender impersonated")
                return
            if ANVIL.poll() is not None:
                break
        if ANVIL.poll() is None:
            ANVIL.terminate()
            ANVIL.wait(timeout=30)
        say(f"anvil attempt {attempt} failed (exit {ANVIL.returncode}); logs/anvil-{attempt}.log")
        time.sleep(15)
    die("anvil did not fork Sepolia in three attempts")


def stop_anvil() -> None:
    if ANVIL is not None and ANVIL.poll() is None:
        ANVIL.send_signal(signal.SIGTERM)  # anvil dumps its state on exit
        try:
            ANVIL.wait(timeout=60)
        except subprocess.TimeoutExpired:
            ANVIL.kill()


# ---- LEZ sessions -------------------------------------------------------------------------------------------------
def session(step: str, extra: dict, inputs_tar: bytes | None = None, may_start: bool = True) -> None:
    """lez_session (16 cores, its own container; app.py) runs or returns session <step>; its records are unpacked into
    <run>/lez/session-<step>/, the channel key into the Volume's cloud/<run>/secret/. Up to three attempts (a preempted
    container, a session that did not save: the zone stays at its last generation)."""
    script_path = f"{REPO}/lez/crown/session.sh"
    if not os.path.isfile(script_path):
        script_path = f"{BUNDLE_DIR}/crown/session.sh"
    script = open(script_path, "rb").read()
    env = {"CROWN_CLAIM_TIME_OFFSET_S": os.environ.get("CROWN_CLAIM_TIME_OFFSET_S"), **extra}
    fn = modal_fn("lez_session")
    for attempt in (1, 2, 3):
        where = os.path.relpath(script_path, REPO) if script_path.startswith(REPO) else "the bundle's crown/session.sh"
        say(f"LEZ session {step.upper()} (attempt {attempt}): lez_session, {where} (sha256"
            f" {hashlib.sha256(script).hexdigest()[:12]}), chain {lez_chain()}")
        t0 = time.time()
        try:
            res = fn.remote(RUN_NAME, step, lez_chain(), jload(f"{RUN}/bundle.json")["sha256"], script, env, inputs_tar, may_start)
        except Exception as e:
            say(f"LEZ session {step.upper()}: the call failed after {time.time() - t0:.0f} s: {type(e).__name__}: {str(e)[:300]}")
            time.sleep(30)
            continue
        say(f"LEZ session {step.upper()}: ok={res['ok']} ran={res['ran']} exit={res['rc']} generation={res['generation']}"
            f" after {time.time() - t0:.0f} s")
        if res["ok"]:
            unpack_session(step, res["records"])
            return
        say("LEZ session tail:\n    " + res.get("tail", "").replace("\n", "\n    "))
        if res["rc"] is None:  # refused before running: retrying changes nothing
            break
    die(f"LEZ session {step} did not complete (logs: the Volume's lez/{RUN_NAME}/session-{step}.out, session-{step}/logs/)")


def unpack_session(step: str, records: bytes) -> None:
    dst = f"{LEZ}/session-{step}"
    shutil.rmtree(dst, ignore_errors=True)
    os.makedirs(LEZ, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(records), mode="r:*") as tf:
        tf.extractall(LEZ, filter="data")
    key = f"{dst}/channel_signing_key"
    if os.path.isfile(key):
        os.makedirs(os.path.dirname(KEY), mode=0o700, exist_ok=True)
        shutil.move(key, KEY)


def have_session(step: str) -> bool:
    need = ["session-a.json", "deployment-settings.yaml"] if step == "a" else ["session-b.json", "burn-notes.json", "node/info.json"]
    return all(os.path.isfile(f"{LEZ}/session-{step}/{n}") for n in need) and (step == "b" or os.path.isfile(KEY))


def st_lez_a() -> bool:
    if have_session("a"):
        return False
    # Once the vault's deployment has begun only the saved session may come back: never a new zone.
    session("a", {}, may_start=not deploy_started())
    a = session_a()
    say(f"session A: T0 = {a['tip']['id']} {a['tip']['hash']}, channel keys {a['channel']['keys']}, bedrockGenesisId"
        f" {a['bedrockGenesisId']}, chain {a.get('chain')}")
    return True


# ---- identity, deploy, deposits -------------------------------------------------------------------------------
def st_identity() -> bool:
    if deploy_started():
        return False
    rj, a = runj(), session_a()
    deployer = os.environ["ETH_FROM"]
    if deployer.lower() != rj["deployer"].lower():
        die(f"ETH_FROM {deployer} is not the run's deployer {rj['deployer']}")
    if int(cast("chain-id")) != CHAIN_ID:
        die("the endpoint is not Sepolia's chain id")
    n, pending = int(cast("nonce", deployer)), int(cast("nonce", deployer, "--block", "pending"))
    if n != pending:
        die(f"the deployer has pending transactions (latest nonce {n}, pending {pending})")
    created = [cast("compute-address", deployer, "--nonce", str(n + k)).split()[-1] for k in range(7)]
    if any(has_code(c) for c in created):
        die(f"one of the deployer's next CREATE addresses already has code: {created}")
    vault = created[2]
    balance = int(cast("balance", deployer))
    need = 2 * 1_002_000_000_000_000 + 10**15  # two deposits with fees, and gas
    if balance < need:
        die(f"the deployer holds {balance} wei; the run needs about {need}")
    cid = json.loads(driver("config-id", "--identity", f"{LEZ}/session-a/session-a.json", "--vault", vault))["configId"]
    ch = a["channel"]
    gs = out(["python3", "-I", f"{REPO}/e2e/crown/identity.py", "genesis-state", "--tip-id", str(a["tip"]["id"]),
              "--tip-hash", a["tip"]["hash"], "--keys", ",".join(ch["keys"]), "--posting-timeframe",
              str(ch["postingTimeframe"]), "--config-tip-hash", ch["configTipHash"]])
    dests = json.loads(driver("destinations"))
    commit = lambda j, kind, dest: out(["python3", "-I", f"{REPO}/e2e/crown/identity.py", "commitment", "--chain-id",
                                        str(CHAIN_ID), "--vault", vault, "--depositor", deployer, "--j", str(j),
                                        "--kind", kind, "--dest", dest])
    c0, c1 = commit(0, "public", dests["public0"]), commit(1, "private", dests["private0"])
    label = "crown run" if MODE == "live" else "crown REHEARSAL on an anvil fork of Sepolia (stand-in beacon committee on LEZ)"
    jsave(f"{RUN}/identity.json", {
        "label": label, "chainId": CHAIN_ID, "vault": vault, "configId": cid, "genesisState": gs,
        "groth16Verifier": VERIFIER, "delta": DELTA, "deployer": deployer, "firstNonce": n, "createAddresses": created,
        "sessionA": {"sha256": sha256(f"{LEZ}/session-a/session-a.json"), "tip": a["tip"], "chain": a.get("chain")}})
    jsave(f"{RUN}/params.json", {
        "_source": f"{label}: configId and genesisState of LEZ session A's devnet, image ids of the crown build",
        "checkpointImageIdA": IDS["checkpoint"], "checkpointImageIdB": IDS["checkpoint_b"],
        "releaseImageId": IDS["release"], "lezConfigId": cid, "genesisState": gs, "nMin": 1, "tMin": 1,
        "postingTimeframe": ch["postingTimeframe"], "ethLezDefinition": ZERO32, "tokenLezDefinition": ZERO32})
    crown = jload(f"{RUN}/crown.json") if os.path.isfile(f"{RUN}/crown.json") else {}
    crown.update(chainId=CHAIN_ID, vault=vault, deposits=[
        {"j": 0, "denomIndex": 0, "commitment": c0, "kind": "public", "dest": dests["public0"]},
        {"j": 1, "denomIndex": 0, "commitment": c1, "kind": "private", "dest": dests["private0"]}])
    jsave(f"{RUN}/crown.json", crown)
    say(f"identity: deployer nonce {n}, vault {vault} (CREATE n+2), configId {cid}, genesisState {gs},"
        f" balance {balance} wei")
    return True


def st_deploy() -> bool:
    idn = identity()
    if os.path.isfile(f"{RUN}/deploy.json") and jload(f"{RUN}/deploy.json").get("complete") and has_code(idn["vault"]):
        return False
    rj = runj()
    if not rj.get("deploy_from_block"):
        rj["deploy_from_block"] = int(cast("block-number"))
        jsave(f"{RUN}/run.json", rj)
    sepolia("deploy", log="sepolia-deploy.log")
    d = jload(f"{RUN}/deploy.json")
    say(f"deploy: vault {d['vault']}, registry {d['registry']}, token {d['token']}, stop {d['emergencyStop']}")
    return True


def deposit_count() -> int:
    return first_int(call("depositCount(address)(uint64)", identity()["deployer"]))


def manifest_block(stage: str) -> int | None:
    p = f"{RUN}/manifest.json"
    if os.path.isfile(p):
        rows = jload(p).get("stages", {}).get(stage) or []
        if rows:
            return int(rows[-1]["blockNumber"], 16)
    return None


def st_deposit(j: int) -> bool:
    if deposit_count() > j:
        return False
    sepolia("deposit", str(j), log=f"sepolia-deposit-{j}.log")
    if deposit_count() <= j:
        die(f"deposit {j}: the vault counts {deposit_count()} deposits of the deployer")
    say(f"deposit {j}: block {manifest_block(f'deposit-{j}')}, leaves {first_int(call('nextLeafIndex()(uint64)'))}")
    return True


def deposit_block(j: int) -> int:
    """Deposit j's block: the manifest's row, or (a row lost with a stopped mirror) the vault's Deposit log."""
    b = manifest_block(f"deposit-{j}")
    if b is not None:
        return b
    topic = "0x" + "00" * 12 + identity()["deployer"].lower()[2:]
    logs = json.loads(cast("logs", "--json", "--from-block", str(runj().get("deploy_from_block") or 0), "--address",
                           identity()["vault"], "Deposit(address,uint64,uint64,address,uint256,bytes32)", topic))
    for lg in logs:
        if int(lg["data"][2:66], 16) == j:
            return int(lg["blockNumber"], 16)
    die(f"deposit {j}: neither the manifest nor the vault's logs have it")


def capture_dir(n: int) -> str:
    return f"{RUN}/captures/capture-{n}"


def st_capture(n: int) -> bool:
    d, vault = capture_dir(n), identity()["vault"]
    if os.path.isfile(f"{d}/fixture.json"):
        return False
    os.makedirs(os.path.dirname(d), exist_ok=True)
    block = deposit_block(n - 1)
    rj = runj()
    if MODE == "fork":
        slots = [s["slot"] for s in json.loads(out(["python3", "-I", f"{REPO}/e2e/crown/capture.py", "slots", "--driver",
                                                    f"{BUNDLE_DIR}/bin/crown-driver"]))["slots"]]
        part = d + ".part"  # renamed only once checked: a capture that fails its check is never taken on a resume
        shutil.rmtree(part, ignore_errors=True)
        args = [a for s in slots for a in ("--slot", s)]
        run(["python3", "-I", f"{REPO}/lez/crown/capture-anvil.py", "--rpc", LOCAL_RPC, "--vault", vault, "--from-block",
             str(rj["fork_block"]), *args, part], log=f"capture-{n}.log")
        chk = json.loads(out(["python3", "-I", f"{REPO}/e2e/crown/capture.py", "check", part, "--vault", vault.lower()]))
        os.rename(part, d)
        say(f"capture {n}: FORK capture of block {chk['block']} (capture-anvil.py, stand-in header), {chk['slots_proven']}"
            f" slots proven against its state root")
    else:
        say(f"capture {n}: waiting for block {block} (deposit {n - 1}) to be finalized")
        while int(cast("block", "finalized", "--field", "number")) < block:
            time.sleep(30)
        for attempt in range(1, CAPTURE_ATTEMPTS + 1):
            rc = run(["python3", "-I", f"{REPO}/e2e/crown/capture.py", "--vault", vault, "--from-block",
                      str(rj["deploy_from_block"]), "--expect-leaves", str(n), "--min-block", str(block), "--driver",
                      f"{BUNDLE_DIR}/bin/crown-driver", d], log=f"capture-{n}.log", check=False)
            if rc == 0:
                break
            if rc != 3:
                die(f"capture {n}: capture.py exited {rc} (logs/capture-{n}.log)")
            say(f"capture {n}, attempt {attempt}: not covered yet; the next finality update in about 6.4 minutes")
            time.sleep(390)
        else:
            die(f"capture {n}: not captured in {CAPTURE_ATTEMPTS} attempts")
        c = jload(d + ".json")
        say(f"capture {n}: block {c['block']}, leaves {c['leaves']}, {c['proofs']['slots_proven']} slots proven;"
            f" registry {c['slots'][3]['value'][:18]}..., fees 0 {c['slots'][4]['value'][:18]}..., 1 {c['slots'][5]['value'][:18]}...")
    return True


# ---- session B ----------------------------------------------------------------------------------------------------
def eth_fees_at() -> str | None:
    """The block time of the deployment's ETH registration (fee schedule 0 is announced and effective then), from the
    FERRATA_SENT lines of the deployment's log; None (crown-driver searches) if the log does not have it."""
    try:
        for line in open(f"{RUN}/logs/deploy.log"):
            parts = line.split()
            if len(parts) >= 3 and parts[0] == "FERRATA_SENT" and parts[1] == "registerETH":
                block = cast("receipt", parts[2], "blockNumber")
                return cast("block", block, "--field", "timestamp")
    except OSError:
        pass
    return None


def st_lez_b() -> bool:
    if have_session("b"):
        return False
    caps = [capture_dir(1), capture_dir(2)]
    chk = run([f"{BUNDLE_DIR}/bin/crown-driver", "check", "--chain", lez_chain(), "--program-bin",
               f"{BUNDLE_DIR}/program/ferrata_lez.bin", "--bridge-bin", f"{BUNDLE_DIR}/program/ferrata_bridge.bin",
               "--fixture", f"{BUNDLE_DIR}/fixture", "--capture", caps[0], "--capture", caps[1], "--identity",
               f"{LEZ}/session-a/session-a.json"], log="lez-check.log", check=False)
    say(f"crown-driver check of the captures: exit {chk} (advisory; logs/lez-check.log)")
    extra = {"CROWN_RECIPIENT": identity()["deployer"], "CROWN_PRIVATE": "real"}
    at = eth_fees_at()
    if at:
        extra["CROWN_ETH_FEES_AT"] = at
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        for n, c in ((1, caps[0]), (2, caps[1])):
            tf.add(c, arcname=f"capture-{n}")
    session("b", extra, inputs_tar=buf.getvalue())
    b = session_b()
    say(f"session B: T1 = {b['tipT1']['id']} {b['tipT1']['hash']}, blocks {b['blocks']}, {len(b.get('claims', []))} claims,"
        f" {len(jload(f'{LEZ}/session-b/burn-notes.json'))} burns")
    return True


# ---- the checkpoint -----------------------------------------------------------------------------------------------
CP = f"{RUN}/checkpoint"


def cp_name() -> str:
    return f"crown-{session_a()['tip']['id']}-{session_b()['tipT1']['id']}"


def cp_meta() -> dict:
    return jload(f"{CP}/input/{cp_name()}.json")


def st_checkpoint_input() -> bool:
    name = cp_name()
    if all(os.path.isfile(f"{CP}/input/{f}") for f in (f"{name}.input", f"{name}.json", "burns.json", "attestor.json")):
        return False
    shutil.rmtree(f"{CP}/input", ignore_errors=True)
    idn = identity()
    run([RB, "--node", f"{LEZ}/session-b/node", "--channel-key", KEY, "--genesis",
         f"{LEZ}/session-a/deployment-settings.yaml", "--session-a", f"{LEZ}/session-a/session-a.json", "--session-b",
         f"{LEZ}/session-b/session-b.json", "--vault", idn["vault"], "--config-id", idn["configId"], "--genesis-state",
         idn["genesisState"], "--name", name, "--out", f"{CP}/input"], log="checkpoint-input.log")
    m, burns = cp_meta(), jload(f"{CP}/input/burns.json")
    n_burns = len(burns.get("burns", [])) if isinstance(burns, dict) else len(burns)
    say(f"checkpoint input {name}: {os.path.getsize(f'{CP}/input/{name}.input')} bytes, parent state {m.get('parent_state')},"
        f" new state {m.get('new_state')}, {n_burns} burns counted")
    if m.get("parent_state") and m["parent_state"].lower() != idn["genesisState"].lower():
        die(f"the checkpoint's parent state {m['parent_state']} is not the vault's genesisState {idn['genesisState']}")
    return True


def st_checkpoint_exec() -> bool:
    if os.path.isfile(f"{CP}/exec.txt"):
        return False
    name = cp_name()
    p = subprocess.run([E2E, "exec", "--program", f"{PROG}/checkpoint_b.bin", "--input", f"{CP}/input/{name}.input",
                        "--meta", f"{CP}/input/{name}.json"], capture_output=True, text=True, cwd=REPO)
    with open(f"{RUN}/logs/checkpoint-exec.log", "a") as f:
        f.write(p.stdout + p.stderr)
    if p.returncode != 0:
        die(f"checkpoint_b's executor run failed: {p.stderr.strip()[-400:]}")
    with open(f"{CP}/exec.txt", "w") as f:
        f.write(p.stdout)
    say(f"checkpoint exec: {p.stdout.strip()[-300:]}")
    return True


def modal_fn(name: str):
    import modal
    return modal.Function.from_name(APP, name)


def stage_gpu(name: str) -> bytes:
    """e2e/crown/stage-gpu.sh's layout in memory: programs/{checkpoint-b,checkpoint,segment}.bin, fixtures/<name>.*,
    MANIFEST.json with the files' hashes and the programs' image ids."""
    files = {"programs/checkpoint-b.bin": f"{PROG}/checkpoint_b.bin", "programs/checkpoint.bin": f"{PROG}/checkpoint.bin",
             "programs/segment.bin": f"{PROG}/segment.bin", f"fixtures/{name}.input": f"{CP}/input/{name}.input",
             f"fixtures/{name}.json": f"{CP}/input/{name}.json"}
    manifest = {"source": f"crown-cloud drive.py: {name}", "commit": out(["git", "-C", REPO, "rev-parse", "HEAD"]),
                "local_image_ids": {"checkpoint": IDS["checkpoint"], "checkpoint-b": IDS["checkpoint_b"], "segment": IDS["segment"]},
                "files": {k: {"bytes": os.path.getsize(v), "sha256": sha256(v)} for k, v in files.items()}}
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        for arc, p in files.items():
            tf.add(p, arcname=arc)
        data = json.dumps(manifest, indent=1).encode()
        ti = tarfile.TarInfo("MANIFEST.json")
        ti.size = len(data)
        tf.addfile(ti, io.BytesIO(data))
    jsave(f"{CP}/gpu-manifest.json", manifest)
    return buf.getvalue()


def verify_receipt(receipt: str, image_id: str, meta: str, log: str) -> None:
    run([E2E, "verify", "--receipt", receipt, "--image-id", image_id, "--meta", meta], log=log)


def st_checkpoint_prove() -> bool:
    name, rec = cp_name(), f"{CP}/receipt-succinct.bin"
    meta = f"{CP}/input/{name}.json"
    if os.path.isfile(rec):
        return False
    import modal
    callf = f"{CP}/gpu-call.json"
    fc = None
    if os.path.isfile(callf):  # a GPU job of an earlier start: wait for it rather than queueing a second
        try:
            fc = modal.FunctionCall.from_id(jload(callf)["call_id"])
            say(f"checkpoint prove: waiting for the GPU call of an earlier start {jload(callf)['call_id']}")
        except Exception as e:
            say(f"checkpoint prove: the earlier GPU call is gone ({type(e).__name__}); queueing again")
            fc = None
    if fc is None:
        fc = modal_fn("gpu_prove").spawn(stage_gpu(name), name, f"crown-{RUN_NAME}", 1200)
        jsave(callf, {"call_id": fc.object_id, "spawned_at": utc()})
        say(f"checkpoint prove: {name} queued on gpu_prove (one L4), call {fc.object_id}; waiting up to {GPU_WAIT_S} s")
    t0, res = time.time(), None
    while time.time() - t0 < GPU_WAIT_S:
        try:
            res = fc.get(timeout=60)
            break
        except TimeoutError:
            continue
        except Exception as e:  # the call failed or its result expired
            say(f"checkpoint prove: the GPU call ended without a result: {type(e).__name__}: {str(e)[:200]}")
            os.remove(callf)
            die("the GPU proof failed: `start` again queues a new one")
    gdir = f"{CP}/gpu"
    if res is not None:
        os.makedirs(gdir, exist_ok=True)
        for fn, data in res["files"].items():
            with open(f"{gdir}/{fn}", "wb") as f:
                f.write(data)
        r = res["record"]
        p = r.get("prove") or {}
        say(f"checkpoint prove: GPU ok={r['ok']} gpu={r['gpu']} prover_wall_s={r['prover_wall_s']:.1f} total_s={p.get('total_s')}"
            f" segments={p.get('segments')} image_ids_equal={r['image_ids_equal']} inputs_equal={r['input_hashes_equal_manifest']}")
        if not res["ok"]:
            os.remove(callf)
            die("the GPU proof is not ok (checkpoint/gpu/run.json): `start` again queues a new one")
        src = f"{gdir}/receipt-succinct.bin"
    else:
        say(f"checkpoint prove: no GPU result after {GPU_WAIT_S} s: cancelling it, proving on this CPU instead")
        try:
            fc.cancel()
        except Exception:
            pass
        os.remove(callf)
        t1 = time.time()
        run([E2E, "prove", "--program", f"{PROG}/checkpoint_b.bin", "--input", f"{CP}/input/{name}.input", "--meta", meta,
             "--out-dir", f"{CP}/cpu"], log="checkpoint-prove-cpu.log")
        say(f"checkpoint prove: CPU proof in {time.time() - t1:.0f} s")
        src = f"{CP}/cpu/receipt-succinct.bin"
    verify_receipt(src, IDS["checkpoint_b"], meta, "checkpoint-prove.log")
    shutil.copy2(src, rec + ".part")
    os.replace(rec + ".part", rec)
    return True


def wrap(receipt: str, image_id: str, meta: str, work: str, dest: str, log: str) -> dict:
    """identity_p254 here, RISC Zero's Groth16 prover image (groth16_wrap), the Groth16 receipt, verified."""
    if not os.path.isfile(f"{work}/input.json"):
        run([E2E, "p254", "--receipt", receipt, "--image-id", image_id, "--out-dir", work], log=log)
    if not os.path.isfile(f"{work}/proof.json"):
        t0 = time.time()
        res = modal_fn("groth16_wrap").remote(open(f"{work}/input.json", "rb").read())
        with open(f"{RUN}/logs/{log}", "a") as f:
            f.write(res["log"].decode("utf-8", "replace")[-3000:] + f"\n[groth16_wrap] exit {res['exit_code']}, {res['seconds']:.0f} s\n")
        if res["exit_code"] != 0 or not res["proof_json"]:
            die(f"groth16_wrap failed (exit {res['exit_code']}; logs/{log})")
        with open(f"{work}/proof.json", "wb") as f:
            f.write(res["proof_json"])
        say(f"groth16_wrap: {res['seconds']:.0f} s ({time.time() - t0:.0f} s with the call)")
    run([E2E, "groth16", "--receipt", receipt, "--image-id", image_id, "--proof", f"{work}/proof.json", "--out-dir", dest],
        log=log)
    verify_receipt(f"{dest}/receipt-groth16.bin", image_id, meta, log)
    seal = "0x" + open(f"{dest}/seal.hex").read().strip().removeprefix("0x")
    journal = "0x" + open(f"{dest}/journal.hex").read().strip().removeprefix("0x")
    if journal[2:] != jload(meta)["journal"].removeprefix("0x"):
        die(f"the wrapped journal of {os.path.basename(meta)} is not the native one")
    if len(seal) != 2 + 2 * 260 or not seal.startswith("0x73c457ba"):
        die(f"the seal of {os.path.basename(meta)} is not 260 bytes with selector 0x73c457ba")
    return {"journal": journal, "seal": seal}


def st_checkpoint_wrap() -> bool:
    crown = jload(f"{RUN}/crown.json")
    if crown.get("candidate", {}).get("seal"):
        return False
    name = cp_name()
    cand = wrap(f"{CP}/receipt-succinct.bin", IDS["checkpoint_b"], f"{CP}/input/{name}.json", f"{CP}/wrap",
                f"{CP}/receipt", "checkpoint-wrap.log")
    crown["candidate"] = cand
    jsave(f"{RUN}/crown.json", crown)
    say(f"checkpoint wrap: seal {cand['seal'][:18]}... journal {len(cand['journal']) // 2 - 1} bytes")
    return True


def journal_words() -> list:
    j = bytes.fromhex(jload(f"{RUN}/crown.json")["candidate"]["journal"][2:])
    return ["0x" + j[i:i + 32].hex() for i in range(0, len(j), 32)]


def new_state() -> str:
    return journal_words()[7]  # domain, chainId, vault, configId, n, t, parentState, newState, burnRoot, ...


def burn_root() -> str:
    return journal_words()[8]


def final_state() -> str:
    return call("finalState()(bytes32)")


def wait_over() -> int:
    """Crown.s.sol waitOver(): submittedAt + deltaAtSubmit + (t - 1) * turnSeconds, 0 if not submitted."""
    rows = call("candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)", new_state()).splitlines()
    t, submitted, delta = first_int(rows[3]), first_int(rows[4]), first_int(rows[5])
    if submitted == 0:
        return 0
    return submitted + delta + (t - 1) * first_int(call("turnSeconds()(uint256)"))


def st_submit() -> bool:
    if final_state() == new_state() or wait_over() != 0:
        return False
    if final_state() != journal_words()[6]:
        die(f"the candidate's parent state {journal_words()[6]} is not the vault's final state {final_state()}")
    sepolia("submit", log="sepolia-submit.log")
    say(f"submit: block {manifest_block('submit')}, the wait is over at block time {wait_over()}")
    return True


# ---- releases ---------------------------------------------------------------------------------------------------
REL = f"{RUN}/releases"


def st_releases_prove() -> bool:
    crown = jload(f"{RUN}/crown.json")
    if len(crown.get("releases") or []) == 2 and all(x.get("seal") for x in crown["releases"]):
        return False
    idn = identity()
    if not os.path.isfile(f"{REL}/releases.json"):
        shutil.rmtree(REL, ignore_errors=True)
        jsave(f"{REL}/identity.json", {"chainId": CHAIN_ID, "vault": idn["vault"], "configId": idn["configId"]})
        run([E2E, "crown-release", "--identity", f"{REL}/identity.json", "--burns", f"{CP}/input/burns.json", "--notes",
             f"{LEZ}/session-b/burn-notes.json", "--out-dir", REL], log="releases.log")
    entries = jload(f"{REL}/releases.json")
    if len(entries) != 2:
        die(f"crown-release wrote {len(entries)} releases, the run has 2 burns")
    out_rows = []
    for e in entries:
        name = e["name"]
        meta = f"{REL}/release-{name}.json"
        rec = f"{REL}/{name}/prove/receipt-succinct.bin"
        if not os.path.isfile(rec):
            t0 = time.time()
            run([E2E, "prove", "--program", f"{PROG}/release.bin", "--input", f"{REL}/release-{name}.input", "--meta", meta,
                 "--out-dir", f"{REL}/{name}/prove"], log="releases.log")
            say(f"release {name}: CPU proof in {time.time() - t0:.0f} s")
        w = wrap(rec, IDS["release"], meta, f"{REL}/{name}/wrap", f"{REL}/{name}/receipt", "releases.log")
        out_rows.append({"name": name, "token": e["token"], "denomIndex": e["denomIndex"], "recipient": e["recipient"],
                         "relayer": e["relayer"], "relayFee": str(int(e["relayFee"], 16)), "burnRoot": e["burnRoot"],
                         "nullifier": e["nullifier"], "seal": w["seal"]})
        if e["burnRoot"].lower() != burn_root().lower():
            die(f"release {name}: its burn root {e['burnRoot']} is not the candidate's {burn_root()}")
    crown["releases"] = out_rows
    jsave(f"{RUN}/crown.json", crown)
    say(f"releases: {[r['name'] for r in out_rows]} proven and wrapped")
    return True


def st_wait() -> bool:
    over = wait_over()
    if final_state() == new_state():
        return False
    if over == 0:
        die("the candidate is not submitted")
    if head_time() >= over:
        return False
    pass_time(over, "the candidate's wait W")
    return True


def st_finalize() -> bool:
    if final_state() == new_state():
        return False
    sepolia("finalize", log="sepolia-finalize.log")
    if final_state() != new_state():
        die("after finalizeAttested the vault's final state is not the candidate's new state")
    say(f"finalize: block {manifest_block('finalizeAttested')}, final state {new_state()}")
    return True


def st_release(i: int) -> bool:
    r = jload(f"{RUN}/crown.json")["releases"][i]
    if call("nullifierUsed(bytes32)(bool)", r["nullifier"]) == "true":
        return False
    rows = call("burnRootStatus(bytes32)(bool,uint256)", r["burnRoot"]).splitlines()
    if rows[0].strip() != "true":
        die(f"release {i}: the burn root {r['burnRoot']} is not final")
    count = first_int(call("checkpointCount()(uint256)"))
    delay = first_int(call("releaseDelayFor(address,uint256)(uint256)", r["token"], str(count - 1)))
    pass_time(first_int(rows[1]) + delay, f"release {i}'s delay")
    sepolia("release", str(i), log=f"sepolia-release-{i}.log")
    if not call_until("true", "nullifierUsed(bytes32)(bool)", r["nullifier"]):
        die(f"release {i}: the nullifier is not used after the release (read 8 times over 70 s)")
    say(f"release {i} ({r['name']}): block {manifest_block(f'release-{i}')}")
    return True


def st_verify() -> bool:
    if os.path.isfile(f"{RUN}/DONE"):
        return False
    cmd = ["python3", "-I", f"{REPO}/e2e/crown/verify.py", RUN, "--lez", LEZ, "--channel-key", KEY, "--e2e", E2E,
           "--realblocks", RB]
    log = "verify.log"
    rc = run(cmd, log=log, check=False)
    if rc != 0:  # the endpoint may answer from a node a few blocks behind the last release (see call_until)
        say("verify: a check failed (logs/verify.log); reading everything again in 60 s")
        time.sleep(60)
        log = "verify-retry.log"
        rc = run(cmd, log=log, check=False)
    with open(f"{RUN}/logs/{log}") as src, open(f"{RUN}/verify.txt", "w") as dst:  # without the command line
        dst.writelines(l for l in src if not l.startswith("[") or "] $ " not in l)
    if rc != 0:
        die(f"verify.py found a failed check (verify.txt)")
    with open(f"{RUN}/DONE", "w") as f:
        f.write(f"crown run {RUN_NAME} ({MODE}) DONE at {utc()}\n")
    say("verify: every check passed; DONE")
    return True


STEPS = {"lez-a": st_lez_a, "identity": st_identity, "deploy": st_deploy, "deposit0": lambda: st_deposit(0),
         "capture1": lambda: st_capture(1), "deposit1": lambda: st_deposit(1), "capture2": lambda: st_capture(2),
         "lez-b": st_lez_b, "checkpoint-input": st_checkpoint_input, "checkpoint-exec": st_checkpoint_exec,
         "checkpoint-prove": st_checkpoint_prove, "checkpoint-wrap": st_checkpoint_wrap, "submit": st_submit,
         "releases-prove": st_releases_prove, "wait": st_wait, "finalize": st_finalize, "release0": lambda: st_release(0),
         "release1": lambda: st_release(1), "verify": st_verify}


def main() -> int:
    os.makedirs(f"{RUN}/logs", exist_ok=True)
    if MODE not in ("live", "fork"):
        print("drive.py <run> live|fork", file=sys.stderr)
        return 2
    rj = runj()
    if rj.get("mode") != MODE:
        say(f"STOP: the run is a {rj.get('mode')} run, not {MODE}")
        return 1
    if MODE == "fork":
        os.environ["ETH_FROM"] = rj["deployer"]
    halt = os.environ.get("CROWN_HALT_AFTER") or rj.get("halt_after")
    say(f"drive.py {RUN_NAME} {MODE}: done so far {done_list() or 'nothing'}" + (f"; halting after {halt}" if halt else ""))
    try:
        prepare_inputs()
        if MODE == "fork":
            start_anvil()
        for stage in STAGES:
            state(stage)
            t0 = time.time()
            acted = STEPS[stage]()
            mark(stage, f"{time.time() - t0:.0f}s" + ("" if acted else " (already done)"))
            if acted:
                say(f"stage {stage}: done in {time.time() - t0:.0f} s")
            if halt == stage:
                say(f"halted after {stage} (CROWN_HALT_AFTER); `start` again continues")
                state(stage, "halted")
                return 0
        state("DONE")
        return 0
    except Stop as e:
        say(f"STOP: {e}")
        state(jload(f"{RUN}/state.json").get("stage", "?") if os.path.isfile(f"{RUN}/state.json") else "?", f"stopped: {str(e)[:300]}")
        return 1
    finally:
        stop_anvil()


if __name__ == "__main__":
    sys.exit(main())
