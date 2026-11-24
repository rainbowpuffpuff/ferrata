# Stream p4-laptop: RFP-021 P.4 and RFP-022 P.4 on the owner's laptop (proposals for the integrator)

Branch `p4-laptop` (from `861c1c0d`). **Both rows stay in part.** The owner approved running the user-device proofs on
the owner's laptop on the night of 24 November 2026. The bundle of this tree was staged and checked, and the release's
succinct proof ran and verified on the laptop. The private attestation, claim and burn did not finish: four attempts
were each stopped within 2–4.25 minutes by the owner's rule for that night (stop if the load stays above 10), because
other local work ran beside the provers' 8 threads. Details and every number:
[`lez/results/perf-p4-laptop.md`](../../lez/results/perf-p4-laptop.md), section "The owner's laptop, 24–25 November
2026"; records: [`lez/results/p4-laptop-owner/`](../../lez/results/p4-laptop-owner/README.md). This stream edited
neither `docs/EVIDENCE.md` nor `docs/requirements-status.md` nor `docs/traceability.md` nor `docs/performance.md`.

Proofs here are real (`RISC0_DEV_MODE=0`). The laptop: AMD Ryzen 9 7940HS (8 cores, 16 threads), 54.7 GiB, Fedora, on
mains power, platform profile `low-power`; the provers held to CPUs 0–7 (`P4_CPUS=8`: cores 0–3, both threads of
each), niceness 19. Nothing was sent to Sepolia, mainnet or any public LEZ network.

## RFP-021 P.4: proposed **in part** (unchanged)

| clause (RFP wording) | shown by |
|---|---|
| wall-clock time and peak memory on the lowest specification the team declares as supported | the padded private attestation on an M1 with 16 GB, the declared Apple minimum (857.4 s, 9.26–9.45 GiB, no swap-out; EVIDENCE row 88), and in containers of the declared x86 minimum's 16 threads and 16 GiB (633.5 s, 9.19 GiB; row 168; 569.4 s from a fresh build, row 50) |
| state that minimum explicitly | `docs/performance.md` §P.4 (RFP-021): 16 GB, 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB |
| this includes running the attestation in a private transaction | the padded private attestation, settled with its receipt verified (rows 38, 88, 168) |
| wall-clock time and peak memory on a mid-range laptop | **not shown**: the run on the owner's laptop was stopped four times, no attestation proof finished (r0vm had reached 4.25 GiB); the bundle is staged and checked |

