# Stream perf-p9: RFP-021 P.9 measured for every action, proposed row

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `perf-p9`
from `907736e8`, revised after the first review (`review-perf-p9`: three Mediums, five Lows, all addressed below). Box
runs: the Modal build box, 23 November 2026, RISC Zero dev mode where a private transaction is settled; laptop: the
`links` job's checks and the shard guard. Nothing was pushed or sent. No guest, program or image id changed (only host
code: the harness, its test and a bench binary); no dependency changed.

## The review's findings and what was done

- **M1 (the private paths' cycles and LEZ's commitment set).** Every private transaction now also runs with K more
  commitments in LEZ's commitment set (`accumulated::add_commitments`), and the fee payer's membership path, which
  LEZ's circuit hashes one SHA-256 per level, is measured: 3 levels as the scenarios leave the chain, 10 with 1,000
  more commitments (CI), 17 with 100,000 (record), with every private transaction charged exactly the same. The
  circuit's own cycles cannot be read in the harness (LEZ drops the prover's statistics), so the growth is stated and
  derived, in `docs/performance.md` §P.9, the status row and the EVIDENCE row: about 90 cycles a level, under 3,000 at
  depth 32 (`lez/results/p4-minimum/README.md`), LEZ's own, unavoidable for the module, no mitigation needed. The texts
  now also say that `VerifyAll`'s private execution reads no chain state (the store travels in its message), so its
  9,049,362 cycles are not a measurement against accumulation.
- **M2 (the accumulator leaf).** New scenario `accumulated::LeafWorld` (the synthetic vault, `harness::vault`): slots 0
  and 1 alone, with one, two and eight deposit leaves under a record that binds a block hash (the `Request.header`
  path), and one leaf under a record that holds the payload's fields, swept to 100,000 records and pinned in CI by
  `the_leaf_statement_costs_the_same_whatever_the_chain_holds` to `lez/results/stage5-vault-synthetic.md` rows 1–5
  (122,067; 187,786; 253,107; 646,014; 167,123: all equal).
