# Ferrata: performance report (RFP-021 P.1–P.9, RFP-022 P.1–P.7)

**Revised for delivery, 22 November 2026** (the second draft: 21 November; the first: 11 November). Each requirement with the measured figures, where and how they were
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
| **cloud CPU container** | a Modal container held to the CPUs and memory named (request = limit; gVisor; mostly AMD Zen 3 servers, the host named per run), r0vm 3.0.5 on CPU; "8 CPUs, 16 GiB" is the mid-range laptop's shape | [`lez/results/perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md); [`lez/results/perf-proofs.md`](../lez/results/perf-proofs.md) |
| **build box, zone** | the Modal build box (16 CPUs of quota, 64 GiB) running a LEZ v0.4.0-rc1 zone we started (sequencer and Logos Blockchain node, LEZ's local configuration) or a local anvil | [`lez/results/perf-gate-sequencer.md`](../lez/results/perf-gate-sequencer.md); [`lez/results/perf-daemons.md`](../lez/results/perf-daemons.md) |

LEZ's limits on v0.4: a block, and so a transaction, may use 10M execution cycles (`MAX_GAS_EXEC`); the
base fee stops rising at 5M (`TARGET_GAS_EXEC`); a private transaction costs a flat 409,764 execution
gas plus its public part's cycles and a flat 224,063 storage gas, and at most four fit in a block
([`attestation.md`](design/attestation.md) §4.2). Execution gas is the user cycles LEZ metered; storage
gas is the transaction's serialized size.

---

## RFP-021

### P.1 Verifying an attestation within one LEZ transaction

**Met** for every fact type measured. In-process harness, mainnet block 26,384,851 (finalized
10 November 2026), RISC Zero's accelerated Keccak in the program
([`lez/results/stage4-attest.md`](../lez/results/stage4-attest.md), `stage4-attest-consumer.md`, regenerated on
22 November 2026 at `7602bc53` with this tree's programs, program `816d1fef…`, and the private consumer's read,
`stage4-attest-private-public-outputs.txt`; [`lez/RESULTS.md`](../lez/RESULTS.md) §5 keeps an earlier copy of the
first two):

| Statement (one transaction each) | Execution gas | Of the 5M target | Storage gas |
|---|---:|---:|---:|
| account | 168,909 | 3% | 4,424 |
| storage slot with a predicate | 275,865 | 6% | 7,338 |
| absent slot is zero | 270,580 | 5% | 7,291 |
| ERC-20 `Transfer` log (receipt of 7 logs) | 97,485 | 2% | 2,623 |
| log in a receipt of 306 logs | 1,619,960 | 32% | 58,812 |
| one EIP-2935 hop (16 blocks back) and a slot | 499,277 | 10% | 13,578 |
| four facts and a relation in one call | 454,255 | 9% | 12,309 |
| reference consumer: claim of a `Transfer` (Claim → Attest → reply → Credit) | 185,389 | 4% | 2,768 |
| private consumer: read + attestation in its own proof | 422,032 (flat 409,764 + 12,268) | — | 224,063 |

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

**Every program action and statement type, proven for real** ([`perf-proofs.md`](../lez/results/perf-proofs.md),
21 and 24 November 2026): the 50 guest executions of the test suite's scenarios (`lez/results/stage1-*.md`,
`stage4-attest.md`, `stage5-vault-v5.md`), which run every message of the attestation program (`Init`, `JobOpen`,
`Adopt`, `Apply`, `Record`, `Open`, `Miller`, `Finish`, `VerifyAll`, `ApplyVerified`, `Attest`, `Read`, `ReadPadded`,
`Succeed`, `Spawn`) and every type of statement (account, absent account, storage slot, absent slot, event log, a slot
behind an EIP-2935 hop, several facts with a relation, accumulator leaf), each with the exact input LEZ v0.4.0-rc1
gives it, proven by r0vm 3.0.5 on a cloud container held to 16 CPUs and 32 GiB (one container per transition,
`bench/perf/prove_modal.py`; `bench/perf/prove-actions.sh` runs the same `perf_prove --prove` on one machine): a
composite receipt verified against the image id with its journal equal to the executor's, then compressed to one
succinct receipt and verified again. Every record is of this tree's images, checked one by one by the command that
writes the results file: each proof's image id is the dump's, and is the attestation program's `0x816d1fef…` or the
reference consumer's `0x8464603b…` of [`image-ids.md`](image-ids.md), whose binaries' sha256 the dumped guests
match. LEZ v0.4 does not prove public transactions (every node re-executes them): these are the proofs a prover of
the same execution would make, RFP-020's method. Execution gas equals the user cycles proven in every row. Proof
times are sums over a transaction's transitions; each transition's machine is in the results file (mostly AMD Zen 3
hosts; eight actions sum transitions proven on different hosts, `Init`, `Init` adopt 2/2, both `Finish` rows, the
succession (`Succeed` on Intel, `Spawn` on Zen 3), the public `VerifyAll` transaction (its `Record` on AMD Zen 2),
the read and the padded read (three of their transitions on Zen 4); the slot, absent-slot and four-facts statements
ran on Zen 4: between hosts, compression differs up to 2.5 times (the same 17,350-cycle `Record`: 6.4 s on Intel, 8.4 s
on Zen 4, 16.1 s on Zen 2), proving more than three times (6.4 s on Zen 4 to 21.4 s on Zen 2)):

