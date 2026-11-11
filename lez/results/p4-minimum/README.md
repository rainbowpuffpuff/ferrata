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

| proof | runs | `execute_and_prove`, each run (s) | median (s) | user cycles proven | segments (cycles with padding) | peak memory, GiB: r0vm RSS / container (whole run, max of the runs) | receipt (B) | transaction (B) | receipt verified |
|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| private attestation | 3 | 904.6 / 904.3 / 902.2 | 904.3 | 2,355,665–2,355,666 | 5 (3,276,800) | 9.21 / 9.23 | 227,170 | 231,214 | yes, 3 of 3 |
| private claim | 3 | 902.5 / 905.4 / 909.8 | 905.4 | 2,136,850–2,136,957 | 5 (3,014,656) | 9.22 / 9.22 | 227,538 | 231,950 | yes, 3 of 3 |
| private burn | 3 | 756.3 / 770.0 / 754.2 | 756.3 | 1,913,422–1,913,488 | 4 (2,490,368) | 9.22 / 9.22 | 226,998 | 230,870 | yes, 3 of 3 |

Medians of three runs; `execute_and_prove` is the binary's own timer around LEZ's
`execute_and_prove`, the proving alone. A whole run, setup and LEZ's settlement included, took
908.4, 908.1 and 906.0 s for the attestation and 1,667.6, 1,684.5 and 1,673.3 s for the bridge's
claim plus burn (GNU time, `run.json`). The claim and the burn are one run of `prove_bridge`, so they
share its memory figure. User cycles are given as a range where the runs differ: LEZ's circuit
varies by up to 107 cycles between runs of the same transaction, the programs' transitions do
not.

- **Every receipt verified: 9 of 9.** After each run the same binary settled its transaction(s)
  through LEZ's private settlement with the receipt verified (`--settle`: "receipt verified",
  exit 0). After run 1, one flipped bit in the middle of each transaction file (bit 0 of byte
  115,607 of the attestation, 115,975 of the claim, 115,435 of the burn) made settlement refuse
  it: "Invalid privacy preserving execution circuit proof", exit 101 (`run-1/tampered-*.log`).
