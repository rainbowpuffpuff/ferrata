#!/usr/bin/env python3
"""The laptop side of e2e/sepolia-cloud.sh (which sets MODAL_PROFILE, FERRATA_REPO and TMPDIR).

    ctl.py setup                          the Secret from the files named by ETH_KEYSTORE and ETH_PASSWORD,
                                          with SEPOLIA_RPC_URL and ETH_FROM; then `modal deploy`; then a ping
    ctl.py start <run> live|fork|selftest [--yes]
    ctl.py status <run>
    ctl.py fetch <run>
    ctl.py stop
"""
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import time

import modal
from modal.exception import AlreadyExistsError, NotFoundError, OutputExpiredError
from modal.volume import FileEntryType

APP_NAME = VOLUME_NAME = "ferrata-sepolia"
SECRET_NAME = "ferrata-sepolia-key"
REPO = os.environ["FERRATA_REPO"]
SCRATCH = os.path.join(os.environ["TMPDIR"], "sepolia-cloud")
IGNORED_INPUTS = ("e2e/programs", "vault/lib")  # git ignores them; the run needs them
PROGRAMS = ("release", "checkpoint", "segment")


def die(msg: str) -> None:
    print(f"sepolia-cloud: {msg}", file=sys.stderr)
    sys.exit(1)


def utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


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


def vjson(vol: modal.Volume, path: str) -> dict | None:
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
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", run):
        die(f"not a run name: {run!r}")


# ---- setup ----------------------------------------------------------------------------------------------
def setup() -> None:
    missing = [k for k in ("ETH_KEYSTORE", "ETH_PASSWORD", "SEPOLIA_RPC_URL", "ETH_FROM") if not os.environ.get(k)]
    if missing:
        die(f"set {', '.join(missing)} (in e2e/sepolia-live.env or FERRATA_ENV_FILE)")
    addr = os.environ["ETH_FROM"]
    if not re.fullmatch(r"0x[0-9a-fA-F]{40}", addr):
        die("ETH_FROM is not an address")
    for k in ("ETH_KEYSTORE", "ETH_PASSWORD"):
        if not os.path.isfile(os.environ[k]):
            die(f"{k} does not name a file")
    # Read by path, here only: the contents go to the Secret and nowhere else.
    try:
        with open(os.environ["ETH_KEYSTORE"], encoding="utf-8") as f:
            keystore = f.read()
        with open(os.environ["ETH_PASSWORD"], encoding="utf-8") as f:
            password = f.read()
    except UnicodeDecodeError:
        die("the keystore file or the password file is not UTF-8 text")
    try:
        ks = json.loads(keystore)
    except ValueError:
        die("the keystore file is not JSON")
    if "crypto" not in ks and "Crypto" not in ks:
        die("the keystore file is not an encrypted keystore (no crypto section)")
    if ks.get("address") and ks["address"].lower().removeprefix("0x") != addr.lower().removeprefix("0x"):
        die("the keystore's address is not ETH_FROM")
    if not password:
        die("the password file is empty")
    contents = {"FERRATA_KEYSTORE_JSON": keystore, "FERRATA_KEYSTORE_PASSWORD": password,
                "SEPOLIA_RPC_URL": os.environ["SEPOLIA_RPC_URL"], "ETH_FROM": addr}
    try:
        modal.Secret.objects.create(SECRET_NAME, contents)
        print(f"Secret {SECRET_NAME}: created (keystore, password, endpoint, sender {addr})")
    except AlreadyExistsError:
        modal.Secret.from_name(SECRET_NAME).update(contents)
        print(f"Secret {SECRET_NAME}: updated (keystore, password, endpoint, sender {addr})")
    del contents, keystore, password, ks
    os.makedirs(SCRATCH, exist_ok=True)
    log = os.path.join(SCRATCH, "deploy.log")
    print(f"deploying {APP_NAME} (the first time builds three images, about 15 minutes); log {log}")
    with open(log, "w") as f:
        rc = subprocess.run(["modal", "deploy", "e2e/cloud/app.py"], cwd=REPO, stdout=f, stderr=subprocess.STDOUT).returncode
    if rc != 0:
        tail = open(log, errors="replace").read().splitlines()[-25:]
        die("modal deploy failed:\n  " + "\n  ".join(tail))
    print("ping:", modal.Function.from_name(APP_NAME, "ping").remote())
    print(f"setup done: app {APP_NAME} deployed (idle: no container runs until `start`)")


# ---- start ----------------------------------------------------------------------------------------------
def check_inputs(rundir: str) -> dict:
    for f in ("fixture.json", "params.json", "programs.json"):
        if not os.path.isfile(os.path.join(rundir, f)):
            die(f"{rundir} has no {f}")
    want = json.load(open(os.path.join(rundir, "programs.json")))
    for p in PROGRAMS:
        path = os.path.join(REPO, "e2e/programs", f"{p}.bin")
        if not os.path.isfile(path):
            die(f"e2e/programs/{p}.bin is missing: the recorded guest programs (e2e/README.md)")
        sha = subprocess.run(["sha256sum", path], capture_output=True, text=True).stdout.split()[0]
        if sha != want[p]["sha256"]:
            die(f"e2e/programs/{p}.bin has sha256 {sha}, the run's programs.json {want[p]['sha256']}")
    if not os.path.isdir(os.path.join(REPO, "vault/lib")) or not os.listdir(os.path.join(REPO, "vault/lib")):
        die("vault/lib is missing: the vault's Foundry libraries (vault/README.md)")
    return json.load(open(os.path.join(rundir, "fixture.json")))


