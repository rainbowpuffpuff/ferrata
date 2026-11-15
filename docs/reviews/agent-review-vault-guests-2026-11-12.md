# Independent review of the vault and the guests against the specifications (docs/REVIEW-vault.md, 52 rows; docs/REVIEW-guests.md, 55 rows)

Reviewer: a headless agent (Claude Code, Opus 5.5), 12 November 2026, 19:50–20:50 CET. Code: `89e6d6d` (every
`file:line` below is at that commit). Review branch: `review-vault` in `spike/ferrata-wt-review-vault` (the two
tables, three vault tests, three checkpoint-core tests, two mutant runners, a docs-only suggested fix; tip in §5).
The branch changes no source file of `vault/src`, `bridge/guests`, `bridge/crates`, `checkpoint/crates`,
`checkpoint/guests`, `lez/guests` or the wire crates (it adds one test file under
`checkpoint/crates/checkpoint-core/tests/`), so no image id and no vault bytecode changes. Modal: profile
Modal, four `mrun` calls (357 s, 1,918 CPU-seconds), 0.055 USD by the ledger's estimates (Modal's own bill for the workspace still
read 8.82 USD at the end, unchanged: it lags).

Specifications read: `docs/design/spec/bridge-spec-v2.md` (`v2 §n`), `bridge-spec-v3-changes.md` (`Dn`),
`docs/design/bridge.md` §§10, 12.3, 13, 14, 16, 19, `docs/design/attestation.md` §§3–7, `lez/BRIDGE.md` §§2, 4, 5,
6, `checkpoint/README.md` ("What the program checks", "Choices"), `vault/README.md` ("Where I interpreted or
departed from the spec", "Known limits", "Storage-slot contract"), and the security review of 11 November with its
fixes (`docs/reviews/security/`), whose ids (C-1, H-1, M-1 … I-10, LD-2) are used here.

Abbreviations: `FV` = `vault/src/FerrataVault.sol`, `FR` = `vault/src/FerrataRegistry.sol`; `BC/` =
`bridge/crates/ferrata-bridge-core/src/`, `CC/` = `checkpoint/crates/checkpoint-core/src/`, `AT` =
`checkpoint/crates/attestor-core/src/lib.rs`; `P`, `B`, `C`, `H` = `lez/guests/{program,bridge,consumer,holdings}/src/main.rs`;
`BW` = `lez/bridge-wire/src/lib.rs`. `Vn` = mutant `n` of `vault/tools/mutation_spotcheck.py`; `VRnn` and `Gnn`
are this review's (§3).

What ran (Modal build box of the worktree, through `mrun`; measured):
- `cd vault && forge test` with the review's test file: **454 passed, 0 failed, 1 skipped in 44 suites** (26.8 s;
  without the review's file: 451 passed, 1 skipped, 43 suites, of which 61 tests with real proofs in 15 suites under
  `test/real/`). Log `spike/tmp/review-vault/mut/full-suite.log`.
- `vault/test/ReviewVault.t.sol`: 3 new tests, all pass on `89e6d6d`'s vault (they pin what the code does where
  nothing pinned it or where a comment says otherwise).
- `checkpoint/crates/checkpoint-core/tests/review_guests.rs`: 3 new tests, all pass on `89e6d6d`'s checkpoint core
  (`cargo test -p checkpoint-core --locked --test review_guests`).
- 25 vault mutants VR01–VR25 (`vault/tools/review_mutants.py`, `mutation_spotcheck.py`'s runner: optimizer off,
  every suite but the invariant campaign, then the campaign for a survivor, here at 64 runs instead of 256): first
  against the suite as it was, then the survivor again with the new tests (§3.1).
- 18 guest mutants G01–G18 (`scripts/review-guest-mutants.py`) against the bridge core's and the checkpoint core's
  own host tests (§3.2). Rule (A)'s tests (`checkpoint/fidelity`, built against the LEZ and Logos Blockchain sources)
  were not run: a checkpoint survivor means "not pinned by checkpoint-core's tests" only.

## Summary

| Verdict | Vault rows | Guest rows |
|---|---|---|
| OK | 17 | 12 |
| deviation as documented | 34 | 42 |
| DOC MISMATCH | 1 | 1 |
| PROBLEM | 0 | 0 |

