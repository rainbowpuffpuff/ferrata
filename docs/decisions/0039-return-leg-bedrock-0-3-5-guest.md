# ADR 0039: For the rc3 return leg, pin the checkpoint guest built for Logos Blockchain 0.3.5-rc.1

- **Date:** 23 November 2026
- **Status:** accepted; deployed on Sepolia for the test network
- **Requirements:** RFP-022 F.6, F.7, R.10, Sec.2

## Context

LEZ v0.4.0-rc3 resolves its Logos Blockchain node to 0.3.5-rc.1. That node puts a 32-byte fork digest before the
operations of a Bedrock transaction and hashes that digest with them. The default checkpoint guests do not read this
layout, so their image ids cannot follow such a zone. A rule (B) return leg for the new RISC Zero 3.1 vault therefore
needed both the prover-version migration and a checkpoint guest that matched the node's transaction format.

## Options considered

1. Pin the default `checkpoint_b_31` guest for the rc3 vault. *Reconstructed for contrast* (not recorded as an option
   at the time); docs/sepolia.md records why it cannot serve (the earlier checkpoint ids cannot follow a Logos
   Blockchain 0.3.5-rc.1 zone), and docs/image-ids.md's 0.3.5 section gives the format they do not compile. Its successful uses are against the other Bedrock layout, not this network's return leg.
2. Pin `checkpoint_b_0_3_5_31`, the RISC Zero 3.1 rebuild of the second checkpoint build. Its `bedrock-0-3-5` feature
   reads the node's fork digest and requires it to equal the configuration's `bedrockGenesisId`; reject the same
   guest family built for a different node layout.

## Decision

The second. The Sepolia parameters for the test network's zone commit its rule (B) mode and configuration to
`checkpoint_b_0_3_5_31`. The deployment record and image-id index pin that id, and the bridge installation takes the
corresponding binary. The return-leg hook likewise selects `checkpoint_b_0_3_5` for rule (B), supplies the node's
`/chain/fork` digest as `bedrockGenesisId` and builds the checkpoint solely from the node's answers.

## Rationale and trade-offs

The guest reads the blocks the selected node actually produces. Binding its required fork digest to the bridge
configuration makes the checkpoint cover the intended Logos Blockchain fork, not a structurally similar transaction
stream with another digest. The full round trip of 23–24 November then verified this guest's real RISC Zero 3.1
checkpoint seal on Sepolia and released its burn, showing the choice end to end.

The cost is another checkpoint image id, workspace and binary to reproduce and distribute. The id is immutable in the
vault, so this vault cannot accept checkpoints from the default-layout guest and a future incompatible Bedrock layout
requires another guest and compatible deployment. The test network additionally uses one sequencer and one validator,
with leader proofs in dev mode; selecting the compatible checkpoint guest does not change those deployment limits.

## Sources

- Commit `19807421`; vault/script/params/sepolia-31.json (the zone configuration and rule (B) checkpoint id)
- docs/image-ids.md, "For zones on Logos Blockchain 0.3.5-rc.1" and the two 3.1 rows
- lez/live/hooks/return-leg.sh (fork-digest input, guest selection and node-only checkpoint inputs)
- deploy/testnet/install-bridge.sh (the RISC Zero 3.1 `checkpoint_b_0_3_5` artifact)
- docs/EVIDENCE.md rows 232 and 250 (the pinned deployment and full round trip)
