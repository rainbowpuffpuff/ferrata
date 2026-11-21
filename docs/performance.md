# Ferrata: performance report (RFP-021 P.1–P.9, RFP-022 P.1–P.7)

**First draft, 11 November 2026.** Each requirement with the measured figures, where and how they were
measured, and the gaps. Paths are relative to the repository root. `just bench` regenerates the vault's gas, the light
client's cycles and a real release proof from a fresh checkout and records the hardware ([EVIDENCE](EVIDENCE.md)
row 49; `just bench-user`, the user-side proofs at the declared minimum, row 50); every other number comes
from the test, script or run named beside it ([`docs/design/attestation.md`](design/attestation.md) §13). [`docs/EVIDENCE.md`](EVIDENCE.md) lists the
evidence with its commands.

## Where things were measured

| Label | What it is | Sources |
|---|---|---|
| **executor** | RISC Zero zkVM 3.0.5's executor: user cycles, deterministic, no proof | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md); [`attestation.md`](design/attestation.md) §5.1 |
| **in-process harness** | LEZ v0.4.0-rc1's own state transition functions, called in-process with LEZ's fee settlement copied; one transaction per block, genesis base fees, no base-fee movement; private parts in dev mode (executed, not proven) unless a real proof is named | [`lez/RESULTS.md`](../lez/RESULTS.md) §1; [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) |
| **live sequencer** | LEZ's own `sequencer_service` at `v0.4.0-rc1` with a Logos Blockchain node (one validator, leader proofs in dev mode) on a cloud container; charges re-checked by replaying the chain through LEZ's `chain_state` | [`lez/RESULTS.md`](../lez/RESULTS.md) §9; [`lez/live/inbound/README.md`](../lez/live/inbound/README.md); [`lez/lab/README.md`](../lez/lab/README.md) |
| **local EVM** | Foundry (forge 1.3.6, solc 0.8.26, 200 runs, cancun), `forge test --isolate`: whole transactions under today's mainnet gas rules | [`vault/GAS.md`](../vault/GAS.md) |
| **Sepolia node** | each transaction priced by a public Sepolia node (reth 2.10.0) with `eth_estimateGas` / `eth_simulateV1` on a fork's state: Glamsterdam's gas rules, nothing sent | [`e2e/README.md`](../e2e/README.md); [`vault/GAS.md`](../vault/GAS.md), "Creating the contracts on today's Sepolia" |
| **Sepolia (live)** | receipts (`gasUsed`) of the transactions the live round trips sent on Sepolia, read back with `cast receipt`: `crown-live-3` on the final programs, `crown-live-1` on the first run's | [`e2e/runs/crown-live-3/README.md`](../e2e/runs/crown-live-3/README.md); [`e2e/runs/crown-live-1/README.md`](../e2e/runs/crown-live-1/README.md); [`vault/GAS.md`](../vault/GAS.md), "The v5 round trip, live on Sepolia" and "The v5 round trip on the final programs" |
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
§5, `results/stage4-attest.md`, `results/stage4-attest-consumer.md`; measured on the images that file names, program
`9f3eb00f…`; with this tree's programs the private consumer's read is charged 422,032,
`results/stage4-attest-private-public-outputs.txt`):

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

