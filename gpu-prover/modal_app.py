"""ferrata-prover: RISC Zero 3.0.5 proofs on one NVIDIA L4 (Modal), in one Modal workspace.

Always run with the workspace profile set per command (never `modal profile activate`), detached so a
dropped client connection does not stop a build or a proof halfway:

    MODAL_PROFILE=<profile> modal run --detach modal_app.py::build             # CPU: binaries -> Volume bin/
    MODAL_PROFILE=<profile> modal run --detach modal_app.py --fixtures cp-empty-1
    MODAL_PROFILE=<profile> modal run --detach modal_app.py::lez --run <name>  # LEZ light-client update
    MODAL_PROFILE=<profile> modal run --detach modal_app.py::lez --job lez-attest --run <name>  # LEZ private attestation

The guests are never rebuilt here. `inputs/programs/*.bin` are the guest program binaries of the local
ferrata-checkpoint build (scripts/stage-inputs.sh); `inputs-lez/prove_private` and
`inputs-lez-attest/prove_attest` are LEZ host binaries with their guests embedded (scripts/stage-lez.sh,
and the repository's lez/scripts/stage-prove-attest.sh). Each run checks the image ids. See README.md.
"""

from __future__ import annotations

import json
import os
import pathlib
import time

import modal

APP_NAME = "ferrata-prover"
VOLUME_NAME = "ferrata-prover"
GPU = "L4"  # NVIDIA L4: Ada Lovelace, compute capability 8.9, 24 GB
# risc0 v3.0.5's kernel build scripts use `-arch=native` unless NVCC_APPEND_FLAGS/NVCC_PREPEND_FLAGS
# is set (risc0/build_kernel/src/lib.rs, circuit/rv32im-sys/build.rs); the build container has no
# GPU, so the architecture is named, as risc0's CI does for its Ada runner (-arch=sm_89).
NVCC_APPEND_FLAGS = "--generate-code arch=compute_89,code=sm_89"
# The CUDA toolkit image RISC Zero's own GPU prover agent (bento) is built on at the v3.0.5 tag.
CUDA_BASE = "nvidia/cuda:12.8.0-devel-ubuntu24.04"
RUST_TOOLCHAIN = "1.89.0"  # risc0 v3.0.5 rust-toolchain.toml: channel = "1.89"
R0VM_VERSION = "=3.0.5"  # crates.io risc0-r0vm 3.0.5; its packaged Cargo.lock = the v3.0.5 tag's versions
CPU_CORES = 4.0  # physical cores (8 vCPUs) next to the GPU, like a one-L4 cloud VM
MEMORY_MIB = 32768
BUILD_CPU_CORES = 16.0
BUILD_MEMORY_MIB = 65536  # nvcc on the rv32im kernels, many at once
HARD_TIMEOUT_S = 2 * 3600  # no function runs longer than 2 h
LEZ_IMAGE_ID = "cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78"
# The LEZ jobs: a prebuilt host binary that proves through r0vm (RISC0_SERVER_PATH). `dir` is where its
# staged inputs are attached in the GPU container; `tx` and `log` are the files it writes; `lines` are
# the starts of the log lines copied into run.json.
LEZ_JOBS = {
    "lez-update": {  # the light-client update, lez RESULTS.md §3
        "dir": "/lez", "binary": "prove_private", "tx": "stage2-private-tx.borsh", "log": "prove-private.log",
        "lines": ("program image", "private transition", "REAL PROOF", "DRY RUN", "settled", "wrote", "store", "record"),
    },
    "lez-attest": {  # one private attestation, lez RESULTS.md §5.5
        "dir": "/lez-attest", "binary": "prove_attest", "tx": "stage4-private-attest-tx.borsh", "log": "prove-attest.log",
        "lines": ("consumer image", "record:", "| AttestPrivately", "| the record's reply", "REAL PROOF", "DRY RUN",
                  "settled", "wrote"),
    },
}

HERE = pathlib.Path(__file__).resolve().parent
PATH = "/root/.cargo/bin:/usr/local/cuda/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
BIN = "/vol/bin"  # built binaries on the Volume

