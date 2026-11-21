# ADR 0001: Follow Ethereum through the sync committee, inside the LEZ program, on finality updates only

- **Date:** 10 November 2026 (the design note)
- **Status:** accepted; built
- **Requirements:** RFP-021 F.1, F.2, F.11, VS.1, VS.6

## Context

RFP-021 asks for a light client that runs inside the LEZ program, checks the sync-committee aggregate itself, and lets
anyone advance it. LEZ v0.4 caps a transaction at 10M execution gas and runs public transactions without proofs.

## Options considered

1. A zero-knowledge proof of a light-client update made off chain and verified on LEZ (a prover becomes a required
   party for every update).
2. The specification's full light client, including optimistic headers and force updates (headers enter on weaker
   evidence).
3. The sync committee followed in the program on **finality updates only**, with a two-thirds supermajority floor.

## Decision

The third. A header enters only through a finality update signed by at least 342 of 512 members (`participants * 3 >=
512 * 2`); the program keeps no optimistic header and has no force update.

## Rationale and trade-offs

Every record is of a finalized header, and anyone can advance the instance with no prover. The cost: if Ethereum
finalizes nothing for a whole sync-committee period, or a third of one committee withholds its signatures for one, the
instance stops safely and must be replaced (ADR 0004). Trust rests on two thirds of each committee, weaker than
Ethereum's full economic finality; the design bounds the loss instead of pricing the attack (ADR 0019, `docs/trust.md`
§2.2).

## Sources

- docs/design/attestation.md §2, §4.1, §11
- docs/trust.md §2.2
- crates/ferrata-core/src/steps.rs
