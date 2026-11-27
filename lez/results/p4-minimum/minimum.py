"""p4-minimum: real CPU proofs at the proposal's declared minimum hardware, on Modal.

A container held to the minimum (cpu and memory request = limit; P4_CPU Modal cores, P4_MEM_MIB MiB
of memory as a hard limit) runs prebuilt LEZ host binaries with r0vm 3.0.5 (the CPU release that
`rzup install r0vm 3.0.5` installs) as their prover, exactly the commands of the GPU jobs
`lez-attest` and `lez-bridge` (`gpu-prover/modal_app.py`) with RISC0_SERVER_PATH pointing at the CPU
r0vm. Nothing is built here: the binaries, the fixture and r0vm come through the Volume
(`/bundle/attest`, `/bundle/bridge`: the output of `lez/scripts/stage-prove-attest.sh` and
`stage-prove-bridge.sh` on the build box; `/bundle/r0vm/r0vm`).

    export MODAL_PROFILE=<workspace>
    modal volume put p4-minimum <stage>/attest /bundle/attest      # once, and bridge, r0vm alike
    P4_CPU=4 modal run minimum.py::probe_main                      # what the container reports
    P4_CPU=4 modal run --detach minimum.py::main --job attest --runs 3 --tag <run>
    P4_CPU=4 modal run --detach minimum.py::main --job bridge --runs 3 --tag <run>
    P4_CPU=4 modal run --detach minimum.py::main --job transfer --runs 2 --tag <run>   # LEZ's wallet yardstick
    P4_CPU=4 modal run --detach minimum.py::release_main --runs 2 --tag <run>          # release: succinct, wrap
    modal volume get p4-minimum /results/<run> <local directory>

Each run: `/usr/bin/time -v <binary> <prove args>` with RISC0_DEV_MODE=0 (real proofs), RISC0_INFO=1
and RUST_LOG=info (r0vm then logs the segments and cycles of every session it proves), a sampler of
the container's memory every 0.5 s (the cgroup's `memory.usage_in_bytes` (v1, as gVisor shows it), /proc/meminfo, the sum
of the processes' VmRSS, r0vm's VmRSS) and of r0vm's CPU time, then `<binary> --settle <tx files>`
(LEZ's settlement with the receipt verified). After run 1, one bit flipped in the middle of each
transaction file must make settlement refuse it. Results go to /vol/results/<tag>/<job>/run-<i>/.
"""

import json
import os
import pathlib
import re
import subprocess
import threading
import time

import modal

CPU = float(os.environ.get("P4_CPU", "4"))  # Modal cores; request = limit
MEM_MIB = int(os.environ.get("P4_MEM_MIB", "16384"))  # request = limit (hard)
VOLUME = os.environ.get("P4_VOLUME", "p4-minimum")
RUN_LIMIT_S = 2 * 3600  # one binary run
HARD_TIMEOUT_S = 6 * 3600  # one function call

app = modal.App(os.environ.get("P4_APP", "p4-minimum"))  # P4_APP: a separate name keeps a pass's spend apart in billing
vol = modal.Volume.from_name(VOLUME, create_if_missing=True)
image = modal.Image.from_registry("ubuntu:24.04", add_python="3.12").apt_install(
    "time", "procps", "file", "util-linux", "ca-certificates"
)
FUNCTION = dict(
    image=image, cpu=(CPU, CPU), memory=(MEM_MIB, MEM_MIB), volumes={"/vol": vol}, timeout=HARD_TIMEOUT_S,
    max_containers=2, single_use_containers=True, retries=0,
)

