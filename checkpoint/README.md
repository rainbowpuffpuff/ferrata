# ferrata-checkpoint

Prototype of the program that lets an Ethereum vault follow a LEZ (Logos Execution Zone) chain:
SPEC-v2 §4.1–§4.2 with SPEC-v3 D3 (checked producer inscription, 17-word journal, posting timeframe
pinned in the key-set hash), as a RISC Zero 3.0.5 guest. It re-hashes LEZ blocks in
their real format, collects the bridge's burns into the burn tree, follows the channel's key
rotations, checks that enough accredited sequencers inscribed blocks on top of the tip, and commits
the 17-word journal the vault reads. Measured numbers are in [RESULTS.md](RESULTS.md).

**Specification.** "SPEC-v2 §x" is [`docs/design/spec/bridge-spec-v2.md`](../docs/design/spec/bridge-spec-v2.md)
and "SPEC-v3 Dn" is [`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md).

Prototype evidence for a grant proposal, not production code: see "Not done" and "UNVERIFIED" below.

## Layout

| Path | What |
|---|---|
| `crates/checkpoint-core` | The rule as a pure `no_std` function (`#![forbid(unsafe_code)]`), used unchanged by the guests and the tests. `lez.rs` strict LEZ block parser and block hash; `mantle.rs` Logos Blockchain transaction framing, BLAKE2b tx hash and `MsgId`, inscription and `ChannelConfigOp` checks; `wire.rs` the bridge messages; `tree.rs` burn leaf and frontier; `state.rs` `configId`, `keySetHash`, `S`; `journal.rs` the ABI journal and the segment journal; `input.rs` the guest input format; `rule.rs` `run_checkpoint` and `run_segment`. |
| `fidelity/` | Host only. Builds real-format LEZ v0.4.0-rc1 blocks and transactions with LEZ's own crates and Logos Blockchain transactions with `logos-blockchain-core` at the revision LEZ pins; tests the core against them and against the real blocks in LEZ's committed sequencer dump; `src/bin/gen.rs` writes the bench fixtures. |
| `guests/` | Three zkVM guests (their own workspace, with RISC Zero's `sha2`, `curve25519-dalek` and `crypto-bigint` patches): `checkpoint`, `segment`, `micro` (component benchmarks). |
| `guests-plain/` | The `micro` guest built *without* those patches (A/B control). |
| `methods-segment/`, `methods/`, `methods-plain/` | Build the guests with risc0-build 3.0.5; the checkpoint guest gets the segment guest's image id at compile time (`FERRATA_SEGMENT_ID`). |
| `host/` | Executes the guests on the fixtures (journal compared with the native run), micro-benchmarks, the composition demo, proving, Groth16 wrapping. |
| `scripts/` | `prove-bench.sh` (proving with machine, load, GNU time and Docker memory logged), `derive.py` (derived figures from the JSON lines). |
| `artifacts/` | The Groth16 seals, journals and receipts of the real proofs (succinct receipts git-ignored). |
| `vectors/external/ferrata-bridge/` | Vectors of `ferrata-bridge`, now [`../bridge/`](../bridge/) (an independent implementation of the same SPEC-v2 encodings), checked by `crates/checkpoint-core/tests/cross_vectors.rs`. |
| `results/` | Logs and JSON lines of every measured run. |

## Run it

All tools write scratch under `tmp/` beside the repository, as `bench/` does (never `/tmp`), and use
one target directory. From the repository root, `cd checkpoint`, then:

```sh
export TMPDIR=$PWD/../../tmp CARGO_TARGET_DIR=$PWD/target
# Logos Blockchain's circuit crates download prebuilt witness generators at build time; keep that
# cache under that tmp/ instead of ~/.cache:
export XDG_CACHE_HOME=$PWD/../../tmp/xdg-cache

cargo +1.88 test -p checkpoint-core                       # unit tests + cross-implementation vectors
cargo +1.88 clippy -p checkpoint-core --all-targets -- -D warnings
cargo +1.98.1 test -p fidelity -- --nocapture             # fidelity to LEZ / Logos Blockchain, the rule's refusals
cargo +1.98.1 run -p fidelity --bin gen -- all            # fixtures/ (28 MB, git-ignored; ~25 min, LEZ's
                                                          # apply_block runs the fee and clock programs)
cargo +1.98.1 run -p fidelity --bin gen -- rejournal      # re-derive expected journals after a format change
cargo +1.93.0 build --release -p host                     # builds the guests too (risc0-build, rzup toolchain)
target/release/host ids | micro [--plain] | bench [name…] | compose [--real] | prove <name> | groth16 <name> | verify <name>
RISC0_DEV_MODE=1 target/release/host compose              # the two-piece composition demo
scripts/prove-bench.sh prove cp-empty-1 && scripts/prove-bench.sh groth16 cp-empty-1
python3 -I scripts/derive.py                              # derived figures from results/*.jsonl
```

`fidelity` needs LEZ's toolchain (Rust 1.98.1, `lez-v0.4.0-rc1/rust-toolchain.toml`); the core builds
with the RISC Zero guest toolchain (Rust 1.88). Upstream sources are used read-only by path, from
`upstream/` beside the repository (where `lez/` expects LEZ too): `upstream/lez-v0.4.0-rc1`
([LEZ](https://github.com/logos-blockchain/logos-execution-zone/tree/v0.4.0-rc1), tag v0.4.0-rc1,
1806e87) and `upstream/logos-blockchain-d480efc6` (a shallow fetch of exactly the revision LEZ pins of
[Logos Blockchain](https://github.com/logos-blockchain/logos-blockchain); an older clone at ee5e344
differs in `inscribe.rs`, `config.rs`, `transactions/codec.rs`, `ops/mod.rs` and more,
so it was not used). The workspace `[patch]` points LEZ's git dependencies on logos-blockchain at
that shallow clone, so the formats checked are the pinned ones and nothing else is fetched from it.

## What the program checks (SPEC-v2 §4.2 with SPEC-v3 D3, as implemented)

`checkpoint-core::rule::run_checkpoint`, in order; any failure refuses the input (the guest panics,
so there is no receipt):

1. `lezConfigId` is recomputed from its preimage (§1), which also fixes `channelId` and
   `bridgeHeader`. The parent state's burn frontier must be canonical for its count. With segments
   (composition, below), the segments must chain from `P` to the state the input continues from.
2. Key rotations: each `ChannelConfigOp` (supplied in the whole Mantle transaction that carries it)
   must be for the pinned channel, chained on the current `configTipHash`, well-formed (non-zero
   thresholds, non-empty, valid, non-small-order keys, as Bedrock's decoder and preverify require),
   and signed over the Mantle transaction hash by at least `t(current n)` distinct current keys
   (`(index, signature)` pairs, `verify_strict`). Its `posting_timeframe` must equal the one bound
   in the current key-set hash and must not be 0 (SPEC-v3 D3: the turn length is pinned). Every key
   set reached must have `n ≥ nMin`, `t(n) ≥ tMin` and no repeated key. The state carries the key
   list and the posting timeframe: `keySetHash = H("ferrata/keyset/v2" ‖ n:u32 ‖
   postingTimeframe:u32 ‖ key_1 ‖ … ‖ key_n)`.
3. Blocks `P.tip+1 … E`: each is parsed strictly, its hash recomputed
   (`SHA256("/LEE/v0.3/Message/Block/"‖0⁸ ‖ borsh(id, prev, timestamp, txs, reverted))`) and
   compared with the hash the block carries, its id must be the previous id + 1 and its
   `prev_block_hash` the previous hash.
4. In blocks up to `T`, every *visible* call to `bridgeHeader` of a non-reverted transaction is
   decoded as a bridge message; only messages for this `configId` count: `Burn` appends
   `burnLeaf(configId, burnCount, token, amount, COut, r)` (§5.1) to the burn tree; `ImportRoot` sets
   `importedLeafCount = max(importedLeafCount, leafCount)` unless `sunset`; `Sunset` sets `sunset`.
   Order: block, then transaction, then call order within the transaction.
5. After `T`: `Y = T` if `echoNonce = 0`, else the first block carrying `Echo{echoNonce}` in a call to
   `bridgeHeader` (reverted or not: inclusion is what dates the block).
6. Producer (SPEC-v3 D3): one inscription of `T` is checked exactly like a counted one (pinned
   channel; payload = `T`'s exact inscribed bytes; BLAKE2b-256 Mantle transaction hash recomputed
   over the whole transaction; Ed25519 `verify_strict` by the signer named in the op). Its signer
   `D` must be in the key set `K`.
7. Support: checked inscriptions of blocks in `(Y, E]` by at least `t(n)` distinct keys of `K` other
   than `D`. Every supplied inscription must check; one by `D` is not counted. A one-key set is
   allowed only when `nMin ≤ 1`, and then the one key is `D` and the supporter.
8. Journal (SPEC-v3 D3): `abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault, lezConfigId, nMin, tMin,
   parentState, newState, burnRoot, burnCount, importedLeafCount, sunset, echoNonce, tipId, tipHash,
   n, t)`, 544 bytes, `CHECKPOINT_DOMAIN = sha256("ferrata/vault/checkpoint/v3")`; `newState = S` at
   tip `T` (§4.1); `tipId`, `tipHash` are `T`'s; `n`, `t` are `|K|` and `t(n)` of the key set in
   `newState`.

Composition: the `segment` guest proves `S_a → S_b` over a bounded run of blocks (steps 3–4 only) and
commits `"ferrata/checkpoint-segment/v1" ‖ lezConfigId ‖ S_a ‖ S_b`. The `checkpoint` guest accepts a
list of such journals, verifies each with `env::verify(SEGMENT_ID, journal)` (the segment image id
is compiled into the checkpoint guest), checks they chain from `P` to the state it continues from,
and does the rest. Segments are independent of each other, so a backlog can be proven in parallel
pieces and joined by one checkpoint; the vault sees one journal and one image id.

## Real format vs stand-in

| Thing | Status |
|---|---|
| LEZ block encoding, block hash, id/link rules | **Real.** Blocks built with LEZ's `HashableBlockData::into_pending_block`; every parsed field, every transaction's byte span and the hash equal LEZ's own (`fidelity/tests/formats.rs`), including the 7 blocks of LEZ's committed sequencer database dump (`test_fixtures/fixtures/prebuilt_sequencer_db.dump`, made by LEZ's sequencer and wallet). |
| Empty blocks | **Real.** The forced fee transaction and the clock transaction, built by LEZ's `fee_invocation`/`clock_invocation` with the summary and payout from `derive_block_summary`, on LEZ's testnet genesis state; every such block is accepted by LEZ's `apply_block`. 839 bytes (fee tx 447, clock tx 303), the same size as the empty blocks in LEZ's dump. |
| Public transactions | **Real format.** Native transfers executed by `apply_block`; calls to the bridge signed with LEZ's `WitnessSet::for_message`, with a `FeeDeclaration`. The bridge calls cannot execute (no bridge program exists), so blocks holding them are *stand-in blocks*: real format, fee and clock transactions from the equivalent empty block, `reverted` chosen by the test. |
| Privacy-preserving transactions | **Message part real, proof stand-in.** (a) The two PP transactions in LEZ's dump (made by LEZ's wallet; their receipts are `RISC0_DEV_MODE` receipts), used as they are for parsing tests and, for the benches, with the receipt replaced by 223,551 filler bytes (`PROOF_BYTES`, LEZ's fee market). (b) Shapes built from LEZ's own types (`ProvenExecution`, `Delivery`, `PrivateAction`, …) with bridge deliveries, a public root, sealed messages, both program-claim kinds and a fee fallback: real format, not provable. |
| Inscriptions | **Real.** Mantle transactions `[ChannelInscribe, Transfer]` (and with `ChannelTransfer` / `ChannelWithdraw`) built and signed with `logos-blockchain-core` at `d480efc6`; the core's transaction hash, `MsgId` and signature check equal upstream's. The fee transfer's note ids and values are made up (format real). |
| Channel configuration changes | **Real.** `ChannelConfigOp` in a Mantle transaction, built with upstream types; id (`config_tip_hash`), transaction hash and `(index, signature)` multi-signature as Bedrock verifies them. |
| Bridge messages | **Defined here** (`wire.rs`): the bridge program does not exist yet. |
| Keys, configuration values, user accounts | Test values. |

### Transaction shapes the extraction handles

| Shape | Handled |
|---|---|
| Public transaction whose root call targets `bridgeHeader` (`to.program_account_id`) | yes |
| Privacy-preserving transaction: each `PrivateToPublic` boundary delivery to `bridgeHeader` (how a burn from a private holding reaches the public bridge) | yes |
| Privacy-preserving transaction: `public_root` call to `bridgeHeader` | yes |
| Reverted transactions (`BlockBody::reverted`) | skipped (but counted for the echo) |
| `PublicToPrivate` deliveries (the bridge replying to a private actor) | ignored: not a public execution of the bridge |
| `ProvenFee.fallback.boundary` deliveries | ignored: they apply only when the transaction reverts |
| A message to `bridgeHeader` that is not exactly one `Burn`/`ImportRoot`/`Sunset`/`Echo`, or is for another `configId` | ignored |
| A public program calling the bridge inside a public execution (sync message between public actors) | **not visible in the block, cannot be extracted** |
| Async messages to the bridge, the bridge running as a private actor | **not visible, cannot be extracted** |

The bridge program must therefore act on `Burn`, `ImportRoot` and `Sunset` only when they reach it as
a root call or a private→public boundary delivery, and must decode with `wire.rs`; otherwise a burn
could destroy tokens without a burn leaf. Whether a v0.4 program can tell those cases apart is not
checked here (UNVERIFIED).

### Inscription transactions the support check parses

Mantle opcodes `0x00` Transfer, `0x10` ChannelConfig, `0x11` ChannelInscribe, `0x13`
ChannelWithdraw, `0x14` ChannelTransfer: what LEZ's sequencer puts next to a block inscription
(`fund_ops` appends the wallet's fee transfer; deposit pins add a channel transfer; atomic
withdrawals a channel withdrawal). Any other opcode is refused rather than skipped.

## Choices this implementation makes where SPEC-v2 is silent or would be unsafe as written

| Item | SPEC-v2 | Here | Why |
|---|---|---|---|
| `burnFrontierHash` encoding | "hash of the frontier (32 nodes)" | `sha256("ferrata/burn-frontier/v1" ‖ f_0 ‖ … ‖ f_31)`, `f_h` = 0 when bit `h` of the count is 0 | The encoding `ferrata-bridge` (now `../bridge/`) chose; checked against its vectors. Zeroing makes the hash a function of the leaves alone. |
| Burn tree capacity | depth 32 | at most 2^32 − 1 leaves | A frontier cannot store the last leaf (as in Ethereum's deposit contract). |
| `T`'s producer | SPEC-v2: "other than the key that inscribed `T`"; SPEC-v3 D3: one checked inscription of `T` by `D ∈ K` | as SPEC-v3 (implemented before v3 arrived) | Without the check the prover names `T`'s producer freely (any key can sign *an* inscription of `T`'s bytes); see RESULTS, finding 1. |
| `allowSingleKey` | a deployment constant, not in the journal | allowed exactly when `nMin ≤ 1` | The vault pins `nMin` through the journal; a constant the journal does not carry could be set by the prover. |
| `importedLeafCount` | "successful `ImportRoot` updates it" | `max(current, leafCount)`, ignored after `Sunset` | Lowering it would let leaves already claimable on LEZ be refunded on Ethereum. |
| Rotation threshold | "at least `t(current n)`" signatures | at least `t(current n)` distinct current keys, indices unique; repeated keys in a set refused | Bedrock itself requires *exactly* the channel's recorded `configuration_threshold` (LEZ sets it to `t(n)`), and its key list is a vector that would let one key fill two indices. |
| Echo in a reverted transaction | silent | counts | Inclusion in a block built after the nonce was known is what the echo proves. |
| Echo variant | silent | `Echo{nonce}` has no `configId` | As the prototype's wire format defines it (`wire.rs`). |
| Support entries by `D` | SPEC-v3: support is by keys "other than `D`" | checked, not counted | A useless entry costs the prover cycles, nothing else. |
| Empty input | silent | at least one block up to `T` is required | A pure key-rotation checkpoint is not supported. |
| Parent state with `postingTimeframe = 0` | SPEC-v3 D3 refuses *ops* with 0 | not refused (as written); no rotation can then ever apply (each op either differs or is 0) | A genesis state with 0 must be caught by the deployment check (`turnSeconds` = timeframe × slot). Reported in RESULTS. |

## UNVERIFIED

1. That LEZ's sequencer, on a live network, inscribes blocks only in Mantle transactions made of the
   opcodes above, and only its *own* blocks (an honest key that re-inscribed a peer's block would
   weaken the "other than `T`'s producer" argument). Read from `lez/sequencer/actors/bedrock` and
   zone-sdk `tx_builder.rs` at the pinned revision; not observed on a network.
2. That a burn from a private holding reaches the bridge as exactly one `PrivateToPublic` delivery
   whose `message` is the `Burn` encoding: it depends on how the (unwritten) token and bridge programs
   are built. The delivery format itself is LEZ's and is tested.
3. That a v0.4 program can refuse the calls that would not be visible in a block (another public
   program calling it; being run as a private actor): the bridge program needs this; not checked.
4. Privacy-preserving transactions with real (non-dev-mode) proofs: none was produced; the benches
   use LEZ-wallet message parts with filler proofs of the size LEZ's fee market assumes (223,551 bytes).
5. secp256k1 public keys in LEZ witness sets and admission evidence are taken as 32 opaque bytes; LEZ's
   decoder validates them (the mutation test finds exactly this difference). A block that only differs
   in this way is invalid for LEZ and cannot gather honest support.
6. RISC Zero patch versions: `sha2-v0.10.9-risczero.0`, `curve25519-4.1.3-risczero.0` and
   `crypto-bigint v0.5.5-risczero.0` match the versions LEZ and logos-blockchain lock (sha2 0.10.9,
   curve25519-dalek 4.1.3); they build and run under r0vm 3.0.5 and the A/B run shows they are active
   (RESULTS). Whether RISC Zero lists exactly these tags for 3.0.5 was not checked against its
   documentation (no web access during this build).
7. Guest image ids depend on the local RISC Zero toolchain (Rust 1.88, no Docker build): not
   reproducible elsewhere as they stand.
8. Network parameters: 10 s blocks (LEZ's testnet config), committee sizes `n` = 3, 4, 7. The live
   testnet-0.4 values are unknown.

## Not done

- The vault side (§4.3–§4.5, SPEC-v3 D4–D5) is in [`../vault/`](../vault/); the Groth16 seal produced here
  was not submitted to a contract.
- Composition was demonstrated in dev mode only; `host compose --real` (real recursion) was not run.
- Mode (B) of SPEC-v3 D7 is estimated from measured components (RESULTS §4), not built.
- No real LEZ node or Bedrock node was run; no proof of LEZ block validity (O2) or Bedrock finality
  (O1) is attempted, as SPEC-v2 intends.

## Network use

Crates from crates.io; git dependencies of LEZ's crates (resolved to the pinned logos-blockchain
revision through the workspace `[patch]`, plus Overwatch, logos-blockchain-circuits, jellyfish,
rust-rapidsnark, spongefish); RISC Zero's patch forks; a shallow `git fetch` of logos-blockchain at
`d480efc6`. Building logos-blockchain's circuit crates downloads their prebuilt witness generators and
rapidsnark from those projects' GitHub releases (kept under that `tmp/` with `XDG_CACHE_HOME`). The
Groth16 prover image was already present. Nothing was uploaded.
