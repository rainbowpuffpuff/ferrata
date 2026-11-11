# Proving on a CPU at the declared minimum: 8 and 16 CPUs, 16 GiB

RFP-021 P.4 and RFP-022 P.4 ask for the user-side proofs to be measured on a laptop and on the
declared minimum hardware. The proposal declares 16 GB of memory and 8 cores. Until this run the
repository had GPU figures for these proofs (one NVIDIA L4: the private attestation in 16.1 s, the
private claim and burn in 15.3 s and 12.4 s) and CPU figures from the build box only (up to 32
cores). Here the same proofs ran for real (`RISC0_DEV_MODE=0`) on the CPU, three times each, in a
cloud container held to the minimum, with `r0vm` 3.0.5 as the prover. Measured on 11 November 2026.
A second pass the same day ran them at 16 CPUs (what an 8-core x86 laptop's 16 threads give), proved the
release receipt and LEZ's own wallet's cheapest private transaction at 8 and 16 CPUs, and read what sets
the cost of LEZ's circuit: [below](#second-pass-16-cpus-the-release-receipt-and-lezs-own-yardstick-11-november-2026).

All figures below are **measured** in that container unless marked otherwise; every one is in the
logs under [`runs/`](runs/), and `summarize.py` recomputes the table from them.

## Result

| proof | runs | `execute_and_prove`, each run (s) | median (s) | user cycles proven | segments (cycles with padding) | peak memory, GiB: r0vm RSS / container (whole run, max of the runs) | receipt (B) | transaction (B) | receipt verified |
|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| private attestation | 3 | 904.6 / 904.3 / 902.2 | 904.3 | 2,355,665–2,355,666 | 5 (3,276,800) | 9.21 / 9.23 | 227,170 | 231,214 | yes, 3 of 3 |
| private claim | 3 | 902.5 / 905.4 / 909.8 | 905.4 | 2,136,850–2,136,957 | 5 (3,014,656) | 9.22 / 9.22 | 227,538 | 231,950 | yes, 3 of 3 |
| private burn | 3 | 756.3 / 770.0 / 754.2 | 756.3 | 1,913,422–1,913,488 | 4 (2,490,368) | 9.22 / 9.22 | 226,998 | 230,870 | yes, 3 of 3 |

Medians of three runs; `execute_and_prove` is the binary's own timer around LEZ's
`execute_and_prove`, the proving alone. A whole run, setup and LEZ's settlement included, took
908.4, 908.1 and 906.0 s for the attestation and 1,667.6, 1,684.5 and 1,673.3 s for the bridge's
claim plus burn (GNU time, `run.json`). The claim and the burn are one run of `prove_bridge`, so they
share its memory figure. User cycles are given as a range where the runs differ: LEZ's circuit
varies by up to 107 cycles between runs of the same transaction, the programs' transitions do
not.

- **Every receipt verified: 9 of 9.** After each run the same binary settled its transaction(s)
  through LEZ's private settlement with the receipt verified (`--settle`: "receipt verified",
  exit 0). After run 1, one flipped bit in the middle of each transaction file (bit 0 of byte
  115,607 of the attestation, 115,975 of the claim, 115,435 of the burn) made settlement refuse
  it: "Invalid privacy preserving execution circuit proof", exit 101 (`run-1/tampered-*.log`).
