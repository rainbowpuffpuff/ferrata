# How CI works

One workflow, [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml), on every push and pull request, on
GitHub's `ubuntu-latest` runners. No job needs a secret, and no job sends anything to a public network: the
network is used only to fetch dependencies (crates, Foundry libraries, RISC Zero's toolchains, the pinned
upstream sources and spec vectors).

Prototype evidence for a grant proposal. **Not audited. Not for production.**

## The jobs

| Job | Runs | Needs | Fails when |
|---|---|---|---|
| `test` | `cargo test --workspace --locked` at the repository root (vectors, Helios oracle, adversarial suites, mainnet and Sepolia replays, nullifier demo), `cargo fmt --check`, `cargo clippy -D warnings`, the `no_std` build of `ferrata-core` for `riscv32im-unknown-none-elf` | Rust stable, the pinned spec vectors (`scripts/fetch-vectors.sh`, cached) | a test, a format or lint finding, or the guest-ISA build fails |
| `vault` | `forge test --no-match-contract PrivacySuite` in `vault/`: the vault's unit, fuzz and invariant suites with the mock verifier, and `test/real/` with RISC Zero's real Groth16 verifier and committed proofs | Foundry 1.3.6, the vault's libraries (cached) | a vault test fails. `PrivacySuite` is left out: it reads the file the LEZ half of the privacy suite writes (`lez/harness/tests/privacy_suite.rs`), and skips without it (it failed instead until 12 November: forge refused the read while `target/privacy/` did not exist) |
| `links` | `scripts/check-links.py` | Python | a relative link in `README.md` or `docs/` does not resolve to a tracked file (the recorded tool reports in `docs/reviews/security/tools-out/` are left out) |
| `witness-independence` | [`scripts/ci/witness-independence.sh`](../../scripts/ci/witness-independence.sh): the bridge's `host invariance 64`; then `cargo test -p ferrata-bridge-core` in `bridge/` | Rust 1.93, RISC Zero's toolchains ([`scripts/ci/install-risc0.sh`](../../scripts/ci/install-risc0.sh)) | the release or claim guest's cycle count (user cycles and segment sizes) differs between any two of 64 random valid witnesses (SPEC-v2 §5.2), or a bridge core test fails |
| `roundtrip` | [`e2e/ci/run.sh`](../../e2e/ci/run.sh): one deposit there and back, every step asserted ([`e2e/ci/README.md`](../../e2e/ci/README.md)); then the `ferrata` tool's own end-to-end test, [`cli/tests/e2e.sh`](../../cli/tests/e2e.sh) (deposits through the binary, scan, claim, eleven burns, a release, recovery from the seed phrase; its checkpoint is a journal the test builds); then the relayer's library tests and `cli/tests/gasdrop.rs` (a real gas-drop receipt, no dev mode) | Rust 1.93 and 1.98.1, RISC Zero's toolchains, Foundry 1.3.6, the upstream sources (`scripts/fetch-upstream.sh`), `libpcsclite-dev` | any step of the round trip, or the build |
| `lez-harness` | [`scripts/ci/lez-harness.sh`](../../scripts/ci/lez-harness.sh): every test file of `lez/harness/tests/` and `lez/updater/tests/` (the attestation and bridge programs through LEZ v0.4.0-rc1's state machine, the update-submitting daemon on recorded data), RISC Zero dev mode; then the privacy suite end to end ([`scripts/privacy/run.sh`](../../scripts/privacy/run.sh)) and its gate ([`scripts/privacy/check-results.py`](../../scripts/privacy/check-results.py)) | Rust 1.98.1, RISC Zero's toolchains, the upstream sources, Foundry 1.3.6 and the vault's libraries | a harness or updater test fails; the privacy gate fails or the published tables in `docs/privacy/generated/` are not what the code produces |
| `monitor` | `cargo test` in `monitor/`, then [`monitor/tests/anvil-e2e.sh`](../../monitor/tests/anvil-e2e.sh): the watchers' rehearsal on a local anvil chain (wrong candidates cancelled in time, a SIGKILL and restart, a lost state file, a watcher without gas, a clean shutdown) | Rust 1.93, Foundry 1.3.6, the vault's libraries | a monitor test or a rehearsal scenario fails |

`witness-independence`, `roundtrip` and `vault` were added on 11 November 2026: the witness-independence check was
a step of `bridge/scripts/check.sh` only, no end-to-end run was in CI, and the vault's suites ran by hand. On the
build box, by the jobs' own commands: `vault` 37 suites, 433 tests passed in 20 s.

### `witness-independence`

The release guest (the burn's proof for the vault) and the claim guest are executed in r0vm 3.0.5 on 64 random
valid witnesses each: the extreme leaf indices (0, 2^32 - 1, 0x55555555, 0xAAAAAAAA), then random indices,
siblings, secrets and public fields. A guest whose cycle count depends on its witness leaks it through the proof's
size and time; a first version of the release guest leaked the popcount of the leaf index this way. Executor only:
no proof is made, and dev mode is refused.

### `lez-harness` and `monitor`

Added on 22 November 2026 (branch `rfp-ci-coverage`): before, the LEZ suites, the watchers' rehearsal and the
privacy suite ran on the build box only, and RFP-022 R.11's check (`scripts/ci/check-row-tests.py --require-ci`)
counted their rows as box-only. Measured on the build box with 4 threads (`taskset -c 0-3`, the hosted runner's
vCPUs), cold, by the jobs' own commands: `lez-harness` built in 622 s and ran the harness's tests in 1,620 s and the
updater's in 1,025 s (target directory 3.6 GB; the tests added since and the privacy suite add a few minutes);
`monitor` 69 s for its tests and 131 s for the rehearsal. The hosted runner is slower than the box's cores: expect
a first `lez-harness` run of one to two hours, inside its 240-minute limit. Its target directory is not cached
(the round trip's caches already use most of GitHub's 10 GB); the toolchains, the upstream sources and the
vault's libraries share the round trip's cache entries.

### `roundtrip`

The vault (v5, rule (B)) on anvil with the vault tests' mock verifier, LEZ v0.4.0-rc1's state transition in the
LEZ harness, the attestation and bridge programs as RISC Zero guests in dev mode, a rule (B) checkpoint made by the
checkpoint guest over blocks that hold the transactions the harness executed, and a release made by the release
guest. The steps, what is real and what stands in: [`e2e/ci/README.md`](../../e2e/ci/README.md).

## Caches

| Cache | Key | Restores |
|---|---|---|
| spec vectors (`test`) | the pinned vector versions | `vectors/v1.7.0-beta.4`, `bls12-381-tests-v0.1.2`, `ethereum-tests-c67e485` |
| RISC Zero toolchains | the versions and `install-risc0.sh` | `~/.risc0` (r0vm, cargo-risczero 3.0.5, guest Rust 1.88.0, C++ 2024.1.5) and `~/risc0-home-lez` (the same with guest Rust 1.91.1) |
| upstream sources | the pinned commits | `../upstream/` (LEZ v0.4.0-rc1 and -rc2, Logos Blockchain d480efc6) |
| vault libraries | the library versions | `vault/lib` (forge-std 1.10.0, OpenZeppelin 5.4.0, risc0-ethereum 3.0.0) |
| cargo (`Swatinem/rust-cache`) | lockfiles, toolchain | `~/.cargo` registry and git, and the dependency artefacts of `bridge/target`, `checkpoint/target`, `cli/target`: the LEZ and Logos Blockchain crates, compiled as path dependencies, and the guests built by risc0-build |

GitHub keeps at most 10 GB of caches per repository and evicts the oldest first.

## What the GitHub runner differs in from the box the job was proven on

The two new jobs were proven by running their scripts on the Modal build box (`spike/modal-build/mrun`), exactly
as the workflow runs them, after the same toolchains were in place; the box has no Docker, and the scripts need none.
Results and timings: `e2e/ci/RESULTS.md`.

| | GitHub `ubuntu-latest` | the build box |
|---|---|---|
| CPU, memory | 4 vCPUs, 16 GB (public repositories) | a Modal container (up to 16 cores and 64 GiB; `nproc` printed 2 during these runs) |
| disk | about 14 GB free; the job deletes preinstalled SDKs first | ample |
| toolchains | installed by the workflow (rustup, rzup, foundry-toolchain), restored from caches after the first run | preinstalled at the same versions (Rust 1.93.0 and 1.98.1, r0vm 3.0.5, guest Rust 1.88.0 and 1.91.1, Foundry 1.3.6) |
| system packages | `libpcsclite-dev` and `protobuf-compiler` installed by the job | `libpcsclite-dev` installed by the box's setup script |
| paths | `/home/runner/work/…`: guests build independent of the path ([`scripts/guest-rustc`](../../scripts/guest-rustc), [`docs/image-ids.md`](../image-ids.md)), so their image ids are the box's; the round trip deploys the vault with the ids of the guests it built | the worktree's absolute path |
| cold build | every crate built once, then cached; expect a first run several times the box's time | the box's own `target/` directories |

## Not in CI

Real proofs (CPU and GPU), the Groth16 wrap (Docker), runs against Sepolia or a fork of it (the relayer's
`tests/relayer_fork.rs`, `monitor/tests/fork-live-vault.sh`), the benches, fuzzing, mutation testing, the LEZ
harness's feature builds (`residue-experimental`, `bridge-next`, `program-class`), `bridge/scripts/check.sh`'s
other steps and `checkpoint/`'s suites: their READMEs give the commands, and `docs/EVIDENCE.md` where each ran.
Which hard requirement has a test CI runs, and why the others have none: `scripts/ci/row-tests.toml`.