**One public transaction with a residue witness (experimental, not deployed).** An unaudited copy of
the program whose `VerifyAll` checks a residue witness (Novakovic and Eagen; `c ≠ 0`, `w ∈ Fp6`,
`f·w == c^λ`) instead of computing the final exponentiation was charged **6,629,246** execution gas
for the same finality update in one transaction (66.3% of `MAX_GAS_EXEC`), against 9,124,896 for the
deployed `VerifyAll` in the same build (in-process harness). Bare guest, one whole update: 6,573,265
user cycles (5,911,714 with the witness folded into the Miller loop); at the fewest signers the
policy accepts (342 of 512) 10,809,028 (10,147,477 folded), and an update that adopts a committee
11.9M to 16.8M: over the limit either way ([`bench/RESIDUE.md`](../bench/RESIDUE.md); EVIDENCE row
127). With the non-signers' keys checked through one Merkle multiproof instead of a branch each (a
non-signer then costs 9,418 cycles instead of 25,094), the 342-signer finality update fits at every
placement of the absent members that was measured: the experimental copy was charged **8,318,645**
execution gas for it in one transaction with every third member absent (83.2%) and 8,326,572 (83.3%)
at the costliest placement found (the maximum over all placements is not established; about 1.67M
gas is left to the limit), and the bare guest takes 7,458,783 folded with every third absent; with a
branch per non-signer the fewest signers that fit are 386
(EVIDENCE row 128, [`bench/RESIDUE.md`](../bench/RESIDUE.md) §9). A period update (a committee handoff) does not
fit with the next committee's 512 keys adopted (5,991,991 cycles for the adoption alone); adopted by its summary
instead (its key root and aggregate key as the update's branch proves them in the signed attested state, which is
sound only under the honest-supermajority assumption the light client already makes, `bench/RESIDUE.md` §10), it
fits one transaction: the experimental copy was charged **6,711,485** execution gas (67.1%) for mainnet update_1 as a
handoff and **8,431,403** (84.3%) for a 342-signer handoff at the costliest placement found, and the LEZ v0.3.1 port
takes every handoff of the mainnet replay and of Sepolia's 32 periods in one transaction, at most 6,737,794
(EVIDENCE row 137). The deployed programs are unchanged.

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
transactions used 208,951 to 3,733,713 execution gas ([`lez/lab/README.md`](../lez/lab/README.md)). **Gap:** the figure at the
recommended cadence with real proving and recorded hardware (milestone A4).

### P.4 User proving (RFP-021)

