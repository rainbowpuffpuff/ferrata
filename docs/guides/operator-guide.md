# Operating Ferrata: the updater, the importer, the checkpoint prover and relayer, watchers

**Operator guide (RFP-021 U.3, S.5, S.7, R.9, R.10, P.6; RFP-022 U.3, R.6, R.9, R.13, P.6). First
draft, 11 November 2026; §3.4's deferral strategy and its measured effect on a sequencer: final, 21 November 2026.** For anyone who keeps Ferrata live: advancing an attestation instance,
importing the vault's deposit roots, proving and finalising LEZ checkpoints for the vault, relaying
users' transactions, and watching for wrong candidates. It says what each role needs, what it costs
(cycles, gas on Foundry's rules and on Sepolia, proof times on CPU and GPU), how concurrent operators
interact, how to detect a stall, what fails and how to recover, and how a configuration is retired.
Every statement cites its source; paths are relative to the repository root.

**No party is obliged to operate.** Liveness rests on interested parties choosing to spend the gas:
protocols whose users need fresh records, ecosystem operators, and the author in the post-delivery
year. Every role below is permissionless except the watchers', and none can make the programs or the
vault accept something false ([`docs/design/attestation.md`](../design/attestation.md) §9, §11.5;
[`docs/design/bridge.md`](../design/bridge.md) §14).

**Status.** Three daemons are built as prototypes: `monitor` (§7), a standalone program
([`monitor/`](../../monitor/README.md)) and a Logos module; the relayer, the binary `ferrata-relayer` (§6: rule (B) checkpoints,
releases, wrapping, fee collection and the gas drop; tested on an anvil fork of Sepolia with real proofs and
in the LEZ harness); and the **updater**, `ferrata-updater` ([`lez/updater/`](../../lez/updater/README.md), §3.6),
tested against LEZ v0.4.0-rc1's state machine in process on the lab's recorded mainnet and Sepolia data, also as a
Logos module on that chain. The relayer's root imports (§4, §6.1) and its Logos module (`ferrata_relayer_module`, the
same daemon code) are built as prototypes, shown on the mini-app's devnet and the Sepolia fork, the module's LEZ side the
devnet's zone (§6.6); the updater's backend for a
running sequencer, a binary of its own (`lez/updater/sequencer/`), ran 8 h on a LEZ devnet following Sepolia live
(`lez/updater/results/live-sepolia/`). All three daemons are designed
as Logos modules run under `logosctl daemon`, with structured logs, clean shutdown and configurable endpoints and depths
([`attestation.md`](../design/attestation.md) §8; [`bridge.md`](../design/bridge.md) §1). What runs
today, and what this guide describes from measurement, is the **lab** (§2), the harness and live
drivers, the checkpoint prover's host, and the end-to-end scripts.

## 1. The roles at a glance

| Role | Does | Needs | Trusted for | Section |
|---|---|---|---|---|
| Updater | submits Ethereum light-client updates to an attestation instance | a beacon (consensus) endpoint, LEZ gas; no standing on Ethereum | nothing: updates stand or fall on their signatures | §3 |
| Root importer | sends `ImportRoot` for a configuration on a fixed cadence | an execution endpoint with `eth_getProof`, LEZ gas | nothing: the attestation proves the vault's slots | §4 |
| Checkpoint prover | proves LEZ blocks since the last final checkpoint; submits and finalises on the vault | a LEZ node's blocks, a RISC Zero prover (GPU advised), an x86 machine for the Groth16 wrap, Ethereum gas | nothing: the vault verifies the proof; a prover cannot leave out a burn | §5 |
| Relayer and wrapper | submits users' claims and releases; wraps succinct release receipts | an x86 machine for the wrap, gas on both chains | nothing: proofs bind destination, recipient and fee | §6 |
| Watcher (`monitor`) | cancels a pending candidate whose tip is not final in its own nodes; pauses checkpoints ≤ 12 h | its own Logos Blockchain, LEZ and Ethereum nodes; a hot key set by the admin | liveness under rule (B); safety under rule (A) outside its conditions | §7 |
| Rule (B) attestor | beside each sequencer, signs the node's final tip with a certified key | the sequencer's node | it is one of the `t` keys rule (B) counts | §5.4 |

Sources: [`attestation.md`](../design/attestation.md) §8–§11; [`bridge.md`](../design/bridge.md) §8–§10, §14.

## 2. The reference deployment: the lab

The lab ([`lez/lab/README.md`](../../lez/lab/README.md)) keeps LEZ v0.4.0-rc1's own
sequencer chain, with its Logos Blockchain node, alive on a cloud container, and two instances of the
light-client program on it, one following Sepolia (through Gloas) and one following mainnet, fed over
the public job path:

| | |
|---|---|
| schedule | every two hours (cron `17 */2 * * *`, UTC), at most one session at a time; each session restores the zone's last good state, starts node and sequencer, fetches the newest light-client data, submits it to both instances, waits until every transaction is final on Bedrock, stops, saves a new state generation, writes the record |
| zone | `sequencer_service` at `v0.4.0-rc1` unmodified, 10-s blocks; one validator, leader proofs in dev mode, as LEZ's container sets it: a local zone, not a network |
| program | deployed once through `program_loader` (7 transactions), immutable |
| driver | LEZ's wallet sends every transaction; the follower applies only blocks `Finalized` on Bedrock and reads each charge with LEZ's own settlement; every job step declares a gas limit of 4,800,000 (largest step measured 3,731,712) |
| reads | public Lodestar beacon APIs for bootstraps and updates (the only public ones found serving light-client data), a second provider to confirm roots; a generic User-Agent, no key |
| keys | LEZ's published test accounts pay; throwaway by design; nothing is signed for Ethereum |
| operate | `LAB_PROFILE=<your Modal profile> lez/lab/lab.sh deploy | run 60 | status | fetch | verify | stop` |
| re-check | `lab.sh verify` replays every block through LEZ's `chain_state::apply_block` from the pre-genesis state, compares every charge, and rebuilds each instance's store natively from the kept bootstrap and updates |

**Measured** (to the session of 11 November 2026, 14:17 UTC): 257 transactions, all applied, none
refused, reverted, dropped or rejected; 258 blocks, all final; 12 sessions, 1 failed to start; Sepolia from
the period-1378 bootstrap through the Gloas fork, 44 updates and 32 committee handoffs (one live, into period
1,410); mainnet from the finalized checkpoint of slot 15,625,920, 14 updates and 1 committee handoff (live,
into period 1,908). A scheduled session restarted the zone after 54 minutes stopped. Program transactions
used 208,951 (`Init`) to 3,733,713 (`Finish`) execution gas, none over the 5M target. **Cost:** the scheduled session of 20:17 UTC billed 0.0064 USD; at twelve such
sessions a day, about 0.08 USD a day (derived). Seven consecutive days end on 17 November, 19:22 UTC
([`lez/lab/README.md`](../../lez/lab/README.md), "Results so far", "The claim that can be made today").

