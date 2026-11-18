# Mutation testing of the vault with Gambit

A tool-generated mutation run over the vault and its library, with the vault's whole Foundry suite run against every
mutant, on 18 November 2026. It complements the hand-written mutants of `tools/mutation_spotcheck.py` (254) and
`tools/review_mutants.py` (25, VR01–VR25), which plant one chosen bug each; Gambit plants every mutation its operators
can make. (ferrata-core's Rust mutants are in [`../docs/mutants-run.log`](../docs/mutants-run.log): 107 killed, 0
survived.)

**Result.** 3,170 mutants of `src/FerrataVault.sol` (1,534 lines) and `src/FerrataRegistry.sol` (606 lines): 1,853 do
not compile; of the 1,317 that do, the suite as it stood killed 1,297 and let 20 survive. Read one by one, 14 of the
20 were test gaps and 6 are equivalent. Ten new tests kill the 14 (run 3). **Now 1,311 killed, 6 equivalent, 0
unexplained, 0 timed out.** No survivor showed a bug in the vault; one flaky invariant failure, found on the
unmutated vault, was a bug in the invariant handler's model (below), fixed with a regression test.

| | Mutants | Invalid (do not compile) | Killed | Equivalent | Timed out | Survived, unexplained |
|---|---|---|---|---|---|---|
| `FerrataVault.sol` | 1,934 | 1,156 | 777 (769 before the new tests) | 1 | 0 | 0 |
| `FerrataRegistry.sol` | 1,236 | 697 | 534 (528 before) | 5 | 0 | 0 |
| **Both** | **3,170** | **1,853** | **1,311** (1,297) | **6** | **0** | **0** |

Of the 1,297 kills of run 2, 1,292 came from the unit and fuzz suites and 5 only from the invariant campaign
(vault-752, vault-753: the sunset checkpoint's index not recorded; vault-853: lifting a checkpoint freeze that had
already lapsed records now instead of when it lapsed; vault-1781: a fee change put in force before it is due; registry-101: an announcement compared with schedule 0 instead of the
schedule in force). The equivalence check by bytecode found none (below); the 6 are equivalent by reading.

By operator (both files, after run 3): `BinaryOpMutation` 1,326 invalid, 465 killed; `AssignmentMutation` 446
invalid, 210 killed, 3 equivalent; `IfStatementMutation` 2 invalid, 295 killed, 1 equivalent;
`DeleteExpressionMutation` 10 invalid, 211 killed, 2 equivalent; `SwapArgumentsOperatorMutation` 120 killed;
`UnaryOperatorMutation` 69 invalid, 10 killed. The invalid ones are type errors, mostly an arithmetic operator put
where a comparison or a boolean was (solc's errors 7407, "not implicitly convertible", and 2271, a binary operator
on the wrong types).

## How it ran

- **Tool.** Certora's Gambit, tag `v1.0.7` (commit `072ff4c6`; its crate still says 0.2.1), built on the box with
  `cargo install --locked --git https://github.com/Certora/gambit --tag v1.0.7`; solc 0.8.26 (the project's pinned
  compiler) as `solc`. Every operator Gambit has, on both whole files ([`gambit.conf.json`](gambit.conf.json)).
  `skip_validate` is set: Gambit's own validation kept none of the vault's 1,934 mutants (it kept 539 of the
  registry's 1,236), though the vault's mutants compile with the same solc; so the build validates instead, and the
  registry's 697 invalid ones are exactly the ones Gambit's validation dropped (1,236 − 539).
- **Suite.** For each mutant, [`tools/gambit_run.py`](tools/gambit_run.py) writes it into a worker's own copy of the
  project (`notes/gambit-ws/wNN`: `foundry.toml`, `src/`, `test/`, `script/`, `lib/` linked) and
  1. builds with the project's settings (optimizer on, 200 runs, cancun) but no CBOR metadata, so that a mutant whose
     creation and runtime bytecode of both contracts equal the original's is "equivalent" (trivial compiler
     equivalence) without running anything; none was;
  2. runs every suite except the invariant campaign, `--fail-fast` (unit, fuzz, the real-proof tests of `test/real/`);
     a failure is a kill;
  3. runs the invariant campaign (`test/invariant/`, 256 runs of depth 100); a failure is a kill, a pass a survival.

  forge runs inside the copy, and the fuzz inputs and invariant sequences forge records on a failure are cleared
  before each mutant, so every mutant meets the suite as a fresh checkout does. Each worker first checks that the
  unmutated suite passes in its copy. Limits: 300 s for the build and suites, 600 s for the campaign; nothing reached
  them.
