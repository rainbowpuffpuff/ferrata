# FerrataVault v3 gas

Measured on 2026-11-10 with forge 1.3.6, solc 0.8.26, optimizer on (200 runs), EVM cancun.

```
forge test --isolate --match-contract GasScenarios -vv
```

`--isolate` runs every top-level call as its own transaction, so each figure is the **whole transaction**: the
21,000 base cost, calldata, and cold storage access. As a check, a plain ETH transfer measured the same way reads
exactly 21,000.

`gas-report.txt` gives per-function min/avg/median/max over the unit, fuzz, vector, privacy and gas suites. It is
produced by `forge test --gas-report --no-match-path 'test/invariant/*'`. The invariant campaign is left out
because gas-report mode isolates every one of its calls, which takes several minutes and yields figures dominated
by random sequences.

State when measured (`test/GasScenarios.t.sol`, `setUp`):
- 42 earlier deposits in the tree, and escrow in every token;
- one finalised checkpoint;
- finite per-token and global caps on every bucket;
- relay fees registered.

The refund rows first retire the vault, finalise a sunset checkpoint and wait `revokeWindow` (v3 D5). The
checkpoint rows use the test committee, n = 4 and t = 3.

## Results

| Operation | v3 gas (whole transaction) | v2 |
|---|---:|---:|
| `depositETH`, first deposit by the address | 173,920 | 173,917 |
| `depositETH`, later deposit by the address | 153,922 | 153,919 |
| `depositETH`, later deposit, all caps UNLIMITED (for comparison) | 133,184 | 133,181 |
| `depositETH`, later deposits over 128 consecutive leaves: min / mean / max | 147,556 / 153,841 / 177,388 | 147,553 / 153,838 / 177,385 |
| `deposit` ERC-20 (18 decimals, returns bool), first deposit by the address | 192,346 | 192,431 |
| `deposit` ERC-20, later deposit by the address | 172,348 | 172,433 |
| `deposit` USDT-style (no return value), first deposit by the address | 192,378 | 192,463 |
| `recordBeacon` (new beacon) | 77,823 | 77,845 |
| `submitCandidate`, including the beacon it records ¹ | 247,481 | 221,157 |
| `finalize` ¹ | 212,565 | 200,736 |
| `finalize`, the first sunset checkpoint (sets the refund boundary) ¹ | 300,999 | – |
| `cancelCandidate` (freeze authority) | 46,740 | – |
| `release` ETH, no relay fee ¹ ² | 167,995 | 168,038 |
| `release` ETH, registered fee paid to `msg.sender` ¹ ² | 179,657 | 179,700 |
| `release` ERC-20 (18 decimals), registered fee paid to `msg.sender` ¹ | 200,397 | 200,440 |
| `release` ETH, the recipient cannot receive, so the amount is credited to `owed` ¹ | 191,758 | 191,801 |
| `refund` ETH (32-step path checked against `depositRoot`) | 127,577 | 121,087 |
| `refund` ERC-20 (18 decimals) | 153,965 | 147,475 |
| `claimOwed` ERC-20, by the payee | 60,132 | 60,112 |
| `claimOwed` ETH, called through the payee contract | 37,713 | 37,697 |
| `revokeFrom` (from `gas-report.txt`: min / median / max) | 23,698 / 69,882 / 80,514 | 23,698 / 67,217 / 77,849 |

¹ **The real verifier's gas is not included.** These calls go through `RiscZeroVerifierEmergencyStop`, which a
real deployment also pays for. Inside it sits `MockRiscZeroVerifier`, whose own execution is about 2,765 gas
(`test_gas_mockVerifierExecution`, and the median of `verify` in `gas-report.txt`). Subtract that figure, then add
the real RISC Zero verifier's execution, which this spike did not measure. For scale only: the 4-pair BN254
pairing check of a standard Groth16 verifier costs 45,000 × 4 + 34,000 = 214,000 gas under EIP-1108 prices,
before any input handling. That is a lower bound for that one step, not a measurement. The wrapper's `verify`
has a median of 8,504 gas in the report, about 5.7k of it the wrapper's own: a paused check, plus the call into
the base verifier.

² The recipient is a fresh, empty account, which is the privacy-preserving default. A value transfer to an empty
account pays the 25,000 new-account charge, and that charge is in these figures.

## What changed against v2

- **`submitCandidate` +26.3k.** The candidate now also stores its tip (`tipHash` takes a new slot, about 22k;
  `tipId` and `t` pack into an existing one) so that `cancelCandidate` can report it, and the journal has four more
  words of calldata. The event carries four more fields.
- **`finalize` +11.8k.** Four more journal words in calldata and in the event, the checkpoint-freeze, key-set and
  monotonicity checks, and the parent's imported leaf count and sunset flag recorded with the checkpoint (they pack
  into the slot that already holds the counts). I have not broken the difference down further. The first sunset
  checkpoint pays about 88k more again: it writes the boundary, the index of the checkpoint that set it, its
  finalisation time, and the imported leaf count and sunset flag of `finalState`, all from zero.
- **`refund` +6.5k.** The retirement, release-freeze and opening-time checks read three more cold slots.
- Deposits, releases and claims are within ±100 gas of v2 (function-dispatch order changed).

## What a checkpoint costs

A checkpoint is paid once per finalised LEZ state, not per user. It takes three calls: `submitCandidate` (which
records the first beacon), one more `recordBeacon` at least `W = Δ + (t − 1) × turnSeconds` later, and `finalize`.
That is about 247k + 78k + 213k ≈ 538k gas, plus two real verifier executions. Every burn in that state's burn
tree then releases against it. A cancelled candidate costs its submitter the 247k and the wait; resubmitting the
same state costs the same again.

## Where the deposit gas goes

These are approximate figures, derived from the measurements above.

- **Rate limits: about 20,700.** This is the difference between the capped and UNLIMITED later deposits
  (153,922 − 133,184).
- **First deposit by an address: about 20,000 more.** `depositCount[depositor]` goes from zero to non-zero.
- **The tree.** Each deposit makes 63 SHA-256 precompile calls: 32 for the path and 31 for the empty-subtree hashes,
  which are recomputed rather than hard-coded. It also does one cold read of `_branch[h]` per set bit of the leaf
  index, one branch write, and the writes to `depositRoot` and `nextLeafIndex`. The per-bit read is why cost varies
  with the leaf index.

## Optimisations not applied (clarity first)

- Hard-coding the 32 empty-subtree hashes would save 31 precompile calls per deposit (estimated at under 10,000
  gas; **not measured**).
- Packing each bucket's two pairs of fields into one slot each would halve the rate-limit storage traffic.
- The candidate keeps its tip only for the cancellation event; emitting the tip from the caller's arguments instead
  would save the extra slot but trust the caller.
