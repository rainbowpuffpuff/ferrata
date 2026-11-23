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
  RISC Zero's prover image `v2026-10-20.1` for the Groth16 step (11-18 s each). All of them are **rule (B)** checkpoints
  and releases. `vault` `forge test` at `c3d9a0ec`: **596 passed, 0 failed, 12 skipped** (608; 68 suites).
  **47 of the 98 real-proof cases are skipped**: 11 suites (forge counts each as one skipped test), whose fixtures are
  records of earlier programs under RISC Zero 3.0.5 that this tree's guests cannot prove again (an input not recorded,
  or recorded for an earlier program); **every rule (A) case is among them** (the return leg). `test/real/*`: 51
  passed, 11 skipped. (Commit `402cdf54`'s message, "every real-proof fixture now holds RISC Zero 3.1 seals", is wrong:
  the 6 record fixtures still hold 13 seals of 3.0.5.) The ids rebuilt at another path from a fresh target directory:
  9 of 9.
- **The prover service's path on a GPU** (`67122945`): `gpu-prover` `prove_bytes`, STARK and Groth16 on one L4 (a
  release: 1.2 s + 11.1 s); its seal verifies on verifier 3.1.0 (`test_aSealMadeOnTheGpuVerifies`).
- **Decision D1 (a)** of the migration plan, as recommended: our own verifier 3.1.0 behind RISC Zero's emergency stop
  (owner renounced), a freeze authority and at least one watcher with keys, capped defaults; no RISC Zero router.
- **The fork rehearsal** with real 3.1 proofs: [`vault/results/fork-rehearsal-31/`](../../vault/results/fork-rehearsal-31/README.md).
- **The owner's one-command deploy**: `vault/script/deploy_sepolia.py` (Modal, the Secret `ferrata-sepolia-key`) and its
  logic `vault/script/deploy_core.py`: refusals before anything is sent (roles, Δ, programs, address, and a LEZ
  configuration whose hash is not `lezConfigId` or that names another vault or chain), a preflight of
  the whole deployment on a fork, the stop renounced before any registration, every value read back.
  `scripts/test_deploy_sepolia.py` runs it end to end, signed path included, on a local anvil with throwaway keys (6 of
  6); its Modal wrapper ran `--dry` on a Sepolia fork without the Secret. Never run with the Secret or on Sepolia.
