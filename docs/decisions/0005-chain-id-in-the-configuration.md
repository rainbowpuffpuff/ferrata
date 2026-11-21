# ADR 0005: The chain id is part of the configuration and of every record; the program cannot check it, so the checkpoint is verified off chain

- **Date:** 10 November 2026 (Stage 4)
- **Status:** accepted; built
- **Requirements:** RFP-021 F.5, F.9, F.14

## Context

The design put the chain id in every instance id, but the program's `Config` had none. Nothing a light client verifies
commits to the execution chain id.

## Options considered

1. Leave the chain id out and rely on `genesis_validators_root` alone.
2. Put it in `Config` (so in the instance id) and in every record, and check it against the providers off chain.

## Decision

The second: every request names the chain id the caller expects (140 otherwise), and `verify_checkpoint` in
`ferrata_attest_module` asks at least three beacon providers, whose `DEPOSIT_CHAIN_ID` ties the chain id to the
genesis root.

## Rationale and trade-offs

One program serves any Ethereum-consensus chain with instances kept apart; a statement for one chain is refused by a
verifier configured for another. The cost: a consumer must verify the checkpoint, and with it the chain id, before
pinning an instance.

## Sources

- lez/RESULTS.md §5, "Decisions" item 1
- docs/guides/consumer-guide.md §7
- EVIDENCE rows 59, 105