- **16 GiB is enough.** The largest process, `r0vm`, peaked at 9.21 to 9.22 GiB resident (9,664,900 KiB
  at most, `/usr/bin/time -v`), and the whole container at 9.23 GiB (9,910,034,432 bytes at most:
  the cgroup's `memory.usage_in_bytes`, sampled every 0.5 s, `run-*/samples.csv`), under the hard
  limit of 16 GiB. The peak is the same within 0.01 GiB for all three proofs; each includes
  segments of 2^20 cycles, r0vm's default segment limit.
- **The 8 CPUs are used.** Each run kept 7.53 to 7.55 CPUs busy on average (GNU time: 753–755 %
  CPU; r0vm's own CPU time 6,832–6,844 s per attestation run, from `/proc` samples).
- **Other figures in the repository**, not comparable one to one (other machines, other state,
  for row 10 an earlier program): one NVIDIA L4 GPU, 16.1 s for the attestation (EVIDENCE row 5,
  earlier images) and 15.3 s and 12.4 s for the claim and burn of the earlier bridge program
  `25dfd9b2…` (row 10); Modal CPUs without a fixed limit, 254.8 s for a private claim on the build
  box (row 11) and 276.8 s and 336.0 s for the final program's claim and burn, proven and sent, in
  the crown rehearsal (row 31).

### Where the time goes: LEZ's privacy-preserving circuit

r0vm logs every session it executes (`RISC0_INFO=1`, `RUST_LOG=info`): the dev-mode executions the
binaries make for their own tables and checks (milliseconds each), and the proven ones, each
followed by a gap of seconds to minutes while it is proven. LEZ's `execute_and_prove` proves each
private transition of the program (`prove(env, program.elf())`), then its
`PRIVACY_PRESERVING_CIRCUIT_ELF` with `ProverOpts::succinct()` over those receipts as assumptions
(`lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs`), so the last proven session
of each proof is LEZ's circuit. `summarize.py` takes the gap after each session as its proving time,
recursion included:

| proof | proven session | user cycles | segments (cycles with padding) | proving time, each run (s) | share of `execute_and_prove` (run 1) |
|---|---|---:|---:|---|---:|
| private attestation | the consumer's private transition 1 | 34,337 | 1 (131,072) | 26.2 / 26.5 / 26.4 | 3 % |
|  | the consumer's private transition 2 | 346,240 | 1 (524,288) | 105.2 / 107.8 / 105.8 | 12 % |
|  | LEZ's privacy-preserving circuit | 1,975,088–1,975,089 | 3 (2,621,440) | 773.0 / 769.9 / 769.9 | 85 % |
| private claim | the bridge program's private transition 1 | 41,160 | 1 (131,072) | 27.7 / 27.7 / 28.5 | 3 % |
|  | the bridge program's private transition 2 | 111,596 | 1 (262,144) | 56.7 / 55.4 / 56.6 | 6 % |
|  | LEZ's privacy-preserving circuit | 1,984,094–1,984,201 | 3 (2,621,440) | 817.5 / 821.8 / 824.2 | 91 % |
| private burn | the bridge program's private transition 1 | 25,377 | 1 (131,072) | 27.6 / 29.0 / 27.6 | 4 % |
|  | LEZ's privacy-preserving circuit | 1,888,045–1,888,111 | 3 (2,359,296) | 728.5 / 740.7 / 726.3 | 96 % |

The programs' own transitions take 4 to 15 % of each proof; LEZ's circuit, three segments of about
1.9 to 2.0 million user cycles, takes 85 to 96 %. EVIDENCE row 5's 380,577 user cycles are the
consumer's two transitions only (the same here, 34,337 + 346,240): `lee` does not report its
circuit, r0vm's log does. The final bridge program's transitions (41,160 + 111,596 user cycles for
the claim, 25,377 for the burn) are close to the earlier program's of BRIDGE-RESULTS §7 (39,314 +
113,417 and 23,799).

## Second pass: 16 CPUs, the release receipt and LEZ's own yardstick (11 November 2026)

An x86-64 laptop sold with 8 cores usually has 16 hardware threads (two per core); Modal's `cpu=8` above is 8
threads' worth of time, and Apple's cores have one thread each. So the same three proofs ran again at
`cpu=(16, 16)` with the same 16 GiB hard limit, three runs each; beside them, two proofs P.4 also needs:
the release receipt of the final release program, and the cheapest private transaction LEZ v0.4.0-rc1's
own wallet makes, as the yardstick of what any private transaction costs a user of LEZ. Same app
(`minimum.py`, run under the app name `p4-scale`), same `r0vm` 3.0.5, same checks inside every container
(`nproc`, CPU quota, memory limit, CPU identity; inputs against their manifest; every receipt verified, one
flipped bit refused). Tables: `python3 -I summarize_scale.py` (writes [`summary-scale.json`](summary-scale.json));
logs under `runs/<tag>/<job>/`.

### Result at 8 and at 16 CPUs (16 GiB hard limit for both)

| proof | 8 CPUs: `execute_and_prove` (s), each run | median | 16 CPUs: each run | median | 8 → 16 | user cycles proven (segments) | peak GiB, r0vm RSS / container | verified |
|---|---|---:|---|---:|---:|---:|---:|---|
| private attestation | 904.6 / 904.3 / 902.2 | 904.3 | 497.3 / 474.3 / 501.0 | 497.3 | 1.82× | 2,355,666 (5) | 9.25 / 9.23 | 6 of 6 |
| private claim | 902.5 / 905.4 / 909.8 | 905.4 | 371.4ᴵ / 469.3 / 463.7 | 463.7 | 1.95× | 2,136,957 (5) | 9.24 / 9.22 | 6 of 6 |
| private burn | 756.3 / 770.0 / 754.2 | 756.3 | 310.6ᴵ / 389.0 / 392.3 | 389.0 | 1.94× | 1,913,488 (4) | 9.24 / 9.22 | 6 of 6 |
| release, succinct (the holder's proof) | 46.0 / 46.0 | 46.0 | 26.4 / 27.0 | 26.7 | 1.72× | 77,736 (1) | 1.48 / 1.42 | 4 of 4 |
| LEZ's wallet: a deshielded native transfer (yardstick) | 999.2 / 984.1 | 991.7 | 490.4 / 486.9 | 488.6 | 2.03× | 2,851,842 (4) | 9.24 / 9.22 | 4 of 4 |

ᴵ an Intel Xeon (family 6, model 85, AVX-512) on AWS; every other run on AMD Zen 3 servers. The 8-CPU
columns of the first three rows are the first pass (above). User cycles: the most of the runs (LEZ's circuit
varies by up to 107 cycles between runs, the wallet's transfer by 247: its six dummy notes are random).
"8 → 16": the 8-CPU median over the 16-CPU median (*derived*). Every receipt was verified by LEZ's settlement
(the same binary, `--settle`, "receipt verified"); after run 1 of each container one flipped bit in each
transaction file was refused (exit 101, "Invalid privacy preserving execution circuit proof"). Memory is the
same at 8 and at 16 CPUs: segments of 2^20 cycles fix it, not the thread count.

At 16 CPUs the circuit's share is the same as at 8 (*measured*, proving time per session, `summarize_scale.py`):

| proof (16 CPUs) | proven session | user cycles | segments (padded) | proving time, each run (s) |
|---|---|---:|---:|---|
| private attestation | the consumer's transitions 1 and 2 | 34,337 + 346,240 | 1 + 1 (131,072 + 524,288) | 14.3 + 56.0 / 13.6 + 54.7 / 14.3 + 57.1 |
|  | LEZ's privacy-preserving circuit | 1,975,088–1,975,089 | 3 (2,621,440) | 426.9 / 405.8 / 429.5 |
| private claim | the bridge program's transitions 1 and 2 | 41,160 + 111,596 | 1 + 1 (131,072 + 262,144) | 10.8 + 21.2ᴵ / 14.5 + 28.9 / 14.3 + 28.4 |
|  | LEZ's privacy-preserving circuit | 1,984,079–1,984,085 | 3 (2,621,440) | 338.9ᴵ / 425.3 / 420.5 |
| private burn | the bridge program's transition | 25,377 | 1 (131,072) | 10.9ᴵ / 14.5 / 14.4 |
|  | LEZ's privacy-preserving circuit | 1,888,045 | 3 (2,359,296) | 299.4ᴵ / 374.2 / 377.7 |
| LEZ's wallet transfer | LEZ's privacy-preserving circuit (no program runs privately) | 2,851,614–2,851,842 | 4 (3,407,872) | 490.4 / 486.9 |

### The machines as measured (second pass)

Every container reported the shape it was given: `nproc`, `os.sched_getaffinity` and `/proc/cpuinfo` 16
(or 8), cgroup v1 `cpu.cfs_quota_us` 1,600,000 (or 800,000) per 100,000 and `cpuset.cpus` 0-15 (or 0-7),
`memory.limit_in_bytes` 17,179,869,184 (16 GiB), `MemTotal` 16,777,216 kB, no swap; gVisor ("4.19.0-gvisor"),
Ubuntu 24.04.5, glibc 2.39; `r0vm` sha256 `36c016a5…515b`; every input equal to its manifest. The CPU
model name is hidden ("unknown"); vendor, family and model are not (`runs/*/*/container.json`):

| tag / job (container) | CPUs | CPU (vendor, family, model; *derived* generation) | cloud, region | runs |
|---|---:|---|---|---|
| `cpu16-mem16g/attest` | 16 | AuthenticAMD 25 / 1 (Zen 3) | GCP us-central | attestation 1, 2 |
| `cpu16-mem16g-2/attest` | 16 | AuthenticAMD 25 / 1 (Zen 3) | Azure eu-west | attestation 3 |
| `cpu16-mem16g/bridge` | 16 | GenuineIntel 6 / 85 (Xeon Skylake-SP or Cascade Lake; `avx512f`) | AWS ap-south | claim and burn 1 |
| `cpu16-mem16g-2/bridge` | 16 | AuthenticAMD 25 / 1 (Zen 3) | Azure us-west | claim and burn 2, 3 |
| `cpu16-mem16g/transfer` | 16 | AuthenticAMD 25 / 1 (Zen 3) | GCP us-central | transfer 1, 2 |
| `cpu8-mem16g/transfer` | 8 | AuthenticAMD 25 / 1 (Zen 3) | Azure eu-west | transfer 1, 2 |
| `cpu16-mem16g/release` | 16 | AuthenticAMD 25 / 1 (Zen 3) | Azure us-west | release 1, 2 (proof and p254) |
| `cpu8-mem16g/release` | 8 | AuthenticAMD 25 / 1 (Zen 3) | Azure eu-west | release 1, 2 (proof and p254) |
| Groth16 step (`release_wrap`, one container per run) | 8 / 16 | Zen 3 (GCP us-west), Zen 4 (25 / 17, Azure us-east); Zen 3 (Azure eu-west, us-east) | | release 1, 2 at 8; 1, 2 at 16 |

Two calls did not run as planned, and the tables keep only what completed: the first 16-CPU call
(`cpu16-mem16g`) was cancelled by Modal at 09:24 UTC, after the attestation's run 2 and the bridge's run 1
("Received a cancellation signal while processing input"; the bridge's local client: "Deadline exceeded"),
most likely because the laptop's upload of a 149 MB bundle saturated its link and the detached clients lost the
server (*not verified*); the missing runs were made by a second call (`cpu16-mem16g-2`, attestation 1 run,
bridge 2 runs). The 8-CPU transfer's container was preempted once; Modal restarted the call from the start,
and only the restarted call's runs exist. Modal gives no choice of CPU model: the Intel container made the
fastest claim and burn (371.4 s, 310.6 s), and Zen 3 containers differ by up to 6 % (474.3 to 501.0 s for the
attestation).

### The release receipt: the holder's proof; the wrap is anyone's

The release is proven by the holder of the burn, on the user's device. The release guest's input is
private (`bridge/README.md`: `burnRoot ‖ burnIndex ‖ directions ‖ siblings ‖ sOut ‖ r ‖ …`), and the proof
"checks the path … to `burnRoot` and the opening of `COut`" (`docs/design/bridge.md` §12.2), where
`COut = H("ferrata/Cout/v1" ‖ sOut ‖ evmChainId ‖ vault ‖ recipient)` and `sOut = HMAC(kAuth, …)` is the
holder's burn secret (§4, §12.1). Whoever makes the proof needs `sOut` and learns which burn goes to which
recipient; nobody can make it for the user with nothing private. The Groth16 wrap is different: it takes
only the succinct receipt ("only succinct receipts go to a wrapper", §12.2; "any relayer can wrap a holder's
succinct receipt and learns only what the release publishes", §12.3), so it is a relayer's work, not a
user-device requirement. It is two steps: `identity_p254` (recursion in `r0vm`, `ferrata-e2e p254`) and
RISC Zero's Groth16 prover image (`risczero/risc0-groth16-prover:v2025-04-03.1`, which needs x86). Both ran
here in gVisor containers of the same shape, the image as a plain function as `e2e/crown/cloud/app.py` runs it.

The program: the final release guest (`NOut` v2), `release.bin` from the crown build (R0BF, 230,480 bytes,
sha256 `a6abe992…668e`, image `0x17257f91…f5cd`; `e2e/crown/README.md`). The witness: the crown rehearsal 3's
burn `private-1`, regenerated with `ferrata-e2e crown-release` from the repository's records of that run
(`e2e/runs/crown-rehearsal-3/`: `identity.json`, `checkpoint/input/burns.json`, `lez/session-b/burn-notes.json`);
its journal equals the rehearsal's `releases/release-private-1.json` (so does `public-0`'s). Executor: 77,736
user cycles, one segment of 2^17, journal 352 bytes, equal to the native one; the same for both witnesses.

| CPUs | run | succinct proof (s) | `identity_p254` (s) | Groth16 prover (s) | peak memory, GiB (proof; p254; Groth16 RSS / container) | verified |
|---:|---:|---:|---:|---:|---|---|
| 8 | 1 | 46.0 | 68.8 | 23.7 (Zen 3) | 1.46 / 1.42; 1.45; 6.61 / 6.67 | succinct; Groth16 |
| 8 | 2 | 46.0 | 70.8 | 18.8 (Zen 4) | 1.48 / 1.42; 1.45; 7.25 / 7.31 | succinct; Groth16 |
| 16 | 1 | 26.4 | 39.3 | 15.6 | 1.49 / 1.42; 1.47; 7.58 / 7.64 | succinct; Groth16 |
| 16 | 2 | 27.0 | 39.3 | 16.6 | 1.47 / 1.42; 1.46; 7.47 / 7.53 | succinct; Groth16 |

"Verified": `ferrata-e2e prove` verifies the succinct receipt against the image id and checks its journal
against the native one, and `ferrata-e2e verify` checks it again; `ferrata-e2e groth16` builds the Groth16
receipt from the prover's `proof.json` and verifies it, and `verify` checks its journal against the native one:
4 of 4, each seal 260 bytes with selector `0x73c457ba`, what the vault's verifier takes. The receipt is
223,930 bytes. The wrap took 89.6–92.5 s at 8 CPUs and 54.9–55.9 s at 16 (the two steps added; *derived*).

### LEZ's yardstick: the cheapest private transaction of LEZ's own wallet

[`harness/src/bin/prove_native_transfer.rs`](../../harness/src/bin/prove_native_transfer.rs) builds the
transaction exactly as LEZ v0.4.0-rc1's wallet does (`lez/wallet/src/lib.rs`, `send_proven` from line 1484;
`account_manager.rs`): a private account sends 5,000 native units to a public account and pays the fee in the
proof (the wallet requires a private payer, lib.rs line 1670); the wallet pads every private transaction to
`PADDED_PRIVATE_ACTIONS` = 7 private actions with dummy inputs (account_manager.rs lines 387, 656–670; here
one real account and six dummies, each with a real ML-KEM-768 epk and a random 512-byte note), pads every note
to `CIPHERTEXT_PAD_SIZE` = 512 bytes (line 35; lib.rs line 1585) and adds a recovery note for the fee's refund
to the payer's alias (`fee_recovery`, account_manager.rs line 791; lib.rs line 1695). No program runs
privately: the native token runs inside LEZ's circuit (`privacy_preserving_circuit/src/private_backend.rs`
line 66), so LEZ's circuit is the only session proven. It is the cheapest private transaction the wallet
makes: every one has at least one private account (the payer) and the same padding, and anything else adds
program transitions or private accounts. It settles through the same settlement as ours (execution gas
409,764, storage gas 224,063, the receipt verified); transaction 254,244 bytes, receipt 238,685 bytes.

At 16 CPUs it took 488.6 s (median of 2), as long as our private attestation (497.3 s): any private
transaction a LEZ user's wallet makes costs about what Ferrata's cost, at either size of machine
(991.7 s at 8 CPUs against 904.3 s).

### What sets the circuit's cycles, and could ours pay less (from LEZ's source; not changed)

Dev-mode executor counts of LEZ's circuit on the build box (`r0vm` 3.0.5 logs; `box-build-scale.sh`, logs in
`spike/tmp/p4-scale/box/dry/`, the counts copied here), all one private account paying its fee:

| transaction (dev mode) | circuit user cycles | segments (padded) | difference |
|---|---:|---:|---|
| bare: as this harness builds ours (no dummies, no padding, no recovery note, refund to a public account) | 1,855,254 | 3 (2,228,224) | |
| bare + the notes padded to 512 bytes | 1,915,403 | 3 (2,359,296) | padding: +60,149 |
| bare + a recovery note | 2,711,745 | 3 (3,145,728) | recovery note: +856,491 |
| the wallet's shape without dummies | 2,774,254 | 4 (3,211,264) | |
| the wallet's shape, 3 dummies | 2,812,312 | 4 (3,276,800) | about 12,700 per dummy |
| the wallet's shape (6 dummies, as proven above) | 2,852,164 | 4 (3,407,872) | wallet − bare: +996,910 |

What the cycles go to (`lee/privacy_preserving_circuit/src/`, LEZ `1806e87`):

- **ML-KEM-768 encapsulations, about 0.86M user cycles each.** Every private action's note is encrypted to a
  key that `SharedSecretKey::encapsulate_deterministic` derives (`output.rs` line 227; ML-KEM-768 from the
  `ml-kem` 0.3 crate, its SHAKE in software). A private account paying the fee gets two notes: its new state
  (`private_action`, line 199) and the fee's fallback note (`fee_fallback`, line 189, a second `private_action`
  for the payer); a recovery note (`bind_recovery`, `state_machine/core/src/recovery.rs` line 88) is a third.
  The recovery note alone measured +856,491; so the two notes every one of our transactions carries are about
  1.7M of the bare transfer's 1.86M (*estimated*: 92 %).