| action | transitions | exec gas | stor gas | user cycles proven | segments | prove (composite) s | compress to succinct s | composite B | succinct B (one per transition) | peak RSS GiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Init (bootstrap): Init | 2 | 210,666 | 1,896 | 210,666 | 2 | 66.5 | 19.2 | 497,440 | 225,833 + 224,519 | 4.60 |
| Init (bootstrap): adopt 1/2 | 1 | 3,399,141 | 24,980 | 3,399,141 | 4 | 469.2 | 71.0 | 1,177,113 | 249,068 | 9.19 |
| Init (bootstrap): adopt 2/2 | 3 | 3,485,575 | 25,108 | 3,485,575 | 6 | 488.1 | 86.6 | 1,649,283 | 249,201 + 225,511 + 224,041 | 9.19 |
| committee handoff (update_0): Open (step 1) | 2 | 2,156,300 | 2,853 | 2,156,300 | 4 | 290.6 | 57.9 | 1,089,152 | 227,532 + 225,865 | 9.19 |
| committee handoff (update_0): Miller (step 2) | 1 | 3,313,372 | 399 | 3,313,372 | 4 | 415.5 | 69.3 | 1,117,523 | 225,897 | 9.19 |
| committee handoff (update_0): adopt 1/2 | 1 | 3,410,995 | 24,980 | 3,410,995 | 4 | 464.6 | 70.3 | 1,180,189 | 250,606 | 9.19 |
| committee handoff (update_0): adopt 2/2 | 1 | 3,418,189 | 24,980 | 3,418,189 | 4 | 467.8 | 73.4 | 1,180,317 | 250,670 | 9.19 |
| committee handoff (update_0): Finish (step 3) | 2 | 3,699,098 | 463 | 3,699,098 | 5 | 471.8 | 55.5 | 1,378,633 | 225,614 + 225,254 | 9.19 |
| finality update: Open (step 1) | 2 | 2,194,902 | 3,525 | 2,194,902 | 4 | 307.9 | 60.3 | 1,089,102 | 228,025 + 225,347 | 9.19 |
| finality update: Miller (step 2) | 1 | 3,308,048 | 399 | 3,308,048 | 4 | 410.1 | 70.5 | 1,116,487 | 225,379 | 9.19 |
| finality update: Finish (step 3) | 3 | 3,725,955 | 527 | 3,725,955 | 6 | 514.0 | 89.3 | 1,600,717 | 224,968 + 225,461 + 224,041 | 9.19 |
| finality update in one public transaction: VerifyAll (then ApplyVerified, Record) | 3 | 9,124,896 | 4,105 | 9,124,896 | 12 | 1114.4 | 214.9 | 3,267,807 | 227,408 + 225,429 + 224,041 | 9.20 |
| statement: account: the WETH contract (nonce, balance, storage root, code hash) | 1 | 168,909 | 4,424 | 168,909 | 1 | 67.2 | 19.6 | 488,178 | 227,717 | 3.47 |
| statement: storage slot: a WETH balance, value >= 1 | 1 | 275,865 | 7,338 | 275,865 | 1 | 81.0 | 16.0 | 506,294 | 230,663 | 4.60 |
| statement: storage slot: an absent slot is zero | 1 | 270,580 | 7,291 | 270,580 | 1 | 87.7 | 17.5 | 506,200 | 230,616 | 4.60 |
| statement: account: an address without an account | 1 | 151,829 | 4,150 | 151,829 | 1 | 68.2 | 19.8 | 487,630 | 227,443 | 3.47 |
| statement: event log: an ERC-20 Transfer (receipt with 7 logs) | 1 | 97,485 | 2,623 | 97,485 | 1 | 71.0 | 20.7 | 484,576 | 225,916 | 3.47 |
| statement: event log: a Transfer in the block's largest receipt (306 logs) | 1 | 1,619,960 | 58,812 | 1,619,960 | 2 | 277.1 | 40.7 | 903,717 | 282,105 | 9.18 |
| statement: storage slot 16 blocks back: one EIP-2935 hop, then the slot | 1 | 499,277 | 13,578 | 499,277 | 1 | 162.4 | 21.1 | 532,022 | 236,903 | 9.15 |
| statement: four facts in one call (account, two slots, a log) and a relation | 1 | 454,255 | 12,309 | 454,255 | 1 | 134.7 | 16.5 | 529,612 | 235,698 | 9.15 |
| statement: accumulator leaf: FerrataVault v5's slots 0 and 1 and deposit leaf 0 (real Sepolia state) | 1 | 327,016 | 7,659 | 327,016 | 1 | 108.5 | 20.8 | 506,840 | 230,936 | 4.62 |
| statement: accumulator leaf: the same with both deposit leaves | 1 | 392,358 | 8,731 | 392,358 | 1 | 109.1 | 20.8 | 509,048 | 232,040 | 4.62 |
| consumer: a public actor calls Attest: storage slot | 3 | 386,181 | 7,507 | 386,181 | 3 | 136.9 | 42.0 | 1,014,414 | 237,300 + 231,045 + 224,122 | 4.61 |
| consumer: a public actor calls Attest: event log | 3 | 164,872 | 2,792 | 164,872 | 3 | 102.9 | 40.7 | 987,070 | 228,624 + 226,261 + 224,358 | 3.47 |
| consumer: reference consumer: claim an ERC-20 Transfer (credits its sender once) | 4 | 185,389 | 2,768 | 185,389 | 4 | 108.6 | 51.8 | 1,206,854 | 227,890 + 226,261 + 224,064 + 223,480 | 3.47 |
| read: a public reader of the consumer reads a record (ReadRecord, then Read, then the reply) | 3 | 39,410 | 604 | 39,410 | 3 | 21.3 | 30.1 | 667,870 | 223,462 + 224,105 + 224,048 | 1.44 |
| read: the same with 1,024 bytes of padding (ReadPadded) | 3 | 63,319 | 1,628 | 63,319 | 3 | 27.0 | 27.9 | 698,824 | 226,106 + 225,133 + 224,333 | 1.44 |
| instance succession: Succeed (then Spawn at the successor) | 2 | 70,610 | 697 | 70,610 | 2 | 29.2 | 16.2 | 494,422 | 224,827 + 224,464 | 1.44 |

