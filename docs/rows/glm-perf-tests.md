# Stream glm-perf-tests: P.5's and P.6's figures recomputed in CI, and the operator guide's stall detection

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. On
`glm-perf-tests` from `8c38d723`: `2486a015` (the test and its wiring), `ed46b562` (the operator guide §3.5), then
this file; reviewed by `scratch-glm/review-1.md` (23 November 2026: not safe, five Mediums) and fixed in the three
commits after it (M1–M5, each at its root). Checks run on the laptop after the fixes: `python3 -I
scripts/test_perf_records.py` (19 tests OK), `python3 -I scripts/ci/check-row-tests.py` (consistent),
`scripts/ci/test_ci_model.py` (88 passed), `check-guides.py` (0 findings), `check-links.py` (the `links` job's
command: 112 files, 2376 links, 0 broken; `README.md docs`: 107 files, 2314 links, 0 broken), and a mutation demo
(`scratch-glm/mutate-docs.py`): 13 single document-figure mutations, every one now fails the right test — the nine
that passed before the fixes among them (P.5's stamps, its breakdown tables' 1115.0, its 66.2 min, the finalized
slot, §P.5's 6.6 and 25.2 min, 8.3 s and 7.0 s, the updater row's 0.043–0.092 and 10.3–10.4 MB), and, answering the
review's D15 and D21, a wrong "7.3 GiB" and a wrong "9.3 GB" in place of the corrected 9.85 GB. Nothing was pushed
or sent. No code outside `scripts/`, `.github/workflows/ci.yml`, the results documents and the guides changed; no
program, guest or image id changed; no dependency changed.

## What changed

- **RFP-022 P.5, P.6 (a CI test each).** `scripts/test_perf_records.py`, run by CI's `links` job, recomputes the
  documented performance figures from the committed raw records and asserts each equals the documented value at
  the documented rounding and that the document states it (the exact string). What it asserts, exactly:
  - **The three generated records, byte for byte** — `bench/perf/latency_bridge.py`'s output on the committed
    records is `lez/results/perf-latency-bridge.md` whole; `bench/perf/summarize_daemons.py`'s output follows the
    hand-written header of `lez/results/perf-daemons.md`; `bench/perf/gate_report.py`'s follows that of
    `lez/results/perf-gate-sequencer.md`. Every figure of all three is thereby recomputed at its generator's own
    rounding, stamps, slots and table cells included.
  - **`docs/performance.md` §P.5's table and §P.6's rows**, figure by figure from the raw records: P.5's
    32.9 min (captured 25.2, finality 18.6, availability 6.6, steady 29.8, hold-derived 1.1–12.1 h, the claim's
    231.9 + 8.3 s, the burn's 282.8 + 7.0 s, LEZ steps 39.6 s, verification 37 s, the checkpoint's 167 s, the
    release stage's 156 s), 71.0 / 66.3 / 66.2 min, W 3,607 s (the row's 60.1 min, which the row and EVIDENCE
    row 174 state and the test now checks them for); P.6's daemons (monitor 0.53 CPU-s in 1,284 s, 0.0004 of a
    core, 14.0 MiB, 86,265 / 254,218 B, ≈23 MB a day; the relayer 0.87 CPU-s, 0.0007, 20.3 MiB, 408,005 /
    975,682 B, ≈93 MB a day), the four L4 totals (14.0 / 24.1 / 41.4 / 219.3 s, GPU memory 11,076–11,159 MiB,
    host 0.76–1.08 GiB), the laptop's 1,157.5 / 1,758.9 s and 9.85 GB peak, the wrap's 195.1 / 215.2 s and the
    prover container's 7.3 GiB (7.30 / 7.31 GiB `VmHWM` in the committed wrap logs — recomputable, as the review's
    M1 found), the rule (B) 31.9 s (n = 4, 9 segments), the live run's L40S 31.9 s prover wall (119 s with the
    queue; 18,499,940 user cycles, 20 segments, blocks 25..=90, 2 burns), and the updater row (0.043–0.092 of a
    core, 57.3–57.8 MiB peak, 10.3–10.4 MB in 31 min, from the gate runs' raw records — recomputed now, not merely
    named, as the review's M3 asked).
- **Two figures are not recomputable from committed data and are asserted nowhere**; §P.5's production-parameter
  estimates and its closing sentence's crown-live-1 "64.7 min" are likewise named, precisely why, in the test's
  docstring (the estimates are the design's and the simulator's, cited to `bridge.md` §9, §11.7; crown-live-1's
  release-0 stage is stamped at the runner's late re-check — "0s (already done)" — so the release block's time is
  not in its stamps). The two: the GPU cost row's "385 L4-seconds billed … about 0.09 USD" (Modal's own billing
  statement; only the 366 s the functions themselves ran recomputes, asserted) and the wrap row's "recursion
  18.7 s on 16 cores + 14 s Groth16" (an end-to-end run cited to `docs/design/bridge.md` §18.7, whose raw
  per-step timings are not committed). No recomputed figure disagreed with its document — except the unit error
  below, which the review's M2 found and this branch corrects.