- **Dummies, padding, tree depth, journal: small.** A dummy is hashes only (`Nullifier::for_dummy`,
  `Commitment::for_dummy`; its note is given, not encrypted: `output.rs` lines 74–82, 110–150): about 12,700.
  Padding the notes to 512 bytes: +60,149 for two (ChaCha20 over more bytes). The commitment tree's depth costs
  one accelerated SHA-256 compression per level (`compute_digest_for_path`, `core/src/commitment.rs` line 117;
  r0vm logs about 90 cycles per SHA-256 call): under 3,000 cycles at depth 32 (*derived*). The journal is
  hashed once (`main.rs` line 135), a few thousand cycles for these outputs.
- **Our programs' part of the circuit: 2–7 %.** Each private transition is verified as an assumption
  (`env::verify` over its borsh frame, `private_backend.rs` line 77). Against the bare transfer (same shape,
  same harness): attestation +119,835, claim +128,947, burn +32,857 user cycles (*derived* from the counts).

Could our transactions pay less? Not much, and not by fewer accounts or smaller outputs: each has one private
account, the fewest possible, since the fee must come from a private account inside the proof. Halving the
transitions' frames would save at most about 65,000 circuit cycles (3 %, *estimated*), and merging the
attestation's two transitions into one would save about the cost of the smaller one's segment (26 s of 904 s
at 8 CPUs, 14 s of 497 s at 16, *estimated* from its proving time). The rest is LEZ's per-account cost: two
encapsulations, which only LEZ can make cheaper (an accelerated SHAKE, or one note where the fee's fallback now
takes a second). In the other direction: a client that pads as LEZ's wallet pads (for the same anonymity set)
adds 996,910 cycles to each of our transactions, which would make the attestation about 3.35M user cycles and,
*estimated* from the wallet transfer's times, about 10 minutes at 16 CPUs and 20 at 8.

