# FerrataVault gas: v5, with v4, v3 and v2 beside it

v5 (D13's fees) measured on 2026-11-10 on the Modal build box (`spike/modal-build/mrun`), v4 and v3 earlier the same
day, all with forge 1.3.6, solc 0.8.26, optimizer on (200 runs), EVM cancun. "v3 Dn" is item Dn of
[`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md). These are Foundry's
gas rules (Prague). Sepolia has run Glamsterdam since 6 October 2026, which prices new state far higher (EIP-8037):
creation costs there are in "Creating the contracts on today's Sepolia" below; calls that write new storage cost
more there too: the v3 round trip's 16 transactions as Sepolia's own node prices them are in "On Sepolia since
Glamsterdam" below, with the receipts of the v5 round trip sent live on Sepolia (11 November 2026).

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
- relay fees registered (fee schedule 0: no mint, burn or checkpoint fee); the v5 fee rows first announce schedule 1
  on ETH (mint, burn and checkpoint fee 0.1% of each denomination) and wait out its seven days.

The refund rows first retire the vault, finalise a sunset checkpoint and wait until refunds open, the second after
`revokeWindow` (v3 D5, D11). The
checkpoint rows use the test committee, n = 4 and t = 3.

## On Sepolia since Glamsterdam: what the round trip really costs

**Every figure below this section is Foundry's** but those of "Creating the contracts on today's Sepolia" (forge 1.3.6's EVM, which knows Ethereum's gas schedule up to
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
UTC). The round trip deploys the v3 vault of the end-to-end fixture (`e2e/`, branch `sepolia-cloud`), not v5; v5's
creations on Sepolia are in "Creating the contracts on today's Sepolia" below. The transactions are exactly those of the cloud rehearsal ([`../e2e/runs/cloud-fork/`](../e2e/runs/cloud-fork/)
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

**The v5 round trip, live on Sepolia (11 November 2026): the receipts.** `gasUsed` of the 13 transactions of
`crown-live-1` as Sepolia's receipts give them (read back with `cast receipt` on publicnode; records and hashes:
[`../e2e/runs/crown-live-1/`](../e2e/runs/crown-live-1/README.md)), beside anvil's receipts of the same transactions
in the fork rehearsal `crown-rehearsal-3` (Foundry's rules: the deployment from its deploy log, the rest from its
`manifest.json`) and the node's `eth_estimateGas` that set each deployment limit (`deploy.json`; limit = 1.25 x).
The vault is the secured v5 in mode (B), ETH registered with three denominations and fee schedules (relay 0.2 %,
checkpoint 0.1 %).

| Nonce | Transaction | Foundry (anvil receipt, crown-rehearsal-3) | **Sepolia receipt (live)** | Sepolia `eth_estimateGas` | Sepolia / Foundry |
|---:|---|---:|---:|---:|---:|
| 10 | create RiscZeroVerifierEmergencyStop | 451,744 | 2,953,028 | 2,977,632 | 6.54 |
| 11 | create FerrataRegistry (the library) | 2,757,365 | 19,544,631 | 19,700,914 | 7.09 |
| 12 | create FerrataVault, rule (B) | 5,686,715 | 39,247,243 | 39,559,896 | 6.90 |
| 13 | create TestToken | 508,036 | 3,315,490 | 3,342,971 | 6.53 |
| 14 | registerToken (ETH) | 709,314 | 3,165,774 | 3,218,360 | 4.46 |
| 15 | registerToken (test token) | 710,092 | 3,167,052 | 3,219,659 | 4.46 |
| 16 | the stop's owner renounced | 25,524 | 25,220 | 31,788 | 0.99 |
| 17 | depositETH, the tree's first (public kind) | 200,572 | 559,652 | | 2.79 |
| 18 | depositETH, the second (private kind) | 151,394 | 268,014 | | 1.77 |
| 19 | rule (B) submitCandidate, real `checkpoint_b` seal | 375,293 | 729,361 | | 1.94 |
| 20 | rule (B) finalizeAttested | 218,385 | 844,361 | | 3.87 |
| 21 | release (ETH), real seal: the vault's first release and fee credits | 393,072 | 661,668 | | 1.68 |
| 22 | release (ETH), real seal, the second | 358,908 | 465,828 | | 1.30 |
| | **all 13** | **12,546,414** | **74,947,322** | | **5.97** |

A rule (B) checkpoint on Sepolia costs 729,361 + 844,361 = **1,573,722** gas (the same two transactions on Foundry's
rules: 593,678). On Sepolia's rules none of the proof-verifying calls is under the M0 thresholds (candidate and
finalisation ≤ 500k, release ≤ 450k); on Foundry's all are. The node's estimates were 0.8 % (creations) and 1.7 %
(registrations) above the gas used, 26 % for the renounce. One run on one state (a fresh vault, two deposits, the
first checkpoint and releases), not a measurement over states.

## The security fixes of 11 November 2026 (branch `secfix-vault`)

The fixes of the vault findings of `notes/reviews/security/review-2026-11-11.md` change these rows; everything else
in this file is unchanged (all rows below were re-measured; the release rows, the hot path, did not move). Measured
on the build box with Foundry 1.3.6, `forge test --isolate --match-contract 'GasScenarios|ReturnLegGas' -vv`, on
`integration4` (1bb20d5) and on this branch, in one run (logs kept outside the repository):

| Operation (whole transaction) | 1bb20d5 | with the fixes | change | why |
|---|---:|---:|---:|---|
| `submitCandidate`, including its beacon ¹ | 178,555 | 203,207 | +24,652 | M-2: the candidate stores its prover (one new slot); M-1: the submission restarts the keyless clock |
| `submitCandidate`, real checkpoint seal (records a beacon) | 438,444 | 463,096 | +24,652 | the same |
| rule (B): `submitCandidate`, no beacon ¹ | 122,109 | 146,758 | +24,649 | the same |
| `finalize` ¹ | 215,130 | 218,442 | +3,312 | reads the candidate's prover; counts a change of `finalState` (L-1) |
| `finalize`, the first sunset checkpoint ¹ | 283,672 | 286,984 | +3,312 | the same |
| `finalize`, real checkpoint seal echoing the beacon | 457,965 | 461,277 | +3,312 | the same |
| rule (B): `finalizeAttested` | 197,961 | 201,273 | +3,312 | the same |
| `cancelCandidate` (freeze authority / watcher) | 35,663 / 37,438 | 39,702 / 41,477 | +4,039 | clears the prover's slot too |
| `depositETH`, first / later deposit by the address | 179,275 / 159,285 | 181,420 / 161,430 | +2,145 | L-3: reads `finalSunset` (no deposits after a finalised sunset checkpoint) |
| `deposit` ERC-20, first / later deposit by the address | 197,762 / 177,772 | 199,873 / 179,883 | +2,111 | the same |
| `claimOwed` ERC-20 / ETH | 64,992 / 37,761 | 68,334 / 40,934 | +3,342 / +3,173 | its body moved into `FerrataRegistry` (one DELEGATECALL), for EIP-170 |
| `registerToken`, three denominations (ETH / ERC-20) | 647,208 / 648,046 | 649,470 / 650,308 | +2,262 | the library reads the four deployment bounds from the vault's getters, for EIP-170 |
| `refund` ETH / ERC-20 | 179,155 / 205,540 | 179,267 / 205,652 | +112 | |
| `release` (every row, mock and real verifier) | | | 0 | |

`retire()` by anyone now evaluates `retireOpensAt()` (8,938 to 11,263 gas for the view in `gas-report.txt`); the
freeze authority's `retire()` is unchanged. `setTokenCap`, `setGlobalCap` and `setReleaseDelay` cost about 2.2k more
(the bounds, as `registerToken`). Sizes (`forge build --sizes`, box): FerrataVault runtime 24,254 bytes (base 24,242;
322 under EIP-170's 24,576), FerrataRegistry 12,504 (base 11,606). Deployment of the vault: 5,706,902 gas in
`gas-report.txt` (base 5,701,184). Under Sepolia's pricing since Glamsterdam (section above) the new candidate slot
costs more than on mainnet; not measured on the fork.

## Results with the mock verifier

These are the vault's own scenarios. Rows marked ¹ go through RISC Zero's emergency stop into a **mock**
verifier; the same calls with RISC Zero's real Groth16 verifier and real proofs are measured in "With the real
verifier" below.

| Operation | v5 gas (whole transaction) | v4 | v3 | v2 |
|---|---:|---:|---:|---:|
| `depositETH`, first deposit by the address | 179,275 | 173,873 | 173,920 | 173,917 |
| `depositETH`, later deposit by the address | 159,285 | 153,875 | 153,922 | 153,919 |
| `depositETH`, later deposit, all caps UNLIMITED (for comparison) | 138,539 | 133,137 | 133,184 | 133,181 |
| `depositETH`, later deposits over 128 consecutive leaves: min / mean / max | 152,919 / 159,204 / 182,751 | 147,509 / 153,794 / 177,341 | 147,556 / 153,841 / 177,388 | 147,553 / 153,838 / 177,385 |
| `deposit` ERC-20 (18 decimals, returns bool), first deposit by the address | 197,762 | 192,338 | 192,346 | 192,431 |
| `deposit` ERC-20, later deposit by the address | 177,772 | 172,340 | 172,348 | 172,433 |
| `deposit` USDT-style (no return value), first deposit by the address | 197,794 | 192,370 | 192,378 | 192,463 |
| `recordBeacon` (new beacon) | 77,937 | 77,850 | 77,823 | 77,845 |
| `submitCandidate`, including the beacon it records ¹ | 178,555 | 178,501 | 247,481 | 221,157 |
| `finalize` ¹ | 215,130 | 216,659 | 212,565 | 200,736 |
| `finalize`, the first sunset checkpoint (sets the refund boundary) ¹ | 283,672 | 285,193 | 300,999 | – |
| `cancelCandidate` (freeze authority) | 35,663 | 33,023 | 46,740 | – |
| `release` ETH, no relay fee ¹ ² | 173,683 | 167,943 | 167,995 | 168,038 |
| `release` ETH, registered fee paid to `msg.sender` ¹ ² | 185,343 | 179,599 | 179,657 | 179,700 |
| `release` ERC-20 (18 decimals), registered fee paid to `msg.sender` ¹ | 206,083 | 200,339 | 200,397 | 200,440 |
| `release` ETH, the recipient cannot receive, so the amount is credited to `owed` ¹ | 197,485 | 191,706 | 191,758 | 191,801 |
| `refund` ETH (32-step path checked against `depositRoot`) | 179,155 | 167,710 | 127,577 | 121,087 |
| `refund` ERC-20 (18 decimals) | 205,540 | 194,099 | 153,965 | 147,475 |
| `claimOwed` ERC-20, by the payee | 64,992 | 60,126 | 60,132 | 60,112 |
| `claimOwed` ETH, called through the payee contract | 37,761 | 37,708 | 37,713 | 37,697 |
| `revokeFrom` (from `gas-report.txt`: min / median / max) | 23,786 / 73,420 / 84,052 | 23,676 / 73,280 / 83,912 | 23,698 / 69,882 / 80,514 | 23,698 / 67,217 / 77,849 |
| rule (B): `submitCandidate`, no beacon ¹ | 122,109 | 122,055 | – | – |
| rule (B): `finalizeAttested` after the wait (no proof; reads the stop) | 197,961 | 199,413 | – | – |
| `cancelCandidate` (watcher) | 37,438 | 34,798 | – | – |
| `pauseCheckpoints` (watcher) | 47,260 | 47,282 | – | – |
| `proposeRoles` (admin: two watchers and a freeze authority) | 123,391 | 123,368 | – | – |
| `applyRoles` (anyone, 14 days later) | 68,988 | 68,951 | – | – |
| `announceFees`, three denominations (admin; v5) | 248,450 | – | – | – |
| `applyFees`, puts a due fee change in force (anyone; v5) | 82,892 | – | – | – |
| `depositETH`, first deposit by the address, also puts a due fee change in force (v5) | 232,182 | – | – | – |
| `depositETH`, later deposit by the address, a mint fee in force (v5) | 159,285 | – | – | – |
| `release` ETH, relay, burn and checkpoint fees; first credits to the fee recipient and the finaliser ¹ ² (v5) | 256,557 | – | – | – |
| `release` ETH, the same, later credits ¹ ² (v5) | 200,257 | – | – | – |

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

## What changed in v5 (D13 fees; mock verifier, same state)

Measured differences against v4 in the table above; the causes are read from the code, not traced.
- **Deposits about +5.4k** (179,275 against 173,873 for a first ETH deposit): the token's fee state and its schedule's
  word are read, and the total escrowed is the denomination plus the mint and relay fees. A deposit that also puts a
  due fee change in force costs 232,182 (first deposit by the address); `applyFees` alone costs 82,892.
- **Releases about +5.7k** (185,343 against 179,599 with the relay fee): the schedule in force and the root's finaliser
  are read. With burn and checkpoint fees in force, the two credits to `owed` cost 256,557 when both slots are
  written from zero and 200,257 later (fresh recipients in both).
- **Refunds about +11.4k** (179,155 against 167,710): the leaf's checks and its escrow moved into the library
  (a DELEGATECALL) with a search for the leaf's schedule.
- **`cancelCandidate` +2.6k**: its body moved into the library (EIP-170).
- **Checkpoints unchanged**: `submitCandidate` +54, `recordBeacon` +87, `finalize` −1,529, `finalizeAttested` −1,452. The
  finaliser's address is packed with `finalizedAt` in the slot the history already wrote.
- **`registerToken`** (real-verifier table below): 647,208 against 472,235; registration also stores fee schedule 0
  and publishes the registry entry's hash and schedule 0's hash for the LEZ side.

## v5 with the real verifier

Same tests and state as "Rule (B) and rule (A) with the real verifier (v4)" below (`ReturnLegGas`, `RuleBRealGas`,
`forge test --isolate`; the fixture's registrations carry no checkpoint fee):

| Step | v5 | v4 |
|---|---:|---:|
| rule (B): `submitCandidate`, real `checkpoint_b` seal (no beacon) | 347,820 | 347,766 |
| rule (B): `finalizeAttested` after the wait | 195,149 | 196,601 |
| **a rule (B) checkpoint** | **542,969** | 544,367 |
| rule (A): `submitCandidate`, real checkpoint seal (records a beacon) | 438,444 | 438,390 |
| rule (A): `recordBeacon` | 77,937 | 77,850 |
| rule (A): `finalize`, real seal echoing the beacon | 457,965 | 459,494 |
| **a rule (A) checkpoint** | **974,346** | 975,734 |
| `release` ETH / ERC-20, real release seal | 373,657 / 403,523 | 363,117 / 388,179 |
| `registerToken`, three denominations: ETH / ERC-20 | 647,208 / 648,046 | 472,235 / 473,073 |
| `depositETH`, first of the tree / `deposit` ERC-20, second of the address | 198,427 / 197,945 | 193,072 / 192,529 |

Every row is under the M0 thresholds (`docs/traceability.md`, P.3: candidate and finalisation ≤ 500k, release ≤ 450k).

## Creating the contracts on today's Sepolia

Measured read-only on 2026-11-10 with `eth_estimateGas` against `https://ethereum-sepolia-rpc.publicnode.com` (reth
v2.10.0, block 12,107,166, block gas limit 200,000,000), the vault's constructor given the stop's and the library's
runtime code by state override; beside them the receipts of the same creations on a local anvil fork (Foundry's
rules) from `script/fork-rehearsal.sh`:

v5 before the security fixes (the fixed vault's runtime is 24,254 bytes, its library's 12,504: created on Sepolia
by the live round trip with 39,247,243 and 19,544,631 gas, "The v5 round trip, live on Sepolia" above):

| Creation | Runtime bytes | Sepolia (Glamsterdam) | Foundry / anvil |
|---|---:|---:|---:|
| `RiscZeroGroth16Verifier` | 4,810 | 7,795,331 | 1,179,986 |
| `RiscZeroVerifierEmergencyStop` | 1,698 | 2,977,644 | 451,756 |
| `FerrataRegistry` (the library) | 11,606 | **18,301,151** | 2,563,131 |
| `FerrataVault`, rule (A) / rule (B) | 24,242 | **39,538,106 / 39,538,070** | 5,681,056 / 5,681,009 |
| test token | 1,862 | 3,342,971 | 508,036 |

EIP-8037 charges 1,530 gas per byte of code deposited, 183,600 for a new account and 97,920 for a new storage slot,
as *state gas*; EIP-7825's per-transaction cap of 2^24 = 16,777,216 now applies to *execution* gas only, and the
whole `tx.gas` is capped at 2^32 − 1 and by the block gas limit (EIP-8037). Sepolia's fork date and the node's
behaviour come from the Sepolia measurement work on the branch `sepolia-cloud`: Glamsterdam active since
6 October 2026 (`eth_config`), and this node's `eth_estimateGas` refusing a transaction whose execution gas exceeds
2^24, so every estimate above, all of which succeeded, fits the execution cap. The library's and the vault's
totals are above 2^24 (by 1,523,935 and 22,760,890), almost all of it state gas: the vault's code alone is
24,242 × 1,530 = 37,090,260. With the deployment script's margin of 1.25 their limits are 22,876,438 and 49,422,632,
under the block gas limit: **both fit, and no split is needed.** On a chain that capped the whole transaction at
2^24 at this price, a creation could deposit about 10.6 kB of code (8.4 kB with the margin; derived from the
library's and the token's estimates), and the vault and library together (35,848 bytes) would have to become at
least five contracts. `script/deploy.sh` takes every limit from the node's estimate and refuses a transaction above
the cap of the rule it is told (`FERRATA_GAS_RULE=eip8037` on Sepolia); see `README.md`, "Deployment".

## What changed in v4 (mock verifier, same state)

- **`submitCandidate` 178,501 (v3 247,481).** A candidate keeps the digest of its journal, its tip, `t`, times and epoch in
  three slots instead of seven; under rule (A) it still records a beacon. Rule (B) records none: 122,055.
- **`finalize` 216,659 (v3 212,565).** The journal is hashed a second time with `echoNonce` 0 and compared with the
  candidate's digest, and the D11 leaf-count bound is read; the candidate's fields are no longer read from storage.
  Rule (B)'s `finalizeAttested` (199,413) skips the proof and the beacon but reads the verifier stop (an external call).
- **`refund` 167,710 / 194,099 (v3 127,577 / 153,965).** A refund now draws on the token's and the global release bucket
  (two finite buckets in this state) and reads the stop.
- **`cancelCandidate` 33,023 (v3 46,740)**: three slots to clear instead of seven.
- Deposits, releases and claims are within ±60 gas of v3 (function dispatch).

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

## Rule (B) and rule (A) with the real verifier (v4)

Measured on 2026-11-10 on the Modal build box with RISC Zero's unmodified `RiscZeroGroth16Verifier` (risc0-ethereum
v3.0.0) behind its unmodified `RiscZeroVerifierEmergencyStop`, whole transactions (`forge test --isolate`):
- rule (B): `test/real/RuleBReal.t.sol` (`RuleBRealGas`), the real rule (B) proof (guest `checkpoint_b`, cp-empty-360's
  blocks with three certified attestations, n = 4, t = 3; proven on one NVIDIA L4, `gpu-prover/results/ruleb-l4b/`),
  wrapped to Groth16 for this run (identity_p254 on the build box, the Groth16 step with RISC Zero's prover image
  through `mwrap`); the rule (B) vault constructed at the address the journal binds, on chain id 1, a fresh vault;
