# Stream G: CI, platforms, release and migrations, proposed statuses

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch
`rfp-ci-release` from `f13faee2`. Nothing was pushed, so no GitHub run exists for anything new here: a clause that
says "in CI" is met by the workflow and shown by the same script run on the build box; its first GitHub run is the
owner's push. Box runs: the Modal build box (Linux x86-64), RISC Zero dev mode unless stated.

## RFP-021

| ID | Proposed | Clause by clause | Evidence (path; command; where, commit) |
|---|---|---|---|
| S.2 | in part (unchanged; advanced) | *End-to-end integration tests exercise attestation production and consumption against a LEZ sequencer (standalone mode) and an Ethereum test network or local fork, and are included in CI*: [`scripts/ci/sequencer-attest.sh`](../../scripts/ci/sequencer-attest.sh) builds LEZ v0.4.0-rc1's `sequencer_service` with its `standalone` feature from source and drives the live driver's phases against it; [`.github/workflows/sequencer.yml`](../../.github/workflows/sequencer.yml) runs it (manual only for now). **Shown:** the programs deployed through a standalone sequencer's own loader (20 transactions applied; image `816d1fef…` as in `docs/image-ids.md`). **Not shown:** production and consumption in that run: `scenarios` stopped because its recorded mainnet checkpoint (about 31 October 2026) is now outside the `Init`'s validity window on a sequencer running on the wall clock. Next: the same phases on a stand-in committee's fresh data (`inbound --chain stand-in`), then push and pull_request triggers. Production and consumption on sequencers we ran remain shown by hand (EVIDENCE 6, 21, 76, 99). | Box, 21 Nov 2026, commit `503eb3b0`: [`e2e/runs/ci-sequencer-attest-2026-11-21/`](../../e2e/runs/ci-sequencer-attest-2026-11-21/README.md) (build 16 min; 21 sent, 20 applied, 1 reverted: twin `Init`, validity window). |
| S.9 | in part (one owner step) | *Publish the modules in a catalog of the team's own, built from the template, installable by Logos clients*: the catalog is laid out from `logos-modules-release-base` and was installed from 127.0.0.1 before (EVIDENCE 61, 98); publishing is now one owner command, `modules/scripts/release-publish.sh ORG CATALOG_REPO KEY --yes`, after creating the organisation and the signing key ([`modules/catalog/RELEASE.md`](../../modules/catalog/RELEASE.md)). **Not published.** | Dry run of the owner step into local bare repositories: [`modules/catalog/dry-run-publish/README.md`](../../modules/catalog/dry-run-publish/README.md) (five of six modules exported; the updater's export needs built guests). Laptop, 21 Nov 2026, commit `60c830a4`. |
| S.10 | in part (one owner step) | *Built and released to that catalog using logos-modules-release-action*: the catalog's workflows call the action pinned to commit `bbdc5287` (its `v1`), with `signing_mode: inline` (secret `LOGOS_SIGNING_KEY`, a JWK Ed25519 key) and the variants `darwin-arm64`, `linux-amd64`, `linux-arm64`, `windows-x86_64`. **The action has not run on a release.** | `modules/catalog/.github/workflows/*.yml` (8 files, all parse); the action's steps run locally for `linux-amd64` before (EVIDENCE 61). Commit `60c830a4`. |
| VS.3 | shown | *No consensus or inclusion verification reached through a `ProgramId` held in mutable account state*: the program is deployed through LEZ's loader with an immutable header naming its image id; every replacement attempt is refused. *Change requires deploying a new version and having consumers move to it; document the migration mechanism and what consumers must do*: consumer guide §9 (branch `rfp-docs-final`), `lez/updater/tests/replacement.rs` (a consumer moves to a replacement, stream H). *The consumer documentation states the same rule for consumers*: consumer guide §2, "Pin the program, never a pointer (VS.3)". | `lez/harness/tests/immutable_program.rs`, 2 of 2: `RISC0_DEV_MODE=1 cargo test -p harness --test immutable_program -- --nocapture` (from `lez/`); box, 21 Nov 2026, commit `7a7e7b6d`; log `e2e/runs/vs3-immutable-program.log`. `UpdateHeader` to other code (twice), `CreateHeader` over the deployment and `WriteSegment` over its first segment refused with the loader's own assertions; every account they could touch unchanged; the same `Attest` byte for byte before and after; a gate naming another program records nothing. Control: a mutable copy of the same code is swapped by `UpdateHeader` and then runs other code. |

