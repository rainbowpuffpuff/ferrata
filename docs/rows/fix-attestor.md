# Stream fix-attestor: the rule (B) attestor's review findings (for the integrator)

Branch `fix-attestor` from `5cf1e8e7`. Findings fixed: H-2 (CK-1), M-3 (CK-2), M-4 (CK-3) of
`review-code-bridge`; M7, L7, L8 of `review-code-modules` (L-6/CK-8 is L8). No guest or program changed: no
image id moves (`checkpoint/attestor` and the module's calls crate are host code; `checkpoint-core` is
untouched).

| Finding | Commit | Test that shows it (all in `checkpoint/`, `cargo test -p ferrata-attestor --locked`) |
|---|---|---|
| M-3: a block with an atomic withdrawal stopped every attestor | `9a4ea7ff` | `attestor/tests/withdrawals.rs`: `[0x11, 0x14, 0x13, 0x00]` block signed and proven as T by the program from the same bytes; an inscription that is not the first op too (`8a51ac40`) |
| H-2: the attestor signed blocks LEZ parked | `8a51ac40` (+ module `81a75215`) | `attestor/tests/lez_check.rs`: `h2_a_block_that_lez_parked_is_never_signed_and_the_forgery_needs_it`, `h2_the_lez_node_must_serve_the_inscribed_bytes_with_status_finalized`, `h2_a_lez_node_that_does_not_hold_the_signed_tip_refuses` |
| M-4: one garbage or repeated inscription stopped every attestor | `8a51ac40` | `lez_check.rs`: `m4_garbage_repeats_and_unreadable_ops_never_halt` (ids 0 and `u64::MAX` included) |
| M7: whole-chain reads; a narrowed read switched the two-histories guard off | `8a51ac40` (+ module `81a75215`) | `lez_check.rs`: `m7_the_read_starts_at_the_signed_tip_and_the_state_round_trips`, `m7_never_two_histories_across_restarts` (4 seeds x 150 rounds); `io.rs` (rounds over curl: round 2 reads one window from the signed tip's slot); `attestor.rs`: `the_attestor_never_signs_two_histories` (a signed tip below the run is refused) |
| L7: attestation published before the state, non-atomic | `8a51ac40`, `81a75215` | state written atomically before `--out`/publish (`round`); `m7_..._state_round_trips` |
| L8 / L-6: key file mode | `8a51ac40` | `keygen` writes 0600 with `create_new` (`write_secret`) |

Before/after (measured on the build box): the unchanged code signed the forged block F (`59041fd0…205e`),
refused tiers with 0x14 or a garbage payload, and signed 202 past a non-ancestor signed 200; after, 24 tests
pass, clippy `-D warnings` is clean, and each of four guard mutants is killed by at least one test.

## Proposed status changes

No row of `docs/requirements-status.md` changes status. The fix restores a claim two rows rely on:

- **RFP-022 Sec.7** (in part; "off-chain roles are liveness-only", `docs/trust.md`): with H-2 open, a
  watcher was a safety role under rule (B) (one channel key plus honest attestors forged a checkpoint).
  Now each attestor signs only what its own LEZ node holds final. Proposed `trust.md` §3.3 and
  `bridge.md` §8.3 wording: "Under rule (B) each attestor signs a tip only when its own LEZ node holds it
  in LEZ's final tier (applied, not parked); a forged checkpoint needs `t` attestors (their keys, or the LEZ
  nodes they trust) together with one channel key. A LEZ node may show an adopted head block `Finalized`
  early (LEZ #1053); the attestor also requires the block's inscription to be final on its Logos
  Blockchain node." Status stays **in part** (the row is about the documents).
- **EVIDENCE row 95** (`ferrata_attestor_module` doing the attestor's job): its commands
  `modules/scripts/attestor-e2e.sh` and `attestor-zone-live.sh` now need a LEZ node (`lez` for the daemon
  and node-mode `attest`) or saved LEZ answers (`lez_dir`). Not changed on this branch (not this stream's
  files): until they pass one, row 95's re-run fails. The calls crate's own test passes
  (`the_zone_replayed_gives_the_proven_input`, with the zone's blocks stood in as final).

## Proposed EVIDENCE text

Row 63, replace the description and numbers with: "`ferrata_attestor`, the rule (B) signer a sequencer
runs: reads its Logos Blockchain node's final tier (LIB, channel key set, immutable blocks) from the tip it
signed last, follows the chain as LEZ folds it, and signs a tip only when its own LEZ node serves it
`Finalized` and byte for byte as inscribed (a block LEZ parked is never signed); the tip's inscription is
checked as the checkpoint program checks `T`'s (withdrawal transactions included); garbage and repeated
inscriptions are passed over; the state is written atomically before publishing; never signs two
histories, across restarts; the collector keeps valid attestations and refuses below `t(n)`" | "24 tests
(the lab node's real answers; a forged turn; 4 x 150 random rounds with restarts; rounds over HTTP against
a node stub); `devnet.sh` and `zone-attest.sh` re-run: the zone's four attestations byte-identical to the
recorded ones, the 3-of-4 input identical to the proven one" | build box | none (native) |
`cd checkpoint && cargo test -p ferrata-attestor --locked` | `checkpoint/attestor/RESULTS.md` §1, §2, §10.

New row (review fixes): "Review of 21 Nov, rule (B) attestor: H-2 (a turn holder's block that LEZ parks
was signed by every honest attestor; `t` signatures of it make `checkpoint_b` accept it), M-3, M-4, M7
fixed" | "probe of `5cf1e8e7`: F signed; after: F never signed, the honest 201 signed once final, the
program accepts T = F only with forced signatures; 4 mutants killed" | build box | none |
`cargo test -p ferrata-attestor --locked --test lez_check` | `checkpoint/attestor/RESULTS.md` §10.