base = (
    modal.Image.from_registry(CUDA_BASE, add_python="3.12")
    .apt_install(
        "curl", "ca-certificates", "build-essential", "pkg-config", "libssl-dev", "git",
        # circom-witnesscalc (pulled in by risc0-groth16's cuda feature) runs prost-build and bindgen
        "protobuf-compiler", "clang", "libclang-dev",
        "time",  # GNU time: peak resident memory
    )
    .run_commands(
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs"
        f" | sh -s -- -y --profile minimal --default-toolchain {RUST_TOOLCHAIN}"
    )
    .env({"PATH": PATH, "RUSTUP_HOME": "/root/.rustup", "CARGO_HOME": "/root/.cargo",
          "NVCC_APPEND_FLAGS": NVCC_APPEND_FLAGS, "CARGO_TERM_COLOR": "never", "CARGO_INCREMENTAL": "0"})
)
# Local files are attached when a container starts (not image layers).
build_image = base.add_local_dir(HERE / "prover", "/src/prover", ignore=["target", "**/target"])
# FERRATA_INPUTS: another staged inputs directory (programs/, fixtures/, MANIFEST.json), for example
# the one `e2e/run.sh stage-gpu <run>` writes; the default is the one scripts/stage-inputs.sh fills.
INPUTS = pathlib.Path(os.environ.get("FERRATA_INPUTS") or HERE / "inputs")
gpu_image = (
    base.add_local_dir(INPUTS, "/inputs")
    .add_local_dir(HERE / "inputs-lez", "/lez")
    .add_local_dir(HERE / "inputs-lez-attest", "/lez-attest")
)

app = modal.App(APP_NAME)
volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)


def _sh(*cmd: str) -> str:
    import subprocess

    r = subprocess.run(cmd, capture_output=True, text=True)
    return (r.stdout + r.stderr).strip()


def _sha256(path: str) -> str:
    import hashlib

    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _local_copy(src: str, dst: str) -> str:
    """Executables are copied off the Volume before they run."""
    import os
    import shutil

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    os.chmod(dst, 0o755)
    return dst


def _tree_sha256(root: str) -> str:
    import hashlib
    import os

    h = hashlib.sha256()
    for dirpath, dirnames, filenames in sorted(os.walk(root)):
        dirnames.sort()
        for name in sorted(filenames):
            path = os.path.join(dirpath, name)
            h.update(os.path.relpath(path, root).encode() + b"\0" + _sha256(path).encode())
    return h.hexdigest()


@app.function(image=build_image, cpu=BUILD_CPU_CORES, memory=BUILD_MEMORY_MIB, timeout=HARD_TIMEOUT_S,
              volumes={"/vol": volume}, max_containers=1, single_use_containers=True, retries=0)
