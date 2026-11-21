# ADR 0020: Pairing by one configuration id that both halves check; the vault's address by plain CREATE

- **Date:** 10 November 2026 (SPEC-v3 D2)
- **Status:** accepted; built
- **Requirements:** RFP-022 F.15, F.16

## Context

A vault and a LEZ configuration must name each other, and a front-run `Init` must not install other roles.

## Options considered

1. Registration of the vault on LEZ by an admin.
2. `configId = H("ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖ bridgeImageId ‖ evmChainId ‖ vault ‖ attestationRootId
   ‖ salt)`, `salt` committing to the LEZ roles and caps; the vault holds it in slot 2; LEZ `Init` attests that slot.

## Decision

The second; CREATE2 is unusable because the vault's init code contains `configId`, which contains the vault's address.

## Rationale and trade-offs

Neither half can be paired with another, and every proof journal carries the chain, the vault and `configId`. The
cost: the vault's address is predicted from the deployer's nonce, and a configuration is fixed for good.

## Sources

- docs/design/spec/bridge-spec-v3-changes.md D2
- docs/design/bridge.md §3
- docs/guides/deployer-guide.md §3, §6, §11
