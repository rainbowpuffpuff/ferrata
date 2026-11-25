# Stream p4-laptop: RFP-021 P.4 and RFP-022 P.4 on the owner's laptop (proposals for the integrator)

Branch `p4-laptop`, rebased onto `183c9ea2`; this version follows the first review (`review-p4`, findings 1–6).

**RFP-022 P.4 is proposed shown. RFP-021 P.4 stays in part.** RFP-021's reason is a user-proving gap the laptop run does
not touch: chains of hops inside the user's proof, below.

All four user-device proofs ran on the owner's laptop on 24–25 November 2026 with this tree's programs, and each settled
or verified:

| proof | time | result |
|---|---:|---|
| private attestation (RFP-021) | 4,596.7 s | settled |
| private claim (RFP-022) | 3,313.4 s | settled |
| private burn (RFP-022) | 2,802.0 s | settled |
| release's succinct proof (RFP-022) | 129.0 s | verified |

**The laptop ran below the declared minimum.** It used 8 of its 16 threads (cores 0–3), in its `low-power` profile,
with other work on the other cores and its 8 GiB swap full at the start of every run. It is a laptop measurement, not a
measurement at the minimum.

The declared minimum was measured on an M1 with 16 GB and in 16-CPU, 16-GiB containers (EVIDENCE rows 33, 50, 88, 168),
and is stated in `docs/performance.md`.

- Details and every number: [`lez/results/perf-p4-laptop.md`](../../lez/results/perf-p4-laptop.md), section "The
  owner's laptop, 24–25 November 2026".
- Records: [`lez/results/p4-laptop-owner/`](../../lez/results/p4-laptop-owner/README.md).
- This stream edited none of `docs/EVIDENCE.md`, `docs/requirements-status.md`, `docs/traceability.md` and
  `docs/performance.md`.

Proofs here are real (`RISC0_DEV_MODE=0`). Nothing was sent to Sepolia, mainnet or any public LEZ network.

## "Practical": M0 check 1

The RFP's first clause is "has to be practical on the desktop hardware Basecamp runs on". The project's written test
of it is M0 check 1 (`docs/release-engineering.md`): at most 20 min and 16 GB a proof at the declared minimum.

- **Met on the M1 with 16 GB** (row 88): attestation 14.3 min, claim 13.6 min, burn 11.9 min, release 31.7 s;
  9.26–9.45 GiB, no swap-out.
- **Met in the 16-CPU, 16-GiB containers** (rows 168, 33): 10.6, 10.7 and 8.9 min, release 26.7 s; 9.19–9.24 GiB.
- **Not met on the laptop, which was below the minimum:** 76.6, 55.2 and 46.7 min, release 129.0 s; 9.17 and 1.39 GiB.
  M0 already marks 8 threads NO-GO (the attestation takes 20.6 min in 8-CPU containers).
- **Risk** (*estimated*, not measured): the x86 minimum's figures come from server containers, and the one real
  laptop proved 2.8–3.7 times slower than 8-CPU containers at the same thread count, in its `low-power` profile. A
  laptop at the declared minimum in that profile could therefore take about 30–39 min an attestation (633.5 s ×
  2.8–3.7), above M0's 20 min.
- The run that settles it: this laptop with all 16 threads (`P4_CPUS` unset) in its default `balanced` profile. That is
  about 1.5 hours of the whole laptop (*estimated*: half the 3.0 hours the four proofs took at 8 threads), and the
  owner's call.

## RFP-022 P.4: proposed **shown**