- **The "9.62 GB" unit error (review M2).** GNU time's "Maximum resident set size (kbytes)" is KiB: the laptop
  proof's 9,617,860 KiB is **9.85 GB** (9.17 GiB), not the "9.62 GB" main states (a /1e6 misreading of KiB). The
  same error made "1.44 GB" of the wrap's host peak (1,444,084 KiB) out of "1.48 GB". This branch corrects its own
  files and the documents the test cites; the one shared file left to the integrator is `docs/traceability.md`
  (exact text below). Every place:
  | File (main) | Old | New |
  |---|---|---|
  | `checkpoint/RESULTS.md` (3 places) | `peak 9.62 GB (host + r0vm)` / `9.62 GB (host + r0vm)` (×2) | `peak 9.85 GB (host + r0vm)` / `9.85 GB (host + r0vm)` (×2) |
  | `checkpoint/RESULTS.md` (2 places) | `host + r0vm 1.44 GB; container seen 48.2 s, cgroup peak 8.48 GiB …` / `… 53.8 s, cgroup peak 7.36 GiB …` | the same with `host + r0vm 1.48 GB` |
  | `docs/performance.md` | `9.62 GB peak` | `9.85 GB peak` |
  | `docs/design/bridge.md` | `peak 9.62 GB` | `peak 9.85 GB` |
  | `gpu-prover/RESULTS.md` | `against 9.62 GB for the CPU proof of \`cp-empty-1\`` | `against 9.85 GB for the CPU proof of \`cp-empty-1\`` |
  | `docs/guides/operator-guide.md` | `against 9.62 GB for the` | `against 9.85 GB for the` |
  | `scripts/ci/row-tests.toml` | `wrap 195.1-215.2 s, peak 9.62 GB;` | `wrap 195.1-215.2 s, peak 9.85 GB;` |
  | **`docs/traceability.md` (the integrator's)** | `wrap 195.1–215.2 s, peak 9.62 GB, laptop CPU` (the RFP-022 P.6 row) | `wrap 195.1–215.2 s, peak 9.85 GB, laptop CPU` |
  The first seven rows are already corrected on this branch. The review also flagged, as pre-existing and
  unverified, other GB figures that may share the error (for example `docs/performance.md`'s "peak 1.46 GB" of
  P.4): not touched here, and now certified by nothing.
- **The wiring.** A `links` job step runs the test; `scripts/ci/row-tests.toml` maps it to RFP-022 P.5 and P.6
  (whose notes keep the reason no CI test *measures* the latency or the resources themselves), so
  `check-row-tests.py` counts both rows with a test CI runs: **RFP-022 66 of 68** (were 64), 2 without (R.14 and
  S.1, box or local tests only; were 4). `test_ci_model.py`'s two count checks are updated with the map, as its
  own comment directs. The S.7 entry's note no longer says the guide is a first draft that does not describe
  stall detection (review M5); it says §3.5 describes attest-monitor's alarms, thresholds, defaults and the
  operator's action on each.
- **RFP-021 S.7 (the operator guide).** §3.5 describes stall detection with `attest-monitor` beside the updater:
  the `instance-stalled` (warn, `--stall-after` default 15,300 s), `ws-half` and `ws-expired` (critical) and
  `recovered` (info) alarms with their thresholds, `check`'s exit codes (0, 10, 20, 1 and 2 — the usage error the
  review's Low 6 found missing) and `watch`'s `--interval` (300), `--repeat` (3,600), `--alert-log`, `--alert-cmd`
  and `--alert-cmd-level` (warn), what an operator does on each alarm, and the tests
  (`monitor/attest/tests/monitor.rs` on the R.7 test's real stalled instance, `lez/results/r7-stall.md`): lag
  702,336 s raising `instance-stalled` and `ws-half`, the resumed instance none, `ws-expired` past the period,
  `recovered` and SIGTERM in `watch`. An outage holds every alarm when the instance cannot be read and holds the
  stall alarm when no provider's finalized slot answers (the review's Low 6: the weak-subjectivity alarms need no
  provider). Every flag and default is quoted from `monitor/attest/src/main.rs`, `lib.rs` and `watch.rs`.

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-022 P.5 | shown (EVIDENCE 41, 91, 174) | **shown** (unchanged) | New: CI recomputes the figures from the records (`scripts/test_perf_records.py`, run by `links`). |
| RFP-022 P.6 | in part (EVIDENCE 15, 17, 43, 91, 100, 175) | **in part** (unchanged) | The same test recomputes the measured figures; the unmeasured parts are as the row says. |
| RFP-022 R.11 | in part (EVIDENCE 162: 64 of 68) | **in part** (EVIDENCE 162: 66 of 68) | P.5 and P.6 now have a test CI runs; R.14 and S.1 still do not. The review's verdict is **shown on the RFP's wording** (the clause asks for a test, not for CI; R.14's own row is "shown" the same way) — the integrator decides; if it moves to shown, RFP-022's counts move from 43/22 to 44/21 and the reason should say the shortfall is against the team's CI bar, not the clause. |
| RFP-021 S.7 | in part (EVIDENCE 21, 80, 81, 193, 200): "the guide does not describe stall detection with `attest-monitor` yet" | **in part**, that clause lifted | The guide's §3.5 describes it; nothing else of the row changed. The review's verdict is **shown on the RFP's wording** once the clause lifts (every part is in the guide) — if it moves, RFP-021's counts move from 39/16 to 40/15. |

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

**`docs/requirements-status.md`, row RFP-022 R.11 (line 206)** — the count (the review's Low 1):

- Old: `Every hard row of both RFPs mapped to a test, record or document, and CI's claims checked in the \`links\` job; 64 of 68 RFP-022 rows (54 of 59 RFP-021) have a test CI runs, 4 do not, so the gate (\`--require-ci RFP-022\`) exits 1.`
- New: `Every hard row of both RFPs mapped to a test, record or document, and CI's claims checked in the \`links\` job; 66 of 68 RFP-022 rows (54 of 59 RFP-021) have a test CI runs, 2 do not, so the gate (\`--require-ci RFP-022\`) exits 1.`

**`docs/traceability.md`, row RFP-021 S.7 (line 122)** — the closing clause:

- Old: `Remains: the operator guide's account of stall detection with \`attest-monitor\`.`
- New: `The [operator guide](guides/operator-guide.md) §3.5 describes stall detection with \`attest-monitor\` (its alarms, thresholds and defaults, and what an operator does on each).`

**`docs/traceability.md`, row RFP-022 R.11 (line 204)** — the count:

- Old: `\`check-row-tests.py\` in CI's \`links\` job fails unless every test said to run in a CI job is run by it; 64 of the 68 RFP-022 rows have a test CI runs since the coverage stream's jobs (\`lez-harness\`, \`monitor\`; [EVIDENCE](EVIDENCE.md) row 207), 4 do not (R.14 and S.1 with box or local tests only, P.5 and P.6 with records and documents only), so the gate \`--require-ci RFP-022\` exits 1 (row 162).`
- New: `\`check-row-tests.py\` in CI's \`links\` job fails unless every test said to run in a CI job is run by it; 66 of the 68 RFP-022 rows have a test CI runs since the coverage stream's jobs (\`lez-harness\`, \`monitor\`) and the performance-records test ([EVIDENCE](EVIDENCE.md) row 225), 2 do not (R.14 and S.1 with box or local tests only), so the gate \`--require-ci RFP-022\` exits 1 (row 162).`

**`docs/traceability.md`, row RFP-022 P.6 (line 218)** — the unit error (review M2):

- Old: `a 3.70M-cycle one-block checkpoint proven in 1,157.5 s (started quiet) and 1,758.9 s (under load), wrap 195.1–215.2 s, peak 9.62 GB, laptop CPU`
- New: `a 3.70M-cycle one-block checkpoint proven in 1,157.5 s (started quiet) and 1,758.9 s (under load), wrap 195.1–215.2 s, peak 9.85 GB, laptop CPU`

**`docs/traceability.md`, rows RFP-022 P.5 (line 217) and P.6 (line 218)** — name the test in the "Named test or
artefact" column (R.11's gate reads this column):

- P.5, Old: `| \`docs/design/finality-rule/RESULTS.md\` table (k); \`e2e/runs/crown-live-1/README.md\`; \`e2e/runs/crown-live-3/README.md\` | M0, B4 |`
- P.5, New: `| \`docs/design/finality-rule/RESULTS.md\` table (k); \`e2e/runs/crown-live-1/README.md\`; \`e2e/runs/crown-live-3/README.md\`; \`scripts/test_perf_records.py\` | M0, B4 |`
- P.6, Old: `| \`cp-empty-1\`; \`cp-empty-360\`; \`cp-pp5-360\`; \`cp-privburns10-360\`; \`cp-pp50-360\`; \`results/bench.log\` | M0, B4 |`
- P.6, New: `| \`cp-empty-1\`; \`cp-empty-360\`; \`cp-pp5-360\`; \`cp-privburns10-360\`; \`cp-pp50-360\`; \`results/bench.log\`; \`scripts/test_perf_records.py\` | M0, B4 |`

**`docs/EVIDENCE.md`, row 162 (line 148)** — the count:

- Old: `RFP-022: 64 of 68 hard rows with a test CI runs, 4 without (R.14 and S.1 with box or local tests only, P.5 and P.6 with records and documents only); RFP-021: 54 of 59; \`--require-ci RFP-022\` exits 1 until the 4 have one`
- New: `RFP-022: 66 of 68 hard rows with a test CI runs, 2 without (R.14 and S.1 with box or local tests only); RFP-021: 54 of 59; \`--require-ci RFP-022\` exits 1 until the 2 have one`

**`docs/EVIDENCE.md`, row 207 (line 64)** — the count; **merge caution** (the review's Low 2): `ci-shards`
rewrites the same row (keeping "64 of 68" and "test_ci_model.py 108 passed") — whichever stream lands second must
leave "66 of 68" and, with `ci-shards`' tests added, "108 passed":

- Old: `rows with a test CI runs: RFP-021 54 of 59, RFP-022 64 of 68 (\`check-row-tests.py --list\`; \`test_ci_model.py\` 88 passed)`
- New: `rows with a test CI runs: RFP-021 54 of 59, RFP-022 66 of 68 (\`check-row-tests.py --list\`; \`test_ci_model.py\` 88 passed)`

**A new EVIDENCE row** (number 225 — 226 if `fix-n1`'s proposed 225 lands first; the review's M4 corrections in
it: row 17 among the cited rows, the date 23 November) is proposed for the recomputation itself:

> | 225 | RFP-022 P.5, P.6: the documented performance figures recomputed from the committed records | `scripts/test_perf_records.py` parses the raw records the documents cite (the live run's stamps, captures and driver log; the daemons' GNU time, samples, RPC byte counts and phases; the provers' `run.json` files, `prove.jsonl`, `groth16.jsonl` and the GNU-time and wrap logs; the gate runs' raw records for the updater row) and regenerates the three generated records byte for byte, asserting each figure equals its documented value at the documented rounding and that the document states it; two figures (the GPU cost's 385 L4-seconds billed, the wrap's 18.7 s + 14 s end-to-end) are not recomputable from committed data and asserted nowhere, named in the test's docstring, as are §P.5's estimates and the first run's 64.7 min | the figures of rows 15, 16, 17, 91, 174 and 175 | laptop; 23 Nov 2026 (and on every push: CI's `links` job) | none (it reads committed files only) | `python3 -I scripts/test_perf_records.py` | `docs/performance.md` §P.5, §P.6; `lez/results/perf-latency-bridge.md`; `lez/results/perf-daemons.md`; `lez/results/perf-gate-sequencer.md` — as links relative to `docs/`, as EVIDENCE.md's other last columns are |

The corrected "9.85 GB" (review M2) belongs in the integrator's hands too: `docs/traceability.md`'s P.6 row
(above), while `checkpoint/RESULTS.md`, `docs/performance.md`, `docs/design/bridge.md`,
`gpu-prover/RESULTS.md`, `docs/guides/operator-guide.md` and `scripts/ci/row-tests.toml` are already corrected
on this branch. No EVIDENCE row or `requirements-status.md` row states the mislabelled figure, so none of them
needs this correction.

Existing EVIDENCE rows 174 and 175 could also name the test in their last column (the links row does the work;
not proposed, to keep them a record of the measurement).