**What is short, and what it needs:** one run of `P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT` (or
`P4_JOBS=attest-pad` alone, about 21 minutes at the containers' rate) on the owner's laptop with other local work
stopped, preferably in a faster platform profile than `low-power` (the owner's choice; see the release's 2.8× below),
then `python3 -I bench/perf/summarize_p4.py --owner OUT`. Owner step; nothing from Logos.

**Proposed status text:** in part | This tree's padded private attestation proven at the declared minimum's 8 and 16
CPUs (1,254.8 s and 633.5 s), on an M1 and a GPU; on a laptop the owner's run of 24 November was stopped four times by
its load rule (other local work), so no attestation finished there; the bundle of this tree is staged and checked. |
32, 38, 50, 88, 168; `lez/results/perf-p4-laptop.md`

**When the run finishes** (the integrator's text then): shown | The padded private attestation on a Ryzen 9 7940HS
laptop held to 8 threads: X s, Y GiB peak, settled with the receipt verified; at the declared minimum: 633.5 s at 16
CPUs and 16 GiB, 857.4 s on an M1 with 16 GB (minimum stated in docs/performance.md). | 88, 168, the new row

## RFP-022 P.4: proposed **in part** (unchanged), its release now measured on the laptop with the current image

| clause (RFP wording) | shown by |
|---|---|
| wall-clock time and peak memory on the lowest specification declared as supported | the padded private claim and burn on an M1 with 16 GB (818.7 s, 716.5 s; the release's succinct proof 31.7 s; r0vm 9.26–9.45 GiB; row 88) and in 16-CPU, 16-GiB containers (639.2 s, 534.5 s; row 168; release 26.7 s, 1.5 GiB, row 33) |
| state that minimum explicitly | `docs/performance.md` §P.4 (RFP-022) |
| on a mid-range laptop: the release's succinct proof (the holder's) | **new:** `release_31` (RISC Zero 3.1), 129.0 s, 1.39 GiB, 77,736 user cycles, receipt 223,930 B, verified against the image id with the native journal: `lez/results/p4-laptop-owner/release-run/` (commit `919cd5d1`), from `P4_JOBS=release P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT` and `python3 -I bench/perf/summarize_p4.py --owner OUT` |
| on a mid-range laptop: the private claim and the private burn | **not shown**: no attempt reached them (the attestation runs first); bundle staged and checked (bridge program `0x847a4336…`, B1) |

**What is short, and what it needs:** the same owner run as RFP-021's (or `P4_JOBS=bridge-pad` alone, about 34 minutes
at the containers' rate). The Groth16 wrap is a relayer's step, not the user's (docs/design/bridge.md §12.2, §12.3).

**Proposed status text:** in part | Private claim and burn proven at the minimum's 8 and 16 CPUs (1,093.0 / 639.2 s and
932.8 / 534.5 s), on Mac minis and a GPU; on a laptop only the release: the current image's succinct proof 129.0 s,
1.39 GiB on a Ryzen 9 7940HS held to 8 threads in its low-power profile (the new row); the owner's laptop run of the
claim and burn was stopped by its load rule. | 33, 38, 50, 88, 168, the new row

## Proposed EVIDENCE row (new)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| new | The release's succinct proof, the holder's part of RFP-022 P.4, with this tree's release (`release_31`, RISC Zero 3.1) on the owner's laptop | **129.0 s**; 77,736 user cycles in one segment of 2^17; peak RSS **1.39 GiB**; receipt 223,930 B, verified against `0xf279af41…` with the journal equal to the native one; 983.2 CPU-seconds, against 344.9–345.8 for the same proof in 8-CPU containers (46.0 s, row 33) and 330.8 on the 32-thread build box (13.1 s) | laptop CPU: AMD Ryzen 9 7940HS held to 8 of its 16 threads (cores 0–3), platform profile `low-power`, mains power, niceness 19; other local work beside it (1.1–5.9 busy threads, median 2.6) | real | `bench/perf/p4-laptop-build.sh` (build box, `P4_RELEASE_BIN`), then `P4_JOBS=release P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`; `python3 -I bench/perf/summarize_p4.py --owner OUT` | `lez/results/perf-p4-laptop.md`; `lez/results/p4-laptop-owner/` |

## Proposed text for other files (the integrator's choice)

- `docs/traceability.md`, RFP-021 P.4, the sentence "The private attestation on a laptop: not measured (M0); the
  owner's script (`bench/perf/p4-laptop-owner.sh`) is ready and rehearsed." becomes: "The private attestation on a
  laptop: not measured; the owner's run of 24 November 2026 (Ryzen 9 7940HS, bundle of this tree) was stopped four
  times by its load rule, other local work beside the provers (`lez/results/perf-p4-laptop.md`)."
- `docs/traceability.md`, RFP-022 P.4, the sentence "The full private claim and burn on a laptop: not measured (M0); the
  owner's script is ready." becomes: "On the owner's laptop (24–25 November 2026): this tree's release, the holder's
  succinct proof, 129.0 s and 1.39 GiB held to 8 threads in the `low-power` profile (the new row); the full private
  claim and burn not measured, the run stopped by its load rule."
- `docs/performance.md`, both "Gaps" sentences of P.4: "a run on a real mid-range laptop (the owner's step ...)" gains
  "attempted on 24 November 2026 and stopped by its load rule; on that laptop the release's succinct proof took
  129.0 s (`perf-p4-laptop.md`)"; and the RFP-022 table gains the row "release, current image (`release_31`), the
  holder's succinct proof | **129.0 s**, peak 1.39 GiB, receipt 223,930 B | laptop CPU, 8 of 16 threads, low-power
  profile | `lez/results/perf-p4-laptop.md`".

## Files this stream changed

`bench/perf/p4-laptop-build.sh` (`P4_BUILD_DIR`; the release stage with `P4_RELEASE_BIN`; r0vm 3.0.5 taken from rzup's
copy; rzup isolated), `bench/perf/p4-laptop-owner.sh` (the optional release job; `P4_JOBS`; the niceness in effect,
platform profile, energy preference and battery in `machine.txt`), `bench/perf/summarize_p4.py` (`--owner`: the release
row, r0vm's sampled peak, the machine's memory at the start and at the peak), `lez/results/perf-p4-laptop.md`,
`lez/results/p4-laptop-owner/` (new), this file. No image id moved.
