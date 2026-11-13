"""ferrata-crown: the crown run (e2e/crown/README.md) in the cloud, Modal workspace ws-3.

Deployed and driven by e2e/crown.sh (setup, start, status, fetch, stop; e2e/crown/README.md, "Commands").
Never `modal profile activate`: the profile is given per command.

  runner            CPU 2..16 cores (billed on use), non-preemptible, one container per start: rebuilds the repository
                    at the path it has on the machine that started it from what `start` uploaded, restores the run
                    from the Volume, builds the host tools (ferrata-e2e, realblocks-crown, the vault) before anything is
                    sent, then runs the stage machine e2e/crown/cloud/drive.py of that tree, which does every CPU stage
                    here (both LEZ sessions, the Sepolia stages, the captures, the release proofs, the verify) and calls
                    gpu_prove and groth16_wrap. The run directory is mirrored to the Volume every 20 s and at the end.
                    Mode "live" signs with the keystore of the Secret; mode "fork" (runner_rehearsal) forks Sepolia
                    with anvil and impersonates the deployer: no key is read, nothing is sent to any public network.
  runner_rehearsal  the same, preemptible (cheaper), fork mode only.
  lez_session       one LEZ session (lez/crown/session.sh a|b) on 16 guaranteed cores (up to 32): LEZ's wallet proves
                    the two private transactions of B on this CPU inside a claim's ten-minute window (the box: 274 s and
                    321 s). It alone writes lez/<run>/ on the Volume; it takes the captures as bytes and returns the
                    session's records as bytes, so the runner never waits on a Volume reload. Idempotent: a session
                    whose generation is saved is not run again, its records are returned.
  gpu_prove         one NVIDIA L4 (an L40S if no L4 is free): the succinct checkpoint_b proof of the staged inputs, with
                    the CUDA prover gpu-prover/modal_app.py::build put on the Volume ferrata-prover (read-only).
  groth16_wrap      RISC Zero's Groth16 prover image, run as a plain function (no Docker).

Volume ferrata-crown: inputs/ (the LEZ bundle and the R0BF programs, from `setup`), cloud/<run>/ (what `start`
uploaded: src.bundle, src.tar.gz, src.json, call.json; what the runner writes: status.json, runner.log, build-*.log,
drive.out, forge's broadcast/ and deployments/), runs/<run>/ (the run directory), lez/<run>/ (the LEZ side's
CROWN_LEZ_VOL: bundle, generations, captures, session-a/, session-b/), cache/ (built tools by source hash).
"""

from __future__ import annotations

import hashlib
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

APP_NAME = "ferrata-crown"
VOLUME_NAME = "ferrata-crown"
PROVER_VOLUME = "ferrata-prover"  # bin/ferrata-modal-cuda, built by gpu-prover/modal_app.py::build
SECRET_NAME = "ferrata-sepolia-key"  # the owner's (e2e/sepolia-cloud.sh setup): the same throwaway deployer
SECRET_KEYS = ("FERRATA_KEYSTORE_JSON", "FERRATA_KEYSTORE_PASSWORD", "SEPOLIA_RPC_URL", "ETH_FROM")
PUBLIC_SEPOLIA = "https://ethereum-sepolia-rpc.publicnode.com"  # fork mode without the Secret's endpoint
VOL = "/vol"
UPSTREAM = "/opt/upstream"  # linked to <repository>/../upstream
KEY_DIR = "/root/.ferrata-key"
MIRROR_EVERY_S = 20
RUNNER_TIMEOUT_S = 8 * 3600  # live: build ~10 min, session A, two finality waits (~15 min each), session B, the GPU
#                              queue (up to 2 h), W = 1 h, the releases; a stop or a timeout resumes with `start`

# ---- the runner's image: e2e/cloud/app.py's (layer for layer, so Modal reuses its cache), then what LEZ needs ----
LEZ_COMMIT = "1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21"  # tag v0.4.0-rc1
LB_COMMIT = "d480efc6bbb48fb493c10fe9afefd84c18a04f02"
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
    # The LEZ bundle's sequencer links libpcsclite (lez/crown/build.sh's runtime list); the rest is above.
    .apt_install("libpcsclite1")
)

