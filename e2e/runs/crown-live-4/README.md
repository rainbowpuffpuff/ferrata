# crown-live-4: the round trip live on Sepolia with the LEZ side on LEZ v0.4.0-rc3, to DONE (18 Nov 2026)

The main session's `e2e/crown.sh start --inputs rc3 crown-live-4` (approved by the owner) on our Modal workspace (app
`ferrata-crown`, function `runner`), from the tree `spike/ferrata-wt-crown-rc3`, branch `crown-rc3`, signing on Sepolia
with the throwaway deployer `0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07` (keystore in the Modal Secret
`ferrata-sepolia-key`). The same round trip as [`crown-live-3`](../crown-live-3/README.md), with the LEZ side on LEZ
v0.4.0-rc3: rc3's own `sequencer_service` (`bafd07be`) and the Logos Blockchain node it pins, 0.3.5-rc.1 (`f6d4267f`),
**one sequencer and one validator** with dev-mode leader proofs, the final LEZ programs unchanged (attestation
`0x816d1fef…03df`, bridge `0xccde6171…71dd`), and a new FerrataVault v5 in mode (B) pinning the **second build** of the
checkpoint guests, `checkpoint_b_0_3_5` `0xafbf4023…3bcf`, with the final release `0xb64900bc…29e9`
([`docs/image-ids.md`](../../../docs/image-ids.md); input set `rc3`, [`e2e/crown/README.md`](../../crown/README.md) "On LEZ
v0.4.0-rc3"). The light client started from the fresh checkpoint
[`sepolia-2026-11-18`](../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-18/SOURCE.md) (period 1416). Rehearsed
first on a fork: [`crown-rc3-rehearsal-1`](../crown-rc3-rehearsal-1/README.md),
[`crown-rc3-sessiona-sepolia-1`](../crown-rc3-sessiona-sepolia-1/README.md). 19 of 19 stages, **DONE at 20:15:43Z**, the
runner's verify **14 of 14** (`verify.txt`). **Vault `0x80555Fe53CCcEf46951B23A3c75643a4C9283470`**. Every number here is
from this run's files (`stages.done`, `manifest.json`, `deploy.json`, `crown.log` with local paths shortened, LEZ
sessions A and B's `logs/`) or was read back from Sepolia with `cast` (ethpandaops' public endpoint, 18 Nov 20:44–20:52Z).

**Real**: every Ethereum transaction (13, on Sepolia); the sync committee's signatures LEZ's light client checked (the
period-1416 checkpoint through `Init`, then Sepolia's own finality updates, headers and storage proofs of captures 1
and 2); the LEZ v0.4.0-rc3 zone with the final programs; both private LEZ transactions, proven by rc3's wallet with the
sequencer verifying the receipts; the `checkpoint_b_0_3_5` proof over the zone's blocks 34..=87 (GPU); both release
proofs; the Groth16 wraps; FerrataVault v5 in mode (B) behind RISC Zero's Groth16 verifier, which accepted every seal on
Sepolia. **Not shown by this run**: Logos's devnet or a public LEZ network (the zone is ours: one sequencer, one
validator, one channel key, n = t = 1, leader proofs in dev mode); a zone of several sequencers on 0.3.5-rc.1; a
`ferrata_attestor` beside the sequencer (the run's driver signed rule (B)'s attestation once T1 was final on the node);
a Bedrock era change (one fork digest is pinned); the proposal's production parameters (deploy.sh's testnet defaults:
revocation window 1 day, release delay 120 s, caps unlimited, a freeze authority nobody holds a key for, no watchers);
depositor, recipient and checkpoint prover are all the deployer.

## The four starts

