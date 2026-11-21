"""RFP-021 P.2: every measured transition proven for real on parallel Modal containers.

The command is bench/perf/prove-actions.sh's, `perf_prove --prove DUMP N` (lez/harness/src/bin/perf_prove.rs: a
composite receipt, verified against the image id with its journal compared to the executor's, compressed to a
succinct receipt, verified again; one RESULT line), with r0vm 3.0.5 as the prover and RISC0_DEV_MODE=0. Each
transition runs in a fresh container held to P2_CPU CPUs and P2_MEM_MIB MiB (request = limit), under GNU time.
Nothing is built here: perf_prove (stripped) and the dump come from bench/perf/p2-stage.sh on the build box, r0vm from
the Volume's /bundle/r0vm/r0vm (the one lez/results/perf-p4-laptop.md's proofs used).

    export MODAL_PROFILE=<workspace>
    modal volume put p4-minimum <stage>/bin /bundle/p2/bin        # and <stage>/dump to /bundle/p2/dump
    P2_CPU=16 modal run --detach bench/perf/prove_modal.py --tag <tag> --index <stage>/dump/index.tsv [--only 3,25]
    modal volume get p4-minimum /results/p2-<tag> bench/perf/results/
    python3 -I bench/perf/summarize_proofs.py bench/perf/results/p2-<tag> > lez/results/perf-proofs.md

Results: /vol/results/p2-<tag>/prove-NNN.log (perf_prove's output, then GNU time's), machine-NNN.json (what the
container reported), index.tsv and machine.txt (written by the local entrypoint). A transition whose log already
holds a RESULT line is skipped on a rerun.
"""

import json
import os
import re
import shutil
import subprocess
import time

import modal

CPU = float(os.environ.get("P2_CPU", "16"))
MEM_MIB = int(os.environ.get("P2_MEM_MIB", "32768"))
CONTAINERS = int(os.environ.get("P2_CONTAINERS", "8"))
VOLUME = "p4-minimum"

app = modal.App("perf-proofs")
vol = modal.Volume.from_name(VOLUME)
image = modal.Image.from_registry("ubuntu:24.04", add_python="3.12").apt_install("time", "procps", "util-linux")


def _read(path: str) -> str:
    try:
        return open(path).read().strip()
    except OSError:
        return ""


def _machine() -> dict:
    cpuinfo = _read("/proc/cpuinfo")
    ids = dict(re.findall(r"^(vendor_id|cpu family|model)\s*:\s*(.*)$", cpuinfo, re.M))
    return {
        "nproc": subprocess.run(["nproc"], capture_output=True, text=True).stdout.strip(),
        "cfs_quota_us": _read("/sys/fs/cgroup/cpu/cpu.cfs_quota_us") or _read("/sys/fs/cgroup/cpu.max"),
        "memory_limit_bytes": _read("/sys/fs/cgroup/memory/memory.limit_in_bytes") or _read("/sys/fs/cgroup/memory.max"),
        "vendor": ids.get("vendor_id"), "family": ids.get("cpu family"), "model": ids.get("model"),
        "cloud": os.environ.get("MODAL_CLOUD_PROVIDER", "").replace("CLOUD_PROVIDER_", ""),
        "region": os.environ.get("MODAL_REGION", ""),
    }


@app.function(image=image, cpu=(CPU, CPU), memory=(MEM_MIB, MEM_MIB), volumes={"/vol": vol}, timeout=3 * 3600,
              max_containers=CONTAINERS, single_use_containers=True, retries=1)
def prove(n: int, tag: str) -> dict:
    out = f"/vol/results/p2-{tag}"
    os.makedirs(out, exist_ok=True)
    log = f"{out}/prove-{n:03d}.log"
    if "\nRESULT\t" in "\n" + _read(log):
        return {"n": n, "skipped": True}
    work = "/root/p2"
    shutil.copytree("/vol/bundle/p2", work, dirs_exist_ok=True)
    r0vm = shutil.copy("/vol/bundle/r0vm/r0vm", f"{work}/bin/r0vm")
    for b in (r0vm, f"{work}/bin/perf_prove"):
        os.chmod(b, 0o755)
    os.makedirs(f"{work}/tmp", exist_ok=True)
    m = _machine()
    json.dump(m, open(f"{out}/machine-{n:03d}.json", "w"), indent=1)
    env = {**os.environ, "RISC0_DEV_MODE": "0", "RISC0_SERVER_PATH": r0vm, "RISC0_INFO": "1", "RUST_LOG": "info",
           "TMPDIR": f"{work}/tmp"}
    t0 = time.time()
    with open(f"{work}/prove.log", "w") as f:
        rc = subprocess.run(["/usr/bin/time", "-v", f"{work}/bin/perf_prove", "--prove", f"{work}/dump", str(n)],
                            stdout=f, stderr=subprocess.STDOUT, env=env, timeout=2 * 3600).returncode
    wall = time.time() - t0
    shutil.copy(f"{work}/prove.log", log)
    vol.commit()
    text = open(log, errors="replace").read()
    res = re.search(r"^RESULT\t.*$", text, re.M)
    return {"n": n, "rc": rc, "wall_s": round(wall, 1), "result": res.group(0) if res else None, "machine": m}


@app.function(image=image, volumes={"/vol": vol}, timeout=600)
def write(tag: str, files: dict) -> None:
    out = f"/vol/results/p2-{tag}"
    os.makedirs(out, exist_ok=True)
    for name, text in files.items():
        open(f"{out}/{name}", "w").write(text)
    vol.commit()


@app.local_entrypoint()
def main(tag: str, index: str, only: str = ""):
    rows = [l.split("\t") for l in open(index).read().splitlines()]
    col = {h: i for i, h in enumerate(rows[0])}
    rows = rows[1:]
    if only:
        keep = {int(x) for x in only.split(",")}
        rows = [r for r in rows if int(r[col["n"]]) in keep]
    # The longest first, so that the containers finish together.
    rows.sort(key=lambda r: -int(r[col["exec_cycles"]]))
    machine = (f"Modal containers, one per transition: {CPU:g} CPUs and {MEM_MIB} MiB each (request = limit), "
               f"at most {CONTAINERS} at a time; r0vm 3.0.5 (CPU); started {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}")
    write.remote(tag, {"index.tsv": open(index).read(), "machine.txt": machine + "\n"})
    for r in prove.starmap([(int(r[col["n"]]), tag) for r in rows], return_exceptions=True):
        print(json.dumps(r if isinstance(r, dict) else {"error": repr(r)}), flush=True)
