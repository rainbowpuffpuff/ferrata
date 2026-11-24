# The owner's laptop step of P.4 (RFP-021 P.4, RFP-022 P.4): records of 24–25 November 2026

What ran, on what, and why the three long proofs did not finish: [`../perf-p4-laptop.md`](../perf-p4-laptop.md),
section "The owner's laptop, 24–25 November 2026". Machine paths in these files are replaced by `<REPO>` (this
repository), `<SCRATCH>` (the scratch directory beside it), `<STAGE_DIR>` (the staged bundle) and `<OUT_DIR>` (the
run's output directory); nothing else was changed.

The laptop: AMD Ryzen 9 7940HS (8 cores, 16 threads; CPUs 2k and 2k+1 are core k's two threads), 54.7 GiB, Fedora
(kernel 7.3.4), on mains power, platform profile `low-power`. The provers were held to CPUs 0–7 (`P4_CPUS=8`: cores
0–3, both threads of each). In attempts 1–4 and the release run they ran at niceness 19: the shell that started them
already ran at 19, so the script's `nice -n 10` could not lower it (the first attempt's `machine.txt` says 10, the line
the script then wrote; the script now records the niceness in effect). In attempt 5 they ran at 10 (the script started
as a user service).

| path | what |
|---|---|
| `release-run/` | The release's succinct proof alone (`P4_JOBS=release`), 25 November 2026 00:00 CET: `out/machine.txt`, `out/release/` (`exec.log`, `prove.log` with GNU time and `ferrata-e2e`'s JSON record, `prove/prove.json`, `prove/receipt-succinct.bin`, `settle.log`: `ferrata-e2e verify`, `samples.txt`: the machine's used memory and r0vm's RSS every second, `exit.txt`), `summary.md` (`summarize_p4.py --owner`), `gate.log`, `watch.log`, `verdict.txt`, `owner.log` |
| `attempt-N-stopped/` | The four attempts at the whole run (the private attestation first), each stopped by the watchdog within 2–4.25 minutes: `machine.txt`, `gate.log`, `watch.log`, `verdict.txt`, `owner.log`, `attest-pad-samples.txt` (no proof finished) |
| `attempt-5-stopped/` | 25 November 00:24 CET, the attestation alone (`P4_JOBS=attest-pad`) at niceness 10, under the looser rule (stop only if the one-minute load average stays above 14 for five minutes, or under 8 GB available): stopped after 9 minutes, no proof finished |
| `run-part.sh`, `part-service.sh` | The fifth attempt's runner: the script started as a transient user service (`systemd-run --user`), so it starts at niceness 0 and its provers run at 10, with that rule's watchdog |
| `pinning-probe.txt` | 60 s of the attestation's prover, sampled: all 25 r0vm threads on CPUs 0–7, their allowed list 0–7 |
| `run-owner.sh` | The runner that wrapped `bench/perf/p4-laptop-owner.sh`: the gate before it and the watchdog during it (below) |

The gate (every 30 s): on mains power, at least 18 GiB available, and the machine's busy hardware threads
(`/proc/stat`) averaging at most 4 (attempts 1 and 2) or 3 (from attempt 3) over the last 1, 2, 3 and 10 minutes
(attempts 1–4) and 2 minutes (the release run, `GATE_N=4`; its `gate.log` column keeps the name `busy_avg_600s`).
The watchdog (every 5 s): stop the whole run if less than 8 GiB is available, if the busy hardware threads stay above
10 for 120 s, or if the laptop is on battery for 60 s. The provers alone are 8 busy threads, so the run stops when
everything else on the machine stays above about 2 threads for two minutes. `watch.log` columns: Unix time, mains,
available memory, the one-minute load average, busy threads (all CPUs, and CPUs 8–15, which the provers do not use),
r0vm's thread count (the fifth attempt's: thread count and niceness).

The fifth attempt's lesson: r0vm runs 1 + 8 (rayon) + 16 (the C++ kernels' pool, sized from the machine's 16 hardware
threads) threads on its 8 CPUs, so the prover alone holds the one-minute load average at about 14–17. A load rule has
to sit above that.