- **M3 (the reference consumer's other messages).** All measured: `AttestPrivately` (Stage 4 chain, dev mode), and the
  vault pattern on FerrataVault v5's real deposits (`accumulated::VaultWorld`): `ClaimDeposit` → `CreditHolder` for a
  first credit and to a holder credited before, and `ClaimDepositPrivately` with its public `Read` → `Spend` →
  `CreditHolder`. Pinned in CI by `the_vault_pattern_costs_the_same_whatever_the_chain_holds` and in
  `the_reference_consumer_costs_the_same_whatever_the_chain_holds`; every figure that an earlier record holds matches it
  (`stage5-consumer-public.md` row 1: 500,206; `stage5-consumer-private.md`: 112,597 + 1,392,412 private cycles, 456,847
  execution gas; `docs/performance.md` §P.1: `AttestPrivately` 422,032). The claimant's private `holdings` list was
  measured as it grows: a private-kind claim's own proof costs about 2,100 cycles more per pool held (1,523,845 with
  none; 1,737,559 with 100; 3,717,087 with 1,000). §P.9 states it with why it may (the user's own proving on their own
  state, bounded by the pools they hold, unseen by LEZ's charge) and two mitigations. The Gaps line and every "not
  measured" list now name the bridge only, and §P.9 says the consumer's messages are application code by RFP-021's
  scope.
- **L1.** The largest state is now 143,615,971 B (the private read with 100,000 more records, consumer accounts and
  commitments); all texts give it, or "143.6 MB".
- **L2.** The counts table of `docs/traceability.md` (line 16) is in the proposed edits below.
- **L3.** The texts name what was copied (records, instances; spent claims, balances, holders, readers; commitments) and
  that job accounts are not; the row-tests note says `state.sh` sweeps records only.
- **L4.** Regenerating `perf-state.md` on this tree moves §4a, §4 and §5's absolute serialized sizes (+14,667 B, the
  bridge program's ELF; the per-record 165 and 273 B stay): for whoever owns RFP-022 P.2 and P.7.
- **L5.** EVIDENCE has no row 225 (it goes 224, 226, 227): the proposed row is 228, if no other stream takes it first.

## What changed

- **`lez/harness/src/accumulated.rs`** (new): the scenarios, each run on copies of one world, one of which also holds K
  more of the accounts that accumulate beside it (copies of accounts the programs wrote, at keys no measured transaction
  names), and every private transaction K more commitments. The accounts an action reads that change with use are
  measured as they change: a reader empty and holding the reply or record it read before, a balance or holder credited
  once or K + 1 times before, and a claimant's private holdings.
- **`perf_state actions` → [`lez/results/perf-state-actions.md`](../../lez/results/perf-state-actions.md)**
  (`bash bench/perf/state-actions.sh`), K = 0, 1,000, 10,000 and 100,000, every charge asserted equal to K = 0's before
  its row is printed, then the holdings table (not a comparison). `lez/results/perf-state.md` and `bench/perf/state.sh`
  are unchanged (L4).
- **CI: `lez/harness/tests/costs_attestation.rs`** (job `lez-harness`, shard `harness-1`, no shard change): five new
  tests and one extended, every number pinned: `init_and_succeed_…`, `an_update_verified_in_one_call_…` (with the
  commitments), `the_reference_consumer_…` (now also `AttestPrivately`; needs `RISC0_DEV_MODE=1`, as the CI command sets),
  `the_leaf_statement_…`, `the_vault_pattern_…` (dev mode), and `an_attestation_costs_…` for all eight statement types.
- `docs/performance.md`: §P.9 (the measurement; "What grows, and why it may"); the Gaps line. `scripts/ci/row-tests.toml`:
  the P.9 entry.
- Minimal changes to shared harness code: `lez/harness/src/attest.rs` (`Fixture` gets a `Clone` that rebuilds its
  receipts trie; `Stage4` derives `Clone`), `lez/harness/src/lib.rs` (`pub mod accumulated`), `lez/harness/src/vault.rs`
  (`RealVault::placed`: the fixture's data under the record placed rather than replayed; `load` split into the replay
  and that data loading, building the same world, `vault_consumer` 4 of 4).

Commits since the first review: `bc39a01d` (the scenarios), `4558af81` (CI pins; the record's commit), `6584d159` (`RealVault::placed`), `2cefdb43` (texts, rustfmt), then this file.

## Every action and where each is shown

The clause: "No action's cycle count or gas cost scales with accumulated on-chain storage. Where a design cannot avoid
it, the proposal justifies why and documents the mitigations." Every message of the attestation program, and every
message of the reference consumer, at two state sizes or more (K = 1,000 in CI; to 100,000 in the record):

| Action | Execution gas (storage gas) | Shown by |
|---|---|---|
| `Open` → `JobOpen`, `Miller`, `Adopt` ×2, `Finish` → `Apply` → `Record` | flat steps (`Miller` 3,313,372) | CI `the_per_period_updates_…` (1 to 5 records); `perf-state.md` §3 (to 100,000) |
| `Init` → `JobOpen`, then two `Adopt`s (→ `Apply` → `Record`) | 210,666; 3,399,141; 3,485,575 | CI `init_and_succeed_…`; `perf-state-actions.md` |
| `Succeed` → `Spawn` | 70,610 (697) | same |
| `VerifyAll` public (→ `ApplyVerified` → `Record`) | 9,124,896 (4,105) | CI `an_update_verified_in_one_call_…`; record |
| `VerifyAll` private, the proven path | 485,298 (224,063); private execution 9,049,362 cycles; membership path 3 → 10 → 17 levels | same (dev mode) |
| `Attest`: eight statement types (absent account 151,829; absent slot 270,580; four facts 454,255) | 97,485 to 1,619,960 | CI `an_attestation_costs_…`; record; five also `perf-state.md` §2 |
| `Attest`: the accumulator leaf and the vault's slots, header path and payload record | 122,067 to 646,014 | CI `the_leaf_statement_…`; record |
| `Read`, `ReadPadded` (via the consumer's `ReadRecord`, `ReadPadded`) | 39,410 (`Read` 12,268); 46,288 holding the record; 63,319 with 1 KiB | CI `the_reference_consumer_…`; record |
| consumer `CallAttest` | 386,181 empty reader; 395,003 holding a reply, and the call after | same |
| consumer `Claim` (→ `Credit`) | 185,389 first credit; 187,721 to a balance credited 1 or 100,001 times | same |
| consumer `AttestPrivately` (public part `Read`) | 422,032 (224,063); private 34,717 + 346,763 cycles; path 3 → 10 → 17 | same (dev mode) |
| consumer `ClaimDeposit` (→ `CreditHolder`) | 500,206 first credit; 502,821 to a holder credited 1 or 100,001 times (9,105) | CI `the_vault_pattern_…`; record |
| consumer `ClaimDepositPrivately` (public part `Read` → `Spend` → `CreditHolder`) | 456,847 (224,063); private 112,597 + 1,392,412 cycles; path 3 → 10 → 17 | same (dev mode) |
| consumer private-kind claim against the claimant's own holdings | private cycles 1,523,845 (0 pools), 1,737,559 (100), 3,717,087 (1,000): **grows**, ~2,100 a pool | same (pinned at 0 and 100) |

Commands: `bash bench/perf/state-actions.sh` (box, 177 s at `4558af81`); `cd lez && RISC0_DEV_MODE=1 cargo test -p
harness --locked --test costs_attestation` (box, `taskset -c 0-3`, `--test-threads 4`: 9 of 9, 18.8 s and 19.0 s; the four earlier tests alone 11.2 s and 11.4 s). Largest state
measured: 143,615,971 B serialized (100,000 more records, spent claims, balances and readers, and 100,000 commitments).

What grows, and why it may (both stated in §P.9, neither the module's, neither seen by LEZ's charge):
1. LEZ's commitment path: one SHA-256 a level, ⌈log2(all commitments)⌉ levels; under 3,000 cycles at depth 32
   (derived, `p4-minimum/README.md`) against ~1.86M for a bare private transfer's circuit and 9.05M for the private
   `VerifyAll`; unavoidable for the module, no mitigation needed.
2. The reference consumer's private holdings: ~2,100 cycles of the claimant's own proof per pool it holds privately;
   bounded by the pools a user holds; mitigations: separate private accounts per pool, or a consumer that keeps one pool
   per private account.

What the measurement cannot catch, as before: the copies sit at keys no measured transaction names, so equal charges
show that LEZ's metering does not depend on the rest of the state; the review's mutation B shows the comparison does
catch a named account that changes.

Scope: RFP-021's out-of-scope list puts application actions in the consuming program, so P.9 is read as the attestation
program's actions; the reference consumer's are measured as well. Not measured against accumulation: the bridge's
actions (RFP-022's program; RFP-022 has no such clause, its accounts' growth is its §P.7).

## Rows

| Row | Now | Proposed |
|---|---|---|
| RFP-021 P.9 | in part | **shown** |

Proposed `docs/requirements-status.md` row:

| P.9 | shown | Every message the attestation program handles, and every message of the reference consumer, charged the same handler cycles, execution and storage gas with more state accumulated (asserted): the update steps over five updates as records grow; all eight statement types and the accumulator leaf, `Init` with its adoption, `VerifyAll` public and private (the proven path), `Succeed`/`Spawn`, `Read`/`ReadPadded`, `CallAttest`, `Claim`, `AttestPrivately`, `ClaimDeposit` and `ClaimDepositPrivately` (with `Spend`, `CreditHolder`) with 1,000 more of the accounts that accumulate beside them, and for the private ones 1,000 more commitments, pinned in CI; all of them to 100,000 (serialized state to 143.6 MB; `lez/results/perf-state.md` §2–§3, `perf-state-actions.md`). What grows is stated with why it may: LEZ's commitment path in every private transaction's circuit (3 to 17 levels measured, charge unchanged; under 3,000 cycles at depth 32, derived) and the reference consumer's private holdings (~2,100 cycles of the claimant's own proof per pool held, measured; mitigations given). Not measured: the bridge's actions (RFP-022's program). | 170, 211, 228; `lez/results/perf-state-actions.md`; `lez/results/perf-state.md`; `lez/lab/README.md` |

Counts: RFP-021 hard 40 shown and 15 in part (from 39 and 16); Performance 6 shown, 3 in part (from 5 and 4); the
README's status line likewise ("40 and 41 shown, 15 and 24 in part", if no other stream moves a row).

Proposed `docs/traceability.md` (its links as that file writes them):
- The counts table under "Status of the hard requirements" (line 16): RFP-021 `| 40 | 15 | 1 | 3 | **59** |`.
- The P.9 row's evidence column: replace the sentence from "Not measured against accumulation
  (`[performance](performance.md)` §P.9): …" to "…in CI the update path only from 1 to 5 records." with: "Every other
  message of the program and of the reference consumer measured with 0 to 100,000 more of the accounts that accumulate
  beside it, and the private ones with as many more commitments, and pinned in CI with 1,000 (`[EVIDENCE](EVIDENCE.md)`
  row 228, `results/perf-state-actions.md`, `harness/tests/costs_attestation.rs`): `Init`, `VerifyAll` public and
  private, `Succeed`/`Spawn`, `Read`/`ReadPadded`, all eight statement types and the accumulator leaf, `CallAttest`,
  `Claim`, `AttestPrivately`, `ClaimDeposit`, `ClaimDepositPrivately`. What grows, stated with why it may
  (`[performance](performance.md)` §P.9): LEZ's commitment path in a private transaction's circuit, and the reference
  consumer's private holdings." Add `results/perf-state-actions.md` to its records column.
- In the "in part" list, the bullet "R.5, P.9, a component untested and actions unmeasured": drop P.9 ("**R.5, a
  component untested** (in part). R.5: …").

`scripts/ci/row-tests.toml` (done on this branch): P.9 maps `costs_attestation.rs` (CI, `lez-harness`),
`bench/perf/state-actions.sh` and `bench/perf/state.sh` (box), the two records, `stage1-replay.md`, `p4-minimum/README.md`
and the lab's README; `check-row-tests.py` passes.

## Proposed EVIDENCE row 228

The Details column's paths, as links relative to `docs/` in `docs/EVIDENCE.md`. Main's last row is 227 and there is no
row 225; another stream may take 228 first.

| 228 | **Every other action charged the same against accumulated state (RFP-021 P.9)**: `Init` and its adoption, `VerifyAll` in a public and in a private transaction (the proven path), `Succeed` → `Spawn`, `Read` and `ReadPadded`, all eight statement types and the accumulator leaf, and every message of the reference consumer (`CallAttest`, `Claim`, `AttestPrivately`, `ClaimDeposit`, `ClaimDepositPrivately` with `Spend` and `CreditHolder`), each on copies of one world holding 0, 1,000, 10,000 and 100,000 more of the accounts that accumulate beside it, the private ones with as many more commitments; with EVIDENCE 170's update steps, every message of the attestation program. What grows: LEZ's commitment path (derived cycles) and the reference consumer's private holdings (measured) | every swept charge equal at every size (asserted): `Init` 210,666 + 3,399,141 + 3,485,575 execution gas; `VerifyAll` public 9,124,896, private 485,298 with the membership path 3, 10 and 17 levels; `Succeed` → `Spawn` 70,610; the leaf statements 122,067 to 646,014; `Read` 12,268 cycles; `CallAttest` 386,181 / 395,003; the claim 185,389 / 187,721; `AttestPrivately` 422,032; `ClaimDeposit` 500,206 / 502,821; `ClaimDepositPrivately` 456,847; serialized state to 143,615,971 B. Grows: the commitment path, under 3,000 cycles at depth 32 (derived); a private-kind claim's own proof, ~2,100 cycles per pool held (1,523,845 to 3,717,087 from 0 to 1,000 pools). CI: 9 of 9 (18.8 s at four CPUs) | the Modal build box; CI's test at 4 CPUs (`taskset -c 0-3`), dev mode | none (dev mode: the private transactions' receipts) | `bash bench/perf/state-actions.sh`; `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test costs_attestation` | `lez/results/perf-state-actions.md`, `lez/harness/src/accumulated.rs`, `lez/harness/tests/costs_attestation.rs`, `lez/results/p4-minimum/README.md` |

## CI time

`costs_attestation` grows from 4 tests to 9: 18.8 s and 19.0 s at four CPUs on the box (`taskset -c 0-3`, four test
threads), against 11.2 s and 11.4 s for the four earlier tests alone, measured the same way the same day: **+7.6 s**
(the review measured +5.9 s for the first version). Alone: the vault pattern 4.9 s, the reference consumer 11.6 s,
`init_and_succeed_…` 6.1 s, the update in one call 7.9 s, the leaf 1.8 s. The vault test runs on the vault's record
placed, not replayed (`RealVault::placed`): on the replayed world it took 72.5 s alone, and its pins are the replayed
world's figures, so CI also checks that the placed record is the one the instance writes. The shard as CI runs it
(`FERRATA_TEST_THREADS=4 bash scripts/ci/lez-harness.sh --shard harness-1`) passed in 463 s while the vault test still
replayed (this binary 79.2 s then; about 60 s less now, derived). Far from the workflow's critical path (`updater-gate`
/ `updater-privacy`, 828–834 s). The shard map is unchanged (no new test binary); `check-lez-shards.py` and
`test_lez_shards.py` pass.

