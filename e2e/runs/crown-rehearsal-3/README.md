# crown-rehearsal-3: the whole crown run on the merged programs, on an anvil fork of Sepolia, to DONE (11 Nov 2026)

`e2e/crown.sh start --fork crown-rehearsal-3` on our Modal workspace (app `ferrata-crown`, `runner_rehearsal`),
from branch `integration5` at commit cf5edf0: the programs after the merges of `lez-time-fix` (LEZ-side time from
attested records, rule (B)'s check of the tip's producer inscription) and of the security fixes (`secfix-lez`:
holder PDAs, visible imports only, `NOut` v2; `secfix-vault`: FerrataVault with M-1, M-2, L-1, L-3, I-2, I-7).
Started 05:20:49Z, DONE 05:45:08Z, no stop: 24 min 19 s of wall time. Every number below is from this run's files
(`stages.done`, `crown.log`, `lez/session-b/logs/driver.log`, `verify.txt`, the records here).

Programs, built on the Modal build box at the crown build path (`spike/ferrata-wt-crown`, where the earlier crown
build ran; the attestation program came out byte-identical to that build's, image `0x2228bb46…67cf`): bridge program
`ferrata_bridge.bin` image `0x0e9dcd79…bbb6`; `checkpoint_b` `0xa475e8f9…d3b2`, release `0x17257f91…f5cd` (R0BF
programs, `ferrata-e2e program-binary`); LEZ bundle sha256 `7cc85db0…`, 139,311,009 bytes.

**STAND-IN, labelled**: the Ethereum side is an anvil fork of Sepolia block 12,109,639 (nothing was sent to any public
network; the deployer impersonated); LEZ's Sepolia light client is fed by the stand-in beacon committee over the
fork's own blocks (`lez/crown/capture-anvil.py`); W = 3,600 s and the 120 s release delay were passed with
`evm_increaseTime` (run.json `warped`). **Real**: the LEZ v0.4.0-rc1 devnet (sequencer + Logos Blockchain node), the
attestation program and the merged bridge program, both private LEZ transactions proven by LEZ's wallet, the
`checkpoint_b` proof over the devnet's own blocks (GPU), the release proofs, the Groth16 wraps, FerrataVault v5 in
mode (B) behind RISC Zero's real Groth16 verifier, which accepted every seal.

| Stage | Result | Time |
|---|---|---|
| lez-a | new devnet, both programs, stand-in instance; T0 = block 18 | 205 s |
| identity | deployer nonce 10, vault CREATE(n+2) = 0xfE31…9E54, configId 0x4e8a…103b, genesisState 0xad21…fcbc4 | 2 s |
| deploy, deposits, captures | the secured vault deployed (mode B); deposit 0 public, deposit 1 private; fork captures, 6 slots proven each | 1-5 s each |
| lez-b | Init, Register ETH, ImportRoot 1, Claim (to the holder PDA, 253,531 gas), ImportFees, ImportRoot 2, ClaimPrivately (proven and sent in 276.8 s, 551,574 gas), Burn public (through the holder PDA, 157,961 gas), Burn private (336.0 s, 514,235 gas); T1 = block 95 | 857 s |
| checkpoint-input, exec | realblocks-crown crown-18-95: 602,779 B, parent = genesisState, 2 burns, T1's producer inscription; executor 18,512,514 user cycles, journal = native | 1 s |
| checkpoint-prove | an L40S: 33.3 s prover wall, 20 segments, ids and inputs equal the manifest | 41 s |
| checkpoint-wrap, submit | Groth16 seal 0x73c457ba…, journal 544 B; `submitCandidate` accepted | 57 s |
| releases-prove | both releases proven and wrapped | 166 s |
| wait, finalize, release0, release1 | W warped; finalised; 0.001 ETH each to the recipient | 2 s |
| verify | **14 of 14 checks passed** (`verify.txt`) | 2 s |

The checkpoint proof is about six times the cycles of the earlier programs' (crown-rehearsal-1: 2,989,773 user
cycles, 4 segments): rule (B) now checks T's producer inscription, and T1 here is the block of the private burn.