**The proven update path** (one private transaction proving a whole finality update, `VerifyAll` at a private
account's actor, this tree's program `0x816d1fef…`; [`perf-proofs.md`](../lez/results/perf-proofs.md)): the private
transition 9,049,362 user cycles in 10 segments, `execute_and_prove` **1,673.6 s** on a 16-CPU container (AMD Zen 3),
peak **9.21 GiB**, receipt **227,120 B**, transaction 231,114 B; settled through LEZ's state machine with the receipt
verified, charged **485,298** execution gas (the flat 409,764 plus a 75,534-cycle public part) and 224,063 storage
gas. Earlier images: 3,240.5 s on the laptop CPU (16 threads, shared), 49.5 s on one L4, 895.6 s on a cloud CPU with
32 prover threads ([`lez/RESULTS.md`](../lez/RESULTS.md) §3, §9). The same update in one public transaction
(`VerifyAll`, then `ApplyVerified` and `Record`) is charged 9,124,896 execution gas and 4,105 storage gas
(`lez/results/stage1-one-transaction.md`), and LEZ v0.4 does not prove it; its three executions are proven all the
same (the table's row): `VerifyAll` itself, the private transition's 9,049,362 user cycles in the same 10 segments,
**1,079.3 s** composite and 189.7 s to compress on a 16-CPU container (AMD Zen 3), peak 9.20 GiB, succinct receipt
**227,408 B** (InnerReceipt, the table's measure; 231,713 B as a whole `Receipt`). **The proven committee handoff** (`prove_private
--handoff`: update_1 as served, its 512 keys adopted in the same private call, no new header): 15,796,512 user cycles in
17 segments, `execute_and_prove` **2,254.2 s** on a 16-CPU container (AMD Zen 4), peak 9.20 GiB, receipt **227,347 B**,
transaction 231,568 B; settled with the receipt verified, charged **451,895** execution gas (409,764 + a 42,131-cycle
public part) and 224,063 storage gas.

**The private attestation** (the consumer's read of a record and LEZ's privacy circuit, padded as the wallet pads
it): 633.5 s at 16 CPUs and 1,254.8 s at 8 (3.34M user cycles, 6 segments), receipt 239,246 B, transaction 255,366 B,
peak 9.19 GiB (P.4, [`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md)); unpadded, of an earlier image, 16.1 s on
one L4 (receipt 227,170 B; [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md), run `gpu-l4-attest-2`). A handler
pays 5.7 cycles per byte it receives and about 3.4 per byte it sends ([`attestation.md`](design/attestation.md) §4.3).

**Reproducible from the test suite**: `lez/harness/tests/public_path.rs`, `attest_public.rs`, `attest_private.rs`,
`sepolia.rs`, `vault.rs` and `costs_attestation.rs` write or pin the cycle and gas tables (`lez/RESULTS.md` §8);
`perf_prove --dump` captures the same scenarios' guest inputs (all 50 transitions) and `bench/perf/prove-actions.sh`
(or `p2-stage.sh` and `prove_modal.py`) proves each one; `python3 -I bench/perf/summarize_proofs.py
bench/perf/results/p2-c bench/perf/results/p2-b bench/perf/results/p2-a` writes the results file from the records
and stops if a record is not of this tree's images.

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

### P.3 Tracking cost, apart from attestation cost

**Measured over eight days of operation** ([`lez/results/perf-tracking.md`](../lez/results/perf-tracking.md), written by
`python3 -I bench/perf/tracking_cost.py lez/lab/results/record`): every transaction the lab sent to its LEZ v0.4.0-rc1
sequencer while it followed mainnet and Sepolia, one session every two hours from 10 to 18 November 2026, with the gas
and the fee that sequencer charged (read back from the chain, [`lez/lab/README.md`](../lez/lab/README.md)). Public job
path; fees in LEZ's native unit at the chain's base fee of 8. Steady state (the 97 sessions after the first, 8.06 days):

| | mainnet | Sepolia |
|---|---:|---:|
| a finality update (3 transactions): execution gas, median of 98 (range) | 9,225,372 (9,169,394–9,333,584) | 9,097,182 |
| a handoff (5 transactions): execution gas, median | 16,060,242 | 15,903,846 |
| per day, as the lab ran: execution gas; fees | **127,294,275; 1,019,185,463** | 126,556,100; 1,013,454,096 |
| per sync-committee period (27.3 h): execution gas; fees | 144,832,597; 1,159,606,572 | 143,992,718; 1,153,085,549 |

