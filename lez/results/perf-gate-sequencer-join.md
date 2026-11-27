# RFP-021 R.9 with join or wait: two updater daemons on one instance, 106 updates, windows 8 and 30, two builds

**Measured** on the build boxes (Modal containers), on LEZ v0.4.0-rc1's own sequencer with a Logos Blockchain node
(LEZ's local configuration; nothing is sent to any public network), exactly as the runs of
[`perf-gate-sequencer-100.md`](perf-gate-sequencer-100.md) (G6 and G7), which describe the method, the replay and the
terms: each run reveals 106 of the lab's recorded mainnet updates (96 finality updates and 10 sync-committee period
updates), one every 60 s, to two honest `ferrata-updater` daemons polling every 4 s, no adversary, the clock set back
five days with libfaketime (`FAKETIME=-5d`, monotonic clocks real; the dates in these records are the faked ones, the
runs were made on 26 and 27 November 2026), `bash bench/perf/gate-sequencer.sh NAME` with `GATE_ENTRIES=106
GATE_STEP=60 GATE_POLL=4 GATE_ADVERSARY=0 GATE_DEADLINE_MIN=150` and the window `GATE_WINDOW` 8 or 30 (each run's
`machine.txt`). What differs from G6 and G7 is the updater: it now joins or waits (`policy.join_wait_rounds`, default
`2 × defer_window + 10`; [`lez/updater/README.md`](../updater/README.md) item 7, [ADR
0046](../../docs/decisions/0046-updater-join-or-wait.md)). Two builds ran, each built once on the window-8 box and
copied to the window-30 box, so each pair compares the windows on one binary: `gate-g8` and `gate-g9` the first build
(commit `b9377f5f`, updater sha256 `d42374d3…`), `gate-g10` and `gate-g11` the final one (commit `39a87ce4`,
updater sha256 `0354a758…`). The first build differed from the final policy: at as many steps the older update's
job led, a failed read of another job ended the round in an error, a wait that ended did not restart the step's turn, a
carrier asked its sources only in the rounds it would send, the cap was counted per step, the updates it knew were not
kept by kind, and a job it had opened and left held it back. The commits after `39a87ce4` change the policy only on paths these runs did not take
(a wait ended by a stall or by the cap: no job was passed and no cap was reached, the counters below; copies of an
update: one replay served both daemons the same bytes; a source refused; a step held after a refusal) and the
classification of a failed read (only a read that fails because a block not yet final holds a transaction of the job is
waited for; in these runs no other read failed). The machine paths in the logs are written `<repo>`.

**R.9.** In the four runs one job was abandoned, in `gate-g10`, opened in the block of the job that overtook it (two
daemons each opened the job of a different update in block 236, neither in any block when the other sent its `Open`;
the one behind was not continued: abandoned with its `Open` alone, 2,249,783 execution gas); 76, 30, 76 and 30 jobs
completed. G6 and G7 abandoned 23 and 15, in each of which the later of the two `Open`s was sent once the other job's
block was published (the classification at the end: a daemon reads a job from its block's publication, as a failed
read until the block is final, then as the job). In all four runs every refused
step shared its block with the same step of the same job, applied, so each was sent before its twin was in any block
(the generator's "in the same block as the applied step" column).
So the gas besides the completed jobs' is refused duplicates and that one `Open`: all the daemons' gas is **1.077**,
**1.022**, **1.088** and **1.036** times the completed jobs' (G6 and G7: 1.289 and 1.249).

**Against A3's gate** (refused / applied at most 0.10; each honest daemon's share of the honest fees 0.35–0.65):
`gate-g8` (window 8) **0.0770** and honest-1's share **0.530**; `gate-g9` (window 30) **0.0222** and honest-1's share **0.617**;
`gate-g10` (window 8) **0.0853** and honest-1's share **0.541**; `gate-g11` (window 30) **0.0358** and honest-1's share **0.577**.
No step was applied twice. With join or wait the daemon that opens a job carries it and the other waits, so a job's
fees fall on its opener, and the share varies more from run to run (G9: honest-1 opened most of the jobs).

