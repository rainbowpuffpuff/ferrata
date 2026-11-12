# The quick start in one command

The README's quick start, and the rebuild of every guest program, in one container with every toolchain pinned. It
ends with one line per check and a verdict.

```sh
MODAL_PROFILE=<yours> modal run --detach scripts/reproduce/modal_app.py   # on Modal: the route we have run
scripts/reproduce/run.sh                                                 # with Docker (x86-64 images): not run yet
```

Both build the image of [`Dockerfile`](Dockerfile), copy this checkout's committed `HEAD` (`git archive`; uncommitted
changes are not checked) to `/work/ferrata` beside the upstream sources, and run [`check.sh`](check.sh) there. The
Docker route is that same Dockerfile and that same `check.sh`, which we have built and run on Modal; it has not yet
been run under Docker itself.
`--only NAME[,NAME...]` runs some of the checks. The logs land in `target/reproduce/<UTC time>/` (or `--out DIR`):
one log per check, `summary.txt`, `environment.txt` and `packages.txt` (every version used),
`guest-ids/` (the rebuilt ids) and `tree-changed.txt`. On a machine that already has these toolchains, `check.sh`
runs on its own, on a copy of the checkout: it writes into the tree it runs in.

| Check | What runs (the commands of the README and of [verify.md](../../docs/guides/verify.md)) | It passes when |
|---|---|---|
| `verify-sepolia` | `python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3`, then the same on `e2e/runs/crown-live-1`, read-only calls to a public Sepolia endpoint (a second public one if the first fails; `SEPOLIA_RPC_URL` to use yours) | 0 failed for each run (13 passed each) |
| `vault` | `cd vault`, `forge install --no-git` at [vault/README.md](../../vault/README.md)'s pins, `FOUNDRY_OFFLINE=false forge build`, `forge test` | forge exits 0 (451 passed, 1 skipped) |
| `core` | `scripts/fetch-vectors.sh && cargo test --workspace --locked` | cargo exits 0 (163 passed) |
| `guest-ids` | `scripts/reproduce-image-ids.sh build`, then `compare` | 14 of 14 guests equal [docs/image-ids.md](../../docs/image-ids.md), 6 of 6 crown programs equal [e2e/crown/README.md](../../e2e/crown/README.md) |
| `lez-harness` | `cd lez && RISC0_HOME=../../risc0-home-lez RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1` | cargo exits 0 (56 passed) |

## Run on Modal, 12 November 2026

`MODAL_PROFILE=… modal run --detach scripts/reproduce/modal_app.py` from a checkout of commit 596b158 (the scripts as
here; the commits after it change documents only). Its last lines (`/work/out` is in the container; the logs came
back to the machine that started it):

```text
Ferrata reproduction check: commit 596b158fe90435857cb17d5a468008d6dff9005e
2026-11-12 18:29:16 UTC, 16 CPUs; rust 1.93.0 1.98.1 1.88.0 (default 1.93.0); RISC Zero r0vm 3.0.5, guest rust 1.88.0 (lez: 1.91.1); forge 1.3.6, solc 0.8.26; Python 3.12.3

check           result    time  detail
verify-sepolia  PASS      0:20  crown-live-3: 13 passed, 0 pending, 0 failed (61 RPC calls); crown-live-1: 13 passed, 0 pending, 0 failed (61 RPC calls)
vault           PASS      1:10  451 tests passed, 0 failed, 1 skipped (452 total tests)
core            PASS      1:40  163 passed, 0 failed, 0 ignored in 25 test runs
guest-ids       PASS      8:02  14 of 14 guests equal docs/image-ids.md; 6 of 6 ids of the crown run's programs equal the recorded ones
lez-harness     PASS     21:49  56 passed, 0 failed, 0 ignored in 34 test runs

tree: no committed file changed
VERDICT: PASS, 5 of 5 checks, 33:04 in all
logs: /work/out
```

The image: 10.1 GiB on disk (10,770 MiB measured in the container, of which 451 MiB are Modal's own Python); Modal
built it once, in 233 s, and later runs reuse it (this one did). The run: 16 CPUs and up to 64 GiB, 17:56–18:29 UTC,
1,995 s from the call to the logs back. No committed file changed: the tables the LEZ harness writes in
`lez/results/` came out as committed. Earlier full runs of the same checks, whose Sepolia check was crown-live-1's
alone: commit 03f1a9b, 5 of 5 in 26:36 (a side-branch commit from before the tables were re-recorded, so 21 of them
changed there), and commit 031b8c5, 5 of 5 in 36 minutes, the LEZ harness taking 24 of them; Modal had preempted
that run's first container 20 minutes in and ran the function again from the start, so its call took 57 minutes.
The Docker route, `run.sh`, builds the same Dockerfile and runs the same `check.sh`; it has not yet been run under
Docker itself (no Docker on our machines).

The Modal route needs the `modal` client (`pip install modal`, then `modal setup`) and a Modal account; it sends the
committed tree, about 41 MiB compressed. If the connection drops, Modal stops the run (one of ours ended that way);
`modal run --detach scripts/reproduce/modal_app.py` keeps it going, and `modal app logs` then shows its output.

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
- **The live vaults' programs.** The `guest-ids` check compares with the recorded tables, not with a vault; add
  `--vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` to `scripts/reproduce-image-ids.sh compare` to compare with the
  live vault of the final programs (crown-live-3: 6 of 6, [verify.md](../../docs/guides/verify.md) §2). The first live
  vault, `0xfE3199fa…9E54`, gives 4 of 6: crown-live-1 ran programs built at the maintainer's path before guest builds
  were made path-independent. `verify-sepolia` checks both live runs from their committed records,
  crown-live-3 (the final programs) first, each vault pinned to the image ids it was deployed with.
- **Records that are not committed.** `e2e/crown.sh verify crown-live-1` needs LEZ session files kept on the
  maintainers' Modal volume.
- **The rest of the components table**: the CI round trip (`e2e/ci/run.sh`), `cli/` and its tests, the mini-app,
  `bench/`, fuzzing, Halmos, the privacy suite, the finality experiments, `monitor/`. Their commands are in the
  README's components table; some need tools the image does not have (Qt for the mini-app, for one).
- **Writes.** Nothing is sent to any chain; the only network calls are downloads (crates.io, GitHub, Foundry's and
  Solidity's release servers, Ubuntu's archive) and read-only JSON-RPC.
