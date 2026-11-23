# Stream glm-perf-tests: P.5's and P.6's figures recomputed in CI, and the operator guide's stall detection

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. On
`glm-perf-tests` from `8c38d723`: the test and its wiring, the operator guide §3.5, then this file — reviewed
twice (23 November 2026; the second round's one remaining Medium and its Lows are fixed in the commits after it).
Arithmetic below is against `integration10b` (tag `integ10-READY`), the branch this stream merges into; every
"Old" text was re-checked against it (EVIDENCE row 162 is at line 149 there; no conflict, the two files both
streams touch auto-merge). Checks on the laptop after the fixes: `python3 -I scripts/test_perf_records.py`
(20 tests OK), `python3 -I scripts/ci/check-row-tests.py` (consistent), `scripts/ci/test_ci_model.py` (88
passed), `check-guides.py` (0 findings), `check-links.py` (the `links` job's command: 112 files, 2376 links, 0
broken; `README.md docs`: 107 files, 2314 links, 0 broken), and a local mutation run: one document figure at a
time changed in a scratch copy of everything the test reads, the unmutated copy asserted green first, **18
mutations, all 18 fail the right test** — P.5's stamps, its breakdown tables' 1115.0, its 66.2 min and its
finalized slot (the byte-for-byte generator checks); §P.5's 6.6, 25.2, 8.3 and 7.0; a wrong "7.3 GiB" and a
wrong "9.3 GB"; the RFP-022 updater row's 0.043–0.092, its 57.3–57.8 **as that row**, and its "122 times"
(now one unique string, so RFP-021 §P.6's copy of the same figures cannot mask a drift); one of the three
"9.85 GB (host + r0vm)" copies reverted (occurrence counts are asserted); the status line's and EVIDENCE
row 175's own headlines; and the corrected P.4 "1.49 GB" reverted to the mislabelled value. Nothing was pushed
or sent. No code outside `scripts/`, `.github/workflows/ci.yml`, the results documents and the guides changed;
no program, guest or image id changed; no dependency changed.

## What changed

- **RFP-022 P.5, P.6 (a CI test each).** `scripts/test_perf_records.py`, run by CI's `links` job, recomputes the
  documented performance figures from the committed raw records and asserts each equals the documented value at
  the documented rounding and that the document states it — with the string's **count of occurrences** asserted,
  so a copy of a figure elsewhere in the same document (the updater row's figures also appear in RFP-021 §P.6)
  cannot mask a drift. What it asserts, exactly:
  - **The three generated records, byte for byte** — `bench/perf/latency_bridge.py`'s output on the committed
    records is `lez/results/perf-latency-bridge.md` whole; `bench/perf/summarize_daemons.py`'s output follows the
    hand-written header of `lez/results/perf-daemons.md`; `bench/perf/gate_report.py`'s output is
    `lez/results/perf-gate-sequencer.md` whole. Every *computed* figure of all three is thereby recomputed, at
    its generator's own rounding, stamps, slots and table cells included; the figures hard-coded in the
    generators' own prose ("6 slots", "121.6 min", the testnet and production parameters) are checked as text
    only, by the byte comparison.
  - **`docs/performance.md` §P.5's table and §P.6's rows**, figure by figure from the raw records: P.5's 32.9 min
    (captured 25.2, finality 18.6, availability 6.6, steady 29.8, hold-derived 1.1–12.1 h, the claim's 231.9 +
    8.3 s, the burn's 282.8 + 7.0 s, LEZ steps 39.6 s, verification 37 s, the checkpoint's 167 s, the release
    stage's 156 s), 71.0 / 66.3 / 66.2 min, W 3,607 s (the row's 60.1 min); P.6's daemons (monitor 0.53 CPU-s in
    1,284 s, 0.0004 of a core, 14.0 MiB, 86,265 / 254,218 B, ≈23 MB a day; the relayer 0.87 CPU-s, 0.0007,
    20.3 MiB, 408,005 / 975,682 B, ≈93 MB a day), the four L4 totals (14.0 / 24.1 / 41.4 / 219.3 s, GPU memory
    11,076–11,159 MiB, host 0.76–1.08 GiB), the laptop's 1,157.5 / 1,758.9 s and 9.85 GB peak, the wrap's
    195.1 / 215.2 s and the prover container's 7.3 GiB (7.30 / 7.31 GiB `VmHWM` in the committed wrap logs), the
    rule (B) 31.9 s (n = 4, 9 segments), the live run's L40S 31.9 s prover wall (119 s with the queue;
    18,499,940 user cycles, 20 segments, blocks 25..=90, 2 burns), and the **updater row as one string unique to
    §P.6** — 0.043–0.092 of a core and 57.3–57.8 MiB over the gate runs' honest daemons (read from the generated
    gate record, which is itself checked byte for byte against its raw records), 10.3–10.4 MB over gate G2's
    1,849 s, and the row's "about 122 times … 60 times" replay-rate gloss recomputed from G2's own line.
  - **The rows' own status and Evidence lines** (`docs/requirements-status.md`, `docs/EVIDENCE.md` rows 174 and
    175) state the headlines — the P.5 latencies and W, the daemons' four figures — and the test asserts those
    files state them.
