# Stream fix-updater-4: proposed changes for the integrator

Branch `fix-updater-4`, rebased onto public main `8c38d723`. It fixes the updater's last known hold by one lying
light-client source, across two reviews:
- review-fix-updater3b M1: a source re-dating a real update that adopts the next committee to slots that have started, a
  new slot each round, chosen with the updater's public deferral tag, had the updater defer that update's `Open` every
  round: nothing sent in the 113 rounds the review ran after the rotation, the source never demoted.
- review-fix-updater4 M1: the same source also sending each copy's `Open` (or `Open` and `Miller`) itself, which anyone
  may, held the instance again: the updater's next step was a later one, still counted by the job (nothing sent in 107
  to 115 rounds).
- review-fix-updater4 L2: with `finish_own_jobs = false` and honest sources serving their newest update, an `Open` about
  every third round.
- review-fix-updater4 L1: a stale figure; review-fix-updater3b L2 and L3 (texts) and the nits.
- review-fix-updater4 round 2 (texts only): M3, the bound holds on each state of the instance, and a party applying
  older real updates itself restarts the turns at each; L3, what duplicate jobs cost honest updaters with live sources;
  two nits.

No row moves. **RFP-021 F.12 stays shown** on the RFP's wording: for beacon providers it rests on replacement, as
`docs/trust.md` §2.4 says; the passages below replace main's "Not covered … A fix is in progress" with what the code now
does. **RFP-021 R.10** (submission timing does not concentrate cost) is unchanged: the A3 gate's `results/gate.txt` is
byte for byte as committed.

Files changed, all of this stream: `lez/updater/src/{updater,config}.rs`, `lez/updater/tests/redating.rs` (new),
`lez/updater/dev/{mutants,test_mutants}.py`, `lez/updater/README.md`, and one shared document, minimally:
`docs/trust.md` §2.4 (the beacon providers' row). No program, guest or wire code: **no image id moves**, nothing deployed
changes. No public item changed; the updater module and the sequencer backend pick the change up when rebuilt.

## The fix

The deferral (`policy.defer_window`, ADR 0011) spreads who sends a step among updaters: each waits `H(key, job, step) mod
defer_window` rounds from the first round it saw the step due. A job's id hashes the whole update, `signature_slot`
included, which the signature does not cover; any beacon node can build another real update of the period from each of
its blocks; and anyone may open a job and send its steps. Counted by the job, a source serving a new copy each round, or
opening each copy itself, had the updater count again each round. Now:
1. A step of a job this updater carries (it sent a step of it) counts by the job, as before: nobody can change that
   job, and what others send of it only moves it on.
2. An `Open`, and a step of any other job, count from the first round a step of that kind was due on the instance's
   state, whatever job it is for: one turn per kind of step and state, at most `defer_window - 1` rounds.
3. After this updater sends a step on a state, the next of that kind there waits a whole turn (`defer_window - 1`
   rounds), not a fresh one drawn from each round's job (L2's cause).

A source can then only present jobs at another step, each kind's turn running once. The updater's first step on a state
goes out within `K × (defer_window - 1)` rounds of the first round one was due (`K = 3 + adoption_parts` kinds), and the
job it then carries takes each of its other steps within `defer_window` rounds. So, on each state of the instance, an
update adopting the next committee is applied at most `K × (defer_window - 1) + 1 + (K - 1) × defer_window` rounds after
the first round it was due there: **68** at the defaults (`defer_window` 8, two parts). Derived; measured below. The
turns start again whenever the state changes, and anyone may change it by applying a real update (M3 of round 2, below).

With one candidate per state (honest sources serving one update each), every wait is as before: `results/gate.txt` is
unchanged.

## Per finding

Measured on the Modal build box (16 cores, gVisor), LEZ v0.4.0-rc1's state machine in process, `RISC0_DEV_MODE=1`, the
lab's recorded mainnet data, `defer_window` 8 where named. "Old code" is `06ed8bc1`'s `updater.rs` and `config.rs`
with the new tests.

