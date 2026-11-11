#!/usr/bin/env python3
"""The laptop side of e2e/crown.sh (which sets MODAL_PROFILE, FERRATA_REPO and TMPDIR).

    ctl.py setup [--bundle FILE|none] [--programs DIR]  deploy ferrata-crown; the LEZ bundle and the R0BF programs to
                                                    the Volume (checked: sha256, image ids); a ping
    ctl.py start <run> live|fork|selftest [--yes] [--halt-after STAGE]
    ctl.py status <run>
    ctl.py fetch <run> [--lez]
    ctl.py stop [--app]
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import time

import modal
from modal.exception import NotFoundError, OutputExpiredError
from modal.volume import FileEntryType

APP_NAME = VOLUME_NAME = "ferrata-crown"
SECRET_NAME = "ferrata-sepolia-key"
REPO = os.environ["FERRATA_REPO"]
SPIKE = os.path.dirname(REPO)
SCRATCH = os.path.join(os.environ["TMPDIR"], "crown-cloud")
IGNORED_INPUTS = ("vault/lib",)  # git ignores it; forge needs it
DEPLOYER = "0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07"  # the throwaway deployer of the Secret (sepolia-live-2's)
PUBLIC_SEPOLIA = "https://ethereum-sepolia-rpc.publicnode.com"
IDS = {"checkpoint_b": "0x7e51065ffd0a8004fbdd0452871c4a77da7b59e6499365f43d61363fd4307754",
       "checkpoint": "0x117384b6322ee8563e147eddb61341699fb3edfd3672c3cb39bc44a49d72086d",
       "segment": "0xdad887eb1bc97fb6901d6b0f92779a18ebf4d70bc425405dc1de8265cfadeb78",
       "release": "0x20595dc54bbd80cf487e4666286db9aab7a048e2e0d7646b42ec6d3c90ca2625"}
BUNDLES = ("tmp/crown-lez/bundle.tar.zst", "tmp/crown-lez/rehearsal/vol/bundle.tar.zst")
STAGES = ["lez-a", "identity", "deploy", "deposit0", "capture1", "deposit1", "capture2", "lez-b", "checkpoint-input",
          "checkpoint-exec", "checkpoint-prove", "checkpoint-wrap", "submit", "releases-prove", "wait", "finalize",
          "release0", "release1", "verify"]


def die(msg: str) -> None:
    print(f"crown: {msg}", file=sys.stderr)
    sys.exit(1)


def utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def git(*args: str, binary: bool = False):
    r = subprocess.run(["git", "-C", REPO, *args], capture_output=True, text=not binary)
    if r.returncode != 0:
        die(f"git {' '.join(args)} failed: {r.stderr if not binary else r.stderr.decode(errors='replace')}")
    return r.stdout


def volume() -> modal.Volume:
    return modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)


def vread(vol: modal.Volume, path: str) -> bytes | None:
    try:
        return b"".join(vol.read_file(path))
    except (NotFoundError, FileNotFoundError):
        return None


def vjson(vol: modal.Volume, path: str):
    data = vread(vol, path)
    return json.loads(data) if data else None


def call_state(call_id: str) -> tuple[str, object]:
    """('active', None) | ('finished', result) | ('failed', message) | ('expired', None)"""
    try:
        return "finished", modal.FunctionCall.from_id(call_id).get(timeout=0)
    except TimeoutError:
        return "active", None
    except OutputExpiredError:
        return "expired", None
    except Exception as e:
        return "failed", f"{type(e).__name__}: {str(e)[:300]}"


def runs_on_volume(vol: modal.Volume) -> list[str]:
    try:
        return sorted(e.path.rstrip("/").split("/")[-1] for e in vol.listdir("cloud") if e.type == FileEntryType.DIRECTORY)
    except (NotFoundError, FileNotFoundError):
        return []


def check_run_name(run: str) -> None:
    if not re.fullmatch(r"crown-[A-Za-z0-9._-]+", run):
        die(f"not a crown run name (crown-...): {run!r}")


# ---- setup --------------------------------------------------------------------------------------------------------
def check_programs(d: str) -> list:
    """The R0BF programs of crown-proofs (ids.jsonl): every file's sha256 as recorded, every id the crown build's."""
    rows = [json.loads(l) for l in open(os.path.join(d, "ids.jsonl")) if l.strip()]
    seen = {}
    for r in rows:
        name = os.path.basename(r["program"])[:-4]
        path = os.path.join(d, f"{name}.bin")
        if sha256(path) != r["sha256"]:
            die(f"{path}: sha256 is not ids.jsonl's")
        if IDS.get(name) != r["image_id"]:
            die(f"{name}: image id {r['image_id']} is not the crown build's {IDS.get(name)}")
        seen[name] = path
    if set(seen) != set(IDS):
        die(f"{d} holds {sorted(seen)}, the run needs {sorted(IDS)}")
    return [os.path.join(d, "ids.jsonl")] + [seen[n] for n in sorted(seen)]


def setup(bundle: str | None, programs: str | None) -> None:
    try:
        modal.Secret.from_name(SECRET_NAME).hydrate()
    except NotFoundError:
        die(f"no Secret {SECRET_NAME} in this workspace: `e2e/sepolia-cloud.sh setup` makes it (the crown run reuses it)")
    if bundle != "none":
        bundle = bundle or next((os.path.join(SPIKE, b) for b in BUNDLES if os.path.isfile(os.path.join(SPIKE, b))), None)
        if not bundle or not os.path.isfile(bundle):
            die(f"no LEZ bundle (lez/crown/build.sh all on the build box, fetched to one of {BUNDLES}); give --bundle FILE")
    progs = check_programs(programs or os.path.join(SPIKE, "tmp/crown-proofs/programs"))
    os.makedirs(SCRATCH, exist_ok=True)
    log = os.path.join(SCRATCH, "deploy.log")
    print(f"deploying {APP_NAME} (the first time builds its images, about 15 minutes); log {log}")
    with open(log, "w") as f:
        rc = subprocess.run(["modal", "deploy", "e2e/crown/cloud/app.py"], cwd=REPO, stdout=f, stderr=subprocess.STDOUT).returncode
    if rc != 0:
        die("modal deploy failed:\n  " + "\n  ".join(open(log, errors="replace").read().splitlines()[-25:]))
    vol = volume()
    meta = {"sha256": sha256(bundle), "bytes": os.path.getsize(bundle), "source": os.path.relpath(bundle, SPIKE),
            "uploaded_at": utc()} if bundle != "none" else {"sha256": None, "source": "none (--bundle none)", "bytes": 0}
    have = vjson(vol, "inputs/bundle.json") or {}
    with vol.batch_upload(force=True) as b:
        if meta["sha256"] and have.get("sha256") != meta["sha256"]:
            b.put_file(bundle, "inputs/bundle.tar.zst")
            mp = os.path.join(SCRATCH, "bundle.json")
            with open(mp, "w") as f:
                json.dump(meta, f, indent=1)
            b.put_file(mp, "inputs/bundle.json")
        for p in progs:
            b.put_file(p, f"inputs/programs/{os.path.basename(p)}")
    if meta["sha256"]:
        print(f"inputs: LEZ bundle {meta['source']} ({meta['bytes']} bytes, sha256 {meta['sha256'][:16]}...)"
              f"{' already there' if have.get('sha256') == meta['sha256'] else ' uploaded'}")
    else:
        print(f"inputs: no LEZ bundle given (--bundle none); on the Volume: {have.get('source', 'none')}")
    print(f"inputs: programs {', '.join(sorted(IDS))} (ids and sha256 as ids.jsonl)")
    print("ping:", modal.Function.from_name(APP_NAME, "ping").remote())
    print(f"setup done: {APP_NAME} deployed (idle until `start`)")


# ---- start --------------------------------------------------------------------------------------------------------
def cast(*a: str, rpc: str) -> str:
    r = subprocess.run(["cast", *a], env={**os.environ, "ETH_RPC_URL": rpc}, capture_output=True, text=True, timeout=60)
    if r.returncode != 0:
        die(f"cast {a[0]} failed (read-only): {r.stderr.strip()[:200]}")
    return r.stdout.strip()


def gate(deployer: str) -> None:
    """Before a live run: what will be signed, by whom, at what price; then a typed yes (ethskills wallets)."""
    rpc = os.environ.get("SEPOLIA_RPC_URL") or PUBLIC_SEPOLIA
    chain = cast("chain-id", rpc=rpc)
    if chain != "11155111":
        die(f"the endpoint is chain {chain}, not Sepolia")
    nonce, pending = cast("nonce", deployer, rpc=rpc), cast("nonce", deployer, "--block", "pending", rpc=rpc)
    balance, price = cast("balance", deployer, "--ether", rpc=rpc), int(cast("gas-price", rpc=rpc))
    vault = cast("compute-address", deployer, "--nonce", str(int(nonce) + 2), rpc=rpc).split()[-1]
    gas = 60_000_000  # deploy (the vault alone ~39.5 M under EIP-8037) + 6 stage transactions (~1 M): an upper bound
    print(f"""