## Checks run

- Box: `bash bench/perf/state-actions.sh` (exit 0; the record at `4558af81`); `costs_attestation` 9 of 9, twice;
  `vault_consumer` 4 of 4 after `RealVault::load`'s refactor (the replay and the data loading split, `load` unchanged in
  what it builds); shard `harness-1` passed (463 s). That shard run rewrote other tests' committed records
  (`bridge-burn.md`, `redteam-2026-11-18/*.json`, `tenant/*.txt`: B1's drift, as the review found); they were restored,
  the diff kept in this stream's scratch directory. `cargo fmt -p ferrata-lez-wire -p harness -p methods -- --check`:
  only the seven files `lez/RESULTS.md` §8 names. `RISC0_SKIP_BUILD=1 cargo clippy … --all-targets --keep-going -D
  warnings` (clippy 0.1.98, in `target/clippy`): only the pre-existing `harness/tests/captured_committee_exposure.rs:392`.
- Laptop: every step of the `links` job (`check-links.py`, `check-guides.py`, `check-decisions.py`, the Python tests,
  `check-row-tests.py`, `test_ci_model.py`, `check-lez-shards.py`, `test_lez_shards.py`, `ci_model.py --compare-yq`,
  `check-portable.py`, `test-sequencer-records.sh`, `check-unaudited.sh`): all exit 0.