| clause (RFP wording) | shown by |
|---|---|
| has to be practical on the desktop hardware Basecamp runs on | M0 check 1 met at the declared minimum: on an M1 with 16 GB, claim 13.6 min, burn 11.9 min, release 31.7 s (row 88); in 16-CPU, 16-GiB containers 10.7 and 8.9 min and 26.7 s (rows 168, 33); all within 16 GB. The risk for an x86 laptop at the minimum in a low-power profile (about 30–39 min, estimated) is stated above |
| wall-clock time and peak memory on a mid-range laptop: the private claim and burn | **The owner's laptop** (`lez/results/p4-laptop-owner/bridge-pad-run/`), below the minimum as above: claim 3,313.4 s and burn 2,802.0 s in one run, 9.17 GiB peak, 3,123,496 and 2,900,415 user cycles, receipts 239,628 and 239,046 B, transactions 256,130 and 254,966 B. Both settled with the receipts verified (`prove_bridge --settle`). An earlier, stopped run proved the claim in 3,242.9 s (`bridge-pad-run-1-stopped/`) |
| the release's succinct proof (the holder's: it needs the burn secret) | **The same laptop** (`release-run/`): `release_31` (RISC Zero 3.1), 129.0 s, 1.39 GiB, 77,736 user cycles, receipt 223,930 B, verified against `0xf279af41…` with the native journal. The Groth16 wrap is a relayer's step, not the user's (`docs/design/bridge.md` §12.2, §12.3) |
| wall-clock time and peak memory on the lowest specification declared as supported | An M1 with 16 GB: claim 818.7 s, burn 716.5 s, release 31.7 s; r0vm 9.26–9.45 GiB (row 88; programs built on the Macs from an earlier tree, release `0x17257f91…`). 16-CPU, 16-GiB containers: claim 639.2 s and burn 534.5 s (row 168; the bridge program before B1, `0xccde6171…`, whose claim and burn have user cycles within 0.2 % of B1's) and release 26.7 s, 1.5 GiB (row 33, `0x17257f91…`) |
| state that minimum explicitly | `docs/performance.md` §P.4 (RFP-022): 16 GB, 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB |

**Proposed status text:** shown | Practical by M0 check 1 at the declared minimum: claim, burn and release in 13.6,
11.9 and 0.5 min on an M1 with 16 GB, and 10.7, 8.9 and 0.4 min in 16-CPU, 16-GiB containers (the bridge program
before B1), all within 16 GB. On the owner's laptop, below the minimum (8 of 16 threads, `low-power`), B1's private
claim took 3,313.4 s and its burn 2,802.0 s, at 9.17 GiB, both settled; the release's succinct proof (`release_31`)
took 129.0 s at 1.39 GiB, verified. An x86 laptop at the minimum in a low-power profile may need about 30–39 min
(estimated). The wrap is a relayer's. | 33, 50, 88, 168, the new rows A and B

## RFP-021 P.4: proposed **in part** (unchanged status)

| clause (RFP wording) | shown by |
|---|---|
| has to be practical on the desktop hardware Basecamp runs on | **At 0 hops:** M0 check 1 is met at the declared minimum for the padded private attestation: 14.3 min on the M1 with 16 GB (row 88) and 10.6 min in 16-CPU, 16-GiB containers (row 168), within 16 GB. **With hops in the user's proof: not shown** (below) |
| wall-clock time and peak memory on a mid-range laptop | **The owner's laptop** (`lez/results/p4-laptop-owner/attest-pad-run/`), below the minimum as above: the padded private attestation took 4,596.7 s at 9.17 GiB peak (GNU time RSS of the prover with r0vm), 3,336,413 user cycles in 6 segments, receipt 239,246 B, transaction 255,366 B; settled with the receipt verified (`prove_attest --settle`) |
| wall-clock time and peak memory on the lowest specification declared as supported | An M1 with 16 GB: 857.4 s, 9.26–9.45 GiB, no swap-out (row 88). 16-CPU, 16-GiB containers: 633.5 s, 9.19 GiB (row 168, this tree's attestation program `0x816d1fef…`); 569.4 s from a fresh build (row 50) |
| state that minimum explicitly | `docs/performance.md` §P.4 (RFP-021) |
| this includes running the attestation in a private transaction | The padded private attestation, settled, in every run above (also row 38) |

**What is missing, exactly.** Every private attestation proven so far is at 0 hops. Hops inside the user's proof are
not declared with figures:

- **The design.** A private consumer reads the newest record, and "its reach is the chain its own proof can carry from
  there (the hops run in its proof: not measured beyond one hop)" (`docs/design/attestation.md` §6.2, line 270).
- **When hops are needed.** A fact more than 8,191 blocks behind the newest record (about 27.3 h on mainnet) needs hops
  inside the user's proof.
- **What exists for hops.** Hops were run through the deployed program in the executor: 0–32 hops, about 222,000–229,000
  cycles each (row 145). They were also run inside a private claim in dev mode: 3 hops (row 152). The bare guest's hop
  is 0.18M user cycles (`attestation.md` §5.1).
- **What does not exist.** No real proof with hops inside the user's proof has been made, and no limit on the hops a
  user's proof may carry is declared. `docs/performance.md` keeps "a chain of hops (a 320-hop claim is estimated at about
  58M accelerated user cycles)" as a gap: about 17 times the measured attestation's 3.34M user cycles.

To move it to shown, either of these:

1. **Declare the limit with figures** (a design decision for the owner and integrator; the figures below are *derived*,
   not measured). Assume time scales with user cycles; segment padding makes the real cost step-wise. At the declared
   minimum each hop then adds about 0.18–0.23M user cycles:
   - in the 16-CPU containers, 34–44 s a hop, so 12–16 hops fit within M0's 20 min;
   - on the M1, 46–59 s a hop, so 5–7 hops fit.

   A declaration could read: "user-side proving is supported for facts up to 5 hops behind the newest record (about
   41,000 blocks, 5.7 days on mainnet); measured at 0 hops; each hop adds about 0.18–0.23M user cycles (executor); a
   private consumer cannot reach further back".
