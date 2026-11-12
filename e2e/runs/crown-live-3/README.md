# crown-live-3: the round trip on the final programs, live, Sepolia -> LEZ -> Sepolia, to DONE (12 Nov 2026)

The owner's `e2e/crown.sh start crown-live-3` on Modal workspace ws-3 (app `ferrata-crown`, function `runner`),
from the tree `spike/ferrata-wt-integ7`, branch `integration7`, signing on Sepolia with the throwaway deployer
`0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07` (keystore in the Modal Secret `ferrata-sepolia-key`). The programs are
the final ones, [`docs/image-ids.md`](../../../docs/image-ids.md), which any checkout rebuilds to the same ids
(`scripts/reproduce-image-ids.sh`); the light client starts from the fresh checkpoint
[`sepolia-2026-11-12`](../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/SOURCE.md) (period 1411), because
the programs' weak-subjectivity bound refused the 38-day-old one `crown-live-2` started from
([`../crown-live-2/`](../crown-live-2/README.md)). 19 of 19 stages, **DONE at 16:23:30Z**, the runner's verify **14 of
14** (`verify.txt`). **Vault `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`**. Every number here is from this run's files
(`stages.done`, `manifest.json`, `deploy.json`, LEZ sessions A and B's `logs/driver.log` and `export/summary.txt`, and
the runner's `crown.log`, which stays uncommitted because it names local paths) or was read back from Sepolia with `cast`
on publicnode's public endpoint (12 Nov, 16:42-16:50Z).

**Real**: every Ethereum transaction (13, on Sepolia); the sync committee's signatures LEZ's light client checked
(from the period-1411 checkpoint through `Init`, inside the programs' weak-subjectivity bound, then captures 1 and 2:
Sepolia's own finality updates, headers and storage proofs); the LEZ v0.4.0-rc1 devnet (sequencer and Logos Blockchain
node) with the final attestation and bridge programs; both private LEZ transactions, proven by LEZ's wallet; the
`checkpoint_b` proof over the devnet's blocks 25..=90; both release proofs; the Groth16 wraps; FerrataVault v5 in mode
(B) behind RISC Zero's Groth16 verifier, which accepted every seal on Sepolia. **Not shown by this run**: a public LEZ
network (the devnet has one validator, one channel key, n = t = 1, leader proofs in dev mode); a `ferrata_attestor`
beside a sequencer (rule (B)'s attestation was signed by the run's driver once T1 was final on the node); the
proposal's production parameters (this vault has deploy.sh's testnet defaults: revocation window 1 day, release delay
120 s, caps unlimited, a freeze authority nobody holds a key for, no watchers); depositor, recipient and checkpoint
prover are all the deployer.

## The two starts

| Start | Runner (Modal call), started | Commit | What it did | How it ended |
|---|---|---|---|---|
| 1 | `fc-01M7813X6QN5VXADY0VFGDBWEA`, 14:03:23Z | f38f431 | `lez-a` (267 s), `identity`; deploy: the emergency stop (nonce 23), the FerrataRegistry library (nonce 24) | stopped itself 14:09:03Z before the vault: forge's next state read got "-32001 block not found: 0xb8ed32" (block 12,119,346, the library's own); its retry was refused because single nonce reads on the load-balanced endpoint disagreed; nothing more sent |
| 2 | `fc-01M7823A7CFXKMQPA4H0JRD182`, 14:20:29Z | 148ff07 | the rest of the deployment (vault at nonce 25, token, both registrations, the stop's owner renounced), deposits, captures, LEZ session B, the checkpoint, submit, W, finalize, both releases, verify | **DONE** 16:23:30Z |

The two commits differ only in tooling (`git diff --stat f38f431 148ff07`: `e2e/crown/sepolia.sh`,
`vault/script/deploy.sh`): the nonce a retry compares is the highest of three latest-and-pending reads, a retry also
needs no transaction hash in the failed run's log, and the deploy stage's pending check takes the first agreeing of five
reads. The contracts, the programs and their image ids are the same; both commits are in `integration8`'s history (the
merge 6c6b2d4). **No retry fired after the resume** (no "again in" in any log of start 2). The resume's start gate
printed a wrong vault preview, `0x6B15d304…d13F` (CREATE at the current nonce 25 + 2); the run deployed the vault its
identity had fixed at the first start, CREATE(deployer, 23 + 2) = `0x04eE8FF1…e44c` (`identity.json`, and `deploy.sh`
checks the deployed address against it). The gate now prints the identity's vault on a resume (79b9bbc). Wall time from
the first start to DONE 2 h 20 min 7 s, of which the runners ran about 2 h 8 min 42 s; the rest, about 11 min 25 s, was
the retry fix and the owner's restart.

