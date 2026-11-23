# Stream perf-p9: RFP-021 P.9 measured for every action, proposed row

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `perf-p9`
from `907736e8`. Box runs: the Modal build box, 23 November 2026, RISC Zero dev mode where a private transaction is
settled; laptop: the `links` job's checks and the shard guard. Nothing was pushed or sent. No guest, program or image
id changed (only host code: the harness, its test and a bench binary); no dependency changed.

## What changed

- **New: `lez/harness/src/accumulated.rs`.** Every action [performance](../performance.md) §P.9 listed as not
  measured, run on copies of one world, one of which also holds K more of each account that accumulates beside it
  (copies of accounts the programs wrote, at keys no measured transaction names: records and other instances; for the
  statements and the consumer, records and the consumer's spent claims, balances and readers). The accounts an action
  reads that change with use are measured as they change: a reader empty and holding the reply or record it read
  before, and a claimant's balance credited once or K + 1 times before.
- **`perf_state actions` → [`lez/results/perf-state-actions.md`](../../lez/results/perf-state-actions.md)**
  (`bash bench/perf/state-actions.sh`), K = 0, 1,000, 10,000 and 100,000, every charge asserted equal to K = 0's before
  its row is printed. `lez/results/perf-state.md` and `bench/perf/state.sh` are **unchanged**: regenerating that file
  on this tree moves its §4a and §4 bridge rows to milestone B1's program (`ImportRoot` 230,731 → 231,635, the claims
  286,627 → 291,261, the burns 164,842 → 156,678; the §2 and §3 charges identical), and RFP-022 §P.2 of
  `docs/performance.md` cites §4a as the deployed program's (`0xccde6171…`). So the new sweep has its own record.
