# ADR 0046: Updaters join or wait for a job open on the instance's store instead of opening another

- **Date:** 27 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 R.9, R.10, F.12

## Context

A job's id hashes the instance, the store it starts from and the update, so of two jobs open on one store only one can
finish: the other's paid steps are wasted once the first moves the store. On a running LEZ v0.4.0-rc1 sequencer, two
honest updaters following recorded updates revealed every 60 s abandoned 23 and 15 jobs at windows 8 and 30, 127.0M and
72.7M execution gas, while A3's ratio of refused to applied gas stayed under 0.10
(`lez/results/perf-gate-sequencer-100.md`). Each of those jobs was opened, or overtaken, by an `Open` sent after the
block holding the other job's `Open` was published (`bench/perf/gate_abandoned.py` on those records). The updater
opened the best update's job whatever else was open on the store.

## Options considered

1. Keep opening the best update's job whatever else is open, and count abandoned jobs as contention. *Recorded:*
   `lez/results/perf-gate-sequencer-100.md` (runs G6 and G7) and `docs/guides/operator-guide.md` §3.4 as they stood
   before this record.
2. Make a duplicate `Open` cheap in the program, the job's actor as the transaction's root or the instance recording its
   open job. *Recorded:* `docs/rows/rfp-C.md` (R.9, "needs a program restructure"). It moves the attestation image id
   that deployed instances and their consumers pin, so existing instances would have to be re-bootstrapped; and it makes
   a second job cheaper to refuse, not unneeded.
3. In the updater, before sending any step, read which jobs of the updates it knows are open on the store; join a copy of
   its own update that leads, and wait for another job that leads while that job moves, within bounds.

## Decision

The third, in `lez/updater/src/updater.rs` (`join_or_wait`, `leads`, `policy.join_wait_rounds`). A job leads another
when the chain holds more of its steps, so any open job leads an `Open`; at as many, the newer update leads. A leading
copy (the same finalized header and adoption) is joined; another leading job is waited for while the most steps the
chain holds of jobs of its kind grow within `join_wait_rounds` rounds (default `2 × defer_window + 10`), and on one state
of the instance for at most `JOIN_WAIT_CAP` (3) times that. A job whose account fails to read is waited for, and a wait
that ends restarts the step's turn. The program is unchanged.

## Rationale and trade-offs

With every party following the policy, a second job on a store is opened only when its `Open` was sent before the first
job could be read, and a trailing job is not continued once the leading one can be read, so what remains is contention
no read can avoid. The in-process gate with overlapping updates abandons 29 jobs without the policy and none with it
(`lez/updater/results/gate-overlap.txt`). The costs: only jobs of updates the updater was served can be found (a job's
account is found by its id); waiting for an older job applies its update before the newer one, which a second job would
have skipped; a carrier asks its sources each round; and a party opening jobs can hold an updater back up to the cap on
each state, on top of the turns' bound of ADR 0036 (68 rounds at the defaults). `join_wait_rounds = 0` restores the
earlier behaviour.

## Sources

- lez/updater/src/updater.rs (`join_or_wait`, `leads`, `open_jobs`, `learn_updates`)
- lez/updater/tests/join_wait.rs (the policy's tests) and lez/updater/dev/mutants.py (mutants `j1` to `j13`)
- lez/updater/tests/recorded.rs (`gate_overlapping_updates_join_or_wait`) and lez/updater/results/gate-overlap.txt
- lez/updater/README.md item 7 (join or wait) and item 8 (the turns' bound)
- lez/results/perf-gate-sequencer-100.md (runs G6 and G7) and bench/perf/gate_abandoned.py
- lez/results/perf-gate-sequencer-join.md (the runs with join or wait)