**Not in the lab**: the proven path, the bridge, a multi-validator network.

## 3. The updater (RFP-021 U.3, S.7)

### 3.1 What it does

Follow Ethereum, detect when the instance is behind, submit the outstanding updates, defer to avoid
duplicates, and resume from on-chain state ([`attestation.md`](../design/attestation.md) §8). An instance
needs **at least one update per sync-committee period** (8,192 slots of 12 s, 27.3 h; 321 a year) to
learn each next committee; the recommended cadence adds **one per hour while roots are imported** for
the bridge ([`attestation.md`](../design/attestation.md) §10).

### 3.2 Two paths

| Path | Transactions | Execution gas | Storage gas | Who can use it |
|---|---:|---:|---:|---|
| Public job, finality update (`Open`, `Miller`, `Finish`) | 3 | 9,216,121 (largest 3,719,489) | 4,451 | anyone, no prover |
| Public job, update that rotates and adopts a committee | 5 | 16,076,120 (largest 3,730,145) | 54,491 | anyone |
| Proven (`VerifyAll` privately, then `ApplyVerified`) | 1 | 479,854 | 224,063 | anyone who can prove 9.0M cycles |
| Proven handoff with no new header (dev mode) | 1 | 448,530 | 224,063 | — |

Measured in the in-process harness on mainnet periods 1899–1903 ([`attestation.md`](../design/attestation.md)
§4.2). The largest step is 75% of LEZ's 5M base-fee target; at most four proven (private) transactions
fit in a block. **Proof of an update** (9.0M user cycles): 3,240.5 s on a 16-thread laptop CPU, peak
9.18 GiB; 895.6 s on a cloud container's CPU with 32 prover threads, verified and applied by a live
sequencer; 49.5 s on one NVIDIA L4 ([`attestation.md`](../design/attestation.md) §4.2;
[`lez/RESULTS.md`](../../lez/RESULTS.md) §9). Proving one update per hour on the laptop would take
about 54 of every 60 minutes (derived).

### 3.3 Daily and yearly cost (derived)

From [`attestation.md`](../design/attestation.md) §10: once per period on the public path, 16,076,120
execution gas per period, about 5.16 billion a year; once per hour while roots are imported, 221,186,904
execution and 106,824 storage gas per day on the public path, or 11,516,496 and 5,377,512 on the proven
path. LEZ gas has no market price yet (no public v0.4 network), so these are not converted to money. The
lab's whole zone, sequencer and node included, ran on a cloud container for about 0.08 USD a day at its
two-hour cadence (§2).

### 3.4 Concurrency and deferral (R.9, R.10)

- **Anyone can start one, without permission.** No step checks who sent it; an account with no prior
  relationship to the instance advanced it in the harness ([`attestation.md`](../design/attestation.md) §4.5).
- **What another operator's activity does to you.** A job's id is the hash of the instance, its starting
  store and the update, so a job can finish only on the store it began on. A duplicate first step is
  refused once the job is stored, but only after step 1 has run: **2.2M** cycles of gas wasted. `Open` of
  an already applied update is refused at 207,687; `Finish` after the job applied at 5,731 (6,111 on
  the sequencer, 9 of 9, [`perf-gate-sequencer.md`](../../lez/results/perf-gate-sequencer.md))
  ([`attestation.md`](../design/attestation.md) §4.4, codes as renumbered: 207 `JOB_EXISTS`, 52
  `NotNewer`, 205 `NO_JOB`).
- **Deferral.** Each daemon waits a random share of a window `W`, re-checks the chain, then sends. The
  design note gives the duplicate probability as `1 − (1 − Δ/W)^n` for block interval Δ and `n` daemons
  ([`attestation.md`](../design/attestation.md) §9). With LEZ's 10-s blocks ([`lez/lab/README.md`](../../lez/lab/README.md)):

  | `W` | mean added latency (`W/2`) | `n` = 2 | 3 | 5 | 10 |
  |---:|---:|---:|---:|---:|---:|
  | 60 s | 30 s | 0.306 | 0.421 | 0.598 | 0.838 |
  | 120 s | 60 s | 0.160 | 0.230 | 0.353 | 0.581 |
  | 300 s | 150 s | 0.066 | 0.097 | 0.156 | 0.288 |
  | 600 s | 300 s | 0.033 | 0.049 | 0.081 | 0.155 |
  | 1,800 s | 900 s | 0.011 | 0.017 | 0.027 | 0.054 |

  Derived by arithmetic from that formula. A duplicate costs the 2.2M step above.
  `ferrata-updater` defers per step rather than per update: each (job, step) waits
  `H(key, job, step) mod W` rounds, `key` the daemon's own (so the wait differs per daemon and per step), reads the
  job's account just before sending, and any updater continues any open job.
- **The strategy's effect, measured on a running LEZ v0.4.0-rc1 sequencer** (21 November 2026;
  [`perf-gate-sequencer.md`](../../lez/results/perf-gate-sequencer.md), `bash bench/perf/gate-sequencer.sh`): two honest
  daemons following recorded mainnet updates, configured to poll every 4 s, the window `W` in rounds. Latency: a
  revealed finality update to the first `Finish`, final on Bedrock, that leaves the instance at least as fresh. Gas:
  all execution gas, abandoned jobs (opened, then overtaken by another job) and refused steps included.

  | run | `W` (rounds; measured s per round) | reveal step | latency median / p90 | updates covered: own job / a later one | execution gas per revealed update | total / completed-job gas | honest waste: refused and abandoned / completed | A3's ratio: refused / applied | honest-1's fee share |
  |---|---|---:|---|---|---:|---:|---:|---:|---:|
  | G4 | 1 (17.6 s) | 60 s | 105 / 106 s | 20 / 1 | 16.0M | 1.721 | 0.721 | 0.149 | 0.493 |
  | G3 | 8 (5.9–6.2 s) | 60 s | 126 / 176 s | 12 / 9 | 8.1M | 1.222 | 0.222 | 0.102 | 0.563 |
  | G5 | 30 (4.6–4.8 s) | 90 s | 306 / 576 s | 5 / 12 | 4.6M | 1.081 | 0.081 | 0.0036 | 0.609 |
  | G2: G3 with a daemon that never defers | 8 (4.8–5.0 s) | 60 s | 35 / 45 s | 26 / 0 | 11.1M | 1.095 | 1.311 | 1.311 | 0.588 |

  A longer window costs latency and saves gas: fewer races, and intermediate updates skipped (a later update's job
  covers them). Caveats: the replay reveals updates 81 to 122 times faster than the lab recorded them (one about every
  two hours, its session schedule), about 40–60 times the design's hourly cadence, so jobs overlap more than in
  operation; G5's 90-s reveal step against 60 s confounds its comparison with G3 and G4;
  the runs hold 11 to 42 jobs, under A3's planned 100 updates. Most of the waste at window 1 (G4: 111.1M abandoned against
  50.0M refused) is abandoned jobs, at window 8 (G3) about half (17.4M against 18.0M): a daemon that lost a race opens a job for the next update on the store the winner is about to change. A policy of
  not opening a job on a store another party's open job will change (join or wait) would remove it; it is not built.