- rule (A): `ReturnLegGas` again on the v4 vault, the e2e return leg's real seals (same state as in "With the real
  verifier" above: a fresh vault, its first checkpoint, two deposits, UNLIMITED caps).

| Step | Rule (B), gas | Rule (A), gas |
|---|---:|---:|
| `submitCandidate` with a real checkpoint seal | 347,766 (no beacon) | 438,390 (records a beacon) |
| `recordBeacon` at least `W` later | – | 77,850 |
| finalisation | 196,601 (`finalizeAttested`, no proof) | 459,494 (`finalize`, a second real seal) |
| **a checkpoint** | **544,367** | **975,734** |
| `release` ETH / ERC-20 with a real release seal (v4) | – | 363,117 / 388,179 |

Rule (B) costs 0.56 times rule (A) per finalised LEZ state. The estimate before this measurement was about 0.71M
against 1.06M (`docs/design/bridge.md` §10.3). Against the M0 thresholds (`docs/traceability.md`, P.3: candidate and
finalisation ≤ 500k, release ≤ 450k), every row is under them; the v3 vault's first candidate was 7,370 over.

## What a checkpoint costs

A checkpoint is paid once per finalised LEZ state, not per user. Under rule (A) it takes three calls:
`submitCandidate` (which records the first beacon), one more `recordBeacon` at least `W = Δ + (t − 1) × turnSeconds`
later, and `finalize`. With the mock verifier that is about 179k + 78k + 217k ≈ 473k gas on v4 (v3: 538k); with the
real verifier and real proofs it is 975,734 gas on v4 (v3: 1,060,493). Under rule (B) it takes two calls,
`submitCandidate` and `finalizeAttested` an hour later: about 122k + 199k ≈ 321k with the mock verifier, and 544,367
with the real verifier and the real rule (B) proof. Every burn in that state's burn tree then releases against it. A
cancelled candidate costs its submitter the candidate and the wait; resubmitting the same state costs the same again.

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