Declared minimum for user-side proving: **16 GB of memory and 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB**
([`attestation.md`](design/attestation.md) §13). Measured: the private attestation on one L4, 16.1 s,
0.76 GiB host memory (P.2); on CPUs, in cloud containers held to 8 and to 16 CPUs with 16 GiB hard (AMD Zen 3
servers), **904.3 s** and **497.3 s** (medians of 3), 9.25 GiB peak: 2,355,666 user cycles, of which LEZ's
privacy-preserving circuit takes 1,975,089 and 85 % of the time ([`p4-minimum`](../lez/results/p4-minimum/README.md),
[EVIDENCE](EVIDENCE.md) row 32). For scale, the cheapest private transaction of LEZ's own wallet takes 991.7 s and
488.6 s on the same shapes (row 34). Modal's 8 CPUs are 8 hardware threads; an 8-core x86 laptop usually has 16.
Padded as LEZ's wallet pads every private transaction (7 private actions, 512-byte notes, the fee's recovery note:
what a client sends), the attestation takes **1,236.8 s** and **633.3 s** on the same shapes (medians of 2), LEZ's
circuit 979,362–980,875 user cycles more (4 segments, was 3), the same 9.25 GiB peak (row 38). Under macOS 15.8 on
Apple Silicon (Mac minis, CPU only: r0vm 3.0.5 has no Metal prover), padded: **857.4 s** on an M1 with 16 GB
and **475.2 s** on an M4 (medians of 2), r0vm peak 9.26–9.45 GiB, no swap-out on the M1
([`p4-macos`](../lez/results/p4-macos/README.md), row 88). The update proof is an updater's, not a user's
(9.18 GiB peak). **This tree's final programs** (image `0x816d1fef…`, consumer `0x8464603b…`), padded, in containers
held to a mid-range laptop's shape (8 CPUs of time, 16 GiB hard) and to 16 CPUs: **1,254.8 s** (1,250.0 and 1,259.6 s)
and **633.5 s**, 3.34M user cycles in 6 segments, r0vm peak **9.19 GiB** (container 9.23 GiB), receipt 239,246 B,
transaction 255,366 B, each settled with the receipt verified and refused with one bit flipped
([`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md), 21 November 2026). **Gaps:** a run on a real mid-range
laptop (the owner's step: `bench/perf/p4-laptop-owner.sh` on the same staged binaries, about 45 minutes); a chain of
hops (a 320-hop claim is estimated at about 58M accelerated user cycles).

### P.5 End-to-end attestation latency

**Measured end to end** on the live run of 12–13 November 2026 (EVIDENCE row 99): `ferrata-updater` following Sepolia
through Lodestar's public light-client API on a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain node, every
transaction read back final on Bedrock ([`lez/results/perf-latency-e2e.md`](../lez/results/perf-latency-e2e.md),
`python3 -I bench/perf/latency_e2e.py lez/updater/results/live-sepolia`). An event in the block at slot *e* becomes a
statement usable on LEZ when the first applied update whose finalized header is at or after *e* has its `Finish` final,
plus the consuming transaction (verification, P.1). Every slot of the run's 5.4 hours of continuous operation (1,632
Sepolia slots, 51 updates; the daemon applied every finalized epoch) is taken as an event's block:

| | median | p10 | p90 | max |
|---|---:|---:|---:|---:|
| **Ethereum event to statement usable on LEZ** | **18.1 min** | 15.6 min | 20.7 min | 22.1 min |
| event to the next finalized header's slot | 3.1 min | 0.6 min | 5.6 min | 6.4 min |
| finality wait: that header to the signature slot of the update proving it | 13.0 min | 13.0 min | 13.0 min | 13.2 min |
| update availability: the beacon node serving it, the daemon's poll | 0.2 min | 0.1 min | 0.4 min | 1.0 min |
| the update applied on LEZ (Open, Miller, Finish, each final on Bedrock) | 1.5 min | 1.5 min | 1.6 min | 1.6 min |
| on-chain verification: the consuming transaction, send to final | 0.3 min | | | |

The verification row is the run's own median send-to-final time of a transaction (15.7 s over 159 transactions; P.1:
the statement is verified inside that transaction). With the run's two outages included (the zone stopped when its
Bedrock funds ran out) the median is 19.6 min and the p90 86.7 min. At a slower cadence the header wait grows: an
operator updating once an hour adds up to 60 minutes to it (*derived*); deferral (R.10) adds a random share of the
window, measured on a sequencer in [`perf-gate-sequencer.md`](../lez/results/perf-gate-sequencer.md).

### P.6 Off-chain compute

| Component | Resources measured | Source |
|---|---|---|
| the lab zone (sequencer, Bedrock node, driver) | two physical cores requested; a session of 6–11 transactions 123–133 s; state 20–31 MB compressed per generation; 0.0064 USD per scheduled session, about 0.08 USD a day (derived) | [`lez/lab/README.md`](../lez/lab/README.md) |
| proven updater | 9.18 GiB, 54 min laptop CPU; 49.5 s one L4 | P.2 |

**Gap:** the CPU, memory and bandwidth of the updater daemon, and the CPU and bandwidth of `monitor` (both built,
[EVIDENCE](EVIDENCE.md) rows 43–45, 79–81; of `monitor` only memory is measured: 34,216 kB resident, row 43).

### P.7 State growth

**Measured** in LEZ v0.4.0-rc1's own state ([`lez/results/perf-state.md`](../lez/results/perf-state.md), `bash
bench/perf/state.sh`), whose borsh serialization is what a LEZ node stores (its state breakpoints), each account its
32-byte key and its `Account`:

| account | actor state | `Account` | in the serialized state | grows with |
|---|---:|---:|---:|---|
| `header(id, slot)` record | 365 B | 421 B | **453 B** (measured: +453.0 B per record from 0 to 100,000 records) | one per applied update that finalizes a new header |
| `instance(id)` | 505 B | 561 B | 593 B | constant (committees as summaries) |
| `job(id, job_id)` | 1,022 B | 1,078 B | 1,110 B while open | cleared by `Finish` |

**Projections** ([`perf-projections.md`](../lez/results/perf-projections.md), `python3 -I bench/perf/projections.py`;
*derived*: the measured size times a count):

| cadence | records a year | one year | five years |
|---|---:|---:|---:|
| as the lab ran (measured: 106 jobs in 8.06 days on mainnet, 9 of them period updates) | 4,804 | 2.18 MB | 10.88 MB |
| the design's: one update an hour while roots are imported, plus each period's | 9,087 | 4.12 MB | 20.58 MB |
| every finalized epoch, as the live Sepolia daemon ran | 82,181 | 37.23 MB | 186.14 MB |

LEZ v0.4 charges storage gas on a transaction's bytes, not on the state it leaves, so the growth costs nothing in fees:
it is disk on every node. A failed job's account stays until cleaned (clean-up not built).

### P.8 BLS: in-program against a hypothetical precompile

**Modelled from measured inputs** ([`lez/results/perf-precompile.md`](../lez/results/perf-precompile.md), `python3 -I
bench/perf/precompile_model.py bench/perf/results`). The program's BLS work for a finality update (mainnet update_1,
510 of 512 signers), in this tree's program: hash-to-G2 1,169,105 and the subtractive key aggregation 285,875 cycles
inside `Open` (bench/RESULTS.md), the two Miller loops 3,308,048 (the `Miller` step), the final exponentiation and check
3,581,111: **8,344,139 of the update's 9,228,905 cycles**. The model prices a native precompile at time parity: the
operation's native time (blst 0.3.16 on the build box's CPU, one thread, median of 300: hash-to-G2 228 µs, the two
Miller loops 651 µs, the final exponentiation 425 µs) times the executor's own throughput on the same CPU (11.3–14.5
Mcycles/s, measured on these steps), so that a node spends the same time per unit of gas inside and outside the zkVM.

| step | in-program cycles (BLS) | precompile, modelled | delta |
|---|---:|---:|---:|
| `Open`: hash-to-G2, key aggregation | 1,454,980 | 2,652 | 1,452,328 |
| `Miller`: two Miller loops | 3,308,048 | 9,003 | 3,299,045 |
| `Finish`: final exponentiation, check | 3,581,111 | 6,187 | 3,574,924 |
| **finality update** | **8,344,139** | **17,841** | **8,326,298** |

With the precompile a finality update would meter about 902,607 cycles instead of 9,228,905 (10.2 times less, and one
transaction instead of three); a handoff about 845,451 instead of 15,997,954 (the adoption of 256 keys natively 269 µs).
At ten times the native cost the conclusions stand. **A precompile is reachable only from public transactions**: a
private transaction's program runs inside the zkVM proof, where only RISC Zero's accelerators exist (the program already
uses RISC Zero's accelerated blst), so the proven path and every private consumer would not change. Reference point:
Ethereum's EIP-2537 charges 102,900 gas for the pairing check that costs this program 6.9M cycles.

### P.9 No action scales with accumulated state

**Measured** ([`lez/results/perf-state.md`](../lez/results/perf-state.md) §2, §3): with 0, 1,000, 10,000 and 100,000
records held by the instance (the serialized state growing from 2.8 MB to 48.1 MB), five statement types (an account, a
storage slot with a predicate, a `Transfer` log, a log in the block's largest receipt, a slot through one EIP-2935 hop)
and every step of a handoff and of a finality update were charged the same execution and storage gas at every size
(asserted, not only printed): for example 275,865 for the storage slot and 3,308,048 for `Miller` each time. By
construction no action reads a variable-length collection (records, roots and nullifiers are addressed by key). In
operation, the lab's 98 mainnet finality updates over eight days were charged within 1.2% of their median as records
accumulated ([`perf-tracking.md`](../lez/results/perf-tracking.md)). The node's own work is not charged: serializing
the state took 0.002 s at 2.8 MB and 0.228 s at 48.1 MB (the breakpoint a node writes, not a transaction's cost).

---

## RFP-022

### P.1 Claim and mint in one LEZ public transaction

**Met.** In-process harness, this tree's bridge program (image `ccde6171…`, the same built at any path:
[`image-ids.md`](image-ids.md)), synthetic vault storage (tables `lez/results/bridge-*.md`, recorded on 12 November 2026):

| Claim | Handlers (cycles each) | Execution gas | Storage gas |
|---|---|---:|---:|
| plaintext, 1 ETH, fee schedule 0 | Claim 107,702 → Issue 51,166 → Admit 35,565 → Spend 30,916 → Mint 18,161 → Mint 18,161 | 261,671 | 2,053 |
| plaintext, 10 ETH, mint and relay fees | Claim 107,702 → Issue 51,586 → Admit 35,565 → Spend 37,291 → three Mints (18,161; 18,161; 22,496) | 290,962 | 2,053 |
| plaintext, under a Gloas record | — | 257,785 | 2,053 |
| plaintext, on the v5 vault's real storage | — | 257,706–296,766 | — |
| plaintext on a live sequencer (the frozen earlier program, stand-in beacon chain) | — | 142,432 | — |
| private, to a private destination | private: ClaimPrivately 41,736 + reply 110,862 user cycles; public: Read, Issue, Admit, Spend, Mint | 553,641 (409,764 + 143,877) | 224,063 |
| private, to a public destination | the same private part; public part with three Mints | 603,981 (the test's output, 12 November: `bridge_private.rs`, `a_private_claim_to_a_public_destination_and_the_refusals`) | 224,063 |

Breakdown by component: the claim statement (path, opening of `C`, nullifier) runs in `Claim`; the
controls in `Issue` (denomination, fee schedule, token freeze and bucket) and `Admit` (global freeze and
bucket); `Spend` writes the nullifier; one `Mint` per payee. The controls, attested time and the security fixes
cost +119,227 against the program without them (×1.84; with B2's roles and milestones A1 and A2). The bridge's own claim statement in a bare guest is 74,379 user cycles, constant
over 64 witnesses ([`bridge.md`](design/bridge.md) §18.3). Sources: [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md)
§1–§3, §8a, §11.

### P.2 Additional LEZ verification and its amortised cost per deposit

The root import is the only additional verification (the claim needs no Ethereum proof of its own). **Its cost does
not depend on how many deposits it covers**, measured on this tree's bridge program (in-process harness,
[`perf-state.md`](../lez/results/perf-state.md) §4a, `bash bench/perf/state.sh`):

| import | leaves it covers | newly covered | execution gas | storage gas | execution gas per newly covered deposit |
|---|---:|---:|---:|---:|---:|
| vault A, `ImportRoot` n = 5 | 5 | 5 | 230,731 | 1,895 | 46,146 |
| vault A, n = 8 | 8 | 3 | 236,224 | 2,025 | 78,741 |
| vault A, n = 6 (a lagging count) | 6 | 0 | 236,224 | 2,025 | — |
| vault B, n = 8 | 8 | 8 | 236,202 | 1,918 | 29,525 |

The same import in other settings:

| `ImportRoot` | Execution gas | Storage gas | Where |
|---|---:|---:|---|
| 8 leaves, synthetic storage | 236,224 | 2,025 | in-process harness |
| the v5 vault's real storage, 3 and 6 leaves | 282,103; 282,881 | — | in-process harness |
| under a Gloas record, with the header | 266,879 | 2,612 | in-process harness |
| live sequencer, stand-in beacon chain | 232,278 | 2,460 | live sequencer |
| Sepolia's real state: the live vault, one leaf, capture 1 (the frozen program) | 426,479 | 6,646 | live sequencer ([`lez/live/inbound/README.md`](../lez/live/inbound/README.md)) |
| Sepolia's real state: the v5 vault, one leaf then two, captures 1 and 2 (this tree's program, the live round trip `crown-live-3`) | 432,182; 432,182 | — | crown devnet, LEZ v0.4.0-rc1 ([`e2e/runs/crown-live-3/README.md`](../e2e/runs/crown-live-3/README.md)) |
| the same in the first live run (`integration5`'s program, `crown-live-1`) | 433,528; 429,805 | — | crown devnet, LEZ v0.4.0-rc1 ([`e2e/runs/crown-live-1/README.md`](../e2e/runs/crown-live-1/README.md)) |

**Amortised per deposit at the recommended cadence** (*derived* from the measured figures: one import an hour while
deposits arrive, its cost divided by the deposits of that hour): on Sepolia's real state (432,182, the live round
trip's import of the v5 vault) **432,182 per deposit at one deposit an hour, 43,218 at ten, 4,322 at a hundred**; on
the harness's synthetic storage (236,224) 236,224, 23,622 and 2,362. The import also needs a light-client record,
whose tracking cost is RFP-021 P.3's (127.3M execution gas a day as the lab ran), shared with every other consumer of
the instance. Sources: [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §1, §2a, §8a;
[`lez/live/inbound/README.md`](../lez/live/inbound/README.md). **Gap:** an import under a mainnet state root
(estimated at roughly 275k cycles of attestation plus one slot proof; not measured).

### P.3 Ethereum gas: deposit, release, checkpoints

| Transaction | Local EVM, v5 vault | Sepolia node, v3 vault | **Sepolia (live) receipts, v5 vault** (`crown-live-3`, the final programs; `crown-live-1` where it differs) |
|---|---:|---:|---:|
| `depositETH`, first by an address | 179,275 (mock-verifier state); 198,427 (first of the tree) | 552,152 (first of the tree) | 559,652 (first of the tree) |
| `depositETH`, later | 159,285 | 260,526 | 268,026 (the second; 268,014) |
| `deposit` ERC-20, first by an address | 197,762 | 375,237 (the token's first deposit, the address's third) | not sent |
| `release` ETH, real verifier | 373,657 | 619,477 | 661,632 (the first, with the first fee credits); 465,768 (the second) (661,668; 465,828) |
| `release` ERC-20 with a relayer's fee, real verifier | 403,523 | 648,447 | not sent |
| rule (A) checkpoint: `submitCandidate` + `recordBeacon` + `finalize`, real verifier | 438,444 + 77,937 + 457,965 = **974,346** | 1,381,770 + 262,063 + 1,183,452 = **2,827,285** (derived sum) | — (a rule (B) vault) |
| rule (B) checkpoint: `submitCandidate` + `finalizeAttested`, real verifier | 347,820 + 195,149 = **542,969** | not measured | 729,349 + 844,349 = **1,573,698** (1,573,722) |
| `registerToken`, three denominations | 647,208 | 2,136,791 (v3 vault: 472,211 on Foundry) | 3,165,774 (ETH, with fee schedules) |
| `refund` ETH / ERC-20 | 179,155 / 205,540 | not measured | not sent |

Sources: [`vault/GAS.md`](../vault/GAS.md) ("Results with the mock verifier", "v5 with the real
verifier"); [`e2e/README.md`](../e2e/README.md) ("What the round trip costs on today's Sepolia": all 16
transactions 62,096,366 gas on Sepolia against 11,030,062 on Foundry, ×5.63). Checkpoint costs are paid
once per finalised LEZ state, not per burn. Against the M0 thresholds (release ≤ 450k, candidate and
finalisation ≤ 500k, [`docs/traceability.md`](traceability.md) P.3) every v5 row is under them on Foundry's
rules; on Sepolia's state-gas rules none of the proof-verifying calls is (crown-live-1's live receipts: 1.3 to 3.9
times Foundry's for the same transactions, [`vault/GAS.md`](../vault/GAS.md)). Mainnet has not scheduled
Glamsterdam, so it still charges the Foundry column ([`e2e/README.md`](../e2e/README.md)). **Gaps:** v5's
ERC-20 calls and refunds on Sepolia (the live runs sent ETH only); a later (not first) candidate with the real
verifier.

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
| private claim and private burn of the merged program, padded as LEZ's wallet pads them (what a client sends) | claim **1,146.0 s** / **600.7 s**, burn **988.9 s** / **525.9 s** at 8 / 16 CPUs (medians of 2); LEZ's circuit about 0.98–0.99M user cycles more; peak 9.25 GiB | the same | [`p4-minimum`](../lez/results/p4-minimum/README.md), [EVIDENCE](EVIDENCE.md) row 38 |
| LEZ's own wallet: its cheapest private transaction (the yardstick) | **991.7 s** / **488.6 s** at 8 / 16 CPUs; 2,851,842 user cycles | the same | [`p4-minimum`](../lez/results/p4-minimum/README.md), row 34 |
| private claim and private burn, padded, under macOS (CPU only) | claim **818.7 s** / **453.5 s**, burn **716.5 s** / **396.1 s** on an M1 with 16 GB / an M4 (medians of 2); the release's succinct proof **31.7 s** / **18.4 s**; r0vm peak 9.26–9.45 GiB | Mac minis, macOS 15.8 | [`p4-macos`](../lez/results/p4-macos/README.md), row 88 |
| recovery scan | median 128.9 ms per 100,000 deposits, 1,308.6 ms per 1,000,000 | laptop CPU, one thread | [`bridge.md`](design/bridge.md) §18.3 |

Declared minimum: 16 GB and 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB. The release's succinct proof needs the holder's burn
secret `sOut`, so it is the user's; the wrap takes only the succinct receipt, so any relayer can make it
([`bridge.md`](design/bridge.md) §12.2, §12.3). **This tree's bridge program** (image `0xccde6171…`), padded, at a
mid-range laptop's shape (8 CPUs, 16 GiB hard) and at 16 CPUs: private claim **1,093.0 s** (941.1 and 1,245.0 s) and
**639.2 s**, private burn **932.8 s** (820.9 and 1,044.7 s) and **534.5 s**, r0vm peak **9.19 GiB**, each transaction
settled with its receipt verified ([`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md)). **Gaps:** a run on a real
mid-range laptop (the owner's step, `bench/perf/p4-laptop-owner.sh`); the current program image on a GPU; the wrap on
Apple Silicon (not supported today).

### P.5 Latency: deposit to mint, burn to release

**Measured end to end on Sepolia with testnet parameters** (the live round trip `crown-live-3` on the final programs:
FerrataVault v5 in mode (B), W = 1 h, release delay 120 s; LEZ v0.4.0-rc1 on a devnet we ran, n = t = 1;
[`perf-latency-bridge.md`](../lez/results/perf-latency-bridge.md), `python3 -I bench/perf/latency_bridge.py
e2e/runs/crown-live-3`, from the run's own records):

| | inbound: deposit 1 to its private claim applied | outbound: the private burn to its release |
|---|---:|---:|
| **as it ran** | **32.9 min** | **66.2 min** |
| source-chain finality wait | 18.6 min (Ethereum: the deposit to its checkpoint's finalization, *derived* two epochs after the checkpoint's slot) | LEZ: the burn's block final on Bedrock before the checkpoint's input (devnet) |
| update availability and capture | 6.6 min (the covering finality update served and read, the vault's slots proven) | — |
| proof generation | 231.9 s (the private claim, LEZ's wallet on a cloud CPU) | 167 s (checkpoint on one L40S and the Groth16 wrap) |
| privacy-motivated delay | 0 (the client's choice; none in this run) | 0 (the client's choice) |
| LEZ steps before the claim | 39.6 s (the light-client update, three transactions; the root import) | — |
| the vault's wait W, release delay | — | 3,607 s (W = 1 h of block time); 156 s (120 s delay and the release) |
| on-chain verification | 8.3 s (the claim, send to final) | 37 s (`submitCandidate`, `finalizeAttested`) |

Inbound without the runner's other work in its LEZ session (deposit 0's steps, done first): **29.8 min**. With
production parameters the outbound is dominated by W (1 h in rule (B); 3 h 16 min to 4 h 31 min in rule (A) at the
default floor; tip to finalisation 4.00–7.68 h median in simulation, [`bridge.md`](design/bridge.md) §9, §11.7) and
the release delay (at least 24 h outside testnets, §13); the inbound by Ethereum's finality and the operators' import
cadence (hourly in the design: up to 60 min more). The first live run (`crown-live-1`, the same parameters) took 64.7
min from the burns to the first release ([`e2e/runs/crown-live-1/README.md`](../e2e/runs/crown-live-1/README.md)).

### P.6 Off-chain compute (RFP-022)

| Component | CPU | Memory | Time | Source |
|---|---|---|---|---|
| checkpoint prover | one L4 GPU, one CPU core | about 10.9 GiB GPU, 0.76–1.08 GiB host | 14.0 s (fixed part), 24.1 s (idle hour), 41.4 s (5 private tx), 219.3 s (50 private tx) | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md) |
| the same on a laptop CPU | 16 threads | 9.62 GB peak | 1,157.5–1,758.9 s fixed part; ≈ 34 min per idle hour and ≈ 4.8 h per busy hour (extrapolated) | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) |
| rule (B) checkpoint | one L4 | — | 31.9 s (idle hour, `n` = 4, 9 segments) | [`bridge.md`](design/bridge.md) §18.5 |
| rule (B) checkpoint of the live run on the final programs (`crown-live-3`: LEZ blocks 25..=90, 2 burns, 18,499,940 user cycles, 20 segments) | one L40S | — | 31.9 s prover wall (119 s with the queue and transfers) | [`e2e/runs/crown-live-3/README.md`](../e2e/runs/crown-live-3/README.md) |
| Groth16 wrap of a checkpoint | x86 | 7.3 GiB | 195.1–215.2 s (laptop); recursion 18.7 s on 16 cores + 14 s Groth16 (end-to-end run) | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md); [`bridge.md`](design/bridge.md) §18.7 |
| cloud cost of the GPU proofs | — | — | 385 L4-seconds billed for four checkpoint proofs and an update: about 0.09 USD of GPU | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md), "Cost" |

