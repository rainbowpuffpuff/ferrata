# The crown run: the whole round trip on the final programs (design)

Status (11 Nov 2026, 01:45 CET): design done; the Sepolia stages rehearsed on a fork with stand-in values
(`e2e/runs/crown-fork-3r/`); every guest built at this worktree's path; a fresh devnet deployed the crown programs.
NOT READY TO RUN: the LEZ side's driver on the merged wire, the session save/restore, the attestor/checkpoint input
from the session's blocks, the GPU session, the release inputs from real burns, the cloud app and the full rehearsal
remain (the list below, "What remains"). `spike/tmp/crown/STATE.md` (outside the repository) tracks the work. One cloud run, started once, that crosses Sepolia -> LEZ -> Sepolia with the final programs and real
proofs everywhere, and a verify script that re-checks every step from public data and the recorded blocks.

## Programs (all built on the build box at this worktree's path; image ids are path-dependent)

| Program | Source | Role |
|---|---|---|
| attestation program | `lez/guests/program` (as `integration` 5a94634) | Sepolia light client on LEZ (Gloas) |
| merged bridge program | `lez/guests/bridge`, `lez/bridge-wire` (as `integration`; `f11b1aba…` at that worktree's path) | registry, fees, caps, freezes, Sunset, Gloas header |
| `checkpoint_b` | `checkpoint/` (after the `ImportRoot` fix 12d17e3) | rule (B) checkpoint over the session's blocks |
| release | `bridge/` | one release per burn |
| FerrataVault v5 + FerrataRegistry | `vault/src` | mode (B), RISC Zero's Groth16 verifier behind an emergency stop |

The ids at this path are measured in piece 2 and written into the run's `params.json`; nothing reuses an id of
another worktree. The checkpoint decoder reads bridge variants 0-3 (Burn, ImportRoot, Sunset, Echo), byte-identical
in the merged wire (`checkpoint/crates/checkpoint-core/src/wire.rs`, `lez/harness/tests/bridge_burn.rs`).

## Stages and where each runs

Modal workspace ws-3, app `ferrata-crown` (CPU runner, GPU session, wrap), Volume `ferrata-crown`. Builds on
the build box (`mrun --profile ws-4`). The laptop only starts, reads status and fetches.

| # | Stage | Machine | Persists (Volume) |
|---|---|---|---|
| 0 | pre-flight: deployer nonce N (pending = latest), balance, gas price, node-priced plan; the owner types yes | runner (CPU) | `run.json` |
| 1 | LEZ session A: fresh devnet (LEZ v0.4.0-rc1 sequencer + Logos Blockchain node, LEZ's local config, dev-mode leader proofs), `--create-channel`, deploy attest + bridge, Sepolia instance Init + 2 adopts, period updates 1378 -> current; stop cleanly; T0 = tip | CPU function | `lez/gen-1.tar.zst`, `lez/session-a/` (blocks, channel key's public part, deployment settings, T0) |
| 2 | identity: bedrockGenesisId = sha256(deployment-settings), channelId 0x01..01, keys [channel pk], postingTimeframe 0, configTipHash 0 (from `/channel`), genesisState(T0), vault = CREATE(deployer, N+2), configId | runner | `identity.json`, `params.json` |
| 3 | Sepolia deploy (script/deploy.sh, FERRATA_MODE=B, eip8037, verifier reused): stop N, library N+1, vault N+2, token N+3, register ETH N+4 (relay 0.2 %, checkpoint 0.1 %), register token N+5, renounce N+6 | runner | `deploy.json`, manifest |
| 4 | deposit j0: 0.001 ETH (+ 0.000002 relay fee) public to LEZ public test account 0; wait finality; capture 1 (vault slots 0,1,2 + the ETH registry and fee slots, deposits, updates) | runner | `lez-capture-1/` |
| 5 | deposit j1: 0.001 ETH private to LEZ private test account 0; wait finality; capture 2 | runner | `lez-capture-2/` |
| 6 | LEZ session B: restore gen-1; catch-up + finality updates of captures 1, 2; bridge Init (pairing: slot 2), Register ETH, ImportRoot n=1, n=2; Claim j0 (public), ClaimPrivately j1 (real proof); Burn public (account 0), Burn private (private account 0, real proof); wait Bedrock finality of the tip T1; stop; save | GPU function (L4, L40S fallback): `r0vm` 3.0.5 with CUDA as LEZ's prover, so both private proofs are GPU proofs against the live sequencer | `lez/gen-2.tar.zst`, `lez/session-b/` (driver export: blocks, txs, receipts) |
| 7 | attestor + checkpoint: the channel key certifies the attestation key (attestor-core), which signs FinalTip(T1); `checkpoint_b` input over T0+1..=T1 from the node's blocks and the driver's record; real proof (GPU), identity_p254 (runner CPU), Groth16 (prover image) | GPU function + runner + wrap function | `checkpoint/` |
| 8 | submitCandidate (N+9); wait W = 3600 s; finalizeAttested (N+10) | runner | manifest |
| 9 | release proofs for both burns (release program, CPU, during W) + wraps; after the release delay (120 s) release ETH twice (N+11, N+12) | runner + wrap | `releases/` |
| 10 | verify (both chains, from public data and the recorded blocks), DONE | runner (re-runnable on the box) | `verify.txt` |

Why the devnet comes first: the vault's `genesisState` is the checkpoint state at a real LEZ tip (tip id, tip hash,
key set of the channel, configTipHash; vault/test/real/RealBlocks.t.sol `_state`), and `setup --create-channel`
draws a random channel key (7 setups, 7 keys). So the devnet exists before the vault and survives the deposits and
their finality waits as a saved generation on the Volume (as lez/lab does: a restart after 54 min worked).

