"""The lab on Modal: app `ferrata-lab`, a Volume `ferrata-lab`, one scheduled function.

Every two hours `session` restores the zone (a LEZ v0.4.0-rc1 sequencer and its Logos
Blockchain node, with the driver's state) from the Volume, runs `session.sh` (start, catch up,
submit the newest light-client updates to both instances, wait until final, stop cleanly, save a
new generation) and commits the Volume. At most one session runs at a time.

Deploy and operate it with `lab.sh` (never `modal profile activate`; MODAL_PROFILE per command).
"""
import os
import subprocess

import modal

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", ".."))
LEZ = os.path.normpath(os.path.join(REPO, "..", "upstream", "lez-v0.4.0-rc1"))
FIXTURE = "crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10"
SCHEDULE = os.environ.get("LAB_SCHEDULE", "17 */2 * * *")  # minute 17 of every second hour (UTC)

image = (
    modal.Image.from_registry("ubuntu:24.04", add_python="3.12")
    .apt_install("ca-certificates", "curl", "zstd", "procps", "libssl3", "libpcsclite1", "python3", "gzip", "tar")
    .add_local_file(os.path.join(HERE, "session.sh"), "/lab/session.sh")
    .add_local_file(os.path.join(HERE, "fetch.py"), "/lab/fetch.py")
    .add_local_file(os.path.join(LEZ, "lez/sequencer/service/networks/local.json"), "/lab/lez-config/local.json")
    .add_local_file(os.path.join(LEZ, "bedrock/node-config.yaml"), "/lab/lez-config/node-config.yaml")
    .add_local_file(os.path.join(LEZ, "bedrock/deployment-settings.yaml"), "/lab/lez-config/deployment-settings.yaml")
    .add_local_file(os.path.join(REPO, FIXTURE, "bootstrap.ssz"), f"/lab/repo/{FIXTURE}/bootstrap.ssz")
    .add_local_file(os.path.join(REPO, FIXTURE, "SHA256SUMS"), f"/lab/repo/{FIXTURE}/SHA256SUMS")
)

app = modal.App("ferrata-lab")
volume = modal.Volume.from_name("ferrata-lab", create_if_missing=True)


@app.function(
    image=image,
    volumes={"/vol": volume},
    cpu=(2.0, 8.0),
    memory=(2048, 16384),
    timeout=100 * 60,
    max_containers=1,
    schedule=modal.Cron(SCHEDULE),
)
def session(deadline_min: int = 20) -> int:
    volume.reload()
    env = dict(os.environ, LAB_DEADLINE_MIN=str(deadline_min))
    rc = subprocess.run(["bash", "/lab/session.sh"], env=env).returncode
    volume.commit()
    print(f"session.sh exit {rc}")
    return rc
