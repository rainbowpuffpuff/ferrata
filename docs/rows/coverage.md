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
| `links` (changed checks) | `check-row-tests.py`: a Rust test compiled out by a feature the command leaves off no longer counts (file, test or `mod` gates: the three `residue-experimental` files are refused); a cargo name filter is matched against the test's whole path (`--lib relayer` selects `cli/src/relayer.rs`) | `test_ci_model.py` 59 passed |

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
7. RFP-021 R.12 ("CI green on the default branch") will depend on the two new jobs passing on GitHub; their first
   run on a hosted runner is untested (estimate: `lez-harness` one to two hours, inside its 240-minute limit).

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
