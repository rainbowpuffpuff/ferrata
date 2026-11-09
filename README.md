# Ferrata core

An Ethereum sync-committee light client in Rust, `no_std`, written as the verification core of
**Ferrata**, a proposal for [Logos RFP-021](https://github.com/logos-co/rfp) (trustless
Ethereum state attestation on LEZ). The name comes from the *via ferrata*, a mountain route with
fixed anchors that anyone can cross without a guide. This repository is the proposal's evidence.
There is no LEZ program code here yet; that is the proposal's first milestone.

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
| Adversarial tests | 16 (listed below) |
| Guard tests | 23: each pins a check that an independent review showed could be deleted without any test failing, plus the two committee checks |
| **Execution layer** | |
| Real mainnet, end to end (`fixtures/mainnet-e2e-2026-11-09`) | A bootstrap (slot 15,613,952) and a finality update (512/512 signers) of period 1906, verified with both libraries and both committee checks, finalize execution block 26,379,842; its `HeaderRecord` equals two providers' block. WETH, USDC and an address without an account: every fact equals `eth_getProof` and `balanceOf`. The receipts trie, rebuilt with alloy-trie, equals the record's root, and all 870 logs of its 245 receipts (types 0, 2, 3, 4; 54 USDC transfers) are attested. EIP-2935 hops reach the parent block, whose 3 failed transactions are refused, and n − 16, then n − 16 − 8191 (the window's edge), with a storage fact under each of these two. Tampering with each kind of fixture byte is refused, each with its expected code (a corrupted signature with either BLS code, as in the adversarial tests). |
| `ethereum/tests` TrieTests (pinned at `c67e485`) | **25 / 25 cases**, every root, and 368 inclusion and exclusion proofs (`trietestnextprev.json` tests iteration, not proofs) |
| Merkle-Patricia proofs against alloy-trie 0.9.9 | 240 random tries in four key families (179 with embedded nodes): equal roots, byte-identical proofs, 19,173 inclusion and 8,968 exclusion proofs accepted by both; 4,453 mutated proofs refused by both. Two documented differences (below). |
| Strict RLP against alloy-rlp 0.3.17 | 21 non-canonical or malformed encodings, each refused with its code (and by alloy-rlp); 3,000 random round trips; 20,000 mutated encodings with the same verdict from both (6,237 accepted, 13,763 refused) |
| Execution objects | Headers of the Cancun (20 fields), Prague/Osaka (21) and Amsterdam (23) layouts and every field rule; accounts; storage values; receipts of types 0 to 4, pre-Byzantium refused; record encoding; statement identifiers equal to an independent Python computation |
| Execution adversarial tests | 7 tests (listed below), each refusal checked for its exact code |
| Mutation run | 93 hand-picked mutants, all killed (`scripts/mutants.py`, `docs/mutants-run.log`). 43 cover the light client; 32 of those come from an independent review, under which 20 had survived the earlier suite. 50 cover the execution layer: length checks, hash comparisons, indices and bounds, canonical-form checks, the receipt status check, the EIP-2935 window, identifiers and predicates. |
| `no_std` | Builds for `riscv32im-unknown-none-elf` with the pure-Rust backend. The `blst` backend needs a RISC-V C toolchain (RISC Zero's), which this repository does not set up yet. |

| RISC Zero guest (`bench/`) | RISC Zero zkVM 3.0.5 executor, real mainnet update, RISC Zero's accelerated blst: a finality update costs 10.2M cycles, and splits into steps of 2.3M, 4.2M and 3.6M through the core's `miller_half` / `final_half`. A handoff adds 1.1M for the committee root, plus either 4.6M for the adoption check with uncompressed keys or 48.7M with compressed keys (`check_committee_onchain` today). See `bench/RESULTS.md`, including six library configurations. |
| RISC Zero guest, execution layer (`bench/exec/`) | The same executor, mainnet block 26,379,842, with RISC Zero's accelerated Keccak (without it in brackets): `attest_storage` 0.21M user cycles (1.04M), `attest_log` 0.04M for a one-log receipt and 0.43M for a 96-log one (0.21M, 2.58M), one EIP-2935 hop 0.18M (0.95M). With the accelerator, the Keccak-f permutations (54 for a storage slot, 50 for a hop) are proven by RISC Zero's separate Keccak circuit, outside these counts. See `bench/RESULTS.md`. |

**Not done yet** (the proposal's M0 to M2):
- the LEZ v0.4 program and its costs inside the v0.4 shell;
- the job-step API (validation up to the signature, applying an update after a split pairing) and
  a store restored from the program's own state without re-checking;
- the uncompressed-key adoption witness;
- the `single_merkle_proof` vectors;
- a Gloas record on real data: `HeaderRecord::from_block_hash` is tested on synthetic stores only,
  since mainnet has no Gloas date and the fixture is Fulu;
- fuzzing (`cargo-fuzz` on RLP, Merkle-Patricia and query decoding) and `cargo-mutants` over the
  whole core.

## Run it

```sh
scripts/fetch-vectors.sh          # pinned releases and TrieTests; hashes verified on every run
cargo test --workspace -- --nocapture
scripts/mutants.py                # optional: the 93-mutant run (45 minutes here)
```

The mainnet fixtures are committed. `scripts/fetch-mainnet-e2e.py OUTDIR` shows how the
end-to-end one was made; run today, it fetches a newer block into a new directory.

The vector tests fail rather than skip when the vectors are missing, and they pin the number of
cases, so an empty or moved directory cannot pass.

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
| `crates/ferrata-core/src/attest.rs` | `HeaderRecord`, the attestations consumers call, identifiers, conditions, the EIP-2935 hop |
| `crates/ferrata-spectests/src/execkit.rs` | A reference trie (independent Keccak), synthetic blocks, JSON-RPC readers |
| `scripts/fetch-mainnet-e2e.py` | The fetch tool of the mainnet end-to-end fixture (standard library only) |
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
     agree exactly. `check_committee_onchain` is what the program will run: every key must be on
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
   block hash, so the tampering tests on Gloas headers matter more than the vectors here.

7. **Execution roots come only from records.** A `HeaderRecord` holds one block's number, hash,
   state and receipts roots, the chain id and the binding it came from. It is built from a
   `FinalizedStore`'s finalized header (the payload header up to Fulu; under Gloas, the RLP
   header that hashes to the bound block hash), derived through EIP-2935, or restored from the
   program's own record bytes. Every attestation takes a record and the chain id the consumer
   expects; nothing takes a root from the caller.

8. **Identifiers name facts.** A storage fact's identifier is SHA-256 over a tag, the chain id,
   the address, the slot and the value; an account's over the chain id, the address and its four
   fields. The block is left out, so the same value read at two blocks has one identifier (the
   mainnet test shows this for a real balance). That suits "this value holds", not "this event
   happened once": a balance that changes and changes back gets the same identifier, so
   once-per-event consumers key on logs, whose identifier includes the block number and the
   transaction's and log's positions. `attest.rs` documents the exact derivations, and a test pins
   them against an independent implementation.

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

With AI assistance (Claude Code); every commit carries an `Assisted-by` trailer. Acceptance is
mechanical: the vectors, the BLS vectors, the mainnet replay and end-to-end run, the
TrieTests, the alloy differentials, the adversarial and guard suites, clippy with warnings as
errors, and the `no_std` build. `docs/REVIEW-core.md` maps every function
to its spec section and records the author's line-by-line sign-off.

## License

MIT OR Apache-2.0, at your option.
