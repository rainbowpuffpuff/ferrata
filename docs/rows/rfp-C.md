# Stream rfp-C: measurements for the performance and cost rows (proposals for the integrator)

Branch `rfp-measure` (from `f13faee2`). Every figure comes from a command run on this branch, on 21 November 2026;
*derived* marks a figure computed from measured inputs. Where the build box ran a command, its copy of the tree holds
no `.git`; the commit is the branch's at that time. The integrator applies the statuses and EVIDENCE texts; this
stream edited neither `docs/EVIDENCE.md` nor `docs/requirements-status.md` nor `docs/traceability.md`.

Proofs here are real (`RISC0_DEV_MODE=0`) unless a row says otherwise. "Cloud container" means a Modal container held
to the CPUs and memory named (request = limit). Nothing was sent to Sepolia, mainnet or any public LEZ network.

## RFP-021

### P.2 — proposed **shown** (was in part; the review's H2 fixed by proving both missing actions)

| clause (RFP wording) | shown by |
|---|---|
| breakdown by program action: advancing the tracked state | `lez/results/perf-proofs.md` §"Per action": `Init` (bootstrap, adoption in two parts), a committee handoff (`Open`, `Miller`, two adoption parts, `Finish`) and a finality update (`Open`, `Miller`, `Finish`), each transaction with its transitions |
| … and statement verification by type of statement | the same table: account, absent account, storage slot with a predicate, absent slot, ERC-20 `Transfer` log, a log in a 306-log receipt, a slot through one EIP-2935 hop, four facts and a relation; a public consumer's `CallAttest` (slot, log); the reference consumer's claim |
| cycle count and gas cost for each | the same rows (execution gas equals the user cycles proven in every row; storage gas beside it) |
| proof time and proof size for each | 37 transitions proven for real, one 16-CPU, 32-GiB cloud container each: composite proof time and size, compression to succinct, succinct receipt size, peak memory, machine (`perf-proofs.md` §"Per transition"); the proven update path in one private transaction, this tree's program: 9,049,362 user cycles, **1,673.6 s** on 16 CPUs, 9.21 GiB, receipt 227,120 B, settled with the receipt verified at 485,298 + 224,063 gas (`perf-proofs.md` §"The proven update path"); **the committee handoff on the proven path** in one private transaction (`prove_private --handoff`): 15,796,512 user cycles, **2,254.2 s** on 16 CPUs, 9.20 GiB, receipt 227,347 B, settled with the receipt verified at 451,895 + 224,063 gas; **instance succession**: `Succeed` 45,512 and `Spawn` 25,098 user cycles (the transaction's 70,610 execution gas), 14.6 s each composite, succinct 224,827 / 224,464 B (`perf-proofs.md` §"Added 21 November", records `bench/perf/results/p2-b/`, commits `96747309`, `beaf71e9`); the private attestation, padded: 633.5 s / 1,254.8 s at 16 / 8 CPUs, receipt 239,246 B (`perf-p4-laptop.md`) |
| extending RFP-020's methodology | each guest execution proven with exactly the input LEZ v0.4.0-rc1 gives it (`trace::TAPE`), composite then succinct, both verified, journal equal to the executor's (`lez/harness/src/bin/perf_prove.rs`) |
| reproducible from the test suite | `perf_prove --dump` runs the test suite's scenarios (`lez/results/stage1-*.md`, `stage4-attest.md`) and keeps each transition's input; `bash bench/perf/prove-actions.sh OUT` proves them on one machine, `bench/perf/prove_modal.py` in parallel; `python3 -I bench/perf/summarize_proofs.py bench/perf/results/p2-a` writes the table |

Commands: on the build box `bash bench/perf/p2-stage.sh STAGE` (and `P2_WITH_PRIVATE=1`); then `P2_CPU=16 modal run
bench/perf/prove_modal.py --tag a --index STAGE/dump/index.tsv` and `--private 1`. Records: `bench/perf/results/p2-a/`
(37 `prove-NNN.log`, `machine-NNN.json`, `private-1.log`, `private-1-tx.borsh`). Commits `e89392a0`, `750a36c5`,
`b60ff84a`, and for the handoff and succession `bash bench/perf/p2-stage.sh STAGE` (`P2_WITH_PRIVATE=1`), `P2_CPU=16
P2_BUNDLE=/bundle/p2b modal run bench/perf/prove_modal.py --tag b --private 1 --handoff` and `--index
STAGE/dump/index.tsv --only 38,39`. Caveat: the containers' hosts differ (mostly AMD Zen 3; some Zen 4 and Intel),
named per row.

