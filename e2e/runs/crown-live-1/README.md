# crown-live-1: the whole round trip, live, Sepolia -> LEZ -> Sepolia, to DONE (11 Nov 2026)

The owner's `e2e/crown.sh start` on our Modal workspace (app `ferrata-crown`, function `runner`), from a
worktree of branch `integration5`, signing on Sepolia with the throwaway deployer
`0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07` (keystore in the Modal Secret `ferrata-sepolia-key`). 19 of 19
stages, **DONE at 15:14:17Z**, the runner's verify **14 of 14** (`verify.txt`). Every number here is from this run's
files (`stages.done`, `manifest.json`, `deploy.json`, the runner's `crown.log` and LEZ session B's `driver.log`, which
stay uncommitted because they name local paths) or was read back from Sepolia with `cast` on publicnode's public
endpoint (11 Nov, 15:20-15:35Z).

**Real**: every Ethereum transaction (13, on Sepolia); the sync committee's signatures LEZ's light client checked
(captures 1 and 2: Sepolia's own finality updates, headers and storage proofs); the LEZ v0.4.0-rc1 devnet (sequencer
and Logos Blockchain node) with the attestation program and the merged bridge program; both private LEZ transactions,
proven by LEZ's wallet; the `checkpoint_b` proof over the devnet's blocks 179..=252; both release proofs; the Groth16
wraps; FerrataVault v5 in mode (B) behind RISC Zero's Groth16 verifier, which accepted every seal on Sepolia.
**Not shown by this run**: a public LEZ network (the devnet has one validator, one channel key, n = t = 1, leader proofs
in dev mode); a `ferrata_attestor` beside a sequencer (rule (B)'s attestation was signed by the run's driver once T1
was final on the node); the proposal's production parameters (this vault has deploy.sh's testnet defaults: revocation
window 1 day, release delay 120 s, caps unlimited, a freeze authority nobody holds a key for, no watchers); depositor,
recipient and checkpoint prover are all the deployer.

## The four starts

Each stop came from the Secret's load-balanced RPC endpoint answering a read for a block it had just announced with
"block not found" (a node behind the one that answered before). The first two hit forge's next state read before
anything was sent; the third hit the check that follows a send.

