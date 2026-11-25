# Stream p4-laptop: RFP-021 P.4 and RFP-022 P.4 on the owner's laptop (proposals for the integrator)

Branch `p4-laptop`, rebased onto `183c9ea2`. **Both rows proposed shown.** All four user-device proofs ran on the
owner's laptop on 24–25 November 2026 with this tree's programs, and each settled or verified:

| proof | time | result |
|---|---:|---|
| private attestation (RFP-021) | 4,596.7 s | settled |
| private claim (RFP-022) | 3,313.4 s | settled |
| private burn (RFP-022) | 2,802.0 s | settled |
| release's succinct proof (RFP-022) | 129.0 s | verified |

The declared minimum was already measured (EVIDENCE rows 33, 50, 88, 168) and is stated in `docs/performance.md`.

- Details and every number: [`lez/results/perf-p4-laptop.md`](../../lez/results/perf-p4-laptop.md), section "The
  owner's laptop, 24–25 November 2026".
- Records: [`lez/results/p4-laptop-owner/`](../../lez/results/p4-laptop-owner/README.md).
- This stream edited none of `docs/EVIDENCE.md`, `docs/requirements-status.md`, `docs/traceability.md` and
  `docs/performance.md`.

Proofs here are real (`RISC0_DEV_MODE=0`). Nothing was sent to Sepolia, mainnet or any public LEZ network.

**The laptop and its configuration.**

- AMD Ryzen 9 7940HS (8 cores, 16 threads), 54.7 GiB, Fedora, on mains power, platform profile `low-power`.
- The provers were held to CPUs 0–7 (`P4_CPUS=8`): cores 0–3 with both threads of each, half the machine.
- They ran at niceness 10 (the release at 19), with other sessions' local work on the other four cores.
- The LEZ proofs took 3.0–3.7 times, and the release 2.8 times, the 8-CPU containers' times: these are slow figures
  for this laptop.

## RFP-021 P.4: proposed **shown**

| clause (RFP wording) | shown by |
|---|---|
| wall-clock time and peak memory on a mid-range laptop | **The owner's laptop** (`lez/results/p4-laptop-owner/attest-pad-run/`): the padded private attestation, 4,596.7 s and 9.17 GiB peak (GNU time RSS of the prover with r0vm), 3,336,413 user cycles in 6 segments, receipt 239,246 B, transaction 255,366 B. Settled with the receipt verified (`prove_attest --settle`). The table is from `python3 -I bench/perf/summarize_p4.py --owner` |
| on the lowest specification the team declares as supported | An M1 with 16 GB, the declared Apple minimum: 857.4 s, 9.26–9.45 GiB, no swap-out (row 88). Containers of the declared x86 minimum's 16 threads and 16 GiB: 633.5 s, 9.19 GiB (row 168); 569.4 s from a fresh build (row 50) |
| state that minimum explicitly | `docs/performance.md` §P.4 (RFP-021): 16 GB, 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB |
| this includes running the attestation in a private transaction | The laptop run is the padded private attestation, settled; also rows 38, 88 and 168 |

**Proposed status text:** shown | The padded private attestation on the owner's laptop (Ryzen 9 7940HS held to 8 of its
16 threads, `low-power` profile) took 4,596.7 s at 9.17 GiB peak, settled. At the declared minimum: 857.4 s on an M1
with 16 GB and 633.5 s at 16 CPUs and 16 GiB. The minimum is stated in `docs/performance.md`. | 50, 88, 168, the new
row A

## RFP-022 P.4: proposed **shown**

