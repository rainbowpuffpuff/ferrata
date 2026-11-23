# Stream bridge-31: proposed changes for the integrator

Branch `bridge-31` (from `3d984678`): phase 1 of the move to RISC Zero 3.1 ([migration plan](../design/risc0-3.1-migration.md)),
the Ethereum side, which needs nothing from Logos. This file proposes the rows it moves; it edits none of
`docs/EVIDENCE.md`, `docs/requirements-status.md`, `docs/traceability.md` or the README's status lines.

What the branch does, with its commits:

- **Guests on risc0-zkvm 3.1.0** (`fce77353`): `bridge/` and `checkpoint/` (hosts and the four guest workspaces) on
  risc0-zkvm 3.1.0, risc0-build 3.0.7, risc0-groth16 3.0.6 (circuits 4.1.0, risc0-zkp 3.0.6: none yanked, the versions
  GHSA-g6qg-9j8p-7j87 names as fixed); `checkpoint/fidelity` a workspace of its own (LEZ v0.4.0-rc1's `lee` pins
  risc0-zkos-v1compat =2.2.2, risc0-zkvm 3.1.0 needs ^2.2.3; it runs checkpoint-core natively, no proof).
  New ids: [`docs/image-ids.md`](../image-ids.md), "RISC Zero 3.1" (`*_31` rows; `a992ccb1`).
- **Verifier 3.1.0** (`7762f2ec`, `98d2e9f1`): risc0-ethereum v3.1.0 (commit `ff30da77`, `vault/lib.lock`);
  `DeployVault.s.sol` refuses a Groth16 verifier whose `VERSION()`/`SELECTOR()` is not 3.1.0's and, with
  `FERRATA_EXPECT_VAULT`, a vault creation that would not land where the LEZ configuration names;
  `vault/test/Verifier31.t.sol`.
- **Seals made again under 3.1** (`d34c75c2`, `8008bec0`, `402cdf54`): 22 seals of 9 fixtures from their runs' recorded
  inputs (`scripts/reprove_fixtures.py`, `vault/test/real/reprove-31.json`,
  [`vault/test/real/README.md`](../../vault/test/real/README.md)); STARKs on the build box's CPU (r0vm 3.1.0) or, for
  three checkpoints, on an NVIDIA L4 (`gpu-prover` built for 3.1: 15 segments in 44.8 s, 20 in 68.1 s, 29 in 88.9 s);
  RISC Zero's prover image `v2026-10-20.1` for the Groth16 step (11-18 s each). `vault` `forge test` at `67122945`:
  **589 passed, 0 failed, 12 skipped** (601; 67 suites; 11 of the skips are the 3.0.5 record suites, with their
  reasons); `test/real/*`: 47 passed, 11 skipped. The ids rebuilt at another path from a fresh target directory: 9 of 9.
- **The prover service's path on a GPU** (`67122945`): `gpu-prover` `prove_bytes`, STARK and Groth16 on one L4 (a
  release: 1.2 s + 11.1 s); its seal verifies on verifier 3.1.0 (`test_aSealMadeOnTheGpuVerifies`).
- **Decision D1 (a)** of the migration plan, as recommended: our own verifier 3.1.0 behind RISC Zero's emergency stop
  (owner renounced), a freeze authority and at least one watcher with keys, capped defaults; no RISC Zero router.
- **The fork rehearsal** with real 3.1 proofs: [`vault/results/fork-rehearsal-31/`](../../vault/results/fork-rehearsal-31/README.md).
- **The owner's one-command deploy**: `vault/script/deploy_sepolia.py` (Modal, the Secret `ferrata-sepolia-key`;
  `--dry` first: checks, then the whole deployment on a Sepolia fork). Tested only with `--dry`, without the Secret.

## Requirement rows (proposed)