JOBS = {
    # one private attestation (lez/RESULTS.md §5, gpu-prover job lez-attest)
    "attest": {
        "binary": "prove_attest",
        "prove": ["--tx-out", "tx-attest.borsh"],
        "txs": ["tx-attest.borsh"],
    },
    # the bridge program's private claim, then the private burn of what it minted (BRIDGE-RESULTS.md §7)
    "bridge": {
        "binary": "prove_bridge",
        "prove": ["--claim-out", "tx-claim.borsh", "--burn-out", "tx-burn.borsh"],
        "txs": ["tx-claim.borsh", "tx-burn.borsh"],
    },
    # the padded pass: the same transactions padded as LEZ's wallet pads every private transaction
    # (harness/src/private.rs, Shape::Wallet: seven private actions, 512-byte notes, the fee's recovery note)
    "attest-pad": {
        "binary": "prove_attest",
        "prove": ["--shape", "wallet", "--tx-out", "tx-attest.borsh"],
        "txs": ["tx-attest.borsh"],
    },
    "bridge-pad": {
        "binary": "prove_bridge",
        "prove": ["--shape", "wallet", "--claim-out", "tx-claim.borsh", "--burn-out", "tx-burn.borsh"],
        "txs": ["tx-claim.borsh", "tx-burn.borsh"],
    },
    # RFP-021 P.4, hops inside the user's proof: the reference consumer's private claim of the vault fixture's deposit 0,
    # its block reached through K EIP-2935 hops (harness/src/bin/prove_vault_claim.rs; staged by
    # lez/scripts/stage-prove-vault-claim.sh, the fixtures read through the stage's MANIFEST.json "env"), padded
    **{f"claim-hops{k}-pad": {
        "binary": "prove_vault_claim",
        "prove": ["--hops", str(k), "--shape", "wallet", "--tx-out", "tx-claim.borsh"],
        "txs": ["tx-claim.borsh"],
        "bundle": "claim-hops",
    } for k in range(1, 6)},
    # LEZ's yardstick: the cheapest private transaction of LEZ's own wallet, a deshielded native transfer
    # padded as the wallet pads it (harness/src/bin/prove_native_transfer.rs)
    "transfer": {
        "binary": "prove_native_transfer",
        "prove": ["--shape", "wallet", "--tx-out", "tx-transfer.borsh"],
        "txs": ["tx-transfer.borsh"],
    },
}

# The release program of the final programs (e2e/crown/README.md) and the Groth16 prover image that
# risc0-groth16 3.0.5 starts with Docker, run here as a plain function as e2e/crown/cloud/app.py does.
RELEASE_IMAGE_ID = "0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd"
wrap_image = modal.Image.from_registry("risczero/risc0-groth16-prover:v2025-04-03.1", add_python="3.12").entrypoint([])


def _read(path: str):
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return None


def _sh(*cmd: str) -> str:
    try:
        r = subprocess.run(list(cmd), capture_output=True, text=True)
    except OSError as e:
        return f"({cmd[0]}: {e})"
    return (r.stdout + r.stderr).strip()


def _sha256(path: str) -> str:
    import hashlib

    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _meminfo_kib() -> dict:
    out = {}
    for line in open("/proc/meminfo"):
        k, _, v = line.partition(":")
        if k in ("MemTotal", "MemFree", "MemAvailable", "Cached", "Shmem", "SwapTotal", "SwapFree"):
            out[k] = int(v.split()[0])
    return out


def _cgroup() -> dict:
    v2 = ("cgroup.controllers", "cpu.max", "cpu.weight", "cpuset.cpus.effective", "memory.max", "memory.high",
          "memory.current", "memory.peak", "memory.swap.max", "memory.events")
    v1 = ("memory/memory.limit_in_bytes", "memory/memory.max_usage_in_bytes", "memory/memory.usage_in_bytes",
          "cpu/cpu.cfs_quota_us", "cpu/cpu.cfs_period_us", "cpuset/cpuset.cpus")
    return {f: _read(f"/sys/fs/cgroup/{f}") for f in (*v2, *v1)}


