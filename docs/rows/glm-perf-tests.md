# Stream glm-perf-tests: P.5's and P.6's figures recomputed in CI, and the operator guide's stall detection

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. On
`glm-perf-tests` from `8c38d723`: `2486a015` (the test and its wiring), `ed46b562` (the operator guide §3.5), then
this file. Checks run on the laptop after each: `python3 -I scripts/ci/check-row-tests.py`,
`python3 -I scripts/ci/test_ci_model.py` (88 passed), `python3 -I scripts/test_perf_records.py` (15 passed),
`python3 -I scripts/check-guides.py` (0 findings), `python3 -I scripts/check-links.py README.md docs
cli/README.md monitor/README.md lez/ATTEST-CLI.md lez/updater/README.md e2e/seqnet/README.md` (111 files, 2376
links, 0 broken). Nothing was pushed or sent. No code outside `scripts/` and `.github/workflows/ci.yml` changed; no
program, guest or image id changed; no dependency changed.

## What changed

- **RFP-022 P.5, P.6 (a CI test each).** `scripts/test_perf_records.py`, run by CI's `links` job, recomputes the
  documented performance figures from the committed raw records and asserts each equals the documented value at
  the documented rounding and that the document states it (the exact string). P.5: `e2e/runs/crown-live-3/`'s
  `stages.done`, `captures/capture-2.json` (+ its `block.json`) and `lez/session-b/logs/driver.log`, plus the
  Sepolia constants for the one *derived* figure — inbound 32.9 min (captured 25.2, finality 18.6, availability
  6.6, steady 29.8, hold-derived 1.1–12.1 h), outbound 71.0 min from the burn's proving (66.3 from its sending,
  66.2 from it final on LEZ), W 3,607 s (the row's 60.1 min), the claim 231.9 + 8.3 s, the burn 282.8 + 7.0 s,
  LEZ steps 39.6 s, verification 37 s, the checkpoint's proofs 167 s, the release stage 156 s. P.6:
  `bench/perf/results/daemons-anvil/` (GNU time, 5 s samples, RPC byte counts, phases) for `monitor` (0.53 CPU-s
  in 1,284 s; 0.0004 of a core; 14.0 MiB peak, median 14.0; 86,265 / 254,218 B; 36 CPU-s and 22.9 MB a day) and
  the relayer (0.87 CPU-s; 0.0007; 20.3 MiB peak, median 20.0; 408,005 / 975,682 B; 59 CPU-s and 93.1 MB a day),
  both rows' per-phase cells; `gpu-prover/results/*/run.json` for the four L4 totals (14.0 / 24.1 / 41.4 / 219.3 s,
  GPU memory 11,076–11,159 MiB, host 0.76–1.08 GiB) and the rule (B) 31.9 s (n = 4, 9 segments);
  `checkpoint/results/prove.jsonl` + the GNU-time logs for 1,157.5 / 1,758.9 s and 9.62 GB;
  `checkpoint/results/groth16.jsonl` for 195.1 / 215.2 s; `checkpoint/gpu/run.json` + `exec.txt` + the stage
  stamps for the live run's L40S 31.9 s prover wall (119 s with the queue; 18,499,940 user cycles, 20 segments,
  blocks 25..=90, 2 burns).
- **Three figures of P.6 are not recomputable from committed data** and are asserted nowhere; each is named in a
  comment at its place in the test: the GPU cost row's "385 L4-seconds billed … about 0.09 USD" (Modal's own
  billing statement; only the 366 s the functions themselves ran recomputes, asserted), the Groth16 wrap's
  "7.3 GiB" of prover memory (the Docker container's `VmHWM`, polled live; `groth16.jsonl` records wall times
  only), and the wrap's "recursion 18.7 s on 16 cores + 14 s Groth16" (an end-to-end run cited to
  `docs/design/bridge.md` §18.7, whose raw per-step timings are not committed). No recomputed figure disagreed
  with its document.
