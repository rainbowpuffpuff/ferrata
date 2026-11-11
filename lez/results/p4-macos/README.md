# Proving on Apple Silicon: an M1 and an M4 Mac mini, macOS 15.8

RFP-021 P.4 and RFP-022 P.4 ask for the user-side proofs to be measured on a laptop and on the lowest specification
the team declares. The proposal declares 16 GB of memory and 8 cores with 16 threads, x86-64, with Apple Silicon to be
measured in M0; [p4-minimum](../p4-minimum/README.md) measured the x86-64 side in cloud containers held to 8 and 16
CPUs. Here the same three padded transactions (the private attestation, the private claim and the private burn, padded
as LEZ v0.4.0-rc1's wallet pads every private transaction) were built natively on two Apple Silicon Macs and proven
for real (`RISC0_DEV_MODE=0`), two runs each, with `r0vm` 3.0.5 as the prover, and every receipt was checked by
LEZ's settlement. Measured on 11 November 2026, 22:00–@@END@@ UTC, on two Mac minis rented from AWS (EC2 Mac dedicated
hosts), the two machines at the same time, one proof at a time on each.

All figures are **measured** on these two machines unless marked otherwise; every one is in the logs under
[`runs/`](runs/), and [`summarize.py`](summarize.py) recomputes the tables from them (`python3 -I summarize.py`; it
reads the r0vm session log with p4-minimum's parser). The cloud columns are p4-minimum's padded pass, copied.

## Result

@@TABLE1@@

"each run": `execute_and_prove`, as the binaries print it (`REAL PROOF: execute_and_prove … s`); "median" of two is
their mean. "M1 / 16 CPUs": the Mac's median over the cloud median at 16 CPUs (*derived*). Receipts and transactions
have the same sizes as in the cloud: 239,214 / 255,302 bytes (attestation), 239,628 / 256,130 (claim), 239,046 /
254,966 (burn).

@@SUMMARY@@

## Metal: not used, by r0vm 3.0.5's own build

The prover ran on the Macs' CPUs, not on their GPUs. RISC Zero 3.0.5 compiles the Apple branch out of all three of its
provers: in `risc0-circuit-rv32im` 4.0.5 (`src/prove/mod.rs`, lines 45–53), `risc0-circuit-recursion` 4.0.5
(`src/prove/mod.rs`, lines 84–90) and `risc0-circuit-keccak` 4.0.5 (`src/prove/mod.rs`, lines 76–82) the
`cfg(target_os = "macos")` arm that would select `hal::metal` is commented out, so on macOS only `cfg(feature =
"cuda")` or the CPU HAL can be chosen, and the CPU HAL is (crates.io sources as published). `r0vm` still links
`Metal.framework` (`otool -L`, [`runs/m1/r0vm.txt`](runs/m1/r0vm.txt)): `risc0-zkp` 3.0.5 depends on the `metal` crate on
macOS unconditionally. What the machines showed (`sudo powermetrics --samplers gpu_power,cpu_power`, six samples of
10 s from 120 s into run 1 of each job, while LEZ's circuit was being proven: `powermetrics.txt` beside each run):
both CPU clusters at 85–100 % active residency at their top frequencies, the GPU idle (active residency in the table
above, 1.4–3.1 %), and r0vm's log names no GPU device. So these are CPU figures for Apple's cores (with NEON), and the
brief's expectation that "the macOS build uses the Metal GPU" does not hold for r0vm 3.0.5. Enabling Metal would take a
patched RISC Zero, so the proof would no longer be LEZ's pinned prover; not done.

## The machines as measured

| | M1 | M4 |
|---|---|---|
| `sysctl -n machdep.cpu.brand_string hw.ncpu hw.perflevel0.physicalcpu hw.memsize` | Apple M1, 8, 4, 17,179,869,184 | Apple M4, 10, 4, 25,769,803,776 |
| cores | 4 performance + 4 efficiency (`hw.perflevel1.physicalcpu` 4), no SMT (`hw.logicalcpu` 8) | 4 performance + 6 efficiency (`hw.perflevel1.physicalcpu` 6), no SMT (`hw.logicalcpu` 10) |
| model, GPU (`system_profiler SPDisplaysDataType`) | `Macmini9,1`; Apple M1, 8 GPU cores, Metal 3 | `Mac16,10`; Apple M4, 10 GPU cores, Metal 3 |
| `sw_vers` | macOS 15.8, build 24H23 | macOS 15.8, build 24H23 |
| kernel | Darwin 24.6.0, `xnu-11417.140.69.712.69~1/RELEASE_ARM64_T8103` | Darwin 24.6.0, `xnu-11417.140.69.712.69~1/RELEASE_ARM64_T8132` |
| where | EC2 `mac2.metal`, us-east-1a, AMI `amzn-ec2-macos-15.8-20260918-024048-arm64` | EC2 `mac-m4.metal`, us-west-2b, Amazon's macOS 15.8 arm64 AMI of us-west-2 (`ami-01782b03c0702c552`) |
| page size | 16,384 B | 16,384 B |

Both are bare-metal Mac minis on EC2 dedicated hosts: the whole machine, no container and no memory limit but the
machine's own (16 GB on the M1, 24 GB on the M4; macOS itself and the file cache take part of it). The M1 Mac mini is
a reasonable stand-in for the lowest Apple Silicon a user may bring (8 cores, 16 GB, the first generation); the M4 is
the current base Mac mini, the same chip as a current MacBook Air. Neither is a laptop: no battery, no thermal limit of
a laptop chassis (a MacBook Air has no fan and may throttle under minutes of full load; *not measured*).

Toolchains, the same on both (`runs/<mac>/toolchains.log`, `r0vm.txt`, `build-env.txt`): Apple clang 17.0.0
(Command Line Tools 26.3, as the AMI ships them), git 2.50.1; rustup 1.29.3 with Rust 1.98.1 (LEZ's pin; `lez/rust-toolchain.toml`);
rzup 0.6.0 with RISC Zero's Rust 1.91.1 (`rustc 1.91.1-dev (30e2186d0 2025-11-25)`), its C toolchain 2024.1.5 and
`cargo-risczero` / `r0vm` 3.0.5 (`risc0-r0vm 3.0.5`, the aarch64-apple-darwin release rzup installs, sha256
`e57ef8a01d97d1b87ca7f1fb7ab55eb636d478d7e5a8e498bbe985120c3e81f9`).

Sources, the same on both (`runs/<mac>/sources.log`): this repository at `integration6` 0523f77 (`main` on GitHub),
copied as a git bundle; LEZ v0.4.0-rc1 (1806e87) and Logos Blockchain d480efc6 (the revision LEZ pins) cloned from
their public GitHub repositories into `upstream/` beside it, as [`scripts/fetch-upstream.sh`](../../../scripts/fetch-upstream.sh)
lays them out (the harness itself needs no Logos Blockchain crate; `checkpoint/` does). No file was changed.

The build (`cargo build -p harness --locked --bin prove_attest --bin prove_bridge`, debug as on the build box, the
seven guests built by `risc0-build` with RISC Zero's Rust natively): 283.2 s on the M1, 180.2 s on the M4, from
nothing (`runs/<mac>/build.log`). The guests' image ids are the same on both Macs (same build path and toolchain) and
differ from the build box's, as the stage script says they would (their ELFs hold absolute paths): consumer
`6a4b5999…b2b5` (419,696 ELF bytes; the box's `0af07391…2347`, 419,472), attestation program `3d2f597b…e499`
(602,928; `cefed220…ee21`, 602,908), bridge program `af14e3b5…3a9a` (541,732). LEZ's privacy-preserving circuit is
LEZ's prebuilt artifact, the same on every machine; it is 85–96 % of the proving time.

@@RELEASE@@

## Commands

[`pipeline.sh`](pipeline.sh) is the whole run on one Mac, as it ran (as `ec2-user`, from `~/w`, after the bundle was
copied there with `scp`); [`release.sh`](release.sh) ran after it. In short:

```sh
# toolchains
sh rustup-init.sh -y --profile minimal --default-toolchain none && rustup toolchain install 1.98.1 --profile minimal
bash rzup-install.sh                      # https://risczero.com/install: rzup 0.6.0
rzup install rust 1.91.1; rzup install cpp 2024.1.5; rzup install r0vm 3.0.5; rzup install cargo-risczero 3.0.5
# sources
git clone -b integration6 ferrata.bundle ferrata                                   # 0523f77
git clone --depth 1 --branch v0.4.0-rc1 https://github.com/logos-blockchain/logos-execution-zone.git upstream/lez-v0.4.0-rc1
git -C upstream/logos-blockchain-d480efc6 fetch --depth 1 origin d480efc6bbb48fb493c10fe9afefd84c18a04f02   # after git init + remote add
# build, from ferrata/lez
RISC0_BUILD_LOCKED=1 cargo build -p harness --locked --bin prove_attest --bin prove_bridge
# each run (R0VM=~/.risc0/extensions/v3.0.5-cargo-risczero-aarch64-apple-darwin/r0vm), one at a time
RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info RISC0_SERVER_PATH=$R0VM /usr/bin/time -l \
  target/debug/prove_attest --shape wallet --tx-out tx-attest.borsh > prove.log 2>&1
RISC0_DEV_MODE=0 target/debug/prove_attest --settle tx-attest.borsh > settle.log 2>&1        # receipt verified
RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info RISC0_SERVER_PATH=$R0VM /usr/bin/time -l \
  target/debug/prove_bridge --shape wallet --claim-out tx-claim.borsh --burn-out tx-burn.borsh > prove.log 2>&1
RISC0_DEV_MODE=0 target/debug/prove_bridge --settle tx-claim.borsh tx-burn.borsh > settle.log 2>&1
# after run 1: bit 0 of the middle byte of each transaction flipped (minimum.py's _flip), settlement must refuse it
```

Order on each Mac: run 1 of the attestation, run 1 of the claim and burn (one `prove_bridge`), run 2 of each. Beside
every run: `command.txt`, `prove.log` (the binary's output, r0vm's session log and `time -l`), `mem.csv` (every 2 s:
r0vm's RSS, the binary's RSS, `vm_stat`'s free, active, inactive, wired and compressed pages and swapouts),
`settle.log`, `result.txt` (exit codes, wall time), the proven transactions, and for run 1 `powermetrics.txt` and the
tampered settlements' logs. `progress.txt` has every phase's start and end.

## What macOS needed

No source change. The repository, LEZ and RISC Zero built and ran unmodified on macOS 15.8 / arm64. What differed
from the Linux runs is in the scripts only:

- `/usr/bin/time -l` (BSD) instead of GNU `time -v`: it reports the maximum resident set size in bytes, and on macOS
  that includes the children the binary waited for (r0vm), so it matches the sampled r0vm peak;
- the memory sampler reads `ps` and `vm_stat` (no `/proc`, no cgroup on macOS);
- the bit flip is a Perl one-liner (the same byte and bit as `minimum.py`);
- `nohup` refuses to run under `ssh` without a terminal on macOS ("can't detach from console"), so the pipeline was
  started as `(trap "" HUP; exec bash pipeline.sh …) &`.

## Not measured

- A laptop: both machines are Mac minis (no battery, a fan); a fanless MacBook Air may throttle on a 15-minute
  proof.
- The GPU: r0vm 3.0.5 cannot use Metal (above); a later RISC Zero with the Metal provers enabled is not LEZ v0.4.0-rc1's
  pin.
- An 8 GB Mac: the proofs' r0vm peak is 9.3–9.4 GiB, so an 8 GB machine would swap (*derived*; not run).