No row implements its specification wrongly: I found no input on which a function accepts what its specification
(or, for Ferrata's own rows, the stated design) refuses, or refuses what it accepts. Every deviation a row states
is in the code, and every deviation in the code that changes what is accepted is stated, in the row or in the
README's departures. The findings, most severe first (§2):

1. **A cross-chain interface with no test on the vault side** (gap, closed on the branch). The LEZ bridge program's
   keyless `Sunset` reads the vault's storage slot 37, byte 11 (`retired`, packed beside `freezeAuthority`); the
   vault declares only slots 0–2 as its interface, and a mutant that moves `retired` survived all 451 tests and the
   invariant campaign (at 64 runs). v5 is correct (the new tests show it); a later vault could break the keyless `Sunset` silently.
2. **A safety check of the checkpoint guests with no test** (gap, closed on the branch): the refusal of a key set
   that repeats a key (one key would fill several signature indices of a rotation) survived every test of the
   checkpoint core and is in no rule (A) test either. New tests pin it, and show the one case it does not cover: a
   genesis parent that repeats a key, which only the deployment check can refuse.
3. **The bound on `revokeFrom`'s loop rests on the checkpoint programs, not on the vault** (DOC MISMATCH): two
   finalisations can share a block in the vault's own logic; the pinned programs never make the no-op checkpoint
   that allows it.
4. Four statements the code contradicts, none with a safety consequence: README's epoch formula, `bridge.md` §16 on
   the refund second, the rule (A) guest's "416-byte journal", EVIDENCE row 12's test counts (§2.4–2.7).

## 1. Rows

### 1.1 The vault (docs/REVIEW-vault.md)

| # | Function | Verdict | Evidence (code; spec) | Finding |
|---|---|---|---|---|
| 1 | `deposit` | deviation as documented | FV:676-690; v2 §3.4, D13 | Pull and exact balance delta (FV:685-687) before `_recordDeposit`; the only earlier write is a due fee change in `_checkDeposit` (FV:1206), which the comment FV:680-683 names. L-3 at FV:1195; I-3 (no maximum) still open, as the row says. |
| 2 | `depositETH` | OK | FV:694-698 | Exact `msg.value` (FV:696). |
| 3 | `_checkDeposit` | deviation as documented | FV:1190-1208; v2 §3.4, D9, D13 | v2's refusals in order, the tree bound below 2^32 (FV:1204), total = denomination + mint + relay fee of the schedule in force (FV:1206-1207). |
| 4 | `_recordDeposit` | deviation as documented | FV:1211-1219 | Caps and escrow on the total (FV:1212, 1214), counter (FV:1213); the leaf binds the denomination only (FV:1217), which is what the LEZ claim mints. |
| 5 | `_leafHash` | OK | FV:1176-1182; v2 §3.2 | 1 + 32 + 8 + 20 + 20 + 32 + 32 = 145 bytes, field order as specified; the bridge core's vector test compares the vault's tree bit for bit. |
| 6 | `_insertLeaf`, `_hashNode`, empty root | OK | FV:1227-1250, 636-640; v2 §3.2 | `node = H(0x01 ‖ l ‖ r)`, empty leaf zero, depth 32; one branch write per append, none when the tree fills. |
| 7 | `receive`, `fallback` | OK | FV:660-666 | Both revert. |
| 8 | `recordBeacon`, `_recordBeacon` | deviation as documented | FV:741-744, 1256-1265; v2 §4.3 | Nonce expression verbatim; one per block (a second call returns the existing index); rule (A) only. |
| 9 | `submitCandidate` | deviation as documented | FV:750-776; v2 §4.3, D4, D13 | Freeze, parent, echo 0, floors, then the pending check on the current epoch only (FV:758-759: a stale candidate is replaced, L-1), proof, the stored digest and fields, the keyless clock (M-1), a beacon under (A). |
| 10 | `_checkCommittee` | deviation as documented | FV:1314-1317; D4 | At submission; the finalisation re-presents the same journal (digest), so D4's "and at finalisation" holds by equality. |
| 11 | `_checkpointDigest` | OK | FV:1271-1277; D3 item 5 | Three `abi.encode` parts of static types (6 + 7 + 4 words) concatenate to the one 17-word encoding; `CHECKPOINT_DOMAIN` = `sha256("ferrata/vault/checkpoint/v3")` (FV:244, CC/hash.rs). |
| 12 | `finalize` | deviation as documented | FV:784-794; v2 §4.3, D4, D11 | Mode, live candidate with echo 0 (freeze, epoch, digest), beacon index, `timestamp ≥ submittedAt + W` (FV:789), echo equal to the beacon's (FV:790), monotonicity, the second proof over the echo (FV:792). |
| 13 | `finalizeAttested` | deviation as documented | FV:799-807; D7, D13 | `_liveCandidate(c, c.echoNonce)` forces `echoNonce = 0` through the digest; wait (FV:802); stop (FV:805). |
| 14 | `_liveCandidate`, `_epoch` | deviation as documented | FV:1330-1342 | Epoch = `revocationEpoch + _stateChanges`; README:434 still says `checkpointCount()` (§2.5). |
| 15 | `_wait` | deviation as documented | FV:1322-1324; D4 | `t ≥ tMin ≥ 1` at submission, so `t − 1` cannot underflow; `turnSeconds = 0` under (B). |
| 16 | `_checkMonotone` | OK | FV:1346-1350; D4, D11 | As specified. |
| 17 | `_finalize` | deviation as documented | FV:811-846; v2 §4.3, D5 | Parent count and flag recorded (FV:822-823), `_stateChanges` only on a state change (FV:832), candidate deleted (FV:833), a root keeps its first non-revoked checkpoint (FV:835-836), the first sunset sets the boundary (FV:838-844). |
| 18 | `_emitCandidateSubmitted`, `_emitCheckpointFinalized` | OK | FV:1280-1310; D4 | Tip, `n`, `t` in both; VR19 (tip hash replaced) killed. |
| 19 | `cancelCandidate` | deviation as documented | FV:1027-1030; FR:262-271; D4, D13 | Watchers or the freeze authority; no proof, so it works under a freeze and after a stop. |
| 20 | `pauseCheckpoints` | deviation as documented | FV:1034-1039; D13 | 12 h from now, renewable, never shortens a freeze; README "Known limits" states the renewals. |
| 21 | `setCheckpointsFrozen` | deviation as documented | FV:973-978; D4, M-1 | Lifting keeps the end time (FV:975). |
| 22 | `revokeFrom` | **DOC MISMATCH** | FV:1045-1060; FR:341-353; v2 §4.4, D4, D5 | The function is as specified: window inclusive (FR:347 is v2's `finalizedAt ≥ now − revokeWindow`), revoked index refused (FR:346), every later checkpoint marked, state, count and flag restored, epoch advanced, boundary cleared if set at or after `index` (FV:1053). Its bound (FR:339-340, FV:232-234, README:431) is not the vault's: §2.3. |
| 23 | `raiseDelta` | OK | FV:948-953; v2 §4.3, D4 | Raise only, ceiling 30 days. |
| 24 | `release` | deviation as documented | FV:859-873; v2 §5.3–5.4, D11, D13 | Checks, proof, nullifier, caps, escrow, then payments, under `nonReentrant`; each fee ≤ 1%, so `amount − relayFee − fees` cannot underflow; fees credited (FV:870). |
| 25 | `_checkRelease` | deviation as documented | FV:1357-1378 | Freezes, registration, denomination, recipient, root accepted, the delay of the root's checkpoint (+1 s when it equals the window, FV:1372), nullifier, fee 0 or registered (FV:1376), escrow. |
| 26 | `_releaseJournalDigest` | OK | FV:1382-1398; v2 §5.2 | Eleven words in the specified order. |
| 27 | `_creditReleaseFees` | deviation as documented | FV:1430-1437; D13 | Schedule in force at the release (L-2); prover from the root's checkpoint (M-2). |
| 28 | `_releaseDelayFor`, `releaseDelayFor` | deviation as documented | FV:1403-1413; FR:82, 256 | Last setting with `fromCheckpoint ≤ index`; registration writes `fromCheckpoint = 0`, so every checkpoint of a registered token finds one. |
| 29 | `_payOrCredit`, `_credit`, `_trySendEth` | deviation as documented | FV:1417-1456; v2 §5.4 | `trySafeTransfer`, ETH without return data; VR13 (2300 gas) killed. |
| 30 | `claimOwed` | OK | FV:877-879; FR:276-295 | Cleared before paying; a failed ETH claim reverts, so the credit stays. |
| 31 | `refund` | deviation as documented | FV:707-733; v2 §3.4, D5, D11, D13 | D5's conditions and the stop (FV:715-719), escrow check, release buckets (FV:728), marked before paying. |
| 32 | `refundAmount`, `_inboundFeesOfLeaf` | deviation as documented | FR:149-194; v2 §3.2–3.4, D9, D13 | Known leaf (FR:160), the leaf's preimage (FR:165), direction bits = index (FR:166-168) to slot 0 (FR:169); fees of the last schedule with `fromLeaf ≤ leafIndex`, the one the deposit paid (a deposit puts a due change in force before it takes its index, so no later schedule can start at or below it). |
| 33 | `_refundsOpenAt` | deviation as documented | FV:1018-1020; D5, D11 | `+1`: D11's boundary second. `bridge.md` §16's last sentence says otherwise (§2.4). |
| 34 | `retire` | deviation as documented | FV:995-1000; v2 §3.4, D11, D13 | One-way. It sets slot 37's byte that LEZ reads (§2.1). |
| 35 | `retireOpensAt` | deviation as documented | FV:1007-1015; D11, D13, M-1, L-3 | As the row and README state. |
| 36 | `transferAdmin`, `acceptAdmin` | deviation as documented | FV:1068-1079; v2 §7, D13 | Two steps, roles distinct. |
| 37 | `proposeRoles`, `applyRoles` | deviation as documented | FV:1087-1095; FR:298-334; D13 | The admin is refused as freeze authority at both steps (FR:304, 329). |
| 38 | freezes | OK | FV:961-990, 1506-1512; v2 §7 | Lapse after `maxFreeze`; renewal restarts. |
| 39 | role checks | OK | FV:1514-1524, 564-572 | `_isWatcher` never matches `address(0)` as a sender. |
| 40 | `_stopped` | OK | FV:1527-1529, 602-606; v2 §6, D13 | The constructor requires an untripped stop. |
| 41 | `registerToken` | deviation as documented | FV:892-896; FR:54-94 | All bounds before any write. |
| 42 | registry checks | deviation as documented | FR:419-473 | `2 × max / lezUnit × weight` cannot overflow (FR:471-472). |
| 43 | publishing | OK | FR:477-588; lez/BRIDGE.md §5 | Preimages and slots equal BRIDGE §5 byte for byte (`abi.encodePacked` of `uint256[]` = 32-byte words, no length). |
| 44 | `announceFees`, `_storeFees` | deviation as documented | FV:903-910; FR:98-120, 501-526; D13 | A due change first (FR:109); announcing the fees in force withdraws (FR:113). |
| 45 | `applyFees`, `_feesInForce` | deviation as documented | FV:914-916, 1440-1447; FR:124-143 | `fromLeaf` = leaf count at application. |
| 46 | views | OK | FV:1101-1172 | VR01, VR02, VR21 killed. |
| 47 | `setDepositsEnabled` | OK | FV:919-923 | Releases unaffected. |
| 48 | `setReleaseDelay`, `checkReleaseDelay` | deviation as documented | FV:943-945; FR:246-258, 375-379; v2 §5.3, D5 | `fromCheckpoint = checkpoints.length` (VR17 killed). |
| 49 | cap settings | deviation as documented | FV:927-939; FR:198-242, 385-434; D13 | Floor on release buckets only, saturating conversion. |
| 50 | cap internals | deviation as documented | FV:1462-1500; FR:366-405 | The `_valueUnits` comment (FV:1472-1474) names a stale bound; no overflow (§2.8). |
| 51 | `_limits` | OK | FR:358-361 | A self-call to the vault's immutables' getters. |
| 52 | `constructor` | deviation as documented | FV:578-657; v2 §1, §4.3, §6, §7; D4, D5, D13 | Every bound the row lists, in that order; VR10–VR12 killed. |

### 1.2 The guests (docs/REVIEW-guests.md)

| # | Entry point or check | Verdict | Evidence (code; spec) | Finding |
|---|---|---|---|---|
| 1 | `release` guest `main` | deviation as documented | `bridge/guests/src/bin/release.rs`; v2 §5.2 | Fixed input, panic on an invalid witness, journal committed as encoded. |
| 2 | `verify_release` | deviation as documented | BC/outbound.rs:185-191; v2 §5.1–5.2, D13 | `COut` from the witness secret and the public chain, vault, recipient; the leaf binds configuration, index, token, amount, `COut`, `r`; direction bits = index; `NOut` v2 (G02, G06 below). |
| 3 | burn derivations | deviation as documented | BC/outbound.rs:50-76; BC/encoding.rs | Preimages as v2 §5.1 with D12's MAC v2 and D13's `NOut`. |
| 4 | `verify_path`, `root_from_path`, `ordered` | deviation as documented | BC/tree.rs:56-88; v2 §3.3, D9, D12 | 32 direction bits equal to the index (so < 2^32); mask, not branch. |
| 5 | `ReleaseInput::decode`, `ReleaseJournal::encode` | OK | BC/encoding.rs:553-700 | Journal words as the vault's `_releaseJournalDigest` (G05). |
| 6 | `claim` guest `main` | deviation as documented | `bridge/guests/src/bin/claim.rs` | Measurement only; its 168-byte journal is read by nothing on chain. |
| 7 | `verify_claim` and derivations | OK | BC/inbound.rs:49-98; v2 §3.2–3.3 | `N` binds chain, vault, index and leaf through `fid` (G04). |
| 8 | `gasdrop` guest, `verify_gas_drop`, journal | deviation as documented | BC/gasdrop.rs:27-141; bridge.md §6, D13 | Journal binds `(N, oneTimeAddress, relayer)` and the root, chain, vault (G03). |
| 9 | `ClaimInput::decode`, `claim_journal` | deviation as documented | BC/encoding.rs:746-805 | Unknown kind or non-zero padding panics. |
| 10 | `checkpoint` guest `main` | **DOC MISMATCH** | `checkpoint/guests/checkpoint/src/main.rs:1`; CC/journal.rs:31 | It commits 17 words (544 bytes); its doc comment says "the 416-byte journal" (SPEC-v2's 13 words). §2.6. |
| 11 | `checkpoint_b` guest `main` | OK | `checkpoint/guests/checkpoint-b/src/main.rs` | "544-byte journal", as built. |
| 12 | `segment` guest, `run_segment` | deviation as documented | CC/rule.rs:349-361 | Canonical parent frontier, at least one block, the journal binds the configuration and both states. |
| 13 | input decoders | OK | CC/input.rs:209-341; CC/reader.rs:68-108 | Every length bounded by the bytes left times the smallest element; `finish()` refuses trailing bytes. |
| 14 | `up_to_tip` | deviation as documented | CC/rule.rs:219-268; v2 §4.2 items 1–3, D3, D10 | Segments: same configuration, chained, ending at the input's parent (CC/rule.rs:233-246); floors on every set reached (CC/rule.rs:250-256). |
| 15 | `link_block`, `parse_block`, `hash_parsed_block` | deviation as documented | CC/rule.rs:75-100; CC/lez.rs:128-182 | Id + 1, previous hash, recomputed hash equal to the stored one. |
| 16 | `apply_message`, `extend`, `BridgeMessage::decode` | deviation as documented | CC/rule.rs:102-142; CC/wire.rs:88-116; BW:878-887 | The decoder's variants 0–3 and field layouts are the bridge program's (`Burn`, `ImportRoot` with three `Vec<Vec<u8>>` proofs, `Sunset`, `Echo`, in that order in `BridgeMessage`); only publicly executed calls are visited. |
| 17 | `apply_config_change`, `check_key_set`, `threshold` | deviation as documented | CC/rule.rs:144-217; CC/hash.rs:37-41 | I-10 stands: `t(current n)` distinct current keys, not the live `configuration_threshold`. The repeated-key refusal had no test (§2.2). |
| 18 | `run_checkpoint` | deviation as documented | CC/rule.rs:270-347; D3 item 4 | `Y` from `Echo` in any call, reverted or not; producer checked and in `K`; support strictly after `Y`, distinct keys of `K` other than `D` unless single-key. |
| 19 | `ElidedInscription::check`, `verify_strict` | OK | CC/mantle.rs:149-209 | Framing, channel, payload length, BLAKE2b transaction hash streamed over the block, `verify_strict`. |
| 20 | `run_checkpoint_b` | deviation as documented | CC/rule_b.rs:54-145; D7, D13 | Membership, no repeated channel or attestation key, tip on the linked path from `T` (id and hash), both signatures, `t(n)`, producer in `K` (G07–G10). |
| 21 | attestor core | deviation as documented | AT:89-215 | The prefix binds chain, vault, configuration, channel (G18). |
| 22 | `Frontier`, `burn_leaf` | deviation as documented | CC/tree.rs:11-88 | Capacity 2^32 − 1 (CC/tree.rs:9-11), so the full-tree root never has to be stored. |
| 23 | `State::digest`, `Config` | deviation as documented | CC/state.rs:30-110 | `S` commits `tipId`; with at least one new block a checkpoint always changes the state (used in §2.3). |
| 24 | journals | OK | CC/journal.rs:33-128 | 17 words as the vault's digest (G15). |
| 25 | attestation `main`, `handle` | OK | P:50-100 | Coded refusals; every message routed. |
| 26 | `init`, `check_config`, `period_window` | deviation as documented | P:129-235; att §3.4, §4.6 | Receiver is `instance(H(config ‖ checkpoint))`, empty state, valid config. |
| 27 | `open`, `update_window` | deviation as documented | P:147-267; att §4.2, §4.6 | I-5 (sequencer clock) as documented. |
| 28 | job steps | deviation as documented | P:198-401; att §3.2, §4.2–4.3 | `JobOpen` only from the instance; `Miller`, `Adopt`, `Finish` open to anyone, as att §3.2's table says. |
| 29 | `apply`, `apply_job` | deviation as documented | P:269-349; att §4.5 | `Apply` only from this instance's job actor; the store comparison is the core's (`steps::apply`). |
| 30 | `verify_all`, `apply_verified` | deviation as documented | P:288-305, 470-500; att §3.2, §4.2 | `from_own_program`; activation refused. |
| 31 | `write_record` | deviation as documented | P:405-425; att §3.1 | Only from the instance, at `header(id, slot)`; an identical rewrite is accepted, a different one refused. |
| 32 | `attest`, `attest::evaluate` | deviation as documented | P:435-461; W (`evaluate`); att §5, §6.1 | The request's instance must be the record's or its root id; chain checked. |
| 33 | `read` | deviation as documented | P:463-466; att §6.2 | Replies only to a sender. |
| 34 | `succeed`, `check_successor`, `spawn` | deviation as documented | P:508-593; att §3.5 | The three invariants; `Spawn` only from the parent. |
| 35 | bridge `main`, `handle` | OK | B:53-235 | Replies accepted only from the awaited actor (B:112-128). |
| 36 | `init`, `check_pairing` | OK | B:237-257, 342-372; BW:1359-1374 | Slot 2 must hold `configId`. |
| 37 | `import_root`, `check_import`, `write_root` | deviation as documented | B:260-288, 410-428; BW:1336-1357 | Top-level only (B:274), refused after `Sunset`, root and count equal to the message's (BW:1344), once per count; so the checkpoint's `max(leafCount)` is what LEZ imported. |
| 38 | `register`, `check_register` | deviation as documented | B:291-317; BW:1376-1407 | Entry, schedule 0 and the leaf count under one record; `known_until`. |
| 39 | `import_fees`, `check_fees`, `extend_fees` | deviation as documented | B:319-340, 431-470; BW:1409-1430 | Schedules start at or after `known_until` and the previous one, matching the vault's "last schedule with `fromLeaf ≤ leaf`" also when two share a `fromLeaf`. |
| 40 | `claim`, `verify_claim` | deviation as documented | B:519-543; BW:1453-1475 | `leafIndex < count`, inside the range, < 2^32, bridge core statement. |
| 41 | `claim_privately`, `claim_reply` | deviation as documented | B:546-597 | Authorised; private kind only at its destination. |
| 42 | `issue`, `fees_for`, `admit` | deviation as documented | B:600-670; BW:1159-1169 | Denomination registered, range starts at its schedule and ends by the next or `known_until`. |
| 43 | `spend`, `Mint` | deviation as documented | B:165-180, 672-715 | Spent once; mints only at `holder(account)`. |
| 44 | `burn` | deviation as documented | B:720-801 | Root call or private root only; the visible call is the root (public) or the boundary delivery (private); every refusal reverts the whole transaction (LD-2 below). |
| 45 | `transfer`, `credit` | deviation as documented | B:806-889 | Multiples of `lezUnit`; `Credit` only from the token actor. |
| 46 | `role_message` | deviation as documented | B:902-1023 | Role messages forwarded from a signed root call; `Sunset` executes only as a visible root call (authority's own actor, or keyless at `config(cid)`). |
| 47 | `cosigned` | deviation as documented | B:1069-1121 | A multisig `Sunset` only approves (B:1098-1104). |
| 48 | `attest_retired`, `check_retired` | deviation as documented | B:1026-1047; BW:150-155, 1433-1439 | Byte 11 of slot 37 is FerrataVault v5's `retired`; pinned now on the vault side too (§2.1). |
| 49 | clock and handover | deviation as documented | B:1049-1067, 1179-1244; BW:521 | Monotone attested time; a proposal valid only within a day of its record. |
| 50 | `Echo`, `Read` | deviation as documented | B:137, 146-149 | `Echo` changes nothing anywhere. |
| 51 | consumer reader | OK | C:63-83, 308-321 | Replies only from the awaited actor. |
| 52 | consumer `Claim` | OK | C:252-271, 322-334 | The claim actor must be the one the statement's nullifier names. |
| 53 | `AttestPrivately` | deviation as documented | C:88-100, 335-354 | Binds the record's own instance id, not a root id (stricter than `attest`). |
| 54 | deposit claims | OK | C:127-236, 355-405 | `C` opened, `N` from the secret, one claim actor per `N`. |
| 55 | holdings `Check` | deviation as documented | H:31-66 | Only the awaited record replies; later block required. |

## 2. Problems and gaps, most severe first

No PROBLEM: nothing below makes a function accept what its specification refuses, or refuse what it accepts, on
any input that reaches it on the shipped path.

### 2.1 Gap (closed on the branch): slot 37, which LEZ reads, has no test or documentation on the vault side

- **What.** The bridge program's keyless `Sunset` path attests the vault's storage slot 37 and requires byte 11 of
  its big-endian word to be 1 (`BW:150-155` `SLOT_RETIRED`, `RETIRED_BYTE`; `check_retired`, BW:1433-1439;
  lez/BRIDGE.md §4.10, §5). That byte is `retired`, which Solidity packs at offset 20 beside `freezeAuthority`
  (FV:294-296). The vault names only slots 0–2 (and 3–34) as its cross-chain interface (FV:274-278, README
  "Storage-slot contract", where 35–37 are "the rest of the layout"); no vault test reads slot 37. The LEZ harness
  tests the program against a synthetic slot 37 (`bridge_roles.rs:105-108`), not against the vault.
- **Test.** VR25 declares `retired` before `freezeAuthority`: it survived every suite and the invariant campaign
  (§3.1). New tests `ReviewVault::test_review_slot37_holdsTheFreezeAuthorityAndTheRetiredByteTheLezSideReads` and
  `test_review_slot37_keylessRetirementSetsTheSameByte` pass on `89e6d6d` and kill VR25 (both fail on it; `spike/tmp/review-vault/run3.log`).
- **Impact.** None on v5 or on the live round trip. A later vault whose layout moves `retired` breaks the keyless
  `Sunset` either way. With a zero at byte 11 it never opens: if the freeze key is also lost, LEZ can never be
  sunset, so no sunset checkpoint, so deposits LEZ never imported are never refundable (the case D13's keyless path
  exists for). With another non-zero byte there (an address byte) it is open from the start: anyone can sunset the
  LEZ configuration while the vault runs, ending its imports for good (no loss: the vault stops deposits at the
  sunset checkpoint (L-3) and refunds what LEZ never imported, but a shutdown nobody authorised).
- **Fix suggested.** Add slot 37 (`freezeAuthority`, and `retired` at byte offset 20, word byte 11) to the vault's
  storage-slot contract (README table, done on the branch as a suggested docs-only fix, `bbdfe41`; the comment at
  FV:274-278 waits for the next vault, since a comment changes the vault's metadata hash), and keep the two tests.

### 2.2 Gap (closed on the branch), guest row 17: the refusal of a repeated key had no test

- **What.** `check_key_set` (CC/rule.rs:144-158) refuses a key list that repeats a key, because Bedrock's list is a
  vector and a repeated key would let one key fill several signature indices of the next rotation (the comment at
  CC/rule.rs:142-143; `checkpoint/README.md`, "Every key set reached must have … no repeated key"). G13 removes the
  refusal: it survived the checkpoint core's 28 tests, and no rule (A) test in `checkpoint/fidelity/tests/rule.rs`
  asserts `ConfigMalformed` either (by reading; "the same key twice counts once" there is about support, and the
  attestor's `refused_a_repeated_key…` about rule (B)'s attestors).
- **Test.** `checkpoint/crates/checkpoint-core/tests/review_guests.rs`: a rotation to a list repeating a key, and a
  parent repeating a key with no rotation, are both refused with `ConfigMalformed` (they kill G13); a third test
  shows what the refusal does not cover: the parent's own set is checked only when no rotation replaces it
  (CC/rule.rs:246-258, so that a genesis channel may start below the floors), so a parent that repeats a key lets
  that key give two of the `t(4) = 3` signatures of the rotation that replaces it, and the checkpoint is accepted.
  All three pass on `89e6d6d`'s code.
- **Impact.** None on the live programs: every set a checkpoint reaches is checked, so only a genesis state can
  repeat a key, and the genesis state is a trust root (I-1). Had the refusal been dropped in a later program,
  nothing would have caught it.
- **Fix suggested.** Keep the tests; add to the deployment check of I-1 (`docs/trust.md`'s row for the vault's
  deployment parameters) "the genesis key list repeats no key". No program change.

### 2.3 DOC MISMATCH, row 22: `revokeFrom`'s bound is the checkpoint programs', not the vault's

- **What.** FR:339-340, FV:232-234 and README:431 bound the loop by `revokeWindow / deltaFloor + 1` because "two
  finalisations are at least `deltaFloor` apart". In the vault's own logic they need not be: a no-op checkpoint
  (`newState == parentState`) does not move the epoch (FV:832, 1340), so it and a state-changing candidate
  submitted beside it finalise in the same block (`Checkpoint::test_finalize_aNoOpCheckpointDoesNotInvalidatePendingCandidates`
  already shows the second stays live).
- **Test.** `ReviewVault::test_review_twoFinalisationsShareABlock_soTheRevokeBoundRestsOnTheProgram`: a rule (B) vault
  with `revokeWindow / deltaFloor = 2` ("at most 3 marked") finalises six checkpoints in pairs, one pair per block,
  and `revokeFrom(0)` at the last revocable second marks six. Passes.
- **Impact.** None with the pinned programs: `S` commits `tipId` (CC/state.rs:75-95) and a checkpoint needs at
  least one new block (CC/rule.rs:261, 353), so every finalisation changes the state, and state changes are
  at least `deltaFloor` apart (each needs a candidate submitted after the previous one and a full wait). With a
  program that could make no-ops, the bound roughly doubles: about 2,000 marks at the constructor's limit, some
  10 M gas by the per-mark estimate of FV:233 (estimated, not measured), still under a block.
- **Fix suggested.** README:431 on the branch (`bbdfe41`, docs only); FR:339-340 and FV:232-234 with the next vault
  (a comment changes the metadata hash, so the bytecode). The wording, in all three: "every checkpoint of the pinned programs
  changes the state (`S` commits the tip), and state changes are at least `deltaFloor` apart; a no-op checkpoint,
  which no pinned program makes, could double the count".

### 2.4 DOC MISMATCH, row 33: `bridge.md` §16 says refunds open at the last revocable second

- **What.** `docs/design/bridge.md` §16, "Window boundary", ends: "The prototype opens them at that same second
  (§19)." The vault opens refunds and releases whose delay equals `revokeWindow` one second later (FV:1018-1020,
  FV:1372), as D11 asks, and §19's own table says "built (vault v4)".
- **Test.** `Refund::test_refund_noSecondAllowsBothRefundAndRevocation`,
  `ReleaseDelay::test_releaseDelay_equalToRevokeWindowOpensAfterTheLastRevocableSecond` (V91, V152).
- **Impact.** None; a reader of the design note would believe a one-second overlap exists.
- **Fix suggested.** On the branch (`bbdfe41`): "The vault opens them one second after the last revocable second
  (since v4; §19)."

### 2.5 DOC MISMATCH, row 14: README's epoch formula predates the L-1 fix

- **What.** README:434: "A candidate records `epoch = revocationEpoch + checkpointCount()`". The code uses
  `revocationEpoch + _stateChanges` (FV:1340-1342), which leaves out no-op checkpoints, so that they do not stale
  pending candidates (the L-1 fix).
- **Test.** `Checkpoint::test_finalize_aNoOpCheckpointDoesNotInvalidatePendingCandidates`; V46, V47.
- **Fix suggested.** On the branch (`bbdfe41`): "`epoch = revocationEpoch +` the number of finalisations that moved
  `finalState`".

### 2.6 DOC MISMATCH, guest row 10: the rule (A) guest's comment gives SPEC-v2's journal size

- **What.** `checkpoint/guests/checkpoint/src/main.rs:1`: "commits the 416-byte journal". The guest commits
  `CheckpointJournal::encode`, 17 words, 544 bytes (CC/journal.rs:31, 68-86); 416 bytes is SPEC-v2's 13-word
  journal, which no vault of this tree accepts. Also `CC/lib.rs:5`: "[`wire`]: the bridge program's messages
  (defined here; the program does not exist yet)": it exists (`lez/guests/bridge`), and `CC/wire.rs:3-7` says so.
- **Impact.** None on the program.
- **Fix suggested.** Not in this tree: the guests may not change. "416" → "544" keeps every line number, so it
  should leave the image id unchanged; confirm with `scripts/reproduce-image-ids.sh` before the next guest release
  takes it.

### 2.7 DOC MISMATCH: EVIDENCE row 12's test counts are older than the tree

- **What.** Row 12: "433 tests passed, 1 skipped … in 38 suites (… 43 tests with real proofs in 10 suites …)".
  `forge test` at `89e6d6d`: 451 passed, 1 skipped, 43 suites; 61 tests with real proofs in 15 suites under
  `test/real/` (measured, log above). `vault/README.md` already says 451. The coverage and mutant figures of the row
  were not re-measured here.
- **Fix suggested.** The row text in `spike/tmp/review-vault/REPORT.md` (the owner edits EVIDENCE).

### 2.8 Smaller notes (no verdict change)

- FV:1472-1474 (`_valueUnits`): "amount ≤ MAX_DENOMINATION and unitWeight ≤ MAX_UNIT_WEIGHT, both 2**128 − 1".
  `MAX_DENOMINATION` is `(2^128 − 1)/2` (FR:25), and the callers pass totals with fees (a deposit's, a refund's).
  Still no overflow: total ≤ 1.02 × 2^127, weight < 2^128, product < 2^256.
- Specification paths in source comments: FV:14 and `BC/lib.rs:1` name `notes/design/SPEC-v2.md` and
  `SPEC-v3-delta.md`; FR:12 "SPEC-v3-delta D11". The files are `docs/design/spec/bridge-spec-v2.md` and
  `bridge-spec-v3-changes.md`.
- `BC/checkpoint.rs` and `BC/encoding.rs`'s checkpoint journal are SPEC-v2's (13 words, `keyset/v1`, domain v2). No
  guest uses them (the checkpoint guests use `CC/`); `bridge/e2e` wraps them as v3. A note in that module would stop
  someone building a vault journal from it.
- The brief lists EVIDENCE row 40 for the vault: it is the core's cargo-fuzz campaign and pins no vault function.
  The vault's fuzzing is forge's `testFuzz_*` (row 12) and the invariant campaign; the guests' is row 78.
- LD-2 of the security review is still an assumption of guest rows 16, 44 and 46: the checkpoint skips a
  transaction only if LEZ lists it in `BlockBody::reverted`, so a failing private transaction's public part must be
  listed there. Not checked here either (it needs LEZ's settlement code).
- Guest row 53: the reference consumer's private path binds the record's instance id, not a root id, so a pool bound
  to a root id is served publicly (by `attest`) but not privately. Stricter, so safe; worth a line in the consumer
  guide.
- The release guest commits whatever `relayer` and `relayFee` the prover chose; the vault bounds the fee (row 25).
  A front-running relayer of a proof with `relayer = 0` takes the fee (I-8, documented).

## 3. Mutants

### 3.1 Vault (VR01–VR25, `vault/tools/review_mutants.py`)

First run against the suite as it was (`REVIEW_WITHOUT_NEW_TESTS=1`): 24 killed, 1 survived, 117 s for five slices
of five on the box (logs `spike/tmp/review-vault/mut/vr-*.log`). Re-check of the survivor with the new tests:
killed by both slot-37 tests (19 s, `spike/tmp/review-vault/run3.log`).

| Id | Change | Result (first run) | Killed by (first failing test) |
|---|---|---|---|
| VR01 | `fees()`: mint and relay fee swapped | killed | `PullPayment::test_relayerThatCannotReceive_isCredited` (8 failing) |
| VR02 | `burnRootStatus()` ignores revocation | killed | `Revocation::test_reprovenCheckpointIsAcceptedAgain_butANullifierPaysOnce` (4) |
| VR03 | beacon nonce without the vault's address | killed | `real/ReturnLeg::test_gas_everyStepWithTheRealVerifier` (11) |
| VR04 | `_finalize` keeps the candidate | killed | `ModeB::test_modeB_finalisesAtItsWaitNotASecondBefore` (3) |
| VR05 | revoking the sunset checkpoint itself keeps the boundary | killed | `Refund::test_refund_forgedSunsetRevokedInsideTheWindowPaysNothing` (3) |
| VR06 | refund leaf binds the caller | killed | `Refund::test_refund_drawsOnTheReleaseBuckets` (18) |
| VR07 | leaf-schedule search leaves out the schedule in force | killed | `Refund` (18; arithmetic underflow) |
| VR08 | `acceptAdmin` leaves `pendingAdmin` | killed | `Admin::test_transferAdmin_isTwoStep` (1) |
| VR09 | relay fee never paid | killed | `Fees` fuzz (11) |
| VR10 | `minReleaseDelay` above 30 days | killed | `Admin::test_constructor_turnSecondsAndMinReleaseDelay` (1) |
| VR11 | `initialDelta` above 30 days | killed | `Admin::test_constructor_deltaRules` (1) |
| VR12 | zero `maxFreeze` | killed | `Admin::test_constructor_deltaRules` (1) |
| VR13 | ETH sent with 2,300 gas | killed | `PullPayment::test_claimOwed_cannotBeReenteredFromAPayout` (3) |
| VR14 | value units without `/ lezUnit` | killed | `setUp` of the cap suites (4) |
| VR15 | library `revokeFrom` skips the first | killed | `Revocation` (6) |
| VR16 | rule (A)'s second proof over echo 0 | killed | `real/ReturnLeg` and 26 more |
| VR17 | a delay change applies to every root | killed | `ReleaseDelay::testFuzz_delayLookupMatchesALinearScan` (3) |
| VR18 | `_isWatcher` ignores the middle slot | killed | `Roles::test_applyRoles_fourteenDaysLaterByAnyone` (2) |
| VR19 | `CandidateSubmitted` carries `newState` as the tip hash | killed | `ModeB::test_modeB_submitRecordsNoBeaconAndKeepsTheJournalDigest` (2) |
| VR20 | `Release` event names the bound relayer, not the payee | killed | `Release` (3) |
| VR21 | `tokenCap()` without refill | killed | `Caps::test_capUpdate_raisingAddsNoInstantHeadroom` (5) |
| VR22 | relay fee of exactly 1% refused | killed | `Admin::testFuzz_registerToken_relayFeeCeiling` (4) |
| VR23 | mint fee of exactly 1% refused | killed | `Fees` fuzz (8) |
| VR24 | refund paid to the caller | killed | `Refund::test_refund_revokingTheSunsetCheckpointDisablesRefunds` (8) |
| VR25 | `retired` declared before `freezeAuthority` | **survived** (unit, fuzz, real proofs, invariants at 64 runs) | with the new tests: `ReviewVault::test_review_slot37_*` (2 failing) |

The vault's own list (V0–V253) is thorough: every behaviour I expected to be unpinned was pinned. The one survivor
is not a behaviour of the vault but its layout, which another program reads.

### 3.2 Guests (G01–G18, `scripts/review-guest-mutants.py`)

Baselines: `ferrata-bridge-core`, `attestor-core` and `checkpoint-core` pass. 18 mutants, 165 s on the box (log
`spike/tmp/review-vault/run3.log`; the re-check `run4.log`); the runner restored every source (the local tree's
`git diff` of the guests' sources is empty after both runs). First run: 14 killed, 4 survived the two crates' own
tests; with `review_guests.rs` one more is killed; the other three are pinned by rule (A)'s tests in
`checkpoint/fidelity/tests/rule.rs`, which I read but did not run.

| Id | Crate | Change | Result | Killed by (first failing test), or pinned where |
|---|---|---|---|---|
| G01 | bridge core | `verify_path` ignores the direction bits | killed | `gasdrop::tests::wrong_witness_or_foreign_journal_is_refused` |
| G02 | bridge core | `NOut` from the leaf (v1-like, index-dependent) | killed | `outbound::recover_burns_follows_the_gap_limit_and_ignores_notes_it_did_not_author` |
| G03 | bridge core | gas-drop journal's relayer is the one-time address | killed | `gasdrop::tests::journal_binds_n_one_time_address_and_relayer` |
| G04 | bridge core | `N` without `fid` | killed | `gasdrop::tests::journal_binds_n_one_time_address_and_relayer` |
| G05 | bridge core | release journal: recipient and relayer swapped | killed | `encoding::release_journal_is_eleven_abi_words` |
| G06 | bridge core | burn leaf binds index 0 | killed | `outbound::recover_burns_follows_the_gap_limit_and_ignores_notes_it_did_not_author` |
| G07 | checkpoint core | rule (B): one attestation key under two channel keys | killed | `rule_b::refuses_a_duplicate_signer` |
| G08 | checkpoint core | rule (B): tip hash not compared | killed | `rule_b::refuses_an_attestation_over_a_block_that_is_not_a_descendant` |
| G09 | checkpoint core | rule (B): producer need not be in `K` | killed | `rule_b::checks_the_producer_inscription_of_t` |
| G10 | checkpoint core | rule (B): one attestation short | killed | `rule_b::refuses_too_few` |
| G11 | checkpoint core | wire: trailing bytes accepted | killed | `units::wire_decode_is_exact` |
| G12 | checkpoint core | rotation may change the posting timeframe | survived the crate's tests | `fidelity` `key_rotations_need_t_current_signatures_and_respect_the_floors` asserts `ConfigTimeframe` (rule.rs:290-299) |
| G13 | checkpoint core | a repeated key accepted | **survived**; killed with the new tests | `review_guests::a_parent_key_set_with_a_repeated_key_is_refused_when_no_rotation_replaces_it` (§2.2) |
| G14 | checkpoint core | frontier count above capacity canonical | killed | `units::frontier_rejects_non_canonical_and_full` |
| G15 | checkpoint core | journal: `tipId` and `burnCount` swapped | killed | `cross_vectors::key_sets_states_journals` |
| G16 | checkpoint core | a segment of another configuration accepted | survived the crate's tests | `fidelity` `segments_compose_to_the_same_journal` asserts `SegmentConfigMismatch` (rule.rs:365-368) |
| G17 | checkpoint core | an `ImportRoot` after `Sunset` still counts | survived the crate's tests | `fidelity` `sunset_and_imports` asserts an imported count of 5, not 9 (rule.rs:321-341) |
| G18 | attestor core | the messages' prefix leaves out the vault | killed | `attestor::every_field_is_bound` |

The bridge core's proptest suites wrote two `*.proptest-regressions` files while mutants failed; they were deleted,
not committed.

## 4. Sign-off checklist (the "Reviewed" column is the owner's)

Commands from the worktree root on the build box: `M=…/spike/modal-build; $M/mrun --profile <yours> -- '<command>'`
(the vault needs `vault/lib`, see `vault/README.md`). `VT` = `cd vault && forge test --match-contract`;
`mut n` = `python3 -I tools/mutation_spotcheck.py n n+1` from `vault/`; `vr` = `python3 -I tools/review_mutants.py`.

**Vault, deposits (rows 1–7).** FV:676-698 and 1190-1250 beside v2 §3.1–3.4; the fee total (FV:1206-1207) beside
D13; `VT 'DepositTest|MerkleTest|TreeVectorsTest'`; `mut 0`…`mut 17`.

**Vault, checkpoints (rows 8–23).** FV:741-846 and 1256-1350 beside v2 §4.3, D3 item 5, D4, D7, D11, D13; FR:262-271
and 341-353 beside v2 §4.4 and D5; read §2.3 and decide the comment; `VT 'CheckpointTest|ModeBTest|JournalBindingTest|RevocationTest|ReviewVaultTest'`;
`mut 29`…`mut 87`, `mut 135`…`mut 151`; `vr 3 5`, `vr 14 16`.

**Vault, releases and refunds (rows 24–33).** FV:707-733, 859-879, 1357-1456 beside v2 §3.4, §5.2–5.4, D5, D11, D13;
FR:149-194, 276-295; `VT 'ReleaseTest|RefundTest|PullPaymentTest|FeesTest|ReleaseDelayTest|EmergencyStopTest'`;
`mut 88`…`mut 125`, `mut 201`…`mut 224`.

**Vault, roles, retirement, freezes (rows 34–40).** FV:961-1095, 1506-1529; FR:298-334 beside v2 §7, D11, D13;
§2.1 (slot 37); `VT 'RolesTest|FreezeTest|AdminTest|SecurityReviewTest|ReviewVaultTest'`; `mut 126`…`mut 130`,
`mut 154`…`mut 189`; `vr 24 25`.

**Vault, registry, fees, caps (rows 41–51).** FR:54-143, 198-258, 358-588 beside v2 §3.4, D13, lez/BRIDGE.md §5;
`VT 'AdminTest|FeesTest|CapsTest|RegistryStorageTest'`; `mut 19`…`mut 26`, `mut 131`…`mut 134`, `mut 190`…`mut 200`,
`mut 225`…`mut 253`.

**Vault, constructor (row 52).** FV:578-657; `VT 'AdminTest|ModeBTest|RolesTest|SecurityReviewTest'`.

**Guests, release and statements (rows 1–9).** `bridge/guests/src/bin/*.rs`, BC/outbound.rs:50-200, BC/inbound.rs,
BC/tree.rs:56-88, BC/gasdrop.rs, BC/encoding.rs:553-805 beside v2 §3.2–3.3, §5.1–5.2, D12, D13;
`cd bridge && cargo test -p ferrata-bridge-core --locked`; `scripts/ci/witness-independence.sh` (row 77).

**Guests, checkpoint (rows 10–24).** CC/rule.rs, rule_b.rs, input.rs, wire.rs, mantle.rs:106-209, tree.rs,
state.rs, journal.rs, AT beside v2 §4.1–4.2, D3, D7, D10, D12, D13 and `checkpoint/README.md`'s two lists; §2.2, §2.6;
`cd checkpoint && cargo test -p checkpoint-core -p attestor-core --locked` and the rule (A) tests in `fidelity`
(`checkpoint/README.md`, "Run it").

**Guests, attestation program (rows 25–34).** P:50-593 beside att §3–6; `cd lez && RISC0_DEV_MODE=1 cargo test -p
harness --locked --test public_path --test private_placement --test attest_public --test successor`
(`RISC0_HOME` as `lez/RESULTS.md` §6 says).

**Guests, bridge program (rows 35–50).** B:53-1244 and BW's checks beside v2 §1, §3.3, §5.1, D10, D13, lez/BRIDGE.md
§§2, 4, 5, 6; §2.1 and LD-2; `… --test bridge_inbound --test bridge_private --test bridge_burn --test bridge_controls
--test bridge_roles --test bridge_time --test bridge_secfix --test bridge_vault_storage`.

**Guests, consumers (rows 51–55).** C, H beside att §6–7; `… --test attest_private --test private_consumer --test
vault_consumer --test holdings`.

## 5. Branch

`review-vault` (from `89e6d6d`), tip `0d6eb75`. Commits: `a2e42bb` (docs/REVIEW-vault.md), `321620d`
(docs/REVIEW-guests.md), `4921c9c` (the vault tests and the two mutant runners), `4c4e068` (the checkpoint core's review tests),
`d8620bb` (the tables name the new tests), `bbdfe41` ("suggested fix, not merged": `vault/README.md`'s bound, epoch
and slot 37, `docs/design/bridge.md` §16; docs only), `0d6eb75` (README and `docs/how-it-was-built.md` point at the
two tables, "drafted, not yet signed").
`git diff 89e6d6d -- vault/src bridge/guests bridge/crates checkpoint/crates checkpoint/guests lez/guests lez/wire
lez/bridge-wire lez/holdings-wire` is empty. Not pushed.
