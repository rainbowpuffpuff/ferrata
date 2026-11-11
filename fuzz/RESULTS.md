# Fuzzing the consensus-critical parsers and verifiers

Coverage-guided fuzzing (cargo-fuzz, libFuzzer) of `ferrata-core`: everything a relayer or a
prover hands the light client and the inclusion code is attacker-chosen bytes. Seven targets,
three of them differential against the reference implementations the tests already compare
with (alloy-rlp 0.3.17, alloy-trie 0.9.9, the consensus specs' Merkle-branch definitions).

Run on 11 November 2026 on the Modal build box of branch `fuzz-core` (workspace `stufflaters`):
a container with 32 vCPUs visible (`nproc --all`), 64 GiB at most; Rust 1.93.0, cargo-fuzz
0.13.2, libfuzzer-sys 0.4.13. Built with `cargo fuzz build -a -s none` and
`RUSTFLAGS=-Coverflow-checks=on`: optimized, with debug assertions and overflow checks, so an
arithmetic overflow is a crash. No sanitizer: `ferrata-core` is `#![deny(unsafe_code)]`, and
the stable toolchain has no `-Zsanitizer`.

## Targets

| Target | Input | Properties checked | Seeds |
|---|---|---|---|
| `rlp` | arbitrary bytes | the whole item tree (walked iteratively) accepted by ferrata's strict RLP exactly when alloy-rlp accepts it, same shape and bytes; every item read as `u64` and as a 256-bit word exactly when alloy's `u64`/`U256` decoders accept it, same value; `split_first` agrees with alloy's header; `encode_u64` is alloy's encoding and decodes back | mainnet headers, receipts and proof nodes of the fixtures |
| `mpt` | structured: a trie (32-byte keys, short fixed-length keys, or receipt keys `RLP(i)`), a present or absent key, one proof mutation (bit flip, node removed, donor node inserted, swap, truncation, bytes appended, node pushed); raw: a key and nodes, the root being the first node's hash | the honest proof (alloy's `HashBuilder`/`ProofRetainer`) verifies to the trie's value; a mutated proof ferrata accepts proves the same value and alloy's `verify_proof` accepts it; a mutated proof ferrata refuses is refused by alloy for both the true value and absence; in raw mode whatever ferrata accepts alloy accepts with the same value, and the value is a substring of a node | account, storage and receipt proofs of the mainnet and Sepolia fixtures |
| `exec` | arbitrary bytes | block header: decodes with its own hash exactly when it decodes, never with another; account, storage value, receipt and logs against reference decoders built from alloy-rlp's primitives (same accept/reject and fields, minus ferrata's documented extra rules: zero storage root or code hash, zero storage value, more than four topics); every counted log readable, none past the count; `locate_log` and `block_log_index` inverse | fixture headers, receipts, accounts |
| `ssz` | fork, committee size (32 or 512), bytes | beacon header, execution payload header (Capella and Deneb), sync aggregate, sync committee, light-client header, bootstrap, update: no panic in decoding or in anything run on a decoded value (hash-tree roots, `is_valid_light_client_header`, `get_lc_execution_root`, header records, every fork upgrade); decode then encode returns the input wherever ferrata has an encoder (one encoding per value); the compact forms of a decoded bootstrap and update round-trip | spec sync vectors (every bootstrap and update, minimal preset, Altair to Gloas), mainnet bootstrap and finality updates |
| `witness` | fork, committee size, bytes | `CompactStore`, `CompactBootstrap`, `CompactUpdate`, `SignerWitness`, `AdoptionProgress`, `CommitteeSummary`, `witness::HeaderRecord`, `attest::HeaderRecord`: no panic; every accepted value encodes back to exactly the input | compact forms built from the spec vectors |
| `merkle` | leaf, depth, index, generalized index, branch | `compute_merkle_branch_root`, `is_valid_merkle_branch`, `is_valid_normalized_merkle_branch`, `normalize_merkle_branch` against the consensus specs' definitions transcribed in the harness; a flipped root is refused | none needed |
| `lc` | chain spec (preset, nine fork versions and epochs), bootstrap, store and update forks, current slot, genesis validators root, bootstrap, update | the spec store (`initialize`, `process_update`, `process_force_update`, `upgrade`) and the policy store (`FinalizedStore`) with the blst backend: no panic; neither store applies an update the spec's validation refuses; an applied update never moves the finalized slot back | every update of the spec sync cases with its bootstrap, spec and slot (signatures valid) |

