# Ferrata: performance report (RFP-021 P.1–P.9, RFP-022 P.1–P.7)

**First draft, 11 November 2026.** Each requirement with the measured figures, where and how they were
measured, and the gaps. Paths are relative to the repository root. The planned `just bench`, which
would regenerate every number from a fresh checkout with real proving and record the hardware, is
**not built**; today each number comes from the test, script or run named beside it
([`docs/design/attestation.md`](design/attestation.md) §13). [`docs/EVIDENCE.md`](EVIDENCE.md) lists the
evidence with its commands.

## Where things were measured

| Label | What it is | Sources |
|---|---|---|
| **executor** | RISC Zero zkVM 3.0.5's executor: user cycles, deterministic, no proof | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md); [`attestation.md`](design/attestation.md) §5.1 |
| **in-process harness** | LEZ v0.4.0-rc1's own state transition functions, called in-process with LEZ's fee settlement copied; one transaction per block, genesis base fees, no base-fee movement; private parts in dev mode (executed, not proven) unless a real proof is named | [`lez/RESULTS.md`](../lez/RESULTS.md) §1; [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) |
| **live sequencer** | LEZ's own `sequencer_service` at `v0.4.0-rc1` with a Logos Blockchain node (one validator, leader proofs in dev mode) on a cloud container; charges re-checked by replaying the chain through LEZ's `chain_state` | [`lez/RESULTS.md`](../lez/RESULTS.md) §9; [`lez/live/inbound/README.md`](../lez/live/inbound/README.md); [`lez/lab/README.md`](../lez/lab/README.md) |
| **local EVM** | Foundry (forge 1.3.6, solc 0.8.26, 200 runs, cancun), `forge test --isolate`: whole transactions under today's mainnet gas rules | [`vault/GAS.md`](../vault/GAS.md) |
| **Sepolia node** | each transaction priced by a public Sepolia node (reth 2.10.0) with `eth_estimateGas` / `eth_simulateV1` on a fork's state: Glamsterdam's gas rules, nothing sent | [`e2e/README.md`](../e2e/README.md); [`vault/GAS.md`](../vault/GAS.md), "Creating the contracts on today's Sepolia" |
| **laptop CPU** | AMD Ryzen 9 7940HS, 8 cores, 16 threads, no GPU; shared with other jobs, so wall times state the load | [`docs/design/bridge.md`](design/bridge.md) §18 |
| **cloud GPU** | one NVIDIA L4 (24 GB) on Modal, 4 CPU cores, 32 GiB; r0vm 3.0.5 built with CUDA | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md) |
| **simulation** | the Python model of the v0.4 sequencer and Logos Blockchain code | `docs/design/finality-rule/RESULTS.md` |

LEZ's limits on v0.4: a block, and so a transaction, may use 10M execution cycles (`MAX_GAS_EXEC`); the
base fee stops rising at 5M (`TARGET_GAS_EXEC`); a private transaction costs a flat 409,764 execution
gas plus its public part's cycles and a flat 224,063 storage gas, and at most four fit in a block
([`attestation.md`](design/attestation.md) §4.2). Execution gas is the user cycles LEZ metered; storage
gas is the transaction's serialized size.

---

## RFP-021

### P.1 Verifying an attestation within one LEZ transaction

**Met** for every fact type measured. In-process harness, mainnet block 26,384,851 (finalized
10 November 2026), RISC Zero's accelerated Keccak in the program ([`lez/RESULTS.md`](../lez/RESULTS.md)
§5, `results/stage4-attest.md`, `results/stage4-attest-consumer.md`):

| Statement (one transaction each) | Execution gas | Of the 5M target | Storage gas |
|---|---:|---:|---:|
| account | 167,882 | 3% | 4,424 |
| storage slot with a predicate | 274,842 | 5% | 7,338 |
| absent slot is zero | 269,564 | 5% | 7,291 |
| ERC-20 `Transfer` log (receipt of 7 logs) | 96,584 | 2% | 2,623 |
| log in a receipt of 306 logs | 1,618,872 | 32% | 58,812 |
| one EIP-2935 hop (16 blocks back) and a slot | 498,204 | 10% | 13,578 |
| four facts and a relation in one call | 453,182 | 9% | 12,309 |
| reference consumer: claim of a `Transfer` (Claim → Attest → reply → Credit) | 183,538 | 4% | 2,768 |
| private consumer: read + attestation in its own proof | 421,641 (flat 409,764 + 11,877) | — | 224,063 |

