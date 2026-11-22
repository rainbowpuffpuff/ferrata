# Stream fix-updater-3: proposed changes for the integrator

Branch `fix-updater-3` (from `0ddb5168`, the tag `integ9-FINAL`). It fixes the updater's remaining liveness gap,
review-integ9d M1 (review-integ9c M4's bypass: one lying light-client source holding every update back for up to a
sync-committee period), the updater Lows the integrator listed (review-integ9c L-U2, L-U3, L-U4f, L-U5, L-U6, L-U7) and
review-integ9b N5 (the image-id compare). It moves no row's status: F.12 is shown and its text now matches the code
(below); S.7 and P.6 stay in part with corrected texts. It also answers review-integ9d L4 (the README's "Choice" item and
test counts).

Files changed, all of this stream: `lez/updater/src/{updater,serve}.rs`, `lez/updater/tests/{refusals,health,jobs,bounds,
serve}.rs`, `lez/updater/dev/mutants.py` (new), `lez/updater/README.md`, `scripts/reproduce-image-ids.sh`,
`scripts/reproduce/{guest_ids,compare_ids,test_image_id_tools}.py`, and three shared documents, minimally:
`docs/trust.md` §2.4 (the beacon providers' row and the paragraph after the table), `docs/guides/operator-guide.md`
(the updater paragraph: demotion, `down`, holds, the listener) and `docs/guides/verify.md` (what `build` builds and what
`compare` reports). No program, guest or wire code: **no image id moves**, nothing deployed changes. API: additions only
(`updater::{FUTURE_MARGIN_MS, MAX_UNJUDGED, state_digest}`, `SourceStats::last_refused`, `Metrics::unread_judged`,
`Held::state`; the metric `ferrata_updater_unread_refusals_judged_total{as}`; the file `state_dir/<chain>/held.tsv`).
`plan` now returns an error, not `Plan::Stuck`, for a round whose committees no source serves (the daemon logs it as any
failed round). The updater module (`modules/ferrata_updater_module`) and the sequencer backend pick this up when
rebuilt; `lez/updater/sequencer` compiles against it (`cargo check --locked --all-targets`, build box).

## Per finding

All measured on the Modal build box (16 cores, gVisor), LEZ v0.4.0-rc1's state machine in process, `RISC0_DEV_MODE=1`,
the lab's recorded mainnet data; red = the same test with the fix undone by `lez/updater/dev/mutants.py NAME` (the
mutants the box ran, byte for byte the edits that script makes).

