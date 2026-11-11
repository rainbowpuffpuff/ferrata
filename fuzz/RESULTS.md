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
checkpoint, bridge and LEZ crates (fuzzed since by `fuzz/programs/`, next section); sanitizers. The runs are minutes, not days: a clean run is
evidence of absence for shallow and medium-depth faults only.

## The programs: `fuzz/programs/` (bridge, checkpoint, release)

A second cargo-fuzz crate, `fuzz/programs/`, fuzzes what the LEZ-side programs and the guests read:
the bridge program's input decoding and its own message handler, the checkpoint programs' LEZ block
and Mantle inscription parsers and their inputs (rule (A), rule (B) with its tip and channel checks,
segments), the release and claim guests' inputs, and the Rust decoders of the journals (the release journal, which the vault
reads, and the v2 checkpoint journal, the segment journal and rule (B)'s signed attestation). It is its own workspace because it needs LEZ v0.4.0-rc1's crates (the clone beside the
repository, as `checkpoint/fidelity` uses them) and their toolchain (Rust 1.98.1, `lez/rust-toolchain.toml`);
its `Cargo.lock` started from `checkpoint/Cargo.lock`, so the upstream crates resolve to the versions
the fidelity tests use. The bridge program is fuzzed as written: `build.rs` compiles the guest's own
source (`lez/guests/bridge/src/main.rs`, its leading `//!` lines turned into `//` because `include!`
refuses inner doc comments) into the crate, and the target calls its `handle`; no guest was changed,
so no image id changed.

Run on 11 November 2026 on the Modal build box of branch `fuzz-bridge` (workspace `stufflaters`, 32
vCPUs visible); Rust 1.98.1, cargo-fuzz 0.13.1, libfuzzer-sys 0.4.13; built with
`cargo +1.98.1 fuzz build --fuzz-dir fuzz/programs -a -s none` and `RUSTFLAGS=-Coverflow-checks=on`
(optimized, debug assertions and overflow checks: an arithmetic overflow is a crash, where the
guests, built in release, would wrap silently). No sanitizer, as for `fuzz/`.

### Targets

