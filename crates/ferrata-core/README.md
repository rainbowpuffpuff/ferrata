# Ferrata core

An Ethereum sync-committee light client in Rust, `no_std`, written as the verification core of
**Ferrata**, a proposal for [Logos RFP-021](https://github.com/logos-co/rfp) (trustless
Ethereum state attestation on LEZ). The name comes from the *via ferrata*, a mountain route with
fixed anchors that anyone can cross without a guide. This repository is the proposal's evidence.
The light client also runs as a LEZ v0.4.0-rc1 program (`lez/`), driven with mainnet data through
LEZ's own state transition functions and fee settlement; [`lez/RESULTS.md`](../../lez/RESULTS.md) has
what it does, what each step costs, and what is missing.

This file documents the core, its test crate (`crates/ferrata-spectests`), `bench/` and `lez/`; it
was the repository's README before the bridge prototypes joined it, and the
[repository README](../../README.md) now gives the overview. Paths below are relative to the
repository root, and commands run from there.

## Results

Every row is produced by `cargo test --workspace`.

| Suite | Result |
|---|---|
| `light_client/sync`, consensus-specs v1.7.0-beta.4, minimal preset (32-member committees), Altair to Gloas, including fork transitions and stores fed older forks' data | **52 / 52 cases**, 185 steps (160 `process_update`, 21 `force_update`, 4 `upgrade_store`). Finalized and optimistic headers, beacon and execution roots, are checked after every step. |
| The same, four times | `blst` with additive, subtractive and automatic key aggregation, and zkcrypto `bls12_381`: identical |
| `light_client/update_ranking` | **7 / 7 cases**, 399 updates in canonical order |
| Real mainnet data (`fixtures/mainnet-p1899`) | Periods 1899 to 1903 (Fulu): 512-member committees, four handoffs, 510 to 512 signers per update, with both committee checks (full and on-chain). Every finalized root equals the canonical root a second provider reports. A test also verifies one of these signatures through the split API's 576-byte encoding. |
| Official BLS vectors (61 cases) | consensus-specs `eth_fast_aggregate_verify` (12) and `eth_aggregate_pubkeys` (8); ethereum/bls12-381-tests `fast_aggregate_verify` (12) and `verify` (29). Both libraries, all three compositions; both verification paths agree. |
| Policy store against the spec store | Equal state on 150 steps. After a forced update the two diverge, and the policy store never holds a header the spec store did not finalize (checked at every step). |
| Job-step API against the policy store (`tests/steps.rs`) | The steps the LEZ program runs (`steps.rs`, over the compact store and witnesses of `witness.rs`) accept exactly what `FinalizedStore::process_update` (`Adoption::OnChain`) accepts, with the same store and refusal code: all 52 `sync` cases (160 updates: 91 applied, 69 refused), the mainnet replay with the committee adoption in 1, 2 and 4 parts, and 21 variants of a mainnet update. Step 2's product equals `Blst::miller_half` byte for byte; skipped, replayed and misdirected steps are refused; the uncompressed-key adoption agrees with `check_committee_onchain` on 9 real and 6 crafted committees. |
| Adversarial tests | 16 (listed below) |
| Guard tests | 23: each pins a check that an independent review showed could be deleted without any test failing, plus the two committee checks |
| **Execution layer** | |
| Real mainnet, end to end (`fixtures/mainnet-e2e-2026-11-09`) | A bootstrap (slot 15,613,952) and a finality update (512/512 signers) of period 1906, verified with both libraries and both committee checks, finalize execution block 26,379,842; its `HeaderRecord` equals two providers' block. WETH, USDC and an address without an account: every fact equals `eth_getProof` and `balanceOf`. The receipts trie, rebuilt with alloy-trie, equals the record's root, and all 870 logs of its 245 receipts (types 0, 2, 3, 4; 54 USDC transfers) are attested. EIP-2935 hops reach the parent block, whose 3 failed transactions are refused, and n − 16, then n − 16 − 8191 (the window's edge), with a storage fact under each of these two. Tampering with each kind of fixture byte is refused, each with its expected code (a corrupted signature with either BLS code, as in the adversarial tests). |
| Real Sepolia data through the Gloas fork (`fixtures/sepolia-gloas-2026-11-10`, `tests/sepolia_gloas.rs`) | Sepolia activated Gloas at epoch 353,024, the first slot of period 1379. The one public Sepolia node found serving light-client data (Lodestar) serves no bootstrap for a Gloas-era checkpoint: HTTP 500 (Lighthouse answers 404 to every light-client request; see the fixture's `SOURCE.md`). So the stores start from the bootstrap of period 1378 and follow the real chain: the period-1378 update, the upgrade to the Gloas layout, the update of every period from 1379 to 1409 (31 committee handoffs under Gloas, 506 to 512 of 512 signers), and a Gloas finality update (477/512) that finalizes slot 11,547,104. Both libraries, both committee checks, policy and spec store: identical. Every finalized root equals the canonical root a second provider reports, and every update's fork digest is the one computed for Sepolia. The `HeaderRecord` comes from the bid's parent block hash and the real RLP header (Amsterdam, 23 fields) of block 12,103,422, and equals two providers' block. Tampering is refused, each with its expected code (a corrupted signature with either BLS code): the bound or the attested block hash, the execution or the finality branch, the signature, a signer bit, mainnet's `genesis_validators_root`, a Gloas update to a store not upgraded, a next-committee key, five flipped bytes of the RLP header and a truncated one, and the payload-header constructor on a Gloas store. |
| `ethereum/tests` TrieTests (pinned at `c67e485`) | **25 / 25 cases**, every root, and 368 inclusion and exclusion proofs (`trietestnextprev.json` tests iteration, not proofs) |
| Merkle-Patricia proofs against alloy-trie 0.9.9 | 240 random tries in four key families (179 with embedded nodes): equal roots, byte-identical proofs, 19,173 inclusion and 8,968 exclusion proofs accepted by both; 4,453 mutated proofs refused by both. Two documented differences (below). |
| Strict RLP against alloy-rlp 0.3.17 | 21 non-canonical or malformed encodings, each refused with its code (and by alloy-rlp); 3,000 random round trips; 20,000 mutated encodings with the same verdict from both (6,237 accepted, 13,763 refused) |
| Execution objects | Headers of the Cancun (20 fields), Prague/Osaka (21) and Amsterdam (23) layouts and every field rule; accounts; storage values; receipts of types 0 to 4, pre-Byzantium refused; record encoding; statement identifiers equal to an independent Python computation |
| Execution adversarial tests | 7 tests (listed below), each refusal checked for its exact code |
| Accumulator-leaf facts (`vectors/acc-leaf.json`) | Vectors written by an independent Python implementation and shared with the bridge crate: 14 leaves in two snapshots, 7 refusals with their codes, 4 identifier variants. A leaf proven under two roots attested at two blocks has one identifier; every sibling and every index bit is bound; an index of 2^32 or more and the empty leaf are refused. |
| Differential oracle: Helios (`helios-consensus-core` 0.12.0) | 33 of the 52 official `sync` cases replayed step by step through Helios and ferrata's spec store (129 steps: 111 updates, 15 forced updates, 3 store upgrades), among them all 7 whose chain schedules Gloas: identical verdicts, finalized and optimistic headers after every step. The other 19, each listed, hold Altair-layout data, which Helios cannot decode. Mainnet periods 1899–1903, the mainnet end-to-end finality update, and Sepolia through its Gloas fork (32 updates and a Gloas finality update): identical. 8 mutated mainnet updates, a store holding the wrong committee, and 11 mutated Sepolia Gloas inputs: refused by both. Participation 0 to 32 of 32 with real signatures: identical. Three inputs on which Helios 0.12.0 departs from the spec, and two on which 0.9.4 did and 0.12.0 no longer does (below). |
| Signature checks on every path | The identity, a point outside G2, a point off the curve and two bad encodings are refused by the spec path, the checked path and the split path (`miller_half` / `final_half`), with both libraries and every key aggregation, and by both stores |
| Mutation run | 107 hand-picked mutants, all killed (`scripts/mutants.py`, `docs/mutants-run.log`). 43 cover the light client; 32 of those come from an independent review, under which 20 had survived the earlier suite. 64 cover the execution layer: length checks, hash comparisons, indices and bounds, canonical-form checks, the receipt status check, the EIP-2935 window, identifiers, predicates and relations, accumulator paths and identifiers, log-index conversion. |
| `no_std` | Builds for `riscv32im-unknown-none-elf` with the pure-Rust backend. The `blst` backend needs a RISC-V C toolchain (RISC Zero's), which CI does not set up; `bench/` and `lez/` build it for RISC Zero's guests with that toolchain. |

| RISC Zero guests and the LEZ program | Result |
|---|---|
| RISC Zero guest (`bench/`) | RISC Zero zkVM 3.0.5 executor, real mainnet update, RISC Zero's accelerated blst: a finality update costs 10.2M cycles, and splits into steps of 2.3M, 4.2M and 3.6M through the core's `miller_half` / `final_half`. A handoff adds 1.1M for the committee root, plus either 4.6M for the adoption check with uncompressed keys or 48.7M with compressed keys (`check_committee_onchain` today). See `bench/RESULTS.md`, including six library configurations. |
| RISC Zero guest, execution layer (`bench/exec/`) | The same executor, mainnet block 26,379,842, with RISC Zero's accelerated Keccak (without it in brackets): `attest_storage` 0.21M user cycles (1.04M), `attest_log` 0.04M for a one-log receipt and 0.43M for a 96-log one (0.21M, 2.58M), one EIP-2935 hop 0.18M (0.95M). With the accelerator, the Keccak-f permutations (54 for a storage slot, 50 for a hop) are proven by RISC Zero's separate Keccak circuit, outside these counts. See `bench/RESULTS.md`. |
| LEZ v0.4.0-rc1 program (`lez/`) | LEZ's own settlement with mainnet data, on the program image built after the merge (`cec0bc68…`; these figures supersede the earlier image's, and came out the same): `Init` 3 transactions, 7.09M execution gas; a finality update 3 transactions (2.19M, 3.31M, 3.72M; 9.22M); an update that adopts a committee 5 transactions, 16.08M; every transaction under the 5M target, the largest 3.73M (75%). The whole update in one public transaction (`VerifyAll`): 9.12M. Verified privately (dev mode): 479,854 execution gas and 224,063 storage gas (482,524 on the current image). Two real proofs of that path exist, both of earlier images: `e7cec90e…` on a laptop CPU (9.05M user cycles, 54 min, 9.18 GiB, a 227,080-byte receipt) and `cec0bc68…` on one NVIDIA L4. The current image (`9f3eb00f…`, which adds attestation, the store's upgrade in place and attestation under Gloas records) measures within 0.6% of these figures and has no real proof. On a mainnet block finalized on 10 November 2026 it attests a storage slot in 0.27M cycles and an ERC-20 `Transfer` log in 0.10M, each in one transaction against the record it wrote; a reference consumer's claim of that log is one transaction of 0.18M, and a privately executed consumer verifies the same log in 0.38M user cycles with only the read of the record public, proven on one NVIDIA L4 (16.1 s). On Sepolia it follows the chain from the period-1378 bootstrap through the Gloas fork to period 1409 (166 transactions, the store upgrading itself), and attests a storage slot (0.28M), a log and an account under a Gloas-era record with the block's RLP header as witness; the vault's deposit leaves run in the guest (about 65,500 cycles each, synthetic state). See `lez/RESULTS.md`. |

**Not done yet** (the proposal's M0 to M2):
- in the LEZ program's attestation (`lez/RESULTS.md` §5, §6): accumulator leaves on real data
  (they run in the guest on a synthetic state; the vault on Sepolia, deployed on 11 November 2026, has not been
  attested this way), a chain of several
  hops, history hops and the private path under a Gloas record, and of the consumer pattern the
  nullifier keyed on a depositor's secret, with it a private claim;
- in the program (`lez/RESULTS.md` §7): a fork that is not in the schedule fixed at `Init` (a
  scheduled one is crossed in place), successors and the root id of the proposal's §1, a submitter daemon, joining an open job, and
  clean-up of jobs whose last step fails; deployment by `program_loader` transactions (it is
  deployed at genesis); settlement through LEZ's `chain_state` crate itself (the harness runs a
  copy of its two settlement functions);
- a real proof of an update on the current program image (`lez/RESULTS.md` §3 says how to make
  it): the two real proofs of a finality update verified privately are of earlier images; the
  private attestation was proven once, on a cloud GPU; the handoff ran in RISC Zero dev mode
  only;
- the `single_merkle_proof` vectors;
- under Gloas on real data, a bootstrap: no public Sepolia node found serves a light-client
  bootstrap for a Gloas-era checkpoint yet (Lodestar answers HTTP 500 "syncCommitteeWitness not
  available", Lighthouse 404), so the Sepolia fixtures start one period before the fork (the
  facts of a Gloas-era block are attested through the LEZ program, `lez/RESULTS.md` §6; mainnet
  has no Gloas date);
- fuzzing (`cargo-fuzz` on RLP, Merkle-Patricia and query decoding) and `cargo-mutants` over the
  whole core; no hand-picked mutant covers `steps.rs` or `witness.rs`, and `docs/REVIEW-core.md`
  does not review them;
- a weak-subjectivity bound (see "Weak subjectivity" below), in the core and in the program.

## Run it

```sh
scripts/fetch-vectors.sh          # pinned releases and TrieTests; hashes verified on every run
cargo test --workspace -- --nocapture
scripts/mutants.py                # optional: the 107-mutant run (78 minutes here, on a loaded machine)
```

The mainnet and Sepolia fixtures are committed. `scripts/fetch-mainnet-e2e.py OUTDIR` and
`scripts/fetch-sepolia-gloas.py OUTDIR` show how the end-to-end and the Sepolia ones were made;
run today, each fetches newer data into a new directory.
`vectors/acc-leaf.json` is committed too, and `scripts/gen-acc-leaf-vectors.py` rewrites it.
The Helios oracle is a git dev-dependency of the test crate only (a16z/helios tag 0.12.0, rev
`2af9b41`), fetched by cargo; `ferrata-core` never depends on it.

The vector tests fail rather than skip when the vectors are missing, and they pin the number of
cases, so an empty or moved directory cannot pass.

`lez/` is a workspace of its own, outside the root one: it needs RISC Zero's guest toolchain and
a clone of LEZ v0.4.0-rc1 beside this repository. `lez/RESULTS.md` (§8) has the commands.

## Layout

| Path | What |
|---|---|
| `crates/ferrata-core/src/validate.rs` | `validate_light_client_update`, `is_valid_light_client_header`, `get_lc_execution_root`, `is_better_update` and the gindex functions, each a transcription of the spec function of the same name |
| `crates/ferrata-core/src/store.rs` | The spec's `LightClientStore` (optimistic header, best valid update, forced updates), used to replay the vectors |
| `crates/ferrata-core/src/policy.rs` | `FinalizedStore`, Ferrata's on-chain policy |
| `crates/ferrata-core/src/upgrade.rs` | Light-client data across forks, including Gloas's `execution_block_hash` |
| `crates/ferrata-core/src/types.rs`, `ssz.rs`, `merkle.rs` | Containers, strict SSZ decoding, Merkle helpers and generalized indices |
| `crates/ferrata-core/src/bls/` | The `BlsBackend` trait, with `blst` and zkcrypto backends |
| `crates/ferrata-core/src/rlp.rs`, `mpt.rs`, `keccak.rs` | Strict RLP views, Merkle-Patricia proof verification, Keccak-256 behind one function |
| `crates/ferrata-core/src/exec.rs` | Execution headers, accounts, storage values, receipts and logs |
| `crates/ferrata-core/src/attest.rs` | `HeaderRecord`, the attestations consumers call, identifiers, conditions and relations, the EIP-2935 hop |
| `crates/ferrata-core/src/accumulator.rs` | Accumulator-leaf facts: a root attested from a storage slot, a leaf proven against it by a SHA-256 path |
| `crates/ferrata-core/src/witness.rs` | Compact forms of the store, bootstrap and update for a program's state and messages, with strict encodings; the record a store keeps of each finalized header; the signer witness (non-signers' keys with Merkle branches); the committee adoption from uncompressed keys, in parts |
| `crates/ferrata-core/src/ws.rs` | The weak-subjectivity period (phase 0 and Electra) and the bound on a store's finalized header |
| `crates/ferrata-core/src/steps.rs` | The job-step API: `process_update` split into steps of under 5M cycles (validation and hash-to-G2, the Miller loops, the final exponentiation), the adoption, and `apply` to the store the job started from |
| `lez/` | The light client as a LEZ v0.4.0-rc1 program: wire types and guests, the harness that drives them through LEZ's settlement, the results (`lez/RESULTS.md`) |
| `crates/ferrata-spectests/src/execkit.rs` | A reference trie (independent Keccak), synthetic blocks, JSON-RPC readers |
| `scripts/fetch-mainnet-e2e.py` | The fetch tool of the mainnet end-to-end fixture (standard library only) |
| `scripts/gen-acc-leaf-vectors.py`, `vectors/acc-leaf.json` | The accumulator vectors and their independent generator (hashlib) |
| `crates/ferrata-spectests/tests/sepolia_gloas.rs`, `scripts/fetch-sepolia-gloas.py` | Real Sepolia data through the Gloas fork, and the fixture's fetch tool (standard library only) |
| `crates/ferrata-spectests/tests/helios_oracle.rs` | The differential oracle against Helios |
| `scripts/count-lines.py` | The line counts of "Reuse and audit surface" |
| `crates/ferrata-spectests/` | The vector runner, the fixtures and the test suites |
| `docs/REVIEW-core.md` | The author's per-function review against the spec |

## Design notes

1. **Two stores, one validation.** The spec store and the policy store call the same
   `validate_light_client_update` and the same `apply_core`. The policy accepts only finality
   updates signed by at least two thirds of the committee. It keeps no optimistic header and
   never forces one.

2. **The liveness limit of a finalized-only client.** If Ethereum goes a whole sync-committee
   period without a finality update that hands off the next committee, the policy store cannot
   learn that committee. It stops at its last finalized handoff, and the instance must be
   replaced. The spec's client keeps going by forcing a non-finalized header. The official
   `light_client_sync` vector exercises exactly this, and
   `stops_safely_after_a_period_without_finality` asserts the stop on all seven forks.

3. **Two BLS paths and two committee checks.**
   - `fast_aggregate_verify` is the spec's `FastAggregateVerify` as the pyspec runs it: every
     signer key must pass `KeyValidate`. The spec store uses it.
   - `verify_checked` serves committees that the policy store checked when it adopted them. The
     signing key is then `aggregate_pubkey − Σ non-signers`, a handful of additions at mainnet
     participation, with no per-update subgroup check.
   - `check_committee` runs `KeyValidate` on every key and checks the sum. After it, the two paths
     agree exactly. `check_committee_onchain` is what the program checks (`lez/` runs it from
     uncompressed keys, `witness.rs`, and `tests/steps.rs` shows the two agree): every key must be on
     the curve and not the identity, and the keys must sum to the aggregate. It skips the per-key
     subgroup check, which costs 185M cycles per committee in the guest (`bench/`), because every
     key in a genuine beacon state passed `KeyValidate` at deposit. The off-chain tools run the
     full check.
   - The vectors and the mainnet replay run under both checks (`Adoption::Full` and
     `Adoption::OnChain`).

4. **The pairing splits.** blst computes a Miller-loop product and then one final
   exponentiation. `Blst::miller_half` returns the product as a canonical 576-byte encoding, and
   `Blst::final_half` finishes the check from it, so a program can store the product between
   transactions. zkcrypto computes a single `multi_miller_loop` and offers no such seam.

5. **Both libraries report the same codes.** blst refuses the order-3 points (x = 0) while
   decoding, and zkcrypto refuses them at its subgroup check. Both checks are part of
   `KeyValidate`, so both report `PubkeyInvalid`. An earlier version validated only the aggregate,
   and there the two libraries returned different codes for the same input.

6. **Gloas.** Under EIP-7732 a block commits to a payload bid, and the payload can be withheld.
   The light-client header therefore binds the bid's `parent_block_hash` (gindex 2,856). Older
   headers carried into a Gloas store become a block hash plus a zero-padded branch, and the guard
   tests check that the padding must stay zero. The native Gloas vectors bind a single constant
   block hash; the Sepolia fixture binds real ones, and Gloas headers are tampered with on both.

7. **Execution roots come only from records.** A `HeaderRecord` holds one block's number, hash,
   state and receipts roots, the chain id and the binding it came from. It is built from a
   `FinalizedStore`'s finalized header (the payload header up to Fulu; under Gloas, the RLP
   header that hashes to the bound block hash), derived through EIP-2935, restored from the record
   the LEZ program wrote for a finalized header (`from_recorded`), or from its own bytes. Every attestation takes a record and the chain id the consumer
   expects; nothing takes a root from the caller. A record's `anchor_slot` is the beacon slot of
   the light-client header it rests on; for a record reached through EIP-2935 hops it is the slot
   of the record the hops started from, not a slot of the older block (place that block by its
   number and timestamp).

8. **Identifiers name facts.** A storage fact's identifier is SHA-256 over a tag, the chain id,
   the address, the slot and the value; an account's over the chain id, the address and its four
   fields. The block is left out, so the same value read at two blocks has one identifier (the
   mainnet test shows this for a real balance). That suits "this value holds", not "this event
   happened once": a balance that changes and changes back gets the same identifier, so
   once-per-event consumers key on logs or accumulator leaves (note 12), whose identifiers are one
   per event. A block-bound variant (`id_at_block`, kinds `0x11` and `0x12`) adds the block number,
   for consumers that pin a block (a snapshot) and want one use per pinned block; it does not
   identify events either, since an unchanged value at two blocks gets two of them. `attest.rs`
   documents the exact derivations, and tests pin both variants against an independent
   implementation.

9. **Header shapes.** The decoder accepts the Cancun (20 fields), Prague/Osaka (21) and Amsterdam
   (23) layouts. Amsterdam, as `execution-specs` defines it today, adds `block_access_list_hash`
   (EIP-7928) and then `slot_number` (EIP-7843). Anything else is refused, including unknown
   trailing fields, so a fork that adds a header field needs a new release before it activates
   (`exec.rs` explains the choice).

10. **EIP-2935.** A hop takes the older block's number from its header, never from the caller,
    requires `R − 8191 ≤ n < R`, the history contract with the code EIP-2935 specifies, and slot
    `n mod 8191` holding the header's hash. Hops chain; the mainnet test makes three real ones.

11. **Two differences from alloy-trie.** The empty trie is proven only by the empty proof
    (alloy-trie also accepts `[0x80]`), and a trie whose root node is shorter than 32 bytes is
    still rooted by its hash (alloy-trie 0.9.9 refuses every proof of such a trie; Ethereum's
    state, storage and receipt tries never have one).

12. **Accumulator facts.** Some contracts keep the root of a SHA-256 Merkle accumulator in one
    storage slot (the Ferrata bridge vault keeps its deposits that way). `attest_accumulator_root`
    is a storage attestation whose value is that root; `attest_accumulator_leaf` proves a leaf
    against it: depth 32, inner node `SHA-256(0x01 ‖ left ‖ right)`, empty leaf zero, direction
    bits from the leaf index, which must be below 2^32. Its identifier is
    `SHA-256("ferrata/fact/acc-leaf/v1" ‖ chain_id ‖ address ‖ slot ‖ leaf_index ‖ leaf)` (chain id
    and slot as 32 bytes, index as 8), and leaves the root out, so a leaf has one identifier under
    every later root. The leaf is opaque to the core: nothing is specific to one contract.

13. **Log positions.** `attest_log` takes the log's position in its receipt, counted from zero.
    JSON-RPC's `logIndex` counts across the whole block; `exec::locate_log` converts it, given each
    receipt's log count, and `exec::block_log_index` converts back (a test checks both on all 870
    logs of the mainnet block). A wrong conversion proves another log or none, never a false one.