### Commands (second pass)

```sh
# 1. On the build box (mrun --profile ws-4), from lez/ of this worktree, with $S a scratch directory
#    beside the repository holding in/release.bin (the crown build's R0BF release program):
bash results/p4-minimum/box-build-scale.sh     # builds prove_native_transfer (debug) and ferrata-e2e (release),
                                               # the dev-mode cycle counts, the release witnesses, both stages
strip -o $S/box/stage-stripped/transfer/prove_native_transfer target/debug/prove_native_transfer
                                               # 156,093,544 -> 11,393,944 bytes, the same code (manifest records both)
# 2. From the laptop, in lez/results/p4-minimum, with MODAL_PROFILE=ws-4 P4_APP=p4-scale P4_MEM_MIB=16384:
modal volume put p4-minimum $S/box/stage-stripped/transfer /bundle/transfer
modal volume put p4-minimum $S/box/stage/release /bundle/release
P4_CPU=16 modal run --detach minimum.py::main --job attest --runs 3 --tag cpu16-mem16g     # cancelled after run 2
P4_CPU=16 modal run --detach minimum.py::main --job bridge --runs 3 --tag cpu16-mem16g     # cancelled after run 1
P4_CPU=16 modal run --detach minimum.py::main --job attest --runs 1 --tag cpu16-mem16g-2
P4_CPU=16 modal run --detach minimum.py::main --job bridge --runs 2 --tag cpu16-mem16g-2
P4_CPU=8  modal run --detach minimum.py::main --job transfer --runs 2 --tag cpu8-mem16g
P4_CPU=16 modal run --detach minimum.py::main --job transfer --runs 2 --tag cpu16-mem16g
P4_CPU=8  modal run --detach minimum.py::release_main --runs 2 --tag cpu8-mem16g
P4_CPU=16 modal run --detach minimum.py::release_main --runs 2 --tag cpu16-mem16g
modal volume get p4-minimum /results/<tag>/<job> runs/<tag>/
python3 -I summarize_scale.py
```

