"""ferrata-sepolia: e2e/sepolia-live.sh run in the cloud (Modal), so that the laptop can be off.

Deployed and driven by e2e/sepolia-cloud.sh (setup, start, status, fetch, stop; e2e/README.md,
"Running it in the cloud"). Never `modal profile activate`: the profile is given per command.

  runner        CPU, non-preemptible, one container per start. Rebuilds the repository at the path it
                has on the machine that started it, from what `start` uploaded to the Volume, builds the
                Rust tools and the vault before anything is sent, then runs e2e/sepolia-live.sh itself:
                the identity_p254 step and the other cargo steps run in this container, the GPU proof
                and the prover image through the driver's hooks FERRATA_GPU_CMD and FERRATA_MWRAP
                (e2e/cloud/hook.py). The run directory is mirrored to the Volume every 20 s and when the
                driver stops. Mode "live" signs with the keystore of the Secret; mode "fork" starts
                anvil on a fork of Sepolia and the driver impersonates the deployer (no key is read).
  gpu_prove     one NVIDIA L4 (an L40S if no L4 is free): the succinct checkpoint proof of the staged inputs, with the CUDA prover
                that gpu-prover/modal_app.py::build put on the Volume ferrata-prover (mounted read-only).
  groth16_wrap  RISC Zero's Groth16 prover image, run as a plain function (no Docker).

Volume ferrata-sepolia: runs/<run>/ is the run directory; cloud/<run>/ holds what start uploaded
(src.bundle, src.tar.gz, src.json, call.json) and what the runner writes (status.json, runner.log,
build.log, driver.out, anvil.log, broadcast/ = forge's records of the stages).
"""

from __future__ import annotations

import json
import os
import pathlib
import shlex
import shutil
import signal
import subprocess
import threading
import time

import modal

APP_NAME = "ferrata-sepolia"
VOLUME_NAME = "ferrata-sepolia"
PROVER_VOLUME = "ferrata-prover"  # bin/ferrata-modal-cuda, built by gpu-prover/modal_app.py::build
SECRET_NAME = "ferrata-sepolia-key"
# What the Secret holds. The runner takes them out of its environment before anything else runs.
SECRET_KEYS = ("FERRATA_KEYSTORE_JSON", "FERRATA_KEYSTORE_PASSWORD", "SEPOLIA_RPC_URL", "ETH_FROM")
VOL = "/vol"
UPSTREAM = "/opt/upstream"  # linked to <repository>/../upstream, where checkpoint/ expects the clones
KEY_DIR = "/root/.ferrata-key"
HOOKS = "/root/hooks"
MIRROR_EVERY_S = 20
RUNNER_TIMEOUT_S = 6 * 3600  # the run: ~5 min build, two finality waits (~15 min each), W, the GPU queue (up to 2 h)
CHAIN_ID = "11155111"