| Start | Runner (Modal call), started | Commit | What it did | How it ended |
|---|---|---|---|---|
| 1 | `fc-01M7QMNRBYMC1E0ERMYX9R7KVX`, 15:33:53Z | 12d0486e | `lez-a` (499 s), `identity`; deploy: the emergency stop (nonce 36), the FerrataRegistry library (nonce 37) | stopped 15:49:25Z: the public endpoint's lagging read ("block not found") while deploy.sh simulated the vault; nothing more was sent (nonce 38 next) |
| 2 | `fc-01M7QNZ5P6QM2Q7AMVMYGGV3M1`, 15:56:26Z | 8b9ffb7c | the rest of the deployment (nonces 38–42), deposit 0, capture 1, deposit 1 | stopped 16:21:37Z: "the vault counts 1 deposits of the deployer", a lagging read right after deposit 1, which had landed (nonce 45 next) |
| 3 | `fc-01M7QQYHSDDBYXBNNVMGGZ8QDT`, 16:31:11Z | 8a9e1a12 | deposit 1 skipped (already counted), capture 2, LEZ session B, the checkpoint, `submitCandidate`, the release proofs, W, `finalizeAttested` | stopped 20:09:14Z at release 0: forge could not fork the endpoint's head ("failed to get block for block number"); nothing was sent (nonce 47 next) |
| 4 | `fc-01M7R4NPYZKVG2BR93NAFJ1RYD`, 20:13:23Z | 8a9e1a12 | release 0, release 1, verify | DONE 20:15:43Z |