**Per attestation** (mainnet, as the lab ran): an instance's tracking divided by the attestations that use its records,
127,294,275 / N execution gas a day: 12.7M at 10 attestations a day, 1.27M at 100, 127,294 at 1,000, 12,729 at 10,000;
**apart from each attestation's own verification**, which its consumer pays whatever N is: 93,316 execution gas for a
`Transfer` log without a predicate (97,485 with the reference consumer's conditions, P.2) and 166,249 for an account
with one condition, its code hash (168,909 with P.2's three), 275,865 for a storage slot, 499,277 for a slot through one EIP-2935 hop ([`perf-state.md`](../lez/results/perf-state.md) §2,
this tree's program). At the design's cadence (one update an
hour while roots are imported, plus each period's) the measured per-update figures give 24 × 9,225,372 + 0.88 handoffs
× 16,060,242 ≈ 235.5M execution gas a day (*derived* from the measured medians; every one of the 98 measured finality
updates was within +1.17 / −0.61 % of the median). **At the once-per-period cadence** (an instance without bridge
imports, [`attestation.md`](design/attestation.md) §10: one handoff per sync-committee period of 27.3 h): 16,060,242
execution gas per period, about 14.1M a day, **14.1M / N per attestation** at N attestations a day (*derived* from the
measured handoff median).

**The proven path, with real proving** (this tree's program, P.2): an update is one private transaction, charged
485,298 execution gas and 224,063 storage gas (709,361 gas; fee 5,674,888 at the base fee its settlement saw), after a
proof of **1,673.6 s on 16 CPUs** (7.4 CPU-hours allocated, 16 × the wall time; 6.76 CPU-hours used, GNU time) or
49.5 s on one L4 (earlier image). For the design's 24 hourly updates a day that is 11.6M execution and 5.4M storage gas
a day, and 179 allocated CPU-hours or 20 L4-minutes of proving (*derived* from the measured update); each period's
handoff adds one more private transaction, charged 451,895 execution and 224,063 storage gas after a proof of 2,254.2 s
on 16 CPUs (P.2: proven for real in one private transaction), 0.879 a day: with it, 12.0M execution and 5.6M storage
gas and 187 allocated CPU-hours a day (24 × 485,298 + 0.879 × 451,895; *derived*). At the once-per-period cadence the
proven path costs that handoff alone: 451,895 execution gas per period. Per attestation, at N attestations a day:
235.5M / N execution gas on the public path at the design's cadence (14.1M / N once per period), or 12.0M / N plus
187 / N allocated CPU-hours of proving on the proven path, each period's handoff included (11.6M / N and 179 / N for
the 24 hourly updates alone).

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
([`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md), 21 November 2026). The owner's step, rehearsed as the owner
would run it in an 8-CPU, 16-GiB container: 1,110.8 s for the attestation, all three transactions settled, 50 minutes in
all. On the owner's laptop, below the declared minimum (8 of its 16 threads, `low-power` profile, other work on the
other cores, its swap full): **4,596.7 s**, 9.17 GiB, settled ([`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md),
25 November 2026). **Gaps:** a laptop at the declared minimum (in a low-power profile about 30–39 min an attestation,
*estimated*); a chain of hops (a 320-hop claim is estimated at about 58M accelerated user cycles; a limit of 5 hops
is proposed from derived figures, not declared).

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

The verification row is a proxy: no consuming transaction was sent in that run, so the row is the run's own median
send-to-final time of a transaction (15.7 s over its 159 update transactions; P.1: the statement is verified inside
the consuming transaction). An event in a block between two records' headers needs one EIP-2935 hop (about 223k
execution gas more, P.1); LEZ ran a one-validator devnet with 10-s blocks. With the run's two outages included (the zone stopped when its
Bedrock funds ran out) the median is 19.6 min and the p90 86.7 min. At a slower cadence the header wait grows: an
operator updating once an hour adds up to 60 minutes to it (*derived*); deferral (R.10) adds a random share of the
window: on a sequencer two daemons applied an update in 105 s (median; p90 106 s) without deferral, 126 s (p90 176 s)
with a window of 8 rounds (nominally 32 s; rounds took 5.9–6.2 s) and 306 s (p90 576 s) with 30 rounds (nominally
120 s; 4.6–4.8 s each), while the execution gas spent per revealed update fell from 16.0M to 8.1M and 4.6M, abandoned
jobs and refused steps included (G5 revealed every 90 s, not 60, and 12 of its 17 updates were covered by a later
update's job: much of the fall is updates skipped, not the window alone; [`perf-gate-sequencer.md`](../lez/results/perf-gate-sequencer.md); updates replayed
81 to 122 times faster than the lab recorded them, about 40–60 times the design's hourly cadence, so jobs overlapped).

### P.6 Off-chain compute

| Component | CPU | Memory | Time, bandwidth | Source |
|---|---|---|---|---|
| **the updater daemon** (`ferrata-updater`, sequencer backend), two or three instances following 20–30 recorded mainnet updates on a LEZ v0.4.0-rc1 zone, configured to poll every 4 s (the adversary 2 s; rounds took 4.6–17.6 s), gates G2–G5 | **0.043–0.092 of one core** per daemon (G2: 151 CPU-seconds in 1,849 s, 0.08) | **57.3–57.8 MiB peak**, 33–38 MiB median resident | beacon API: G2's honest daemons 650 and 666 requests, **10.3 and 10.4 MB** (the adversary 604, 9.0 MB); sequencer RPC **0.57–2.07 MB up, 7.4–19.6 MB down** per daemon (the least for G2's honest daemons, which applied 5 and 1 steps; the most for daemons that did the work: G4 about 1.8 MB up and 19.0–19.6 MB down each in 1,545 s, G2's adversary 2.07 / 19.0 MB) | measured, gates G2–G5 ([`perf-gate-sequencer.md`](../lez/results/perf-gate-sequencer.md)) |
| the proven path's prover (one private transaction per update) | 16 CPUs for 1,673.6 s (7.4 CPU-hours allocated; 6.76 used) per update | 9.21 GiB peak | 49.5 s on one L4 (earlier image) | measured (P.2, [`perf-proofs.md`](../lez/results/perf-proofs.md)) |
| the lab zone (sequencer, Bedrock node, driver) | two physical cores requested | — | a session 83–142 s over 85 sessions; state 20–31 MB compressed per generation; 0.0064 USD per scheduled session, about 0.08 USD a day (derived) | [`lez/lab/README.md`](../lez/lab/README.md) |
| **the updater as a Logos module** (`ferrata_updater_module` in a headless Logos Core, sequencer backend, its light-client sources through `beacon_api_module`), on a LEZ v0.4.0-rc3 network we run, 90 min | **0.040 of one core** after its start (216.7 CPU s in 5,391.5 s, 15 jobs); its start, the follower from the network's genesis: 492.9 CPU s in 531.4 s | **148.1 MiB median, 270.9 MiB peak** (its host process with the executor it starts) | the network's RPC 22.1 MB an hour down, 1.5 up (it applies every block of the network), 5.2 MB an hour from each of the two light-client providers (uncompressed, TLS not counted); each transaction final 15.1–26.8 s after sent | measured (`modules/results/live-testnet-2026-11-23/`, session 1) |
| the same with an execution source through `eth_rpc_module`, reaching the network directly, 46 min | 0.031 of one core after its start (84.1 CPU s in 2,734.9 s, 8 jobs); its start from session 1's state 345.6 CPU s in 525.9 s; 165.1 MiB median, 296.3 MiB peak | | the execution checks through `eth_rpc_module`: 16 requests, 3,512 B up, 225,915 B down | measured (same record, session 2) |
| `ferrata attest` (cli/, release), on the network | `status` from a new state (following the network from genesis, about 4,190 blocks) 1,259.6 s wall, 827.6 CPU s, 137.1 MiB peak; with its state kept 44.1 s; `config` 1.3 s; an attestation 28.5 s as a dry run, 23.2 s sent (until final); a finality update 1.6 s as a dry run, 70.0 s sent (three steps) | | | measured (same record, session 2) |
| `ferrata_attest_module` | 0.0002 of a core (its calls 0.07–5.31 s: `verify_checkpoint` 5.31 s, `fetch_log_proof` through `eth_rpc_module` 1.16 s) | its host process 58.7 MiB resident (shared pages counted) | | measured (session 1) |
| attest-monitor (the §8 `monitor`), polling every 60 s; Logos Core itself | attest-monitor 0.0033 of a core (18.0 CPU s in 5,391.5 s); Logos Core's daemon 0.0001 | attest-monitor 22.2 MiB peak, 4.2 MiB median; Logos Core's daemon 58.2 MiB; each module host process 57–60 MiB resident, shared pages counted in each | attest-monitor 0.15 MB an hour from the network, 0.14 and 0.17 MB from its two providers | measured (session 1) |

The gates replayed recorded updates 81 to 122 times faster than the lab recorded them (one every 60 or 90 s; the lab
recorded one about every two hours, its session schedule), about 40–60 times the design's hourly cadence, so their traffic is that of a daemon busier than in operation; per-day figures from them would
overstate a real day, and none is given. The live Sepolia daemon polled every 24 s. Measured as a Logos module on a LEZ
v0.4.0-rc3 network we run, with `ferrata attest`, the attestation module and attest-monitor on that network (above); the
gates' figures are the binary's.

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

**Jobs that never finish** leave their `job` account (1,110 B each; clean-up is not built): the lab, one operator,
left 0 of 248 jobs; the gates, two daemons racing on updates replayed 81 to 122 times faster than the lab recorded
them, left 8, 19 and 3 beside 16, 23 and 8 completed (G3, G4, G5; [`perf-gate-sequencer.md`](../lez/results/perf-gate-sequencer.md)).
Abandoned jobs come on top of the completed ones: with k operators racing at the design's cadence (9,087 updates a
year), at most (k − 1) × 10.09 MB a year and (k − 1) × 50.43 MB in five years (one abandoned job per extra operator
per update; *derived*, [`perf-projections.md`](../lez/results/perf-projections.md)); none with one operator. That bound
is for honest operators: anyone can add more, one job account per `Open` it never finishes (a forged signature passes
`Open`, 2,195,028 execution gas: `lez/results/stage1-refusals.md` row 1), and since nothing clears jobs, that growth is
bounded only by what its sender pays. The reference consumer's per-claim accounts are the consumer's state, not the module's
(`lez/guests/consumer`).

LEZ v0.4 charges storage gas on a transaction's bytes, not on the state it leaves, so the growth costs nothing in fees:
it is disk on every node.

### P.8 BLS: in-program against a hypothetical precompile

**Modelled from measured inputs** ([`lez/results/perf-precompile.md`](../lez/results/perf-precompile.md), `python3 -I
bench/perf/precompile_model.py bench/perf/results`). The program's BLS work for a finality update (mainnet update_1,
510 of 512 signers): hash-to-G2 1,169,105 and the subtractive key aggregation 285,875 cycles inside `Open`, and the
final exponentiation and check 3,581,111 (the bare guest's phases with the same accelerated blst, bench/RESULTS.md), the
two Miller loops 3,308,048 (this tree's `Miller` step, counted whole): **8,344,139 of the update's 9,228,905 cycles**. The model prices a native precompile at time parity: the
operation's native time (blst 0.3.16 on the build box's CPU, one thread, median of 300: hash-to-G2 228 µs, the two
Miller loops 651 µs, the final exponentiation 425 µs; the adoption of 256 keys 269 µs, a median of 50) times the
executor's own throughput on the same box (11.3–14.5 Mcycles/s on these steps, 15.4 on the adoption), so that a node
spends the same time per unit of gas inside and outside the zkVM.

| step | in-program cycles (BLS) | precompile, modelled | delta |
|---|---:|---:|---:|
| `Open`: hash-to-G2, key aggregation | 1,454,980 | 2,652 | 1,452,328 |
| `Miller`: two Miller loops | 3,308,048 | 9,003 | 3,299,045 |
| `Finish`: final exponentiation, check | 3,581,111 | 6,187 | 3,574,924 |
| **finality update** | **8,344,139** | **17,841** | **8,326,298** |

With the precompile a finality update would meter about 902,607 cycles instead of 9,228,905 (10.2 times less, and one
transaction instead of three); a handoff about 845,451 instead of 15,997,954. Both are lower bounds: `Miller` and the
adoption parts are counted whole as BLS work, though each also reads and writes the job, so a precompile would leave
somewhat more.
At ten times the native cost the conclusions stand. **A precompile is reachable only from public transactions**: a
private transaction's program runs inside the zkVM proof, where only RISC Zero's accelerators exist (the program already
uses RISC Zero's accelerated blst), so the proven path and every private consumer would not change. Reference point:
Ethereum's EIP-2537 charges 102,900 gas for the pairing check that costs this program 6.9M cycles.

### P.9 No action scales with accumulated state

**Measured** ([`lez/results/perf-state.md`](../lez/results/perf-state.md) §2, §3): with 0, 1,000, 10,000 and 100,000
records held by the instance (the serialized state growing from 2.8 MB to 48.1 MB), five statement types (an account, a
storage slot with a predicate, a `Transfer` log, a log in the block's largest receipt, a slot through one EIP-2935 hop)
and every step of a handoff and of a finality update were charged the same execution and storage gas at every size
(`perf_state` asserts it): for example 275,865 for the storage slot and 3,308,048 for `Miller` each time. What this
shows is limited: the copies sit at keys no measured action reads, and a LEZ transaction sees only the accounts it
names, so the test shows that LEZ's metering does not depend on unrelated state; it cannot catch an account an action
reads growing with use. That is excluded by construction instead: each action reads the instance (593 B, constant:
committees as summaries), its job (1,110 B), one record (453 B), or for the bridge one root (205 B) and one nullifier
or burn record (165 / 273 B), each fixed in size and addressed by key; no action of the module reads a collection (the
reference consumer's private holdings, below, are one). In operation the
lab's `Miller` (3,307,963) and `Finish` (3,723,564) were charged identically in all 98 mainnet finality jobs while 106
records accumulated ([`perf-tracking.md`](../lez/results/perf-tracking.md); `lez/lab/results/record/run.tsv`).
**Every other action, measured** ([`lez/results/perf-state-actions.md`](../lez/results/perf-state-actions.md),
`bash bench/perf/state-actions.sh`; the scenarios in `lez/harness/src/accumulated.rs`): each runs on copies of one
world, the copy for K holding K = 1,000, 10,000 or 100,000 more of the accounts that accumulate beside it, copied from
accounts the programs wrote, at keys no measured transaction names (records of the instance and other instances; for
the consumer, its spent claims, balances or holders, and readers; job accounts are not copied), and every private
transaction K more commitments in LEZ's commitment set (the serialized state up to 143.6 MB). Each was charged the
same handler cycles, execution gas and storage gas at every K (asserted): `Init` and its two adoption transactions
(210,666, 3,399,141 and 3,485,575 execution gas), `VerifyAll` in one public transaction (9,124,896) and in one private
transaction, the proven update path (485,298 execution gas, 224,063 storage gas), `Succeed` → `Spawn` (70,610), all
eight statement types of [`stage4-attest.md`](../lez/results/stage4-attest.md) (the absent account 151,829, the absent
slot 270,580, the four facts 454,255), the accumulator-leaf statement with the vault's slots under a record that binds a
block hash (122,067 with no leaf to 646,014 with eight; 167,123 with one leaf under a record holding the payload's
fields: [`stage5-vault-synthetic.md`](../lez/results/stage5-vault-synthetic.md)), and `Read` and `ReadPadded`
(`Read` 12,268 cycles): with §3's update steps, every message the attestation program handles. The reference
consumer's messages are application code by RFP-021's scope, and were measured the same way: `CallAttest`, `Claim` with
its `Credit`, `AttestPrivately` (422,032 execution gas), and the vault pattern on FerrataVault v5's real deposits,
`ClaimDeposit` → `CreditHolder` (500,206) and `ClaimDepositPrivately` with its public `Read` → `Spend` → `CreditHolder`
(456,847). The accounts an action reads that change with use were measured as they change: a reader keeps only the
last reply or record (`CallAttest` 386,181 from an empty reader, 395,003 once it holds a reply, the same on the call
after), and a credit to an existing balance or holder costs the same whether it was credited once or 100,001 times
(187,721; 502,821). CI pins the same scenarios with a thousand more of each
([`costs_attestation.rs`](../lez/harness/tests/costs_attestation.rs), job `lez-harness`; the vault pattern on the
vault's record placed rather than replayed from Sepolia, its pins the replayed world's figures).

**What grows.** (1) In the program's actions, one cost, and it is LEZ's: a private transaction's circuit verifies its
fee payer's membership path in LEZ's commitment tree, one hash per level, and the tree's depth is log2 of every
commitment ever made (`lee/state_machine/src/merkle_tree/mod.rs` in LEZ v0.4.0-rc1). Measured: with 100,000 more
commitments the path grew from 3 to 17 levels while the private `VerifyAll`, `AttestPrivately` and
`ClaimDepositPrivately` were charged exactly the same, and the private `VerifyAll`'s circuit grew from 1,913,757 to
1,927,235, 1,934,974 and 1,940,779 user cycles at 3, 10, 14 and 17 levels, about 1,935 a level (`AttestPrivately`'s
and `ClaimDepositPrivately`'s the same: 1,975,409 to 2,002,530 and 2,176,439 to 2,203,529 from 3 to 17 levels;
[`perf-state-actions.md`](../lez/results/perf-state-actions.md), its last table), about 62,000 at depth 32 (derived),
beside about 1.9M for the circuit and 9.05M for `VerifyAll`'s own execution: the 14 more levels add 1.4% to the
circuit. The same circuit varies by a few tens of cycles from one run to the next (1,913,726 and 1,913,757 at 3 levels
in two runs; the cause not established), far below one level's cost. LEZ's API drops the prover's statistics; r0vm logs
every session's cycles with `RISC0_INFO=1 RUST_LOG=risc0_zkvm=info`, and `bash bench/perf/state-actions.sh` reads the
circuits' from that log. The module cannot avoid it (it is how LEZ proves that a private account exists), and at that
size it needs no mitigation. `VerifyAll` carries the instance's store in its message, so its private execution reads no
chain state: its 9,049,362 cycles cannot vary with K and are not a measurement against accumulation. (2) In the
reference consumer, which is application code by RFP-021's scope: a private-kind vault claim credits the claimant's own
private state, one `(pool, amount)` entry per pool it holds, and the claim's two private transitions grow by about
2,000–2,300 cycles a pool (measured: 1,523,845 with none, 1,737,559 with 100, 3,717,087 with 1,000); LEZ's circuit,
which hashes and encrypts that state, grows with it too (not measured). LEZ's charge does not see it (the public part is
`Read` and `Spend`). It is avoidable: a claimant avoids it today by sending each pool's deposits to a private account of
its own, and a consumer can keep one pool per private account instead of a list. Not measured against accumulation: the
bridge's actions (RFP-022's program, which has no such clause; the growth of its accounts is RFP-022 §P.7's), and the
soft-requirement programs (`statements`, `rebate`) and the second consumer (`ferrata_holdings`), which are programs of
their own. The
node's own work is not charged: serializing the state took 0.002 s at 2.8 MB and 0.228 s at 48.1 MB (the breakpoint a
node writes, not a transaction's cost).

---

## RFP-022

### P.1 Claim and mint in one LEZ public transaction

**Met.** In-process harness, the bridge program the live runs deployed (image `ccde6171…`, the same built at any path
from `f13faee2`: [`image-ids.md`](image-ids.md); since milestone B1 this tree's is `847a4336…` (ADR 0032; B1's first build `ac1fb59b…` was within −5,272 and +4,718),
its records in `lez/results/b1/`, execution gas within −9,799 and +4,718 of these, the burns now hashing no note; since ADR 0046 `ffb47f6e…` (`ferrata_bridge_f13`, never deployed), whose claims name no fee schedule: 24 bytes shorter, the privacy suite's plaintext claims 261,007 execution gas and 2,029 storage gas, its private claims 552,814: `privacy/generated/summary.md`; since ADR 0048 this tree's is `62199ab6…` (`ferrata_bridge_lezfee`, not deployed), whose claims are encoded as ADR 0046's), synthetic vault storage (tables
`lez/results/bridge-*.md`, recorded on 12 November 2026):

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
not depend on how many deposits it covers**, measured on the bridge program of the live runs before milestone B1 (`0xccde6171…`; in-process harness,
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
| Sepolia's real state: the v5 vault, one leaf then two, captures 1 and 2 (the deployed program, the live round trip `crown-live-3`) | 432,182; 432,182 | — | crown devnet, LEZ v0.4.0-rc1 ([`e2e/runs/crown-live-3/README.md`](../e2e/runs/crown-live-3/README.md)) |
| the same in the first live run (`integration5`'s program, `crown-live-1`) | 433,528; 429,805 | — | crown devnet, LEZ v0.4.0-rc1 ([`e2e/runs/crown-live-1/README.md`](../e2e/runs/crown-live-1/README.md)) |

**Amortised per deposit at the recommended cadence** (*derived* from the measured figures: one import an hour while
deposits arrive, its cost divided by the deposits of that hour): on Sepolia's real state (432,182, the live round
trip's import of the v5 vault) **432,182 per deposit at one deposit an hour, 43,218 at ten, 4,322 at a hundred**; on
the harness's synthetic storage (236,224) 236,224, 23,622 and 2,362. **With the hourly light-client update** the
recommended cadence runs for the bridge ([`attestation.md`](design/attestation.md) §10: one update an hour while roots
are imported): when the bridge is the instance's only consumer, each hour costs the instance's tracking, the hourly
update and its share of the period's handoffs (235.5M execution gas a day on the public path, RFP-021 P.3: 9.81M an
hour, of which the handoffs 0.59M), plus the import (432,182): **10.25M per deposit at one deposit an hour, 1.02M at
ten, 102k at a hundred** (*derived*); on the proven path (485,298 for the update, 0.879 × 451,895 / 24 = 16,549 for the
handoffs and 432,182 execution gas an hour, plus about 232,268 storage gas: 224,063 per update and the handoffs' 8,205)
934,029, 93,403 and 9,340 (*derived*). Shared with other consumers of the instance, the update's share
falls accordingly. Sources: [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §1, §2a, §8a;
[`lez/live/inbound/README.md`](../lez/live/inbound/README.md). **Under a mainnet state root** the import proves the
same account and two slots through mainnet's deeper state trie: P.1's account statement costs 167,882 under the
mainnet record against 148,948 under Sepolia's (169,611 with its 20,663-cycle header), so an import there is
*estimated* at about 450k; the per-deposit figures move by that difference.

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
| release (succinct), earlier image | 80,501 user cycles, constant over 64 witnesses; **37.1–48.6 s**, peak 1.50 GB, receipt 223,930 B (up to 150 s under load) | laptop CPU | [`bridge.md`](design/bridge.md) §18.2 |
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
| private claim and private burn, padded (B1), on the owner's laptop | **3,313.4 s** and **2,802.0 s**; peak 9.17 GiB; both settled | the owner's laptop, below the declared minimum (8 of 16 threads, `low-power`) | [`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md), row 264 |
| release, current image (`release_31`): the holder's succinct proof on the owner's laptop | **129.0 s**; peak 1.39 GiB; verified | the same laptop | [`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md), row 265 |
| recovery scan | median 128.9 ms per 100,000 deposits, 1,308.6 ms per 1,000,000 | laptop CPU, one thread | [`bridge.md`](design/bridge.md) §18.3 |

Declared minimum: 16 GB and 8 cores with 16 threads, x86-64, or an Apple M1 with 16 GB. The release's succinct proof needs the holder's burn
secret `sOut`, so it is the user's; the wrap takes only the succinct receipt, so any relayer can make it
([`bridge.md`](design/bridge.md) §12.2, §12.3). **The deployed bridge program** (image `0xccde6171…`, before milestone B1), padded, at a
mid-range laptop's shape (8 CPUs, 16 GiB hard) and at 16 CPUs: private claim **1,093.0 s** (941.1 and 1,245.0 s) and
**639.2 s**, private burn **932.8 s** (820.9 and 1,044.7 s) and **534.5 s**, r0vm peak **9.19 GiB**, each transaction
settled with its receipt verified ([`perf-p4-laptop.md`](../lez/results/perf-p4-laptop.md)). On the owner's laptop,
below the declared minimum (8 of 16 threads, `low-power`): this tree's claim **3,313.4 s**, burn **2,802.0 s**, 9.17
GiB, the release's succinct proof **129.0 s**, 1.39 GiB. **Gaps:** an x86 laptop at the declared minimum (in a
low-power profile about 30–39 min a proof, *estimated*); the current program image on a GPU; the wrap on Apple
Silicon (not supported today).

### P.5 Latency: deposit to mint, burn to release

**Measured end to end on Sepolia with testnet parameters** (the live round trip `crown-live-3` on the final programs:
FerrataVault v5 in mode (B), W = 1 h, release delay 120 s; LEZ v0.4.0-rc1 on a devnet we ran, n = t = 1;
[`perf-latency-bridge.md`](../lez/results/perf-latency-bridge.md), `python3 -I bench/perf/latency_bridge.py
e2e/runs/crown-live-3`, from the run's own records):

| | inbound: deposit 1 to its private claim applied | outbound: the private burn to its release |
|---|---:|---:|
| **as it ran** | **32.9 min** | **71.0 min** from the burn's proving (66.3 from its sending, 66.2 from it final on LEZ) |
| source-chain finality wait | 18.6 min (Ethereum: the deposit to its checkpoint's finalization, *derived* two epochs after the checkpoint's slot) | 7.0 s (LEZ: the burn sent to final on Bedrock; a one-validator devnet) |
| update availability and capture | 6.6 min (the covering finality update served and read, the vault's slots proven) | — |
| proof generation | 231.9 s (the private claim, LEZ's wallet on the crown runner's cloud CPU, 16–32 CPUs; 1,093 s at a laptop's shape, P.4) | 282.8 s (the private burn, the same) + 167 s (checkpoint on one L40S and the Groth16 wrap) |
| privacy-motivated delay | 0 in this run; the client's default holds a claim 1–12 h after its deposit ([user guide](guides/user-guide.md), "Choice" table): counted from the deposit's block, so Ethereum's finality and the capture (25.2 min) elapse inside it and what followed the capture (7.7 min) comes after it: inbound about 1.1–12.1 h (*derived*) | 0 in this run; the client's default waits for the vault's release delay (below) |
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
| the same on a laptop CPU | 16 threads | 9.85 GB peak | 1,157.5–1,758.9 s fixed part; ≈ 34 min per idle hour and ≈ 4.8 h per busy hour (extrapolated) | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) |
| rule (B) checkpoint | one L4 | — | 31.9 s (idle hour, `n` = 4, 9 segments) | [`bridge.md`](design/bridge.md) §18.5 |
| rule (B) checkpoint of the live run on the final programs (`crown-live-3`: LEZ blocks 25..=90, 2 burns, 18,499,940 user cycles, 20 segments) | one L40S | — | 31.9 s prover wall (119 s with the queue and transfers) | [`e2e/runs/crown-live-3/README.md`](../e2e/runs/crown-live-3/README.md) |
| Groth16 wrap of a checkpoint | x86 | 7.3 GiB | 195.1–215.2 s (laptop); recursion 18.7 s on 16 cores + 14 s Groth16 (end-to-end run) | [`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md); [`bridge.md`](design/bridge.md) §18.7 |
| cloud cost of the GPU proofs | — | — | 385 L4-seconds billed for four checkpoint proofs and an update: about 0.09 USD of GPU | [`gpu-prover/RESULTS.md`](../gpu-prover/RESULTS.md), "Cost" |
| `monitor` (the watchers' daemon), idle and while it checks a candidate, polling every 12 s | 0.53 CPU-seconds in 1,284 s (0.0004 of a core) | **14.0 MiB peak** | JSON-RPC 86 KB up, 254 KB down (about 23 MB a day, *derived*) | measured ([`perf-daemons.md`](../lez/results/perf-daemons.md)) |
| `ferrata-relayer run` (releases and finalisations), idle and while it finalises a checkpoint | 0.87 CPU-seconds in 1,284 s (0.0007 of a core) | **20.3 MiB peak** | JSON-RPC 408 KB up, 976 KB down (about 93 MB a day, *derived*) | measured ([`perf-daemons.md`](../lez/results/perf-daemons.md)) |
| the updater, whose records the root imports need | 0.043–0.092 of one core | 57.3–57.8 MiB peak | 10.3–10.4 MB from the beacon API in 31 min (gate G2, replayed about 122 times faster than the lab recorded them, 60 times the design's hourly cadence) | RFP-021 P.6 |
| `monitor` and the relayer as Logos modules (`ferrata_monitor_module`, `ferrata_relayer_module` in one headless Logos Core), the same anvil run | monitor 0.33 CPU s in 1,278.5 s (0.0003 of a core); relayer 0.70 (0.0005) | 66.4 and 73.9 MiB resident (each host process, shared pages counted; about 57 MiB of it a host process's own) | JSON-RPC 86,279 / 254,039 B and 411,356 / 983,811 B, the binaries' to within 1% | measured (`modules/results/live-testnet-2026-11-23/`, `bench/perf/daemons-anvil-modules.sh`) |
| `ferrata_attestor` (rule (B): an attestor beside every sequencer), four of them on LEZ v0.4.0-rc3's four-sequencer zone, 25 rounds each: 24 a minute apart (60–61 s), then the final round, whose attestations the checkpoint took | a round 0.17 CPU s and 0.19 s wall (medians; at most 0.36 and 0.94); 0.003 of a core at that cadence (*derived*) | **18.0 MiB peak** | 35,714 B read a round from its node and LEZ node (the first round 2.6 MB); about 314 MiB a day (*derived*) | measured (`e2e/runs/zone-bridge-rc3-multi/zone/attestor-*/resources.tsv`, `checkpoint/attestor/resources.py`; EVIDENCE row 266) |

Both Ethereum-side daemons ran against a local anvil with a rule (B) FerrataVault v5 (12-s blocks; ten idle minutes, a
candidate checked and finalised, ten idle minutes). **Gaps:** the importer's LEZ-side work (the relayer's root imports,
a prototype, EVIDENCE row 106) was not measured as a daemon (the relayer ran with `importRoots: false`); one import
transaction is P.2's. `ferrata_attestor` was measured on a build-box zone, each round timed by GNU time and the bytes
of its answers counted (the row above), not on a production host. `monitor`'s
LEZ view came from a file (`--node file:`), so its traffic to a LEZ node is not measured.

### P.7 State growth at 1M and 10M operations

**Measured per operation**, on both chains. LEZ ([`lez/results/perf-state.md`](../lez/results/perf-state.md) §4, §5;
real claims and burns of the deployed bridge program, `0xccde6171…`, measured before milestone B1, then copies to
100,000 records):

| LEZ account | per | actor state | `Account` | in the serialized state, measured |
|---|---|---:|---:|---:|
| `nullifier(…, N)` (`nullifier(cid, chain, vault, N)` since milestone B1; the state is the same) | claim | 77 B | 133 B | **165 B** (+165.00 B per record from 1,000 to 100,000) |
| `burn(cid, r)` (`burn(cid, H(note))` before ADR 0032; the same size) | burn | 185 B | 241 B | **273 B** (+273.00 B per record) |
| `root(cid, n)` | import | 117 B | 173 B | 205 B (account and key) |
| holder (since milestone B1 one account per configuration, `holder(cid, account)`) | new holder (a claim's destination or its relayer; the fee recipient's once) | | | **162 B** (the second claim: its destination's); the first claim's +486 B is three new holders: its destination's, the fee recipient's and its relayer's |

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

LEZ holder accounts grow with new holders, not operations: a claim creates at most two that are not one-time (a new
destination's and a new relayer's, 162 B each; the fee recipient's is created once), so at most 162 MB at 1M claims and
1.62 GB at 10M when claims name relayers already paid, and 324 MB / 3.24 GB if every claim named a new destination and
a new relayer (*derived*, [`perf-projections.md`](../lez/results/perf-projections.md)); a holder that takes another
token grows by a 68-B holding (from the account layout).

Roots grow with the import cadence, not with deposits: 8,760 a year at one an hour, 1.8 MB a year; checkpoints with
theirs (one an hour: 87,600 vault slots a year). LEZ v0.4 charges storage gas on transaction bytes, not on state, so
LEZ's growth is disk on every node; on Ethereum each new slot is paid inside the release and deposit gas of P.3
(97,920 state gas on Sepolia's Glamsterdam rules; 22,100 gas on mainnet's, 20,000 and the cold access). LEZ's own commitments and nullifiers for each
private transaction are LEZ's and not counted.

---

## Gaps, in one list

User-side proofs on a laptop at the declared minimum (cloud containers at its shape, Mac minis and a GPU are
measured, and the owner's laptop below it, 8 of its 16 threads: P.4 of both RFPs), and hops inside the user's proof
(RFP-021 P.4: a limit of 5 hops proposed, not declared); `ferrata-attest` and `ferrata_attest_module` not measured,
the §8 `monitor` not measured (RFP-021 P.6); the bridge's actions (RFP-022's program), the soft-requirement programs
(`statements`, `rebate`) and the second consumer (`ferrata_holdings`) at different state sizes (programs of their own;
every message of RFP-021's attestation program and of its reference consumer is measured, §P.9); A3's adversarial daemon at 100 updates on a sequencer (RFP-021 R.9; the two honest daemons at A3's size ran: EVIDENCE rows 275, 276); the importer's LEZ-side
resources as a daemon and `monitor`'s LEZ traffic (RFP-022 P.6); v5's ERC-20 calls and refunds on Sepolia's gas rules
(RFP-022 P.3); the source-chain finality of LEZ testnet 0.4, which is not released (RFP-022 P.5's outbound leg ran on a
devnet). Every one is a milestone
deliverable ([`attestation.md`](design/attestation.md) §13; [`bridge.md`](design/bridge.md) §18.6;
[`docs/traceability.md`](traceability.md)).