| Start | Runner (Modal call), started | Commit | What it did | How it ended |
|---|---|---|---|---|
| 1 | `fc-01M74KND6KWJCPVCZ68VDRN945`, 06:10:36Z | 2854e4e | `lez-a` (1,808 s), `identity`; deploy: the emergency stop (nonce 10), the FerrataRegistry library (nonce 11) | stopped itself 06:42:27Z before the vault: "-32001 block not found: 0xb8c8d7" (block 12,110,039, the library's own); nothing more sent |
| 2 | `fc-01M75138JKY8NMNFZJ06ZPECP1`, 10:05:19Z | 2854e4e | deploy: the vault (nonce 12) | stopped itself 10:06:01Z before the token: "block not found: 0xb8ccc8" (12,111,048, the vault's own); nothing more sent |
| 3 | `fc-01M7560BPSVH2KMXG2T54GCEE9`, 11:31:06Z | cfabb60 | the rest of the deployment (token, both registrations, the stop's owner renounced), deposits, captures, LEZ session B, the checkpoint, submit, W, finalize, release 0 | stopped itself 13:30:49Z after release 0 had succeeded (block 12,112,040): the check after the send read the nullifier as unused |
| 4 | `fc-01M75JPQXXS6VE11QAHQ9XS3EC`, 15:12:59Z | d92e630 | release 0 found done on chain; release 1; verify | **DONE** 15:14:17Z |

So `lez-a`, `identity`, the stop, the library and the vault ran from 2854e4e; everything from the token to release 0
from cfabb60; release 1 and the verify stage from d92e630. The commits differ only in tooling
(`git diff --stat 2854e4e d92e630`: `e2e/crown/cloud/drive.py`, `e2e/crown/sepolia.sh`, `vault/script/deploy.sh`):
the contracts, the programs and their image ids are the same. cfabb60 runs a forge step again, at most 5 times, 24 s
apart, only when its log shows such a read error and the deployer's nonce (latest and pending) is unchanged; d92e630
reads again before stopping when a state check follows a send. All three commits are in `integration6`'s history
(cfabb60 also as its cherry-pick 6f8dc87). **The retry fired once**: release 0's first forge run, 13:30:16Z, "block
not found: 0xb8d0a5" (12,112,037) for the vault's account before sending, nonce still 21: "again in 24 s (1 of 5)";
the second run sent release 0. In the deployment of start 3 every forge step read fine. Wall time from the first start
to DONE 9 h 3 min 41 s, of which the runners ran 2 h 33 min 34 s; the rest was waiting for the owner's restarts.

## Stages

| Stage | Result | Time |
|---|---|---|
| lez-a | fresh devnet; attestation and bridge programs deployed; the Sepolia instance (`Init`, two adopts) brought from period 1378 to 1409: 177 transactions, all applied; T0 = block 178 | 1,808 s |
| identity | deployer nonce 10; vault = CREATE(deployer, 12) = `0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`; configId `0x0c71ff13…3854`; genesisState `0xa696b3c8…bef9`; bedrockGenesisId `0x19383c41…d2ed` | 2 s |
| deploy | seven transactions over starts 1-3 (table below); start 3's part | 50 s |
| deposit0 | 0.001 ETH + 0.000002 relay fee, public kind, leaf 0, block 12,111,477 | 10 s |
| capture1 | block 12,111,477 final, then the next finality update that covers it: capture of block 12,111,509, 6 slots proven (root, count, configId, the ETH registry and fee slots) | 1,178 s |
| deposit1 | 0.001 ETH + 0.000002, private kind, leaf 1, block 12,111,574 ¹ | 9 s |
| capture2 | capture of block 12,111,602, 2 leaves, 6 slots proven | 1,146 s |
| lez-b | generation 1 restored; finality updates of both captures; bridge `Init` (359,397 execution gas), Register ETH (577,927), `ImportRoot` n = 1 (433,528), Claim of deposit 0, public (253,531), ImportFees (446,801), `ImportRoot` n = 2 (429,805), `ClaimPrivately` of deposit 1 (real proof, proven and sent in 270.7 s; 551,574), Burn public (157,961), Burn private (real proof, 317.5 s; 514,235); T1 = block 252. The private proofs ran on the session's CPU container (`lez_session`, 16 to 32 CPUs) | 875 s |
| checkpoint-input, exec | crown-178-252: 618,401 bytes, parent = genesisState, 2 burns, T1's producer inscription; executor 18,933,901 user cycles, 21 segments, journal = native | 1 s |
| checkpoint-prove | asked for an L4 or an L40S, got an L40S: 32.1 s prover wall, 21 segments, image id and input equal to the run's | 35 s |
| checkpoint-wrap | Groth16 seal `0x73c457ba…`, journal 544 bytes (wrap 16 s) | 55 s |
| submit | `submitCandidate` in block 12,111,746 (the wait ends at block time 1,794,403,668) | 12 s |
| releases-prove | both release proofs on the runner's CPU (22 s each) and their wraps (16 s each) | 141 s |
| wait | W = 3,600 s of block time | 3,470 s |
| finalize | `finalizeAttested` in block 12,112,027; final state `0xe7a78c0f…98bd` | 13 s |
| release0 | after the 120 s release delay: block 12,112,040 (start 3; see above) | — |
| release1 | block 12,112,539 (start 4) | 13 s |
| verify | **14 of 14** (`verify.txt`): the 12 chain checks and, with the channel key the runner holds, the checkpoint input rebuilt from the node's blocks byte for byte and the guest re-executed, and the three Groth16 receipts verified locally | 3 s |

¹ The runner logged "deposit 1: block 12111574, leaves 1": a lagging read of the same load-balanced endpoint.
`nextLeafIndex()` is 2 at block 12,111,574 and 1 at 12,111,573 (`cast call --block`, publicnode); capture 2 proved
2 leaves.

Measured latencies (testnet parameters: W = 1 h, release delay 120 s): a deposit's block to its capture 19.7 and
19.1 min (finality, then the next light-client update); the end of LEZ session B (both burns) to release 0's block
64.7 min (checkpoint input, proof and wrap 1.5 min, `submitCandidate`, W, `finalizeAttested`, the release delay).

## The 13 transactions (Sepolia, all status 1, all from the deployer)

| Nonce | Transaction | Hash | Block | Gas used | What it shows |
|---:|---|---|---:|---:|---|
| 10 | create `RiscZeroVerifierEmergencyStop` | `0x3683997bd13df4d3f7b2fc55a8432ee9b12ef8567cdd7d1c4d92ceb9ddeec125` | 12,110,038 | 2,953,028 | the stop at `0x2Ea6…6Ea9` over RISC Zero's Groth16 verifier `0x8f0A…A2A0` (reused from sepolia-live-2) |
| 11 | create `FerrataRegistry` (the library) | `0xbd13e08a45831878124645e911a21a6b5a1824768c0ec35883f3b5ae5e3bdecd` | 12,110,039 | 19,544,631 | `0xe5D7…4B15`, 12,504 bytes, linked by the vault |
| 12 | create `FerrataVault` v5, mode (B) | `0xee91c261265ac3ada50b525edb35a1faa32bb5bb4ffa2b1d433f5a278d9808a3` | 12,111,048 | 39,247,243 | `0xfE31…9E54`, 24,254 bytes; immutables `checkpointImageId` = `checkpoint_b` `0xa475e8f9…d3b2`, `releaseImageId` = release `0x17257f91…f5cd`, `lezConfigId` = slot 2 = the configId, `genesisState` = the identity's; one creation over 2^24 gas (state gas, EIP-8037) |
| 13 | create the test token | `0xafaa8d88c3ba0fa1cbfda5fdf52ecc99b9d49910b627c2aa491b540a0b4fb2fa` | 12,111,473 | 3,315,490 | `0x8E11…B73b` |
| 14 | `registerToken` ETH | `0x192b01c30111da5475388a0e08955f7977bfcd0f8b00150699999d2030bea045` | 12,111,474 | 3,165,774 | ETH with a relay fee of 0.2 % and a checkpoint fee of 0.1 % |
| 15 | `registerToken` test token | `0xa5ee0cb7f381f76aa9a63f1f29050e19f0fe3696fe75b5628c727e94f074aa21` | 12,111,475 | 3,167,052 | |
| 16 | renounce the stop's owner | `0x5d972e21924e020f5ec542cfc9c31df753a57236b72386acd0cf1d92faf8cdf7` | 12,111,476 | 25,220 | the stop's `owner()` is 0: nobody can pause the verifier |
| 17 | `depositETH` j0, public | `0x759d8c722d6e0ac648d5cf8a5fc1d7ba283fd6a05ae52b92edad75be24e376db` | 12,111,477 | 559,652 | 1,002,000,000,000,000 wei escrowed, leaf 0; commitment recomputed by verify |
| 18 | `depositETH` j1, private | `0x295e3a8c51cb1860098d26f4f64dff9fea1f4774926b69906783cb11179e0b29` | 12,111,574 | 268,014 | the same, leaf 1, the private kind (claimed privately on LEZ) |
| 19 | `submitCandidate` | `0x8cbac9699e88da432b033fe4e47b4c5bb6ed5390b6a7869b392e904c24221074` | 12,111,746 | 729,361 | the `checkpoint_b` Groth16 seal verified on Sepolia by RISC Zero's verifier through the stop; tip T1 = 252, 2 burns, 2 imported leaves |
| 20 | `finalizeAttested` | `0xe34734c26557dc2cfad41d7cd7f1e084d163c85832f2b77a1dac62603d938291` | 12,112,027 | 844,361 | after W: `checkpointCount()` 1, final state `0xe7a78c0f…98bd`, burn root `0xbefcd570…0151` accepted |
| 21 | `release` of the public burn | `0xbcc1300760fe74cfd722131ca94d71a39e1398afa89d3df1568ef6ea11fd64f8` | 12,112,040 | 661,668 | release seal verified on chain; nullifier spent; 999,000,000,000,000 wei paid, 1,000,000,000,000 credited to the prover's `owed` |
| 22 | `release` of the private burn | `0x4f043244ce17ea5db36351deba308d0cb320a73ab00daa61cd916b874c39cc69` | 12,112,539 | 465,828 | the same for the burn made privately on LEZ |

74,947,322 gas in all (the deployment 71,418,438), 5.97 times anvil's receipts of the same transactions in
crown-rehearsal-3 (12,546,414): Sepolia charges EIP-8037's state gas, Foundry does not (`vault/GAS.md`). Fees:
0.0000165 ETH in all, nearly all of it release 0's (a priority fee of 25,000,025 wei per gas, as the endpoint
suggested at that send; every other transaction paid 65 to 72 wei per gas).

The ETH (read on Sepolia): the vault's balance went 0, 1,002,000,000,000,000 (deposit 0), 2,004,000,000,000,000
(deposit 1), 1,005,000,000,000,000 (release 0), 6,000,000,000,000 (release 1). Each `Release` event names the gross
0.001 ETH; the vault paid the recipient 0.000999 ETH and credited the 0.1 % checkpoint fee to `owed` of the
candidate's prover (spec §5.4: fees are credited, never pushed). Now `escrowed(ETH)` 4,000,000,000,000 (the two
deposits' relay fees, escrowed with them) + `totalOwed(ETH)` 2,000,000,000,000 = the balance. The deployer's nonce is
23, latest = pending.

## Re-checks

- `verify.txt`: the runner's verify stage, **14 of 14**.
- `verify-laptop.txt`: `e2e/crown.sh verify` on the laptop against publicnode after `e2e/crown.sh fetch --lez`
  (15:21Z): **12 of 12**; the two CPU checks are skipped there by design (they need the devnet's channel key, which
  `fetch` never copies, and the `ferrata-e2e` and `realblocks-crown` binaries).
- `verify-sepolia-publicnode.txt`, `verify-sepolia-ethpandaops.txt`: `python3 -I scripts/verify-sepolia.py
  e2e/runs/crown-live-1 --rpc <endpoint>` on the committed records (15:34Z, release 1 final): **13 passed, 0 pending,
  0 failed**, 61 RPC calls each, on publicnode's and on ethpandaops' public Sepolia endpoints (the endpoint's URL is
  replaced by its name in the files). Its releases check first failed here (the vault's balance above deposits minus
  the `Release` amounts): it counted the gross amount as paid out; it now subtracts the fees each release credited to
  `owed` and checks the balance against `escrowed` + `totalOwed`.
- `controls/`: the same script on altered copies of these records (publicnode, 15:28-15:38Z; a copy's directory name, `t1`
  to `t3`, is what its first line calls the run): one deposit commitment, one byte of the checkpoint seal and the
  genesis state changed, **5 failed** (`verify-sepolia-tampered.txt`); one byte of a node of capture 1's account
  proof, **1 failed** (`-tampered-capture`); release 1's recipient, **1 failed** (`-tampered-recipient`); exit status
  1 each. crown-rehearsal-3's records (the fork; same deployer, nonces and vault address) against Sepolia: **5 failed**.

## Files

The 26 records of crown-rehearsal-3's pattern (the checkpoint input `crown-178-252`), `programs/ids.jsonl`, and for
`scripts/verify-sepolia.py` each capture's record, `header.rlp` and the vault's storage proof. Added on 12 November
from the run's own files, so that `e2e/crown.sh verify crown-live-1` runs from the committed records: session A's
`deployment-settings.yaml` (the devnet's public genesis settings; its sha256 is `bedrockGenesisId`) and each capture's
`block.json` (Sepolia's answer for the block). Not committed: the logs (local paths), the programs and receipts
(binaries), the LEZ sessions' exports and node data; `e2e/crown.sh fetch --lez` brings them from the Modal Volume.
