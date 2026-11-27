# ADR 0053: Shard the LEZ harness on every change and keep an unsharded daily backstop guarded by exact target coverage

- **Date:** 23 November 2026
- **Status:** accepted; run in CI
- **Requirements:** RFP-021 R.12; RFP-022 R.11

## Context

This record was written on 27 November 2026 from the commits, after the fact. Commits `34c941e4` and `c9953b3e` took
the sharding decision, and `df19510f` moved the full backstop into its own workflow.

The unsharded LEZ harness job ran every program, updater and privacy test serially. Two recorded runs took 50.6 and
74.6 minutes, while the other jobs finished much sooner. Splitting tests by hand risked omitting a newly added target
or running it twice; simply making the full suite scheduled would have weakened every-change verification.

## Options considered

1. Keep one unsharded harness job on every push and pull request. *Recorded:* docs/guides/ci.md, the harness timing
   discussion, retains the measurements of that arrangement. Coverage is simple, but it dominates wall time.
2. Divide tests among runner jobs using only a handwritten list. *Reconstructed for contrast*. It shortens the run but
   lets a new Cargo target, doc-test or renamed individual test disappear unnoticed.
3. Balance a six-job matrix from measured durations, derive its target assignments from the Cargo manifests and test
   sources, enforce exact ownership, and retain a separate unsharded daily and manual backstop.

## Decision

The third. Four shards hold the harness integration targets, including library, binary and doc tests; one isolates the
heaviest updater gate tests, and one holds the remaining updater tests and privacy gate. The slow recorded updater
suite is partitioned by exact test names. Shards do not cancel one another on failure, and each builds only the
targets it runs.

`scripts/ci/check-lez-shards.py` compares the map with both Cargo packages, their feature gates, individual test names
and the workflow matrix. Every applicable target belongs to exactly one shard; feature-gated exclusions must be named
and justified by the manifests and source. The guard runs in the consistency job and at each shard's start. The full
unsharded script still runs on a daily schedule and by manual dispatch in `.github/workflows/lez-full.yml`, separate
from every-change sharded execution.

## Rationale and trade-offs

Sharding attacks wall time without replacing every-change coverage with an occasional run. Static ownership checks
make test additions fail the map rather than silently escape CI, and the full backstop exercises the same driver
without shard selection. The first recorded sharded run took 22.7 minutes instead of the earlier 50.6–74.6 minutes.

The cost is six build and setup workloads, more concurrent runner demand and a map that must follow test names. Cold
builds are repeated because target directories are not cached, and queued jobs can still delay the aggregate result.
The guard intentionally excludes feature-gated suites from the ordinary matrix, so it does not claim all optional
features or real-proof workloads run there. Daily scheduling also depends on the platform continuing to run schedules.

## Sources

- Commits `34c941e4`, `c9953b3e` and `df19510f`; .github/workflows/ci.yml and .github/workflows/lez-full.yml (triggers,
  matrix and unsharded backstop)
- `scripts/ci/lez-harness-shards.json` and scripts/ci/lez-harness.sh (the balanced assignments and driver)
- `scripts/ci/check-lez-shards.py` and scripts/ci/test_lez_shards.py (target ownership and its tests)
- docs/guides/ci.md, "lez-harness, lez-harness-full and monitor" (measured wall times, limits and caching trade-offs)