# ---- the runner's image: the recipe of the build box (spike/modal-build/box.py), what the driver needs --------
LEZ_COMMIT = "1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21"  # tag v0.4.0-rc1
LB_COMMIT = "d480efc6bbb48fb493c10fe9afefd84c18a04f02"  # the Logos Blockchain revision LEZ pins
# Foundry 1.3.6-nightly of 18 Sep 2025, the build that ran the fork rehearsals and the keystore test on the
# laptop. The 1.3.6 release (v1.3.6) refuses `forge script --unlocked` while ETH_FROM is set, which the
# driver's fork mode needs.
FOUNDRY = "nightly-a4d66e8e348b9763208f715a753c82d0a4ba5b63"
RUNNER_PATH = "/root/.cargo/bin:/root/.risc0/bin:/root/.foundry/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
APT = [
    "curl", "ca-certificates", "build-essential", "pkg-config", "libssl-dev", "git",
    "protobuf-compiler", "libprotobuf-dev", "clang", "libclang-dev", "llvm", "lld", "cmake",
    "ninja-build", "time", "zstd", "rsync", "jq", "unzip", "xz-utils", "file", "libudev-dev",
    "netcat-openbsd", "procps", "psmisc", "iproute2", "bc", "m4", "nasm", "libgmp-dev", "perl",
    "sudo", "wget", "sqlite3", "libsqlite3-dev",
]
runner_image = (
    modal.Image.from_registry("ubuntu:24.04", add_python="3.12")
    .apt_install(*APT)
    .env({
        "HOME": "/root", "RUSTUP_HOME": "/root/.rustup", "CARGO_HOME": "/root/.cargo", "PATH": RUNNER_PATH,
        "CARGO_TERM_COLOR": "never", "FOUNDRY_DIR": "/root/.foundry",
    })
    .run_commands(
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs"
        " | sh -s -- -y --profile default --default-toolchain 1.93.0",
        "rustup toolchain install 1.98.1 --profile default",
    )
    .run_commands(
        "curl -L https://risczero.com/install | bash",
        "rzup install cargo-risczero 3.0.5",
        "rzup install r0vm 3.0.5 || true",
        "rzup show && r0vm --version",
    )
    .run_commands(
        "curl -L https://foundry.paradigm.xyz | bash",
        f"foundryup --install {FOUNDRY}",
        f"forge --version | grep -q {FOUNDRY.removeprefix('nightly-')} && anvil --version"
        " && sha256sum /root/.foundry/bin/forge /root/.foundry/bin/cast /root/.foundry/bin/anvil",
        # vault/foundry.toml is offline: the compiler has to be there already
        "mkdir -p /opt/solc-warm/src && cd /opt/solc-warm"
        " && printf '[profile.default]\\nsolc = \"0.8.26\"\\nsrc = \"src\"\\n' > foundry.toml"
        " && printf '// SPDX-License-Identifier: MIT\\npragma solidity 0.8.26;\\ncontract W {}\\n' > src/W.sol"
        " && forge build",
    )
    .run_commands(
        "git clone --depth 1 --branch v0.4.0-rc1"
        f" https://github.com/logos-blockchain/logos-execution-zone.git {UPSTREAM}/lez-v0.4.0-rc1",
        f"test $(git -C {UPSTREAM}/lez-v0.4.0-rc1 rev-parse HEAD) = {LEZ_COMMIT}",
        f"git init -q {UPSTREAM}/logos-blockchain-d480efc6"
        f" && cd {UPSTREAM}/logos-blockchain-d480efc6"
        " && git remote add origin https://github.com/logos-blockchain/logos-blockchain.git"
        f" && git fetch -q --depth 1 origin {LB_COMMIT} && git checkout -q FETCH_HEAD"
        f" && test $(git rev-parse HEAD) = {LB_COMMIT}",
        "git config --global user.name ferrata-cloud && git config --global user.email ferrata-cloud@localhost"
        " && git config --global --add safe.directory '*' && git config --global advice.detachedHead false",
    )
)

# ---- the GPU image: the recipe of gpu-prover/modal_app.py (`base`), unchanged, without the staged inputs --------
# One NVIDIA L4, as gpu-prover/ measured; an L40S only when no L4 can be had (also Ada, compute capability
# 8.9, so the same sm_89 binary). On 10 Nov 2026 an L4 call of this app waited over an hour for capacity.
GPU = ["L4", "L40S"]
CUDA_BASE = "nvidia/cuda:12.8.0-devel-ubuntu24.04"
RUST_TOOLCHAIN = "1.89.0"
NVCC_APPEND_FLAGS = "--generate-code arch=compute_89,code=sm_89"
GPU_PATH = "/root/.cargo/bin:/usr/local/cuda/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
gpu_image = (
    modal.Image.from_registry(CUDA_BASE, add_python="3.12")
    .apt_install(
        "curl", "ca-certificates", "build-essential", "pkg-config", "libssl-dev", "git",
        "protobuf-compiler", "clang", "libclang-dev",
        "time",
    )
    .run_commands(
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs"
        f" | sh -s -- -y --profile minimal --default-toolchain {RUST_TOOLCHAIN}"
    )
    .env({"PATH": GPU_PATH, "RUSTUP_HOME": "/root/.rustup", "CARGO_HOME": "/root/.cargo",
          "NVCC_APPEND_FLAGS": NVCC_APPEND_FLAGS, "CARGO_TERM_COLOR": "never", "CARGO_INCREMENTAL": "0"})
)

# ---- the prover image of risc0-groth16 3.0.5 (spike/modal-build/box.py, groth16_wrap) ----------------------------
wrap_image = (
    modal.Image.from_registry("risczero/risc0-groth16-prover:v2026-10-20.1@sha256:e8c2068fb34bbc41447255aa980a89cbb7d6e5a80c0d66041264431f2a955e47", add_python="3.12")
    .entrypoint([])
)