- **The acceptance run (measured, dev mode).** Two honest `ferrata-updater`s and one adversarial
  updater, on the lab's 58 recorded updates (14 mainnet, 44 Sepolia; not the 100 planned: that is all
  the recorded data), each round a block settled in a shuffled order, `W` = 8 rounds: every update
  applied once; the honest updaters' gas on refused steps was **0.057–0.087** of their gas on applied
  steps (gate ≤ 0.10) and their shares of the honest fees **0.39–0.47** and **0.53–0.61** (gate
  0.35–0.65 each), in seven runs (four orders of the blocks' transactions, three other draws of the
  deferral waits); the adversary raced a third of the due steps and sent 110 junk `Open`s at its own
  cost; without it, 0.0284 and 0.48 / 0.52 (12 November, and the same on the merged tree of 13 November)
  ([`lez/updater/README.md`](../../lez/updater/README.md), `lez/updater/results/gate.txt`).
- **Joining another party's open job** is built in `ferrata-updater` (it reads the job's phase on
  chain and sends its next step). **Not built**: clean-up of jobs whose final step fails (they stay,
  unapplied) ([`attestation.md`](../design/attestation.md) §4.5).

### 3.5 Detecting a stalled instance

Compare the instance's finalized slot and root with the chain's: the lab's `lab.sh status` puts both
instances against the chains' clocks, and each session's fetch record holds the root a second provider
reports for the newest finalized slot, which must equal the instance's `finalized_root`
([`lez/lab/README.md`](../../lez/lab/README.md), "Re-check"). In the design the attestation side's `monitor` (not the watchers' `monitor` of §7) compares records with several providers'
canonical roots and alarms at half the weak-subjectivity bound (**not built**,
[`attestation.md`](../design/attestation.md) §3.6, §8). An instance lags by the cadence plus finality;
it catches up when anyone sends the missing updates, as long as its store knows the signing committee.

### 3.6 Running `ferrata-updater`

One TOML file ([`lez/updater/ferrata-updater.example.toml`](../../lez/updater/ferrata-updater.example.toml))
sets the daemon (state directory, poll interval, `/health` and `/metrics` address, ledger), the payer
key file, the fee limits (`gas_limit` per step, default 4,800,000; `max_fee_per_tx`; a `budget` at which
it stops sending), the policy (deferral window, lag threshold, how many sources must agree, backoff,
demotion) and, per instance, its preset, checkpoint and beacon sources. Per round it reads the instance
from the chain, checks every candidate from every source with the program's own steps natively before
paying for anything (a source serving what the program would refuse is demoted; one far behind is
flagged lagging; one silent is flagged unavailable), and sends the next step of the best candidate's
job. It keeps only the full sync committees (accepted when they hash to the summaries on chain) and a
journal; a restart, in the middle of a job or of a period, reads the chain and sends the job's next
step. SIGTERM finishes the round and exits. `/health` is `ok`, `degraded` (with the reasons) or `down`
(HTTP 503); `/metrics` counts transactions by step and result, gas and fees applied and refused,
retries, deferrals, steps done by others, and per source requests, errors, refusals, lag and
availability. Tested in dev mode (RISC0_DEV_MODE, the in-process chain, recorded data): catch-up to the
lab's final roots on both chains, a lying source, a stale and a down source, `min_agreeing = 2`, a
restart mid-job and mid-period with dropped connections, and the acceptance run of §3.4
([`lez/updater/README.md`](../../lez/updater/README.md)). On a running LEZ sequencer: the build of `lez/updater/sequencer/` (`daemon.ledger = "sequencer"`, a
`[sequencer]` section with the RPC address and the program's account) kept the crown run's Sepolia instance
following Sepolia for 8 h on two LEZ v0.4.0-rc1 devnets in turn, from Lodestar's light-client data with
publicnode's headers as the second source (`kind = "headers"`, `min_agreeing = 2`): 74 updates applied (56 on
the chains kept), restarts mid-job and of the whole zone
([`lez/updater/results/live-sepolia/`](../../lez/updater/results/live-sepolia/)). The Logos
module `ferrata_updater_module` ([`modules/`](../../modules/README.md)) drives the same daemon with either backend: the
in-process chain, or a running sequencer through `lez/updater/sequencer/`'s library, with the same configuration
(`daemon.ledger = "sequencer"`, `[sequencer]`); `start`, `status`, `metrics` and `stop` through Logos Core, `/health`
and `/metrics` on `daemon.listen` as the binary's. It ran inside a headless Logos Core on a crown zone following
Sepolia, with a restart mid-job through `stop` and `start` (EVIDENCE row 104,
[`modules/results/updater-sequencer-2026-11-13/`](../../modules/results/updater-sequencer-2026-11-13/README.md)). The
module's host process needs `curl` on its `PATH` (the beacon sources) and `libpcsclite` (LEZ's wallet links it). Not
built: the proven path.

## 4. The root importer (RFP-022 P.2)

`ImportRoot` attests the vault's slots 0 and 1 under one finalized record and writes `root(cid, n)`
once per leaf count. Run it on a fixed cadence (the design ties it to the hourly update), never in
response to a particular deposit ([`bridge.md`](../design/bridge.md) §5.4).