Inside the containers, besides the commands of the first pass:

```sh
# LEZ's wallet yardstick (job transfer)
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_native_transfer --shape wallet --tx-out tx-transfer.borsh > prove.log 2>&1
./prove_native_transfer --settle tx-transfer.borsh > settle.log 2>&1            # receipt verified
# the release (release_prove, then release_wrap in RISC Zero's prover image, then release_finish)
ferrata-e2e exec --program release.bin --input witness/release-private-1.input --meta witness/release-private-1.json
/usr/bin/time -v ferrata-e2e prove --program release.bin --input <input> --meta <meta> --out-dir run-<i>/prove
/usr/bin/time -v ferrata-e2e p254 --receipt run-<i>/prove/receipt-succinct.bin --image-id 0x17257f91…f5cd --out-dir run-<i>/wrap
bash /app/prover.sh                                                     # /mnt/input.json -> /mnt/proof.json
ferrata-e2e groth16 --receipt run-<i>/prove/receipt-succinct.bin --image-id 0x17257f91…f5cd --proof proof.json --out-dir run-<i>/groth16
ferrata-e2e verify --receipt run-<i>/groth16/receipt-groth16.bin --image-id 0x17257f91…f5cd --meta <meta>
```

## The machine as measured (first pass)

A Modal container (a gVisor sandbox) of the function `prove` in [`minimum.py`](minimum.py), with
`cpu=(8, 8)` and `memory=(16384, 16384)`: request equal to limit for both, so the 8 CPUs and the
16 GiB are reserved and are also the ceiling. What the container reports (`runs/*/container.json`,
`runs/probe-*.json`):

