# Per-function review of `ferrata-core` against the spec

Each row is one function: the spec section it transcribes (`consensus-specs` v1.7.0-beta.4 for
the light client; the Yellow Paper, `execution-specs` and the EIPs named for the execution layer),
any deliberate deviation, the tests that pin it, and the author's sign-off. The deviations column
is part of the audit packet.

Signed: Carol Calin, 12 November 2026, every row below (48), after reading each against its spec section
and the agent review of 11 November.

The agent review that preceded the sign-off: [reviews/agent-review-core-2026-11-11.md](reviews/agent-review-core-2026-11-11.md).

| Function (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `validate_light_client_update` (validate.rs) | altair/light-client/sync-protocol.md `validate_light_client_update` | shape checks added first (layout, branch lengths, committee size); the signature path is chosen by the caller | sync vectors; adversarial; guards M03–M17, M37 | Carol Calin, 12 Nov 2026 |
| `is_valid_light_client_header` (validate.rs) | capella, deneb, gloas `is_valid_light_client_header` | one function per store layout, dispatching on the header's layout | sync vectors; guards M10–M12 | Carol Calin, 12 Nov 2026 |
| `get_lc_execution_root` (validate.rs) | capella, deneb, electra, gloas `get_lc_execution_root` | as above | sync vectors (`execution_root` checks); guard M20 | Carol Calin, 12 Nov 2026 |
| `is_better_update` (validate.rs) | altair `is_better_update` | none | update_ranking vectors; guard M19 | Carol Calin, 12 Nov 2026 |
| `finalized_root_gindex_at_slot` and the committee gindex functions (validate.rs) | electra, gloas helpers | none | sync vectors (fork transitions); M25, M31 | Carol Calin, 12 Nov 2026 |
| `sync_committee_signing_root` (validate.rs) | `compute_fork_version(epoch(signature_slot − 1))`, `compute_domain`, `compute_signing_root` | none | sync vectors; chain-binding tests; M09 | Carol Calin, 12 Nov 2026 |
| `StoreCore::from_bootstrap` (store.rs) | `initialize_light_client_store` | none | bootstrap guard; M38 | Carol Calin, 12 Nov 2026 |
| `apply_core` (store.rs) | `apply_light_client_update` | the optimistic and participation bookkeeping moved to the spec store; reports `adopted` | sync vectors; M29 | Carol Calin, 12 Nov 2026 |
| `LightClientStore::process_update` (store.rs) | `process_light_client_update` | atomic on error (the spec mutates before a late assert) | sync vectors; M23 | Carol Calin, 12 Nov 2026 |
| `LightClientStore::process_force_update` (store.rs) | `process_light_client_store_force_update` | atomic on error | sync vectors; M27 | Carol Calin, 12 Nov 2026 |
| `FinalizedStore::process_update` (policy.rs) | (Ferrata policy) | finality only, ≥ 2/3, no optimistic header, no forcing; an update that advances nothing is `NotNewer`; `check_committee` on adoption (and on both committees in `from_core`) | policy-vs-spec comparison; adversarial; M01, M02, M30, M39, M40 | Carol Calin, 12 Nov 2026 |
| `upgrade_header`, `upgrade_bootstrap`, `upgrade_update` (upgrade.rs) | capella, deneb, electra, gloas `fork.md` | composed step by step | sync fork-transition vectors; Sepolia through its Gloas fork (`sepolia_gloas`); M26 | Carol Calin, 12 Nov 2026 |
| `is_valid_normalized_merkle_branch`, `normalize_merkle_branch` (merkle.rs) | altair helpers, electra `normalize_merkle_branch` | none | unit tests; M36 | Carol Calin, 12 Nov 2026 |
| `split_container`, container decoders (ssz.rs, types.rs) | SSZ specification | strict: exact consumption, first offset equals fixed size, monotone offsets | SSZ guard tests; M05, M06 | Carol Calin, 12 Nov 2026 |
| `Blst::fast_aggregate_verify`, `Zkcrypto::fast_aggregate_verify` (bls/) | `FastAggregateVerify` (IETF BLS, proof-of-possession suite) | pairing computed as two Miller loops and one final exponentiation | BLS vectors; M33 | Carol Calin, 12 Nov 2026 |
| `check_committee` (bls/) | `KeyValidate` per key, `eth_aggregate_pubkeys` | refuses an empty committee | `eth_aggregate_pubkeys` vectors; M35 | Carol Calin, 12 Nov 2026 |
| `check_committee_onchain` (bls/) | (the program's adoption check) | no per-key subgroup check; every key of a genuine committee passed `KeyValidate` when `get_next_sync_committee` aggregated it (`eth_aggregate_pubkeys`), and at deposit | on-chain adoption guards; M41 | Carol Calin, 12 Nov 2026 |
| `Blst::miller_half`, `Blst::final_half` (bls/) | split of the pairing check | `miller_half` writes the canonical big-endian Fp12 encoding; `final_half` reads each coefficient modulo p, so the caller refuses non-canonical bytes (`steps::Job::step3`, `canonical_fp12`) | split round-trip test; M43 | Carol Calin, 12 Nov 2026 |
| `verify_checked` (bls/) | (equivalent to `FastAggregateVerify` for checked committees) | subtractive composition; identity check only | BLS vectors (both paths agree); M34 | Carol Calin, 12 Nov 2026 |

**Not covered by the rows above.** The on-chain program does not call `validate_light_client_update`,
`apply_core` or `FinalizedStore::process_update` directly: it runs their transcription over the
compact store, `steps.rs` (`validate_compact`, `update_advances`, `apply_core_compact`,
`store_for_update`, `step1`, `Job::step2`, `Job::step3`, `apply`, the non-signer witness) and
`witness.rs` (compact encodings, committee adoption from uncompressed keys). Those functions need
rows of their own, each naming the row it mirrors and the equivalence tests that pin it
(`steps.rs` tests; `review_core::the_compact_validation_agrees_with_row_1_on_the_guard_cases`).

## Execution layer

Test files are under `crates/ferrata-spectests/tests/`; mutant ids refer to `scripts/mutants.py`.

| Function (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `keccak256` (keccak.rs) | Yellow Paper `KEC` (Keccak-256, original padding) | none; the one place a guest swaps in an accelerated implementation | `exec_objects::keccak_matches_two_other_implementations` (sha3, alloy) | Carol Calin, 12 Nov 2026 |
| `split_first`, `decode` (rlp.rs) | Yellow Paper, Appendix B | canonical encodings only: minimal lengths, no wrapped single byte, no leading zero in a length, items bounded by their list, no trailing bytes; lazy (never recurses) | `rlp_decoding` (corpus, alloy-rlp round trips and mutation differential, deep nesting); M44–M48 | Carol Calin, 12 Nov 2026 |
| `Item::{bytes, fixed, u64, word, list, items}` (rlp.rs) | Yellow Paper, Appendix B (scalars are big-endian without leading zeros) | integers bounded by their field (64 or 256 bits) | `rlp_decoding::integers_are_canonical`, `typed_accessors`; M49–M51 | Carol Calin, 12 Nov 2026 |
| `encode_u64` (rlp.rs) | Yellow Paper, Appendix B | none | `rlp_decoding::encode_u64_matches_alloy`; M52 | Carol Calin, 12 Nov 2026 |
| `verify` (mpt.rs) | Yellow Paper, Appendices C (hex-prefix) and D (trie); proof format of EIP-1186 `eth_getProof` | embedded nodes are never proof elements; a node under 32 bytes referenced by hash is refused (except the root); the empty trie is proven only by the empty proof (alloy-trie also takes `[0x80]`); every element must be used | `mpt_proofs` (TrieTests, alloy-trie differential, node rules), `exec_adversarial`; M53–M66 | Carol Calin, 12 Nov 2026 |
| `decode_header`, `decode_header_with_hash` (exec.rs) | `execution-specs` `forks/{cancun,prague,osaka,amsterdam}/blocks.py` `Header`; EIP-4844, EIP-4788, EIP-7685, EIP-7928, EIP-7843 | only 20, 21 or 23 fields; number, gas and timestamp limited to 64 bits (the consensus layer's `uint64`); `extra_data` at most 32 bytes | `exec_objects::header_shapes`, `header_fields_are_strict`; mainnet e2e; a real Amsterdam header (Sepolia, `sepolia_gloas`); M67–M69 | Carol Calin, 12 Nov 2026 |
| `Account::decode` (exec.rs) | Yellow Paper 4.1 (account state); EIP-2681 (nonce below 2^64) | refuses a zero storage root or code hash, so `Account::ABSENT` is unambiguous | `exec_objects::accounts_and_storage_values`; M70 | Carol Calin, 12 Nov 2026 |
| `decode_storage_value` (exec.rs) | Yellow Paper 4.1 (storage values as RLP integers) | refuses a stored zero (writing zero deletes a slot) | `exec_objects::accounts_and_storage_values`; M71 | Carol Calin, 12 Nov 2026 |
| `prove_account`, `prove_storage` (exec.rs) | EIP-1186 secure-trie keys `KEC(address)`, `KEC(slot)` | an absent account is all zero (as `eth_getProof` reports it) and has empty storage | mainnet e2e (absent account); M72 | Carol Calin, 12 Nov 2026 |
| `Receipt::decode`, `Receipt::log`, `prove_receipt` (exec.rs) | Yellow Paper 4.3.1; EIP-658 (status); EIP-2718 typed receipts of types 1 (EIP-2930), 2 (EIP-1559), 3 (EIP-4844), 4 (EIP-7702); receipts-trie key `RLP(index)` | pre-Byzantium receipts and unknown types refused; at most four topics per log | `exec_objects::receipts_of_every_type_and_their_refusals`; mainnet e2e (245 receipts, 870 logs); M73–M76 | Carol Calin, 12 Nov 2026 |
| `HeaderRecord::from_payload_header` (attest.rs) | capella `LightClientHeader.execution` (Capella to Fulu layouts) | refuses an empty payload header | `exec_objects::records_come_from_the_finalized_header`; mainnet e2e; M87 | Carol Calin, 12 Nov 2026 |
| `HeaderRecord::from_block_hash` (attest.rs) | gloas `LightClientHeader.execution_block_hash` (EIP-7732) | the RLP header must hash to the bound hash; the roots come from it | `exec_objects::records_come_from_the_finalized_header`; real Sepolia data under Gloas (`sepolia_gloas`); M69 | Carol Calin, 12 Nov 2026 |
| `HeaderRecord::{to_ssz, from_ssz}` (attest.rs) | (Ferrata record layout, a fixed-size SSZ container) | `from_ssz` is the trusted restore path | `exec_objects::record_encoding_round_trips_and_is_strict`; M85, M86 | Carol Calin, 12 Nov 2026 |
| `HeaderRecord::anchor_slot` (attest.rs) | (Ferrata) | for a record reached through EIP-2935 hops, the slot of the light-client record the hops started from, not of the record's own block | `exec_adversarial::history_hops_and_their_refusals`; mainnet e2e; M107 | Carol Calin, 12 Nov 2026 |
| `attest_account`, `attest_storage`, `attest_log` (attest.rs) | (Ferrata; RFP F.3, F.5) | chain id checked first; a log needs receipt status 1 | `exec_objects`, `exec_adversarial`, mainnet e2e; M77, M78 | Carol Calin, 12 Nov 2026 |
| `account_id`, `storage_id`, `log_hash`, `log_id` (attest.rs) | (Ferrata; RFP F.6) | state identifiers leave out the block, by design | `exec_objects::identifiers_are_pinned_and_depend_only_on_the_fact` (independent Python values); M92, M93 | Carol Calin, 12 Nov 2026 |
| `account_id_at_block`, `storage_id_at_block`, `Attested::id_at_block` (attest.rs) | (Ferrata; the block-bound variant) | kinds 0x11 and 0x12, so no preimage is shared with the default identifiers | `exec_objects::block_bound_identifiers` (independent Python values); mainnet e2e; M105, M106 | Carol Calin, 12 Nov 2026 |
| `Predicate::holds`, `Fact::field`, `Fact::check`, `attest_batch` (attest.rs) | (Ferrata; RFP F.4, soft #2 and #4) | an empty batch is refused | `exec_objects::predicates_compare_unsigned_256_bit_words`, `fact_fields_and_conditions`, `a_batch_is_one_compound_statement`; M88–M91 | Carol Calin, 12 Nov 2026 |
| `Relation`, `Operand` in `attest_batch` (attest.rs) | (Ferrata; RFP soft #4: two attested values under one record) | a relation naming a fact that is not there, or a field of another kind of fact, is `QueryInvalid`; a topic or data word the log lacks is `PredicateFailed` | `exec_objects::relations_compare_two_attested_values`; mainnet e2e; M104 | Carol Calin, 12 Nov 2026 |
| `history_hop` (attest.rs) | EIP-2935 (`HISTORY_STORAGE_ADDRESS`, `HISTORY_SERVE_WINDOW` = 8191, slot `n mod 8191`) | `n` from the header only; the contract's code hash must be the specified one | `exec_adversarial::history_hops_and_their_refusals`; mainnet e2e (three real hops); M79–M84 | Carol Calin, 12 Nov 2026 |
| `locate_log`, `block_log_index` (exec.rs) | JSON-RPC `logIndex` (block-wide) against the receipt position `attest_log` takes | query helpers for clients, not checks | `mainnet_e2e::block_wide_log_indices_convert_to_receipt_positions` (all 870 logs); M102, M103 | Carol Calin, 12 Nov 2026 |
| `accumulator_node`, `accumulator_root_of` (accumulator.rs) | the Ferrata bridge specification v2, §3.2 (deposit tree): depth 32, `SHA-256(0x01 ‖ l ‖ r)`, empty leaf zero | an index of 2^32 or more is refused (it would share a path with its low 32 bits) | `accumulator` (vectors from an independent Python implementation, every sibling and index bit); M94–M97 | Carol Calin, 12 Nov 2026 |
| `accumulator_leaf_id` (accumulator.rs) | Ferrata bridge specification v2, §3.3 (`fid`) | the chain id is the record's `u64`, encoded as `uint256` | `accumulator::shared_vectors`; M100, M101 | Carol Calin, 12 Nov 2026 |
| `attest_accumulator_root`, `attest_accumulator_leaf`, `AccumulatorRoot::restore` (accumulator.rs) | (Ferrata; a storage attestation, then a SHA-256 path) | the empty leaf is refused; `restore` is trusted (for a root the consumer attested and stored) | `accumulator` (same leaf under two roots and two blocks: one identifier); M98, M99 | Carol Calin, 12 Nov 2026 |

## Weak subjectivity (milestone A1)

Added after the review above, so the rows above keep their numbers. Same columns; test names are in
`crates/ferrata-spectests/tests/weak_subjectivity.rs`, mutant ids in `scripts/mutants.py`.

| Function (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `compute_weak_subjectivity_period`, `get_balance_churn_limit` (ws.rs) | electra/weak-subjectivity.md `compute_weak_subjectivity_period`; electra/beacon-chain.md `get_balance_churn_limit` | takes the state's total active balance in Gwei, not the state (the light client never holds one); `None` where the spec's `uint64` arithmetic raises (overflow, division by zero); constants in `WsConfig::{MAINNET, MINIMAL}` from `configs/` and `presets/` of the same release | `weak_subjectivity`: `electra_period_matches_the_spec_table` (the spec's six rows), `balance_churn_limit_is_the_spec_formula`, `spec_arithmetic_failures_are_none`, `the_period_is_never_below_the_withdrawability_delay`; M113, M114 | Carol Calin, 12 Nov 2026 |
| `compute_weak_subjectivity_period_phase0`, `get_validator_churn_limit` (ws.rs) | phase0/weak-subjectivity.md `compute_weak_subjectivity_period`; phase0/beacon-chain.md `get_validator_churn_limit` | takes the number of active validators and their total active balance; `None` where the spec raises; for states before Electra | `phase0_period_matches_the_spec_table` (the spec's twelve rows), `phase0_period_in_the_branches_the_table_does_not_reach` (top-ups term, second branch); M115 | Carol Calin, 12 Nov 2026 |
| `is_within_weak_subjectivity_period`, `check_within` (ws.rs) | phase0 and electra `is_within_weak_subjectivity_period` | the store's finalized header stands for `ws_state` (only its slot is used; the spec's two asserts on state and checkpoint are the bootstrap's `CheckpointMismatch`); the period is an argument, fixed per instance; an end past `u64::MAX` is never reached; refusal `OutsideWeakSubjectivityPeriod` (53) | `within_the_period_is_current_epoch_at_most_ws_epoch_plus_period`, `an_end_past_u64_is_never_reached`; M110, M111 | Carol Calin, 12 Nov 2026 |
| `weak_subjectivity_end_slot` (ws.rs) | (Ferrata: the first slot at which the predicate above fails) | the exclusive end of the program's timestamp validity window (`Config::ws_end_ms` in `lez/wire`); `None` if past `u64` | `the_end_slot_agrees_with_the_predicate` (a sweep of slots, periods and headers); M112 | Carol Calin, 12 Nov 2026 |
| `FinalizedStore::with_weak_subjectivity_period`, `FinalizedStore::is_within_weak_subjectivity_period`, the bound in `FinalizedStore::process_update` (policy.rs) | (Ferrata policy; altair/light-client/light-client.md step 1: the trusted block SHOULD be within the weak subjectivity period) | opt-in: without a period nothing is checked (the vectors' setting); checked after the spec's validation and before `NotNewer`, against the caller's `current_slot` (a clock off chain); atomic | `a_store_older_than_its_period_refuses_a_valid_update`, `a_store_past_its_period_is_recovered_by_a_new_bootstrap`, `the_period_moves_with_the_finalized_header`, `the_bound_changes_nothing_on_the_official_sync_vectors`; M108, M109 | Carol Calin, 12 Nov 2026 |