def planned_gas() -> str:
    """The gas of the same fifteen transactions on the fork rehearsal (e2e/runs/fork-driver/manifest.json)."""
    try:
        m = json.load(open(os.path.join(REPO, "e2e/runs/fork-driver/manifest.json")))
        return str(sum(int(tx["gasUsed"], 16) for txs in m["stages"].values() for tx in txs))
    except (OSError, ValueError, KeyError):
        return ""


def gate(fixture: dict, first: int) -> None:
    """Before a live run: what will be signed, by whom, at what price; then a typed yes (ethskills wallets)."""
    for k in ("SEPOLIA_RPC_URL", "ETH_FROM"):
        if not os.environ.get(k):
            die(f"set {k} (in e2e/sepolia-live.env or FERRATA_ENV_FILE)")
    env = {**os.environ, "ETH_RPC_URL": os.environ["SEPOLIA_RPC_URL"]}  # the endpoint stays off command lines

    def cast(*a: str) -> str:
        r = subprocess.run(["cast", *a], env=env, capture_output=True, text=True, timeout=60)
        if r.returncode != 0:
            die(f"cast {a[0]} failed (read-only): {r.stderr.strip()[:200]}")
        return r.stdout.strip()

    sender = os.environ["ETH_FROM"]
    chain, nonce, balance = cast("chain-id"), cast("nonce", sender), cast("balance", sender, "--ether")
    price = cast("gas-price")
    if chain != str(fixture["chainId"]):
        die(f"the endpoint is chain {chain}, the fixture is for {fixture['chainId']}")
    if sender.lower() != fixture["deployer"].lower():
        die("ETH_FROM is not the fixture's deployer")
    gas = planned_gas()
    cost = f"{int(gas) * int(price) / 1e18:.9f} ETH at {price} wei per gas now" if gas else "unknown"
    print(f"""
LIVE RUN on chain {chain} (Sepolia), signed in the cloud with the keystore of the Secret {SECRET_NAME}:
  sender {sender}: nonce {nonce} (the run's first is {first}), balance {balance} ETH
  vault  {fixture['vault']} (created by this run)
  15 transactions; {gas or '?'} gas on the fork rehearsal; cost {cost}
""")
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
    files = [p for p in files if not p.startswith(f"e2e/runs/{run}/") and os.path.lexists(os.path.join(REPO, p))]
    deleted = [p for p in git("ls-files", "-d", "-z", binary=True).decode().split("\0") if p]
    bad = [p for p in files if p.endswith(".env") or "keystore" in p.lower()]
    if bad:
        die(f"refusing to upload {bad}")
    tar = os.path.join(SCRATCH, f"{run}.src.tar.gz")
    with tarfile.open(tar, "w:gz", compresslevel=6) as tf:
        for p in files:
            tf.add(os.path.join(REPO, p), arcname=p, recursive=False)
        for d in IGNORED_INPUTS:
            tf.add(os.path.join(REPO, d), arcname=d)
    src = {"commit": commit, "files": files, "deleted": deleted, "ignored_inputs": list(IGNORED_INPUTS),
           "packed_at": utc()}
    meta = os.path.join(SCRATCH, f"{run}.src.json")
    with open(meta, "w") as f:
        json.dump(src, f)
    return bundle, tar, meta, src


def start(run: str, mode: str, yes: bool) -> None:
    check_run_name(run)
    rundir = os.path.join(REPO, "e2e/runs", run)
    fixture = check_inputs(rundir)
    fn = modal.Function.from_name(APP_NAME, "runner")
    try:
        fn.hydrate()
    except NotFoundError:
        die(f"the app {APP_NAME} is not deployed: run `e2e/sepolia-cloud.sh setup` first")
    vol = volume()
    for other in runs_on_volume(vol):
        c = vjson(vol, f"cloud/{other}/call.json")
        if c and call_state(c["call_id"])[0] == "active":
            if other == run:
                die(f"a runner for {run} is already active ({c['call_id']}): `status {run}`, or `stop`")
            print(f"note: the runner of {other} is active; this one starts when it ends (one runner at a time)")
    if mode == "live" and not yes:
        gate(fixture, json.load(open(os.path.join(rundir, "params.json"))).get("first_nonce", 0))
    have_run = vread(vol, f"runs/{run}/fixture.json") is not None
    bundle, tar, meta, src = pack(run)
    with vol.batch_upload(force=True) as b:
        b.put_file(bundle, f"cloud/{run}/src.bundle")
        b.put_file(tar, f"cloud/{run}/src.tar.gz")
        b.put_file(meta, f"cloud/{run}/src.json")
        if not have_run:
            b.put_directory(rundir, f"runs/{run}")
    for p in (bundle, tar, meta):
        os.remove(p)
    call = fn.spawn(run=run, repo=REPO, mode=mode, commit=src["commit"])
    record = {"call_id": call.object_id, "run": run, "mode": mode, "commit": src["commit"], "started_at": utc()}
    rec = os.path.join(SCRATCH, f"{run}.call.json")
    with open(rec, "w") as f:
        json.dump(record, f, indent=1)
    with vol.batch_upload(force=True) as b:
        b.put_file(rec, f"cloud/{run}/call.json")
    os.makedirs(os.path.join(rundir, "work"), exist_ok=True)
    shutil.move(rec, os.path.join(rundir, "work", "cloud-call.json"))
    print(f"started {mode} run {run}: runner call {call.object_id}, commit {src['commit'][:12]};"
          f" run directory {'from the cloud copy (resumed)' if have_run else 'uploaded'}.")
    print(f"It runs on without this machine. `e2e/sepolia-cloud.sh status {run}`; at the end `fetch {run}`.")


