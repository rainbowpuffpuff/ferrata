# Stream perf-p9: RFP-021 P.9 measured for every action, proposed row

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `perf-p9`
from `907736e8`, revised after two reviews (`review-perf-p9`: round 1, three Mediums and five Lows; round 2, two
Mediums and three Lows; all addressed below). Box runs: the Modal build box, 23 November 2026, RISC Zero dev mode where
a private transaction is settled; laptop: the `links` job's checks and the shard guard. Nothing was pushed or sent. No
guest, program or image id changed (only host code: the harness, its test, a bench binary and its script); no
dependency changed.

## The second review's findings and what was done

- **R2-M1 (the circuit's growth).** The circuit's cycles can be read: LEZ's API drops the prover's statistics, but r0vm
  logs every session's cycles (`RISC0_INFO=1 RUST_LOG=risc0_zkvm=info`) to the stdout it shares with its caller.
  `perf_state actions` now marks each private transaction on that stdout and `bench/perf/state-actions.sh` reads the
  circuits' sessions from it (`bench/perf/circuit-cycles.py`, new) into the record's last table: the private
  `VerifyAll`'s circuit 1,913,757 / 1,927,235 / 1,934,974 / 1,940,779 user cycles at 3 / 10 / 14 / 17 levels,
  `AttestPrivately`'s 1,975,409 → 2,002,530 and `ClaimDepositPrivately`'s 2,176,439 → 2,203,529 from 3 to 17: about
  1,935 a level (1,925 to 1,940 in the table's rows), about 62,000 at depth 32 (derived), the 14 more levels 1.4% of the
  circuit. The 3-level value moves by a few tens of cycles between runs (1,913,726 in the CI test's re-check, 1,913,757
  in the record; cause not established), far below one level. The wrong figure ("about 90 a level", "under 3,000 at
  depth 32") and "cannot be read" are gone from §P.9, this file, the row-tests note, the code's comments and
  `lez/results/p4-minimum/README.md`, where the figure came from (corrected there, with the measurement and the command).
- **R2-M2 (the row's scope).** The status row and §P.9 are the review's text ("Q2"), with this record's numbers: the row
  claims "every message the attestation program handles"; LEZ's commitment path is the one growth in the program's
  actions, justified (the module cannot avoid it); the reference consumer's private-holdings growth is reported as
  application-side and avoidable, not as justified.
- **R2-L1.** The holdings figure says what it counts: the claim's two private transitions, about 2,000–2,300 cycles a
  pool; LEZ's circuit's share is not measured.
- **R2-L2.** The "not measured" lists (§P.9, the Gaps line, the row-tests note, the status row) name the bridge and the
  soft-requirement programs (`statements`, `rebate`) and the second consumer (`ferrata_holdings`) again.
- **R2-L3.** `RealVault::load` asserts that the record the replayed instance wrote equals the one `RealVault::placed`
  places (`HeaderRecord::of` the fixture's finalized header, Gloas layout). `load` runs in CI through `vault`,
  `vault_consumer`, `history_hops` and `redteam_attest`: all pass on the box (2, 4, 2 and 11 of them).

The first review's findings (M1 commitments and the membership path, M2 the accumulator leaf, M3 the reference
consumer's other messages, L1–L5) are fixed as before: the private paths run with K more commitments, the leaf and the
consumer's `AttestPrivately`, `ClaimDeposit` and `ClaimDepositPrivately` are swept and pinned, the largest state is
143,615,971 B, the traceability counts table is in the edits below, the copied kinds are named (job accounts are not
copied), regenerating `perf-state.md` moves its §4a, §4 and §5's absolute sizes (+14,667 B; for RFP-022 P.2 and P.7's
owner), and the EVIDENCE row is 228.

## What changed

- **`lez/harness/src/accumulated.rs`** (new): the scenarios, each run on copies of one world, one of which also holds K
  more of the accounts that accumulate beside it (copies of accounts the programs wrote, at keys no measured transaction
  names), and every private transaction K more commitments. The accounts an action reads that change with use are
  measured as they change: a reader empty and holding the reply or record it read before, a balance or holder credited
  once or K + 1 times before, and a claimant's private holdings.
- **`perf_state actions` → [`lez/results/perf-state-actions.md`](../../lez/results/perf-state-actions.md)**
  (`bash bench/perf/state-actions.sh`), K = 0, 1,000, 10,000 and 100,000, every charge asserted equal to K = 0's before
  its row is printed; then the holdings table and the circuits table (not comparisons). `lez/results/perf-state.md` and
  `bench/perf/state.sh` are unchanged.
- **CI: `lez/harness/tests/costs_attestation.rs`** (job `lez-harness`, shard `harness-1`, no shard change): five new
  tests and one extended, every number pinned: `init_and_succeed_…`, `an_update_verified_in_one_call_…` (with the
  commitments), `the_reference_consumer_…` (also `AttestPrivately`; needs `RISC0_DEV_MODE=1`, as the CI command sets),
  `the_leaf_statement_…`, `the_vault_pattern_…` (dev mode; the vault's record placed rather than replayed), and
  `an_attestation_costs_…` for all eight statement types.
- `docs/performance.md`: §P.9 (the measurement; "What grows"); the Gaps line. `scripts/ci/row-tests.toml`: the P.9 entry.
  `lez/results/p4-minimum/README.md`: the commitment path's cost, measured.
- Minimal changes to shared harness code: `lez/harness/src/attest.rs` (`Fixture` gets a `Clone` that rebuilds its
  receipts trie; `Stage4` derives `Clone`), `lez/harness/src/lib.rs` (`pub mod accumulated`), `lez/harness/src/vault.rs`
  (`RealVault::placed`; `load` split into the replay and the data loading, building the same world, and asserting the
  placed record).

Commits since the second review: `dc493f31` (markers, the log parser, the `load` assert), `8776e635` (the markers on
stdout; the holdings texts; the record's commit), `52fb86e8` (the record), then the texts and this file.

## Every action and where each is shown

The clause: "No action's cycle count or gas cost scales with accumulated on-chain storage. Where a design cannot avoid
it, the proposal justifies why and documents the mitigations." Every message of the attestation program at two state
sizes or more (K = 1,000 in CI; to 100,000 in the record), and the reference consumer's, which is application code by
RFP-021's scope, measured the same way:

| Action | Execution gas (storage gas) | Shown by |
|---|---|---|
| `Open` → `JobOpen`, `Miller`, `Adopt` ×2, `Finish` → `Apply` → `Record` | flat steps (`Miller` 3,313,372) | CI `the_per_period_updates_…` (1 to 5 records); `perf-state.md` §3 (to 100,000) |
| `Init` → `JobOpen`, then two `Adopt`s (→ `Apply` → `Record`) | 210,666; 3,399,141; 3,485,575 | CI `init_and_succeed_…`; `perf-state-actions.md` |
| `Succeed` → `Spawn` | 70,610 (697) | same |
| `VerifyAll` public (→ `ApplyVerified` → `Record`) | 9,124,896 (4,105) | CI `an_update_verified_in_one_call_…`; record |
| `VerifyAll` private, the proven path | 485,298 (224,063); private execution 9,049,362 cycles; path 3 → 10 → 14 → 17 levels; circuit 1,913,757 → 1,940,779 | same (dev mode); the circuit from r0vm's log, record only |
| `Attest`: eight statement types (absent account 151,829; absent slot 270,580; four facts 454,255) | 97,485 to 1,619,960 | CI `an_attestation_costs_…`; record; five also `perf-state.md` §2 |
| `Attest`: the accumulator leaf and the vault's slots, header path and payload record | 122,067 to 646,014 | CI `the_leaf_statement_…`; record |
| `Read`, `ReadPadded` (via the consumer's `ReadRecord`, `ReadPadded`) | 39,410 (`Read` 12,268); 46,288 holding the record; 63,319 with 1 KiB | CI `the_reference_consumer_…`; record |
| consumer `CallAttest` | 386,181 empty reader; 395,003 holding a reply, and the call after | same |
| consumer `Claim` (→ `Credit`) | 185,389 first credit; 187,721 to a balance credited 1 or 100,001 times | same |
| consumer `AttestPrivately` (public part `Read`) | 422,032 (224,063); private 34,717 + 346,763 cycles; path 3 → 17; circuit 1,975,409 → 2,002,530 | same (dev mode) |
| consumer `ClaimDeposit` (→ `CreditHolder`) | 500,206 first credit; 502,821 to a holder credited 1 or 100,001 times (9,105) | CI `the_vault_pattern_…`; record |
| consumer `ClaimDepositPrivately` (public part `Read` → `Spend` → `CreditHolder`) | 456,847 (224,063); private 112,597 + 1,392,412 cycles; path 3 → 17; circuit 2,176,439 → 2,203,529 | same (dev mode) |
| consumer private-kind claim against the claimant's own holdings | its private transitions 1,523,845 (0 pools), 1,737,559 (100), 3,717,087 (1,000): **grows**, about 2,000–2,300 a pool; avoidable | same (pinned at 0 and 100) |

Commands: `bash bench/perf/state-actions.sh` (box, 301 s at `8776e635`, with r0vm's log); `cd lez && RISC0_DEV_MODE=1
cargo test -p harness --locked --test costs_attestation` (box, 9 of 9); the circuit's sessions in CI's test: `cd lez &&
RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=risc0_zkvm=info cargo test -p harness --locked --test costs_attestation
an_update_verified_in_one_call -- --nocapture` (1,913,726 and 1,927,235 user cycles at 3 and 10 levels). Largest state
measured: 143,615,971 B serialized.

What grows:
1. In the program's actions, one cost, LEZ's: a private transaction's circuit hashes its fee payer's commitment path,
   ⌈log2(all commitments)⌉ levels, about 1,935 cycles a level (measured), about 62,000 at depth 32 (derived), beside
   about 1.9M for the circuit and 9.05M for the private `VerifyAll`; the module cannot avoid it; at that size no
   mitigation is needed (the clause's second sentence).
2. In the reference consumer (application code by RFP-021's scope): a private-kind claim's private transitions grow by
   about 2,000–2,300 cycles a pool the claimant already holds, because the consumer keeps a claimant's private holdings
   as one list; LEZ's circuit, which hashes and encrypts that state, grows with it too (not measured). Avoidable: a
   claimant sends each pool's deposits to a private account of its own, or a consumer keeps one pool per private
   account. Disclosed, not justified. If the owner counts the reference consumers' actions under P.9, the row is in part
   until the consumer keeps one pool per private account, a change that moves the consumer's image id.

What the measurement cannot catch, as before: the copies sit at keys no measured transaction names, so equal charges
show that LEZ's metering does not depend on the rest of the state; the review's mutation B shows the comparison does
catch a named account that changes.

Not measured against accumulation: the bridge's actions (RFP-022's program; RFP-022 has no such clause, its accounts'
growth is its §P.7), and the soft-requirement programs (`statements`, `rebate`) and the second consumer
(`ferrata_holdings`), which are programs of their own.

## Rows

| Row | Now | Proposed |
|---|---|---|
| RFP-021 P.9 | in part | **shown** |

Proposed `docs/requirements-status.md` row (the second review's text, with this record's numbers):

| P.9 | shown | Every message the attestation program handles was charged the same handler cycles, execution and storage gas with more state accumulated (asserted): the update steps over five updates as records grow; `Init` with its adoption, `VerifyAll` public and private (the proven path), `Succeed`/`Spawn`, `Read`/`ReadPadded`, all eight statement types and the accumulator leaf, with 1,000 more of the accounts that accumulate beside them and, for the private path, 1,000 more commitments, pinned in CI; all of them to 100,000 (serialized state to 143.6 MB; `lez/results/perf-state.md` §2–§3, `perf-state-actions.md`). One cost of the private path grows, and it is LEZ's, which the module cannot avoid (the clause's second sentence): a private transaction's circuit hashes its fee payer's commitment path, 3 to 17 levels with 100,000 more commitments while the charge stays the same, at about 1,935 circuit cycles a level (1,913,757 at 3 levels, 1,940,779 at 17, from r0vm's session log; about 62,000 at depth 32, derived), small beside the circuit's 1.9M and the update's 9.05M, so no mitigation is needed. The reference consumer is application code by RFP-021's scope and was measured too: every message charged the same; one of its costs grows with the claimant's own private state, which it keeps as one list of pools: a private-kind claim's private transitions take about 2,000–2,300 more cycles per pool already held (1,523,845 with none, 3,717,087 with 1,000; the circuit's share not measured), which a claimant avoids by sending each pool's deposits to its own private account. Not measured: the bridge's actions (RFP-022's program), the soft-requirement programs (`statements`, `rebate`) and the second consumer (`ferrata_holdings`). | 170, 211, 228; `lez/results/perf-state-actions.md`; `lez/results/perf-state.md`; `lez/lab/README.md` |

Counts: RFP-021 hard 40 shown and 15 in part (from 39 and 16); Performance 6 shown, 3 in part (from 5 and 4); the
README's status line likewise ("40 and 41 shown, 15 and 24 in part", if no other stream moves a row).

Proposed `docs/traceability.md` (its links as that file writes them):
- The counts table under "Status of the hard requirements" (line 16): RFP-021 `| 40 | 15 | 1 | 3 | **59** |`.
- The P.9 row's evidence column: replace the sentence from "Not measured against accumulation
  (`[performance](performance.md)` §P.9): …" to "…in CI the update path only from 1 to 5 records." with: "Every other
  message of the program measured with 0 to 100,000 more of the accounts that accumulate beside it, the private path
  with as many more commitments, and pinned in CI with 1,000 (`[EVIDENCE](EVIDENCE.md)` row 228,
  `results/perf-state-actions.md`, `harness/tests/costs_attestation.rs`): `Init`, `VerifyAll` public and private,
  `Succeed`/`Spawn`, `Read`/`ReadPadded`, all eight statement types and the accumulator leaf; the reference consumer's
  messages too. What grows (`[performance](performance.md)` §P.9): LEZ's commitment path in a private transaction's
  circuit, about 1,935 cycles a level (justified: the module cannot avoid it); in the reference consumer, a private-kind
  claim's private transitions with the pools its claimant holds (avoidable, disclosed)." Add
  `results/perf-state-actions.md` to its records column.
- In the "in part" list, the bullet "R.5, P.9, a component untested and actions unmeasured": drop P.9 ("**R.5, a
  component untested** (in part). R.5: …").

`scripts/ci/row-tests.toml` (done on this branch): P.9 maps `costs_attestation.rs` (CI, `lez-harness`),
`bench/perf/state-actions.sh` and `bench/perf/state.sh` (box), the two records, `stage1-replay.md`, `p4-minimum/README.md`
and the lab's README; `check-row-tests.py` passes.

## Proposed EVIDENCE row 228

The Details column's paths, as links relative to `docs/` in `docs/EVIDENCE.md`. Main's last row is 227 and there is no
row 225; another stream may take 228 first.

| 228 | **Every other action charged the same against accumulated state (RFP-021 P.9)**: `Init` and its adoption, `VerifyAll` in a public and in a private transaction (the proven path), `Succeed` → `Spawn`, `Read` and `ReadPadded`, all eight statement types and the accumulator leaf, each on copies of one world holding 0, 1,000, 10,000 and 100,000 more of the accounts that accumulate beside it, the private path with as many more commitments; with EVIDENCE 170's update steps, every message of the attestation program; the reference consumer's messages (`CallAttest`, `Claim`, `AttestPrivately`, `ClaimDeposit`, `ClaimDepositPrivately` with `Spend` and `CreditHolder`) the same way. What grows: LEZ's circuit with the commitment path (justified) and, in the reference consumer, a private-kind claim with the claimant's holdings (avoidable) | every swept charge equal at every size (asserted): `Init` 210,666 + 3,399,141 + 3,485,575 execution gas; `VerifyAll` public 9,124,896, private 485,298 with the membership path 3, 10, 14 and 17 levels; `Succeed` → `Spawn` 70,610; the leaf statements 122,067 to 646,014; `Read` 12,268 cycles; `CallAttest` 386,181 / 395,003; the claim 185,389 / 187,721; `AttestPrivately` 422,032; `ClaimDeposit` 500,206 / 502,821; `ClaimDepositPrivately` 456,847; serialized state to 143,615,971 B. Grows: LEZ's circuit, about 1,935 user cycles a commitment-path level (1,913,757 / 1,927,235 / 1,940,779 at 3 / 10 / 17 levels, r0vm's session log); the reference consumer's private-kind claim's private transitions, about 2,000–2,300 cycles a pool held (1,523,845 to 3,717,087 from 0 to 1,000 pools). CI: 9 of 9 | the Modal build box; CI's test at 4 CPUs (`taskset -c 0-3`), dev mode | none (dev mode: the private transactions' receipts) | `bash bench/perf/state-actions.sh`; `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test costs_attestation`; `cd lez && RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=risc0_zkvm=info cargo test -p harness --locked --test costs_attestation an_update_verified_in_one_call -- --nocapture` | `lez/results/perf-state-actions.md`, `lez/harness/src/accumulated.rs`, `lez/harness/tests/costs_attestation.rs`, `bench/perf/circuit-cycles.py`, `lez/results/p4-minimum/README.md` |

## CI time

`costs_attestation` grows from 4 tests to 9: 18.8 s and 19.0 s at four CPUs on the box (`taskset -c 0-3`, four test
threads), against 11.2 s and 11.4 s for the four earlier tests alone, measured the same way the same day: +7.6 s (the
review measured +12.4 s on its box, with the same ratio; 27.8 s in this round's run, on a busier box). The vault test
runs on the vault's record placed, not replayed (`RealVault::placed`; 72.5 s alone on the replayed world). The shard as
CI runs it passed in 463 s while the vault test still replayed. Far from the workflow's critical path
(`updater-gate` / `updater-privacy`, 828–834 s). The shard map is unchanged; `check-lez-shards.py` and
`test_lez_shards.py` pass.

## Checks run

- Box (this round): `bash bench/perf/state-actions.sh` (exit 0, 301 s; the record at `8776e635`, 129 swept rows and the
  two tables after them); `costs_attestation` 9 of 9, `vault` 2, `vault_consumer` 4, `history_hops` 2, `redteam_attest`
  11 of 11 (the last four run `RealVault::load`'s new assert); the CI test's re-check under r0vm's log (above). `cargo
  fmt -p ferrata-lez-wire -p harness -p methods -- --check`: only the seven files `lez/RESULTS.md` §8 names.
  `RISC0_SKIP_BUILD=1 cargo clippy … --all-targets --keep-going -D warnings` (in `target/clippy`): only the pre-existing
  `harness/tests/captured_committee_exposure.rs:392`.
- Laptop: every step of the `links` job (`check-links.py`, `check-guides.py`, `check-decisions.py`, the Python tests,
  `check-row-tests.py`, `test_ci_model.py`, `check-lez-shards.py`, `test_lez_shards.py`, `ci_model.py --compare-yq`,
  `check-portable.py`, `test-sequencer-records.sh`, `check-unaudited.sh`): all exit 0.