app = modal.App(APP_NAME)
volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)
prover_volume = modal.Volume.from_name(PROVER_VOLUME)
key_secret = modal.Secret.from_name(SECRET_NAME)


def _sha256(path: str) -> str:
    import hashlib

    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


# ---- runner -------------------------------------------------------------------------------------------------
class _Run:
    """Logging, status and the mirror of one run, all on the Volume."""

    def __init__(self, run: str, repo: str):
        self.run, self.repo = run, repo
        self.cloud = f"{VOL}/cloud/{run}"
        self.vrun = f"{VOL}/runs/{run}"
        self.rundir = f"{repo}/e2e/runs/{run}"
        os.makedirs(self.cloud, exist_ok=True)
        self.lock = threading.Lock()
        self.log = open(f"{self.cloud}/runner.log", "a", buffering=1)
        self.status: dict = {}
        self.stop_mirror = threading.Event()

    def say(self, msg: str) -> None:
        line = f"[{_utc()}] {msg}"
        print(line, flush=True)
        self.log.write(line + "\n")

    def commit(self) -> None:
        with self.lock:
            self.log.flush()
            volume.commit()

    def set(self, **kw) -> None:
        self.status.update(kw, updated_at=_utc())
        with open(f"{self.cloud}/status.json.part", "w") as f:
            json.dump(self.status, f, indent=1)
        os.replace(f"{self.cloud}/status.json.part", f"{self.cloud}/status.json")
        self.commit()

    def sh(self, cmd: str, log: str | None = None, env: dict | None = None, cwd: str | None = None) -> int:
        """A shell command, its output appended to cloud/<run>/<log> (or runner.log)."""
        target = open(f"{self.cloud}/{log}", "a") if log else self.log
        try:
            return subprocess.run(["bash", "-c", cmd], stdout=target, stderr=subprocess.STDOUT,
                                  env=env, cwd=cwd).returncode
        finally:
            if log:
                target.close()

    def mirror(self) -> None:
        """The run directory and forge's records to the Volume, then a commit."""
        os.makedirs(self.vrun, exist_ok=True)
        subprocess.run(["rsync", "-a", "--delete", f"{self.rundir}/", f"{self.vrun}/"], capture_output=True)
        bc = f"{self.repo}/vault/broadcast"
        if os.path.isdir(bc):
            os.makedirs(f"{self.cloud}/broadcast", exist_ok=True)
            subprocess.run(["rsync", "-a", "--delete", f"{bc}/", f"{self.cloud}/broadcast/"], capture_output=True)
        self.commit()

    def mirror_loop(self) -> None:
        while not self.stop_mirror.wait(MIRROR_EVERY_S):
            try:
                self.mirror()
            except Exception as e:  # the next round tries again
                self.say(f"mirror failed: {e!r}")


def _prepare_tree(r: _Run, commit: str) -> None:
    """The repository at its path: the uploaded commit, then the working files and the ignored inputs."""
    repo = r.repo
    src = json.load(open(f"{r.cloud}/src.json"))
    if src.get("commit") != commit:
        raise RuntimeError(f"cloud/{r.run}/src.json is for commit {src.get('commit')}, the start was for {commit}")
    shutil.rmtree(repo, ignore_errors=True)
    os.makedirs(os.path.dirname(repo), exist_ok=True)
    for cmd in (
        f"git clone -q {shlex.quote(r.cloud + '/src.bundle')} {shlex.quote(repo)}",
        f"git -C {shlex.quote(repo)} checkout -q --detach {shlex.quote(commit)}",
        f"tar -xzf {shlex.quote(r.cloud + '/src.tar.gz')} -C {shlex.quote(repo)}",
    ):
        if r.sh(cmd) != 0:
            raise RuntimeError(f"preparing the repository failed at: {cmd}")
    for p in src.get("deleted", []):  # tracked files deleted in the uploaded working tree
        full = os.path.join(repo, p)
        if os.path.lexists(full):
            os.remove(full)
    head = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if head != commit:
        raise RuntimeError(f"HEAD is {head}, not {commit}")
    spike = os.path.dirname(repo)
    if not os.path.lexists(f"{spike}/upstream"):
        os.symlink(UPSTREAM, f"{spike}/upstream")
    os.makedirs(f"{spike}/tmp/sepolia-cloud", exist_ok=True)
    # The run directory: the Volume's copy is the latest state of the run.
    if not os.path.isfile(f"{r.vrun}/fixture.json"):
        raise RuntimeError(f"runs/{r.run}/fixture.json is not on the Volume: `start` uploads the run directory")
    shutil.rmtree(r.rundir, ignore_errors=True)
    os.makedirs(r.rundir)
    subprocess.run(["rsync", "-a", f"{r.vrun}/", f"{r.rundir}/"], check=True)
    if os.path.isdir(f"{r.cloud}/broadcast"):
        os.makedirs(f"{repo}/vault/broadcast", exist_ok=True)
        subprocess.run(["rsync", "-a", f"{r.cloud}/broadcast/", f"{repo}/vault/broadcast/"], check=True)
    r.say(f"repository at {repo}, commit {commit[:12]}, {len(src.get('files', []))} files uploaded"
          f" ({len(src.get('deleted', []))} deleted), run directory from the Volume")


