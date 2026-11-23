# Stream cli-rows: RFP-021 R.5 and U.5, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `cli-rows`
from `eb990490`; code at `42c378d4`, records at `4beea895`, then this file. Box runs: the Modal build box (Linux
x86-64), RISC Zero dev mode, 23 November 2026; laptop: the `links` job's checks. Nothing was pushed or sent; nothing
reached a public network (the zone and the Ethereum endpoint of the tests listen on 127.0.0.1). No program, guest or
image id changed; no dependency changed (`cli/Cargo.lock` untouched).

## What changed

`ferrata attest` (`cli/src/attest.rs`):

- **Ethereum reads.** `attest` is the only command that reads Ethereum (`--eth`: `eth_getProof`,
  `eth_getBlockReceipts`; `init` and `advance` read SSZ files, `config`, `status`, `state` and `receipt` only the zone).
  Before any proof it checks that the endpoint serves the record's chain (`eth_chainId`) and block (the hash the record
  binds at `--block`). Each request is bounded (`--eth-timeout`, 60 s) and retried (`--eth-retries`, 2) after a failure
  that may pass (curl's connect, timeout, partial and empty-answer exits; HTTP 5xx and 429; a body that is not JSON),
  not after an error answer or another chain. A failure: exit 4 (new), "nothing was sent", the follower's state kept.
  An endpoint's URL is printed without its path or query (a provider's key often sits there).
- **Interrupted and run again.** `--send --yes` needs `--state-dir` (exit 2 otherwise). Before anything is sent, the
  command's transactions are written there, signed, to `sent/<intent>.json`, the intent being what the command asks for
  (program, instance, files by SHA-256, statements; not the endpoints); each send attempt and each receipt is recorded.
  Run again, the same command signs nothing: a transaction in a final block is reported from its receipt, one in a
  block not yet final (the sequencer's head shows its nonce used) is waited for, one in no block is sent again byte
  for byte (same hash and nonce; LEZ drops a second copy for its used nonce), one never attempted is sent as the
  earlier run signed it; its nonce used by another transaction stops it (exit 1). A finished command run again reports
  what it did and sends nothing; `--again` sends a new one. The follower's state is one file replaced whole
  (`follower.state`; the two files of earlier versions are still read).
- **A stale checkpoint.** LEZ refuses a transaction executed outside the timestamp validity window its program gave
  it with "Execution outside of the validity window" (no exit code; charged the whole gas limit). At `eb990490` the
  tool dropped that text: the refusal printed as "reverted without a code" (`attest.rs:287` there), `init`'s and
  `advance`'s dry run said "DISAGREE" (`:399`) and `--send --yes` sent it (`:413`). Now, for `Init` and `Open` (the steps
  with a window: `ferrata_advance::init_window`, `open_window`), past the window's end the refusal is shown as code
  53's message from the IDL ("OutsideWeakSubjectivityPeriod … recover from a new bootstrap."), naming the checkpoint
  (for `Open`, the instance's newest finalized header), when its period ended, the zone's clock and the remedy (a new
  instance from a recent bootstrap with `ferrata attest init`; operator guide §10), exit 3; a dry run showing it is not
  sent. The same in the zone's receipt after a send, and in `ferrata attest receipt` (how a caller of
  `ferrata_attest_module` reads an outcome). Not hidden: before the window opens (an update before its signature slot)
  LEZ's text stays as it is, with the window's start; inside the window as computed it stays as it is; every other
  refusal is unchanged (a program's code as before; LEZ's other texts now printed as they are).

`ferrata_attest_module` does not surface this refusal: its `lez_submit` reports inclusion only (`in_block`, `not_seen`:
`modules/ferrata_attest_module/rust-lib/calls/src/lez.rs:254`; `lez_core` reports no outcome, `:17`), and its planning
runs natively without the chain's clock. A caller reads the outcome with `ferrata attest receipt`, which now maps it.
The module is unchanged.

Other files: `cli/tests/attest_faults.rs` (new), `cli/results/attest-faults-r1/` (new), `.github/workflows/ci.yml`
(one step added to the `roundtrip` job), `scripts/ci/row-tests.toml` (R.5 and U.5 entries), `lez/ATTEST-CLI.md`,
`cli/tests/attest-seq-hook.sh` (its two deliberate replays pass `--again`, else the journal would report them and send
nothing). Shared files of other streams: only `ci.yml` and `row-tests.toml`, each a local addition.

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-021 R.5 | in part (79, 99, 104) | **shown** | the updater (79, 99, 104), the attestation monitor (`monitor/attest/tests/monitor.rs:442`, `:617`), and now the attestation CLI under a failing Ethereum endpoint and interrupted: proposed EVIDENCE row (below) |
| RFP-021 U.5 | in part (39, 56, 69, 70, 151, 200) | **shown** | the 22 failure-mode cases (151) and the refusal codes (39, 56, 69, 70); the stale-checkpoint case now code 53's message from the CLI: proposed EVIDENCE row (below) |

