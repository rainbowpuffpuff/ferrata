# FerrataVault v3 gas

Measured on 2026-11-10 with forge 1.3.6, solc 0.8.26, optimizer on (200 runs), EVM cancun. "v3 Dn" is item Dn of
[`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md).

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

## Results with the mock verifier

These are the vault's own scenarios. Rows marked ¹ go through RISC Zero's emergency stop into a **mock**
verifier; the same calls with RISC Zero's real Groth16 verifier and real proofs are measured in "With the real
verifier" below.

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

¹ **The real verifier's gas is not included in this table.** These calls go through
`RiscZeroVerifierEmergencyStop`, which a real deployment also pays for. Inside it sits `MockRiscZeroVerifier`,
whose own execution is about 2,765 gas (`test_gas_mockVerifierExecution`, and the median of `verify` in
`gas-report.txt`). The wrapper's `verify` has a median of 8,504 gas in the report, about 5.7k of it the
wrapper's own: a paused check, plus the call into the base verifier. The real verifier is measured below.

² The recipient is a fresh, empty account, which is the privacy-preserving default. A value transfer to an empty
account pays the 25,000 new-account charge, and that charge is in these figures.

## With the real verifier

Measured on 2026-11-10 with the same compiler settings, in the return leg of `test/real/`
([`../e2e/README.md`](../e2e/README.md)): RISC Zero's unmodified `RiscZeroGroth16Verifier` (risc0-ethereum
v3.0.0) behind its unmodified `RiscZeroVerifierEmergencyStop`, and real Groth16 proofs of the checkpoint and
release programs. Nothing is mocked.

```
forge test --isolate --match-contract ReturnLegGas -vv
```

State when measured: a freshly deployed vault, UNLIMITED caps, the first two deposits of the tree, no earlier
checkpoint, committee n = 4 and t = 3, fresh recipients. It is not the state of the table above (42 deposits,
finite caps, one earlier checkpoint), so the two tables differ by more than the verifier.

| Operation | Gas, whole transaction (`forge --isolate`) | Receipt on a local fork of Sepolia (anvil) |
|---|---:|---:|
| `registerToken`, three denominations: ETH / the test token ³ | 472,235 / 473,073 | 472,211 / 473,049 |
| `depositETH`, first deposit of the tree and of the address | 193,072 | 193,072 |
| `deposit` ERC-20, second deposit of the address, first of the token | 192,529 | 192,529 |
| `submitCandidate` with a real checkpoint seal, including the beacon it records | **507,370** | 507,370 |
| `recordBeacon` | 77,823 | 77,823 |
| `finalize` with a real checkpoint seal that echoes the beacon | **475,300** | ⁴ |
| `release` ETH, no relay fee, to a fresh recipient | **363,169** | ⁴ |
| `release` ERC-20, registered fee to a named relayer, both fresh | **388,243** | ⁴ |

Creations, from the receipts of the fork run (`forge --isolate` does not report creations through
`vm.lastCallGas`): `RiscZeroGroth16Verifier` 1,179,986; `RiscZeroVerifierEmergencyStop` 451,744; `FerrataVault`
5,501,261; the test token 508,036.

³ The two columns differ by 24 gas because the registered release delay is 24 h in the test and 120 s in the
fork run: two more non-zero calldata bytes.

⁴ Not yet measured on the fork: see "Sepolia round trip" in README.md for how far the rehearsal went.

Against the mock rows above: a candidate costs 507k where the mocked scenario reads 247k, a finalisation 475k
against 213k, an ETH release 363k against 168k. Most of each difference is the Groth16 verification, measured
on the v2 vault as 230,107 gas through the stop (`bridge/RESULTS.md`, Part 4); the rest is the different state
(first candidate and first checkpoint ever, UNLIMITED caps) and, for the candidate, 260 bytes of seal instead of
4. The v2 vault's releases with the same verifier cost 367,988 (ETH) and 397,874 (ERC-20 with a relayer's
fee).

**A checkpoint with the real verifier**: `submitCandidate` 507,370 + one later `recordBeacon` 77,823 +
`finalize` 475,300 = **1,060,493 gas**, paid once per finalised LEZ state.

Against the proposal's M0 thresholds (`docs/traceability.md`, P.3: release ≤ 450k, candidate and finalisation
≤ 500k): both releases and the finalisation are under them; this candidate is 7,370 gas over. It is a vault's
first candidate, so its beacon and candidate slots are written from zero; a later candidate was not measured
with the real verifier.

**The whole return leg of the test**, from an empty chain to both releases paid: 10.9M gas, of which 7.64M is
the four creations (the vault alone 5.50M) and 0.95M the two registrations.

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
With the mock verifier that is about 247k + 78k + 213k ≈ 538k gas; with the real verifier and real proofs it
is 1,060,493 gas ("With the real verifier"). Every burn in that state's burn tree then releases against it. A cancelled candidate costs its submitter the 247k and the wait; resubmitting the
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
