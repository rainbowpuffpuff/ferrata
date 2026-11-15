# Proving on Apple Silicon: an M1 and an M4 Mac mini, macOS 15.8

RFP-021 P.4 and RFP-022 P.4 ask for the user-side proofs to be measured on a laptop and on the lowest specification
the team declares. The proposal declares 16 GB of memory and 8 cores with 16 threads, x86-64, with Apple Silicon to be
measured in M0; [p4-minimum](../p4-minimum/README.md) measured the x86-64 side in cloud containers held to 8 and 16
CPUs. Here the same three padded transactions (the private attestation, the private claim and the private burn, padded
as LEZ v0.4.0-rc1's wallet pads every private transaction) were built natively on two Apple Silicon Macs and proven
for real (`RISC0_DEV_MODE=0`), two runs each, with `r0vm` 3.0.5 as the prover, and every receipt was checked by
LEZ's settlement. Measured on 11 November 2026, 22:00–23:25 UTC, on two Mac minis rented from AWS (EC2 Mac dedicated
hosts), the two machines at the same time, one proof at a time on each.

All figures are **measured** on these two machines unless marked otherwise; every one is in the logs under
[`runs/`](runs/), and [`summarize.py`](summarize.py) recomputes the tables from them (`python3 -I summarize.py`; it
reads the r0vm session log with p4-minimum's parser). The cloud columns are p4-minimum's padded pass, copied.

## Result

| proof | M1 (8 cores, 16 GB): each run (s) | median | M4 (10 cores, 24 GB): each run (s) | median | cloud, 8 CPUs: median | cloud, 16 CPUs: median | M1 / 16 CPUs | M4 / 16 CPUs | user cycles proven (segments) | LEZ's circuit: user cycles (segments) | settled, receipt verified |
|---|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---|
| private attestation | 858.5 / 856.3 | 857.4 | 475.7 / 474.8 | 475.2 | 1236.8 | 633.3 | 1.35× | 0.75× | 3,334,152–3,335,792 (6) | 2,953,575–2,955,215 (4) | 4 of 4 |
| private claim | 819.6 / 817.7 | 818.7 | 453.5 / 453.5 | 453.5 | 1146.0 | 600.7 | 1.36× | 0.75× | 3,123,093–3,124,558 (6) | 2,970,337–2,971,802 (4) | 4 of 4 |
| private burn | 716.8 / 716.2 | 716.5 | 396.3 / 395.9 | 396.1 | 988.9 | 525.9 | 1.36× | 0.75× | 2,901,706–2,902,995 (5) | 2,876,329–2,877,618 (4) | 4 of 4 |

"each run": `execute_and_prove`, as the binaries print it (`REAL PROOF: execute_and_prove … s`); "median" of two is
their mean. "M1 / 16 CPUs": the Mac's median over the cloud median at 16 CPUs (*derived*). Receipts and transactions
have the same sizes as in the cloud: 239,214 / 255,302 bytes (attestation), 239,628 / 256,130 (claim), 239,046 /
254,966 (burn).

- **At the Apple Silicon minimum** (the M1: 8 cores, 4 of them efficiency cores, 16 GB) the slowest proof, the
  attestation, took 14.3 minutes (856.3–858.5 s), the claim 13.6 and the burn 11.9: inside the M0 check's 20 minutes
  and 16 GB. That is 0.69–0.72× the 8-CPU cloud container's time and 1.35–1.36× the 16-CPU one's (*derived*).
- **The M4** (10 cores: 4 performance, 6 efficiency) took 7.9, 7.6 and 6.6 minutes: 0.75× the 16-CPU container (what
  an 8-core x86 laptop's 16 threads give) and 0.38–0.40× the 8-CPU one (*derived*).
- **Memory**: r0vm's peak was 9.26–9.45 GiB (sampled RSS; `time -l` agrees), as in the cloud (9.23–9.25 GiB padded).
  The M1's 16 GB held it: at most 0.04 GiB compressed by macOS and no swap-out in any run (`vm_stat`).
- **Every receipt verified**: 12 of 12 transactions settled from their files by LEZ's settlement in a fresh process
  (`--settle`, "receipt verified"), charged what the cloud's were (execution gas 421,641 / 551,502 / 518,358, storage
  gas 224,063); after run 1 on each Mac one flipped bit in each transaction was refused, 6 of 6 (exit 101, "Invalid
  privacy preserving execution circuit proof").
- **The same work as in the cloud**: user cycles up to each transaction's fresh randomness (LEZ's circuit for the
  attestation: 2,953,575–2,955,215 here, 2,954,451–2,955,964 there), the same segments (the circuit in 4, 3,670,016
  cycles with padding, for the attestation and the claim), the same receipt and transaction sizes. LEZ's circuit is
  89 % (attestation), 93–94 % (claim) and 97–98 % (burn) of the proving time on both Macs.
- **Repeatable**: the two runs of a proof differ by 0.3 % or less on either Mac (bare metal, one proof at a time),
  against up to 10 % between the cloud's containers.

### Each run

| Mac | run | job | proofs: `execute_and_prove` (s) | the sessions proven: user cycles, segments, proving time (s) | `time -l`: wall (s), user (s), max RSS (GiB) | r0vm RSS, sampled peak (GiB) | vm_stat: least free, most compressed (GiB); swapouts | GPU active residency (powermetrics, %) | settle exit | one flipped bit: settle exit |
|---|---:|---|---|---|---|---:|---|---|---:|---|
| m1 | 1 | attest | private attestation: 858.5 | 34,337 / 1 / 17.6, 346,240 / 1 / 73.7, 2,954,202 / 4 / 767.0 | 860.8, 6521, 9.29 | 9.29 | 0.05, 0.04; 0 | 2.60–3.09 (6 samples of 10 s) | 0 | 101 |
| m1 | 1 | bridge | private claim: 819.6; private burn: 716.8 | 41,160 / 1 / 17.3, 111,596 / 1 / 34.9, 2,971,802 / 4 / 767.0; 25,377 / 1 / 17.3, 2,877,618 / 4 / 699.3 | 1541.8, 11797, 9.28 | 9.26 | 0.06, 0.04; 0 | 2.63–3.09 (6 samples of 10 s) | 0 | 101, 101 |
| m1 | 2 | attest | private attestation: 856.3 | 34,337 / 1 / 17.2, 346,240 / 1 / 73.7, 2,955,215 / 4 / 765.3 | 858.7, 6577, 9.29 | 9.27 | 0.06, 0.04; 0 | — | 0 | — |
| m1 | 2 | bridge | private claim: 817.7; private burn: 716.2 | 41,160 / 1 / 17.2, 111,596 / 1 / 35.0, 2,970,337 / 4 / 765.1; 25,377 / 1 / 17.3, 2,876,329 / 4 / 698.7 | 1539.2, 11790, 9.37 | 9.36 | 0.07, 0.04; 0 | — | 0 | — |
| m4 | 1 | attest | private attestation: 475.7 | 34,337 / 1 / 10.0, 346,240 / 1 / 41.4, 2,953,575 / 4 / 424.1 | 477.3, 4424, 9.28 | 9.28 | 0.05, 0.0; 0 | 1.45–1.86 (6 samples of 10 s) | 0 | 101 |
| m4 | 1 | bridge | private claim: 453.5; private burn: 396.3 | 41,160 / 1 / 9.8, 111,596 / 1 / 19.7, 2,971,387 / 4 / 423.8; 25,377 / 1 / 9.8, 2,876,427 / 4 / 386.4 | 853.6, 7957, 9.45 | 9.44 | 0.06, 0.0; 0 | 1.43–1.71 (6 samples of 10 s) | 0 | 101, 101 |
| m4 | 2 | attest | private attestation: 474.8 | 34,337 / 1 / 9.8, 346,240 / 1 / 40.0, 2,954,924 / 4 / 424.9 | 476.4, 4428, 9.31 | 9.3 | 0.53, 0.0; 0 | — | 0 | — |
| m4 | 2 | bridge | private claim: 453.5; private burn: 395.9 | 41,160 / 1 / 9.8, 111,596 / 1 / 19.7, 2,971,394 / 4 / 423.8; 25,377 / 1 / 9.8, 2,877,529 / 4 / 386.0 | 853.1, 7957, 9.44 | 9.43 | 0.38, 0.0; 0 | — | 0 | — |

"the sessions proven": r0vm's sessions in the order proven (the program's transitions, then LEZ's circuit), each as
user cycles / segments / seconds until r0vm's next log line (p4-minimum's parser). `time -l` covers the whole
binary, both proofs for `bridge`; its maximum RSS includes r0vm (a waited-for child). "least free" is `vm_stat`'s free
pages (macOS keeps the rest as cache), "most compressed" its compressor's pages, swapouts the count's growth during the
run. GPU residency: six 10-s samples from 120 s into run 1 (`powermetrics.txt`).

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
above, 1.43–3.09 %), and r0vm's log names no GPU device. Idle, after the runs, the GPUs showed 2.76–2.80 % (M1) and 1.97–2.28 % (M4) (`runs/<mac>/powermetrics-idle.txt`): during the proofs they did no more than idle. `pmset -g therm` recorded no thermal or performance warning on either Mac (`runs/<mac>/pmset-therm.txt`). So these are CPU figures for Apple's cores (with NEON), and the
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
a reasonable stand-in for the lowest Apple Silicon a user may bring (8 cores, 16 GB, the first generation); the M4 is a later generation (2024),
the chip Apple also sells in MacBook Air laptops. Neither is a laptop: no battery, no thermal limit of
a laptop chassis (a MacBook Air has no fan and may throttle under minutes of full load; *not measured*).