**Gap:** the resources of the relayer and `monitor` daemons (built, [EVIDENCE](EVIDENCE.md) rows 43–46; not
measured, except `monitor`'s memory, 34,216 kB resident, row 43) and of the importer (the relayer's root imports: a prototype, row 106; not measured).

### P.7 State growth at 1M and 10M operations

**Measured per operation**, on both chains. LEZ ([`lez/results/perf-state.md`](../lez/results/perf-state.md) §4, §5;
real claims and burns of this tree's bridge program, then copies to 100,000 records):

| LEZ account | per | actor state | `Account` | in the serialized state, measured |
|---|---|---:|---:|---:|
| `nullifier(…, N)` | claim | 77 B | 133 B | **165 B** (+165.00 B per record from 1,000 to 100,000) |
| `burn(cid, H(note))` | burn | 185 B | 241 B | **273 B** (+273.00 B per record) |
| `root(cid, n)` | import | 117 B | 173 B | 205 B (account and key) |
| holder | new holder | | | +162 B and +486 B measured on two first credits |

Ethereum ([`lez/results/perf-state-ethereum.txt`](../lez/results/perf-state-ethereum.txt), `vault/test/StateGrowth.t.sol`:
Foundry's state-diff recording, each slot's value before and after the operation): a release creates **one** vault slot
(25 releases: 25 slots; the first release 3, two of them once), a deposit **one** for a depositor's first deposit and
none for a later one (25 new depositors: 28 slots, the deposit tree's frontier taking a new level as the tree grows; 25
repeat deposits: 1), a finalised checkpoint about **10** (15 for the first), whatever the number of burns it carries.

**Projections** from these measured sizes ([`perf-projections.md`](../lez/results/perf-projections.md), `python3 -I
bench/perf/projections.py`; *derived*):

| operations | LEZ nullifier records (claims) | LEZ burn records (burns) | Ethereum, vault slots for releases | Ethereum, vault slots for deposits |
|---|---:|---:|---:|---|
| 1M of each | 165 MB | 273 MB | 1,000,000 (64 MB of keys and values) | one per new depositor (at most 1,000,000), plus at most 32 for the frontier |
| 10M of each | 1.65 GB | 2.73 GB | 10,000,000 (640 MB) | one per new depositor (at most 10,000,000), plus at most 32 |

Roots grow with the import cadence, not with deposits: 8,760 a year at one an hour, 1.8 MB a year; checkpoints with
theirs (one an hour: 87,600 vault slots a year). LEZ v0.4 charges storage gas on transaction bytes, not on state, so
LEZ's growth is disk on every node; on Ethereum each new slot is paid inside the release and deposit gas of P.3
(97,920 state gas on Sepolia's Glamsterdam rules; 22,100 gas on mainnet's, 20,000 and the cold access). LEZ's own commitments and nullifiers for each
private transaction are LEZ's and not counted.

---

## Gaps, in one list

user-side proofs on a laptop (cloud CPUs held to 8 and 16 CPUs, and Mac minis under macOS, padded as LEZ's wallet
pads: P.4); latency measured end to end on the inbound leg (the outbound once, on testnet parameters: P.5); the
recommended-cadence tracking cost with real proving; daemon resources; the BLS precompile delta; cost
against state size; v5's ERC-20 calls and refunds on Sepolia's gas rules; an import under a mainnet state root; LEZ account
overhead. Every one is a milestone deliverable ([`attestation.md`](design/attestation.md) §13;
[`bridge.md`](design/bridge.md) §18.6; [`docs/traceability.md`](traceability.md)).
