# The RISC Zero 3.1 deployment rehearsed on a Sepolia fork, with real 3.1 proofs (23 November 2026)

`vault/script/fork-rehearsal-31.sh` on branch `bridge-31` (the Modal build box; anvil forked from a public Sepolia
endpoint, five blocks behind its head, at block 12,195,997, 2026-11-23 13:30:12 UTC; first run at block 12,194,913,
before the review's changes: the same results but the order of the stop's renouncement). No key was used and nothing was
sent to Sepolia: every transaction stayed on the fork, from impersonated accounts. Result: **FORK REHEARSAL 31 OK**
([`transcript.txt`](transcript.txt)).

## What it shows

1. **The deployment the owner runs** (`script/deploy.sh`, as `script/deploy_sepolia.py` runs it on Sepolia): rule (B),
   gas rule `eip8037`, capped defaults, decision D1 (a) of the migration plan: our own `RiscZeroGroth16Verifier`
   **3.1.0** (`VERSION()` "3.1.0", `SELECTOR()` `0xe71e6620`, checked by `DeployVault.s.sol` before the stop and
   after the last step) behind RISC Zero's `RiscZeroVerifierEmergencyStop` with its owner renounced, a freeze authority
   and a watcher that are not the deployer (fork-only addresses here), and `FERRATA_EXPECT_VAULT`: the vault landed on
   `0x169E05aE832232B7B177b717D718bccaC12F65b1`, the address the zone's configuration names; the stop's owner renounced
   before the token registrations (no deposit can reach a vault whose stop a key can still trip). Eight transactions in 7 s
   ([`deploy.txt`](deploy.txt), [`deployment.json`](deployment.json), [`params.json`](params.json); the network
   journal of `scripts/pubnet.sh` in [`deploy/net/`](deploy/net/)).
2. **The round trip on it with real proofs** (`script/Rehearsal31.s.sol`): the zone session recorded in
   `e2e/runs/zone-bridge-rc3` (one LEZ v0.4.0-rc3 sequencer on Logos Blockchain 0.3.5-rc.1: the bridge program's
   deployment, both deposits imported, a public and a private claim, a public and a private burn, its rule (B)
   checkpoint), whose deployer has nonce 0 on Sepolia, so its configuration and proofs bind this vault. Its checkpoint
   and both releases were **proven again under RISC Zero 3.1** from the run's recorded inputs (below); the LEZ side's
   own transactions (deposit imports, claims, burns) are the session's as recorded on LEZ (risc0 3.0.5, LEZ's version
   until LEZ is tagged on 3.1), not executed again.

| Step | Sender | Result | Gas |
|---|---|---|---:|
| the journal names this chain, this vault, its configuration and its genesis | anyone | checked | (a call) |
| the session's two deposits, 0.001 ETH each plus the relay fee | depositor | 2 leaves | 226,342; 172,140 |
| the session's checkpoint with the seal its run made under risc0 3.0.5 | relayer | refused: `SelectorMismatch(0x73c457ba, 0xe71e6620)` | (a call) |
| the same checkpoint with its RISC Zero 3.1 seal | relayer | candidate | 375,305 |
| after Δ = 3,600 s: `finalizeAttested` | relayer | final, burn root accepted | 218,385 |
| D1 (a): the freeze authority freezes releases; a release | freeze authority; relayer | refused: `ReleasesAreFrozen` | 47,347 |
| releases unfrozen | freeze authority | | 25,374 |
| both releases with their RISC Zero 3.1 seals (after the 120 s release delay) | relayer | paid 0.000999 ETH each to the recipient | 421,366; 382,118 (first run: 384,630) |
| the first release again | relayer | refused: `NullifierAlreadyUsed` | (a call) |
| **the advisory's forgery stood in for**: one more deposit, then the verifier's code replaced on the fork by one `STOP` opcode, which accepts every seal (as `docs/security/risc0-groth16-advisory-2026-11-21/fork-rehearsal.sh` does) | depositor | | 160,064 |
| D1 (a): a watcher pauses checkpoints; a forged checkpoint (a state and burn root nobody attested) | watcher; relayer | refused: `CheckpointsAreFrozen` | 47,260; (a call) |
| D1 (a): the freeze authority freezes releases; a forged release (a burn nobody made) | freeze authority; relayer | refused: `ReleasesAreFrozen` | 47,347; (a call) |
| the freeze lifted: the same forged release | freeze authority; relayer | **paid** 0.000999 ETH: what the controls are for | 25,374; 163,122 |

Gas is anvil's (a fork does not price Sepolia's EIP-8037 state creation); the deployment's limits on Sepolia are the
node's own estimates times 1.25 (`script/deploy.sh`).

## The proofs and their timings (measured)

[`proofs/`](proofs/): one record per seal (`scripts/reprove_fixtures.py`, `vault/test/real/reprove-31.json`).

| Seal | Program (RISC Zero 3.1) | User cycles | Segments | STARK (CPU) | identity_p254 | Groth16 (prover image v2026-10-20.1) |
|---|---|---:|---:|---:|---:|---:|
| checkpoint `zone-24-125` | `checkpoint_b_0_3_5` `0x8917f58e…af04` | 18,971,326 | 21 | 1,522.8 s | 20.5 s | 15 s |
| release `public-0` | `release` `0xf279af41…a6fd` | 77,736 | 1 | 14.2 s | 20.6 s | 17 s |
| release `private-1` | `release` | 77,736 | 1 | 14.6 s | 21.2 s | 15 s |

The STARKs and `identity_p254` ran on the Modal build box's CPU (`ferrata-e2e prove` and `p254`, r0vm 3.1.0, 32
threads; the box: 16 physical cores, 32 threads); the Groth16 step in RISC Zero's prover image as a Modal CPU
function. Every Groth16 receipt was verified against its program's id and the journal the run recorded before its seal
was used. Selector of every seal: `0xe71e6620`.

## Run it again

`cd vault && script/fork-rehearsal-31.sh [LOG DIR]` (Foundry 1.3.6; `FERRATA_FORK_URL` a public Sepolia endpoint,
default publicnode). It refuses unless `test/real/fixtures/zone-bridge-rc3.json` holds RISC Zero 3.1 seals and the
session's deployer still has nonce 0 on Sepolia.