def build(what: str = "prover") -> dict:
    """CPU only. `prover`: ferrata-modal with CUDA (the GPU prover) and without (image ids and verification;
    it also runs on the laptop, which has no NVIDIA driver). `r0vm`: risc0-r0vm 3.0.5 with its `cuda`
    feature, for hosts that prove through r0vm. Each binary is committed to bin/ as soon as it is built,
    with <binary>.json beside it, and a step whose binaries exist for the same source is skipped, so a
    preempted container (Modal restarts it with the same input) repeats only the unfinished step."""
    import os
    import shutil
    import subprocess

    os.makedirs(BIN, exist_ok=True)
    work = "/root/build"
    os.makedirs(work, exist_ok=True)
    env = {**os.environ, "CARGO_TARGET_DIR": f"{work}/target"}
    common = {
        "cuda_base_image": CUDA_BASE, "nvcc": _sh("nvcc", "--version").splitlines()[-2:], "rustc": _sh("rustc", "--version"),
        "cargo": _sh("cargo", "--version"), "gcc": _sh("gcc", "--version").splitlines()[0], "nvcc_append_flags": NVCC_APPEND_FLAGS,
        "build_container": {"cpu_cores": BUILD_CPU_CORES, "memory_mib": BUILD_MEMORY_MIB},
    }
    if what == "prover":
        key = _tree_sha256("/src/prover")
        steps = [("ferrata-modal-cuda", ["build", "--release", "--locked", "--features", "cuda"], "target/release/ferrata-modal"),
                 ("ferrata-modal-verify", ["build", "--release", "--locked"], "target/release/ferrata-modal")]
        cwd = f"{work}/prover"
        if not os.path.exists(cwd):
            shutil.copytree("/src/prover", cwd)
    elif what == "r0vm":
        key = f"risc0-r0vm {R0VM_VERSION} --locked --features cuda"
        steps = [("r0vm-cuda", ["install", "risc0-r0vm", "--version", R0VM_VERSION, "--locked", "--features", "cuda",
                                "--root", f"{work}/r0vm"], "r0vm/bin/r0vm")]
        cwd = work
    else:
        raise ValueError(f"unknown build step {what!r}")

    done = {}
    for name, args, product in steps:
        meta_path = f"{BIN}/{name}.json"
        if os.path.exists(meta_path) and os.path.exists(f"{BIN}/{name}") and json.load(open(meta_path)).get("source_key") == key:
            done[name] = json.load(open(meta_path))
            print(f"BUILD {name}: already built for this source, skipped", flush=True)
            continue
        t = time.time()
        subprocess.run(["cargo", *args], cwd=cwd, env=env, check=True)
        secs = round(time.time() - t, 1)
        shutil.copy2(f"{work}/{product}", f"{BIN}/{name}")
        meta = {**common, "binary": name, "source_key": key, "cargo_args": args, "build_seconds": secs,
                "built_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "sha256": _sha256(f"{BIN}/{name}"), "bytes": os.path.getsize(f"{BIN}/{name}"),
                "ldd": _sh("ldd", f"{BIN}/{name}").splitlines()}
        with open(meta_path, "w") as f:
            json.dump(meta, f, indent=2)
        volume.commit()
        done[name] = meta
        print(f"BUILD {name}: {secs} s, sha256 {meta['sha256']}", flush=True)
    return done


class _GpuSampler:
    """nvidia-smi every 200 ms into a CSV, for peak GPU memory, utilisation and power."""

    FIELDS = "timestamp,utilization.gpu,utilization.memory,memory.used,power.draw,clocks.sm,temperature.gpu"

    def __init__(self, path: str):
        import subprocess

        self.path = path
        self.proc = subprocess.Popen(["nvidia-smi", f"--query-gpu={self.FIELDS}", "--format=csv,noheader,nounits", "-lms", "200"],
                                     stdout=open(path, "w"), stderr=subprocess.DEVNULL)
        time.sleep(1.0)  # baseline samples before the work starts

    def stop(self) -> None:
        time.sleep(0.5)
        self.proc.terminate()
        self.proc.wait()

    def stats(self, window_start: float | None = None, window_end: float | None = None) -> dict:
        import datetime as dt

        rows = []
        for line in open(self.path):
            parts = [x.strip() for x in line.split(",")]
            if len(parts) != 7:
                continue
            try:
                ts = dt.datetime.strptime(parts[0], "%Y/%m/%d %H:%M:%S.%f").replace(tzinfo=dt.timezone.utc).timestamp()
                rows.append((ts, *(float(x) for x in parts[1:])))
            except ValueError:
                continue
        if not rows:
            return {"samples": 0}
        busy = [r for r in rows if r[1] > 0]
        out = {
            "samples": len(rows), "sample_period_ms": 200,
            "gpu_memory_used_mib_before": rows[0][3], "gpu_memory_used_mib_peak": max(r[3] for r in rows),
            "power_w_peak": max(r[4] for r in rows), "sm_clock_mhz_peak": max(r[5] for r in rows),
            "temperature_c_peak": max(r[6] for r in rows),
            "gpu_busy_seconds": len(busy) * 0.2,
            "first_busy_to_last_busy_s": (busy[-1][0] - busy[0][0]) if busy else 0.0,
        }
        window = [r for r in rows if (window_start is None or r[0] >= window_start) and (window_end is None or r[0] <= window_end)]
        if window:
            out.update({
                "window_samples": len(window),
                "gpu_util_pct_mean_in_window": sum(r[1] for r in window) / len(window),
                "gpu_util_share_of_samples_above_0_in_window": sum(1 for r in window if r[1] > 0) / len(window),
                "gpu_mem_bandwidth_util_pct_mean_in_window": sum(r[2] for r in window) / len(window),
                "power_w_mean_in_window": sum(r[4] for r in window) / len(window),
            })
        return out