# ---- the GPU image: gpu-prover/modal_app.py's `base` (as e2e/cloud/app.py) ------------------------------------
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

wrap_image = (
    modal.Image.from_registry("risczero/risc0-groth16-prover:v2025-04-03.1", add_python="3.12")
    .entrypoint([])
)

app = modal.App(APP_NAME)
volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)
prover_volume = modal.Volume.from_name(PROVER_VOLUME)
key_secret = modal.Secret.from_name(SECRET_NAME)


def _sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


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
        target = open(f"{self.cloud}/{log}", "a") if log else self.log
        try:
            return subprocess.run(["bash", "-c", cmd], stdout=target, stderr=subprocess.STDOUT, env=env, cwd=cwd).returncode
        finally:
            if log:
                target.close()

    def mirror(self) -> None:
        """The run directory, forge's records and DeployVault's record to the Volume, then a commit."""
        if os.path.isdir(self.rundir):
            os.makedirs(self.vrun, exist_ok=True)
            subprocess.run(["rsync", "-a", "--delete", f"{self.rundir}/", f"{self.vrun}/"], capture_output=True)
        for src, dst in ((f"{self.repo}/vault/broadcast", f"{self.cloud}/broadcast"),
                         (f"{self.repo}/vault/deployments", f"{self.cloud}/deployments")):
            if os.path.isdir(src):
                os.makedirs(dst, exist_ok=True)
                subprocess.run(["rsync", "-a", f"{src}/", f"{dst}/"], capture_output=True)
        self.commit()

    def mirror_loop(self) -> None:
        while not self.stop_mirror.wait(MIRROR_EVERY_S):
            try:
                self.mirror()
            except Exception as e:  # the next round tries again
                self.say(f"mirror failed: {e!r}")


def _prepare_tree(r: _Run, commit: str) -> None:
    """The repository at its path: the uploaded commit, the working files, the ignored inputs; the run from the Volume."""
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
    for p in src.get("deleted", []):
        full = os.path.join(repo, p)
        if os.path.lexists(full):
            os.remove(full)
    head = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if head != commit:
        raise RuntimeError(f"HEAD is {head}, not {commit}")
    spike = os.path.dirname(repo)
    if not os.path.lexists(f"{spike}/upstream"):
        os.symlink(UPSTREAM, f"{spike}/upstream")
    os.makedirs(f"{spike}/tmp/crown-cloud", exist_ok=True)
    if not os.path.isfile(f"{r.vrun}/run.json"):
        raise RuntimeError(f"runs/{r.run}/run.json is not on the Volume: `start` uploads it")
    shutil.rmtree(r.rundir, ignore_errors=True)
    os.makedirs(r.rundir)
    subprocess.run(["rsync", "-a", f"{r.vrun}/", f"{r.rundir}/"], check=True)
    for src_dir, dst in ((f"{r.cloud}/broadcast", f"{repo}/vault/broadcast"),
                         (f"{r.cloud}/deployments", f"{repo}/vault/deployments")):
        if os.path.isdir(src_dir):
            os.makedirs(dst, exist_ok=True)
            subprocess.run(["rsync", "-a", f"{src_dir}/", f"{dst}/"], check=True)
    r.say(f"repository at {repo}, commit {commit[:12]}, {len(src.get('files', []))} files uploaded"
          f" ({len(src.get('deleted', []))} deleted); run directory, forge's records and the deploy record from the Volume")


def _source_key(repo: str) -> str:
    """What the host tools are built from: every file of bridge/ and checkpoint/ but their target directories."""
    h = hashlib.sha256()
    for top in ("bridge", "checkpoint"):
        for root, dirs, files in os.walk(f"{repo}/{top}"):
            dirs[:] = sorted(d for d in dirs if d != "target")
            for fn in sorted(files):
                p = os.path.join(root, fn)
                h.update(os.path.relpath(p, repo).encode() + b"\0" + _sha256(p).encode() + b"\n")
    return h.hexdigest()[:24]