Under a Gloas record (Sepolia block 12,105,936; the RLP header travels with the request, 20,663 cycles
and 691 bytes): storage slot 278,253, `Transfer` 96,713, account 169,611 cycles
([`attestation.md`](design/attestation.md) §4.7). Accumulator leaves in the guest, on a synthetic state
holding the vault's deposits: slots 0 and 1 alone 120,879 cycles, about 65,500 cycles and 1,072 bytes per
leaf (all eight leaves 644,826) ([`lez/RESULTS.md`](../lez/RESULTS.md) §6). Without the accelerated Keccak
the mainnet statements cost 0.34M to 2.1M cycles, and 8.2M for the 306-log receipt
([`attestation.md`](design/attestation.md) §13).

**Component responsible for the only near-limit case:** the receipts trie leaf is the whole receipt, so a
log's cost follows its receipt's size; a receipt several times larger than 58.8 KB would not fit the
target ([`lez/RESULTS.md`](../lez/RESULTS.md) §5, "What is not built").

### P.2 Cost by program action: cycles, gas, proof time, proof size

**Advancing the tracked state** (in-process harness, mainnet periods 1899–1903;
[`attestation.md`](design/attestation.md) §4.2–§4.3):

| Action | Transactions | Cycles per step | Execution gas (total) | Storage gas | Proof |
|---|---:|---|---:|---:|---|
| `Init` (bootstrap, adoption in two parts) | 3 | largest 3,479,974 | 7,085,450 | 51,968 | none (public execution) |
| finality update, public job | 3 | `Open` 2.13–2.18M, `Miller` 3.31M, `Finish` 3.65M | 9,216,121 | 4,451 | none |
| update that rotates and adopts a committee | 5 | adoption 3.40–3.42M per 256 keys | 16,076,120 | 54,491 | none |
| finality update, one public transaction (`VerifyAll`) | 1 | 9,115,325 | 9,115,325 | 4,089 | none |
| finality update, proven path | 1 | public part 70,090 | 479,854 | 224,063 | 9,045,235 user cycles: **3,240.5 s** laptop CPU, peak 9.18 GiB, receipt **227,080 B** |

The same proven update (a later image) took **49.5 s** on one L4, and **895.6 s** on a cloud
container's CPU with 32 prover threads, where a live sequencer verified and applied it (receipt 237,412 B)
([`lez/RESULTS.md`](../lez/RESULTS.md) §9). Public transactions need no proof on v0.4: every node
re-executes them ([`bridge.md`](design/bridge.md) §7). A handler pays 5.7 cycles per byte it receives and
about 3.4 per byte it sends on ([`attestation.md`](design/attestation.md) §4.3).

**Statement verification by type**: P.1's table. **Proof time and size of an attestation**: the private
attestation of a `Transfer`, 380,577 user cycles for the consumer's two transitions plus LEZ's privacy
circuit, proven on one L4 in **16.1 s**, receipt **227,170 B**, transaction 231,214 B, peak host memory
0.76 GiB ([`lez/RESULTS.md`](../lez/RESULTS.md) §5; [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md),
run `gpu-l4-attest-2`). **Bare guest** (executor): storage 210,502, a log 42,445–432,102, a hop 182,627, an
identifier 1,629 user cycles ([`attestation.md`](design/attestation.md) §5.1).

**Reproducible from the test suite**: `lez/harness/tests/public_path.rs`, `attest_public.rs`,
`attest_private.rs`, `sepolia.rs` and the commands of [`lez/RESULTS.md`](../lez/RESULTS.md) §8 write the
`results/*.md` tables.

**Gaps:** proof time and size per public attestation are not applicable (no proof); a private
attestation was not proven on a CPU; a proven handoff was not proven for real (dev mode only).

### P.3 Tracking cost, apart from attestation cost

Tracking (shared by every consumer of an instance) and attestation (per fact) are reported apart. Per
update: public 9.2M execution gas (16.1M at a handoff), proven 0.70M with execution and storage gas added
([`attestation.md`](design/attestation.md) §10). **Amortised** at the recommended cadence (one update per
sync-committee period, plus one per hour while roots are imported; derived):

