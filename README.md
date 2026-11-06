# Plumbline core

An Ethereum sync-committee light client in Rust, `no_std`, written as the verification core of
**Plumbline**, a proposal for [Logos RFP-021](https://github.com/logos-co/rfp) (trustless
Ethereum state attestation on LEZ). This repository is the proposal's evidence. There is no LEZ
program code here yet; that is the proposal's first milestone.

## Results

Every row is produced by `cargo test --workspace`.

| Suite | Result |
|---|---|
| `light_client/sync`, consensus-specs v1.7.0-beta.4, minimal preset (32-member committees), Altair to Gloas, including fork transitions and stores fed older forks' data | **52 / 52 cases**, 185 steps (160 `process_update`, 21 `force_update`, 4 `upgrade_store`). Finalized and optimistic headers, beacon and execution roots, are checked after every step. |
| The same, four times | `blst` with additive, subtractive and automatic key aggregation, and zkcrypto `bls12_381`: identical |
| `light_client/update_ranking` | **7 / 7 cases**, 399 updates in canonical order |
| Real mainnet data (`fixtures/mainnet-p1899`) | Periods 1899 to 1903 (Fulu): 512-member committees, four handoffs, 510 to 512 signers per update. Every finalized root equals the canonical root a second provider reports. About 12 ms per update natively (blst, subtractive aggregation, committee check included at handoffs). |
| Official BLS vectors | consensus-specs `eth_fast_aggregate_verify` (12) and `eth_aggregate_pubkeys` (8); ethereum/bls12-381-tests `fast_aggregate_verify` (12) and `verify` (29). Both libraries, all three compositions; both verification paths agree. |
| Policy store against the spec store | Equal state on 150 steps. After a forced update the two diverge, and the policy store never holds a header the spec store did not finalize (checked at every step). |
| Adversarial tests | 16 (listed below) |
| Guard tests | 21: each pins a check that an independent review showed could be deleted without any test failing |
| Mutation run | 40 hand-picked mutants, all killed (`scripts/mutants.py`). 32 come from that review, under which 20 had survived the earlier suite. |
| `no_std` | Builds for `riscv32im-unknown-none-elf` with the pure-Rust backend. The `blst` backend needs a RISC-V C toolchain (RISC Zero's), which this repository does not set up yet. |

**Not done yet** (the proposal's M0 to M2): the LEZ v0.4 program and its cycle counts per step,
the `single_merkle_proof` vectors, and the Merkle-Patricia, RLP and predicate code.

## Run it

```sh
scripts/fetch-vectors.sh          # pinned releases; tree hashes verified on every run
cargo test --workspace -- --nocapture
scripts/mutants.py                # optional: the 40-mutant run (several minutes)
```

The vector tests fail rather than skip when the vectors are missing, and they pin the number of
cases, so an empty or moved directory cannot pass.

## Layout

| Path | What |
|---|---|
| `crates/plumbline-core/src/validate.rs` | `validate_light_client_update`, `is_valid_light_client_header`, `get_lc_execution_root`, `is_better_update` and the gindex functions, each a transcription of the spec function of the same name |
| `crates/plumbline-core/src/store.rs` | The spec's `LightClientStore` (optimistic header, best valid update, forced updates), used to replay the vectors |
| `crates/plumbline-core/src/policy.rs` | `FinalizedStore`, Plumbline's on-chain policy |
| `crates/plumbline-core/src/upgrade.rs` | Light-client data across forks, including Gloas's `execution_block_hash` |
| `crates/plumbline-core/src/types.rs`, `ssz.rs`, `merkle.rs` | Containers, strict SSZ decoding, Merkle helpers and generalized indices |
| `crates/plumbline-core/src/bls/` | The `BlsBackend` trait, with `blst` and zkcrypto backends |
| `crates/plumbline-spectests/` | The vector runner, the fixtures and the test suites |
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

3. **Two BLS paths.**
   - `fast_aggregate_verify` is the spec's `FastAggregateVerify` as the pyspec runs it: every
     signer key must pass `KeyValidate`. The spec store uses it.
   - `verify_checked` serves committees that passed `check_committee` when the policy store
     adopted them: every key `KeyValidate`d, and the keys summing to `aggregate_pubkey`. The
     signing key can then be `aggregate_pubkey − Σ non-signers`, a handful of additions at
     mainnet participation, with no per-update subgroup check, because a sum of G1 points stays
     in G1.
   - For checked committees the two paths agree exactly. The tests run every vector through both.

4. **The pairing is explicit.** It is computed as two Miller loops and one final
   exponentiation. That is the seam a multi-transaction program can split at: blst exposes the
   Miller-loop output as a plain `blst_fp12` value.

5. **Both libraries report the same codes.** blst refuses the order-3 points (x = 0) while
   decoding, and zkcrypto refuses them at its subgroup check. Both checks are part of
   `KeyValidate`, so both report `PubkeyInvalid`. An earlier version validated only the aggregate,
   and there the two libraries returned different codes for the same input.

6. **Gloas.** Under EIP-7732 a block commits to a payload bid, and the payload can be withheld.
   The light-client header therefore binds the bid's `parent_block_hash` (gindex 2,856). Older
   headers carried into a Gloas store become a block hash plus a zero-padded branch, and the guard
   tests check that the padding must stay zero. The native Gloas vectors bind a single constant
   block hash, so the tampering tests on Gloas headers matter more than the vectors here.

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

`tests/nullifier_illustration.rs` illustrates the nullifier trap of the proposal's §5. It runs no
`plumbline-core` code, and its header states its limits.

## How this was built

With AI assistance (Claude Code); every commit carries an `Assisted-by` trailer. Acceptance is
mechanical: the vectors, the BLS vectors, the mainnet replay, the adversarial and guard suites,
clippy with warnings as errors, and the `no_std` build. The author's line-by-line review against
the spec is recorded per function in `docs/REVIEW-core.md`.

## License

MIT OR Apache-2.0, at your option.
