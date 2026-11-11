# Milestone A1: successors, the root id and the weak-subjectivity bound

Branch `lc-a1` (from `integration6` at `f17b472`). Measured on the build box (Modal CPU container,
profile `ws-5`), 11 November 2026, RISC Zero dev mode (executor, no proofs), in the in-process
harness over LEZ v0.4.0-rc1's state machine and fee settlement. Design: `docs/design/attestation.md`
§3.5, §3.6, §4.6. Core: `crates/ferrata-core/src/ws.rs`, rows in `docs/REVIEW-core.md` (last section).

## What was built

**The weak-subjectivity bound, in the core** (`ferrata_core::ws`): the specification's
`compute_weak_subjectivity_period` (phase 0, and Electra's, with `get_balance_churn_limit`) and
`is_within_weak_subjectivity_period`, with the store's finalized header standing for the
weak-subjectivity state (consensus-specs v1.7.0-beta.4, `specs/phase0/weak-subjectivity.md`,
`specs/electra/weak-subjectivity.md`); `weak_subjectivity_end_slot`, the first slot outside the
period; `FinalizedStore::with_weak_subjectivity_period`, after which `process_update` refuses every
update once the caller's current slot is past the period of the finalized header
(`OutsideWeakSubjectivityPeriod`, code 53). The documented recovery is a new store from a bootstrap of a
trusted checkpoint inside the period.

**In the program** (`lez/guests/program`, `lez/wire`): `Config::ws_period_epochs` (nonzero, part of the
instance id; computed off chain from the checkpoint state's total active balance). Every transaction
that creates or extends an instance (`Init`, `Open`, `VerifyAll`, `Succeed`) carries a timestamp
validity window ending where the period of the instance's finalized header ends
(`Config::ws_end_ms`), so the sequencer refuses it afterwards; an update signed at or after that end is
refused in the guest (53).

**Successors and the root id.** `Succeed { config, checkpoint }` at `instance(parent)` checks the three
invariants of §3.5 and sends `Spawn` to `instance(successor_id)`, with
`successor_id = H(root id ‖ parent id ‖ config ‖ checkpoint)` in a hash domain of its own; `Spawn`,
accepted only from the parent, creates the successor active with the parent's store and root id.
`InstanceState` has `root_id` and `parent_id`; `RecordState` and the `Record` message carry the root
id; `Attest` serves a request bound to the root id as well as one bound to the record's instance.
New program codes: `BAD_SUCCESSOR` 218, `STALE_CHECKPOINT` 219. New event: `SUCCEEDED_SELECTOR`
(`FRT-SUC1`: successor id, root id, checkpoint slot).

## Why a clock

The specification's period rule keeps a valid update's signature slot within two sync-committee
periods of the store's finalized slot (`PeriodSkipped` otherwise): 512 epochs on mainnet. The
weak-subjectivity period is never below `MIN_VALIDATOR_WITHDRAWABILITY_DELAY` (256 epochs) and is
3,532 epochs on mainnet for a total active balance of 8,388,608 ETH or more (the specification's
table). Comparing an update's own signature slot with the store's finalized slot would therefore
refuse nothing on mainnet; the design's "with no clock needed" was wrong. The bound uses the
sequencer's clock, through the validity window LEZ already enforces for `Open`'s start. Trust
consequence: a sequencer whose clock runs late can let a stale store be advanced; one whose clock runs
early can only refuse.

## Results

Core (`cargo test --workspace --locked`, root workspace, build box): **163 passed, 0 failed** (150
before; 13 new in `tests/weak_subjectivity.rs`): both specification tables (6 Electra rows, 12 phase 0
rows), the two phase 0 branches the table does not reach, the churn limit, arithmetic failures, the
predicate at its edges and past `u64`, the end slot against the predicate (sweep), a store past its
period refusing a valid supermajority update (store unchanged) while the same store without a period
applies it, recovery from a newer bootstrap, and the bound changing nothing on the seven
`light_client_sync` cases (Altair to Gloas). The 52/52 `sync` vectors and the Helios comparison pass
unchanged in the same run.

Harness (`cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 4`, build box):
**50 passed, 0 failed** (3 of them new, in `tests/successor.rs`).

`a_successor_crosses_a_fork_not_in_the_schedule_fixed_at_init`, on real Sepolia data: a parent
initialised from the bootstrap of period 1378 with Sepolia's schedule before Gloas was scheduled
(Gloas at `FAR_FUTURE_EPOCH`) follows period 1378, refuses the first update attested under Gloas
(period 1379, exit 61 `ForkMismatch`), and a successor with the schedule that has Gloas (epoch 353,024)
takes periods 1379, 1380 and 1381, its store upgrading itself to the Gloas layout once, every finalized
root equal to the canonical one, every record carrying the parent's id as root id.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | result |
|---|---|---|---:|---:|---:|---|
| 1 | Succeed | Succeed 45,512 → Spawn 25,098 | 70,610 | 1% | 697 | applied |
| 2 | successor: period 1379 Open | Open 1,989,972 → JobOpen 25,468 | 2,015,440 | 40% | 2,363 | applied |
| 3 | successor: period 1379 Miller | Miller 3,310,470 | 3,310,470 | 66% | 399 | applied |
| 4–5 | successor: period 1379 adopt 1/2, 2/2 | Adopt 3,408,111; 3,415,303 | | 68% | 24,980 each | applied |
| 6 | successor: period 1379 Finish (the upgrade) | Finish 3,652,003 → Apply 66,300 → Record 16,310 | 3,734,613 | 75% | 527 | applied |
| 7–16 | periods 1380 and 1381, five transactions each | as above (Apply 63,245) | ≤ 3,731,558 | ≤ 75% | | applied |

Every invariant refused (same test):

| transaction | execution gas | result |
|---|---:|---|
| Succeed from an earlier record (the bootstrap's) | 28,323 | exit 219 `StaleCheckpoint` |
| Succeed with a changed / a shorter weak-subjectivity bound | 28,900 / 28,900 | exit 218 `BadSuccessor` |
| … another chain id / `genesis_validators_root` / genesis time / adoption parts | 28,598 / 28,644 / 28,876 / 28,891 | exit 218 |
| … a settled fork at another epoch / with another version | 29,757 / 29,302 | exit 218 |
| … a fork inserted before the checkpoint / a schedule out of order | 29,844 / 29,869 | exit 218 |
| … another store layout / the parent's own configuration | 28,901 / 29,989 | exit 218 |
| Succeed at the end of the period (the parent's record stale) | 10,000,000 (the limit) | LEZ: execution outside of the validity window |
| Succeed one millisecond earlier (another future schedule) | | applied |
| Succeed again for the same successor | | exit 203 `AlreadyInitialized` |
| Spawn sent directly, not by the parent | 12,256 | exit 202 `WrongSender` |
| Attest bound to the root id, at the successor's record (empty request) | 14,225 | exit 215 `HeaderRequired`: past the binding |
| Attest bound to the successor (empty request) | 14,006 | exit 215, the same |
| Attest bound to another instance | 13,771 | exit 211 `WrongInstance` |
| Succeed from the successor (Heze scheduled too; layout Gloas) | | applied: the second successor's root id is still the parent's, its parent the first successor |

`the_weak_subjectivity_bound_on_init_and_open` (Sepolia, the harness's period of 3,532 epochs, and
1 epoch for the last row):

| transaction | execution gas | result |
|---|---:|---|
| Init at the end of the checkpoint's period | the limit | LEZ: outside the validity window; no instance |
| Init with `ws_period_epochs` 0 | | exit 212 `BadConfig` |
| Open at the end of the stored header's period | 10,000,000 | LEZ: execution outside of the validity window |
| Open in the period's last millisecond | 2,156,426 | applied |
| Open of an update signed after the period (1-epoch period) | 2,117,345 | exit 53 `OutsideWeakSubjectivityPeriod` |

What the change costs elsewhere (the harness rewrites `lez/results/*.md` when it runs; those shared
files are left as they were on `integration6` and the rewrite is kept in the agent's scratch): `Attest`
reads a record 32 bytes longer and compares the root id, about 1,000 cycles more (the bridge's `Init`
168,776 → 169,841 execution gas; `ferrata_holdings`' refused `Attest` 38,360 → 39,317).

## Mutants

`scripts/mutants.py` (115: the 107 of before, M01's snippet moved with the new field, and M108–M115 on
the bound) and `scripts/review-mutants.py` (24), run whole on the build box in five shards at once
(each a scratch copy and the whole workspace's tests, as the scripts do): **139 killed, 0 survived, none
skipped or failing to compile** (2,193 s, 7.5 core-hours). The new ones: M108 (no bound in
`process_update`), M109 (`with_weak_subjectivity_period` sets nothing), M110 (the last epoch outside),
M111 (an end past `u64` reached), M112 (the end slot an epoch early), M113 (the churn not rounded to an
increment), M114 (Electra's factor 2 dropped), M115 (phase 0's top-ups term dropped; killed by the test of
the branches the specification's table does not reach).

## Image ids (built in this worktree on the build box; they embed the build path)

Of the final source (after `rustfmt`; the program's id before it was `7d495478…115e`, same size), from
`tests/successor.rs::prints_the_image_ids`. The tables above are of the run before `rustfmt`; the
successor, Sepolia, exit-code and public-path tests were re-run on the final program, 14 passed.

| guest | image id | ELF bytes |
|---|---|---:|
| program (`lez/guests/program`) | `ae82d5c1d7a07b9ef02c6b86015680cf98a6cc34e5f2a78af1cdd7bf84d0e10d` | 627,792 |
| consumer (`lez/guests/consumer`) | `fd3da60807de1fb9892bdde426fd24962df0fd8528db1f61f40cb8fbf522f862` | 422,488 |
| holdings (`lez/guests/holdings`) | `1840dc022621f6e533e57f9e229ea50aa026095d0b2c904a15b359fa642f88e7` | 307,696 |
| bridge (`lez/guests/bridge`) | `7a99ff33cee8a452bd93aba5d39e88566a1a86629bcdb95f12d9645bce195fb9` | 543,576 |

The consumer, holdings and bridge images change because they compile `ferrata-lez-wire`
(`RecordState` gained the root id). Ids of the published path come from `scripts/reproduce-image-ids.sh`
on the merged tree.

## Not built

- Consumers bound to the root id: the bridge, `ferrata_holdings` and the reference consumer still
  derive a record's address from a pinned instance id.
- The period from a real checkpoint state: the harness, the lab and the live drivers use 3,532 epochs
  (the specification's table for 33,554,432 ETH) as a stand-in; `verify_checkpoint`/CLI support for
  computing it from a beacon state is not built.
- An on-chain cap on `ws_period_epochs`: the program checks it is nonzero; its value is the deployer's,
  pinned in the instance id and checked off chain.
- A successor test on an Ethereum devnet (the proposal's done gate): this run uses real Sepolia data
  through its Gloas fork in the harness.
