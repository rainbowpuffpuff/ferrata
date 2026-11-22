# Stream fix-attestor: the rule (B) attestor's review findings (for the integrator)

Branch `fix-attestor`. Two passes:

1. From `5cf1e8e7` (tip `e7aef656`): H-2 (CK-1), M-3 (CK-2), M-4 (CK-3) of `review-code-bridge`; M7, L7, L8 of
   `review-code-modules`.
2. The branch's own review (`review-fix-attestor`, 22 Nov): M1-M4, row 63's wording, L1-L8 (this pass).

No guest or program changed: no image id moves (`checkpoint/attestor`, the module's calls crate and the scripts are
host code; `checkpoint-core` and every guest untouched).

## Pass 1 (unchanged; see the commits)

| Finding | Commit | Test that shows it (all in `checkpoint/`, `cargo test -p ferrata-attestor --locked`) |
|---|---|---|
| M-3: a block with an atomic withdrawal stopped every attestor | `9a4ea7ff` | `attestor/tests/withdrawals.rs` |
| H-2: the attestor signed blocks LEZ parked | `8a51ac40` (+ module `81a75215`) | `attestor/tests/lez_check.rs`: `h2_*` (three) |
| M-4: one garbage or repeated inscription stopped every attestor | `8a51ac40` | `lez_check.rs`: `m4_garbage_repeats_and_unreadable_ops_never_halt` |
| M7: whole-chain reads; a narrowed read switched the two-histories guard off | `8a51ac40` (+ `81a75215`) | `lez_check.rs`: `m7_*` (two); `io.rs`; `attestor.rs::the_attestor_never_signs_two_histories` |
| L7 (first review): state after publish, non-atomic | `8a51ac40`, `81a75215` | now shown by a test: `files.rs::l7_the_state_holds_the_tip_before_the_attestation_is_written` (pass 2) |
| L8 / L-6 (first review): key file mode | `8a51ac40` | now shown by a test: `files.rs::l3_keygen_writes_an_owner_only_key_and_never_overwrites` (pass 2) |

## Pass 2: the branch's review

