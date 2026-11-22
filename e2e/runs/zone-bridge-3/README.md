# zone-bridge-3: zone-bridge-1's run again, with the attestors checking every tip against their own LEZ node

22 November 2026, the Modal build box: `checkpoint/attestor/zone-bridge.sh` (stages in its header) at the branch
`fix-attestor` (the review fixes M1 and H-2: each attestor signs a tip only when its own sequencer's LEZ node serves
it `Finalized` and byte for byte as inscribed, and the attestors' last round is taken while the zone runs, every
attestor on one tip at or above session B's T1). Results, what is real and what stands in:
[`checkpoint/attestor/RESULTS.md`](../../../checkpoint/attestor/RESULTS.md) §10.1 (and §8 for zone-bridge-1).
Nothing ran on a public network: the Ethereum side is a local anvil (no fork, Sepolia's chain id), the beacon
chain on LEZ crown-driver's stand-in committee.

The box was preempted during the first attempt (its zone stage had reached T1 = 111 and the four attestors had
signed it); Modal ran the call again in a new container, and these are that second run's records.

| Path | What |
|---|---|
| `logs/zone-bridge.log` | the run's own log, every stage (the four phases, session A, the anvil, the attestors, session B, the final round, the proofs, the vault) |
| `zone/` | the zone: `genesis.json`, `phases.txt`, `deployment.json` (what the attestors serve), `attestor-<i>/` (certificate, every round in `rounds.tsv`, the tips signed, the final attestation), `node-answers.tar.zst` (the node's answers the checkpoint is built from, read from the first slot the attestors read to the node's LIB after the sequencers stopped), `logs/` |
| `lez/` | session A (`session-a.json`: T0 = 29, the channel's four keys) and B (`session-b.json`: every step and its block, T1 = 96; `burn-notes.json`; `driver-export.tar.zst`), the two captures of the anvil's blocks |
| `net/` | the network journal of `e2e/crown/sepolia.sh`'s proxy in front of the local anvil (every signed transaction before it left) |
| `identity.json`, `params.json`, `crown.json`, `deploy.json`, `manifest.json` | the vault's identity and deployment parameters (nMin 4, tMin 3), the deposits, the candidate and the releases, DeployVault's record, every anvil transaction with its gas |
| `checkpoint/` | `zone-29-96.input` and `.json` (3 of 4 attestations, the native journal), the 4-of-4 and 2-of-4 metas, the executor runs, `burns.json`, `attestor.json`, `blocks.tsv`, `inscriptions.jsonl`, `prove.json`, `receipts/` (succinct and Groth16 receipts, `seal.hex`, `journal.hex`) |
| `releases/` | both burns' release inputs, metas, executor and proof records, receipts |
| `vault-checks.json` | the vault after the run: `finalState`, the accepted burn root, both releases paid |

Re-check: `bash checkpoint/attestor/zone-bridge.sh verify e2e/runs/zone-bridge-3` on the build box (after `build`;
`ZB_PROGRAMS` = the R0BF programs of [`docs/image-ids.md`](../../../docs/image-ids.md)), and
`cd vault && forge test --match-contract ZoneBridgeLezCheckedTest` (fixture `vault/test/real/fixtures/zone-bridge-3.json`).