Proposed EVIDENCE row: | new | RFP-021 P.2's proof time and size for every program action and statement type: the 37
guest executions of the test suite's scenarios, each with the input LEZ v0.4.0-rc1 gives it, proven for real
(composite, then succinct, both verified, journals equal to the executor's), and the whole finality update proven in
one private transaction and settled | per action and statement type: execution gas = user cycles proven; e.g. a
finality update's `Open` / `Miller` / `Finish` 307.9 / 410.1 / 514.0 s composite on 16 CPUs, `Succeed` and `Spawn`
14.6 s each, succinct receipts
224,041–228,025 B; a storage-slot statement 81.0 s, 230,663 B; the 306-log receipt's log 277.1 s, 282,105 B; the
proven update path 9,049,362 user cycles, 1,673.6 s, 9.21 GiB, receipt 227,120 B, 485,298 + 224,063 gas; the proven
committee handoff 15,796,512 user cycles, 2,254.2 s, 9.20 GiB, receipt 227,347 B, 451,895 + 224,063 gas | cloud
containers held to 16 CPUs and 32 GiB (Modal), r0vm 3.0.5 | real | `bash bench/perf/prove-actions.sh OUT` (one
machine) or `bench/perf/prove_modal.py`; `python3 -I bench/perf/summarize_proofs.py bench/perf/results/p2-a` |
`lez/results/perf-proofs.md` |

### P.3 — proposed **shown** (was in part; the review's M3 fixed: the once-per-period cadence restored)

| clause | shown by |
|---|---|
| the amortised per-attestation cost of sync-committee tracking | `lez/results/perf-tracking.md`: every transaction the lab sent to its LEZ v0.4.0-rc1 sequencer over 8.06 days (98 sessions): per update (finality 9,225,372 execution gas median, a handoff 16,060,242), per day (127,294,275 as the lab ran), per period, and per attestation at 1 to 10,000 attestations a day |
| at the recommended operating cadence | the design's cadence (hourly while importing, plus each period's handoff): 235.5M execution gas a day on the public path, *derived* from the measured per-update medians (all 98 measured finality updates within 1.2% of the median, `perf-tracking.md`); the proven path with a real proof of this tree's program: 485,298 + 224,063 gas and 1,673.6 s on 16 CPUs per update (`perf-proofs.md`), 11.6M execution gas and 179 allocated CPU-hours a day for the 24 hourly updates alone; with each period's handoff (451,895 + 224,063 gas, 2,254.2 s on 16 CPUs, 0.879 a day) 12.0M and 187 allocated CPU-hours a day, 12.0M / N and 187 / N per attestation (*derived*: 24 × 485,298 + 0.879 × 451,895; 16 × each proof's wall time) |
| … at the once-per-period cadence (no bridge imports) | 16,060,242 execution gas per period, about 14.1M a day, 14.1M / N per attestation (*derived* from the measured handoff median); on the proven path the period's handoff alone, 451,895 execution and 224,063 storage gas, proven for real (P.2) |
| separately from the per-attestation verification cost | the same table's verification columns: each statement's own transaction (93,316 for a `Transfer` log without a predicate and 166,249 for an account with one condition, its code hash; with P.2's conditions 97,485 and 168,909, three on the account; 275,865 a slot, 499,277 a hop), paid by its consumer whatever the number of attestations |

Commands: `python3 -I bench/perf/tracking_cost.py lez/lab/results/record` (commit `f6937bb2`, `2c969c7f`); the proven
path as P.2. Text: `docs/performance.md` RFP-021 §P.3.

Proposed EVIDENCE row: | new | RFP-021 P.3: tracking cost measured in eight days of operation on a running LEZ
sequencer, per update, day and period, and amortised per attestation apart from each attestation's verification; the
proven path with a real proof | mainnet: 9,225,372 execution gas per finality update (median of 98, all within 1.2%),
16,060,242 per handoff, 127,294,275 a day as the lab ran; 12,729 per attestation at 10,000 a day; verification
93,316–499,277 per statement; proven path 485,298 + 224,063 gas and 1,673.6 s on 16 CPUs per update | the lab's LEZ
v0.4.0-rc1 sequencer (10–18 Nov); a 16-CPU cloud container for the proof | real (the proven path) | `python3 -I
bench/perf/tracking_cost.py lez/lab/results/record` | `lez/results/perf-tracking.md` |

### P.4 — stays **in part**

The owner's step is ready: `bench/perf/p4-laptop-owner.sh` (fixed, `bfd25e52`) was rehearsed end to end as the owner
would run it, from the staged bundle, in a container held to 8 CPUs and 16 GiB: exit 0, attestation 1,110.8 s, claim
1,007.5 s, burn 868.1 s, all settled, 50 minutes (`lez/results/perf-p4-laptop.md` §"Rehearsal", records
`bench/perf/results/p4-owner-rehearsal/`). Measured on this branch: the private attestation of this tree's programs, padded as LEZ's wallet pads it, in cloud
containers held to a mid-range laptop's shape: **1,254.8 s** (1,250.0 and 1,259.6) at 8 CPUs and 16 GiB, **633.5 s** at
16 CPUs, r0vm peak 9.19 GiB, receipt 239,246 B, each settled with the receipt verified and refused with one bit flipped
(`lez/results/perf-p4-laptop.md`, records `bench/perf/results/p4-laptop/`, commit `ef395ac3`; the 8-CPU "medians" of
the claim and the burn are means of one Intel and one AMD run). **What is short:** the clause "on a mid-range laptop".
It needs the owner: an x86-64 Linux laptop with 16 GB or more and GNU time (`/usr/bin/time`), on mains power, idle,
about 50–55 minutes at 8 threads (rehearsal 50 min): from the worktree, `bash bench/perf/p4-laptop-owner.sh ../tmp/perf-p4/stage OUT` (the
bundle, 136 MB: prove_attest, prove_bridge, the fixture, r0vm 3.0.5 and its SHA256SUMS; OUT a new directory outside
it), then `python3 -I bench/perf/summarize_p4.py --owner OUT`.

Proposed EVIDENCE row: | new | The private attestation, claim and burn of this tree's programs, padded as LEZ's wallet
pads every private transaction, proven in containers held to a mid-range laptop's shape | attestation 1,254.8 s at 8
CPUs (2 runs) and 633.5 s at 16; claim 1,093.0 / 639.2 s; burn 932.8 / 534.5 s; r0vm peak 9.19 GiB in every run;
9 of 9 transactions settled with the receipt verified, 9 flipped bits refused | Modal containers held to 8 or 16
CPUs and 16 GiB hard (gVisor; mostly AMD Zen 3) | real | `lez/results/p4-minimum/minimum.py` jobs `attest-pad`,
`bridge-pad`; `python3 -I bench/perf/summarize_p4.py bench/perf/results/p4-laptop` | `lez/results/perf-p4-laptop.md` |

### P.5 — proposed **shown** (was in part; the review agrees)

| clause | shown by |
|---|---|
| end-to-end latency, from the Ethereum event to a verified statement usable on LEZ | `lez/results/perf-latency-e2e.md`, from the live run of 12–13 November (EVIDENCE row 99: real Sepolia, Lodestar's public light-client data, a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain node, one validator, 10-s blocks): every slot of 5.4 hours of continuous operation taken as an event's block, **median 18.1 min**, p90 20.7, max 22.1 |
| broken down by finality wait | 13.0 min median (the finalized header to the signature slot of the update proving it), plus the wait for the next finalized header (3.1 min median) |
| … update availability | 0.2 min median (the beacon node serving it, the daemon's poll) |
| … on-chain verification | the update applied (three transactions, each final on Bedrock: 1.5 min) and the consuming transaction: 0.3 min, a **proxy** (no consuming transaction was sent: the run's median send-to-final time of its 159 update transactions; the statement is verified inside the consuming transaction, P.1); an event between two records' headers needs one EIP-2935 hop, about 223k execution gas more |

Command: `python3 -I bench/perf/latency_e2e.py lez/updater/results/live-sepolia` (commit `eecd5208`).

Proposed EVIDENCE row: | new | RFP-021 P.5's end-to-end latency, from the live run's own records | event to statement
usable on LEZ: median 18.1 min, p10 15.6, p90 20.7, max 22.1 (1,632 slots, 51 updates); finality 13.0 min, the next
finalized header 3.1, availability 0.2, the update on LEZ 1.5, the consuming transaction 0.3 (a proxy: the run's median
send-to-final time); with the run's outages median 19.6, p90 86.7 | the live run of row 99 (one-validator devnet) | — |
`python3 -I bench/perf/latency_e2e.py lez/updater/results/live-sepolia` | `lez/results/perf-latency-e2e.md` |

### P.6 — stays **in part** (the review: in part)

| clause | shown by |
|---|---|
| CPU, RAM, time of every off-chain component the design requires | the updater daemon on a running LEZ v0.4.0-rc1 sequencer, gates G2–G5 (`lez/results/perf-gate-sequencer.md`): **0.043–0.092 of one core** per daemon, peak RSS **57.3–57.8 MiB**; G2's honest daemons 650 and 666 beacon requests, 10.3 and 10.4 MB; sequencer RPC 0.57–2.07 MB up and 7.4–19.6 MB down per daemon (more for the daemons that did the work); updates replayed 80 to 120 times faster than mainnet, so no per-day figure is given; the proven path's prover: 1,673.6 s on 16 CPUs, 9.21 GiB per update (P.2); the lab's zone (`lez/lab/README.md`) |

**What is short:** the `ferrata-attest` CLI and the `ferrata_attest_module` (the consumer's off-chain side,
`docs/design/attestation.md` §8) are not measured; the §8 `monitor` is not built. The three sentences the review found
unsupported ("hardly depend on the work", "the cost is its poll", "a slower poll divides it") are deleted.

Commands: `GATE_* bash bench/perf/gate-sequencer.sh gN` on the build box after `bash bench/perf/gate-build.sh BUNDLE`;
`python3 -I bench/perf/gate_report.py bench/perf/results/gate-g2 … gate-g5`. Records `bench/perf/results/gate-g*/`
(commits `7e854f33`, `8e0fd144`, `225e3b65`, `e1d7630e`); summarizer fixed in `1b12fbc2` (each daemon's beacon traffic
read from its own port). EVIDENCE: in the R.9 row below.

### P.7 — proposed **shown** (was in part; the review's M7 fixed)

| clause | shown by |
|---|---|
| growth rate of module state that accumulates with usage, committee tracking in particular | `lez/results/perf-state.md` (`bash bench/perf/state.sh`): sizes in LEZ v0.4.0-rc1's serialized state, key and `Account` overhead included: a record 453 B (+453.0 B per record measured from 0 to 100,000 records), the instance 593 B constant (committees as summaries), an open job 1,110 B (cleared by `Finish`); the rate in operation: 106 records in 8.06 days on mainnet (`perf-tracking.md`) |
| … failed and abandoned jobs | each leaves its `job` account (1,110 B; clean-up not built): the lab, one operator, 0 of 248 jobs; the gates G3, G4, G5 (two daemons racing on updates replayed 81 to 122 times faster than the lab recorded them) 8, 19 and 3 beside 16, 23 and 8 completed; with k operators racing at the design's cadence, at most (k − 1) × 10.09 MB a year and (k − 1) × 50.43 MB in five years (one abandoned job per extra operator per update, *derived*: `python3 -I bench/perf/projections.py`, `lez/results/perf-projections.md`); none with one operator; the reference consumer's per-claim accounts are the consumer's state, not the module's (`docs/performance.md` RFP-021 §P.7) |
| on-chain storage cost | LEZ v0.4 charges storage gas on a transaction's bytes, not on state: each update's storage gas is in `perf-tracking.md` (4,592 per finality job, 54,252 per handoff); the state itself is disk on every node, sized above |
| projections over one and five years | `lez/results/perf-projections.md` (`python3 -I bench/perf/projections.py`, *derived* from the measured sizes and rate): 2.18 / 10.88 MB as the lab ran, 4.12 / 20.58 MB at the design's cadence, 37.23 / 186.14 MB at one update per epoch |

Commits `2993debd`, `d14ad6c9`, `af7a0923`.

Proposed EVIDENCE row: | new | RFP-021 P.7 and P.9 and RFP-022 P.7's LEZ side: account sizes in LEZ v0.4.0-rc1's
serialized state, growth per record to 100,000 records, five statement types and every update step charged the same
at every size | record 453 B, instance 593 B, job 1,110 B; nullifier 165 B, burn record 273 B, root 205 B; growth exactly
453.0 / 165.00 / 273.00 B per record; five statement types and every update step charged identical execution and
storage gas at 0, 1,000, 10,000 and 100,000 records (asserted) | the build box, in-process harness (LEZ's state
machine) | — | `bash bench/perf/state.sh` | `lez/results/perf-state.md`, `lez/results/perf-projections.md` |

### P.8 — proposed **shown** (was in part; the review agrees; its L12 wording fixed)

| clause | shown by |
|---|---|
| the cycle-count delta between the in-program BLS12-381 path and a hypothetical native precompile | `lez/results/perf-precompile.md` (`python3 -I bench/perf/precompile_model.py bench/perf/results`): modelled at time parity from measured inputs (blst 0.3.16's native times for hash-to-G2, the Miller loops and the final exponentiation on the build box; the executor's own throughput on the same steps, 11.3–14.5 Mcycles/s): BLS 8,344,139 of a finality update's 9,228,905 cycles, 17,841 as a precompile, **delta 8,326,298**; a finality update about 902,607 cycles instead of 9,228,905, a handoff about 845,451 instead of 15,997,954 (lower bounds: `Miller` and the adoption parts counted whole as BLS work); the `Open` and `Finish` phase figures are the bare guest's with the same accelerated blst; the adoption's native time is a median of 50 runs and its executor rate 15.4 Mcycles/s |
| a precompile is reachable only from public transactions, which the comparison must state | stated: the proven path and every private consumer run inside the zkVM proof and would not change |

Commit `7732e416`.

Proposed EVIDENCE row: | new | RFP-021 P.8: the BLS precompile delta, modelled at time parity from measured native
and in-program costs | in-program BLS 8,344,139 cycles per finality update (`Open` 1,454,980, `Miller` 3,308,048,
`Finish` 3,581,111); as a precompile 17,841; delta 8,326,298; update 9,228,905 → about 902,607 cycles (10.2×) |
the build box (blst native, one thread, median of 300; the executor on the same CPU) | — | `python3 -I
bench/perf/precompile_model.py bench/perf/results` | `lez/results/perf-precompile.md` |

### P.9 — stays **in part** (the review: in part)

`lez/results/perf-state.md` §2 and §3: with 0, 1,000, 10,000 and 100,000 records held (the serialized state from 2.8 MB
to 48.1 MB), five statement types (account, slot with a predicate, `Transfer` log, the 306-log receipt's log, a slot
through one hop) and every step of a handoff and of a finality update were charged the same execution and storage gas
at every size (asserted by `perf_state`). The copies sit at keys no measured action reads, so this shows LEZ's metering
does not depend on unrelated state; that no action reads a growing account holds by construction (each reads the
instance, 593 B, its job, a record, a root or a nullifier, each fixed-size and keyed) and in operation: the lab's
`Miller` (3,307,963) and `Finish` (3,723,564) were charged identically in all 98 mainnet finality jobs while 106
records accumulated (`lez/lab/results/record/run.tsv`). **What is short:** not measured at different sizes: `Init`,
the public `VerifyAll`, the private update path, the absent-account, absent-slot and four-facts statements,
`CallAttest`, the reference consumer's claim, `Succeed` and `Spawn`, and the bridge's actions. Next piece: extend
`perf_state`'s K loop to every action and print each action's read set. Command `bash bench/perf/state.sh`; commits
`2993debd`, `d14ad6c9`; wording `9d6c58ef`. EVIDENCE: the P.7 row above.

### R.9 — stays **in part** (the review agrees; figures corrected)

Gates on a running LEZ v0.4.0-rc1 sequencer (`lez/results/perf-gate-sequencer.md`, `bash bench/perf/gate-sequencer.sh`,
records `bench/perf/results/gate-g*/`): G2 two honest daemons (window 8 rounds) and an adversary that never defers,
30 recorded mainnet updates; G3 the two honest daemons alone, window 8; G4 window 1 (no deferral); G5 window 30, 20
updates revealed every 90 s. The replay reveals updates recorded about two hours apart every 60 or 90 s, 80 to 120
times faster than mainnet and faster than two daemons apply one (median latency 105–306 s): jobs overlap by
construction. Rounds took 4.6–6.2 s (G4 17.6 s) against a configured 4-s poll.

| clause | shown by |
|---|---|
| checks whether an update is still needed immediately before submitting | every refused step landed in the same block as the step it raced (G2 26 of 26, G3 9 of 10, G4 35 of 35, G5 1 of 1; the G3 exception an `Open` whose store another daemon's `Finish` changed in that block): no daemon sent a step after another's was final. The daemon reads the instance at the start of each round and the job's account just before sending |
| does not submit one another party has already applied | every step applied exactly once in every run (G2 99 steps, G3 60, G4 113, G5 31) |
| several instances must not multiply the gas spent beyond what contention makes unavoidable | total / completed-job execution gas (abandoned jobs, opened then overtaken, counted with refused steps): **1.095 (G2), 1.222 (G3), 1.721 (G4), 1.081 (G5)**; counting refused steps only, 1.095 / 1.102 / 1.149 / 1.004; jobs completed / abandoned 31 / 0, 16 / 8, 23 / 19, 8 / 3; execution gas per revealed update 11.1M / 8.1M / 16.0M / 4.6M |

**What is short:** A3's gate (the honest daemons' gas on refused steps at most 10% of their gas on applied steps), met
in the in-process chain (EVIDENCE row 80), is met on the sequencer at window 30 (**0.0036**), not at window 8
(**0.102**) or 1 (**0.149**), and not with the adversary (**1.31**). Counting abandoned jobs as waste too, the honest
daemons' waste is 0.081 (G5), 0.222 (G3), 0.721 (G4), 1.311 (G2). Two causes, measured: (1) a duplicate `Open` costs
2.13–2.30M execution gas, because the program refuses it only in `job_open`, after the instance's step 1 has run;
one `Open` whose store changed in the same block reverted at the whole gas limit (4.8M); (2) abandoned jobs, the
larger waste in G4 and G5: a daemon that lost a race opens a job for the next revealed update on the store the
winner's `Finish` is about to change. **Mitigations, not built:** (1) needs a program restructure: `open` runs at the
instance actor, and LEZ v0.4 gives a handler only its receiver's `pre_state`, so `open` cannot see whether the job
account exists; either the job actor becomes the transaction's root (it refuses a non-empty `pre_state`, then asks
the instance to run step 1) or the instance records its open job (an `InstanceState` layout change: deployed
instances would not decode). Either moves the attestation image id `0x816d1fef…` (a new `Message` variant may also
move the bridge, consumer and holdings guests); the pinned consumers to update are `docs/image-ids.md`,
`lez/testnet04/day1.sh`, `e2e/seqnet/drive.sh`, `e2e/seqnet/run.sh`, `e2e/crown/cloud/app.py` and
`lez/harness/tests/residue_handoff.rs`, and existing instances must be re-bootstrapped. A refused duplicate would then
cost its handler's decoding and the check, of the order of `JobOpen`'s 19,050–26,464 cycles (*estimated*, not
measured after a restructure). It is planned with the RISC Zero 3.1.0 rebuild, which moves the image ids anyway. (2)
needs no image change: an updater policy that does not open a job on a store another party's open job will change
(join it or wait). A lower gas limit for `Open` would cap the same-block revert. Against a daemon that never defers,
the honest daemons' ratio stays high whatever their window while a duplicate costs 2.2M.

Proposed EVIDENCE row: | new | RFP-021 R.9, R.10 and P.6 on a running LEZ sequencer: two honest `ferrata-updater`
daemons with an adversary (window 8) and alone (windows 1, 8, 30), following recorded mainnet updates replayed 80 to
120 times faster than mainnet | every step applied once in every run; refused steps all in the same block as the step
they raced; total / completed-job gas 1.095 / 1.721 / 1.222 / 1.081 (G2 adversary / G4 window 1 / G3 window 8 / G5
window 30; refused steps only 1.095 / 1.149 / 1.102 / 1.004); jobs abandoned 0 / 19 / 8 / 3; A3's ratio honest
refused / applied 1.31 / 0.149 / 0.102 / 0.0036 (gate 0.10); honest-1's fee share 0.588 / 0.493 / 0.563 / 0.609;
latency median 35 / 105 / 126 / 306 s, p90 45 / 106 / 176 / 576 s; each daemon 0.043–0.092 of a core, 57.3–57.8 MiB
peak | the build box: LEZ v0.4.0-rc1 sequencer and Logos Blockchain node (LEZ's local configuration) | — | `GATE_* bash
bench/perf/gate-sequencer.sh gN`; `python3 -I bench/perf/gate_report.py bench/perf/results/gate-g2 …` |
`lez/results/perf-gate-sequencer.md` |

### R.10 — stays **in part** (the review: in part)

| clause | shown by |
|---|---|
| submission timing must not systematically concentrate cost on whichever party reacts fastest; spread across the operators over many updates | honest-1's share of the honest daemons' fees on a sequencer: 0.563 (G3, 16 completed jobs), 0.493 (G4, 23), 0.609 (G5, 8), 0.588 (G2, with the adversary, 31); in the in-process chain 0.39–0.47 over 58 updates (EVIDENCE row 80) |
| the component defers by an interval it varies per instance | each (job, step) waits `H(key, job, step) mod W` rounds, `key` the daemon's own (`lez/updater/src/updater.rs`, `defer`); documented in the operator guide §3.4 (final for that section) |
| still submitting promptly once the deferral elapses and no other party has acted | every revealed finality update covered (the instance at least as fresh, final on Bedrock): G3 12 by its own job and 9 by a later update's, G4 20 / 1, G5 5 / 12, G2 26 / 0; median 126 s at window 8 (G3) |
| document the deferral strategy and its effect on update latency | operator guide §3.4's measured table (window, rounds actually run, reveal step, latency, coverage, gas): window 1 (rounds 17.6 s) median 105 s, p90 106; window 8 (5.9–6.2 s) 126 / 176 s; window 30 (4.6–4.8 s) 306 / 576 s; execution gas per revealed update 16.0M → 8.1M → 4.6M; total / completed-job gas 1.721 → 1.222 → 1.081 |

**What is short:** the runs hold 11 to 42 jobs (8 to 31 completed), under A3's gate of 100 updates
(`docs/traceability.md` R.10); G5's 90-s reveal step (G3 and G4: 60 s) confounds the window comparison; windows in
seconds are nominal (rounds ran 4.6–17.6 s). Next piece: one 100-update run per window, reveals at one step.
Commits `8e0fd144`, `225e3b65`, `e1d7630e` (runs), `1b12fbc2` (summarizer), `9d6c58ef` (guide §3.4). Same EVIDENCE row
as R.9.

## RFP-022

### P.2 — proposed **shown** (was in part; the review's M2 fixed)

| clause | shown by |
|---|---|
| the compute-unit cost of any additional LEZ-side verification beyond the per-claim path | the root import: `lez/results/perf-state.md` §4a (this tree's bridge program, in-process harness): 230,731 execution gas covering 5 leaves, 236,224 covering 8, 236,202 for another vault's 8: flat in the deposits covered; on Sepolia's real state in the live round trip 432,182 (`e2e/runs/crown-live-3`); under a mainnet state root *estimated* at about 450k (P.1's account statement: 167,882 under the mainnet record, 148,948 under Sepolia's without its header) |
| its amortised per-deposit cost at the recommended operating cadence | `docs/performance.md` RFP-022 §P.2 (*derived* from the measured figures): import only, 432,182 / N per deposit at N deposits an hour (432,182, 43,218, 4,322 at 1, 10, 100); with the instance's tracking the cadence runs for the bridge (the hourly light-client update and each period's handoff), when the bridge is the instance's only consumer, (235.5M / 24 + 432,182) / N: **10.25M, 1.02M, 102k** (the handoffs 0.59M of each hour's 9.81M); on the proven path (485,298 + 0.879 × 451,895 / 24 + 432,182) / N: 934,029, 93,403, 9,340, plus about 232,268 storage gas an hour (224,063 per update, the handoffs' share 8,205). Per newly covered deposit in the harness (*derived*, a division): 46,146 (5), 78,741 (3), 29,525 (8) |

Command `bash bench/perf/state.sh`; commits `d14ad6c9`, `9d6c58ef` (text).

Proposed EVIDENCE row: | new | RFP-022 P.2: a root import's cost against the deposits it covers, and per deposit at
the hourly cadence | 230,731 / 236,224 / 236,202 execution gas for imports newly covering 5 / 3 / 8 deposits; 432,182
on Sepolia's real state; per deposit at the hourly cadence with the instance's tracking (the hourly update and the period's
handoffs; bridge as the only consumer) 10.25M / 1.02M / 102k at 1 / 10 / 100 deposits an hour, on the proven path
934,029 / 93,403 / 9,340, import only 432,182 / 43,218 / 4,322 (derived) | the build box,
in-process harness; the live round trip of row 91 | — | `bash bench/perf/state.sh` | `lez/results/perf-state.md` §4a,
`docs/performance.md` RFP-022 §P.2 |

### P.4 — stays **in part**

The private claim and burn of this tree's bridge program, padded, at a laptop's shape: claim **1,093.0 s** (941.1 and
1,245.0) at 8 CPUs and 16 GiB, 639.2 s at 16; burn **932.8 s** (820.9 and 1,044.7), 534.5 s at 16; r0vm peak 9.19 GiB
(`lez/results/perf-p4-laptop.md`). Short: the real laptop (the owner's step, as RFP-021 P.4). EVIDENCE: RFP-021 P.4's
row above.

### P.5 — proposed **shown** (was in part; the review's M8 fixed)

| clause | shown by |
|---|---|
| deposit latency (deposit to LEZ mint) | `lez/results/perf-latency-bridge.md` (`python3 -I bench/perf/latency_bridge.py e2e/runs/crown-live-3`): the live round trip on Sepolia with testnet parameters (EVIDENCE row 91), from the run's own records: deposit 1 to its private claim applied **32.9 min** as it ran (29.8 min without the runner's other work) |
| redemption latency (burn to Ethereum release) | the same run: the private burn's proof started to the release in a Sepolia block **71.0 min** (66.3 from its sending, 66.2 from it final on LEZ; W = 1 h, release delay 120 s) |
| each broken down by source-chain finality wait, proof generation, any privacy-motivated delay, and on-chain verification | inbound: Ethereum finality 18.6 min (*derived*: two epochs after the checkpoint's slot), the update served and the capture 6.6 min, LEZ steps 39.6 s, the claim's proof 231.9 s (the crown runner's CPU; 1,093 s at a laptop's shape), verification 8.3 s; outbound: the burn's proof 282.8 s, the source chain's finality (the burn sent to final on LEZ, a one-validator devnet) 7.0 s, the checkpoint and wrap 167 s, verification 37 s, W 3,607 s, release delay and transaction 156 s; privacy-motivated delay: none in this run, the client's default holds a claim 1–12 h from its deposit's block, inside which Ethereum's finality and the capture (25.2 min) elapse (inbound about 1.1–12.1 h, *derived*) and a release waits for the vault's release delay |

Commits `aa2204ca`, `175dda9f`. Caveat: LEZ ran on a devnet (one validator; testnet 0.4 is not released); production
parameters (W, the 24-h release delay) are stated, not run.

Proposed EVIDENCE row: | new | RFP-022 P.5: deposit to mint and burn to release on Sepolia with testnet parameters,
from the live round trip's records | inbound 32.9 min (finality 18.6, availability and capture 6.6, LEZ steps 0.7,
proof 3.9, verification 0.1, the runner's other work 3.0); outbound 71.0 min from the burn's proving (burn proof 4.7,
LEZ finality 0.1, checkpoint and wrap 2.8, W 60.1, release 2.6, verification 0.6); the client's default claim hold of
1–12 h, counted from the deposit's block, would make the inbound about 1.1–12.1 h (derived) | the live round trip of row 91 | real (its proofs) |
`python3 -I bench/perf/latency_bridge.py e2e/runs/crown-live-3` | `lez/results/perf-latency-bridge.md` |

### P.6 — proposed **in part** (narrower gap than before)

Measured now: `monitor` and `ferrata-relayer run` on a local anvil with a rule (B) FerrataVault v5, idle and through
a candidate's check and finalisation (`lez/results/perf-daemons.md`, `bash bench/perf/daemons-anvil.sh OUT 10`, records
`bench/perf/results/daemons-anvil/`, commit `7e854f33`): `monitor` 0.53 CPU-seconds in 1,284 s, peak 14.0 MiB, 86 KB
up / 254 KB down; the relayer 0.87 CPU-seconds, peak 20.3 MiB, 408 KB / 976 KB; the updater whose records the
imports need (RFP-021 P.6); the provers (existing). **Short:** the importer's LEZ-side work (the relayer's root
imports, a prototype) was not measured as a daemon (the relayer ran with `importRoots: false`); `ferrata_attestor`
(rule (B): an attestor beside every sequencer) was not measured; `monitor`'s LEZ view came from a file
(`--node file:`), so its traffic to a LEZ node is not measured. `bench/perf/results/daemons-anvil/anvil.log` prints
anvil's built-in test mnemonic and keys (public, the same in every anvil).

Proposed EVIDENCE row: | new | RFP-022 P.6: the Ethereum-side daemons' CPU, memory and bandwidth | `monitor` 0.53
CPU-s in 1,284 s (0.0004 of a core), 14.0 MiB peak, 86,265 / 254,218 B; relayer 0.87 CPU-s (0.0007), 20.3 MiB,
408,005 / 975,682 B; the candidate checked and finalised | the build box, local anvil (12-s blocks) | — | `bash
bench/perf/daemons-anvil.sh OUT 10`; `python3 -I bench/perf/summarize_daemons.py bench/perf/results/daemons-anvil` |
`lez/results/perf-daemons.md` |

### P.7 — proposed **shown** (was in part; the review's L14 fixed)

| clause | shown by |
|---|---|
| growth rate of all bridge state that accumulates with usage | LEZ: a nullifier 165 B and a burn record 273 B in the serialized state (+165.00 and +273.00 B per record measured to 100,000), a root 205 B per import (`perf-state.md` §4, §5); Ethereum: one vault slot per release, one per new depositor (plus at most 32 frontier slots ever), about ten per checkpoint (`lez/results/perf-state-ethereum.txt`, `vault/test/StateGrowth.t.sol`, Foundry's state-diff recording) |
| on-chain storage cost | Ethereum: each new slot is paid inside the deposit and release gas of P.3 (20,000 + cold access on mainnet's rules; 97,920 state gas on Sepolia's Glamsterdam rules); LEZ: storage gas on transaction bytes, not on state |
| projected at 1M and 10M operations | `lez/results/perf-projections.md`: nullifiers 165 MB / 1.65 GB, burn records 273 MB / 2.73 GB, Ethereum release slots 1,000,000 / 10,000,000 (64 / 640 MB of keys and values), deposit slots one per new depositor; LEZ holder accounts 162 B per new holder, a claim creating at most two that are not one-time (its destination's and its relayer's; the first claim's +486 B is three, with the fee recipient's, created once): 162 MB / 1.62 GB with relayers already paid, at most 324 MB / 3.24 GB (*derived*, `perf-projections.md`, `docs/performance.md` RFP-022 §P.7) |

Commits `2993debd`, `7732e416`, `af7a0923`. EVIDENCE: RFP-021 P.7's row above, and | new | the vault's storage
growth per operation (Foundry state diffs) | release 1 slot, a new depositor's first deposit 1 (28 for 25 with the
frontier's new levels), a repeat deposit none, a checkpoint 15 (first) then 10 | the build box, Foundry 1.3.6 |
— | `bash bench/perf/vault-growth.sh` | `lez/results/perf-state-ethereum.txt` |