def _build(r: _Run, env: dict) -> dict:
    """ferrata-e2e (bridge/e2e), realblocks-crown (checkpoint/realblocks) and the vault, before anything is sent.
    The two binaries are kept on the Volume under their source hash: a resumed run does not build them again."""
    key = _source_key(r.repo)
    cache = f"{VOL}/cache/tools-{key}"
    tools = {"e2e": f"{cache}/ferrata-e2e", "realblocks": f"{cache}/realblocks-crown"}
    t0 = time.time()
    jobs = []
    if not all(os.path.isfile(p) for p in tools.values()):
        r.say(f"building ferrata-e2e and realblocks-crown (sources {key})")
        jobs = [subprocess.Popen(["bash", "-c", c], cwd=f"{r.repo}/{d}", env=env, stdout=open(f"{r.cloud}/build-{n}.log", "w"),
                                 stderr=subprocess.STDOUT)
                for n, d, c in (("e2e", "bridge", "cargo build --release -p ferrata-e2e"),
                                ("realblocks", "checkpoint", "cargo build --release -p realblocks --bin realblocks-crown"))]
    else:
        r.say(f"ferrata-e2e and realblocks-crown of sources {key} from the Volume")
    forge_rc = r.sh("forge build", log="build-vault.log", env=env, cwd=f"{r.repo}/vault")
    codes = [j.wait() for j in jobs] + [forge_rc]
    r.say(f"build exit codes {codes}, {time.time() - t0:.0f} s")
    if any(codes):
        raise RuntimeError("a build failed: see cloud/<run>/build-*.log; nothing was sent")
    if jobs:
        os.makedirs(cache, exist_ok=True)
        for name, built in (("e2e", f"{r.repo}/bridge/target/release/ferrata-e2e"),
                            ("realblocks", f"{r.repo}/checkpoint/target/release/realblocks-crown")):
            shutil.copy2(built, tools[name] + ".part")
            os.replace(tools[name] + ".part", tools[name])
        r.commit()
    return tools