2. **Or measure it.** Prove one real private attestation with a few hops at 16 CPUs: one Modal run, with a fixture of a
   fact a few windows back (row 152's three-hop case is the nearest). Then state the per-hop time.

**Proposed status text:** in part | The padded private attestation at 0 hops is practical at the declared minimum by M0
check 1 (857.4 s on an M1 with 16 GB, 633.5 s at 16 CPUs and 16 GiB, 9.19–9.45 GiB). It was also proven on the owner's
laptop below the minimum (4,596.7 s, 9.17 GiB, settled; 8 of 16 threads, `low-power`). Hops inside the user's proof
are neither proven nor declared: a fact more than 8,191 blocks back has no stated user-side cost or limit. | 50, 88, 168,
the new row A

## For the reviews: what these figures are and are not

- **The laptop is below the declared minimum and is not counted as it.** The run used 8 of 16 threads in the
  `low-power` profile, with other sessions' work on the other four cores (a median of 5.5 busy threads during the
  attestation's 76.6 minutes, 3.3 during the claim's and burn's 102.2 minutes). The 8 GiB swap was full at the start of
  every run, with 31.8–33.3 GiB in use by other work. The provers themselves did not page: 75 major page faults over the
  attestation, none in the bridge run or the release.
- **"Mid-range laptop".** A 2023 Ryzen 9 laptop held to 4 cores and 8 threads. The memory needed (9.17 GiB) does not
  depend on the machine: 9.19 GiB in the containers, 9.26–9.45 GiB on the M1, with no swap-out on its 16 GB.
- **One run each.** The claim alone was repeated: 3,242.9 s in the stopped run and 3,313.4 s in the finished one, 2.1 %
  apart.
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
| A | The user-device proofs of both RFPs on a laptop, below the declared minimum: this tree's padded private attestation, private claim and private burn, proven on the owner's laptop | attestation **4,596.7 s**, claim **3,313.4 s** (3,242.9 s in an earlier, stopped run), burn **2,802.0 s**; peak **9.17 GiB**; 3,336,413 / 3,123,496 / 2,900,415 user cycles; receipts 239,246 / 239,628 / 239,046 B; transactions 255,366 / 256,130 / 254,966 B; 3 of 3 settled with the receipts verified; 3.0–3.7× the 8-CPU containers of row 168; above M0's 20 min (8 threads is below the minimum) | laptop CPU: AMD Ryzen 9 7940HS, provers held to CPUs 0–7 (cores 0–3, both threads: 8 of 16, below the declared minimum's 16), niceness 10, platform profile `low-power`, mains power; other local work on the other cores (median 3.3–5.5 busy threads); 8 GiB swap full at each start | real | `bench/perf/p4-laptop-build.sh` (build box, `P4_RELEASE_BIN`), then `P4_JOBS=attest-pad` and `P4_JOBS=bridge-pad`, `P4_CPUS=8 bash bench/perf/p4-laptop-owner.sh STAGE OUT`; `python3 -I bench/perf/summarize_p4.py --owner OUT` | `lez/results/perf-p4-laptop.md`; `lez/results/p4-laptop-owner/` |
| B | The release's succinct proof, the holder's part of RFP-022 P.4, with this tree's release (`release_31`, RISC Zero 3.1), on the same laptop | **129.0 s**; 77,736 user cycles in one segment of 2^17; peak RSS **1.39 GiB**; receipt 223,930 B, verified against `0xf279af41…` with the native journal; 983.2 CPU-seconds, against 344.9–345.8 in 8-CPU containers (46.0 s, row 33) | laptop CPU: as row A, the prover at niceness 19 | real | as row A with `P4_JOBS=release` | as row A |