| Finding | Commit | Test (`lez/updater/tests/redating.rs`) | Measured at the fix | Old code / mutant |
|---|---|---|---|---|
| review-fix-updater4 M1: the source opens each copy itself | `88cdb90a` | `opener_a_new_opened_copy_every_turn_does_not_hold_the_instance`, `opener_a_new_opened_copy_each_round_does_not_hold_the_instance`, `opener_sending_open_and_miller_does_not_hold_the_instance` | rotated → adopted in rounds 5 → 21, 13 → 39, 9 → 32 (16, 26, 23 rounds); entry 20 applied; nothing refused; the source not demoted; `/health` `ok` | old code: never adopted, 0 sends after the rotation, 115, 107, 111 deferrals; 17, 107, 222 attacker transactions. `m6` (the step of a job not carried counted by the job) red |
| the bound | `88cdb90a` | `opener_cycling_through_every_kind_of_step_is_bounded` (each kind of step's turn used up in turn, then a job to carry from its `Open`, its later steps chosen to wait longest) | rotated in round 16, adopted in round 82: **66** rounds of the bound's 68; 10 attacker transactions | the same at the old code (its copies change only between kinds, so counting by the job gives the same turns): a measurement of the bound, not a regression test |
| review-fix-updater3b M1 (kept) | `87e7813b` | `started_slots_an_adopting_update_re_dated_each_round_does_not_hold_the_instance`, `started_slots_with_finish_own_jobs_off_an_open_is_sent_once_per_turn` | 5 → 35, the `Open` deferred 7 rounds; `finish_own_jobs = false`: 6 `Open`s of the copies in 52 rounds | `m4` red; `m4reset` (no new turn after a send) red |
| review-fix-updater4 L2: `finish_own_jobs = false`, honest newest-only sources | `88cdb90a` | `newest_only_sources_with_finish_own_jobs_off_open_once_per_turn` | 8 `Open`s in 60 rounds (at most one per 8), fees 141,609,008 | old code: 16 `Open`s, 283,649,752 (the review: 21 with its names); `m7` (a fresh turn after a send) red |
| review-fix-updater3 M1's variants (kept) | `87e7813b` | `earlier_*` (3) | adopted in round 8 with `defer_window` 1 and in the two-liar cases; with `defer_window` 8, rotated in 15 and 10, adopted in 33 and 28 (the review's schedule, SplitMix64) | `m1roll`, `m1` red |
| review-fix-updater4 L1: stale "round 48, rotated in 17" | the docs commit | — | the README's `redating.rs` row gives 33 (rotated 15) and 28 (rotated 10) | — |
| review-fix-updater3b L1: a sequencer clock behind the updater's | not fixed | the review's `v5` (not committed) | stated in the README's limits: with the sequencer 2 s behind and rounds half a second into a slot, `defer_window` 1: 6 refused, 71 of 80 rounds paused; `defer_window` 8: 5 refused, 3 breaker trips, 265,762,928 charged, no adoption in 80 rounds (the same refusals and charge as at `06ed8bc1`); with the clocks equal, adopted in round 31; two honest sources with the lag, round 30 | — |
| review-fix-updater3b L2, L3; nits | `1cf1570e`, the docs commit | `dev/test_mutants.py` | as in the previous round (the clock limit's text; m1's text; the process group killed on a stop, 3 of 3) | old kill: 3 of 3 fail |
| round 2 M3: the bound holds on each state; a party applying older real updates restarts the turns | the round-2 docs commit | — (text; figures from review-fix-updater4's round 2 run 4, `r5_attacks.rs`, not committed) | "on each state of the instance" wherever the bound is given, and the reset caveat (README items 6, 8 and its limits, `docs/trust.md` §2.4, F.12, row N, the operator guide): the party applying the record's entries 14 to 20 itself, with the lying source using up the turns on each state: adopted 475 rounds after the rotation (`/health` not `ok` in 4 rounds); 317 with each reset after the updater's first step; with no lying source, 165, the same on main; within one state 67 with every slot searched (the committed test: 66) | — |
| round 2 L3: duplicate jobs with live sources | the round-2 docs commit | — (text; the same review's run 4, not committed) | README item 8: two updaters in one slot pay 51% more per applied update than on main (2 `Open`s per update against 1), three 28% more (2.25 against 1.5), about the same otherwise; every update applied within its 15 rounds with two or three (on main 2 to 4 of 4); no transaction refused | — |
| round 2 nits | the round-2 docs commit | — | F.12: "16 to 26 with the openers, 30 with the started slots"; README item 8: honest updaters collide on a step when "their turns for it end in the same round"; row N dated 22–23 Nov 2026 | — |

Why L1 is not fixed here: a margin after an update's window opens before it counts as sendable (the review's fix) costs
honest liveness, since sources serving only their newest finality update serve one signed in the current slot, and an
updater whose rounds fall under the margin into each slot would then never find one it may send. A margin learned from
the chain's refusals (no cost while the clocks agree) is the next piece, if wanted.

Measured at the fix (run 5, the code of `88cdb90a`): the whole suite **92 passed, 0 failed** (11 library, 3 `bounds`, 8
`config`, 1 `daemon_clock`, 4 `diversity_limits`, 8 `every_party`, 8 `health`, 2 `jobs`, 13 `recorded`, 10 `redating`,
16 `refusals`, 1 `replacement`, 7 `serve`); `results/gate.txt` byte for byte as committed; clippy `-D warnings` with and
without `harness` clean; fmt clean. Mutants: `m4`, `m6`, `m1roll`, `m1`, `m1near`, `u2`, `u3`, `u5`, `u4f`, `u6`, and
`m1roll` and `m1` on the `earlier_` tests, each red, the sources restored after each; `m4reset` and `m7` (their pattern
fixed after rustfmt rewrapped the line they name; run 6): `m4reset` red on both `finish_own_jobs = false` tests (45
`Open`s of the copies in 52 rounds; 56 in 60 with newest-only sources), `m7` red on the newest-only one (16, as at
`06ed8bc1`). `every_party` 8 of 8 with the edited `docs/trust.md` (run 6). The reviews' attack files (not committed):
review-fix-updater4's V6 hold tests fail (the hold is broken: adopted in rounds 21, 39 and 32), its fix checks pass
(rotated 9, adopted 32; rotated 15, adopted 35), the control adopts in round 40; its V9 is as at `06ed8bc1` (one
updater: 4 of 4 updates; two updaters 6 s apart: 4 of 4, 8 `Open`s; 11 s apart: 3 of 4), and with `finish_own_jobs =
false` 8 `Open`s (the review: 21 at `06ed8bc1`); review-fix-updater3b's v4 hold test fails (12 → 38), its fix check
passes (16 → 30); the first review's `review_fixcheck.rs` 6 of 6. Not run: the updater module's native build,
`updater-e2e.sh`, anything on a sequencer; `lez/updater/sequencer` was not rebuilt (no public item changed).

## Proposed texts (against main `8c38d723`)

### RFP-021 F.12: `docs/requirements-status.md` row F.12 and `docs/traceability.md` row F.12 (the same passage)

Replace, in both, from "Not covered: a source that re-dates a real update adopting the next committee" to "A fix is in
progress." with:

> A source re-dating such an update to slots that have started, a new copy each round chosen with the updater's deferral
> tag (computed from its public name and payer account), even opening each copy's job and sending its steps itself
> (anyone may), delays the instance and does not stop it while the chain's clock is the updater's: every step the
> updater sends of a job it does not carry waits a turn counted by the instance's state, one per kind of step, not by
> the job (review-fix-updater3b M1, review-fix-updater4 M1), so on each state of the instance an update adopting the
> next committee is applied at most 68 rounds after it was first due there, at the default `defer_window` 8 (measured 66
> with the strongest attacker known, 16 to 26 with the openers and 30 with the started slots,
> `lez/updater/tests/redating.rs`; 67 by review-fix-updater4 with every slot searched; EVIDENCE row N); the source is
> not demoted. The turns start again with each new state: a party that also applies older real updates itself (anyone
> may, paying their jobs) can postpone the adoption, and keep the instance behind the newest update, for as long as it
> has such updates to apply, the instance moving at each so that the stall alarm need not fire (measured by
> review-fix-updater4, not committed: adopted 475 rounds after the rotation over seven such updates with the source
> using up the turns on each state, 165 with no lying source, as on main). With a sequencer whose clock lags the
> updater's, a source re-dating to the slot that has just started can make `Open`s the chain refuses, and pause sending,
> while that lasts (`lez/updater/README.md`, "Not built, not tested").

The sentence after it ("An attestation is still produced eventually by an updater whose sources exclude the liar, or by
this one once the source is removed (replacement, `docs/trust.md` §2.4).") stays: F.12 rests on replacement.

### EVIDENCE row 223 (title)

Replace "; not covered: a re-dated update adopting the next committee, deferred every round with the updater's public
tag, stalls the updater until the source is removed (review-fix-updater3b M1, a fix in progress)" with "; a re-dated
update adopting the next committee, deferred every round with the updater's public tag, stalled the updater until the
source was removed (review-fix-updater3b M1; fixed with a bound: row N)".

### New EVIDENCE row N

| N | **A light-client source re-dating an update that adopts the next committee to slots that have started, opening each copy's job itself or not, delays the instance by at most 68 rounds on each state of the instance at the default `defer_window`, with the chain's clock the updater's; a party applying older real updates itself restarts the turns at each (RFP-021 F.12; review-fix-updater3b M1, review-fix-updater4 M1 and L2)**: an `Open`, and a step of a job the updater does not carry, wait their turn counted by the instance's state, one turn per kind of step, not by the job (whose id hashes `signature_slot`, which the signature does not cover, and which anyone may open and take on); after a step is sent on a state, the next of its kind there waits a whole turn | `defer_window` 8, the store lacking its next committee, the source serving copies of a real update adopting it, chosen with the updater's public tag: re-dated to a new started slot each round, adopted 30 rounds after the rotation; opened by the source, a new copy every 7 rounds or each round, or opened and taken through `Miller`: adopted 16, 26 and 23 rounds after the rotation (was never adopted, nothing sent in 107 to 115 rounds); the source using up each kind of step's turn in turn, then leaving a job to carry: 66 rounds (the bound on one state: 68, `5 × 7 + 1 + 4 × 8`; 67 measured by review-fix-updater4 with every slot searched); entry 20 applied each time, nothing refused, the source not demoted; `finish_own_jobs = false` with honest newest-only sources: 8 `Open`s in 60 rounds (was 16; the review measured 21); review-fix-updater3 M1's variants kept; 92 of 92 tests; `results/gate.txt` unchanged. Measured by review-fix-updater4 (round 2, not committed): with a party applying the record's entries 14 to 20 itself, a new state each time, the next committee adopted 475 rounds after the rotation with the source using up the turns on each state (`/health` not `ok` in 4 rounds), 165 with no lying source, as on main; with live sources, two updaters in one slot paid 51% more per applied update than on main, three 28% more. Not fixed: a sequencer clock behind the updater's (review-fix-updater3b L1): with it 2 s behind, 5 refused, 3 breaker trips, no adoption in 80 rounds | in-process LEZ (as row 43), build box, 22–23 Nov 2026 | dev mode | `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --test redating -- --nocapture` (setup: `lez/RESULTS.md` §8); red: `python3 -I lez/updater/dev/mutants.py NAME` for `m4`, `m6`, `m4reset`, `m7`, and `m1roll` / `m1` with `--test redating earlier_` | `lez/updater/README.md` (items 6, 8; "Not built, not tested"; "Tested"); `docs/trust.md` §2.4 (linked as the other rows link them) |

### `docs/guides/operator-guide.md`

Line 211: replace "One lying source can still stall it (review-fix-updater3b M1: a re-dated update adopting the next
committee, deferred every round; `lez/updater/README.md`, "Not built, not tested"): when `/health` reports no progress
while the sources serve updates past the instance, remove or replace that source." with "One lying source can still
delay it, by at most 68 rounds on each state of the instance at the default `defer_window`; a party that also applies
older real updates itself starts a new state, and new turns, with each, so it can postpone the adoption of the next
committee for as long as it has such updates to apply (review-fix-updater3b M1, review-fix-updater4 M1 and its second
round; `lez/updater/README.md`, item 8 and "Not built, not tested"). If `/health` keeps reporting no progress while the
sources serve updates past the instance, or the instance keeps falling behind the newest update, remove or replace a
source that serves such copies."

§3.4, after "each (job, step) waits `H(key, job, step) mod W` rounds,": add "an `Open`, and a step of a job the daemon
does not carry, counted from the first round a step of that kind was due on the instance's state,".

### `docs/decisions/0011-updater-deferral.md`

Add under "Decision": "Amended 22 November 2026 (review-fix-updater3b M1, review-fix-updater4 M1 and L2): an `Open`, and
a step of a job the daemon has not sent a step of, count their wait from the first round a step of that kind was due on
the instance's state, whatever job it is for; after the daemon sends a step there, the next of that kind waits `W - 1`
rounds. The wait is still `H(key, job, step) mod W`. A job's id hashes the update's `signature_slot`, which its
signature does not cover, and anyone may open a job and send its steps, so counted per job a source serving new copies
restarted the count each round. A kind's turn runs once per state: a party applying older real updates itself starts new
states, and new turns, with each (review-fix-updater4, round 2)."

## What holds now, and what does not

- **Holds**, with the chain's clock the updater's (the in-process chain; a sequencer on the updater's machine; a remote
  one whose clock agrees with it): on each state of the instance, one lying light-client source delays an update by at
  most `K × (defer_window - 1) + 1 + (K - 1) × defer_window` rounds (68 at the defaults; measured 66 in the committed
  test, 67 by review-fix-updater4), however it re-dates, whatever jobs it opens and steps it sends, each update it
  delays a real one, applied. Re-dating within the 60 s margin to a slot not started, its copy ranks after every update
  that can be sent; beyond the margin it is refused and demoted.
- **Does not hold:**
  - The turns start again with each new state. A party that also applies older real updates itself (anyone may,
    paying their jobs) can postpone the adoption of the next committee, and keep the instance behind the newest
    update, for as long as it has such updates to apply; the instance moves at each one, so the stall alarm need not
    fire. Measured by review-fix-updater4 (round 2, not committed): adopted 475 rounds after the rotation with the
    lying source using up the turns on each state, 317 with each reset after the updater's first step, 165 with no lying
    source at all, as on main. Derived: up to about a sync-committee period, while the party has older updates.
  - It still holds back an update only it serves, by re-dating it within the margin, for as long as it alone serves it.
  - It is not demoted for re-dating to started slots (that is what an honest update carries), and the stall alarm
    (`6 × defer_window + 10` = 58 rounds) can fire before its update lands.
  - With a sequencer whose clock lags the updater's (L1): refused `Open`s and pauses while it re-dates to the slot that
    has just started.
  - With `finish_own_jobs = false` and sources whose best update changes every round: an `Open` at most once per
    `defer_window` rounds, no job finished.
  - With live sources (a best update changing every slot), several honest updaters open duplicate jobs: two in one
    slot pay 51% more per applied update than on main, three 28% more (review-fix-updater4, round 2, not committed).

## If `fix-audit` merges first

At `fix-audit` `c5ae606c`, RFP-021 F.12 is **in part**: its row in `docs/requirements-status.md` begins "In part: the
attack described below holds the reference updater on this tree; a fix (an `Open`'s turn counted by the instance's
state, with its tests) is under review, and the row returns to shown once it is merged.", and main's "A fix is in
progress." reads "A fix is under review, not on this tree." there ("…; until it is merged the row is in part." in
`docs/traceability.md`). Then: F.12 back to **shown** (one row from in part to shown in the RFP-021 counts); delete
that "In part: …" sentence; replace from "Not covered: a source that re-dates" through that "under review" sentence, in
both files, with the F.12 sentence above. `fix-audit` also edits `docs/trust.md` §2.4's Provers row, the row above the
beacon providers' row this branch edits: take both rows.