## RFP-022

| ID | Proposed | Clause by clause | Evidence |
|---|---|---|---|
| R.10 | in part (unchanged) | *The full deposit and redemption round trip against a LEZ sequencer (standalone mode) and an Ethereum test network or local fork, included in CI*: CI runs the round trip over LEZ's state transition and anvil (`roundtrip` job); the sequencer round trips are by hand (`e2e/seqnet/`, a sequencer and a Logos Blockchain node from a prebuilt bundle). **Not done here:** the seqnet zone start (`lez/crown/session.sh`) needs adapting to LEZ's mock Bedrock and building from source in CI, and the rule (B) checkpoint must read blocks from the standalone sequencer. The attestation half against a standalone sequencer is RFP-021 S.2 above. | EVIDENCE 41, 76, 91, 100, 102 (as before). |
| R.11 | shown (after the branches merge) | *Every hard requirement has at least one corresponding test*: [`scripts/ci/row-tests.toml`](../../scripts/ci/row-tests.toml) maps every hard row of both RFPs to at least one tracked test, test script or recorded run, with the CI job that runs it or the command; [`scripts/ci/check-row-tests.py`](../../scripts/ci/check-row-tests.py) fails CI's `links` job if a hard row has none, if a path is not tracked or a job does not exist. The two audit rows map to `scripts/ci/check-unaudited.sh`, which tests the bid's promise until the reports are published (the code is marked unaudited, not for production, everywhere a user or catalog reader meets it). | `python3 -I scripts/ci/check-row-tests.py`: exit 0; RFP-021 59 hard rows, 20 with a CI-run test; RFP-022 68, 44 with a CI-run test; 16 entries pending on `rfp-docs-final` (they exist there); `bash scripts/ci/check-unaudited.sh`: exit 0 (it found six module descriptions without the marking; fixed). Commit `3d5d92ec`. |
| S.2 | in part | *The modules work on both Linux and macOS*: Linux as before (EVIDENCE 59–62, 94–98); macOS: [`.github/workflows/macos.yml`](../../.github/workflows/macos.yml) on `macos-14` (the user-side crates' tests; every module's `.lgx` built with Nix as `release-local.sh` builds it, `lgx verify`ed, uploaded) and the catalog's `darwin-arm64` variant. **Not run: no push.** | Commit `3d5d92ec`; user-side proofs on macOS before (EVIDENCE 88). |
| U.10 | in part (one owner step) | As RFP-021 S.9. | Commit `60c830a4`. |
| S.3 | in part (one owner step) | As RFP-021 S.10. | Commit `60c830a4`. |
| Sec.8 | shown | *Change requires deploying a new version and migrating to it; document the migration mechanism and how in-flight deposits and burns are honoured across it*: [`vault/MIGRATION.md`](../../vault/MIGRATION.md) (stop, retire, sunset, refunds, releases, the new configuration, users moving) and deployer guide §9; rehearsed on a local EVM with every in-flight position. | `vault/test/Migration.t.sol`, 8 of 8: `cd vault && forge test --match-path 'test/Migration*.t.sol'` (laptop, 21 Nov 2026, re-run by the lead; the CI `vault` job runs it); in-flight burn released to its own recipient only, unimported deposit refunded to its depositor only after the sunset and its revocation window, imported deposit not refundable, no second refund, the holder moves to the new vault. Commit `53c9827e`. |
| Sec.9 | shown | *Specify a failsafe that is not a backdoor, or document why none should be implemented and what happens to affected funds and in-flight positions*: stop-only (ADR 0016, `docs/trust.md` §6, `vault/MIGRATION.md`): RISC Zero's proof of unsoundness trips the stop; no role can move escrow; recovery only by the depositor's own refund before a stop or the holder's own release. What happens after a stop: everything is stranded except owed balances, refunds included even with a final sunset checkpoint (the built D13 rule; D8's wording was wider, now corrected in ADR 0016 on `rfp-docs-final`). | `vault/test/MigrationFailsafe.t.sol`, 4 of 4 (every role's every value-moving function refuses after the stop; owed balances still pay their payee; the refund control before the stop and its refusal after); `vault/test/EmergencyStop.t.sol` (EVIDENCE 35). Commit `53c9827e`. |
| Soft.4 | shown | *Proof-system components pluggable, so future zkVM improvements, proof compression or hardware acceleration can be adopted without restructuring the vault or the bridge program (redeployment and migration necessary)*: the vault takes its verifier (the `IRiscZeroVerifier` interface behind RISC Zero's emergency stop) and image ids as constructor parameters; the same `FerrataVault` deployed with proof system B runs deposit, checkpoint and release; each system's seal is refused by the other vault; no function can change a deployed vault's verifier or image ids. | `vault/test/Migration.t.sol` (v2 with `vault/test/mocks/MockVerifierB.sol`; the vault's 90 external functions read from its artifact). Commit `53c9827e`. |

## Proposed EVIDENCE rows

1. **Replacement attempts on the deployed attestation program (RFP-021 VS.3).** `lez/harness/tests/immutable_program.rs`:
   LEZ v0.4.0-rc1's loader refuses `UpdateHeader` to other code, `CreateHeader` over the deployment and `WriteSegment`
   over its first segment, each with its own assertion; nothing changes and the same `Attest` runs byte for byte; the
   control, a mutable copy of the same ELF, is swapped. 2 of 2. Box, 21 Nov 2026, dev mode.
   `e2e/runs/vs3-immutable-program.log`.
2. **A migration rehearsal with a proof-system change (RFP-022 Sec.8, Sec.9, Soft.4).** `vault/test/Migration.t.sol`
   (8) and `MigrationFailsafe.t.sol` (4): in-flight deposits and burns of a retired vault honoured; the same vault code
   under proof system B; seals not interchangeable; no verifier or image-id setter; after a stop no role moves escrow,
   refunds included. 12 of 12; the full suite 592 passed. Local EVM (Foundry), 21 Nov 2026. `vault/MIGRATION.md`.
3. **Every hard row mapped to a test (RFP-022 R.11).** `scripts/ci/row-tests.toml` and `check-row-tests.py` in CI's
   `links` job: 127 hard rows, 64 with a CI-run test, the rest on the box or by hand, each named with its command.
4. **The catalog ready to publish in one owner step (RFP-021 S.9, S.10; RFP-022 U.10, S.3).** `release-publish.sh`,
   signed releases, the action pinned by commit, a dry run into local repositories, the integrity manifest template,
   every lambda-prize integrity rule mapped (`modules/catalog/RELEASE.md`).
5. **A standalone LEZ sequencer built from source in a CI script (RFP-021 S.2, partial).**
   `scripts/ci/sequencer-attest.sh`: LEZ v0.4.0-rc1's `sequencer_service --features standalone` built in 16 min on the
   box; the attestation program (main, twin) and `scripted` deployed through its loader, 20 transactions applied; the
   `scenarios` phase stopped on aged recorded data (validity window). Box, 21 Nov 2026.
   `e2e/runs/ci-sequencer-attest-2026-11-21/`.

## The owner's steps that move rows to shown

1. Push the merged branches: the first GitHub run of `macos.yml` (RFP-022 S.2). `sequencer.yml` (RFP-021 S.2) waits
   for its fresh-data phases (S.2 above).
2. Pin the actions: `python3 scripts/ci/pin-actions.py --write` (network), then add `check-actions-pinned.py` to the
   `links` job.
3. Publish the catalog: create the organisation and the signing key, then
   `modules/scripts/release-publish.sh ORG CATALOG_REPO KEY --yes` and the browser steps it prints (RFP-021 S.9, S.10;
   RFP-022 U.10, S.3).

## Files outside this stream's own

Changed, minimally: the six `modules/ferrata_*_module/metadata.json` descriptions ("prototype" → "prototype,
unaudited, not for production"; no code, no image id); `docs/release-engineering.md` §3 (a subsection). New:
`lez/harness/tests/immutable_program.rs`, `vault/test/Migration.t.sol`, `vault/test/MigrationFailsafe.t.sol`,
`vault/test/mocks/MockVerifierB.sol`, `vault/MIGRATION.md`, `e2e/runs/`. No guest, program or vault source changed;
no image id moved.
