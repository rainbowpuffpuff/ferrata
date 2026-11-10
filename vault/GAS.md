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

## On Sepolia since Glamsterdam: what the round trip really costs

**Every figure below this section is Foundry's** (forge 1.3.6's EVM, which knows Ethereum's gas schedule up to
Prague/Osaka: today's mainnet). Sepolia has run **Glamsterdam** since 6 October 2026 13:53:36 UTC (epoch 353,024,
timestamp 1,791,294,816: the activation table of [EIP-7773](https://eips.ethereum.org/EIPS/eip-7773), equal to the
`activationTime` the node reports in `eth_config`; reth v2.7.0, 28 September 2026: "prepares Sepolia for the
Glamsterdam hardfork"). Its gas changes are not in Foundry, and the first live run died of it: the Groth16
verifier's creation was sent with forge's limit of 1,533,981 (1.3 x its own 1,179,986) and ran out of gas
(tx `0x673cdb72…c7c1`, block 12,106,853); Sepolia's node asks 7,795,331 for it.

What changed (the specifications, and the constants as the node itself charges them):
- [EIP-8037](https://eips.ethereum.org/EIPS/eip-8037), state creation gas: a new price unit, `CPSB` = 1,530 gas per
  byte of new state, charged as **state gas**. A new account costs 120 x 1,530 = 183,600 (was 32,000 for a creation,
  25,000 for a value transfer to an empty account); a byte of deployed code 1,530 (was 200) plus 6 per word for its
  hash; a new storage slot 64 x 1,530 = 97,920 (was 20,000). Measured on the node with `eth_simulateV1` and code
  overrides (read-only probes, 10 November 2026): SSTORE into an empty slot 125,026 against 27,106 into a set one (the
  difference is 97,920); a creation returning 0, 1,024 and 4,096 bytes of code 207,668, 1,774,690 and 6,475,744.
- The same EIP makes the 2^24 = 16,777,216 transaction cap of [EIP-7825](https://eips.ethereum.org/EIPS/eip-7825)
  a cap on **execution gas only**; state gas comes from the rest of the limit (a "reservoir"), up to 2^32 - 1.
  The node's `eth_estimateGas` enforces the execution cap (a loop of 18.2M execution gas has no estimate) and
  prices state gas in full.
- [EIP-8038](https://eips.ethereum.org/EIPS/eip-8038), state access: cold account 3,000 (was 2,600), account write
  9,000, storage write 10,000, creation access 12,000; [EIP-2780](https://eips.ethereum.org/EIPS/eip-2780): the
  21,000 base becomes 12,000 plus what a transaction touches; [EIP-7976](https://eips.ethereum.org/EIPS/eip-7976),
  [EIP-7981](https://eips.ethereum.org/EIPS/eip-7981): calldata and access-list floors;
  [EIP-7954](https://eips.ethereum.org/EIPS/eip-7954): code up to 64 KiB.

**The fifteen transactions of the rehearsed round trip, priced by Sepolia's node** (10 November 2026, about 21:00
UTC). The transactions are exactly those of the cloud rehearsal ([`../e2e/runs/cloud-fork/`](../e2e/runs/cloud-fork/)
`plan.json`, from forge's records of it), played in order on a local anvil fork of Sepolia; before each, the node
(reth 2.10.0 at a public endpoint, block 12,107,143) was asked with that fork's state as overrides: `eth_estimateGas`,
and `eth_simulateV1` of the transaction alone for the gas it uses. The finalisation echoes the rehearsal's beacon,
written into the fork's vault. Command: `e2e/gasplan.py run --node <Sepolia> --state <fork> --plan plan.json
--nonce 0 --beacon-index 1 --echo-nonce <its echo> --wait 2460 --release-wait 180`; record
[`../e2e/runs/cloud-fork/gas-sepolia.json`](../e2e/runs/cloud-fork/gas-sepolia.json). "Foundry" is anvil's receipt
of the same transaction, equal to the rehearsal's manifest.

| # | Transaction | Foundry (anvil receipt) | Sepolia (node, gas used) | Sepolia `eth_estimateGas` | Limit (x 1.25) | Sepolia / Foundry |
|---|---|---:|---:|---:|---:|---:|
| 0 | create RiscZeroGroth16Verifier | 1,179,986 | 7,732,792 | 7,795,331 | 9,744,164 | 6.55 |
| 1 | create RiscZeroVerifierEmergencyStop | 451,744 | 2,953,028 | 2,977,632 | 3,722,040 | 6.54 |
| 2 | create TestToken | 508,036 | 3,315,490 | 3,342,971 | 4,178,714 | 6.53 |
| 3 | create FerrataVault | 5,501,261 | 38,170,341 | 38,474,447 | 48,093,059 * | 6.94 |
| 4 | registerToken (ETH) | 472,211 | 2,136,791 | 2,154,917 | 2,693,647 | 4.53 |
| 5 | registerToken (test token) | 473,049 | 2,138,129 | 2,156,266 | 2,695,333 | 4.52 |
| 6 | mint (test token) | 68,368 | 238,408 | 241,467 | 301,834 | 3.49 |
| 7 | approve (test token) | 46,331 | 128,251 | 129,343 | 161,679 | 2.77 |
| 8 | depositETH | 193,072 | 552,152 | 557,701 | 697,127 | 2.86 |
| 9 | deposit (test token) | 192,529 | 473,157 | 498,242 | 622,803 | 2.46 |
| 10 | submitCandidate | 507,370 | 1,381,770 | 1,393,904 | 1,742,380 | 2.72 |
| 11 | recordBeacon | 77,823 | 262,063 | 265,310 | 331,638 | 3.37 |
| 12 | finalize | 475,300 | 1,183,464 | 1,263,896 | 1,579,870 | 2.49 |
| 13 | release (ETH) | 363,169 | 607,873 | 625,711 | 782,139 | 1.67 |
| 14 | release (test token, relayer fee) | 388,243 | 648,471 | 675,643 | 844,554 | 1.67 |
| | **all 15** | **10,898,492** | **61,922,180** | 62,552,781 | | **5.68** |

\* over 2^24 and valid: the vault's creation needs 38.5M, of which (23,650 bytes of code + a new account) x 1,530
= 36,368,100 is state gas; its execution gas is far below the cap. **The vault fits in one transaction; no split
is needed** (and splitting would not help: the state gas of the code is per byte wherever it lands).

Where the factor comes from: creations pay 1,530 per byte of code (6.5x to 6.9x), registrations and deposits pay
97,920 per new slot (2.5x to 4.5x), the proof-verifying calls mostly execution (1.7x to 2.7x). At the gas price of
the failed attempt (74 wei) the whole round trip costs about 0.0000000046 ETH; at 1 gwei it would be 0.062 ETH,
of which the deployment is 0.057 ETH. Mainnet has not scheduled Glamsterdam (EIP-7773 leaves its row empty), so the
figures of the following sections are still what mainnet charges today; once it ships, a deposit costs about
0.47M to 0.55M gas instead of 0.19M, and deploying the vault about 38M instead of 5.5M.

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
| `finalize` with a real checkpoint seal that echoes the beacon ⁴ | **475,300** | 475,312 |
| `release` ETH, no relay fee, to a fresh recipient | **363,169** | 363,169 |
| `release` ERC-20, registered fee to a named relayer, both fresh | **388,243** | 388,243 |

Creations, from the receipts of the fork run (`forge --isolate` does not report creations through
`vm.lastCallGas`): `RiscZeroGroth16Verifier` 1,179,986; `RiscZeroVerifierEmergencyStop` 451,744; `FerrataVault`
5,501,261; the test token 508,036.

³ The two columns differ by 24 gas because the registered release delay is 24 h in the test and 120 s in the
fork run: two more non-zero calldata bytes.

⁴ Two different proofs: the fork's finalisation echoes the nonce of the fork's own beacon, the test's echoes
the fixture's. The 12 gas are calldata bytes of the nonce and the seal.

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
