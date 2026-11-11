# The CI jobs, proven on the build box

The two jobs added to [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) on 11 November 2026, run on the
Modal build box (`spike/modal-build/mrun`, profile `indeea`, outside the repository) by their own scripts, exactly
as the workflow's `run:` steps call them. The box has no Docker; the scripts need none. Every number below was
measured there unless marked otherwise; `nproc` in the box's container printed 2.

Prototype evidence for a grant proposal. **Not audited. Not for production.**

## `roundtrip`: one deposit there and back

`e2e/ci/run.sh build` then `e2e/ci/run.sh` (the job's two last steps), `RISC0_HOME_LEZ` set to the box's RISC Zero
home with guest Rust 1.91.1:

```
[   0.8 s] ok: 1. zone configuration and genesis; vault deployed with both
[   2.3 s] ok: 2. Init and Register ETH on LEZ, attested from the vault's storage
[   2.3 s] ok: 3. deposit of 1 ETH on anvil
[   5.9 s] ok: 4. ImportRoot and ImportFees attested; claim minted 1 ETH on LEZ; second claim refused (250)
[   6.9 s] ok: 5. burn of 0.1 ETH on LEZ
checkpoint guest: 6181381 user cycles (dev mode: executed, fake receipt)
checkpoint: 7 blocks, burn root 0xeae3068d45a9da3346fddb3056c4cc72da2c11057ded6e250c0df586a0e09144, newState 0x724c12f79d754e8ccd492a47f690aaffd8cab45e386f00774f1f67f242d6e136
[   8.7 s] ok: 6. rule (B) checkpoint: guest journal == native == vault layout; refused early; finalised after W
[   8.8 s] ok: 7. release of 0.1 ETH on anvil; replay refused
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.79s
```

| | Measured on the box |
|---|---|
| the test itself | 8.8 s (steps 1–7) |
| job steps with every artefact built (`run.sh build`, then `run.sh`) | 33 s and 43 s |
| the vault (anvil), the configuration | vault `0xCf7E…0Fc9` (anvil's account 0, fourth contract), chain id 1, configId `0x2dcfafd7…995409f4`, genesisState `0x8e54c7d6…91a3efb8` (equal in two runs: the genesis block is deterministic) |
| the zone's checkpoint | 7 blocks: genesis, six blocks of the transactions the harness settled (`Init`, `Register`, `ImportRoot`, `ImportFees`, the claim, the burn), `T`; n = 4, t = 3; one burn; imported leaf count 1 |
| the rule (B) guest | 6,181,381 user cycles here, 6,198,307 in the first run: LEZ signs each transaction with fresh randomness, so the blocks, their hashes and `newState` differ between runs; the burn root does not |
| image ids of the guests built on the box | `checkpoint_b` `0x2924ffc4154729bb83736b62f8ee34e21d81f74de713434df3287260dfe55a3a`, `release` `0x54c5d465052553b7b42f7e04cd83e0df39bb706e127825313e8936e7ad49bb4b` (the test deploys the vault with the ids of the guests it built; a build elsewhere gives other ids, since risc0-build embeds paths) |

Cold builds, measured on the box one by one (two cores; the GitHub runner has four):

| Build | Time |
|---|---|
| `bridge`: `host` and its guests, release | 221 s |
| `checkpoint`: `fidelity`'s `ci_zone` (LEZ v0.4.0-rc1 and Logos Blockchain d480efc6 crates), release | 215 s |
| `checkpoint`: the guests (`methods`), release | 140 s |
| `cli`: the test (LEZ's wallet and node crates, alloy, the harness and LEZ's guests) | 422 s, after an earlier build that had compiled part of it (471 s) and stopped at `pcsc-sys` for want of `libpcsclite-dev` |
| for comparison, `lez`: the harness's tests | 830 s |

So a first run on GitHub, with nothing cached, builds for something like 20 to 40 minutes (an estimate from the
box's times, not measured on GitHub); later runs restore the cargo caches.

### Found on the way

- `cli/Cargo.lock` at integration6 did not list the harness's `ferrata-lez-holdings-wire` dependency, so any
  `cargo … --locked` in `cli/` failed; `cli/tests/e2e.sh` runs without `--locked` and rewrote it. Fixed here.
- `cli/`'s build needs `libpcsclite-dev` (`pcsc-sys`, under LEZ's wallet); the job installs it.
- A refused transaction sent through alloy's default provider (nonce filler) leaves a nonce gap: the next
  transaction then never gets a receipt. The test checks expected refusals with `eth_call` and times out every
  receipt at 60 s.

A third run, with `run.sh` building as the job does (no debug info, no incremental artefacts), passed the same
seven steps in 8.6 s (the rule (B) guest: 6,176,865 user cycles); its build step rebuilt the test's dependencies in
184 s, its run step took 51 s.

The final run, on the branch's last code (after formatting), as the job runs it: `run.sh build` 72 s, `run.sh` 53 s,
the same seven steps passed; then the job's last step, `cli/tests/e2e.sh` (the `ferrata` tool's own end-to-end test,
with the job's build settings and `RISC0_HOME` at the LEZ home): passed in 33 s (the test 18.4 s).

## `witness-independence`

`scripts/ci/witness-independence.sh 64` (the job's last step):

```
[release] 64 random valid witnesses (leaf indices 0, 2^32-1, 0x55555555, 0xAAAAAAAA, then random; random siblings, secrets and public fields): distinct (user cycles, segment po2s): {(77736, [17])} (2.7 s)
[claim] 64 random valid witnesses (leaf indices 0, 2^32-1, 0x55555555, 0xAAAAAAAA, then random; random siblings, secrets and public fields): distinct (user cycles, segment po2s): {(74379, [17])} (2.7 s)
cycle counts are independent of the witness for both guests
```

Build 221 s cold (`host` and its guests), 3 s with `bridge/target` restored; the check itself 5 to 8 s.

## The toolchain step (`scripts/ci/install-risc0.sh`)

Run on the box with `HOME` set to an empty directory, as on a runner whose cache is empty (the box's own
toolchains untouched; rustc on the path, as the workflow's `dtolnay/rust-toolchain` step puts it): rzup installed
cargo-risczero and r0vm 3.0.5, C++ 2024.1.5, guest Rust 1.88.0 in `~/.risc0` and 1.91.1 in `~/risc0-home-lez`, in
110 s; a second run installed nothing (0 s). Each home is 3.1 GB on disk, which the cache compresses.

## Sizes, for the caches

| Directory | On the box |
|---|---|
| `bridge/target` | 981 MB |
| `checkpoint/target` | 3.9 GB |
| `cli/target` | 13 GB built with debug info (15 GB after the rebuild without it, both builds' artefacts side by side); the job builds without, and `Swatinem/rust-cache` keeps only the dependencies |
| `~/.cargo/registry` | 675 MB |
| `~/.risc0`, `~/risc0-home-lez` | 3.1 GB each |

GitHub keeps 10 GB of caches per repository (compressed); the first runs on GitHub will show whether the
`roundtrip` cache stays under it. If it does not, the job still passes: it then builds from cold each time.

## What was not run

The workflow itself on GitHub (no push from this branch): the job steps were run on the box, one by one, by the
same scripts, after the same toolchain versions were installed or present. `actions/checkout`, the caches and
`dtolnay/rust-toolchain`/`foundry-toolchain` were not exercised; the workflow file parses as YAML (four jobs).
