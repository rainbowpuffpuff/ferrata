# The harness's and the updater's suites on LEZ v0.4.0, rc3 and rc1 (build box)

Every test binary of `lez/harness` and `lez/updater` (the lez-harness shards' binaries, as CI runs them: `cargo test
--locked -p harness --test NAME` / `-p ferrata-updater --features harness --test NAME`, after
`lez/scripts/lez-select.sh TAG`), dev mode (`RISC0_DEV_MODE=1`), on the Modal build box,
28 November 2026; guests built in the same run (RISC Zero 3.1.0 and r0vm 3.1.0 on v0.4.0, 3.0.5 on the release
candidates). One status line per binary in `suites/` (`NAME exit RC (P passed, F failed, I ignored)`).

The runs, and the tree each tested:

- **run1** (`suites/run1-*.txt`): every binary, on the tree that became c36a60c4 (synced to the box at 02:35Z).
  v0.4.0: 64 of 76 binaries passed outright then; 12 binaries failed for reasons fixed since (rc3's pinned figures,
  the checkpoint's rc1 block parser on v0.4.0 blocks, the loader's trace, a test's lookup order). rc3: two failures
  (immutable_program, residue_handoff: rc2+ meter the native loader; rc3's own ferrata_lez build id), never run at the
  base, where the rc3 selection did not build. rc1: 68 binaries ran before the box's time limit, all passed.
- **rerun1** (`suites/rerun1-v040.txt`): the 12 on 41614cfe plus the fixes of 04:04Z.
- **rerun2** (`suites/rerun2-*.txt`): the 13 changed binaries on d0991d33's tree before the comment edits and
  `#[ignore]`s of the e-INFRA worker's review 7 (lez040_guards then failed 4 of 5: see below); rc1 also its remaining updater binaries
  but `recorded` (still running when this was written).
- **rerun3** (`suites/rerun3-v040.txt`): lez040_guards with its fixture fixed (the clock, bridge escrow and registry
  accounts put in the state; review round 2's finding): 5 of 5.
- **rerun4** (`suites/rerun4-v040.txt`, v0.4.0 only, partial): every harness and updater binary of run1 again, in
  run1's order, on 5214ae68's tree (the drivers' later commit does not touch these suites); 61 of 74 had finished when
  this was written (the rest, from updater-diversity_limits on, did not), all passing. Seven of them are kept out: the
  feature-gated names, names_review, messages, messages_findings, messages_v2, rebate and statements ran without
  their features there (0 tests); their run1 rows stand. The table takes rerun4 for the other 54 on v0.4.0.

The table gives each binary's latest run. A binary with no later run shows run1, on the c36a60c4 tree: those binaries' code
paths changed after c36a60c4 only by 35e5fb81's settlement guards, and an independent laptop run of all four CI
harness shards on v0.4.0 at 41614cfe (P4, lez-040-verify a7a45cb6, docs/port-v0.4.0/harness-a7a45cb6/) passed every
binary but the 13 that rerun2 covers here. That laptop run is a different record; this file reports the box's.

lez040_guards: in rerun2 4 of its 5 cases failed: the fixture did not put the clock, bridge escrow and registry
accounts in the state, and v0.4.0 refuses an unadmitted actor ("Program violated execution rules"); fixed, rerun3.

- v0.4.0: 77 of 77 binaries pass (339 tests passed, 0 failed)
- v0.4.0-rc3: 77 of 77 binaries pass (325 tests passed, 0 failed)
- v0.4.0-rc1: 75 of 75 binaries pass (290 tests passed, 0 failed)

| test binary | v0.4.0 | v0.4.0-rc3 | v0.4.0-rc1 |
|---|---|---|---|
| `harness-attest_cli` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-attest_native` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-attest_private` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-attest_public` | pass 4/4 (rerun4) | pass 4/4 (run1) | pass 4/4 (run1) |
| `harness-bridge_burn` | pass 1/1 (rerun4) | pass 1/1 (rerun2) | pass 1/1 (rerun2) |
| `harness-bridge_burn_tag` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-bridge_controls` | pass 1/1 (rerun4) | pass 1/1 (rerun2) | pass 1/1 (rerun2) |
| `harness-bridge_delist` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-bridge_depth` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-bridge_f13` | pass 15/15 (rerun4) | pass 15/15 (run1) | pass 15/15 (run1) |
| `harness-bridge_inbound` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-bridge_private` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-bridge_refusals` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-bridge_roles` | pass 2/2 (rerun4) | pass 2/2 (rerun2) | pass 2/2 (rerun2) |
| `harness-bridge_secfix` | pass 9/9 (rerun4) | pass 9/9 (run1) | pass 9/9 (run1) |
| `harness-bridge_time` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-bridge_vault_storage` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-captured_committee_exposure` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-costs_attestation` | pass 9/9 (rerun4) | pass 9/9 (rerun2) | pass 9/9 (rerun2) |
| `harness-device_workload` | pass 3/3 (rerun4) | pass 3/3 (rerun2) | pass 3/3 (rerun2) |
| `harness-exit_codes` | pass 4/4 (rerun4) | pass 4/4 (run1) | pass 4/4 (run1) |
| `harness-history_hops` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-history_hops_review` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-holdings` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-immutable_program` | pass 2/2 (rerun4) | pass 2/2 (rerun2) | pass 2/2 (rerun2) |
| `harness-interrupted_operations` | pass 2/2 (rerun4) | pass 2/2 (rerun2) | pass 2/2 (rerun2) |
| `harness-lez040` | pass 9/9 (rerun4) | pass 0/0 (rerun2) | pass 0/0 (rerun2) |
| `harness-lez040_guards` | pass 5/5 (rerun3) | pass 0/0 (rerun2) | pass 0/0 (rerun2) |
| `harness-lib` | pass 0/0 (run1) | pass 0/0 (run1) | pass 0/0 (run1) |
| `harness-messages` | pass 1/1 (run1) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-messages_findings` | pass 1/1 (run1) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-messages_v2` | pass 1/1 (run1) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-names` | pass 6/6 (run1) | pass 6/6 (run1) | pass 6/6 (run1) |
| `harness-names_review` | pass 9/9 (run1) | pass 9/9 (run1) | pass 9/9 (run1) |
| `harness-privacy_suite` | pass 1/1 (rerun4) | pass 1/1 (rerun2) | pass 1/1 (rerun2) |
| `harness-private_consumer` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-private_placement` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-public_path` | pass 5/5 (rerun4) | pass 5/5 (run1) | pass 5/5 (run1) |
| `harness-rebate` | pass 4/4 (run1) | pass 4/4 (run1) | pass 4/4 (run1) |
| `harness-redteam_attest` | pass 11/11 (rerun4) | pass 11/11 (run1) | pass 11/11 (run1) |
| `harness-redteam_bridge` | pass 9/9 (rerun4) | pass 9/9 (run1) | pass 9/9 (run1) |
| `harness-redteam_lc` | pass 11/11 (rerun4) | pass 11/11 (run1) | pass 11/11 (run1) |
| `harness-residue_experimental` | pass 5/5 (rerun4) | pass 5/5 (run1) | pass 5/5 (run1) |
| `harness-residue_handoff` | pass 3/3 (rerun4) | pass 3/3 (rerun2) | pass 3/3 (rerun2) |
| `harness-residue_placements` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-security_lc` | pass 5/5 (rerun4) | pass 5/5 (run1) | pass 5/5 (run1) |
| `harness-security_privacy` | pass 2/2 (rerun4) | pass 2/2 (rerun2) | pass 2/2 (rerun2) |
| `harness-sepolia` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-stall` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-state_growth_bridge` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-statements` | pass 3/3 (run1) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-statements_same_block` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-successor` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `harness-tenant` | pass 4/4 (rerun4) | pass 4/4 (run1) | pass 4/4 (run1) |
| `harness-validity_after_rotations` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `harness-vault` | pass 2/2 (rerun4) | pass 2/2 (rerun2) | pass 2/2 (rerun2) |
| `harness-vault_claim_hops` | pass 2/2 (rerun4) | pass 2/2 (run1) | pass 2/2 (run1) |
| `harness-vault_consumer` | pass 4/4 (rerun4) | pass 4/4 (run1) | pass 4/4 (run1) |
| `harness-wallet_padding` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `updater-bounds` | pass 3/3 (rerun4) | pass 3/3 (run1) | pass 3/3 (run1) |
| `updater-config` | pass 9/9 (rerun4) | pass 9/9 (run1) | pass 9/9 (run1) |
| `updater-copies` | pass 5/5 (rerun4) | pass 5/5 (run1) | pass 5/5 (run1) |
| `updater-daemon_clock` | pass 1/1 (rerun4) | pass 1/1 (run1) | pass 1/1 (run1) |
| `updater-diversity_limits` | pass 4/4 (run1) | pass 4/4 (run1) | pass 4/4 (run1) |
| `updater-every_party` | pass 8/8 (run1) | pass 8/8 (run1) | pass 8/8 (run1) |
| `updater-execution` | pass 2/2 (run1) | pass 2/2 (run1) | pass 2/2 (run1) |
| `updater-health` | pass 8/8 (run1) | pass 8/8 (run1) | pass 8/8 (run1) |
| `updater-jobs` | pass 2/2 (run1) | pass 2/2 (run1) | pass 2/2 (run1) |
| `updater-join_wait` | pass 13/13 (run1) | pass 13/13 (run1) | pass 13/13 (run1) |
| `updater-lib` | pass 11/11 (run1) | pass 11/11 (run1) | pass 11/11 (rerun2) |
| `updater-recorded` | pass 17/17 (run1) | pass 17/17 (run1) | not run |
| `updater-redating` | pass 10/10 (run1) | pass 10/10 (run1) | pass 10/10 (rerun2) |
| `updater-refusals` | pass 16/16 (run1) | pass 16/16 (run1) | pass 16/16 (rerun2) |
| `updater-replacement` | pass 1/1 (run1) | pass 1/1 (run1) | pass 1/1 (rerun2) |
| `updater-serve` | pass 7/7 (run1) | pass 7/7 (run1) | pass 7/7 (rerun2) |
| `updater-turns` | pass 5/5 (run1) | pass 5/5 (run1) | pass 5/5 (rerun2) |
| `wire` | pass 18/18 (run1) | pass 18/18 (run1) | not run |
