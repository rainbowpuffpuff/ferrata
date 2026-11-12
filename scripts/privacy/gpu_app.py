"""The privacy suite with real proofs on one GPU (Modal): the `gpu` step of scripts/privacy/real.sh.

    MODAL_PROFILE=<profile> modal run --detach scripts/privacy/gpu_app.py --run <name> [--limit-s 5400]

One container with one NVIDIA L4 runs, in order:
1. the LEZ half of the suite (the `privacy_suite` test binary `real.sh build` made on the build box) with
   `FERRATA_PRIVACY_PROOFS=real`: every private claim and burn proven by r0vm 3.0.5 built with CUDA
   (`RISC0_SERVER_PATH`, as gpu-prover/modal_app.py's `prove_lez`), every receipt verified by LEZ's settlement;
2. each release statement's succinct proof and its identity_p254 step (`ferrata-e2e prove`, `ferrata-e2e p254`,
   through the same r0vm), from the guest inputs the test wrote.
The Groth16 step is not here: RISC Zero's prover image runs elsewhere (`real.sh wrap`, e2e/crown/wrap.py).

Inputs are the stage directory (`FERRATA_PRIVACY_STAGE`, default target/privacy/real/stage at the repository root):
the test binary, ferrata-e2e, the release program (R0BF) and MANIFEST.json with their sha256 and the image ids
docs/image-ids.md records. The CUDA r0vm is the Volume's bin/r0vm-cuda (`real.sh volume` puts it there); its sha256
must equal the manifest's before anything runs, the release program's image id must equal the expected one, and
the test stops before its first proof unless the bridge program's id is the expected one. Everything the run
writes goes to the Volume `ferrata-prover` as privacy/<run>.tar.gz, with run.json (GPU, timings, hashes) beside it.
"""

import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tarfile
import time

import modal

HERE = pathlib.Path(__file__).resolve().parent
STAGE = pathlib.Path(os.environ.get("FERRATA_PRIVACY_STAGE") or HERE.parent.parent / "target" / "privacy" / "real" / "stage")
VOLUME_NAME = "ferrata-prover"  # the name gpu-prover/modal_app.py uses
GPU, CPU_CORES, MEMORY_MIB = "L4", 4.0, 32768  # as gpu-prover/modal_app.py
HARD_TIMEOUT_S = 2 * 3600

# Ubuntu 24.04 (glibc 2.39, as the build box that built the binaries); r0vm-cuda needs only libcuda.so.1 from the
# driver Modal mounts, with libstdc++ and libgcc_s.
image = (
    modal.Image.from_registry("nvidia/cuda:12.8.0-runtime-ubuntu24.04", add_python="3.12")
    .apt_install("time", "ca-certificates", "libssl3t64", "zlib1g")
    .add_local_dir(STAGE, "/stage")
)
app = modal.App("ferrata-privacy-real")
volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)


def _sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _sh(*cmd: str) -> str:
    r = subprocess.run(cmd, capture_output=True, text=True)
    return (r.stdout + r.stderr).strip()


def _run(cmd: list, log: str, env: dict, cwd: str, limit_s: int) -> tuple:
    t0 = time.time()
    with open(log, "w") as f:
        proc = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT)
        try:
            rc = proc.wait(timeout=limit_s)
        except subprocess.TimeoutExpired:
            proc.kill()
            rc = proc.wait()
    return rc, time.time() - t0


def _gnu_time(log: str) -> dict:
    keys = {"Maximum resident set size (kbytes)": "max_rss_kib", "Elapsed (wall clock) time (h:mm:ss or m:ss)": "elapsed",
            "User time (seconds)": "user_s", "System time (seconds)": "system_s", "Exit status": "exit_status"}
    out = {}
    for line in open(log, errors="replace"):
        s = line.strip()
        for k, v in keys.items():
            if s.startswith(k + ":"):
                out[v] = s[len(k) + 1:].strip()
    return out


def _gpu_stats(path: str) -> dict:
    rows = []
    for line in open(path):
        p = [x.strip() for x in line.split(",")]
        try:
            rows.append((float(p[1]), float(p[2]), float(p[3])))  # utilisation %, memory MiB, power W
        except (IndexError, ValueError):
            continue
    if not rows:
        return {"samples": 0}
    return {"samples": len(rows), "sample_period_s": 1.0, "gpu_memory_mib_peak": max(r[1] for r in rows),
            "gpu_memory_mib_first": rows[0][1], "busy_samples": sum(1 for r in rows if r[0] > 0),
            "util_pct_mean": sum(r[0] for r in rows) / len(rows), "power_w_peak": max(r[2] for r in rows)}


@app.function(image=image, gpu=GPU, cpu=CPU_CORES, memory=MEMORY_MIB, timeout=HARD_TIMEOUT_S, volumes={"/vol": volume},
              max_containers=1, single_use_containers=True, retries=0)