All four stay **in part**: the clauses are about both sides, and the LEZ side waits on Logos (a LEZ tag on risc0 3.1:
LEZ v0.4.0-rc1, rc3 and the public testnet's v0.3.1 verify private transactions with risc0-zkvm 3.0.5). Each row's
text changes to: "Ethereum side shown under RISC Zero 3.1 (phase 1); LEZ side waits on Logos (phase 2)".

| Row | Now | Proposed | The Ethereum half, as the clause words it, and what shows it |
|---|---|---|---|
| RFP-022 R.8 "Use mature, audited proof systems, not new ZK primitives." | in part | in part (Ethereum half shown) | The vault verifies with RISC Zero's own, unmodified `RiscZeroGroth16Verifier` **3.1.0** (risc0-ethereum v3.1.0, `ff30da77`; the fixed release of GHSA-mm6c-35cm-g4x2) behind RISC Zero's `RiscZeroVerifierEmergencyStop`; every proof it takes is made by risc0-zkvm 3.1.0 with RISC Zero's prover image `v2026-10-20.1` (the fixed release of GHSA-g6qg-9j8p-7j87); no new ZK primitive. `vault/test/Verifier31.t.sol` (the library is 3.1.0: control root, BN254 control id, version, selector `0xe71e6620`); every real-proof suite on 3.1 seals; `DeployVault.s.sol` refuses another verifier. LEZ half: LEZ's private transactions are verified with risc0 3.0.5 (Logos). |
| RFP-022 Sec.2 "Both sides reject invalid proofs: wrong inputs, wrong state, tampering, replay." | in part | in part (Ethereum half shown) | Under verifier 3.1.0 with real 3.1 seals: a flipped seal byte refused (`test_rc3_aFlippedSealByteIsRefused`, `test_ruleBMulti_aFlippedByteInTheSealIsRefused`, `test_zoneBridge_aFlippedSealByteOrFewerSignaturesAreRefused`); a proof for another program refused (`test_rc3_aVaultPinningTheDeployedCheckpointBRefusesThisCheckpoint`, `test_ruleB_realProof_refusedByARuleAVault`); another state or committee refused (`test_ruleBMulti_aJournalBelowTheFloorIsRefused`, `…anotherCommitteeUnderTheRealSealIsRefused`, `test_zoneBridgeRotation_aVaultWhoseGenesisIsTheNewSetRefusesIt`); another journal or another proof refused (`Verifier31Test`); replays refused (`test_rc3_checkpointThenBothReleases`, `test_rc3_aRedirectedRepricedOrReplayedReleaseIsRefused`); **a seal of risc0 3.0.5 refused** (`SelectorMismatch(0x73c457ba, 0xe71e6620)`: `test_aSealOfRisc0_3_0_5IsRefused`, and on the Sepolia fork, row below). LEZ half: as now (Logos). |
| RFP-022 Sec.3 "Submitters can't redirect, inflate fees or replay; destinations secretly committed." | in part | in part (Ethereum half shown) | Under verifier 3.1.0 with the release's real 3.1 seal: the recipient, the relayer, the relay fee and the nullifier each changed alone are refused by the verifier, the request as proven is paid once and its replay refused (`ZoneBridgeRc3Test.test_rc3_aRedirectedRepricedOrReplayedReleaseIsRefused`, on both rc3 fixtures); the same on the Sepolia fork for the replay (row below). Destinations committed as before (`C_out`, bridge core). LEZ half: the redirected claim's refusal rests on LEZ's risc0 3.0.5 (Logos). |
| RFP-022 Sec.6 "Wrapped supply never exceeds vault holdings; invalid mints and releases fail." | in part | in part (Ethereum half shown) | "Invalid releases fail" now rests on a verifier without the advisory's forgery (3.1.0): every release suite pays only the proven request (above), and the vault's invariants are unchanged (`vault/test/invariant`, 256 runs × 100 calls). Invalid mints rest on LEZ's check of private-transaction receipts (risc0 3.0.5; `docs/trust.md` §5): Logos. |

## New EVIDENCE row (the next free number; 229 at `3d984678`)

| N | **RISC Zero 3.1, phase 1 (the Ethereum side): new programs, verifier 3.1.0, every vault seal made again, a Sepolia-fork rehearsal with real 3.1 proofs (RFP-022 R.8, Sec.2, Sec.3, Sec.6: Ethereum half)**: the guests the vault verifies built with risc0-zkvm 3.1.0 (`docs/image-ids.md` `*_31`: release `0xf279af41…a6fd`, checkpoint_b `0xdafa6ab9…cda0`, checkpoint_b_0_3_5 `0x8917f58e…af04`, checkpoint `0x7c93425e…97c1`); the vault on RISC Zero's `RiscZeroGroth16Verifier` 3.1.0 (risc0-ethereum v3.1.0, `ff30da77`; selector `0xe71e6620`); the real-proof fixtures proven again from their recorded inputs (STARK on the build box's CPU through r0vm 3.1.0, identity_p254, RISC Zero's prover image `v2026-10-20.1`; each Groth16 receipt verified against the run's journal); a seal of risc0 3.0.5 refused (`SelectorMismatch`); on an anvil fork of Sepolia (block 12,194,913) `script/deploy.sh` with D1 (a), then the recorded LEZ v0.4.0-rc3 zone session's deposits, 3.1 checkpoint final after the hour, a freeze refusing a release, both 3.1 releases paid, a replay refused (FORK REHEARSAL 31 OK) | `vault/results/fork-rehearsal-31/`; `vault/test/real/README.md`; `vault/test/Verifier31.t.sol`; `forge test` 589 passed, 0 failed, 12 skipped (`67122945`); `docs/image-ids.md` "RISC Zero 3.1"; branch `bridge-31` |

## Owner steps (not this branch's)

- Deploy the new vault on Sepolia: `MODAL_PROFILE=<the Secret's workspace> modal run vault/script/deploy_sepolia.py
  --params <the LEZ side's params> --freeze-authority <key> --watchers <key> --expect-vault <address> --dry`, then the
  same without `--dry`. Two inputs are the owner's: the roles' addresses (a freeze authority and watchers with keys,
  not the deployer) and the zone the vault serves (its Logos Blockchain layout decides `checkpointImageIdB`).
- The four November vaults stay capped at 0 and are retired as the security note says.
