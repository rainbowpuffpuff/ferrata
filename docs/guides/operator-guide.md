# Operating Ferrata: the updater, the importer, the checkpoint prover and relayer, watchers

**Operator guide (RFP-021 U.3, S.5, S.7, R.9, R.10, P.6; RFP-022 U.3, R.6, R.9, R.13, P.6). First
draft, 11 November 2026.** For anyone who keeps Ferrata live: advancing an attestation instance,
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

**Status.** The daemons are **not built**: the updater, the bridge relayer (root imports, checkpoint
proofs, gas drops, wrapping and submission) and `monitor` are designed as Logos modules run under
`logosctl daemon`, with structured logs, clean shutdown and configurable endpoints and depths
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
| operate | `lez/lab/lab.sh deploy | run 60 | status | fetch | verify | stop` |
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
  an already applied update is refused at 207,687; `Finish` after the job applied at 5,731
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

  Derived by arithmetic from that formula; not measured. A duplicate costs the 2.2M step above. The
  planned acceptance run (two honest daemons and one adversarial, 100 updates, honest gas on refused
  steps at most 10% of gas on applied steps) is **not done**.
- **Not built**: joining another party's open job; clean-up of jobs whose final step fails (they stay,
  unapplied) ([`attestation.md`](../design/attestation.md) §4.5).

### 3.5 Detecting a stalled instance

Compare the instance's finalized slot and root with the chain's: the lab's `lab.sh status` puts both
instances against the chains' clocks, and each session's fetch record holds the root a second provider
reports for the newest finalized slot, which must equal the instance's `finalized_root`
([`lez/lab/README.md`](../../lez/lab/README.md), "Re-check"). In the design `monitor` compares records with several providers'
canonical roots and alarms at half the weak-subjectivity bound (**not built**,
[`attestation.md`](../design/attestation.md) §3.6, §8). An instance lags by the cadence plus finality;
it catches up when anyone sends the missing updates, as long as its store knows the signing committee.

## 4. The root importer (RFP-022 P.2)

`ImportRoot` attests the vault's slots 0 and 1 under one finalized record and writes `root(cid, n)`
once per leaf count. Run it on a fixed cadence (the design ties it to the hourly update), never in
response to a particular deposit ([`bridge.md`](../design/bridge.md) §5.4).

| Measured | Execution gas | Storage gas |
|---|---:|---:|
| `ImportRoot`, 8 leaves, synthetic vault storage (in-process harness, the merged program) | 231,365 | 2,025 |
| on the v5 vault's own storage, 3 and 6 leaves | 277,263; 278,099 | — |
| under a Gloas record, with the block's RLP header (2 leaves) | 262,195 | 2,612 |
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
**not built** as a module; its core is (`checkpoint/crates/attestor-core`).