def _gnu_time(lines: list[str]) -> dict:
    keys = {
        "Maximum resident set size (kbytes)": "max_rss_kib", "User time (seconds)": "user_s",
        "System time (seconds)": "system_s", "Percent of CPU this job got": "cpu_percent",
        "Elapsed (wall clock) time (h:mm:ss or m:ss)": "elapsed", "Exit status": "exit_status",
    }
    out = {}
    for line in lines:
        s = line.strip()
        for k, v in keys.items():
            if s.startswith(k + ":"):
                out[v] = s[len(k) + 1:].strip()
    return out


def _container_facts() -> dict:
    import os

    return {
        "gpu_type_requested": GPU,
        "gpu": _sh("nvidia-smi", "--query-gpu=name,memory.total,driver_version,compute_cap", "--format=csv,noheader"),
        "container_cpu_cores": CPU_CORES, "container_memory_mib": MEMORY_MIB, "nproc": _sh("nproc"),
        "modal_task_id": os.environ.get("MODAL_TASK_ID"), "modal_region": os.environ.get("MODAL_REGION"),
        "modal_cloud_provider": os.environ.get("MODAL_CLOUD_PROVIDER"),
    }


def _finish(out_dir: str, run: str, record: dict, summary_keys: tuple[str, ...]) -> dict:
    with open(f"{out_dir}/run.json", "w") as f:
        json.dump(record, f, indent=2)
    with open(f"/vol/results/{run}/timings.jsonl", "a") as f:
        f.write(json.dumps({k: record.get(k) for k in summary_keys}) + "\n")
    volume.commit()
    return record


