# Results: RISC Zero 3.0.5 proving on one NVIDIA L4 (Modal)

Every number below comes from a run on 10 Nov 2026. The files are in `results/gpu-l4/<job>/`
(the private attestation's in `results/gpu-l4-attest/lez-attest/`):
`run.json` holds everything, `prove.json` holds the prover's own timings, plus `prover.log`,
`prove-private.log` or `prove-attest.log`, `gpu-samples.csv` and `nvidia-smi.txt`. `python3 -I scripts/summarize.py`
recomputes the table and the cost from `run.json`. CPU numbers are quoted from
`checkpoint/RESULTS.md` and `lez/RESULTS.md` §3; none was re-measured.

## CPU versus GPU

The laptop is an AMD Ryzen 9 7940HS (8 cores, 16 threads, no NVIDIA GPU) with CPU-only r0vm 3.0.5.
The GPU is one NVIDIA L4 (24 GB) on Modal, with 4 CPU cores and 32 GiB. Every GPU figure below is
measured: `total` is execution plus `prove_session` (every segment, then lift, join and the final
check), the same scope as the CPU's `prove_with_opts(…, succinct)`.

| fixture | user cycles | segments | CPU, laptop | GPU (L4) total | execute | segment proofs (mean each) | lift + join + check | speed-up | receipt | GPU memory peak | host RSS peak | GPU util. while proving |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `cp-empty-1` | 3,705,001 | 4 | 1,157.5 s quiet start / 1,758.9 s loaded (measured, on the previous image `0xbbf58fed…`, 3,704,942 cycles) | **14.0 s** | 0.2 s | 10.5 s (2.64 s) | 3.3 s | 83× | 224,314 B | 11,159 MiB | 0.76 GiB | 71% |
| `cp-empty-360` (idle hour) | 5,992,503 | 7 | ≈ 2,023 s (7 × 289 s, extrapolated) | **24.1 s** | 0.3 s | 17.8 s (2.54 s) | 6.0 s | 84× | 224,314 B | 11,140 MiB | 0.84 GiB | 70% |
| `cp-pp5-360` (5 private txs) | 8,821,992 | 12 | ≈ 3,473 s (12 × 289 s, extrapolated) | **41.4 s** | 0.5 s | 30.4 s (2.53 s) | 10.5 s | 84× | 224,314 B | 11,076 MiB | 1.01 GiB | 68% |
| `cp-pp50-360` (50 private txs) | 37,865,667 | 60 | ≈ 17,363 s = 4.8 h (60 × 289 s, extrapolated) | **219.3 s** | 3.7 s | 159.9 s (2.66 s) | 55.7 s | 79× | 224,314 B | 11,158 MiB | 1.08 GiB | 67% |

- **Per segment.** A po2-20 segment proof takes 2.31–3.35 s on the L4. The first segment of each
  run is slower (2.84–3.18 s, CUDA start-up). The po2-19 tail segment of `cp-pp5-360` takes
  1.35 s. Lift, join and the final check add 0.82–0.93 s per segment. With 60 segments the L4 takes
  3.7 s per segment overall, against 289 s on the laptop.
- **Memory.** GPU memory peaks at 11,076–11,159 MiB (10.8–10.9 GiB) of 23,034 MiB whatever the
  number of segments, because segments are proven one at a time. The host process peaks at
  0.76–1.08 GiB (GNU time), against 9.62 GB for the CPU proof of `cp-empty-1`.
- **CPU.** GNU time reports 99% CPU for the prover process, a single core: 13.2 s of user time for
  `cp-empty-1`. The laptop needed 11,726–13,989 CPU-seconds (798–1,017% CPU on 16 threads) for
  the same 4 segments.
- **The receipt** is 224,314 bytes, as on the CPU.
- **Speed-up** is against the quiet-start CPU proof for `cp-empty-1`, and against the 289 s per
  segment extrapolation for the others. The 50-transaction hour, 4.8 h of extrapolated CPU proving,
  is 3.7 min on one L4: one GPU proves such an hour about 16 times faster than the hour lasts.

## The LEZ light-client update (`lez`, RESULTS.md §3)

Prebuilt `prove_private` (sha256 `f23fba6a…0a26`, from `ferrata-lez` at `9b90c79`), unchanged, with
the documented command line. `RISC0_SERVER_PATH` points at the CUDA build of r0vm 3.0.5. The six
mainnet-p1899 fixtures match their SHA256SUMS.

| | laptop CPU (earlier image `e7cec90e…`, §3) | **NVIDIA L4 (image `cec0bc68…`)** |
|---|---|---|
| image proven | `e7cec90e…` (before the merge) | `cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78` (`FERRATA_EXPECT_IMAGE_ID` checked; "RISC0_DEV_MODE off") |
| private transition | 9,045,235 user cycles, 10 segments, `Halted(0)` | the same: 9,045,235 user cycles, 10 segments, `Halted(0)` |
| `execute_and_prove` | 3,240.5 s | **49.5 s** (prove + settle 49.8 s): 65× |
| whole run (`/usr/bin/time -v`) | 54:04 | **0:55.72**: 58× |
| peak host memory (GNU time max RSS: `prove_private` and the r0vm child it waits for) | 9.18 GiB | **0.81 GiB** (845,032 kB) |
| peak GPU memory | none | **11,300 MiB** (41 MiB before); GPU busy 38.4 s of the 55.7 s, mean utilisation 55% over the whole run |
| receipt | 227,080 bytes; transaction 231,034 bytes | **227,080 bytes**; transaction **231,034 bytes** (sha256 `518f5aa7…39bb`) |
| settlement in the run | receipt verified, execution gas 479,854, storage gas 224,063, base fee 5,631,336 | the same figures; exit 0, i.e. the store and record checks after settling passed |

The two files are in `lez/results/` as `stage2-private-tx.borsh` and
`prove-private.log`. The CPU log is still `prove-private-e7cec90e.log`, untouched. Nothing was
committed there. **Local settle check** (laptop, from `lez/`,
`RISC0_DEV_MODE=0 target/debug/prove_private --settle results/stage2-private-tx.borsh`; full output in
`logs/lez-local-settle.log`): it **passed**, exit 0, 2.9 s wall:

```
program image cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78 (508284 ELF bytes); consumer image aec03bcd…; RISC0_DEV_MODE off
SETTLED FROM results/stage2-private-tx.borsh (receipt verified): receipt 227080 bytes, transaction 231034 bytes, settled in 0.2 s
execution.program_image_claims (1):
  - Disclosed { program: ferrata, image_id: cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78 }
settled: execution gas 479,854 (PRIVATE_VERIFY_GAS 409,764 + public part 70,090 cycles), storage gas 224,063, base fee 5,631,336
```

## The LEZ private attestation (`lez`, RESULTS.md §5.5)

One private attestation: a consumer executed at a private account reads the record of mainnet block
26,384,851 (`header(id, 15624480)`) and verifies against it an ERC-20 `Transfer` log of that block
(token `a0b86991…eb48`, transaction 0, log 0). The proof covers the consumer's two private
transitions and LEZ's privacy-preserving circuit over them; the light-client program's part of the
transaction (`Read`) is public. Prebuilt `prove_attest` (sha256 `69d00216…f64e`, built from the
repository at `d498591`), unchanged, with the documented command line. `RISC0_SERVER_PATH` points at
the CUDA build of r0vm 3.0.5. The staged files match `inputs-lez-attest/MANIFEST.json`
(`input_hashes_equal_manifest: true`). Run `gpu-l4-attest`, in a container of its own.

| | **NVIDIA L4** |
|---|---|
| image proven | consumer `711584ffe09f0b115132e33ce1485666710bc73e696e66fcc54d00aad0db949f` (`FERRATA_EXPECT_CONSUMER_IMAGE_ID` checked; "RISC0_DEV_MODE off"). The light-client program in the same binary is `2f3566d4f460620867a93b557776400b1039142ce92203072170c162ba2ec130`. |
| private transitions | `AttestPrivately` (sends `Read`): 36,126 user cycles, 1 segment (131,072 cycles padded). The record's reply (verifies the fact): 346,210 user cycles, 1 segment (524,288 padded). Both exit 0. |
| `execute_and_prove` | **13.5 s** (prove + settle 13.6 s) |
| whole run (`/usr/bin/time -v`) | **0:16.99** (user 15.21 s, system 1.95 s, 100% of one core) |
| peak host memory (GNU time max RSS) | **0.77 GiB** (810,428 kB) |
| peak GPU memory | **11,296 MiB** (5 MiB before); GPU busy 12.2 s of the 17.0 s, mean utilisation 56% over the whole run |
| receipt | **227,170 bytes**; transaction **231,214 bytes** (sha256 `f89bc6e7eca8db8219e8d4abc2f085861020a5b470cce2c9ca25e0776354b746`) |
| settlement in the run | receipt verified; execution gas 421,642 (`PRIVATE_VERIFY_GAS` 409,764 + public part 11,878 cycles), storage gas 224,063, base fee 5,165,640; exit 0 |
| where | Modal task `ta-01M733QC6TQT81W43J260J3AFR`, AWS `eu-south`, 4 CPU cores and 32 GiB; 18.8 s from the function's start to done; the L4 was scheduled at once (client round trip 29.7 s) |

There is no CPU figure to compare with: this transaction was never proven on a CPU. Against the
light-client update above, the private part is 24 times smaller in user cycles (382,336 against
9,045,235) and the proof 3.7 times faster (13.5 s against 49.5 s): most of the 13.5 s is LEZ's
privacy-preserving circuit and the recursion, which do not shrink with the program.

**Local settle check** (laptop, with the staged binary; full output in
`logs/lez-attest-local-settle.log`): `RISC0_DEV_MODE=0 inputs-lez-attest/prove_attest --settle
results/gpu-l4-attest/lez-attest/stage4-private-attest-tx.borsh` **passed**, exit 0, 2.6 s wall:

```
consumer image 711584ffe09f0b115132e33ce1485666710bc73e696e66fcc54d00aad0db949f (411108 ELF bytes); program image 2f3566d4…c130 (592752 ELF bytes); RISC0_DEV_MODE off
SETTLED FROM results/gpu-l4-attest/lez-attest/stage4-private-attest-tx.borsh (receipt verified): receipt 227170 bytes, transaction 231214 bytes, settled in 0.1 s
execution.program_image_claims (1):
  - Disclosed { program: consumer, image_id: 711584ffe09f0b115132e33ce1485666710bc73e696e66fcc54d00aad0db949f }
settled: execution gas 421,642 (PRIVATE_VERIFY_GAS 409,764 + public part 11,878 cycles), storage gas 224,063, base fee 5,165,640
```

**Negative control** (`logs/lez-attest-tampered-settle.log`): the same transaction with bit 0 of byte
115,607 flipped (inside the receipt) is refused by the same command, exit 101: `settlement accepts
the transaction: "Invalid privacy preserving execution circuit proof"`.

## Was the GPU really used?

Yes, on four counts.

1. **The build has no CPU path.** With `cuda`, the rv32im and recursion provers are selected at
   compile time, and `CudaHal::new` panics without a device.
2. **The device was busy.** `nvidia-smi`, sampled every 200 ms (`gpu-samples.csv`): device memory
   went from 8 MiB idle to about 11,150 MiB while proving; mean utilisation was 67–71% from the end
   of execution to the end of the proof, non-zero in 88% of samples for `cp-empty-1`; power about
   57 W of the L4's 72 W; SM clock 2,040 MHz.
3. **The CPU was not doing the work.** The prover process used one CPU core (99%).
4. **The time.** About 2.6 s per segment against 289 s on the laptop's 16 threads.

The same holds for the LEZ runs: for the update, GPU memory went from 41 to 11,300 MiB and the GPU
was busy for 38 s; for the private attestation, from 5 to 11,296 MiB and 12.2 s.

## Image ids and receipt verification

- **Image ids equal.** On the GPU machine, every run recomputed the image ids from the uploaded
  program binaries: checkpoint `0x95b816ed…3caf`, segment `0xbdfb94a6…e043`. They equal the ids
  the local ferrata-checkpoint host reports (`host ids`, commit `aaa9f0c`); `image_ids_equal: true`
  in all four `run.json`. The input files' sha256 equal `inputs/MANIFEST.json`. The guests were
  not rebuilt remotely (see README for why).
- **Local verification** (`scripts/verify.sh`, `results/verify-local.jsonl`, run on the laptop with
  the CUDA-free verifier). All four receipts are `succinct` with exit `Halted(0)`. Each verifies
  against `0x95b816ed…` (the claim's pre-state digest is that image id). Each 544-byte journal
  equals the native run's journal in `checkpoint/fixtures/<name>.json`. The
  staged programs hash to the local host's ids.
- **Negative controls.** The `cp-empty-1` receipt is refused under the segment image id ("claim
  digest does not match") and with `cp-empty-360`'s journal.
- **First real proof of the final checkpoint image.** These are the first real proofs of
  `0x95b816ed…`. The CPU proofs were of the image before the posting-timeframe amendment.
- **Groth16: not made.** The local Groth16 wrap was dropped from these runs to keep the
  laptop free, and it was not moved to Modal.

## Cost

From `modal billing report --for today --resolution h --show-resources` (the report itself is not included here),
in USD:

| item | app | CPU | memory | L4 | total |
|---|---|---:|---:|---:|---:|
| build attempt 1, stopped (client disconnected) | `ap-Ug0CZa9W3MCRJZQGwDhrgH` | 0.089 | 0.028 | – | 0.117 |
| build attempt 2, preempted, then stopped | `ap-XCCM6PTiuIZBvo9mk7hsJ5` | 0.224 | 0.144 | – | 0.368 |
| build: prover and verifier | `ap-0dbHpU3cDCCCVxJHoXwFwu` | 0.325 | 0.217 | – | 0.542 |
| build: r0vm 3.0.5 with CUDA | `ap-UUepZhOBdGJlIb93iRmZl2` | 0.327 | 0.216 | – | 0.543 |
| proof (a) `cp-empty-1` | `ap-xI851feOdDZZ4TmAIm4Gvp` | 0.001 | 0.001 | 0.004 | 0.007 |
| proofs (b), (c), LEZ, (d) in one container | `ap-BfZY9ulBo0gcF7dv2lLhUt` | 0.019 | 0.026 | 0.081 | 0.126 |
| **total** | | | | | **1.70** |
| later, the LEZ private attestation (the same report, read after the run) | `ap-pU6UFjGYoXx7wYAVCXylkC` | 0.001 | 0.002 | 0.006 | 0.009 |

- **GPU.** 385 L4-seconds billed ($0.085 at $0.000222/s). The functions themselves ran 366 s
  (`summarize.py`), which at $0.000345/s for L4 + 4 cores + 32 GiB comes to $0.13. The rest is
  container start-up.
- **CPU and memory.** The builds dominate: 2 × 25 min on 16 cores and 64 GiB, plus $0.49 for the
  two lost attempts.
- **Queueing.** The 33 min proof (a) waited for an L4 was not billed.
- **Storage.** The Volume `ferrata-prover` stays, holding `bin/` (about 260 MB) and `results/`.
  Delete it with `modal volume delete ferrata-prover` if the binaries are not reused.

## Setup

| | |
|---|---|
| GPU | **NVIDIA L4**, 23,034 MiB, compute capability 8.9, driver 610.57.04 (CUDA 13.3 user-mode driver), Modal `gpu="L4"`: GCP `us-east` for `cp-empty-1`, GCP `eu-west` for the batch of four, AWS `eu-south` for the private attestation (`modal_region` and `modal_cloud_provider` in each `run.json`) |
| container | 4 CPU cores requested (`nproc` 4), 32 GiB memory; one proof at a time, each in its own process |
| image | `nvidia/cuda:12.8.0-devel-ubuntu24.04`: index digest `sha256:9a8fffc32a955361aa66d754e7a0cda2513052eaf5aaf8f3ba69b80578d1c6a9`, amd64 manifest `sha256:813c3b834aaafe2df2891f9a840c412a4df3c073dd9e0ac75aab810ed82a5b48`. Plus Modal's Python 3.12, Ubuntu packages (gcc 13.3.0, clang 18, protobuf-compiler, GNU time) and rustup with Rust 1.89.0. nvcc 12.8 (V12.8.61) with `NVCC_APPEND_FLAGS="--generate-code arch=compute_89,code=sm_89"`. |
| prover | `prover/` (`ferrata-modal`) with `risc0-zkvm =3.0.5`, feature `cuda`, in-process (`LocalProver` path). The `risc0-*` crates, `sppark` 0.1.12 and `blst` 0.3.15 are pinned to the risc0 v3.0.5 tag's lockfile. Binary sha256 `085c9bf990e6b0134dac862ec419184f7ec6dc9f5683d59e14e9604c21185c64`, built in 1,503.7 s on 16 CPU cores (`bin/ferrata-modal-cuda.json`). It needs `libcuda.so.1`. |
| r0vm (LEZ) | crates.io `risc0-r0vm` 3.0.5, `cargo install --locked --features cuda`. Its packaged lockfile has the same risc0 versions. sha256 `781ca94605f6b93ac17306e15a7c63ac35bcaaab1e13892b38ec38221b756fb7`, built in 1,511.5 s. |
| verifier (laptop) | the same `prover/` source without `cuda`: sha256 `404a48553e0c1e00bc7a164dbe5ecb679f67a2bae7a6e60ffa785a979fcfcc6c`. It needs only libc, libm and libgcc_s. Built on Modal, downloaded to `bin/`, run on the laptop. |
| segment size | po2 20 (the default), as on the CPU |

## What was looked up, and where

Read on 10 Nov 2026. The risc0 sources are a shallow clone of the tag `v3.0.5` (commit
`8eb06ab020a92dc5b63ba6dd0836d432aba6d890`, 2 Feb 2026) in `a scratch directory/src-risc0/risc0-v3.0.5`,
plus the crates.io sources of the pinned versions.

| Question | Answer | Source |
|---|---|---|
| How does 3.0.x prove on a GPU? | `risc0-zkvm`'s `cuda` feature: `cuda = ["prove", "risc0-circuit-keccak/cuda", "risc0-circuit-recursion/cuda", "risc0-circuit-rv32im/cuda", "risc0-groth16/cuda", "risc0-zkp/cuda"]`. With it, `segment_prover()` and `recursion_prover()` pick the CUDA HAL at compile time and have no CPU fallback. | [risc0/zkvm/Cargo.toml](https://github.com/risc0/risc0/blob/v3.0.5/risc0/zkvm/Cargo.toml), [circuit/rv32im/src/prove/mod.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/circuit/rv32im/src/prove/mod.rs), [circuit/recursion/src/prove/mod.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/circuit/recursion/src/prove/mod.rs) |
| Which prover does a host get? | `default_prover()`: `RISC0_PROVER` = `local` (in-process, needs `prove`), `ipc` (an `r0vm` child, `RISC0_SERVER_PATH`), `bonsai`, `actor`. Unset: Bonsai if its variables are set, else local if `prove` is enabled, else `ipc`. | [zkvm/src/host/client/prove/mod.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/zkvm/src/host/client/prove/mod.rs) |
| Is the `r0vm` that rzup installs GPU-enabled? | No. The release container builds `cargo-risczero` (which ships `r0vm`) without features, and `cargo-risczero`'s manifest says "cuda and metal are excluded because their build scripts require external tools". `r0vm` has its own `cuda = ["risc0-zkvm/cuda"]`, so the other route (a CUDA `r0vm` behind the unchanged `client`-only host) exists. It was not used here. | [.github/containers/release/Dockerfile](https://github.com/risc0/risc0/blob/v3.0.5/.github/containers/release/Dockerfile), [cargo-risczero/Cargo.toml](https://github.com/risc0/risc0/blob/v3.0.5/risc0/cargo-risczero/Cargo.toml), [r0vm/Cargo.toml](https://github.com/risc0/risc0/blob/v3.0.5/risc0/r0vm/Cargo.toml) |
| What does `LocalProver` do? | `get_prover_server(opts)?.prove_with_ctx(env, ctx, elf)`, which is `ExecutorImpl::from_elf(env, elf)?.run()` and then `prove_session`: every segment, then lift and join to one succinct receipt. | [zkvm/src/host/client/prove/local.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/zkvm/src/host/client/prove/local.rs), [zkvm/src/host/server/prove/prover_impl.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/zkvm/src/host/server/prove/prover_impl.rs) |
| Which CUDA toolkit? | The docs name none ("Running on NVIDIA GPUs requires the CUDA runtime, and building the GPU kernels requires the NVIDIA toolkit"). RISC Zero's GPU prover agent at this tag is built on `nvidia/cuda:12.8.0-devel-ubuntu24.04`, which is the image used here. The release container uses 12.2.2, but only for a CPU build. | [docs: Local Proving (3.0)](https://dev.risczero.com/api/generating-proofs/local-proving), [bento/dockerfiles/agent.dockerfile](https://github.com/risc0/risc0/blob/v3.0.5/bento/dockerfiles/agent.dockerfile) |
| Which GPU architecture flags? | Kernel builds pass `-arch=native` unless `NVCC_APPEND_FLAGS` or `NVCC_PREPEND_FLAGS` is set. `-arch=native` needs a GPU at build time. RISC Zero's CI sets `NVCC_APPEND_FLAGS: -arch=sm_89` for its Ada runner. Bento uses `--generate-code arch=compute_89,code=sm_89` among others, and that is the flag used here. | [build_kernel/src/lib.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/build_kernel/src/lib.rs), [circuit/rv32im-sys/build.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/circuit/rv32im-sys/build.rs), [.github/workflows/main.yml](https://github.com/risc0/risc0/blob/v3.0.5/.github/workflows/main.yml) |
| How much GPU memory? | Not stated in the 3.0 docs. The page points to a datasheet (`benchmarks.risczero.com`, `reports.risczero.com`) whose hosts did not resolve from here. Indirect evidence: RISC Zero's CI runs the CUDA test suite on an RTX 4000 Ada (20 GB, sm_89) and benchmarks on g6e.xlarge (L40S). The L4 (24 GB, sm_89) is at least the CI card. Measured here at po2 20: 11,076–11,300 MiB peak. | [.github/workflows/main.yml](https://github.com/risc0/risc0/blob/v3.0.5/.github/workflows/main.yml) |
| Segment size | `DEFAULT_SEGMENT_LIMIT_PO2 = 20`, the same as the CPU runs (4 segments for `cp-empty-1`). | [circuit/rv32im/src/execute/mod.rs](https://github.com/risc0/risc0/blob/v3.0.5/risc0/circuit/rv32im/src/execute/mod.rs) |
| Host Rust | The tag's `rust-toolchain.toml` says `channel = "1.89"`, so the image uses 1.89.0. | [rust-toolchain.toml](https://github.com/risc0/risc0/blob/v3.0.5/rust-toolchain.toml) |
| Groth16 with `cuda` | `risc0-groth16` 3.0.4 shrink-wraps on the GPU (`src/prove/cuda.rs`, needs the rzup `risc0-groth16` component) instead of Docker. Not used: no Groth16 was made in this task. | crate source `risc0-groth16-3.0.4/src/prove/mod.rs` |
| Modal GPU containers | Driver 610.57.04, CUDA driver API 13.3. Use an `nvidia/cuda:*-devel-*` image with a CUDA version no greater than the host's. | [modal.com/docs/guide/cuda](https://modal.com/docs/guide/cuda) |
| Modal GPU names | `T4`, `L4`, `A10`, `L40S`, `A100`, … There is no separate `A10G` string. | [modal.com/docs/guide/gpu](https://modal.com/docs/guide/gpu) |
| Modal prices | L4 $0.000222/s, A10 $0.000306/s, T4 $0.000164/s. CPU $0.0000131 per physical core per second. Memory $0.00000222 per GiB per second. | [modal.com/pricing](https://modal.com/pricing) |
| Modal image layer caching | A `run_function` layer is rebuilt only when that function's source, its kwargs or the globals it references change. | `modal` 1.5.1, `Image.run_function` docstring |
| Modal preemption | Functions are preemptible by default. A preempted input is restarted on a new container. `nonpreemptible=True` (CPU functions only) bills CPU and memory at 3× the list price. | [modal.com/docs/guide/preemption](https://modal.com/docs/guide/preemption) |
| RISC Zero's own GPU timings | crates.io `risc0-r0vm` 3.0.5 ships `testdata/rtx_4000_ada-20.toml`, a per-task profile for po2 20 on an RTX 4000 Ada. It lists `prove_segment_core` 1.847, `segment_preflight` 0.300, `lift` 0.297 and `join` 0.345. These are the scheduler's relative weights, presumably seconds. The L4 measurement above, 2.3–3.4 s per segment proof including preflight (mean 2.5–2.7 s), is consistent with them. | crate source `risc0-r0vm-3.0.5/testdata/` |

## What failed on the way (and what it cost)

1. **Image build stopped by a client disconnect** (`logs/build-1.log`, app `ap-Ug0CZa9W3MCRJZQGwDhrgH`).
   The CUDA build first ran as an image layer (`Image.run_function`, 16 cores, 32 GiB) under a
   non-detached `modal run`. 6 min into the CUDA kernels, Modal logged "Stopping app - local client
   disconnected. Use `modal run --detach` …" (09:13:32). The client printed nothing until it began
   reporting failed heartbeats 11 minutes later. Billed: $0.117.
2. **Build container preempted** (`logs/build-2.log`, app `ap-XCCM6PTiuIZBvo9mk7hsJ5`). The build
   was moved into a detached CPU function (16 cores, 64 GiB). Modal logged "Container terminated
   due to preemption. Your Function will be restarted with the same input" (09:44:05). The restart
   began from scratch, and I stopped it. Billed: $0.368.
3. **Fix.** The build was split into two idempotent steps (`build --what prover`, `build --what
   r0vm`), each committing its binaries to the Volume as soon as they exist, run in parallel and
   detached. Both completed in 25 min (apps `ap-0dbHpU3cDCCCVxJHoXwFwu`, `ap-UUepZhOBdGJlIb93iRmZl2`).
   Billed: $0.542 + $0.543. The two failures had different causes, so no failure repeated.
4. **Queueing for an L4.** Proof (a) waited 33 min in "waiting to be scheduled on a GPU_L4 worker"
   (not billed) for a 16.6 s function. The remaining proofs therefore ran as one batch in one
   container, each still in its own process.

## Not reproducible, or not comparable

- **Guest image ids are not reproducible elsewhere.** They hold the local build's absolute paths
  (see README). Modal proved the uploaded program binaries, whose sha256 are in
  `inputs/MANIFEST.json`, and the ids matched the local host's in every run (`image_ids_equal`).
  Anyone re-checking needs these binaries or the image ids, not a rebuild.
- **Receipts are not byte-reproducible.** RISC Zero's prover is zero-knowledge (randomised), so a
  second proof of the same input has other bytes. Its claim and journal are the same, and those are
  what was compared.
- **Timings are single runs** on a shared cloud GPU. Each job's GPU numbers start with CUDA
  initialisation: the first segment of each run is 0.3–0.6 s slower. In the batch (`cp-empty-360`,
  `cp-pp5-360`, LEZ, `cp-pp50-360`, in that order, in one container) the GPU was already warm for
  the later jobs.
- **The CPU baselines are not the same program run.** The laptop proved the image before the
  last rule change (`0xbbf58fed…34c0`, 3,704,942 user cycles, same 4 segments) and was partly under
  load. The other CPU figures are ferrata-checkpoint's extrapolation (289 s per segment), not
  measurements. The LEZ CPU figure is for the earlier LEZ image `e7cec90e…`, with the same 9,045,235
  cycles in the private transition.
- **The Modal image is not pinned below the tag level** for its Ubuntu packages and rustup. The
  Rust dependencies are pinned by `prover/Cargo.lock` and by r0vm's packaged lockfile. The built
  binaries' sha256 are recorded.

## Next: another workspace (the LEZ light-client update and the private attestation are done this way above)

What this app needs from a RISC Zero 3.0.x workspace, in order of preference:

1. **A host that talks to r0vm** (risc0-zkvm with only `client`). Ship the host binary, built on
   glibc ≤ 2.39, with its embedded guests, never rebuilt remotely. Ship its input files. Point
   `RISC0_SERVER_PATH` at `bin/r0vm-cuda`, the CUDA build of r0vm 3.0.5 already on the Volume. That
   is `prove_lez`: add a row to `LEZ_JOBS` (the staged directory, the binary, the files it writes)
   and the job's environment, and stage with a script like `scripts/stage-lez.sh`. Nothing is
   rebuilt. The private attestation (`lez-attest`) was added this way.
2. **A guest program and its input stream only.** Stage the `.bin` the host embeds, and an expected
   journal from an executor run, as `inputs/programs/<kind>.bin` and `inputs/fixtures/<name>.json`
   (`{"kind", "journal"}`). `ferrata-modal prove` writes the input as ferrata-checkpoint does: a
   u32 little-endian length, then the bytes. A host that uses `env.write(&value)` (risc0 serde
   words) or several writes needs the exact byte stream dumped by that host and a `--raw` mode.
   **Not built.**
3. **Not built either.** Composition inputs: a guest calling `env::verify` needs the assumption
   receipts, which `prove` would add with `add_assumption`. Groth16 on the GPU: `risc0-groth16`'s
   CUDA shrink-wrap needs rzup's `risc0-groth16` component in the image. Multi-GPU or parallel
   segments: r0vm's actor mode (`RISC0_PROVER=actor`, `RISC0_DEFAULT_PROVER_NUM_GPUS`), or
   ferrata's segment composition across containers. The app keeps `max_containers=1`.
4. **Another risc0 version** needs `prover/Cargo.toml`, its lockfile and `R0VM_VERSION` changed to
   that version, and a rebuild (about 25 min on 16 cores, about $0.55 per binary). Receipts are
   tied to the version's recursion control ids.

Cost of a new proof, from the measurements above: about 2.6 s of L4 per po2-20 segment plus about
0.4 s per lift and join, at $0.000345/s for the L4 with 4 cores and 32 GiB. A 10-segment proof
costs about a cent of GPU. Queueing for an L4 is free but can take half an hour.