def _hooks(python: str, repo: str) -> dict:
    os.makedirs(HOOKS, exist_ok=True)
    env = {}
    for name, var in (("gpu", "FERRATA_GPU_CMD"), ("wrap", "FERRATA_MWRAP")):
        path = f"{HOOKS}/{name}"
        with open(path, "w") as f:
            f.write(f"#!/bin/sh\nexec {shlex.quote(python)} {shlex.quote(repo + '/e2e/cloud/hook.py')} {name} \"$@\"\n")
        os.chmod(path, 0o755)
        env[var] = path
    return env


def _start_anvil(r: _Run, upstream: str) -> tuple[subprocess.Popen, str]:
    """anvil on a fork of Sepolia at its latest block, a block every 2 s, every sender impersonable.
    Its output stays in the container (the banner names the endpoint); a copy without it goes to the Volume.
    Tried three times: an anvil that exits at once leaves its redacted log as anvil-attempt-<k>.log."""
    local = "http://127.0.0.1:8545"
    for attempt in (1, 2, 3):
        # five blocks behind the head: at the head a load-balanced endpoint can answer "block not found" from a node
        # that lags, and anvil exits ("failed to create genesis"; seen on 10 Nov 2026)
        head = subprocess.run(["cast", "block-number"], capture_output=True, text=True,
                              env={**os.environ, "ETH_RPC_URL": upstream}).stdout.strip()
        at = ["--fork-block-number", str(int(head) - 5)] if head.isdigit() else []
        out = open("/root/anvil.log", "w")
        proc = subprocess.Popen(["anvil", "--fork-url", upstream, *at, "--auto-impersonate", "--block-time", "2",
                                 "--host", "127.0.0.1", "--port", "8545"], stdout=out, stderr=subprocess.STDOUT,
                                start_new_session=True)
        for _ in range(90):
            time.sleep(2)
            c = subprocess.run(["cast", "chain-id", "--rpc-url", local], capture_output=True, text=True)
            if c.returncode == 0 and c.stdout.strip() == CHAIN_ID:
                block = subprocess.run(["cast", "block-number", "--rpc-url", local], capture_output=True,
                                       text=True).stdout.strip()
                r.say(f"anvil: a fork of Sepolia at block {block}, a block every 2 s, senders impersonated")
                return proc, block
            if proc.poll() is not None:
                break
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            proc.wait(timeout=20)
        out.close()
        _copy_redacted("/root/anvil.log", f"{r.cloud}/anvil-attempt-{attempt}.log", upstream)
        tail = open(f"{r.cloud}/anvil-attempt-{attempt}.log", errors="replace").read().strip().splitlines()[-3:]
        r.say(f"anvil attempt {attempt} failed (exit {proc.returncode}): " + " | ".join(tail))
        time.sleep(15)
    raise RuntimeError("anvil did not fork Sepolia in three attempts: see anvil-attempt-*.log")


def _copy_redacted(src: str, dst: str, secret: str | None) -> None:
    try:
        text = open(src, errors="replace").read()
    except OSError:
        return
    if secret:
        text = text.replace(secret, "<endpoint>")
    with open(dst, "w") as f:
        f.write(text[-200_000:])


@app.function(image=runner_image, cpu=(2.0, 16.0), memory=(4096, 32768), timeout=RUNNER_TIMEOUT_S,
              nonpreemptible=True, retries=0, max_containers=1, single_use_containers=True,
              volumes={VOL: volume}, secrets=[key_secret])
