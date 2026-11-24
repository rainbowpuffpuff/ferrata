# User proving on laptop-sized containers (RFP-021 P.4, RFP-022 P.4)

The three proofs a user's own device makes, with this tree's final programs, each padded as LEZ's wallet pads every
private transaction (seven private actions, 512-byte notes, the fee's recovery note: what a client sends): the private
attestation (RFP-021), the private claim and the private burn (RFP-022). Each is LEZ v0.4.0-rc1's
`execute_and_prove` on CPU (r0vm 3.0.5, `RISC0_DEV_MODE=0`); its transaction is then settled with the receipt
verified, and the same transaction with one bit flipped is refused (exit 101). Measured on 21 November 2026.

**Not a laptop.** The containers are Modal's, held to the shape of a mid-range laptop: 8 CPUs of time and 16 GiB of
memory as a hard limit (`cpu.cfs_quota_us` 800,000 per 100,000; `memory.limit_in_bytes` 17,179,869,184), and the
same with 16 CPUs. Modal's 8 CPUs are 8 hardware threads of server cores (mostly AMD Zen 3); a laptop sold with
8 cores usually has 8 physical cores and 16 threads. A run on a real laptop is the owner's step (below).

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

The owner's step on the owner's laptop, run by an agent with the owner's approval for that night: the bundle of this
tree staged on the build box, then `P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh` behind a gate and under a watchdog.
**The release's succinct proof ran and verified. The private attestation, claim and burn did not finish:** each of
four attempts was stopped within 2–4.25 minutes by the owner's rule for that night (stop if the load stays above 10),
because other local work ran beside the provers' 8 threads. Records: [`p4-laptop-owner/`](p4-laptop-owner/README.md).

**The laptop.** AMD Ryzen 9 7940HS (Zen 4, 8 cores / 16 threads, 16 MiB L3; `lscpu`: 4.0 GHz at most), 54.7 GiB of
memory, Fedora with kernel 7.3.4, on mains power (charging), platform profile `low-power`, energy preference `power`,
governor `powersave` (all as found, not changed). The provers were held to CPUs 0–7 (`P4_CPUS=8`), which on this CPU
are cores 0–3 with both threads of each: half the machine, the CPU of a 4-core, 8-thread laptop. Their niceness was 19:
the shell that started them already ran at 19, which the script's `nice -n 10` cannot lower.

**The bundle** ([`bench/perf/p4-laptop-build.sh`](../../bench/perf/p4-laptop-build.sh) on the build box at commit
`3ae7bab2`, whose sources are those of `861c1c0d`; 224,022,502 B): `prove_attest` (attestation program `0x816d1fef…`,
reference consumer `0x8464603b…`, the mainnet fixture of 10 November), `prove_bridge` (the bridge program since
milestone B1, `0x847a4336…`; the containers above proved the deployed `0xccde6171…`), r0vm 3.0.5 (sha256 `36c016a5…`,
the rehearsal's), and `release/`: `ferrata-e2e` (RISC Zero 3.1's client, without the wrap), `release_31`
(`0xf279af41…`; its sha256 and id checked against [`docs/image-ids.md`](../../docs/image-ids.md)), the crown rehearsal
3's burn `private-1` as the witness, and r0vm 3.1.0 (`990c7e55…`). The build box ran both LEZ binaries' dev-mode dry
runs and one real release proof (13.1 s on 32 threads, verified); on the laptop the script checked every staged file
against its manifest before running.

### The release's succinct proof (the holder's proof) on the laptop

`P4_JOBS=release P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`, 25 November 2026 at 00:00 CET, after two
quiet minutes:

| proof | `ferrata-e2e prove` (s) | user cycles (segments) | peak GiB: GNU time RSS / r0vm sampled | receipt (B) | verified |
|---|---:|---:|---:|---:|---|
| release, succinct (`release_31`, RISC Zero 3.1) | **129.0** | 77,736 (1 of 2^17) | **1.39** / 1.37 | 223,930 | yes: against the image id, the journal equal to the native one (`ferrata-e2e verify`) |

Beside it, other processes kept 1.1–5.9 busy threads (median 2.6) on CPUs 8–15, and the load average reached 12.0. The
prover used 983.2 CPU-seconds (763 % of one CPU). The same proof took 344.9 and 345.8 CPU-seconds in the second
pass's 8-CPU containers (46.0 s; [`p4-minimum`](p4-minimum/README.md)) and 330.8 on the 32-thread build box (13.1 s):
each of the laptop's threads did about a third of a server thread's work that night (*derived*). Likely causes, not
separated: the `low-power` profile, both threads of each core busy, and the other work. The earlier release image took
37.1–48.6 s on all 16 threads of this laptop ([`bridge.md`](../../docs/design/bridge.md) §18.2).

### The whole run: four attempts, each stopped by the load rule

| attempt | started (CET) | stopped after | busy threads, median: all / CPUs 8–15 | load average, max | memory available, min |
|---|---|---:|---|---:|---:|
| 1 | 24 Nov 23:21:28 | 131 s | 13.1 / 5.1 | 15.1 | 20.8 GiB |
| 2 | 23:26:13 | 131 s | 14.0 / 6.0 | 19.1 | 20.1 GiB |
| 3 | 23:35:45 | 251 s | 11.3 / 3.3 | 26.1 | 16.9 GiB |
| 4 | 23:50:33 | 255 s | 12.7 / 4.8 | 16.2 | 17.4 GiB |

Each attempt started after the gate saw a quiet machine (for 1, 2, 3 and 10 minutes, 2–3 busy threads on average) and
was stopped because the busy threads stayed above 10 for 120 s. The provers stayed on their CPUs (a 60 s probe: all 25
r0vm threads on CPUs 0–7, allowed only those, [`pinning-probe.txt`](p4-laptop-owner/pinning-probe.txt)), so the excess
was other work: other sessions' test runs and scripts (one `cargo test` of 8 threads, Python checks) and the desktop,
2–8 threads at a time. r0vm had reached 4.25 GiB when attempts 3 and 4 were stopped; no proof finished. With the
provers at 8 threads, the rule holds only while everything else stays under about 2 threads for the whole run; and at
the release's rate the three proofs would take about 2.5 hours in the `low-power` profile, not the rehearsal's 50
minutes (*estimated*: 3,280.6 s at the containers' 8-CPU rate, times 2.8).

### A fifth attempt (25 November, 00:24 CET): the attestation alone, under a looser rule

With other sessions' local work asked to stay at niceness 19, the attestation alone (`P4_JOBS=attest-pad`) ran as a
transient user service, so that the script started at niceness 0 and its provers ran at 10 as it intends (`machine.txt`:
"prover niceness: 10"; [`run-part.sh`](p4-laptop-owner/run-part.sh)). The rule for this attempt: stop only if the
one-minute load average stays above 14 for five minutes, or less than 8 GB is available. It stopped after 9 minutes
(load average 15.8 on average, 22.6 at most; at least 16.8 GiB available throughout; r0vm at 4.54 GiB), and no proof
finished ([`attempt-5-stopped/`](p4-laptop-owner/attempt-5-stopped/watch.log)).

**The load average counts the prover's own waiting threads.** r0vm ran 25 threads: one main thread, rayon's 8
(`RAYON_NUM_THREADS=8`), and 16 for the C++ kernels' pool, all held to CPUs 0–7. risc0's vendored poolstl sizes that
pool from `std::thread::hardware_concurrency()` (16 on this laptop; `risc0-sys` 1.5, `cxx/vendor/poolstl.hpp`, with no
environment override). While the C++ kernels run, about 16 threads are runnable on 8 CPUs, and the load average counts
each one. Three minutes in, other work used 1.35 busy threads and the load average was 14.46. On this laptop the prover
alone holds the load average at about 14–17 (*estimated* from these samples). A load rule must therefore sit above that:
about 20 for five minutes leaves room for 3 other threads. Alternatively, it can count busy CPUs rather than runnable
threads, as the first four attempts' watchdog did.

### What the laptop run still needs

On this laptop, with the bundle above (kept beside the repository) or a new one from `p4-laptop-build.sh`:

1. Other local work stopped for the length of the run (other sessions' builds and tests included).
2. Mains power and, the owner's choice, a faster platform profile (`powerprofilesctl set balanced`, or `performance`);
   `machine.txt` records it either way.
3. A stop rule that allows for the prover's own threads (previous section): the one-minute load average above about 20
   for five minutes, or other work above about 6 busy threads on CPUs 8–15 for five minutes; and the provers at
   niceness 10 (from an agent's shell at 19, start the script as a user service, as `run-part.sh` does).
4. `P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`, then `python3 -I bench/perf/summarize_p4.py --owner OUT`.
   Where the machine can be kept quiet only for shorter stretches, one job at a time, each into its own `OUT`:
   `P4_JOBS=attest-pad` (the attestation, 21 minutes at the containers' 8-CPU rate), `P4_JOBS=bridge-pad` (the claim
   and the burn, 34 minutes at that rate).