def _runner(run: str, repo: str, mode: str, commit: str) -> dict:
    import sys

    secret = {k: os.environ.pop(k, None) for k in SECRET_KEYS}  # out of the environment before anything runs
    if mode not in ("live", "fork", "selftest"):
        raise ValueError(f"mode {mode!r}")
    r = _Run(run, repo)
    call_id = modal.current_function_call_id()
    r.set(run=run, mode=mode, commit=commit, call_id=call_id, task=os.environ.get("MODAL_TASK_ID"),
          phase="preparing", started_at=_utc(), exit_code=None, finished_at=None)
    r.say(f"runner {call_id} for run {run}, mode {mode}")
    drive = None
    mirror = threading.Thread(target=r.mirror_loop, daemon=True)
    rc = None
    try:
        _prepare_tree(r, commit)
        mirror.start()
        env = dict(os.environ)  # with MODAL_*: drive.py calls gpu_prove and groth16_wrap of this app from here
        spike = os.path.dirname(repo)
        env.update(TMPDIR=f"{spike}/tmp/crown-cloud", FOUNDRY_DISABLE_NIGHTLY_WARNING="1",
                   XDG_CACHE_HOME=f"{spike}/tmp/xdg-cache", CROWN_APP=APP_NAME, CROWN_VOL=VOL,
                   CROWN_INPUTS=f"{VOL}/inputs", CROWN_LEZ_VOL=f"{VOL}/lez/{run}", CROWN_CLOUD=r.cloud)
        for k in ("FERRATA_UNLOCKED", "ETH_KEYSTORE", "ETH_PASSWORD", "ETH_RPC_URL", "RISC0_DEV_MODE", "MODAL_PROFILE",
                  "SEPOLIA_RPC_URL", "ETH_FROM"):
            env.pop(k, None)
        tools = "; ".join((subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip().splitlines() or ["?"])[0]
                          for c in ("cargo --version", "r0vm --version", "forge --version", "anvil --version", "nproc"))
        r.say(f"tools: {tools}")
        if mode == "selftest":
            checks = {
                "git HEAD": subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip(),
                "inputs": sorted(os.listdir(f"{VOL}/inputs")) if os.path.isdir(f"{VOL}/inputs") else None,
                "vault/lib": sorted(os.listdir(f"{repo}/vault/lib")) if os.path.isdir(f"{repo}/vault/lib") else None,
                "ping": modal.Function.from_name(APP_NAME, "ping").remote(),
                "secret keys present": sorted(k for k, v in secret.items() if v),
                "libpcsclite": subprocess.run("ldconfig -p | grep -c libpcsclite", shell=True, capture_output=True,
                                              text=True).stdout.strip(),
            }
            r.say(f"selftest: {json.dumps(checks)}")
            r.set(phase="done", exit_code=0, finished_at=_utc())
            return {"exit_code": 0, "checks": checks}

        r.set(phase="building")
        built = _build(r, env)
        env.update(CROWN_E2E=built["e2e"], CROWN_REALBLOCKS=built["realblocks"])

        if mode == "fork":
            env.update(CROWN_FORK_UPSTREAM=secret.get("SEPOLIA_RPC_URL") or PUBLIC_SEPOLIA, FERRATA_UNLOCKED="1")
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
        r.say(f"driving: e2e/crown/cloud/drive.py {run} {mode}")
        with open(f"{r.cloud}/drive.out", "a") as out:
            drive = subprocess.Popen([sys.executable, f"{repo}/e2e/crown/cloud/drive.py", run, mode], cwd=repo, env=env,
                                     stdout=out, stderr=subprocess.STDOUT, start_new_session=True)
            rc = drive.wait()
        r.say(f"drive.py exited with {rc}")
        return {"exit_code": rc}
    except BaseException as e:
        r.say(f"runner stopped: {e!r}")
        raise
    finally:
        r.stop_mirror.set()
        if drive is not None and drive.poll() is None:
            try:
                os.killpg(drive.pid, signal.SIGTERM)
                drive.wait(timeout=30)
            except Exception:
                try:
                    os.killpg(drive.pid, signal.SIGKILL)
                except OSError:
                    pass
        shutil.rmtree(KEY_DIR, ignore_errors=True)
        try:
            if mode != "selftest":
                r.mirror()
        finally:
            if r.status.get("phase") != "done":
                r.set(phase="done" if rc == 0 else "failed", exit_code=rc, finished_at=_utc())


_RUNNER = dict(image=runner_image, cpu=(2.0, 16.0), memory=(4096, 65536), timeout=RUNNER_TIMEOUT_S, retries=0,
               max_containers=1, single_use_containers=True, volumes={VOL: volume}, secrets=[key_secret])


@app.function(nonpreemptible=True, **_RUNNER)
def runner(run: str, repo: str, mode: str, commit: str) -> dict:
    return _runner(run, repo, mode, commit)


@app.function(**_RUNNER)
def runner_rehearsal(run: str, repo: str, mode: str, commit: str) -> dict:
    if mode == "live":
        raise ValueError("runner_rehearsal never runs a live run")
    return _runner(run, repo, mode, commit)


# ---- the LEZ sessions ---------------------------------------------------------------------------------------
LEZ_ENV = ("CROWN_VAULT", "CROWN_ETH_FEES_AT", "CROWN_RECIPIENT", "CROWN_PRIVATE", "CROWN_R0VM", "CROWN_DEADLINE_MIN",
           "CROWN_CLAIM_TIME_OFFSET_S", "CROWN_MAX_STARTS")


@app.function(image=runner_image, cpu=(16.0, 32.0), memory=(16384, 65536), timeout=3 * 3600, retries=0,
              max_containers=1, single_use_containers=True, volumes={VOL: volume})
