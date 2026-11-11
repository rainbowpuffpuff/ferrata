# The crown run's LEZ side

The attestation program and the merged bridge program on a LEZ v0.4.0-rc1 devnet (sequencer + Logos Blockchain
node, LEZ's local configuration, dev-mode leader proofs), in two sessions with the devnet saved between them as
generations on a Volume (as `lez/lab`). Design: `e2e/crown/README.md` (stages 1 and 6).

| File | What |
|---|---|
| `driver/` | `crown-driver`: the live driver's modules (`../live/driver/src`, by path) on the merged bridge wire (`../bridge-wire`) |
| `session.sh` | one session, `a` or `b`: restore, start the node and the sequencer, run the driver, stop cleanly, save a generation |
| `build.sh` | on the build box: the driver, and `bundle.tar.zst` (binaries, guests, LEZ's configuration, the Sepolia fixture, `session.sh`) |
| `capture-anvil.py` | rehearsals only: a capture's execution half from a local anvil fork |
| `rehearse.sh` | rehearsals only: sessions A and B on the box against an anvil fork of Sepolia and a stand-in beacon committee |

## Session A (`session.sh a`)
A new zone: `sequencer_service setup --create-channel` (a random channel key), the node, the sequencer; the driver
deploys the attestation program (`program/attest/header`) and the bridge (`program/bridge/header`), initialises the
Sepolia instance from the period-1378 bootstrap of `crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10` and
applies its updates 1378..=1409, and writes `session-a.json` at the last block final on Bedrock (T0): the channel's
key, posting timeframe and config tip hash (the node's `/channel/0101..01`), `bedrockGenesisId` = SHA-256 of the
deployment settings, the programs' accounts and image ids, the instance, the controls and their salt. The vault's
`lezConfigId` is `crown-driver config-id --identity session-a.json --vault ADDR`; its `genesisState` is
`e2e/crown/identity.py genesis-state` over T0 and the channel.

## Session B (`session.sh b`)
Restores A's generation; the captures' period updates and finality updates; then, under the first capture's record:
bridge `Init` (the vault's slot 2 holds configId), `Register` ETH (registry entry and schedules 0 and 1 against the
vault's SHA-256 slots), `ImportRoot` n=1, `Claim` of deposit 0 (public, LEZ public test account 0); under the
second's: `ImportFees` (schedule 1 absent at n=2), `ImportRoot` n=2, `ClaimPrivately` of deposit 1 (private test
account 0, proven by LEZ's wallet), `Burn` of 0.001 ETH from public account 0 (note 0) and from private account 0
(note 1, proven). Writes `session-b.json` (T1, the block range T0+1..=T1, every step) and `burn-notes.json`.

Private transactions are proven by LEZ's wallet inside the session (r0vm as LEZ's prover, on the session's CPU;
`CROWN_R0VM` may name a CUDA r0vm, and the cloud runner sets none). Since the time fix a claim carries no time (the
program reads attested Ethereum time), so a proof has no deadline from the claim; `CROWN_CLAIM_TIME_OFFSET_S` is
still parsed by the driver and has no effect.

## Interface
`spike/tmp/crown/interfaces/lez.md` (beside the repository) fixes the environment, the Volume layout and the files.

## Rehearsed (results/README.md)
On the build box: the whole of A and B against a STAND-IN (an anvil fork of Sepolia with the real v5 vault, a beacon
committee held by the driver), with the zone saved after A and restored for B (every step applied, both private
proofs real: 274 s and 321 s on the box's CPU; peak RSS 10.4 GiB, r0vm 9.2 GiB); and session A on the real Sepolia
path (176 transactions, 29.5 min). To repeat on the box (from the worktree):
`../modal-build/mrun --profile ws-4 --timeout 7200 --put ../tmp/crown/build/ids-methods-hex.txt -- 'bash lez/crown/build.sh seed && bash lez/crown/build.sh all && bash lez/crown/rehearse.sh all'`.