LIVE crown run on Sepolia (chain {chain}), signed in the cloud (Modal app {APP_NAME}, workspace
{os.environ.get('MODAL_PROFILE')}) with the keystore of the Secret {SECRET_NAME}:
  sender {deployer}: nonce {nonce} (pending {pending}), balance {balance} ETH
  vault  {vault} (CREATE at nonce {int(nonce) + 2}; the run checks it again before deploying)
  13 transactions: stop, library, vault, token, register ETH, register token, renounce; 2 deposits of
     0.001002 ETH each (0.001 + relay fee; returned by the releases less the checkpoint fees); submitCandidate;
     finalizeAttested (after W = 3600 s); 2 releases. Gas at most ~{gas:,} at {price} wei: ~{gas * price / 1e18:.9f} ETH.
  Modal: about 3 to 4 hours (GPU queue permitting), ~2-4 USD on {os.environ.get('MODAL_PROFILE')}.
""")
    if nonce != pending:
        die("the deployer has pending transactions: nothing started")
    if not sys.stdin.isatty():
        die("not a terminal: nothing started. Run it from a terminal, or add --yes")
    if input("Type yes to start it: ").strip() != "yes":
        die("not started")


def pack(run: str) -> tuple[str, str, str, dict]:
    os.makedirs(SCRATCH, exist_ok=True)
    commit = git("rev-parse", "HEAD").strip()
    bundle = os.path.join(SCRATCH, f"{run}.bundle")
    git("bundle", "create", bundle, "HEAD")
    files = [p for p in git("ls-files", "-co", "--exclude-standard", "-z", binary=True).decode().split("\0") if p]
    files = [p for p in files if not p.startswith("e2e/runs/") and os.path.lexists(os.path.join(REPO, p))]
    deleted = [p for p in git("ls-files", "-d", "-z", binary=True).decode().split("\0") if p]
    bad = [p for p in files if p.endswith(".env") or "keystore" in p.lower() or os.path.basename(p) == "channel_signing_key"]
    if bad:
        die(f"refusing to upload {bad}")
    for d in IGNORED_INPUTS:
        if not os.path.isdir(os.path.join(REPO, d)) or not os.listdir(os.path.join(REPO, d)):
            die(f"{d} is missing (the vault's Foundry libraries: forge-std 1.10.0, openzeppelin 5.4.0, risc0-ethereum 3.0.0)")
    tar = os.path.join(SCRATCH, f"{run}.src.tar.gz")
    with tarfile.open(tar, "w:gz", compresslevel=6) as tf:
        for p in files:
            tf.add(os.path.join(REPO, p), arcname=p, recursive=False)
        for d in IGNORED_INPUTS:
            tf.add(os.path.join(REPO, d), arcname=d)
    src = {"commit": commit, "files": files, "deleted": deleted, "ignored_inputs": list(IGNORED_INPUTS), "packed_at": utc()}
    meta = os.path.join(SCRATCH, f"{run}.src.json")
    with open(meta, "w") as f:
        json.dump(src, f)
    return bundle, tar, meta, src


def start(run: str, mode: str, yes: bool, halt_after: str | None) -> None:
    check_run_name(run)
    if halt_after and halt_after not in STAGES:
        die(f"--halt-after: one of {', '.join(STAGES)}")
    fname = "runner" if mode == "live" else "runner_rehearsal"
    fn = modal.Function.from_name(APP_NAME, fname)
    try:
        fn.hydrate()
    except NotFoundError:
        die(f"the app {APP_NAME} is not deployed: `e2e/crown.sh setup` first")
    vol = volume()
    if mode != "selftest" and (not vjson(vol, "inputs/bundle.json") or vread(vol, "inputs/programs/ids.jsonl") is None):
        die("the Volume has no inputs: `e2e/crown.sh setup`")
    for other in runs_on_volume(vol):
        c = vjson(vol, f"cloud/{other}/call.json")
        if c and call_state(c["call_id"])[0] == "active":
            if other == run:
                die(f"a runner for {run} is already active ({c['call_id']}): `status {run}`, or `stop`")
            print(f"note: the runner of {other} is active")
    rj = vjson(vol, f"runs/{run}/run.json")
    deployer = os.environ.get("ETH_FROM") or DEPLOYER
    if rj:
        if rj["mode"] != mode and mode != "selftest":
            die(f"{run} is a {rj['mode']} run")
        deployer = rj["deployer"]
        done = [l.split()[0] for l in (vread(vol, f"runs/{run}/stages.done") or b"").decode().splitlines() if l.strip()]
        print(f"resuming {run} (created {rj['created_at']}): done {', '.join(done) or 'nothing'}")
    if mode == "live" and not yes:
        gate(deployer)
    bundle, tar, meta, src = pack(run)
    local_rj = None
    if not rj:
        rj = {"run": run, "mode": "fork" if mode == "selftest" else mode, "deployer": deployer, "chainId": 11155111,
              "created_at": utc(), "label": "crown run" if mode == "live" else "crown REHEARSAL (anvil fork of Sepolia)"}
    if halt_after is not None:
        rj["halt_after"] = halt_after
    elif "halt_after" in rj and rj["halt_after"]:
        rj["halt_after"] = None
    local_rj = os.path.join(SCRATCH, f"{run}.run.json")
    with open(local_rj, "w") as f:
        json.dump(rj, f, indent=1)
    with vol.batch_upload(force=True) as b:
        b.put_file(bundle, f"cloud/{run}/src.bundle")
        b.put_file(tar, f"cloud/{run}/src.tar.gz")
        b.put_file(meta, f"cloud/{run}/src.json")
        b.put_file(local_rj, f"runs/{run}/run.json")
    for p in (bundle, tar, meta, local_rj):
        os.remove(p)
    call = fn.spawn(run=run, repo=REPO, mode=mode, commit=src["commit"])
    record = {"call_id": call.object_id, "function": fname, "run": run, "mode": mode, "commit": src["commit"], "started_at": utc()}
    rec = os.path.join(SCRATCH, f"{run}.call.json")
    with open(rec, "w") as f:
        json.dump(record, f, indent=1)
    with vol.batch_upload(force=True) as b:
        b.put_file(rec, f"cloud/{run}/call.json")
    print(f"started {mode} run {run}: {fname} call {call.object_id}, commit {src['commit'][:12]}"
          + (f", halting after {halt_after}" if halt_after else ""))
    print(f"It runs without this machine. `e2e/crown.sh status {run}`; at the end `e2e/crown.sh fetch {run}`.")


# ---- status, fetch, stop ----------------------------------------------------------------------------------------
def status(run: str) -> None:
    check_run_name(run)
    vol = volume()
    call = vjson(vol, f"cloud/{run}/call.json")
    if not call:
        die(f"no run {run} in the cloud")
    st = vjson(vol, f"cloud/{run}/status.json") or {}
    state, info = call_state(call["call_id"])
    if state == "active":
        what = ("running, phase " + st.get("phase", "?")) if st.get("call_id") == call["call_id"] else "waiting for a container"
    elif state == "finished":
        what = f"finished, exit {info.get('exit_code') if isinstance(info, dict) else info}"
    elif state == "failed":
        what = f"failed: {info}"
    else:
        what = "finished (result expired)"
    print(f"run {run} ({call['mode']}, commit {call['commit'][:12]}), {call.get('function', 'runner')} {call['call_id']}"
          f" started {call['started_at']}: {what}")
    ds = vjson(vol, f"runs/{run}/state.json") or {}
    done = [l.split()[0] for l in (vread(vol, f"runs/{run}/stages.done") or b"").decode().splitlines() if l.strip()]
    print(f"  stage {ds.get('stage', '-')}{(': ' + ds['note']) if ds.get('note') else ''} (updated {ds.get('updated_at', '-')})")
    print(f"  done {len(done)}/{len(STAGES)}: {', '.join(done) or 'none'}")
    if vread(vol, f"runs/{run}/DONE"):
        print("  DONE: " + vread(vol, f"runs/{run}/DONE").decode().strip())
    log = vread(vol, f"runs/{run}/crown.log")
    name = "crown.log"
    if not log or st.get("phase") in ("preparing", "building"):
        log, name = vread(vol, f"cloud/{run}/runner.log") or b"", "runner.log"
    print(f"  last lines of {name}:\n    " + "\n    ".join(log.decode(errors="replace").splitlines()[-15:]))


def fetch(run: str, lez: bool) -> None:
    check_run_name(run)
    vol = volume()
    stage = os.path.join(SCRATCH, f"fetch-{run}")
    shutil.rmtree(stage, ignore_errors=True)
    n = 0
    prefixes = [(f"runs/{run}", stage), (f"cloud/{run}", os.path.join(stage, "work", "cloud"))]
    if lez:
        prefixes += [(f"lez/{run}/{d}", os.path.join(stage, "work", "lez", d)) for d in ("session-a", "session-b", "inputs")]
    for prefix, dest in prefixes:
        try:
            entries = vol.listdir(prefix, recursive=True)
        except (NotFoundError, FileNotFoundError):
            continue
        for e in entries:
            rel = e.path.lstrip("/")
            rel = rel[len(prefix):].lstrip("/") if rel.startswith(prefix) else rel
            skip = (prefix.startswith("cloud/") and rel.startswith(("src.", "broadcast/"))) or rel.endswith("channel_signing_key") \
                or rel.startswith("anvil/")
            if e.type != FileEntryType.FILE or skip:
                continue
            outp = os.path.join(dest, rel)
            os.makedirs(os.path.dirname(outp), exist_ok=True)
            with open(outp, "wb") as f:
                for chunk in vol.read_file(f"{prefix}/{rel}"):
                    f.write(chunk)
            n += 1
    if n == 0:
        die(f"nothing of run {run} in the cloud")
    rundir = os.path.join(REPO, "e2e/runs", run)
    shutil.copytree(stage, rundir, dirs_exist_ok=True)
    shutil.rmtree(stage)
    print(f"fetched {n} files into e2e/runs/{run}/ (the runner's own logs in work/cloud/{', the LEZ records in work/lez/' if lez else ''})")


def stop(app_too: bool) -> None:
    vol = volume()
    for run in runs_on_volume(vol):
        c = vjson(vol, f"cloud/{run}/call.json")
        if c and call_state(c["call_id"])[0] == "active":
            modal.FunctionCall.from_id(c["call_id"]).cancel()
            print(f"cancelled the runner of {run} ({c['call_id']}); `start {run}` resumes it")
        g = vjson(vol, f"runs/{run}/checkpoint/gpu-call.json")
        if g and call_state(g["call_id"])[0] == "active":
            modal.FunctionCall.from_id(g["call_id"]).cancel()
            print(f"cancelled the GPU proof of {run} ({g['call_id']})")
    if app_too:
        r = subprocess.run(["modal", "app", "stop", "--yes", APP_NAME], capture_output=True, text=True)
        print(f"app {APP_NAME}: " + ("stopped (`setup` deploys it again)" if r.returncode == 0 else (r.stdout + r.stderr).strip()[-300:]))
    print(f"kept: Volume {VOLUME_NAME} (inputs, runs, LEZ generations); nothing runs")


def main() -> None:
    a = sys.argv[1:]
    opt = lambda name: a[a.index(name) + 1] if name in a and a.index(name) + 1 < len(a) else None
    if a[:1] == ["setup"]:
        setup(opt("--bundle"), opt("--programs"))
    elif len(a) >= 3 and a[0] == "start" and a[2] in ("live", "fork", "selftest"):
        start(a[1], a[2], "--yes" in a, opt("--halt-after"))
    elif len(a) >= 2 and a[0] == "status":
        status(a[1])
    elif len(a) >= 2 and a[0] == "fetch":
        fetch(a[1], "--lez" in a)
    elif a[:1] == ["stop"]:
        stop("--app" in a)
    else:
        print(__doc__, file=sys.stderr)
        sys.exit(2)


if __name__ == "__main__":
    main()
