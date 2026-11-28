# CI's v0.4.0 harness shards, run on a laptop (28 November 2026): 140 of 153 tests pass, 13 fail

**What ran:** the four `harness-*` shards of CI's job `lez-040` (`.github/workflows/ci.yml`), each the command
`FERRATA_LEZ_TAG=v0.4.0 bash scripts/ci/lez-harness.sh --dry-run --shard harness-N` prints, with `--no-fail-fast` added
so that every test runs, in dev mode (`RISC0_DEV_MODE=1`, as CI); then its `updater-gate` and `updater-privacy`
shards (below), so all six of the job's shards. Never run on GitHub.

**Tree:** `lez-040-verify` at `a7a45cb6` (it merges `lez-040-core` `41614cfe`, `lez-040-zone` `bce22c2d` and
`lez-040-clients` `d985b86f`), in a scratch worktree with `lez/scripts/lez-select.sh v0.4.0` applied in place;
LEZ v0.4.0 (`a255b42c`) beside it; cargo 1.93.0; r0vm 3.1.0 on `RISC0_SERVER_PATH` (with the LEZ home's 3.0.5, every
v0.4.0 guest execution fails: [`../../port-v0.4.0.md`](../../port-v0.4.0.md)). 28 November 2026, 04:38 to 05:08 UTC (harness-3, then 1, 2 and 4).

**Record:** [`tests.txt`](tests.txt): cargo's lines for each suite, each test's result and each failure's message,
from the run's logs (the build's output and RISC Zero's dev-mode warnings left out; the worktree's path written
`<tree>`).

| Shard | Passed | Failed |
|---|---|---|
| harness-1 | 40 | 5 |
| harness-2 | 33 | 4 |
| harness-3 | 33 | 2 |
| harness-4 | 34 | 2 |
| all four | 140 | 13 |

The 13 failures, by cause:

- **The checkpoint parses v0.4.0's blocks with the rc1 parser** (4): `bridge_roles`
  `transfers_handover_multisig_watchers_and_keyless_sunset`, `interrupted_operations`
  `a_burn_interrupted_and_a_burn_whose_note_was_never_recorded`, `bridge_burn` `burns_and_what_the_checkpoint_collects`,
  `bridge_controls` `registry_fees_caps_freezes_and_sunset`: each stops at "the checkpoint parses the block:
  NotStrictlyOrdered { what: "actors" }". `checkpoint-core` has a v0.4.0 parser (feature `lez-0-4-0`, `lez_0_4_0`);
  the harness does not use it yet.
- **Figures pinned from rc3 runs that v0.4.0 changes** (8): `costs_attestation` (4:
  `an_attestation_costs_the_same_whatever_the_chain_holds`, `the_leaf_statement_costs_the_same_whatever_the_chain_holds`,
  `the_finality_update_costs_what_the_hourly_row_assumes`,
  `the_per_period_updates_cost_what_the_record_shows_and_do_not_grow`; execution and storage gas, e.g. the WETH
  account's (168872, 4466) against the pinned (168909, 4424)), `device_workload` (3:
  `the_private_attestation_proves_the_work_the_record_measured`, `the_private_burn_proves_the_work_the_record_measured`,
  `the_private_claim_proves_the_work_the_record_measured`; v0.4.0's private transactions are about 1,080 bytes smaller,
  e.g. 230,165 against 231,246), `vault` `the_vault_on_sepolia` (the attestation's cycles 327,093 against the pinned
  327,016). Each test pins rc3's record; none has a v0.4.0 value.
- **A test's comparison of two private claims** (1): `security_privacy` `f8_two_deposits_full_diff` panics at
  `lez/harness/src/compat.rs:383`, "a fee fallback names its payer's private action". Read after the run: the test's
  `align` rewrites the copied claim's private actions (their nullifiers) before it reads that claim's fee view, and
  v0.4.0's compact fee fallback (LEZ #1146) finds its key through the payer action's nullifier; the claims themselves
  were built and settled.

Among the 140 that pass: `lez040` (4: the fee program as the system upgrader's PDA, storage paid by size, receipts as
settlement charged, a refused transaction's failed receipt), `redteam_bridge` (9), `redteam_lc` (11), `security_lc`
(5), `bridge_f13` (15), `vault_consumer` (4).

**The job's `updater-gate` shard** ([`updater-gate.txt`](updater-gate.txt)), on the same tree and toolchain, 05:12 to
05:30 UTC: `ferrata-updater`'s eight recorded-data tests passed (8 of 8, 985 s) and `updater/dev/demo.sh` exited 0.
(While the demo built, a proposed test helper module was in the harness library for about a minute, then taken out;
the updater does not use it.)

**The job's `updater-privacy` shard** ([`updater-privacy.txt`](updater-privacy.txt)), same tree and toolchain, 05:33 to
05:41 UTC: `ferrata-updater`'s other tests passed (105, then the remaining 9 recorded-data tests; none failed). The
privacy suite (`scripts/privacy/run.sh`) failed in its LEZ half: `privacy_suite_lez_half` stops at the same rc1
parser (`privacy_suite.rs:303`, "NotStrictlyOrdered { what: "actors" }"), so `controls.py` then found no
`target/privacy/lez.json` and exited 1. (`check-results.py` exited 0 on a directory an earlier run had written,
27 November 23:46 UTC: no evidence here, and its output is left out.)

So CI's `lez-040` job, which gated until 3359f02f, would fail on five of its six shards until the harness parses
through `lez_0_4_0` (five tests), the pinned figures get v0.4.0 values (or those tests leave the v0.4.0 job with the
reason), and that test reads the fee views before it rewrites the actions.
