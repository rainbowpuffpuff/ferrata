# Stream fix-attestor: the rule (B) attestor's review findings (for the integrator)

Branch `fix-attestor`. Three passes:

1. From `5cf1e8e7` (tip `e7aef656`): H-2 (CK-1), M-3 (CK-2), M-4 (CK-3) of `review-code-bridge`; M7, L7, L8 of
   `review-code-modules`.
2. The branch's own review (`review-fix-attestor`, 22 Nov): M1-M4, row 63's wording, L1-L8.
3. Its second review (`review-fix-attestor2`, 22 Nov): M1 (a machine path in a record), L1-L4, row 95's wording,
   N1-N7 (this pass).

**For the integrator: this branch is based on `5cf1e8e7` (integration9)**, 16 commits past main `f13faee2`. Those
commits edit `README.md`, `docs/EVIDENCE.md`, `docs/requirements-status.md` and `docs/traceability.md` (`2024dc75`,
`5cf1e8e7`); this stream's own commits touch none of these four files. Merging `fix-attestor` brings integration9
along, unless integration9 lands first. Pass 3 rewrote two unpushed commits (no other ref held them): `a1de57aa`
is now `22d0842f` (the same commit without the machine path) and `38ed6f33` is now `6a6c2da5` (unchanged content);
the old ids appear only in the review reports.

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
| M1: `zone-bridge.sh` asked stopped LEZ nodes for its final round | `96bf1a4e` | re-runs: `e2e/runs/zone-bridge-3` (row 100: T1 = 96, all four signed it at the first final round; vault accepted, both releases paid; `ZoneBridgeLezCheckedTest` 4/4) `75e283f8`; `e2e/runs/zone-bridge-4` (row 101, `ZB_ROTATE=1`: five attestors, T1 = 148; `ZoneBridgeRotationLezCheckedTest` 6/6) `22d0842f` |
| M2: `return-leg.sh` passed no `--lez` | `96bf1a4e` | re-run: `e2e/runs/zone-bridge-rc3-2`, `lez/results/v0.4.0-rc3/return-leg-2` (rows 129, 130: T1 = 106 signed against rc3's own sequencer; `checkpoint_b_0_3_5` proven, the vault paid both releases, the deployed id and a replay refused; `ZoneBridgeRc3LezCheckedTest` 5/5) `22d0842f` |
| M3: the module's e2e scripts and README on the old interface (row 95's re-run: 9 PASS 14 FAIL) | `23bdf5f2`, `18bc700f` | `modules/scripts/attestor-e2e.sh` natively: **24 of 24** (`modules/results/fix-attestor-2026-11-22/attestor-e2e-native/summary.md`); `daemon_requests.py`'s 12 attestor requests: every expectation met; calls crate 3 of 3 |
| M4: "byte-identical" was false (only the 264 signed bytes match) | `ac1ec83f` (text), `f624f949` (`zone-attest.sh` writes a replay elsewhere) | replay into scratch: `encoded` equal x4; `node.first_block` 2 -> 30, `node.blocks_checked` 29 -> 1, `node.fork_digest` null (absent in the 11 Nov records); 3-of-4 input identical |
| Row 63's wording | this file | below |
| L1: README said a LEZ node marks `Finalized` exactly up to its final tip (#1053) | `81b008aa` | doc; the LIB filter's mutant fails 3 tests |
| L2: two guards no test covered | `f3c19199` | `lez_check.rs` `l2_a_first_read_never_roots_at_a_sibling_the_lez_node_does_not_hold` (kills `first-root-unchecked`), `l2_a_tip_inscribed_by_a_key_outside_k_is_passed_over` (kills `signer-not-in-k-ok`) |
| L3: key file mode and refusal cited as code | `4bca267e` | `files.rs` `l3_keygen_writes_an_owner_only_key_and_never_overwrites` |
| L4: module `attest`: a silent LEZ node `refused`; node mode without a state | `9427dc4f` | calls crate `a_live_attest_keeps_a_state_and_a_silent_lez_node_is_an_error` |
| L5: 16 junk ids could stop a first read | `b48579bf` | `l5_junk_ids_above_the_chain_never_stop_a_first_read` (kills `first-root-cap16`) |
| L6: fixed temporary file, `File::create` | `4bca267e` | the unique name: `files.rs` `l6_write_atomic_never_writes_through_a_planted_link`, `l6_two_writers_never_share_a_temporary_file` (kill `write-atomic-fixed-tmp`); `create_new` only since pass 3 (its L3) |
| L7: operator procedure; `getBlock` marked for removal | `81b008aa` (README "Operating it"); order test `4bca267e` | `files.rs` `l7_the_state_holds_the_tip_before_the_attestation_is_written` |
| L8: `live-node.sh` cannot be reproduced | `b303d3c8` | RESULTS §6, the script's header; row 63's text below |

Numbers of the three re-runs: `checkpoint/attestor/RESULTS.md` §10.1 (measured on the build box; each attestor signed
every round, none refused, the final round's tips equal at the first try).

## Pass 3: the second review

| Finding | Commit | Test or re-run that shows it |
|---|---|---|
| M1: this machine's absolute path in `e2e/runs/zone-bridge-4/net/proxy.log` (pubnet.py's traceback; `zone-bridge.sh` scrubbed only `logs/`) | record: `22d0842f` (the commit that added it, rewritten); tool: `fccf4508` (`scripts/scrub_records.py`; `zone-bridge.sh` ends every writing stage with it, `return-leg.sh` its hook); records: `5ab1d63e` (the three runs of pass 2 no longer name the worktree's or an agent's folder: path prefixes only) | `python3 -I scripts/test_scrub_records.py`: 28 checks (the review's line in a planted run, through `zone-bridge.sh records`); 15 mutants, all killed (the review's cause, `logs/` only, first); `zone-bridge.sh records` over zone-bridge-3, -4, -rc3-2: nothing left. Every blob the branch added since `5cf1e8e7` (archives opened): no path of this machine, no workspace or personal name (the scrub test's fake `/home/alice` aside); pass 2's commits still hold the relative `spike/ferrata-wt-fix-attestor/` in their record logs, as main's older records do (130 files) |
| L1: the README's `devnet.sh` command rewrote `runs/devnet-n3` | `edfd60e2` | the README's command in a copy: no tracked file changed; the six attestations' `encoded` equal to the records' (RESULTS §10.2) |
| L2: this file pointed to `trust.md` §3.3 wording it no longer held | this file | "Proposed status changes" below |
| L3: L6's `create_new` was code, not a test | `5f106898` | `files.rs` `l6_create_new_never_opens_what_is_at_the_temporary_name` (kills `write-atomic-no-create-new` and `cleanup-removes-foreign`) |
| L4: `attestor_refuse_cert` was met by a refusal of the read | `daf77a69` | natively: 12 of 12 met; the same request with a read that refuses: verdict met, expectation missed |
| Row 95's Logos Core claims | this file | row 95 below |
| N1: the calls crate's `attest` doc listed `state_file?` twice | `8468d8bd` | doc |
| N2-N4: the indexer `Block`'s fields; what §10.1 re-ran; `zone-bridge.sh answers` | `edfd60e2` | doc |
| N5: the recorded tip's signer guard untested | `c427cabc` | `lez_check.rs` `n5_the_signed_tip_is_not_signed_again_once_its_signer_left_k` (kills `recorded-tip-signer-unchecked`) |
| N6: a calls test skips quietly without `../upstream/lez-v0.4.0-rc1` | not changed | as main's older test of the same crate; this pass's box run shows none skipped (CI does not run this crate) |
| N7: the branch's base | this file | the note at the top |

Measured on the build box at the branch tip (pass 3): `cargo test -p ferrata-attestor --locked` **33 passed**
(attestor 12, files 5, io 2, lez_check 11, withdrawals 3), 33 with `cargo +1.88`; clippy `-D warnings` and rustfmt
clean; **25 of 25 mutants** fail at least one test (pass 2's 16, the second review's 8, `cleanup-removes-foreign`);
the module natively: calls crate 3 of 3, `attestor-e2e.sh` 24 of 24, `daemon_requests.py` 12 of 12. RESULTS §10.2.

## Proposed status changes

No row of `docs/requirements-status.md` changes status. **RFP-022 Sec.7** stays **in part** (the row is about the
documents, and neither is edited on this branch): under rule (B) the attestors, and each attestor's LEZ node, are
safety roles, while `docs/trust.md` §3.3 (on main and here) still says the rule "is beaten by `t` attestation keys
together with one channel key" and that "off-chain roles are liveness-only". Proposed wording for `trust.md` §3.3
and `bridge.md` §8.3:

> Under rule (B) each attestor signs a tip only when its own LEZ node holds it in LEZ's final tier (applied, not
> parked); a forged checkpoint needs `t` attestors (their keys, or the LEZ nodes they trust) together with one
> channel key. A LEZ node may show an adopted head block `Finalized` early (LEZ #1053); the attestor also requires
> the block's inscription to be final on its Logos Blockchain node. `lez` must be the attestor's own LEZ node:
> attestors that share one node share its lies.

## Proposed EVIDENCE text

- **Row 63**, replace with: "`ferrata_attestor`, the rule (B) signer a sequencer runs: reads its Logos Blockchain
  node's final tier (LIB, channel key set, immutable blocks) from the tip it signed last, follows the chain as LEZ folds
  it, and signs a tip only when its own LEZ node serves it `Finalized` and byte for byte as inscribed, at or below the
  LIB (a block LEZ parked is never signed; a status shown early, LEZ #1053, is not signed before Bedrock finality); the
  tip's inscription is checked as the checkpoint program checks `T`'s (withdrawal transactions included); garbage and
  repeated inscriptions are passed over; the state is written atomically before publishing; never signs two histories,
  across restarts; the collector keeps valid attestations and refuses below `t(n)`" | "33 tests (the lab node's real
  answers; a forged turn; 4 x 150 random rounds with restarts; rounds over HTTP against a node stub; the CLI's key file
  and write order; the temporary file's `create_new`); 25 of 25 mutants (each removes a guard or undoes a fix) fail a
  test; beside live LEZ sequencers (rc1 zones of four and five, an rc3
  zone of one): every round signed, none refused (zone-bridge-3, -4, -rc3-2); `zone-attest.sh` replayed: the zone's
  four attestations sign the same 264 bytes as the recorded ones (their `node` facts now read `first_block` 30,
  `blocks_checked` 1), the 3-of-4 input identical to the proven one. The run against a copy of the lab node (§6)
  predates the LEZ check and cannot be repeated as recorded" | build box | none (native) |
  `cd checkpoint && cargo test -p ferrata-attestor --locked` | `checkpoint/attestor/RESULTS.md` §1, §2, §10, §10.1,
  §10.2.
- **Row 95**, replace with (its Logos Core and live-zone results are the module's before the LEZ check, which
  signed from its Bedrock node's view alone; after the check it is shown natively on the zone only):
  "`ferrata_attestor_module` doing the attestor's job: on row 65's zone, and beside each sequencer of a running one
  (the running zone and Logos Core: the module of 13 Nov, before the LEZ check)" | "13 Nov, before the LEZ check: on
  the zone's saved node answers: tier, base, the four sequencers' certificates and attestations each equal to that
  run's, the collector's 3 of 4 = byte for byte the input proven and accepted in row 65 (sha256 `f376ad54…c575`), 4 of
  4, 2 of 4 refused, three refusals to sign (a lower tip, another key's certificate, another vault), the module's loop
  against a node served over HTTP: 23/23 natively, through the dev and the portable Logos Core and installed from the
  local catalog. A second run of the zone (four LEZ v0.4.0-rc1 sequencers, the lab bundle, 300 s) with four portable
  Logos Cores, one per sequencer, each module's key certified by its sequencer's channel key: each loop read the node
  every 15 s, 22 rounds, signed 19 rising tips up to the final tip 29, no error; 3 of 4 collected and accepted by the
  checkpoint rule natively (n = 4, t = 3), 4 of 4 accepted, 2 of 4 refused: 13/13. 22 Nov, after the LEZ check
  (natively; the zone's inscribed blocks stood in as the LEZ node's final blocks): `attestor-e2e.sh` 24/24 (one check
  added: a signed block of another history refused), the daemon requests' 12 attestor expectations met (the
  certificate refusal word for word), the calls crate 3/3; through the dev and the portable Logos Core, from the
  catalog and beside a running zone (`attestor-zone-live.sh`, updated): not re-run with the LEZ check" | "Modal build
  box, the zone on loopback; commit `2dbad7c` (from the catalog and the running zone: tree `905731a`, the same module
  sources); 22 Nov: branch `fix-attestor` (`23bdf5f2`, `daf77a69`)" | none (the live input checked natively, not
  proven) | `modules/scripts/attestor-e2e.sh OUT`; `modules/scripts/attestor-zone-live.sh BUNDLE OUT` |
  `modules/RESULTS.md` (the 13 Nov and 22 Nov sections), `modules/results/merged-2026-11-13/attestor-zone-live/`,
  `modules/results/fix-attestor-2026-11-22/attestor-e2e-native/summary.md` (linked from `docs/` as the row's links are
  now).
- **Rows 100, 101, 129, 130**: add the re-run beside each (their 13 and 18 Nov numbers stand): row 100
  `e2e/runs/zone-bridge-3`, row 101 `e2e/runs/zone-bridge-4`, rows 129-130 `e2e/runs/zone-bridge-rc3-2` (+
  `lez/results/v0.4.0-rc3/return-leg-2`), each "with LEZ-checked attestors", numbers in RESULTS §10.1; re-check commands
  as the rows' with the new run dirs and `ZoneBridge{LezChecked,RotationLezChecked,Rc3LezChecked}Test`.
- **New row**: "The rule (B) attestor's reviews (21 and 22 Nov): H-2, M-3, M-4, M7 (pass 1); M1-M4, L1-L8 (pass 2);
  the second review's M1, L1-L4, N1-N5 (pass 3) fixed" | "probe of `5cf1e8e7`: F signed; after: F never signed; 33
  tests, 25 mutants killed; module e2e 24/24; the bridge zones re-run with LEZ-checked attestors (rc1 x2, rc3); every
  file of a run's records scrubbed of the machine's paths and checked, a stage refused otherwise (28 checks, 15
  mutants killed)" | build box (the scrub test: laptop) | none | `cargo test -p ferrata-attestor --locked`;
  `python3 -I scripts/test_scrub_records.py` | RESULTS §10, §10.1, §10.2.
