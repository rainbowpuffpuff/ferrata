# ADR 0042: Exercise the attestation and public bridge round trip in CI against standalone LEZ sequencers

- **Date:** 24 November 2026
- **Status:** accepted; run in CI
- **Requirements:** RFP-021 S.2; RFP-022 R.10

## Context

The existing CI exercised LEZ's own state transition in process and mocked parts of the Ethereum side. That did not
show the sequencer binary deploying the programs, applying their transactions and verifying the private attestation,
nor a bridge round trip through a running sequencer. A first attestation sequencer run on 21 November stopped because
its recorded Ethereum data had aged past the light client's weak-subjectivity bound. LEZ testnet 0.4 was unavailable,
and each CI run needed fresh Sepolia inputs rather than public-network write access.

## Options considered

1. Keep CI's in-process state-machine and mock-verifier coverage only. *Reconstructed for contrast* (not recorded as
   an option at the time); it was the state before: CI's in-process jobs (.github/workflows/ci.yml) and a standalone
   sequencer built from source by a CI script on recorded data (docs/EVIDENCE.md row 164, which row 251 superseded).
   This leaves the running sequencer path outside repeatable CI.
2. Run two scheduled jobs that build standalone `sequencer_service` binaries from source: attestation against LEZ
   v0.4.0-rc3, and the bridge's public-variant round trip against v0.4.0-rc1. Fetch fresh public Sepolia data at run
   time, with freshness bounds and no recorded fallback. Use a local Sepolia fork and mock verifier for the bridge job,
   and do not include its private claim or burn.

## Decision

The second. `sequencer-attest` deploys through rc3's loader, follows a fresh Sepolia finality update, checks designed
refusals, consumes attestations publicly and privately, and requires a real proof that the sequencer verifies.
`sequencer-roundtrip` uses rc1 because `ci_zone` produces rc1 blocks and the pinned `checkpoint_b_31` guest reads that
layout. While rc1 runs, it deploys on an anvil fork and performs deposit, import, plaintext claim and refused replay,
public burn, checkpoint, release and refused replay.

Both jobs run on pushes to `main`, daily and by hand, not on every other ref push or pull request. Their stages have
separate limits, exact outcome checks and uploaded records. A failed public endpoint fails the relevant step; old data
fails the freshness check instead of being replaced by a fixture.

## Rationale and trade-offs

This adds the actual sequencer's deployment, transaction and proof-verification paths to repeatable checks while
keeping Ethereum writes off public networks. Fresh inputs prevent a replay from ageing out unnoticed, and exact counts
detect dropped transactions as well as wrong verdicts. The two LEZ versions are intentional: rc3 matches the live
attestation driver, whereas the CI checkpoint program and zone fixture are the rc1 pair. Limiting the bridge job to the
public variant makes its mock-verifier and dev-mode scope explicit rather than implying private-path coverage.

The jobs are expensive, on the order of 59–92 minutes in the recorded runs, so they do not gate every contribution.
The bridge run retains stand-ins for the verifier, light-client committee, Bedrock checkpoint committee and gas drop;
its private claim and burn are untested there. The attestation proof dominates some runs. Concurrency serialises runs
on `main`, and public endpoint or runner availability can still prevent completion, but neither condition is converted
into a pass by stale records.

## Sources

- Commit `9462e57d`; .github/workflows/sequencer.yml (jobs, LEZ versions, fresh-data policy, triggers and stand-ins)
- scripts/ci/sequencer-attest.sh (fresh attestation data and standalone rc3 execution)
- scripts/ci/sequencer-roundtrip.sh and scripts/ci/sequencer-roundtrip-hook.sh (the rc1 public round trip)
- docs/EVIDENCE.md rows 251–252 (the successful runs, scopes and measured durations)
- e2e/runs/ci-sequencer-2026-11-24/attest/README.md and e2e/runs/ci-sequencer-2026-11-24/roundtrip/README.md