| Target | Input | Properties checked | Seeds |
|---|---|---|---|
| `bridge_wire` | selector, bytes | every borsh type the bridge program and its peers decode (`BridgeMessage`, `BridgeState`, `ClaimWitness`, `Note`, `TokenEntry`, `FeeSchedule`, `BridgeConfig`, `TimeRecord`, `Gate`, the three proof types, the attestation program's `Message` and `Attestation`, LEZ's `StateReply` and `ReceiveInput`): no panic; whatever decodes re-encodes to exactly the input (one encoding per value). **Differential:** the checkpoint's hand-written decoder of the bridge program's messages (`checkpoint_core::wire`) extracts exactly the `Burn`, `ImportRoot`, `Sunset` and `Echo` messages the program decodes, with the same fields, nothing the program refuses, and `None` for every other variant; its encoder writes the program's bytes | the visible calls of the lab's 202 real blocks and of the four recorded privacy-preserving transactions (523 distinct messages), the parts (`Note`, proofs, configuration, registry entry, schedule, time record) of those that decode, and messages built from the program's own types |
| `bridge_handle` | receiving actor (named by the first message), optional initial state, then up to six deliveries (sender, `is_authorized`, message) | the guest's own `handle`, the state carried from one delivery to the next as LEZ carries it: no panic (overflow checks on); a refusal is a code of the program's block (240..=255), never 0 (which would end the guest as a success with nothing committed); a written state decodes as a `BridgeState` and re-encodes to the same bytes (a state the program cannot read back would brick the actor); a message to the program's own actors decodes canonically as a `BridgeMessage` or a `StateReply`. Senders: none, the actor the state waits for, an actor named by the message (configuration, token, root, nullifier, holder), the receiver itself | every decodable message under the eight receivers, unsigned and from the configuration actor; `Mint` at `holder(account)` from the nullifier actor followed by each built message; `Spend` twice at the nullifier actor from the token actor (170) |
| `lez_block` | arbitrary bytes | **differential against LEZ v0.4.0-rc1's own decoder** (`borsh::from_slice::<common::block::Block>`): the checkpoint's `parse_block` accepts exactly the blocks LEZ decodes, except its two documented differences (it also requires `reverted` strictly ascending and naming existing transactions; it takes BIP-340 keys as opaque bytes where LEZ validates the point: the only LEZ refusal it may accept, checked by the error's type); when both accept, the same header, transaction count, reverted list and status, each transaction's span is exactly LEZ's encoding of that transaction and of the same kind, and `hash_parsed_block` equals LEZ's `HashableBlockData::compute_hash` | the lab's 195 recorded blocks 8..=202 (`checkpoint/realblocks/out/real-lab-8-202.input`) and the four recorded privacy-preserving transactions of `lez/results/*.borsh`, each as a one-transaction block (199) |
| `checkpoint_input` | selector, bytes | `CheckpointInput` (rule A), `CheckpointInputB` (rule B) and `SegmentInput`: no panic in decoding or in `run_checkpoint`, `run_checkpoint_b` (the producer's inscription: tip, channel, signer in the live key set; the attestations), `run_segment`; an input that decodes re-encodes to exactly its bytes; an accepted checkpoint's journal names the parent state's digest, the new state's digest, tip id and hash; a segment's journal decodes back to itself. (The target also hands the journal's first 13 words to the bridge core's `CheckpointJournal::decode`, but that reader is the v2 journal's, domain `sha256("ferrata/vault/checkpoint/v2")`, 416 bytes, and refuses the v3 journal the checkpoint guests commit, domain `.../checkpoint/v3`, 544 bytes: the comparison never applies. No Rust decoder of the v3 journal exists; the vault reads it in Solidity.) | the lab's recorded rule (B) input over blocks 186..=202 (accepted by `run_checkpoint_b`), the same as a rule (A) input, and segments of its first 1, 2, 4, 8 and 17 blocks (7) |
| `checkpoint_b` | up to eight 4-byte edits of the lab's recorded rule (B) input over blocks 186..=202 (which the rule accepts): `T`'s index; drop, swap, duplicate a block; flip a bit of a block; re-seal a block (stored hash set to its recomputed hash); flip a bit of the producer inscription's prefix, suffix or signature; flip a bit of an attestation, drop or duplicate one; the parent's tip id, tip hash or a key; a bit of the channel id | rule (B)'s tip and channel checks, re-checked independently whenever `run_checkpoint_b` accepts: every block parses and carries its own hash; the blocks chain from the parent's tip (hash, consecutive ids); the new tip is `blocks[t_index]` (id and hash); `T`'s producer inscription checks against `T`'s bytes on the configured channel, signed by a key of the parent's set; no panic | the empty edit list (the recorded input itself) |
| `mantle` | selector, op index, ops bytes | the opcode walk, the elided inscription (`split`, `check`) and the channel configuration op: no panic; a split inscription is a prefix and a suffix of the ops around the payload; re-signed by a known key (its signer field replaced, the transaction hash signed), `check` accepts it exactly when the whole ops walk is well-formed and then returns the BLAKE2b hash of the whole unelided transaction (`tx_hash`), that signer and the parent written in the op; an accepted configuration op sits in a well-formed transaction at its index, with the whole transaction's hash | the 195 recorded inscription transactions of the lab's blocks 8..=202 (prefix ‖ block ‖ suffix, each checked against the recorded transaction hash) (390) |
| `release` | selector, bytes | `ReleaseInput` (1340 bytes) and `ClaimInput` (1260 bytes) decode to values that encode back to the same bytes (no byte of a private input is ignored); a release `verify_release` accepts commits a journal that the vault-side decoder (`ReleaseJournal::decode`) reads back to exactly the public fields, burn root and nullifier; `ReleaseJournal`, the bridge core's (v2) `CheckpointJournal`, `SegmentJournal` and rule (B)'s `SignedAttestation` decoders accept exactly their encoders' output; no panic in `verify_claim` | the release and claim guests' inputs of `bridge/vectors/{outbound,inbound}-v2.json`, the journals of `release-journal-v2.json` and `checkpoint-v2.json`, the recorded attestation (12) |

Seeds are written by `fuzz/programs/examples/seeds.rs` (`cd fuzz/programs && cargo +1.98.1 run --release --example seeds -- seeds`).

### Runs

Measured on the box; executions, edges ("cov") and features ("ft") are the fork-mode parent's last
report, corpus the minimized result of `-merge=1`.

**Long run** (`./run.sh 2700 5 <dir>`: 45 minutes, 22:25 to 23:11 CET, the six targets at once, five
workers each, started from the smoke runs' corpora and the seeds):

| Target | Seeds | Executions | Edges | Features | Minimized corpus | Crashes |
|---|---|---|---|---|---|---|
| `bridge_wire` | 526 | 687,225,612 | 1,903 | 3,879 | 1,127 files, 2.3 MB | 0 |
| `bridge_handle` | 170 | 543,170,710 | 2,501 | 5,378 | 874 files, 1.4 MB | 4 (H2, harness) |
| `lez_block` | 199 | 280,526,598 | 1,535 | 5,341 | 724 files, 3.8 MB | 0 |
| `checkpoint_input` | 7 | 546,277,034 | 1,051 | 2,551 | 570 files, 14 MB | 0 |
| `mantle` | 390 | 199,044,028 | 317 | 600 | 175 files, 278 KB | 0 |
| `release` | 12 | 1,802,693,845 | 251 | 279 | 156 files, 87 KB | 0 |
| total | 1,304 | 4,058,937,827 | | | | |

**`checkpoint_b`** (added during the long run, run beside it: 2,120 s on two workers, 22:30 to 23:06
CET): 25,108,680 executions, 679 edges, 1,501 features, corpus 165 files, no crash.

**Smoke runs** before it: the five targets with seeds then, 180 s on five workers each (19.1, 17.9,
23.5, 9.8 and 59.5 million executions in `bridge_wire`, `lez_block`, `checkpoint_input`, `mantle`,
`release`), which found H1; then `bridge_handle` and `release` 75 s on 12 workers each (18.6 and 37.2
million), clean.

**Re-run after H2** (`./run.sh 480 24 <dir> bridge_handle`: 8 minutes on 24 workers, 23:14 to 23:22
CET): 229,561,354 executions, 2,624 edges, 5,695 features, corpus 903 files, no crash; the four H2
inputs replay clean.

In all, 4.31 billion executions in the long run, `checkpoint_b` and the re-run (4.50 billion with the
smoke runs).

### Findings

No crash, divergence, timeout or out-of-memory that is a fault of the code under test. In
particular: LEZ's own decoder and the checkpoint's block parser agreed on all 280.5 million inputs
(accept/refuse, header, transaction spans and kinds, block hash) apart from their documented
differences; the checkpoint's decoder of the bridge program's messages agreed with the program's
borsh decoding on every input it was given; no input made the bridge program panic, overflow, refuse with a code
outside 240..=255 or write a state it cannot read back; every decoder that accepted an input
re-encoded it to the same bytes.

| # | Target | What | Status |
|---|---|---|---|
| H1 | `release` | 194 crashes in the smoke run, all `ClaimInput::decode`'s documented panics (kind byte not 0x01/0x02: 114; non-zero padding: 80): the claim measurement guest refuses such inputs by panicking, as `encoding.rs` documents | harness error: the target decodes only the documented domain (commit 30a2777); `release` then ran 1.8 billion executions clean |
| H2 | `bridge_handle` | 4 crashes in the long run, all "a message to the program's own actor does not decode": replayed with `FERRATA_FUZZ_EXPLAIN=1`, each is the attestation program's `Message` (`Attest`/`Read`), sent by the program to the account its configuration names as the attestation program, in a configuration the fuzzer built naming the bridge program itself. In these four the message does not decode as a bridge message, so the bridge program refuses it (`BAD_MESSAGE`) and the transaction fails with nothing written | harness error: the property now accepts the attestation program's messages on the program's own id (commit 2efa8f8); the four inputs (`fuzz/programs/regressions/bridge_handle/`) replay clean; re-run below |

Two facts about committed data, found while seeding (no code fault):
- `checkpoint/realblocks/out/real-lab-{8,186}-202.input` are in the rule (B) input format before
  commit 91c0df8 added `T`'s producer inscription: today's `CheckpointInputB::decode` refuses them
  (`BadLength { what: "suffix" }`). With the producer inserted from the recorded inscription of block
  202 (`inscriptions.jsonl`), the 186..=202 input decodes and `run_checkpoint_b` accepts it
  (`fuzz/programs/src/lib.rs`, `recorded_rule_b_input`). Regenerating the two files with
  `checkpoint/realblocks` would make them current.
- None of the 507 distinct visible calls in the lab's 202 blocks decodes as today's bridge
  `BridgeMessage` (the lab ran the attestation program, not this bridge program); the bridge seeds
  come from the recorded privacy-preserving claim and burn transactions (`lez/results/*.borsh`) and
  from messages built with the program's own types.

### Reproduce

```sh
cargo install cargo-fuzz --locked                    # 0.13.1 on the box
cargo +1.98.1 fuzz build --fuzz-dir fuzz/programs -a -s none   # with RUSTFLAGS=-Coverflow-checks=on, from the repository root
cd fuzz/programs
cargo +1.98.1 run --release --example seeds -- seeds  # seeds/ from the committed recordings and vectors
./run.sh <seconds> <workers per target> <output dir> [targets]   # fuzz in parallel, then minimize into corpus/
./target/x86_64-unknown-linux-gnu/release/<target> <file>       # replay one input (FERRATA_FUZZ_EXPLAIN=1 for bridge_handle)
```

The minimized corpora of `bridge_wire`, `bridge_handle`, `mantle`, `release` and `checkpoint_b` are
committed (`fuzz/programs/corpus/`: 2,522 files, 3.5 MB); those of `lez_block` and `checkpoint_input`
(3.9 and 16 MB, mostly the recorded blocks and input and their mutants) are not, and are git-ignored:
`seeds.rs` regenerates their starting points from committed data.

### What this does not cover

A differential of the Mantle parsers against Logos Blockchain's own transaction decoder (its `Ops`
decoding needs a gas context; `checkpoint/fidelity` compares the formats on transactions built with
the upstream crates); the bridge program's paths behind a valid Ethereum attestation or a valid claim
witness (the fuzzer reaches the refusals before them: deliveries run on the program's own handler,
but no input carries a real attestation, and `bridge_handle`'s seeds hold no consistent claim);
`run_checkpoint` (rule A) beyond mutations of a rule (B) input re-encoded as rule (A); the guests
compiled for RISC Zero (the targets run the same source on the host, with overflow checks, where the
guests wrap); sanitizers. A clean run of minutes is evidence of absence for shallow and medium-depth
faults only.