def runner(run: str, repo: str, mode: str, commit: str) -> dict:
    import sys

    secret = {k: os.environ.pop(k, None) for k in SECRET_KEYS}  # out of the environment before anything runs
    if mode not in ("live", "fork", "selftest"):
        raise ValueError(f"mode {mode!r}")
    r = _Run(run, repo)
    call_id = modal.current_function_call_id()
    r.set(run=run, mode=mode, commit=commit, call_id=call_id, task=os.environ.get("MODAL_TASK_ID"),
          phase="preparing", started_at=_utc(), exit_code=None, finished_at=None)
    r.say(f"runner {call_id} for run {run}, mode {mode}")
    driver = anvil = None
    mirror = threading.Thread(target=r.mirror_loop, daemon=True)
    rc = None
    try:
        _prepare_tree(r, commit)
        mirror.start()
        env = dict(os.environ)  # with MODAL_*: the hooks call the other functions of this app from here
        env.update(TMPDIR=f"{os.path.dirname(repo)}/tmp/sepolia-cloud", FERRATA_ENV_FILE="/dev/null",
                   FOUNDRY_DISABLE_NIGHTLY_WARNING="1", XDG_CACHE_HOME=f"{os.path.dirname(repo)}/tmp/xdg-cache")
        for k in ("FERRATA_MRUN", "FERRATA_UNLOCKED", "FERRATA_FORK", "ETH_KEYSTORE", "ETH_PASSWORD",
                  "ETH_KEYSTORE_ACCOUNT", "ETH_RPC_URL", "RISC0_DEV_MODE", "MODAL_PROFILE"):
            env.pop(k, None)
        env.update(_hooks(sys.executable, repo))
        tools = "; ".join((subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip().splitlines() or ["?"])[0]
                          for c in ("cargo --version", "cargo +1.98.1 --version", "r0vm --version",
                                    "forge --version", "anvil --version"))
        r.say(f"tools: {tools}")

        if mode == "selftest":
            checks = {
                "git HEAD": subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip(),
                "programs": subprocess.run(
                    "cd e2e && for p in release checkpoint segment; do [ \"$(sha256sum programs/$p.bin | cut -d' ' -f1)\" = "
                    f"\"$(jq -r .$p.sha256 runs/{shlex.quote(run)}/programs.json)\" ] && echo -n \"$p ok \"; done",
                    shell=True, capture_output=True, text=True, cwd=repo).stdout.strip(),
                "vault/lib": sorted(os.listdir(f"{repo}/vault/lib")) if os.path.isdir(f"{repo}/vault/lib") else None,
                "hook ping": (lambda p: (p.stdout + p.stderr).strip()[-300:])(subprocess.run(
                    [sys.executable, f"{repo}/e2e/cloud/hook.py", "ping"], capture_output=True, text=True, env=env)),
                "secret keys present": sorted(k for k, v in secret.items() if v),
            }
            r.say(f"selftest: {json.dumps(checks)}")
            r.mirror()
            r.set(phase="done", exit_code=0, finished_at=_utc())
            return {"exit_code": 0, "checks": checks}

        # Everything the finalisation proof will need is built before the first transaction.
        r.set(phase="building")
        r.say("building: bridge (ferrata-e2e, release), checkpoint (fidelity e2e, Rust 1.98.1), vault (forge build)")
        t0 = time.time()
        builds = [
            subprocess.Popen(["bash", "-c", c], cwd=f"{repo}/{d}", env=env, stdout=open(f"{r.cloud}/build-{d}.log", "w"),
                             stderr=subprocess.STDOUT)
            for d, c in (("bridge", "cargo build --release -p ferrata-e2e"),
                         ("checkpoint", "cd fidelity && cargo +1.98.1 build --bin e2e"))
        ]
        forge_rc = r.sh("forge build", log="build-vault.log", env=env, cwd=f"{repo}/vault")
        codes = [b.wait() for b in builds] + [forge_rc]
        r.say(f"build exit codes bridge/checkpoint/vault {codes}, {time.time() - t0:.0f} s")
        if any(codes):
            raise RuntimeError("a build failed: see cloud/<run>/build-*.log; nothing was sent")

        if mode == "fork":
            if not secret.get("SEPOLIA_RPC_URL"):
                raise RuntimeError("the Secret has no SEPOLIA_RPC_URL to fork")
            anvil, block = _start_anvil(r, secret["SEPOLIA_RPC_URL"])
            r.set(fork_block=block)
            deployer = json.load(open(f"{r.rundir}/fixture.json"))["deployer"]
            # the stages run on the fork; their gas is still priced by Sepolia's own node (e2e/gasplan.py)
            env.update(SEPOLIA_RPC_URL="http://127.0.0.1:8545", ETH_FROM=deployer, FERRATA_FORK="1",
                       FERRATA_GAS_RPC=secret["SEPOLIA_RPC_URL"])
        else:  # live: the keystore and its password as files of this container only
            missing = [k for k, v in secret.items() if not v]
            if missing:
                raise RuntimeError(f"the Secret {SECRET_NAME} lacks {missing}: run `e2e/sepolia-cloud.sh setup`")
            os.makedirs(KEY_DIR, mode=0o700, exist_ok=True)
            for name, key in (("keystore.json", "FERRATA_KEYSTORE_JSON"), ("password", "FERRATA_KEYSTORE_PASSWORD")):
                fd = os.open(f"{KEY_DIR}/{name}", os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
                with os.fdopen(fd, "w") as f:
                    f.write(secret[key])
            env.update(SEPOLIA_RPC_URL=secret["SEPOLIA_RPC_URL"], ETH_FROM=secret["ETH_FROM"],
                       ETH_KEYSTORE=f"{KEY_DIR}/keystore.json", ETH_PASSWORD=f"{KEY_DIR}/password")
        secret["FERRATA_KEYSTORE_JSON"] = secret["FERRATA_KEYSTORE_PASSWORD"] = None

        r.set(phase="driving")
        r.say(f"driving: e2e/sepolia-live.sh {run}")
        with open(f"{r.cloud}/driver.out", "a") as out:
            driver = subprocess.Popen(["bash", f"{repo}/e2e/sepolia-live.sh", run], cwd=repo, env=env,
                                      stdout=out, stderr=subprocess.STDOUT, start_new_session=True)
            rc = driver.wait()
        r.say(f"the driver exited with {rc}")
        return {"exit_code": rc}
    except BaseException as e:  # recorded, then raised: the call shows as failed
        r.say(f"runner stopped: {e!r}")
        raise
    finally:
        r.stop_mirror.set()
        for proc in (driver, anvil):
            if proc is not None and proc.poll() is None:
                try:
                    os.killpg(proc.pid, signal.SIGTERM)
                    proc.wait(timeout=20)
                except Exception:
                    try:
                        os.killpg(proc.pid, signal.SIGKILL)
                    except OSError:
                        pass
        shutil.rmtree(KEY_DIR, ignore_errors=True)
        if anvil is not None:
            _copy_redacted("/root/anvil.log", f"{r.cloud}/anvil.log", secret.get("SEPOLIA_RPC_URL"))
        try:
            if mode != "selftest" and os.path.isdir(r.rundir):
                r.mirror()
        finally:
            if r.status.get("phase") != "done":
                r.set(phase="done" if rc == 0 else "failed", exit_code=rc, finished_at=_utc())


# ---- the GPU proof ------------------------------------------------------------------------------------------
@app.function(image=gpu_image, gpu=GPU, cpu=4.0, memory=32768, timeout=3600,
              volumes={"/vol": prover_volume.read_only()}, max_containers=1, single_use_containers=True, retries=0)
def gpu_prove(inputs_tar: bytes, name: str, job: str, limit_s: int) -> dict:
    """The succinct proof of fixture <name> of the staged inputs (programs/, fixtures/, MANIFEST.json, as
    `e2e/run.sh stage-gpu` writes them), as gpu-prover/modal_app.py proves one: the inputs' hashes and
    image ids are checked against the manifest, then the CUDA prover runs. Returns its output files."""
    import io
    import tarfile

    t_start = time.time()
    shutil.rmtree("/inputs", ignore_errors=True)
    os.makedirs("/inputs")
    with tarfile.open(fileobj=io.BytesIO(inputs_tar), mode="r:*") as tf:
        tf.extractall("/inputs", filter="data")
    os.makedirs("/root/bin", exist_ok=True)
    prover = "/root/bin/ferrata-modal-cuda"
    shutil.copy2("/vol/bin-31/ferrata-modal-cuda", prover)
    os.chmod(prover, 0o755)
    manifest = json.load(open("/inputs/MANIFEST.json"))
    hashes_ok = all(os.path.exists(f"/inputs/{p}") and _sha256(f"/inputs/{p}") == v["sha256"]
                    for p, v in manifest["files"].items())
    id_lines = subprocess.run([prover, "image-id", "/inputs/programs/checkpoint.bin", "/inputs/programs/segment.bin"],
                              capture_output=True, text=True).stdout.splitlines()
    ids = {pathlib.Path(x["program"]).stem: x["image_id"] for x in (json.loads(l) for l in id_lines if l.startswith("{"))}
    ids_equal = ids == manifest["local_image_ids"]
    meta = json.load(open(f"/inputs/fixtures/{name}.json"))
    out = f"/root/out/{name}"
    os.makedirs(out, exist_ok=True)
    gpu = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.total,driver_version,compute_cap", "--format=csv,noheader"],
                         capture_output=True, text=True).stdout.strip()
    cmd = ["/usr/bin/time", "-v", prover, "prove", name, f"/inputs/programs/{meta['kind']}.bin",
           f"/inputs/fixtures/{name}.input", f"/inputs/fixtures/{name}.json", out]
    env = {**os.environ, "RUST_BACKTRACE": "1"}
    env.pop("RISC0_DEV_MODE", None)
    t0 = time.time()
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
    killer = threading.Timer(limit_s, proc.kill)
    killer.start()
    with open(f"{out}/prover.log", "w") as log:
        for line in proc.stdout:
            log.write(f"{time.time() - t0:9.3f} {line}")
            print(f"[{name} +{time.time() - t0:7.1f}s] {line}", end="", flush=True)
    rc = proc.wait()
    killer.cancel()
    t_end = time.time()
    prove_json = json.load(open(f"{out}/prove.json")) if rc == 0 and os.path.exists(f"{out}/prove.json") else None
    record = {
        "ok": rc == 0 and prove_json is not None and ids_equal and hashes_ok,
        "name": name, "job": job, "exit_code": rc, "timed_out": rc != 0 and (t_end - t0) >= limit_s, "gpu": gpu,
        "image_ids_here": ids, "image_ids_local": manifest["local_image_ids"], "image_ids_equal": ids_equal,
        "input_hashes_equal_manifest": hashes_ok, "source_commit": manifest.get("commit"),
        "prover_wall_s": t_end - t0, "container_s_until_done": t_end - t_start, "prove": prove_json,
        "build_info": json.load(open("/vol/bin-31/ferrata-modal-cuda.json")), "prover_binary_sha256_here": _sha256(prover),
        "task": os.environ.get("MODAL_TASK_ID"), "finished_at_utc": _utc(),
    }
    with open(f"{out}/run.json", "w") as f:
        json.dump(record, f, indent=2)
    files = {}
    for fn in sorted(os.listdir(out)):
        with open(f"{out}/{fn}", "rb") as f:
            files[fn] = f.read()
    return {"ok": record["ok"], "record": record, "files": files}