def lez_session(run: str, step: str, chain: str, bundle_sha256: str, script: bytes, env: dict,
                inputs_tar: bytes | None = None, may_start: bool = True) -> dict:
    """lez/crown/session.sh <step> (the run's own copy, `script`) with CROWN_LEZ_VOL = lez/<run> on the Volume.
    Session A runs only on an empty zone, B only on generation 1; a session whose generation exists returns its saved
    records (may_start=False: never run, only return). Returns {ok, ran, rc, generation, records (tar of session-<step>/),
    tail}."""
    import io
    import tarfile

    lez = f"{VOL}/lez/{run}"
    want = {"a": 1, "b": 2}[step]
    os.makedirs(f"{lez}/state", exist_ok=True)
    gen = lambda: int(open(f"{lez}/state/current").read().strip()) if os.path.isfile(f"{lez}/state/current") else 0
    outlog = f"{lez}/session-{step}.out"
    ran, rc = False, None
    stop = threading.Event()

    def committer():
        while not stop.wait(60):
            try:
                volume.commit()
            except Exception:
                pass

    if gen() < want:
        if not may_start or gen() != want - 1:
            return {"ok": False, "ran": False, "rc": None, "generation": gen(),
                    "tail": f"session {step} needs generation {want - 1} and may_start; the zone is at {gen()}"}
        if not os.path.isfile(f"{lez}/bundle.tar.zst"):
            shutil.copy2(f"{VOL}/inputs/bundle.tar.zst", f"{lez}/bundle.tar.zst.part")
            os.replace(f"{lez}/bundle.tar.zst.part", f"{lez}/bundle.tar.zst")
        if _sha256(f"{lez}/bundle.tar.zst") != bundle_sha256:
            return {"ok": False, "ran": False, "rc": None, "generation": gen(),
                    "tail": f"lez/{run}/bundle.tar.zst is not the run's bundle {bundle_sha256}"}
        if inputs_tar:
            with tarfile.open(fileobj=io.BytesIO(inputs_tar), mode="r:*") as tf:
                names = {n.split("/")[0] for n in tf.getnames()}
                for n in names:
                    shutil.rmtree(f"{lez}/inputs/{n}", ignore_errors=True)
                tf.extractall(f"{lez}/inputs", filter="data")
        with open("/root/session.sh", "wb") as f:
            f.write(script)
        e = {k: v for k, v in os.environ.items() if not k.startswith("CROWN_")}
        e.update({k: str(v) for k, v in env.items() if k in LEZ_ENV and v is not None})
        e.update(CROWN_LEZ_VOL=lez, CROWN_LEZ_WORK="/root/crown-lez", CROWN_CHAIN=chain)
        e.pop("RISC0_DEV_MODE", None)
        threading.Thread(target=committer, daemon=True).start()
        with open(outlog, "a") as out:
            out.write(f"[{_utc()}] session {step} (chain {chain}), {os.cpu_count()} CPUs visible\n")
            out.flush()
            rc = subprocess.run(["bash", "/root/session.sh", step], stdout=out, stderr=subprocess.STDOUT, env=e).returncode
            out.write(f"[{_utc()}] session {step}: exit {rc}, generation {gen()}\n")
        stop.set()
        ran = True
        volume.commit()
    d = f"{lez}/session-{step}"
    need = ["session-a.json", "channel_signing_key", "deployment-settings.yaml"] if step == "a" else \
        ["session-b.json", "burn-notes.json", "node/info.json"]
    ok = gen() >= want and all(os.path.isfile(f"{d}/{n}") for n in need)
    buf = io.BytesIO()
    if os.path.isdir(d):
        with tarfile.open(fileobj=buf, mode="w:gz") as tf:
            tf.add(d, arcname=f"session-{step}")
    tail = open(outlog, errors="replace").read().splitlines()[-25:] if os.path.isfile(outlog) else []
    return {"ok": ok, "ran": ran, "rc": rc, "generation": gen(), "records": buf.getvalue(), "tail": "\n".join(tail)}


# ---- the updater session: ferrata-updater (lez/updater/sequencer) on a zone of session A ------------------
UPDATER_ENV = ("CROWN_UPDATER_MIN", "CROWN_UPDATER_RESTART_MIN", "CROWN_UPDATER_PROBE_S", "CROWN_UPDATER_POLL_S",
               "CROWN_SAMPLE", "CROWN_MAX_STARTS")
# The final programs (docs/image-ids.md): session A's deployment must have computed these, or session U does not run.
FINAL_IMAGE_IDS = {"attestation": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df",
                   "bridge": "0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd"}
control = modal.Dict.from_name("ferrata-crown-control", create_if_missing=True)


