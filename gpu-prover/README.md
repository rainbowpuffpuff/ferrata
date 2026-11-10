# modal-prover

RISC Zero 3.0.5 proving of `checkpoint` fixtures, of the LEZ light-client update
(`lez`) and of one private attestation against a light-client record (`lez/RESULTS.md` §5.5), on one
NVIDIA L4 GPU on Modal, in a Modal workspace (app `ferrata-prover`, Volume `ferrata-prover`), with the
receipts verified on the laptop. Measured numbers, the CPU-versus-GPU table, costs, and links to the
documentation consulted are in [RESULTS.md](RESULTS.md).

Prototype evidence for a grant proposal. Nothing here is pushed or published. The only upload is to the
owner's Modal workspace: this directory's `prover/` sources and `modal_app.py`; `inputs/` (two guest
program binaries and four fixtures); `inputs-lez/` (the prebuilt `prove_private` and six
mainnet-p1899 fixtures); and `inputs-lez-attest/` (the prebuilt `prove_attest` and the five files of the
`mainnet-attest-2026-11-10` fixture it reads). No personal data is in any of them.

## What is proven, and with which program

The guests are **not rebuilt** on Modal. The ELFs of the local build embed absolute build paths
(`<home>/.cargo/registry/src/…`, `<home>/.cargo/git/checkouts/…`,
`<project>/…`; `strings` finds 16 in `checkpoint.bin` and 12 in
`segment.bin`). risc0-build 3.0.5 built them without Docker, so a rebuild anywhere else produces
different bytes and different image ids (ferrata-checkpoint README, UNVERIFIED 7). Instead,
`scripts/stage-inputs.sh` copies the program binaries the local host embeds
(`target/riscv-guest/…/checkpoint.bin`, `segment.bin`) and the fixtures into `inputs/`. It records
their sha256 and the image ids the local host reports (`host ids`) in `inputs/MANIFEST.json`. Every
Modal run recomputes the image ids from the uploaded binaries and compares them with the manifest
(`image_ids_equal` in each `run.json`). Each receipt is then verified on the laptop against the local
host's image id.

| guest | image id (local build of `aaa9f0c`, = Modal) |
|---|---|
| checkpoint | `0x95b816ed2614e340c85fed3ba3dad1f70e7dff2da1363406ade87e1333583caf` |
| segment | `0xbdfb94a6b6ea12e996f332b28ce4791dc7362f28da602769192f589a61fee043` |

## Layout