def _facts(declared: dict) -> dict:
    cpuinfo = open("/proc/cpuinfo").read()
    m = re.search(r"^flags\s*:\s*(.*)$", cpuinfo, re.M)
    flags = set(m.group(1).split()) if m else set()
    keep = ("MODAL_REGION", "MODAL_CLOUD_PROVIDER")
    return {
        "declared": declared,
        "nproc": _sh("nproc"), "nproc_all": _sh("nproc", "--all"), "os_cpu_count": os.cpu_count(),
        "sched_getaffinity": len(os.sched_getaffinity(0)),
        "cpuinfo_processors": len(re.findall(r"^processor\s*:", cpuinfo, re.M)),
        "cpuinfo_model_names": sorted(set(re.findall(r"^model name\s*:\s*(.*)$", cpuinfo, re.M))),
        "cpuinfo_ids": sorted(set(re.findall(r"^(vendor_id|cpu family|model|stepping|cache size|bogomips|microcode)\s*:\s*(.*)$",
                                             cpuinfo, re.M))),
        "cpuinfo_siblings": sorted(set(re.findall(r"^siblings\s*:\s*(.*)$", cpuinfo, re.M))),
        "cpuinfo_cpu_cores": sorted(set(re.findall(r"^cpu cores\s*:\s*(.*)$", cpuinfo, re.M))),
        "cpu_flags": sorted(f for f in flags if f in ("avx2", "avx512f", "avx512ifma", "sha_ni", "bmi2", "adx", "ht")),
        "cgroup": _cgroup(), "proc_self_cgroup": _read("/proc/self/cgroup"), "meminfo_kib": _meminfo_kib(),
        "kernel": _read("/proc/version"), "os": _sh("bash", "-c", ". /etc/os-release; echo $PRETTY_NAME"),
        "glibc": _sh("ldd", "--version").splitlines()[0], "env": {k: os.environ[k] for k in keep if k in os.environ},
    }


def _spin_test(procs: int, seconds: float = 5.0) -> dict:
    """procs busy processes for `seconds` of wall time: CPU seconds obtained / wall = CPUs actually usable."""
    import resource

    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    ps = [subprocess.Popen(["python3", "-c", f"import time\nt=time.time()\nwhile time.time()-t<{seconds}: pass"])
          for _ in range(procs)]
    for p in ps:
        p.wait()
    wall = time.time() - t0
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    return {"procs": procs, "wall_s": round(wall, 2), "cpu_s": round(cpu, 2), "cpus_used": round(cpu / wall, 2)}


class _Sampler(threading.Thread):
    """Every `period` s: cgroup memory.current, /proc/meminfo used, the sum of VmRSS, r0vm's VmRSS and
    the last CPU ticks of every r0vm and host-binary process."""

    def __init__(self, path: str, binary: str, period: float = 0.5):
        super().__init__(daemon=True)
        self.path, self.binary, self.period = path, binary[:15], period
        self.stop_evt = threading.Event()
        self.max = {"cgroup_current_bytes": 0, "meminfo_used_kib": 0, "rss_sum_kib": 0, "r0vm_rss_kib": 0}
        self.ticks: dict[int, tuple[str, int]] = {}
        self.samples = 0

    def run(self) -> None:
        hz = os.sysconf("SC_CLK_TCK")
        self.hz = hz
        t0 = time.time()
        with open(self.path, "w") as f:
            f.write("t_s,cgroup_current_bytes,meminfo_used_kib,rss_sum_kib,r0vm_rss_kib,r0vm_procs\n")
            while not self.stop_evt.is_set():
                cur = _read("/sys/fs/cgroup/memory.current") or _read("/sys/fs/cgroup/memory/memory.usage_in_bytes")
                cur = int(cur) if cur and cur.isdigit() else 0
                mi = _meminfo_kib()
                used = mi.get("MemTotal", 0) - mi.get("MemAvailable", 0)
                rss_sum = r0vm_rss = r0vm_n = 0
                for d in os.listdir("/proc"):
                    if not d.isdigit():
                        continue
                    try:
                        st = open(f"/proc/{d}/status").read()
                        name = re.search(r"^Name:\s*(\S+)", st, re.M).group(1)
                        m = re.search(r"^VmRSS:\s*(\d+)", st, re.M)
                        rss = int(m.group(1)) if m else 0
                        if name in ("r0vm", self.binary):
                            fields = open(f"/proc/{d}/stat").read().rsplit(")", 1)[1].split()
                            self.ticks[int(d)] = (name, int(fields[11]) + int(fields[12]))
                    except (OSError, AttributeError, ValueError, IndexError):
                        continue
                    rss_sum += rss
                    if name == "r0vm":
                        r0vm_rss += rss
                        r0vm_n += 1
                row = {"cgroup_current_bytes": cur, "meminfo_used_kib": used, "rss_sum_kib": rss_sum, "r0vm_rss_kib": r0vm_rss}
                for k, v in row.items():
                    self.max[k] = max(self.max[k], v)
                f.write(f"{time.time() - t0:.1f},{cur},{used},{rss_sum},{r0vm_rss},{r0vm_n}\n")
                self.samples += 1
                self.stop_evt.wait(self.period)

    def finish(self) -> dict:
        self.stop_evt.set()
        self.join(timeout=10)
        cpu = {}
        for _, (name, t) in self.ticks.items():
            cpu[name] = cpu.get(name, 0) + t
        return {"max": self.max, "samples": self.samples, "period_s": self.period,
                "cpu_s_by_process_name": {k: round(v / self.hz, 1) for k, v in cpu.items()}}