14. **Relations.** Besides conditions against constants, a batch can compare two attested values
    of one record (`Relation`, soft requirement #4): for example "this account's balance is at
    least that slot's value".

15. **Where the signature is checked.** The aggregate signature must be a canonical compressed
    encoding of a point of G2 (on the curve, in the prime-order subgroup) and not the identity.
    With blst this is `miller()` in `bls/blst_backend.rs`, which every path calls before any
    pairing, `miller_half` included (`final_half` only finishes a product `miller_half` made);
    with zkcrypto, `pairing_check()` in `bls/zkcrypto_backend.rs`. `tests/signature_paths.rs`
    checks every path and both stores.

## Weak subjectivity

`ws.rs` transcribes the specification's weak-subjectivity period (`compute_weak_subjectivity_period`,
phase 0 and Electra, consensus-specs v1.7.0-beta.4) and `is_within_weak_subjectivity_period`, with the
store's finalized header standing for the weak-subjectivity state. A policy store given a period
(`FinalizedStore::with_weak_subjectivity_period`) refuses every update once the caller's
`current_slot` is past the period of its finalized header (`OutsideWeakSubjectivityPeriod`, 53),
however validly signed; such a store is replaced by one from a new bootstrap of a trusted checkpoint
inside the period. Without a period nothing is checked (the vectors run so). The period needs the
checkpoint state's total active balance, so it is computed off chain and fixed per instance.