# ---- the Groth16 step of the wrap ---------------------------------------------------------------------------
# risc0-groth16 3.0.6 (RISC Zero 3.1) runs `docker run -v <dir>:/mnt risczero/risc0-groth16-prover:v2026-10-20.1`, whose
# entrypoint /app/prover.sh reads /mnt/input.json and writes /mnt/proof.json. The same image runs here.
@app.function(image=wrap_image, cpu=(2.0, 16.0), memory=(8192, 32768), timeout=1800, max_containers=2,
              retries=0, single_use_containers=True)
def groth16_wrap(input_json: bytes) -> dict:
    t0 = time.time()
    os.makedirs("/mnt", exist_ok=True)
    for stale in ("/mnt/proof.json", "/tmp/output.wtns"):
        if os.path.lexists(stale):
            os.remove(stale)
    with open("/mnt/input.json", "wb") as f:
        f.write(input_json)
    p = subprocess.run(["bash", "/app/prover.sh"], cwd="/app", capture_output=True)
    proof = b""
    if os.path.exists("/mnt/proof.json"):
        with open("/mnt/proof.json", "rb") as f:
            proof = f.read()
    t = os.times()
    return {"exit_code": p.returncode, "proof_json": proof, "seconds": time.time() - t0,
            "cpu_s": t.children_user + t.children_system, "log": (p.stdout + p.stderr)[-6000:]}


@app.function(image=modal.Image.debian_slim(python_version="3.12"), cpu=0.25, memory=256, timeout=60, retries=0)
def ping() -> dict:
    return {"ok": True, "at": _utc()}
