# Refused, repeated and capped claims (`harness/tests/bridge_refusals.rs`, RFP-022 Priv.5)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`.

## The private claim (the default)

| claim | outcome | blocks added | public accounts changed |
|---|---|---:|---:|
| rejected: leaf 6 with a wrong s | refused before any proof (CLAIM_INVALID) | 0 | 0 |
| leaf 6, its owner's claim | applied | 1 | (a mint) |
| repeated: leaf 6 again | refused before any proof (ALREADY_CLAIMED) | 0 | 0 |
| capped: leaf 2 (1 ETH and its fees) | refused before any proof (CAPPED) | 0 | 0 |

No refusal of the private path adds a block, a fee or any public state: on chain the three are the same (nothing), and none names a deposit.

## The plaintext claim (opt-out)

| claim | exit code | transaction bytes | gas_exec | deposit fields in the transaction |
|---|---:|---:|---:|---|
| rejected: leaf 2 with a wrong s | 249 | 2053 | 94862 | the destination |
| leaf 6, applied (for comparison) | 0 (applied) | 2053 | 291261 | s, the destination |
| repeated: leaf 6 again | 250 | 2053 | 217289 | s, the destination |
| capped: leaf 2 | 255 | 2053 | 136962 | s, the destination |

A plaintext claim carries its witness, so every plaintext claim, applied or refused, names its deposit (`s`, the destination; the leaf and `N` follow from them). A refusal names no more than the claim that applies.
