# User proving on laptop-sized containers (RFP-021 P.4, RFP-022 P.4)

The three proofs a user's own device makes, with this tree's final programs, each padded as LEZ's wallet pads every
private transaction (seven private actions, 512-byte notes, the fee's recovery note: what a client sends): the private
attestation (RFP-021), the private claim and the private burn (RFP-022). Each is LEZ v0.4.0-rc1's
`execute_and_prove` on CPU (r0vm 3.0.5, `RISC0_DEV_MODE=0`); its transaction is then settled with the receipt
verified, and the same transaction with one bit flipped is refused (exit 101). Measured on 21 November 2026.

**Not a laptop.** The containers are Modal's, held to the shape of a mid-range laptop: 8 CPUs of time and 16 GiB of
memory as a hard limit (`cpu.cfs_quota_us` 800,000 per 100,000; `memory.limit_in_bytes` 17,179,869,184), and the
same with 16 CPUs. Modal's 8 CPUs are 8 hardware threads of server cores (mostly AMD Zen 3); a laptop sold with
8 cores usually has 8 physical cores and 16 threads. The same proofs on a real laptop: [below](#the-owners-laptop-2425-november-2026).

## Results

| proof (padded as LEZ's wallet pads it) | container | `execute_and_prove` (s), each run | median (s) | user cycles proven (segments) | peak GiB: r0vm RSS / container | receipt (B) | transaction (B) | settled, receipt verified |
|---|---|---|---:|---:|---:|---:|---:|---|
| private attestation | 8 vCPU, 16 GiB | 1250.0 / 1259.6 | 1254.8 | 3,335,954–3,336,897 (6) | 9.19 / 9.23 | 239,246 | 255,366 | 2 of 2 |
| private attestation | 16 vCPU, 16 GiB | 633.5 | 633.5 | 3,336,963 (6) | 9.19 / 9.24 | 239,246 | 255,366 | 1 of 1 |
| private claim | 8 vCPU, 16 GiB | 941.1 / 1245.0 | 1093.0 | 3,123,202–3,123,367 (6) | 9.19 / 9.23 | 239,628 | 256,130 | 2 of 2 |
| private claim | 16 vCPU, 16 GiB | 639.2 | 639.2 | 3,123,878 (6) | 9.19 / 9.23 | 239,628 | 256,130 | 1 of 1 |
| private burn | 8 vCPU, 16 GiB | 820.9 / 1044.7 | 932.8 | 2,902,554–2,903,330 (5) | 9.19 / 9.23 | 239,046 | 254,966 | 2 of 2 |
| private burn | 16 vCPU, 16 GiB | 534.5 | 534.5 | 2,903,989 (5) | 9.19 / 9.23 | 239,046 | 254,966 | 1 of 1 |

| container (tag / job) | CPUs | machine | runs: execute_and_prove (s) | tamper: settle exit |
|---|---:|---|---|---|
| laptop-cpu16-a / attest-pad | 16 | AMD Zen 3, AZURE us-west | private attestation 633.5 | [101] |
| laptop-cpu16-a / bridge-pad | 16 | AMD Zen 3, AZURE us-west | private claim 639.2; private burn 534.5 | [101, 101] |
| laptop-cpu8-a / attest-pad | 8 | AMD Zen 3, AZURE us-west | private attestation 1250.0 | [101] |
| laptop-cpu8-a / bridge-pad | 8 | GenuineIntel family 6 model 173, AWS uk | private claim 941.1; private burn 820.9 | [101, 101] |
| laptop-cpu8-b / attest-pad | 8 | AMD Zen 3, AZURE eu-west | private attestation 1259.6 | [101] |
| laptop-cpu8-b / bridge-pad | 8 | AMD Zen 3, AZURE us-west | private claim 1245.0; private burn 1044.7 | [101, 101] |

Read with the tables:

- Memory does not depend on the CPU count: r0vm's peak is 9.19 GiB in every run, the container's 9.23–9.24 GiB. A
  16 GB machine has room; an 8 GB one does not.
- Time scales with the threads: 16 CPUs take about half of 8 (attestation 633.5 against 1254.8 s). The spread at 8
  CPUs (claim 941.1 and 1245.0 s) is the host: the faster run's container was moved to an Intel host after a
  preemption (Modal picks the machine).
- The same proofs at commit `2854e4e` (EVIDENCE rows 32 and 38: padded attestation 1,236.8 and 633.3 s at 8 and 16
  CPUs) agree within 1.5 %: the final programs did not make user proving heavier.

## What was proven, with what

- Sources: branch `rfp-measure` at `2993debd` (this tree's programs; image ids as [`docs/image-ids.md`](../../docs/image-ids.md):
  attestation program `0x816d1fef…`, bridge `0xccde6171…`, reference consumer `0x8464603b…`, the image the attestation
  discloses). LEZ v0.4.0-rc1 (`1806e87`).
- Binaries: `prove_attest` and `prove_bridge` (`cargo build -p harness --locked`, debug, stripped), the attestation's
  mainnet fixture (`mainnet-attest-2026-11-10`), staged with their sha256 in a `MANIFEST.json` by
  [`bench/perf/p4-laptop-build.sh`](../../bench/perf/p4-laptop-build.sh) on the build box; every container checked
  the hashes before running (`input_hashes_equal_manifest: true`).
- Runner: [`lez/results/p4-minimum/minimum.py`](p4-minimum/minimum.py), jobs `attest-pad` and `bridge-pad`
  (`P4_CPU=8` twice, `P4_CPU=16` once), memory sampled every 0.5 s beside GNU time.
- Records: [`bench/perf/results/p4-laptop/`](../../bench/perf/results/p4-laptop/) (`container.json`, and per run
  `prove.log`, `run.json`, `samples.csv`, `settle.log`, the transactions and the tamper logs).
- Table: `python3 -I bench/perf/summarize_p4.py bench/perf/results/p4-laptop`.

## What a laptop run needs from the owner

An x86-64 Linux laptop with 16 GB or more, on mains power, nothing else busy, about 50–55 minutes at 8 threads
(the rehearsal below: 50 minutes):

1. The staged folder that `bench/perf/p4-laptop-build.sh` writes on the build box (`attest-pad/`, `bridge-pad/`,
   `r0vm/r0vm` and its `SHA256SUMS`; 136 MB, 136,438,291 B), brought to the laptop with `mrun --get`. With
   `P4_RELEASE_BIN` set to `release_31`'s binary it also holds `release/`, the release's succinct proof (RFP-022's
   third user-device proof), which the script then runs last (224,022,502 B in all at `3ae7bab2`).
2. `bash bench/perf/p4-laptop-owner.sh STAGE_DIR OUT_DIR`: the same three proofs and settlements, GNU time's peak
   and the machine's used memory every second, `lscpu` and the power state in `OUT_DIR/machine.txt`. The provers take
   every hardware thread at nice 10; `P4_CPUS=8` holds them to the first 8 (CPU affinity with `taskset`, and
   `RAYON_NUM_THREADS`: the circuits' C++ kernels start one thread per hardware thread whatever rayon is told), which
   `machine.txt` and the summary record. `P4_JOBS="attest-pad bridge-pad release"` (any of them) runs only those.
3. `python3 -I bench/perf/summarize_p4.py --owner OUT_DIR` prints the rows for the table above.

A Mac follows [`p4-macos`](p4-macos/README.md) (M1 16 GB: 857.4 s padded attestation, EVIDENCE row 88).

## Rehearsal of the owner's step (21 November 2026)

`bench/perf/p4-laptop-owner.sh` (fixed: it no longer stops after the first proof) run exactly as the owner would, from
the staged bundle (`bench/perf/p4-laptop-build.sh`'s `stage/`: attest-pad, bridge-pad, r0vm/r0vm with its SHA256SUMS),
by an unprivileged user in a Modal container held to 8 CPUs and 16 GiB (`bench/perf/p4_owner_modal.py`; Intel family 6
model 173, AWS): `bash bench/perf/p4-laptop-owner.sh STAGE OUT`, then `python3 -I bench/perf/summarize_p4.py --owner
OUT`. Both exited 0; records `bench/perf/results/p4-owner-rehearsal/` (`owner.log`, `summary.md`, `out/`).

| proof | `execute_and_prove` s | user cycles (segments) | peak GiB: r0vm / machine | receipt B | transaction B | settled |
|---|---:|---:|---:|---:|---:|---|
| private attestation | 1,110.8 | 3,336,834 (6) | 9.19 / 9.24 | 239,246 | 255,366 | yes |
| private claim | 1,007.5 | 3,122,740 (6) | 9.19 / 9.23 | 239,628 | 256,130 | yes |
| private burn | 868.1 | 2,903,241 (5) | 9.19 / 9.23 | 239,046 | 254,966 | yes |

The whole step took 50 minutes at 8 CPUs (the prover at nice 10). The 8-CPU "medians" of the claim and the burn above
are the means of one Intel and one AMD run. The owner's real laptop remains to be run for these three (next section).

## The owner's laptop, 24–25 November 2026

All four user-device proofs ran on the owner's laptop with this tree's programs, and each settled or verified. RFP-021's
private attestation took **4,596.7 s**. RFP-022's private claim took **3,313.4 s** and its private burn **2,802.0 s**
(each padded as LEZ's wallet pads it), and the release's succinct proof (the holder's) took **129.0 s**. Peak memory
was **9.17 GiB** for the three LEZ proofs and **1.39 GiB** for the release. The laptop ran at half its CPU and in its
`low-power` profile, so these figures are slow for this machine: 2.8–3.7 times the 8-CPU containers' times above. Six
attempts and the first bridge run were stopped by those nights' stop rules (below). Records:
[`p4-laptop-owner/`](p4-laptop-owner/README.md).

**The laptop.** AMD Ryzen 9 7940HS (Zen 4, 8 cores / 16 threads, 16 MiB L3; `lscpu`: 4.0 GHz at most), 54.7 GiB of
memory, Fedora with kernel 7.3.4, on mains power. Platform profile `low-power`, energy preference `power`, governor
`powersave`: all as found, not changed. The provers were held to CPUs 0–7 (`P4_CPUS=8`: `taskset -c 0-7`,
`RAYON_NUM_THREADS=8`). On this CPU those are cores 0–3 with both threads of each: half the machine, the CPU of a
4-core, 8-thread laptop. The three LEZ proofs ran at niceness 10, the script started as a transient user service
([`run-part.sh`](p4-laptop-owner/run-part.sh)); the release ran at 19, the niceness of the agent's shell. Other
sessions' local work ran beside them on CPUs 8–15 (below).

**The bundle** ([`bench/perf/p4-laptop-build.sh`](../../bench/perf/p4-laptop-build.sh), 224,022,502 B) was built on
the build box at commit `3ae7bab2`. That is this branch before its rebase onto `183c9ea2`; the guests, `prove_attest`,
`prove_bridge` and `ferrata-e2e` are the same at both, and their sources are those of `861c1c0d`. It holds:

- `prove_attest`: attestation program `0x816d1fef…`, reference consumer `0x8464603b…`, the mainnet fixture of 10
  November;
- `prove_bridge`: the bridge program since milestone B1, `0x847a4336…` (the containers above proved the deployed
  `0xccde6171…`);
- r0vm 3.0.5 (sha256 `36c016a5…`, the rehearsal's);
- `release/`: `ferrata-e2e` (RISC Zero 3.1's client, without the wrap); `release_31` (`0xf279af41…`, its sha256 and
  id checked against [`docs/image-ids.md`](../../docs/image-ids.md)); the crown rehearsal 3's burn `private-1` as the
  witness; and r0vm 3.1.0 (`990c7e55…`).

The build box ran both LEZ binaries' dev-mode dry runs and one real release proof (13.1 s on 32 threads, verified). On
the laptop the script checked every staged file against its manifest before each run.

### Results

| proof | ran (CET) | `execute_and_prove` (s) | user cycles (segments) | peak GiB: GNU time RSS / r0vm sampled | receipt (B) | transaction (B) | settled, receipt verified | 8-CPU containers above (s); laptop ÷ containers |
|---|---|---:|---:|---:|---:|---:|---|---|
| private attestation | 25 Nov 00:59–02:16 | **4,596.7** | 3,336,413 (6) | **9.17** / 9.17 | 239,246 | 255,366 | yes | 1,254.8; 3.66× |
| private claim | 25 Nov 04:07–05:49, one run of both | **3,313.4** | 3,123,496 (6) | **9.17** / 9.17 (the run's) | 239,628 | 256,130 | yes | 1,093.0; 3.03× |
| private burn | (the same run) | **2,802.0** | 2,900,415 (5) | (the same) | 239,046 | 254,966 | yes | 932.8; 3.00× |
| release, succinct (`release_31`; `ferrata-e2e prove`) | 25 Nov 00:00–00:02 | **129.0** | 77,736 (1) | **1.39** / 1.37 | 223,930 | none: to a relayer | verified (image id, native journal) | 46.0; 2.80× |

`python3 -I bench/perf/summarize_p4.py --owner` on each run's output gives these rows (`*-run/summary.md`). "Settled"
means `prove_attest --settle` and `prove_bridge --settle` on the transactions, with the receipts verified.

**Conditions** (from the runs' `watch.log`, one sample every 5 s; the provers' CPUs were busy 7.8–7.9 threads on
average):

- The attestation's run (73 minutes): other work on CPUs 8–15 had a median of 5.5 busy threads (up to 8.0), the load
  average was 18.3 on average (30.3 at most), and at least 11.4 GiB was available.
- The claim's and burn's run (99 minutes, from 04:07): other work had a median of 3.3 busy threads (up to 8.0), the load
  average was 13.6 on average (23.0 at most), and at least 12.7 GiB was available.

The stop rule for both: stop only if other work used more than 6 busy threads on CPUs 8–15 for five minutes, if the
one-minute load average stayed above 24 for five minutes, or if less than 8 GB was available.

The first bridge run (02:16–03:32) proved the claim in 3,242.9 s, 2.1 % less than the second run's, and was stopped during
the burn ([`bridge-pad-run-1-stopped/`](p4-laptop-owner/bridge-pad-run-1-stopped/out/machine.txt); that run's watchdog
files were overwritten by the second run's). It is the only repeated proof on the laptop.

Read with the table:

- **Memory does not depend on the machine.** The peak was 9.17 GiB here, against 9.19 GiB in the containers and
  9.26–9.45 GiB on the M1 with 16 GB, with no swap-out on its 16 GB ([`p4-macos`](p4-macos/README.md)). The release
  needs 1.39 GiB.
- **Time.** The laptop took 2.8–3.7 times the containers' 8-CPU times. The release's CPU-seconds (983.2 against
  344.9–345.8 in the containers) put each laptop thread at about a third of a container thread's work (*derived*).
  The likely causes are not separated: the `low-power` profile, both threads of each core busy, and other work on the
  other four cores sharing the power budget. The laptop at a faster profile or with all 16 threads was not measured.
- **The declared minimum** (16 GB, 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB). On the M1 with 16 GB:
  attestation 857.4 s, claim 818.7 s, burn 716.5 s, release 31.7 s (EVIDENCE row 88). In 16-CPU, 16-GiB containers:
  633.5 s, 639.2 s, 534.5 s (above) and 26.7 s (row 33).

### The release's succinct proof on the laptop

`P4_JOBS=release P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`, 25 November 2026 at 00:00 CET, after two
quiet minutes, verified with `ferrata-e2e verify` against the image id, with the journal equal to the native one.
Beside it, other processes kept 1.1–5.9 busy threads (median 2.6) on CPUs 8–15, and the load average reached 12.0. The
prover used 983.2 CPU-seconds (763 % of one CPU). The same proof took 344.9 and 345.8 CPU-seconds in the second pass's
8-CPU containers (46.0 s; [`p4-minimum`](p4-minimum/README.md)) and 330.8 on the 32-thread build box (13.1 s). The
earlier release image took 37.1–48.6 s on all 16 threads of this laptop ([`bridge.md`](../../docs/design/bridge.md)
§18.2).

### The stopped attempts

| attempt | started (CET) | stopped after | stop rule of that night | busy threads, median: all / CPUs 8–15 | load average, max | memory available, min |
|---|---|---:|---|---|---:|---:|
| 1 | 24 Nov 23:21:28 | 131 s | busy threads above 10 for 120 s | 13.1 / 5.1 | 15.1 | 20.8 GiB |
| 2 | 23:26:13 | 131 s | the same | 14.0 / 6.0 | 19.1 | 20.1 GiB |
| 3 | 23:35:45 | 251 s | the same | 11.3 / 3.3 | 26.1 | 16.9 GiB |
| 4 | 23:50:33 | 255 s | the same | 12.7 / 4.8 | 16.2 | 17.4 GiB |
| 5 | 25 Nov 00:24:24 | 538 s | load average above 14 for 5 minutes | 13.0 / 5.0 | 22.6 | 16.8 GiB |
| 6 | 00:38:16 | 1,149 s | other work above 6 busy threads on CPUs 8–15, or the load average above 24, for 5 minutes (the first fired) | 14.6 / 6.6 | 31.5 | 13.0 GiB |
| bridge 1 | 02:16:36 | 4,544 s | the same (which clause fired is not on file) | (overwritten) | (overwritten) | (overwritten) |

Attempts 1–6 were all of the private attestation, and none finished it. In attempts 1–4 the provers ran at niceness 19,
and in 5 and 6 at 10. The provers stayed on their CPUs: a 60 s probe found all 25 r0vm threads on CPUs 0–7, allowed only
those ([`pinning-probe.txt`](p4-laptop-owner/pinning-probe.txt)). The work beside them was other sessions' test runs and
scripts (one `cargo test` of 8 threads, Python checks, `modal` commands), `updatedb`, and the desktop.

**The load average counts the prover's own waiting threads.** r0vm ran 25 threads: one main thread, rayon's 8
(`RAYON_NUM_THREADS=8`), and 16 for the C++ kernels' pool, all held to CPUs 0–7. risc0's vendored poolstl sizes that
pool from `std::thread::hardware_concurrency()`: 16 on this laptop, with no environment override (`risc0-sys` 1.5,
`cxx/vendor/poolstl.hpp`). While the C++ kernels run, about 16 threads are runnable on 8 CPUs, and the load average
counts each one. Three minutes into attempt 5, other work used 1.35 busy threads and the load average was 14.46. On
this laptop the prover alone holds the load average at about 14–17 (*estimated* from these samples). That is why the
last rule counts other work's busy CPUs, and why it puts the load-average limit at 24.

### Running it again

From the worktree, with the staged bundle (kept beside the repository) or a new one from `p4-laptop-build.sh`, on mains
power, in the profile wanted (`machine.txt` records it):

1. `P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`, or one job at a time with
   `P4_JOBS=attest-pad|bridge-pad|release`, each into its own `OUT`.
2. From an agent's shell at niceness 19, start the script as a user service so that its provers run at 10, as
   [`run-part.sh`](p4-laptop-owner/run-part.sh) does.
3. Then run `python3 -I bench/perf/summarize_p4.py --owner OUT`.