Three stops on the public endpoint's lagging reads (deploy 15:49, deposit 1's count 16:21, release 0 20:09), each
resumed by the main session with the same `start` after reading the deployer's nonce on chain (38, 45, 47). The commits
differ only in records and host code: 8b9ffb7c adds a rehearsal's record; 8a9e1a12 adds documentation,
`scripts/verify-sepolia.py`'s reading of an rc3 run and drive.py's re-reads for up to 70 s after a deposit or
`finalizeAttested` it sent (the second stop's cause). The contracts, the programs and their image ids are the same in
all three. In start 3, LEZ session B's first container ended after about 70 minutes and the second ran from 17:51 UTC:
it restored generation 1 again (the zone after session A) and ran session B whole. Wall time from the first start to
DONE 4 h 41 min 50 s.

## Stages

| Stage | Result | Time |
|---|---|---|
| lez-a | a new rc3 zone (node slot 0 = unix second 1,785,493,027, 4-s slots; the sequencer's blocks 5 s after an epoch second, one start); the node's fork digest `0xc025c6d2…58ac` as `bedrockGenesisId` (`lez/session-a/chain-fork.json`); both programs deployed; the Sepolia instance `0x0740a232…22b7` (`Init` from the period-1416 checkpoint, 114,811 gas, and two adopts), the period-1416 update (adopted); 23 transactions; T0 = block 33 | 499 s |
| identity | deployer nonce 36; vault = CREATE(deployer, 38) = `0x80555Fe5…3470`; configId `0xaafbcbf6…893c` (the same as the rehearsal's session A predicted); genesisState `0xdaa0ae2a…ffab` | 2 s |
| deploy | seven transactions over starts 1 and 2 (table below); start 2's part | 132 s |
| deposit0 | 0.001 ETH + 0.000002 relay fee, public kind, leaf 0, block 12,162,489 | 25 s |
| capture1 | block 12,162,489 final, then the next finality update that covers it: capture of block 12,162,496, 6 slots proven | 1,272 s |
| deposit1 | 0.001 ETH + 0.000002, private kind, leaf 1, block 12,162,595 (sent in start 2; start 3 found it counted) | 1 s |
| capture2 | capture of block 12,162,621, 2 leaves, 6 slots proven | 456 s |
| lez-b | (the second container, 17:51:28Z–19:01:42Z) generation 1 restored; the prover given 32 threads (`RAYON_NUM_THREADS`, nproc 16); finality updates of both captures (3 steps each); bridge `Init` (358,912 execution gas), Register ETH (577,096), `ImportRoot` n = 1 (440,501), Claim of deposit 0, public (256,314), ImportFees (446,538), `ImportRoot` n = 2 (441,668), `ClaimPrivately` of deposit 1 (real proof, **proven and sent in 246.3 s**, 553,328), Burn public (160,297), Burn private (real proof, **proven and sent in 286.1 s**, 515,555); T1 = block 87. Most blocks were final 19–25 s after the driver sent a transaction; three (`Init`, `ClaimPrivately`, the private Burn) only after 1,034–1,227 s, the 0.3.5-rc.1 node's long finality waits seen on rc3 before (EVIDENCE row 112) | 8,490 s |
| checkpoint-input, exec | crown-33-87 over blocks 34..=87: 606,621 bytes, parent = genesisState, 2 burns; the executor on `checkpoint_b_0_3_5`: 18,773,532 user cycles, 20 segments, journal = native | 0 s |
| checkpoint-prove | an L4: **67.6 s** prover wall, 20 segments, image ids and input equal to the run's | 131 s |
| checkpoint-wrap | Groth16 seal `0x73c457ba…`, journal 544 bytes (wrap 15 s) | 39 s |
| submit | `submitCandidate` in block 12,163,384 (the wait ends at block time 1,795,032,300) | 17 s |
| releases-prove | both release proofs on the runner's CPU (17 s each) and their wraps (11 and 16 s) | 108 s |
| wait | W = 3,600 s of block time (3,492 s waited after the proofs) | 3,504 s |
| finalize | `finalizeAttested` in block 12,163,687; final state `0x99ebd540…df71` | 110 s |
| release0 | after the 120 s release delay (waited in start 3, sent in start 4): block 12,163,725 | 19 s |
| release1 | block 12,163,730 | 59 s |
| verify | **14 of 14** (`verify.txt`): the 12 chain checks and, with the channel key the runner holds, the checkpoint input rebuilt from the node's blocks byte for byte and the guest re-executed, and the three Groth16 receipts verified | 6 s |

## The 13 transactions (Sepolia, all status 1, all from the deployer)

| Nonce | Transaction | Hash | Block | Gas used |
|---:|---|---|---:|---:|
| 36 | create `RiscZeroVerifierEmergencyStop` (`0xB249…EfCA`, over RISC Zero's verifier `0x8f0A…A2A0`) | `0x462a24df8765b842ee8064a512a79266450bf0e07d911baac19734d45460ce15` | 12,162,419 | 2,953,028 |
| 37 | create `FerrataRegistry` (the library, `0x837F…EAfe`) | `0x8f16738889e6535415e7ce417d1ba81ec489ad5fa24dd345ad9ff75d63844a94` | 12,162,431 | 19,544,631 |
| 38 | create `FerrataVault` v5, mode (B), `0x8055…3470`: `checkpointImageId` = `checkpoint_b_0_3_5` | `0x5f1de5e64625d6ac3a3112bc6e861d72ba453956e06f1d9362fb45cbe8c5ca93` | 12,162,478 | 39,247,219 |
| 39 | create the test token (`0xD599…Ab43`) | `0x8ea6fae61cc21d0696d63f518e16d98f4d95f1504c576e5ee2e47eec8e91dabc` | 12,162,480 | 3,315,490 |
| 40 | `registerToken` ETH (relay fee 0.2 %, checkpoint fee 0.1 %) | `0x3ef4bb0cb776bd844a4c597332030f37affe36b005a14549c3b3747db4ae5208` | 12,162,482 | 3,165,774 |
| 41 | `registerToken` test token | `0x6ade649c813513ad4c0d407bbc95150a2dd8e171447823c06291569a84ac96e5` | 12,162,484 | 3,167,052 |
| 42 | renounce the stop's owner | `0xb1eb9ceef0e95409fcc34d99eb4b2160e1ae2feb86e2920c828bfd5e8c5795be` | 12,162,486 | 25,220 |
| 43 | `depositETH` j0, public (1,002,000,000,000,000 wei) | `0xfd6f89b823bbbaaabcddc65b9ebd1eacea90887d6ac5c5e0f5f6eb148f372990` | 12,162,489 | 559,640 |
| 44 | `depositETH` j1, private (the same) | `0x672306b9373f4b0ad4f3141f9f492079340dc4535f2c6acf2139505a7d334973` | 12,162,595 | 268,026 |
| 45 | `submitCandidate` (the `checkpoint_b_0_3_5` seal verified on Sepolia) | `0x41f727b5dd52427a7797873a1fdd6fa03f61bbcade014f399c65ff26fcb05d66` | 12,163,384 | 729,373 |
| 46 | `finalizeAttested` (after W) | `0xc971955cf4cf400ebca844c417ed9a69fc119dd59d2f46f09447856de5c56b02` | 12,163,687 | 844,361 |
| 47 | `release` of the public burn (0.001 ETH to the deployer) | `0xcacacf397e9d2f1b647474efcbd940dac63b145df0e9aa20d84259029dd5544e` | 12,163,725 | 661,668 |
| 48 | `release` of the private burn (the same) | `0x4c54ad76e2d16e6205d39aaffcb0898a6e24cda6f3864876830ee3c4e20479dd` | 12,163,730 | 465,828 |

74,947,310 gas in all (the deployment 71,418,414, the six stage transactions 3,528,896): 108 more than crown-live-3's
74,947,202 for the same transactions (the vault 12 less; the stage transactions' calldata and ids differ). Fees:
11,397,928,834,757 wei (0.0000114 ETH), nearly all of it the stop's and the library's (550,016 and 500,016 wei per gas,
as the endpoint priced them then); the other eleven paid 19 to 21 wei per gas. The deployer is now at nonce 49.

## Re-checks

- `verify.txt`: the runner's verify stage, **14 of 14**.
- `verify-laptop.txt`: `e2e/crown.sh verify crown-live-4` on the laptop against publicnode after `fetch --lez`
  (18 Nov 20:41Z): **12 of 12**; the two CPU checks are skipped there by design (they need the zone's channel key, which
  `fetch` never copies).
- `verify-sepolia-publicnode.txt`, `verify-sepolia-ethpandaops.txt`: `python3 -I scripts/verify-sepolia.py
  e2e/runs/crown-live-4 [--rpc URL]` (20:42Z, 20:43Z): **13 passed, 0 pending, 0 failed**, 61 RPC calls each, on both
  public endpoints; its image-id check names the second build of the checkpoint guests and reads 8 of 8 ids equal to
  docs/image-ids.md, its configId check reads `bedrockGenesisId` = the node's fork digest.
- `sourcify-status.txt`: `vault/script/sourcify.sh status` with this run's `deploy.json` (read-only, 20:48Z): the vault,
  its registry library and its emergency stop exact matches on Sourcify, the test token a match under another
  contract's name (`MockUSDT`).
- With `cast` (ethpandaops, 20:44–20:52Z): the 13 transactions above (status, block, sender, nonce, gas used, price, value)
  and the deployer's nonce.

## Files

The records of crown-live-3's pattern: `run.json`, `identity.json`, `params.json`, `deploy.json`, `manifest.json`,
`crown.json`, `bundle.json`, `stages.done`, `DONE`, `crown.log` (local paths shortened to `spike/`), the captures'
records with `header.rlp` and the vault's storage proof, the checkpoint input's metadata `crown-33-87.json`,
`attestor.json`, `burns.json`, `exec.txt`, the GPU's `prove.json` and `run.json`, the Groth16 `journal.hex` and
`seal.hex`, the releases' records and seals, `programs/ids.jsonl`, LEZ session A's `session-a.json`,
`deployment-settings.yaml` and `chain-fork.json`, session B's `session-b.json`, `burn-notes.json`, the node's
`chain-fork.json`, both sessions' `driver.log`, session B's `session.log`, and the verify outputs. Not committed: the
zone's channel key (never fetched), the checkpoint's 606,621-byte input and the receipts (on the Volume), the node's
block dump, the sequencer logs, the runner's own logs and forge's broadcast records.