| clause (RFP wording) | shown by |
|---|---|
| wall-clock time and peak memory on a mid-range laptop: the private claim and burn | **The owner's laptop** (`bridge-pad-run/`): claim 3,313.4 s and burn 2,802.0 s in one run, 9.17 GiB peak; 3,123,496 and 2,900,415 user cycles; receipts 239,628 and 239,046 B; transactions 256,130 and 254,966 B; both settled with the receipts verified (`prove_bridge --settle`). An earlier, stopped run proved the claim in 3,242.9 s (`bridge-pad-run-1-stopped/`) |
| the release's succinct proof (the holder's: it needs the burn secret) | **The same laptop** (`release-run/`): `release_31` (RISC Zero 3.1), 129.0 s, 1.39 GiB, 77,736 user cycles, receipt 223,930 B, verified against `0xf279af41…` with the native journal. The Groth16 wrap is a relayer's step, not the user's (`docs/design/bridge.md` §12.2, §12.3) |
| on the lowest specification declared as supported | An M1 with 16 GB: claim 818.7 s, burn 716.5 s, release 31.7 s; r0vm 9.26–9.45 GiB (row 88). 16-CPU, 16-GiB containers: 639.2 s and 534.5 s (row 168); release 26.7 s and 1.5 GiB (row 33) |
| state that minimum explicitly | `docs/performance.md` §P.4 (RFP-022) |

**Proposed status text:** shown | On the owner's laptop (Ryzen 9 7940HS held to 8 of its 16 threads, `low-power`
profile) the private claim took 3,313.4 s and the private burn 2,802.0 s, at 9.17 GiB, both settled. The release's
succinct proof (`release_31`) took 129.0 s at 1.39 GiB, verified. At the declared minimum: M1 818.7 / 716.5 / 31.7 s;
16 CPUs 639.2 / 534.5 / 26.7 s. The wrap is a relayer's. | 33, 50, 88, 168, the new rows A and B

## For the reviews: what these figures are and are not

- **The laptop ran at half its CPU and in its `low-power` profile.** The figures are 3.0–3.7 times the 8-CPU
  containers' (2.8 for the release). The release's CPU-seconds put each laptop thread at about a third of a container
  thread's work. Whether 77 minutes for a private attestation on this configuration is "practical" is the reviewers'
  judgment; the declared minimum's figures are 633–857 s. The laptop at a faster profile, or with all 16 threads, was
  not measured.
- **"Mid-range laptop".** A 2023 Ryzen 9 laptop held to 4 cores and 8 threads. The memory needed (9.17 GiB) does not
  depend on the machine: 9.19 GiB in the containers, 9.26–9.45 GiB on the M1, with no swap-out on its 16 GB.
- **One run each.** The claim alone was repeated (3,242.9 s in the stopped run, 3,313.4 s in the finished one, 2.1 %
  apart). Other sessions' local work ran on the other four cores throughout: a median of 5.5 busy threads during the
  attestation and 3.3 during the claim and burn, recorded every 5 s in `watch.log`.