| Cadence | Path | Execution gas | Storage gas |
|---|---|---:|---:|
| once per period (a handoff) | public | 16,076,120 per period, about 5.16 billion a year | 54,491 per period |
| once per period (handoff, no new header) | proven | 448,530 per period | 224,063 per period |
| once per hour while importing | public | 221,186,904 per day | 106,824 per day |
| once per hour while importing | proven | 11,516,496 per day | 5,377,512 per day |

Per attestation: the interval's cost divided by the attestations its records serve. **Measured in
operation:** the lab followed Sepolia and mainnet on a live sequencer every two hours; its program
transactions used 208,951 to 3,731,712 execution gas ([`lez/lab/README.md`](../lez/lab/README.md)). **Gap:** the figure at the
recommended cadence with real proving and recorded hardware (milestone A4).

### P.4 User proving (RFP-021)

Declared minimum for user-side proving: **16 GB of memory and 8 cores, x86-64 or Apple Silicon**
([`attestation.md`](design/attestation.md) §13). Measured: the private attestation on one L4, 16.1 s,
0.76 GiB host memory (P.2); on CPUs, in cloud containers held to 8 and to 16 CPUs with 16 GiB hard (AMD Zen 3
servers), **904.3 s** and **497.3 s** (medians of 3), 9.25 GiB peak: 2,355,666 user cycles, of which LEZ's
privacy-preserving circuit takes 1,975,089 and 85 % of the time ([`p4-minimum`](../lez/results/p4-minimum/README.md),
[EVIDENCE](EVIDENCE.md) row 32). For scale, the cheapest private transaction of LEZ's own wallet takes 991.7 s and
488.6 s on the same shapes (row 34). Modal's 8 CPUs are 8 hardware threads; an 8-core x86 laptop usually has 16.
The update proof is an updater's, not a user's (9.18 GiB peak). **Gaps:** a mid-range laptop and macOS; the
attestation as a client that pads like LEZ's wallet would send it (*estimated* +1.0M cycles, about 10 min at
16 CPUs); a chain of hops (a 320-hop claim is estimated at about 58M accelerated user cycles).

### P.5 End-to-end attestation latency

| Component | Figure | Basis |
|---|---|---|
| Ethereum finality wait | about 13–19 minutes | design ([`attestation.md`](design/attestation.md) §13) |
| update availability | the operator's cadence: hourly in the design while importing; every two hours in the lab | design; [`lez/lab/README.md`](../lez/lab/README.md) |
| the job on LEZ | 3 transactions (5 at a handoff); one block each at the target; 10-s blocks | [`attestation.md`](design/attestation.md) §4.2; lab |
| on-chain verification | one transaction, same block as the action (P.1) | measured |

**Gap:** not measured end to end; deferral adds a random share of the chosen window (operator guide §3.4).

### P.6 Off-chain compute

| Component | Resources measured | Source |
|---|---|---|
| the lab zone (sequencer, Bedrock node, driver) | two physical cores requested; a session of 6–11 transactions 123–133 s; state 20–31 MB compressed per generation; 0.0064 USD per scheduled session, about 0.08 USD a day (derived) | [`lez/lab/README.md`](../lez/lab/README.md) |
| proven updater | 9.18 GiB, 54 min laptop CPU; 49.5 s one L4 | P.2 |

**Gap:** the updater daemon and `monitor` (not built): CPU, memory, bandwidth.

### P.7 State growth

| State | Size | Grows with | Source |
|---|---:|---|---|
| `header(id, slot)` record | 333 B of actor state (293 before the chain id and parent hash) | one per update that finalizes a new header | [`lez/RESULTS.md`](../lez/RESULTS.md) §5, "Decisions" 2 |
| `instance(id)` | 425–505 B | constant (committees as 80-byte summaries) | [`attestation.md`](design/attestation.md) §3.1 |
| `job(id, job_id)` | 1,249–1,409 B | cleared by `Finish`; a failed job stays (clean-up not built) | [`attestation.md`](design/attestation.md) §3.1, §4.5 |