def _gnu_time(text: str) -> dict:
    keys = {"Elapsed (wall clock) time (h:mm:ss or m:ss)": "wall", "User time (seconds)": "user_s",
            "System time (seconds)": "sys_s", "Percent of CPU this job got": "cpu_percent",
            "Maximum resident set size (kbytes)": "max_rss_kib", "Exit status": "exit_status"}
    out = {}
    for line in text.splitlines():
        k, _, v = line.strip().rpartition(": ")
        if k in keys:
            out[keys[k]] = v
    return out


def _run(cmd: list, cwd: str, env: dict, log: str, limit_s: int) -> tuple[int, float]:
    t0 = time.time()
    with open(log, "w") as f:
        p = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT)
        try:
            rc = p.wait(timeout=limit_s)
        except subprocess.TimeoutExpired:
            p.kill()
            rc = p.wait()
    return rc, time.time() - t0


def _copy(src: str, dst: str) -> str:
    import shutil

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if os.path.isdir(src):
        shutil.copytree(src, dst, dirs_exist_ok=True)
    else:
        shutil.copy2(src, dst)
    return dst


def _flip(src: str, dst: str) -> int:
    data = bytearray(open(src, "rb").read())
    at = len(data) // 2
    data[at] ^= 1
    open(dst, "wb").write(bytes(data))
    return at


@app.function(**FUNCTION)
def probe(declared: dict, alloc_gib: int = 0) -> dict:
    facts = _facts(declared)
    facts["spin"] = [_spin_test(n) for n in (4, 8, 16)]
    # Does a file written to the container's disk count as memory here?
    before = (_read("/sys/fs/cgroup/memory/memory.usage_in_bytes"), _meminfo_kib())
    with open("/root/p4-probe.bin", "wb") as f:
        for _ in range(1024):
            f.write(os.urandom(1 << 20))
    after = (_read("/sys/fs/cgroup/memory/memory.usage_in_bytes"), _meminfo_kib())
    os.remove("/root/p4-probe.bin")
    facts["file_1gib_written"] = {"before": before, "after": after}
    if alloc_gib:
        # allocate and touch 1 GiB at a time in a child; where does it stop?
        code = ("import sys\nbufs=[]\nfor i in range(1, %d+1):\n    b=bytearray(1<<30)\n    b[::4096]=b'\\x01'*(1<<18)\n"
                "    bufs.append(b)\n    print(i, flush=True)\n") % alloc_gib
        print("P4-ALLOC limit_in_bytes", _read("/sys/fs/cgroup/memory/memory.limit_in_bytes"), flush=True)
        r = subprocess.run(["python3", "-c", code.replace("print(i,", "print('P4-ALLOC GiB touched', i,")])
        facts["alloc"] = {"asked_gib": alloc_gib, "returncode": r.returncode}
        facts["cgroup_after_alloc"] = _cgroup()
    return facts


