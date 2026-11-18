# bench: ferrata-core cycle counts in a RISC Zero zkVM 3.0.5 guest

This bench runs ferrata-core inside RISC Zero guests on real mainnet light-client data
(`../crates/ferrata-spectests/fixtures/mainnet-p1899/`, checked against its `SHA256SUMS`). It
uses the executor only (r0vm 3.0.5, no proving) and reports user cycles per phase and per
execution. The numbers and what they mean are in `RESULTS.md`.

It is a workspace of its own, excluded from the repository's root workspace.

* `guests/bench-core/`: the measurement code, every mode.
* `guests/<config>/`: one guest per configuration. Each is its own workspace, with its own
  `[patch.crates-io]` and `Cargo.lock`.
* `methods/`: builds the guests with risc0-build and embeds them.
* `host/`: runs the guests and writes `results/<config>.<modes>.txt` (`all` = every mode).
* `tools/table.py`: makes the markdown tables in `RESULTS.md` from the result files.
* `results/`: the latest run (`blst-accel.all.txt`, `PROVENANCE.txt`, `tables.md`, run log).
  `results/2026-11-06_snapshot-25aa07e/` holds the earlier six-configuration run.

## Prerequisites (once, no sudo, no shell rc edits)

```sh
cargo install rzup --version 0.5.2 --locked --no-default-features --features cli,install
rzup install rust 1.88.0       # RISC Zero Rust toolchain for the guests
rzup install cpp               # 2024.1.5, the riscv32 C compiler (blst is C)
rzup install r0vm 3.0.5        # the executor; default_executor() finds it through rzup
```

## Run

```sh
cd bench                                      # from the repository root
export TMPDIR=$PWD/../../tmp                  # tmp/ beside the repository, not /tmp
mkdir -p "$TMPDIR"                            # rustc stops if it does not exist
cargo build --release -p host                 # compiles the six guests, then the host
./target/release/host blst-accel              # every mode for blst-accel, about 2.5 min
./target/release/host blst-accel --modes onchain,single-onchain,coresplit,steps   # the newer modes, under 1 min
./target/release/host                         # all six configurations, about an hour
python3 -I tools/table.py results --headline  # the headline table of RESULTS.md
python3 -I tools/table.py results > results/tables.md
```

`./run.sh [CONFIG ...] [--modes ...]` does the same. It sets `TMPDIR` and records the measured
core (HEAD, uncommitted changes, file hashes) in `results/PROVENANCE.txt`, then builds and
runs.

Configurations: `blst-accel` (RISC Zero blst `v0.3.16-risczero.0`, the one to cite),
`blst-accel-v0314`, `zkcrypto-accel`, `zkcrypto-plain`, `blst-plain`, `nopatch`.

Modes (all on update_1: period 1900, handoff, 510/512 signers):

| mode | what one run measures |
|---|---|
| `onchain` | `Adoption::OnChain`: `process_update` with and without the handoff, `check_committee_onchain` alone, the restore paths (`from_core` vs. the OnChain policy on a decoded `StoreCore`) |
| `single-onchain` | one whole execution per update: decode saved state + update, OnChain policy, no committee re-check |
| `coresplit` | two executions: finality checks + `Blst::miller_half` (576 bytes out), then `Blst::final_half` |
| `steps` | three executions: everything before the Miller loops, the Miller loops, `final_half` |
| `phases` | phases 1–10 with raw library calls, ferrata's backend calls, `process_update` (Full policy) |
| `e2e` | bootstrap → update_0 → update_1 through `FinalizedStore` (Full policy) |
| `single` | one execution that restores the store with `from_core` |
| `split` | the earlier split with raw `blst_fp12` bytes |

The host checks every 576-byte Fp12 handed between executions against the core's `miller_half`,
computed natively. The guests assert every verdict.

## The execution layer (`exec/`)

`exec/` is a second workspace with its own host, methods and two guests. It measures
`attest_storage`, `attest_log` and `history_hop` on the mainnet end-to-end fixture
(`../crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09/`, checked against its
`SHA256SUMS`), with and without RISC Zero's accelerated Keccak:

* `keccak-accel`: `tiny-keccak` from `risc0/tiny-keccak` tag `tiny-keccak/v2.0.2-risczero.0`
  (the fork's only tag; it calls `risc0_keccak_update`, which risc0-zkvm 3.0.5 exports), plus
  the accelerated SHA-256 used by the light-client guests;
* `keccak-plain`: the same, with `tiny-keccak` 2.0.2 from crates.io.

```sh
cd bench/exec                           # from the repository root
export TMPDIR=$PWD/../../../tmp && mkdir -p "$TMPDIR"
cargo build --release -p exec-host      # compiles the two guests, then the host
./target/release/exec-host              # both configurations, results/<config>.txt
```

The host runs the light client natively to obtain the record of block 26,379,842, computes every
expected result natively, and the guests assert them. Results and their reading: `RESULTS.md`.

## The residue-witness check (`residue/`, experimental, unaudited)

An experimental replacement of the final exponentiation, used by no deployed program
([`RESIDUE.md`](RESIDUE.md)):

* `residue/`: the check and the honest witness on blst (`src/lib.rs`), its constants
  (`gen_constants.py` writes `src/constants.rs` from p, r and u, asserting every relation), and its
  tests: `constants`, `real_data` (tests (a) and (b); needs `../scripts/fetch-vectors.sh`),
  `differential` (test (c), 11,034 products).
* `guests/residue-accel/`, built by `residue-methods/` alone: one whole update per execution, as the
  LEZ program's `VerifyAll` runs it, with either final check.
* `residue-host/`: `measure` (the table of `RESULTS.md`, into `results/residue/`) and `check`
  (test (d): the guest's verdicts on the real data's products).
* `residue-agg/`: the signing key composed from one Merkle multiproof of all the non-signers (the
  core's `step1` with that one change; [`RESIDUE.md`](RESIDUE.md) §9) and its tests (`tests/agg.rs`:
  the core's `step1` compared on the vectors, the real chains and 2,288 committee/bitfield pairs;
  refusals; forged multiproofs; `tests/batch_g1.rs`: blst's batch addition on colliding G1 points;
  `tests/edge_cases.rs`, from an independent review: small trees exhaustively, committees that are
  not a power of two, the identity, the decoder's edges, the batch addition in and outside G1).
* `guests/residue-agg-accel/`, built by `residue-methods/` beside `residue-accel`: one whole update
  with either composition of the signing key, and the profile of a non-signer's cost;
  `residue-host profile`, `measure-agg` and `boundary` run it (into `results/residue/agg/`), and so
  does `residue-host/tests/placements.rs` (from the same review: the 342-signer update at other
  placements of the absent members, and 1 to 170 members absent).
* `results/residue/agg/placements.md`: the cycle counts of those placements and participations
  (every third member absent needs the most hashes but is not the costliest placement).
* `results/residue/agg/placement-phases.txt`: the guest's phases for seven placements with the same
  425 hashes and 256 witness nodes (the difference is in step 1).
* `results/residue/agg/edge-cases.txt`: the lines `tests/edge_cases.rs` prints, with its counts.
* `results/residue/agg/lez-placements.txt`: what LEZ's state machine charged the experimental copy
  of the program at 12 placements, and what it refused (`../lez/harness/tests/residue_placements.rs`,
  behind the harness's feature `residue-experimental`).

```sh
cd bench && export TMPDIR=$PWD/../../tmp && mkdir -p "$TMPDIR"
cargo test --release -p residue -- --nocapture         # (a)-(c); writes target/residue-tuples-a.bin
cargo build --release -p residue-host
./target/release/residue-host measure                  # results/residue/one-tx.txt, tables.md
./target/release/residue-host check                    # results/residue/guest-check.txt
cargo test --release -p residue-agg -- --nocapture     # RESIDUE.md §9.5
./target/release/residue-host profile                  # results/residue/agg/profile.txt
./target/release/residue-host measure-agg              # results/residue/agg/one-tx.txt, tables.md
./target/release/residue-host boundary                 # results/residue/agg/boundary.txt, boundary.md
cargo test --release -p residue-host --test placements # results/residue/agg/placements.md, placement-phases.txt
```

## Notes

* The guest lockfiles are resolved for the guest toolchain's Rust 1.88. If they ever have to be
  regenerated, run
  `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update --manifest-path guests/<g>/Cargo.toml`.
* Build output goes to `target/` (about 3 GB, git-ignored); `rm -rf target` frees it.
