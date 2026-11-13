# Per-entry-point review of the guest programs against the spec

Each row is one entry point of a guest program with an image id ([image-ids.md](image-ids.md)), or one check it
runs: the specification it implements, any deliberate deviation, what pins it, and the author's sign-off. Line
numbers are at `bf430cc`. The guests may not change (their image ids are those of the live round trip), so a
deviation found here is fixed, if at all, by a new program and a new id. As in [REVIEW-core.md](REVIEW-core.md), the
deviations column is part of the audit packet; the light-client core the attestation program runs (`ferrata-core`,
`steps.rs`) has rows there and is not repeated here.

Specifications: `v2 §n` is [bridge-spec-v2.md](design/spec/bridge-spec-v2.md); `Dn` an item of
[bridge-spec-v3-changes.md](design/spec/bridge-spec-v3-changes.md); `att §n` is
[the attestation design note](design/attestation.md); `BRIDGE §n` is [the LEZ bridge program's plan](../lez/BRIDGE.md);
`bridge.md §n` is [the bridge design note](design/bridge.md); "checkpoint README" is
[`checkpoint/README.md`](../checkpoint/README.md), "What the program checks" and its table "Choices this
implementation makes", the checkpoint design as built. Review ids (C-1, H-1, M-3, I-10, …) are those of
[the security review of 11 November](../notes/reviews/security/review-2026-11-11.md).

Files: `bridge/crates/ferrata-bridge-core/src/` is `BC/`; `checkpoint/crates/checkpoint-core/src/` is `CC/`;
`checkpoint/crates/attestor-core/src/lib.rs` is `AT`; `lez/guests/program/src/main.rs` is `P`,
`lez/guests/bridge/src/main.rs` `B`, `lez/guests/consumer/src/main.rs` `C`, `lez/guests/holdings/src/main.rs` `H`;
`lez/bridge-wire/src/lib.rs` is `BW`, `lez/wire/src/lib.rs` `W`.

Pinned by: test names in the crate's `tests/` (bridge core: `outbound`, `inbound`, `tree`, `encoding`, `vectors`,
`secfix`; checkpoint core: `units`, `rule_b`, `cross_vectors`; rule (A): `checkpoint/fidelity/tests/rule.rs`;
`attestor`), in `lez/harness/tests/<file>.rs` (the programs run by LEZ's own state machine, dev mode unless the row
says otherwise), the fuzz targets of `fuzz/programs/` (EVIDENCE row 78), the CI gate of row 77 (cycle counts
independent of the witness), and the real seals the vault verifies in `vault/test/real/` (EVIDENCE rows 24, 64–68,
83, 91). The live round trips on the final programs are EVIDENCE rows 91–92.

Signed: not yet. Drafted on 12 November 2026 for the owner to read and sign, row by row, with the agent review of
12 November.

## The release guest and the bridge core's statements (`bridge/guests`)

The agent review of these rows: [reviews/agent-review-vault-guests-2026-11-12.md](reviews/agent-review-vault-guests-2026-11-12.md).

