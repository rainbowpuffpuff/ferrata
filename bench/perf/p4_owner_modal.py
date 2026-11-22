"""Rehearsal of the owner's laptop step (RFP-021 P.4, RFP-022 P.4) in a container held to a mid-range laptop's shape.

A container held to 8 CPUs and 16 GiB (request = limit) gets the owner's bundle (bench/perf/p4-laptop-build.sh's
stage/: attest-pad/, bridge-pad/, r0vm/r0vm) on its local disk and a copy of the files the owner's commands use
(bench/perf/p4-laptop-owner.sh, bench/perf/summarize_p4.py and the parsers it imports from lez/results/p4-minimum).
An unprivileged user then runs exactly the owner's two commands from that copy:

    bash bench/perf/p4-laptop-owner.sh STAGE OUT
    python3 -I bench/perf/summarize_p4.py --owner OUT

The container's own facts (CPUs, CPU quota, memory limit, CPU model) go beside the records. Usage, from the
repository root:

    export MODAL_PROFILE=<workspace>
    modal volume create p4-owner
    modal volume put p4-owner <bundle>/stage /bundle/stage
    modal run bench/perf/p4_owner_modal.py --tag <tag>
    modal volume get p4-owner /results/<tag> bench/perf/results/p4-owner-rehearsal
"""

import json
import os
import pathlib
import shutil
import subprocess

import modal

CPU = float(os.environ.get("P4_CPU", "8"))  # request = limit
MEM_MIB = int(os.environ.get("P4_MEM_MIB", "16384"))  # request = limit (hard)
HERE = pathlib.Path(__file__).resolve()
REPO = HERE.parents[2] if len(HERE.parents) > 2 else None  # inside the container the module sits at /root
FILES = ["bench/perf/p4-laptop-owner.sh", "bench/perf/summarize_p4.py", "lez/results/p4-minimum/summarize.py",
         "lez/results/p4-minimum/summarize_scale.py"]

app = modal.App("p4-owner")
vol = modal.Volume.from_name("p4-owner", create_if_missing=True)
image = modal.Image.from_registry("ubuntu:24.04", add_python="3.12").apt_install(
    "time", "procps", "util-linux", "ca-certificates"
).run_commands("useradd -m -s /bin/bash owner")
if modal.is_local():
    for f in FILES:
        image = image.add_local_file(REPO / f, f"/opt/ferrata/{f}")


def facts() -> dict:
    def read(p):
        try:
            return pathlib.Path(p).read_text().strip()
        except OSError:
            return None
    cpuinfo = read("/proc/cpuinfo") or ""
    model = next((l.split(":", 1)[1].strip() for l in cpuinfo.splitlines() if l.startswith("model name")), None)
    family = next((l.split(":", 1)[1].strip() for l in cpuinfo.splitlines() if l.startswith("cpu family")), None)
    cpu_model = next((l.split(":", 1)[1].strip() for l in cpuinfo.splitlines() if l.startswith("model\t")), None)
    return {
        "nproc": subprocess.run(["nproc"], capture_output=True, text=True).stdout.strip(),
        "cpu_model_name": model, "cpu_family": family, "cpu_model": cpu_model,
        "cfs_quota_us": read("/sys/fs/cgroup/cpu/cpu.cfs_quota_us"), "cfs_period_us": read("/sys/fs/cgroup/cpu/cpu.cfs_period_us"),
        "cpu_max": read("/sys/fs/cgroup/cpu.max"),
        "memory_limit_bytes": read("/sys/fs/cgroup/memory/memory.limit_in_bytes") or read("/sys/fs/cgroup/memory.max"),
        "meminfo_total": next((l for l in (read("/proc/meminfo") or "").splitlines() if l.startswith("MemTotal")), None),
        "modal_region": os.environ.get("MODAL_REGION"), "modal_cloud_provider": os.environ.get("MODAL_CLOUD_PROVIDER"),
        "requested": {"cpu": CPU, "memory_mib": MEM_MIB},
    }


@app.function(image=image, cpu=(CPU, CPU), memory=(MEM_MIB, MEM_MIB), volumes={"/vol": vol}, timeout=3 * 3600,
              retries=0, single_use_containers=True)
def rehearse(tag: str) -> dict:
    home = pathlib.Path("/home/owner")
    shutil.copytree("/vol/bundle/stage", home / "stage")  # the bundle on local disk, as the owner has it
    shutil.copytree("/opt/ferrata", home / "ferrata")
    subprocess.run(["chown", "-R", "owner:owner", str(home)], check=True)
    before = facts()
    cmd = ("cd ~/ferrata && bash bench/perf/p4-laptop-owner.sh ~/stage ~/out > ~/owner.log 2>&1; echo $? > ~/owner.exit; "
           "python3 -I bench/perf/summarize_p4.py --owner ~/out > ~/summary.md 2> ~/summary.err; echo $? > ~/summary.exit")
    env = {"HOME": str(home), "USER": "owner", "LOGNAME": "owner", "LANG": "C.UTF-8",
           "PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"}
    subprocess.run(["setpriv", "--reuid=owner", "--regid=owner", "--init-groups", "bash", "-c", cmd], env=env,
                   cwd=str(home))
    dest = pathlib.Path("/vol/results") / tag
    shutil.rmtree(dest, ignore_errors=True)
    shutil.copytree(home / "out", dest / "out")
    for f in ("owner.log", "owner.exit", "summary.md", "summary.err", "summary.exit"):
        shutil.copy(home / f, dest / f)
    (dest / "container.json").write_text(json.dumps({"before": before, "uid_of_run": "owner (unprivileged)"}, indent=2) + "\n")
    vol.commit()
    return {"owner_exit": (home / "owner.exit").read_text().strip(), "summary_exit": (home / "summary.exit").read_text().strip(),
            "owner_log_tail": (home / "owner.log").read_text()[-1500:], "summary": (home / "summary.md").read_text()}


@app.local_entrypoint()
def main(tag: str = "rehearsal"):
    r = rehearse.remote(tag)
    print(json.dumps(r, indent=2))