- **Where.** The build box (a Modal CPU container, 28 workers, forge 1.3.6), run 2 in 2,108 s (16.8 core-hours by
  `mrun`'s count, about 0.90 USD by its estimate), run 3 (the 20 survivors, 20 workers) in 223 s (1.23 core-hours, about 0.07 USD).
- **Commands** (from `vault/`, with `gambit` and `solc` on `PATH`):

  ```sh
  WORKERS=28 RUN=run2 ./run-mutation.sh                                     # every mutant
  WORKERS=20 RUN=run3 IDS=results/mutation/run3-ids.txt ./run-mutation.sh   # run 2's survivors, with the new tests
  python3 -I tools/gambit_run.py summary notes/gambit_out/vault,notes/gambit_out/registry \
      results/mutation/run2.jsonl results/mutation/run3.jsonl --survivors results/mutation/final-survivors.md
  ```

- **Records** ([`results/mutation/`](results/mutation/)): Gambit's own output, `gambit-{vault,registry}-mutants.log`
  (one line per mutant: operator, place, original, replacement) and `gambit-{vault,registry}-results.json.gz` (every
  mutant's diff); one JSON line per mutant per run (`run2.jsonl`, `run3.jsonl`: status, stage, the first failing
  test), the runs' logs (`run2.log`, `run3.log`), summaries and survivors (`run2-*`, `run3-*`, `final-*`). 1,303 of
  `run2.jsonl`'s 3,170 lines were rebuilt from `run2.log` (the line the runner printed for each mutant; the failing
  test's text is cut at 120 characters there): calls to the box during the run had copied an older `run2.jsonl` over
  the one being written. Where both exist they agree (0 differences); `run-mutation.sh` now writes under `notes/`.

## The 20 survivors of run 2

| Mutant | Where | What it changes | Classification | Killed by (run 3) |
|---|---|---|---|---|
| vault-430 | constructor | `keylessFrom = 1` instead of the deployment time | gap: setUp deploys at timestamp 1 | `MutationGaps.t.sol` `test_theKeylessClockStartsAtTheDeploymentTime` |
| vault-719 | `_finalize` | a re-proven root checks checkpoint 0's `revoked` flag instead of its own checkpoint's | gap: the suite revoked only checkpoint 0 | `test_aRootRevokedAtALaterCheckpointIsAcceptedAgainWhenReproven` |
| vault-794 | `release` | pays the fee payee even a zero relay fee (a zero transfer, or a zero-value call) | gap: no test looked for the absent transfer | `test_aReleaseWithoutARelayFeeMakesNoTransferToTheFeePayee` |
| vault-1061, vault-1063 | `revokeFrom` | `sunsetSetAt` not cleared (or set to 1) when the sunset checkpoint is revoked; it is public | gap: the revoked sunset checkpoint was checkpoint 0 or 1 | `test_revokingTheSunsetCheckpointClearsItsIndex` |
| vault-1367, vault-1373 | `_recordBeacon` | a second request in a block returns beacon 0 (1367), or records a new beacon outside block 1 (1373) | gap: two requests in a block only with one beacon, or in block 1 | `test_aSecondBeaconRequestInABlockReturnsThatBlocksBeacon` |
| vault-1580 | `_checkRelease` | the checkpoint fee credited to checkpoint 0's prover, not the root's checkpoint's | gap: one prover for every checkpoint | `Fees.t.sol` `test_release_checkpointFeeGoesToTheProverOfTheRootsOwnCheckpoint` |
| registry-283 | `_inboundFeesOfLeaf` | the binary search's `high = mid` becomes `high = 1` | gap: right with three schedules or fewer; the suite had three | `test_feeChange_aRefundFindsItsLeafsScheduleAmongFour` |
| registry-484 | `applyRoles` | emits `FreezeAuthorityTransferred(…, address(0))` when the authority stays | gap: no test looked for the absent event | `MutationGaps.t.sol` `test_applyingAProposalThatKeepsTheFreezeAuthorityEmitsNoTransfer` |
| registry-728, registry-731 | `_updateBucket` | a cap update keeps the old `lastUpdate` (or writes 1), so refill earned before the update counts again at the new rate | gap: the suite changed caps with no time between | `test_aCapUpdateRestartsTheRefillClock` |
| registry-1021, registry-1027 | `_initFees` (registration) | schedule 0's `announcedAt` (or `effectiveAt`) is 1, not the registration's time, in the published schedule hash | gap: setUp registers at timestamp 1 | `Fees.t.sol` `test_registration_scheduleZeroCarriesTheRegistrationsTime` |
| vault-1761 | `_feesInForce` | always calls the library's `applyFees` | **equivalent**: `applyFees` returns the index in force, changing nothing, unless `pendingAt != 0 && block.timestamp >= pendingAt`, the very condition removed; only gas differs | — |
| registry-115, -116, -118 | `announceFees` | the announced schedule's `effectiveAt` not stored (or 0, or 1) | **equivalent**: a dead store. The pending time lives in `FeeState.pendingAt`; `applyFees` overwrites the schedule's `effectiveAt` with the time it applies it, before `_publishSchedule`, the field's only reader; a withdrawn or replaced announcement is overwritten by the next one; `feeSchedules` is internal | — |
| registry-463 | `applyRoles` | the applied proposal's watchers not cleared | **equivalent**: the getter `roleProposal()` omits the array; `applyRoles` reads it only after `effectiveAt != 0`, which it zeroes; `proposeRoles` writes all three slots first | — |
| registry-686 | `initBucket` | a new bucket's `lastUpdate` is 1 | **equivalent**: a new bucket is full, so `currentLevel` returns its capacity whatever `lastUpdate` is (the refill saturates; `lastUpdate` is never in the future), and the first consumption or update writes `lastUpdate` | — |

The four equivalent kinds are also places where the vault spends gas for nothing (registry-115's store, vault-1761's
check before a call that checks again); `src/` is left as it is.

## Found on the way

- **A flaky invariant failure on the unmutated vault, a bug of the test model.** In the first run, one worker's
  baseline failed `invariant_feesFollowTheAnnouncedSchedules` (`1 != 0`) after the shrunk sequence
  `announceFees(728, 7)`, `proposeRoles(…)`, `applyRoles(7427)`, `applyFees(7508)`, `claim(12688)`: setUp had
  announced ETH's fees from seed 7, the fuzzer announced them again from seed 7, so exactly the fees in force, which
  `FerrataRegistry.announceFees` takes as withdrawing the pending announcement; the handler's model put them in force
  14 days on, and the claim found the vault's published schedules one short of the model's. The vault is right; the
  model withdrew only on its own one-in-five choice. Fixed in `test/invariant/VaultHandler.sol`;
  `test_announcingTheFeesInForceIsAWithdrawalInTheModelToo` replays the sequence (it fails on the old handler,
  `1 != 0`, and passes now). The failure was rare (one of 28 campaigns): the campaign is not the same from one forge
  process to the next here, even with the fixed seed.
- **Recorded failures leak between runs.** forge records a failing fuzz input or invariant sequence under
  `cache/fuzz` and `cache/invariant` relative to its working directory, not to `--root`, and replays it first next
  time. In the first run every worker shared `vault/cache/`, so each mutant's campaign replayed the sequence above
  and failed; that run (stopped after 636 mutants) is not used, and the runner now isolates and clears them.
- **The suite after this work:** 476 tests passed, 1 skipped (`forge test`, 47 suites, on the box), up from 464: the
  ten tests above (seven in the new [`test/MutationGaps.t.sol`](test/MutationGaps.t.sol), three in
  `test/Fees.t.sol`) and the regression test, which runs in both invariant suites.