def suite(run: str, limit_s: int) -> dict:
    t_start = time.time()
    out = f"/root/out/{run}"
    os.makedirs(f"{out}/releases", exist_ok=True)
    manifest = json.load(open("/stage/MANIFEST.json"))
    record = {"run": run, "ok": False, "manifest": manifest, "started_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    record["staged_hashes_equal_manifest"] = all(_sha256(f"/stage/{n}") == v["sha256"] for n, v in manifest["files"].items())
    bin_dir = "/root/bin"
    os.makedirs(bin_dir, exist_ok=True)
    for name in manifest["files"]:
        shutil.copy2(f"/stage/{name}", f"{bin_dir}/{name}")
        os.chmod(f"{bin_dir}/{name}", 0o755)
    r0vm = f"{bin_dir}/r0vm"
    shutil.copy2("/vol/bin/r0vm-cuda", r0vm)
    os.chmod(r0vm, 0o755)
    record["r0vm_sha256"] = _sha256(r0vm)
    record["r0vm_equal_manifest"] = record["r0vm_sha256"] == manifest["r0vm_cuda_sha256"]
    record["r0vm_version"] = _sh(r0vm, "--version")
    e2e, program = f"{bin_dir}/ferrata-e2e", f"{bin_dir}/release.bin"
    ids = json.loads(_sh(e2e, "image-id", program).splitlines()[-1])
    record["release_image_id"] = ids["image_id"]
    record["release_id_equal_expected"] = ids["image_id"].lower() == manifest["expected"]["release"].lower()
    record["gpu"] = _sh("nvidia-smi", "--query-gpu=name,memory.total,driver_version", "--format=csv,noheader")
    record["nproc"] = _sh("nproc")
    if not (record["staged_hashes_equal_manifest"] and record["r0vm_equal_manifest"] and record["release_id_equal_expected"]):
        record["error"] = "the staged inputs, r0vm or the release program are not the expected ones: nothing proven"
        return record

    sampler = subprocess.Popen(["nvidia-smi", "--query-gpu=timestamp,utilization.gpu,memory.used,power.draw",
                                "--format=csv,noheader,nounits", "-l", "1"],
                               stdout=open(f"{out}/gpu-samples.csv", "w"), stderr=subprocess.DEVNULL)
    env = {**os.environ, "RISC0_SERVER_PATH": r0vm, "RISC0_DEV_MODE": "0", "FERRATA_PRIVACY_PROOFS": "real",
           "FERRATA_PRIVACY_DIR": f"{out}/data", "FERRATA_EXPECT_BRIDGE_IMAGE_ID": manifest["expected"]["bridge"],
           "RUST_BACKTRACE": "1"}
    rc, secs = _run(["/usr/bin/time", "-v", f"{bin_dir}/privacy_suite", "--exact", "privacy_suite_lez_half",
                     "--nocapture", "--test-threads", "1"], f"{out}/lez-half.log", env, out, limit_s)
    record["lez_half"] = {"exit_code": rc, "wall_s": secs, "gnu_time": _gnu_time(f"{out}/lez-half.log")}
    releases = []
    if rc == 0:
        for k in range(int(manifest.get("users", 20))):
            name = f"release-{k:02}"
            d = f"{out}/releases/{name}"
            inp, meta = f"{out}/data/releases/{name}.input", f"{out}/data/releases/{name}.json"
            rp, sp = _run([e2e, "prove", "--program", program, "--input", inp, "--meta", meta, "--out-dir", d],
                          f"{d}.prove.log", env, out, 900)
            rw, sw = (_run([e2e, "p254", "--receipt", f"{d}/receipt-succinct.bin", "--image-id", ids["image_id"],
                            "--out-dir", f"{d}/p254"], f"{d}.p254.log", env, out, 900) if rp == 0 else (None, 0.0))
            releases.append({"name": name, "prove_exit": rp, "prove_s": sp, "p254_exit": rw, "p254_s": sw})
    sampler.terminate()
    sampler.wait()
    record["releases"] = releases
    record["gpu_stats"] = _gpu_stats(f"{out}/gpu-samples.csv")
    record["ok"] = rc == 0 and len(releases) > 0 and all(r["prove_exit"] == 0 and r["p254_exit"] == 0 for r in releases)
    record["container_s"] = time.time() - t_start
    record["finished_at_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(f"{out}/run.json", "w") as f:
        json.dump(record, f, indent=1)
    os.makedirs("/vol/privacy", exist_ok=True)
    with tarfile.open(f"/vol/privacy/{run}.tar.gz", "w:gz") as tf:
        tf.add(out, arcname=run)
    shutil.copy2(f"{out}/run.json", f"/vol/privacy/{run}.run.json")
    volume.commit()
    return record


@app.local_entrypoint()
def main(run: str = "", limit_s: int = 5400):
    run = run or time.strftime("privacy-real-%Y%m%dT%H%M%SZ", time.gmtime())
    assert limit_s < HARD_TIMEOUT_S - 1200, "the LEZ half's limit must leave time for the releases"
    t0 = time.time()
    record = suite.remote(run, limit_s)
    record["client_round_trip_s"] = time.time() - t0
    keep = {k: record.get(k) for k in ("run", "ok", "error", "gpu", "r0vm_equal_manifest", "release_id_equal_expected",
                                       "lez_half", "gpu_stats", "container_s")}
    keep["releases_ok"] = sum(1 for r in record.get("releases", []) if r["prove_exit"] == 0 and r["p254_exit"] == 0)
    print("RESULT " + json.dumps(keep))
