# The crown run: the whole round trip on the final programs (design)

Status: DESIGN (11 Nov 2026). Nothing here has run yet; `spike/tmp/crown/STATE.md` (outside the repository) tracks
what is built. One cloud run, started once, that crosses Sepolia -> LEZ -> Sepolia with the final programs and real
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

## Commands (for the owner, once built and rehearsed)

See the end of this file once piece 4 is done: `e2e/crown.sh setup | start [--fork] | status | fetch | stop`.