| Measured | Execution gas | Storage gas |
|---|---:|---:|
| `ImportRoot`, 8 leaves, synthetic vault storage (in-process harness, this tree's program) | 236,224 | 2,025 |
| on the v5 vault's own storage, 3 and 6 leaves | 282,103; 282,881 | — |
| under a Gloas record, with the block's RLP header (2 leaves) | 266,879 | 2,612 |
| on a live LEZ sequencer, stand-in beacon chain | 232,278 | 2,460 |
| on a live LEZ sequencer, Sepolia's real state: the live vault, one leaf (the frozen program; [`lez/live/inbound/README.md`](../../lez/live/inbound/README.md)) | 426,479 | 6,646 |

Sources: [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §1, §2a, §8a;
[`lez/live/inbound/README.md`](../../lez/live/inbound/README.md). Under a mainnet state root an account
and a slot proof cost about 275k cycles in the attestation alone (Stage 4), so a real import should cost
roughly that plus one more slot proof (estimated, [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §1).
**Amortised per deposit**: one import serves every deposit under its root, so the cost per deposit is the
import's cost divided by the deposits it newly covers (the measurement at the recommended cadence is a
milestone B4 deliverable). Failure: an import at a count already imported is refused (244, after the
attestation: 233,436 gas wasted); an import after `Sunset` is refused before it (242).

**The relayer imports** (`ferrata-relayer --import-roots`, §6.1 step 0; prototype). In the first round of each
`--import-every` period of Ethereum time (default 3,600 s: one import at most per period, whenever the deposits came)
it reads the configuration's newest record on LEZ (`instance(id)`'s finalized slot, then `header(id, slot)`), checks
that its Ethereum endpoint's block at the record's number has the recorded hash, reads the vault's slots 0 and 1 there
with `eth_getProof`, and, when the count is above the newest imported one, builds `ImportRoot` and dry-runs it with
the attestation and bridge programs' own functions (`ferrata_wallet::import`, the code `ferrata_bridge_module`'s
`import_root` runs) before sending it: an import they would refuse is not sent, so no gas is spent on it; one sent is
confirmed from `root(cid, n)`. A Gloas record (a block hash only) is waited on: the relayer does not fetch the
block's header yet. On the mini-app's devnet (anvil's own `eth_getProof`, the light client's record written by the
devnet) an import of 2 leaves cost 273,511–275,460 execution gas and 2,677–2,709 storage gas (§6.6; not yet run on a
LEZ sequencer).

## 5. The checkpoint prover (the return leg)

### 5.1 What it proves

Starting from the vault's `finalState`, the checkpoint program re-hashes every LEZ block since, collects
every non-reverted burn of the configuration into the burn tree, follows the channel's key-set changes,
and shows that `t` keys stand behind the tip, by rule (B) or (A) ([`bridge.md`](../design/bridge.md) §8).
It cannot leave out a burn: every block's id and link is checked.

**Keep the blocks you proved.** LEZ signs transactions with fresh randomness, so a prover must keep the
blocks it proved a candidate for: the finalisation (rule (A)) has to prove the same ones
([`bridge.md`](../design/bridge.md) §18.7).

### 5.2 Cycles and proof times

Executor (r0vm 3.0.5; user cycles are deterministic), `n = 4`, `t = 3`, 360 blocks = one hour at 10 s
([`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md) headline, §3; [`gpu-prover/RESULTS.md`](../../gpu-prover/RESULTS.md)):

| Input (rule (A) candidate) | User cycles | Segments | One NVIDIA L4 | Laptop CPU (16 threads) |
|---|---:|---:|---:|---:|
| one empty `T` + 3 support blocks (fixed part) | 3,705,001 | 4 | 14.0 s | 1,157.5 s quiet; 1,758.9 s loaded (measured) |
| idle hour | 5,992,503 | 7 | 24.1 s | ≈ 2,023 s (extrapolated, 289 s per segment) |
| hour with 5 private transactions | 8,821,992 | 12 | 41.4 s | ≈ 3,473 s (extrapolated) |
| hour with 50 private transactions | 37,865,667 | 60 | 219.3 s | ≈ 4.8 h (extrapolated) |
| `T` and 3 support blocks with 4 private transactions each | 224,848,717 | 232 | — | — |

- Marginal costs: 6,405 cycles per empty block; 2,684 cycles per KB of block data (one ~237 KB private
  transaction outside `T` and the support blocks: 637k cycles).
- One L4 proves an hour of 50 private transactions about 16 times faster than the hour lasts; GPU memory
  peaks at about 10.9 GiB whatever the segment count; host memory 0.76–1.08 GiB, against 9.62 GB for the
  CPU proof ([`gpu-prover/RESULTS.md`](../../gpu-prover/RESULTS.md), "CPU versus GPU").
- **Rule (B)** checks 1.69M cycles per attestor (two Ed25519 `verify_strict`), 5.1M at `t` = 3, 11.8M at
  `t` = 7, whatever the blocks hold; one proof per checkpoint. Idle hour at `n` = 4: 7.51M against rule
  (A)'s 5.98M, but (A) proves twice (derived per checkpoint: 20.1M against 7.5M). One real rule (B) proof:
  the idle hour, 9 segments, 31.9 s on one L4 ([`bridge.md`](../design/bridge.md) §18.5;
  [`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md) §4).
- **Groth16 wrap** (x86 only): 195.1–215.2 s on the laptop, Groth16 prover 7.3 GiB, seal 260 bytes; in the
  end-to-end run, recursion 18.7 s on 16 cores and the Groth16 prover 14 s
  ([`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md) headline; [`bridge.md`](../design/bridge.md) §18.7).
- **A backlog** costs the per-hour figure times the hours plus one fixed part, and can be split into
  segments of 180 blocks proven independently and in parallel, then verified by one checkpoint
  (composition, dev mode) ([`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md) §3, §5). A busy zone
  (every block full) costs about 56M cycles per counted support block under rule (A).

### 5.3 Vault gas per checkpoint