### Proposed status texts (`requirements-status.md`, the "Why" column)

**R.5 (shown):** The updater daemon: a down source and dropped connections handled; restarted mid-job, it finishes with
no second Open, harness and sequencer. The attestation monitor: unreachable and wrong-chain providers not counted
(`monitor/attest/tests/monitor.rs:442`); a divergence held while the instance is unreadable and while every provider is
down, and its freeze hook retried until it is up (`:617`). The attestation CLI (`ferrata attest`, its one command that
reads Ethereum, `attest`), as a process against a stand-in zone (LEZ v0.4.0-rc1's own block application) and a scripted
endpoint, in CI: an endpoint refusing connections, never answering, cutting its answer (the proof; the receipts),
answering 503 or an error, serving another chain id or another block hash: exit 4, nothing sent, the same command applied
once afterwards; killed while reading Ethereum, with its transaction in the pool, in a block not yet final, and an
`advance` after its `Open`: run again, nothing signed again, nothing applied twice (12 submissions, 11 included, each
once; one byte-for-byte resubmission). `ferrata_attest_module`'s Ethereum reads (`fetch_log_proof`,
`verify_checkpoint`) are single calls that send nothing; an unreachable provider is not counted (its crate's test, on the
box).

**U.5 (shown):** Every listed failure mode refused through the program with its code (22 cases, a committee handoff
refused with 27, 45 and 23 among them), each code's name and message from the IDL the CLI prints. A checkpoint stale by
the zone's clock is refused by LEZ with its own text ("Execution outside of the validity window"), on the window the
program declares; `ferrata attest` shows it as code 53's message, naming the checkpoint (for an update, the instance's
newest finalized header), when its period ended and the remedy (a new bootstrap), from the dry run (then not sent), from
the zone's receipt and from `receipt`: on a real rc1 sequencer's recorded refusal (the library's tests) and on a stand-in
zone's (CI); an update before its window opens keeps LEZ's text with the window's start. The 53 message also applies
when the update is signed after the end; the updater checks the program's window before it pays and its `/health` names
the action. `ferrata_attest_module` reports inclusion only; its callers read outcomes with `ferrata attest receipt`.

### Proposed traceability texts (`traceability.md`, the evidence column)

- **R.5**, append: "The attestation CLI (EVIDENCE row NEW; CI's `roundtrip` job): `attest` against a
  failing Ethereum endpoint (refused, no answer, an answer cut short, 503, an error, another chain id or block hash):
  exit 4 and nothing sent, then applied once; killed mid-command at four points and run again: nothing signed again,
  nothing applied twice (`cli/tests/attest_faults.rs`)." Tests column: add `cli/tests/attest_faults.rs`.
- **U.5**, replace the closing "In part: … is not built." with: "LEZ's refusal of a checkpoint stale by the zone's clock
  is shown by `ferrata attest` as code 53's message naming the checkpoint, when its period ended and the remedy (a new
  bootstrap), from the dry run (then not sent), the receipt and `receipt`, on a real rc1 sequencer's recorded refusal
  and a stand-in zone's; before an update's window opens LEZ's text stays as it is (EVIDENCE row NEW;
  `cli/src/attest.rs` tests, `cli/tests/attest_faults.rs`)." The gaps list's item "R.5, P.9, a component untested and actions unmeasured"
  loses its R.5 half.

## Exact tests and commands, by clause