| | |
|---|---|
| `nproc`, `os.sched_getaffinity`, `/proc/cpuinfo` processors | 8, 8, 8 |
| CPU quota (cgroup v1) | `cpu.cfs_quota_us` 800,000 per `cpu.cfs_period_us` 100,000 = 8 CPUs of time; `cpuset.cpus` 0-7 |
| memory limit (cgroup v1) | `memory.limit_in_bytes` 17,179,869,184 (16 GiB); `MemTotal` 16,777,216 kB; no swap |
| CPU | `model name` "unknown" (gVisor hides it); `vendor_id` AuthenticAMD, `cpu family` 25, `model` 1: AMD's Zen 3 server generation (EPYC "Milan"; *derived* from family and model); flags with `avx2`, `bmi2`, `adx`, `sha_ni`, without `avx512f` |
| where | attestation: GCP, Modal region ap-northeast (`bogomips` 2450.00); bridge: Azure, us-east (`bogomips` 3099.27). Each job ran in its own container, the two at the same time; the three runs of a job in one container, one after the other |
| system | kernel string "4.19.0-gvisor"; Ubuntu 24.04.5 LTS, glibc 2.39 |

The shape was checked, not taken on trust ([`runs/probe-cpu8.json`](runs/probe-cpu8.json),
[`runs/probe-cpu8-alloc18.txt`](runs/probe-cpu8-alloc18.txt)): busy loops in 4, 8 and 16
processes obtained 3.93, 7.83 and 7.02 CPUs, never more than 8; a child that allocated and touched
memory 1 GiB at a time was killed after 15 GiB of 18 (Modal: "Runner killed (SIGKILL), exit code:
137"). With `cpu=4` the same container reports 4 of everything ([`runs/probe-cpu4.json`](runs/probe-cpu4.json)).

What "8 cores" means here: 8 hardware threads' worth of time on server cores. Whether two of them
share a physical core is not visible from inside. A laptop sold with "8 cores" usually has 8
physical cores and 16 threads, often with faster cores, but it may also throttle under a 15-minute
load; these figures are not a laptop's.

## What was proven, with what

- **Sources**: branch `p4-minimum` = `integration5` at `0b135f7` plus this directory; LEZ v0.4.0-rc1
  (`1806e87`). The merged programs, built at this worktree's path. Their image ids
  (`--image-ids`): consumer `3f357bae4537eafd9a922befade5275e7cf3453deb91c93d1698631ff9feee6d`
  (419,456 ELF bytes; the image the attestation discloses), attestation program
  `e517399a94269f1a65b50c53c6187cd630c1f8f6d91f9617619ef94a3cba7177`, bridge
  `1156918fdfee530b4fc9cb1eae2078d161d21cb11e26ffbfc7f75aa427de9773` (541,456 ELF bytes). They differ
  from other worktrees' ids because a guest ELF built without Docker holds its absolute build paths
  ([`gpu-prover/README.md`](../../../gpu-prover/README.md)); the sources are the same.
- **The proofs**: the commands of the GPU jobs `lez-attest` and `lez-bridge`
  (`../../../gpu-prover/modal_app.py`), with the CPU `r0vm` in place of the CUDA one. `prove_attest`:
  one private attestation on the mainnet fixture `mainnet-attest-2026-11-10` (`../../RESULTS.md` §5).
  `prove_bridge`: the private claim of vault A's leaf 5 (1 ETH) and then that holder's private burn,
  on the state of the tests (`../../BRIDGE-RESULTS.md` §3, §4, §7). Each is LEZ's
  `execute_and_prove`; the receipt is the succinct `InnerReceipt` of LEZ's circuit, borsh.
- **Binaries**: `lez/target/debug/prove_attest` (157,634,064 bytes, sha256 `7699c9d2567d…`) and
  `prove_bridge` (158,126,064 bytes, `b92df59c2248…`), built on the build box (Modal, through
  `mrun`): `cargo build -p harness --locked --bin prove_attest --bin prove_bridge` in 5 min 10 s,
  with `RISC0_HOME` the guest toolchain of `../../RESULTS.md` §8 and `RISC0_BUILD_LOCKED=1`. Debug
  host binaries, as for the GPU runs: the proving is done by `r0vm`, a release binary, in its own
  process. Staged with `lez/scripts/stage-prove-attest.sh` and `stage-prove-bridge.sh`
  (manifests in [`runs/build-box.txt`](runs/build-box.txt)), moved to the container through a Modal
  Volume, not rebuilt; each container checked every staged file against its manifest.
- **r0vm**: 3.0.5, the CPU build that RISC Zero ships with `cargo-risczero` 3.0.5 (`rzup`):
  108,998,816 bytes, sha256 `36c016a5bb2ded5bd1f8f92cc487e6ffaeb1e95ec05850c983081a0f716b515b`, the
  same file on the build box and on the laptop it was uploaded from.

The commands, in the order they ran:

```sh
# 1. On the build box, from lez/ of this worktree (box-build.sh, through the build box's mrun):
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1
cargo build -p harness --locked --bin prove_attest --bin prove_bridge
RISC0_DEV_MODE=1 target/debug/prove_attest --allow-dev-mode --tx-out attest-dry.borsh      # dry runs: the same
RISC0_DEV_MODE=1 target/debug/prove_bridge --allow-dev-mode --claim-out claim-dry.borsh \  # commands, fake
  --burn-out burn-dry.borsh                                                                # receipts, settled
FERRATA_HEAD=0b135f76ebd6e12787ca9ebf1f5b83c53519a69c FERRATA_DIRTY=0 \
  scripts/stage-prove-attest.sh STAGE/attest target/debug/prove_attest
FERRATA_HEAD=0b135f76ebd6e12787ca9ebf1f5b83c53519a69c FERRATA_DIRTY=0 \
  scripts/stage-prove-bridge.sh STAGE/bridge target/debug/prove_bridge

# 2. From the laptop (network only), in lez/results/p4-minimum, with MODAL_PROFILE=ws-4:
modal volume put p4-minimum STAGE/attest /bundle/attest
modal volume put p4-minimum STAGE/bridge /bundle/bridge
modal volume put p4-minimum ~/.risc0/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm /bundle/r0vm/r0vm
P4_CPU=8 P4_MEM_MIB=16384 modal run minimum.py::probe_main                    # runs/probe-cpu8.json
P4_CPU=8 P4_MEM_MIB=16384 modal run minimum.py::probe_main --alloc-gib 18     # runs/probe-cpu8-alloc18.txt
P4_CPU=8 P4_MEM_MIB=16384 modal run --detach minimum.py::main --job attest --runs 3 --tag cpu8-mem16g
P4_CPU=8 P4_MEM_MIB=16384 modal run --detach minimum.py::main --job bridge --runs 3 --tag cpu8-mem16g
modal volume get p4-minimum /results/cpu8-mem16g/attest runs/
modal volume get p4-minimum /results/cpu8-mem16g/bridge runs/
python3 -I summarize.py                     # writes summary.json, prints the two tables
```

Inside the container, each run is (`minimum.py`, `prove`):

```sh
export RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/root/p4/bin/r0vm     # r0vm 3.0.5, CPU, from the Volume
# the attestation
export FERRATA_EXPECT_CONSUMER_IMAGE_ID=3f357bae…ee6d FERRATA_ATTEST_FIXTURE_DIR=<bundle>/mainnet-attest-2026-11-10
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_attest --tx-out tx-attest.borsh > prove.log 2>&1
./prove_attest --settle tx-attest.borsh > settle.log 2>&1                  # LEZ's settlement, receipt verified
# the claim and the burn
export FERRATA_EXPECT_BRIDGE_IMAGE_ID=1156918f…9773
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_bridge --claim-out tx-claim.borsh --burn-out tx-burn.borsh > prove.log 2>&1
./prove_bridge --settle tx-claim.borsh tx-burn.borsh > settle.log 2>&1
```

The settlements charge what the dev-mode runs charge (`settle.log`): execution gas 421,641 for
the attestation, 551,502 and 518,358 for the final program's claim and burn.

## Repeat measurement 1 on a laptop (x86-64 Linux, CPU only)

The prebuilt binary and its input, as the minimum container ran them: no build, about a minute of
downloads, then the proof. Needs glibc 2.39 or newer (the binary was built on Ubuntu 24.04), about
10 GiB of free memory, 0.5 GB of disk, and the Modal client for the download. **Timing**: in the
container above the proof took 902–905 s and the whole run 906–908 s, so the 15 minutes hold only on
a machine faster than that container; `r0vm` uses every CPU the machine has.

```sh
mkdir -p ~/p4-attest && cd ~/p4-attest
# r0vm 3.0.5 for the CPU: RISC Zero's release, the same file as measured
curl -L https://risczero.com/install | bash
~/.risc0/bin/rzup install cargo-risczero 3.0.5
R0VM=~/.risc0/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/r0vm
sha256sum $R0VM          # 36c016a5bb2ded5bd1f8f92cc487e6ffaeb1e95ec05850c983081a0f716b515b
# the prebuilt prove_attest and the fixture it reads (152 MB), from the Volume the container read
MODAL_PROFILE=ws-4 modal volume get p4-minimum /bundle/attest .
cd attest && chmod +x prove_attest
python3 -I -c 'import hashlib,json; m=json.load(open("MANIFEST.json")); print([p for p,v in m["files"].items() if hashlib.sha256(open(p,"rb").read()).hexdigest()!=v["sha256"]] or "all files match MANIFEST.json")'
./prove_attest --image-ids          # consumer image 3f357bae…ee6d
# the proof (real, on the CPU), then LEZ's settlement with the receipt verified
export FERRATA_EXPECT_CONSUMER_IMAGE_ID=3f357bae4537eafd9a922befade5275e7cf3453deb91c93d1698631ff9feee6d \
  FERRATA_ATTEST_FIXTURE_DIR=$PWD/mainnet-attest-2026-11-10 RISC0_DEV_MODE=0 RISC0_SERVER_PATH=$R0VM
RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v ./prove_attest --tx-out tx-attest.borsh > prove.log 2>&1
grep -E 'REAL PROOF|Elapsed|Maximum resident|Percent of CPU' prove.log
./prove_attest --settle tx-attest.borsh | grep 'receipt verified'
python3 -I <repository>/lez/results/p4-minimum/summarize.py --log prove.log   # cycles, segments, time per session
```

To hold a larger machine to the minimum, put
`systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 taskset -c 0-7` in front of
`/usr/bin/time` (8 CPUs, 16 GiB; needs cgroup v2 and a systemd user session). Without it the figure
is the laptop's own, which is what P.4's "on a laptop" asks for.

Checked on the owner's laptop (Fedora, a glibc newer than 2.39), everything but the proof: the
download (16.6 s), the hashes, `--image-ids`, and `--settle` of run 1's transaction from the
container ("receipt verified", 2.1 s wall). The proof itself was not run on the laptop. Without the
Volume, rebuild and restage with step 1 of the commands above (about 5 minutes of build on the
build box; on a laptop, longer): built at another path the image ids differ, so set
`FERRATA_EXPECT_CONSUMER_IMAGE_ID` to what `--image-ids` prints.

## What this does not show

- A laptop, or Apple Silicon. These are cloud servers held to the minimum's numbers (and to 16 CPUs); the
  owner's laptop run (above) is the other half of P.4.
- Our transactions as a wallet that pads like LEZ's would make them: only *estimated* (second pass, last
  paragraph of "What sets the circuit's cycles"), not proven.
