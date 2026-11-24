# How CI works

Four workflows. [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml), on every push, pull request and manual
dispatch (`workflow_dispatch`), runs the fifteen jobs below (the table also lists `lez-full.yml`'s `lez-harness-full`)
on GitHub's `ubuntu-latest` runners with `lez-harness`
split across a 6-shard matrix (20 runner jobs total, none skipped on any trigger; `concurrency` gives every run on
`main` its own `github.run_id` group with `cancel-in-progress: false` so runs on `main` never queue behind or
cancel/replace one another, while cancelling superseded pending and in-progress runs on other refs).
[`.github/workflows/lez-full.yml`](../../.github/workflows/lez-full.yml), daily at 04:17 UTC
(`schedule`, automatically disabled by GitHub after 60 days without repository activity in a public repository) and on
`workflow_dispatch`, runs `lez-harness-full` unsharded once as a backstop; the row checker counts it as manual, not as
push/PR CI. [`macos.yml`](../../.github/workflows/macos.yml), on pushes to `main` and by hand, runs the light crates
a user's machine runs and builds five of the six modules' `.lgx` for macOS (arm64) on `macos-15` (RFP-022 S.2; the
updater's module builds for x86-64 Linux only). [`sequencer.yml`](../../.github/workflows/sequencer.yml) runs the
attestation CLI against a sequencer (`scripts/ci/sequencer-attest.sh`), started by hand only (`workflow_dispatch`):
the row checker counts it as manual, not as CI. No job needs a secret, and no job sends anything to a public network: the
network is used only to fetch dependencies (crates, Foundry libraries, RISC Zero's toolchains, the pinned
upstream sources and spec vectors).

Prototype evidence for a grant proposal. **Not audited. Not for production.**

## The jobs

| Job | Runs | Needs | Fails when |
|---|---|---|---|
| `test` | `cargo test --workspace --locked` at the repository root (vectors, Helios oracle, adversarial suites, mainnet and Sepolia replays, nullifier demo), `cargo fmt --check`, `cargo clippy -D warnings -A clippy::unnecessary_map_or`, `cargo +1.85.0 check --workspace` (the MSRV), the `no_std` build of `ferrata-core` for `riscv32im-unknown-none-elf` | Rust stable and 1.85.0, the pinned spec vectors (`scripts/fetch-vectors.sh`, cached) | a test, a format or lint finding, or the guest-ISA build fails |
| `vault` | `forge test --no-match-contract PrivacySuite` in `vault/`: the vault's unit, fuzz and invariant suites with the mock verifier, and `test/real/` with RISC Zero's real Groth16 verifier and committed proofs | Foundry 1.3.6, the vault's libraries (cached) | a vault test fails. `PrivacySuite` is left out: it reads the file the LEZ half of the privacy suite writes (`lez/harness/tests/privacy_suite.rs`), and skips without it (it failed instead until 12 November: forge refused the read while `target/privacy/` did not exist) |
| `links` | `scripts/check-links.py` over `README.md`, `docs/` and the READMEs the job names; the guide and decision checkers and their tests; the image-id, native-archive, release, vault-library and dependency-policy tools' tests; the performance figures and the live test-network record recomputed from their records (`scripts/test_perf_records.py`, `modules/scripts/test_live_testnet.py`); the row-to-test map's checker and its model's tests (`scripts/ci/`); the LEZ shard coverage guard (`scripts/ci/check-lez-shards.py`) and its tests (`scripts/ci/test_lez_shards.py`); the portable-shell and unaudited-label checks | Python, `yq` if present | a relative link does not resolve to a tracked file (the recorded tool reports in `docs/reviews/security/tools-out/` are left out), or any of those checks or tests fails |
| `deps` | `scripts/deny_all.py --advisories off` over every tracked Cargo workspace with cargo-deny 0.20.2 (its release asset checked by sha256): licences, bans, sources; then `--advisories baseline`, not blocking | Rust 1.93.0 and 1.98.1, the upstream sources | a workspace fails a policy, or the root, `cli` or `lez` is not among those passed |
| `release-publish` | `modules/scripts/test-release-publish.sh`: the owner's publish step with a stub `gh` and no network | bash | the publish step's checks fail |
| `image-ids` | `scripts/test_image_ids.py`: `docs/image-ids.md` names each program once, and `verify-sepolia.py` and `compare_ids.py` resolve the right row | Python | a program is named twice, or a tool resolves the wrong row |
| `idl` | `idl/check.py`: the IDL alone decodes every committed borsh vector of both programs | Python | a vector does not decode as the IDL says |
| `admin` | `node admin/dump.mjs --selftest`: the admin view's self-test | Node | the self-test fails |
| `modules-resolve` | [`scripts/ci/modules-resolve.sh`](../../scripts/ci/modules-resolve.sh): five Logos Core modules' crates (`ferrata_{attest,bridge,attestor,monitor,relayer}_module`) staged as the module builder sees them, with the logos-rust-sdk revision their `flake.lock` pins, then `cargo metadata --locked` in each; then `ferrata_attest_module`'s `calls/tests/failing_endpoint.rs` (RFP-021 R.5); then `cli/`'s library compiled as `ferrata_relayer_module` uses it, without `lez-wallet` (`cargo +1.98.1 check --locked --no-default-features --lib`) | Rust 1.93 and 1.98.1, the upstream sources (`scripts/fetch-upstream.sh`), Python, `libpcsclite-dev` and `protoc` | a module's staged crate does not resolve with its committed `Cargo.lock` (for instance a crate it vendors gained a dependency its staging does not copy, as `ferrata-cli` did on 21 November 2026), or `cli/` uses a `lez-wallet`-only crate outside `#[cfg(feature = "lez-wallet")]` (the default-feature builds would not see it), or the module's failing-endpoint test; `ferrata_updater_module` is left out (its staging needs `lez/`'s guests) |
| `witness-independence` | [`scripts/ci/witness-independence.sh`](../../scripts/ci/witness-independence.sh): the bridge's `host invariance 64`; then `cargo test -p ferrata-bridge-core` in `bridge/` | Rust 1.93, RISC Zero's toolchains ([`scripts/ci/install-risc0.sh`](../../scripts/ci/install-risc0.sh)) | the release or claim guest's cycle count (user cycles and segment sizes) differs between any two of 64 random valid witnesses (SPEC-v2 §5.2), or a bridge core test fails |
| `roundtrip` | [`e2e/ci/run.sh`](../../e2e/ci/run.sh): one deposit there and back, every step asserted ([`e2e/ci/README.md`](../../e2e/ci/README.md)); then the `ferrata` tool's own end-to-end test, [`cli/tests/e2e.sh`](../../cli/tests/e2e.sh) (deposits through the binary, scan, claim, eleven burns, a release, recovery from the seed phrase; its checkpoint is a journal the test builds); then the relayer's library tests and `cli/tests/gasdrop.rs` (a real gas-drop receipt, no dev mode); then the attestation CLI's library tests (`cargo test --lib attest`) and `cli/tests/attest_faults.rs` (RFP-021 R.5, U.5) | Rust 1.93 and 1.98.1, RISC Zero's toolchains, Foundry 1.3.6, the upstream sources (`scripts/fetch-upstream.sh`), `libpcsclite-dev` | any step of the round trip, or the build, or an attestation fault or code-53 test |
| `lez-harness` | [`scripts/ci/lez-harness.sh`](../../scripts/ci/lez-harness.sh) `--shard "$SHARD"` (`SHARD` from `matrix.shard`) across a 6-shard matrix (`harness-1`–`harness-4`, `updater-gate`, `updater-privacy`; [`scripts/ci/lez-harness-shards.json`](../../scripts/ci/lez-harness-shards.json)), guarded by [`scripts/ci/check-lez-shards.py`](../../scripts/ci/check-lez-shards.py) at the start of each shard: every non-feature-gated Cargo target of `lez/harness` and `lez/updater` (`--lib`, `--bins`, `--doc`, and integration `--test` binaries), RISC Zero dev mode, and the privacy suite end to end ([`scripts/privacy/run.sh`](../../scripts/privacy/run.sh)) with its gate ([`scripts/privacy/check-results.py`](../../scripts/privacy/check-results.py)) | Rust 1.98.1, RISC Zero's toolchains, the upstream sources, Foundry 1.3.6 and the vault's libraries | the shard map disagrees with `lez/harness` / `lez/updater` / `ci.yml`, a harness or updater test fails, or the privacy gate fails or the published tables in `docs/privacy/generated/` are not what the code produces |
| `lez-harness-full` | In [`.github/workflows/lez-full.yml`](../../.github/workflows/lez-full.yml) (daily `schedule` at 04:17 UTC and `workflow_dispatch`): [`scripts/ci/lez-harness.sh`](../../scripts/ci/lez-harness.sh) with no arguments (unsharded) once; separate from `ci.yml` so no job in `ci.yml` is skipped on push, pull request or dispatch (counted as manual by `check-row-tests.py`; GitHub disables scheduled runs after 60 days without repo activity) | same as `lez-harness` | any suite or privacy check fails in the unsharded run |
| `names` | [`scripts/ci/names.sh`](../../scripts/ci/names.sh) `build`, then `scripts/ci/names.sh`: the test network's names program (not an RFP requirement; [`names/README.md`](../../names/README.md)): its wire crate's tests, the harness suites `names` and `names_review` (feature `names`, feature-gated in the shard map; RISC Zero dev mode), the IDL check (`idl/tools/check_names_idl.py`) and the JS resolver's and the CLI's tests (`names/js/test/`) | Rust 1.93 and 1.98.1, RISC Zero's toolchains, the upstream sources, Node 22.7+ or 20.19+ (otherwise Node 22.12.0 from nodejs.org, checked against its SHASUMS256) | a test or the IDL check fails, or the harness run changes `lez/results/names-vectors.json` |
| `msg` | the test network's messages programs (not an RFP requirement; [`docs/design/messages.md`](../design/messages.md)): `node --test deploy/testnet/web/msg/test/*.test.mjs` (the JS library and CLI, an implementation sharing no code with it against OpenSSL's ML-KEM-768 values, the docs' own check), `python3 -I idl/tools/msg_idl.py --check`, the wire crates' tests (`cargo test -p ferrata-lez-msg-wire -p ferrata-lez-msg-v2-wire`) and the harness suites `messages`, `messages_v2` and `messages_findings` (feature `msg`, feature-gated in the shard map; RISC Zero dev mode) | Node 20 or later, Python, Rust 1.93 and 1.98.1, RISC Zero's toolchains, the upstream sources | a test fails, or the committed IDLs are not what `idl/tools/msg_idl.py` writes |
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

### `lez-harness`, `lez-harness-full` and `monitor`

Added on 22 November 2026 (branch `rfp-ci-coverage`): before, the LEZ suites, the watchers' rehearsal and the
privacy suite ran on the build box only, and RFP-022 R.11's check (`scripts/ci/check-row-tests.py --require-ci`)
counted their rows as box-only. Measured on the build box with 4 threads (`taskset -c 0-3`, the hosted runner's
vCPUs), by the jobs' own commands: `lez-harness` built cold in 622 s (target directory 3.6 GB), and its script at
the branch's tip ran the harness's tests in 2,074 s, the updater's in 1,136 s and the privacy suite with its gate
in 106 s (3,362 s after a warm build; every test binary passed, the gate 33 of 33); `monitor` 69 s for its tests
and 131 s for the rehearsal.

On GitHub Actions, the unsharded `lez-harness` job passed in **50.6–74.6 min** across two measured runs: 3,038 s
(50.6 min; setup 166 s, cold build 512 s) in run `45075900693` (`8c38d723`), and 4,474 s (74.6 min; setup 137 s, cold
build 809 s, warm re-check 59 s, harness 2,158 s, updater 1,217 s of which `tests/recorded.rs` alone took 806.59 s,
privacy suite + gate 88 s) in run `45072883393` (`ff0cb39a`, job `137363580490`). In the green runs, every other job
finished within 16.4 min (`roundtrip` 985 s in run `45072883393`, 937 s in run `45075900693` attempt 2; on attempt 1
of `45075900693`, `roundtrip` hung 73 min in `cli/tests/e2e.sh` before being cancelled).

To cut push and pull-request wall time without testing less, `lez-harness` runs as a 6-shard matrix balanced on the
measured durations (`scripts/ci/lez-harness-shards.json`, `fail-fast: false`): four harness shards
(`harness-1`–`harness-4`, 343–517 s of tests each across the two measured runner speeds, with `harness-1` also running
`harness --lib --bins` and `harness --doc`), `updater-gate` (splitting `lez/updater/tests/recorded.rs` with `--exact`
so `gate_two_honest_one_adversary`, derived at 475–725 s as a single test, starts at `t = 0` alongside four other heavy
`recorded` tests on the remaining 3 cores), and `updater-privacy` (`ferrata-updater --lib --bins`, the remaining 11
updater binaries (with `redating`, added after the measured runs: 87 s on the build box at the shard's four test threads), `ferrata-updater --doc`, `recorded` with `--exact --skip` for those five names, and the privacy suite
with its gate, running `privacy_suite` once instead of twice). Each shard builds only the targets it runs; sharded
workflow wall time was derived at **~19–28 min** (~62–63% reduction across both measured runner speeds); the first
sharded run on GitHub (run 45110242188 at `907736e8`, 23 November 2026) took **22.7 min**, all 18 jobs green, its shards
`harness-4` 1,362 s, `harness-3` 1,191 s, `harness-2` 1,156 s, `updater-gate` 1,155 s, `updater-privacy` 1,119 s,
`harness-1` 1,035 s. On GitHub's Free plan (20 concurrent jobs), a push to `main` starts 18 `ci.yml` jobs plus
`macos.yml`'s 2 = 20 jobs, while a push to a branch with an open same-repo PR starts 36 `ci.yml` jobs (excess jobs queue
without failing; `concurrency` isolates each `main` run in its own `ci-main-<run_id>` group with `cancel-in-progress:
false` so intermediate commits on `main` are never cancelled or replaced, while cancelling superseded runs on other
refs). The daily/manual unsharded backstop runs separately in
[`.github/workflows/lez-full.yml`](../../.github/workflows/lez-full.yml) (`lez-harness-full`, 04:17 UTC; GitHub
auto-disables scheduled workflows after 60 days without repo activity; `deps` reports RustSec advisories against its
baseline without blocking on `ci.yml`). Target directories are not cached (the round trip's caches already use most of
GitHub's 10 GB); the toolchains, the upstream sources and the vault's libraries share the round trip's cache entries.
Long steps in `roundtrip`, `lez-harness` and `lez-harness-full` carry explicit `timeout-minutes` bounds, and
`cli/tests/e2e.rs` runs its `CliLocal` `forge script` offline (`FOUNDRY_OFFLINE=true`), `--slow`, and under a 300 s
portable timeout.

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