- **The wiring.** A `links` job step runs the test; `scripts/ci/row-tests.toml` maps it to RFP-022 P.5 and P.6
  (whose notes keep the reason no CI test *measures* the latency or the resources themselves), so
  `check-row-tests.py` counts both rows with a test CI runs: **RFP-022 66 of 68** (were 64), 2 without (R.14 and
  S.1, box or local tests only; were 4). `test_ci_model.py`'s two count checks are updated with the map, as its
  own comment directs.
- **RFP-021 S.7 (the operator guide).** §3.5 now describes stall detection with `attest-monitor` beside the
  updater: the `instance-stalled` (warn, `--stall-after` default 15,300 s), `ws-half` and `ws-expired` (critical)
  and `recovered` (info) alarms with their thresholds, `check`'s exit codes and `watch`'s `--interval` (300),
  `--repeat` (3,600), `--alert-log`, `--alert-cmd` and `--alert-cmd-level` (warn), what an operator does on each
  alarm, and the tests (`monitor/attest/tests/monitor.rs` on the R.7 test's real stalled instance,
  `lez/results/r7-stall.md`): lag 702,336 s raising `instance-stalled` and `ws-half`, the resumed instance none,
  `ws-expired` past the period, `recovered` and SIGTERM in `watch`. Every flag and default is quoted from
  `monitor/attest/src/main.rs`, `lib.rs` and `watch.rs`.

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-022 P.5 | shown (EVIDENCE 41, 91, 174) | **shown** (unchanged) | New: CI recomputes the figures from the records (`scripts/test_perf_records.py`, run by `links`). |
| RFP-022 P.6 | in part (EVIDENCE 15, 17, 43, 91, 100, 175) | **in part** (unchanged) | The same test recomputes the measured figures; the unmeasured parts are as the row says. |
| RFP-022 R.11 | in part (EVIDENCE 162: 64 of 68) | **in part** (EVIDENCE 162: 66 of 68) | P.5 and P.6 now have a test CI runs; R.14 and S.1 still do not. |
| RFP-021 S.7 | in part (EVIDENCE 21, 80, 81, 193, 200): "the guide does not describe stall detection with `attest-monitor` yet" | **in part**, that clause lifted | The guide's §3.5 describes it; nothing else of the row changed. |

## What the three documents need (exact old and new text)

**`docs/requirements-status.md`, row RFP-021 S.7 (line 122)** — lift the clause:

- Old: `…in the operator guide §3.5, §3.6; the guide does not describe stall detection with \`attest-monitor\` yet. Known limit:`
- New: `…in the operator guide §3.5, §3.6, which describe stall detection with \`attest-monitor\`. Known limit:`

**`docs/requirements-status.md`, row RFP-022 P.5 (line 219)** — name the test in the Evidence column:

- Old: `| 41, 91, 174; \`lez/results/perf-latency-bridge.md\` |`
- New: `| 41, 91, 174; \`lez/results/perf-latency-bridge.md\`; the figures recomputed from the records by \`scripts/test_perf_records.py\` (CI's \`links\` job) |`

**`docs/requirements-status.md`, row RFP-022 P.6 (line 220)** — the same:

- Old: `| 15, 17, 43, 91, 100, 175 |`
- New: `| 15, 17, 43, 91, 100, 175; the measured figures recomputed from the records by \`scripts/test_perf_records.py\` (CI's \`links\` job) |`

**`docs/traceability.md`, row RFP-021 S.7 (line 122)** — the closing clause:

- Old: `Remains: the operator guide's account of stall detection with \`attest-monitor\`.`
- New: `The [operator guide](guides/operator-guide.md) §3.5 describes stall detection with \`attest-monitor\` (its alarms, thresholds and defaults, and what an operator does on each).`

**`docs/traceability.md`, row RFP-022 R.11 (line 204)** — the count:

- Old: `\`check-row-tests.py\` in CI's \`links\` job fails unless every test said to run in a CI job is run by it; 64 of the 68 RFP-022 rows have a test CI runs since the coverage stream's jobs (\`lez-harness\`, \`monitor\`; [EVIDENCE](EVIDENCE.md) row 207), 4 do not (R.14 and S.1 with box or local tests only, P.5 and P.6 with records and documents only), so the gate \`--require-ci RFP-022\` exits 1 (row 162).`
- New: `\`check-row-tests.py\` in CI's \`links\` job fails unless every test said to run in a CI job is run by it; 66 of the 68 RFP-022 rows have a test CI runs since the coverage stream's jobs (\`lez-harness\`, \`monitor\`) and the performance-records test ([EVIDENCE](EVIDENCE.md) row 207), 2 do not (R.14 and S.1 with box or local tests only), so the gate \`--require-ci RFP-022\` exits 1 (row 162).`

