# ADR 0031: Committed amounts: a new vault on both legs, the next one built so that a later switch is LEZ-side only

- **Date:** 21 November 2026
- **Status:** design only
- **Requirements:** RFP-022 Soft.1, Sec.8

## Context

RFP-022 soft requirement 1 asks that what is delivered be designed so that committed amounts can be adopted later
without redeploying the vault or resetting accumulated anonymity, and that the migration path be documented.
Deposits and burns are fixed denominations (ADR 0014); a claim and a burn show the denomination. The vault pins the
release guest's image id as an immutable (Security #8) and, through `configId`, one bridge program and its header
account.

## Options considered

1. Build committed amounts now (a new LEZ bridge program and claim and burn formats). *Reconstructed for contrast* (not recorded as an option at the time).
2. Adopt them inbound without the vault: a second LEZ program claims the same deposit tree from leaf 0 with a value
   commitment, the deployed program's imports stopped with `Sunset`, the new one refusing any nullifier the deployed
   one spent; outbound, a new release guest, hence a new vault. *Recorded:* docs/design/committed-amounts.md ("The
   withdrawn plan: deploy a second LEZ bridge program whose claims publish a value commitment instead of the amount").
3. Document the path for the deployed system: a new vault and a new LEZ bridge program built for committed amounts on
   both legs, the old configuration retired as for any migration; and what the next vault and program need so that a
   later switch is LEZ-side only (a hand-over of claims that is not a sunset, the successor reading its predecessor's
   nullifiers, the vault following the hand-over with a release image for committed burns pinned from its deployment,
   caps, fees).

## Decision

The third. The second was the design note's first version the same day; it is unsound and was withdrawn.

## Rationale and trade-offs

Three facts of the deployed code make the second unsound, each shown by a test that passes: the deployed program has
no permanent stop for claims (`Sunset` refuses imports only; a freeze of mints lapses), so one leaf could be minted on
both programs; a `Sunset` the checkpoint program collects ends the vault and refunds on Ethereum every leaf at or above
its boundary, which a program claiming from leaf 0 would also mint; and the vault releases only burns of its own
configuration, so tokens a second program mints would have no exit. Outbound, a committed burn changes the burn leaf,
hence the release guest, whose image id the vault pins. The cost: for the deployed vault neither half of the clause is
met; the vault is redeployed and the anonymity accumulated in the old one is not carried over. The next vault's hooks
are not built and are unproven; adding them at the redeployment the deployed program's known issues already require
(milestone B1) is what would let a later switch keep every deposit as a candidate, LEZ-side only.

## Sources

- docs/design/committed-amounts.md
- docs/design/bridge.md §13, §16, §17
