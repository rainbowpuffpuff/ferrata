# Independent review of `ferrata-core` against the specifications (docs/REVIEW-core.md, 43 rows)

Reviewer: a headless agent (Claude Code, Opus 5.5), 11 November 2026, 14:15–14:44 CET. Modal
spend for this review (mrun ledger estimates, profile `deeferentleeg`): 0.30 USD over 11 calls.
Code: branch `integration6` at `c52eee4` (every `file:line` below is at that commit; the review
branch changes no file of `ferrata-core`). Review branch: `review-core` in
`spike/ferrata-wt-review-core` (tests and mutant scripts only; tip in §4).
Spec text used (kept under `spike/tmp/review-core/spec/`, with `SOURCES.txt`): consensus-specs
`v1.7.0-beta.4` (light-client `sync-protocol.md` and `fork.md` of Altair, Capella, Deneb, Electra,
Gloas; `phase0/beacon-chain.md`; `altair/beacon-chain.md`; `altair/bls.md`; `gloas/beacon-chain.md`); the SSZ text of
consensus-specs `v1.6.0` (the `ssz/` directory is gone at `v1.7.0-beta.4`); execution-specs
`forks/{cancun,prague,osaka}/blocks.py` (master) and `forks/amsterdam/blocks.py` (commit `1689800`,
identical to the branch tip); EIP-2935 (ethereum/EIPs master). Line numbers `alt-sp:406` etc. refer
to those files (`alt-sp` = `specs/altair/light-client/sync-protocol.md`, `cap-sp`, `den-sp`, `ele-sp`,
`glo-sp` likewise; `*-fork` = the light-client `fork.md`).

What ran (all on Modal, profile `deeferentleeg`, through `mrun`; measured):
- `scripts/fetch-vectors.sh && cargo test -p ferrata-core -p ferrata-spectests --locked`: **142 passed, 0
  failed** (log `spike/tmp/review-core/test1.log`). Every test a row names exists and passed.
- New tests on the review branch, `crates/ferrata-spectests/tests/review_core.rs`: 6, all pass on
  `c52eee4`'s code (they pin what the code does where a row says otherwise or where nothing pinned it).
- New mutants, `scripts/review-mutants.py` (same runner as `scripts/mutants.py`, whole workspace):
  R01–R13 on the 43 rows' functions, S01–S11 on `steps.rs` (results in §2.3).

## Summary

| Verdict | Rows |
|---|---|
| OK | 8 |
| deviation as documented | 33 |
| DOC MISMATCH | 2 |
| PROBLEM | 0 |

