# Plumbline core

An Ethereum sync-committee light client in Rust, `no_std`, written as the verification core of
**Plumbline**, a proposal for [Logos RFP-021](https://github.com/logos-co/rfp) (trustless
Ethereum state attestation on LEZ). This repository is the proposal's evidence. The core passes
the official `consensus-spec-tests` light-client vectors, Altair through Gloas, with two
independent BLS libraries.

## Results

All vectors come from `consensus-specs` **v1.7.0-beta.4**, minimal preset. Every row is produced
by `cargo test --workspace`.

| Suite | Result |
|---|---|
| `light_client/sync`: 7 forks (Altair to Gloas), including fork transitions and stores fed older forks' data | **52 / 52 cases**, 185 steps (160 `process_update`, 21 `force_update`, 4 `upgrade_store`). Finalized and optimistic headers, both beacon root and execution root, are checked after every step. |
| The same, four times | `blst` additive, `blst` subtractive, `blst` auto, zkcrypto `bls12_381` auto: identical results |
| `light_client/update_ranking`: 7 forks | **7 / 7 cases**, 399 updates in canonical `is_better_update` order |
| Plumbline's policy store against the spec store | Equal state on 150 steps. The two diverge only after a forced update (7 cases), which the policy never accepts. |
| Adversarial suite | 17 tests: tampering, a fabricated chain, the participation boundary, BLS edge cases (listed below) |
| Nullifier linkability | Naive nullifiers link 100 of 100 private mints to their deposits. Secret-bound nullifiers link none and stay unique. |
| `no_std` | The core with the pure-Rust backend builds for `riscv32im-unknown-none-elf`, the RISC Zero guest ISA |

**Not done yet** (these are milestones M0 and M1 of the proposal):
- the `single_merkle_proof` vectors, which need full `BeaconState` and `BeaconBlockBody` SSZ;
- replays of recorded mainnet and Sepolia update streams;
- the LEZ v0.4 program shell and cycle counts per update step.

## Run it

```sh
scripts/fetch-vectors.sh          # pinned release, sha256-checked, light-client subset only
cargo test --workspace -- --nocapture
```

The vector tests fail rather than skip when the vectors are missing. They also pin the expected
number of cases, so an empty or moved directory cannot pass.

## Layout

| Path | What |
|---|---|
| `crates/plumbline-core/src/validate.rs` | `validate_light_client_update`, `is_valid_light_client_header`, `get_lc_execution_root`, `is_better_update`, the gindex functions. Each is a transcription of the spec function of the same name. |
| `crates/plumbline-core/src/store.rs` | The spec `LightClientStore` (optimistic header, best valid update, forced updates). It exists to replay the vectors step for step. |
| `crates/plumbline-core/src/policy.rs` | `FinalizedStore`, Plumbline's on-chain policy (below) |
| `crates/plumbline-core/src/upgrade.rs` | Upgrading light-client data across forks, including Gloas's `execution_block_hash` binding |
| `crates/plumbline-core/src/types.rs`, `ssz.rs`, `merkle.rs` | The containers, strict SSZ decoding, Merkle helpers and generalized indices |
| `crates/plumbline-core/src/bls/` | The `BlsBackend` trait, with `blst` and zkcrypto `bls12_381` backends |
| `crates/plumbline-spectests/` | The vector runner, the adversarial suite and the nullifier demonstration |

## Design notes

1. **Two stores, one validation.** The spec store and Plumbline's `FinalizedStore` call the same
   `validate_light_client_update` and the same `apply_core`. The policy only refuses more. A header
   enters only through a finality proof signed by at least two thirds of the committee. There is
   no optimistic header and no forced update. The vector runner feeds every update to both stores
   and requires equal state until a forced update first changes the spec store.

2. **Signing key by subtraction, made exact.** On mainnet more than 95% of the committee signs, so
   `aggregate_pubkey − Σ non-signers` needs a handful of point additions instead of about 500.
   That matters in a zkVM. On its own, though, the subtractive path never decodes the signers'
   keys. It agrees with the spec's `FastAggregateVerify` only if every key is a curve point and the
   keys sum to `aggregate_pubkey`. So `FinalizedStore` runs `check_committee` once on every
   committee it adopts (bootstrap and handoff). From then on, subtraction is exactly the spec's
   result for every committee an instance can hold. Both compositions run over every vector.

3. **`KeyValidate` on the aggregate.** Keys are decoded with an on-curve check only. The
   aggregate is then checked once: it must not be the identity and must lie in the prime-order
   subgroup. That matches the pyspec's `py_ecc`, which applies `KeyValidate` to the aggregate
   inside `CoreVerify`. The suite includes the `{pk, −pk}` identity attack and a curve point
   outside the subgroup.

4. **Differential finding: the two libraries refuse the same input with different codes.** `blst`
   rejects the x = 0 points (order 3) while decoding (`PubkeyInvalid`). zkcrypto decodes them and
   rejects the aggregate at the subgroup check (`SigningKeyNotInGroup`). Both refuse. Because of
   this, the program pins one backend: error codes are part of its interface, and the dry run
   must return the same codes as the program.

5. **Gloas.** Under EIP-7732 a block commits to a payload bid, and the payload itself can be
   withheld. The light-client header therefore binds the bid's `parent_block_hash` (gindex
   2,856). Older forks' headers are carried into a Gloas store as a block hash plus a normalized
   branch. All Gloas sync cases pass, including the Deneb→Gloas, Electra→Gloas and Fulu→Gloas
   transitions.

## Adversarial suite

`crates/plumbline-spectests/tests/adversarial.rs` uses real vector data. Every refusal is checked
for its exact typed code, and every refused update must leave the store unchanged.

- **The genuine update** is accepted by all six backend configurations.
- **Signatures:** a valid signature over a different header; corrupted signature bytes; a
  non-compressed signature encoding; a dropped signer; a claimed non-signer.
- **Branches:** the finality branch tampered at every depth; the next-committee branch tampered at
  every depth; a substituted committee key or aggregate key.
- **Headers:** a tampered finalized beacon header; a tampered finalized or attested execution
  payload; a different attested header.
- **Chain binding (F.5):** a different `genesis_validators_root`; a wrong fork version.
- **Slot ordering:** an update from the future; a signature slot not after the attested slot.
- **Policy:** non-finality updates and replays are refused (`NotFinalized`, `NotNewer`).
- **Fabricated chain (VS.1):** a self-consistent update about a chain that never existed, in which
  both headers' execution branches and the finality branch verify. It is accepted only by a store
  that trusts the forger's committee (the positive control). The real store refuses it, and only
  on the signature.
- **Participation floor:** exactly two thirds. With 32 seats, 21 signers are refused, while 22,
  23 and 32 are accepted, with real signatures from a committee built for the test.
- **BLS edge cases:** the identity signing key; a signing key outside the subgroup (both
  compositions); off-curve keys; a committee whose keys do not sum to its aggregate.

## How this was built

With AI assistance (Claude Code). Acceptance is mechanical, and nothing merges unless all of these
pass:
- the official vectors, with both BLS libraries and both compositions;
- the adversarial suite, with exact error codes;
- clippy with warnings as errors;
- the `no_std` RISC-V build.

Every commit carries an `Assisted-by` trailer.

## License

MIT OR Apache-2.0, at your option.
