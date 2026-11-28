# CI's v0.4.0 shards again, after the core's fixes, on a laptop (28 November 2026): 159 of 159 tests pass, 0 fail

**What ran:** all six shards of CI's job `lez-040` (`.github/workflows/ci.yml`), each the command
`FERRATA_LEZ_TAG=v0.4.0 bash scripts/ci/lez-harness.sh --dry-run --shard NAME` prints, with `--no-fail-fast` added, in
dev mode (`RISC0_DEV_MODE=1`, as CI); the privacy suite's output written to this agent's own folder instead of the
shared `../tmp/ci-privacy`. Never run on GitHub.

**Tree:** `lez-040-verify` at `cec9d834` (it merges `lez-040-core` `d0991d33`, `lez-040-zone` `444e39b2` and
`lez-040-clients` `9d74124a`), in a scratch worktree with `lez/scripts/lez-select.sh v0.4.0` applied in place; LEZ
v0.4.0 (`a255b42c`) beside it; cargo 1.93.0; r0vm 3.1.0 on `RISC0_SERVER_PATH`. 28 November 2026, from 07:13 UTC.
The same laptop, toolchain and commands as [`../harness-a7a45cb6/`](../harness-a7a45cb6/README.md), the run before
the core's fixes (140 of 153 then).

**Record:** [`tests.txt`](tests.txt), [`updater-gate.txt`](updater-gate.txt),
[`updater-privacy.txt`](updater-privacy.txt): cargo's lines for each suite, each test's result and each failure's
message (the build's output and RISC Zero's dev-mode warnings left out; the worktree's path written `<tree>`, this
agent's folder `<agent>`). `python3 -I scripts/verify.py lez040-run-harness` holds this README to them.

| Shard | Passed | Failed |
|---|---|---|
| harness-1 | 45 | 0 |
| harness-2 | 37 | 0 |
| harness-3 | 41 | 0 |
| harness-4 | 36 | 0 |
| all four | 159 | 0 |

4 ignored, all in `lez040_guards`, each with its reason in the test (on LEZ v0.4.0 in the harness, the native
transfer to that system account reverts in execution before the guard runs; cause not established):
`a_public_transfer_to_the_clock_is_refused`, `a_public_native_balance_increase_of_the_bridge_escrow_is_accepted`,
`a_private_native_balance_increase_of_the_bridge_escrow_is_refused`,
`a_public_native_balance_increase_does_not_modify_the_system_registry_actor_state`. Its fifth case,
`a_public_transfer_to_each_fee_account_is_refused_including_the_fee_inbox`, passes.

The 13 tests that failed at `a7a45cb6` pass here, among them the four parser tests, `f8_two_deposits_full_diff` and the
eight with pinned figures (on v0.4.0 those print v0.4.0's figures beside rc3's records instead of asserting them).
`lez040` passes 9 of 9 (the fee program's address, storage paid by size, receipts, public payers, double fees).

**The `updater-gate` shard** ([`updater-gate.txt`](updater-gate.txt)), 07:40 to 07:59 UTC: `ferrata-updater`'s eight recorded-data tests passed (8 of 8, 1,064 s) and `updater/dev/demo.sh` exited 0.

**The `updater-privacy` shard** ([`updater-privacy.txt`](updater-privacy.txt)), 07:59 to 08:07 UTC: `ferrata-updater`'s other tests passed (105, then the remaining 9 recorded-data tests; none failed). The privacy suite's LEZ half, `privacy_suite_lez_half`, passed on v0.4.0 (at `a7a45cb6` it stopped at the rc1 parser). Its Ethereum half did not run here: `scripts/privacy/run.sh` builds the vault with Foundry, and the vault's libraries (`vault/lib/`, which CI installs in a step of its own) are absent in this scratch worktree, so `check-results.py` found no data (32 of 33 checks failed) and `controls.py` exited 1. That is this laptop's setup, not a result about v0.4.0: the privacy suite's Ethereum half and its checks are not shown here.

At `d0991d33` the core piece's build-box record showed the same: no failure, `lez040_guards` 4 of 5 ignored. Its
`5214ae68` has since fixed those tests' fixture (5 of 5 on the box,
[`lez/results/v0.4.0/suites.md`](../../../lez/results/v0.4.0/suites.md)); not run here.
