# ADR 0018: LEZ-side caps and freezes run on attested Ethereum time

- **Date:** 11 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-022 F.11, F.14, Sec.4, Sec.5

## Context

LEZ programs cannot read a clock on v0.4.0-rc1 and rc2 (rc3 adds the producer's timestamp); a caller's time can be
chosen.

## Options considered

1. A caller-supplied time. *Recorded:* lez/BRIDGE.md §6 (the flaw it replaces: "the program took its time from the
   caller").
2. The producer's timestamp (rc3 only, and chosen by the producer). *Recorded later:* lez/results/v0.4.0-rc3/README.md
   (rc3 was examined on 14 November, after this decision; Ferrata does not use it).
3. The newest Ethereum execution timestamp the attestation program has recorded, read from a record.

## Decision

The third.

## Rationale and trade-offs

A freeze holds while the light client stalls, and a captured committee's far-future record refills no bucket unless a
block producer also stamps its block that far ahead. The cost: buckets refill only as records arrive.

## Sources

- lez/BRIDGE.md §6
- docs/guides/deployer-guide.md §5
- EVIDENCE row 28
- lez/results/v0.4.0-rc3/README.md
