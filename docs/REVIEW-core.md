# Per-function review of `ferrata-core` against the spec

Each row is one function: the spec section it transcribes (`consensus-specs` v1.7.0-beta.4),
any deliberate deviation, the tests that pin it, and the author's sign-off. The deviations column
is part of the audit packet.

| Function (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `validate_light_client_update` (validate.rs) | altair/light-client/sync-protocol.md `validate_light_client_update` | shape checks added first (layout, branch lengths, committee size); the signature path is chosen by the caller | sync vectors; adversarial; guards M03–M17, M37 | |
| `is_valid_light_client_header` (validate.rs) | capella, deneb, gloas `is_valid_light_client_header` | one function per store layout, dispatching on the header's layout | sync vectors; guards M10–M12 | |
| `get_lc_execution_root` (validate.rs) | capella, deneb, electra, gloas `get_lc_execution_root` | as above | sync vectors (`execution_root` checks); guard M20 | |
| `is_better_update` (validate.rs) | altair `is_better_update` | none | update_ranking vectors; guard M19 | |
| `finalized_root_gindex_at_slot` and the committee gindex functions (validate.rs) | electra, gloas helpers | none | sync vectors (fork transitions); M25, M31 | |
| `sync_committee_signing_root` (validate.rs) | `compute_fork_version(epoch(signature_slot − 1))`, `compute_domain`, `compute_signing_root` | none | sync vectors; chain-binding tests; M09 | |
| `StoreCore::from_bootstrap` (store.rs) | `initialize_light_client_store` | none | bootstrap guard; M38 | |
| `apply_core` (store.rs) | `apply_light_client_update` | the optimistic and participation bookkeeping moved to the spec store; reports `adopted` | sync vectors; M29 | |
| `LightClientStore::process_update` (store.rs) | `process_light_client_update` | atomic on error (the spec mutates before a late assert) | sync vectors; M23 | |
| `LightClientStore::process_force_update` (store.rs) | `process_light_client_store_force_update` | atomic on error | sync vectors; M27 | |
| `FinalizedStore::process_update` (policy.rs) | (Ferrata policy) | finality only, ≥ 2/3, no optimistic header, no forcing; `check_committee` on adoption | policy-vs-spec comparison; adversarial; M01, M02, M30, M39, M40 | |
| `upgrade_header`, `upgrade_bootstrap`, `upgrade_update` (upgrade.rs) | capella, deneb, electra, gloas `fork.md` | composed step by step | sync fork-transition vectors; M26 | |
| `is_valid_normalized_merkle_branch`, `normalize_merkle_branch` (merkle.rs) | altair helpers, electra `normalize_merkle_branch` | none | unit tests; M36 | |
| `split_container`, container decoders (ssz.rs, types.rs) | SSZ specification | strict: exact consumption, first offset equals fixed size, monotone offsets | SSZ guard tests; M05, M06 | |
| `Blst::fast_aggregate_verify`, `Zkcrypto::fast_aggregate_verify` (bls/) | `FastAggregateVerify` (IETF BLS, proof-of-possession suite) | pairing computed as two Miller loops and one final exponentiation | BLS vectors; M33 | |
| `check_committee` (bls/) | `KeyValidate` per key, `eth_aggregate_pubkeys` | refuses an empty committee | `eth_aggregate_pubkeys` vectors; M35 | |
| `check_committee_onchain` (bls/) | (the program's adoption check) | no per-key subgroup check; relies on deposit-time `KeyValidate` | on-chain adoption guards; M41 | |
| `Blst::miller_half`, `Blst::final_half` (bls/) | split of the pairing check | canonical big-endian Fp12 encoding | split round-trip test; M43 | |
| `verify_checked` (bls/) | (equivalent to `FastAggregateVerify` for checked committees) | subtractive composition; identity check only | BLS vectors (both paths agree); M34 | |
