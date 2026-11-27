# RFP-021 R.9 and R.10 with the turns on the chain's height: five runs of two updater daemons, 106 updates each

**Measured** on the build boxes (Modal containers), on LEZ v0.4.0-rc1's own sequencer with a Logos Blockchain node
(LEZ's local configuration; nothing is sent to any public network), as the runs of
[`perf-gate-sequencer-100.md`](perf-gate-sequencer-100.md) and [`perf-gate-sequencer-join.md`](perf-gate-sequencer-join.md)
(G6–G11), which describe the method, the replay and the terms: each run reveals 106 of the lab's recorded mainnet updates
(96 finality updates and 10 sync-committee period updates), one every 60 s, the clock set back five days with libfaketime
(`FAKETIME=-5d`, monotonic clocks real; the dates in these records are the faked ones, the runs were made on 27 November
2026), `bash bench/perf/gate-sequencer.sh NAME` with `GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4
GATE_DEADLINE_MIN=150` and the settings below (each run's `machine.txt`). What differs from G10 and G11 is the updater:
its turns, join or wait's bounds and its stall alarm count the chain's height (its follower's last block, final on
Bedrock: LEZ makes one every 10 s here), not its own polling rounds, and it learns other parties' jobs from their `Open`s
on chain ([ADR 0049](../../docs/decisions/0049-updater-turns-on-the-chains-height.md)). The five ran one build (commit
`860be342`, updater sha256 `ac1c2df1…`), each on a box of its own, with the zone's bundle G7–G9 used (sha256
`5f8937d7…`); the commits after `860be342` change tests, comments and documents only. The machine paths in the logs are
written `<repo>`.

- `gate-g22`: honest-1 polling every 2 s, honest-2 every 6 s (`GATE_POLLS=2,6`; both read the chain at least once a
  10-s block): RFP-021 R.10's first clause.
- `gate-g23`: honest-2's beacon port serves a copy of every update, its signature slot moved by one slot
  (`GATE_COPIES=1`, `bench/perf/beacon_replay.py --copies`), so the two daemons are never served the same bytes and a
  job one opens is found by the other only from its `Open` on chain: RFP-021 R.9's several instances.
- `gate-g24`: A3's adversarial daemon beside the two honest ones: it never defers (window 1), waits for no other job
  (`join_wait_rounds = 0`) and polls every 2 s (`GATE_ADVERSARY=1`).
- `gate-g25` and `gate-g26`: the default (both honest daemons polling every 4 s), so the window-8 figures are not one run
  (`gate-g10`, the earlier updater, the third).

**R.10.** Polling three times as often did not change who pays: in `gate-g22` honest-1, polling every 2 s, paid 0.575 of
the honest fees and honest-2, every 6 s, 0.425; with both polling every 4 s honest-1 paid 0.577 (`gate-g25`) and 0.583
(`gate-g26`). Honest-1 paid more than half in each run at this size where one replay served both daemons the same bytes
and no adversary ran, before this updater too (G6–G11: 0.521–0.617; here 0.575, 0.577, 0.583), whatever the polls; with
copies (`gate-g23`) 0.498, beside the adversary (`gate-g24`) 0.497. Those runs share one instance, one sequence of
updates and the daemons' two keys, so they repeat largely the same draws of `H(key, job, step)`; no run varied the keys. Under the earlier updater a daemon that never
deferred, polling every 2 s against 4, applied 93 of 99 steps (G2). Every revealed finality update was covered in every
run. Latency at window 8 grew with the turns in blocks: medians 145–165 s and p90 255–286 s without the adversary
(G10, rounds of about 6 s: 105 / 157 s).

**R.9.** No step was applied twice in any run, and every honest refused step shared its block with the same step of the
same job, applied (or is a `Finish` beside another job's, exit 82). In `gate-g23` the two daemons were never served the
same bytes: each learned the other's copies from their `Open`s on chain and joined their jobs; the 10 jobs abandoned
there were each left at its `Open` alone, opened before the other's `Open` was in a final block (5 in the same block, 4
in a block published but not yet final, 1 within a second of it: a copy is learned only from a final block), so all gas
was 1.042 times the completed jobs', as in the default runs (1.059, 1.048). With A3's adversary (`gate-g24`), which
never defers, waits for no one and polls every 2 s, the adversary sent 304 of the 337 applied steps and 105 of the 106
`Open`s; each honest daemon applied 16–17 steps and lost 29 races in a block, so A3's ratio of honest refused to applied
gas is **0.9529**, outside A3's limit (G2, the earlier updater over 30 updates: 1.31), while all the daemons' gas was
1.107 times the completed jobs' and the adversary paid 0.81 of all fees.

**Against A3's gate** (honest gas on refused steps / on applied steps at most 0.10; each honest daemon's share of the
honest fees 0.35–0.65): `gate-g22` (honest-1 polling every 2 s, honest-2 every 6 s) **0.0331** and honest-1's share **0.575**; `gate-g23` (honest-2 served copies) **0.0001** and honest-1's share **0.498**; `gate-g24` (with A3's adversary) **0.9529** and honest-1's share **0.497**; `gate-g25` (the default) **0.0550** and honest-1's share **0.577**; `gate-g26` (the default) **0.0484** and honest-1's share **0.583**.

**Latency**, a revealed finality update to the first `Finish`, final on Bedrock, that leaves the instance at least as
fresh, median / p90: `gate-g22` 156 / 265 s (96 of 96 covered); `gate-g23` 161 / 286 s (96 of 96 covered); `gate-g24` 35 / 45 s (96 of 96 covered); `gate-g25` 165 / 285 s (96 of 96 covered); `gate-g26` 145 / 255 s (96 of 96 covered).

**All gas / the completed jobs' gas**: `gate-g22` 1.037 (1 job(s) abandoned); `gate-g23` 1.042 (10 job(s) abandoned); `gate-g24` 1.107 (0 job(s) abandoned); `gate-g25` 1.059 (1 job(s) abandoned); `gate-g26` 1.048 (0 job(s) abandoned). **Applied `Open`s, honest-1 / honest-2**: `gate-g22` 33 / 20; `gate-g23` 28 / 32; `gate-g24` 0 / 1; `gate-g25` 28 / 23; `gate-g26` 36 / 17. **Copies**:
in `gate-g23` honest-1 learned 32 updates from honest-2's `Open`s and honest-2 29 from honest-1's (`ferrata_updater_updates_from_chain_total`, each daemon's last `/metrics`). **The turn clock**: every daemon's
last `/metrics` reports `ferrata_updater_turn_clock_is_height` 1.

Re-check: `python3 -I scripts/test_perf_records.py` (`TestGateR910` checks this record byte for byte against the
generators, the runs' settings, the one build, the reveals and the copies against the recorded updates, A3's figures,
the steps applied twice and every honest refused step's twin from the journals, the turn clock and the updates learned
from each daemon's last `/metrics`, and the operator guide's rows); the tables: `python3 -I bench/perf/summarize_gate.py
bench/perf/results/gate-g22 bench/perf/results/gate-g23 bench/perf/results/gate-g24 bench/perf/results/gate-g25
bench/perf/results/gate-g26` and `python3 -I bench/perf/gate_abandoned.py` on the same five. `gate_abandoned.py` classes a
later `Open` sent once the other job's block was published, before it was final, as one a read would have stopped: for
`gate-g23` that holds only for a daemon that knows the other's copy, which it learns from that `Open` once its block is
final, so there the class `served` is contention too.

## Run `gate-g22`

```
date: 2026-11-22T18:01:36Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 ac1c2df1864f8e226664c15b9ec8768f3feb356bd9a57f5fecce7e21c31736d0
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0 GATE_POLLS=2,6 GATE_COPIES=0 GATE_ADV_POLL=2
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 102 | 5 | 0 | 314,067,138 | 8,858,174 | 365,749 | 15,695 | 2,515,463,096 | 70,990,952 |
| honest-2 | 74 | 7 | 0 | 229,771,108 | 9,139,684 | 319,545 | 39,554 | 1,840,725,224 | 73,433,904 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 17,997,858 / 543,838,246 = **0.0331** (fees: 0.0332; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.575** (honest-2 0.425; the gate: 0.35 to 0.65).

Jobs with an applied step: 54; steps applied: 176; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 8 | 17,576,403 | 8 of 8 |
| honest daemons | adopt0 | 1 | 1 | 354,442 | 1 of 1 |
| honest daemons | miller | 81 | 3 | 67,013 | 3 of 3 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 314,067,138 | 0 | 8,858,174 | 322,925,312 |
| honest-2 | 227,602,945 | 2,168,163 | 9,139,684 | 238,910,792 |
| all daemons | 541,670,083 | 2,168,163 | 17,997,858 | 561,836,104 |

Jobs: 54 with an applied step, 53 completed, **1 abandoned** (1 with Open applied).

All daemons together: the updates cost **1.037** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.033.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 20,166,021 / 541,670,083 = **0.0372** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (43 by their own job, 53 by a later update's): median **156 s**, p90 265 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 55 s, max 446 s; finished by honest-1 49, honest-2 47.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (156 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 2 | 2441, 2.6 | 6462 | 422.3 | 0.0654 | 57.8 | 34.9 | 4,667 / 82,999,611 | 6,291,314 / 48,103,215 |
| honest-2 | 6 | 871, 7.4 | 6462 | 326.2 | 0.0505 | 57.8 | 35.2 | 1,681 / 29,950,827 | 3,347,676 / 29,474,954 |

Per-daemon applied Opens, fee share, and updates learned on chain:
- honest-1: 33 applied Opens; fees / honest fees = 0.575; updates learned from other parties' Opens: 0.
- honest-2: 20 applied Opens; fees / honest fees = 0.425; updates learned from other parties' Opens: 0.

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g23`

```
date: 2026-11-22T18:01:12Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 ac1c2df1864f8e226664c15b9ec8768f3feb356bd9a57f5fecce7e21c31736d0
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0 GATE_POLLS=4,4 GATE_COPIES=1 GATE_ADV_POLL=2
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 88 | 2 | 0 | 271,233,283 | 12,222 | 343,881 | 990 | 2,172,617,312 | 105,696 |
| honest-2 | 91 | 1 | 0 | 273,695,022 | 21,237 | 361,591 | 399 | 2,192,452,904 | 173,088 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 33,459 / 544,928,305 = **0.0001** (fees: 0.0001; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.498** (honest-2 0.502; the gate: 0.35 to 0.65).

Jobs with an applied step: 61; steps applied: 179; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | miller | 81 | 1 | 21,237 | 1 of 1 |
| honest daemons | finish | 205 | 2 | 12,222 | 2 of 2 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 266,787,607 | 4,445,676 | 12,222 | 271,245,505 |
| honest-2 | 256,322,750 | 17,372,272 | 21,237 | 273,716,259 |
| all daemons | 523,110,357 | 21,817,948 | 33,459 | 544,961,764 |

Jobs: 61 with an applied step, 51 completed, **10 abandoned** (10 with Open applied).

All daemons together: the updates cost **1.042** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.000.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 21,851,407 / 523,110,357 = **0.0418** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (42 by their own job, 54 by a later update's): median **161 s**, p90 286 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 56 s, max 455 s; finished by honest-1 45, honest-2 51.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (161 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1321, 4.9 | 6510 | 284.6 | 0.0437 | 57.9 | 37.7 | 2,529 / 44,766,496 | 4,504,152 / 34,931,815 |
| honest-2 | 4 | 1308, 5.0 | 6510 | 286.0 | 0.0439 | 57.8 | 37.6 | 2,523 / 44,678,773 | 4,794,627 / 37,257,603 |

Per-daemon applied Opens, fee share, and updates learned on chain:
- honest-1: 28 applied Opens; fees / honest fees = 0.498; updates learned from other parties' Opens: 32.
- honest-2: 32 applied Opens; fees / honest fees = 0.502; updates learned from other parties' Opens: 29.

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g24`

```
date: 2026-11-22T18:01:35Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 ac1c2df1864f8e226664c15b9ec8768f3feb356bd9a57f5fecce7e21c31736d0
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=1 GATE_POLLS=4,4 GATE_COPIES=0 GATE_ADV_POLL=2
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adversary | 304 | 33 | 0 | 926,845,221 | 3,034,426 | 884,246 | 42,936 | 7,421,835,736 | 24,618,896 |
| honest-1 | 16 | 29 | 0 | 55,135,637 | 55,072,233 | 31,605 | 113,681 | 441,337,936 | 441,487,312 |
| honest-2 | 17 | 29 | 0 | 58,504,548 | 53,210,576 | 11,331 | 115,343 | 468,127,032 | 426,607,352 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 108,282,809 / 113,640,185 = **0.9529** (fees: 0.9545; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.497** (honest-2 0.503; the gate: 0.35 to 0.65).

Jobs with an applied step: 107; steps applied: 337; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 49 | 107,470,100 | 49 of 49 |
| adversary | open | 207 | 1 | 2,216,222 | 1 of 1 |
| honest daemons | adopt1 | 84 | 2 | 709,428 | 2 of 2 |
| adversary | miller | 81 | 18 | 384,319 | 18 of 18 |
| adversary | adopt0 | 1 | 1 | 354,442 | 1 of 1 |
| honest daemons | miller | 81 | 4 | 84,948 | 4 of 4 |
| adversary | finish | 205 | 13 | 79,443 | 13 of 13 |
| honest daemons | finish | 205 | 3 | 18,333 | 3 of 3 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| adversary | 926,845,221 | 0 | 3,034,426 | 929,879,647 |
| honest-1 | 55,135,637 | 0 | 55,072,233 | 110,207,870 |
| honest-2 | 58,504,548 | 0 | 53,210,576 | 111,715,124 |
| all daemons | 1,040,485,406 | 0 | 111,317,235 | 1,151,802,641 |

Jobs: 107 with an applied step, 107 completed, **0 abandoned**.

All daemons together: the updates cost **1.107** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.107.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 108,282,809 / 113,640,185 = **0.9529** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (96 by their own job, 0 by a later update's): median **35 s**, p90 45 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 35 s, max 56 s; finished by adversary 84, honest-1 4, honest-2 8.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; this run's median latency (35 s) is shorter than the reveal step.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| adversary | 2 | 1383, 4.6 | 6391 | 396.2 | 0.0620 | 57.8 | 37.8 | 2,313 / 34,507,786 | 7,305,256 / 57,743,896 |
| honest-1 | 4 | 1377, 4.6 | 6391 | 400.8 | 0.0627 | 57.8 | 36.9 | 2,459 / 38,450,318 | 2,151,857 / 23,133,653 |
| honest-2 | 4 | 1370, 4.7 | 6391 | 401.5 | 0.0628 | 57.8 | 36.7 | 2,467 / 38,567,785 | 2,162,339 / 22,520,012 |

Per-daemon applied Opens, fee share, and updates learned on chain:
- adversary: 105 applied Opens; fees / honest fees = 4.189 (the adversary's numerator is excluded from the honest total); updates learned from other parties' Opens: 0.
- honest-1: 0 applied Opens; fees / honest fees = 0.497; updates learned from other parties' Opens: 0.
- honest-2: 1 applied Opens; fees / honest fees = 0.503; updates learned from other parties' Opens: 0.

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g25`

```
date: 2026-11-22T18:01:16Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 ac1c2df1864f8e226664c15b9ec8768f3feb356bd9a57f5fecce7e21c31736d0
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0 GATE_POLLS=4,4 GATE_COPIES=0 GATE_ADV_POLL=2
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 99 | 7 | 0 | 309,092,912 | 10,903,743 | 449,303 | 17,679 | 2,476,337,720 | 87,371,376 |
| honest-2 | 71 | 11 | 0 | 216,318,250 | 17,980,570 | 226,821 | 54,694 | 1,732,360,568 | 144,282,112 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 28,884,313 / 525,411,162 = **0.0550** (fees: 0.0550; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.577** (honest-2 0.423; the gate: 0.35 to 0.65).

Jobs with an applied step: 52; steps applied: 170; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 13 | 28,473,524 | 13 of 13 |
| honest daemons | adopt0 | 1 | 1 | 354,442 | 1 of 1 |
| honest daemons | miller | 81 | 2 | 44,125 | 2 of 2 |
| honest daemons | finish | 205 | 2 | 12,222 | 2 of 2 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 306,854,154 | 2,238,758 | 10,903,743 | 319,996,655 |
| honest-2 | 216,318,250 | 0 | 17,980,570 | 234,298,820 |
| all daemons | 523,172,404 | 2,238,758 | 28,884,313 | 554,295,475 |

Jobs: 52 with an applied step, 51 completed, **1 abandoned** (1 with Open applied).

All daemons together: the updates cost **1.059** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.055.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 31,123,071 / 523,172,404 = **0.0595** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (41 by their own job, 55 by a later update's): median **165 s**, p90 285 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 44 s, max 384 s; finished by honest-1 55, honest-2 41.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (165 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1238, 5.2 | 6398 | 349.3 | 0.0546 | 57.6 | 36.9 | 2,385 / 42,728,511 | 4,478,732 / 38,106,575 |
| honest-2 | 4 | 1295, 4.9 | 6398 | 351.1 | 0.0549 | 57.8 | 36.6 | 2,493 / 44,424,088 | 4,052,814 / 31,198,071 |

Per-daemon applied Opens, fee share, and updates learned on chain:
- honest-1: 28 applied Opens; fees / honest fees = 0.577; updates learned from other parties' Opens: 0.
- honest-2: 23 applied Opens; fees / honest fees = 0.423; updates learned from other parties' Opens: 0.

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g26`

```
date: 2026-11-22T18:01:26Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 ac1c2df1864f8e226664c15b9ec8768f3feb356bd9a57f5fecce7e21c31736d0
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0 GATE_POLLS=4,4 GATE_COPIES=0 GATE_ADV_POLL=2
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 107 | 9 | 0 | 327,932,361 | 8,823,512 | 403,426 | 15,393 | 2,626,686,296 | 70,711,240 |
| honest-2 | 71 | 13 | 0 | 223,048,265 | 17,850,582 | 283,382 | 54,050 | 1,786,653,176 | 143,237,056 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 26,674,094 / 550,980,626 = **0.0484** (fees: 0.0485; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.583** (honest-2 0.417; the gate: 0.35 to 0.65).

Jobs with an applied step: 54; steps applied: 178; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 12 | 26,155,469 | 12 of 12 |
| honest daemons | adopt0 | 1 | 1 | 354,442 | 1 of 1 |
| honest daemons | miller | 81 | 7 | 151,961 | 7 of 7 |
| honest daemons | finish | 205 | 2 | 12,222 | 2 of 2 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 327,932,361 | 0 | 8,823,512 | 336,755,873 |
| honest-2 | 223,048,265 | 0 | 17,850,582 | 240,898,847 |
| all daemons | 550,980,626 | 0 | 26,674,094 | 577,654,720 |

Jobs: 54 with an applied step, 54 completed, **0 abandoned**.

All daemons together: the updates cost **1.048** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.048.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 26,674,094 / 550,980,626 = **0.0484** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (44 by their own job, 52 by a later update's): median **145 s**, p90 255 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 55 s, max 405 s; finished by honest-1 54, honest-2 42.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (145 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1269, 5.1 | 6477 | 274.5 | 0.0424 | 57.8 | 36.1 | 2,455 / 43,384,062 | 4,483,536 / 36,591,004 |
| honest-2 | 4 | 1342, 4.8 | 6477 | 278.5 | 0.0430 | 57.8 | 36.0 | 2,585 / 45,752,135 | 4,021,135 / 32,845,898 |

Per-daemon applied Opens, fee share, and updates learned on chain:
- honest-1: 36 applied Opens; fees / honest fees = 0.583; updates learned from other parties' Opens: 0.
- honest-2: 17 applied Opens; fees / honest fees = 0.417; updates learned from other parties' Opens: 0.

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## The runs side by side

| run | honest daemons' deferral window: rounds of a nominal s (measured s per round, all daemons) | adversary | reveal step s | jobs: completed / abandoned | steps applied twice | finality updates covered: own job / a later one | latency median / p90 (s) | execution gas, all daemons, per recorded update revealed | gas multiple: total / completed jobs ((applied + refused) / applied) | honest waste: refused and abandoned / completed | A3's ratio: honest refused / applied (gate at most 0.10) | honest-1's share of the honest fees (gate 0.35–0.65) |
|---|---|---|---:|---|---:|---|---|---:|---|---:|---:|---:|
| gate-g22 | 8 of 4 (2.6–7.4) | no | 60 | 53 / 1 | 0 | 43 / 53 | 156 / 265 | 5,300,341 | **1.037** (1.033) | 0.0372 | 0.0331 | 0.575 |
| gate-g23 | 8 of 4 (4.9–5.0) | no | 60 | 51 / 10 | 0 | 42 / 54 | 161 / 286 | 5,141,149 | **1.042** (1.000) | 0.0418 | 0.0001 | 0.498 |
| gate-g24 | 8 of 4 (4.6–4.7) | yes | 60 | 107 / 0 | 0 | 96 / 0 | 35 / 45 | 10,866,063 | **1.107** (1.107) | 0.9529 | 0.9529 | 0.497 |
| gate-g25 | 8 of 4 (4.9–5.2) | no | 60 | 51 / 1 | 0 | 41 / 55 | 165 / 285 | 5,229,203 | **1.059** (1.055) | 0.0595 | 0.0550 | 0.577 |
| gate-g26 | 8 of 4 (4.8–5.1) | no | 60 | 54 / 0 | 0 | 44 / 52 | 145 / 255 | 5,449,573 | **1.048** (1.048) | 0.0484 | 0.0484 | 0.583 |
### Abandoned jobs of `gate-g22`

| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |
|---|---|---|---|---|---:|---:|---|---:|
| 0x55931913 (15637216) | honest-2, 158 | Open (2,168,163) | 0x6de3cbaa (15637792) | honest-1, 158 | 18:29:36 | 18:29:44 / 18:29:50 | same block | 0 |

`gate-g22`: 1 abandoned jobs, 2,168,163 execution gas (applied and refused steps): same block 1 (2,168,163), in flight 0 (0), served 0 (0), final 0 (0); **0 with the later `Open` sent once a job open on the store was published** (0); of the 1 whose `Open`s no read could have avoided, 0 with later steps sent once the overtaking job was published and led them (0 steps, 0).

### Abandoned jobs of `gate-g23`

| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |
|---|---|---|---|---|---:|---:|---|---:|
| 0xf2963ccd (15636000) | honest-2, 141 | Open (2,140,642) | 0x4f9ce32a (15636000) | honest-1, 141 | 18:25:50 | 18:25:52 / 18:25:58 | same block | 0 |
| 0xbe938b55 (15637792) | honest-2, 161 | Open (2,140,607) | 0x6de3cbaa (15637792) | honest-1, 160 | 18:29:03 | 18:29:02 / 18:29:08 | served | 0 |
| 0xcd5292f9 (15643808) | honest-1, 229 | Open (2,222,689) | 0x08036a59 (15644416) | honest-2, 230 | 18:40:34 | 18:40:32 / 18:40:38 | served | 0 |
| 0x99816d5a (15646208) | honest-2, 253 | Open (2,221,959) | 0x0f8a2ca6 (15646720) | honest-1, 254 | 18:44:34 | 18:44:32 / 18:44:38 | served | 0 |
| 0x1c5abf3d (15647392) | honest-2, 266 | Open (2,140,375) | 0xa3e78580 (15647392) | honest-1, 266 | 18:46:33 | 18:46:42 / 18:46:48 | same block | 0 |
| 0x748d1ec7 (15648608) | honest-2, 279 | Open (2,168,632) | 0xda31d43c (15648608) | honest-1, 278 | 18:48:43 | 18:48:42 / 18:48:48 | served | 0 |
| 0xcc340c53 (15653408) | honest-2, 326 | Open (2,168,826) | 0xd5d5dd15 (15653408) | honest-1, 326 | 18:56:35 | 18:56:42 / 18:56:48 | same block | 0 |
| 0x28c15911 (15666016) | honest-2, 469 | Open (2,221,896) | 0x2f36c403 (15666016) | honest-1, 469 | 19:20:29 | 19:20:32 / 19:20:38 | same block | 0 |
| 0x723d8bae (15668992) | honest-1, 499 | Open (2,222,987) | 0x4b116a68 (15669600) | honest-2, 500 | 19:25:31 | 19:25:32 / 19:25:38 | ambiguous | 0 |
| 0x5f74522a (15672608) | honest-2, 542 | Open (2,169,335) | 0xdbf8ca48 (15672608) | honest-1, 542 | 19:32:32 | 19:32:42 / 19:32:48 | same block | 0 |

`gate-g23`: 10 abandoned jobs, 21,817,948 execution gas (applied and refused steps): same block 5 (10,841,074), in flight 0 (0), ambiguous 1 (2,222,987), served 4 (8,753,887), final 0 (0); **4 with the later `Open` sent once a job open on the store was published** (8,753,887); of the 5 whose `Open`s no read could have avoided, 0 with later steps sent once the overtaking job was published and led them (0 steps, 0).

### Abandoned jobs of `gate-g24`

None.

### Abandoned jobs of `gate-g25`

| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |
|---|---|---|---|---|---:|---:|---|---:|
| 0x157b852c (15654944) | honest-1, 350 | Open (2,238,758) | 0xd5ae3d51 (15655200) | honest-2, 350 | 19:02:13 | 19:02:19 / 19:02:25 | same block | 0 |

`gate-g25`: 1 abandoned jobs, 2,238,758 execution gas (applied and refused steps): same block 1 (2,238,758), in flight 0 (0), served 0 (0), final 0 (0); **0 with the later `Open` sent once a job open on the store was published** (0); of the 1 whose `Open`s no read could have avoided, 0 with later steps sent once the overtaking job was published and led them (0 steps, 0).

### Abandoned jobs of `gate-g26`

None.