@app.function(**FUNCTION)
def prove(job: str, runs: int, tag: str, declared: dict, affinity: int = 0) -> dict:
    spec = JOBS[job]
    binary = spec["binary"]
    src = f"/vol/bundle/{spec.get('bundle', job)}"
    work = f"/root/p4/{job}"
    out = f"/vol/results/{tag}/{job}"
    os.makedirs(out, exist_ok=True)
    manifest = json.load(open(f"{src}/MANIFEST.json"))
    hashes_ok = all(_sha256(f"{src}/{p}") == v["sha256"] for p, v in manifest["files"].items())
    _copy(src, work)  # binary and fixture to the container's own disk
    r0vm = _copy("/vol/bundle/r0vm/r0vm", "/root/p4/bin/r0vm")
    os.chmod(r0vm, 0o755)
    os.chmod(f"{work}/{binary}", 0o755)
    os.makedirs(f"{work}/tmp", exist_ok=True)
    env = {**os.environ, "RISC0_DEV_MODE": "0", "RISC0_SERVER_PATH": r0vm, "RISC0_INFO": "1", "RUST_LOG": "info",
           "TMPDIR": f"{work}/tmp"}
    if job in ("attest", "attest-pad"):
        fixture = pathlib.PurePosixPath(manifest["fixtures_from"]).name
        env["FERRATA_EXPECT_CONSUMER_IMAGE_ID"] = manifest["expected_consumer_image_id"]
        env["FERRATA_ATTEST_FIXTURE_DIR"] = f"{work}/{fixture}"
    elif job in ("bridge", "bridge-pad"):
        env["FERRATA_EXPECT_BRIDGE_IMAGE_ID"] = manifest["expected_bridge_image_id"]
    elif spec.get("bundle") == "claim-hops":
        env["FERRATA_EXPECT_CONSUMER_IMAGE_ID"] = manifest["expected_consumer_image_id"]
        env.update({k: f"{work}/{v}" for k, v in manifest["env"].items()})
    pin = ["taskset", "-c", f"0-{affinity - 1}"] if affinity else []
    facts = _facts(declared)
    facts.update({
        "job": job, "tag": tag, "affinity": affinity, "manifest": manifest, "input_hashes_equal_manifest": hashes_ok,
        "r0vm_version": _sh(r0vm, "--version"), "r0vm_sha256": _sha256(r0vm),
        "image_ids": _sh(*pin, f"{work}/{binary}", "--image-ids"),
        "env_set": {k: env[k] for k in ("RISC0_DEV_MODE", "RISC0_INFO", "RUST_LOG") if k in env},
    })
    if affinity:
        facts["nproc_under_affinity"] = _sh(*pin, "nproc")
    json.dump(facts, open(f"{out}/container.json", "w"), indent=1)
    vol.commit()
    records = []
    for i in range(1, runs + 1):
        rd = f"{work}/run-{i}"
        os.makedirs(rd, exist_ok=True)
        sampler = _Sampler(f"{rd}/samples.csv", binary)
        sampler.start()
        rc, wall = _run(["/usr/bin/time", "-v", *pin, f"{work}/{binary}", *spec["prove"]], rd, env,
                        f"{rd}/prove.log", RUN_LIMIT_S)
        sampled = sampler.finish()
        cg_after = _cgroup()
        log = open(f"{rd}/prove.log").read()
        settle_env = {k: v for k, v in env.items() if k not in ("RISC0_INFO", "RUST_LOG")}
        txs = [t for t in spec["txs"] if os.path.exists(f"{rd}/{t}")]
        s_rc, s_wall = (None, None)
        if rc == 0 and len(txs) == len(spec["txs"]):
            s_rc, s_wall = _run([f"{work}/{binary}", "--settle", *txs], rd, settle_env, f"{rd}/settle.log", 1800)
        tamper = []
        if i == 1 and s_rc == 0:
            for k, t in enumerate(txs):
                bad = f"{rd}/tampered-{t}"
                at = _flip(f"{rd}/{t}", bad)
                files = [bad if j == k else f"{rd}/{u}" for j, u in enumerate(txs)]
                t_rc, _ = _run([f"{work}/{binary}", "--settle", *files], rd, settle_env, f"{rd}/tampered-{t}.log", 1800)
                tail = open(f"{rd}/tampered-{t}.log").read()[-400:]
                tamper.append({"tx": t, "flipped_bit0_of_byte": at, "settle_exit": t_rc, "log_tail": tail})
                os.remove(bad)
        rec = {
            "run": i, "exit_code": rc, "wall_s": round(wall, 2), "gnu_time": _gnu_time(log),
            "proof_lines": [l for l in log.splitlines() if "REAL PROOF" in l or "DRY RUN" in l],
            "sampled": sampled, "cgroup_after": cg_after,
            "txs": {t: {"bytes": os.path.getsize(f"{rd}/{t}"), "sha256": _sha256(f"{rd}/{t}")} for t in txs},
            "settle_exit": s_rc, "settle_wall_s": round(s_wall, 2) if s_wall else None,
            "settle_lines": [l for l in open(f"{rd}/settle.log").read().splitlines() if "verified" in l or "settled" in l]
            if s_rc is not None else [],
            "tamper": tamper, "finished_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        }
        json.dump(rec, open(f"{rd}/run.json", "w"), indent=1)
        _copy(rd, f"{out}/run-{i}")
        vol.commit()
        records.append({k: rec[k] for k in ("run", "exit_code", "wall_s", "proof_lines", "settle_exit")}
                       | {"max_rss_kib": rec["gnu_time"].get("max_rss_kib"), "sampled_max": sampled["max"]})
        print(json.dumps(records[-1]), flush=True)
        if rc != 0:
            break
    return {"job": job, "tag": tag, "runs": records, "nproc": facts["nproc"], "models": facts["cpuinfo_model_names"]}


def _bundle(name: str) -> tuple[dict, bool, str]:
    """Copy /vol/bundle/<name> to the container's disk, check it against its MANIFEST.json, install r0vm."""
    src, work = f"/vol/bundle/{name}", f"/root/p4/{name}"
    manifest = json.load(open(f"{src}/MANIFEST.json"))
    hashes_ok = all(_sha256(f"{src}/{p}") == v["sha256"] for p, v in manifest["files"].items())
    _copy(src, work)
    for p in manifest["files"]:
        if "/" not in p and not p.endswith((".bin", ".json")):
            os.chmod(f"{work}/{p}", 0o755)
    return manifest, hashes_ok, work


def _r0vm() -> str:
    r0vm = _copy("/vol/bundle/r0vm/r0vm", "/root/p4/bin/r0vm")
    os.chmod(r0vm, 0o755)
    return r0vm


@app.function(**FUNCTION)
def release_prove(runs: int, tag: str, declared: dict, witness: str = "private-1") -> dict:
    """The release program's succinct proof (`ferrata-e2e prove`, the user's part) and the first step
    of the wrap (`ferrata-e2e p254`: identity_p254 in r0vm, then the Groth16 prover's input.json),
    each timed apart, `runs` times in this container."""
    manifest, hashes_ok, work = _bundle("release")
    r0vm = _r0vm()
    out = f"/vol/results/{tag}/release"
    os.makedirs(out, exist_ok=True)
    os.makedirs(f"{work}/tmp", exist_ok=True)
    env = {**os.environ, "RISC0_DEV_MODE": "0", "RISC0_SERVER_PATH": r0vm, "RISC0_INFO": "1", "RUST_LOG": "info",
           "TMPDIR": f"{work}/tmp"}
    e2e, prog = f"{work}/ferrata-e2e", f"{work}/release.bin"
    inp, meta = f"{work}/witness/release-{witness}.input", f"{work}/witness/release-{witness}.json"
    facts = _facts(declared)
    facts.update({"job": "release", "tag": tag, "witness": witness, "manifest": manifest,
                  "input_hashes_equal_manifest": hashes_ok, "r0vm_version": _sh(r0vm, "--version"),
                  "r0vm_sha256": _sha256(r0vm), "image_id": _sh(e2e, "image-id", prog)})
    json.dump(facts, open(f"{out}/container.json", "w"), indent=1)
    rc, _ = _run([e2e, "exec", "--program", prog, "--input", inp, "--meta", meta], work, env, f"{work}/exec.log", 600)
    _copy(f"{work}/exec.log", f"{out}/exec.log")
    vol.commit()
    records = []
    for i in range(1, runs + 1):
        rd = f"{work}/run-{i}"
        os.makedirs(rd, exist_ok=True)
        rec = {"run": i}
        for step, cmd in (("prove", [e2e, "prove", "--program", prog, "--input", inp, "--meta", meta, "--out-dir", f"{rd}/prove"]),
                          ("p254", [e2e, "p254", "--receipt", f"{rd}/prove/receipt-succinct.bin", "--image-id",
                                    RELEASE_IMAGE_ID, "--out-dir", f"{rd}/wrap"])):
            sampler = _Sampler(f"{rd}/samples-{step}.csv", "ferrata-e2e")
            sampler.start()
            rc, wall = _run(["/usr/bin/time", "-v", *cmd], rd, env, f"{rd}/{step}.log", RUN_LIMIT_S)
            sampled = sampler.finish()
            log = open(f"{rd}/{step}.log").read()
            last = [l for l in log.splitlines() if l.startswith("{")]
            rec[step] = {"exit_code": rc, "wall_s": round(wall, 2), "gnu_time": _gnu_time(log), "sampled": sampled,
                         "record": json.loads(last[-1]) if last else None}
            if rc != 0:
                break
        if rec.get("prove", {}).get("exit_code") == 0:
            v_rc, _ = _run([e2e, "verify", "--receipt", f"{rd}/prove/receipt-succinct.bin", "--image-id", RELEASE_IMAGE_ID,
                            "--meta", meta], rd, env, f"{rd}/verify-succinct.log", 600)
            rec["succinct_verified"] = v_rc == 0
        rec["finished_at_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        json.dump(rec, open(f"{rd}/run.json", "w"), indent=1)
        _copy(rd, f"{out}/run-{i}")
        vol.commit()
        records.append({"run": i, **{k: {kk: rec[k].get(kk) for kk in ("exit_code", "wall_s")} for k in ("prove", "p254") if k in rec},
                        "succinct_verified": rec.get("succinct_verified")})
        print(json.dumps(records[-1]), flush=True)
    return {"job": "release", "tag": tag, "runs": records, "nproc": facts["nproc"]}


@app.function(image=wrap_image, cpu=(CPU, CPU), memory=(MEM_MIB, MEM_MIB), volumes={"/vol": vol}, timeout=3600,
              max_containers=2, single_use_containers=True, retries=0)
def release_wrap(tag: str, run: int, declared: dict) -> dict:
    """The Groth16 step of the wrap: RISC Zero's prover image (/app/prover.sh: /mnt/input.json -> proof.json),
    on the run's input.json, in a container of the same shape."""
    import resource

    vol.reload()  # the input.json release_prove committed
    d = f"/vol/results/{tag}/release/run-{run}/wrap"
    facts = _facts(declared)
    os.makedirs("/mnt", exist_ok=True)
    for stale in ("/mnt/proof.json", "/tmp/output.wtns"):
        if os.path.lexists(stale):
            os.remove(stale)
    _copy(f"{d}/input.json", "/mnt/input.json")
    os.makedirs("/root/p4/wrap", exist_ok=True)
    sampler = _Sampler("/root/p4/wrap/samples-groth16.csv", "prover")
    sampler.start()
    t0 = time.time()
    p = subprocess.run(["bash", "/app/prover.sh"], cwd="/app", capture_output=True)
    seconds = time.time() - t0
    sampled = sampler.finish()
    ru = resource.getrusage(resource.RUSAGE_CHILDREN)
    ok = p.returncode == 0 and os.path.exists("/mnt/proof.json")
    if ok:
        _copy("/mnt/proof.json", f"{d}/proof.json")
    rec = {"run": run, "exit_code": p.returncode, "seconds": round(seconds, 2), "cpu_s": round(ru.ru_utime + ru.ru_stime, 1),
           "max_rss_kib": ru.ru_maxrss, "sampled": sampled, "proof_json": ok, "container": facts,
           "finished_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    open(f"{d}/groth16.log", "wb").write((p.stdout + p.stderr)[-20000:])
    _copy("/root/p4/wrap/samples-groth16.csv", f"{d}/samples-groth16.csv")
    json.dump(rec, open(f"{d}/groth16-run.json", "w"), indent=1)
    vol.commit()
    return {k: rec[k] for k in ("run", "exit_code", "seconds", "cpu_s", "max_rss_kib", "proof_json")} | {
        "nproc": facts["nproc"], "sampled_max": sampled["max"]}


@app.function(image=image, cpu=2.0, memory=4096, volumes={"/vol": vol}, timeout=900, retries=0)
def release_finish(tag: str, run: int, witness: str = "private-1") -> dict:
    """The Groth16 receipt from proof.json (`ferrata-e2e groth16`, verified there), then verified again with the
    native journal (`ferrata-e2e verify`): what the vault's verifier receives (seal, journal)."""
    vol.reload()  # the proof.json release_wrap committed
    _, hashes_ok, work = _bundle("release")
    rd = f"/vol/results/{tag}/release/run-{run}"
    e2e, meta = f"{work}/ferrata-e2e", f"{work}/witness/release-{witness}.json"
    env = {**os.environ, "RISC0_DEV_MODE": "0"}
    g_rc, _ = _run([e2e, "groth16", "--receipt", f"{rd}/prove/receipt-succinct.bin", "--image-id", RELEASE_IMAGE_ID,
                    "--proof", f"{rd}/wrap/proof.json", "--out-dir", f"{rd}/groth16"], work, env, f"{work}/groth16.log", 600)
    v_rc, _ = _run([e2e, "verify", "--receipt", f"{rd}/groth16/receipt-groth16.bin", "--image-id", RELEASE_IMAGE_ID,
                    "--meta", meta], work, env, f"{work}/verify-groth16.log", 600)
    seal = open(f"{rd}/groth16/seal.hex").read().strip() if g_rc == 0 else ""
    journal = open(f"{rd}/groth16/journal.hex").read().strip().removeprefix("0x") if g_rc == 0 else ""
    native = json.load(open(meta))["journal"].removeprefix("0x")
    rec = {"run": run, "inputs_equal_manifest": hashes_ok, "groth16_exit": g_rc, "verify_exit": v_rc,
           "seal_bytes": (len(seal) - 2) // 2 if seal else 0, "selector": seal[:10], "journal_equals_native": journal == native}
    _copy(f"{work}/groth16.log", f"{rd}/groth16.log")
    _copy(f"{work}/verify-groth16.log", f"{rd}/verify-groth16.log")
    json.dump(rec, open(f"{rd}/finish.json", "w"), indent=1)
    vol.commit()
    return rec


def _declared() -> dict:
    return {"modal_cpu_request_and_limit": CPU, "modal_memory_mib_request_and_limit": MEM_MIB}


@app.local_entrypoint()
def probe_main(alloc_gib: int = 0):
    print("P4-PROBE " + json.dumps(probe.remote(_declared(), alloc_gib)), flush=True)
    print("P4-DONE", flush=True)


@app.local_entrypoint()
def release_main(runs: int = 2, tag: str = "minimum"):
    r = release_prove.remote(runs, tag, _declared())
    print("P4-RELEASE-PROVE " + json.dumps(r), flush=True)
    for rec in r["runs"]:
        if rec.get("p254", {}).get("exit_code") != 0:
            continue
        w = release_wrap.remote(tag, rec["run"], _declared())
        print("P4-RELEASE-WRAP " + json.dumps(w), flush=True)
        if w["proof_json"]:
            print("P4-RELEASE-FINISH " + json.dumps(release_finish.remote(tag, rec["run"])), flush=True)
    print("P4-DONE", flush=True)


@app.local_entrypoint()
def main(job: str = "attest", runs: int = 3, tag: str = "minimum", affinity: int = 0):
    r = prove.remote(job, runs, tag, _declared(), affinity)
    print("P4-RESULT " + json.dumps(r), flush=True)
    print("P4-DONE", flush=True)