Seeds are written by `examples/seeds.rs` from the committed vectors (`fuzz/seeds/`).

## Runs

Measured on the box; executions, edges ("cov", libFuzzer's covered edges) and features
("ft") are the fork-mode parent's last report, corpus the minimized result of `-merge=1`.

**Long run** (`./run.sh 1560 5`: 26 minutes, the seven targets at once, five workers each,
started from the smoke run's corpus and the seeds):

| Target | Seeds | Executions | Edges | Features | Minimized corpus | Crashes |
|---|---|---|---|---|---|---|
| `rlp` | 211 | 180,040,704 | 246 | 1,193 | 155 files, 116 KB | 0 |
| `mpt` | 70 | 32,302,447 | 1,005 | 4,305 | 424 files, 828 KB | 4 (P2) |
| `exec` | 87 | 176,898,580 | 630 | 1,276 | 328 files, 1.4 MB | 0 |
| `ssz` | 422 | 287,035,353 | 578 | 1,218 | 114 files, 498 KB | 0 |
| `witness` | 296 | 700,179,021 | 476 | 1,043 | 166 files, 200 KB | 0 |
| `merkle` | 0 | 392,616,140 | 113 | 343 | 65 files, 33 KB | 0 |
| `lc` | 140 | 25,226,851 | 578 | 1,550 | 220 files, 1.3 MB | 0 |
| total | 1,226 | 1,794,299,096 | | | 1,472 files, 3.7 MB | |

**Smoke run** before it (`./run.sh 240 4`, four minutes): 164.6 million executions; it found L1
(`lc`, 1 crash), P1 (`mpt`, 87) and M1/M2 (`merkle`, 2,665), all handled before the long run.

**Re-runs after the last changes:** `mpt` with P2 excluded, 200 s on 24 workers: 17,636,940
executions, 1,029 edges, no crash; the four P2 inputs replay clean. `merkle` on branch
`fuzz-core-merkle-domain` with the whole domain (depth to 255, generalized index 0), 120 s on 24
workers: 87,560,463 executions, 118 edges, no crash. `lc` ran the whole long run after the L1 fix
without a crash.

The minimized corpus is committed (`fuzz/corpus/`, 3.7 MB); the seeds (7.4 MB) are not, as
`examples/seeds.rs` regenerates them and the corpus holds their features.

## Findings

| # | Target | What | Reachable from attacker bytes? | Status |
|---|---|---|---|---|
| L1 | `lc` | `LightClientStore::process_force_update` computed `finalized_slot + UPDATE_TIMEOUT` in u64: a panic with overflow checks, a wrapped (early) deadline without them, once the finalized slot is within a sync-committee period of 2^64 | only through a finalized header with such a slot (a bootstrap of the trusted block, or an update signed by the committee); the spec store only: Ferrata's on-chain policy never forces an update | fixed in `crates/ferrata-core/src/store.rs` (exact arithmetic: a deadline past `u64::MAX` is never reached); regression `crates/ferrata-core/tests/fuzz_regressions.rs`, which failed before the fix with `attempt to add with overflow` at `store.rs:222`; `lc` re-run in the long run with no crash |
| P1 | `mpt` | a difference from alloy-trie 0.9.9: an exclusion proof ending at an extension whose path diverges from the key, padded with that extension's (correctly hashed) child. alloy keeps walking below the diverging extension and accepts; ferrata refuses the unused node (`ProofTrailingNodes`) | — (ferrata is the stricter side; both accept the unpadded proof, which is what `eth_getProof` returns) | intended: every statement has one proof. Pinned in `crates/ferrata-spectests/tests/fuzz_regressions.rs`; the harness checks that the padded proof's accepted prefix proves the same absence to both |
| P2 | `mpt` | a difference from alloy-trie 0.9.9: a branch node with a value (a key that ends at the branch). alloy refuses every such node ("branch node values are not supported"); ferrata follows the Yellow Paper and walks it (already tested by `values_at_branch_slots_and_prefix_keys` in `mpt_proofs.rs`) | no: such a node can only appear in a trie whose keys are prefixes of one another, and Ethereum's state, storage and receipt tries have 32-byte or `RLP(i)` (prefix-free) keys; a proof must hash-link to the real root | intended. The harness's raw mode skips the alloy comparison for proofs holding such a node (found in the long run: 4 inputs) |
| M1 | `merkle` | `floorlog2(0)` overflowed: generalized index 0 made `is_valid_normalized_merkle_branch` and `normalize_merkle_branch` panic (overflow checks) or, without overflow checks, compute a depth of 2^32 - 1 and allocate 2^32 roots | no: every caller passes a constant generalized index of at least 1 | fixed on branch `fuzz-core-merkle-domain` (not on `fuzz-core`: the fix changes code inside the guests, so their image ids) |
| M2 | `merkle` | `compute_merkle_branch_root` shifted the index by the level: `index >> i` overflowed for a depth above 64 (a panic, or a wrapped shift reading bit `i % 64`) | no: every caller passes a constant depth of at most 9 | fixed with M1 on `fuzz-core-merkle-domain` |

Fixes on branch `fuzz-core` change no code that the guests run except `store.rs`'s
`process_force_update`, which only the spec store calls (Ferrata's programs use the policy
store). The image ids were measured unchanged when this branch was merged into `integration6`
(11 Nov 2026): with this `store.rs`, the six programs of `e2e/crown/README.md`'s table rebuilt at
the crown build path, and the lab's attestation program rebuilt at the lab's path
(`lez/lab/README.md`, `61b7b70d…2f9a`), came out byte-identical to the deployed ones
(`scripts/reproduce-image-ids.sh build` on the build box; `docs/EVIDENCE.md` row 37).

One harness error of ours, found by the smoke run and corrected before the long run: `lc`
first asserted that `process_update` succeeds exactly when validation does; the spec's
`apply_light_client_update` may still refuse a valid update (its `assert`), so the property is
one-way.

No crash, divergence, timeout or out-of-memory in `rlp`, `exec`, `ssz` and `witness`.

## Reproduce

```sh
cargo install cargo-fuzz --locked                    # 0.13.2
cd fuzz
RUSTFLAGS=-Coverflow-checks=on cargo fuzz build -a -s none
FERRATA_VECTORS=<consensus-spec-tests>/tests/minimal cargo run --release --example seeds -- seeds   # optional: seeds are committed
./run.sh <seconds> <workers per target> <output dir>  # every target in parallel, then minimizes into fuzz/corpus
./target/x86_64-unknown-linux-gnu/release/<target> <file>   # replay one input
```

`run.sh` runs each target with `-fork=<workers> -ignore_crashes=1 -timeout=10 -rss_limit_mb=4096`
and then merges (`-merge=1`) its working corpus and seeds into `fuzz/corpus/<target>`.

## What this does not cover

BLS arithmetic itself (blst and the zkcrypto backend are dependencies; `lc` reaches them only
with signatures of the seeds or mutations of them); a differential of the SSZ containers
against Helios's types (the light-client tests already replay 33 cases against Helios); the
checkpoint, bridge and LEZ crates; sanitizers. The runs are minutes, not days: a clean run is
evidence of absence for shallow and medium-depth faults only.