- **CI: `lez/harness/tests/costs_attestation.rs`** (job `lez-harness`, shard `harness-1`, no shard change): three new
  tests and one extended, each action charged the same handler cycles, execution and storage gas on its scenario's
  chain and with 1,000 more of each accumulating account, every number pinned to the record:
  `init_and_succeed_cost_the_same_whatever_the_chain_holds`,
  `an_update_verified_in_one_call_costs_the_same_whatever_the_chain_holds` (needs `RISC0_DEV_MODE=1`, as the file's
  private test already does), `the_reference_consumer_costs_the_same_whatever_the_chain_holds`, and
  `an_attestation_costs_the_same_whatever_the_chain_holds` now for all eight statement types of
  [`stage4-attest.md`](../../lez/results/stage4-attest.md) (pinned to that record's rows 1–8).
- `docs/performance.md`: §P.9's "Not measured" sentence replaced by the measurement; the "Gaps" list's P.9 item
  narrowed to the bridge. `scripts/ci/row-tests.toml`: the P.9 entry (below).
- Minimal changes to shared harness code: `lez/harness/src/attest.rs` (`Fixture` gets a `Clone` that rebuilds its
  receipts trie, since `ferrata_spectests::execkit::Trie` is not `Clone`; `Stage4` derives `Clone`), `lez/harness/src/lib.rs`
  (`pub mod accumulated`).

Commits: `e29714e5`, `1ef48fbb`, `b0c8f53e`, `5c32fb91`, `e2222b54`, `72e99c6c`, `1adfc7b1` (the record's commit),
`564ad650` (rustfmt only), then this file.

## Every action and where each is shown

The clause: "No action's cycle count or gas cost scales with accumulated on-chain storage." Every message the
attestation program handles, and the reference consumer's, at two state sizes or more:

| Action | Charge (execution gas; storage gas) | Sizes | Shown by |
|---|---|---|---|
| `Open` → `JobOpen`, `Miller`, `Adopt` ×2, `Finish` → `Apply` → `Record` | flat steps (e.g. `Miller` 3,313,372) | 1 to 5 records; 0 to 100,000 | CI `the_per_period_updates_cost_what_the_record_shows_and_do_not_grow`; `perf-state.md` §3 (EVIDENCE 170, 211) |
| `Init` → `JobOpen`, then its two `Adopt`s (→ `Apply` → `Record`) | 210,666 / 3,399,141 / 3,485,575; 1,896 / 24,980 / 25,108 | 0 to 100,000 records of another instance and other instances | CI `init_and_succeed_…`; `perf-state-actions.md` |
| `VerifyAll` public (→ `ApplyVerified` → `Record`) | 9,124,896; 4,105 | 0 to 100,000 records and instances | CI `an_update_verified_in_one_call_…`; `perf-state-actions.md` |
| `VerifyAll` private, the proven path | 485,298 (flat 409,764 + public part 75,534); 224,063; private execution 9,049,362 cycles (executor) | same | same (dev mode) |
| `Succeed` → `Spawn` | 70,610 (45,512 + 25,098); 697 | same | CI `init_and_succeed_…`; `perf-state-actions.md` |
| `Attest`, all eight statement types (absent account 151,829; absent slot 270,580; four facts 454,255) | 97,485 to 1,619,960 | 2 and 1,002 records (CI); 0 to 100,000 records and consumer accounts | CI `an_attestation_costs_…`; `perf-state-actions.md`; five also `perf-state.md` §2 |
| `Read`, `ReadPadded` (via the consumer's `ReadRecord`, `ReadPadded`) | 39,410 (`Read` 12,268); 46,288 with the reader holding the record; `ReadPadded` with 1 KiB 63,319 | same | CI `the_reference_consumer_…`; `perf-state-actions.md` |
| `CallAttest` (→ `Attest` → reply) | 386,181 from an empty reader; 395,003 holding a reply, and the same on the next call; 7,507 | same | same |
| Reference consumer's `Claim` (→ `Attest` → reply → `Credit`) | 185,389 for a first credit; 187,721 to a balance credited once or 100,001 times; 2,768 | same | same |

Commands: `bash bench/perf/state-actions.sh` (box: 116 s at 1adfc7b1, build included); `cd lez && RISC0_DEV_MODE=1 cargo test -p harness
--locked --test costs_attestation` (box, `taskset -c 0-3`, `--test-threads 4`: 7 of 7, 31.2 s; 31.3 s and 31.6 s in two
earlier runs). Largest states measured: 115,429,006 B serialized (100,000 records and 100,000 instances) and
128,029,379 B (100,000 each of records, spent claims, balances and readers).

What the measurement cannot catch, as before: the copies sit at keys no measured transaction names, so equal charges
show that LEZ's metering does not depend on the rest of the state. The accounts an action does read are fixed in size
or replaced (the instance's committees as summaries, a job, one record, a reader's last reply, a balance's fixed-width
counters); the two kinds that change with use are measured as they change.

Not measured against accumulation, outside RFP-021's module and the listed reference-consumer actions: the bridge's
actions (RFP-022's program; RFP-022 has no such clause, its accounts' growth is its §P.7), the consumer program's vault
pattern (`ClaimDeposit`, `ClaimDepositPrivately`, `Spend`, `CreditHolder`), the private parts of `AttestPrivately` and
`ClaimDepositPrivately` (their public part is `Read`, measured; a claimant's `holdings` list in its private state grows
with the number of pools it uses), the holdings consumer and the soft-requirement programs (`statements`, `rebate`).

## Rows

| Row | Now | Proposed |
|---|---|---|
| RFP-021 P.9 | in part | **shown** |

Proposed `docs/requirements-status.md` row:

| P.9 | shown | Every message the attestation program handles, and the reference consumer's read, call and claim, charged the same handler cycles, execution and storage gas with more state accumulated (asserted): the update steps over five updates as records grow, all eight statement types with 2 and 1,002 record accounts, and `Init` with its adoption, `VerifyAll` public and private (the proven path), `Succeed`/`Spawn`, `Read`/`ReadPadded`, `CallAttest` and the claim with 1,000 more of each accumulating account, pinned in CI; all of them to 100,000 (serialized state to 128.0 MB; `lez/results/perf-state.md` §2–§3, `perf-state-actions.md`). The accounts an action reads that change with use measured as they change (a reader keeps one reply; a claim to a balance credited once or 100,001 times costs 187,721 either way). The bridge's actions (RFP-022's program) not measured against accumulation. | 170, 211, NEW; `lez/results/perf-state-actions.md`; `lez/results/perf-state.md`; `lez/lab/README.md` |

Counts: RFP-021 hard 40 shown and 15 in part (from 39 and 16); Performance 6 shown, 3 in part (from 5 and 4); the
README's status line likewise ("40 and 41 shown, 15 and 24 in part", if no other stream moves a row).

Proposed `docs/traceability.md` (its links as that file writes them): in the P.9 row's evidence column, replace the sentence from "Not measured against
accumulation (`[performance](performance.md)` §P.9): …" to "…in CI the update path only from 1 to 5 records." with:
"Every other action measured with 0 to 100,000 more of each accumulating account and pinned in CI with 1,000
(`[EVIDENCE](EVIDENCE.md)` row NEW, `results/perf-state-actions.md`, `harness/tests/costs_attestation.rs`): `Init`,
`VerifyAll` public and private, `Succeed`/`Spawn`, `Read`/`ReadPadded`, all eight statement types, `CallAttest` and the
reference consumer's claim; with the update steps, every message the program handles." Add
`results/perf-state-actions.md` to its records column. In the "in part" list (the bullet "R.5, P.9, a component untested
and actions unmeasured"), drop P.9: "**R.5, a component untested** (in part). R.5: …".

`scripts/ci/row-tests.toml` (done on this branch): P.9 maps `costs_attestation.rs` (CI, `lez-harness`),
`bench/perf/state-actions.sh` and `bench/perf/state.sh` (box), the two records, `stage1-replay.md` and the lab's README;
`check-row-tests.py` passes.

## Proposed EVIDENCE row

The Details column's paths, as links relative to `docs/` in `docs/EVIDENCE.md`; the number is the next free one (225–227 are taken at `907736e8`; other streams may take 228).

| NEW | **Every other action charged the same against accumulated state (RFP-021 P.9)**: `Init` and its adoption, `VerifyAll` in a public and in a private transaction (the proven path), `Succeed` → `Spawn`, `Read` and `ReadPadded`, all eight statement types (the absent account, the absent slot and the four facts among them), `CallAttest` and the reference consumer's claim, each on copies of one world holding 0, 1,000, 10,000 and 100,000 more of each account that accumulates beside it; with EVIDENCE 170's update steps, every message the attestation program handles | every charge equal at every size (asserted): `Init` 210,666 + 3,399,141 + 3,485,575 execution gas; `VerifyAll` public 9,124,896, private 485,298 (its private execution 9,049,362 cycles); `Succeed` → `Spawn` 70,610; `Read` 12,268 cycles; `CallAttest` 386,181 from an empty reader, 395,003 holding a reply; the claim 185,389 for a first credit, 187,721 to a balance credited once or 100,001 times; serialized state to 128,029,379 B. CI: 7 of 7 (31.2 s) | the Modal build box; CI's test at 4 CPUs (`taskset -c 0-3`), dev mode | none (dev mode: the private path's receipt) | `bash bench/perf/state-actions.sh`; `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test costs_attestation` | `lez/results/perf-state-actions.md`, `lez/harness/src/accumulated.rs`, `lez/harness/tests/costs_attestation.rs` |

## CI time

`costs_attestation` grows from 4 tests to 7: 31.2 s at four CPUs on the box (the new tests alone: 11.2, 13.4 and 14.8 s, the last before the reads were added to it;
the replay, the longest, 20.5 s alone on the same day; EVIDENCE 211's 15.1 s for the four earlier tests was another
day's box). About +11 s for the binary in shard `harness-1` (derived from the per-test runs), which stays far from the
workflow's critical path (`updater-gate` / `updater-privacy`, 828–834 s; `docs/guides/ci.md`). The shard map is
unchanged (no new test binary); `check-lez-shards.py` and `test_lez_shards.py` pass.

## Checks run

- Box: `bash bench/perf/state-actions.sh` (exit 0); `costs_attestation` 7 of 7; `cargo fmt -p ferrata-lez-wire -p harness
  -p methods -- --check`: only the seven files `lez/RESULTS.md` §8 names as unformatted before (none of this stream's);
  `RISC0_SKIP_BUILD=1 cargo clippy -p ferrata-lez-wire -p harness -p methods --all-targets --locked --keep-going --
  -D warnings` (clippy 0.1.98, in its own target directory, so no zero-id guest outputs reach the main one): clean
  but for one pre-existing error in a file this stream did not touch, `harness/tests/captured_committee_exposure.rs:392`
  (`needless_range_loop`), unchanged since `907736e8`.
- Laptop: every step of the `links` job (`check-links.py`, `check-guides.py`, `check-decisions.py`, the Python tests,
  `check-row-tests.py`, `test_ci_model.py`, `check-lez-shards.py`, `test_lez_shards.py`, `ci_model.py --compare-yq`,
  `check-portable.py`, `test-sequencer-records.sh`, `check-unaudited.sh`): all exit 0.