# ---- status, fetch, stop --------------------------------------------------------------------------------
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
        what = f"finished, driver exit {info.get('exit_code') if isinstance(info, dict) else info}"
    elif state == "failed":
        what = f"failed: {info}"
    else:
        what = "finished (result expired)"
    print(f"run {run} ({call['mode']}, commit {call['commit'][:12]}), runner {call['call_id']} started {call['started_at']}: {what}")
    if st.get("call_id") == call["call_id"]:
        print(f"  phase {st.get('phase')}, updated {st.get('updated_at')}"
              + (f", exit {st['exit_code']}" if st.get("exit_code") is not None else "")
              + (f", fork block {st['fork_block']}" if st.get("fork_block") else ""))
    done = (vread(vol, f"runs/{run}/stages.done") or b"").decode().split()
    print("  stages done: " + (", ".join(done[i] for i in range(0, len(done), 2)) or "none"))
    log = vread(vol, f"runs/{run}/sepolia-live.log")
    name = "sepolia-live.log"
    if not log or st.get("phase") in ("preparing", "building"):
        log, name = vread(vol, f"cloud/{run}/runner.log") or b"", "runner.log"
    lines = log.decode(errors="replace").splitlines()[-15:]
    print(f"  last lines of {name}:\n    " + "\n    ".join(lines))


def fetch(run: str) -> None:
    check_run_name(run)
    vol = volume()
    stage = os.path.join(SCRATCH, f"fetch-{run}")
    shutil.rmtree(stage, ignore_errors=True)
    n = 0
    for prefix, dest in ((f"runs/{run}", stage), (f"cloud/{run}", os.path.join(stage, "work", "cloud"))):
        try:
            entries = vol.listdir(prefix, recursive=True)
        except (NotFoundError, FileNotFoundError):
            continue
        for e in entries:
            rel = e.path.lstrip("/")
            rel = rel[len(prefix):].lstrip("/") if rel.startswith(prefix) else rel
            if e.type != FileEntryType.FILE or (prefix.startswith("cloud/") and rel.startswith(("src.", "broadcast/"))):
                continue
            out = os.path.join(dest, rel)
            os.makedirs(os.path.dirname(out), exist_ok=True)
            with open(out, "wb") as f:
                for chunk in vol.read_file(f"{prefix}/{rel}"):
                    f.write(chunk)
            n += 1
    if n == 0:
        die(f"nothing of run {run} in the cloud")
    rundir = os.path.join(REPO, "e2e/runs", run)
    shutil.copytree(stage, rundir, dirs_exist_ok=True)
    shutil.rmtree(stage)
    print(f"fetched {n} files into e2e/runs/{run}/ (the runner's own logs in work/cloud/)")


def stop() -> None:
    vol = volume()
    for run in runs_on_volume(vol):
        c = vjson(vol, f"cloud/{run}/call.json")
        if c and call_state(c["call_id"])[0] == "active":
            modal.FunctionCall.from_id(c["call_id"]).cancel()
            print(f"cancelled the runner of {run} ({c['call_id']}); `start {run}` resumes it from the chain")
    r = subprocess.run(["modal", "app", "stop", "--yes", APP_NAME], capture_output=True, text=True)
    print(f"app {APP_NAME}: " + ("stopped" if r.returncode == 0 else (r.stdout + r.stderr).strip()[-300:]))
    print(f"kept: Volume {VOLUME_NAME} (the runs), Secret {SECRET_NAME}; neither runs anything")


def main() -> None:
    a = sys.argv[1:]
    if a == ["setup"]:
        setup()
    elif len(a) in (3, 4) and a[0] == "start" and a[2] in ("live", "fork", "selftest") and a[3:] in ([], ["--yes"]):
        start(a[1], a[2], a[3:] == ["--yes"])
    elif len(a) == 2 and a[0] in ("status", "fetch"):
        (status if a[0] == "status" else fetch)(a[1])
    elif a == ["stop"]:
        stop()
    else:
        print(__doc__, file=sys.stderr)
        sys.exit(2)


if __name__ == "__main__":
    main()
