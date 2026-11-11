# Proving on a CPU at the declared minimum: 8 CPUs and 16 GiB

RFP-021 P.4 and RFP-022 P.4 ask for the user-side proofs to be measured on a laptop and on the
declared minimum hardware. The proposal declares 16 GB of memory and 8 cores. Until this run the
repository had GPU figures for these proofs (one NVIDIA L4: the private attestation in 16.1 s, the
private claim and burn in 15.3 s and 12.4 s) and CPU figures from the build box only (up to 32
cores). Here the same proofs ran for real (`RISC0_DEV_MODE=0`) on the CPU, three times each, in a
cloud container held to the minimum, with `r0vm` 3.0.5 as the prover. Measured on 11 November 2026.

All figures below are **measured** in that container unless marked otherwise; every one is in the
logs under [`runs/`](runs/), and `summarize.py` recomputes the table from them.

## Result

<!-- TABLE: python3 -I summarize.py, last lines -->

- **Every receipt verified.** After each run the same binary settled its transaction(s) through LEZ's
  private settlement with the receipt verified (`--settle`, "receipt verified", exit 0): 9 of 9
  receipts. After run 1, one flipped bit in the middle of each transaction file (bit 0 of byte
  115,607 of the attestation, 115,975 of the claim, 115,435 of the burn) made settlement refuse it:
  "Invalid privacy preserving execution circuit proof", exit 101 (`run-1/tampered-*.log`).
- **16 GiB is enough.** The largest process, `r0vm`, peaked at 9.21 GiB resident (`/usr/bin/time -v`),
  and the whole container at 9.23 GiB (the cgroup's `memory.usage_in_bytes`, sampled every 0.5 s,
  `run-*/samples.csv`), under the hard limit of 16 GiB; the peak barely moves between the proofs, because it
  is set by the largest segments (2^20 cycles), not by the program.
- **The 8 CPUs are used.** `r0vm` averaged 7.54 to 7.55 busy CPUs over each whole run (GNU time:
  754–755 % CPU; 6,843 s of CPU time for a 908 s attestation run).
- **Where the time goes: LEZ's privacy-preserving circuit.** r0vm logs every session it proves
  (`RISC0_INFO=1`, `RUST_LOG=info`). Each proof is the program's private transitions, each one
  segment, then LEZ's circuit over their receipts, three segments of about 1.9 to 2.0 million user
  cycles; the circuit takes 81 to 96 % of each proof:

<!-- SESSIONS -->

  EVIDENCE row 5's 380,577 user cycles are the consumer's two transitions only (the same here,
  34,337 + 346,240); `lee` does not report the circuit, which r0vm's own log now shows.
- **Against the GPU and the build box** (*derived*, different machines, and for the bridge an earlier
  program): the attestation took 16.1 s on one L4 (EVIDENCE row 5, earlier images), here about 904 s;
  the claim and burn of the earlier bridge program `25dfd9b2…` took 15.3 s and 12.4 s on the L4
  (row 10), the final program's claim and burn 276.8 s and 336.0 s proven and sent by the crown runner
  on the build box (row 31, up to 32 cores, CPU), here about 902 s and 756 s.

## The machine as measured

A Modal container (a gVisor sandbox) of the function `prove` in [`minimum.py`](minimum.py), with
`cpu=(8, 8)` and `memory=(16384, 16384)`: request equal to limit for both, so the 8 CPUs and the
16 GiB are reserved and are also the ceiling. What the container reports (`runs/*/container.json`,
`runs/probe-*.json`):

| | |
|---|---|
| `nproc`, `os.sched_getaffinity`, `/proc/cpuinfo` processors | 8, 8, 8 |
| CPU quota (cgroup v1) | `cpu.cfs_quota_us` 800,000 per `cpu.cfs_period_us` 100,000 = 8 CPUs of time; `cpuset.cpus` 0-7 |
| memory limit (cgroup v1) | `memory.limit_in_bytes` 17,179,869,184 (16 GiB); `MemTotal` 16,777,216 kB; no swap |
| CPU | `model name` "unknown" (gVisor hides it); `vendor_id` AuthenticAMD, `cpu family` 25, `model` 1: AMD's Zen 3 server generation (EPYC "Milan"; *derived* from family and model); flags with `avx2`, `bmi2`, `adx`, `sha_ni`, without `avx512f` |
| where | attestation: GCP, Modal region ap-northeast (`bogomips` 2450.00); bridge: Azure, us-east (`bogomips` 3099.27). Each job ran in its own container, the two at the same time; the three runs of a job in one container, one after the other |
| system | kernel string "4.19.0-gvisor"; Ubuntu 24.04.5 LTS, glibc 2.39 |