| Finding | Commit | Test | Measured (green) | Red (fix undone) |
|---|---|---|---|---|
| 9d M1 / 9c M4: one source re-dating real updates holds every update back | `944456c4` | `lez/updater/tests/refusals.rs`: `a_re_dated_copy_of_an_update_does_not_hold_it_back`, `a_re_dated_update_only_one_source_serves_is_refused`, `a_re_dated_update_with_more_signers_does_not_hold_back_another` | a copy re-dated 600 slots, its source listed first: 4 rounds, 0 waiting, applied (15,626,400 → 15,627,008), the liar demoted, `/health` naming it ("starts 7188 s after the chain's clock"); an update only the liar serves, re-dated 600 slots, the other source one update behind: 4 rounds, 0 waiting, the other's update applied (15,626,400 → 15,627,008), the liar demoted; Lodestar's real update of finalized slot 15,682,752 (511 signers, from the client-diversity probe of 18 November) re-dated 600 slots against Nimbus's of the same header (510): Nimbus's applied in 4 rounds (one `Open`), the liar demoted; re-dated 24 s, or 60 s (the margin itself), past the chain's clock: Nimbus's applied in 4 rounds, the liar not demoted; 72 s past it: Nimbus's applied, the liar demoted; nothing refused in any | `m1`: the copy applied (the earlier tie-break) but its source not demoted; the update only the liar serves: 30 of 30 rounds waiting, the store unmoved; Lodestar's re-dated 600 slots: 30 of 30 waiting, unmoved, `/health` `down` by the stall alarm; `m1near` (the cases within the margin alone): 30 of 30 waiting, unmoved |
| 9c L-U2: rounds that cannot fetch the committees never reach `down` | `d32d4df0` | `health.rs` `committees_no_source_serves_take_it_down` | the store in period 1908, the only source serving the checkpoint's bootstrap and nothing else: `degraded` in rounds 1-4, `down` in 5-10, naming "the store's committees are not served by any source (period 1908)"; a source serving the period's update added: the next round `UpToDate`, no error | `u2`: the round returns `Stuck` (the test's first assertion fails) |
| 9c L-U3: a refusal followed by unreadable reads only holds | `251fc030` | `refusals.rs` `a_refusal_with_the_chain_unreadable_after_it_is_judged_once_it_reads`; `a_refusal_with_the_chain_unreadable_after_it_is_held_briefly` (updated) | a `Miller` refused three times, every read after each failing, no other party: sent in rounds 2, 6, 14, 30; each judged due the round after (failing sends 1, 2, 3, strikes 1, 2, 3), `/health` "did not land" from round 3, sending paused in rounds 15-23, the fourth applied in round 30, `ok`; 9 races lost, each followed by 5 failed reads: 9 judged races, holds dropped, 0 strikes, 0 failing sends | `u3`: no failing send, no strike, `/health` `ok` throughout; the race test: nothing judged |
| 9c L-U4f: the per-host bound counts each IPv6 address as a host | `8c56e70e` | library `serve::tests::ipv6_clients_count_by_their_64_network` | five addresses of one /64: four full places and a quick one; another /64: a full one; IPv4-mapped counted as IPv4; `::1` uncounted | `u4f`: the fifth address got a full place |
| 9c L-U5: holds for good in memory (4,096 rounds, and lost at restart) | `bbb01063` | `refusals.rs` `a_hold_for_good_outlives_its_rounds_and_a_restart` | the initialization's `Adopt` out of gas: 1 refusal in 4,106 rounds, `held.tsv` one line; restarted on the same state directory: 1 refusal after ten more rounds, `/health` naming the gas limit; raised: applied, `held.tsv` empty | `u5`: 2 refusals in 4,106 rounds |
| 9c L-U6: RSS read after the free | `45f82b48` | `bounds.rs` `huge_answers_are_refused_without_growing_the_process`; `serve.rs` `request_heads_are_bounded` | the test server pushed 4,194,304 bytes of a 256 MiB answer before curl was stopped (asserted < 64 MiB); 1,769,472 bytes of 64 MiB offered got through before the listener ended the connection (asserted < 32 MiB); `VmHWM` checked where the kernel reports it (gVisor does not: on the box the byte counts are the check) | `u6` (the reader's bound and curl's `--max-filesize` both removed): the server pushed all 268,435,456 bytes, the test fails |
| 9c L-U7: row 200's figures printed, weaker bounds asserted | `671138c8` | `jobs.rs`, `refusals.rs` | asserted exactly: 5 updates, 5 `Miller`s, 5 `Finish`es; 9 races (both race tests); 5 refusals in 40 rounds | — (assertions only) |
| 9b N5: zero ids compared as differing; four guests not built | `6479be7f` | `scripts/reproduce/test_image_id_tools.py` (3 new) | `python3 -I scripts/reproduce/test_image_id_tools.py`: 21 of 21 (laptop); an all-zero id is `NOT BUILT` (no row, no conflict beside a real build); `compare` reports such a row `NOT BUILT` and names every later-section guest the rebuild lacks; `build` builds `methods-program-class`, `methods-statements`, `methods-rebate` and `methods-0-3-5` too | the 3 new tests on the scripts of `0ddb5168`: 3 failures |
| 9d L4: README "Choice" and counts | the docs commit | — | the README's item 6 gives the new order and the margin; "Tested" gives 80 tests and the per-file counts | — |

The whole suite at the tip: `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked`
(`RISC0_HOME` the LEZ guest toolchain, `lez/RESULTS.md` §8): **80 passed, 0 failed** (11 library, 3 `bounds`, 8 `config`,
1 `daemon_clock`, 4 `diversity_limits`, 8 `every_party`, 8 `health`, 2 `jobs`, 13 `recorded`, 14 `refusals`, 1
`replacement`, 7 `serve`); clippy `-D warnings` with and without `harness` clean; `cargo fmt --check` clean; the A3
gate's `results/gate.txt` byte for byte as committed (246 updates, 0.0752, 0.4841 / 0.5159). Each intermediate commit
(`944456c4` .. `8c56e70e`) builds and passes the tests it adds (build box, run 3). Not run on this branch: the updater
module's native build and `updater-e2e.sh`, the sequencer e2e runs, the full `reproduce-image-ids.sh build` (only its
Python halves, offline).

## Proposed texts

(Links are given as plain paths; in the target files they are links as usual.)

**RFP-021 F.12** (`docs/requirements-status.md` and `docs/traceability.md`, the same sentence in both; status stays
shown). Replace "One light-client source re-dating its copies of real updates (a later `signature_slot`, which the
signature does not cover) no longer holds back the copy other sources serve: the earliest is taken
(`lez/updater/tests/refusals.rs`); a copy only it serves still waits for its later window, up to a sync-committee
period, and the source is not demoted." with:

> One light-client source re-dating real updates (a later `signature_slot`, which the signature does not cover), copies
> or another real update of the same finalized header with more signers, no longer holds updates back: an update signed
> more than 60 s past the chain's clock is refused and its source demoted, and of one finalized header the updater takes
> the earliest-signed, so a re-dated update delays none that another source serves and one only it serves by at most
> 60 s (`lez/updater/tests/refusals.rs`, three tests, each red with the fix undone; EVIDENCE row N below).