**`docs/traceability.md`, rows RFP-022 P.5 (line 217) and P.6 (line 218)** — name the test in the "Named test or
artefact" column (R.11's gate reads this column):

- P.5, Old: `| \`docs/design/finality-rule/RESULTS.md\` table (k); \`e2e/runs/crown-live-1/README.md\`; \`e2e/runs/crown-live-3/README.md\` | M0, B4 |`
- P.5, New: `| \`docs/design/finality-rule/RESULTS.md\` table (k); \`e2e/runs/crown-live-1/README.md\`; \`e2e/runs/crown-live-3/README.md\`; \`scripts/test_perf_records.py\` | M0, B4 |`
- P.6, Old: `| \`cp-empty-1\`; \`cp-empty-360\`; \`cp-pp5-360\`; \`cp-privburns10-360\`; \`cp-pp50-360\`; \`results/bench.log\` | M0, B4 |`
- P.6, New: `| \`cp-empty-1\`; \`cp-empty-360\`; \`cp-pp5-360\`; \`cp-privburns10-360\`; \`cp-pp50-360\`; \`results/bench.log\`; \`scripts/test_perf_records.py\` | M0, B4 |`

**`docs/EVIDENCE.md`, row 162 (line 148)** — the count:

- Old: `RFP-022: 64 of 68 hard rows with a test CI runs, 4 without (R.14 and S.1 with box or local tests only, P.5 and P.6 with records and documents only); RFP-021: 54 of 59; \`--require-ci RFP-022\` exits 1 until the 4 have one`
- New: `RFP-022: 66 of 68 hard rows with a test CI runs, 2 without (R.14 and S.1 with box or local tests only); RFP-021: 54 of 59; \`--require-ci RFP-022\` exits 1 until the 2 have one`

**`docs/EVIDENCE.md`, row 207 (line 64)** — the count:

- Old: `rows with a test CI runs: RFP-021 54 of 59, RFP-022 64 of 68 (\`check-row-tests.py --list\`; \`test_ci_model.py\` 88 passed)`
- New: `rows with a test CI runs: RFP-021 54 of 59, RFP-022 66 of 68 (\`check-row-tests.py --list\`; \`test_ci_model.py\` 88 passed)`

**A new EVIDENCE row** (number 225 — 226 if `fix-n1`'s proposed 225 lands first) is proposed for the
recomputation itself:

> | 225 | RFP-022 P.5, P.6: the documented performance figures recomputed from the committed records | `scripts/test_perf_records.py` parses the raw records the documents cite (the live run's stamps, captures and driver log; the daemons' GNU time, samples, RPC byte counts and phases; the provers' `run.json` files, `prove.jsonl`, `groth16.jsonl`, the GNU-time logs) and asserts each figure equals its documented value at the documented rounding and that the document states it; three figures of P.6 (the GPU cost's 385 L4-seconds billed, the wrap's 7.3 GiB, the wrap's 18.7 s + 14 s end-to-end) are not recomputable from committed data and asserted nowhere, named in comments | the figures of rows 15, 16, 24, 91, 174 and 175 | laptop; 22 Nov 2026 (and on every push: CI's `links` job) | none (it reads committed files only) | `python3 -I scripts/test_perf_records.py` | `docs/performance.md` §P.5, §P.6; `lez/results/perf-latency-bridge.md`; `lez/results/perf-daemons.md` — as links relative to `docs/`, as EVIDENCE.md's other last columns are |

Existing EVIDENCE rows 174 and 175 could also name the test in their last column (the links row does the work;
not proposed, to keep them a record of the measurement):

- Row 174, Old: `` | `lez/results/perf-latency-bridge.md` | ``  — leave as is.