| Entry point or check (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `release` guest `main` (`bridge/guests/src/bin/release.rs`) | v2 §5.2 | reads a fixed 1,340-byte input; an invalid witness panics, so no receipt exists; commits exactly the vault's 352-byte journal | `vault/test/real/ReturnLeg`, `real/CrownR1::test_crownR4_*` (real seals); row 77 (64 witnesses, one cycle count); fuzz `release`; EVIDENCE rows 83, 91 | |
| `verify_release` (BC/outbound.rs:185) | v2 §5.1 (`COut`, `burnLeaf`), §5.2; D13 (`NOut = H("ferrata/nf-out/v2" ‖ sOut)`) | `relayer` and `relayFee` are not checked in the guest: the journal binds them and the vault accepts only 0 or the registered fee | `outbound::release_verifies_and_commits_the_vault_journal`, `every_single_field_change_fails_or_changes_the_journal`, `attack_burn_from_another_config_cannot_satisfy_a_release`, `attack_wrong_token_or_amount_fails`, `same_burn_same_nullifier_under_every_later_root`; `secfix::m3_a_reindexed_burn_is_released_once` | |
| `burn_commitment`, `burn_leaf`, `burn_nullifier`, `burn_id` and their preimages (BC/outbound.rs:50–76; BC/encoding.rs) | v2 §5.1–5.2; D13 | `burnId` is no longer in `NOut` (D13) | `vectors` (`outbound_derivations`, `burn_tree_and_checkpoint`), `outbound::derivations_bind_every_input`, `encoding::layouts_match_the_spec_field_by_field` | |
| `verify_path`, `root_from_path`, `ordered` (BC/tree.rs:80, 69, 56) | v2 §3.3, §5.2 (direction bits equal the index); D9 (index below 2^32) | left and right chosen with a mask, not a branch (the cycle count must not depend on the witness: D12) | `tree::direction_bits_must_equal_the_leaf_index`, `a_path_to_the_root_with_other_direction_bits_is_still_rejected`, `every_path_verifies`, `tampered_path_leaf_or_root_fails`; row 77 | |
| `ReleaseInput::decode`, `ReleaseJournal::encode` (BC/encoding.rs:688, 555) | v2 §5.2 (journal: 11 ABI words) | fixed-size input, decoded without checks (every byte string is some witness) | `encoding::release_input_round_trips`, `release_journal_is_eleven_abi_words`; vault `JournalBinding`, `real/ReturnLeg::test_releaseJournals_areTheVaultsEncoding` | |
| `claim` guest `main` (`bridge/guests/src/bin/claim.rs`) | v2 §3.3 (the claim statement) | a measurement guest, not a protocol component: no program or contract accepts its receipt | row 77; fuzz `release` (the claim input) | |
| `verify_claim`, `commitment`, `deposit_leaf`, `deposit_id`, `nullifier` (BC/inbound.rs:94, 49–64) | v2 §3.2–3.3; D9 | none | `inbound::claim_verifies`, `every_single_field_change_fails_or_changes_the_nullifier`, `attack_same_deposit_under_two_roots_gives_same_nullifier`, `attack_private_kind_commitment_cannot_be_opened_as_public`; `vectors::inbound_derivations` | |
| `gasdrop` guest `main` (`bridge/guests/src/bin/gasdrop.rs`), `verify_gas_drop`, `GasDropJournal::{encode, decode}`, `drop_domain` (BC/gasdrop.rs:131, 87, 107, 27) | bridge.md §6; D13 (the receipt binds `(N, oneTimeAddress, relayer)`) | the journal also carries the root, chain and vault the relayer checks; `DROP_DOMAIN` makes it unreadable as another journal | `gasdrop` unit tests (3); EVIDENCE rows 47, 48 | |
| `ClaimInput::decode`, `claim_journal` (BC/encoding.rs:767, 793) | v2 §3.3 | an unknown kind byte panics (no receipt) | `encoding::claim_input_round_trips`, `claim_input_rejects_an_unknown_kind` | |

## The checkpoint guests (`checkpoint/guests`: `checkpoint`, `checkpoint_b`, `segment`)

| Entry point or check (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `checkpoint` guest `main` (rule (A)) | v2 §4.2 with D3; checkpoint README ("Composition") | segment journals verified with `env::verify` against the segment image id compiled in, before the rule runs; the doc comment's "416-byte journal" is SPEC-v2's size: it commits 544 bytes (agent review) | `fidelity` rule tests (10); `vault/test/real/ReturnLeg` (rule (A) seals); EVIDENCE row 83 | |
| `checkpoint_b` guest `main` (rule (B)) | D7, D13 | as rule (A)'s guest, with `run_checkpoint_b` | `rule_b` (14); `vault/test/real/RuleBReal`, `RuleBMulti`, `RealBlocks`, `CrownR1`; EVIDENCE rows 24, 29, 64–68, 91 | |
| `segment` guest `main`, `run_segment` (CC/rule.rs:349) | checkpoint README ("Composition": the crate's own segment journal) | no support check and no key rotation in a segment; at least one block | `fidelity::segments_compose_to_the_same_journal`; fuzz `checkpoint_input` | |
| `CheckpointInput::decode`, `CheckpointInputB::decode`, `SegmentInput::decode` (CC/input.rs:292, 313, 333) | (the guests' input format) | strict: every length bounded by the bytes left, nothing after the last field | fuzz `checkpoint_input`, `checkpoint_b` | |
| `up_to_tip` (CC/rule.rs:219) | v2 §4.2 items 1–3; D3 (posting timeframe); D10 | rotations are applied to the parent's key set before the blocks; the parent's own set is checked against the floors only when no rotation replaces it | `fidelity::key_rotations_need_t_current_signatures_and_respect_the_floors`, `segments_compose_to_the_same_journal`; `rule_b::a_key_removed_by_a_rotation_is_refused_and_a_key_added_counts` | |
| `link_block`, `parse_block`, `hash_parsed_block` (CC/rule.rs:75; CC/lez.rs:128, 174) | v2 §4.2 item 1 | the parser is LEZ v0.4.0-rc1's `borsh(Block)`, strict; ids must increase by one | `units::block_parses_and_hashes`, `block_parser_is_strict`; `fidelity::broken_links_and_reordering_are_refused`; fuzz `lez_block` (differential against LEZ's decoder); EVIDENCE row 24 (202 real blocks) | |
| `apply_message`, `extend`, `BridgeMessage::decode` (CC/rule.rs:102, 127; CC/wire.rs:88) | v2 §4.2 item 2; D10 (only visible calls); D13 (`importedLeafCount` = the imported count) | `importedLeafCount` is the maximum seen and frozen by `Sunset`; a malformed message is ignored; `Echo` has no `configId` | `fidelity::a_checkpoint_collects_exactly_the_non_reverted_burns_of_its_configuration`, `reverted_and_foreign_burns_are_not_collected`, `sunset_and_imports`; `units::wire_codec_equals_borsh_derive`, `wire_import_root_skips_its_proof_strictly`, `wire_decode_is_exact`; harness `bridge_burn::burns_and_what_the_checkpoint_collects`; fuzz `bridge_wire` | |
| `apply_config_change`, `check_key_set`, `threshold` (CC/rule.rs:165, 144; CC/hash.rs:37) | v2 §4.2 item 3; D3; D10 | at least `t(current n)` distinct current keys sign, not "exactly the live `configuration_threshold`" (I-10, not fixed); repeated keys refused | `fidelity::key_rotations_need_t_current_signatures_and_respect_the_floors`; `units::threshold_matches_lez`; `review_guests` (3: a repeated key refused in a rotation and in the parent; a genesis parent's repeat not covered); EVIDENCE rows 67, 68; fuzz `mantle` | |
| `run_checkpoint`: `Y`, producer, support (CC/rule.rs:270) | D3 item 4; v2 §4.2 items 4–5 | an echo counts in a reverted transaction; support entries by `D` are checked, not counted; one key only when `nMin ≤ 1` | `fidelity::a_finalisation_checkpoint_counts_support_only_after_the_echo`, `support_must_come_from_enough_distinct_accredited_keys_other_than_t_s_producer`, `the_producer_is_a_checked_inscription_of_t`, `a_single_key_zone_needs_n_min_one` | |
| `ElidedInscription::check`, `verify_strict`, `tx_hash` (CC/mantle.rs:149, 204, 98) | D3 item 4 (channel pinned, payload = `T`'s bytes, Ed25519 over the Mantle transaction hash) | `verify_strict` (small-order keys and non-canonical signatures refused) | `units::inscription_check`; fuzz `mantle`; EVIDENCE row 24 (real inscriptions) | |
| `run_checkpoint_b` (CC/rule_b.rs:54) | D7; D13 (final-tier message v2, certificates, `t` attestations over `T` or a descendant) | also checks one inscription of `T` by a key of `K` (rows 29); `finalMsgId` is signed, not checked; a repeated channel or attestation key refuses the input | `rule_b` (14 tests); EVIDENCE rows 29, 64–68; fuzz `checkpoint_b` | |
| `SignedAttestation::verify`, `certificate_message`, `attestation_message`, `key_set_hash` (AT:201, 99, 107, 154) | D13 (`attest-key/v1`, `final-tier/v2`); D3 (`keyset/v2`) | the certificate also binds `evmChainId`, `vault`, `lezConfigId` (one certificate per deployment) | `attestor` (6 tests: vectors, layout, every field bound, small-order key) | |
| `Frontier::{append, root, hash, is_canonical}`, `burn_leaf` (CC/tree.rs:53, 71, 80, 49, 19) | v2 §5.1 (burn leaf, tree); D12 (tagged canonical frontier) | at most 2^32 − 1 leaves (a frontier cannot hold the last one) | `units::frontier_root_equals_naive_root_and_stays_canonical`, `frontier_rejects_non_canonical_and_full`; `cross_vectors::burn_tree` | |
| `State::digest`, `Config::config_id`, `Config::allow_single_key` (CC/state.rs:100, 30, 48) | v2 §1 (`configId`), §4.1 (`S`); D3 (`keySetHash` v2) | single-key support tied to `nMin ≤ 1`, not a separate constant | `cross_vectors::config_ids`, `key_sets_states_journals`; `rule_b::single_key_only_where_the_floor_allows_it` | |
| `CheckpointJournal::encode`, `SegmentJournal::{encode, decode}` (CC/journal.rs:68, 110, 119) | D3 item 5 (17 words) | the segment journal is the crate's own | `units::journal_layout`; `rule_b::journal_is_rule_a_candidate_journal_over_the_same_blocks`; vault `JournalBinding` and `real/` | |

## The attestation program (`lez/guests/program`, `ferrata_lez`)

| Entry point or check (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `main`, `handle` (P:50, 69) | att §3.1–3.2 (actors, messages), §5.5 (codes) | refusals exit with a code (LEZ charges the cycles used) | `exit_codes` (3); `public_path::refusals_exit_with_a_code_and_are_charged_what_they_used`; `attest_public::refusals_exit_with_their_codes_and_change_nothing` | |
| `init`, `check_config`, `period_window` (P:209, 129, 164) | att §3.4 (`id = H(config ‖ checkpoint)`), §4.2, §4.6 (weak subjectivity) | `Init` is valid only inside the checkpoint's weak-subjectivity period, by the sequencer's clock | `public_path::mainnet_replay_through_lez`; `successor::the_weak_subjectivity_bound_on_init_and_open`; EVIDENCE rows 69, 70 | |
| `open`, `update_and_store`, `update_window` (P:241, 175, 147) | att §4.2 (public job, step 1), §4.6, §4.7 | `current_slot ≥ signature_slot` rests on the transaction's validity window, the sequencer's clock (I-5) | `public_path::a_finality_update_is_three_transactions`, `mainnet_replay_through_lez`; `sepolia::*` | |
| `job_open`, `miller`, `adopt`, `finish`, `complete` (P:355, 376, 382, 392, 198) | att §4.2 (steps 2–3), §4.3 (adoption in parts), §4.5 | the steps after `Open` are open to anyone (the job is fixed when it opens) | `public_path::a_finality_update_is_three_transactions`, `refusals_exit_with_a_code_and_are_charged_what_they_used` | |
| `apply`, `apply_job` (P:269, 307) | att §4.2, §4.5 (a job applies only to the store it started from) | an activation of an active instance, or an update of an inactive one, is refused | `public_path`; `private_placement::an_update_verified_privately_applies_to_the_public_instance` | |
| `verify_all`, `apply_verified` (P:470, 288) | att §4.2 (the proven path), §3.2 (`ApplyVerified` authenticated by program) | an `ApplyVerified` activation is refused; adoption in one part | `private_placement` (3 tests); `public_path::a_finality_update_in_one_public_transaction` | |
| `write_record` (P:405) | att §3.1 (`header(id, slot)` written once), §3.3 | a rewrite with the same bytes is accepted | `public_path`; `attest_public::every_kind_of_fact_is_attested_under_the_real_record` | |
| `attest`, `attest::evaluate` (P:435; W) | att §5 (inclusion, identifiers, predicates, hops), §6.1 | a request bound to the root id is served by any successor's record (att §3.5) | `attest_public` (4), `attest_native` (2), `sepolia::real_sepolia_facts_under_a_gloas_record`; EVIDENCE rows 71, 72 | |
| `read` (`Read`, `ReadPadded`) (P:463) | att §6.2 (private consumers read) | needs a sender to reply to | `private_consumer` (2), `attest_private` (3) | |
| `succeed`, `check_successor`, `spawn` (P:537, 508, 571) | att §3.5 (three invariants) | anyone may `Succeed` an active instance | `successor::a_successor_crosses_a_fork_not_in_the_schedule_fixed_at_init`; EVIDENCE row 70 | |

## The bridge program (`lez/guests/bridge`, `ferrata_bridge`)

| Entry point or check (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| `main`, `handle`: reply routing and dispatch (B:53, 105) | BRIDGE §2 (actors, messages), §4.6 (codes 240–255) | a reply is accepted only from the actor this one waits for | `bridge_inbound::the_bridge_codes_are_its_own_block`; `exit_codes`; fuzz `bridge_handle`, `bridge_wire` | |
| `init`, `check_pairing`, `config_reply` (pairing) (B:237, 342; BW:1359) | v2 §1 (reciprocal pairing); BRIDGE §4.1–4.2 (`attestationRootId`, `salt` from the controls) | none | `bridge_inbound::inbound_configuration_import_and_plaintext_claim`; `bridge_secfix::c1_a_a_claim_cannot_block_init` | |
| `import_root`, `check_import`, `write_root` (B:260, 410; BW:1336) | D13 (slots 0 and 1 under one record); v2 §4.5 (none after `Sunset`); BRIDGE §4.4–4.5 (`root(cid, n)` once per count) | only a top-level call (H-1): what the checkpoint sees is what was imported | `bridge_inbound`; `bridge_secfix::h1_a_nested_import_is_refused`, `h1_b_import_with_a_sender_is_refused`, `c1_d_a_claim_cannot_block_an_import`; `bridge_vault_storage` | |
| `register`, `check_register`, `WriteToken` (B:291, 187; BW:1376) | v2 §3.4 (write-once registry); BRIDGE §5 (entry and schedule 0 attested, bounds) | the bounds are checked before the attestation | `bridge_controls::registry_fees_caps_freezes_and_sunset`; `bridge_vault_storage::the_bridge_reads_the_vaults_own_registry_fees_and_tree`; `bridge_secfix::c1_b_*` | |
| `import_fees`, `check_fees`, `extend_fees` (B:319, 431; BW:1409) | D13 (a deposit keeps the schedule of its leaf index); BRIDGE §5 ("Fees by leaf index") | `known_until` bounds which leaves have a known schedule | `bridge_controls`; `bridge_vault_storage` | |
| `claim` (plaintext), `verify_claim`, `issue_message` (B:519, 494; BW:1453) | v2 §3.3; D9; D13 (`leafIndex < count`) | public destinations only; the claim names its fee-schedule range | `bridge_inbound` (3); fuzz `bridge_handle` | |
| `claim_privately`, `claim_reply` (B:546, 568) | v2 §3.3 (a private-kind claim executes as its destination); BRIDGE §2 ("What each path makes public") | the leaf index stays in the private execution; the range is published | `bridge_private` (2); `bridge_secfix::c1_c_a_claim_cannot_destroy_a_pending_claim`; privacy gate, EVIDENCE rows 23, 89 | |
| `issue`, `TokenState::fees_for`, `admit` (B:600, 649; BW:1159) | D13 (the claim mints the denomination and the fees); BRIDGE §5 (caps, freezes) | the relay fee goes to the named relayer, else to the fee recipient | `bridge_controls`; `bridge_time::freezes_and_caps_follow_attested_time_only` | |
| `spend`, `Mint` (B:672, 165) | v2 §1 (one nullifier namespace `(chain, vault, N)`) | mints land only at `holder(account)` PDAs (C-1) | `bridge_secfix::c1_c_*`, `c1_f_a_mint_is_accepted_only_at_its_holder`; `bridge_inbound` | |
| `burn` (four receivers) (B:720) | v2 §5.1; D10 (a burn only as a root call or a private root); D13 (the burn account keyed on the whole note) | a public holder's burn forwards to its `holder(account)` (a nested call the checkpoint need not see) | `bridge_burn::burns_and_what_the_checkpoint_collects`; `bridge_secfix::c1_e_a_claim_cannot_block_a_burn`; `bridge_private` | |
| `transfer`, `credit` (B:806, 862) | BRIDGE §5 ("Transfers"; not in the specifications) | amounts in multiples of `lezUnit`; not gated by freezes or caps | `bridge_roles::a_private_holder_transfers`, `transfers_handover_multisig_watchers_and_keyless_sunset`; EVIDENCE row 75 | |
| `role_message` (`Sunset`, `Freeze`, `Pause`, `SetCap`, `ProposeRoles`, `CancelRoles`) (B:902) | D13 (roles); BRIDGE §4.9–4.11, §5 | role messages are top-level calls at the role's own actor, forwarded to `config(cid)`; an executed `Sunset` is always visible to the checkpoint | `bridge_controls`; `bridge_roles`; `bridge_time`; EVIDENCE row 74 | |
| `cosigned` (B:1069) | BRIDGE §5 (multisig freeze authority), §4.11 | a multisig `Sunset` only approves; anyone then sends the visible `Sunset` | `bridge_roles::transfers_handover_multisig_watchers_and_keyless_sunset` | |
| `attest_retired`, `check_retired` (B:1026; BW:1433) | D13 (keyless `Sunset`); BRIDGE §4.10 | reads the vault's `retired` flag at slot 37, byte 11 (FerrataVault v5's layout) | `bridge_roles` (a synthetic slot 37); vault side: `ReviewVault::test_review_slot37_*` | |
| `apply_roles`, `tick`, `apply_time`, `token_time_reply`, `record_time` (B:1049, 1179, 1201, 1239; BW:521) | BRIDGE §6 (attested Ethereum time); D13 (14-day handover, 12-hour pause) | a role proposal is valid only within a day of the record it reads | `bridge_time`; `bridge_roles`; EVIDENCE row 28 | |
| `Echo`, `Read` (B:137, 146) | v2 §4.2 item 4 (the echo); LEZ's read idiom | `Echo` is accepted at any actor and changes nothing | `fidelity::a_finalisation_checkpoint_counts_support_only_after_the_echo` (the checkpoint's side) | |

## The consumers (`lez/guests/consumer`, `lez/guests/holdings`)

| Entry point or check (file) | Spec section | Deviation | Pinned by | Reviewed |
|---|---|---|---|---|
| reference consumer `ReadRecord`, `ReadPadded`, `CallAttest` and their reply (C:63–83, 308) | att §6.1–6.2 (call and read), §7 | none | `private_consumer` (2); `attest_private::a_private_consumer_calling_attest_publishes_its_request` | |
| `Claim` and its reply (C:252, 308) | att §7 (the Recommended Consumer Pattern: one claim actor per statement) | none | `attest_public::the_reference_consumer_credits_a_transfer_once` | |
| `AttestPrivately` and its reply (C:88, 308) | att §6.2 | the record's own instance id binds the request (not the root id) | `attest_private::a_private_consumer_attests_a_transfer_and_only_the_read_is_public`, `a_private_attestation_that_does_not_verify_cannot_be_proven` | |
| `ClaimDeposit`, `ClaimDepositPrivately`, `Spend`, `CreditHolder`, `Credit` (C:127, 152, 177, 210, 274) | att §7 rules 5–7 (a vault deposit through its event, `C` opened, `N` from the secret) | none | `vault_consumer` (4); EVIDENCE row 73 | |
| holdings `Check` and its reply (H:31) | att §6.1; the second consumer's design (`lez/holdings-wire`) | a later check must be at a later block | `holdings::holdings_consumer`; EVIDENCE row 39 | |

**Not covered by the rows above.** `ferrata_lez_keccak_soft` and `ferrata_consumer_keccak_accel` are the
attestation program and the reference consumer built with the other Keccak, for the cycle comparison only;
`scripted` is LEZ's own test program, copied; `micro` is a benchmark guest. None of them is deployed by the
bridge. The wire crates' encodings (`lez/wire`, `lez/bridge-wire`, the IDL) are pinned by EVIDENCE rows 55 and 58.
