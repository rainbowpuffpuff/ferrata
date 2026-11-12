# crown-rehearsal-5: the whole crown run on the final programs, on an anvil fork of Sepolia, to DONE (12 Nov 2026)

`e2e/crown.sh start --fork crown-rehearsal-5` on Modal workspace ws-3 (app `ferrata-crown`, `runner_rehearsal`),
from branch `integration7` at commit 2d13f90: the thirteen merged branches and guest builds independent of the build
path. Started 23:45:05Z (11 Nov), DONE 00:08:25Z, no stop: 23 min 20 s of wall time. Every number below is from this
run's files (`stages.done`, `crown.log`, `lez/session-b/logs/driver.log`, `verify.txt`, the records here). The same
programs ran first as [`crown-rehearsal-4`](../crown-rehearsal-4/README.md) (18 of 19 stages; verify stopped on stale
pinned ids, since fixed).

Programs: the final programs of [`docs/image-ids.md`](../../../docs/image-ids.md), which any checkout rebuilds to the
same ids: attestation `0x816d1fef…03df`, bridge `0xccde6171…71dd`, `checkpoint_b` `0xf8116722…626e`, release
`0xb64900bc…29e9` (R0BF programs, `ferrata-e2e program-binary`); LEZ bundle sha256 `4dd813e2…`, 139,304,302 bytes, with
`crown-driver` rebuilt from this tree (the merged wire: lc-a1's `ws_period_epochs`, bridge-controls' controls).

**STAND-IN, labelled**: the Ethereum side is an anvil fork of Sepolia block 12,115,071 (nothing was sent to any public
network; the deployer impersonated); LEZ's Sepolia light client is fed by the stand-in beacon committee over the
fork's own blocks (`lez/crown/capture-anvil.py`); W = 3,600 s and the 120 s release delay were passed with
`evm_increaseTime`. **Real**: the LEZ v0.4.0-rc1 devnet (sequencer + Logos Blockchain node), the attestation program
and the bridge program, both private LEZ transactions proven by LEZ's wallet, the `checkpoint_b` proof over the
devnet's own blocks (GPU), the release proofs, the Groth16 wraps, FerrataVault v5 in mode (B) behind RISC Zero's real
Groth16 verifier, which accepted every seal.

| Stage | Result | Time |
|---|---|---|
| lez-a | new devnet, both programs, stand-in instance; T0 = block 19 | 339 s |
| identity | deployer nonce 23 (the fork's), vault CREATE(n+2) = `0x04eE…de44c`, configId `0xbba1…c51e`, genesisState `0xa558…373a` | 3 s |
| deploy, deposits, captures | the vault deployed (mode B); deposit 0 public, deposit 1 private; fork captures, 6 slots proven each | 0-5 s each |
| lez-b | 2 finality updates (Open, Miller, Finish), Init (222,187 gas), Register ETH, ImportRoot 1, Claim public (256,314 gas), ImportFees, ImportRoot 2, ClaimPrivately (proven and sent in 215.9 s, 553,328 gas), Burn public (160,297 gas), Burn private (proven and sent in 259.4 s, 515,555 gas); T1 = 82 | 675 s |
| checkpoint-input, exec | crown-19-82: 591,104 B, parent = genesisState, 2 burns; journal = native | 1 s |
| checkpoint-prove | an L40S: 32.6 s prover wall, 20 segments, 18,808,076 user cycles, ids and inputs equal the manifest | 43 s |
| checkpoint-wrap, submit | Groth16 seal `0x73c457ba…`, journal 544 B; `submitCandidate` accepted | 55 s |
| releases-prove | both releases proven (26 s each, CPU) and wrapped | 166 s |
| wait, finalize, release0, release1 | W warped; finalised; 0.001 ETH each to the recipient | 3 s |
| verify | **14 of 14 checks passed** (`verify.txt`) | 2 s |

`verify-from-laptop-against-sepolia.txt`: `e2e/crown.sh verify crown-rehearsal-5` from the laptop reads public Sepolia,
where this fork's vault and transactions do not exist: 5 of 12 checks pass (the chain id, bedrockGenesisId, the
captures' proofs, the LEZ session records, the checkpoint journal's fields), the 7 that need the fork's chain fail
(no code at the vault, no deposit logs, no receipts), as expected of a fork run; the runner's verify above ran
against the fork itself.