def _prove_checkpoint(name: str, run: str, limit_s: int) -> dict:
    """Succinct proof of one ferrata-checkpoint fixture. Writes receipt-succinct.bin, journal.hex,
    prove.json, run.json, prover.log, gpu-samples.csv and nvidia-smi.txt to results/<run>/<name>/."""
    import os
    import subprocess
    import threading

    t_start = time.time()
    out = f"/vol/results/{run}/{name}"
    os.makedirs(out, exist_ok=True)
    with open(f"{out}/nvidia-smi.txt", "w") as f:
        f.write(_sh("nvidia-smi") + "\n\n" + _sh("nvidia-smi", "-q", "-d", "MEMORY,CLOCK,POWER,COMPUTE") + "\n")
    prover = _local_copy(f"{BIN}/ferrata-modal-cuda", "/root/bin/ferrata-modal-cuda")

    # The inputs as staged on the laptop, and the image ids recomputed here from the uploaded programs.
    manifest = json.load(open("/inputs/MANIFEST.json"))
    hashes_ok = all(_sha256(f"/inputs/{p}") == v["sha256"] for p, v in manifest["files"].items() if os.path.exists(f"/inputs/{p}"))
    meta = json.load(open(f"/inputs/fixtures/{name}.json"))
    program = f"/inputs/programs/{meta['kind']}.bin"
    # The segment guest, rule (A)'s checkpoint guest and the fixture's own program (checkpoint-b.bin for a
    # rule (B) fixture): every id the manifest names is recomputed from the uploaded binaries.
    programs = sorted({"/inputs/programs/checkpoint.bin", "/inputs/programs/segment.bin", program})
    id_lines = _sh(prover, "image-id", *programs).splitlines()
    ids = {pathlib.Path(r["program"]).stem: r["image_id"] for r in (json.loads(l) for l in id_lines if l.startswith("{"))}
    ids_equal = ids == manifest["local_image_ids"]

    sampler = _GpuSampler(f"{out}/gpu-samples.csv")
    cmd = ["/usr/bin/time", "-v", prover, "prove", name, program, f"/inputs/fixtures/{name}.input", f"/inputs/fixtures/{name}.json", out]
    env = {**os.environ, "RUST_BACKTRACE": "1"}
    env.pop("RISC0_DEV_MODE", None)
    t0 = time.time()
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
    killer = threading.Timer(limit_s, proc.kill)
    killer.start()
    lines, t_exec_end = [], None
    with open(f"{out}/prover.log", "w") as log:
        for line in proc.stdout:
            now = time.time()
            if t_exec_end is None and "] executed:" in line:
                t_exec_end = now
            lines.append(line)
            log.write(f"{now - t0:9.3f} {line}")
            log.flush()
            print(f"[{name} +{now - t0:7.1f}s] {line}", end="", flush=True)
    rc = proc.wait()
    killer.cancel()
    t_end = time.time()
    sampler.stop()

    prove_json = json.load(open(f"{out}/prove.json")) if rc == 0 and os.path.exists(f"{out}/prove.json") else None
    record = {
        "ok": rc == 0 and prove_json is not None and ids_equal and hashes_ok,
        "name": name, "run": run, "exit_code": rc, "timed_out": rc != 0 and (t_end - t0) >= limit_s,
        **_container_facts(),
        "image_ids_here": ids, "image_ids_local": manifest["local_image_ids"], "image_ids_equal": ids_equal,
        "input_hashes_equal_manifest": hashes_ok, "source_commit": manifest["commit"],
        "prover_wall_s": t_end - t0, "container_s_until_done": t_end - t_start,
        "gnu_time": _gnu_time(lines), "gpu_stats": sampler.stats(t_exec_end, t_end), "gpu_stats_window": "execution end to prover exit",
        "prove": prove_json, "build_info": json.load(open(f"{BIN}/ferrata-modal-cuda.json")),
        "prover_binary_sha256_here": _sha256(prover),
        "finished_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    return _finish(out, run, record, ("ok", "name", "run", "gpu", "image_ids_equal", "prover_wall_s", "container_s_until_done", "finished_at_utc"))


def _prove_lez(run: str, limit_s: int, job: str = "lez-update") -> dict:
    """One LEZ proof with a prebuilt host binary, r0vm 3.0.5 built with CUDA as its prover
    (RISC0_SERVER_PATH), exactly the documented command. `lez-update` is the light-client update (lez
    RESULTS.md §3): prove_private writes prove-private.log and stage2-private-tx.borsh. `lez-attest` is one
    private attestation (lez RESULTS.md §5.5): prove_attest writes prove-attest.log and
    stage4-private-attest-tx.borsh. Both, with run.json and gpu-samples.csv, go to results/<run>/<job>/."""
    import os
    import shutil
    import subprocess

    spec = LEZ_JOBS[job]
    src, binary, tx_name, log_name = spec["dir"], spec["binary"], spec["tx"], spec["log"]
    t_start = time.time()
    out = f"/vol/results/{run}/{job}"
    os.makedirs(out, exist_ok=True)
    with open(f"{out}/nvidia-smi.txt", "w") as f:
        f.write(_sh("nvidia-smi") + "\n")
    work = f"/root/{job}"
    r0vm = _local_copy(f"{BIN}/r0vm-cuda", "/root/bin/r0vm")
    _local_copy(f"{src}/{binary}", f"{work}/{binary}")
    manifest = json.load(open(f"{src}/MANIFEST.json"))
    hashes_ok = all(_sha256(f"{src}/{p}") == v["sha256"] for p, v in manifest["files"].items())
    if job == "lez-update":
        job_env = {"FERRATA_EXPECT_IMAGE_ID": LEZ_IMAGE_ID, "FERRATA_FIXTURE_DIR": f"{src}/mainnet-p1899"}
        source = {"expected_image_id": LEZ_IMAGE_ID, "ferrata_lez_commit": manifest["ferrata_lez_commit"]}
    else:
        # The image the proof covers is the consumer's; the manifest holds the ids the staged binary
        # reported (`prove_attest --image-ids`), and the binary stops unless its consumer image is that one.
        fixture = pathlib.PurePosixPath(manifest["fixtures_from"]).name
        job_env = {"FERRATA_EXPECT_CONSUMER_IMAGE_ID": manifest["expected_consumer_image_id"],
                   "FERRATA_ATTEST_FIXTURE_DIR": f"{src}/{fixture}"}
        source = {"expected_consumer_image_id": manifest["expected_consumer_image_id"],
                  "program_image_id": manifest["program_image_id"], "ferrata_commit": manifest["ferrata_commit"]}

    sampler = _GpuSampler(f"{out}/gpu-samples.csv")
    env = {**os.environ, **job_env, "RISC0_DEV_MODE": "0", "RISC0_SERVER_PATH": r0vm}
    t0 = time.time()
    with open(f"{work}/{log_name}", "w") as log:  # `> <log> 2>&1`
        proc = subprocess.Popen(["/usr/bin/time", "-v", f"./{binary}", "--tx-out", tx_name],
                                cwd=work, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            rc = proc.wait(timeout=limit_s)
        except subprocess.TimeoutExpired:
            proc.kill()
            rc = proc.wait()
    t_end = time.time()
    sampler.stop()
    for f in (log_name, tx_name):
        if os.path.exists(f"{work}/{f}"):
            shutil.copy2(f"{work}/{f}", f"{out}/{f}")
    lines = open(f"{work}/{log_name}").read().splitlines()
    print("\n".join(lines[-45:]), flush=True)
    tx = f"{work}/{tx_name}"
    record = {
        "ok": rc == 0 and hashes_ok and os.path.exists(tx), "name": job, "run": run, "exit_code": rc,
        "timed_out": rc != 0 and (t_end - t0) >= limit_s,
        **_container_facts(), **source, "input_hashes_equal_manifest": hashes_ok,
        "binary_sha256": manifest["files"][binary]["sha256"],
        "r0vm": _sh(r0vm, "--version"), "wall_s": t_end - t0, "container_s_until_done": t_end - t_start,
        "gnu_time": _gnu_time(lines), "gpu_stats": sampler.stats(t0, t_end), "gpu_stats_window": f"whole {binary} run",
        "log_lines": [l for l in lines if l.startswith(spec["lines"])],
        "tx_bytes": os.path.getsize(tx) if os.path.exists(tx) else None, "tx_sha256": _sha256(tx) if os.path.exists(tx) else None,
        "build_info": json.load(open(f"{BIN}/r0vm-cuda.json")), "r0vm_sha256_here": _sha256(r0vm),
        "finished_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    return _finish(out, run, record, ("ok", "name", "run", "gpu", "wall_s", "container_s_until_done", "tx_bytes", "finished_at_utc"))


GPU_FUNCTION = dict(image=gpu_image, gpu=GPU, cpu=CPU_CORES, memory=MEMORY_MIB, timeout=HARD_TIMEOUT_S,
                    volumes={"/vol": volume}, max_containers=1,
                    single_use_containers=True,  # the container exits after its input; nothing stays warm
                    retries=0)


@app.function(**GPU_FUNCTION)
def prove(name: str, run: str, limit_s: int) -> dict:
    return _prove_checkpoint(name, run, limit_s)


@app.function(**GPU_FUNCTION)
def prove_lez(run: str, limit_s: int, job: str = "lez-update") -> dict:
    return _prove_lez(run, limit_s, job)


@app.function(**GPU_FUNCTION)
def batch(jobs: list[str], run: str, limits: dict) -> list[dict]:
    """Several proofs one after another in one GPU container, each in its own process with its own
    time limit, so the queue for an L4 is waited for once. A failed job does not stop the next."""
    records = []
    for job in jobs:
        try:
            r = _prove_lez(run, limits[job], job) if job in LEZ_JOBS else _prove_checkpoint(job, run, limits[job])
        except Exception as e:  # recorded, then the next job
            r = {"ok": False, "name": job, "run": run, "error": repr(e)}
        records.append(r)
        print(f"BATCH {job}: ok={r.get('ok')}", flush=True)
    return records


@app.local_entrypoint()
def batch_run(jobs: str, run: str = ""):
    """`--jobs name=limit_s,…` (fixture names, `lez-update` or `lez-attest`), proven in this order in one container."""
    run = run or time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    pairs = [j.split("=") for j in jobs.split(",") if j]
    limits = {name: int(limit) for name, limit in pairs}
    assert sum(limits.values()) < HARD_TIMEOUT_S - 300, "the limits must fit in the function's hard timeout"
    t0 = time.time()
    records = batch.remote([name for name, _ in pairs], run, limits)
    (HERE / "logs").mkdir(exist_ok=True)
    for r in records:
        r["client_round_trip_s_of_batch"] = time.time() - t0
        (HERE / "logs" / f"run-{run}-{r['name']}.json").write_text(json.dumps(r, indent=2) + "\n")
        p = r.get("prove") or {}
        print(f"RESULT {r['name']}: ok={r.get('ok')} total_s={p.get('total_s', r.get('wall_s'))} segments={p.get('segments')} error={r.get('error')}")


@app.local_entrypoint()
def main(fixtures: str = "cp-empty-1", run: str = "", limit_s: int = 1800):
    """Proves ferrata-checkpoint fixtures one after another (each in its own container); stops at the first failure."""
    run = run or time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    for name in [f for f in fixtures.split(",") if f]:
        t0 = time.time()
        record = prove.remote(name, run, limit_s)
        record["client_round_trip_s"] = time.time() - t0
        (HERE / "logs").mkdir(exist_ok=True)
        (HERE / "logs" / f"run-{run}-{name}.json").write_text(json.dumps(record, indent=2) + "\n")
        p = record.get("prove") or {}
        print(f"RESULT {name}: ok={record['ok']} image_ids_equal={record['image_ids_equal']} total_s={p.get('total_s')} "
              f"segments={p.get('segments')} round_trip_s={record['client_round_trip_s']:.1f} gpu={record['gpu']}")
        if not record["ok"]:
            raise SystemExit(f"{name}: failed (exit {record['exit_code']}); not continuing")


@app.local_entrypoint()
def spawn(fixtures: str = "cp-empty-1", run: str = "", limit_s: int = 1800):
    """Spawns the proofs (one container each) and returns at once: with --detach the app lives until they
    finish, and each result lands on the Volume under results/<run>/<name>/. `main` waits instead, and its
    client gave up ("Deadline exceeded") after 30 min with no L4 scheduled (rule-b branch, 10 Nov 2026)."""
    run = run or time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    for name in [f for f in fixtures.split(",") if f]:
        call = prove.spawn(name, run, limit_s)
        print(f"SPAWNED {name}: call {call.object_id}, results/{run}/{name}/ on the Volume")


@app.local_entrypoint()
def lez(run: str = "", limit_s: int = 3600, job: str = "lez-update"):
    """One LEZ proof in one container: `--job lez-update` (the light-client update, the default) or
    `--job lez-attest` (one private attestation)."""
    assert job in LEZ_JOBS, f"unknown LEZ job {job!r}: one of {sorted(LEZ_JOBS)}"
    run = run or time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    t0 = time.time()
    record = prove_lez.remote(run, limit_s, job)
    record["client_round_trip_s"] = time.time() - t0
    (HERE / "logs").mkdir(exist_ok=True)
    (HERE / "logs" / f"run-{run}-{job}.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"RESULT {job}: ok={record['ok']} wall_s={record['wall_s']:.1f} gpu={record['gpu']} tx_bytes={record['tx_bytes']}")
