# ADR 0031: Committed amounts: adopt them inbound without the vault; outbound needs a new release guest

- **Date:** 21 November 2026
- **Status:** design only
- **Requirements:** RFP-022 Soft.1, Sec.8

## Context

RFP-022 soft requirement 1 asks that what is delivered be designed so that committed amounts can be adopted later
without redeploying the vault or resetting accumulated anonymity, and that the migration path be documented.
Deposits and burns are fixed denominations (ADR 0014); a claim and a burn show the denomination.

## Options considered

1. Build committed amounts now (a new LEZ bridge program and claim and burn formats).
2. Document the path: inbound, a second LEZ program claims the same deposit tree from leaf 0 with a value
   commitment, refusing any nullifier the retired program spent; outbound, a new release guest, hence a new vault.
3. Redeploy the vault now with a second, unused release image for committed burns.

## Decision

The second; the third is recorded as what the next vault version should do.

## Rationale and trade-offs

The deposit leaf already binds the amount and the nullifier does not depend on the LEZ program, so the inbound leg
needs no vault change and keeps every deposit as a candidate. The vault pins the release image, and a committed burn
changes the burn leaf, so the outbound leg cannot switch without a new vault: the clause is met inbound only.

## Sources

- branch rfp-softs, commit ff64e72f: docs/design/committed-amounts.md
- docs/design/bridge.md §13, §17