@app.function(image=runner_image, cpu=(2.0, 8.0), memory=(8192, 32768), timeout=14 * 3600, retries=0,
              max_containers=1, single_use_containers=True, volumes={VOL: volume})
def updater_session(run: str, script: bytes, env: dict) -> dict:
    """lez/crown/session.sh a (a new zone: the final programs deployed, the Sepolia instance from the bundle's fixture;
    skipped when the zone has a generation), then session.sh u (ferrata-updater from lez/<run>/inputs/updater/ on that
    zone, for CROWN_UPDATER_MIN minutes), one container, CROWN_LEZ_VOL = lez/<run>. Fork-safe by construction: nothing
    here holds a key of any public network; the zone is local to the container, the beacon APIs are only read.
    `control["stop/<run>"] = reason` ends session U early (the next check, within 30 s). Returns {ok, steps, tail}."""
    lez = f"{VOL}/lez/{run}"
    os.makedirs(f"{lez}/state", exist_ok=True)
    gen = lambda: int(open(f"{lez}/state/current").read().strip()) if os.path.isfile(f"{lez}/state/current") else 0
    outlog = f"{lez}/session-u.out"
    stop = threading.Event()

    def committer():
        while not stop.wait(60):
            try:
                volume.commit()
            except Exception:
                pass

    def stop_watch():
        while not stop.wait(30):
            try:
                why = control.get(f"stop/{run}")
            except Exception:
                why = None
            if why:
                with open("/root/updater-stop", "w") as f:
                    f.write(str(why))

    def say(msg: str) -> None:
        with open(outlog, "a") as out:
            out.write(f"[{_utc()}] {msg}\n")

    if not os.path.isfile(f"{lez}/bundle.tar.zst"):
        shutil.copy2(f"{VOL}/inputs/bundle.tar.zst", f"{lez}/bundle.tar.zst.part")
        os.replace(f"{lez}/bundle.tar.zst.part", f"{lez}/bundle.tar.zst")
    bundle_sha = _sha256(f"{lez}/bundle.tar.zst")
    if not os.path.isfile(f"{lez}/inputs/updater/ferrata-updater"):
        return {"ok": False, "steps": {}, "tail": f"no lez/{run}/inputs/updater/ferrata-updater (ctl.py updater uploads it)"}
    with open("/root/session.sh", "wb") as f:
        f.write(script)
    e = {k: v for k, v in os.environ.items() if not k.startswith("CROWN_")}
    e.update({k: str(v) for k, v in env.items() if k in UPDATER_ENV and v is not None})
    e.update(CROWN_LEZ_VOL=lez, CROWN_LEZ_WORK="/root/crown-lez", CROWN_CHAIN="sepolia")
    e.pop("RISC0_DEV_MODE", None)
    threading.Thread(target=committer, daemon=True).start()
    threading.Thread(target=stop_watch, daemon=True).start()
    steps = {}
    say(f"updater session {run}: bundle sha256 {bundle_sha}, {os.cpu_count()} CPUs visible, generation {gen()}")
    for step in ("a", "u"):
        if step == "a" and gen() >= 1:
            say("session a: the zone has a generation; skipped")
            continue
        if step == "u":
            ident = json.load(open(f"{lez}/session-a/session-a.json")) if os.path.isfile(f"{lez}/session-a/session-a.json") else {}
            got = {k: (ident.get(k) or {}).get("imageId") for k in FINAL_IMAGE_IDS}
            if got != FINAL_IMAGE_IDS:
                say(f"session u not run: session A's programs {got} are not the final ones {FINAL_IMAGE_IDS}")
                steps["u"] = None
                break
        with open(outlog, "a") as out:
            out.write(f"[{_utc()}] session {step}\n")
            out.flush()
            rc = subprocess.run(["bash", "/root/session.sh", step], stdout=out, stderr=subprocess.STDOUT, env=e).returncode
            out.write(f"[{_utc()}] session {step}: exit {rc}, generation {gen()}\n")
        steps[step] = rc
        volume.commit()
        if rc != 0:
            break
    stop.set()
    volume.commit()
    tail = open(outlog, errors="replace").read().splitlines()[-30:]
    return {"ok": steps.get("u") == 0, "steps": steps, "generation": gen(), "bundle_sha256": bundle_sha,
            "tail": "\n".join(tail)}