## Stages

| Stage | Result | Time |
|---|---|---|
| lez-a | fresh devnet; attestation and bridge programs deployed (15 transactions); the Sepolia instance `0xa5fb2591…2b5` (`Init` from the period-1411 checkpoint, root `0x3b901f5e…`, and two adopts), then the period-1411 update (adopts the next committee): 23 transactions, all applied; T0 = block 24 | 267 s |
| identity | deployer nonce 23; vault = CREATE(deployer, 25) = `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`; configId `0x10837afe…5553`; genesisState `0x73f67b7f…3b51`; bedrockGenesisId `0x19383c41…d2ed` | 11 s |
| deploy | seven transactions over starts 1 and 2 (table below); start 2's part | 143 s |
| deposit0 | 0.001 ETH + 0.000002 relay fee, public kind, leaf 0, block 12,119,421 | 25 s |
| capture1 | block 12,119,421 final, then the next finality update that covers it: capture of block 12,119,428, 6 slots proven (root, count, configId, the ETH registry and fee slots) | 877 s |
| deposit1 | 0.001 ETH + 0.000002, private kind, leaf 1, block 12,119,492 ¹ | 22 s |
| capture2 | capture of block 12,119,519, 2 leaves, 6 slots proven; its first attempt was not covered yet and waited for the next finality update (about 6.4 min) | 1,512 s |
| lez-b | generation 1 restored; finality updates of both captures (3 steps each); bridge `Init` (359,299 execution gas), Register ETH (577,165), `ImportRoot` n = 1 (432,182), Claim of deposit 0, public (256,314), ImportFees (443,763), `ImportRoot` n = 2 (432,182), `ClaimPrivately` of deposit 1 (real proof, proven and sent in 231.9 s; 553,328), Burn public (160,297), Burn private (real proof, 282.8 s; 515,555): 15 transactions in blocks 25..=90 (38 on the devnet in all), all applied; T1 = block 90 | 760 s |
| checkpoint-input, exec | crown-24-90: 610,824 bytes, parent = genesisState, 2 burns, T1's producer inscription; executor 18,499,940 user cycles, 20 segments, journal = native | 1 s |
| checkpoint-prove | asked for an L4, got an L40S: 31.9 s prover wall, 20 segments, image id and input equal to the run's | 119 s |
| checkpoint-wrap | Groth16 seal `0x73c457ba1603bb74…`, journal 544 bytes (wrap 15 s) | 48 s |
| submit | `submitCandidate` in block 12,119,693 (the wait ends at block time 1,794,500,400) | 22 s |
| releases-prove | both release proofs on the runner's CPU (20 s each) and their wraps (15 and 16 s) | 137 s |
| wait | W = 3,600 s of block time | 3,470 s |
| finalize | `finalizeAttested` in block 12,119,990; final state `0x919b8573…bf19`; burn root `0xf353b9c3…0043` accepted at block time 1,794,500,424 | 15 s |
| release0 | after the 120 s release delay: block 12,120,002 | 156 s |
| release1 | block 12,120,004 | 24 s |
| verify | **14 of 14** (`verify.txt`): the 12 chain checks and, with the channel key the runner holds, the checkpoint input rebuilt from the node's blocks byte for byte and the guest re-executed, and the three Groth16 receipts verified locally | 5 s |

¹ The runner logged "deposit 1: block 12119492, leaves 1": a lagging read of the load-balanced endpoint, as in
crown-live-1. `nextLeafIndex()` is 2 at block 12,119,492 and 1 at 12,119,491 (`cast call --block`, publicnode);
capture 2 proved 2 leaves.

Measured latencies (testnet parameters: W = 1 h, release delay 120 s): a deposit's block to its capture 14.7 and
25.2 min (finality, then the next light-client update; deposit 1 waited for one more update); the end of LEZ session B
(both burns, 15:16:52Z) to release 0's block (16:23:00Z) 66.1 min (checkpoint input, proof and wrap 2.8 min,
`submitCandidate`, W, `finalizeAttested`, the release delay).

## The 13 transactions (Sepolia, all status 1, all from the deployer)