(In `requirements-status.md` add the new row's number to the Evidence column.)

**RFP-021 S.7** (both files; stays in part). Delete "; and a round that stops before it reads the sources' head, as
when the committees cannot be fetched, stays `degraded`: not counted toward `down`" (the parenthesis then ends after
"naming what to change"). In `traceability.md` also replace "five rounds in a row that end in an error (a chain that
answers nothing)" with "five rounds in a row that end in an error (a chain that answers nothing, or committees no
source serves)", and after "five sends in a row that do not land" add "(a refusal after which the chain could not be
read counts once the next round reads its step still due)".

**RFP-021 P.6** (`traceability.md`; stays in part). Replace "four per host other than loopback" with "four per host
other than loopback (an IPv4 address or an IPv6 /64)".

**EVIDENCE row 200**, three corrections the code now makes necessary:
- "(for good, in memory: 4,096 rounds, about 13.7 h at 12 s rounds, and until a restart, when the program refuses it or
  it runs out of gas, ..." → "(for good, kept across restarts until the instance moves, when the program refuses it or
  it runs out of gas, ..." (row N);
- "a refusal whose re-reads fail only holding the step, neither a strike nor a failing send" → "a refusal whose
  re-reads fail held and judged once the chain reads again: a lost race, or a failing send and a strike" (row N);
- delete "; and a round that stops before it reads the sources' head, as when the committees cannot be fetched, stays
  `degraded`: not counted toward `down`" and "The figures are those the tests print; they assert weaker bounds." (the
  tests assert them now, row N); "a 256 MiB answer cut at 1 MiB, RSS +0.9 MiB" → "a 256 MiB answer cut at 1 MiB, 4 MiB of
  it pushed before curl was stopped".

**A new EVIDENCE row (N)**:

| # | What | Result | Where | Proof | Command | Docs |
|---|---|---|---|---|---|---|
| N | **One lying light-client source no longer holds updates back; the updater's Lows (RFP-021 F.12, S.7, R.9, P.6)**: an update signed more than 60 s past the chain's clock refused and its source demoted; of one finalized header the earliest-signed taken; rounds that cannot fetch the committees counted towards `down`; a refusal whose re-reads fail judged once the chain reads again; holds for good kept across restarts until the instance moves; an IPv6 client counted by its /64; the bound tests counting the bytes the peer pushed; row 200's figures asserted | a copy re-dated 600 slots, listed first: 0 of 4 rounds waiting, the liar demoted; an update only the liar serves: 0 waiting, the other source's applied (was 30 of 30 waiting, the store unmoved); Lodestar's real update (511 signers) re-dated against Nimbus's of the same finalized header (510): 0 waiting, Nimbus's applied, re-dated two hours or 72 s past the clock the liar demoted, 24 s or 60 s not (was 30 of 30 waiting); committees no source serves: `down` from round 5 (was `degraded` for good); a `Miller` refused three times with the chain unreadable after each: failing sends and strikes 1, 2, 3, sending paused, the fourth applied (was neither, `/health` `ok`); 9 lost races so followed: judged races, 0 strikes; a hold for good: 1 refusal in 4,106 rounds and after a restart (was 2); five addresses of one IPv6 /64: four places (was five); a reader taking the answer whole: 268,435,456 bytes pushed, the test fails (bounded: 4,194,304); 80 of 80 tests, each new one red with its fix undone | in-process LEZ (as row 43), build box, 22 Nov 2026 | dev mode | `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked -- --nocapture` (setup: `lez/RESULTS.md` §8); red: `python3 -I lez/updater/dev/mutants.py NAME` for `m1`, `m1near`, `u2`, `u3`, `u5`, `u4f`, `u6` | `lez/updater/README.md` (items 4, 6, 9, 10; "Health and metrics"; "Tested"); `docs/trust.md` §2.4 |

**Review 9b N5** needs no row of its own; if EVIDENCE cites `reproduce-image-ids.sh`'s counts, `build` now produces the
20 guests of `docs/image-ids.md` (14 + 6) rather than 14, and `compare` names any it lacks.

## What this does not fix

- Review 9d M4 (RFP-021 U.5 shown while its row states the open CLI mapping of LEZ's window text) is the integrator's;
  nothing here touches the CLI.
- S.7's known limit stays: one source claiming a head the store cannot verify (signed in a period whose committee the
  store does not hold) can still turn an idle, up-to-date instance `down` after twice the stall rounds (it fails
  closed). The future check does not cover it: such an update's signature slot can be in the past.
- A chain clock more than 60 s behind Ethereum's makes honest sources' newest updates look signed in the future: they
  are refused and the sources demoted, `/health` naming the gap (before this, such a clock already kept them waiting for
  their windows). A source re-dating within the 60 s, alone in serving the newest finalized header, still delays it by
  up to 60 s.
- On the build box (gVisor) `/proc/self/status` has no `VmHWM` and `clear_refs` is refused: the peak-RSS checks run
  only on a kernel that reports them.
- Everything here ran on the in-process chain; nothing on a sequencer beyond the backend's compile check.