**Latency (R.10)**, a revealed finality update to the first `Finish` that leaves the instance at least as fresh, all 96
covered in each run: median / p90 106 / 167 s and 105 / 157 s at window 8 (G6: 105 / 175 s), 341 / 636 s and
326 / 595 s at window 30 (G7: 275 / 546 s): at the long window a daemon now waits for the other's job instead of
racing a newer update. Execution gas per revealed update: 7.7M and 7.7M at window 8, 3.2M and 3.2M at 30 (G6, G7:
8.9M, 4.1M).

**The policy's counters** (each daemon's last `/metrics`): rounds a step whose turn had come waited for another job,
honest-1 + honest-2: 197, 465, 223 and 368; jobs passed after they stopped moving, waits ended by the cap: none in
any run; joins: none in G10 and G11 (no copies: one replay). Every round that ended in an error in G10 and G11 (223 and
127) was a read the sequencer and the follower disagreed on, of an account a block not yet final had changed, as in G6
and G7.

Re-check: `python3 -I scripts/test_perf_records.py` (`TestGateJoin` recomputes A3's figures and limits, the twins of
the refused steps, the abandoned jobs and when their `Open`s were sent, the builds, and the operator guide's rows, from
the raw files); the tables: `python3 -I bench/perf/summarize_gate.py bench/perf/results/gate-g8
bench/perf/results/gate-g9 bench/perf/results/gate-g10 bench/perf/results/gate-g11` and `python3 -I
bench/perf/gate_abandoned.py bench/perf/results/gate-g6 bench/perf/results/gate-g7 bench/perf/results/gate-g8
bench/perf/results/gate-g9 bench/perf/results/gate-g10 bench/perf/results/gate-g11`.

## Run `gate-g8`

```
date: 2026-11-21T23:58:24Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 d42374d33c8a9fd3cbad89065abe516bebecd6ccc93f8cec5ad67c3b9e54806e
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 130 | 34 | 0 | 401,319,471 | 29,179,853 | 449,611 | 81,444 | 3,214,152,656 | 234,090,376 |
| honest-2 | 114 | 29 | 0 | 352,704,692 | 28,858,834 | 335,002 | 75,694 | 2,824,317,552 | 231,476,224 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 58,038,687 / 754,024,163 = **0.0770** (fees: 0.0771; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.530** (honest-2 0.470; the gate: 0.35 to 0.65).

Jobs with an applied step: 76; steps applied: 244; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 26 | 56,851,900 | 26 of 26 |
| honest daemons | miller | 81 | 17 | 367,633 | 17 of 17 |
| honest daemons | adopt1 | 84 | 1 | 354,714 | 1 of 1 |
| honest daemons | adopt0 | 1 | 1 | 354,442 | 1 of 1 |
| honest daemons | finish | 205 | 18 | 109,998 | 18 of 18 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 401,319,471 | 0 | 29,179,853 | 430,499,324 |
| honest-2 | 352,704,692 | 0 | 28,858,834 | 381,563,526 |
| all daemons | 754,024,163 | 0 | 58,038,687 | 812,062,850 |

Jobs: 76 with an applied step, 76 completed, **0 abandoned**.

All daemons together: the updates cost **1.077** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.077.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 58,038,687 / 754,024,163 = **0.0770** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (65 by their own job, 31 by a later update's): median **106 s**, p90 167 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 45 s, max 276 s; finished by honest-1 41, honest-2 55.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (106 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1048, 6.2 | 6461 | 410.2 | 0.0635 | 57.9 | 38.2 | 1,211 / 20,506,306 | 5,085,399 / 40,666,474 |
| honest-2 | 4 | 1113, 5.8 | 6461 | 412.4 | 0.0638 | 57.8 | 38.4 | 1,321 / 22,394,459 | 4,636,055 / 36,722,873 |

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g9`

