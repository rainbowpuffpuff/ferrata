# The quick start in one command

The README's quick start, and the rebuild of every guest program, in one container with every toolchain pinned. It
ends with one line per check and a verdict.

```sh
scripts/reproduce/run.sh                                         # with Docker (x86-64 images)
MODAL_PROFILE=<yours> modal run scripts/reproduce/modal_app.py    # or on Modal, without Docker
```

Both build the image of [`Dockerfile`](Dockerfile), copy this checkout's committed `HEAD` (`git archive`; uncommitted
changes are not checked) to `/work/ferrata` beside the upstream sources, and run [`check.sh`](check.sh) there.
`--only NAME[,NAME...]` runs some of the checks. The logs land in `target/reproduce/<UTC time>/` (or `--out DIR`):
one log per check, `summary.txt`, `environment.txt` and `packages.txt` (every version used),
`guest-ids/` (the rebuilt ids) and `tree-changed.txt`. On a machine that already has these toolchains, `check.sh`
runs on its own, on a copy of the checkout: it writes into the tree it runs in.

| Check | What runs (the commands of the README and of [verify.md](../../docs/guides/verify.md)) | It passes when |
|---|---|---|
| `verify-sepolia` | `python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-1`, read-only calls to a public Sepolia endpoint (a second public one if the first fails; `SEPOLIA_RPC_URL` to use yours) | 0 failed (13 passed) |
| `vault` | `cd vault`, `forge install --no-git` at [vault/README.md](../../vault/README.md)'s pins, `FOUNDRY_OFFLINE=false forge build`, `forge test` | forge exits 0 (451 passed, 1 skipped) |
| `core` | `scripts/fetch-vectors.sh && cargo test --workspace --locked` | cargo exits 0 (163 passed) |
| `guest-ids` | `scripts/reproduce-image-ids.sh build`, then `compare` | 14 of 14 guests equal [docs/image-ids.md](../../docs/image-ids.md), 6 of 6 crown programs equal [e2e/crown/README.md](../../e2e/crown/README.md) |
| `lez-harness` | `cd lez && RISC0_HOME=../../risc0-home-lez RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1` | cargo exits 0 (56 passed) |

## Run on Modal, 12 November 2026

`MODAL_PROFILE=… modal run scripts/reproduce/modal_app.py` from a checkout of commit 031b8c5 (the scripts as here,
but for how the two cargo lines count their test runs). Its last lines:

```text
Ferrata reproduction check: commit 031b8c533685da1e36be2fd82645a79f51a7d1ce
2026-11-12 14:27:01 UTC, 16 CPUs; rust 1.93.0 1.98.1 1.88.0 (default 1.93.0); RISC Zero r0vm 3.0.5, guest rust 1.88.0 (lez: 1.91.1); forge 1.3.6, solc 0.8.26; Python 3.12.3

check           result    time  detail
verify-sepolia  PASS      0:05  13 passed, 0 pending, 0 failed (61 RPC calls)
vault           PASS      1:15  451 tests passed, 0 failed, 1 skipped (452 total tests)
core            PASS      1:56  163 passed, 0 failed, 0 ignored (23 test binaries, 2 doc-test runs)
guest-ids       PASS      8:37  14 of 14 guests equal docs/image-ids.md; 6 of 6 ids of the crown run's programs equal the recorded ones
lez-harness     PASS     24:05  56 passed, 0 failed, 0 ignored (33 test binaries, 1 doc-test runs)

tree: 21 committed files rewritten (lez/results/ 21): tree-changed.txt
VERDICT: PASS, 5 of 5 checks, 36:01 in all
logs: /work/out
```

The image: 10.1 GiB on disk (10,770 MiB measured in the container, of which 451 MiB are Modal's own Python); Modal
built it in 233 s, and later runs reuse it. The run: 16 CPUs and up to 64 GiB. Modal preempted the
first container 20 minutes in, during the LEZ harness, and ran the function again from the start, so the call took
57 minutes in all; the 36 minutes above are the attempt that finished (`/work/out` is in the container; the logs
came back to `target/reproduce/`). The 21 rewritten files are the harness's
tables in `lez/results/`, which the committed tables predate (the README's quick start says so). The Docker route,
`run.sh`, builds the same Dockerfile and runs the same `check.sh`; we have not run it (no Docker on our machines).

The Modal route needs the `modal` client (`pip install modal`, then `modal setup`) and a Modal account; it sends the
committed tree, about 41 MiB compressed.

## What is pinned

Checked while the image builds: Ubuntu 24.04 by digest; rustup 1.29.3 by sha256; Rust 1.93.0 (the default),
1.98.1 (`lez/`, `cli/`) and 1.88; rzup 0.5.2 from crates.io with `--locked` (the rzup that risc0-build 3.0.5 links;
RISC Zero's install script fetches whatever rzup is newest, and 0.6.0 came out the day before this image was first
built); RISC Zero 3.0.5 with guest Rust 1.88.0 and C++ 2024.1.5 in `~/.risc0`, and guest Rust 1.91.1 in the LEZ home
`/work/risc0-home-lez`; Foundry v1.3.6 by the sha256 of its release tarball; solc 0.8.26 by the sha256 of
binaries.soliditylang.org's list; LEZ v0.4.0-rc1 (1806e87) and v0.4.0-rc2 (e3edf47) and Logos Blockchain d480efc6 by
commit. Pinned by the tree itself: the crates (every `Cargo.lock`, `--locked`), the test vectors (sha256 in
`scripts/fetch-vectors.sh`), the vault's libraries (release tags). Not pinned: Ubuntu's packages, which are the
archive's of the day the image is built (`packages.txt` lists them; Ubuntu's snapshot archive, which would pin them
by date, served about 60 KB/s when this was written). The image ids do not depend on them: the guests are built by
RISC Zero's toolchains above.

## What it does not check, and why

- **Proofs.** The LEZ harness runs RISC Zero in dev mode: the programs execute and their journals are checked, but
  no receipt is proven or verified. A real proof takes about an hour on a laptop's CPU, and the Groth16 wraps run in
  RISC Zero's own Docker image; [EVIDENCE](../../docs/EVIDENCE.md) gives the runs that made them.
- **The live vault's programs.** `scripts/reproduce-image-ids.sh compare --vault 0xfE3199fa…9E54` gives 4 of 6:
  crown-live-1 ran programs built at the maintainer's path before guest builds were made path-independent
  ([verify.md](../../docs/guides/verify.md) §2). `verify-sepolia` checks that run from its committed records.
- **Records that are not committed.** `e2e/crown.sh verify crown-live-1` needs LEZ session files kept on the
  maintainers' Modal volume.
- **The rest of the components table**: the CI round trip (`e2e/ci/run.sh`), `cli/` and its tests, the mini-app,
  `bench/`, fuzzing, Halmos, the privacy suite, the finality experiments, `monitor/`. Their commands are in the
  README's components table; some need tools the image does not have (Qt for the mini-app, for one).
- **Writes.** Nothing is sent to any chain; the only network calls are downloads (crates.io, GitHub, Foundry's and
  Solidity's release servers, Ubuntu's archive) and read-only JSON-RPC.