**Projection (derived)** at the recommended cadence, one record per hour while importing (8,760 a year)
plus one per period (321): about 9,081 records, **3.0 MB a year, 15.1 MB over five years** of record
state. LEZ v0.4 charges storage gas on transaction bytes, not on state written, so growth costs nothing
per byte in fees; account overhead per record is not counted. **Gap:** a measured projection (milestone A4).

### P.8 BLS: in-program against a hypothetical precompile

The two pairing steps (`Miller` 3.31M, `Finish` 3.65M) are 7.0M of an update's 9.2M public-path cycles
(in-process harness); in the bare guest, Miller loops 40%, final exponentiation 35% and hash to G2 11% of
10.21M ([`attestation.md`](design/attestation.md) §13). So a precompile that made pairing free would save
at most about 7.0M of 9.2M per update (derived upper bound). **A precompile is reachable only from public
transactions**: the proven path, which runs in a private actor, would not benefit, and it already costs
0.48M execution gas. **Gap:** the delta against a modelled precompile cost.

### P.9 No action scales with accumulated state

By construction: no action reads a variable-length collection; records, roots and nullifiers are addressed
by key; committees are fixed-size summaries ([`attestation.md`](design/attestation.md) §5.6). The lab's
program transactions used 208,951 to 3,731,712 execution gas over its first 202 blocks
([`lez/lab/README.md`](../lez/lab/README.md)), not analysed against state size. **Gap:** no measurement of cost against state
size.

---

## RFP-022

### P.1 Claim and mint in one LEZ public transaction