```
date: 2026-11-21T23:59:42Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 db185c3df2e1efd9e23a2f1070cc100898e36bd86c3ebefb4caceb2c87f90e64
ferrata-updater sha256 d42374d33c8a9fd3cbad89065abe516bebecd6ccc93f8cec5ad67c3b9e54806e
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=30 GATE_POLL=4 GATE_ADVERSARY=0
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 67 | 2 | 0 | 204,991,876 | 2,597,254 | 338,426 | 29,290 | 1,642,642,416 | 21,012,352 |
| honest-2 | 39 | 4 | 0 | 124,202,129 | 4,716,561 | 239,751 | 32,683 | 995,535,040 | 37,993,952 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 7,313,815 / 329,194,005 = **0.0222** (fees: 0.0224; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.617** (honest-2 0.383; the gate: 0.35 to 0.65).

Jobs with an applied step: 30; steps applied: 106; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 3 | 6,619,348 | 3 of 3 |
| honest daemons | adopt0 | 1 | 1 | 354,442 | 1 of 1 |
| honest daemons | adopt1 | 205 | 1 | 333,914 | 1 of 1 |
| honest daemons | finish | 205 | 1 | 6,111 | 1 of 1 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 204,991,876 | 0 | 2,597,254 | 207,589,130 |
| honest-2 | 124,202,129 | 0 | 4,716,561 | 128,918,690 |
| all daemons | 329,194,005 | 0 | 7,313,815 | 336,507,820 |

Jobs: 30 with an applied step, 30 completed, **0 abandoned**.

All daemons together: the updates cost **1.022** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.022.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 7,313,815 / 329,194,005 = **0.0222** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (21 by their own job, 75 by a later update's): median **341 s**, p90 636 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 56 s, max 935 s; finished by honest-1 75, honest-2 21.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (341 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1428, 4.7 | 6690 | 199.6 | 0.0298 | 57.6 | 37.3 | 1,005 / 21,085,992 | 4,584,211 / 36,922,556 |
| honest-2 | 4 | 1490, 4.5 | 6690 | 213.3 | 0.0319 | 57.9 | 37.1 | 1,759 / 35,350,846 | 4,400,050 / 32,348,877 |

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g10`

```
date: 2026-11-22T01:58:57Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 0354a75821e0acfa83d7df0cbba184c85c08cfb1dc326895e9d24942285f55e2
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 130 | 35 | 0 | 406,666,165 | 37,647,542 | 416,648 | 70,865 | 3,256,662,504 | 301,747,256 |
| honest-2 | 115 | 27 | 0 | 349,771,442 | 26,840,280 | 374,593 | 74,059 | 2,801,168,280 | 215,314,712 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 64,487,822 / 756,437,607 = **0.0853** (fees: 0.0854; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.541** (honest-2 0.459; the gate: 0.35 to 0.65).

Jobs with an applied step: 77; steps applied: 245; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 29 | 63,690,587 | 29 of 29 |
| honest daemons | adopt1 | 84 | 1 | 354,714 | 1 of 1 |
| honest daemons | miller | 81 | 16 | 344,745 | 16 of 16 |
| honest daemons | finish | 205 | 16 | 97,776 | 16 of 16 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 406,666,165 | 0 | 37,647,542 | 444,313,707 |
| honest-2 | 347,521,659 | 2,249,783 | 26,840,280 | 376,611,722 |
| all daemons | 754,187,824 | 2,249,783 | 64,487,822 | 820,925,429 |

Jobs: 77 with an applied step, 76 completed, **1 abandoned** (1 with Open applied).

All daemons together: the updates cost **1.088** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.085.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 66,737,605 / 754,187,824 = **0.0885** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (65 by their own job, 31 by a later update's): median **105 s**, p90 157 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 45 s, max 245 s; finished by honest-1 54, honest-2 42.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (105 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1042, 6.1 | 6399 | 429.0 | 0.0670 | 57.9 | 34.2 | 2,043 / 34,466,528 | 4,976,302 / 39,471,633 |
| honest-2 | 4 | 1103, 5.8 | 6399 | 428.9 | 0.0670 | 57.9 | 34.4 | 2,155 / 36,169,000 | 4,601,177 / 37,478,622 |

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g11`

