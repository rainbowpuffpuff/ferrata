# The one-command check under Docker, 12 Nov 2026

`scripts/reproduce/run.sh` (the Docker route) on a fresh x86-64 Linux machine: an AWS EC2 m7i.4xlarge (16 vCPUs, 64 GiB),
Ubuntu 24.04 with Ubuntu's `docker.io` (29.1.3), the repository copied in as a git bundle of commit `bf430cc`. It built
the image from `scripts/reproduce/Dockerfile` and ran `check.sh`: **5 of 5 PASS in 22:16** (19:00-19:22 UTC), no committed
file changed. [`summary.txt`](summary.txt) is its verdict; [`environment.txt`](environment.txt) and
[`packages.txt`](packages.txt) record the toolchains and Ubuntu packages it used; [`verify-sepolia.log`](verify-sepolia.log)
is the Sepolia check (both live runs, read-only); `guest-ids/` holds the rebuilt ids. The other logs (vault, core, LEZ
harness) are not committed; they are what the summary counts.
