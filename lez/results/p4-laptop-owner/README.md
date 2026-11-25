# The owner's laptop step of P.4 (RFP-021 P.4, RFP-022 P.4): records of 24–25 November 2026

The four user-device proofs on the owner's laptop: the private attestation (4,596.7 s), the private claim (3,313.4 s),
the private burn (2,802.0 s), each settled with its receipt verified, and the release's succinct proof (129.0 s,
verified). The stopped attempts are here too. What ran, on what, and how to read it:
[`../perf-p4-laptop.md`](../perf-p4-laptop.md), section "The owner's laptop, 24–25 November 2026".

Machine paths in these files are replaced by placeholders, and nothing else was changed:

- `<REPO>`: this repository;
- `<SCRATCH>`: the scratch directory beside it;
- `<STAGE_DIR>`: the staged bundle;
- `<OUT_DIR>`: the run's output directory.

The laptop: AMD Ryzen 9 7940HS (8 cores, 16 threads; CPUs 2k and 2k+1 are core k's two threads), 54.7 GiB, Fedora
(kernel 7.3.4), on mains power, platform profile `low-power`. The provers were held to CPUs 0–7 (`P4_CPUS=8`: cores
0–3, both threads of each). Their niceness was 10 in attempts 5 and 6 and in the finished attestation, claim and burn,
because the script was started as a user service. In attempts 1–4 and the release run it was 19: the shell that
started them already ran at 19, and the script's `nice -n 10` could not lower it. The first attempt's `machine.txt`
says 10, the line the script wrote then; the script now records the niceness in effect.

The machine was not quiet. At the start of every run here the laptop's 8 GiB swap was full (8.00 of 8.00 GiB in all
ten `machine.txt`), and other work held 31.8–33.3 GiB of the memory. During the runs, other sessions' work kept busy
threads on CPUs 8–15 (`watch.log`). The provers themselves did not page: 75 major page faults over the attestation,
none in the bridge run or the release (GNU time, in `prove.log`). At 8 of 16 threads the runs were below the declared
minimum (8 cores with 16 threads), so they are laptop measurements, not measurements at the minimum.

| path | what |
|---|---|
| `attest-pad-run/` | The private attestation, 25 November 00:59–02:16 CET (`P4_JOBS=attest-pad`). `out/machine.txt`; `out/attest-pad/` holds `prove.log` (with GNU time), `samples.txt` (the machine's used memory and r0vm's RSS every second), `settle.log`, `exit.txt` and `tx-attest.borsh`. Also `summary.md` (`summarize_p4.py --owner`), `owner.log`, `owner.exit`, `watch.log` and `verdict.txt` |
| `bridge-pad-run/` | The private claim and burn, 25 November 04:07–05:49 CET (`P4_JOBS=bridge-pad`), with the same files (`tx-claim.borsh`, `tx-burn.borsh`) |
| `release-run/` | The release's succinct proof alone (`P4_JOBS=release`), 25 November 00:00 CET. `out/release/` holds `exec.log`, `prove.log` (GNU time and `ferrata-e2e`'s JSON record), `prove/prove.json`, `prove/receipt-succinct.bin`, `settle.log` (`ferrata-e2e verify`), `samples.txt` and `exit.txt`; plus `summary.md`, `gate.log`, `watch.log`, `verdict.txt` and `owner.log` |
| `bridge-pad-run-1-stopped/` | The first bridge run, 02:16–03:32 CET: its claim was proven (3,242.9 s, in `prove.log`), then it was stopped during the burn. `out/machine.txt` and `out/bridge-pad/` (`prove.log`, `samples.txt`); its watchdog's files were overwritten by the next run's |
| `attempt-1-stopped/` … `attempt-4-stopped/` | 24 November, the whole run (the attestation first), each stopped within 2–4.25 minutes: busy threads above 10 for 120 s. Files: `machine.txt`, `gate.log`, `watch.log`, `verdict.txt`, `owner.log`, `attest-pad-samples.txt` |
| `attempt-5-stopped/` | 25 November 00:24, the attestation alone at niceness 10, stopped after 9 minutes: the load average above 14 for 5 minutes |
| `attempt-6-stopped/` | 00:38, the attestation alone, stopped after 19 minutes: other work above 6 busy threads on CPUs 8–15 for 5 minutes |
| `pinning-probe.txt` | 60 s of the attestation's prover, sampled: all 25 r0vm threads on CPUs 0–7, allowed only those |
| `run-owner.sh` | Attempts 1–4 and the release run: the gate before the script and the watchdog during it |
| `run-part.sh`, `part-service.sh` | Attempts 5 and 6 and the finished runs: the script started as a transient user service (`systemd-run --user`), so it starts at niceness 0 and its provers run at 10, under the watchdog of the last rule. Attempt 5 ran an earlier version of this runner, whose only rule was the load average above 14 |

`watch.log` columns: Unix time, mains, available memory, the one-minute load average, busy threads (all CPUs, and CPUs
8–15, which the provers do not use), and r0vm's thread count (from attempt 5 on, with its niceness).

The stop rules of those nights:

- **Attempts 1–4 and the release run:** stop if busy threads stay above 10 for 120 s, if less than 8 GiB is available,
  or after 60 s on battery. Each run waited for a gate first: on mains, at least 18 GiB available, and at most 4 busy
  threads on average (3 from attempt 3) over 1, 2, 3 or 10 minutes. The release run's gate averaged over 2 minutes
  (`GATE_N=4`), although its `gate.log` column is named `busy_avg_600s`.
- **Attempt 5:** stop if the one-minute load average stays above 14 for 5 minutes, or if less than 8 GB is available.
- **Attempt 6 and the finished runs:** stop if other work stays above 6 busy threads on CPUs 8–15 for 5 minutes, if the
  load average stays above 24 for 5 minutes, or if less than 8 GB is available.

r0vm runs 1 + 8 (rayon) + 16 threads (the C++ kernels' pool, sized from the machine's 16 hardware threads) on its 8 CPUs.
The prover alone therefore holds the one-minute load average at about 14–17, which is why the last rule counts other
work's busy CPUs instead.
