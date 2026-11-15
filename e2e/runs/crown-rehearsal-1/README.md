# crown-rehearsal-1: the whole crown run in the cloud, on an anvil fork of Sepolia, to DONE (11 Nov 2026)

`e2e/crown.sh start --fork crown-rehearsal-1` on our Modal workspace (app `ferrata-crown`, `runner_rehearsal`),
from branch `crown-rehearsal` = crown-cloud + crown-lez 4453767 + crown-proofs c1c78d1 (commits a8ca7ef, then 754037c
after one fix). Started 02:24:34Z, stopped once at capture 1 (a check of mine that did not know capture-anvil.py's
stand-in header; fixed in the capture check, `start` again resumed at capture 1 with anvil's state reloaded), DONE
02:53:24Z: about 29 minutes of wall time. Every number below is from this run's files (`stages.done`, `crown.log`,
`verify.txt`, the records here).

**STAND-IN, labelled**: the Ethereum side is an anvil fork of Sepolia block 12,108,774 (nothing was sent to any public
network; the deployer 0x79A9…5C07 impersonated); LEZ's Sepolia light client is fed by crown-lez's stand-in beacon
committee over the fork's own blocks (`CROWN_CHAIN=stand-in`, lez/crown/capture-anvil.py); W = 3,600 s and the release
delay of 120 s were passed with `evm_increaseTime` (run.json `warped`). **Real**: the LEZ v0.4.0-rc1 devnet (sequencer +
Logos Blockchain node), the attestation program and the merged bridge program at the crown path, both private LEZ
transactions proven by LEZ's wallet, the `checkpoint_b` proof over the devnet's own blocks (GPU), the release proofs
(CPU), the Groth16 wraps, FerrataVault v5 in mode (B) behind RISC Zero's real Groth16 verifier (0x8f0A…A2A0 of the fork),
which accepted every seal.

| Stage | Where | Result | Time |
|---|---|---|---|
| prepare | runner | tree at the commit, ferrata-e2e + realblocks-crown built (cached on the Volume after) | 82 s build |
| lez-a | lez_session (32 CPUs visible) | new devnet, both programs, stand-in instance; T0 = block 17 | 209 s |
| identity | runner | deployer nonce 10, vault CREATE(n+2) = 0xfE31…9E54, configId 0xdd03…206e, genesisState 0x9d37…c83f | 1 s |
| deploy | runner | sepolia.sh deploy (mode B, eip8037, verifier reused): 7 transactions | 4 s |
| deposit0 / capture1 | runner | j0 public, block 12,108,782; fork capture, 6 slots proven (its check, wrong for fork headers, stopped the first start; see above) | 1 s |
| deposit1 / capture2 | runner | j1 private, block 12,108,783; fork capture, 6 slots proven | 1 s |
| lez-b | lez_session | Init, Register ETH, ImportRoot 1, Claim, ImportFees, ImportRoot 2, ClaimPrivately (real proof, 265.8 s), Burn public, Burn private (real proof, 323.1 s); T1 = block 92 | 827 s |
| checkpoint-input / exec | runner | realblocks-crown crown-17-92: 600,642 B, parent = genesisState, 2 burns; executor journal = native | 1 s |
| checkpoint-prove | gpu_prove | an L40S (no L4 free): 7.0 s prove, 4 segments, ids and inputs equal the manifest | 250 s with the queue |
| checkpoint-wrap | runner + groth16_wrap | p254, Groth16 (15 s), seal 0x73c457ba…, journal 544 B | 49 s |
| submit | runner | submitCandidate accepted (block 12,108,784) | 1 s |
| releases-prove | runner (CPU) | crown-release, 2 CPU proofs (22 s for private-1), 2 wraps | 154 s |
| wait, finalize | runner | W passed (FORK ONLY), finalizeAttested (block 12,108,786) | 1 s |
| release0, release1 | runner | both paid (blocks 12,108,788 and 12,108,789) | 2 s |
| verify | runner | 14 of 14 checks (verify.txt) | 1 s |

Deployment limits (node estimate × 1.25 on the fork, Foundry's gas): stop 564,680, library 3,203,913, vault 7,101,246,
token 635,045, register ETH 897,122, register token 898,111, renounce 37,905. On Sepolia itself the vault needs ~39.5 M
under EIP-8037 (vault/script/deploy.sh): the fork does not price state gas.

verify.txt, in short: configId recomputed from session A's fields (261-byte preimage) = the vault's storedLezConfigId;
bedrockGenesisId = sha256(deployment settings); genesisState recomputed from T0 and the channel key = the vault's; the
vault is CREATE(deployer, 12) in mode B; 6 stage transactions succeeded from the deployer; both Deposit logs carry the
recomputed commitments; both captures' slots verify against their headers; session B's records (T0 17 → T1 92, 9 steps,
a public and a private claim, 2 burn notes) match the run's identity; the journal's fields (domain, chain, vault,
configId, nMin = tMin = 1, parent = genesisState, tip = T1) are right; the checkpoint is final and RISC Zero's verifier
accepts its seal by eth_call (and refuses it with one byte changed); the burn root is accepted; both releases are paid
once, their seals verified on chain, Release events of 1,000,000,000,000,000 wei each (the denomination) to the
deployer, the vault's balance 6,000,000,000,000 wei after them; the checkpoint input rebuilt from the node's recorded blocks is the proven one byte for byte (sha256
369a2434…) and the guest executed again gives the same journal (2,989,773 user cycles); 3 Groth16 receipts verify.

Files: run.json, identity.json, params.json, crown.json (deposits, candidate, releases with seals), deploy.json,
manifest.json, stages.done, verify.txt; lez/ (session-a.json, session-b.json, burn-notes.json); checkpoint/ (the input's
meta, attestor.json, burns.json, the GPU record, exec.txt, seal.hex, journal.hex); releases/ (releases.json, the two
release metas and seals). Not kept here: logs, binaries, the generations (on the Volume ferrata-crown).
