# ADR 0032: A burn's tag is used once: the burn record keyed on the tag, the wallet's counters journalled

- **Date:** 22 November 2026
- **Status:** accepted; built, not deployed
- **Requirements:** RFP-022 R.2, R.4, R.5, R.7

## Context

Two reviews of 22 November 2026 (the final review's H1, the bridge code review's H-1 and its probe P1; neither is in
the repository: `docs/reviews/lost-burn-2026-11-22/README.md` restates the finding and keeps both probes) found this.
A wallet's `i`-th burn under a configuration has the tag `r_i = HMAC(kView, cid ‖ i)`; its release secret
`sOut = HMAC(kAuth, cid ‖ r)` and its release nullifier `NOut = H("ferrata/nf-out/v2" ‖ sOut)` (D13, ADR 0017) follow
from the tag alone, and the vault keys its replay protection on `NOut`. D13 also keyed the write-once burn record on
the hash of the whole note, `burn(cid, H(note))`, so that copying a pending burn's tag could not make it revert. The
two together left one guard against two notes with one tag: the wallet never reusing a counter. The `ferrata` tool
broke it twice: a second burn planned while a first was in flight (from another holder of the seed) took the same
counter, because the counter came from the burns already applied; and after an interrupted burn the next burn took
the interrupted burn's counter whatever its recipient, token or amount. Both notes were applied, both shared one
`NOut`, and the vault could release only one: the other denomination was destroyed on LEZ and stayed in escrow on
Ethereum. A client restored from the seed alone, a second device on the seed, or the Logos module beside the tool
cannot see another client's counters at all (RFP-022 R.5 forbids depending on local state for recovery), and R.4 asks
that later activity never invalidate an outstanding entitlement.

## Options considered

1. Keep the program as D13 has it and fix the wallet only: reserve counters locally. *Recorded:*
   docs/design/bridge.md ("The wallet advances `i` on every published burn"). Not enough alone: it
   cannot cover two clients of one seed, a restored client, or another implementation.
2. Bind the note into the release nullifier, `NOut = H("ferrata/nf-out/v3" ‖ sOut ‖ token ‖ amount ‖ COut)`, so that
   two notes with one tag are each releasable. *Reconstructed for contrast* (the bridge code review of 22 November 2026
   proposed it for the next vault; that review is not in the repository). Keeps D13's protection of a
   pending burn against a copied tag, but moves the release guest's image id, which every deployed vault pins as an
   immutable, with its consumers and real-proof fixtures. Under either option two notes sent with one tag publish
   `memo₁ ⊕ memo₂ = recipient₁ ⊕ recipient₂`: under this one both are applied, under option 3 the second is refused
   but stays in its block with its note (a public burn; a private burn proven before the tag was used), which is why
   the tool counts a refused tag as used (review L4 of 22 November 2026).
3. Key the burn record on the tag, `burn(cid, r)`: a second note with a used tag is refused and its whole transaction
   reverts; and journal the tool's counters (option 1) so that it never sends one. *Recorded:*
   docs/design/spec/bridge-spec-v2.md §5.1 ("The bridge program keeps a write-once account per `r`: a repeated `r` is
   refused"); bridge/RESULTS.md (the review of 11 November 2026: "the LEZ burn program rejects a repeated `r`, and the
   wallet scans before every burn").

## Decision

The third. The bridge program since milestone B1 (not deployed) records a burn at `burn(cid, r)`; the `ferrata` tool reserves a counter in
`<LEZ wallet home>/ferrata-burns.json` before it proves and sends a burn, never gives a reserved counter to another
burn, takes the same counter again only for the same burn (the same note: one of the two is applied), counts a refused
burn's tag as used, and reserves at most 10 counters past the zone's while none of them is on the zone (a recovery
from the seed alone looks 20 counters past the last one it finds used, a refused burn's tag counting as used, since
the review M1 of 22 November 2026: before it, 20 refused burns hid the next applied one; `ferrata release` and
`ferrata recover` also search 20 past the last counter the journal reserved).

## Rationale and trade-offs

Every applied burn of a configuration has its own tag, hence its own `sOut` and `NOut`, whichever client made it: a
reused counter costs a refused transaction's fee, never a denomination. The change is the program's alone (the
release guest, the vault and the checkpoint are unchanged; the record's size is unchanged), and the program since
milestone B1 is deployed nowhere, so no live deployment is affected: its image id moves (`docs/image-ids.md`), and it
ships with the RISC Zero 3.1 rebuild (`docs/design/risc0-3.1-migration.md`), which moves every LEZ guest again.

The cost is D13's protection of a pending burn: whoever sees a burn before it is applied can make it revert by
burning a denomination of its own, under any registered token, with the same tag first. The copy costs the copier
fees only: the release statement ties `sOut` to the leaf's `COut` and `NOut`, and the tag only to the leaf, so a copy
made with a `COut` of the copier's own secret and recipient is released like any burn (corrected after the review M4
of 22 November 2026; `bridge/crates/ferrata-bridge-core/tests/outbound.rs`, `a_tag_copier_can_release_its_own_copy`).
Each copy costs the owner the refused transaction's fee (the whole transaction reverts, its denomination whole), and
the owner burns again with its next counter. So any party with sight of pending transactions could make every burn
revert, at the price of fees and of tying up its own denominations until it releases them. On LEZ v0.4 only the
sequencer sees a transaction before it is applied, and it can drop the transaction anyway. If a LEZ network ever gives
third parties sight of pending transactions, that is the trigger for option 2 (with the next vault), which restores
the protection; it can be added beside this record without changing it.

## Sources

- docs/reviews/lost-burn-2026-11-22/README.md (the two probes, before and after)
- `lez/guests/bridge/src/main.rs` (`burn`), `lez/bridge-wire/src/lib.rs` (`burn_seed`, `burn_actor`)
- `lez/harness/tests/bridge_burn_tag.rs`; `cli/src/burns.rs`; `cli/tests/e2e.rs` (`counters_pairing_and_quiet_dry_runs`)
- `bridge/crates/ferrata-bridge-core/tests/outbound.rs` (`a_tag_copier_can_release_its_own_copy`); `cli/src/scan.rs`
  (`burns_past` and its tests: the recovery past refused tags and the journal's counters)
- `docs/design/bridge.md` §12.1 (the burn note); `docs/design/spec/bridge-spec-v3-changes.md` D13, D14
