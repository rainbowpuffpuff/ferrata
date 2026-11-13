# zone-bridge-2: the same with a rotation its committee proposed (four sequencers, then five)

13 November 2026, the Modal build box (workspace ws-4): `checkpoint/attestor/zone-bridge.sh` (stages in its
header); results: [`checkpoint/attestor/RESULTS.md`](../../../checkpoint/attestor/RESULTS.md) §9 (§8 for what is real and what stands in). Run with `ZB_ROTATE=1`: after the attestors started and before session B, a fifth key staked on LEZ (`sequencer_service stake`), the committee's `ChannelConfigOp` applied by Bedrock (`zone/channel-before-rotation.json`, `channel-after-rotation.json`), the fifth sequencer and its attestor started.
Nothing ran on a public network: the Ethereum side is a local anvil (no fork, Sepolia's chain id), the beacon
chain on LEZ crown-driver's stand-in committee.

| Path | What |
|---|---|
| `logs/zone-bridge.log` | the run's own log, every stage (the sequencers' phases, session A, the anvil, the attestors, the stake and the rotation, session B, the final tier, the proofs, the vault) |
| `zone/` | the zone: `genesis.json` (upstream's devnet genesis plus `local.json`'s two funded accounts), `phases.txt`, `deployment.json` (what the attestors serve), `attestor-<i>/` (certificate, every round, the final attestation), `node-answers.tar.zst` (the node's answers the checkpoint is built from: attestor 0's final round), `logs/` (sequencers, node, driver, anvil, poller) |
| `lez/` | session A (`session-a.json`: T0, the channel's four keys, the programs, the instance) and B (`session-b.json`: every step and its block; `burn-notes.json`; `driver-export.tar.zst`, the blocks as seq-0 served them), the two captures of the anvil's blocks |
| `identity.json`, `params.json`, `crown.json`, `deploy.json`, `manifest.json` | the vault's identity and deployment parameters (nMin 4, tMin 3), the deposits, the candidate and the releases, DeployVault's record, every anvil transaction with its gas |
| `checkpoint/` | `zone-28-119.input` and `.json` (4 of 5 attestations, the rotation, the native journal), the 5-of-5 and 3-of-5 metas, the executor runs, `burns.json`, `attestor.json`, `blocks.tsv`, `inscriptions.jsonl`, `prove.json`, `receipts/` (succinct and Groth16 receipts, `seal.hex`, `journal.hex`) |
| `releases/` | both burns' release inputs, metas, executor and proof records, receipts |
| `vault-checks.json` | the vault after the run: `finalState`, the accepted burn root, both releases paid |

Re-check: `bash checkpoint/attestor/zone-bridge.sh verify e2e/runs/zone-bridge-2` on the build box (after `build`;
`ZB_PROGRAMS` = the R0BF programs of [`docs/image-ids.md`](../../../docs/image-ids.md)), and
`cd vault && forge test --match-contract ZoneBridgeRotationTest` (fixture `vault/test/real/fixtures/zone-bridge-rotation.json`).