Rehearsed end to end: `e2e/sepolia-live.sh` and, on `sepolia-cloud`, `e2e/sepolia-cloud.sh` (deployment,
deposits, candidate, the 40-min wait, beacon, the finalisation proven on a GPU during the run, the release
delay, releases); its fork rehearsal reached ALL CHECKS PASSED with 16 transactions. A real run takes
about an hour and three quarters plus the wait for a GPU ([`e2e/README.md`](../../e2e/README.md), "Running it in
the cloud").

## 6. The relayer and the wrapper

- **Submission.** A relayer submits users' claims (on LEZ) and releases (on Ethereum) and is paid the
  registered relay fee, which the proof binds, so it can neither redirect nor overcharge; a user can pick
  another relayer per operation, or submit directly ([`bridge.md`](../design/bridge.md) §14).
- **Wrapping.** A release needs a Groth16 seal; the wrap runs only on x86 (Docker, about 7.3 GiB,
  77.5–79.9 s on the laptop for a release). Any relayer can wrap a user's succinct receipt and learns only
  what the release publishes ([`bridge.md`](../design/bridge.md) §12.3, §18.2).
- **What a relayer learns**: a time, a one-time address and the transaction it submits; never which
  deposit or burn ([`bridge.md`](../design/bridge.md) §6, §14).
- **The gas drop** (a relayer funding a user's private LEZ fee through a sealed credit per nullifier) is
  **not built** (milestone M0 tests it).
- Release gas: 373,657 (ETH) and 403,523 (ERC-20 with the relayer's fee) on Foundry's rules, v5 vault, real
  verifier; 619,477 and 648,447 on Sepolia's node prices, v3 vault ([`vault/GAS.md`](../../vault/GAS.md);
  [`e2e/README.md`](../../e2e/README.md)).

## 7. Watchers (`monitor`)

Up to three hot addresses, set by the admin's proposal and applied 14 days later; each runs its own Logos
Blockchain, LEZ and Ethereum nodes ([`bridge.md`](../design/bridge.md) §14; [`vault/README.md`](../../vault/README.md), "Roles").

- **Cancel** a pending candidate (`cancelCandidate`, 37,438 gas) when, under rule (B), its tip is not
  already in the watcher's node's final tier; under rule (A), when the tip is not final by
  `submittedAt + Δ_g`, or the key set lags the live one.
- **Pause** checkpoints until 12 h from now (`pauseCheckpoints`, 47,260 gas); a watcher cannot lift a
  pause or shorten a freeze.
- Nothing else: watchers cannot freeze anything else, revoke, retire or move funds.
- Gas from [`vault/GAS.md`](../../vault/GAS.md) (v5, mock-verifier table; these calls verify no proof).

A watcher that lies can only delay finalisation until the admin replaces it; one that stops lets a bad
candidate through, which under rule (B) the release delay and revocation still catch, and under rule (A),
outside its conditions, can lose funds ([`bridge.md`](../design/bridge.md) §11.8, §14; [`docs/trust.md`](../trust.md)).
`monitor` is **not built**; the vault side is.

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
| Updater, public path | light (fetch, encode, send) | — | one session of the lab: 123–133 s for 6–11 transactions | [`lez/lab/README.md`](../../lez/lab/README.md) |
| Updater, proven path | 9.0M-cycle proof | 9.18 GiB peak (laptop CPU) | 54 min laptop CPU; 49.5 s one L4 | [`attestation.md`](../design/attestation.md) §4.2; [`lez/RESULTS.md`](../../lez/RESULTS.md) §9 |
| Checkpoint prover | GPU advised | ~10.9 GiB GPU, ≤ 1.08 GiB host | 24.1 s per idle hour, 219.3 s per busy hour on one L4 | [`gpu-prover/RESULTS.md`](../../gpu-prover/RESULTS.md) |
| Groth16 wrap | x86 | ~7.3 GiB | 195–215 s per checkpoint seal, 78–80 s per release seal (laptop) | [`checkpoint/RESULTS.md`](../../checkpoint/RESULTS.md); [`bridge.md`](../design/bridge.md) §18.2 |
| The whole lab zone (sequencer, node, driver) | 2 physical cores requested | state 20–31 MB compressed per generation, growing with write-ahead logs | 0.08 USD a day at a two-hour cadence | [`lez/lab/README.md`](../../lez/lab/README.md) |

Daemon CPU, memory and bandwidth for the real modules are **not measured** (they do not exist).

## 10. Failure modes and recovery

| Failure | Effect | What to do | Source |
|---|---|---|---|
| Beacon or execution RPC unreachable | nothing sent | the next session fetches again and continues from what the chain holds; a session that cannot finish changes nothing it cannot redo | [`lez/lab/README.md`](../../lez/lab/README.md), "The two instances" |
| Sequencer or node restart | — | the zone survives being stopped (the lab restarted it after 54 min); start the sequencer so that its blocks fall 1–6 s after a Bedrock epoch second | [`lez/lab/README.md`](../../lez/lab/README.md), "Survival" |
| Transaction outside its LEZ validity window | charged the **whole declared gas limit** | declare tight limits (the lab: 4.8M per job step) | [`attestation.md`](../design/attestation.md) §4.4 |
| Duplicate update step | 2.2M wasted (first step) | deferral (§3.4) | [`attestation.md`](../design/attestation.md) §4.4 |
| Ethereum finalizes nothing for a whole period | the instance stops safely and must be replaced from a fresh checkpoint (a new trust decision) | consumers move under the replacement recipe | [`attestation.md`](../design/attestation.md) §4.1; [`consumer-guide.md`](consumer-guide.md) §9 |
| Instance past its weak-subjectivity bound | design: replacement; **the bound is not built** | — | [`attestation.md`](../design/attestation.md) §4.6 |
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
checkpoint has finalised for 30 days**, calls `retire()` on the vault; the freeze authority sends `Sunset`
on LEZ (permissionless `Sunset` is **not built** on LEZ). After that: no deposits, no root imports;
claims against imported roots, burns, checkpoints and releases continue. The first non-revoked final
checkpoint carrying the sunset sets the refund boundary at its imported leaf count; once its revocation
window has passed, anyone can refund each unimported deposit to its depositor. Operators keep proving
checkpoints after a sunset until holders have released ([`bridge.md`](../design/bridge.md) §16;
[`vault/README.md`](../../vault/README.md), "Refunds"; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md)
§8: the checkpoint collects exactly the one `Sunset` that executed).

## 12. What is not built

The updater, relayer and `monitor` daemons and their Logos modules; `verify_checkpoint`; the rule (B)
attestor module; the gas drop; joining an open job and job clean-up; the weak-subjectivity bound; the
duplicate-rate acceptance run; daemon resource measurements; the amortised costs at the recommended
cadence with real proving (milestones A4, B4); a deployment on a public LEZ network. Sources:
[`attestation.md`](../design/attestation.md) §15; [`bridge.md`](../design/bridge.md) §1, §18.6, §19;
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §9.