No row transcribes its spec wrongly; I found no input on which a function returns something the
spec (or, for Ferrata's own rows, the stated design) would not. The two DOC MISMATCHes are
statements of a row that the code contradicts, both without a safety consequence on the shipped
path. §2 also lists six test gaps that let a plausible bug survive (three in the rows' functions,
three in `steps.rs`; all closed by new tests on the review branch) and one scope gap that matters more for the sign-off than any single row: the
on-chain program runs a **second transcription** of rows 1, 8 and 11 (`steps.rs`), which has no row.

## 1. Rows

`V` = `crates/ferrata-core/src/validate.rs`, `S` = `store.rs`, `P` = `policy.rs`, `U` = `upgrade.rs`,
`M` = `merkle.rs`, `Z` = `ssz.rs`, `T` = `types.rs`, `B` = `bls/blst_backend.rs`,
`K` = `bls/zkcrypto_backend.rs`, `R` = `rlp.rs`, `MP` = `mpt.rs`, `X` = `exec.rs`, `A` = `attest.rs`,
`AC` = `accumulator.rs`.

| # | Function | Verdict | Evidence (code; spec) | Finding |
|---|---|---|---|---|
| 1 | `validate_light_client_update` | deviation as documented | V:251-376; alt-sp:406-489 | Every spec assert is there in spec order: participation V:276, attested header V:281, slot order V:286-291, period V:292-302, relevance V:305-310, finality V:315-339, next committee V:343-362, committee by period and signature V:365-375. The added shape checks (V:263-272) only refuse inputs the spec's fixed-size types cannot hold. Nit: of "M03–M17", M05–M07 are in `types.rs`/blst, not here. The finalized header's layout check was unpinned (R12 survived; new test). |
| 2 | `is_valid_light_client_header` | deviation as documented | V:144-180; cap-sp:149, den-sp:64, glo-sp:211 | Payload layouts follow the Deneb text (blob-gas guard V:152-155, pre-Capella empty V:156-158, branch at gindex 25 V:159-165); the Gloas layout follows the Gloas text (V:167-178: 2856 / 812 / 412 by era, normalized branch; pre-Capella zero hash and branch). |
| 3 | `get_lc_execution_root` | deviation as documented | V:99-140; cap-sp:137, den-sp:30, glo-sp:177 | Matches the three texts, including the Gloas reconstruction of the payload root from `block_hash` (`inner`, tail of depth 4/5, V:99-104) and the genesis cases. Electra defines no own version (it inherits Deneb's), so "electra" in the row is loose. The store-layout dispatch `fork >= LcFork::Deneb` (V:112) was unpinned: R03 survived; new test. |
| 4 | `is_better_update` | OK | V:193-239; alt-sp:251-310 | Line-for-line, with the same `>=` supermajority and the three tiebreakers (M18, M19, M21). |
| 5 | gindex helpers | OK | V:68-96, M:15-46; ele-sp:97-131, glo-sp:135-175, tables alt-sp:103-105, ele-sp:88-90, glo-sp:99-113 | Constants equal the spec tables (105/54/55, 169/86/87, 735/2945/2946, 25, 412, 812, 2856). Selection: by the store's layout, then by the attested (not finalized) slot, as each fork's helper (M25, M31). |
| 6 | `sync_committee_signing_root` | OK | V:243-248, V:47-65, fork.rs:150-157; alt-sp:482-488, phase0:1366/1382/1401 | `compute_fork_version(epoch(saturating_sub(signature_slot, 1)))`, `DOMAIN_SYNC_COMMITTEE = 0x07000000`, `ForkData`/`SigningData` roots as two-chunk hashes. `compute_fork_version` is the generic "latest fork whose epoch has passed" (I did not find its definition in the tag's fork documents; vectors and the Sepolia digest test pin it). |
| 7 | `StoreCore::from_bootstrap` | OK | S:41-75; alt-sp:363-384 | The three asserts in order (S:55, S:58, S:61), empty next committee. Adds the same shape checks as row 1 (S:47-54) although the row says "none": refusals of malformed input only. |
| 8 | `apply_core` | deviation as documented | S:109-130 (+S:167-177); alt-sp:494-510 | Precondition checked before any write (S:113-116); rotation S:119-124; the participation and optimistic bookkeeping is in `LightClientStore::apply`. `adopted` on rotation is pinned (R01 killed). |
| 9 | `LightClientStore::process_update` | deviation as documented | S:180-218; alt-sp:537-580 | Same effects in the same order on a clone, committed at S:216; the safety threshold is read after the participant maximum is raised, as in the spec. The spec mutates three fields before `apply`'s assert; here an error changes nothing. |
| 10 | `LightClientStore::process_force_update` | deviation as documented | S:221-236; alt-sp:516-531 | Timeout and `best_valid_update` condition S:222-225, `<=` at S:230 (M27), atomic. |
| 11 | `FinalizedStore::process_update` | deviation as documented | P:98-122 | Refuses non-finality (P:104), below 2/3 of the preset size (P:107-110), runs row 1 with the checked-committee path (P:111), refuses an update that advances nothing (`NotNewer`, P:112-114: the spec would silently keep it as best/optimistic; implied by "no optimistic header", not stated), checks an adopted committee before committing (P:117-120). `from_core`'s next-committee check was unpinned (R07 survived; new test). The program runs `steps.rs` instead (§2.4). |
| 12 | `upgrade_header`, `upgrade_bootstrap`, `upgrade_update` | deviation as documented | U:18-154; cap-fork:26-54, den-fork:26-74, ele-fork:26-79, glo-fork:26-120 | Each step equals its `fork.md` function; the Gloas header proof is the payload-header proof of `block_hash` (T:264-267) followed by the execution branch, normalized (U:26-31); state branches normalized when crossing Electra and Gloas (U:81-96). R02 (Deneb proof for a Capella-era header) killed. |
| 13 | `is_valid_normalized_merkle_branch`, `normalize_merkle_branch` | OK | M:89-125; alt-sp:334-347, phase0:1195, ele-fork:26-34 | Same padding test and depth/index arithmetic; a short branch is `false` where Python raises. R10 (padding at the wrong end) killed. |
| 14 | `split_container`, container decoders | deviation as documented | Z:39-88, T:88-518; SSZ (v1.6.0 text) offsets and containers | Exact consumption (Z:66-70), first offset = fixed part (Z:68), non-decreasing in-bounds offsets (Z:71-78), Bitvector padding (T:343), `ByteList[32]` (T:193). The hash-tree roots (Capella 16 leaves, Deneb 32, `SyncCommittee`, `BeaconBlockHeader`) match the containers. |
| 15 | `Blst::fast_aggregate_verify`, `Zkcrypto::fast_aggregate_verify` | deviation as documented | B:235-250, B:145-203; K:93-107, K:74-90; phase0:1082 (BLS) | `KeyValidate` on each signer (B:102-109, K:58-64), identity aggregate refused (B:246), signature decoded and subgroup-checked (B:148-154), POP DST (mod.rs:41), `e(−pk, H(m))·e(σ, g1) = 1`. The identity signature is refused before the pairing (B:157): same verdict as the spec. zkcrypto runs one two-pair `multi_miller_loop`, not "two Miller loops". |
| 16 | `check_committee` | deviation as documented | B:127-142, B:252-254; K:45-56; altair/bls.md:36-50 | `KeyValidate` on every key and the sum against the decoded `aggregate_pubkey`; empty committee refused (M35). |
| 17 | `check_committee_onchain` | deviation as documented | B:92-99, B:256-258; K:36-42 | As documented, and safe on a genuine state for a stronger reason than the row gives: every sync committee is built by `get_next_sync_committee` → `eth_aggregate_pubkeys`, which asserts `KeyValidate` on each key (altair/beacon-chain.md:362, altair/bls.md:47), besides deposit-time `Verify` (phase0:2543). |
| 18 | `Blst::miller_half`, `Blst::final_half` | **DOC MISMATCH** | B:37-51, B:210-227; steps.rs:413, steps.rs:765 | `miller_half` writes canonical coefficients, but `final_half` reads each 48-byte coefficient modulo p (`blst_fp_from_bendian`), so it accepts a second encoding of every product. The canonical check exists only in the caller `steps::Job::step3` (`canonical_fp12`), which the row does not name. §2.1. |
| 19 | `verify_checked` | deviation as documented | B:260-269, B:54-76; K:117-135 | Subtractive `aggregate − Σ absent` (B:58-68) or additive; identity key refused (B:72); keys decoded on-curve only, so correct only for checked committees, as `bls/mod.rs:94-96` says. All compositions run on every vector. |
| 20 | `keccak256` | OK | keccak.rs:12-18; YP `KEC` | `tiny_keccak` `Keccak::v256` (original padding); pinned against two implementations. |
| 21 | `split_first`, `decode` | deviation as documented | R:38-92 | Wrapped single byte refused (R:62), long-form length without leading zero and ≥ 56 (R:72, R:79), bounded by input (R:56-58), no trailing bytes (R:88); iterative. |
| 22 | `Item::{bytes, fixed, u64, word, list, items}` | deviation as documented | R:96-153 | No leading zero (R:111), at most 8 or 32 bytes (R:114), exact arity (R:148). |
| 23 | `encode_u64` | OK | R:184-199 | `0x80` for zero, the byte itself below `0x80`, else `0x80+len` and minimal big-endian bytes. |
| 24 | `verify` (MPT) | deviation as documented | MP:174-227, MP:82-164 | Hash check MP:185-188; a hashed node under 32 bytes refused except the root (MP:189-192); embedded only as a list under 32 bytes (MP:100-112); empty-value leaf and empty-path extension refused (MP:151-153, MP:156-158); every element used (MP:223-225); empty trie only by the empty proof (MP:175-177). Exclusion at an empty slot, a diverging path or an empty branch value is sound for a canonical trie. Same element set as geth's `Prove` (embedded nodes are not proof elements). |
| 25 | `decode_header`, `decode_header_with_hash` | deviation as documented | X:93-143; execution-specs `Header` of cancun (20), prague/osaka (21), amsterdam (23) | Field order and types match all three; number, gas and timestamp narrowed to `u64`, `extra_data` ≤ 32 bytes, `logs_bloom` exactly 256 bytes, `nonce` 8 bytes. |
| 26 | `Account::decode` | deviation as documented | X:168-180; YP 4.1, EIP-2681 | Four items, nonce `u64`, balance ≤ 32 bytes, both roots 32 bytes and non-zero. |
| 27 | `decode_storage_value` | deviation as documented | X:185-191 | The leaf value is `RLP(uint)`; a stored zero refused. |
| 28 | `prove_account`, `prove_storage` | deviation as documented | X:194-213 | Keys `KEC(address)`, `KEC(slot)`; absent account is `Account::ABSENT` and its storage only the empty proof of `EMPTY_ROOT`. |
| 29 | `Receipt::decode`, `Receipt::log`, `prove_receipt` | deviation as documented | X:261-306, X:341-349; execution-specs osaka `encode_receipt` | Types 1–4 and legacy lists only; a 32-byte first item (state root) refused; status exactly 0 or 1; ≤ 4 topics; key `RLP(index)`. |
| 30 | `HeaderRecord::from_payload_header` | deviation as documented | A:102-121 | Roots from the finalized header's payload (proven against `body_root` when the update was accepted). "Refuses an empty payload header" is implemented as "refuses a zero `block_hash`" (A:107), which covers it. |
| 31 | `HeaderRecord::from_block_hash` | deviation as documented | A:125-145; glo-sp:113 (gindex 2856), gloas/beacon-chain.md:2193 and :1830 | The bound hash is the bid's `parent_block_hash`, which the state transition requires to equal `state.latest_block_hash`, the last delivered (FULL) payload; the RLP header must hash to it. |
| 32 | `HeaderRecord::{to_ssz, from_ssz}` | deviation as documented | A:256-310 | 165 bytes = `(uint64 ×4, Bytes32 ×4, uint8, uint32)`; binding/hops consistency enforced (A:293-298). |
| 33 | `HeaderRecord::anchor_slot` | deviation as documented | A:221-223, A:795-797 | Hops keep the anchor (M107). |
| 34 | `attest_account`, `attest_storage`, `attest_log` | deviation as documented | A:611-653 | Chain id first (A:617, A:629, A:645), status 1 for logs (A:647-649). |
| 35 | `account_id`, `storage_id`, `log_hash`, `log_id` | deviation as documented | A:330-411 | Tag, kind byte and fixed-width fields: no two statements share a preimage. |
| 36 | block-bound identifiers | deviation as documented | A:363-383, A:466-489 | Kinds `0x11`/`0x12` and the block number. |
| 37 | `Predicate::holds`, `Fact::field`, `Fact::check`, `attest_batch` | deviation as documented | A:509-587, A:726-761 | Big-endian words compare as unsigned 256-bit integers; empty batch refused (A:732-734). Conditions on slots and logs are pinned (R04, R05 killed), as is the emitter's padding (R11 killed). |
| 38 | `Relation`, `Operand` in `attest_batch` | **DOC MISMATCH** | A:754-759, A:555-576 | A relation naming a topic or data word that the log does not have fails with `PredicateFailed`, not `QueryInvalid` (only a missing fact or a field of another kind of fact is `QueryInvalid`). §2.2. |
| 39 | `history_hop` | deviation as documented | A:770-806, A:60-66; EIP-2935 | Window `R − 8191 ≤ n < R` is exactly what the post-state of block `R` holds; slot `n mod 8191`; address and window equal the EIP; `HISTORY_CODE_HASH` equals Keccak-256 of the runtime code in the EIP's deployment transaction (computed: `6e49e667…0a5d`). |
| 40 | `locate_log`, `block_log_index` | OK | X:318-338 | Checked arithmetic; inverse of each other; helpers only. |
| 41 | `accumulator_node`, `accumulator_root_of` | deviation as documented | AC:121-147; bridge-spec-v2 §3.2 (docs/design/spec/bridge-spec-v2.md:29-32) | `SHA-256(0x01‖l‖r)`, 32 siblings, index bits leaf-up, index ≥ 2^32 refused (AC:135). Fixed depth also rules out presenting an inner node as a leaf. |
| 42 | `accumulator_leaf_id` | deviation as documented | AC:151-162; bridge-spec-v2 §3.3 (line 37) | Matches; the spec text does not state the width of `evmChainId` — the code (and the shared vectors) make it `uint256`. |
| 43 | `attest_accumulator_root`, `attest_accumulator_leaf`, `AccumulatorRoot::restore` | deviation as documented | AC:52-61, AC:90-107, AC:168-188 | Root is a storage attestation; empty leaf refused (AC:175); `restore` is trusted input. |

## 2. Problems and gaps, most severe first

No PROBLEM: nothing below makes a function accept what its spec refuses, or refuse what it
accepts, on any input that reaches it on the shipped path.

### 2.1 DOC MISMATCH, row 18: `final_half` is not canonical; the check lives in `steps.rs`

- **What.** The row says "canonical big-endian Fp12 encoding". `miller_half` encodes canonically
  (`blst_bendian_from_fp`, B:210-217), but `final_half` decodes with `blst_fp_from_bendian`
  (B:219-227), which reduces any 48-byte value modulo p. A product with a coefficient replaced by
  `c + p` is a different byte string for the same element, and `final_half` accepts it. The only
  canonicity check is `canonical_fp12` in `steps::Job::step3` (steps.rs:413, 765-777).
- **Test.** `review_core::final_half_reads_coefficients_modulo_p` (passes: for each of the 12
  coefficients, `final_half(product with c+p) == Ok(())`). Asserting the row's claim
  (`final_half(non-canonical) != Ok(())`) fails.
- **Impact.** None on the program: `step3` checks first, and the product is the job's own state.
  A future caller of `Blst::final_half` that stores products elsewhere would get two encodings per
  product (no forged verdict: the element is the same).
- **Fix suggested.** Either say in the row "canonical encoding written by `miller_half`;
  `final_half` reads modulo p; `Job::step3` refuses non-canonical bytes" (wording committed on the
  review branch as `428ab75`, "suggested fix, not merged", docs only), or move the check into
  `final_half` (changes the core, so a new image id).

### 2.2 DOC MISMATCH, row 38: a relation on a missing log topic is `PredicateFailed`

- **What.** The row (and `attest_batch`'s doc comment, A:723-725) says "a relation naming a fact or
  field that is not there is `QueryInvalid`". `Fact::field` returns `PredicateFailed` for a topic or
  data word the log does not have (A:564-571, and its own doc comment A:553-554 says so).
- **Test.** `review_core::a_relation_on_a_missing_log_topic_or_data_word_is_predicate_failed`
  (passes: `Topic(3)` and `DataWord(1)` of a 3-topic, 32-byte log give `PredicateFailed`; `Value` of
  a log and fact index 1 of a one-fact batch give `QueryInvalid`).
- **Impact.** Error code only; the batch fails either way and no statement is attested. A consumer
  that treats `QueryInvalid` as "my query is malformed" and `PredicateFailed` as "the chain says
  no" would read a malformed field reference as a false statement.
- **Fix suggested.** Correct the row and A:723-725 ("…a fact that is not there, or a field of
  another kind of fact, is `QueryInvalid`; a topic or data word the log lacks is
  `PredicateFailed`"). The row's wording is in `428ab75` (docs only); A:723-725 is a comment in the
  core, left for the next core release.

### 2.3 Test gaps: six plausible bugs survived the suite (all closed on the review branch)

24 extra mutants (`scripts/review-mutants.py`, run on Modal with the whole workspace; log
`spike/tmp/review-core/mutants-review.log` and `mutants-steps.log`):

| Id | File | Change | First run | With the new tests |
|---|---|---|---|---|
| R01 | store.rs | rotation does not report `adopted` (policy skips the check) | killed | |
| R02 | upgrade.rs | Gloas upgrade proves a Capella-era `block_hash` in the Deneb layout | killed | |
| R03 | validate.rs | `get_lc_execution_root` ignores the store layout | **survived** | killed (`execution_root_follows_the_store_layout_after_deneb`) |
| R04 | attest.rs | a slot's conditions skipped in a batch | killed | |
| R05 | attest.rs | a log's conditions skipped in a batch | killed | |
| R06 | store.rs | `best_valid_update` kept after a 2/3 apply | killed | |
| R07 | policy.rs | `from_core` does not check a known next committee | **survived** | killed (`from_core_checks_a_known_next_committee`) |
| R08 | validate.rs | Capella-era Gloas header checked at the Deneb gindex | killed | |
| R09 | validate.rs | Capella-era Gloas execution root at the Deneb payload gindex | killed | |
| R10 | merkle.rs | `normalize_merkle_branch` pads at the root end | killed | |
| R11 | attest.rs | emitter left-aligned | killed | |
| R12 | validate.rs | finalized header's layout not in the shape checks | **survived** | killed (`an_empty_finalized_header_of_another_layout_is_refused`) |
| R13 | exec.rs | account with a zero storage root accepted | killed | |
| S01 | steps.rs | no period-skip check | killed | |
| S02 | steps.rs | no relevance check | **survived** | killed (`the_compact_validation_agrees_with_row_1_on_the_guard_cases`) |
| S03 | steps.rs | no `NextCommitteeMismatch` check | **survived** | killed (`the_compact_validation_agrees_with_row_1_on_the_guard_cases`) |
| S04 | steps.rs | fork version at `signature_slot` (no −1) | killed | |
| S05 | steps.rs | finality gindex by the finalized slot | killed | |
| S06 | steps.rs | signature always against the current committee | killed | |
| S07 | steps.rs | `update_advances` without the finalized-next-committee clause | killed | |
| S08 | steps.rs | committee taken at rotation not adopted | killed | |
| S09 | steps.rs | attested ≥ finalized becomes > | **survived** | killed (`the_compact_validation_agrees_with_row_1_on_the_guard_cases`) |
| S10 | steps.rs | store may move to a layout its schedule has not reached | killed | |
| S11 | steps.rs | `current_slot >= signature_slot` becomes `>` | killed | |

First run: 18 killed, 6 survived (`mutants-review.log`, `mutants-steps.log`; 503 s and 447 s on
the box). Re-check of the six survivors with the new tests: 6 killed, 0 survived
(`mutants-recheck.log`, `mutants-recheck-steps.log`).

- **R03** (row 3) dropped `fork >= LcFork::Deneb` (V:112): a Capella-layout store would hash a
  post-Deneb header as Deneb. Not a safety issue (the header is still proven against its body), but
  it is the store-layout dispatch the row is about, and nothing pinned it. New test
  `review_core::execution_root_follows_the_store_layout_after_deneb`.
- **R07** (row 11's store) removed `from_core`'s check of a known next committee (P:86-88), which
  its doc comment promises ("Both committees are checked, as at adoption"). `from_core` is not on
  the on-chain path (the program restores a `CompactStore`), so this is a gap in a guarantee for
  off-chain callers. New test `review_core::from_core_checks_a_known_next_committee`.
- **R12** (row 1) dropped the finalized header's layout from the shape checks (V:264): an update
  without finality carrying the all-zero header of another layout passed. Decoded updates always
  have the store's layout, so only in-memory callers are exposed. New test
  `review_core::an_empty_finalized_header_of_another_layout_is_refused`.

- **S02, S03, S09** (`steps.rs`, the on-chain transcription of row 1): the same changes as M17,
  M03 and M04, which row 1's guards kill, survived in `validate_compact` because those guards run
  on row 1 only. Impact if such a bug shipped: S02 none for safety (on the `step1` path
  `update_advances` and `apply_core_compact`'s precondition still refuse every stale update, with
  `NotNewer` or `CommitteeUnknown` instead of `UpdateIrrelevant`); S03 an update whose next
  committee conflicts with the known one would be checked against its branch only — reachable
  only with a supermajority-signed contradictory state, and the conflicting committee would then be
  ignored (no rotation in that period), but the program would accept an update row 1 refuses;
  S09 liveness only (an attested slot equal to the finalized slot does not occur on a real chain).
  New test `review_core::the_compact_validation_agrees_with_row_1_on_the_guard_cases` runs both
  transcriptions on the three guard inputs and requires the same verdict and code.

### 2.4 Scope gap: the on-chain path is `steps.rs`/`witness.rs`, which have no row

The LEZ program does not call rows 1, 8 and 11. It runs `steps::step1`/`Job::step2`/`step3`/`apply`
over the compact store of `witness.rs` (1,811 lines, 29% of the crate), which contain a **second
transcription** of the spec: `validate_compact` (steps.rs:131-251, row 1 without the pairing),
`update_advances` (steps.rs:294-301), `apply_core_compact` (steps.rs:304-335),
`store_for_update` (steps.rs:265-279), the non-signer witness check (steps.rs:630-664) and the
committee adoption from uncompressed keys (`witness.rs`). M01–M107 plant nothing in these files.
What I checked there: `validate_compact` is line-for-line row 1 over summaries; the non-signer
keys are deserialized with an on-curve check (`blst_p1_deserialize`, witness.rs:792-803) and each
is bound to its committee position by a branch at leaf index `i` (steps.rs:645-649), so the
signing key `aggregate − Σ non-signers` cannot be steered; `step3` refuses non-canonical products.
The S01–S11 mutants above measure how well the equivalence tests (`steps.rs` test file: every
sync vector, mainnet replay, mutated mainnet updates) pin this transcription: 8 of 11 killed; the
three survivors are the guard cases no vector or mainnet update exercises (closed by the new
test).
**Suggested:** add rows for `validate_compact`, `update_advances`, `apply_core_compact`,
`store_for_update`, `step1`, `Job::step2/step3`, `apply`, `signing_key` and the adoption witness, each
naming the row it mirrors and the equivalence test; add the S-mutants (or equivalents) to
`scripts/mutants.py`. Also note there that the program passes `signature_slot` as `current_slot`
(steps.rs:547-549), so "not from the future" rests on LEZ's validity window, not on row 1's check.

### 2.5 Smaller notes (no verdict change)

- Row 1's "M03–M17" includes M05–M07, which plant in `types.rs` and `bls/` (they pin row 14/15).
- Row 15: zkcrypto runs one two-pair Miller loop; "two Miller loops" is blst's.
- Row 17: the stronger reason (committee formation runs `KeyValidate`) could replace "relies on
  deposit-time `KeyValidate`".
- Row 42: `bridge-spec-v2.md` §3.3 should state `evmChainId` as 32 bytes, as the code and vectors do.
- `bls/zkcrypto_backend.rs:34-35`: the doc comment of `key_validate` sits on `on_curve_not_identity`.
- `store.rs:222`: `finalized slot + update_timeout()` can overflow for a slot within 8192 of
  `u64::MAX` (spec store only, never on chain).

## 3. Sign-off checklist (one block per row; the "Reviewed" column is the owner's)

Commands run from the worktree root on the build box:
`M=…/spike/modal-build; $M/mrun --profile deeferentleeg -- '<command>'` (vectors fetched by
`scripts/fetch-vectors.sh`). `mut X Y` means `python3 scripts/mutants.py X Y` (each must print
`killed`); `rmut X` means `python3 scripts/review-mutants.py own X`.

`T <args>` = `$M/mrun --profile deeferentleeg -- 'cargo test -p ferrata-spectests --locked <args>'`.

**1 `validate_light_client_update`**
- V:251-376 beside alt-sp:406-489: tick participation V:276, slot order V:286-291, period V:292-302, relevance V:305-310.
- Finality V:315-339 (genesis case V:320-324) and next committee V:343-362 against alt-sp:437-468.
- Signature committee by period V:365-369 (alt-sp:470-488); the shape checks V:263-272 refuse only what the spec's fixed-size types cannot hold.
- `T --test vectors --test adversarial --test review_core`; `mut M03 M04 M08 M13 M14 M15 M16 M17 M37`; `rmut R12`.
- The program's copy: steps.rs:131-251 line by line against V:251-376; `rmut S01 S02 S03 S04 S05 S06 S09 S11`.

**2 `is_valid_light_client_header`**
- V:151-166 beside den-sp:64-86 (blob-gas guard, pre-Capella empty, gindex 25 branch).
- V:167-178 beside glo-sp:211-243 (2856 / 812 / 412 by era; pre-Capella zero hash and branch).
- V:145 `has_layout` (T:403-410) is the only addition.
- `mut M10 M11 M12`.

**3 `get_lc_execution_root`**
- V:107-140 beside cap-sp:137-146, den-sp:30-61, glo-sp:177-209 (Electra has none; it inherits Deneb's).
- V:99-104: `inner` = branch minus the last 4; tail of depth 4 (Capella, gindex 28) or 5 (Deneb, gindex 44): glo-sp:196-205.
- `T --test review_core execution_root_follows`; `mut M20`; `rmut R03 R09`.

**4 `is_better_update`**
- V:193-239 beside alt-sp:251-310: same order, `>=` for the supermajority, three tiebreakers.
- `T --test vectors update_ranking`; `mut M18 M19 M21`.

**5 gindex helpers**
- M:15-46 constants against the tables alt-sp:103-105, ele-sp:88-90, glo-sp:99-113.
- V:68-96: Gloas gindex only for a Gloas store at a Gloas slot; Electra's for Electra/Gloas stores from Electra; argument is the attested slot (V:334, V:357).
- `cargo test -p ferrata-core` (unit tests); `mut M25 M31`.

**6 `sync_committee_signing_root`**
- V:243-248 beside alt-sp:482-488: `saturating_sub(signature_slot, 1)`, then the epoch.
- V:47-65 beside phase0:1366-1405 (ForkData, `compute_domain`, SigningData); `DOMAIN_SYNC_COMMITTEE` V:23.
- fork.rs:150-157 (`fork_at_epoch`, `compute_fork_version`).
- `T --test adversarial another_chain_or_fork_version`; `mut M09`.

**7 `StoreCore::from_bootstrap`**
- S:55, S:58, S:61 against alt-sp:363-384 (header valid, root equals trusted root, current-committee branch at the bootstrap slot's gindex).
- S:47-54 shape checks; S:73 empty next committee.
- `T --test mutation_guards bootstrap`; `mut M38`.

**8 `apply_core`**
- S:109-130 beside alt-sp:494-510; the error at S:114-116 precedes every write.
- S:167-177: the moved bookkeeping (previous/current max on rotation, optimistic header on advance).
- `adopted` on both branches (S:118, S:123); `mut M29`; `rmut R01`.

**9 `LightClientStore::process_update`**
- S:180-218 beside alt-sp:537-580: best update, max participants, optimistic header (threshold after the max), then the 2/3 apply.
- S:188, S:216: clone and commit (the atomicity the row claims).
- `T --test vectors`; `mut M23`; `rmut R06`.

**10 `LightClientStore::process_force_update`**
- S:221-236 beside alt-sp:516-531; `<=` at S:230.
- `mut M27`.

**11 `FinalizedStore::process_update`**
- P:104-114: non-finality, below 2/3 of the preset size, row 1 with `CheckedCommittee`, `NotNewer`.
- P:115-121: apply on a clone, check an adopted committee, then commit.
- P:83-90 `from_core` (both committees checked).
- `T --test mutation_guards --test adversarial --test review_core`; `mut M01 M02 M30 M39 M40 M42`; `rmut R07`.
- The program's copy: steps.rs:550-623 (`step1`, `apply`) and 294-335; `rmut S07 S08 S10`.

**12 `upgrade_header`, `upgrade_bootstrap`, `upgrade_update`**
- U:54-71 beside cap-fork:26-31, den-fork:26-51, ele-fork:41-46.
- U:18-36 beside glo-fork:26-81 (empty → empty; Deneb proof gindex 44, Capella 28; pre-Capella zero).
- U:81-96 beside ele-fork:50-79, glo-fork:84-120 (state branches normalized).
- `T --test sepolia_gloas --test vectors`; `mut M26`; `rmut R02`.

**13 `is_valid_normalized_merkle_branch`, `normalize_merkle_branch`**
- M:107-117 beside alt-sp:334-347; M:89-103 beside phase0:1195-1208; M:120-125 beside ele-fork:26-34.
- `cargo test -p ferrata-core`; `mut M36`; `rmut R10`.

**14 `split_container`, container decoders**
- Z:39-80: fixed part, offsets, exact consumption; Z:83-88 branch length.
- T:170-218 (payload header 15/17 fields, extra_data ≤ 32), T:336-349 (Bitvector padding), T:413-517 (layouts).
- T:221-260, T:315-319 hash-tree roots against the containers.
- `T --test mutation_guards ssz`; `mut M05 M06`.

**15 `fast_aggregate_verify`**
- B:235-250 and B:145-203 beside IETF `FastAggregateVerify`/`CoreVerify`: KeyValidate per signer (B:102-109), identity aggregate, signature subgroup (B:152), DST (mod.rs:41).
- K:93-107, K:74-90: same steps with zkcrypto.
- `T --test bls_vectors --test signature_paths`; `mut M07 M24 M33`.

**16 `check_committee`**
- B:127-142, K:45-56 beside altair/bls.md:36-50.
- `T --test bls_vectors consensus_specs_eth_aggregate_pubkeys`; `mut M35`.

**17 `check_committee_onchain`**
- B:92-99, B:256-258: on-curve, not identity, sum; no subgroup check.
- Why it is safe: altair/beacon-chain.md:356-363 and altair/bls.md:45-47.
- `T --test mutation_guards onchain_adoption`; `mut M41`.

**18 `miller_half`, `final_half`**
- B:210-227: encode canonical, decode modulo p; steps.rs:408-419 and 765-777: the canonical check.
- Decide: correct the row, or move the check into `final_half` (§2.1).
- `T --test review_core final_half --test signature_paths`; `mut M43`.

**19 `verify_checked`**
- B:54-76 (subtractive and additive), B:72 identity; mod.rs:94-103 (defined for checked committees only).
- `T --test vectors sync_vectors_blst`; `mut M34`.

**20 `keccak256`**
- keccak.rs:12-18 (`Keccak::v256`, not SHA3).
- `T --test exec_objects keccak`.

**21 `split_first`, `decode`**
- R:56-66 (prefix classes, bounds, wrapped byte at R:62), R:70-83 (long length), R:86-92.
- `T --test rlp_decoding`; `mut M44 M45 M46 M47 M48`.

**22 `Item` accessors**
- R:109-118 (no leading zero, ≤ max), R:142-152 (exact arity).
- `T --test rlp_decoding integers typed`; `mut M49 M50 M51`.

**23 `encode_u64`**
- R:184-199.
- `T --test rlp_decoding encode_u64`; `mut M52`.

**24 MPT `verify`**
- MP:174-227: hash check, small-node rule, walk, `used == proof.len()`.
- MP:82-164: hex-prefix flags, child references, node arity, empty leaf, empty extension path.
- `T --test mpt_proofs --test exec_adversarial`; `mut M53 M54 M55 M56 M57 M58 M59 M60 M61 M62 M63 M64 M65 M66`.

**25 `decode_header`**
- X:93-135 against `spec/es_osaka_blocks.py` and `es_amsterdam_blocks_1689800.py` `class Header` (field order).
- X:101-106 shapes; X:107-110 bloom and extra_data.
- `T --test exec_objects header --test sepolia_gloas`; `mut M67 M68 M69`.

**26 `Account::decode`**
- X:168-180.
- `T --test exec_objects accounts`; `mut M70`; `rmut R13`.

**27 `decode_storage_value`**
- X:185-191.
- `mut M71`.

**28 `prove_account`, `prove_storage`**
- X:194-213 (keys, `ABSENT`, `EMPTY_ROOT` for an absent account).
- `T --test mainnet_e2e accounts`; `mut M72`.

**29 Receipts**
- X:261-290 (type byte, pre-Byzantium, status 0/1, logs), X:298-306, X:341-349 (key `RLP(index)`).
- `T --test exec_objects receipts --test mainnet_e2e every_log`; `mut M73 M74 M75 M76`.

**30 `from_payload_header`**
- A:102-121 (zero `block_hash` refused at A:107).
- `T --test exec_objects records`; `mut M87`.

**31 `from_block_hash`**
- A:125-145; glo-sp:113 (gindex 2856 = bid's `parent_block_hash`).
- `T --test sepolia_gloas the_gloas_record`; `mut M69`.

**32 `to_ssz`, `from_ssz`**
- A:256-310 (165 bytes; A:293-298 binding/hops).
- `T --test exec_objects record_encoding`; `mut M85 M86`.

**33 `anchor_slot`**
- A:221-223, A:795-797.
- `mut M107`.

**34 `attest_account`, `attest_storage`, `attest_log`**
- A:611-653: `for_chain` first; status at A:647.
- `mut M77 M78`.

**35 identifiers**
- A:330-411 (tag, kind, fixed widths); compare with the Python values in `exec_objects::identifiers_are_pinned…`.
- `mut M92 M93`.

**36 block-bound identifiers**
- A:363-383, A:466-489.
- `T --test exec_objects block_bound`; `mut M105 M106`.

**37 `Predicate`, `Fact`, `attest_batch`**
- A:509-522 (unsigned compare), A:555-586, A:726-753.
- `T --test exec_objects predicates fact_fields a_batch`; `mut M88 M89 M90 M91`; `rmut R04 R05 R11`.

**38 `Relation`, `Operand`**
- A:754-759 with A:555-576: which errors a missing operand gives (§2.2).
- Decide the row's wording.
- `T --test review_core a_relation --test exec_objects relations`; `mut M104`.

**39 `history_hop`**
- A:776-805: header decoded first, window, code hash, slot `n mod 8191`, hash comparison, hops.
- A:60-66 against `spec/eip-2935.md` lines 29-32 and the deployment input (line 142: runtime = input after its first 9 bytes; `cast keccak <runtime>` = `HISTORY_CODE_HASH`).
- `T --test exec_adversarial history --test mainnet_e2e history`; `mut M79 M80 M81 M82 M83 M84`.

**40 `locate_log`, `block_log_index`**
- X:318-338.
- `T --test mainnet_e2e block_wide`; `mut M102 M103`.

**41 `accumulator_node`, `accumulator_root_of`**
- AC:121-147 against bridge-spec-v2.md:29-32.
- `T --test accumulator`; `mut M94 M95 M96 M97`.

**42 `accumulator_leaf_id`**
- AC:151-162 against bridge-spec-v2.md:37 (chain id width: 32 bytes in code).
- `mut M100 M101`.

**43 `attest_accumulator_root`, `attest_accumulator_leaf`, `restore`**
- AC:90-107, AC:168-188 (empty leaf AC:175, root AC:178).
- `T --test accumulator a_leaf_under_two`; `mut M98 M99`.

## 4. Branch

`review-core` (from `integration6` `c52eee4`), tip `428ab75`. Commits: `5f520e5` (R mutants), `fd70d05` (tests for §2.1, §2.2), `be3f6cf` (S mutants),
`020112d`, `3d713da`, `12d3a68`, `6d6db6f` (tests that kill R03, R12, R07, S02/S03/S09), `428ab75`
("suggested fix, not merged": `docs/REVIEW-core.md` rows 11, 17, 18, 38 and the `steps.rs` scope
note; docs only). `git diff c52eee4 -- crates/ferrata-core` is empty, so no image id changes.
Not pushed. Run the review tests: `$M/mrun --profile deeferentleeg -- 'cargo test -p
ferrata-spectests --locked --test review_core'` (6 pass).