| Clause | Test | Command | Commit, run |
|---|---|---|---|
| R.5 "temporary RPC or connectivity failure on the Ethereum side" | `cli/tests/attest_faults.rs`, `failing_endpoint_and_interruptions_do_nothing_twice`: eight endpoint faults, each exit 4 and `submitted()` unchanged; retries counted per method | `cd cli && RISC0_DEV_MODE=1 cargo test --locked --test attest_faults -- --nocapture` (CI `roundtrip`) | `42c378d4`; box run 23 Nov, 2 passed in 32.4 s; records `cli/results/attest-faults-r1/r5/05–12` |
| R.5 "leaves any off-chain component in a recoverable state, able to resume" | the same: every failed command then run with the endpoint answering and applied once (13, 14, 17); the follower state reused across 22 runs and 4 kills | as above | records `r5/13–17` |
| R.5 "without duplicating work already done" | the same: kills after `Open` (`advance`), in the pool, in a block not yet final, while reading Ethereum; reruns sign nothing; the zone's included transactions each submitted once, no `Attest` request or job step twice | as above | records `r5/02–04`, `16–22`, `journal-example.json` |
| R.5, the other components | updater: `lez/updater/tests/recorded.rs` (CI `lez-harness`); monitor: `monitor/attest/tests/monitor.rs` (box) | as mapped in `scripts/ci/row-tests.toml` | rows 79, 99, 104 |
| U.5 "checkpoint stale or outside the weak-subjectivity period": clear, actionable | `cli/src/attest.rs` `tests::recorded_refusal_of_a_stale_checkpoint_maps_to_code_53` (the real rc1 sequencer's refusal: `e2e/runs/ci-sequencer-attest-2026-11-21/records/run.tsv` and its block in `blocks.borsh`); `tests::lez_text_is_the_constant` (the constant is `LeeError::OutOfValidityWindow`'s text) | `cd cli && cargo test --locked --lib attest` (CI `roundtrip`) | `42c378d4`; 5 passed |
| U.5, the same through the tool, LEZ's text produced by LEZ's state machine | `cli/tests/attest_faults.rs`, `stale_checkpoint_shown_as_code_53`: `init` past the end (dry run, not sent), `init` and `advance` crossing the end (the receipts), both `receipt`s, `advance` before its window opens (not 53) | the `attest_faults` command above | records `u5-*` |
| U.5, "do not hide other LEZ errors" | `tests::a_window_not_open_yet_is_not_code_53`: not open, inside, the gas limit's text, a program's own 53 | `cargo test --locked --lib attest` | as above |

The tests fail without what they test (mutants on the box, the file restored after each, 23 November): the window's
end never reached (no code 53) fails `recorded_refusal_…` (`Inside` for `Ended`) and `stale_checkpoint_…` (exit 1 for
3); resume disabled fails `failing_endpoint_…` (the `advance` rerun exit 1); the endpoint check removed fails it (the
wrong chain id applied, exit 0 for 4); retries off fails it (one `eth_getProof` request for two). Script:
`spike/tmp/cli-rows/mutants.sh` (not in the repository).

## Proposed EVIDENCE row (next free number; 227 at `eb990490` + the other streams' rows)

Paths below are written for `docs/EVIDENCE.md` as plain paths; as links there (relative to `docs/`):
`../cli/tests/attest_faults.rs`, `../cli/src/attest.rs`, `../cli/results/attest-faults-r1/README.md`,
`../e2e/runs/ci-sequencer-attest-2026-11-21/README.md`.

| NEW | **The attestation CLI under a failing Ethereum endpoint and interrupted, nothing done twice; a stale checkpoint as code 53's message (RFP-021 R.5, U.5), in CI's `roundtrip`**: `ferrata attest` as a process against a stand-in zone (LEZ v0.4.0-rc1's block application, the program at genesis, on 127.0.0.1) and a scripted endpoint (recorded mainnet responses): refused connections, no answer, answers cut short (proof; receipts), 503, an error, another chain id, another block hash, each exit 4 with nothing sent and the command applied once afterwards; killed after an `advance`'s `Open`, while reading Ethereum, with its `Attest` in the pool and in a block not yet final, each run again signing nothing; LEZ's "Execution outside of the validity window" for `Init` and `Open` past the checkpoint's weak-subjectivity period shown as code 53's message with the checkpoint, the time its period ended and the remedy (a new bootstrap), from the dry run (not sent), the receipt and `receipt`; the same mapping on a real rc1 sequencer's recorded refusal | 30 commands: 14 exit 0, 4 exit 3, 8 exit 4, 4 SIGKILLed; 12 submissions, 11 transactions included, each once; `Attest` 274,613–275,925 execution gas; a refused `Init` charged 10,000,000 (the gas limit); 2 + 5 tests, 32.4 s | build box; stand-in zone (LEZ's state machine and block application, not its sequencer binary) | none: public transactions, no RISC Zero proof on these paths (`RISC0_DEV_MODE` set) | `cd cli && cargo test --locked --lib attest && RISC0_DEV_MODE=1 cargo test --locked --test attest_faults -- --nocapture` | `cli/results/attest-faults-r1/README.md` |

## Limits

- The zone is LEZ's state machine and block application behind a stand-in RPC, not LEZ's sequencer binary (no mempool
  limits, no Bedrock node); its block builder drops a transaction it rejects, as LEZ's does. The tool, its follower and
  its RPC client are the real ones. A run against a sequencer we start (`cli/tests/attest-seq.sh`) was not repeated.
- The journal needs `--state-dir` for `--send --yes`, and two runs of one command at the same time on one state
  directory are not guarded against each other.
- An `Open`'s window start is not known to `receipt` (it would need the update's layout): before the window opens,
  `receipt` prints LEZ's text as it is; the run itself names the start.
