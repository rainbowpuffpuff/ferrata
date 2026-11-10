# FerrataVault gas

Measured on 2026-11-09 with forge 1.3.6, solc 0.8.26, optimizer on (200 runs), EVM cancun.

```
forge test --isolate --match-contract GasScenarios -vv
```

`--isolate` runs every top-level call as its own transaction, so each figure is the **whole transaction**: the
21,000 base cost, calldata, and cold storage access. As a check, a plain ETH transfer measured the same way reads
exactly 21,000. `gas-report.txt` (from `forge test --gas-report`) gives per-function min/avg/median/max over the
whole suite. For state-changing calls those figures are on the same basis: the `depositETH` median there, 146,485,
is the same as the "later deposit" row below.

State when measured: 41 earlier deposits in the tree, escrow in every token, one accepted burn root, finite
per-token and global caps on every bucket, standard relay fees set (`test/GasScenarios.t.sol`, `setUp`).

## Results

| Operation | Gas (whole transaction) |
|---|---:|
| `depositETH`, first deposit by the address | 163,573 |
| `depositETH`, later deposit by the address | 146,485 |
| `depositETH`, later deposit, all caps UNLIMITED (for comparison) | 125,747 |
| `depositETH`, later deposits over 128 consecutive leaves: min / mean / max | 142,241 / 148,509 / 172,073 |
| `deposit` ERC-20 (18 decimals, returns bool), first deposit by the address | 182,123 |
| `deposit` ERC-20, later deposit by the address | 165,035 |
| `deposit` USDT-style (no return value), first deposit by the address | 182,155 |
| `acceptBurnRoot`, new root ¹ | 61,767 |
| `release` ETH, no relay fee ¹ ² | 145,170 |
| `release` ETH, standard fee paid to `msg.sender` ¹ ² | 155,053 |
| `release` ERC-20 (18 decimals), standard fee paid to `msg.sender` ¹ | 175,559 |

¹ **The real verifier's gas is not included.** These rows use `MockRiscZeroVerifier`, whose own execution is about
2,687 gas (one cold storage read). That figure was measured separately (`test_gas_mockVerifierExecution`) and
matches the median of `verify` in `gas-report.txt`. Subtract it for the vault's own cost. Keep the cost of calling
the verifier (a cold account access, 2,600, plus copying the seal), because a real verifier incurs it too. Add the
real RISC Zero verifier's execution, which this spike did not measure. For scale only: the 4-pair BN254 pairing check
in a standard Groth16 verifier costs 45,000 × 4 + 34,000 = 214,000 gas under EIP-1108 prices, before any input
handling. This is a lower bound for that step, not a measurement of RISC Zero's verifier.

² The recipient is a fresh, empty account, which is the privacy-preserving default. A value transfer to an empty
account pays the 25,000 new-account charge, and that charge is in these figures. For ERC-20 releases, the
equivalent cost is the token setting the recipient's balance from zero.

## Where the deposit gas goes

These are approximate figures, derived from the measurements above.

- **Rate limits: about 20,700.** This is the difference between the capped and UNLIMITED later deposits
  (146,485 − 125,747). Each finite bucket costs four cold reads and two writes. An UNLIMITED bucket costs one read.
- **First deposit by an address: about 17,100 more.** `depositCount[depositor]` goes from zero to non-zero.
- **The tree.** Each deposit makes 63 SHA-256 precompile calls: 32 for the path and 31 for the empty-subtree hashes,
  which are recomputed from the node rule rather than hard-coded. It also does one cold read of `_branch[h]` per set
  bit of the leaf index, one branch write, and the writes to `depositRoot` and `nextLeafIndex`. That per-bit read
  is why cost varies with the leaf index. The 128-leaf maximum includes the first write to a new branch height,
  which is a zero-to-non-zero store.
- **ERC-20 deposits** include the token's own `transferFrom` and two `balanceOf` calls, for the exact-amount check.
  These are cheap mock tokens. Real USDC and USDT (proxy, blacklist and fee logic) cost more.

## Optimisations not applied (clarity first)

- Hard-coding the 32 empty-subtree hashes would save 31 precompile calls per deposit. Estimated at under 10,000 gas;
  **not measured**.
- Packing each bucket's `level` and `lastUpdate`, and `capacity` and `refill`, into one slot each would halve the
  rate-limit storage traffic.
- Storing only `_branch` and computing the root on request (as in the Ethereum deposit contract) would save the
  root write, but the brief requires the root in slot 0 for the LEZ storage proof.