| Path | What |
|---|---|
| `modal_app.py` | The Modal app. The image is CUDA 12.8 devel with Rust 1.89. `build` is CPU only (16 cores, 64 GiB). It compiles three binaries into the Volume's `bin/`: the GPU prover, the same program without CUDA, and `risc0-r0vm` 3.0.5 with its `cuda` feature. `prove` proves one ferrata-checkpoint fixture per container on an L4. `prove_lez` runs one LEZ proof with a prebuilt host binary (`LEZ_JOBS`): `lez-update`, the light-client update, or `lez-attest`, one private attestation. `batch` runs several of these one after another in one GPU container. Every function has a 2 h hard timeout. Entrypoints: `main` (fixtures, one container each), `lez` (`--job lez-update`, the default, or `--job lez-attest`), and `batch_run`. |
| `prover/` | `ferrata-modal`, one small Rust program on `risc0-zkvm =3.0.5`. With `--features cuda` it is the GPU prover (`prove`). Without it, it only computes image ids and verifies receipts (`image-id`, `verify`). The lockfile pins every `risc0-*` crate, `sppark` and `blst` to the versions in the risc0 v3.0.5 tag's lockfile, which is also the lockfile packaged with crates.io `risc0-r0vm` 3.0.5. |
| `scripts/stage-inputs.sh` | Copies the guest programs and fixtures from `checkpoint` (read-only) into `inputs/`, and writes `inputs/MANIFEST.json`. |
| `scripts/stage-lez.sh` | Copies `lez/target/debug/prove_private` and the six `mainnet-p1899` fixtures into `inputs-lez/` (read-only on both repositories), and writes `inputs-lez/MANIFEST.json`. |
| `inputs-lez-attest/` | The input of the private attestation proof, written by `lez/scripts/stage-prove-attest.sh OUTDIR [BINARY]`: the prebuilt `prove_attest` (the consumer and program guests and LEZ's privacy-preserving circuit embedded), the fixture files it reads, and `MANIFEST.json` with their sizes and SHA-256, the two image ids the binary reports and the command. |
| `scripts/fetch.sh [RUN]` | Downloads `results/` and the CUDA-free verifier (`bin/`) from the Volume; with a run name, only `results/<RUN>/`. |
| `scripts/verify.sh` | Local verification of every downloaded receipt; writes `results/verify-local.jsonl`. |
| `scripts/summarize.py` | The CPU-versus-GPU table and the cost estimate, computed from the `run.json` files. |
| `results/gpu-l4/<fixture>/` | `receipt-succinct.bin`, `journal.hex`, `prove.json` (the prover's own timings), `run.json` (everything: GPU, image ids, GNU time, GPU samples summary, binary metadata), `prover.log`, `gpu-samples.csv` (nvidia-smi every 200 ms), `nvidia-smi.txt`. `results/gpu-l4/lez-update/` holds `prove-private.log`, `stage2-private-tx.borsh` and `run.json`. `results/gpu-l4/timings.jsonl` has one JSON line per proof; `results/verify-local.jsonl` has the laptop's verification. `results/gpu-l4-attest/lez-attest/` holds `prove-attest.log`, `stage4-private-attest-tx.borsh`, `run.json`, `gpu-samples.csv` and `nvidia-smi.txt` of the private attestation. |
| `logs/` | Run logs (`run-a-…`, `run-b-…`, and `run-c-lez-attest.log` for the private attestation) and the LEZ local settle checks (`lez-local-settle.log`; `lez-attest-local-settle.log`, and `lez-attest-tampered-settle.log` for the refusal of a transaction with one bit of its receipt flipped). The build logs and the billing reports are not included in this repository. `lock-risc0-versions.txt` lists the pinned versions. |

The scripts are the ones the runs used, with their paths changed to this repository's layout
(`../checkpoint`, `../lez`, `../crates`) and the Modal profile taken from `MODAL_PROFILE`. In this
layout only `scripts/summarize.py` has been run (it reproduces the figures from `results/`); the
four shell scripts were syntax-checked (`bash -n`), not run: the runs here were made from a
working directory in which `checkpoint/` and `lez/` were separate checkouts.

## Run it

Every Modal command takes the workspace profile in its own environment. `modal profile activate` is never
used, because the machine's active profile belongs to another workspace.

```sh
export MODAL_PROFILE=<profile>
scripts/stage-inputs.sh && scripts/stage-lez.sh             # inputs/ and inputs-lez/
modal run --detach modal_app.py::build --what prover       # CPU only, once: GPU prover + verifier -> Volume bin/
modal run --detach modal_app.py::build --what r0vm         # CPU only, once: r0vm 3.0.5 with CUDA (in parallel)
modal run --detach modal_app.py::main --fixtures cp-empty-1 --run gpu-l4 --limit-s 1800
# the rest in one GPU container, in this order, each with its own time limit (what was run):
modal run --detach modal_app.py::batch_run --run gpu-l4 \
  --jobs cp-empty-360=900,cp-pp5-360=1200,lez-update=2700,cp-pp50-360=1800
scripts/fetch.sh && scripts/verify.sh && python3 -I scripts/summarize.py
modal app list                                              # nothing of ours left running

# the private attestation (what was run, in a container of its own):
../lez/scripts/stage-prove-attest.sh $PWD/inputs-lez-attest <prove_attest built on glibc <= 2.39>
modal run --detach modal_app.py::lez --job lez-attest --run gpu-l4-attest --limit-s 900
scripts/fetch.sh gpu-l4-attest                                # the second run, on the later images:
                                                              # --run gpu-l4-attest-2, the same commands
FERRATA_ATTEST_FIXTURE_DIR=inputs-lez-attest/mainnet-attest-2026-11-10 RISC0_DEV_MODE=0 \
  inputs-lez-attest/prove_attest --settle results/gpu-l4-attest/lez-attest/stage4-private-attest-tx.borsh
```

The runs are detached because the first build attempt was lost. Modal stopped the non-detached app
10 minutes in, during the CUDA kernel compilation, logging "local client disconnected"; the client
only reported failed heartbeats afterwards (`logs/build-1.log`). A detached ephemeral app still stops when its last
function returns. Every function runs one input per container (`single_use_containers=True`), with
`max_containers=1`, `retries=0`, no `min_containers` and a hard `timeout` of 2 h. Inside `prove`,
`prove_lez` and `batch`, each job's limit kills it earlier. The CUDA binaries are compiled once into
the Volume, not into the image, so a dropped connection or a failed proof never repeats the 16-core
build. Each build step commits its binaries as soon as they exist and is skipped when they are
already there for the same source, because a preempted container (the second build attempt) is
restarted from the beginning. `batch` exists because an L4 can take half an hour to be scheduled:
proof (a) waited 33 min for a 17 s job. The batch waited once for four.

## How the GPU is used (RISC Zero 3.0.5)

- `risc0-zkvm`'s `cuda` feature (`cuda = ["prove", "risc0-circuit-rv32im/cuda", "risc0-circuit-recursion/cuda",
  "risc0-circuit-keccak/cuda", "risc0-zkp/cuda", "risc0-groth16/cuda"]`) selects the CUDA segment and recursion
  provers at compile time. `segment_prover()` and `recursion_prover()` have no CPU fallback in a
  `cuda` build, and `CudaHal::new` panics without a device. The checkpoint proofs use this route,
  in process. The other route the version supports, an `r0vm` built with its own `cuda` feature
  behind a `client`-only host, is what the LEZ proof uses: `prove_private` is such a host, and
  `RISC0_SERVER_PATH` points it at `bin/r0vm-cuda` (crates.io `risc0-r0vm` 3.0.5, `--features cuda`).
  The `r0vm` release binaries that rzup installs are built without CUDA.
- The program calls what `LocalProver` (`RISC0_PROVER=local`) calls, in two timed steps:
  `ExecutorImpl::from_elf(env, elf)?.run()`, then `get_prover_server(&ProverOpts::succinct())?.prove_session(…)`.
  A session hook times each segment's proof. The rest of `prove_session` is lift, join and the
  final integrity check.
- The kernels are compiled for `sm_89` (`NVCC_APPEND_FLAGS`). With neither `NVCC_APPEND_FLAGS` nor
  `NVCC_PREPEND_FLAGS` set, the build scripts pass `-arch=native`, which fails without a GPU at build
  time.
- Each run records `nvidia-smi` every 200 ms. `run.json` has peak GPU memory, mean utilisation while
  proving, and power.

## Proving another RISC Zero workspace with this app

See the end of [RESULTS.md](RESULTS.md) ("Next: another workspace").

## Receipts and other outputs

All are under 1 MB and are committed:

| file | bytes |
|---|---:|
| `results/gpu-l4/cp-empty-1/receipt-succinct.bin` | 224,314 |
| `results/gpu-l4/cp-empty-360/receipt-succinct.bin` | 224,314 |
| `results/gpu-l4/cp-pp5-360/receipt-succinct.bin` | 224,314 |
| `results/gpu-l4/cp-pp50-360/receipt-succinct.bin` | 224,314 |
| `results/gpu-l4/lez-update/stage2-private-tx.borsh` (LEZ transaction with its 227,080-byte receipt) | 231,034 |
| `results/gpu-l4-attest/lez-attest/stage4-private-attest-tx.borsh` (LEZ transaction with its 227,170-byte receipt) | 231,214 |
| `results/gpu-l4-attest-2/lez-attest/stage4-private-attest-tx.borsh` (the same attestation on the later images) | 231,214 |

Not committed (git-ignored, and none of them a receipt): `bin/` (binaries downloaded from the Volume:
CUDA prover 119 MB, r0vm 137 MB, verifier 3 MB), `inputs/programs`, `inputs/fixtures`, and
`inputs-lez/` and `inputs-lez-attest/` except the manifests (the 136 MB `prove_private`, the 154 MB
`prove_attest` and the fixtures). Their sha256 are in
the manifests, in `logs/bin-meta/*.json` and in each `run.json` (`build_info`).