A clock is needed: the spec's period rule keeps a valid update's signature slot within two
sync-committee periods of the finalized slot (512 epochs on mainnet), and the period is at least 256
epochs (3,532 on mainnet), so an update's own signature slot can never show a store to be stale on
mainnet. Off chain the caller's clock gives `current_slot`; the LEZ program uses the sequencer's clock
through each transaction's timestamp validity window (`lez/results/a1-successors-ws.md`). Tests:
`tests/weak_subjectivity.rs`; rows: `docs/REVIEW-core.md`, last section.

## Differential oracle: Helios

`tests/helios_oracle.rs` runs Helios's consensus core (`helios-consensus-core` from a16z/helios,
tag 0.12.0 of 5 October 2026, rev `2af9b41`; the light-client logic of the Helios lineage that SP1
Helios and `r0vm-helios` derive from) beside ferrata on the same bytes. It is a dev-dependency of
the test crate only, locked at the dependency versions of Helios's own lockfile (alloy 2.1.0).
Until November 2026 the pin was tag 0.9.4 (rev `3686d99`); what is said below of 0.9.4 is said
of that tag only.

- **What Helios can express.** At 0.12.0 its types decode light-client data in the Capella,
  Deneb, Electra and Gloas layouts, and its fork schedule runs to Gloas. Data in the Altair layout
  does not decode: Helios decodes the header inside a bootstrap or an update as a variable-size
  union, and the Altair header is a fixed-size field. Of the 52 official `sync` cases, 33 run step
  by step through Helios and ferrata's spec store (Capella 10, Deneb 8, Electra 5, Fulu 6, Gloas
  4: 129 steps, 111 updates, 15 forced updates, 3 store upgrades), among them all 7 whose chain
  schedules Gloas, with identical verdicts and identical finalized and optimistic headers (slot,
  beacon root, execution root) after every step. The other 19 hold Altair-layout data; the test
  names each and asserts that they are exactly the cases Helios cannot decode. At 0.9.4, which had
  no Gloas fork and no Gloas layout, 26 cases ran, with identical verdicts and the same headers by
  slot and beacon root (execution roots were not compared then); the 7 Gloas cases could not.
