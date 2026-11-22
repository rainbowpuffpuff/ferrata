# Rows file: CI coverage of the hard requirements (stream rfp-T2, branch `rfp-ci-coverage`)

Branch `rfp-ci-coverage`, from `rfp-ci-release` at `e3ff6240`. Goal: as many hard rows of RFP-021 and RFP-022 as
possible get a real test that CI runs (RFP-022 R.11, "Every hard requirement has at least one corresponding test";
the A4/B4 gates' "one named test per hard requirement, passing in CI"). Measured with the row checker
(`python3 -I scripts/ci/check-row-tests.py --list`), which counts a row only when a job that runs on push or pull
request really runs one of its tests (`scripts/ci/ci_model.py` reads the workflows and the scripts they call).

| RFP | hard rows with a test CI runs, at `e3ff6240` | on this branch |
|---|---:|---:|
| RFP-021 | 23 of 59 | **50 of 59** |
| RFP-022 | 46 of 68 | **62 of 68** |

`--require-ci RFP-022` still exits 1: six rows have no test CI runs (below, with the reason each gives in the map).
Nothing ran on GitHub: every number here is from the Modal build box, with the jobs' own commands, at 4 CPUs
(`taskset -c 0-3`, the hosted runner's vCPU count) unless marked "not pinned".

## What CI runs now that it did not

| Job or step (`.github/workflows/ci.yml`) | Runs | Build box, measured |
|---|---|---|
| `lez-harness` (new) | `scripts/ci/lez-harness.sh`: every test file of `lez/harness/tests/` and `lez/updater/tests/` that builds without an optional feature, RISC Zero dev mode; then `scripts/privacy/run.sh` and `scripts/privacy/check-results.py` | cold, 4 threads, at `e3ff6240`: build 622 s, harness 1,620 s, updater 1,025 s, all binaries passed, `lez/target` 3.6 GB; the whole script at the tip (warm build): harness 2,074 s, updater 1,136 s, privacy suite and gate 106 s, 3,362 s in all, 59 test binaries ok, gate 33 of 33 |
| `monitor` (new) | `cargo test` in `monitor/`; `monitor/tests/anvil-e2e.sh` from `monitor/` | 4 threads, cold: 69 s and 131 s, passed |
| `roundtrip`, new step | `cargo test --lib relayer` and `--test gasdrop` in `cli/` (the offline part of `cli/tests/relayer.sh`) | not pinned to 4 CPUs: 384 s (cold library build) and 47 s, passed; the review's pinned warm reruns 26 s and 14 s |
| `witness-independence`, new step | `cargo test -p ferrata-bridge-core` in `bridge/` | not pinned: 24 s, passed; the review's pinned warm rerun 37 s |
| `links` (changed checks) | `check-row-tests.py`: a Rust test compiled out by a feature the command leaves off no longer counts (file, test or `mod` gates: the three `residue-experimental` files are refused); a cargo name filter is matched against the test's whole path (`--lib relayer` selects `cli/src/relayer.rs`); since the first review, a command that cannot fail its job no longer counts either (a step or job under an `if:` or with `continue-on-error`, `\|\| X` masking, `&`, a condition, a pipe without pipefail, a text without `set -e`, after an `exit`, `-- --skip`), nor a test that asserts an environment variable the command does not set | `test_ci_model.py` 88 passed; the review's 27 checker mutants all fail it |

## New tests (all run by `lez-harness`; build box, dev mode, at the commit named)

| Test | Rows | What it shows | Box result | Commit |
|---|---|---|---|---|
| `lez/harness/tests/validity_after_rotations.rs` | 021 R.3 | a header finalized on a chain whose committees the test holds, its execution payload the real mainnet block of `mainnet-attest-2026-11-10`; a storage slot and a log attested under its record (as a transaction's root and through the reference consumer); three committee rotations through the program (each checked, an update by the retired committee refused with 40); afterwards the record is unchanged byte for byte, the statements and identifiers are the same, the consumer's first claim under it credits once | 1 passed, 32 s | `cf645127` |
| `lez/harness/tests/statements_same_block.rs` | 021 R.11 | four private statement submissions (two record reads, a Transfer verified inside the transaction, a call of the program's attestation) from four accounts, proven against one pre-state, settled into one block (one block id, one opening fee state, one block fee summary, as LEZ v0.4.0-rc1's `apply.rs`) in both orders; each one's commitments and nullifiers as computed before inclusion; the control (one note spent twice) refused | 2 passed, 16 s | `cf645127` |
| `lez/harness/tests/attest_cli.rs` | 021 U.2 | the built `ferrata-attest`: attestations dry-run then sent and equal; a false predicate refused with 130 in the dry run, the scratch settlement and the execution; an update advanced and refused again with 52 as predicted; the configuration read; `config --send` a usage error | 3 passed, 121 s | `cf645127` |
| `lez/harness/tests/costs_attestation.rs` | 021 P.3, P.7, P.8, P.9 | update steps flat over five updates as records grow (P.9), per-update and per-finality-update totals and the proven path's charge (P.3), the pairing steps 6,955,700 of 9,228,905 (P.8), the accumulating accounts 365 B / 1,281-1,441 B / 505-585 B (P.7), one attestation 275,865 cycles before and after 1,000 more records (P.9) | 4 passed, 21.5 s | `dbc607aa` |
| `lez/harness/tests/device_workload.rs` | 021 P.4, 022 P.4 | the programs' private transitions of the private attestation, claim and burn in `lez/results/p4-minimum`'s segments and padded sizes (user cycles pinned at this tree's values); the claim's and burn's transactions at the recorded sizes to the byte, bare and padded; the attestation's 32 bytes larger than recorded | 3 passed, 25 s | `18d62935` |
| `scripts/privacy/check-results.py` (with `scripts/privacy/run.sh`) | 022 Priv.1, Priv.2, Priv.3, Priv.4, Priv.6 | the gate on every private default, the controls linkable, the adversary at chance on both legs, every hidden value found 0 times, recovery complete, `docs/privacy/generated/*` reproduced byte for byte, image ids included (only the wall-clock proving times normalised) | 33 of 33 on a second run; `--self-test` 27 altered cases refused (another image id among them), 1 accepted (proving times) | `18d62935` |
| `lez/harness/tests/captured_committee_exposure.rs` | 021 VS.4 | a record of a block Ethereum never finalized with ten invented deposits: claims mint until the per-token (255) and global caps refuse them, one more window's worth after the window rolls, nothing under the freeze (242), also under a second and a far-future fabricated record; minted total within cap per window | 1 passed, 38 s | `731db7f1` |
| `lez/harness/tests/state_growth_bridge.rs` | 022 P.7 | `root(cid, n)` 117 B, `nullifier(chain, vault, N)` 77 B, `burn(cid, H(note))` 185 B as the program writes them (the last two were estimates; equal), the same for a second claim and burn, no growth of the configuration or the token | 1 passed, 14 s | `d92a20be` |
| `lez/harness/tests/interrupted_operations.rs` | 022 R.7 | a private claim proven and dropped consumes nothing; a fresh client from the seed finds and claims the deposit once; the dropped proof sent late is refused and changes nothing; a dropped burn leaves the holding; a burn the client never recorded is found again from the burner seed by scanning the zone's blocks | 2 passed, 13 s | `39e872f7` |

Seven of the nine were drafted by GLM 5.3 workers (one task file each), reviewed, corrected where needed and run on
the box by the lead; `state_growth_bridge.rs` was written by the lead. Their commits carry both trailers.

## Rows, one by one

Status: the project's classification (`docs/requirements-status.md`); "proposed" where this branch's evidence
changes it. "CI test" names the test(s) a CI job now runs for the row; the map (`scripts/ci/row-tests.toml`) has the
full entries and notes.

### RFP-021

| Row | Status (now → proposed) | CI test (job) | Note |
|---|---|---|---|
| F.4 | in part (unchanged) | `attest_public.rs` (lez-harness) | `≠` and `≤` still not run through the program from a request |
| F.7 | waits on Logos (unchanged) | `private_consumer.rs`, `attest_public.rs` | names LEZ testnet 0.4 |
| F.8 | in part (unchanged) | `attest_private.rs`, `private_consumer.rs`, `vault_consumer.rs` | the two-deposit full-diff test is not built |
| F.11 | shown (unchanged) | `private_placement.rs`, `lez/updater/tests/recorded.rs` | |
| F.12 | in part (unchanged) | `lez/updater/tests/recorded.rs` | every_party.rs is on `rfp-docs-final` |
| F.13 | shown (unchanged) | `public_path.rs` | |
| F.15 | shown (unchanged) | `successor.rs` | |
| P.1 | shown (unchanged) | `attest_public.rs` | every kind one handler execution under `TARGET_GAS_EXEC` |
| P.2 | in part (unchanged) | `public_path.rs`, `attest_public.rs` | cycles and gas reproduced in CI; proof time and size need real proving |
| P.3 | in part (unchanged) | `costs_attestation.rs` | the inputs pinned; the amortised tables are derived in a first draft |
| P.4 | in part (unchanged) | `device_workload.rs`, `wallet_padding.rs` | not on a laptop; the attestation's transaction is 32 bytes larger than measured (finding 3) |
| P.7 | in part (unchanged) | `costs_attestation.rs` | the projection rests on an earlier image's sizes (finding 2) |
| P.8 | in part (unchanged) | `costs_attestation.rs` | the delta is a derived upper bound |
| P.9 | in part → **shown** | `costs_attestation.rs` | the missing clause, an attestation's cost against accumulated records, now measured: 275,865 cycles with 2 and with 1,002 record accounts |
| R.3 | not yet → **shown** | `validity_after_rotations.rs` | the committees are synthetic (no real committee signs test handoffs); the block, the program and LEZ's state machine are real; the lab's 38 real handoffs remain the run on live data |
| R.5 | shown (unchanged) | `lez/updater/tests/recorded.rs` | |
| R.8 | shown (unchanged) | `public_path.rs` | |
| R.9 | in part (unchanged) | `lez/updater/tests/recorded.rs` | the gate now runs over 246 updates (finding 4) |
| R.10 | in part (unchanged) | `lez/updater/tests/recorded.rs` | |
| R.11 | not yet → **in part** | `statements_same_block.rs` | shown in the harness, with the block assembled by a helper built from LEZ's settlement pieces (as `apply.rs`), not on a sequencer: the weaker status taken |
| S.3 | in part (unchanged) | `holdings.rs` | the guide is a first draft here |
| U.2 | in part (unchanged) | `attest_cli.rs` | against the in-process chain, not a sequencer |
| U.3 | in part (unchanged) | `lez/updater/tests/recorded.rs` | |
| U.6 | shown (unchanged) | `attest_public.rs`, `attest_native.rs`, `vault_consumer.rs` | |
| VS.3 | in part (unchanged here) | `immutable_program.rs` | stream G proposed shown once `rfp-docs-final` merges |
| VS.4 | in part (unchanged) | `captured_committee_exposure.rs` | the consumer-side bound now tested; trust.md is a first draft, the attestation side's monitor is not built |
| VS.6 | shown (unchanged) | `lez/updater/tests/recorded.rs` | |

Still without a test CI runs (the map's note says why): **P.5** (live latency), **P.6** (real proving on stated
hardware; the updater daemon's resources unmeasured), **S.1** (LEZ testnet 0.4 not released; public networks),
**S.5, S.6, VS.5** (documents; `scripts/check-guides.py` on `rfp-docs-final`), **S.7** (a document; what it
describes is tested elsewhere), **S.11** (`scripts/check-decisions.py` on `rfp-docs-final`), **U.1** (needs Nix and
a headless Logos Core; a Linux modules job was not attempted).

### RFP-022

| Row | Status (now → proposed) | CI test (job) | Note |
|---|---|---|---|
| F.4 | shown (unchanged) | `bridge_inbound.rs`, `bridge_private.rs` (lez-harness) | |
| F.5 | shown (unchanged) | `bridge_burn.rs` (lez-harness), `ferrata-bridge-core/tests/outbound.rs` (witness-independence) | |
| F.6 | shown (unchanged) | `bridge_burn.rs` | |
| F.16 | in part (unchanged) | `bridge_inbound.rs`, `tenant.rs`, `bridge_controls.rs` | the tenant limit stands |
| P.1 | shown (unchanged) | `bridge_inbound.rs` | each claim one public transaction under `TARGET_GAS_EXEC`; it rewrites `lez/results/bridge-inbound.md`, unchanged by the box run |
| P.2 | in part (unchanged) | `bridge_inbound.rs` | the per-deposit figure is derived |
| P.4 | in part (unchanged) | `device_workload.rs`, `wallet_padding.rs` | |
| P.7 | in part (unchanged) | `state_growth_bridge.rs` | all three sizes now measured; the 1M/10M figures are derived in a first draft |
| Priv.1 | shown (unchanged) | `bridge_private.rs`, `privacy_suite.rs`, `scripts/privacy/run.sh`, `check-results.py` | |
| Priv.2, Priv.3 | shown (unchanged; already counted as CI) | `check-results.py`, `PrivacySuite.t.sol` (now run) | |
| Priv.4 | in part (unchanged) | `privacy_suite.rs`, `run.sh`, `check-results.py` | the document of who handles what is a first draft |
| Priv.6 | in part (unchanged) | `check-results.py` | fields.md now reproduced from the code in CI (finding 1) |
| R.1 | shown (unchanged) | `bridge_inbound.rs`, `bridge_private.rs`, `ferrata-bridge-core/tests/inbound.rs` | |
| R.6 | in part (unchanged) | `monitor/tests/anvil-e2e.sh` (monitor), `cli/src/relayer.rs`, `cli/tests/gasdrop.rs` (roundtrip) | no relayer kill mid-checkpoint |
| R.7 | in part (unchanged) | `interrupted_operations.rs` | protocol level: a claim and a burn dropped between proof and sending consume nothing, a late send is refused, an unrecorded burn is recovered from the seed (CI); the test's client is written inside the test: the `ferrata` CLI and the Basecamp module are not killed between proof and sending, the deposit and release legs are not interrupted, and "nothing exposed" holds only because nothing was sent (review M1) |
| R.9 | in part (unchanged) | `monitor/tests/anvil-e2e.sh` | first-draft documents |
| R.11 | in part (unchanged) | the map itself (links) | 62 of 68 rows with a test CI runs; the six below have none |
| U.3 | in part (unchanged) | `monitor/tests/check.rs`, `anvil-e2e.sh` | |

Still without a test CI runs: **P.5** (live latency), **P.6** (provers on stated hardware; daemons' CPU and
bandwidth unmeasured), **R.14** (a second deployer from a fresh clone, on `rfp-docs-final`; needs a LEZ zone built
from source), **S.1** (testnets), **Sec.7** (documents; `check-guides.py` on `rfp-docs-final`), **S.4**
(`check-decisions.py` on `rfp-docs-final`).

## Findings for the integrator

1. **`docs/privacy/generated/` was stale.** The committed tables were of bridge image `cf41b7e5` (built at the
   `integration5` path); the code builds `ccde6171` (`docs/image-ids.md`), and its run differs (finer proof fields,
   the calldata's gas, a claim's gas step 4,335 instead of 4,720). Every verdict is the same. Regenerated here
   (`18d62935`), with a paragraph in `docs/privacy/RESULTS.md`. Of `docs/EVIDENCE.md`, only row 23 (line 100) is a
   current claim that names `cf41b7e5`: its corrected text is below; lines 404 and 472 are dated records of the fifth
   and sixth integration passes and rightly name that pass's image.
2. **The attestation side's size records are an earlier image's.** `lez/results/stage1-replay.md`,
   `stage1-finality.md` and `docs/performance.md` RFP-021 §P.7 give a 333 B header record, 1,249-1,409 B job
   accounts and a 425-505 B instance; this tree writes 365 B, 1,281-1,441 B and 505-585 B (`costs_attestation.rs`
   pins them). The one- and five-year projections should be redone from these. The stage tables are written by
   `cargo run -p harness --bin tables`, which no test runs.
3. **The P.4 measurements predate small program changes.** The programs' private transitions now take -734 to +576
   cycles from the recorded ones, inside the same segments and padded sizes; the private attestation's transaction
   is 32 bytes larger (231,246 bytes bare, 255,334 padded, against 231,214 and 255,302). The recorded proving times
   are of the smaller transaction; LEZ's circuit, which dominates them, cannot be measured in dev mode.
4. **The updater's gate now runs over more data.** `lez/updater/tests/recorded.rs`'s gate ran over 246 updates
   (1,101 rounds) on the box, refused-step gas 0.0752 of applied (gate ≤ 0.10), fee shares 0.4841 / 0.5159; the
   committed `lez/updater/results/gate.txt` (cited by R.9 and R.10) records 58 updates. Not committed here.
5. **`lez/results/attest-cli/run.txt` names instance `0x6d65…`**; the code derives `0x88df…` (the configuration's
   encoding changed). `attest_cli.rs` derives the id instead of copying it.
6. `cargo test` 1.98.1 on the build box did not give `CARGO_BIN_EXE_ferrata-attest` to the integration test (`cargo
   check` did); `attest_cli.rs` falls back to the binary beside its own executable.
7. RFP-021 R.12 ("CI green on the default branch") is shown today by runs of seven jobs; after this branch merges
   it needs a green run of all ten, and must not carry "shown" forward until that run exists. The new jobs' first run
   on a hosted runner is untested (estimate for `lez-harness`: 1.5 to 2.5 hours, inside its 240-minute limit).

## Proposed EVIDENCE rows

- **CI runs the LEZ suites and the watchers** (`rfp-ci-coverage`, 22 November 2026): jobs `lez-harness` (every
  harness and updater test file, dev mode, then the privacy suite and its gate) and `monitor` (tests and the anvil
  rehearsal); the roundtrip job runs the relayer's library and gas-drop tests, the witness-independence job the
  bridge core's tests. Build box, 4 CPUs, the job's script at `f28e4837`: harness 2,074 s, updater 1,136 s, privacy
  suite and gate 106 s (3,362 s after a warm build; the build cold, at `e3ff6240`, 622 s); every test binary
  passed, the gate 33 of 33; monitor 69 s + 131 s (cold). Rows with a test CI runs: RFP-021 50 of 59, RFP-022 62 of
  68 (`scripts/ci/check-row-tests.py --list`). Command: `scripts/ci/lez-harness.sh`. Not run on GitHub yet.
- **An attestation outlives committee rotations** (RFP-021 R.3): `validity_after_rotations.rs`, 1 of 1, build box,
  dev mode; synthetic committees, real mainnet block. Command: `cd lez && RISC0_DEV_MODE=1 cargo test -p harness
  --locked --test validity_after_rotations`.
- **Private statements in one block, either order** (RFP-021 R.11): `statements_same_block.rs`, 2 of 2.
- **The attestation CLI's dry runs and codes** (RFP-021 U.2): `attest_cli.rs`, 3 of 3 (121 s).
- **Attestation costs pinned** (RFP-021 P.3, P.7, P.8, P.9): `costs_attestation.rs`, 4 of 4; one attestation 275,865
  cycles with 2 and with 1,002 record accounts; record 365 B.
- **The P.4 proofs' workload held in CI (attestation +32 B)** (RFP-021 and RFP-022 P.4): `device_workload.rs`, 3 of 3; the
  attestation's transaction 32 bytes larger than measured.
- **Privacy gate in CI** (RFP-022 Priv.1-4, Priv.6): `scripts/privacy/check-results.py`, 33 of 33 on the
  regenerated tables (image `ccde6171`), reproduced byte for byte by a second run.
- **A captured committee's take bounded** (RFP-021 VS.4): `captured_committee_exposure.rs`, 1 of 1; caps 3 per token
  and 4 globally per window, 255 and 242 refusals.
- **Bridge state per operation measured** (RFP-022 P.7): `state_growth_bridge.rs`, 1 of 1; 117 / 77 / 185 bytes.
- **Interrupted operations** (RFP-022 R.7): `interrupted_operations.rs`, 2 of 2.

## Files outside this stream's own

`docs/privacy/generated/*` (regenerated) and `docs/privacy/RESULTS.md` (one paragraph, one figure), `docs/guides/ci.md`
(the new jobs, the path line, "Not in CI"), `lez/results/captured-committee-exposure.md` and
`lez/results/interrupted-operations-{claim,burn}.md` (records the new tests write). Base files of stream G edited:
`.github/workflows/ci.yml`, `scripts/ci/ci_model.py`, `scripts/ci/test_ci_model.py`, `scripts/ci/row-tests.toml`,
`scripts/ci/README.md`.

## Review round 1 (the first independent review, of `f28e4837`): each finding and its fix

| Finding | Fix | Commit |
|---|---|---|
| M1: RFP-022 R.7 overclaimed as shown | stays in part, with the reason (the test's client is written inside it; the CLI and the module are not killed between proof and sending; the deposit and release legs are not interrupted) | `0f7dcbf7` |
| L1: the P.7 and P.9 map notes contradict themselves | rewritten from `costs_attestation.rs`'s pins | `a35c96b9` |
| L2: the privacy gate's reason for ignoring image ids is false | image ids compared as they are (the guests build independent of the path); only the dev-mode proving times normalised; the self-test refuses another image id | `b4b9a6d5` |
| L3: the per-test and module feature-gate rule untested | `cargo_runs` takes a text; checks on every-test-gated, module-gated and `#[ignore]`d files | `8975d167` |
| L4: silent non-runs and masked failures counted | guards for `if:`, `continue-on-error`, `\|\| X`, `&`, conditions; top-level and argument-guarded `exit` (a build-only job runs no test); `-- --skip`; assert-form environment gates; then pipes without pipefail and texts without `set -e`; the lez-harness job's last step pinned. The review's 27 checker mutants all fail `test_ci_model.py` | `8975d167`, `104dbecc`, `50d59888` |
| L5: unpinned times labelled as 4-thread | labelled "not pinned", with the review's pinned reruns beside them | `0f7dcbf7` |
| L6: the proposed EVIDENCE row mixes commits | the tip's figures (`f28e4837`) | `0f7dcbf7` |
| L7: the P.4 EVIDENCE title overstates | "The P.4 proofs' workload held in CI (attestation +32 B)" | `0f7dcbf7` |
| L8: finding 1 inexact | only EVIDENCE row 23 is a current claim to update | `0f7dcbf7` |
| L9: `state_growth_bridge.rs`'s header claims an import is measured | the header says what is measured | `19303ddc` |
| L10: no workflow-level permissions | `permissions: contents: read` | `19303ddc` |
| Stale figures (records 365 B, updater gate 246 updates, privacy image, P.4, attest-cli) | the corrected text below, for the integrator (those files changed on merged streams) | this section's commit |

## Corrected text for the integrator (stale figures this branch measured)

The tests CI now runs on this branch measured figures that several committed documents still give
from earlier program builds. Those documents belong to other streams, so this branch does not edit
them; the corrected text is collected here for the integrator to apply. Each item quotes the current
text exactly ("Now"), gives the replacement ("Corrected"; everything not shown changed stays word
for word), and names where the new figure comes from ("Source"). Line numbers are of this worktree,
22 November 2026. Inside the quotes, link targets and relative paths are those of the file being
corrected, not of this one, and sentences that wrap in the document are shown joined.

Where the figures come from (this branch's measurements and their records):

| Fact | Source |
|---|---|
| F1 | `lez/harness/tests/costs_attestation.rs` (CI, job `lez-harness`): RISC Zero dev mode through LEZ v0.4.0-rc1's state machine; its pinned constants name every figure and the committed printouts they cite |
| F2 | the §P.7 recomputation by the document's own method; the arithmetic is shown in the item |
| F3 | `cargo run -p harness --bin tables` on the build box, dev mode, at this branch |
| F4 | `scripts/privacy/check-results.py` (CI, job `lez-harness`) on the regenerated `docs/privacy/generated/` of bridge image `ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd`; `docs/privacy/generated/summary.md`; `docs/privacy/RESULTS.md`, its 22 November paragraph |
| F5 | `lez/harness/tests/device_workload.rs` (CI): the programs' private transitions and transaction sizes against `lez/results/p4-minimum`'s recorded ones |
| F6 | CI's updater gate, `lez/updater/tests/recorded.rs` (job `lez-harness`), which writes `lez/updater/results/gate.txt`; two box runs gave the same bytes |
| F7 | `lez/harness/tests/attest_cli.rs` (CI): the instance id this code derives, against `lez/results/attest-cli/run.txt` |

### docs/performance.md

Line 74 (§P.2):

Now:

```
| finality update, public job | 3 | `Open` 2.13–2.18M, `Miller` 3.31M, `Finish` 3.65M | 9,216,121 | 4,451 | none |
```

Corrected:

```
| finality update, public job | 3 | `Open` 2.13–2.18M, `Miller` 3.31M, `Finish` 3.65M | 9,228,905 | 4,451 | none |
```

The per-step figures still round as written (`Open` 2,171,611, `Miller` 3,308,048, `Finish`
3,647,652; the storage gas is unchanged). Source: F1.

Line 75 (§P.2):

Now:

```
| update that rotates and adopts a committee | 5 | adoption 3.40–3.42M per 256 keys | 16,076,120 | 54,491 | none |
```

Corrected:

```
| update that rotates and adopts a committee | 5 | adoption 3.41–3.42M per 256 keys | 16,092,355 | 54,491 | none |
```

The two `Adopt` steps are 3,410,995 and 3,418,189 cycles (3.41–3.42M); the total is update_1's, the
row's update; storage gas unchanged. Source: F1.

Line 76 (§P.2): unchanged. The one-public-transaction (`VerifyAll`) row has no re-measurement on
this branch to rest a new figure on — `costs_attestation.rs` pins the three-transaction job, the
adopting job and the proven path, not the one-transaction form — so 9,115,325 stands until re-run.
The `Init` row above it (line 73) is in the same position: the test runs `Init` but pins none of its
figures.

Line 77 (§P.2):

Now:

```
| finality update, proven path | 1 | public part 70,090 | 479,854 | 224,063 | 9,045,235 user cycles: **3,240.5 s** laptop CPU, peak 9.18 GiB, receipt **227,080 B** |
```

Corrected:

```
| finality update, proven path | 1 | public part 75,534 | 485,298 | 224,063 | 9,045,235 user cycles: **3,240.5 s** laptop CPU, peak 9.18 GiB, receipt **227,080 B** |
```

The last column is the real proof of image `e7cec90e…`, a dated record, and stays; the charge is the
current build's (`PRIVATE_VERIFY_GAS` 409,764 + the public part's 75,534 cycles). Source: F1.

Lines 110–113 (§P.2) — not on the task's list, but the same fact and the same recorded sizes: the
statement-verification paragraph's private attestation is the earlier build's too.

Now (the sentence wraps over lines 110–113):

```
**Proof time and size of an attestation**: the private attestation of a `Transfer`, 380,577 user cycles for the consumer's two transitions plus LEZ's privacy circuit, proven on one L4 in **16.1 s**, receipt **227,170 B**, transaction 231,214 B, peak host memory 0.76 GiB ([`lez/RESULTS.md`](../lez/RESULTS.md) §5; [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md), run `gpu-l4-attest-2`).
```

Corrected — the same sentence with this appended after it:

```
On this tree's programs the two transitions take 381,480 user cycles and the transaction is 231,246 bytes, 32 larger (`lez/harness/tests/device_workload.rs`, CI).
```

Source: F5 (34,337 + 346,240 → 34,717 + 346,763 user cycles; 231,214 → 231,246 bytes).

Line 126 (§P.3; the sentence wraps, the link at its end is unchanged):

Now:

```
Per update: public 9.2M execution gas (16.1M at a handoff), proven 0.70M with execution and storage gas added
```

Corrected:

```
Per update: public 9.2M execution gas (16.1M at a handoff), proven 0.71M with execution and storage gas added
```

Public 9.2M (9,228,905) and 16.1M at a handoff (16,092,355) still round as written; the proven
update now costs 485,298 + 224,063 = 709,361, so 0.71M. Source: F1.

Lines 132–135 (§P.3's amortised table; the task named the first two rows, the two hourly rows move
with the same inputs):

Now:

```
| once per period (a handoff) | public | 16,076,120 per period, about 5.16 billion a year | 54,491 per period |
| once per period (handoff, no new header) | proven | 448,530 per period | 224,063 per period |
| once per hour while importing | public | 221,186,904 per day | 106,824 per day |
| once per hour while importing | proven | 11,516,496 per day | 5,377,512 per day |
```

Corrected:

```
| once per period (a handoff) | public | 16,092,355 per period, about 5.17 billion a year | 54,491 per period |
| once per period (handoff, no new header) | proven | 451,895 per period | 224,063 per period |
| once per hour while importing | public | 221,493,720 per day | 106,824 per day |
| once per hour while importing | proven | 11,647,152 per day | 5,377,512 per day |
```

Derived as the document derives them (the document's method is a plain product; see §P.7 below for
the arithmetic shown in full): 16,092,355 × 321 = 5,165,645,955 ≈ 5.17 billion; 24 × 9,228,905 =
221,493,720; 24 × 485,298 = 11,647,152. Every storage-gas cell is unchanged (24 × 4,451; 24 ×
224,063). Source: F1.

Lines 157–158 (§P.4): the note the task asks for, inserted between "(9.18 GiB peak)." and
"**Gaps:**" (the surrounding text wraps; shown joined).

Now:

```
(9.18 GiB peak). **Gaps:** a mid-range laptop; a chain of hops (a 320-hop claim is estimated at about 58M accelerated user cycles).
```

Corrected:

```
(9.18 GiB peak). **The programs have changed slightly since these runs**, and CI now pins their shape (`lez/harness/tests/device_workload.rs`): the private attestation's transaction is 231,246 bytes bare and 255,334 padded, 32 bytes larger than the 231,214 and 255,302 proven above, the claim's and the burn's are the recorded sizes to the byte, and the programs' private user cycles moved by -734 to +576 inside the same segments and padded sizes; LEZ's circuit, 85–96 % of the proving time, cannot be measured in dev mode, so the recorded times are of the slightly smaller programs. **Gaps:** a mid-range laptop; a chain of hops (a 320-hop claim is estimated at about 58M accelerated user cycles).
```

Source: F5.

Line 185 (§P.7):

Now:

```
| `header(id, slot)` record | 333 B of actor state (293 before the chain id and parent hash) | one per update that finalizes a new header | [`lez/RESULTS.md`](../lez/RESULTS.md) §5, "Decisions" 2 |
```

Corrected:

```
| `header(id, slot)` record | 365 B of actor state (333 before the root id the successors merge added) | one per update that finalizes a new header | [`lez/RESULTS.md`](../lez/RESULTS.md) §5, "Decisions" 2 |
```

The 289-byte record encoding that "Decisions" 2 names is unchanged; the actor state grew by the
root id. Source: F1 (`costs_attestation.rs` pins 365 B), with `lez/RESULTS.md`'s opening note ("365
bytes of state, not 333").

Line 186 (§P.7):

Now:

```
| `instance(id)` | 425–505 B | constant (committees as 80-byte summaries) | [`attestation.md`](design/attestation.md) §3.1 |
```

Corrected:

```
| `instance(id)` | 505–585 B | constant (committees as 80-byte summaries) | [`attestation.md`](design/attestation.md) §3.1 |
```

Source: F1 (505 B as `Init` leaves it, 585 B once an update supplies the next committee's summary).

Line 187 (§P.7):

Now:

```
| `job(id, job_id)` | 1,249–1,409 B | cleared by `Finish`; a failed job stays (clean-up not built) | [`attestation.md`](design/attestation.md) §3.1, §4.5 |
```

Corrected:

```
| `job(id, job_id)` | 1,281–1,441 B | cleared by `Finish`; a failed job stays (clean-up not built) | [`attestation.md`](design/attestation.md) §3.1, §4.5 |
```

Source: F1 (1,281 B after `JobOpen`, 1,377 after `Miller`, 1,409 and 1,441 after the two `Adopt`s).

Lines 189–190 (§P.7's projection; the sentence wraps, shown joined).

Now:

```
plus one per period (321): about 9,081 records, **3.0 MB a year, 15.1 MB over five years** of record state.
```

Corrected:

```
plus one per period (321): about 9,081 records, **3.3 MB a year, 16.6 MB over five years** of record state.
```

The document's method is a plain product of its own record counts and the record size, and it
reproduces the old figures exactly (9,081 × 333 = 3,023,973 ≈ 3.0 MB; five years 15,119,865 ≈
15.1 MB), so the same method with the measured 365 B gives the new ones:

```
$ python3 -I -c 'print(9081*365); print(9081*365/1e6); print(5*9081*365); print(5*9081*365/1e6)'
3314565
3.314565
16572825
16.572825
```

≈ 3.3 MB a year and ≈ 16.6 MB over five years — the review's arithmetic, confirmed against the
document's own method. The record counts (8,760 + 321 = 9,081) are unchanged. Source: F1, F2.

Lines 196–199 (§P.8): unchanged — "7.0M of 9.2M" still rounds the same. The sentence's measured
inputs are now `Miller` 3,308,048 (3.31M) and `Finish` 3,647,652 (3.65M), together 6,955,700 (7.0M)
of the finality update's 9,228,905 (9.2M); the pairing is 75.4% of the update (43.3% of an adopting
one, 6,967,210 of 16,092,355). Not a word changes. Source: F1.

Line 201 (§P.8, the end of the same paragraph; the sentence wraps, shown joined):

Now:

```
**A precompile is reachable only from public transactions**: the proven path, which runs in a private actor, would not benefit, and it already costs 0.48M execution gas.
```

Corrected:

```
**A precompile is reachable only from public transactions**: the proven path, which runs in a private actor, would not benefit, and it already costs 0.49M execution gas.
```

(485,298 rounds to 0.49M.) Source: F1.

### docs/design/attestation.md

Line 45 (§3) labels the figures of §3 and §4 as image `e7cec90e…`'s and those of §5 to §7 as
`2f3566d4…`'s. Those passages keep their numbers and get a one-sentence pointer to the current
figures, which CI pins; §10's tables and §13's P.7 row are not labelled by image and take the new
numbers.

Line 55 (§3.1) — after the `id` line, a pointer for the actor table above it:

Now:

```
`id = H(config ‖ checkpoint)`; `job_id = H(id ‖ store digest ‖ update digest)`.
```

Corrected — the same line, then a new paragraph:

```
`id = H(config ‖ checkpoint)`; `job_id = H(id ‖ store digest ‖ update digest)`.

The current programs write larger accounts, which CI pins (`lez/harness/tests/costs_attestation.rs`): `instance(id)` 505–585 bytes, `job(id, job_id)` 1,281–1,441 bytes for a job that adopts a committee and 1,022–1,118 for a finality job, and `header(id, slot)` 365 bytes, the record having gained the root id (`lez/RESULTS.md`'s opening note: '365 bytes of state, not 333').
```

Source: F1.

Line 131 (§4.2) — before the paragraph under the measured table, a pointer for the table above it
(the table itself keeps its labelled numbers):

Now:

```
Execution gas is the user cycles LEZ metered, storage gas the transaction's serialized size. After every job the instance's store equals the native `FinalizedStore`'s, every finalized root equals the canonical root a second provider reports, and the record equals the native finalized header's.
```

Corrected — a new paragraph first, then that paragraph unchanged:

```
The current programs charge a little more for the same updates, which CI pins (`lez/harness/tests/costs_attestation.rs`): a finality update 9,228,905 execution gas (4,451 storage gas), update_1's rotation and adoption 16,092,355 (54,491), update_0's committee-only job 15,997,954 (53,675), the proven path 485,298 with a 75,534-cycle public part, and a proven handoff 451,895 with a 42,131-cycle public part.

Execution gas is the user cycles LEZ metered, storage gas the transaction's serialized size. After every job the instance's store equals the native `FinalizedStore`'s, every finalized root equals the canonical root a second provider reports, and the record equals the native finalized header's.
```

Source: F1.

Line 288 (§6.2, the sentence is one line in the file):

Now:

```
A private consumer therefore sends one fixed-size `Read` to a write-once record, receives it in a `StateReply` (333 bytes of state; 293 before the record carried the chain id and the parent hash), and runs `ferrata-core` inside its own proof.
```

Corrected:

```
A private consumer therefore sends one fixed-size `Read` to a write-once record, receives it in a `StateReply` (333 bytes of state; 293 before the record carried the chain id and the parent hash; the current programs' record also carries the root id and is 365 bytes, `lez/harness/tests/costs_attestation.rs`), and runs `ferrata-core` inside its own proof.
```

Source: F1.

Lines 358–361 (§10's per-update table), numbers only:

Now:

```
| Finality update | public job, 3 transactions | 9,216,121 | 4,451 | in-process harness |
| Handoff that rotates and adopts a committee | public job, 5 transactions | 16,076,120 | 54,491 | in-process harness |
| Finality update | proven, 1 transaction | 479,854 | 224,063 | in-process harness; the proof made on the laptop CPU in 3,240.5 s |
| Handoff with no new header | proven, 1 transaction | 448,530 | 224,063 | in-process harness, dev mode (no real proof) |
```

Corrected:

```
| Finality update | public job, 3 transactions | 9,228,905 | 4,451 | in-process harness |
| Handoff that rotates and adopts a committee | public job, 5 transactions | 16,092,355 | 54,491 | in-process harness |
| Finality update | proven, 1 transaction | 485,298 | 224,063 | in-process harness; the real proof, of an earlier image's update (479,854), made on the laptop CPU in 3,240.5 s |
| Handoff with no new header | proven, 1 transaction | 451,895 | 224,063 | in-process harness, dev mode (no real proof) |
```

Source: F1.

Line 363 (§10):

Now:

```
A proven finality update costs 0.70M gas with execution and storage gas added together.
```

Corrected:

```
A proven finality update costs 0.71M gas with execution and storage gas added together.
```

(485,298 + 224,063 = 709,361; the sentence after it is unchanged.) Source: F1.

Lines 369–372 (§10's amortised table; the task's range reaches the first rows, the two hourly rows
move with the same inputs):

Now:

```
| Once per period (one handoff) | public | 16,076,120 per period; about 5.16 billion per year | 54,491 per period |
| Once per period (one handoff, no new header) | proven | 448,530 per period | 224,063 per period |
| Once per hour while roots are imported | public | 221,186,904 per day of importing | 106,824 per day of importing |
| Once per hour while roots are imported | proven | 11,516,496 per day of importing | 5,377,512 per day of importing |
```

Corrected:

```
| Once per period (one handoff) | public | 16,092,355 per period; about 5.17 billion per year | 54,491 per period |
| Once per period (one handoff, no new header) | proven | 451,895 per period | 224,063 per period |
| Once per hour while roots are imported | public | 221,493,720 per day of importing | 106,824 per day of importing |
| Once per hour while roots are imported | proven | 11,647,152 per day of importing | 5,377,512 per day of importing |
```

Derived as in §P.3 above; every storage-gas cell unchanged. Source: F1.

Line 438 (§13's P.7 row):

Now:

```
| P.7 state growth | record 333 bytes (293 before the chain id and parent hash), job 1,249–1,409, instance 425–505 (in-process harness); committee storage constant | projections over one and five years |
```

Corrected:

```
| P.7 state growth | record 365 bytes (333 before the root id), job 1,281–1,441, instance 505–585 (in-process harness; pinned by `lez/harness/tests/costs_attestation.rs` in CI); committee storage constant | projections over one and five years |
```

Source: F1.

### lez/RESULTS.md

Line 122 (§1's actor table):

Now:

```
| `header(id, slot)` | `H(…/header ‖ id ‖ slot)` | the record of a finalized header (the instance's chain id; the beacon header, execution root, block number, block hash, parent hash, state and receipts roots, timestamp), written once; 333 bytes | `Record` |
```

Corrected:

```
| `header(id, slot)` | `H(…/header ‖ id ‖ slot)` | the record of a finalized header (the instance's chain id and root id; the beacon header, execution root, block number, block hash, parent hash, state and receipts roots, timestamp), written once; 365 bytes | `Record` |
```

Source: F1 (365 B, pinned by `costs_attestation.rs`), with the file's own line 45 ("a record carries
the instance's root id (365 bytes of state, not 333)"). One further row of the same table is not on
the task's list but carries the earlier build's figures too: line 120's `instance(id)` sizes ("433
bytes, 513 with a next committee"); CI pins 505 as `Init` leaves it and 585 with a next committee,
for the integrator to align if they agree (F1).

Line 45 already says 365 and is left as it is.

### lez/results/ (the generated stage tables)

`stage1-replay.md`, `stage1-finality.md` and the other tables are generated; correct them by
regenerating, not by hand. The command, from `lez/`, RISC Zero dev mode:

```
cargo run -p harness --bin tables
```

What changes on this branch (the lead's run on the build box; F3): 15 of the generated tables, 541
lines. `images.md` then names program
`816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df` (627,064 B ELF, against the
committed `9f3eb00f…`, 602,912 B) and consumer `8464603b…` (471,408 B). In `stage1-finality.md` the
`Record` step becomes 17,350 cycles with a 365-byte post-state (was 16,533 and 333), and in
`stage1-replay.md` update_0's `Miller` becomes 3,313,372 cycles (was 3,313,287) — both also pinned
by `costs_attestation.rs` (F1). The committed rows being replaced:

```
| update_1 as finality update Finish (step 3) | Finish 3,647,613 → Apply 58,904 → Record 16,533 | 3,723,050 | 74% | 527 | 29,788,616 | applied |
| 5 | update_0 Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
```

Source: F3.

### docs/EVIDENCE.md

The four P.4 rows (32, 38, 50, 88) share one note, so it is given once here (F5); a row is
corrected by appending it at the end of its "what it shows" column, after the clause quoted in each
item:

```
The programs measured here have changed slightly since: this tree's private attestation transaction is 231,246 B bare and 255,334 B padded, 32 bytes larger than the one proven in this row, the claim's and the burn's are the recorded sizes to the byte, and the programs' private user cycles moved by -734 to +576 inside the same segments and padded sizes (`lez/harness/tests/device_workload.rs`, CI); LEZ's circuit, 85–96 % of the proving time, cannot be measured in dev mode, so the times above are of the slightly smaller programs.
```

The rows that are dated records of earlier passes stay as they are: line 74 (row 80, the 58-update
gate runs of 12–13 November; its link is to `lez/updater/results/`, a directory, so it still
resolves when `gate.txt` holds the new content), line 404 (the fifth integration pass) and line 472
(the privacy suite's sixth pass) rightly name their own pass's image `cf41b7e5…5390`.

Row 32 (line 52) — the column ends with:

Now:

```
peak 9.25 GiB; 6 of 6 receipts verified by LEZ's settlement, a flipped bit refused
```

Corrected: that clause, then the P.4 note. Source: F5.

Row 56 (line 56):

Now:

```
`config`: the instance's configuration read from the zone, its id derived again and equal
```

Corrected:

```
`config`: the instance's configuration read from the zone, its id derived again and equal (this code derives `0x88df8428…64c358`, as `lez/harness/tests/attest_cli.rs` derives and asserts in CI; the committed `results/attest-cli/run.txt` of that run names `0x6d65e4ef…f6b4`, the id of the earlier configuration encoding)
```

In full: `0x88df84285312923d6be0d04a7cd95a0ec13d61dececcdbfb26d7675ebd64c358` (derived) and
`0x6d65e4ef2af67eed1ce3a5c2d7b04fa02517707d2dff044869389f310b69f6b4` (`run.txt`). The rest of the
row is the record of the run and is unchanged. Source: F7.

Row 23 (line 100) — the row's current claim names image `cf41b7e5…5390` and predates CI's gate.

Now:

```
| 23 | The privacy gate (milestone B2): 20 users' round trip, every public byte of both chains, an adversary pairing each side | adversary at chance (5.0%) on private claims and private burns, 100% on the plaintext claim and on the naive nullifiers (`N = fid`, `NOut = burnId`); private claims (8,937 B), burns (7,857 B) and releases (324 B of calldata) identical once nullifiers, fresh randomness, own addresses and time are cut; 0 of 320 hidden values found in public bytes; from the seed alone 10 of 10 unclaimed deposits and 10 of 10 unreleased burns recovered, the released ones seen as released; on the merged programs (image `cf41b7e5…5390`); seven public fields §B6 as first drafted did not list (now listed) | in-process harness (dev mode) and local EVM (mock verifier), build box; **stand-in** Ethereum records on LEZ | none (dev mode, mock verifier) | `scripts/privacy/run.sh` | [`docs/privacy/RESULTS.md`](privacy/RESULTS.md) |
```

Corrected:

```
| 23 | The privacy gate (milestone B2): 20 users' round trip, every public byte of both chains, an adversary pairing each side | adversary at chance (5.0%) on private claims and private burns, 0 of 20 exactly identified on each leg; 100% on the plaintext claim and on the naive nullifiers (`N = fid`, `NOut = burnId`, 20 of 20 each); private claims (8,937 B), burns (7,857 B) and releases (324 B of calldata) identical once nullifiers, fresh randomness, own addresses and time are cut; 0 of 320 hidden values found in public bytes; from the seed alone 10 of 10 unclaimed deposits and 10 of 10 unreleased burns recovered, the released ones seen as released; on the merged programs (image `ccde6171…71dd`, the deployed one of `docs/image-ids.md`); the CI gate `scripts/privacy/check-results.py` passes 33 of 33 on these tables and a second run reproduced them byte for byte (the dev-mode proving times aside); seven public fields §B6 as first drafted did not list (now listed) | in-process harness (dev mode) and local EVM (mock verifier), build box; **stand-in** Ethereum records on LEZ | none (dev mode, mock verifier) | `scripts/privacy/run.sh`, then `python3 -I scripts/privacy/check-results.py` | [`docs/privacy/RESULTS.md`](privacy/RESULTS.md) |
```

Source: F4 — `scripts/privacy/check-results.py` on the regenerated tables of image
`ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd`: 33 of 33 checks, the adversary
at chance (5.0%) on the private claim and private burn legs with 0 exactly identified, the controls
(naive nullifier, naive `NOut`, plaintext claim) at 20 of 20, 0 hidden values found, recovery from
the seed complete; every figure is in `docs/privacy/generated/summary.md`, and the byte-for-byte
second run in `docs/privacy/RESULTS.md`'s 22 November paragraph.

Row 38 (line 116) — the column ends with:

Now:

```
Dev mode: each padded transaction's private part equals the wallet's field for field (7 actions of 1,704 B, 516-byte ciphertexts, one 1,258-byte recovery note, refund to the payer's alias)
```

Corrected: that clause, then the P.4 note. Source: F5.

Row 88 (line 117) — the column ends with:

Now:

```
On the CPU only: r0vm 3.0.5 compiles its Metal provers out, and the GPU stayed at idle residency (`powermetrics`)
```

Corrected: that clause, then the P.4 note. Source: F5.

Row 50 (line 147) — the column ends with:

Now:

```
all under M0's 20 min and 16 GB (earlier medians 633.3, 600.7, 525.9 s on an older AMD generation)
```

Corrected: that clause, then the P.4 note. Source: F5.

### lez/updater/README.md

Lines 226–232 (the close of the client-diversity paragraph; the text wraps, shown joined).

Now:

```
The lab's record has grown since 13 November (mainnet 95, Sepolia 125 updates): `restart_mid_job_and_mid_period` now picks its stop point from the data (it had assumed the third entry from the end was a finality update), and the A3 gate's default run replays all **220** updates, each applied once, refused gas / applied 0.0603, honest-1's share 0.5027, 1,020 rounds (`results/client-diversity-2026-11-18/suite/gate-default.txt`; the table below keeps the 58-update runs). Running the suite rewrites the tracked `results/gate.txt` with the grown record's figures; `git checkout` it afterwards to keep the 58-update file the README and EVIDENCE row 80 cite.
```

Corrected:

```
The lab's record has grown since 13 November (mainnet 108, Sepolia 138 updates): `restart_mid_job_and_mid_period` now picks its stop point from the data (it had assumed the third entry from the end was a finality update), and the A3 gate's default run replays all **246** updates, each applied once, refused gas / applied 0.0752, honest-1's share 0.4841, 1,101 rounds (`results/gate.txt`, which CI's `lez/updater/tests/recorded.rs` writes; the table below keeps the 58-update runs). Running the suite rewrites the tracked `results/gate.txt` with the record's figures.
```

With the grown `gate.txt` committed (below), the instruction to check it out again goes; the
58-update table that follows in the README stays a dated record, as does EVIDENCE row 80. Source:
F6.

### lez/updater/results/gate.txt

The whole file is the place. Now (tab-separated, as committed; line 1 ends with the trailing tab of
its empty last field; the 58-update runs):

```
gate	rounds	437	seed	default	adversary	true	salt	
mainnet	updates applied	14	jobs (with Init)	15	finalized slot	15631808
sepolia	updates applied	44	jobs (with Init)	45	finalized slot	11556000
honest-1	applied tx	45	refused tx	18	gas applied	138600059	gas refused	13737300	fee	1218698872	deferrals	428
honest-2	applied tx	56	refused tx	14	gas applied	179335282	gas refused	12321836	fee	1533256944	deferrals	421
adversary	applied tx	149	refused tx	17	gas applied	467823409	gas refused	19172549	fee	3895967664	deferrals	0
honest refused gas / applied gas	0.0820	(gate <= 0.10)
honest-1 share of honest fees	0.4428	honest-2	0.5572	(gate 0.35-0.65 each)
updates	58	adversary junk Opens	110	ledger transactions	409
```

Corrected — the new file's full content (tab-separated, the same format, line 1's trailing tab
included; two box runs gave these bytes):

```
gate	rounds	1101	seed	default	adversary	true	salt	
mainnet	updates applied	108	jobs (with Init)	109	finalized slot	15684000
sepolia	updates applied	138	jobs (with Init)	139	finalized slot	11608192
honest-1	applied tx	174	refused tx	52	gas applied	535999073	gas refused	38013397	fee	4592099760	deferrals	1404
honest-2	applied tx	183	refused tx	56	gas applied	566718210	gas refused	44923130	fee	4893130720	deferrals	1391
adversary	applied tx	483	refused tx	62	gas applied	1506035027	gas refused	57731469	fee	12510131968	deferrals	0
honest refused gas / applied gas	0.0752	(gate <= 0.10)
honest-1 share of honest fees	0.4841	honest-2	0.5159	(gate 0.35-0.65 each)
updates	246	adversary junk Opens	486	ledger transactions	1496
```

The gate's own bounds hold: refused / applied 0.0752 ≤ 0.10, the fee shares 0.4841 and 0.5159 inside
0.35–0.65. Source: F6 — `lez/updater/tests/recorded.rs` (CI, job `lez-harness`) writes the file.

### Left as they are, and why

`docs/performance.md` line 73 (`Init`) and line 76 (`VerifyAll`): no re-measurement on this branch
backs a new figure (see the note at line 76 above); the real-proof figures in line 77's last column
and the P.4 rows' times are dated records of earlier images. `docs/design/attestation.md` line 45
is the image label itself. `docs/EVIDENCE.md` lines 74, 404 and 472 and `lez/updater/README.md`'s
58-update table are dated records of their own runs. `lez/RESULTS.md` line 45 is already correct
and line 120 is flagged under line 122 above.