- Other memory settings: 16 GiB was enough with r0vm's default segment size (2^20 cycles), so no
  smaller limit was tried. A smaller `segment_limit_po2` would lower the peak and lengthen the proof;
  not measured.
- Variance across machines: in the first pass three runs in one container per job (spreads of 2.4, 7.3
  and 15.8 s for the attestation, the claim and the burn), on one CPU generation; in the second, two
  containers for some jobs and one Intel container among the AMD ones (the tables name each run's machine).
  Containers that ran at the same time may have shared a host; that is not known.

## Files

| Path | What |
|---|---|
| `minimum.py` | The Modal app: the probe and the proving function, with its memory sampler and verification |
| `summarize.py` | The two tables from `runs/` (writes `summary.json`); `--log FILE` reads one `prove.log` |
| `summary.json` | Its output for these runs: every figure of the tables, per run, with the machine |
| `box-build.sh` | The build-box script (build, dry runs, staging) |
| `runs/build-box.txt` | The build box's toolchains, the build's timing, the image ids, the dry runs and both stage manifests |
| `runs/probe-cpu4.json`, `runs/probe-cpu8.json`, `runs/probe-cpu8-alloc18.txt` | What the container reports at `cpu=4` and `cpu=8`, the CPU test and the memory-limit test |
| `summarize_scale.py`, `summary-scale.json` | The second pass's tables from `runs/<tag>/<job>/` (and the first pass's runs, for the comparison), with each run's machine |
| `box-build-scale.sh` | The second pass's build-box script: `prove_native_transfer` and `ferrata-e2e`, the dev-mode cycle counts, the release witnesses, the stages |
| [`../../harness/src/bin/prove_native_transfer.rs`](../../harness/src/bin/prove_native_transfer.rs) | LEZ's wallet yardstick: a deshielded native transfer built as the wallet builds it (`--shape wallet`) or as this harness builds its own (`--shape bare`) |
| `runs/cpu16-mem16g/`, `runs/cpu16-mem16g-2/`, `runs/cpu8-mem16g/` | The second pass, per tag and job: `attest`, `bridge`, `transfer` as the first pass's; `release`: `container.json`, `exec.log`, per run `prove.log`, `p254.log`, `run.json`, `prove/` (receipt, `prove.json`), `wrap/` (`input.json`, `proof.json`, `groth16-run.json` with the prover container's machine, `groth16.log`), `groth16/` (the Groth16 receipt, `seal.hex`, `journal.hex`), `finish.json` |
| `runs/attest/`, `runs/bridge/` | `container.json` (the machine, the manifest, r0vm's hash, the image ids) and per run `prove.log` (the binary's output, r0vm's session log, GNU time), `settle.log`, `run.json`, `samples.csv` (memory every 0.5 s); run 1 also the transactions (`tx-*.borsh`, receipts inside) and the tamper checks |
