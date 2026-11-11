# The crown run's proofs: stages 7 (checkpoint) and 9 (releases)

Part of the crown run (`README.md` beside this file). One entry point, `e2e/crown/proofs.sh <run dir> <step>`; its header
lists the run directory's layout and every step. Rehearsed end to end three times (below); every number here comes from
those runs ("box" = the Modal build box through `spike/modal-build/mrun`, "L4" = one NVIDIA L4 on the gpu-prover app).

## Programs and their image ids
The guests as the first crown agent built them at the crown path. The `.bin` files kept from that build for
checkpoint_b, checkpoint, segment and release are the guests' user ELFs, which the provers refuse ("Malformed
ProgramBinary"); `ferrata-e2e program-binary <elf> <out.bin>` combines each with the v1compat kernel exactly as
risc0-build 3.0.5 does, and the ids it prints equal the ids the build's methods crates generated:

| Program | Image id | R0BF bytes |
|---|---|---|
| checkpoint_b (rule B, with the ImportRoot decoding fix 12d17e3) | `0x7e51065ffd0a8004fbdd0452871c4a77da7b59e6499365f43d61363fd4307754` | 438,668 |
| release | `0x20595dc54bbd80cf487e4666286db9aab7a048e2e0d7646b42ec6d3c90ca2625` | 231,080 |
| checkpoint (rule A) | `0x117384b6322ee8563e147eddb61341699fb3edfd3672c3cb39bc44a49d72086d` | 443,604 |
| segment | `0xdad887eb1bc97fb6901d6b0f92779a18ebf4d70bc425405dc1de8265cfadeb78` | 269,960 |

The vault's `checkpointImageIdB` and `releaseImageId` are the first two.

## Stage 7: attestor and checkpoint
- `checkpoint/realblocks/crown-extract.sh <node api> <out>` at the end of LEZ session B, while the node runs: the node's
  `/cryptarchia/info`, `/channel/0101…01` and its immutable blocks (`/cryptarchia/blocks`: the node's own finality).
- `realblocks-crown` (bin of `checkpoint/realblocks`): every final inscription of the channel parsed strictly,
  re-hashed and linked by checkpoint-core as the guest does, framed, BLAKE2b-hashed and `verify_strict`-checked, the
  MsgId chain followed; refuses unless T1's inscription is at or below the node's LIB slot, the configuration's
  configId (checkpoint-core) equals the bridge wire's, bedrockGenesisId is the SHA-256 of the deployment settings, the
  session files' T0/T1 hashes are the inscribed ones and the state at T0 is the vault's genesisState. The attestor: the
  channel key certifies an attestation key (attestor-core) whose secret is
  `sha256("ferrata/crown/attestation-key/v1" ‖ channel secret)` (never written); it signs FinalTip(T1).
  Writes the checkpoint_b input over T0+1..=T1, `attestor.json` and `burns.json` (every burn the checkpoint counts,
  checked against the guest rule's own frontier).
- Proof on one L4 (`proofs.sh gpu`), identity_p254 on the box, RISC Zero's Groth16 prover image (`wrap.py`: the build
  box app's `groth16_wrap`), the Groth16 receipt verified: `seal.hex` (260 B, selector 0x73c457ba) and `journal.hex`.

## Stage 9: releases
`ferrata-e2e crown-release`: the burn tree from `burns.json` (every leaf recomputed by the bridge core and compared),
one release statement per burn of the LEZ wallet's `burn-notes.json` (COut recomputed from sOut, the chain id, the vault
and the recipient), relayer = recipient and relay fee 0; then a CPU proof each and the same wrap. `proofs.sh fragment`
writes `proofs.json`: crown.json's `candidate {journal, seal}` and `releases[]` with seals. The release journal's
nullifier is the bridge core's v1 `sha256("ferrata/nf-out/v1" ‖ sOut ‖ burnId)`; burn-notes' `nOut` (v2) is another value.

## Rehearsals
| Run | Blocks | Result |
|---|---|---|
| R1 (`checkpoint/realblocks/out-crown-r1`) | the lab's real LEZ blocks 8..=202 (no bridge), STAND-IN Sepolia configuration | 195 blocks, 7,676,000 user cycles, 11 segments; L4 33.6 s (container 35.2 s, GPU peak 11,148 MiB); p254 25.2 s; Groth16 prover 23 s; vault accepts (CrownR1Test 5/5). Release on STAND-IN synthetic burns: 80,501 cycles, CPU 24.1 s (32 threads) |
| R3 | as R1, through `proofs.sh` | every step; L4 39.9 s; `verify` re-derives the input and journal and checks 5 receipts |
| R4 (`checkpoint/realblocks/out-crown-r4`) | the CROWN devnet's blocks 18..=92 of the LEZ part's STAND-IN rehearsal session (crown programs; Init, Register ETH, ImportRoot n = 1, 2, public and private claim, public and private burn; Ethereum side an anvil fork with a stand-in beacon committee) | all 132 final inscriptions accepted; 75 blocks, 165 transactions, 3,372,674 user cycles, 5 segments; 2 burns and imported leaf count 2 counted; L4 16.7 s (container 18.7 s); both releases from the wallet's notes proven (CPU) and wrapped; CrownR4Test: with the run's two deposits the v5 vault (mode B, RISC Zero's verifier) accepts the checkpoint, finalizes it after the hour and pays both releases with their real seals; replays refused |

Still to do in the crown run itself: the same steps on the real session (Sepolia deposits, the real beacon chain), run by
the cloud app; the Groth16 prover step there needs RISC Zero's prover image in that app (here: the build box app).