- **A review of the branch** (23 November): four Mediums and ten Lows, fixed in `4a8d53ed` to `c3d9a0ec`:
  the tool's crash after a signed run and its refusals (M1, M2), the evidence map (M3), R.8's wording (M4), and the
  Lows (`check-verifier.sh` reads the caps of a 3.0.0 vault, the record kept on the Volume, an allow-listed
  environment, the read-back, pinned Foundry and prover image, stale lines, counts, the stop renounced first, the
  verifier checked by its code, cargo-deny, the fork rehearsal's forgery case).

## Requirement rows (proposed)

All four stay **in part**: the clauses are about both sides, and the LEZ side waits on Logos (a LEZ tag on risc0 3.1:
LEZ v0.4.0-rc1, rc3 and the public testnet's v0.3.1 verify private transactions with risc0-zkvm 3.0.5). On the Ethereum
side the evidence under RISC Zero 3.1 is **rule (B)** only: no rule (A) proof was made under 3.1 (the deploy tool
deploys rule (B)). Sec.2, Sec.3 and Sec.6's text changes to: "Ethereum side shown for rule (B) under RISC Zero 3.1
(phase 1); LEZ side waits on Logos (phase 2)"; R.8's as below (whether 3.1.0 is audited is not established).

| Row | Now | Proposed | The Ethereum half, as the clause words it, and what shows it |
|---|---|---|---|
| RFP-022 R.8 "Use mature, audited proof systems, not new ZK primitives." | in part | in part | Ethereum half: RISC Zero's own, unmodified `RiscZeroGroth16Verifier` **3.1.0** (risc0-ethereum v3.1.0, `ff30da77`, installed from upstream's tag and checked by content, `vault/lib.lock`) behind RISC Zero's `RiscZeroVerifierEmergencyStop`; every proof it takes made by risc0-zkvm 3.1.0 and the prover image risc0-groth16 3.0.6 runs (`v2026-10-20.1`): the fix the advisories give (risc0-zkvm 3.1.0 with a new trusted setup, verifier 3.1.0; [security note](../security/risc0-groth16-advisory-2026-11-21/README.md)); no new ZK primitive. **Whether 3.1.0 is covered by an audit is not established**; this proposal cites no audit of it. Shown by `vault/test/Verifier31.t.sol` (the library is 3.1.0; a 3.0.5 seal refused), `vault/test/DeployVaultChecks.t.sol` (no other verifier code behind the stop), the rule (B) real-proof suites. LEZ half: LEZ's private transactions are verified with risc0 3.0.5 (Logos). |
| RFP-022 Sec.2 "Both sides reject invalid proofs: wrong inputs, wrong state, tampering, replay." | in part | in part (Ethereum half shown for rule (B)) | Under verifier 3.1.0 with real 3.1 seals of rule (B): a flipped seal byte refused (`test_rc3_aFlippedSealByteIsRefused`, `test_ruleBMulti_aFlippedByteInTheSealIsRefused`, `test_zoneBridge_aFlippedSealByteOrFewerSignaturesAreRefused`); a proof for another program refused (`test_rc3_aVaultPinningTheDeployedCheckpointBRefusesThisCheckpoint`, `test_ruleB_realProof_refusedByARuleAVault`); another state or committee refused (`…anotherCommitteeUnderTheRealSealIsRefused`, `test_zoneBridgeRotation_aVaultWhoseGenesisIsTheNewSetRefusesIt`; `test_ruleBMulti_aJournalBelowTheFloorIsRefused` is the vault's own check, before the verifier); another journal or another proof refused (`Verifier31Test`); replays refused (`test_rc3_checkpointThenBothReleases`, `test_rc3_aRedirectedRepricedOrReplayedReleaseIsRefused`); a real proof refused once the stop trips (`test_rc3_afterTheStop_aRealProofIsRefused`); **a seal of risc0 3.0.5 refused** (`SelectorMismatch(0x73c457ba, 0xe71e6620)`: `test_aSealOfRisc0_3_0_5IsRefused`, and on the Sepolia fork, row below). Rule (A): no real-seal test under 3.1 (its cases are among the 47 skipped). LEZ half: as now (Logos). |
| RFP-022 Sec.3 "Submitters can't redirect, inflate fees or replay; destinations secretly committed." | in part | in part (Ethereum half shown for rule (B)) | Under verifier 3.1.0 with the release's real 3.1 seal: the recipient, the relayer, the relay fee and the nullifier each changed alone are refused by the verifier, the request as proven is paid once and its replay refused (`ZoneBridgeRc3Test.test_rc3_aRedirectedRepricedOrReplayedReleaseIsRefused`, on both rc3 fixtures); the same on the Sepolia fork for the replay (row below). Destinations committed as before (`C_out`, bridge core). LEZ half: the redirected claim's refusal rests on LEZ's risc0 3.0.5 (Logos). |
| RFP-022 Sec.6 "Wrapped supply never exceeds vault holdings; invalid mints and releases fail." | in part | in part (Ethereum half shown for rule (B)) | "Invalid releases fail" now rests on a verifier without the advisory's forgery (3.1.0): every rule (B) release suite pays only the proven request (above); with a forgery stood in for on the Sepolia fork, a watcher's pause refuses a forged checkpoint and a freeze a forged release (row below); the vault's invariants are unchanged (`vault/test/invariant`, 256 runs × 100 calls). Invalid mints rest on LEZ's check of private-transaction receipts (risc0 3.0.5; `docs/trust.md` §5): Logos. |

## New EVIDENCE row (the next free number; 229 at `3d984678`)

| N | **RISC Zero 3.1, phase 1 (the Ethereum side, rule (B)): new programs, verifier 3.1.0, the rule (B) vault seals made again, a Sepolia-fork rehearsal with real 3.1 proofs (RFP-022 R.8, Sec.2, Sec.3, Sec.6: Ethereum half)**: the guests the vault verifies built with risc0-zkvm 3.1.0 (`docs/image-ids.md` `*_31`: release `0xf279af41…a6fd`, checkpoint_b `0xdafa6ab9…cda0`, checkpoint_b_0_3_5 `0x8917f58e…af04`, checkpoint `0x7c93425e…97c1`); the vault on RISC Zero's `RiscZeroGroth16Verifier` 3.1.0 (risc0-ethereum v3.1.0, `ff30da77`; selector `0xe71e6620`); 22 rule (B) seals of 9 fixtures proven again from their recorded inputs (STARK on the build box's CPU through r0vm 3.1.0 or on an L4, identity_p254, RISC Zero's prover image `v2026-10-20.1`; each Groth16 receipt verified against the run's journal); 11 suites (47 of the 98 real-proof cases, every rule (A) case among them) skipped as records of 3.0.5 that cannot be proven again; a seal of risc0 3.0.5 refused (`SelectorMismatch`); on an anvil fork of Sepolia (block 12,195,997) `script/deploy.sh` with D1 (a), then the recorded LEZ v0.4.0-rc3 zone session's deposits, 3.1 checkpoint final after the hour, a freeze refusing a release, both 3.1 releases paid, a replay refused, and with the verifier's code replaced by STOP (the forgery stood in for) a forged checkpoint refused under a watcher's pause and a forged release under a freeze (FORK REHEARSAL 31 OK); the owner's deploy tool end to end on a local anvil with throwaway keys (6 of 6) | `vault/results/fork-rehearsal-31/`; `vault/test/real/README.md`; `vault/test/Verifier31.t.sol`; `scripts/test_deploy_sepolia.py`; `forge test` 596 passed, 0 failed, 12 skipped (`c3d9a0ec`); `docs/image-ids.md` "RISC Zero 3.1"; branch `bridge-31` |

## Shared files the integrator should also change

- `docs/traceability.md:235` (Sec.2's evidence) still cites the release fixtures of risc0-ethereum v3.0.0
  (`bridge/evm-verify`); the map (`scripts/ci/row-tests.toml`) now cites `vault/test/Verifier31.t.sol` and
  `vault/test/real/ZoneBridgeRc3.t.sol` for R.8 and Sec.2 and `ZoneBridge.t.sol` for F.7 (`b5d80aa0`).
- `bridge/evm-verify` is not moved to verifier 3.1 (the migration plan's phase 1 lists it): its four fixtures
  (`test/fixtures/v2/`) are seals of the v2 programs (release `0xdda76672…`, chain 31337) whose inputs are not in the
  repository, so this tree's guests cannot prove them again, and a 3.1 selector without new seals only makes its tests
  fail. It stays a record of RISC Zero 3.0.5 on its own pin (risc0-ethereum v3.0.0), unchanged by this branch. The map
  no longer cites it for R.8 or Sec.2 (they cite the suites above, which run under verifier 3.1.0); R.2 and Sec.8 still
  cite `bridge/evm-verify/test/v2/VaultV2.t.sol` for the v2 vault's logic: the integrator may want to say there that
  its seals are 3.0.5's.

## Owner steps (not this branch's)

- Deploy the new vault on Sepolia: `MODAL_PROFILE=<the Secret's workspace> modal run vault/script/deploy_sepolia.py
  --params <P.json> --freeze-authority <F> --watchers <W> --expect-vault <V> --dry`, then the same without `--dry`.
  `P.json`: `vault/script/params/sepolia-31.template.json` filled from the zone (`lezConfig`, the bridge's
  configuration naming `V`, and `lezConfigId`, its hash; genesisState, nMin/tMin, the LEZ definitions; its Logos
  Blockchain layout decides `checkpointImageIdB`). `F`, `W`: a freeze authority and one
  to three watchers (comma-separated) with keys, none of them the deployer. `V`: the vault's address, the deployer's
  creation at its next nonce + 3, which the zone's configuration must name (the tool prints and checks it). The
  relayer is not a role of the vault: its address is only funded.
- The four November vaults stay capped at 0 and are retired as the security note says.