# ---- the GPU proof ------------------------------------------------------------------------------------------
@app.function(image=gpu_image, gpu=GPU, cpu=4.0, memory=32768, timeout=3600,
              volumes={"/vol": prover_volume.read_only()}, max_containers=1, single_use_containers=True, retries=0)
def gpu_prove(inputs_tar: bytes, name: str, job: str, limit_s: int) -> dict:
    """The succinct proof of fixture <name> of the staged inputs (programs/, fixtures/, MANIFEST.json; e2e/crown/
    stage-gpu.sh's layout), as gpu-prover/modal_app.py proves one: the inputs' hashes and every image id the manifest
    names (checkpoint, segment and the fixture's own program) are recomputed here, then the CUDA prover runs."""
    import io
    import tarfile

    t_start = time.time()
    shutil.rmtree("/inputs", ignore_errors=True)
    os.makedirs("/inputs")
    with tarfile.open(fileobj=io.BytesIO(inputs_tar), mode="r:*") as tf:
        tf.extractall("/inputs", filter="data")
    os.makedirs("/root/bin", exist_ok=True)
    prover = "/root/bin/ferrata-modal-cuda"
    shutil.copy2("/vol/bin/ferrata-modal-cuda", prover)
    os.chmod(prover, 0o755)
    manifest = json.load(open("/inputs/MANIFEST.json"))
    hashes_ok = all(os.path.exists(f"/inputs/{p}") and _sha256(f"/inputs/{p}") == v["sha256"]
                    for p, v in manifest["files"].items())
    meta = json.load(open(f"/inputs/fixtures/{name}.json"))
    program = f"/inputs/programs/{meta['kind']}.bin"
    programs = sorted({"/inputs/programs/checkpoint.bin", "/inputs/programs/segment.bin", program})
    id_lines = subprocess.run([prover, "image-id", *programs], capture_output=True, text=True).stdout.splitlines()
    ids = {pathlib.Path(x["program"]).stem: x["image_id"] for x in (json.loads(l) for l in id_lines if l.startswith("{"))}
    ids_equal = ids == manifest["local_image_ids"]
    out = f"/root/out/{name}"
    os.makedirs(out, exist_ok=True)
    gpu = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.total,driver_version,compute_cap", "--format=csv,noheader"],
                         capture_output=True, text=True).stdout.strip()
    cmd = ["/usr/bin/time", "-v", prover, "prove", name, program, f"/inputs/fixtures/{name}.input",
           f"/inputs/fixtures/{name}.json", out]
    env = {**os.environ, "RUST_BACKTRACE": "1"}
    env.pop("RISC0_DEV_MODE", None)
    rc, t0 = None, time.time()
    if hashes_ok and ids_equal:
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
        "name": name, "job": job, "exit_code": rc, "timed_out": rc not in (0, None) and (t_end - t0) >= limit_s, "gpu": gpu,
        "image_ids_here": ids, "image_ids_local": manifest["local_image_ids"], "image_ids_equal": ids_equal,
        "input_hashes_equal_manifest": hashes_ok, "source_commit": manifest.get("commit"),
        "prover_wall_s": t_end - t0, "container_s_until_done": t_end - t_start, "prove": prove_json,
        "build_info": json.load(open("/vol/bin/ferrata-modal-cuda.json")), "prover_binary_sha256_here": _sha256(prover),
        "task": os.environ.get("MODAL_TASK_ID"), "finished_at_utc": _utc(),
    }
    with open(f"{out}/run.json", "w") as f:
        json.dump(record, f, indent=2)
    files = {}
    for fn in sorted(os.listdir(out)):
        with open(f"{out}/{fn}", "rb") as f:
            files[fn] = f.read()
    return {"ok": record["ok"], "record": record, "files": files}


# ---- the Groth16 step of the wrap (risc0-groth16 3.0.5's prover image, /app/prover.sh: /mnt/input.json -> proof.json)
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