```
date: 2026-11-22T02:00:07Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 db185c3df2e1efd9e23a2f1070cc100898e36bd86c3ebefb4caceb2c87f90e64
ferrata-updater sha256 0354a75821e0acfa83d7df0cbba184c85c08cfb1dc326895e9d24942285f55e2
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=30 GATE_POLL=4 GATE_ADVERSARY=0
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 62 | 4 | 0 | 191,947,265 | 4,761,015 | 323,021 | 32,949 | 1,538,162,288 | 38,351,712 |
| honest-2 | 44 | 8 | 0 | 137,245,967 | 7,038,047 | 255,120 | 38,664 | 1,100,008,696 | 56,613,688 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 11,799,062 / 329,193,232 = **0.0358** (fees: 0.0360; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.577** (honest-2 0.423; the gate: 0.35 to 0.65).

Jobs with an applied step: 30; steps applied: 106; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 5 | 11,047,976 | 5 of 5 |
| honest daemons | adopt1 | 84 | 1 | 354,714 | 1 of 1 |
| honest daemons | adopt1 | 205 | 1 | 333,914 | 1 of 1 |
| honest daemons | miller | 81 | 2 | 44,125 | 2 of 2 |
| honest daemons | finish | 205 | 3 | 18,333 | 3 of 3 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 191,947,265 | 0 | 4,761,015 | 196,708,280 |
| honest-2 | 137,245,967 | 0 | 7,038,047 | 144,284,014 |
| all daemons | 329,193,232 | 0 | 11,799,062 | 340,992,294 |

Jobs: 30 with an applied step, 30 completed, **0 abandoned**.

All daemons together: the updates cost **1.036** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.036.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 11,799,062 / 329,193,232 = **0.0358** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (21 by their own job, 75 by a later update's): median **326 s**, p90 595 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 86 s, max 925 s; finished by honest-1 61, honest-2 35.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (326 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1390, 4.7 | 6568 | 229.7 | 0.0350 | 57.8 | 35.2 | 2,663 / 51,152,699 | 4,641,721 / 36,771,859 |
| honest-2 | 4 | 1434, 4.6 | 6568 | 233.2 | 0.0355 | 57.8 | 35.6 | 2,749 / 52,847,714 | 4,157,683 / 33,371,041 |

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## The runs side by side

| run | honest daemons' deferral window: rounds of a nominal s (measured s per round, all daemons) | adversary | reveal step s | jobs: completed / abandoned | steps applied twice | finality updates covered: own job / a later one | latency median / p90 (s) | execution gas, all daemons, per recorded update revealed | gas multiple: total / completed jobs ((applied + refused) / applied) | honest waste: refused and abandoned / completed | A3's ratio: honest refused / applied (gate at most 0.10) | honest-1's share of the honest fees (gate 0.35–0.65) |
|---|---|---|---:|---|---:|---|---|---:|---|---:|---:|---:|
| gate-g8 | 8 of 4 (5.8–6.2) | no | 60 | 76 / 0 | 0 | 65 / 31 | 106 / 167 | 7,660,970 | **1.077** (1.077) | 0.0770 | 0.0770 | 0.530 |
| gate-g9 | 30 of 4 (4.5–4.7) | no | 60 | 30 / 0 | 0 | 21 / 75 | 341 / 636 | 3,174,602 | **1.022** (1.022) | 0.0222 | 0.0222 | 0.617 |
| gate-g10 | 8 of 4 (5.8–6.1) | no | 60 | 76 / 1 | 0 | 65 / 31 | 105 / 157 | 7,744,580 | **1.088** (1.085) | 0.0885 | 0.0853 | 0.541 |
| gate-g11 | 30 of 4 (4.6–4.7) | no | 60 | 30 / 0 | 0 | 21 / 75 | 326 / 595 | 3,216,908 | **1.036** (1.036) | 0.0358 | 0.0358 | 0.577 |
### Abandoned jobs of `gate-g6`

| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |
|---|---|---|---|---|---:|---:|---|---:|
| 0x62a05e3c (15630016) | honest-2, 68 | Open + Miller (5,501,875) | 0x8e4dd58b (15629408) | honest-1, 67 | 20:38:59 | 20:38:58 / 20:39:04 | served | 1 (3,308,048) |
| 0x9d1b0f11 (15630592) | honest-2, 80 | Open + Miller (5,503,728) | 0x82cd4f08 (15630336) | honest-1, 75 | 20:41:03 | 20:40:18 / 20:40:24 | final | 1 (3,308,048) |
| 0x5d56cf21 (15631808) | honest-1, 98 | Open + Miller (5,477,060) | 0xc4f6b89d (15631200) | honest-2, 96 | 20:44:02 | 20:43:48 / 20:43:54 | final | 1 (3,308,048) |
| 0x83a1e348 (15634208) | honest-2, 122 | Open (2,168,889) | 0x2ba97116 (15633600) | honest-1, 119 | 20:48:02 | 20:47:38 / 20:47:44 | final | 0 |
| 0x7a6348a2 (15636608) | honest-1, 146 | Open + Miller (5,476,039) | 0x04ae51a0 (15636000) | honest-2, 142 | 20:52:02 | 20:51:28 / 20:51:34 | final | 1 (3,308,048) |
| 0x2070711c (15638592) | honest-2, 175 | Open + Miller + adopt 1/2 + adopt 2/2 (12,383,274) | 0xc1aed0c7 (15639008) | honest-1, 178 | 20:57:18 | 20:56:58 / 20:57:04 | final | 0 |
| 0xf338f735 (15644416) | honest-2, 235 | Open + Miller (5,557,831); 1 refused (3,683,081) | 0x2de3c5cd (15644992) | honest-1, 236 | 21:07:06 | 21:06:58 / 21:07:04 | final | 2 (6,991,129) |
| 0xfa6f2c10 (15645600) | honest-1, 246 | Open + Miller (5,529,308) | 0x43bff404 (15646208) | honest-2, 248 | 21:09:00 | 21:08:48 / 21:08:54 | final | 1 (3,308,048) |
| 0x6c44219e (15646720) | honest-2, 256 | Open + Miller + adopt 1/2 + adopt 2/2 (12,298,973); 1 refused (3,689,678) | 0x43c69e36 (15646816) | honest-1, 260 | 21:10:59 | 21:10:28 / 21:10:34 | final | 0 |
| 0x9f9193f1 (15649216) | honest-2, 284 | Open + Miller (5,477,176); 1 refused (3,683,081) | 0xeb0a4302 (15648608) | honest-1, 283 | 21:15:02 | 21:14:58 / 21:15:04 | served | 1 (3,308,048) |
| 0x65a945cb (15650400) | honest-1, 296 | Open (2,168,122) | 0xf5be393c (15649792) | honest-2, 291 | 21:17:00 | 21:16:18 / 21:16:24 | final | 0 |
| 0x886285b0 (15651008) | honest-2, 302 | Open + Miller (5,476,824) | 0x45dc4989 (15650400) | honest-1, 299 | 21:18:02 | 21:17:38 / 21:17:44 | final | 1 (3,308,048) |
| 0xced5890d (15652192) | honest-2, 314 | Open + Miller (5,475,747) | 0x3636d3eb (15651616) | honest-1, 310 | 21:20:02 | 21:19:28 / 21:19:34 | final | 1 (3,308,048) |
| 0x1befc164 (15654016) | honest-2, 332 | Open (2,168,248) | 0x2b7d42c1 (15653408) | honest-1, 327 | 21:22:59 | 21:22:18 / 21:22:24 | final | 0 |
| 0x57656943 (15654592) | honest-2, 338 | Open (2,195,350) | 0xf886f259 (15654016) | honest-1, 334 | 21:24:00 | 21:23:28 / 21:23:34 | final | 0 |
| 0xd3250acc (15663232) | honest-1, 434 | Open (2,186,345) | 0xcdb79ad0 (15663008) | honest-2, 430 | 21:39:58 | 21:39:28 / 21:39:34 | final | 0 |
| 0x3f806c6b (15663232) | honest-2, 437 | Open + Miller + adopt 1/2 + adopt 2/2 (12,328,901) | 0x3b88ea11 (15663616) | honest-1, 440 | 21:41:01 | 21:40:38 / 21:40:44 | final | 0 |
| 0x2bf34c3b (15668415) | honest-2, 488 | Open (2,222,465) | 0xf65dd0cd (15667808) | honest-1, 485 | 21:49:02 | 21:48:38 / 21:48:44 | final | 0 |
| 0x1290d223 (15668992) | honest-2, 499 | Open + Miller (5,531,035) | 0xfe62e37f (15669600) | honest-1, 500 | 21:50:59 | 21:50:58 / 21:51:04 | served | 1 (3,308,048) |
| 0x98759d16 (15675008) | honest-2, 566 | Open + Miller (5,474,975) | 0x7a91d448 (15674400) | honest-1, 564 | 22:02:01 | 22:01:48 / 22:01:54 | final | 0 |
| 0x5a847e80 (15675616) | honest-1, 573 | Open + Miller (5,476,821) | 0x0d46fcdd (15675008) | honest-2, 571 | 22:03:14 | 22:02:58 / 22:03:04 | final | 1 (3,308,048) |
| 0x5be0db97 (15678016) | honest-1, 596 | Open + Miller (5,476,583) | 0x519ebef0 (15677408) | honest-2, 595 | 22:07:02 | 22:06:58 / 22:07:04 | served | 1 (3,308,048) |
| 0x5f08e5c0 (15681600) | honest-1, 638 | Open + Miller (5,476,426) | 0xe3901b04 (15680992) | honest-2, 637 | 22:13:59 | 22:13:58 / 22:14:04 | served | 1 (3,308,048) |

`gate-g6`: 23 abandoned jobs, 138,087,835 execution gas (applied and refused steps): same block 0 (0), in flight 0 (0), served 5 (31,146,176), final 18 (106,941,659); **23 with the later `Open` sent once a job open on the store was published** (138,087,835); of the 0 whose `Open`s no read could have avoided, 0 with later steps sent once the overtaking job was published and led them (0 steps, 0).

### Abandoned jobs of `gate-g7`

| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |
|---|---|---|---|---|---:|---:|---|---:|
| 0xa6d7d1fe (15630720) | honest-2, 86 | Open + Miller (5,498,599) | 0xfb97de59 (15630592) | honest-1, 80 | 20:41:20 | 20:40:29 / 20:40:35 | final | 1 (3,313,372) |
| 0x5343d8c2 (15636000) | honest-2, 140 | Open + Miller (5,448,690) | 0xac92e0c5 (15635392) | honest-1, 139 | 20:50:24 | 20:50:19 / 20:50:25 | served | 0 |
| 0xb7b9c1d2 (15638592) | honest-2, 172 | Open + Miller + adopt 1/2 (8,965,085) | 0xde5e67c0 (15639008) | honest-1, 177 | 20:56:34 | 20:55:49 / 20:55:55 | final | 0 |
| 0x161b3659 (15644992) | honest-2, 236 | Open (2,221,283) | 0xc1807470 (15644416) | honest-1, 235 | 21:06:21 | 21:06:19 / 21:06:25 | served | 0 |
| 0x86db19b4 (15646720) | honest-2, 254 | Open (2,156,417) | 0xec1cfcb5 (15646208) | honest-1, 252 | 21:09:22 | 21:09:09 / 21:09:15 | final | 0 |
| 0x43c69e36 (15646816) | honest-1, 263 | Open (2,140,873) | 0xb28634ba (15647392) | honest-2, 266 | 21:11:21 | 21:10:59 / 21:11:05 | final | 0 |
| 0xced5890d (15652192) | honest-2, 315 | Open + Miller (5,475,747) | 0x3636d3eb (15651616) | honest-1, 311 | 21:19:35 | 21:18:59 / 21:19:05 | final | 1 (3,308,048) |
| 0xb2c1d73b (15654016) | honest-2, 337 | Open + Miller (5,476,296) | 0x82e78d49 (15654592) | honest-1, 343 | 21:24:14 | 21:23:19 / 21:23:25 | final | 1 (3,308,048) |
| 0x97671a47 (15661216) | honest-1, 410 | Open + Miller (5,530,906) | 0x1f5c8e5e (15660608) | honest-2, 404 | 21:35:22 | 21:34:29 / 21:34:35 | final | 1 (3,308,048) |
| 0x7541356e (15663232) | honest-2, 434 | Open + Miller (5,499,717) | 0x0bef8712 (15663008) | honest-1, 428 | 21:39:23 | 21:38:29 / 21:38:35 | final | 1 (3,313,372) |
| 0xcd5a2a76 (15669600) | honest-1, 500 | Open (2,195,092) | 0xc1dfab8c (15668992) | honest-2, 494 | 21:50:22 | 21:49:29 / 21:49:35 | final | 0 |
| 0x683a9fc9 (15670816) | honest-1, 512 | Open + Miller (5,557,954) | 0x1aaa4851 (15670208) | honest-2, 511 | 21:52:23 | 21:52:19 / 21:52:25 | served | 0 |
| 0xc4966f83 (15671392) | honest-2, 527 | Open + Miller (5,503,798) | 0x7c48533e (15672000) | honest-1, 530 | 21:55:22 | 21:54:59 / 21:55:05 | final | 1 (3,308,048) |
| 0x1079047e (15679520) | honest-1, 615 | Open + Miller + adopt 1/2 (8,910,064) | 0x196ba9a5 (15679200) | honest-2, 608 | 22:09:30 | 22:08:29 / 22:08:35 | final | 1 (3,313,372) |
| 0xcec0fb83 (15681600) | honest-1, 638 | Open (2,168,403) | 0x677de7e8 (15680992) | honest-2, 637 | 22:13:20 | 22:13:19 / 22:13:25 | served | 0 |

`gate-g7`: 15 abandoned jobs, 72,748,924 execution gas (applied and refused steps): same block 0 (0), in flight 0 (0), served 4 (15,396,330), final 11 (57,352,594); **15 with the later `Open` sent once a job open on the store was published** (72,748,924); of the 0 whose `Open`s no read could have avoided, 0 with later steps sent once the overtaking job was published and led them (0 steps, 0).

### Abandoned jobs of `gate-g8`

None.

### Abandoned jobs of `gate-g9`

None.

### Abandoned jobs of `gate-g10`

| abandoned job (finalized slot) | opened by, in block | steps applied (execution gas); refused | overtaken by (finalized slot) | its `Open` by, in block | the later `Open` sent | the first block of a job open on the store, published / final | the later `Open` | later steps sent once the overtaking job was published and led |
|---|---|---|---|---|---:|---:|---|---:|
| 0xf338f735 (15644416) | honest-2, 236 | Open (2,249,783) | 0x2de3c5cd (15644992) | honest-1, 236 | 02:40:47 | 02:40:49 / 02:40:55 | same block | 0 |

`gate-g10`: 1 abandoned jobs, 2,249,783 execution gas (applied and refused steps): same block 1 (2,249,783), in flight 0 (0), served 0 (0), final 0 (0); **0 with the later `Open` sent once a job open on the store was published** (0); of the 1 whose `Open`s no read could have avoided, 0 with later steps sent once the overtaking job was published and led them (0 steps, 0).

### Abandoned jobs of `gate-g11`

None.
