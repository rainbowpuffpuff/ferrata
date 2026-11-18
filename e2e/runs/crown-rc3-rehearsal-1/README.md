# crown-rc3-rehearsal-1: the whole crown run with the LEZ side on LEZ v0.4.0-rc3, on an anvil fork of Sepolia, to DONE (18 Nov 2026)

`e2e/crown.sh start --fork --inputs rc3 crown-rc3-rehearsal-1` on our Modal workspace (app `ferrata-crown`,
`runner_rehearsal`), from branch `crown-rc3` at commit 799b4297 (main `476a2bc9`, the merged branch `rc3-return-leg`,
and this branch's host changes: [`e2e/crown/README.md`](../../crown/README.md), "On LEZ v0.4.0-rc3"). Started
14:18:22Z, DONE 15:11:45Z, no stop: 53 min 23 s of wall time. Every number below is from this run's files
(`stages.done`, `crown.log`, `lez/session-*/logs/driver.log`, `checkpoint/gpu/run.json`, `verify.txt`, the records here).
The same `start` without `--fork`, under another run name, is the live run `crown-live-4`.

Input set `rc3` ([`e2e/crown/cloud/inputs.json`](../../crown/cloud/inputs.json); the Volume's `inputs-rc3/`, uploaded by
`setup --inputs rc3`; crown-live-3's `inputs/` untouched): the LEZ bundle `bundle.json` (142,517,880 bytes, sha256
`2b79196a…`): LEZ v0.4.0-rc3's `sequencer_service` (`bafd07be`, sha256 `be2ac354…`), its Logos Blockchain node
0.3.5-rc.1 (release asset of `f6d4267f`, sha256 `2f301729…`, 4-s slots), rc3's local configuration, crown-driver built
with rc3 selected (sha256 `37f3369d…`), and crown-live-3's attestation and bridge programs and r0vm byte for byte
(attestation `0x816d1fef…03df`, bridge `0xccde6171…71dd`; [`docs/image-ids.md`](../../../docs/image-ids.md)); the R0BF
programs of `programs/ids.jsonl`: the second build's `checkpoint_b_0_3_5` `0xafbf4023…3bcf` and `checkpoint_0_3_5`
`0x259cd89e…9b0f` (built on the build box, ids and sha256 equal to docs/image-ids.md), the final `segment` and
`release` `0xb64900bc…29e9`. The light client's checkpoint in the bundle: [`sepolia-2026-11-18`](../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-18/SOURCE.md)
(used by the live run; this rehearsal's session A runs on the stand-in, below).

**STAND-IN, labelled**: the Ethereum side is an anvil fork of Sepolia block 12,162,003 (nothing was sent to any public
network; the deployer impersonated); LEZ's Sepolia light client is fed by the stand-in beacon committee over the
fork's own blocks (`lez/crown/capture-anvil.py`); W = 3,600 s and the 120 s release delay were passed with
`evm_increaseTime`; the gas figures are anvil's (its deployment does not price as Sepolia does under EIP-8037: the
same vault code cost 71.4 M gas live in crown-live-3). **Real**: the LEZ v0.4.0-rc3 zone (rc3's sequencer and the
0.3.5-rc.1 node, one sequencer, one validator, dev-mode leader proofs), the attestation and bridge programs on it,
both private LEZ transactions proven by rc3's wallet with the sequencer verifying the receipts, the
`checkpoint_b_0_3_5` proof over the zone's own blocks (GPU), the release proofs, the Groth16 wraps, FerrataVault v5 in
mode (B) pinning `checkpoint_b_0_3_5` behind RISC Zero's real Groth16 verifier (Sepolia's, on the fork), which
accepted every seal.

| Stage | Result | Time |
|---|---|---|
| lez-a | a new rc3 zone; node slot 0 = unix second 1,785,493,027 (4-s slots), the sequencer's blocks 3 s after an epoch second (one start); the node's fork digest `0xc025c6d2…58ac` (`GET /chain/fork`, `lez/session-a/chain-fork.json`) passed to crown-a as `bedrockGenesisId`; both programs deployed and the stand-in instance made, 18 transactions applied; T0 = block 23 | 287 s |
| identity | deployer nonce 36 (the fork's, Sepolia's at that block), vault CREATE(n+2) = `0x8055…3470`, configId `0xccc88690…f1f7`, genesisState `0x0518d191…bca3` | 2 s |
| deploy, deposits, captures | the vault deployed (mode B, `checkpointImageId` = `checkpoint_b_0_3_5`), the stop `0xB249…EfCA`, the library `0x837F…EAfe`, the token `0xD599…Ab43`; deposit 0 public, deposit 1 private; fork captures, 6 slots proven each | 71 s, 9 s, 9 s |
| lez-b | generation 1 restored (the sequencer's blocks 1 s after an epoch second); 2 finality updates (Open, Miller, Finish), Init (227,707 gas), Register ETH (464,288), ImportRoot 1 (312,106), Claim public (256,314), ImportFees (322,912), ImportRoot 2 (312,106), ClaimPrivately (**proven and sent in 773.9 s**, 553,328 gas), Burn public (160,297), Burn private (**proven and sent in 938.5 s**, 515,555 gas); T1 = 225; the node's final blocks dumped (slots 2,376,676..=2,380,276) | 2,118 s |
| checkpoint-input, exec | crown-23-225 over blocks 24..=225: 682,838 B, parent = genesisState, 2 burns; `checkpoint_b_0_3_5` in the executor: 19,540,105 user cycles, 21 segments, journal = native | 1 s |
| checkpoint-prove | an L40S: 33.9 s prover wall, 21 segments, ids and inputs equal the manifest (257 s with the GPU's queue and start) | 257 s |
| checkpoint-wrap, submit | Groth16 14 s, seal `0x73c457ba…`, journal 544 B; `submitCandidate` accepted (375,293 gas) | 58 s, 9 s |
| releases-prove | both releases proven on the CPU (27 s, 25 s) and wrapped (11 s, 15 s) | 161 s |
| wait, finalize, release0, release1 | W warped (FORK ONLY); `finalizeAttested` (218,385 gas), final state `0x0c1b4706…6be7`; release delay warped; 0.001 ETH each to the recipient (393,108 and 358,884 gas) | 27 s |
| verify | **14 of 14 checks passed** (`verify.txt`), among them `bedrockGenesisId` = the node's fork digest at sessions A and B, the vault's ids = the run's records, the checkpoint input rebuilt from the node's recorded blocks byte for byte and executed again | 2 s |

Two observations for the live run. The private claim and burn took 773.9 s and 938.5 s here (crown-live-3 on rc1:
231.9 s and 282.8 s on the same function; rc3's own sessions with 32 prover threads: 264.3 s and 302.2 s, EVIDENCE row
123), so session B took 35 min instead of 13; `session.sh` now gives the prover every CPU the container shows
(commit 0ef6a47b), which `crown-rc3-rehearsal-2` runs. The fork digest is the same as rc3-return's zone's: it commits
to the genesis and era digests of LEZ's local deployment settings, so every zone from those settings carries it; what
tells this zone apart is its channel key (one key per `setup --create-channel`), as on rc1.
