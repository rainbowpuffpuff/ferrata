# Per-function review of `ferrata-core` against the spec

Each row is one function: the spec section it transcribes (`consensus-specs` v1.7.0-beta.4 for
the light client; the Yellow Paper, `execution-specs` and the EIPs named for the execution layer),
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

## Execution layer

Test files are under `crates/ferrata-spectests/tests/`; mutant ids refer to `scripts/mutants.py`.

| Function (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `keccak256` (keccak.rs) | Yellow Paper `KEC` (Keccak-256, original padding) | none; the one place a guest swaps in an accelerated implementation | `exec_objects::keccak_matches_two_other_implementations` (sha3, alloy) | |
| `split_first`, `decode` (rlp.rs) | Yellow Paper, Appendix B | canonical encodings only: minimal lengths, no wrapped single byte, no leading zero in a length, items bounded by their list, no trailing bytes; lazy (never recurses) | `rlp_decoding` (corpus, alloy-rlp round trips and mutation differential, deep nesting); M44–M48 | |
| `Item::{bytes, fixed, u64, word, list, items}` (rlp.rs) | Yellow Paper, Appendix B (scalars are big-endian without leading zeros) | integers bounded by their field (64 or 256 bits) | `rlp_decoding::integers_are_canonical`, `typed_accessors`; M49–M51 | |
| `encode_u64` (rlp.rs) | Yellow Paper, Appendix B | none | `rlp_decoding::encode_u64_matches_alloy`; M52 | |
| `verify` (mpt.rs) | Yellow Paper, Appendices C (hex-prefix) and D (trie); proof format of EIP-1186 `eth_getProof` | embedded nodes are never proof elements; a node under 32 bytes referenced by hash is refused (except the root); the empty trie is proven only by the empty proof (alloy-trie also takes `[0x80]`); every element must be used | `mpt_proofs` (TrieTests, alloy-trie differential, node rules), `exec_adversarial`; M53–M66 | |
| `decode_header`, `decode_header_with_hash` (exec.rs) | `execution-specs` `forks/{cancun,prague,osaka,amsterdam}/blocks.py` `Header`; EIP-4844, EIP-4788, EIP-7685, EIP-7928, EIP-7843 | only 20, 21 or 23 fields; number, gas and timestamp limited to 64 bits (the consensus layer's `uint64`); `extra_data` at most 32 bytes | `exec_objects::header_shapes`, `header_fields_are_strict`; mainnet e2e; M67–M69 | |
| `Account::decode` (exec.rs) | Yellow Paper 4.1 (account state); EIP-2681 (nonce below 2^64) | refuses a zero storage root or code hash, so `Account::ABSENT` is unambiguous | `exec_objects::accounts_and_storage_values`; M70 | |
| `decode_storage_value` (exec.rs) | Yellow Paper 4.1 (storage values as RLP integers) | refuses a stored zero (writing zero deletes a slot) | `exec_objects::accounts_and_storage_values`; M71 | |
| `prove_account`, `prove_storage` (exec.rs) | EIP-1186 secure-trie keys `KEC(address)`, `KEC(slot)` | an absent account is all zero (as `eth_getProof` reports it) and has empty storage | mainnet e2e (absent account); M72 | |
| `Receipt::decode`, `Receipt::log`, `prove_receipt` (exec.rs) | Yellow Paper 4.3.1; EIP-658 (status); EIP-2718 typed receipts of types 1 (EIP-2930), 2 (EIP-1559), 3 (EIP-4844), 4 (EIP-7702); receipts-trie key `RLP(index)` | pre-Byzantium receipts and unknown types refused; at most four topics per log | `exec_objects::receipts_of_every_type_and_their_refusals`; mainnet e2e (245 receipts, 870 logs); M73–M76 | |
| `HeaderRecord::from_payload_header` (attest.rs) | capella `LightClientHeader.execution` (Capella to Fulu layouts) | refuses an empty payload header | `exec_objects::records_come_from_the_finalized_header`; mainnet e2e; M87 | |
| `HeaderRecord::from_block_hash` (attest.rs) | gloas `LightClientHeader.execution_block_hash` (EIP-7732) | the RLP header must hash to the bound hash; the roots come from it | `exec_objects::records_come_from_the_finalized_header`; M69 | |
| `HeaderRecord::{to_ssz, from_ssz}` (attest.rs) | (Ferrata record layout, a fixed-size SSZ container) | `from_ssz` is the trusted restore path | `exec_objects::record_encoding_round_trips_and_is_strict`; M85, M86 | |
| `attest_account`, `attest_storage`, `attest_log` (attest.rs) | (Ferrata; RFP F.3, F.5) | chain id checked first; a log needs receipt status 1 | `exec_objects`, `exec_adversarial`, mainnet e2e; M77, M78 | |
| `account_id`, `storage_id`, `log_hash`, `log_id` (attest.rs) | (Ferrata; RFP F.6) | state identifiers leave out the block, by design | `exec_objects::identifiers_are_pinned_and_depend_only_on_the_fact` (independent Python values); M92, M93 | |
| `Predicate::holds`, `Fact::field`, `Fact::check`, `attest_batch` (attest.rs) | (Ferrata; RFP F.4, soft #2 and #4) | an empty batch is refused | `exec_objects::predicates_compare_unsigned_256_bit_words`, `fact_fields_and_conditions`, `a_batch_is_one_compound_statement`; M88–M91 | |
| `history_hop` (attest.rs) | EIP-2935 (`HISTORY_STORAGE_ADDRESS`, `HISTORY_SERVE_WINDOW` = 8191, slot `n mod 8191`) | `n` from the header only; the contract's code hash must be the specified one | `exec_adversarial::history_hops_and_their_refusals`; mainnet e2e (three real hops); M79–M84 | |
