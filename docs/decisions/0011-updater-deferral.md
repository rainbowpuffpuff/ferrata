# ADR 0011: The updater re-checks the chain and defers each step by a hashed share of a window; it finishes the job it opened

- **Date:** 12 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 R.4, R.5, R.8, R.9, R.10, U.3

## Context

Several updaters may serve one instance; a duplicate first step wastes about 2.2M cycles; the cost should not always
fall on the fastest operator.

## Options considered

1. Race (the fastest pays every time, duplicates frequent). *Ruled out:* RFP-021 R.10 ("Submission timing must not
   systematically concentrate cost on whichever party reacts fastest").
2. A coordinator or a role. *Ruled out:* RFP-021 F.11 ("submission carries no access control, allowlist, role, or
   registration").
3. Each daemon waits `H(key, job, step) mod W` rounds, re-checks the chain, sends; any updater continues any open job;
   an updater finishes the job it opened before opening another (`finish_own_jobs`).

## Decision

The third.

## Rationale and trade-offs

Measured on 58 recorded updates with two honest daemons and an adversary: every update applied once, honest gas on
refused steps 0.057–0.087 of applied (gate 0.10), fee shares 0.39–0.61. The cost: a mean added latency of half the
window.

## Sources

- lez/updater/README.md, "The A3 gate"
- docs/guides/operator-guide.md §3.4
- EVIDENCE rows 79–81