## Identity of the configuration (merged wire, `lez/bridge-wire` `BridgeConfig::id`)

configId = sha256("ferrata/config/v1" || bedrockGenesisId || channelId || bridgeHeader || bridgeImageId ||
evmChainId (u256) || vault || attestationRootId || salt), 261 bytes, where
- bridgeHeader `0x8face982…e68d`, attestation program account `0x86ce4d7f…a04c` (derived keys of the labels
  `program/bridge/header`, `program/attest/header`; independent of the images);
- attestationRootId = sha256("ferrata-lez/v1/bridge/attestation-root" || attestation account || instance id);
  instance id `0x6d65e4ef…f6b4` (sepolia_config(2), bootstrap period 1378) unless the crown build says otherwise;
- salt = sha256("ferrata-lez/v1/bridge/controls" || borsh(controls)); controls = LEZ admin, freeze authority,
  fee recipient (throwaway test accounts), floor and caps (unlimited);
- bedrockGenesisId and channelId: the devnet's real values (stage 2), not the inbound plan's tags;
- vault: CREATE(deployer, N+2), computed at stage 0 from the deployer's nonce and checked after stage 3.
The vault takes configId as `lezConfigId`; the LEZ Init proves slot 2 (`storedLezConfigId`) at a finalized block.

## The deployer, its nonces and the money

Same throwaway deployer as sepolia-live-2 (`0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07`, keystore in the Modal Secret
`ferrata-sepolia-key`); after the owner's run it is at nonce 17. RISC Zero's Groth16 verifier of that run
(`0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0`, nonce 1, SELECTOR 0x73c457ba, read 10 Nov 23:43Z) is reused
(FERRATA_GROTH16_VERIFIER). With N = 17: stop `0xF1eD1125D6E4Bc1ecBE39792CE1EC14DeF986942`, library
`0x86D919BBC31D3BEE94c2E1D078b3bC3c2b91C091`, vault `0x57b437F0BE9f312f0a3bbc784C2F12a831aEce6E`, then nonces 20-29
(13 transactions). The run refuses to start unless latest = pending nonce and the CREATE addresses have no code.
Ether: 2 x 1,002,000,000,000,000 wei of deposits, returned by the releases less the checkpoint fees; gas ~100 M at
Sepolia's price (~73 wei on 10 Nov: ~0.00001 ETH). Modal (ws-3, cap 20 USD; ~2.4 spent before this work):
runner ~4 h of 2 cores (~1 USD at the nonpreemptible rate), GPU session ~1 h (~1 USD), wraps cents.

## Built and measured so far (build box = Modal, workspace ws-4, through `mrun`)

