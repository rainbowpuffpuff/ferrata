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
   `r0vm/r0vm` and its `SHA256SUMS`; 136 MB, 136,438,291 B), brought to the laptop with `mrun --get`.
2. `bash bench/perf/p4-laptop-owner.sh STAGE_DIR OUT_DIR`: the same three proofs and settlements, GNU time's peak
   and the machine's used memory every second, `lscpu` and the power state in `OUT_DIR/machine.txt`.
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
are the means of one Intel and one AMD run. The owner's real laptop remains to be run.