- **Stopped attempts, all recorded.** Six attempts at the attestation and the first bridge run were stopped by those
  nights' stop rules, with no proof lost or altered. The rules changed twice, by the lead's decision, because r0vm's
  16-thread C++ pool on its 8 CPUs alone holds the load average at about 14–17 (`perf-p4-laptop.md`, "The stopped
  attempts").
- **The programs measured are this tree's.** The bundle was built at `3ae7bab2`, this branch before its rebase. Between
  `861c1c0d` and `183c9ea2`, main changed none of the guests, `prove_attest`, `prove_bridge`, `ferrata-e2e` or
  `docs/image-ids.md`. Image ids: attestation `0x816d1fef…`, consumer `0x8464603b…`, bridge (B1) `0x847a4336…`,
  `release_31` `0xf279af41…`.

## Proposed EVIDENCE rows (new)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| A | The user-device proofs of both RFPs on a laptop: this tree's padded private attestation, private claim and private burn, proven on the owner's laptop | attestation **4,596.7 s**, claim **3,313.4 s** (3,242.9 s in an earlier, stopped run), burn **2,802.0 s**; peak **9.17 GiB**; 3,336,413 / 3,123,496 / 2,900,415 user cycles; receipts 239,246 / 239,628 / 239,046 B; transactions 255,366 / 256,130 / 254,966 B; 3 of 3 settled with the receipts verified; 3.0–3.7× the 8-CPU containers of row 168 | laptop CPU: AMD Ryzen 9 7940HS, provers held to CPUs 0–7 (cores 0–3, both threads: 8 of 16), niceness 10, platform profile `low-power`, mains power; other local work on the other cores (median 3.3–5.5 busy threads) | real | `bench/perf/p4-laptop-build.sh` (build box, `P4_RELEASE_BIN`), then `P4_JOBS=attest-pad` and `P4_JOBS=bridge-pad`, `P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`; `python3 -I bench/perf/summarize_p4.py --owner OUT` | `lez/results/perf-p4-laptop.md`; `lez/results/p4-laptop-owner/` |
| B | The release's succinct proof, the holder's part of RFP-022 P.4, with this tree's release (`release_31`, RISC Zero 3.1) on the same laptop | **129.0 s**; 77,736 user cycles in one segment of 2^17; peak RSS **1.39 GiB**; receipt 223,930 B, verified against `0xf279af41…` with the native journal; 983.2 CPU-seconds, against 344.9–345.8 in 8-CPU containers (46.0 s, row 33) | laptop CPU: as row A, the prover at niceness 19 | real | as row A with `P4_JOBS=release` | as row A |

## Proposed text for other files (the integrator's choice)

**`docs/traceability.md`, RFP-021 P.4.** Replace:

> The private attestation on a laptop: not measured (M0); the owner's script (`bench/perf/p4-laptop-owner.sh`) is ready
> and rehearsed.

with:

> On the owner's laptop (Ryzen 9 7940HS held to 8 of its 16 threads, `low-power` profile), 25 November 2026: the padded
> private attestation 4,596.7 s, 9.17 GiB peak, settled (row A).

**`docs/traceability.md`, RFP-022 P.4.** Replace:

> The full private claim and burn on a laptop: not measured (M0); the owner's script is ready.

with:

> On the owner's laptop (the same configuration): the private claim 3,313.4 s and the private burn 2,802.0 s, 9.17 GiB,
> both settled; the release's succinct proof (`release_31`) 129.0 s, 1.39 GiB, verified (rows A, B).

**`docs/traceability.md`, the condition "The declared minimum is measured on cloud servers and Mac minis, not on a
laptop".** It becomes "on cloud servers, Mac minis and the owner's laptop". Add: "On the laptop, at half its CPU in its
`low-power` profile: attestation 4,596.7 s, claim 3,313.4 s, burn 2,802.0 s, release 129.0 s." The M0 sentence "user
proving at the declared minimum (cloud CPUs and an M1, not a laptop)" gains "and, since 25 November, the owner's
laptop".

**`docs/performance.md`, P.4 (RFP-021).** The "Gaps" sentence's "a run on a real mid-range laptop (the owner's step:
...)" becomes the laptop's figure, with its configuration and `perf-p4-laptop.md` as the source.

**`docs/performance.md`, P.4 (RFP-022).** The table gains two rows:

| proof | time | memory | where |
|---|---:|---:|---|
| private claim and burn, padded | 3,313.4 s and 2,802.0 s | 9.17 GiB | the owner's laptop |
| release, current image (`release_31`) | 129.0 s | 1.39 GiB | the owner's laptop |

Its "Gaps" sentence drops the laptop.

## Files this stream changed

| file | change |
|---|---|
| `bench/perf/p4-laptop-build.sh` | `P4_BUILD_DIR`; the release stage with `P4_RELEASE_BIN`; r0vm 3.0.5 from rzup's copy; rzup isolated |
| `bench/perf/p4-laptop-owner.sh` | the optional release job; `P4_JOBS`; `machine.txt` records the niceness in effect, the platform profile, the energy preference and the battery |
| `bench/perf/summarize_p4.py` | `--owner`: the release row, r0vm's sampled peak, the machine's memory at the start and at the peak |
| `lez/results/perf-p4-laptop.md` | updated |
| `lez/results/p4-laptop-owner/` | new |
| `docs/rows/p4-laptop.md` | this file |

No image id moved.