- **16 GiB is enough.** The largest process, `r0vm`, peaked at 9.21 to 9.22 GiB resident (9,664,900 KiB
  at most, `/usr/bin/time -v`), and the whole container at 9.23 GiB (9,910,034,432 bytes at most:
  the cgroup's `memory.usage_in_bytes`, sampled every 0.5 s, `run-*/samples.csv`), under the hard
  limit of 16 GiB. The peak is the same within 0.01 GiB for all three proofs; each includes
  segments of 2^20 cycles, r0vm's default segment limit.
- **The 8 CPUs are used.** Each run kept 7.53 to 7.55 CPUs busy on average (GNU time: 753–755 %
  CPU; r0vm's own CPU time 6,832–6,844 s per attestation run, from `/proc` samples).
- **Other figures in the repository**, not comparable one to one (other machines, other state,
  for row 10 an earlier program): one NVIDIA L4 GPU, 16.1 s for the attestation (EVIDENCE row 5,
  earlier images) and 15.3 s and 12.4 s for the claim and burn of the earlier bridge program
  `25dfd9b2…` (row 10); Modal CPUs without a fixed limit, 254.8 s for a private claim on the build
  box (row 11) and 276.8 s and 336.0 s for the final program's claim and burn, proven and sent, in
  the crown rehearsal (row 31).

### Where the time goes: LEZ's privacy-preserving circuit

r0vm logs every session it executes (`RISC0_INFO=1`, `RUST_LOG=info`): the dev-mode executions the
binaries make for their own tables and checks (milliseconds each), and the proven ones, each
followed by a gap of seconds to minutes while it is proven. LEZ's `execute_and_prove` proves each
private transition of the program (`prove(env, program.elf())`), then its
`PRIVACY_PRESERVING_CIRCUIT_ELF` with `ProverOpts::succinct()` over those receipts as assumptions
(`lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs`), so the last proven session
of each proof is LEZ's circuit. `summarize.py` takes the gap after each session as its proving time,
recursion included:

| proof | proven session | user cycles | segments (cycles with padding) | proving time, each run (s) | share of `execute_and_prove` (run 1) |
|---|---|---:|---:|---|---:|
| private attestation | the consumer's private transition 1 | 34,337 | 1 (131,072) | 26.2 / 26.5 / 26.4 | 3 % |
|  | the consumer's private transition 2 | 346,240 | 1 (524,288) | 105.2 / 107.8 / 105.8 | 12 % |
|  | LEZ's privacy-preserving circuit | 1,975,088–1,975,089 | 3 (2,621,440) | 773.0 / 769.9 / 769.9 | 85 % |
| private claim | the bridge program's private transition 1 | 41,160 | 1 (131,072) | 27.7 / 27.7 / 28.5 | 3 % |
|  | the bridge program's private transition 2 | 111,596 | 1 (262,144) | 56.7 / 55.4 / 56.6 | 6 % |
|  | LEZ's privacy-preserving circuit | 1,984,094–1,984,201 | 3 (2,621,440) | 817.5 / 821.8 / 824.2 | 91 % |
| private burn | the bridge program's private transition 1 | 25,377 | 1 (131,072) | 27.6 / 29.0 / 27.6 | 4 % |
|  | LEZ's privacy-preserving circuit | 1,888,045–1,888,111 | 3 (2,359,296) | 728.5 / 740.7 / 726.3 | 96 % |

The programs' own transitions take 4 to 15 % of each proof; LEZ's circuit, three segments of about
1.9 to 2.0 million user cycles, takes 85 to 96 %. EVIDENCE row 5's 380,577 user cycles are the
consumer's two transitions only (the same here, 34,337 + 346,240): `lee` does not report its
circuit, r0vm's log does. The final bridge program's transitions (41,160 + 111,596 user cycles for
the claim, 25,377 for the burn) are close to the earlier program's of BRIDGE-RESULTS §7 (39,314 +
113,417 and 23,799).

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
  ([`gpu-prover/README.md`](../../../gpu-prover/README.md)); the sources are the same.
- **The proofs**: the commands of the GPU jobs `lez-attest` and `lez-bridge`
  (`../../../gpu-prover/modal_app.py`), with the CPU `r0vm` in place of the CUDA one. `prove_attest`:
  one private attestation on the mainnet fixture `mainnet-attest-2026-11-10` (`../../RESULTS.md` §5).
  `prove_bridge`: the private claim of vault A's leaf 5 (1 ETH) and then that holder's private burn,
  on the state of the tests (`../../BRIDGE-RESULTS.md` §3, §4, §7). Each is LEZ's
  `execute_and_prove`; the receipt is the succinct `InnerReceipt` of LEZ's circuit, borsh.
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
# 1. On the build box, from lez/ of this worktree (box-build.sh, through the build box's mrun):
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
cargo build -p harness --locked --bin prove_attest --bin prove_bridge
RISC0_DEV_MODE=1 target/debug/prove_attest --allow-dev-mode --tx-out attest-dry.borsh      # dry runs: the same
RISC0_DEV_MODE=1 target/debug/prove_bridge --allow-dev-mode --claim-out claim-dry.borsh \  # commands, fake
  --burn-out burn-dry.borsh                                                                # receipts, settled
FERRATA_HEAD=0b135f76ebd6e12787ca9ebf1f5b83c53519a69c FERRATA_DIRTY=0 \
  scripts/stage-prove-attest.sh STAGE/attest target/debug/prove_attest
FERRATA_HEAD=0b135f76ebd6e12787ca9ebf1f5b83c53519a69c FERRATA_DIRTY=0 \
  scripts/stage-prove-bridge.sh STAGE/bridge target/debug/prove_bridge

# 2. From the laptop (network only), in lez/results/p4-minimum, with MODAL_PROFILE=ws-4:
modal volume put p4-minimum STAGE/attest /bundle/attest
modal volume put p4-minimum STAGE/bridge /bundle/bridge
modal volume put p4-minimum ~/.risc0/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm /bundle/r0vm/r0vm
P4_CPU=8 P4_MEM_MIB=16384 modal run minimum.py::probe_main                    # runs/probe-cpu8.json
P4_CPU=8 P4_MEM_MIB=16384 modal run minimum.py::probe_main --alloc-gib 18     # runs/probe-cpu8-alloc18.txt
P4_CPU=8 P4_MEM_MIB=16384 modal run --detach minimum.py::main --job attest --runs 3 --tag cpu8-mem16g
P4_CPU=8 P4_MEM_MIB=16384 modal run --detach minimum.py::main --job bridge --runs 3 --tag cpu8-mem16g
modal volume get p4-minimum /results/cpu8-mem16g/attest runs/
modal volume get p4-minimum /results/cpu8-mem16g/bridge runs/
python3 -I summarize.py                     # writes summary.json, prints the two tables
```

Inside the container, each run is (`minimum.py`, `prove`):

```sh
export RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/root/p4/bin/r0vm     # r0vm 3.0.5, CPU, from the Volume
# the attestation
export FERRATA_EXPECT_CONSUMER_IMAGE_ID=3f357bae…ee6d FERRATA_ATTEST_FIXTURE_DIR=<bundle>/mainnet-attest-2026-11-10
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_attest --tx-out tx-attest.borsh > prove.log 2>&1
./prove_attest --settle tx-attest.borsh > settle.log 2>&1                  # LEZ's settlement, receipt verified
# the claim and the burn
export FERRATA_EXPECT_BRIDGE_IMAGE_ID=1156918f…9773
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_bridge --claim-out tx-claim.borsh --burn-out tx-burn.borsh > prove.log 2>&1
./prove_bridge --settle tx-claim.borsh tx-burn.borsh > settle.log 2>&1
```

The settlements charge what the dev-mode runs charge (`settle.log`): execution gas 421,641 for
the attestation, 551,502 and 518,358 for the final program's claim and burn.

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
- Variance across machines: three runs in one container per job (spreads of 2.4, 7.3 and 15.8 s for
  the attestation, the claim and the burn), on one CPU generation. The two containers ran at the
  same time; whether they shared a host is not known.

## Files

| Path | What |
|---|---|
| `minimum.py` | The Modal app: the probe and the proving function, with its memory sampler and verification |
| `summarize.py` | The two tables from `runs/` (writes `summary.json`); `--log FILE` reads one `prove.log` |
| `summary.json` | Its output for these runs: every figure of the tables, per run, with the machine |
| `box-build.sh` | The build-box script (build, dry runs, staging) |
| `runs/build-box.txt` | The build box's toolchains, the build's timing, the image ids, the dry runs and both stage manifests |
| `runs/probe-cpu4.json`, `runs/probe-cpu8.json`, `runs/probe-cpu8-alloc18.txt` | What the container reports at `cpu=4` and `cpu=8`, the CPU test and the memory-limit test |
| `runs/attest/`, `runs/bridge/` | `container.json` (the machine, the manifest, r0vm's hash, the image ids) and per run `prove.log` (the binary's output, r0vm's session log, GNU time), `settle.log`, `run.json`, `samples.csv` (memory every 0.5 s); run 1 also the transactions (`tx-*.borsh`, receipts inside) and the tamper checks |