| Nonce | Transaction | Hash | Block | Gas used | What it shows |
|---:|---|---|---:|---:|---|
| 23 | create `RiscZeroVerifierEmergencyStop` | `0x624c37f6ed4d974aed0faf295bd22aee8b68bd477ab43eea22bc239ff00061c5` | 12,119,345 | 2,953,028 | the stop at `0x1dB3…8221` over RISC Zero's Groth16 verifier `0x8f0A…A2A0` (reused) |
| 24 | create `FerrataRegistry` (the library) | `0x331a9ce725760b45188280dd36de5db83671290ebec45a7d868ecdba904fda09` | 12,119,346 | 19,544,631 | `0xa75e…7673`, 12,504 bytes, linked by the vault |
| 25 | create `FerrataVault` v5, mode (B) | `0xa67bfa471825f0dad32e8243444700635a08f39d85b328672f5959b122c12822` | 12,119,409 | 39,247,231 | `0x04eE…e44c`, 24,254 bytes; immutables `checkpointImageId` = `checkpoint_b` `0xf8116722…626e` and `releaseImageId` = release `0xb64900bc…29e9`, both equal to [`docs/image-ids.md`](../../../docs/image-ids.md); `lezConfigId` = slot 2 = the configId, `genesisState` = the identity's; one creation over 2^24 gas (state gas, EIP-8037) |
| 26 | create the test token | `0x9390483b5e4627c159207cdb5e67470cc33825105e6899242d3159be46f30ddb` | 12,119,412 | 3,315,490 | `0x6b47…C3CB` |
| 27 | `registerToken` ETH | `0xb54a468c81ca5163eba5b6696165a6076c74280e6e3ae181d5fd21e1ba77abb4` | 12,119,414 | 3,165,774 | ETH with a relay fee of 0.2 % and a checkpoint fee of 0.1 % |
| 28 | `registerToken` test token | `0x2238bf392d026a2a9efaacd4c2ab47370bb26b23eab6823f65ab8fbe61f50ddf` | 12,119,416 | 3,167,052 | |
| 29 | renounce the stop's owner | `0xb239d611c8297bd5cf488247d710c78f7f38cc1f0a86e5526df03e16be334bf7` | 12,119,418 | 25,220 | the stop's `owner()` is 0 and `paused()` false: nobody can pause the verifier |
| 30 | `depositETH` j0, public | `0x32b188db031993f23358ca29c22b2bc1d3fabb638dc02e80bbe737bbd39cdec6` | 12,119,421 | 559,652 | 1,002,000,000,000,000 wei escrowed, leaf 0; commitment recomputed by verify |
| 31 | `depositETH` j1, private | `0x8f813d221868234eb53a890e52a1e29cdf01862ebf22dc7dd861614fd97cfa08` | 12,119,492 | 268,026 | the same, leaf 1, the private kind (claimed privately on LEZ) |
| 32 | `submitCandidate` | `0xdd09676ec4282beb9822350227cdb1f1f52b3f139b4987c757ab6a1d4b2e025c` | 12,119,693 | 729,349 | the `checkpoint_b` Groth16 seal verified on Sepolia by RISC Zero's verifier through the stop; tip T1 = 90, 2 burns, 2 imported leaves |
| 33 | `finalizeAttested` | `0x427b55d179e77c3f831a5a1ed7e21bc95ed6554d00b64c64f62c8c4fc2d59597` | 12,119,990 | 844,349 | after W: `checkpointCount()` 1, final state `0x919b8573…bf19`, burn root `0xf353b9c3…0043` accepted |
| 34 | `release` of the public burn | `0x1c4827303bae79af1b56e838402316ec783a722da01a4ce1d91870f7bb13ff8e` | 12,120,002 | 661,632 | release seal verified on chain; nullifier spent; 999,000,000,000,000 wei paid, 1,000,000,000,000 credited to the prover's `owed` |
| 35 | `release` of the private burn | `0x1481fea3afaaa50d1e7391412149a3df6b9579a19989658d609667b936de5fa6` | 12,120,004 | 465,768 | the same for the burn made privately on LEZ |

74,947,202 gas in all (the deployment 71,418,426, the six stage transactions 3,528,776): crown-live-1's 13
transactions on the earlier programs used 74,947,322, 120 more (the vault's creation 12 more, the stage transactions
the rest: their calldata and the image ids differ). Fees: 0.0000248635 ETH in all, nearly all of it the deployment's
(the stop and the library at 500,043 wei per gas, the vault at 281,374, the token at 640,703, the token's registration
at 140,711, as the endpoint priced them then); the eight others paid 24 to 543 wei per gas.

