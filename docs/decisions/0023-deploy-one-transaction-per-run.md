# ADR 0023: Deploy one transaction per run, with limits from the target node's estimate

- **Date:** 10 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-022 R.14, S.1

## Context

The first live Sepolia attempt ran out of gas: Sepolia runs Glamsterdam's state gas (EIP-8037), which Foundry's EVM
does not price, so forge's limits were several times too low.

## Options considered

1. Forge's own limits in one broadcast. *Recorded:* docs/guides/deployer-guide.md §4.1 (the first live attempt "sent
   the Groth16 verifier's creation with forge's limit of 1,533,981 and it ran out of gas").
2. One transaction per run: ask the node for `eth_estimateGas` on the chain as earlier runs left it, refuse unless
   estimate × 1.25 fits the cap, send with that margin, read the limit back.

## Decision

The second (`vault/script/deploy.sh`).

## Rationale and trade-offs

Every estimate is real; a run that would put the vault at another address stops before broadcasting. The cost: a deployment is nine runs, eight of them a transaction each.

## Sources

- docs/guides/deployer-guide.md §4
- vault/GAS.md, "On Sepolia since Glamsterdam"
