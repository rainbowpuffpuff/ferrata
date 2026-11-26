# RFP-021 R.9 and R.10 at a hundred updates: two updater daemons on one instance, windows 8 and 30

**Measured** on the build box (a Modal container), on LEZ v0.4.0-rc1's own sequencer with a Logos Blockchain node
(LEZ's local configuration; nothing is sent to any public network), as the runs of
[`perf-gate-sequencer.md`](perf-gate-sequencer.md) (which describes the method, the replay and the terms) but at A3's
size: each run reveals 106 of the lab's recorded mainnet updates (96 finality updates and 10 sync-committee period
updates; `GATE_ENTRIES=106`; A3's gate asks 100), one every 60 s (`GATE_STEP=60`), to two honest `ferrata-updater`
daemons polling every 4 s and no adversary (`GATE_ADVERSARY=0`); `gate-g6` at the default deferral window of 8 rounds,
`gate-g7` at 30, everything else equal (each run's `machine.txt`). Each run is `bash bench/perf/gate-sequencer.sh NAME`
with those parameters, after `bench/perf/gate-build.sh` built the updater and the zone's bundle on that run's box
(their sha256 in `machine.txt`). The machine paths in the logs are written `<repo>` (the worktree on the box).

**The clock is set back.** The recorded chain's bootstrap left its weak-subjectivity period on 24 November 2026, so
each run sets every process's wall clock back five days with libfaketime (`FAKETIME=-5d`, monotonic clocks real);
`machine.txt` records it, and the dates in these records are the faked ones (the runs were made on 26 November).

**Against A3's gate** ([`attestation.md`](../../docs/design/attestation.md) §9: the honest daemons' execution gas on
refused steps at most 0.10 of their gas on applied steps, each honest daemon paying 35–65% of the honest fees):
`gate-g6` (window 8) **0.0989** and honest-1's share **0.522**; `gate-g7` (window 30) **0.0327** and **0.521**. Both
runs meet both limits, window 8 by 0.0011. A3's gate as the design words it also has an adversarial daemon; these runs
have none (the one run with it on a sequencer is G2 of `perf-gate-sequencer.md`, 30 updates). In both runs no step
was applied twice, and all 96 revealed finality updates were covered, at window 8 64 by their own job (latency median
105 s, p90 175 s), at window 30 23 (median 275 s, p90 546 s). A3's ratio leaves out the applied steps of abandoned
jobs (opened, then overtaken by another job): 127,031,995 execution gas at window 8 and 72,748,924 at window 30, so
that all the daemons' gas is **1.289** and **1.249** times the completed jobs' ("All daemons together" below).

**Every refused step raced a step in its own block.** The table of refused steps pairs a refused step with the same
step of the same job applied, so `gate-g6`'s three refused `Finish` steps with exit code 82 (`StoreChanged`) show as
"0 of 3": their jobs never finished. Each was overtaken in that same block by the other daemon's applied `Finish` of
another job (blocks 243, 267 and 289), which changed the store the job had started on. So in both runs every refused
step landed in the block of the applied step that made it moot; no daemon sent a step after that step was in an
earlier block.

Below: `python3 -I bench/perf/summarize_gate.py bench/perf/results/gate-g6 bench/perf/results/gate-g7`, as printed.

## Run `gate-g6`

```
date: 2026-11-21T20:25:47Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 5f8937d7af34af21fbcf29fd5e6bda892843fb79508fc72133c14956a0dcd373
ferrata-updater sha256 61b2c35099d378d9e2075e8bb51cd45f8581dcd3945dbf7af827f561087aaebf
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=8 GATE_POLL=4 GATE_ADVERSARY=0
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 150 | 36 | 0 | 456,688,762 | 38,291,351 | 466,629 | 120,589 | 3,657,243,128 | 307,295,520 |
| honest-2 | 134 | 35 | 0 | 405,821,378 | 47,008,219 | 543,969 | 113,968 | 3,250,922,776 | 376,977,496 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 85,299,570 / 862,510,140 = **0.0989** (fees: 0.0991; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.522** (honest-2 0.478; the gate: 0.35 to 0.65).

Jobs with an applied step: 97; steps applied: 284; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 33 | 72,419,532 | 33 of 33 |
| honest daemons | finish | 82 | 3 | 11,055,840 | 0 of 3 |
| honest daemons | adopt0 | 1 | 3 | 1,063,326 | 3 of 3 |
| honest daemons | adopt1 | 84 | 1 | 354,714 | 1 of 1 |
| honest daemons | miller | 81 | 14 | 302,271 | 14 of 14 |
| honest daemons | finish | 205 | 17 | 103,887 | 17 of 17 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 419,422,058 | 37,266,704 | 38,291,351 | 494,980,113 |
| honest-2 | 316,056,087 | 89,765,291 | 47,008,219 | 452,829,597 |
| all daemons | 735,478,145 | 127,031,995 | 85,299,570 | 947,809,710 |

Jobs: 97 with an applied step, 74 completed, **23 abandoned** (14 with Open + Miller applied; 6 with Open applied; 3 with Open + Miller + adopt 1/2 + adopt 2/2 applied).

All daemons together: the updates cost **1.289** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.099.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 212,331,565 / 735,478,145 = **0.2887** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (64 by their own job, 32 by a later update's): median **105 s**, p90 175 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 54 s, max 294 s; finished by honest-1 49, honest-2 47.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (105 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1008, 6.4 | 6419 | 337.6 | 0.0526 | 57.8 | 37.4 | 799 / 13,288,170 | 5,377,706 / 46,521,562 |
| honest-2 | 4 | 1073, 6.0 | 6419 | 340.6 | 0.0531 | 57.6 | 38.1 | 899 / 14,410,659 | 5,166,705 / 47,208,376 |

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## Run `gate-g7`

```
date: 2026-11-21T20:26:12Z
nproc --all: 32
model name	: unknown
MemTotal:       67108864 kB
bundle sha256 db185c3df2e1efd9e23a2f1070cc100898e36bd86c3ebefb4caceb2c87f90e64
ferrata-updater sha256 eb0488c04c6870f44c64a8b2f3bbf7205b44dc8eb07fe1e8f9be030f05083d47
GATE_ENTRIES=106 GATE_STEP=60 GATE_WINDOW=30 GATE_POLL=4 GATE_ADVERSARY=0
FAKETIME=-5d LD_PRELOAD=/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1 (libfaketime: every process's wall clock; monotonic clocks real: FAKETIME_DONT_FAKE_MONOTONIC=1)
done=1
```

| daemon | steps applied | refused | rejected at the RPC | exec gas applied | exec gas refused | storage gas applied | storage gas refused | fees applied | fees refused |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 73 | 3 | 0 | 221,891,657 | 4,343,762 | 372,236 | 6,881 | 1,778,111,144 | 34,805,144 |
| honest-2 | 66 | 7 | 0 | 198,535,518 | 9,390,406 | 319,828 | 63,975 | 1,590,842,768 | 75,635,048 |

**R.9:** the honest daemons' execution gas on refused steps / on applied steps = 13,734,168 / 420,427,175 = **0.0327** (fees: 0.0328; the gate: at most 0.10).
**R.10:** honest-1's share of the honest daemons' fees = **0.521** (honest-2 0.479; the gate: 0.35 to 0.65).

Jobs with an applied step: 47; steps applied: 139; applied more than once: 0.

| refused steps of | step | exit code | count | exec gas (total) | in the same block as the applied step |
|---|---|---:|---:|---:|---:|
| honest daemons | open | 207 | 6 | 13,037,417 | 6 of 6 |
| honest daemons | adopt0 | 1 | 1 | 350,615 | 1 of 1 |
| honest daemons | adopt1 | 205 | 1 | 333,914 | 1 of 1 |
| honest daemons | finish | 205 | 2 | 12,222 | 2 of 2 |

| execution gas of | in completed jobs | in abandoned jobs (their applied steps) | on refused steps | total |
|---|---:|---:|---:|---:|
| honest-1 | 195,388,365 | 26,503,292 | 4,343,762 | 226,235,419 |
| honest-2 | 152,289,886 | 46,245,632 | 9,390,406 | 207,925,924 |
| all daemons | 347,678,251 | 72,748,924 | 13,734,168 | 434,161,343 |

Jobs: 47 with an applied step, 32 completed, **15 abandoned** (8 with Open + Miller applied; 5 with Open applied; 2 with Open + Miller + adopt 1/2 applied).

All daemons together: the updates cost **1.249** times the execution gas of the completed jobs (total / completed; one submitter that never raced would pay the completed jobs alone); counting only refused steps as waste, (applied + refused) / applied = 1.033.
**Honest waste:** the honest daemons' gas on refused steps and abandoned jobs / on completed jobs = 86,483,092 / 347,678,251 = **0.2487** (A3's ratio above counts refused steps only).

**Latency** (a recorded finality update appearing at the beacon API to the first `Finish`, applied in a block final on Bedrock, that leaves the instance at least as fresh: the update's own job or a later update's), 96 of 96 revealed finality updates covered (23 by their own job, 73 by a later update's): median **275 s**, p90 546 s (the value at index round(0.9 (n − 1)) of the sorted latencies), min 85 s, max 756 s; finished by honest-1 56, honest-2 40.

**Replay rate:** 106 recorded updates revealed one every 60 s (median); the lab recorded them 121.6 min apart (median of the finalized headers' spacing; 19.2 to 768.0 min; its session schedule, not mainnet's update rate), so the replay runs about 122 times faster than the lab recorded them and 60 times the design's hourly cadence; the reveal step is shorter than this run's median latency (275 s), so jobs overlap by construction.

| daemon | poll s (configured) | rounds run, s per round | wall s | CPU s (user + sys) | CPU share of one core | peak RSS MiB | median RSS MiB | beacon API: requests / bytes served | sequencer RPC: bytes up / down |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| honest-1 | 4 | 1402, 4.8 | 6720 | 237.9 | 0.0354 | 57.9 | 36.7 | 875 / 15,349,851 | 4,073,900 / 36,800,138 |
| honest-2 | 4 | 1411, 4.8 | 6720 | 238.6 | 0.0355 | 57.9 | 36.2 | 869 / 15,370,157 | 4,047,659 / 37,441,462 |

Beacon bytes are the replay's own count (each response's body plus 200 bytes), per port; sequencer RPC bytes are counted by a proxy in front of each daemon.

## The runs side by side

| run | honest daemons' deferral window: rounds of a nominal s (measured s per round, all daemons) | adversary | reveal step s | jobs: completed / abandoned | steps applied twice | finality updates covered: own job / a later one | latency median / p90 (s) | execution gas, all daemons, per recorded update revealed | gas multiple: total / completed jobs ((applied + refused) / applied) | honest waste: refused and abandoned / completed | A3's ratio: honest refused / applied (gate at most 0.10) | honest-1's share of the honest fees (gate 0.35–0.65) |
|---|---|---|---:|---|---:|---|---|---:|---|---:|---:|---:|
| gate-g6 | 8 of 4 (6.0–6.4) | no | 60 | 74 / 23 | 0 | 64 / 32 | 105 / 175 | 8,941,601 | **1.289** (1.099) | 0.2887 | 0.0989 | 0.522 |
| gate-g7 | 30 of 4 (4.8–4.8) | no | 60 | 32 / 15 | 0 | 23 / 73 | 275 / 546 | 4,095,862 | **1.249** (1.033) | 0.2487 | 0.0327 | 0.521 |
