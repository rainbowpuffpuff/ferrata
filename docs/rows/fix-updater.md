# Stream fix-updater: proposed changes for the integrator

Branch `fix-updater` (from `42b14ca4`). This stream fixes the updater findings of the code reviews of 21 November
2026: UP-H1 (a refused step resent every round at the whole gas limit, its reason not logged), UP-M1 (`/health` `ok`
while nothing reaches the chain), UP-M2 (unbounded input), UP-M3 (abandoned jobs cost one `Miller` a round), and the
operations review's M3 (`/health` hung by one idle connection, memory without bound). It owns no requirement row and
moves none to shown; it extends the reasons of six RFP-021 rows and proposes one EVIDENCE row. Beyond the brief it
also fixes UP-L5 (the daemon's round re-reads the chain just before sending), which R.9's first clause asks.
A second round fixes the findings of the review of these fixes (22 November 2026: M1 and L1 to L7, below).

Files changed, all owned by this stream: `lez/updater/src/{serve,source,updater,ledger,daemon,config}.rs`,
`lez/advance/src/lib.rs` (new public items only), `lez/updater/tests/{serve,bounds,refusals,health,jobs}.rs` (new),
`lez/updater/tests/diversity_limits.rs` (one test's expected status, second round),
`lez/updater/README.md`, comments of `lez/updater/ferrata-updater.example.toml`, this file. No program or guest code: no image id
moves, nothing deployed changes. API: additions only (`Ledger::now_ms` with a default, `ferrata_advance::{Window,
init_window, open_window}`, `HarnessLedger::now`, `Updater::{held, breaker, expired, carried_jobs, stall_rounds,
stall_down_rounds}`, `updater::{After, DOWN_AFTER_FAILED_ROUNDS, MAX_REFUSAL_BACKOFF, LEAVE_JOB_AFTER}`,
`Metrics::failed_rounds`, `Held::due`, `serve::{MAX_PER_CLIENT, MAX_QUICK, QUICK_DEADLINE, stale_after}`), except
`updater::MAX_REFUSALS`, removed in the second round (no other user in the tree); the tuple field of `serve::Snapshot`
is no longer public (its only other user, `modules/ferrata_updater_module`, calls `default`, `clone` and `set`, which
are unchanged). The updater module picks the fixes up when rebuilt. The attest module is unaffected: its `advance`
uses none of the new items (it plans a job and sends nothing; the window check and the refusal handling are the
updater's, before and after it sends).

## Per finding

| Finding | Commit | Test (in `lez/updater/tests/`) | Measured (build box, in-process LEZ v0.4.0-rc1 chain, dev mode, recorded mainnet data) |
|---|---|---|---|
| ops M3, UP-M2 (server) | `5867dc24` | `serve.rs` (5) | `/health` answered in 0.8 ms beside two idle connections (was: no answer while one is open); each idle one closed after the 2 s deadline; a head of 8,192 bytes answered, 8,193 refused; 1 MiB without a newline: RSS 6,252 → 6,264 KiB (was +257 MiB); 24 idle connections: answered again after 1.73 s; a snapshot no round refreshed: 503 |
| UP-M2 (sources) | `40cbabf6`, `88d3cf22` | `bounds.rs` (3) | 1,000 bytes read with a bound of 1,000, 1,001 refused, with and without `Content-Length`; a 256 MiB answer refused after 1 MiB in 42 ms, RSS +0.9 MiB (was accepted, +263 MB); `updates` past 8 MiB, a headers answer past 64 KiB, a module reply past 16 MiB refused; `file://` refused; a 10,000-character provider message kept to 200 printable characters |
| UP-H1 | `09cedec6`, `55012742` (the clock's default) | `refusals.rs` (5 of 6), `daemon_clock.rs` (1) | `Init` and `Open` not sent at the window's end minus 60 s or after it: 0 transactions, 0 charged, `/health` degraded naming the action (was: refused and charged 38,415,184 every round); 1 ms inside the margin: sent and applied. `gas_limit` 1,000,000: `Adopt` refused once in ten rounds (fee 8,199,840), nothing charged after, released when the limit is raised (was: refused every round). A new update each round with every `Open` refused: 5 refusals in 40 rounds (was 40). A refusal lost to another updater (exit 207): not held |
| UP-M1 | `700d7673` | `health.rs` (4) | Every send rejected (a payer without funds) or failing at transport (25 errors): degraded in rounds 1-4, `down` (503) in round 5 (was `ok` throughout); `ok` again once one lands; the only light-client source down: `down` (was degraded); no progress for 16 rounds while the sources are 58,080 slots past the instance: degraded (was `ok`) |
| UP-M3 | `a6802f92` | `jobs.rs` (1) | Ten rounds, a new finality update each, another party opening its job first and leaving it: the updater sends `Miller` then `Finish` of each job it continues, 5 updates applied (store 15,625,920 → 15,630,592), 5 `Miller`s for 5 applied updates, nothing refused, no job kept at the end (was: 7 rounds, 7 `Miller`s, store unmoved) |
| UP-L5 (Low, beyond the brief; R.9's re-check) | `cd1fd4ae` | `refusals.rs` (`a_step_made_moot_while_planning_is_not_sent`) | Another updater applies the update's job while this one asks its sources: the `Open` planned against the old store is not sent, 0 charged (sent anyway: refused, exit 52, fee 1,706,408) |

## Second round: the review of these fixes (22 November 2026)

| Finding | Commit | Test (in `lez/updater/tests/`) | Measured (build box, as above) |
|---|---|---|---|
| M1 (Medium): `/health` 200 while the updater can move nothing | `5c6c05c7` | `health.rs` (`an_unreachable_chain_takes_it_down_and_back`, `an_instance_past_its_weak_subjectivity_period_is_down`, `a_daemon_cut_off_from_its_chain_answers_503`, `no_progress_while_the_sources_move_on_is_degraded_then_down`), `refusals.rs` P1/P2, `diversity_limits.rs` (`waiting_for_clients_is_reported_in_health`) | The chain unreachable for 200 rounds (probe C1): degraded in rounds 1-4, `down` in rounds 5-200 (was degraded in 200 of 200, HTTP 200), 0 failing sends; once it answers, `down` 3 more rounds (no progress) until the job lands, then `ok`. The instance past its weak-subjectivity period for 200 rounds (probe C2): `down` in 200 of 200 (was degraded), nothing sent. A daemon cut off from its chain, its health set on its listener as the binary does: `/health` 200 in rounds 1-4, 503 from round 5. `Init`/`Open` past their window: `down` (was degraded); 1 ms inside the margin: sent, the instance advances (15,626,400 → 15,627,008), `ok`. No progress while the sources are 58,080 slots ahead: degraded from round 19 (16 rounds), `down` from round 35 (32 rounds); waiting for clients: degraded from round 4, `down` from round 36 |
| L3: a refusal of unknown cause given up after three | `7ff14648` | `refusals.rs` (`a_refusal_of_unknown_cause_is_sent_again_until_it_lands`) | Probe A: a `Miller` refused three times for an unknown cause, then accepted: sent in rounds 2, 6, 14 and 30 (held 4, 8, 16 rounds), applied in round 30, the instance moved in round 31 (15,626,400 → 15,627,008), `/health` `ok` (was: not sent again in 286 of 300 rounds, the instance unmoved) |
| L4: a failed read after a refusal counted as "stayed due" | `7ff14648` (with L3: the same classification and hold) | `refusals.rs` (`a_failed_read_after_a_lost_race_is_read_again`, `a_refusal_with_the_chain_unreadable_after_it_is_held_briefly`) | Probe B: 9 races lost, each followed by one failed read: 9 classed `race`, 0 strikes, 0 failing sends (was: 4 races classed `due`, 4 failing sends, 2 breaker trips). Every read failing after the refusal (5 attempts): 9 classed `unread`, each held 4 rounds, 0 strikes, 0 failing sends |
| L2: a kept job left on a short hold | `140dc50c` | `jobs.rs` (`a_kept_job_is_not_left_for_a_short_hold`) | Probe D: the job kept through its 3 held rounds (1 job kept, its file in `open/` kept), its `Miller` sent again in round 6 and finished: 2 `Open`s paid (35,618,136 in fees) for 2 updates applied (was 2 `Open`s for 1 update) |
| L1: eight clients re-opening idle connections deny `/health` | `27d85e43` | `serve.rs` (`health_answers_while_clients_keep_reopening_idle_connections`, `four_hundred_connections_at_once`, `more_connections_than_it_serves`, `request_heads_are_bounded`); library `serve::tests::per_client_and_total_bounds` | Probe E: eight clients re-opening idle connections for 8 s (40 connections): `/health` answered in 80 of 80 tries, the slowest in 1.06 ms (was 0 of 80). 400 connections at once: 8 kept open, the rest closed at once, `/health` answered meanwhile. 24 idle connections: `/health` in 0.6 ms (was after 1.7 s). A head of 8,193 bytes: `400` received (was a reset). 16 MiB without a newline: 1,572,770 bytes written before the server ended it; the test process's RSS +1.9 MiB, other tests running in it (bound 4 MiB; an unbounded server takes the 16 MiB). One host other than loopback: at most 4 of the 8 full places (unit test) |
| L5: two proposed texts inaccurate | this file | — | R.9 and S.7 below, rewritten from the code (the README's wording) |
| L6: claims the code does not support | `fda13ad5`, this file, the README | — | The attest module: unaffected (said above). `beacon_api_module`'s reply: "refused past 16 MiB once received" (README, P.6 below). The sequencer's clock: also the owner's named remote sequencer, whose blocks carry its own clock (README item 10, `daemon.rs`, `ledger.rs`) |
| L7: shared docs contradicting the behaviour | this file | — | Sentences for `docs/design/attestation.md` §4.5 and §15, `docs/guides/operator-guide.md` §3.5 and §3.6 below |
| Nits | `27d85e43`, `fda13ad5`, the README | `serve.rs` | The 400 is received (drained before the close); the RSS check sends 16 MiB; curl gets the URL with `--url`; the retry formula as the code has it, `retry_base_ms * 2^(attempt-1)` (also UP-Info); the README's dated branch name removed; `serve::stale_after(poll_secs)` gives any host of the daemon the binary's 503 age (for `modules/ferrata_updater_module`, which still uses the 600 s default: a two-line change there, its `listener` given the daemon's `poll_secs` and its snapshot built with `.with_stale_after(ferrata_updater::serve::stale_after(poll_secs))`, left to that module's stream) |

The whole suite (the guests built with LEZ's guest toolchain in `RISC0_HOME`, `scripts/ci/install-risc0.sh`):
`cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked`: **64 tests pass** (10 in the
library, 3 `bounds`, 8 `config`, 1 `daemon_clock`, 4 `diversity_limits`, 7 `health`, 2 `jobs`, 13 `recorded`, 9
`refusals`, 7 `serve`) at `fda13ad5` formatted as `4f6cbf72` (the first round: 54 at `55012742`). Clippy `-D warnings`
with and without `harness` clean (`RISC0_SKIP_BUILD=1`); fmt clean; the sequencer backend
(`lez/updater/sequencer`, its own workspace) compiles against the changed crate (`cargo check --locked
--all-targets`). The A3 gate on that tree, unchanged: the default run replays the grown record's 246 updates (108
mainnet, 138 Sepolia), each applied once, in 1,101 rounds; honest refused / applied gas 0.0752 (gate ≤ 0.10),
honest-1's share 0.4841, honest-2's 0.5159 (gate 0.35-0.65) (the suite rewrites `lez/updater/results/gate.txt`; not
committed).

Merging: `git merge-tree` with `rfp-docs-final` reports conflicts only in `docs/design/attestation.md` and
`docs/guides/consumer-guide.md` (that branch's with main); none in any file of this stream. Its new
`lez/updater/tests/every_party.rs` and `replacement.rs`, run on this branch's tip (copied, not committed): 8 of 9
pass, as in the first round; the ninth reads `docs/trust.md` §2.4, which only that branch adds. Its `Censoring` ledger
wraps the in-process chain without forwarding `Ledger::now_ms`, which is why that method defaults to each
transaction's own time (`55012742`): its tests keep passing after the recorded data's windows end in wall-clock time.

## Requirement rows (proposed)

| Row | Now | Proposed | Why |
|---|---|---|---|
| RFP-021 R.9 | in part | in part (reason extended; evidence adds the new row) | Add: "Each daemon reads the chain again just before it sends (a step made moot meanwhile is not sent), and again after a refusal: a refusal lost to another daemon is not held. A step the chain refuses while it stays due is held: for good when the program refuses it or it runs out of gas (out of gas: until `gas_limit` is raised), otherwise 4, 8, 16, 32, then 64 rounds, sent again after each hold; three such refusals in a row pause sending for 8 rounds, then 16, up to 512 (5 refusals in 40 rounds where each round brings a new job, against one a round). A refusal after which the chain cannot be read is held 4 rounds (doubling if it happens again to the same step) and does not count towards the pause. The A3 gate still passes on that tree." Still in part: not run on a sequencer with several daemons. |
| RFP-021 R.7 | in part | in part (reason extended; evidence adds the new row) | Add: "Past its weak-subjectivity period the updater sends nothing to the instance (nothing charged), says to initialise a new one, and its `/health` says `down` (HTTP 503)." |
| RFP-021 S.7 | in part | in part (reason extended; evidence adds the new row) | Add: "The updater's `/health` raises a stall alarm (degraded: the instance unmoved for 6 × defer_window + 10 rounds while the sources serve updates more than `max_lag_slots` past it) and says `down` (HTTP 503) whenever the daemon moves nothing: five sends in a row that do not land, five rounds in a row that end in an error (a chain that answers nothing), the instance past its weak-subjectivity period, no progress for twice the alarm's rounds, no light-client source answering, or the budget reached." The operator guide does not say this yet (sentences proposed below). |
| RFP-021 F.12 | in part | in part (reason extended; evidence adds the new row) | Add: "A party that opens the job of every new update and leaves it neither stalls the instance nor makes the updater pay for the jobs it leaves; a job the updater has paid a step for is not left for a short hold of its next step." Still no test over every party. |
| RFP-021 P.6 | in part | in part (reason extended) | Add: "The updater bounds what it reads (a source's answer through curl 8 MiB; a `beacon_api_module` reply refused past 16 MiB once received; `/health` request heads 8 KiB; eight connections served with the full two seconds, four per host other than loopback, eight more with 250 ms)." Its CPU, memory and bandwidth are still not measured. |
| RFP-021 U.5 | shown | shown (evidence adds the new row) | The updater checks the program's validity window before it pays, and its `/health` names the action when a checkpoint or an instance is past its period. |

## Shared documents (owned by other streams; sentences for the integrator)

These contradict the updater's behaviour on this branch (review of the fixes, L7, and M1). The passages are the same
on `main` and on `rfp-docs-final`.

1. `docs/design/attestation.md` §4.5, last sentence. Now: "**Not built**: a daemon joining another party's open job,
   and clean-up of jobs whose final step fails (they stay, unapplied)." Proposed: "**Built**: the updater daemon
   continues any open job, its own or another party's, and keeps every job it has sent a step for until the job is
   finished or moot, so a party that opens jobs and leaves them neither stalls the instance nor makes it pay again
   ([`lez/updater/README.md`](../../lez/updater/README.md), "What a round does" 7; `lez/updater/tests/jobs.rs`).
   **Not built**: clean-up of jobs whose final step fails (they stay, unapplied)."
2. `docs/design/attestation.md` §15, first paragraph. Now: "...; joining an open job and clean-up of failed jobs; ...".
   Proposed: "...; clean-up of failed jobs (the updater daemon joins open jobs, §4.5); ...".
3. `docs/guides/operator-guide.md` §3.5, after "...(**not built**, [`attestation.md`](../design/attestation.md) §3.6,
   §8)." Insert: "The updater raises its own alarm: its `/health` is `degraded` when the instance has not moved for
   `6 × defer_window + 10` rounds while its sources serve updates more than `max_lag_slots` past it, and `down` (HTTP
   503) after twice as many, or at once when the instance is past its weak-subjectivity period (then initialise a new
   instance)."
4. `docs/guides/operator-guide.md` §3.6. Now: "`/health` is `ok`, `degraded` (with the reasons) or `down` (HTTP 503);".
   Proposed: "`/health` is `ok`, `degraded` (with the reasons) or `down` (HTTP 503: whenever the daemon moves nothing,
   that is no light-client source answering, five sends in a row that do not land, five rounds in a row that end in an
   error, the instance past its weak-subjectivity period, no progress for `2 × (6 × defer_window + 10)` rounds while
   the sources are ahead, or the budget reached; and when no round has refreshed it for ten polls). A step the chain
   refuses is not resent every round: it is held, for good when the program refuses it or it runs out of gas, otherwise
   for 4 to 64 rounds and then sent again, and three refusals in a row of steps that stayed due pause sending for 8 to
   512 rounds; `/health` and the metrics (`_held_steps`, `_sending_paused`, `_refusals_total`) show both. The listener
   serves eight connections at once (four per host other than loopback) and eight more quick ones; keep
   `daemon.listen` on loopback;".

## New EVIDENCE row (number: the next free one)

To paste into `docs/EVIDENCE.md`'s table (columns `# | What it shows | Numbers | Where it ran | Proofs | Re-check |
Details`; links written for that file):

| N | The updater's review fixes (findings UP-H1, UP-M1, UP-M2, UP-M3, UP-L5 and the operations review's M3, then M1 and L1 to L4 of the review of those fixes): no step sent outside the program's validity window by the chain's clock; a refused step that stays due held, not resent every round (for good when the program refuses it or it runs out of gas, otherwise for 4 to 64 rounds and then sent again), three in a row pausing sending; a failed read after a refusal read again, never counted against the pause; `/health` from what lands (degraded from the first send that does not land; `down`, HTTP 503, whenever the daemon moves nothing: five sends or five rounds in a row failing, the instance past its weak-subjectivity period, no progress for twice the stall alarm's rounds, no light-client source answering, the budget reached; 503 when no round refreshed it); every source answer and `/health` request bounded, and a monitor served while clients re-open idle connections; jobs another party opens and leaves finished, not paid for again, and a kept job not left for a short hold; the chain read again just before sending | `Init`/`Open` past their window: 0 charged (was 38.4M a round), `/health` `down`; out of gas: one refusal in ten rounds (was one a round); 5 refusals in 40 rounds each bringing a new job; a `Miller` refused three times for an unknown cause lands on its fourth try, after holds of 4, 8, 16 rounds (was not sent again in 286 of 300 rounds); 9 races lost, each followed by a failed read: 0 strikes (was 2 pauses); the chain unreachable: `down` from round 5 (was degraded, HTTP 200, in 200 of 200); past its period: `down` in 200 of 200; a 256 MiB answer cut at 1 MiB, RSS +0.9 MiB (was accepted, +263 MB); `/health` in 0.8 ms beside idle connections (was no answer), 80 of 80 tries answered beside eight clients re-opening idle connections (was 0 of 80); every send rejected: degraded, `down` in round 5 (was `ok`); 5 `Miller`s for 5 applied updates against an opener who leaves every job (was 7 `Miller`s, no update); 2 `Open`s for 2 updates across a short hold (was 2 for 1); A3 gate on 246 updates: refused / applied gas 0.0752, shares 0.4841 / 0.5159 | in-process LEZ (as row 43), build box | dev mode | `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --lib --test refusals --test health --test jobs --test bounds --test serve --test diversity_limits -- --nocapture` (setup: `lez/RESULTS.md` §8) | [`lez/updater/README.md`](../lez/updater/README.md) ("What a round does" 7, 9, 10; "Health and metrics"; "Review fixes"), [`docs/rows/fix-updater.md`](rows/fix-updater.md) |