| | Foundry rules (today's mainnet), v5 vault, real verifier | Sepolia node prices, v3 vault |
|---|---:|---:|
| rule (A): `submitCandidate` (records a beacon) | 438,444 | 1,381,770 |
| rule (A): `recordBeacon`, at least `W` later | 77,937 | 262,063 |
| rule (A): `finalize` (second proof, echoing the beacon) | 457,965 | 1,183,452 |
| **rule (A) checkpoint** | **974,346** | **2,827,285** (derived sum) |
| rule (B): `submitCandidate` (no beacon) | 347,820 | not measured |
| rule (B): `finalizeAttested` after the wait (no proof) | 195,149 | not measured |
| **rule (B) checkpoint** | **542,969** | not measured |

Sources: [`vault/GAS.md`](../../vault/GAS.md), "v5 with the real verifier";
[`e2e/README.md`](../../e2e/README.md), "What the round trip costs on today's Sepolia" (each transaction priced
by Sepolia's node on a fork's state; first candidate and first checkpoint of a fresh v3 vault). One
checkpoint is paid per finalised LEZ state, not per burn. The **checkpoint fee**, a fixed amount per
token and denomination kept from each release, is credited to whoever finalised the root the release
uses (`finalize` or `finalizeAttested`) and withdrawn with `claimOwed`; it is how provers are paid
([`vault/README.md`](../../vault/README.md), "Fees (v5, D13)").

### 5.4 The sequence

Rule (A): submit the candidate; wait `W = Δ + (t − 1) × turnSeconds` (at least 3 h 16 min for three
sequencers with the design's defaults; 40 min with the testnet values 600 s + 2 × 900 s); someone
records a beacon; post its nonce on LEZ (`Echo`); prove the finalisation over blocks containing it; call
`finalize`. Rule (B): submit the candidate with the proof of `t` final-tier attestations; after `Δ`
(1 h by default) anyone calls `finalizeAttested`. A candidate cancelled by a watcher can be submitted
again and waits a full `W` again ([`vault/README.md`](../../vault/README.md), "LEZ checkpoints";
[`bridge.md`](../design/bridge.md) §9). The attestor beside each sequencer (`ferrata_attestor`) is
built as a program (`checkpoint/attestor/`, EVIDENCE rows 63–66: a real zone of four sequencers), with its core
in `checkpoint/crates/attestor-core`, and as a Logos module, `ferrata_attestor_module` (rows 94–95).

Rehearsed end to end: `e2e/sepolia-live.sh` and, on `sepolia-cloud`, `e2e/sepolia-cloud.sh` (deployment,
deposits, candidate, the 40-min wait, beacon, the finalisation proven on a GPU during the run, the release
delay, releases); its fork rehearsal reached ALL CHECKS PASSED with 16 transactions. A real run takes
about an hour and three quarters plus the wait for a GPU ([`e2e/README.md`](../../e2e/README.md), "Running it in
the cloud").

## 6. The relayer and the wrapper

### 6.1 What `ferrata-relayer` does

`ferrata-relayer` (the daemon in `cli/src/relayerd.rs`, the binary `cli/src/bin/relayer.rs`, its checks in
`cli/src/relayer.rs`; prototype, unaudited; the same code runs as the Logos module `ferrata_relayer_module`,
[`modules/README.md`](../../modules/README.md)) runs one round every `--poll-secs` (default 12 s, one Ethereum slot):

0. **Root imports** (with `--import-roots` and the LEZ side): §4. At most one `ImportRoot` per `--import-every`
   period, tried from the period's first round until it is sent or there is nothing to import; the period done is
   kept in `<state>/root-import.json`, so a restart does not import twice in it.
1. **Inbox.** Each `<inbox>/*.json` job is checked, then sent; the file moves to `<state>/done/` or
   `<state>/refused/` and one JSON line on stdout says what happened and why. A job that fails for a
   temporary reason (the root not final yet, the release delay, a freeze, a cap, the endpoint unreachable)
   stays in the inbox and is retried every round (`job-deferred`) until it is older than `--max-job-age`
   (default 7 days). Jobs:
   `{"kind":"release","journal":"0x…","seal":"0x…"}` from a holder who opts in (the release journal
   and its Groth16 seal), `{"kind":"release-succinct","receipt":{…}}` from a holder who cannot wrap (the
   succinct receipt; the relayer wraps it, §6.5), `{"kind":"checkpoint","journal":"0x…","seal":"0x…"}` from the checkpoint
   prover (§5.4), `{"kind":"gas-drop","receipt":{…}}` from a claimant (§6.4).
2. **Candidates.** Every live candidate on the vault, anyone's, is rebuilt from its `CandidateSubmitted`
   event and the vault's `nMin`/`tMin`, checked against the journal digest the vault stored, and finalised
   with `finalizeAttested` once its wait W has passed (rule (B); anyone may finalise, the checkpoint fee
   goes to the candidate's submitter).
3. **Fees.** Whatever the vault credited the relayer (`owed`: every checkpoint fee, and a relay fee whose
   push failed) is withdrawn with `claimOwed`, for ETH and every token it released.
4. **Burns** (with `--sequencer`): the configuration's burns on LEZ past the newest accepted burn count; if
   there are any and no live candidate, `<state>/needs-checkpoint.json` asks the checkpoint prover (§5,
   a separate process) for a checkpoint, which comes back as a checkpoint job (`relayer::checkpoint_needed`;
   tested against the harness zone: 11 burns and none accepted ask for one, none once they are accepted).
   On a rule (A) vault the daemon does not finalise (that needs the beacon proof, the prover's job).

Logs are one JSON object per line (`start`, `root-imported`, `root-import-waiting`, `root-import-refused`,
`root-import-failed`, `root-import-closed`, `job-done`, `job-refused`, `candidate-waiting`, `finalized`,
`fees-collected`, `checkpoint-needed`, `job-deferred`, `round-failed`, `stop`). The state directory is all
it keeps between rounds: the drop book (`drops.json`), the done and refused jobs. SIGTERM or SIGINT stops it
after the round in progress (a `stop` line names the signal), so a service manager can restart it at will; a gas drop is recorded before its credit is sent, so a crash cannot cause a second drop for
the same `N` (a failed credit stays recorded as failed: remove the entry only knowing it was not sent).

### 6.2 Starting a relayer

```sh
cd cli && cargo build --release --bin ferrata-relayer      # LEZ's toolchain as for the CLI (cli/README.md)
target/release/ferrata-relayer \
  --eth-rpc "$SEPOLIA_RPC" --vault 0x… --chain-id 11155111 --from-block <the vault's deployment block> \
  --keystore relayer.json --password-file relayer.pass \
  --state /var/lib/ferrata-relayer \
  run --inbox /var/lib/ferrata-relayer/inbox
target/release/ferrata-relayer --eth-rpc … --vault 0x… --keystore … --password-file … status
```

For gas drops and the burn watch add the LEZ side: `--sequencer <url> --seed <the relayer's own seed file>
--lez-wallet <dir> --lez-password-file <file> --bridge-account <program id> --lez-account <its public
account, which pays the drops> --gasdrop-image-id 0x… --drop-amount <native LEZ per drop>`. The Ethereum
account needs gas money only; it is paid relay fees by the releases that name it and checkpoint fees through
`owed`. `--accept-unpaid` also submits releases that pay no relay fee (off by default). `--wrap-command
<prover>` makes it wrap succinct receipts (§6.5); jobs must be written atomically (write `x.json.tmp`, then
rename it to `x.json`: the daemon reads only `*.json`). `--unlocked <address>`
lets a local anvil sign (tests and rehearsals only; refused unless the endpoint answers `anvil_nodeInfo`).

Root imports: add `--import-roots` (with the LEZ side above; `--import-every <seconds>`, default 3600, 0 for every
round). `--lez-devnet http://127.0.0.1:<port>/rpc` reaches the mini-app devnet's zone instead of a sequencer (tests
only: the devnet signs and settles). Inside Logos Core the module takes the same arguments:
`logosctl call ferrata_relayer_module start @request.json` with `{"args": [...]}`, then `status` and `stop`
([`modules/README.md`](../../modules/README.md); built without LEZ's wallet, so `--sequencer` is the binary's only).

### 6.3 What it holds, and what it refuses

It holds its own keys and nothing of a user's: a release job is what the vault call publishes anyway
(never `sOut`, `r`, the burn index or a seed: the holder proves, any wrapper wraps), a checkpoint job is
public, a gas-drop receipt shows `N`, a root, a one-time address and the relayer. Before spending gas it
refuses a job for another vault, chain or configuration; a release whose proof names another relayer
(only that one can be paid, and the proof cannot be changed); an unpaid release (by default); a used
nullifier; a checkpoint not built on the vault's final state, or already a candidate. Everything else the
vault would refuse (root not accepted, delay, fee, caps, freezes, the proof) is caught by an `eth_call`
before signing and reported with the vault's error. Whatever a relayer sends anyway, the vault refuses:
on the Sepolia fork below, a forged checkpoint root and releases with the relayer, the recipient or the
fee changed were all refused by RISC Zero's verifier (`VerificationFailed`, `0x439cc0cd`), and a replay by
`NullifierAlreadyUsed`.

### 6.4 The gas drop (bridge.md §6; v3 D13)

A first private claim needs native LEZ for its fee. The claimant proves the **gas-drop receipt**
(`bridge/guests/src/bin/gasdrop.rs`, statement `ferrata_bridge_core::gasdrop`): the claim statement over
its deposit under an imported root, with the journal `DROP_DOMAIN ‖ root ‖ evmChainId ‖ vault ‖ N ‖
oneTimeAddress ‖ relayer` (212 bytes), so the receipt cannot be replayed to another relayer or redirected
to another address. The relayer verifies it against the gas-drop image id (fake receipts verify only in
RISC Zero's dev mode), checks that the receipt names it and its configuration, that the root is one LEZ
imported (recomputed from the vault's verified deposit list at each imported leaf count), that `N` is not
claimed on LEZ and has not had a drop from it (`drops.json`, kept across restarts), then sends one native
credit to the one-time address (LEZ wallet's native transfer, `CreditDelivery::Automatic`). The claim names
the relayer and mints it the token's relay fee. It learns a time, the one-time address and `N` (the claim
publishes `N`), never which deposit, its depositor or amount.

Measured (build box, CPU, 16 cores): 76,230 user cycles for every one of 16 random witnesses (one count),
a real succinct proof in 14.6 s and 14.9 s, 584,106 bytes as JSON; image id at this branch's build path
`0xe10554850874c952eae7a7a939cc832be97bcd9f4426214269360d736ce0d207` (path-dependent; rebuild and record it
for a deployment).

Not built: the sealed variant (a credit to a private one-time account); the sweep of the credit inside the
private claim by LEZ's wallet; a run against a live sequencer (the harness does not model LEZ's native
token program, so its credit is a recorded stand-in).

### 6.5 Wrapping

A release needs a Groth16 seal; RISC Zero's prover image runs only on x86 (Docker, about 7.3 GiB, 77.5–79.9 s
on the laptop for a release; 55–93 s on the build box). Any relayer can wrap a user's succinct receipt and
learns only what the release publishes ([`bridge.md`](../design/bridge.md) §12.3, §18.2). With
`--wrap-command <prover>` the daemon takes `release-succinct` jobs: it checks the release as for a sealed job
(before any work), verifies the receipt against the vault's `releaseImageId`, runs identity_p254 through
r0vm, calls `<prover> <input.json> <proof.json>` (RISC Zero's prover image, e.g. `docker run` of
`risczero/risc0-groth16-prover:v2025-04-03.1` as risc0-groth16 3.0.5 starts it, or a remote prover such as the
build box's `groth16_wrap` function), builds the Groth16 receipt as risc0's `succinct_to_groth16` does,
verifies it, and sends the release. Measured on the build box: 18.6 s from the succinct receipt to the seal
with the prover stood in by the proof it had produced on Modal (15 s there), identity_p254 21.1 s on its own.

### 6.6 Tested, and the numbers

- `cli/tests/relayer.sh` on the build box: (0) the library's unit tests (the recorded crown checkpoint journal
  decoded and re-encoded byte for byte, the burn watch, which failures are retried, imported roots); (1) the
  gas-drop checks on a real receipt (one drop per `N`;
  another relayer, another configuration, an unimported root, a claimed deposit, another image id and a
  changed journal refused, also after a restart); (2) **the daemon on an anvil fork of Sepolia** at the
  block before `crown-live-1`'s `submitCandidate`, with that run's real checkpoint and release proofs:
  candidate 375,281 gas, `finalizeAttested` 218,385 after W = 3,600 s, releases 358,908 and 393,108, the
  checkpoint fees (2 × 10¹² wei) collected with `claimOwed` (the same transactions and gas in every run); a
  new real release proof of the same burn naming a relayer with the registered fee, handed to the daemon as a
  succinct receipt, wrapped by it and sent as that relayer (407,052 gas, 2 × 10¹² wei received), its copy
  naming another relayer refused; `status`, and a
  daemon without a round limit stopped cleanly by SIGTERM;
  the early release deferred and released once the root was final; a second candidate, another relayer's
  daemon, an unpaid release and a replay refused ([`e2e/runs/relayer-fork/`](../../e2e/runs/relayer-fork/results.json));
  (3) **the LEZ harness** (`cli/tests/e2e.rs` with `FERRATA_GASDROP_ELF`): a real gas-drop receipt for a
  vault deposit checked against the zone's imported roots and nullifiers, the claim naming the relayer
  (relay fee 10¹⁶ wei minted to it on LEZ), the receipt refused afterwards as claimed, the burn watch on the
  zone's 11 burns, and a release that pays the relayer 10¹⁴ wei (134,973–134,985 gas with the mock
  verifier), its redirected copy refused ([`e2e/runs/relayer-harness/`](../../e2e/runs/relayer-harness/test.log)).
- `modules/scripts/relayer-e2e.sh` (13 November, build box; [`modules/results/relayer-2026-11-13/`](../../modules/results/relayer-2026-11-13/README.md)),
  with the binary and with `ferrata_relayer_module` in a headless Logos Core (dev and portable builds): (2) above
  through the module, the same transactions and gas; and **root imports on the mini-app's devnet** (the light
  client's record of anvil's own block written by the devnet, the proofs anvil's `eth_getProof`): waiting without a
  light client, n = 2 imported (273,511–275,460 execution gas) and byte for byte `ferrata_bridge_module`'s
  `import_root` for the same record, nothing sent when up to date, a record the proofs do not match refused before
  sending (code 110, the bridge module's for it), n = 3, the hourly cadence (no import within the hour, n = 4 in the
  next), a clean stop on SIGTERM; also through the module installed from the local catalog.
- Release gas on the v5 vault with the real verifier: 373,657 (ETH) and 403,523 (ERC-20 with the relayer's
  fee) on Foundry's rules; 619,477 and 648,447 on Sepolia's node prices, v3 vault
  ([`vault/GAS.md`](../../vault/GAS.md); [`e2e/README.md`](../../e2e/README.md)).

## 7. Watchers (`monitor`)

Up to three hot addresses, set by the admin's proposal and applied 14 days later. Each watcher runs its
own Logos Blockchain, LEZ and Ethereum nodes ([`bridge.md`](../design/bridge.md) §14;
[`vault/README.md`](../../vault/README.md), "Roles"). The program is [`monitor/`](../../monitor/README.md):
one process per watcher key, following one vault. It is **built**, and rehearsed on a local chain and on a fork of
Sepolia at the live v5 vault. It has not run with its transactions on a public network.

What a watcher can do:

- **Cancel** a pending candidate (`cancelCandidate`; 37,438 gas in the vault's Foundry table; 41,477
  in `monitor`'s own transactions on anvil, base cost included). `monitor` does this when:
  - the tip conflicts with its nodes' final tier;
  - `n` or `t` is not the live key set's at the tip (D13);
  - a checkpoint field differs from its node's own computation;
  - under rule (B), the tip is not already final when the candidate is submitted (it allows its own
    node `--b-lag-grace`, 300 s);
  - under rule (A), the tip is not final by `submittedAt + Δ_g`.

  `monitor` always cancels before the deadline minus `--margin`.
- **Pause** checkpoints until 12 h from now (`pauseCheckpoints`, 47,260 gas). `monitor` does this
  when a cancelled state is submitted again (D11), and, under rule (A), when its nodes' final tier has
  not advanced for `--stall-pause-after`, which should be set to `(t − 1) × turnSeconds + S/2` (D4). A
  watcher cannot lift a pause or shorten a freeze; the persistent case is raised to the freeze
  authority, which decides on the checkpoint freeze.
- Nothing else: watchers cannot freeze anything else, revoke, retire or move funds. A wrong checkpoint
  that was finalised while no watcher acted raises `finalized-wrong`, with `revokeFrom(index)` and its
  deadline, for the freeze authority's multisig. A `monitor` holding the freeze authority's key, as on
  a testnet, revokes it itself.
- Gas from [`vault/GAS.md`](../../vault/GAS.md) (v5, mock-verifier table; these calls verify no proof)
  and [`monitor/README.md`](../../monitor/README.md) (receipts on anvil).

Running it:
- **Node view.** `monitor` reads its nodes through a node view, a JSON document per poll: the final
  tier, the channel's key sets, and optionally the checkpoint fields at a tip and the attestors'
  published final-tier messages. A file or a command supplies it; `--node` can be given once per
  node, and all nodes must agree. `monitor node-view` builds the view from a LEZ sequencer's block
  record. A client that reads the sequencer's RPC directly is not built.
- **Key.** The watcher key comes from the environment (`--key-env`) or a keystore, never from the
  command line.
- **Alerts.** Alerts are JSON lines, written to a log and passed to an operator command (a pager).
- **Restarts.** The state file is rewritten atomically, and a restart rescans the vault's events and
  re-reads every pending candidate before acting. A lost state file costs a rescan, never a missed
  cancellation. One process per state file is enforced by an OS lock. A stuck transaction times out,
  and the next poll retries.
- **Rehearsal.** `monitor/tests/anvil-e2e.sh`, on a local anvil chain, makes 46 checks:
  - an honest candidate passes untouched;
  - wrong ones are cancelled in time;
  - a monitor killed mid-window and restarted still cancels, 3,000 s before the deadline;
  - a lost state file is recovered from the chain;
  - a revocation by the freeze authority's monitor, the persistent-candidate pause, the rule (A)
    cases, a corrupted attestor's final-tier message flagged, a failed cancellation (no gas
    money) retried at the next poll, a clean SIGTERM shutdown, and two node views of which one forked.
  `monitor/tests/fork-live-vault.sh` runs on an anvil fork of Sepolia at the live v5 vault (8 checks):
  - the monitor follows the vault's own history and audits the round trip's checkpoint and its real
    attestation as good;
  - on the fork it cancels a forged candidate 3,599 s before its deadline and leaves an honest one.

A watcher that lies can only delay finalisation until the admin replaces it. One that stops lets a bad
candidate through: under rule (B) the release delay and revocation still catch it; under rule (A),
outside its conditions, funds can be lost ([`bridge.md`](../design/bridge.md) §11.8, §14;
[`docs/trust.md`](../trust.md)). Not built:
- a client for the LEZ RPC and the Logos Blockchain node's channel answer;
- the node-side computation of the checkpoint fields;
- the Logos module;
- fee bumping;
- a run against a public network.

## 8. Keys

| Key | Held by | Can | Kept |
|---|---|---|---|
| LEZ fee payer | updater, importer, relayer | pay LEZ gas | hot; holds only gas money |
| Ethereum sender | checkpoint prover, relayer | pay Ethereum gas; receives checkpoint and relay fees through `owed` | hot |
| Watcher key | each watcher | cancel candidates, pause 12 h | hot by design |
| Attestation key (rule (B)) | each sequencer's attestor, certified once by its channel key | sign the node's final tip for this deployment | beside the sequencer; the channel key never signs bridge data |
| Admin, freeze authority | multisigs | the bounded powers of the deployer guide | cold |
| Deployment key | the deployer | nothing after deployment | throwaway on testnets; never on a command line (Foundry keystore variables) |

Sources: [`bridge.md`](../design/bridge.md) §8.3, §14; [`vault/README.md`](../../vault/README.md),
"Roles", "Deployment (v5)". An updater needs no Ethereum key and no standing on Ethereum
([`attestation.md`](../design/attestation.md) §9).

## 9. Off-chain resources (P.6)

| Component | CPU | Memory | Time | Source |
|---|---|---|---|---|
| Updater daemon (`ferrata-updater`, sequencer backend), public path, gates G2–G5 on a LEZ v0.4.0-rc1 zone | 0.043–0.092 of one core per daemon | 57.3–57.8 MiB peak, 33–38 MiB median resident | beacon API 10.3–10.4 MB per honest daemon (G2); sequencer RPC 0.57–2.07 MB up, 7.4–19.6 MB down per daemon; updates replayed 81 to 122 times faster than the lab recorded them, so no per-day figure | [`perf-gate-sequencer.md`](../../lez/results/perf-gate-sequencer.md); [`performance.md`](../performance.md) P.6 |
| Updater, proven path (one private transaction per update, this tree's program) | 16 CPUs for 1,673.6 s per update (9,049,362 user cycles) | 9.21 GiB peak | 49.5 s on one L4, 54 min on a laptop CPU (both the earlier image) | [`perf-proofs.md`](../../lez/results/perf-proofs.md); [`lez/RESULTS.md`](../../lez/RESULTS.md) §9 |
| Checkpoint prover | GPU advised | ~10.9 GiB GPU, ≤ 1.08 GiB host | 24.1 s per idle hour, 219.3 s per busy hour on one L4 | [`gpu-prover/RESULTS.md`](../../gpu-prover/RESULTS.md) |
| Groth16 wrap | x86 | ~7.3 GiB | 195–215 s per checkpoint seal, 78–80 s per release seal (laptop) | [`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md); [`bridge.md`](../design/bridge.md) §18.2 |
| The whole lab zone (sequencer, node, driver) | 2 physical cores requested | state 20–31 MB compressed per generation, growing with write-ahead logs | a session 83–142 s over 85 sessions; 0.08 USD a day at a two-hour cadence | [`lez/lab/README.md`](../../lez/lab/README.md) |

Measured as binaries: the updater (above), and the bridge's `monitor` and relayer
([`perf-daemons.md`](../../lez/results/perf-daemons.md)). **Not measured:** the daemons as Logos modules under
`logosctl daemon` (`ferrata_updater_module`, `ferrata_monitor_module`, `ferrata_relayer_module`), and the
`ferrata-attest` CLI and its module ([`performance.md`](../performance.md) P.6).

## 10. Failure modes and recovery

| Failure | Effect | What to do | Source |
|---|---|---|---|
| Beacon or execution RPC unreachable | nothing sent | the next session fetches again and continues from what the chain holds; a session that cannot finish changes nothing it cannot redo | [`lez/lab/README.md`](../../lez/lab/README.md), "The two instances" |
| Sequencer or node restart | — | the zone survives being stopped (the lab restarted it after 54 min); start the sequencer so that its blocks fall 1–6 s after a Bedrock epoch second | [`lez/lab/README.md`](../../lez/lab/README.md), "Survival" |
| Transaction outside its LEZ validity window | charged the **whole declared gas limit** | declare tight limits (the lab: 4.8M per job step) | [`attestation.md`](../design/attestation.md) §4.4 |
| Duplicate update step | 2.2M wasted (first step) | deferral (§3.4) | [`attestation.md`](../design/attestation.md) §4.4 |
| Ethereum finalizes nothing for a whole period | the instance stops safely and must be replaced from a fresh checkpoint (a new trust decision) | consumers move under the replacement recipe | [`attestation.md`](../design/attestation.md) §4.1; [`consumer-guide.md`](consumer-guide.md) §9 |
| Instance past its weak-subjectivity bound | the store refuses every update (53); the instance is replaced from a fresh checkpoint under a new root id (the bound: EVIDENCE rows 69, 70) | — | [`attestation.md`](../design/attestation.md) §4.6 |
| A job whose final step fails | stays unapplied; clean-up **not built** | start a new job | [`attestation.md`](../design/attestation.md) §4.5 |
| Prover backlog | larger proofs | split into 180-block segments proven in parallel | [`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md) §3 |
| Candidate cancelled | one more wait | submit again; it waits a full `W` | [`vault/README.md`](../../vault/README.md) |
| Silent sequencers | rule (B) needs `t` live keys; rule (A) `t + 1`, so one silent key stalls (A) for `n ≤ 5` | — | [`bridge.md`](../design/bridge.md) §9, §20 |
| A wrong checkpoint not revoked within 7 days | the vault halts for good: no honest checkpoint can follow it | revoke within the window (freeze authority) | [`bridge.md`](../design/bridge.md) §20 |
| Verifier stop tripped (RISC Zero's proof of unsoundness) | no more releases or refunds | — | [`vault/README.md`](../../vault/README.md), "Verifier and stop" |
| Ethereum gas mispriced (Glamsterdam state gas) | out-of-gas | take every limit from the node's estimate × 1.25 | [`deployer-guide.md`](deployer-guide.md) §4.1 |

### 10.1 Reading a deployed vault: the admin view

[`admin/index.html`](../../admin/index.html) is one static page (no build step, its JavaScript
inline) that reads any Ethereum JSON-RPC endpoint you type, with read-only methods only, and shows a
deployed vault: its configuration and roles, the watchers, the verifier's emergency stop, freezes,
tokens with their caps and fee schedules, fees owed, deposits (the root rebuilt from the events and
compared with the vault's), checkpoints and pending candidates with their windows, releases and
refunds, and retirement. Whatever the contract does not expose is said on the page. `node
admin/dump.mjs <rpc> <vault>` runs the same code without a browser and prints it as text;
[`admin/results/`](../../admin/results/) holds a read of the live Sepolia vault (block 12,113,610:
mode B, 2 deposits, checkpoint #0 final, 2 releases, the vault's ETH balance equal to escrow plus
fees owed). See [`admin/README.md`](../../admin/README.md) for each value's getter or event.

## 11. Retirement and `Sunset`

The vault and the checkpoint program parse LEZ v0.4 blocks; a LEZ change they cannot follow (including a
change of turn or slot length) needs a retirement first. The freeze authority, or **anyone once no
checkpoint has finalised for 30 days**, calls `retire()` on the vault; on LEZ the freeze authority sends `Sunset`,
or anyone does once the vault's `retired` flag is attested there (the keyless `Sunset`, EVIDENCE row 74). After that: no deposits, no root imports;
claims against imported roots, burns, checkpoints and releases continue. The first non-revoked final
checkpoint carrying the sunset sets the refund boundary at its imported leaf count; once its revocation
window has passed, anyone can refund each unimported deposit to its depositor. Operators keep proving
checkpoints after a sunset until holders have released ([`bridge.md`](../design/bridge.md) §16;
[`vault/README.md`](../../vault/README.md), "Refunds"; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md)
§8: the checkpoint collects exactly the one `Sunset` that executed).

## 12. What is not built

The relayer's Logos module (`ferrata-relayer` is a plain binary, §6; the monitor, the rule (B) attestor and the
updater are built as Logos modules, the updater's with both of its backends, §3.6); the relayer's root
imports and its run against a live sequencer, the sealed gas drop to a private one-time account and its sweep inside
the private claim (§6.4); `verify_checkpoint` as an API of Logos Core's Ethereum module (its execution check runs through `verified_proxy_module`); the attestation side's `monitor`; job clean-up; the acceptance run at 100 updates and on a sequencer (it ran on 58 recorded updates in process, §3.4);
daemon resource measurements; the amortised costs at the recommended
cadence with real proving (milestones A4, B4); a deployment on a public LEZ network. Sources:
[`attestation.md`](../design/attestation.md) §15; [`bridge.md`](../design/bridge.md) §1, §18.6, §19;
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §9.