## Proposed text for other files (the integrator's choice)

**`docs/traceability.md`, the condition "The declared minimum is measured on cloud servers and Mac minis, not on a
laptop" (021: P.4; 022: P.4).** It stays as it is. After it, add:

> The owner's laptop was measured below it (8 of its 16 threads, `low-power` profile, other work on the other cores):
> attestation 4,596.7 s, claim 3,313.4 s, burn 2,802.0 s, release 129.0 s (rows A, B).

**The M0 sentence** "user proving at the declared minimum (cloud CPUs and an M1, not a laptop)" stays unchanged.

**`docs/traceability.md`, RFP-021 P.4.** Replace:

> The private attestation on a laptop: not measured (M0); the owner's script (`bench/perf/p4-laptop-owner.sh`) is ready
> and rehearsed.

with:

> On the owner's laptop, below the declared minimum (8 of 16 threads, `low-power` profile), 25 November 2026: the padded
> private attestation 4,596.7 s, 9.17 GiB peak, settled (row A). Hops inside the user's proof: not proven and no limit
> declared.

**`docs/traceability.md`, RFP-022 P.4.** Replace:

> The full private claim and burn on a laptop: not measured (M0); the owner's script is ready.

with:

> On the owner's laptop, below the declared minimum (8 of 16 threads, `low-power`): the private claim 3,313.4 s and the
> private burn 2,802.0 s, 9.17 GiB, both settled; the release's succinct proof (`release_31`) 129.0 s, 1.39 GiB,
> verified (rows A, B).

**`docs/performance.md`, P.4 (RFP-021).** In the "Gaps" sentence, replace only its laptop half, "a run on a real
mid-range laptop (the owner's step: ...)", with the laptop's figures below the minimum (`perf-p4-laptop.md`). The
second half, "a chain of hops (a 320-hop claim is estimated at about 58M accelerated user cycles)", stays.

**`docs/performance.md`, P.4 (RFP-022).** The table gains two rows:

| proof | time | memory | where |
|---|---:|---:|---|
| private claim and burn, padded (B1) | 3,313.4 s and 2,802.0 s | 9.17 GiB | the owner's laptop, below the minimum (8 of 16 threads, `low-power`) |
| release, current image (`release_31`) | 129.0 s | 1.39 GiB | the same |

Its "Gaps" sentence drops the laptop, and gains the estimated 30–39 min risk for an x86 laptop at the minimum in a
low-power profile.

## Files this stream changed

| file | change |
|---|---|
| `bench/perf/p4-laptop-build.sh` | `P4_BUILD_DIR`, refusing a directory whose removal would reach the repository; the release stage with `P4_RELEASE_BIN`; r0vm 3.0.5 from rzup's copy; rzup isolated |
| `bench/perf/p4-laptop-owner.sh` | the optional release job; `P4_JOBS`; `machine.txt` records the niceness in effect, the platform profile, the energy preference and the battery |
| `bench/perf/summarize_p4.py` | `--owner`: the release row, r0vm's sampled peak, the machine's memory at the start and at the peak |
| `lez/results/perf-p4-laptop.md` | updated |
| `lez/results/p4-laptop-owner/` | new |
| `docs/rows/p4-laptop.md` | this file |

No image id moved.
