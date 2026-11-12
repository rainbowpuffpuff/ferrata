"""Ferrata's reproduction check on Modal, the route without Docker: Modal builds the image of
scripts/reproduce/Dockerfile, this checkout's HEAD goes into a container (git archive: committed files only), and
scripts/reproduce/check.sh runs there with 16 CPUs. Its progress, one line per check and the verdict
print here; its logs come back to target/reproduce/modal-<UTC time>/ (or --out DIR).

    MODAL_PROFILE=<your profile> modal run scripts/reproduce/modal_app.py [--only NAME[,NAME...]] [--out DIR]

The first run builds the image; later runs reuse it. Sent to Modal: the Dockerfile, the committed tree and, if set
here, SEPOLIA_RPC_URL and FERRATA_HARNESS_THREADS (check.sh). Times and cost: scripts/reproduce/README.md.
"""

import os
import pathlib
import subprocess
import sys
import time

import modal

HERE = pathlib.Path(__file__).resolve().parent
SRC = HERE.parent.parent
PASSED_ON = ("SEPOLIA_RPC_URL", "FERRATA_HARNESS_THREADS")

image = modal.Image.from_dockerfile(HERE / "Dockerfile", context_dir=HERE, add_python="3.12")
app = modal.App("ferrata-reproduce")


@app.function(image=image, cpu=(16.0, 32.0), memory=(16384, 65536), timeout=3 * 3600, retries=0, max_containers=1)
def check(tree: bytes, commit: str, only: str, env_in: dict) -> tuple[int, bytes]:
    """Unpack the tree at /work/ferrata, run check.sh there; its exit status and its logs (tar.gz, no binaries)."""
    os.makedirs("/work/ferrata", exist_ok=True)
    os.makedirs("/work/out", exist_ok=True)
    # The image's size on disk (MiB per directory: /usr/local is Modal's Python), measured beside the checks; what they
    # write is left out (the cargo registry, which the image has empty; the tree and the logs are elsewhere).
    du = subprocess.Popen("du -scm --exclude=/home/builder/.cargo/registry --exclude=/home/builder/.cargo/git /usr/local"
                          " /usr /etc /var /opt /home/builder /work/upstream /work/risc0-home-lez"
                          " > /work/out/image-size.txt 2>/dev/null", shell=True)
    subprocess.run(["tar", "-xz", "-C", "/work/ferrata"], input=tree, check=True)
    env = dict(os.environ, FERRATA_COMMIT=commit, **{k: v for k, v in env_in.items() if k in PASSED_ON})
    # Modal's own Python (add_python) is in /usr/local/bin: the scripts get Ubuntu's python3, as in Docker.
    path = env.get("PATH", "").split(":")
    env["PATH"] = ":".join([d for d in path if d != "/usr/local/bin"] + ["/usr/local/bin"])
    cmd = ["bash", "/work/ferrata/scripts/reproduce/check.sh", "--out", "/work/out"] + (["--only", only] if only else [])
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env, text=True, bufsize=1)
    for line in p.stdout:
        print(line, end="", flush=True)
    rc = p.wait()
    du.wait()
    logs = subprocess.run(["tar", "-cz", "--exclude=*.bin", "-C", "/work/out", "."], capture_output=True, check=True)
    return rc, logs.stdout


@app.local_entrypoint()
def main(only: str = "", out: str = ""):
    def git(*args: str) -> bytes:
        return subprocess.run(["git", "-C", str(SRC), *args], capture_output=True, check=True).stdout

    try:
        commit = git("rev-parse", "--verify", "HEAD").decode().strip()
        if git("status", "--porcelain", "--untracked-files=no").strip():
            print(f"note: uncommitted changes are not checked; the container gets HEAD, {commit}")
        tree = git("archive", "--format=tar.gz", "HEAD")
    except (subprocess.CalledProcessError, FileNotFoundError):
        commit = "not a git checkout"
        tree = subprocess.run(["tar", "-cz", "--exclude=target", "--exclude=./vault/lib", "--exclude=./vault/out",
                               "--exclude=./vault/cache", "-C", str(SRC), "."], capture_output=True, check=True).stdout
    print(f"ferrata-reproduce: {commit}, {len(tree) / 2**20:.0f} MiB of tree to the container")
    t0 = time.time()
    rc, logs = check.remote(tree, commit, only, {k: os.environ[k] for k in PASSED_ON if os.environ.get(k)})
    dest = pathlib.Path(out) if out else SRC / "target" / "reproduce" / time.strftime("modal-%Y%m%dT%H%M%SZ", time.gmtime())
    dest.mkdir(parents=True, exist_ok=True)
    subprocess.run(["tar", "-xz", "-C", str(dest)], input=logs, check=True)
    print(f"logs: {dest} ({time.time() - t0:.0f} s from the call to the end)")
    if rc:
        sys.exit(rc)