- **Real chains.** Mainnet periods 1899 to 1903 (bootstrap and five updates), the mainnet
  end-to-end finality update, and Sepolia from period 1378 through the Gloas fork to a Gloas
  finality update (32 updates, 31 of them under Gloas): ferrata's policy store, its spec store
  and Helios agree on every verdict and finalized root, and the spec store and Helios on every
  finalized and optimistic header (slot, beacon root, execution root); on Sepolia that execution
  root is the bound block hash.
- **Mutations.** On mainnet Electra data: a flipped signature byte, finality-branch byte,
  next-committee-branch byte, next-committee key or attested state root, a flipped signer bit, no
  signers, a signature slot in the future, and a store holding the wrong committee. On Sepolia
  Gloas data: the same kinds, the attested and the finalized execution block hash, and the
  execution branch. All three refuse each, and nothing changes. Participation from 0 to 32 of 32,
  with real signatures on a chain whose keys the test holds: identical verdicts; both finalize
  from 22 signers, and the policy store refuses below.
- **Where Helios departs from the specification** (`where_helios_departs_from_the_specification`).
  None of these inputs is produced by a genuine chain with an honest committee.

  | Input | The spec, and ferrata | Helios 0.9.4 | Helios 0.12.0 |
  |---|---|---|---|
  | 1. An update whose next committee contradicts the one the store holds | refused (`validate_light_client_update`) | accepted | refused (a16z/helios#819) |
  | 2. A signer key that is the identity point | refused: `FastAggregateVerify` requires `KeyValidate` on every key | accepted | accepted |
  | 3. An empty finalized header with a finality branch that does not prove the zero root (anyone can build it from a genuine signed update: the signature covers only the attested header) | refused | accepted: the finality check is skipped | accepted, the same way |
  | 4. A forced update whose best update carries no finality | the attested header is finalized | panic: `force_update` unwraps the absent finalized header | the attested header is finalized (#820) |
  | 5. A committee whose `aggregate_pubkey` is not the sum of its keys, more than half of them signing | accepted: the spec never reads that field | accepted: it sums the signers' keys | refused: it derives the signers' key from the field (#751) |

  On inputs 2 and 5 ferrata's policy store refuses the committee when it adopts it, before any
  update. The 0.9.4 column for inputs 1 to 4 is what this test asserted while 0.9.4 was pinned
  (commit `a4064bf`); for input 5 it is read from 0.9.4's source, not run. Helios's own client
  never calls `force_update`; its test runner and library users do. At 0.9.4 Helios's own vector
  runner covered six cases (Deneb and Electra) and did not assert that an update verifies; at
  0.12.0 it runs 17 committed cases (Deneb, Electra, Gloas and the transitions into Gloas), and
  every Capella-to-Gloas case of a release on request, and asserts each verdict and header. The
  comparison here asserts, after every step of the 33 cases, that both verdicts and both stores'
  finalized and optimistic headers (slot, beacon root, execution root) are the same.

## Reuse and audit surface

`scripts/count-lines.py` counts the non-test Rust of `ferrata-core` (physical lines as `wc -l`
counts them, then lines that are neither blank nor comments; the four in-crate test modules, 172
lines, are left out):

| File | Category | Lines | Code lines |
|---|---|---:|---:|
| `bls/blst_backend.rs` | BLS glue and new logic | 270 | 209 |
| `bls/mod.rs` | BLS glue and new logic | 116 | 57 |
| `bls/zkcrypto_backend.rs` | BLS glue and new logic | 136 | 112 |
| `fork.rs` | consensus-spec transcription | 173 | 124 |
| `hash.rs` | consensus-spec transcription | 18 | 11 |
| `merkle.rs` | consensus-spec transcription | 131 | 82 |
| `ssz.rs` | consensus-spec transcription | 89 | 70 |
| `store.rs` | consensus-spec transcription | 251 | 197 |
| `types.rs` | consensus-spec transcription | 518 | 421 |
| `upgrade.rs` | consensus-spec transcription | 154 | 136 |
| `validate.rs` | consensus-spec transcription | 376 | 306 |
| `exec.rs` | execution-spec transcription | 349 | 241 |
| `keccak.rs` | execution-spec transcription | 18 | 9 |
| `mpt.rs` | execution-spec transcription | 227 | 174 |
| `rlp.rs` | execution-spec transcription | 199 | 145 |
| `accumulator.rs` | new protocol logic | 188 | 130 |
| `attest.rs` | new protocol logic | 806 | 581 |
| `policy.rs` | new protocol logic | 131 | 91 |
| `steps.rs` | new protocol logic | 777 | 576 |
| `witness.rs` | new protocol logic | 943 | 700 |
| `error.rs` | plumbing | 142 | 63 |
| `lib.rs` | plumbing | 47 | 27 |
| **total** | | **6059** | **4462** |

| Category | Lines | Code lines |
|---|---:|---:|
| BLS glue and new logic | 522 | 378 |
| consensus-spec transcription | 1710 | 1347 |
| execution-spec transcription | 793 | 569 |
| new protocol logic | 2845 | 2078 |
| plumbing | 189 | 90 |

- **Transcription of the consensus specification:** `validate.rs`, `store.rs`, `upgrade.rs`,
  `types.rs`, `ssz.rs`, `merkle.rs`, `fork.rs`, `hash.rs`: each function follows the spec
  function of the same name (`docs/REVIEW-core.md`). **Transcription of execution-layer
  specifications** (Yellow Paper appendices B to D, the EIPs named in `exec.rs`): `rlp.rs`,
  `mpt.rs`, `exec.rs`, `keccak.rs`.
- **New protocol logic, with no specification to transcribe:** `policy.rs` (the finalized-only
  policy and adoption checks), `attest.rs` (records, identifiers, conditions and relations, the
  EIP-2935 hop), `accumulator.rs`, `steps.rs` and `witness.rs` (the LEZ program's job steps,
  compact encodings and witnesses), and in `bls/` the checked path, the on-chain adoption check and
  the pairing split around the libraries' primitives.
- **Third-party cryptography** (normal dependencies of `ferrata-core`):

  | Crate | Version | Used for | Public audit |
  |---|---|---|---|
  | `blst` (supranational) | 0.3.17 | BLS12-381, default backend | NCC Group, January 2021 (cited in the blst README: `research.nccgroup.com/wp-content/uploads/2021/01/NCC_Group_EthereumFoundation_ETHF002_Report_2021-01-20_v1.0.pdf`); formal verification by Galois described there as ongoing. The audited code predates 0.3.17. |
  | `bls12_381` (zkcrypto), with `group` 0.13, `ff` 0.13, `pairing` 0.23 | 0.8.0 | BLS12-381, second backend (differential oracle off chain) | none: its README says "This implementation has not been reviewed or audited" |
  | `sha2` (RustCrypto) | 0.10.9; 0.9.9 for zkcrypto's hash-to-curve | SHA-256 | none found |
  | `tiny-keccak` | 2.0.2 | Keccak-256 | none found |

  RISC Zero's accelerated forks of `blst`, `sha2` and `tiny-keccak` are used only by guests: all
  three in `bench/`, and those of `blst` (0.3.16) and `sha2` in `lez/`. Every line counted above is
  new code written for Ferrata and unaudited; the proposal's audit covers it.

## Adversarial and guard tests

`tests/adversarial.rs` uses real vector data. Every refusal is checked for its error code. Where
a corrupted signature can either fail to decode or decode to another point, the test accepts
either code, and requires both libraries to agree. Every refused update must leave the store
unchanged.

- **Signatures:** a valid signature over another header; corrupted signature bytes; a
  non-compressed encoding; the identity signature; a dropped signer; a claimed non-signer.
- **Branches:** finality and next-committee branches tampered at every depth; a substituted key
  or aggregate.
- **Headers:** tampered finalized and attested headers and payloads.
- **Chain binding:** another `genesis_validators_root`; another fork version.
- **Slots:** an update from the future; a signature slot not after the attested slot.
- **Policy:** non-finality updates and replays refused.
- **Fabricated chain (VS.1, consensus half):** a self-consistent update about a chain that never
  existed. A store that trusts the forger's committee accepts it (the positive control); the real
  store refuses it on the signature alone.
- **Participation floor:** 21 of 32 signers refused; 22, 23 and 32 accepted, with real signatures
  from a committee built for the test.
- **BLS edge cases:** the `{pk, −pk}` identity key; keys outside the subgroup; off-curve keys;
  committees whose keys do not sum to their aggregate; an empty committee.

`tests/mutation_guards.rs` adds:
- committee checks at bootstrap and at adoption;
- conflicting and unexpected next committees;
- stale updates;
- the attested slot equal to the finalized slot;
- finality at genesis;
- the period-skip rule;
- the ranking tiebreakers;
- the optimistic-header threshold;
- header validity across forks;
- the Gloas binding and padding;
- G2 points outside the subgroup;
- strict SSZ decoding;
- the bootstrap's checkpoint and branch;
- the liveness limit above.

`tests/exec_adversarial.rs` covers the execution layer, each case with its exact code:
- **Proofs:** a tampered node at every depth; truncated, extended, reordered and spliced proofs;
  a substituted value, key or root; inclusion and exclusion confused (a truncated inclusion
  proof, an absent key's proof for a present key, a leaf appended to an exclusion proof).
- **Bridge-exploit classes:** a forged trie that holds the attacker's value (refused at the
  root); a forged subtree spliced under genuine nodes (refused where it is attached, at every
  depth); a proof that stops at an ancestor (never yields the ancestor); forged nodes with extra
  items or trailing bytes behind a hash-consistent path.
- **Receipts and records:** a receipt at another index, an index past the last transaction, a
  log index out of range, a failed transaction, a storage proof replayed under another account,
  an account proof under another block's record, another chain.
- **EIP-2935:** the window's edges, a slot mismatch, a substituted header, an unstored block, an
  unknown header shape, a missing contract or one with other code, chained hops.

`tests/mainnet_e2e.rs` repeats the tampering on real data, and `tests/mpt_proofs.rs` checks the
node rules behind the hash check by re-linking forged nodes to a new root.

`tests/nullifier_illustration.rs` illustrates the nullifier trap of the proposal's §5. It runs no
`ferrata-core` code, and its header states its limits.

## How this was built

With AI assistance (Claude Code): every commit was made with it, though not every one carries an
`Assisted-by` trailer ([how it was built](../../docs/how-it-was-built.md)).
Acceptance is mechanical: the vectors, the BLS vectors, the mainnet replay and end-to-end run, the Sepolia
run through Gloas, the TrieTests, the alloy differentials, the Helios oracle, the adversarial
and guard suites, the job steps' equivalence with the policy store, clippy with warnings as
errors, and the `no_std` build.
`docs/REVIEW-core.md` maps every function of the light client and the execution layer to its
spec section, with a column for the author's line-by-line sign-off (not filled in yet); it does
not cover `steps.rs` and `witness.rs` yet.

## License

MIT OR Apache-2.0, at your option.
