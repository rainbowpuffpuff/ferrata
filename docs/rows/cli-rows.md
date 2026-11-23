# Stream cli-rows: RFP-021 R.5 and U.5, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `cli-rows`
from `eb990490`, third round (the first review: U.5 shown, R.5 in part, two Mediums, twelve Lows; the second: R.5 in
part until M3, four Lows; all addressed below). Code at `69be43a3`, records at `94871f7a`, then this file. Box runs:
the Modal build box (Linux x86-64), RISC Zero dev mode, 23 November 2026; laptop: the `links` job's checks. Nothing was
pushed or sent; nothing reached a public network (the zones and the Ethereum endpoints of the tests listen on
127.0.0.1). No program, guest or image id changed; no dependency or lockfile changed.

## The second review, finding by finding

| Finding | Fix | Shown by |
|---|---|---|
| M3 (a resend turned away recorded as "not admitted"; `--again` then duplicates) | Only a first submission the sequencer turns away is recorded as refused (the run finished). A resend turned away, while the earlier run's copy of the same bytes may be pending, leaves the run unfinished and suggests no `--again` ("run the same command again to read what became of it"); so does a resumed run held near the window's end with such a copy | `the_journal_guards`, the review's case: killed with the `Attest` in the pool, the resend answered "The mempool is full", exit 1 and unfinished; the pooled copy lands; run again: found applied, one `Attest`. It fails on `8faee805` (box, the new test against that tip's `attest.rs`); `stale_checkpoint_shown_as_code_53`, `u5-resume-near-end-copy-pending` |
| L1′ (no block since the end: still sent and charged) | A window's end is judged at the later of the zone's clock and this machine's clock, as `ferrata-updater` judges its windows (`MachineClock`), with the same 60 s margin; its start at the zone's clock only. `FERRATA_ATTEST_CLOCK_MS` sets this machine's clock for tests (every test sets it, `0` by default, so no test depends on the date) | `u5-resume-past-end-no-block`, the first review's case exactly (no block stamped): not sent, "this machine's clock", journal cleared, nothing charged. Fails on `8faee805`. The remaining limit is stated: with this machine's clock more than the margin behind a zone that stamped no block since the end, a step is still sent and charged |
| L13 (the key's sort untested) | Two `--storage` statements in both orders give one key | the library's `the_journal_key_is_the_command_as_parsed` (the review's surviving mutant N6 now killed) |
| L14 (the module test's route) | A second test: the route through Logos Core's `eth_rpc_module`, Logos's call replaced by a stand-in (the module crate installs the real one): unreached, `ok: false`, not JSON, each an error, the request's `timeout_ms` handed on, then answered. The status text names both routes | `failing_endpoint.rs` `the_eth_rpc_module_route_fails_cleanly_and_recovers` |
| L15 (the not-open hold's wording) | Said as it is: judged at the dry run's time (the zone's clock plus 10 s); a zone with a longer block interval can hold a valid step up to one interval past its start | `lez/ATTEST-CLI.md`, this file |
| Nits | The command block runs `scripts/fetch-upstream.sh` first | this file |

## The first review, finding by finding

| Finding | Fix | Shown by |
|---|---|---|
| M1 (R.5: the module's Ethereum reads untested) | `ferrata_attest_module`'s `fetch_log_proof`, and the `eth_getProof` read behind `build_attest_tx` and `dry_run_attest`, against a refusing, holding, cutting, 503 and erroring endpoint, and `fetch_log_proof` cut in its second request: each an error reply within its bound, each answered once the endpoint is back | `modules/ferrata_attest_module/rust-lib/calls/tests/failing_endpoint.rs`, in CI's `modules-resolve` job (after the staging that job does) |
| M2 (the EVIDENCE row's gas) | `Attest` 94,795 (a log) to 275,925 (a slot with its predicate) | the records |
| L1 (a resumed run past the period resends and pays 10,000,000) | Before sending (again) a journaled transaction, its window is checked: ended, or ending within 60 s (as `ferrata-updater`'s `WINDOW_MARGIN_MS`), nothing is sent; code 53's message; when nothing of the command was applied the journal is cleared. Judged at the zone's clock (its newest block's time, or the followed tip's if later) and, since the second review, this machine's clock too (L1′ above) | `attest_faults.rs` `stale_checkpoint_shown_as_code_53` (`u5-resume-past-end`, `u5-resume-past-end-no-block`) |
| L2 (the hold read the final tip's clock) | The dry run is made at the zone's clock; the hold and the 60 s margin use it | the same test, `u5-final-tip-lagging` (the follower at block 1, the zone's head past the end) |
| L3, L4 (concurrent runs; no directory fsync) | An exclusive lock on `<state-dir>/lock` for the run's life (`File::try_lock`; a second run exits 2); temporary files named with the process id; the directory synced after each rename | `the_journal_guards` (a `status` beside a waiting send exits 2) |
| L5 (a refused submission leaves a journal only deletion clears) | A JSON-RPC error answer to a first submission (the transaction not admitted) ends the run recorded as finished (exit 1, the answer kept); `--again` then plans it anew. A resend's error answer, and no answer at all, keep the run unfinished (M3 above) | `the_journal_guards` (LEZ's "Transaction too large" from the stand-in; the M3 case) |
| L6 (three guards untested) | Tests for each | `the_journal_guards`: the nonce taken (exit 1, "NOT APPLIED"), `--again` on an interrupted run (exit 2, nothing sent), `--send --yes` without `--state-dir` (exit 2, nothing sent) |
| L7 (a hash in no record) | The records README quotes no transaction hash (signatures are randomized) | `cli/results/attest-faults-r2/README.md` |
| L8, L9 (EVIDENCE wording; a scratch path in this file) | "signing nothing a second time"; the path removed | this file |
| L10 (a command spelled otherwise applied again) | The journal key is built from the parsed command: addresses and words canonical, statements sorted, files by SHA-256 | `the_journal_guards` (no `0x`, capitals, the value unpadded: "already sent"), and the library's `the_journal_key_is_the_command_as_parsed` |
| L11 (the not-open remedy) | "it can be sent from then (with --again once a run has sent it)" | the library's tests |
| L12 (a superseded step reported "not sent") | Recorded and reported "not applied: its nonce N was used by another transaction" | `the_journal_guards` |
| N1-N5 | The time named as the dry run's or the block's; `--again` on a dry run says nothing is sent; the follower's state written only when it moved; `ci.yml`'s `fetch_native.py` line stops the step on failure; the commands below carry CI's environment | the records; `ci.yml` |

## What `ferrata attest` does now (`cli/src/attest.rs`)

- **Ethereum reads.** `attest` is the only command that reads Ethereum (`--eth`: `eth_getProof`, `eth_getBlockReceipts`;
  `init` and `advance` read SSZ files; `config`, `status`, `state` and `receipt` only the zone). Before any proof it
  checks the endpoint's chain (`eth_chainId`) and the record's block hash at `--block`. Each request is bounded
  (`--eth-timeout`, 60 s) and retried (`--eth-retries`, 2) after a failure that may pass, not after an error answer or
  another chain. A failure: exit 4, "nothing was sent", the follower's state kept. URLs are printed without path or
  query.
- **Interrupted and run again.** `--send --yes` needs `--state-dir`. Before anything is sent, the command's transactions
  are written there, signed, to `sent/<intent>.json` (the intent: the command as parsed); each attempt and receipt is
  recorded. Run again, the command signs nothing: a transaction in a final block is reported from its receipt; its nonce
  taken by another, reported not applied (exit 1); in a block not yet final, waited for; in no block, sent again byte
  for byte (one hash and nonce; LEZ drops a second copy for its used nonce), or sent as the earlier run signed it if
  never attempted, unless its window has ended or ends within 60 s (then nothing is sent; a journal nothing of which was
  applied is cleared; while the earlier copy may be pending, the run stays unfinished). A finished command run again
  reports what it did and sends nothing; `--again` sends a new one (refused on an interrupted run). A first submission
  the sequencer answers with an error ends the run, recorded; a resend so answered leaves it unfinished, with no
  `--again` suggested. One run at a time per state directory; files replaced whole, the directory synced.
- **A stale checkpoint.** LEZ refuses a transaction executed outside its program's timestamp window with "Execution
  outside of the validity window" (no exit code; charged the whole gas limit). For `Init` and `Open` past the window's
  end the tool shows code 53's IDL message ("OutsideWeakSubjectivityPeriod … recover from a new bootstrap."), naming the
  checkpoint (for `Open`, the instance's newest finalized header), when its period ended, the time it was judged at (the
  dry run's, the block's, or this machine's) and the remedy (a new instance from a recent bootstrap, `ferrata attest
  init`; operator guide §10), exit 3. Such a step is not sent, nor one whose window ends within 60 s: a window's end is
  judged at the later of the zone's clock (its newest block's time) and this machine's clock, as `ferrata-updater`
  judges it. Nor is a step whose window has not opened by the dry run's time (the zone's clock plus 10 s; on a zone
  whose block interval exceeds 10 s a valid step can be held up to one interval past its start: run it again then). The
  same message in the zone's receipt and in `ferrata attest receipt` (how a caller of `ferrata_attest_module` reads an
  outcome). Not hidden: before the window opens LEZ's text stays, with the window's start; inside the computed window it
  stays as it is; every other refusal is unchanged. The limit: with this machine's clock more than the margin behind a
  zone that has stamped no block since a window ended, the step is still sent and charged the gas limit; its receipt
  then shows code 53 (as `u5-crossing-end` shows for a zone whose next block is stamped past the end).

`ferrata_attest_module` does not surface the window refusal (`lez_submit` reports inclusion only; `lez_core` reports no
outcome, `modules/ferrata_attest_module/rust-lib/calls/src/lez.rs:17`, `:254`); its source is unchanged, and the new
test file is its only change.

Other files: `cli/tests/attest_faults.rs`, `cli/results/attest-faults-r2/` (replaces `-r1`), `.github/workflows/ci.yml`
(a step in `roundtrip`, a step in `modules-resolve`), `scripts/ci/row-tests.toml` (R.5, U.5), `lez/ATTEST-CLI.md`,
`cli/tests/attest-seq-hook.sh` (its two deliberate replays pass `--again`). On a real LEZ v0.4.0-rc1 zone we started
(local sequencer and Logos Blockchain node), `cli/tests/attest-seq.sh` ran again with this tool (run rf2,
`cli/results/attest-seq-rf2/`, tool built at `1cb2c8ee`): 22 of 22 commands as expected, 24 sent transactions each equal
to its dry run, the 52 outcome lines identical to rf1's; its two deliberate replays recognised by the journal and sent
with `--again`.

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-021 R.5 | in part (79, 99, 104) | **shown** | the updater (79, 99, 104); the attestation monitor (`monitor/attest/tests/monitor.rs:442`, `:617`); the attestation CLI and `ferrata_attest_module`'s Ethereum reads under a failing endpoint, the CLI interrupted: proposed EVIDENCE row (below) |
| RFP-021 U.5 | in part (39, 56, 69, 70, 151, 200) | **shown** | the 22 failure-mode cases (151) and the refusal codes (39, 56, 69, 70); the stale-checkpoint case now code 53's message from the CLI, and not sent: proposed EVIDENCE row (below) |

### Proposed status texts (`requirements-status.md`, the "Why" column)

**R.5 (shown):** The updater daemon: a down source and dropped connections handled; restarted mid-job, it finishes with
no second Open, harness and sequencer. The attestation monitor: unreachable and wrong-chain providers not counted
(`monitor/attest/tests/monitor.rs:442`); a divergence held while the instance is unreadable and while every provider is
down, and its freeze hook retried until it is up (`:617`). The attestation CLI (`ferrata attest`, whose one command
reading Ethereum is `attest`), as a process against a stand-in zone (LEZ v0.4.0-rc1's own block application) and a
scripted endpoint, in CI: an endpoint refusing connections, never answering, cutting its answer (the proof; the
receipts), answering 503 or an error, serving another chain id or another block hash: exit 4, nothing sent, the same
command applied once afterwards; killed while reading Ethereum, with its transaction in the pool, in a block not yet
final, and an `advance` after its `Open`: run again, nothing signed a second time, nothing applied twice (12
submissions, 11 included, each once; one byte-for-byte resubmission); the same command spelled otherwise recognised, a
first submission the sequencer turns away recorded, a resend turned away while the earlier copy was pending left
unfinished (the copy then found applied: one `Attest`), a nonce another transaction took reported, one run per state
directory. `ferrata_attest_module`'s own Ethereum reads (`fetch_log_proof`; `eth_getProof` behind `build_attest_tx` and
`dry_run_attest`), in CI: over the `rpc_url` route, against a refusing, holding, cutting, 503 and erroring endpoint,
each returns an error within its bound (`timeout_ms`; a held request cut at 1,001 ms for a 1,000 ms bound) and the same
call succeeds once the endpoint answers; over the route through Logos Core's `eth_rpc_module`, with Logos's call
replaced by a stand-in (the module itself is not run: it makes the request, with the bound the calls crate hands it), an
unreached module, an `ok: false` answer and a reply that is not JSON each give an error, then the call succeeds. Its
`verify_checkpoint` does not count an unreachable provider (its crate's test, on the box).

**U.5 (shown):** Every listed failure mode refused through the program with its code (22 cases, a committee handoff
refused with 27, 45 and 23 among them), each code's name and message from the IDL the CLI prints. A checkpoint stale by
the zone's clock is refused by LEZ with its own text ("Execution outside of the validity window"), on the window the
program declares; `ferrata attest` shows it as code 53's message, naming the checkpoint (for an update, the instance's
newest finalized header), when its period ended and the remedy (a new bootstrap): from the dry run, made at the zone's
clock (then nothing is sent, nor within 60 s of the end judged also by this machine's clock, nor by a run resumed past
it), from the zone's receipt and from `receipt`, on a real rc1 sequencer's recorded refusal (the library's tests) and on
a stand-in zone's (CI); an update before its window opens keeps LEZ's text with the window's start. The 53 message also
applies when the update is signed after the end; the updater checks the program's window before it pays and its
`/health` names the action. `ferrata_attest_module` reports inclusion only; its callers read outcomes with `ferrata
attest receipt`.

### Proposed traceability texts (`traceability.md`, the evidence column)

- **R.5**, append: "The attestation CLI (EVIDENCE row NEW; CI's `roundtrip` job): `attest` against a failing Ethereum
  endpoint (refused, no answer, an answer cut short, 503, an error, another chain id or block hash): exit 4 and nothing
  sent, then applied once; killed mid-command at four points and run again: nothing signed a second time, nothing
  applied twice; the journal's guards (`cli/tests/attest_faults.rs`). `ferrata_attest_module`'s own Ethereum reads
  against the same failures: an error within their bound, then answered, over the `rpc_url` route, and over the route
  through `eth_rpc_module` with Logos's call replaced by a stand-in (CI's `modules-resolve` job,
  `modules/ferrata_attest_module/rust-lib/calls/tests/failing_endpoint.rs`)." Tests column: add both files.
- **U.5**, replace the closing "In part: … is not built." with: "LEZ's refusal of a checkpoint stale by the zone's clock
  is shown by `ferrata attest` as code 53's message naming the checkpoint, when its period ended and the remedy (a new
  bootstrap), from the dry run (then not sent), the receipt and `receipt`, on a real rc1 sequencer's recorded refusal
  and a stand-in zone's; nothing is sent past or within a minute of the end, also on a resumed run; before an update's
  window opens LEZ's text stays as it is (EVIDENCE row NEW; `cli/src/attest.rs` tests, `cli/tests/attest_faults.rs`)."
  The gaps list's item "R.5, P.9, a component untested and actions unmeasured" loses its R.5 half.

## Exact tests and commands, by clause

The commands as CI runs them (the `roundtrip` job sets `RISC0_HOME` and the checked native archives; the
`modules-resolve` job stages the module's crates first):

```sh
cd cli && export RISC0_HOME="$HOME/risc0-home-lez" && vars=$(python3 -I ../scripts/fetch_native.py env) && eval "$vars"
cargo test --locked --lib attest
RISC0_DEV_MODE=1 cargo test --locked --test attest_faults -- --nocapture
cd .. && scripts/fetch-upstream.sh && scripts/ci/modules-resolve.sh ferrata_attest_module
cd modules/ferrata_attest_module/rust-lib && cargo test --locked -p ferrata-attest-calls --test failing_endpoint -- --nocapture
```

| Clause | Test | Records (`cli/results/attest-faults-r2/`) |
|---|---|---|
| R.5 "temporary RPC or connectivity failure on the Ethereum side", the CLI | `attest_faults.rs` `failing_endpoint_and_interruptions_do_nothing_twice`: eight endpoint faults, each exit 4 with nothing sent; retries counted per method | `r5/05–12` |
| the same, the module | `failing_endpoint.rs`: `fetch_log_proof`, `build_attest_tx`, `dry_run_attest` × refused, held, cut, 503, error, and `fetch_log_proof` cut in its second request: 16 error replies within their bound (the `rpc_url` route); over `eth_rpc_module` with Logos's call replaced by a stand-in: unreached, `ok: false`, not JSON: 9 error replies, the bound handed on | `module/failing_endpoint.txt` |
| R.5 "leaves any off-chain component in a recoverable state, able to resume" | the CLI: every failed command then applied once with the endpoint answering (`r5/13`, `14`, `17`); the module: every failing call then answered | `r5/13–17`; `module/` |
| R.5 "without duplicating work already done" | the CLI: kills after `Open`, in the pool, in a block not yet final, while reading Ethereum; reruns sign nothing; the zone's included transactions each submitted once; the guards: the same command spelled otherwise, a first submission turned away, a resend turned away with the earlier copy pending (left unfinished; one `Attest`), a nonce taken, `--again` on an interrupted run, no `--state-dir`, two runs on one state directory (`the_journal_guards`) | `r5/02–04`, `16–22`, `r5/journal-example.json`, `r5-guards/` |
| R.5, the other components | updater: `lez/updater/tests/recorded.rs` (CI `lez-harness`); monitor: `monitor/attest/tests/monitor.rs` (box) | rows 79, 99, 104 |
| U.5 "checkpoint stale or outside the weak-subjectivity period": clear, actionable | `cli/src/attest.rs` tests: the real rc1 sequencer's refusal (`e2e/runs/ci-sequencer-attest-2026-11-21/records/run.tsv` and its block in `blocks.borsh`) mapped to 53; the constant pinned to `LeeError::OutOfValidityWindow`; `send_check` past and near the end | (unit tests) |
| U.5, through the tool, LEZ's text produced by LEZ's state machine | `stale_checkpoint_shown_as_code_53`: `init` past the end (dry run, not sent), `init` and `advance` crossing the end (receipts) and both `receipt`s, a lagging final tip (held), a run resumed past the end with a block stamped since and with none (judged by this machine's clock): nothing sent, journal cleared; a run resumed near the end with the earlier copy pending: unfinished, then found applied; `advance` before its window opens (not 53, not sent) | `u5-*` |
| U.5, "do not hide other LEZ errors" | `a_window_not_open_yet_is_not_code_53`: not open, inside, the gas limit's text, a program's own 53 | (unit tests) |

## Mutants (the box, each file restored after each)

Round 1, 4 of 4 killed: no code 53, no resume, no endpoint check, no retry. The first review ran 13 more: 10 killed; the
3 that survived (the nonce-taken path, `--again` on an interrupted run, the `--state-dir` guard) are killed by
`the_journal_guards` now. This round's 11 mutants of the new code, 12 runs, all killed (each fails a named assertion): a
resumed run's window check removed (`u5-resume-past-end`: "not sent again" missing); the dry run at the followed tip's
time (`u5-final-tip-lagging`: "not sent" missing); the journal key from the spelling (the library's key test and
`the_journal_guards`); no lock (a second run exit 0 for 2); a sequencer's error answer taken as no answer; the
nonce-taken check off ("NOT APPLIED" missing); `--again` allowed on an interrupted run (exit 1 for 2); no `--state-dir`
guard (exit 1 for 2); a step before its window opens sent ("not sent" missing); the module's read timeout removed (a
held request takes 30 s); the module's errors swallowed (the reply is not the method's error). The second review ran 11
more: 10 killed; its survivor (the key's sort, N6) is killed by the library's key test now. This round's 5 mutants, all
killed: a resend turned away recorded as refused (the M3 case fails); this machine's clock ignored (the no-block case
fails); a resumed hold finishing a run whose copy may be pending (the near-end case fails); the key not sorted (the key
test fails); the module taking an `ok: false` answer (the `eth_rpc_module` test fails).

## Proposed EVIDENCE row (next free number; 227 at `eb990490`, after the other streams' rows)

Paths written plainly; as links in `docs/EVIDENCE.md` (relative to `docs/`): `../cli/tests/attest_faults.rs`,
`../cli/src/attest.rs`, `../modules/ferrata_attest_module/rust-lib/calls/tests/failing_endpoint.rs`,
`../cli/results/attest-faults-r2/README.md`, `../e2e/runs/ci-sequencer-attest-2026-11-21/README.md`.

| NEW | **The attestation CLI and module under a failing Ethereum endpoint, the CLI interrupted, nothing done twice; a stale checkpoint as code 53's message (RFP-021 R.5, U.5), in CI's `roundtrip` and `modules-resolve`**: `ferrata attest` as a process against a stand-in zone (LEZ v0.4.0-rc1's block application, the program at genesis, on 127.0.0.1) and a scripted endpoint (recorded mainnet responses): refused connections, no answer, answers cut short (proof; receipts), 503, an error, another chain id, another block hash, each exit 4 with nothing sent and the command applied once afterwards; killed after an `advance`'s `Open`, while reading Ethereum, with its `Attest` in the pool and in a block not yet final, each run again signing nothing a second time; the journal's guards (a command spelled otherwise, a first submission turned away, a resend turned away with the earlier copy pending, a nonce taken, `--again` on an interrupted run, no `--state-dir`, two runs on one state directory); `ferrata_attest_module`'s `fetch_log_proof`, `build_attest_tx` and `dry_run_attest` against the same failures, each an error within its bound, then answered (its `rpc_url` route; its `eth_rpc_module` route with Logos's call replaced by a stand-in); LEZ's "Execution outside of the validity window" for `Init` and `Open` past the checkpoint's weak-subjectivity period shown as code 53's message with the checkpoint, the time its period ended and the remedy (a new bootstrap), from the dry run (not sent; also within 60 s of the end, judged by the zone's clock and this machine's, with a lagging final tip, and on a run resumed past the end, with or without a block since); the receipt and `receipt`; the same mapping on a real rc1 sequencer's recorded refusal | 62 tool commands (26 exit 0, 5 exit 1, 3 exit 2, 10 exit 3, 8 exit 4, 10 SIGKILLed); the R.5 zone 12 submissions, 11 transactions included, each once; the guards' zone 15 submissions, 2 turned away, 13 included, each once; the module 16 failing calls over `rpc_url`, each an error within its bound (a held request cut at 1,001 ms for 1,000 ms), and 9 over `eth_rpc_module` (stand-in), each answered after; `Attest` 94,795 (a log) to 275,925 (a slot with its predicate) execution gas; a refused `Init` charged 10,000,000 (the gas limit); 7 + 3 + 2 tests, 35.3 s and 5.9 s | build box; stand-in zone (LEZ's state machine and block application, not its sequencer binary) | none: public transactions, no RISC Zero proof on these paths (`RISC0_DEV_MODE` set) | the commands above | `cli/results/attest-faults-r2/README.md` |

## Limits

- The zone in the tests is LEZ's state machine and block application behind a stand-in RPC, not LEZ's sequencer binary
  (no mempool limits, no Bedrock node); its block builder drops a transaction it rejects, as LEZ's does; it stamps a
  block only when a test says so, where LEZ's sequencer stamps one every interval (the resumed-past-the-end case stamps
  one when it moves the clock). The sequencer run rf2 used real LEZ binaries but none of this round's failure cases (no
  fault, no kill, no window crossing): those run on the stand-in only. The mainnet fixture's checkpoint leaves its
  period on 2026-11-25 22:40:23 UTC; a wall-clock rerun of the hook after that holds `mainnet-init` as stale, by design.
- The module's `eth_rpc_module` route is tested with Logos's call replaced by a stand-in, not inside Logos Core: the
  request itself, and its bound, are Logos's module's (the calls crate hands it `timeout_ms` and keeps no state).
- A window's end is judged at the later of the zone's clock and this machine's: with this machine's clock more than the
  60 s margin behind a zone that has stamped no block since the end, a step is still sent and charged the gas limit (its
  receipt then shows code 53).
- `receipt` knows an `Open`'s window end, not its start: before an update's window opens, `receipt` prints LEZ's text as
  it is (the run itself names the start).
