# ADR 0012: The updater can require byte-identical data from several consensus clients before it pays

- **Date:** 18 November 2026
- **Status:** accepted; built, off by default
- **Requirements:** RFP-021 R.6, F.12; trust

## Context

A forged update that one client's node serves (a bug, or a captured committee's accomplice) passes every check
of the updater when its signatures do, and the program accepts updates on signatures alone.

## Options considered

1. Trust any one source.
2. `min_distinct_clients = k`: send only an update that sources reporting `k` different clients served byte for byte
   (or agree on the finalized header).

## Decision

The second, as an option (`client_match = "bytes"` the setting to use).

## Rationale and trade-offs

An honest updater does not pay for or relay an update one client alone serves. The cost: it binds only this updater
(the program accepts the same update from anyone), the client is the provider's unauthenticated claim, and two hosts
of one provider count twice.

## Sources

- docs/trust.md §2.4, "The updater's sources"
- lez/updater/README.md, "Client diversity"
- EVIDENCE row 147