| Finding | Commit | Test or re-run that shows it |
|---|---|---|
| M1: `zone-bridge.sh` asked stopped LEZ nodes for its final round | `96bf1a4e` | re-runs: `e2e/runs/zone-bridge-3` (row 100: T1 = 96, all four signed it at the first final round; vault accepted, both releases paid; `ZoneBridgeLezCheckedTest` 4/4) `75e283f8`; `e2e/runs/zone-bridge-4` (row 101, `ZB_ROTATE=1`: five attestors, T1 = 148; `ZoneBridgeRotationLezCheckedTest` 6/6) `a1de57aa` |
| M2: `return-leg.sh` passed no `--lez` | `96bf1a4e` | re-run: `e2e/runs/zone-bridge-rc3-2`, `lez/results/v0.4.0-rc3/return-leg-2` (rows 129, 130: T1 = 106 signed against rc3's own sequencer; `checkpoint_b_0_3_5` proven, the vault paid both releases, the deployed id and a replay refused; `ZoneBridgeRc3LezCheckedTest` 5/5) `a1de57aa` |
| M3: the module's e2e scripts and README on the old interface (row 95's re-run: 9 PASS 14 FAIL) | `23bdf5f2`, `18bc700f` | `modules/scripts/attestor-e2e.sh` natively: **24 of 24** (`modules/results/fix-attestor-2026-11-22/attestor-e2e-native/summary.md`); `daemon_requests.py`'s 12 attestor requests: every expectation met; calls crate 3 of 3 |
| M4: "byte-identical" was false (only the 264 signed bytes match) | `ac1ec83f` (text), `f624f949` (`zone-attest.sh` writes a replay elsewhere) | replay into scratch: `encoded` equal x4; `node.first_block` 2 -> 30, `node.blocks_checked` 29 -> 1, `node.fork_digest` null (absent in the 11 Nov records); 3-of-4 input identical |
| Row 63's wording | this file | below |
| L1: README said a LEZ node marks `Finalized` exactly up to its final tip (#1053) | `81b008aa` | doc; the LIB filter's mutant fails 3 tests |
| L2: two guards no test covered | `f3c19199` | `lez_check.rs` `l2_a_first_read_never_roots_at_a_sibling_the_lez_node_does_not_hold` (kills `first-root-unchecked`), `l2_a_tip_inscribed_by_a_key_outside_k_is_passed_over` (kills `signer-not-in-k-ok`) |
| L3: key file mode and refusal cited as code | `4bca267e` | `files.rs` `l3_keygen_writes_an_owner_only_key_and_never_overwrites` |
| L4: module `attest`: a silent LEZ node `refused`; node mode without a state | `9427dc4f` | calls crate `a_live_attest_keeps_a_state_and_a_silent_lez_node_is_an_error` |
| L5: 16 junk ids could stop a first read | `b48579bf` | `l5_junk_ids_above_the_chain_never_stop_a_first_read` (kills `first-root-cap16`) |
| L6: fixed temporary file, `File::create` | `4bca267e` | `files.rs` `l6_write_atomic_never_writes_through_a_planted_link`, `l6_two_writers_never_share_a_temporary_file` (kill `write-atomic-fixed-tmp`) |
| L7: operator procedure; `getBlock` marked for removal | `81b008aa` (README "Operating it"); order test `4bca267e` | `files.rs` `l7_the_state_holds_the_tip_before_the_attestation_is_written` |
| L8: `live-node.sh` cannot be reproduced | `b303d3c8` | RESULTS §6, the script's header; row 63's text below |

Measured on the build box at the branch tip: `cargo test -p ferrata-attestor --locked` 31 passed (attestor 12, files 4,
io 2, lez_check 10, withdrawals 3); clippy `-D warnings` and rustfmt clean; 16 of 16 guard mutants fail at least one
test (the review's 14 and two that undo L5 and L6).

Numbers of the three re-runs: `checkpoint/attestor/RESULTS.md` §10.1 (measured on the build box; each attestor signed
every round, none refused, the final round's tips equal at the first try).

## Proposed status changes

No row of `docs/requirements-status.md` changes status (RFP-022 Sec.7 stays **in part**: the row is about the
documents; pass 1's `trust.md` §3.3 / `bridge.md` §8.3 wording stands, and `lez` must be the attestor's own LEZ node).

## Proposed EVIDENCE text

- **Row 63**, replace with: "`ferrata_attestor`, the rule (B) signer a sequencer runs: reads its Logos Blockchain
  node's final tier (LIB, channel key set, immutable blocks) from the tip it signed last, follows the chain as LEZ folds
  it, and signs a tip only when its own LEZ node serves it `Finalized` and byte for byte as inscribed, at or below the
  LIB (a block LEZ parked is never signed; a status shown early, LEZ #1053, is not signed before Bedrock finality); the
  tip's inscription is checked as the checkpoint program checks `T`'s (withdrawal transactions included); garbage and
  repeated inscriptions are passed over; the state is written atomically before publishing; never signs two histories,
  across restarts; the collector keeps valid attestations and refuses below `t(n)`" | "31 tests (the lab node's real
  answers; a forged turn; 4 x 150 random rounds with restarts; rounds over HTTP against a node stub; the CLI's key file
  and write order); 16 of 16 guard mutants fail a test; beside live LEZ sequencers (rc1 zones of four and five, an rc3
  zone of one): every round signed, none refused (zone-bridge-3, -4, -rc3-2); `zone-attest.sh` replayed: the zone's
  four attestations sign the same 264 bytes as the recorded ones (their `node` facts now read `first_block` 30,
  `blocks_checked` 1), the 3-of-4 input identical to the proven one. The run against a copy of the lab node (§6)
  predates the LEZ check and cannot be repeated as recorded" | build box | none (native) |
  `cd checkpoint && cargo test -p ferrata-attestor --locked` | `checkpoint/attestor/RESULTS.md` §1, §2, §10, §10.1.
- **Row 95**: the first run's numbers stand as of 13 Nov (23/23, before the LEZ check); add "22 Nov, after the LEZ
  check: `attestor-e2e.sh` natively 24/24 (the zone's inscribed blocks stood in as the LEZ node's), the daemon requests'
  12 attestor expectations met; through Logos Core and `attestor-zone-live.sh` (updated) not re-run" |
  `modules/RESULTS.md` (22 Nov section), `modules/results/fix-attestor-2026-11-22/attestor-e2e-native/summary.md`.
- **Rows 100, 101, 129, 130**: add the re-run beside each (their 13 and 18 Nov numbers stand): row 100
  `e2e/runs/zone-bridge-3`, row 101 `e2e/runs/zone-bridge-4`, rows 129-130 `e2e/runs/zone-bridge-rc3-2` (+
  `lez/results/v0.4.0-rc3/return-leg-2`), each "with LEZ-checked attestors", numbers in RESULTS §10.1; re-check commands
  as the rows' with the new run dirs and `ZoneBridge{LezChecked,RotationLezChecked,Rc3LezChecked}Test`.
- **New row**: "Two reviews of the rule (B) attestor (21 and 22 Nov): H-2, M-3, M-4, M7 (pass 1); M1-M4, L1-L8 (pass
  2) fixed" | "probe of `5cf1e8e7`: F signed; after: F never signed; 31 tests, 16 mutants killed; module e2e 24/24; the
  bridge zones re-run with LEZ-checked attestors (rc1 x2, rc3)" | build box | none | `cargo test -p ferrata-attestor
  --locked` | RESULTS §10, §10.1.