- **Two figures are not recomputable from committed data and are asserted nowhere**, named precisely why in the
  test's docstring: the §P.6 cost row's "385 L4-seconds billed … about 0.09 USD" (Modal's own billing statement;
  only the 366 s the functions themselves ran recomputes, asserted) and the §P.6 wrap row's "recursion 18.7 s on
  16 cores + 14 s Groth16" (an end-to-end run cited to `docs/design/bridge.md` §18.7, whose raw per-step timings
  are not committed). §P.5's production-parameter estimates and its closing sentence's crown-live-1 "64.7 min"
  are likewise named, precisely why (the estimates are the design's and the simulator's, cited to `bridge.md`
  §9, §11.7; crown-live-1's `stages.done` stamps its release-0 stage at the runner's late re-check — "0s
  (already done)" — so the release block's time is not in its stamps). No other recomputed figure disagreed with
  its document — except the unit errors below, which the reviews found and this branch corrects.
- **Two KiB-as-GB unit errors, corrected.** GNU time's "Maximum resident set size (kbytes)" is KiB:
  - The laptop checkpoint proof's 9,617,860 KiB is **9.85 GB** (9.17 GiB), not the "9.62 GB" main states, and
    the wrap's host peak (1,444,084 / 1,443,392 KiB) is **1.48 GB**, its container peak recomputed as 7.3 GiB.
    Corrected on this branch in `checkpoint/RESULTS.md` (three 9.85 GB sites and two 1.48 GB sites),
    `docs/performance.md`, `docs/design/bridge.md`, `gpu-prover/RESULTS.md`, `docs/guides/operator-guide.md`
    and `scripts/ci/row-tests.toml`; the test asserts 9.85 GB's **three** occurrences in
    `checkpoint/RESULTS.md` (so reverting one copy fails) and the two wrap rows' 1.48 GB in full. The one
    shared file left to the integrator is
    `docs/traceability.md` (exact text below). No EVIDENCE or `requirements-status.md` row states the
    mislabelled figure.
  - **P.4's release proofs' peak, already on main with the same misreading:** the committed v1 prove logs peak
    at 1,458,420 KiB, which is **1.49 GB** (1.39 GiB), not the stated "1.46 GB" (KiB / 1e6). Recomputed in the
    test from `bridge/artifacts/v1/logs/*prove-*.log` and the p254 log, and asserted. Corrected on this branch
    in six places — `docs/performance.md` (§P.4's table), `docs/design/bridge.md` §18.2, `bridge/RESULTS.md`
    (three), `docs/guides/user-guide.md` — the two further copies in `docs/traceability.md` are the
    integrator's (exact text below).
- **The wiring.** A `links` job step runs the test; `scripts/ci/row-tests.toml` maps it to RFP-022 P.5 and P.6
  (whose notes keep the reason no CI test *measures* the latency or the resources themselves), so
  `check-row-tests.py` counts both rows with a test CI runs: **RFP-022 66 of 68** (were 64), 2 without (R.14 and
  S.1, box or local tests only; were 4). `test_ci_model.py`'s two count checks are updated with the map, as its
  own comment directs. The S.7 entry's note no longer says the guide is a first draft that does not describe
  stall detection; it says §3.5 describes attest-monitor's alarms, thresholds, defaults and the operator's
  action on each.
- **RFP-021 S.7 (the operator guide).** §3.5 describes stall detection with `attest-monitor` beside the updater:
  the `instance-stalled` (warn, `--stall-after` default 15,300 s), `ws-half` and `ws-expired` (critical) and
  `recovered` (info) alarms with their thresholds, `check`'s exit codes (0, 10, 20, 1 and 2) and `watch`'s
  `--interval` (300), `--repeat` (3,600), `--alert-log`, `--alert-cmd` and `--alert-cmd-level` (warn), what an
  operator does on each alarm, and the tests (`monitor/attest/tests/monitor.rs` on the R.7 test's real stalled
  instance, `lez/results/r7-stall.md`): lag 702,336 s raising `instance-stalled` and `ws-half`, the resumed
  instance none, `ws-expired` past the period, `recovered` and SIGTERM in `watch`. An outage holds every alarm
  when the instance cannot be read and holds the stall alarm when no provider's finalized slot answers (the
  weak-subjectivity alarms need no provider). Every flag and default is quoted from `monitor/attest/src/main.rs`,
  `lib.rs` and `watch.rs`.

## Rows

The counts are `integration10b`'s (RFP-021: 38 shown, 17 in part, 1 not yet, 3 waiting; RFP-022: 40, 25, 1, 2).

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-022 P.5 | shown (EVIDENCE 41, 91, 174) | **shown** (unchanged) | New: CI recomputes the figures from the records (`scripts/test_perf_records.py`, run by `links`). |
| RFP-022 P.6 | in part (EVIDENCE 15, 17, 43, 91, 100, 175) | **in part** (unchanged) | The same test recomputes the measured figures; the unmeasured parts are as the row says. |
| RFP-022 R.11 | in part (EVIDENCE 162: 64 of 68) | **in part** (EVIDENCE 162: 66 of 68), or **shown** — see the note | P.5 and P.6 now have a test CI runs; R.14 and S.1 still do not. Both reviews' verdict is **shown on the RFP's wording** (the clause asks for a test, not for CI; R.14's own row is "shown" the same way, its test running on the box): that move is RFP-022 41 shown / 24 in part. If the integrator keeps "in part", the reason should say the shortfall is against the team's own B4 CI bar, not the clause. |
| RFP-021 S.7 | in part (EVIDENCE 21, 80, 81, 193, 200): "the guide does not describe stall detection with `attest-monitor` yet" | **in part**, that clause lifted — or **shown** (both reviews' verdict) | The guide's §3.5 describes it; nothing else of the row changed. Moving it is RFP-021 39 shown / 16 in part; kept "in part" here only because the row's Known limit keeps it partly open — the integrator decides. |

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

**`docs/requirements-status.md`, row RFP-022 R.11 (line 206)** — the count:

- Old: `Every hard row of both RFPs mapped to a test, record or document, and CI's claims checked in the \`links\` job; 64 of 68 RFP-022 rows (54 of 59 RFP-021) have a test CI runs, 4 do not, so the gate (\`--require-ci RFP-022\`) exits 1.`
- New: `Every hard row of both RFPs mapped to a test, record or document, and CI's claims checked in the \`links\` job; 66 of 68 RFP-022 rows (54 of 59 RFP-021) have a test CI runs, 2 do not, so the gate (\`--require-ci RFP-022\`) exits 1.`

**`docs/traceability.md`, row RFP-021 S.7 (line 122)** — the closing clause:

- Old: `Remains: the operator guide's account of stall detection with \`attest-monitor\`.`
- New: `The [operator guide](guides/operator-guide.md) §3.5 describes stall detection with \`attest-monitor\` (its alarms, thresholds and defaults, and what an operator does on each).`

**`docs/traceability.md`, row RFP-022 R.11 (line 204)** — the count:

- Old: `\`check-row-tests.py\` in CI's \`links\` job fails unless every test said to run in a CI job is run by it; 64 of the 68 RFP-022 rows have a test CI runs since the coverage stream's jobs (\`lez-harness\`, \`monitor\`; [EVIDENCE](EVIDENCE.md) row 207), 4 do not (R.14 and S.1 with box or local tests only, P.5 and P.6 with records and documents only), so the gate \`--require-ci RFP-022\` exits 1 (row 162).`
- New: `\`check-row-tests.py\` in CI's \`links\` job fails unless every test said to run in a CI job is run by it; 66 of the 68 RFP-022 rows have a test CI runs since the coverage stream's jobs (\`lez-harness\`, \`monitor\`) and the performance-records test ([EVIDENCE](EVIDENCE.md) row 225), 2 do not (R.14 and S.1 with box or local tests only), so the gate \`--require-ci RFP-022\` exits 1 (row 162).`

**`docs/traceability.md`, row RFP-022 P.6 (line 218)** — the first unit error:

- Old: `wrap 195.1–215.2 s, peak 9.62 GB, laptop CPU`
- New: `wrap 195.1–215.2 s, peak 9.85 GB, laptop CPU`

**`docs/traceability.md`, rows RFP-022 P.4 (line 216) and the P.4 note (line 274)** — the second unit error:

- Line 216, Old: `release succinct receipt 37.1–48.6 s (the earlier release image); claim statement 38.90 s; about 1.46 GB peak each.`
- Line 216, New: `release succinct receipt 37.1–48.6 s (the earlier release image); claim statement 38.90 s; about 1.5 GB peak each.`
- Line 274, Old: `Release and claim-statement receipts took about 1.46 GB;`
- Line 274, New: `Release and claim-statement receipts took about 1.5 GB;`
- (The release's peak is 1,458,420 KiB = 1.49 GB, the claim statement's 1,461,752 KiB = 1.50 GB, both in
  `bridge/artifacts/logs/`; "about 1.5 GB peak each" is exact for both. The release-only rows elsewhere —
  corrected on this branch — state 1.49 GB.)

**`docs/traceability.md`, rows RFP-022 P.5 (line 217) and P.6 (line 218)** — name the test in the "Named test or
artefact" column (R.11's gate reads this column):

- P.5, Old: `| \`docs/design/finality-rule/RESULTS.md\` table (k); \`e2e/runs/crown-live-1/README.md\`; \`e2e/runs/crown-live-3/README.md\` | M0, B4 |`
- P.5, New: `| \`docs/design/finality-rule/RESULTS.md\` table (k); \`e2e/runs/crown-live-1/README.md\`; \`e2e/runs/crown-live-3/README.md\`; \`scripts/test_perf_records.py\` | M0, B4 |`
- P.6, Old: `| \`cp-empty-1\`; \`cp-empty-360\`; \`cp-pp5-360\`; \`cp-privburns10-360\`; \`cp-pp50-360\`; \`results/bench.log\` | M0, B4 |`
- P.6, New: `| \`cp-empty-1\`; \`cp-empty-360\`; \`cp-pp5-360\`; \`cp-privburns10-360\`; \`cp-pp50-360\`; \`results/bench.log\`; \`scripts/test_perf_records.py\` | M0, B4 |`

**`docs/EVIDENCE.md`, row 162 (line 148 on main; line 149 on `integration10b`)** — the count:

- Old: `RFP-022: 64 of 68 hard rows with a test CI runs, 4 without (R.14 and S.1 with box or local tests only, P.5 and P.6 with records and documents only); RFP-021: 54 of 59; \`--require-ci RFP-022\` exits 1 until the 4 have one`
- New: `RFP-022: 66 of 68 hard rows with a test CI runs, 2 without (R.14 and S.1 with box or local tests only); RFP-021: 54 of 59; \`--require-ci RFP-022\` exits 1 until the 2 have one`

**`docs/EVIDENCE.md`, row 207 (line 64)** — the count; **merge caution**: `ci-shards` rewrites the same row
(keeping "64 of 68" and "test_ci_model.py 108 passed") — whichever stream lands second must leave "66 of 68"
and, with `ci-shards`' tests added, the count it measured:

- Old: `rows with a test CI runs: RFP-021 54 of 59, RFP-022 64 of 68 (\`check-row-tests.py --list\`; \`test_ci_model.py\` 88 passed)`
- New: `rows with a test CI runs: RFP-021 54 of 59, RFP-022 66 of 68 (\`check-row-tests.py --list\`; \`test_ci_model.py\` 88 passed)`

**A new EVIDENCE row** (number 225 — 226 if `fix-n1`'s proposed 225 lands first) is proposed for the
recomputation itself:

> | 225 | RFP-022 P.5, P.6: the documented performance figures recomputed from the committed records | `scripts/test_perf_records.py` parses the raw records the documents cite (the live run's stamps, captures and driver log; the daemons' GNU time, samples, RPC byte counts and phases; the provers' `run.json` files, `prove.jsonl`, `groth16.jsonl` and the GNU-time and wrap logs) and regenerates the three generated records byte for byte (`perf-latency-bridge.md`, `perf-daemons.md`, and the gate runs' `perf-gate-sequencer.md`, whence the updater row's figures are read), asserting each figure equals its documented value at the documented rounding and that the document states it, each string with its count of occurrences (the updater row as one string unique to the RFP-022 table); two figures (the GPU cost's 385 L4-seconds billed, the wrap's 18.7 s + 14 s end-to-end) are not recomputable from committed data and asserted nowhere, named in the test's docstring, as are §P.5's estimates and the first run's 64.7 min | the figures of rows 15, 16, 17, 91, 172, 174 and 175 | laptop; 23 Nov 2026 (and on every push: CI's `links` job) | none (it reads committed files only) | `python3 -I scripts/test_perf_records.py` | `docs/performance.md` §P.5, §P.6; `lez/results/perf-latency-bridge.md`; `lez/results/perf-daemons.md`; `lez/results/perf-gate-sequencer.md` — as links relative to `docs/`, as EVIDENCE.md's other last columns are |

Existing EVIDENCE rows 174 and 175 could also name the test in their last column (the links row does the work;
not proposed, to keep them a record of the measurement). No EVIDENCE row states either mislabelled GB figure, so
none of them needs the unit corrections; every traceability change they need is listed above.