Toolchains, the same on both (`runs/<mac>/toolchains.log`, `r0vm.txt`, `build-env.txt`): Apple clang 17.0.0
(Command Line Tools 26.3, as the AMI ships them), git 2.50.1; rustup 1.29.3 with Rust 1.98.1 (LEZ's pin; `lez/rust-toolchain.toml`);
rzup 0.6.0 with RISC Zero's Rust 1.91.1 (`rustc 1.91.1-dev (30e2186d0 2025-11-25)`), its C toolchain 2024.1.5 and
`cargo-risczero` / `r0vm` 3.0.5 (`risc0-r0vm 3.0.5`, the aarch64-apple-darwin release rzup installs, sha256
`e57ef8a01d97d1b87ca7f1fb7ab55eb636d478d7e5a8e498bbe985120c3e81f9`).

Sources, the same on both (`runs/<mac>/sources.log`): this repository at `integration6` 17030d4 (`main` on GitHub),
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

## The release receipt: the holder's proof

The release guest builds natively on macOS: `bridge/methods` (risc0-build, RISC Zero's Rust 1.91.1) built
`release.bin` and `claim.bin` on both Macs ([`runs/m4/release/ids.txt`](runs/m4/release/ids.txt); their image ids
are this path's and this commit's, release `0xa4fc7331…`). What was proven is the program p4-minimum's second pass
proved, the crown's release program (`release.bin`, 230,480 bytes, sha256 `a6abe992…668e`, image `0x17257f91…f5cd`,
the live run's), on the same witness (`release-private-1`, the crown run's burn private-1), copied from the build box's
stage: `ferrata-e2e prove` for the succinct receipt, then `ferrata-e2e verify` (the image id, and the journal against
the witness's record), two runs on each Mac.

| Mac | run | succinct proof, `ferrata-e2e prove` (s) | user cycles (segments) | `time -l` wall (s), max RSS (GiB) | r0vm RSS, sampled peak (GiB) | verified |
|---|---:|---:|---:|---|---:|---|
| m1 | 1 | 31.8 | 77,736 (1) | 31.8, 1.62 | 1.62 | yes |
| m1 | 2 | 31.6 | 77,736 (1) | 31.7, 1.63 | 1.61 | yes |
| m4 | 1 | 18.6 | 77,736 (1) | 18.6, 1.63 | 1.61 | yes |
| m4 | 2 | 18.3 | 77,736 (1) | 18.3, 1.63 | 1.61 | yes |

"succinct proof": `ferrata-e2e`'s own `wall_s` (`prove/prove.json`), as in the cloud: 46.0 / 46.0 s at 8 CPUs and
26.4 / 27.0 s at 16 (p4-minimum, the same program and witness, 77,736 user cycles in one segment). The journal's
SHA-256 is the cloud's, `7904cd1a…428e` (`verify-succinct.log`).

`ferrata-e2e` needed one change to build on macOS. Its `risc0-groth16` dependency had the feature `prove` on, which
builds `risc0-sys`'s GPU kernels; on macOS these are Metal kernels, compiled with `xcrun metal`, which comes with full
Xcode and not with the Command Line Tools (`xcrun: error: unable to find utility "metal"`:
[`runs/m4/release/build-default-features-failed.log`](runs/m4/release/build-default-features-failed.log)).
`RISC0_SKIP_BUILD_KERNELS` skips the C++ and CUDA kernels only (`risc0-build-kernel` 2.0.1), so it does not help. The
feature is used by one subcommand, `p254` (`prove::to_json`, the input of the Groth16 prover image: the wrap, which
anyone can run for the holder, on Linux). The commit on this branch puts it behind a cargo feature `wrap`, on by
default ([`e2e-wrap-feature.patch`](e2e-wrap-feature.patch): `bridge/e2e/Cargo.toml`, `bridge/e2e/src/main.rs`), so
other builds are unchanged, and the Macs built `cargo build --release --locked -p methods`, then `-p ferrata-e2e
--no-default-features` (`release.sh`). With `wrap`, the code is what it was, the call moved into a function; that
build was not compiled here (on macOS it needs Xcode, and nothing is built on the laptop): **unchecked**.

## Commands

[`pipeline.sh`](pipeline.sh) is the whole run on one Mac, as it ran (as `ec2-user`, from `~/w`, after the bundle was
copied there with `scp`); [`release.sh`](release.sh) ran after it. In short:

```sh
# toolchains
sh rustup-init.sh -y --profile minimal --default-toolchain none && rustup toolchain install 1.98.1 --profile minimal
bash rzup-install.sh                      # https://risczero.com/install: rzup 0.6.0
rzup install rust 1.91.1; rzup install cpp 2024.1.5; rzup install r0vm 3.0.5; rzup install cargo-risczero 3.0.5
# sources
git clone -b integration6 ferrata.bundle ferrata                                   # 17030d4
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

For the three proofs, no source change: the repository, LEZ and RISC Zero built and ran unmodified on macOS 15.8 /
arm64 with Apple's Command Line Tools. For the release driver `ferrata-e2e`, one (the feature `wrap`, above: full
Xcode's Metal compiler is otherwise needed). The rest differs from the Linux runs in the scripts only:

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
- An 8 GB Mac: the proofs' r0vm peak is 9.3–9.5 GiB, so an 8 GB machine would swap (*derived*; not run).
