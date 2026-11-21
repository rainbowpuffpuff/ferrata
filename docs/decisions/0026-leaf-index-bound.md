# ADR 0026: Every leaf index stays below 2^32

- **Date:** 10 November 2026 (SPEC-v3 D9)
- **Status:** accepted; built
- **Requirements:** RFP-021 F.6; RFP-022 F.3, R.3

## Context

The tree is 32 levels deep, so a Merkle path carries 32 direction bits, while the leaf's identifier and nullifier
carry the index as a plain field: indices `i` and `i + 2^32` would share a path, giving one leaf two identifiers and
two nullifiers.

## Options considered

1. Bound the index everywhere: `leafIndex < 2^32`. The vault refuses a deposit once the tree is full, the core
   refuses an accumulator-leaf fact with an index of 2^32 or more, and the claim recomputes the leaf from a preimage
   that binds the claimed index.
2. The source records no alternative; the obvious one is to leave the index unbounded, and indices `i` and `i + 2^32`
   then share a path, giving one leaf two identifiers and two nullifiers.

## Decision

The first.

## Rationale and trade-offs

An index and that index plus 2^32 no longer name the same leaf: one leaf, one identifier, one nullifier, and no alias
that spends a deposit twice. The cost: the deposit tree holds at most 2^32 leaves, and the vault refuses deposits once
it is full.

## Sources

- docs/design/spec/bridge-spec-v3-changes.md D9
- docs/design/bridge.md §5.2, §5.5
- vault/README.md ("Leaf indices stay below 2^32")
- crates/ferrata-core/README.md