**Met.** In-process harness, the merged bridge program (time fix and security fixes; image `cf41b7e5…` at the
fifth integration pass's path), synthetic vault storage (tables `lez/results/bridge-*.md`, regenerated at b93a945):

| Claim | Handlers (cycles each) | Execution gas | Storage gas |
|---|---|---:|---:|
| plaintext, 1 ETH, fee schedule 0 | Claim 107,479 → Issue 52,100 → Admit 33,886 → Spend 30,300 → Mint 17,560 → Mint 17,560 | 258,885 | 2,053 |
| plaintext, 10 ETH, mint and relay fees | as above, three Mints (17,560; 17,560; 22,280) | 287,943 | 2,053 |
| plaintext, under a Gloas record | — | 254,999 | 2,053 |
| plaintext, on the v5 vault's real storage | — | 254,920–294,144 | — |
| plaintext on a live sequencer (the frozen earlier program, stand-in beacon chain) | — | 142,432 | — |
| private, to a private destination | private: ClaimPrivately 41,160 + reply 111,596 user cycles; public: Read, Issue, Admit, Spend, Mint | 551,502 (409,764 + 141,738) | 224,063 |
| private, to a public destination | the same private part; public part with three Mints | 600,607 (the test's output: `bridge_private.rs`, `a_private_claim_to_a_public_destination_and_the_refusals`) | 224,063 |

Breakdown by component: the claim statement (path, opening of `C`, nullifier) runs in `Claim`; the
controls in `Issue` (denomination, fee schedule, token freeze and bucket) and `Admit` (global freeze and
bucket); `Spend` writes the nullifier; one `Mint` per payee. The controls, attested time and the security fixes
cost +116,441 against the program without them (×1.82). The bridge's own claim statement in a bare guest is 74,379 user cycles, constant
over 64 witnesses ([`bridge.md`](design/bridge.md) §18.3). Sources: [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md)
§1–§3, §8a, §11.

### P.2 Additional LEZ verification and its amortised cost per deposit

The root import is the only additional verification (the claim needs no Ethereum proof of its own):

| `ImportRoot` | Execution gas | Storage gas | Where |
|---|---:|---:|---|
| 8 leaves, synthetic storage | 231,365 | 2,025 | in-process harness |
| the v5 vault's real storage, 3 and 6 leaves | 277,263; 278,099 | — | in-process harness |
| under a Gloas record, with the header | 262,007 | 2,612 | in-process harness |
| live sequencer, stand-in beacon chain | 232,278 | 2,460 | live sequencer |
| Sepolia's real state: the live vault, one leaf, capture 1 (the frozen program) | 426,479 | 6,646 | live sequencer ([`lez/live/inbound/README.md`](../lez/live/inbound/README.md)) |

**Amortised per deposit (derived)**: an import's cost divided by the deposits it newly covers, so with one
import an hour: about 278k per deposit at one deposit an hour, 27.8k at ten, 2.8k at a hundred (from the
real-storage figure). The import also needs a record, whose tracking cost is RFP-021 P.3's, shared with
every other consumer. Sources: [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §1, §2a, §8a;
[`lez/live/inbound/README.md`](../lez/live/inbound/README.md). **Gap:** an import under a mainnet state
root (estimated at roughly 275k cycles of attestation plus one slot proof; not measured).

### P.3 Ethereum gas: deposit, release, checkpoints

| Transaction | Local EVM, v5 vault | Sepolia node, v3 vault |
|---|---:|---:|
| `depositETH`, first by an address | 179,275 (mock-verifier state); 198,427 (first of the tree) | 552,152 (first of the tree) |
| `depositETH`, later | 159,285 | 260,526 |
| `deposit` ERC-20, first by an address | 197,762 | 375,237 (the token's first deposit, the address's third) |
| `release` ETH, real verifier | 373,657 | 619,477 |
| `release` ERC-20 with a relayer's fee, real verifier | 403,523 | 648,447 |
| rule (A) checkpoint: `submitCandidate` + `recordBeacon` + `finalize`, real verifier | 438,444 + 77,937 + 457,965 = **974,346** | 1,381,770 + 262,063 + 1,183,452 = **2,827,285** (derived sum) |
| rule (B) checkpoint: `submitCandidate` + `finalizeAttested`, real verifier | 347,820 + 195,149 = **542,969** | not measured |
| `registerToken`, three denominations | 647,208 | 2,136,791 (v3 vault: 472,211 on Foundry) |
| `refund` ETH / ERC-20 | 179,155 / 205,540 | not measured |

Sources: [`vault/GAS.md`](../vault/GAS.md) ("Results with the mock verifier", "v5 with the real
verifier"); [`e2e/README.md`](../e2e/README.md) ("What the round trip costs on today's Sepolia": all 16
transactions 62,096,366 gas on Sepolia against 11,030,062 on Foundry, ×5.63). Checkpoint costs are paid
once per finalised LEZ state, not per burn. Against the M0 thresholds (release ≤ 450k, candidate and
finalisation ≤ 500k, [`docs/traceability.md`](traceability.md) P.3) every v5 row is under them on Foundry's
rules; on Sepolia's state-gas rules none of the proof-verifying calls is. Mainnet has not scheduled
Glamsterdam, so it still charges the Foundry column ([`e2e/README.md`](../e2e/README.md)). **Gaps:** the v5
vault on Sepolia's rules; a later (not first) candidate with the real verifier.

### P.4 User proving (RFP-022)

| Proof | Figure | Where | Source |
|---|---|---|---|
| release (succinct), earlier image | 80,501 user cycles, constant over 64 witnesses; **37.1–48.6 s**, peak 1.46 GB, receipt 223,930 B (up to 150 s under load) | laptop CPU | [`bridge.md`](design/bridge.md) §18.2 |
| release, current image (`NOut` v2, review M-3) | 77,736 user cycles, constant over 64 witnesses (executor); proven and wrapped in the crown rehearsal | build box (executor); cloud | [`evidence-suites.txt`](evidence-suites.txt); [`crown-rehearsal-3`](../e2e/runs/crown-rehearsal-3/README.md) |
| release, end-to-end run | 15.1 s and 14.7 s | 16 cores | [`bridge.md`](design/bridge.md) §18.7 |
| release, current image: the holder's succinct proof | **46.0 s** at 8 CPUs, **26.4–27.0 s** at 16; peak 1.5 GiB; receipt 223,930 B | cloud containers held to 8 / 16 CPUs and 16 GiB (AMD Zen 3) | [`p4-minimum`](../lez/results/p4-minimum/README.md) |
| its Groth16 wrap (any relayer's) | `identity_p254` 68.8–70.8 s / 39.3 s, then the prover image 18.8–23.7 s / 15.6–16.6 s; 7.6 GiB at most; seal 260 B | the same shapes; the prover image in a gVisor container | [`p4-minimum`](../lez/results/p4-minimum/README.md) |
| Groth16 wrap of a release | 77.5–79.9 s; about 7.3 GiB; seal 260 B; **x86 only** | laptop CPU | [`bridge.md`](design/bridge.md) §18.2 |
| claim statement (the bridge's part) | 74,379 user cycles; 38.90 s | laptop CPU | [`bridge.md`](design/bridge.md) §18.3 |
| full LEZ private claim (earlier image) | **15.3 s**; receipt 227,477 B; 0.78 GiB host for claim and burn | one L4 | [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §7 |
| full LEZ private claim, LEZ's wallet (earlier program) | **254.8 s**; transaction 252,185 B | a cloud build box's CPU | [`lez/live/inbound/README.md`](../lez/live/inbound/README.md) |
| private claim and private burn of the merged program, LEZ's wallet on a devnet | **276.8 s** and **336.0 s**, proving and sending | a cloud CPU container, 16 cores reserved, up to 32 | [`crown-rehearsal-3`](../e2e/runs/crown-rehearsal-3/README.md) |
| private burn (earlier image) | **12.4 s**; receipt 226,978 B | one L4 | [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §7 |
| private claim and private burn of the merged program, as this harness builds them | claim **905.4 s** / **463.7 s**, burn **756.3 s** / **389.0 s** at 8 / 16 CPUs (medians of 3; at 16 one run each on an Intel Xeon, 371.4 s and 310.6 s); peak 9.24 GiB | cloud containers held to 8 / 16 CPUs and 16 GiB | [`p4-minimum`](../lez/results/p4-minimum/README.md), [EVIDENCE](EVIDENCE.md) row 33 |
| LEZ's own wallet: its cheapest private transaction (the yardstick) | **991.7 s** / **488.6 s** at 8 / 16 CPUs; 2,851,842 user cycles | the same | [`p4-minimum`](../lez/results/p4-minimum/README.md), row 34 |
| recovery scan | median 128.9 ms per 100,000 deposits, 1,308.6 ms per 1,000,000 | laptop CPU, one thread | [`bridge.md`](design/bridge.md) §18.3 |

Declared minimum: 16 GB and 8 cores, x86-64 or Apple Silicon. The release's succinct proof needs the holder's burn
secret `sOut`, so it is the user's; the wrap takes only the succinct receipt, so any relayer can make it
([`bridge.md`](design/bridge.md) §12.2, §12.3). **Gaps:** a mid-range laptop and macOS; the claim and burn as a client
that pads like LEZ's wallet would send them (*estimated* +1.0M cycles each); the current program image on a GPU; the
wrap on Apple Silicon (not supported today).

### P.5 Latency: deposit to mint, burn to release

**Inbound** (deposit to mint): Ethereum finality (about 13–19 min) + the next light-client update and root
import (the operators' cadence; hourly in the design) + the privacy delay the client proposes + one claim
transaction. Not measured end to end. On the live sequencer the stand-in crossing ran `Init`, two imports
and both claims; its duration is not a latency figure (stand-in beacon chain)
([`lez/live/inbound/README.md`](../lez/live/inbound/README.md)).

**Outbound** (burn to release):

| Component | Rule (B) | Rule (A) | Basis |
|---|---|---|---|
| LEZ finality and proving | proof of the blocks: 24.1 s per idle hour, 219.3 s per busy hour on one L4; wrap about 3.5 min | the same, twice | measured ([`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md); [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md)) |
| the vault's wait | 1 h | `W` = 3 h 16 min (`n` = 3) to 4 h 31 min (`n` = 10) at the default floor | design ([`bridge.md`](design/bridge.md) §11.6) |
| tip to finalisation | about 3 h | 7–22 h; **4.00–7.68 h median** in simulation (`n` = 3 to 10) | estimated; simulated ([`bridge.md`](design/bridge.md) §9, §11.7) |
| release delay | ≥ 24 h outside testnets (0 allowed on testnets) | the same | design ([`bridge.md`](design/bridge.md) §13) |
| the user's release proof and a relayer's wrap | 46.0 s at 8 CPUs, 26.7 s at 16, plus 55–93 s of wrap (about 40 s + 80 s on a laptop, earlier image) | the same | measured (P.4) |
| privacy delay | the client's choice | the same | design |

The rehearsed testnet round trip (wait 40 min, release delay 120 s) runs in about 55 minutes plus the queue
for a GPU (`e2e/sepolia-live.sh`, [`vault/README.md`](../vault/README.md)); the cloud run, with the
deposits' finality waits, in about an hour and three quarters ([`e2e/README.md`](../e2e/README.md)).

### P.6 Off-chain compute (RFP-022)

| Component | CPU | Memory | Time | Source |
|---|---|---|---|---|
| checkpoint prover | one L4 GPU, one CPU core | about 10.9 GiB GPU, 0.76–1.08 GiB host | 14.0 s (fixed part), 24.1 s (idle hour), 41.4 s (5 private tx), 219.3 s (50 private tx) | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md) |
| the same on a laptop CPU | 16 threads | 9.62 GB peak | 1,157.5–1,758.9 s fixed part; ≈ 34 min per idle hour and ≈ 4.8 h per busy hour (extrapolated) | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) |
| rule (B) checkpoint | one L4 | — | 31.9 s (idle hour, `n` = 4, 9 segments) | [`bridge.md`](design/bridge.md) §18.5 |
| Groth16 wrap of a checkpoint | x86 | 7.3 GiB | 195.1–215.2 s (laptop); recursion 18.7 s on 16 cores + 14 s Groth16 (end-to-end run) | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md); [`bridge.md`](design/bridge.md) §18.7 |
| cloud cost of the GPU proofs | — | — | 385 L4-seconds billed for four checkpoint proofs and an update: about 0.09 USD of GPU | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md), "Cost" |

**Gap:** the relayer, importer and `monitor` daemons (not built).

### P.7 State growth at 1M and 10M operations

What accumulates, per operation ([`vault/README.md`](../vault/README.md), "Storage-slot contract";
[`lez/BRIDGE.md`](../lez/BRIDGE.md) §2; type definitions in [`lez/bridge-wire/src/lib.rs`](../lez/bridge-wire/src/lib.rs)):

| Chain | State | Per | Size |
|---|---|---|---|
| Ethereum | the deposit tree | deposit | none: the incremental frontier is 32 fixed slots |
| Ethereum | deposit counter | new depositor address | one slot |
| Ethereum | used nullifier `NOut` | release | one slot |
| Ethereum | refunded leaf | refund | one slot |
| Ethereum | checkpoint history | finalised checkpoint | a few slots, not per burn |
| LEZ | `root(cid, n)` | import | 117 B (measured, [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §3) |
| LEZ | `nullifier(chain, vault, N)` | claim | 77 B (estimated from the type: `NullifierState` borsh, 32 + 20 + 8 + 16, plus the enum tag) |
| LEZ | `burn(cid, H(note))` | burn | 185 B (estimated: `BurnRecord`, 32 + a 152-byte note, plus the tag) |
| LEZ | holder state | holder | grows with the configurations and tokens it holds |

The estimate method reproduces the measured root state (116 bytes of fields plus the tag = 117).
**Projection (derived; account overhead not counted):**

| Operations | Ethereum, new slots | LEZ nullifier accounts | LEZ burn accounts |
|---|---|---:|---:|
| 1M claims and 1M releases | 1M `NOut` slots (32 MB of values), plus one per new depositor | 77 MB | — |
| 1M burns | — | — | 185 MB |
| 10M of each | 10M slots (320 MB of values) | 770 MB | 1.85 GB |

Roots grow with the import cadence, not with deposits: 8,760 a year at one an hour, about 1.0 MB a year.
On Sepolia's Glamsterdam rules each new Ethereum slot costs 97,920 state gas, already inside the release
and deposit figures of P.3. LEZ's own private-state commitments and nullifiers for each private
transaction are LEZ's and not counted. **Gaps:** measured LEZ account sizes and overhead; the projections
over one and five years with real traffic (milestone B4).

---

## Gaps, in one list

`just bench`; user-side proofs on a laptop and on macOS (cloud CPUs held to 8 and 16 CPUs: P.4), and as a client
that pads like LEZ's wallet; latency measured end to end on either leg; the
recommended-cadence tracking cost with real proving; daemon resources; the BLS precompile delta; cost
against state size; the v5 vault on Sepolia's gas rules; an import under a mainnet state root; LEZ account
overhead. Every one is a milestone deliverable ([`attestation.md`](design/attestation.md) §13;
[`bridge.md`](design/bridge.md) §18.6; [`docs/traceability.md`](traceability.md)).
