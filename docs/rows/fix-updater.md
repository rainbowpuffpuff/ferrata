# Stream fix-updater: proposed changes for the integrator

Branch `fix-updater` (from `42b14ca4`). This stream fixes the updater findings of the code reviews of 21 November
2026: UP-H1 (a refused step resent every round at the whole gas limit, its reason not logged), UP-M1 (`/health` `ok`
while nothing reaches the chain), UP-M2 (unbounded input), UP-M3 (abandoned jobs cost one `Miller` a round), and the
operations review's M3 (`/health` hung by one idle connection, memory without bound). It owns no requirement row and
moves none to shown; it extends the reasons of six RFP-021 rows and proposes one EVIDENCE row. Beyond the brief it
also fixes UP-L5 (the daemon's round re-reads the chain just before sending), which R.9's first clause asks.

Files changed, all owned by this stream: `lez/updater/src/{serve,source,updater,ledger,daemon,config}.rs`,
`lez/advance/src/lib.rs` (new public items only), `lez/updater/tests/{serve,bounds,refusals,health,jobs}.rs` (new),
`lez/updater/README.md`, comments of `lez/updater/ferrata-updater.example.toml`, this file. No program or guest code: no image id
moves, nothing deployed changes. API: additions only (`Ledger::now_ms` with a default, `ferrata_advance::{Window,
init_window, open_window}`, `HarnessLedger::now`, `Updater::{held, breaker, carried_jobs, stall_rounds}`); the tuple
field of `serve::Snapshot` is no longer public (its only other user, `modules/ferrata_updater_module`, calls
`default`, `clone` and `set`, which are unchanged). The updater module and the attest module's `advance` pick the
fixes up unchanged when rebuilt.

## Per finding

| Finding | Commit | Test (in `lez/updater/tests/`) | Measured (build box, in-process LEZ v0.4.0-rc1 chain, dev mode, recorded mainnet data) |
|---|---|---|---|
| ops M3, UP-M2 (server) | `5867dc24` | `serve.rs` (5) | `/health` answered in 0.8 ms beside two idle connections (was: no answer while one is open); each idle one closed after the 2 s deadline; a head of 8,192 bytes answered, 8,193 refused; 1 MiB without a newline: RSS 6,252 → 6,264 KiB (was +257 MiB); 24 idle connections: answered again after 1.73 s; a snapshot no round refreshed: 503 |
| UP-M2 (sources) | `40cbabf6`, `88d3cf22` | `bounds.rs` (3) | 1,000 bytes read with a bound of 1,000, 1,001 refused, with and without `Content-Length`; a 256 MiB answer refused after 1 MiB in 42 ms, RSS +0.9 MiB (was accepted, +263 MB); `updates` past 8 MiB, a headers answer past 64 KiB, a module reply past 16 MiB refused; `file://` refused; a 10,000-character provider message kept to 200 printable characters |
| UP-H1 | `09cedec6`, `55012742` (the clock's default) | `refusals.rs` (5 of 6), `daemon_clock.rs` (1) | `Init` and `Open` not sent at the window's end minus 60 s or after it: 0 transactions, 0 charged, `/health` degraded naming the action (was: refused and charged 38,415,184 every round); 1 ms inside the margin: sent and applied. `gas_limit` 1,000,000: `Adopt` refused once in ten rounds (fee 8,199,840), nothing charged after, released when the limit is raised (was: refused every round). A new update each round with every `Open` refused: 5 refusals in 40 rounds (was 40). A refusal lost to another updater (exit 207): not held |
| UP-M1 | `700d7673` | `health.rs` (4) | Every send rejected (a payer without funds) or failing at transport (25 errors): degraded in rounds 1-4, `down` (503) in round 5 (was `ok` throughout); `ok` again once one lands; the only light-client source down: `down` (was degraded); no progress for 16 rounds while the sources are 58,080 slots past the instance: degraded (was `ok`) |
| UP-M3 | `a6802f92` | `jobs.rs` (1) | Ten rounds, a new finality update each, another party opening its job first and leaving it: the updater sends `Miller` then `Finish` of each job it continues, 5 updates applied (store 15,625,920 → 15,630,592), 5 `Miller`s for 5 applied updates, nothing refused, no job kept at the end (was: 7 rounds, 7 `Miller`s, store unmoved) |
| UP-L5 (Low, beyond the brief; R.9's re-check) | `cd1fd4ae` | `refusals.rs` (`a_step_made_moot_while_planning_is_not_sent`) | Another updater applies the update's job while this one asks its sources: the `Open` planned against the old store is not sent, 0 charged (sent anyway: refused, exit 52, fee 1,706,408) |

The whole suite on the branch tip (the guests built with LEZ's guest toolchain in `RISC0_HOME`,
`scripts/ci/install-risc0.sh`): `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked`: **54 tests pass** (9 in the library, 3 `bounds`, 8 `config`, 1 `daemon_clock`, 4 `diversity_limits`, 4 `health`, 1
`jobs`, 13 `recorded`, 6 `refusals`, 5 `serve`) at `55012742`. Clippy `-D warnings`
with and without `harness` clean (`RISC0_SKIP_BUILD=1`); fmt clean. The A3 gate on the tip: the default run replays the grown record's 246 updates (108 mainnet, 138 Sepolia), each applied once, in
1,101 rounds; honest refused / applied gas 0.0752 (gate ≤ 0.10), honest-1's share 0.4841, honest-2's 0.5159 (gate
0.35-0.65) (the suite
rewrites `lez/updater/results/gate.txt`; not committed).

Merging: `git merge-tree` with `rfp-docs-final` reports no conflict in any file of this stream (its two conflicts,
`docs/design/attestation.md` and `docs/guides/consumer-guide.md`, are that branch's with main). Its new
`lez/updater/tests/every_party.rs` and `replacement.rs`, run on this branch's tip (copied, not committed): 8 of 9
pass; the ninth reads `docs/trust.md` §2.4, which only that branch adds. Its `Censoring` ledger wraps the in-process
chain without forwarding `Ledger::now_ms`, which is why that method defaults to each transaction's own time
(`55012742`): its tests keep passing after the recorded data's windows end in wall-clock time.

## Requirement rows (proposed)

| Row | Now | Proposed | Why |
|---|---|---|---|
| RFP-021 R.9 | in part | in part (reason extended; evidence adds the new row) | Add: "Each daemon reads the chain again just before it sends (a step made moot meanwhile is not sent). A step the chain refuses while it stays due is charged once and held, not resent; a refusal lost to another daemon is not held; three such refusals in a row pause sending (5 refusals in 40 rounds where each round brings a new job, against one a round); the A3 gate still passes on that tree." Still in part: not run on a sequencer with several daemons. |
| RFP-021 R.7 | in part | in part (reason extended; evidence adds the new row) | Add: "Past its weak-subjectivity period the updater sends nothing to the instance (nothing charged) and says to initialise a new one." |
| RFP-021 S.7 | in part | in part (reason extended; evidence adds the new row) | Add: "The updater's `/health` raises a stall alarm (the instance unmoved for 6 × defer_window + 10 rounds while the sources serve updates past it) and says `down` after five sends that do not land." The operator guide is still a first draft. |
| RFP-021 F.12 | in part | in part (reason extended; evidence adds the new row) | Add: "A party that opens the job of every new update and leaves it neither stalls the instance nor makes the updater pay for the jobs it leaves." Still no test over every party. |
| RFP-021 P.6 | in part | in part (reason extended) | Add: "The updater bounds what it reads (a source's answer 8 MiB, `/health` request heads 8 KiB, eight connections)." Its CPU, memory and bandwidth are still not measured. |
| RFP-021 U.5 | shown | shown (evidence adds the new row) | The updater checks the program's validity window before it pays, and its `/health` names the action when a checkpoint or an instance is past its period. |

## New EVIDENCE row (number: the next free one)

To paste into `docs/EVIDENCE.md`'s table (columns `# | What it shows | Numbers | Where it ran | Proofs | Re-check |
Details`; links written for that file):

| N | The updater's review fixes (findings UP-H1, UP-M1, UP-M2, UP-M3, UP-L5 and the operations review's M3): no step sent outside the program's validity window by the chain's clock; a refused step that stays due held, not resent, three in a row pausing sending; `/health` from what lands (degraded from the first send that does not land, `down` after five, when no light-client source answers or the budget is reached; a stall alarm; 503 when no round refreshed it); every source answer and `/health` request bounded; jobs another party opens and leaves finished, not paid for again; the chain read again just before sending | `Init`/`Open` past their window: 0 charged (was 38.4M a round); out of gas: one refusal in ten rounds (was one a round); 5 refusals in 40 rounds each bringing a new job; a 256 MiB answer cut at 1 MiB, RSS +0.9 MiB (was accepted, +263 MB); `/health` in 0.8 ms beside idle connections (was no answer); every send rejected: degraded, `down` in round 5 (was `ok`); 5 `Miller`s for 5 applied updates against an opener who leaves every job (was 7 `Miller`s, no update); A3 gate on 246 updates: refused / applied gas 0.0752, shares 0.4841 / 0.5159 | in-process LEZ (as row 43), build box | dev mode | `cd lez && RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --test refusals --test health --test jobs --test bounds --test serve -- --nocapture` | [`lez/updater/README.md`](../lez/updater/README.md) ("What a round does" 7, 9, 10; "Health and metrics"; "Review fixes"), [`docs/rows/fix-updater.md`](rows/fix-updater.md) |
