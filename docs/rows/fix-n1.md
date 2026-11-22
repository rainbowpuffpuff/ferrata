# Stream fix-n1: the second bridge-cli review's N1, L-A and L-C, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. On `integration9`
from `4e0e34e2`: `ebbf4381` (N1, L-C), `588588e3` (L-A), then this file. Box runs: the Modal build box, RISC Zero dev
mode, on the tree of `588588e3`, 22 November 2026; laptop: the `links` job's checks. Nothing was pushed or sent. No
program, guest or image id changed; no dependency changed.

## What changed

- **N1 (Medium).** `ferrata recover` listed every open entry of the burn journal as "reserved and not on the zone".
  An entry stays open until the next burn resolves it, so after every burn the last burn was listed although the zone
  showed it applied (or refused), and a user who believed it could burn a second denomination. `flow::recover` now
  lists only the entries whose tag the zone does not show, applied or refused (the review's filter), as
  [`cli/README.md`](../../cli/README.md) already said it did.
- **L-C (Low).** `ferrata release` and `ferrata recover` searched `BURN_GAP` counters past the journal's next counter,
  unchecked: a damaged `next` near `u64::MAX` made them compute tags without end. A next counter more than
  `burns::MAX_SEARCH_AHEAD` (2^20) past the zone's next one is now an error naming the file, returned before any
  search, telling the user to move the file aside (nothing the zone shows needs it); the README says so. The review's
  other bound, the zone's next counter plus `MAX_AHEAD` (10), was not used: it would refuse for good the journals the
  M1 fix's journal half exists for (the unit test `burns_searched_past_the_journal_counters`: a journal's next 31
  against a history read whose next is 1, the refused transactions not shown). Up to the bound the search is as
  before.
- **L-A (Low).** [`docs/REVIEW-guests.md`](../REVIEW-guests.md)'s `burn` row no longer says "the owner loses nothing":
  the owner pays the refused transaction's fee, its denomination whole, and the copier can release its copy (ADR 0032).

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-022 R.5 | shown (EVIDENCE 23, 84, 89, 102, 224); the second review agreed, with caveat N1 (a false line in `ferrata recover`'s output) | **shown**, caveat N1 lifted | Recovery itself unchanged (rows 23, 84, 224). `ferrata recover`'s journal lines now match the zone: proposed EVIDENCE row 225 (e2e, the old listing failing the same check). |
| RFP-022 R.7 | in part (EVIDENCE 221) | in part (unchanged) | The burn leg's e2e rerun with the two checks added; the deposit and release legs still not interrupted, as the row says. |

Not done here (not in this stream's brief): the review's L-B (the mini-app burns with a counter that skips no refused
tag: `cli/examples/miniapp_devnet.rs:956`, one line), L-D (the Logos module's recovery counts applied tags only,
disclosed) and L-E (the review's zone-level runs of 21 and more refused burns, two journals and a lost journal, kept
in the e2e).

## Proposed EVIDENCE row 225

Links as in `docs/EVIDENCE.md` (relative to `docs/`): `cli/src/flow.rs`, `cli/tests/e2e.rs`, `cli/src/burns.rs`.

> | 225 | **`ferrata recover` lists only the journal's burns that the zone does not show, and a damaged journal stops
> `recover` and `release` at once (RFP-022 R.5, R.7; the second bridge-cli review's N1 and L-C), in CI's `roundtrip`
> job**: on anvil and the harness zone, after the e2e's burns, the tool's own journal still holds counter 18 open (an
> entry is resolved only by the next burn), applied on the zone: `flow::recover` with that journal prints no "burn
> journal:" line and finds the same burns and next counter (19) as the seed alone; with a journal of the seed made
> through the tool's store holding counters 17 (refused on the zone), 18 (applied) and 19 (not on the zone), one line,
> counter 19's; the same e2e with the listing of `4e0e34e2` fails at the first check ("counter 18 … is reserved and not
> on the zone"); that journal's next counter set to 2^64 − 1, `flow::recover` and `flow::release_burn` both fail,
> within 40 ms for the two calls, with an error naming the file, where they searched without end; the unit test reads a journal whose next counter
> is 2^20 past the zone's and refuses 2^20 + 1 past and 2^64 − 1 at once | e2e 1 passed (43.75 s); with the old listing
> 1 failed, as intended; lib 46 passed (`--lib burns` 12 of 12) | the build box | anvil (chain 1), dev mode, mock
> verifier | `cli/tests/e2e.sh`; `cd cli && cargo test --locked --lib burns` | `cli/src/flow.rs`, `cli/tests/e2e.rs`,
> `cli/src/burns.rs` |

## Amendments to existing rows

- EVIDENCE 221, result column: after "again 47.96 s at `55c0139e`" add "; 43.75 s at `588588e3`, with row 225's checks".
- EVIDENCE 224, result column: "CLI lib 45 passed (`--lib burns` 11 of 11)" becomes "CLI lib 45 passed (`--lib burns`
  11 of 11); 46 and 12 of 12 at `588588e3` (row 225's unit test added)".
- `docs/requirements-status.md` and `docs/traceability.md`, RFP-022 R.5: evidence "23, 84, 89, 102, 224" becomes
  "23, 84, 89, 102, 224, 225"; status and counts unchanged.

## Reruns (tree `588588e3`, measured)

| Step | Command | Result |
|---|---|---|
| journal tests | `cd cli && cargo test --locked --lib burns` | 12 passed, 34 filtered (588 s, cold build) |
| CLI lib | `cd cli && cargo test --locked --lib` | 46 passed |
| lib without `lez-wallet` | `cd cli && cargo +1.98.1 check --locked --no-default-features --lib` | exit 0 (65 s) |
| e2e | `cli/tests/e2e.sh` | 1 passed (43.75 s): `RECOVER (N1)` and `JOURNAL (L-C)` lines as above; `COUNTERS`, `PAIRING`, `QUIET` as row 221 |
| e2e, old listing | the same, `flow::recover` listing every open entry | 1 failed at "a burn the zone shows, listed as not on the zone" (counter 18) |
| roundtrip job | `e2e/ci/run.sh build`; `e2e/ci/run.sh`; `cargo test --locked --lib relayer`; `--test gasdrop` | build exit 0 (629 s); round trip passed (62 s); relayer 17 passed; gas drop 1 passed |
| links job (laptop) | `scripts/check-links.py` (CI's arguments), `check-guides.py`, `check-decisions.py` | 0 broken; 0 findings; 32 records, 0 findings |