The ETH (read on Sepolia): the vault's balance went 0, 1,002,000,000,000,000 (deposit 0), 2,004,000,000,000,000
(deposit 1), 1,005,000,000,000,000 (release 0), 6,000,000,000,000 (release 1). Each `Release` event names the gross
0.001 ETH; the vault paid the recipient 0.000999 ETH and credited the 0.1 % checkpoint fee (`OwedCredited`,
1,000,000,000,000 wei) to `owed` of the candidate's prover (spec §5.4: fees are credited, never pushed). Now
`escrowed(ETH)` 4,000,000,000,000 (the two deposits' relay fees, escrowed with them) + `totalOwed(ETH)`
2,000,000,000,000 = the balance. The deployer's nonce is 36, latest = pending.

## Re-checks

- `verify.txt`: the runner's verify stage, **14 of 14**.
- `verify-laptop.txt`: `e2e/crown.sh verify` on the laptop against publicnode after `e2e/crown.sh fetch --lez`
  (16:33Z): **12 of 12**; the two CPU checks are skipped there by design (they need the devnet's channel key, which
  `fetch` never copies, and the `ferrata-e2e` and `realblocks-crown` binaries). The same from the committed records
  alone: 12 of 12.
- `verify-sepolia-publicnode.txt`, `verify-sepolia-ethpandaops.txt`: `python3 -I scripts/verify-sepolia.py
  e2e/runs/crown-live-3 --rpc <endpoint>` on the committed records (16:48Z): **13 passed, 0 pending, 0 failed**, 61
  RPC calls each, on publicnode's and on ethpandaops' public Sepolia endpoints (the endpoint's URL is replaced by its
  name in the files). The script pins this vault to the final programs: its image-id check reads "crown-live-3's
  vault: 6 of 6 = docs/image-ids.md".
- `controls/`: the same script on altered copies (publicnode, 16:49Z; a copy's directory name is what its first line
  calls the run): these records with crown-live-1's six program ids in every record that names one (`t1`), **3
  failed** (the pin, the vault's immutables, configId recomputed) (`verify-sepolia-tampered-ids.txt`); crown-live-1's
  records with the final programs' ids (`t3`), **3 failed** the same way (`verify-sepolia-crown-live-1-final-ids.txt`);
  exit status 1 each.
- `compare-vault.txt`: `scripts/reproduce-image-ids.sh compare --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`
  (publicnode, 16:57Z) after `scripts/reproduce-image-ids.sh build` of `integration8` (79b9bbc; the guests as of
  4abfbf5) on the Modal build box (2 CPUs, 563 s): **6 of 6** ids of the crown run's programs equal the recorded ones,
  the vault's `checkpointImageId` and `releaseImageId` equal to the rebuilt `checkpoint_b` and release, and **14 of 14**
  guests equal [`docs/image-ids.md`](../../../docs/image-ids.md) ([`docs/guides/verify.md`](../../../docs/guides/verify.md) §2).
- With `cast` (publicnode, 16:42Z): every transaction above (status, block, sender, nonce, gas used, value), the
  deployer's nonce, the vault's code, address and immutables, the stop's verifier, owner and pause flag, `finalState`,
  the burn root, both nullifiers, the balances at each block, `escrowed`, `totalOwed`, `owed` and the `Release` and
  `OwedCredited` events.

## Files

The 36 records of crown-live-1's pattern (the checkpoint input `crown-24-90`; `programs/ids.jsonl`; for
`scripts/verify-sepolia.py` each capture's record, `header.rlp` and the vault's storage proof; for `e2e/crown.sh verify`
session A's `deployment-settings.yaml`, the devnet's public genesis settings whose sha256 is `bedrockGenesisId`, and
each capture's `block.json`), LEZ sessions A and B's `logs/driver.log` and `export/summary.txt` (no local path in
them), the verify outputs, `compare-vault.txt` and the controls. Not committed: the runner's logs and `crown.log` (local
paths),
the programs and receipts (binaries), the LEZ sessions' exports and node data, the captures' other RPC answers, forge's
broadcast records; `e2e/crown.sh fetch --lez crown-live-3` brings them from the ws-3 Volume.
