# ADR 0036: Count updater deferral turns per state of the instance, not per job

- **Date:** 22–23 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 F.12, R.9, R.10

## Context

ADR 0011 gave each updater a hashed, per-job-and-step turn so independent updaters would not normally pay for the same
step. A job id hashes the whole update, including `signature_slot`, but the committee signature does not cover that
slot. A source can therefore serve another genuine update of one period from each block, or open and advance each copy
itself. Under per-job deferral, every new copy restarted the wait. In the recorded attacks, a source that knew the
updater's public key kept copies choosing the longest waits and prevented the next committee from being adopted.

## Options considered

1. Keep counting each `(job, step)` from the first round that step was due. *Recorded:* lez/updater/README.md item 8,
   which retains this rule for a job the updater already carries and describes the attacks against applying it to every
   job. A subsequently carried job can no longer be replaced by a source: its id commits to its starting store and
   update, and work by others only advances it.
2. Retain the hashed wait, but count an `Open`, and every step of a job this updater has not carried, from the first
   round that kind of step was due on the instance's current state, regardless of job. After sending there, wait a whole
   turn before another step of that kind.

## Decision

The second. Commits `87e7813b` and `88cdb90a` first changed `Open` and then every step of an uncarried job to per-state
turns. A step of a job the updater has already sent remains per-job. Each kind of step has one turn per state, bounded
by `defer_window - 1`; after a send, the next of its kind waits `defer_window - 1` rounds. The turns restart whenever
the instance's state changes.

## Rationale and trade-offs

A source can still choose jobs whose waits are longest, but it can no longer reset those waits merely by changing
`signature_slot` or opening the copies itself. It can only present work at another kind of step. This gives a bound on
each state: with `K = 3 + adoption_parts` kinds, an adopting update is applied within
`K × (defer_window - 1) + 1 + (K - 1) × defer_window` rounds of first being due there, 68 rounds at the default window
of 8 and two adoption parts. `lez/updater/tests/redating.rs` measured 66 rounds against the strongest committed attack; the former
per-job mutant never adopted the committee in its run.

The bound is per state, not across an arbitrary sequence of states. A party that pays to apply older genuine updates
creates new states and fresh turns; the documented measurements and `/health` consequences retain this limit. With
sources whose best update changes every slot, a second updater opens its own job for the newer update once its own
`Open` turn on the state has run; with `finish_own_jobs = false` such sources make an updater open jobs and finish
none, so the default finishes the job it opened. Duplicate opens cost more in an independent review run, not
reproduced by a committed test: two updaters in one slot paid 51% more per applied update than before the per-state
turns, and three paid 28% more. The tests are on the in-process chain, not a sequencer.

## Sources

- Commits `87e7813b` and `88cdb90a`; lez/updater/src/updater.rs (the per-state and carried-job deferral calculations)
- lez/updater/README.md items 6–8 and "Not built, not tested" (the attack, bounds and remaining costs)
- lez/updater/tests/redating.rs (re-dating, source-opened copies, turn reset and mutants)
- docs/EVIDENCE.md row 226 (the measured bound, variants and multi-updater costs)