The shape was checked, not taken on trust ([`runs/probe-cpu8.json`](runs/probe-cpu8.json),
[`runs/probe-cpu8-alloc18.txt`](runs/probe-cpu8-alloc18.txt)): busy loops in 4, 8 and 16
processes obtained 3.93, 7.83 and 7.02 CPUs, never more than 8; a child that allocated and touched
memory 1 GiB at a time was killed after 15 GiB of 18 (Modal: "Runner killed (SIGKILL), exit code:
137"). With `cpu=4` the same container reports 4 of everything ([`runs/probe-cpu4.json`](runs/probe-cpu4.json)).

What "8 cores" means here: 8 hardware threads' worth of time on server cores. Whether two of them
share a physical core is not visible from inside. A laptop sold with "8 cores" usually has 8
physical cores and 16 threads, often with faster cores, but it may also throttle under a 15-minute
load; these figures are not a laptop's.

## What was proven, with what

- **Sources**: branch `p4-minimum` = `integration5` at `0b135f7` plus this directory; LEZ v0.4.0-rc1
  (`1806e87`). The merged programs, built at this worktree's path. Their image ids
  (`--image-ids`): consumer `3f357bae4537eafd9a922befade5275e7cf3453deb91c93d1698631ff9feee6d`
  (419,456 ELF bytes; the image the attestation discloses), attestation program
  `e517399a94269f1a65b50c53c6187cd630c1f8f6d91f9617619ef94a3cba7177`, bridge
  `1156918fdfee530b4fc9cb1eae2078d161d21cb11e26ffbfc7f75aa427de9773` (541,456 ELF bytes). They differ
  from other worktrees' ids because a guest ELF built without Docker holds its absolute build paths
  (`../../gpu-prover/README.md`); the sources are the same.
- **The proofs**: the commands of the GPU jobs `lez-attest` and `lez-bridge`
  (`../../../gpu-prover/modal_app.py`), with the CPU `r0vm` in place of the CUDA one. `prove_attest`:
  one private attestation on the mainnet fixture `mainnet-attest-2026-11-10` (`../../RESULTS.md` §5).
  `prove_bridge`: the private claim of vault A's leaf 5 (1 ETH) and then that holder's private burn,
  on the state of the tests (`../../BRIDGE-RESULTS.md` §3, §4, §7). Each `execute_and_prove` with
  `ProverOpts::succinct()`; the receipt is the succinct `InnerReceipt`, borsh.
- **Binaries**: `lez/target/debug/prove_attest` (157,634,064 bytes, sha256 `7699c9d2567d…`) and
  `prove_bridge` (158,126,064 bytes, `b92df59c2248…`), built on the build box (Modal, through
  `mrun`): `cargo build -p harness --locked --bin prove_attest --bin prove_bridge` in 5 min 10 s,
  with `RISC0_HOME` the guest toolchain of `../../RESULTS.md` §8 and `RISC0_BUILD_LOCKED=1`. Debug
  host binaries, as for the GPU runs: the proving is done by `r0vm`, a release binary, in its own
  process. Staged with `lez/scripts/stage-prove-attest.sh` and `stage-prove-bridge.sh`
  (manifests in [`runs/build-box.txt`](runs/build-box.txt)), moved to the container through a Modal
  Volume, not rebuilt; each container checked every staged file against its manifest.
- **r0vm**: 3.0.5, the CPU build that RISC Zero ships with `cargo-risczero` 3.0.5 (`rzup`):
  108,998,816 bytes, sha256 `36c016a5bb2ded5bd1f8f92cc487e6ffaeb1e95ec05850c983081a0f716b515b`, the
  same file on the build box and on the laptop it was uploaded from.

The commands, in the order they ran:

```sh
# 1. On the build box, from spike/ferrata-wt-p4/lez (box-build.sh, through spike/modal-build/mrun):
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
cargo build -p harness --locked --bin prove_attest --bin prove_bridge
RISC0_DEV_MODE=1 target/debug/prove_attest --allow-dev-mode --tx-out attest-dry.borsh      # dry runs: the same
RISC0_DEV_MODE=1 target/debug/prove_bridge --allow-dev-mode --claim-out claim-dry.borsh \  # commands, fake
  --burn-out burn-dry.borsh                                                                # receipts, settled
FERRATA_HEAD=0b135f76ebd6e12787ca9ebf1f5b83c53519a69c FERRATA_DIRTY=0 \
  scripts/stage-prove-attest.sh STAGE/attest target/debug/prove_attest
FERRATA_HEAD=0b135f76ebd6e12787ca9ebf1f5b83c53519a69c FERRATA_DIRTY=0 \
  scripts/stage-prove-bridge.sh STAGE/bridge target/debug/prove_bridge

# 2. From the laptop (network only), in lez/results/p4-minimum, MODAL_PROFILE=<workspace>:
modal volume put p4-minimum STAGE/attest /bundle/attest
modal volume put p4-minimum STAGE/bridge /bundle/bridge
modal volume put p4-minimum ~/.risc0/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm /bundle/r0vm/r0vm
P4_CPU=8 P4_MEM_MIB=16384 modal run minimum.py::probe_main                    # runs/probe-cpu8.json
P4_CPU=8 P4_MEM_MIB=16384 modal run minimum.py::probe_main --alloc-gib 18     # runs/probe-cpu8-alloc18.txt
P4_CPU=8 P4_MEM_MIB=16384 modal run --detach minimum.py::main --job attest --runs 3 --tag cpu8-mem16g
P4_CPU=8 P4_MEM_MIB=16384 modal run --detach minimum.py::main --job bridge --runs 3 --tag cpu8-mem16g
modal volume get p4-minimum /results/cpu8-mem16g/attest runs/
modal volume get p4-minimum /results/cpu8-mem16g/bridge runs/
python3 -I summarize.py > summary.json.txt
```

Inside the container, each run is (`minimum.py`, `prove`):

```sh
RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/root/p4/bin/r0vm RISC0_INFO=1 RUST_LOG=info \
FERRATA_EXPECT_CONSUMER_IMAGE_ID=3f357bae…ee6d FERRATA_ATTEST_FIXTURE_DIR=<bundle>/mainnet-attest-2026-11-10 \
  /usr/bin/time -v ./prove_attest --tx-out tx-attest.borsh > prove.log 2>&1
./prove_attest --settle tx-attest.borsh > settle.log 2>&1              # LEZ's settlement, receipt verified

RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/root/p4/bin/r0vm RISC0_INFO=1 RUST_LOG=info \
FERRATA_EXPECT_BRIDGE_IMAGE_ID=1156918f…9773 \
  /usr/bin/time -v ./prove_bridge --claim-out tx-claim.borsh --burn-out tx-burn.borsh > prove.log 2>&1
./prove_bridge --settle tx-claim.borsh tx-burn.borsh > settle.log 2>&1
```

`execute_and_prove` in the table is the binary's own timer around LEZ's `execute_and_prove` (the
proving); the whole run, setup and settlement included, is GNU time's wall clock in `run.json`
(908.4, 908.1 and 906.0 s for the attestation runs, 1,667.6 s and so on for the bridge's claim plus burn).
The binary writes its settlement figures as in the GPU runs (execution gas 421,641 for the
attestation; 551,502 and 518,358 for the final program's claim and burn).

## Repeat measurement 1 on a laptop (x86-64 Linux, CPU only)

The prebuilt binary and its input, as the minimum container ran them: no build, about a minute of
downloads, then the proof. Needs glibc 2.39 or newer (the binary was built on Ubuntu 24.04), about
10 GiB of free memory, 0.5 GB of disk, and the Modal client for the download. **Timing**: in the
container above the proof took 902–905 s and the whole run 906–908 s, so the 15 minutes hold only on
a machine faster than that container; `r0vm` uses every CPU the machine has.

```sh
mkdir -p ~/p4-attest && cd ~/p4-attest
# r0vm 3.0.5 for the CPU: RISC Zero's release, the same file as measured
curl -L https://risczero.com/install | bash
~/.risc0/bin/rzup install cargo-risczero 3.0.5
R0VM=~/.risc0/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm
sha256sum $R0VM          # 36c016a5bb2ded5bd1f8f92cc487e6ffaeb1e95ec05850c983081a0f716b515b
# the prebuilt prove_attest and the fixture it reads (152 MB), from the Volume the container read
MODAL_PROFILE=ws-4 modal volume get p4-minimum /bundle/attest .
cd attest && chmod +x prove_attest
python3 -I -c 'import hashlib,json; m=json.load(open("MANIFEST.json")); print([p for p,v in m["files"].items() if hashlib.sha256(open(p,"rb").read()).hexdigest()!=v["sha256"]] or "all files match MANIFEST.json")'
./prove_attest --image-ids          # consumer image 3f357bae…ee6d
# the proof (real, on the CPU), then LEZ's settlement with the receipt verified
export FERRATA_EXPECT_CONSUMER_IMAGE_ID=3f357bae4537eafd9a922befade5275e7cf3453deb91c93d1698631ff9feee6d \
  FERRATA_ATTEST_FIXTURE_DIR=$PWD/mainnet-attest-2026-11-10 RISC0_DEV_MODE=0 RISC0_SERVER_PATH=$R0VM
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_attest --tx-out tx-attest.borsh > prove.log 2>&1
grep -E 'REAL PROOF|Elapsed|Maximum resident|Percent of CPU' prove.log
./prove_attest --settle tx-attest.borsh | grep 'receipt verified'
python3 -I <repository>/lez/results/p4-minimum/summarize.py --log prove.log   # cycles, segments, time per session
```

To hold a larger machine to the minimum, put
`systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 taskset -c 0-7` in front of
`/usr/bin/time` (8 CPUs, 16 GiB; needs cgroup v2 and a systemd user session). Without it the figure
is the laptop's own, which is what P.4's "on a laptop" asks for.

Checked on the owner's laptop (Fedora, a glibc newer than 2.39), everything but the proof: the
download (16.6 s), the hashes, `--image-ids`, and `--settle` of run 1's transaction from the
container ("receipt verified", 2.1 s wall). The proof itself was not run on the laptop. Without the
Volume, rebuild and restage with step 1 of the commands above (about 5 minutes of build on the
build box; on a laptop, longer): built at another path the image ids differ, so set
`FERRATA_EXPECT_CONSUMER_IMAGE_ID` to what `--image-ids` prints.

## What this does not show

- A laptop. This is a cloud server held to the minimum's numbers; the owner's laptop run (above) is
  the other half of P.4.
- Other memory settings: 16 GiB was enough with r0vm's default segment size (2^20 cycles), so no
  smaller limit was tried. A smaller `segment_limit_po2` would lower the peak and lengthen the proof;
  not measured.
- Variance across machines: three runs in one container per job (spread 2.4 s for the attestation),
  on one CPU type. Two containers ran at the same time; whether they shared a host is not known.

## Files

| Path | What |
|---|---|
| `minimum.py` | The Modal app: the probe and the proving function, with its memory sampler and verification |
| `summarize.py` | The table and the per-session figures from `runs/` (prints JSON, then the table) |
| `summary.json.txt` | Its output for these runs |
| `box-build.sh` | The build-box script (build, dry runs, staging) |
| `runs/build-box.txt` | The build box's toolchains, the build's timing, the image ids, the dry runs and both stage manifests |
| `runs/probe-cpu4.json`, `runs/probe-cpu8.json`, `runs/probe-cpu8-alloc18.txt` | What the container reports at `cpu=4` and `cpu=8`, the CPU test and the memory-limit test |
| `runs/attest/`, `runs/bridge/` | `container.json` (the machine, the manifest, r0vm's hash, the image ids) and per run `prove.log` (the binary's output, r0vm's session log, GNU time), `settle.log`, `run.json`, `samples.csv` (memory every 0.5 s); run 1 also the transactions (`tx-*.borsh`, receipts inside) and the tamper checks |