| What | Result |
|---|---|
| Guests at this path | attestation program `ferrata_lez.bin` image `0x2228bb46…67cf` (602,904 B); merged bridge `ferrata_bridge.bin` image `0x0be48090…25df` (507,088 B); `checkpoint_b` `0x7e51065f…7754`; release `0x20595dc5…2625` |
| Fresh devnet (LEZ v0.4.0-rc1 sequencer + Logos Blockchain node, dev-mode leader proofs) | deployed both programs in 14 blocks, all Finalized on Bedrock, 147 s; channel key, tip, deployment settings readable (the vault's identity inputs) |
| `identity.py` | genesisState reproduces the real-blocks proof's parent state; commitments reproduce sepolia-live-2's |
| Sepolia stages on a fork (stand-in identity, accept-all verifier on the fork only) | `deploy.sh` mode B, 2 deposits with fees, submitCandidate, finalizeAttested after 3,601 s, 2 releases: `e2e/runs/crown-fork-3r/README.md` |

## What remains (in order)

1. A crown driver on the merged wire (`lez/live/crown/`: the live driver's wire-free modules by `#[path]`, a new
   `crown.rs`): session A (deploy, instance Init, period updates of the repository fixture) and session B (the
   captures' updates and finality updates, bridge Init, Register ETH, ImportRoot n=1, n=2, Claim, ClaimPrivately,
   Burn public, Burn private), porting `lez/harness/src/bridge.rs`'s builders to the live sequencer.
2. Session save/restore between A and B (lez/lab/session.sh's generations on a Volume).
3. Attestor + `checkpoint_b` input from the session's own blocks (checkpoint/realblocks with the run's configuration
   instead of labels and chain id 31337).
4. The GPU session: LEZ proves through r0vm (risc0-zkvm `client` only), so r0vm 3.0.5 with CUDA as
   RISC0_SERVER_PATH makes both private proofs GPU proofs against the live sequencer.
5. Release inputs from real burns (bridge/e2e `release` uses the v1 burn MAC; the merged notes use v2).
6. The capture with the ETH registry and fee slots (register_slots) beside slots 0-2.
7. The cloud app (`ferrata-crown` on ws-3), `e2e/crown.sh`, the verify script, one full rehearsal to DONE.

## Commands (for the owner)

The cloud app (`e2e/crown/cloud/`, Modal app `ferrata-crown` on workspace ws-3) runs the whole round trip from
one `start`; it needs this machine only to start, to look and to fetch. Rehearsed to DONE on an anvil fork of Sepolia
on 11 Nov 2026 (`e2e/runs/crown-rehearsal-1/README.md`: 19 stages in ~29 min, verify 14 of 14, about 0.6 USD of
Modal, estimated from the container times).

Before the first start:
- a tree with all three parts: crown-cloud, crown-lez and crown-proofs merged (the rehearsal ran from branch
  `crown-rehearsal` = crown-cloud + crown-lez 5815c6b + crown-proofs 4f2f0be); `vault/lib` present in it (forge-std
  1.10.0, OpenZeppelin 5.4.0, risc0-ethereum 3.0.0, as in the other worktrees);
- the Secret `ferrata-sepolia-key` on ws-3 (it exists: `e2e/sepolia-cloud.sh setup` made it; the crown run
  signs with the same throwaway deployer);
- the LEZ bundle (`lez/crown/build.sh seed` and `all` on the build box) and the R0BF programs
  (`spike/tmp/crown-proofs/programs/`, ids checked against the crown build's);
- no ferrata-sepolia runner active (`e2e/sepolia-cloud.sh status`): both runs use the deployer's nonces, and `start`
  refuses while one is active.

From that tree (`e2e/sepolia-live.env` is read for SEPOLIA_RPC_URL and ETH_FROM, used read-only by `start`):

```
e2e/crown.sh setup --bundle <bundle.tar.zst>    # deploy ferrata-crown, upload the bundle and the programs (~1 min)
e2e/crown.sh start --fork crown-rehearsal-2     # optional: one more rehearsal on a fork (~30 min, nothing sent)
e2e/crown.sh start                              # the live run crown-live-1: prints sender, nonce, vault address,
                                                #   the 13 transactions, the gas price; type yes
e2e/crown.sh status                             # any time: the stage, the stages done, the last lines of the log
e2e/crown.sh fetch                              # e2e/runs/crown-live-1/ (identity, transactions, captures, the LEZ
                                                #   sessions' records, checkpoint, releases, verify.txt)
e2e/crown.sh verify                             # the chain checks again from this machine (read-only)
e2e/crown.sh stop                               # cancels the runner (and a queued GPU proof); `start` resumes
```

A stop, a timeout or a crash anywhere is resumed by `start` with the same run name: every stage asks the chain or the
Volume first (deploy.sh resumes from its record; a deposit, submit, finalize or release already on chain is not sent
again; a LEZ session whose generation is saved is not run again, and once the vault's deployment has begun no new
devnet can be made). `--halt-after <stage>` stops cleanly after a stage.

Expected duration of the live run (derived: the rehearsal's measured stage times plus Sepolia's own clock): build ~2
min; session A ~5-10 min (the real Sepolia instance: bootstrap and 32 period updates); deployment ~3 min (7
transactions, `--slow`); each deposit then waits for its block's finality (~13-20 min) and a light-client update that
covers it (up to ~7 min more): ~40 min for both; session B ~14 min (827 s in the rehearsal: two private proofs of
266 s and 323 s on 16-32 cores); the checkpoint ~5 min (GPU queue permitting: up to 2 h, then the CPU here); W = 60 min
(the release proofs, ~3 min, run in it); finalize, the 120 s release delay and both releases ~4 min; verify ~1 min.
About **2 h 15 min to 2 h 45 min** in all, longer if no GPU is free. Modal: about 1-2 USD on ws-3 (estimated).
Sepolia: 13 transactions, ~50 M gas (the vault ~39.5 M under EIP-8037), 0.002004 ETH escrowed by the deposits and
paid back by the releases less the fees.

Not exercised by the fork rehearsal (exercised elsewhere, or simple): the live captures with their finality waits
(`e2e/crown/capture.py`, tested read-only against Sepolia: block 12,108,569, 6 slots proven), signing with the
keystore in the cloud (as ferrata-sepolia's runner does), LEZ's real Sepolia instance (crown-lez's rehearsal), the
node's gas pricing on Sepolia (deploy.sh), a real hour of W.
