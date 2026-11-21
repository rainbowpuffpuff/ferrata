# ADR 0004: An immutable program; instances keyed by their configuration and checkpoint; replacement, never refresh; successors for forks after `Init`

- **Date:** 10–11 November 2026
- **Status:** accepted; built (successors, root ids and the bound: 11 November)
- **Requirements:** RFP-021 F.9, F.10, F.13–F.15, VS.3

## Context

The checkpoint, the chain and the finality parameters must be fixed at `Init`, no instruction may rewrite them, and a
stale instance must be replaced rather than refreshed. Ethereum schedules forks after an instance starts.

## Options considered

1. A settable configuration or checkpoint (forbidden by F.13).
2. A program id held in mutable state, swappable by an admin (forbidden by VS.3).
3. The program deployed immutable through LEZ's loader; an instance id `H(config ‖ checkpoint)`; a second `Init`
   refused; past the weak-subjectivity bound an instance refuses updates and is replaced from a fresh checkpoint; a
   fork not in the schedule crossed by a successor instance under the parent's root id.

## Decision

The third.

## Rationale and trade-offs

A consumer that pins an instance id pins its checkpoint and configuration; no party can change what an instance
accepts. The cost: replacement is a new trust decision for every consumer, and consumers bound to an instance id move
by hand (`docs/guides/consumer-guide.md` §9.3); our reference consumers do not yet bind to the root id.

## Sources

- docs/design/attestation.md §3.3–§3.6, §4.6
- lez/harness/tests/successor.rs
- lez/updater/tests/replacement.rs
- EVIDENCE rows 69, 70
