# Ferrata on LEZ v0.4.0-rc1: the light client and its attestations as a program, measured through LEZ's state machine

This directory holds a LEZ v0.4.0-rc1 program that runs Ferrata's Ethereum sync-committee light
client and attests Ethereum facts under the headers it has verified, a reference consumer of
those attestations, and a harness that drives both through LEZ's own state transition functions
and fee settlement with real mainnet data (`../crates/ferrata-spectests/fixtures/mainnet-p1899`:
the period-1899 bootstrap and the best updates of periods 1899–1903, Fulu, 512-member committees;
`…/mainnet-attest-2026-11-10`: a block finalized on 10 November 2026 with proofs of its state and
receipts). Every number below is LEZ's own accounting of a transaction that ran, unless it says
otherwise.

**What ran end to end.** Through LEZ's own settlement path (fee reserve, metered action, refund,
over `lee`'s state transition functions): `Init` from the mainnet checkpoint, then all five mainnet
updates — each hands over or supplies a committee, four rotate it — as jobs of signed, fee-paying
public transactions. After every job the instance's store equals the compact form of the native
`FinalizedStore` (`Adoption::OnChain`), every finalized root equals the canonical root a second
provider reports, and `header(id, slot)` holds the native finalized header's record. A finality
update runs as three transactions; 18 refusals ran through the same path (§2). Stage 2 (one
private actor verifies a whole update and the public instance applies it) and Stage 3 (a private
consumer reads a record, or calls) ran through LEZ's privacy-preserving circuit and private
settlement in RISC Zero dev mode; real proofs were made of Stage 2 on earlier images (§3, and on
a running sequencer, §9), none of Stage 3.
Stage 4 (§5): on a mainnet block finalized on 10 November 2026, the program attests an account, a
storage slot and event logs under the record it wrote for that block, each in one public
transaction; a reference consumer credits an ERC-20 `Transfer` once, in one transaction; a
privately executed consumer verifies the same `Transfer` against the record it reads (in dev
mode, and once with a real proof on one NVIDIA L4, §5.5); 41 refusals ran through the same paths. Stage 5 (§6): on Sepolia, an instance initialised from the
bootstrap of the last period before Gloas follows every period's update to 1409 over the public
job path, its store upgrading itself to the Gloas layout at the fork of its schedule; under the
record of a Gloas-era block, which binds only the block hash, the program attests a storage slot,
an ERC-20 `Transfer` and an account with the block's RLP header as witness, and refuses the same
proofs asked as mainnet statements; the vault's deposit leaves are proven in the guest, on a
synthetic state until the vault is deployed.

**Which image these numbers come from.** Every number in this file, except those of the real
proofs and of the run on a sequencer (§9), was measured on the images built from this branch, which add the attestation, the store's
upgrade in place and attestation under Gloas records: program `9f3eb00f…`, consumer `f883e6a4…`
(`results/images.md`), in runs at commit ad40044 on a Modal container (`results/environment.md`),
not on the laptop the earlier runs used. The executor is deterministic, so the cycle counts do not
depend on the machine; the guest ELF holds the absolute paths of its sources, so the image ids are
those of a build under this checkout's path. The images before were program `2f3566d4…` and
consumer `711584ff…` (commit d498591, on which Stage 4 was first measured), before them
`cec0bc68…` and `aec03bcd…` (commit 8d0c647), and before the merge with `main` `e7cec90e…` and
`620e6390…`. Against `2f3566d4…`, the execution gas of every public transaction of §2 to §5 of a
million cycles or more changed by at most 0.2% (at most 3,216 cycles), that of the cheaper ones by
up to 7.3% (at most 5,871 cycles), and the public part of the private update of §3 by 781 cycles
(71,979 to 72,760): updates are decoded in the layout their encoding names, applying a job looks
for the store it started from, a request carries an optional header, and the program is another
binary. Between `cec0bc68…` and `2f3566d4…` the same comparison gave at most 0.08% and 4.4% (at
most 1,826 cycles), and 1,889 cycles for the private update (70,090 to 71,979): the configuration
gained the chain id (8 bytes), the record the parent hash and the chain id (40 bytes). **The two
real proofs of an update (§3) are of the earlier images**: `e7cec90e…` on the laptop CPU
(9,045,235 cycles, 54 min, 9.18 GiB, a 227,080-byte receipt) and `cec0bc68…` on one NVIDIA L4
GPU (`gpu-prover/RESULTS.md`). **The private attestation (§5.5) was proven on these images**
(consumer `f883e6a4…`), on one NVIDIA L4: 16.1 s, a 227,170-byte receipt, settled with the receipt
verified; a first proof had been made of the previous images (consumer `711584ff…`, 13.5 s). The update path of §3 has no real proof on this program image.

**On a running sequencer (§9).** The program of the image before the attestation, `cec0bc68…`,
deployed by LEZ's `program_loader`, ran the scenario of §2 and the private update of §3 on LEZ's
own v0.4.0-rc1 sequencer with a Logos Blockchain node (one validator, leader proofs in dev mode):
the 65 transactions that had a row in §2 as measured on that image had the same outcome there, 64
at the same execution gas, and a real proof made against the live instance was verified by the
sequencer and applied. The attestation (§5, §6) and the bridge program (`BRIDGE-RESULTS.md`) have not
run on a sequencer.

| operation (mainnet, 512-member committees) | transactions | largest transaction, execution gas (cycles) | total execution gas | total storage gas (bytes) |
|---|---:|---:|---:|---:|
| `Init`: bootstrap checks, then the adoption of its committee in two parts | 3 | 3,482,647 | 7,090,909 | 51,976 |
| finality update (update_1 without its next committee) | 3 | 3,723,050 | 9,224,206 | 4,451 |
| update that rotates and adopts a committee (mainnet updates 1–4; update_1 shown) | 5 | 3,733,713 | 16,087,430 | 54,491 |
| update that only supplies the next committee (mainnet update_0) | 5 | 3,697,038 | 15,993,932 | 53,675 |
| finality update as one public transaction (`VerifyAll`) | 1 | 9,121,514 | 9,121,514 | 4,097 |
| finality update verified privately (Stage 2; one private transaction) | 1 | 482,524 | 482,524 | 224,063 |
| attestation of a storage slot under a record (Stage 4; mainnet block 26,384,851) | 1 | 274,842 | 274,842 | 7,338 |
| attestation of an ERC-20 `Transfer` log (receipt with 7 logs; with 306 logs) | 1 | 96,584; 1,618,872 | 96,584; 1,618,872 | 2,623; 58,812 |
| a consumer's claim of that log: it calls the attestation and credits once | 1 | 183,538 | 183,538 | 2,768 |
| the same log attested by a privately executed consumer (one private transaction) | 1 | 421,641 | 421,641 | 224,063 |
| Sepolia: `Init` and every period's update 1378–1409 through the Gloas fork, then a finality update (Stage 5) | 166 | 3,731,712 | 525,494,412 | 1,776,529 |
| the job that upgrades the store to the Gloas layout (period 1379) | 5 | 3,731,686 | 15,879,033 | 53,249 |
| attestation of a storage slot under a Gloas record, with the block's RLP header (Sepolia block 12,105,936) | 1 | 278,253 | 278,253 | 7,633 |
| the vault's slots 0 and 1 and one deposit leaf (synthetic state under a Gloas record; each further leaf about 65,500) | 1 | 186,719 | 186,719 | 3,391 |

Every transaction of the jobs is under `TARGET_GAS_EXEC` (5,000,000), the largest at 75% (3,733,713) of
it; none is near `MAX_GAS_EXEC` (10,000,000). Every attestation is under the target too, the
largest at 32% (a log in a receipt of 306 logs). Execution gas is the cycles LEZ metered (`lee`
reports `SessionInfo::cycles`, user cycles); storage gas is the transaction's serialized size.


## 1. What was built

**The program** (`guests/program`, one RISC Zero guest; image id `9f3eb00f54f911e41710fc57d6b0258846bd4db61b57674b8a33a6457bda3e7b`, 602,912 bytes). Its actors are public
PDAs of the program (`wire/src/lib.rs`):

| actor | seed | state | written by |
|---|---|---|---|
| `instance(id)` | `H(…/instance ‖ id)` | the config, an active flag and the compact store: fork, finalized beacon header, current and next committee as *summaries* (key root + aggregate key, 80 bytes each); 433 bytes, 513 with a next committee | `Init`; `Apply` / `ApplyVerified` |
| `job(id, job_id)` | `H(…/job ‖ id ‖ job_id)` | one job: store and update digests, the signature check's phase (H(m), −key, signature: 480 bytes; then the 576-byte Miller product), the adoption's progress, what to apply; 1,281–1,441 bytes for an update that adopts a committee, 1,022–1,118 for a finality update | `JobOpen`; `Miller`, `Adopt`; cleared by `Finish` |
| `header(id, slot)` | `H(…/header ‖ id ‖ slot)` | the record of a finalized header (the instance's chain id; the beacon header, execution root, block number, block hash, parent hash, state and receipts roots, timestamp), written once; 333 bytes | `Record` |

`id = H(config ‖ checkpoint)`; `job_id = H(id ‖ store digest ‖ update digest)`.

| message | sent to | by | does |
|---|---|---|---|
| `Init` | `instance(id)` | anyone | verifies the bootstrap against the checkpoint (committee as summary), creates the instance (inactive) and opens the adoption job |
| `Open` | `instance(id)` | anyone | step 1 (`ferrata_core::steps::step1`), opens `job(id, job_id)`; valid only from the signature slot's start (validity window) |
| `Miller` | job | anyone | step 2: the Miller loops |
| `Adopt{part, keys}` | job | anyone | one part (256 keys, uncompressed) of a committee adoption |
| `Finish` | job | anyone | step 3: final exponentiation; clears the job, sends `Apply` |
| `Read`, `ReadPadded` | any actor | another actor | replies `StateReply` with the actor's state (LEZ v0.4's read idiom); `ReadPadded` carries bytes the program ignores, for the shell-cost measurement |
| `Attest` | `header(id, slot)` | anyone | proves the request's facts under the record and replies to the caller with their statement identifiers and values (§5); changes no state |
| `VerifyAll` | any actor | anyone | steps 1–3 and the adoption in one call, then `ApplyVerified` (Stage 2) |
| `JobOpen` | job | **only** `instance(id)` of this program | stores the job |
| `Apply` | instance | **only** `job(id, job_id)` of this program | applies the verified job to the store it started from; sends `Record` |
| `ApplyVerified` | instance | **only an actor of this program** (`from_own_program`) | the same, for a job verified in one call (possibly privately) |
| `Record` | header | **only** `instance(id)` of this program | writes the record once |

Every handler checks that its receiver is the PDA the message implies. Refusals exit with a code
(`env::exit(code)`); none panics. An exit code is one byte, 0 is success, and the program passes
on the core's and the step API's codes as they are, so every code has one owner:

| codes | owner | defined in | in use |
|---|---|---|---|
| 1–79 | core: the light client | `ferrata_core::Error` | 1, 10–12, 20–29, 40–43, 45, 50–52, 60–61 |
| 80–99 | step API | `ferrata_core::steps::StepError` | 80–84 |
| 100–199 | core: the execution layer (from `main`) | `ferrata_core::Error` | 100–101, 110–113, 120–125, 130–131, 140–141, 150–152, 160–163 |
| 200–219 | the program's own refusals | `wire/src/lib.rs`, `code` | 200–214 (213 and 214 are `Attest`'s: no record at the actor; a request it cannot serve) |
| 220–239 | the reference consumer (§4, §5) | `wire/src/lib.rs`, `consumer::code` | 220–229 |
| 240–255 | unassigned | | |

`wire::CODE_BLOCKS` declares these blocks, and `harness/tests/exit_codes.rs` fails if two of them
overlap, or if a code of the core (read from `error.rs`), the step API, the program or the
consumer lies outside its owner's block or does not fit a byte. Until the merge with `main` the
program's own codes were 100–112 and the consumer's 120–122; `main` gave 100–163 to the core's
execution layer, so they moved to 200–212 and 220–222. The step API's 80–84 overlapped nothing
and stayed.

**The core additions** (`../crates/ferrata-core/src/steps.rs`, `witness.rs`; relative to `main`,
`lib.rs` gains three lines and nothing else in the core changed; since the merge the core also
holds `main`'s execution layer, which the program does not call):

- `witness.rs`: `CommitteeSummary`; `CompactStore`, `CompactBootstrap`, `CompactUpdate` with
  strict encodings (fixed order, every length checked, flags 0/1, input consumed exactly; headers
  as SSZ through the existing strict decoders) and digests; `SignerWitness` (the non-signers'
  keys uncompressed, each with its Merkle branch to the committee's key root); `AdoptionProgress`
  (adoption from uncompressed keys, in parts: every key in canonical uncompressed form, on the
  curve, not the identity, its re-compression hashed into the key root; the keys summing to the
  aggregate; no per-key subgroup check).
- `steps.rs`: `step1` / `Job::step2` / `Job::step3` / `Job::adopt` / `apply`, and `initialize`.
  `apply` accepts a job only on the store whose digest it started from; each step checks the
  job's phase.
- `../crates/ferrata-spectests/tests/steps.rs`: the steps accept exactly what
  `FinalizedStore::process_update` (`Adoption::OnChain`) accepts, with the same outcome, store and
  refusal code, on all 52 sync cases (160 updates: 91 applied, 69 refused), on the mainnet replay
  (adoption in 1, 2 and 4 parts) and on 21 variants of mainnet update_1 (19 refused: 17 single
  changes, a replay and an update out of order; 2 applied: as served and as a finality update); step 2's
  product equals `Blst::miller_half` byte for byte on every mainnet update; skipping, replaying or
  moving a step to another store is refused; the witness adoption agrees with
  `check_committee_onchain` on 9 real committees and 6 crafted ones (wrong aggregate, identity,
  order-3, off-curve, outside the subgroup — accepted by both —, empty).

**The harness** (`harness/`). Each transaction is signed, pays a fee, and is settled by
`settle::settle_charged_transaction` / `settle_private_transaction`: copies of LEZ's
`chain_state::apply` functions of the same names (with `classify` and the fee invocations of
`common::transaction`), which reserve the fee, run the action through
`lee::ValidatedStateDiff::from_public_transaction_metered` (or `…privacy_preserving…_metered`)
under the declared gas limit, apply the diff with `V03State::apply_state_diff`, and refund. They
are copied rather than linked because `chain_state` and `common` pull in the Bedrock node
(libp2p, RocksDB, circuit builds); every call into `lee` is the sequencer's. The state holds LEZ's
fee program and accounts at genesis; each transaction is settled in a block of its own with a gas
limit of `MAX_GAS_EXEC` (10M). `trace.rs` replays each action with `lee_core`'s own
`WholeExecution` and a backend that runs each guest as `lee`'s `PublicBackend` does, to split the
cycles by handler; every test asserts that the split sums to what LEZ charged.

## 2. Stage 1: the public path, as LEZ charges it

Tables generated by `cargo run -p harness --bin tables` into `results/` (each with a second table
of every handler execution: cycles, segments, input, message, pre-state, journal and post-state
bytes). Execution gas = the cycles LEZ metered for the transaction (all its handlers), clamped to
its gas limit; storage gas = its borsh size as a `LeeTransaction`; base fee = 8 × (execution gas +
storage gas), the genesis base fees (`BASE_FEE_EXEC_MIN` = `BASE_FEE_STOR_MIN` = 8).

### Init and the five mainnet updates (`results/stage1-replay.md`)

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init Init | Init 190,351 → JobOpen 18,861 | 209,212 | 4% | 1,888 | 1,688,800 | applied |
| 2 | Init adopt 1/2 | Adopt 3,399,050 | 3,399,050 | 68% | 24,980 | 27,392,240 | applied |
| 3 | Init adopt 2/2 | Adopt 3,409,191 → Apply 56,923 → Record 16,533 | 3,482,647 | 70% | 25,108 | 28,062,040 | applied |
| 4 | update_0 Open (step 1) | Open 2,128,330 → JobOpen 26,275 | 2,154,605 | 43% | 2,853 | 17,259,664 | applied |
| 5 | update_0 Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
| 6 | update_0 adopt 1/2 | Adopt 3,410,904 | 3,410,904 | 68% | 24,980 | 27,487,072 | applied |
| 7 | update_0 adopt 2/2 | Adopt 3,418,098 | 3,418,098 | 68% | 24,980 | 27,544,624 | applied |
| 8 | update_0 Finish (step 3) | Finish 3,653,799 → Apply 43,239 | 3,697,038 | 74% | 463 | 29,580,008 | applied |
| 9 | update_1 Open (step 1) | Open 2,185,153 → JobOpen 26,275 | 2,211,428 | 44% | 3,605 | 17,720,264 | applied |
| 10 | update_1 Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
| 11 | update_1 adopt 1/2 | Adopt 3,410,904 | 3,410,904 | 68% | 24,980 | 27,487,072 | applied |
| 12 | update_1 adopt 2/2 | Adopt 3,418,098 | 3,418,098 | 68% | 24,980 | 27,544,624 | applied |
| 13 | update_1 Finish (step 3) | Finish 3,653,799 → Apply 63,381 → Record 16,533 | 3,733,713 | 75% | 527 | 29,873,920 | applied |
| 14 | update_2 Open (step 1) | Open 2,185,727 → JobOpen 26,275 | 2,212,002 | 44% | 3,615 | 17,724,936 | applied |
| 15 | update_2 Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
| 16 | update_2 adopt 1/2 | Adopt 3,410,904 | 3,410,904 | 68% | 24,980 | 27,487,072 | applied |
| 17 | update_2 adopt 2/2 | Adopt 3,418,098 | 3,418,098 | 68% | 24,980 | 27,544,624 | applied |
| 18 | update_2 Finish (step 3) | Finish 3,653,799 → Apply 63,381 → Record 16,533 | 3,733,713 | 75% | 527 | 29,873,920 | applied |
| 19 | update_3 Open (step 1) | Open 2,130,193 → JobOpen 26,275 | 2,156,468 | 43% | 2,852 | 17,274,560 | applied |
| 20 | update_3 Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
| 21 | update_3 adopt 1/2 | Adopt 3,410,904 | 3,410,904 | 68% | 24,980 | 27,487,072 | applied |
| 22 | update_3 adopt 2/2 | Adopt 3,418,098 | 3,418,098 | 68% | 24,980 | 27,544,624 | applied |
| 23 | update_3 Finish (step 3) | Finish 3,653,799 → Apply 63,381 → Record 16,533 | 3,733,713 | 75% | 527 | 29,873,920 | applied |
| 24 | update_4 Open (step 1) | Open 2,158,156 → JobOpen 26,275 | 2,184,431 | 44% | 3,221 | 17,501,216 | applied |
| 25 | update_4 Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
| 26 | update_4 adopt 1/2 | Adopt 3,410,904 | 3,410,904 | 68% | 24,980 | 27,487,072 | applied |
| 27 | update_4 adopt 2/2 | Adopt 3,418,098 | 3,418,098 | 68% | 24,980 | 27,544,624 | applied |
| 28 | update_4 Finish (step 3) | Finish 3,653,799 → Apply 63,381 → Record 16,533 | 3,733,713 | 75% | 527 | 29,873,920 | applied |

### A finality update, and an update that only supplies the next committee (`results/stage1-finality.md`)

update_1 without its next committee (a finality update, after update_0), then update_1 as served,
which then only supplies the period-1901 committee (it does not advance the header):

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | update_1 as finality update Open (step 1) | Open 2,170,091 → JobOpen 23,102 | 2,193,193 | 44% | 3,525 | 17,573,744 | applied |
| 2 | update_1 as finality update Miller (step 2) | Miller 3,307,963 | 3,307,963 | 66% | 399 | 26,466,896 | applied |
| 3 | update_1 as finality update Finish (step 3) | Finish 3,647,613 → Apply 58,904 → Record 16,533 | 3,723,050 | 74% | 527 | 29,788,616 | applied |
| 4 | update_1 (next committee only) Open (step 1) | Open 2,183,622 → JobOpen 26,275 | 2,209,897 | 44% | 3,605 | 17,708,016 | applied |
| 5 | update_1 (next committee only) Miller (step 2) | Miller 3,313,287 | 3,313,287 | 66% | 399 | 26,509,488 | applied |
| 6 | update_1 (next committee only) adopt 1/2 | Adopt 3,410,904 | 3,410,904 | 68% | 24,980 | 27,487,072 | applied |
| 7 | update_1 (next committee only) adopt 2/2 | Adopt 3,418,098 | 3,418,098 | 68% | 24,980 | 27,544,624 | applied |
| 8 | update_1 (next committee only) Finish (step 3) | Finish 3,653,799 → Apply 43,239 | 3,697,038 | 74% | 463 | 29,580,008 | applied |

### A finality update in one public transaction (`results/stage1-one-transaction.md`)

`VerifyAll` at a public actor of the program does steps 1–3 in one handler and hands the job to
the instance (`ApplyVerified`, accepted from an actor of the program):

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | finality update in one transaction (VerifyAll) | VerifyAll 9,048,754 → ApplyVerified 56,227 → Record 16,533 | 9,121,514 | 182% | 4,097 | 73,004,888 | applied |

### Refusals (`results/stage1-refusals.md`)

Each against an instance that has applied update_0. A refusal leaves every state as it was (the
tests check the store); the fee is paid and the payer's nonce advances.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | forged signature: Open (passes) | Open 2,170,217 → JobOpen 23,102 | 2,193,319 | 44% | 3,525 | 17,574,752 | applied |
| 2 | forged signature: Miller (passes) | Miller 3,307,963 | 3,307,963 | 66% | 399 | 26,466,896 | applied |
| 3 | forged signature: Finish | Finish 3,636,217 | 3,636,217 | 73% | 463 | 29,093,440 | refused: exit 40 (SignatureInvalid) |
| 4 | malformed signature: Open | Open 356,015 | 356,015 | 7% | 3,525 | 2,876,320 | refused: exit 41 (SignatureMalformed) |
| 5 | duplicate Open (same job) | Open 2,170,091 → JobOpen 16,411 | 2,186,502 | 44% | 3,525 | 17,520,216 | refused: exit 207 (JobExists) |
| 6 | Finish before Miller (skipped step) | Finish 24,159 | 24,159 | 0% | 463 | 196,976 | refused: exit 81 (StepError::Phase) |
| 7 | Miller replayed | Miller 21,155 | 21,155 | 0% | 399 | 172,432 | refused: exit 81 (StepError::Phase) |
| 8 | Finish replayed after the job applied | Finish 5,981 | 5,981 | 0% | 527 | 52,064 | refused: exit 205 (NoJob) |
| 9 | Open an applied update again | Open 210,294 | 210,294 | 4% | 3,525 | 1,710,552 | refused: exit 52 (NotNewer) |
| 10 | Apply sent as the transaction root | Apply 17,630 | 17,630 | 0% | 937 | 148,536 | refused: exit 202 (WrongSender) |
| 11 | Apply from another program's actor | scripted 13,436 → Apply 18,483 | 31,919 | 1% | 1,157 | 264,608 | refused: exit 202 (WrongSender) |
| 12 | ApplyVerified from another program's actor | scripted 13,172 → ApplyVerified 16,099 | 29,271 | 1% | 1,125 | 243,168 | refused: exit 202 (WrongSender) |
| 13 | JobOpen from another program's actor | scripted 13,372 → JobOpen 10,892 | 24,264 | 0% | 1,226 | 203,920 | refused: exit 202 (WrongSender) |
| 14 | Record from another program's actor | scripted 9,348 → Record 9,155 | 18,503 | 0% | 739 | 153,936 | refused: exit 202 (WrongSender) |
| 15 | update for another chain's instance: Finish | Finish 3,638,435 | 3,638,435 | 73% | 463 | 29,111,184 | refused: exit 40 (SignatureInvalid) |
| 16 | instance A's job id sent to instance B | Finish 5,981 | 5,981 | 0% | 464 | 51,560 | refused: exit 205 (NoJob) |
| 17 | Open before its signature slot (validity window) | Open 2,170,091 → JobOpen 23,102 | 10,000,000 | 200% | 3,525 | 80,028,200 | reverted: Execution outside of the validity window |
| 18 | Init of an existing instance | Init 18,188 | 18,188 | 0% | 1,824 | 160,096 | refused: exit 203 (AlreadyInitialized) |

### Against the bare guest

`../bench/RESULTS.md` measured the same work in bare RISC Zero 3.0.5 guests (executor, accelerated
blst and SHA-256, core of commit f030475, RISC Zero Rust 1.88). Here, per handler:

| work | bare guest (`../bench/`), user cycles | on LEZ: the handler(s), user cycles as LEZ metered them |
|---|---:|---|
| step 1 | 2,305,240 (execution total: decodes the 26.9 KB SSZ update and a 24.6 KB committee, then everything before the Miller loops) | `Open` 2,128,330–2,183,272, then `JobOpen` 23,102–26,275. The program decodes a 2.9–3.6 KB compact update and checks the non-signers' branches (two per update here) instead of decoding a committee. |
| step 2 | 4,203,683 (two single Miller loops); the two-pair loop alone: 3,251,802 (phase 8m) | `Miller` 3,307,963–3,313,287 (the two-pair loop since commit 95c56e9; 4,142,918–4,147,152 with two loops, measured on an image before that commit and not again) |
| step 3 | 3,597,597 (final exponentiation, execution total) | `Finish` 3,647,613–3,653,799 (also checks that the stored product is canonical, and encodes the job for `Apply`), then `Apply` 43,239–63,381 and `Record` 16,533 |
| adoption, 512 keys | 5,685,329 (10b0: 4,562,038 for the keys, plus 3a: 1,123,291 for the committee root) | `Adopt` 3,399,050–3,418,092, twice: 6,808,233–6,828,990 |
| a whole finality update | 10,369,179 (one execution: saved state and SSZ update decoded, policy, new root) | `VerifyAll` 9,046,319, then `ApplyVerified` 56,227 and `Record` 16,533 |

The steps cost on LEZ about what they cost in a bare guest: step 1 less (a smaller input), step 3
1.5% more. The adoption costs 1.12–1.14M more than the bench's two measurements added up. About
0.3M of that is the shell (two inputs of 25 KB, below); the remaining 0.8M (about 12% per part)
was not broken down. Candidates: per-part bookkeeping (the running sum decoded and re-serialized,
which costs an inversion; the chunk's merkleization) and differences between this code and the
bench's measurement loop. The guests here are built with RISC Zero Rust 1.91.1, the bench's with
1.88.

### What the LEZ v0.4 shell costs (`results/shell-costs.md`)

A handler reads its whole `ReceiveInput` (receiver, sender, pre-state, message), and its journal
is the whole `Transition`: the input echoed, plus the response. To price that with this program,
a public consumer actor reads a header record with `ReadPadded`, whose padding grows and which
the program ignores; the header's handler then differs only in the bytes it receives and echoes.

| padding bytes | `ReadPadded` handler: input bytes | journal bytes | cycles | the consumer's handler, which also sends the padding: cycles |
|---:|---:|---:|---:|---:|
| 0 | 476 | 906 | 12,005 | 16,360 |
| 8,192 | 8,668 | 9,098 | 58,700 | 97,362 |
| 24,576 | 25,052 | 25,482 | 151,765 | 257,444 |
| 49,152 | 49,628 | 50,058 | 291,388 | 497,575 |

Linear to within 0.4% for the program's handler: a handler pays **5.7 cycles per byte it
receives** (reading the input, decoding it, echoing it into the journal and hashing the journal)
and about **4.1 more per byte it sends on** (this consumer pays 9.8 per byte received and sent,
within 1.2% of linear); a handler that does almost nothing costs 11,000–26,000 cycles (`Read`
11,878, `Record` 16,531, `JobOpen` 18,861–26,275). A public consumer's whole read of a record
(consumer → `Read` → `StateReply` → consumer) costs 38,115 cycles. At 5.7 cycles per byte, a handler that loaded a
24.6 KB committee from its state would pay about 0.14M cycles for it on every call, and the
journal echo makes every byte of a message count once on receipt and again when sent on.
Storage gas, by contrast, counts only the transaction's bytes
(`lez/programs/fee/core/src/assess.rs:33–38`): actor state a handler writes costs only the cycles
to write it.


## 3. Stage 2: the whole update in one privately executed actor

**Question.** Can one privately executed actor of this program verify a whole update and hand the
result to the public instance, and can the instance tell that the message came from this
program's code although the sender ran privately?

**Answer, from the source (LEZ v0.4.0-rc1, paths relative to `upstream/lez-v0.4.0-rc1/`): yes, by
program, not by account.**

1. A private actor's message to a public actor is recorded in the proof's public output, the
   boundary, with the sender as presented: `lee/state_machine/core/src/execution_state.rs:781`
   (`PrivateMode::deliver_to_public` pushes `BoundaryStep::PrivateToPublic(executor.disclose(…))`).
   The presentation replaces at most the *account*: `Executor::sender`,
   `execution_state.rs:1409–1426`, returns the actor itself or
   `Actor::new(alias, actor.program_account_id)` — the program account id is kept.
2. At settlement the public part replays that delivery with exactly that sender
   (`PublicMode::replay_proven_calls`, `execution_state.rs:842–849`), and the receiving handler
   sees it as `ReceiveInput::from` (`execution_state.rs:1307`).
3. The private transition itself is accepted by the circuit only with a receipt of the claimed
   image for the receiver's program account: the circuit builds `image_id_by_account_id` from the
   program-image witnesses (`lee/privacy_preserving_circuit/src/main.rs:49`) and verifies every
   private transition against it (`lee/privacy_preserving_circuit/src/private_backend.rs:68–77`:
   `env::verify(image_id, transition)`). The sequencer checks every `Disclosed` claim against the
   image deployed at that account (`lee/state_machine/src/validated_state_diff/mod.rs:630–637`);
   an `Undisclosed` claim is a membership proof of the account's *immutable* header mirror
   (`lee/state_machine/core/src/circuit_io.rs:85–86`), whose root the sequencer checks is known
   (`validated_state_diff/mod.rs:641`). Shadow programs get their own derived account ids
   (`privacy_preserving_circuit/src/main.rs:54`) and cannot name a deployed program's.

So `from.program_account_id == the program` authenticates "this message was produced by this
program's code" (its current image if the program is upgradable; this one is deployed
immutable). The sender's *account* is not authenticated: it is any private account, or a blinded
alias of one. The program therefore accepts `ApplyVerified` from any actor of its own program
(`ReceiveInput::from_own_program`, `lee/state_machine/core/src/program/mod.rs:390`), and the only
handler of the program that sends it, `VerifyAll`, sends it after the whole verification
(`guests/program/src/main.rs`, `verify_all`). The job carries the digest of the store it was
verified against, so it applies only to the instance's current store, once.

**Demonstrated through the state machine** (`harness/tests/private_placement.rs`, RISC Zero dev
mode: the private transitions and the privacy-preserving circuit are executed, not proven; the
settlement, the public part's execution and its metering are LEZ's; the test's output is in
`results/harness-tests-dev-mode.log`):

- A finality update (mainnet update_1 without its next committee) verified by `VerifyAll` at a
  private account's actor of the program, against an instance that had applied update_0 through
  the public path: the instance's store afterwards equals the native `FinalizedStore`'s, and the
  record is written.
- A handoff (update_1 as served, with its 512 keys in the same private call): applied likewise.
- A private actor of *another* program (LEZ's `scripted` test program) sending the same
  `ApplyVerified`: an honest prover's simulation stops at the instance's refusal (exit 202,
  `WrongSender`); a proof made without simulating the public part
  (`execute_and_prove_with_cross_messages`) settles as a revert to its fee fallback — the
  instance refused the message (exit 202 after 16,099 cycles) and the transaction paid only its
  fee (execution gas 425,862, storage gas 224,063).
- A forged update (update_2's signature on update_1) cannot be proven: the private transition
  exits with code 40 (`SignatureInvalid`) after 9,028,838 cycles, and LEZ's prover refuses a
  non-zero exit (`lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs:674–697`).

**What LEZ charges the private transaction** (`settle_private_transaction`,
`lez/chain_state/src/apply.rs:435–556`): a flat `PRIVATE_VERIFY_GAS` = 409,764 execution gas for the
receipt (`lez/programs/fee/core/src/market.rs:49`), plus the cycles of its public part, plus a flat
`PRIVATE_GAS_STOR` = 224,063 storage gas (`market.rs:51–56`). Dev mode, from the test's output; the
base fee is 8 × (execution gas + storage gas). The settlements of the two real proofs (below, of
the earlier images) charged the finality update 479,854 execution gas: their public part took
70,090 cycles.

| private transaction | public part (cycles, metered) | execution gas | storage gas | base fee at genesis prices |
|---|---:|---:|---:|---:|
| finality update (`ApplyVerified` → `Record`) | 72,760 | 482,524 | 224,063 | 5,652,696 |
| update handing over a committee, no new header (`ApplyVerified`) | 40,155 | 449,919 | 224,063 | 5,391,856 (derived) |

For comparison, the same finality update through the public job costs 9,224,206
execution gas and 4,451 storage gas in three transactions (§2). The private
placement trades on-chain execution (about 19× less) for storage gas (about 50× more: a
succinct receipt is 223,551 bytes by LEZ's constant) and for a proof made off chain. With
`MAX_GAS_STOR` = 1,000,000 per block, at most four such transactions fit in a block.

What the transaction makes public (the finality update on the image of this branch, from
`results/prove-private-dry-run.log`, `describe_public_outputs` in `harness/src/private.rs`; a real
proof publishes the same fields, with a succinct receipt where the dry run has a fake one):

```
public_context.actors (3): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB, ferrata@8eFzBrUp5Avp8A6BgTmExBiCJtPTPrp2AdNtVb1L96tK
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (4 steps):
  - PrivateToPublic from ferrata@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to ferrata@8eFzBrUp5Avp8A6BgTmExBiCJtPTPrp2AdNtVb1L96tK: ApplyVerified { instance_id: ce624027489c, job: 470 bytes } (507 bytes)
  - EndPublicSubtree
  - PrivateToPublic from native@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: ferrata, image_id: 9f3eb00f54f911e41710fc57d6b0258846bd4db61b57674b8a33a6457bda3e7b }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1233 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: Some(1793602427000), to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0
```

The proof claims the program's image (`Disclosed`), the boundary shows the program, the instance
and the header record it writes, and the sender only as a blinded alias, which is also the fee
payer's alias.

**Two real proofs, both of images before this branch: `e7cec90e…` on the laptop CPU, `cec0bc68…` on a GPU.** The image `cec0bc68…` (the program before attestation was added) was proven on one NVIDIA L4 (49.5 s for `execute_and_prove`, 55.7 s for the whole run; receipt 227,080 bytes, transaction 231,034 bytes), and the transaction settled through LEZ's state machine with the receipt verified, at 479,854 execution gas and 224,063 storage gas: `results/prove-private.log`, `results/stage2-private-tx.borsh`, `gpu-prover/RESULTS.md`. The binary built from this branch holds another program image (`2f3566d4…`) and refuses that transaction; this path has not been proven on the image of this branch (a private attestation has: §5.5). The rest of this paragraph is the CPU proof of `e7cec90e…`. The one real proof
of this path (`harness/src/bin/prove_private.rs`, `RISC0_DEV_MODE=0`, CPU only) proved the
program image `e7cec90e…` (the image of commit 83b4b55, before the merge) and was settled through
LEZ's state machine: proven, then settled with the real receipt verified (`Proof::is_valid_for`,
`validated_state_diff/mod.rs` via `check_privacy_preserving_circuit_proof_is_valid`), and
applied; afterwards the instance's store equalled the native store and the record was written.
Its transaction is the one above with the earlier image's id and the account ids derived from
it. Log: `results/prove-private-e7cec90e.log`. Every figure in this table is the earlier image's:

| earlier image `e7cec90e…`, commit 83b4b55 | |
|---|---|
| proven | the program's `VerifyAll` transition (step 1, Miller loops, final exponentiation, `ApplyVerified`) at a private account's actor, the fee transfer from that account, and LEZ's privacy-preserving circuit over both (`lee::execute_and_prove`, `ProverOpts::succinct()`) |
| the program's private transition | 9,045,235 user cycles in 10 segments (9,699,328 cycles padded); on the image of this branch the executor counts 9,046,319, in 10 segments of the same sizes (`results/prove-private-dry-run.log`) |
| wall clock | **3,240.5 s** for `execute_and_prove` (54 min 04 s for the whole run, setup and settlement included) |
| CPU | AMD Ryzen 9 7940HS (8 cores, 16 threads), no GPU: 39,206 s of user time, 1,211% average |
| peak memory | **9.18 GiB** maximum resident set (`/usr/bin/time -v`: 9,628,928 kB); a 2-second `ps` sample of all `r0vm` processes peaked at 10.2 GiB (10,710,560 kB) |
| receipt | **227,080 bytes** (the succinct `InnerReceipt`, borsh); the transaction is 231,034 bytes. LEZ charges a flat 224,063 bytes of storage gas whatever the size (`PROOF_BYTES` + 512) |
| settlement | execution gas 479,854 (`PRIVATE_VERIFY_GAS` 409,764 + public part 70,090 cycles), storage gas 224,063, base fee 5,631,336; the dev-mode settlement on the image of this branch charges 482,524 (its public part takes 72,760 cycles) |
| load | not recorded during the run (2026-11-09, 22:53–23:47). For 8 of its 54 minutes my own executor-only test suites ran beside it at `nice 19`; whether other agents used the CPU is not known. Load average when the result was collected (01:51): 2.46 |

Proving time is that of a single run on a shared machine, not a benchmark. A proof of
`cec0bc68…` was started on the laptop (2026-11-10, 08:27) and stopped after about 29 minutes to
free the machine; it was made on the GPU instead.

**Proving the image of this branch elsewhere (not done).** What a run elsewhere needs, as the GPU
run of `cec0bc68…` had it; `results/prove-private-dry-run.log` shows these commands on the image
of this branch in dev mode:

- **The image.** Program `9f3eb00f54f911e41710fc57d6b0258846bd4db61b57674b8a33a6457bda3e7b`
  (602,912 bytes; consumer `f883e6a4…`). The guest ELF holds the absolute paths of the sources it
  was built from (the cargo registry, the LEZ clone, this checkout) in its panic locations, so a
  rebuild under other paths is another image. Use the binary built here,
  `lez/target/debug/prove_private` (it embeds both guests, LEZ's privacy-preserving circuit and
  its programs), or rebuild under the same paths; it prints the image it holds first, and
  `FERRATA_EXPECT_IMAGE_ID` makes it stop at once on any other.
- **What it runs on.** Linux x86-64 with glibc 2.39 or newer, and r0vm 3.0.5:
  `RISC0_SERVER_PATH` selects the r0vm to use (a CUDA build for a GPU); without it, rzup's r0vm
  3.0.5, then `r0vm` on `PATH`.
- **What it reads.** `bootstrap.ssz` and `update_0.ssz` to `update_4.ssz` of
  `crates/ferrata-spectests/fixtures/mainnet-p1899`, from `FERRATA_FIXTURE_DIR` (default: this
  checkout); their SHA-256 sums are compiled in. Nothing else.
- **The command:**
  ```sh
  FERRATA_EXPECT_IMAGE_ID=9f3eb00f54f911e41710fc57d6b0258846bd4db61b57674b8a33a6457bda3e7b \
  FERRATA_FIXTURE_DIR=/path/to/mainnet-p1899 RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/path/to/r0vm \
  /usr/bin/time -v ./prove_private --tx-out stage2-private-tx.borsh > prove-private.log 2>&1
  ```
  It proves, settles the proof there through LEZ's settlement (receipt verified), checks the
  store and the record, and writes the proven transaction, its receipt included (about 231 KB).
- **The receipt, back here.** `RISC0_DEV_MODE=0 target/debug/prove_private --settle FILE`, from
  `lez/`, settles a proven transaction again through LEZ's state machine, verifying the receipt,
  in seconds; it refuses a fake receipt and any transaction that does not claim the image the
  binary holds. `results/stage2-private-tx.borsh` and `results/prove-private.log` are still the
  GPU proof of `cec0bc68…`, which a binary of this branch therefore refuses; a proof of this
  branch's image would replace them.


## 4. Stage 3: a private consumer reads a record; the same consumer calling instead

`guests/consumer` is a program of its own; this section uses its minimal reader (§5 has its
claims and its private attestation). Run privately, `ReadRecord` sends `Read`
to `header(id, slot)` of the Ferrata program; the program replies with `StateReply` (the
`ReadState` → `StateReply` idiom of LEZ v0.4's native token and fee programs); the consumer
checks that the reply comes from the actor it asked and keeps the record in its private state.
`ReadPadded` sends the query along with the read instead (the program ignores it), to show what a
call exposes; the real call, `Attest` with a request and its proofs, is in §5.
`harness/tests/private_consumer.rs` runs both through `execute_and_prove` and LEZ's private
settlement (dev mode) and prints every public field of each transaction.

**Mechanism, from the source.** The read leaves the private part as a `PrivateToPublic` boundary
step (`execution_state.rs:781`) and the reply enters it as a `PublicToPrivate` step
(`execution_state.rs:805`). The prover predicts the reply from the public state it assumes; at
settlement LEZ runs the public handler again and refuses the transaction if its reply differs
from the prediction (`execution_state.rs:939–940`, `CrossMessageMismatch`). A record written once
keeps the prediction valid; `instance(id)`, which every update changes, would not.

**What became public — the read** (record of slot 15,564,800, the consumer's image disclosed):

```
public_context.actors (2): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (6 steps):
  - PrivateToPublic from consumer@61yQrZPL8iZ8NbnqXkUEs1JR6DiuSkX3bWxdZSjYSWEu to ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB: Read (1 bytes)
  - PublicToPrivate from ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB to consumer@61yQrZPL8iZ8NbnqXkUEs1JR6DiuSkX3bWxdZSjYSWEu: StateReply { state: 333 bytes } (337 bytes)
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@61yQrZPL8iZ8NbnqXkUEs1JR6DiuSkX3bWxdZSjYSWEu to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: consumer, image_id: f883e6a4dbd8e49439465f918884abe1379b02276ff88676f0a4bf958ee49068 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1647 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0

The 8717 serialized bytes of the transaction were searched for the consumer's private inputs:
- the query (75 bytes): absent
- the account id the consumer acts for (32 bytes): absent
- the authorization secret key (32 bytes): absent
- the nullifier secret key (32 bytes): absent
- the nullifier public key (32 bytes): absent
- the viewing public key (its first 32 of 1,184 bytes) (32 bytes): absent
- the viewing key seed d (32 bytes): absent
- the viewing key seed z (32 bytes): absent
```

**The read with the consumer's image undisclosed** (`ProgramKind::Undisclosed`: an immutable
program proven by membership of its header mirror, `circuit_io.rs:85–86`):

```
public_context.actors (2): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (6 steps):
  - PrivateToPublic from consumer@AodAGCgEG5tEZ9G66zav6VgeBX9hfvyxnimA6U3L6j1C to ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB: Read (1 bytes)
  - PublicToPrivate from ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB to consumer@AodAGCgEG5tEZ9G66zav6VgeBX9hfvyxnimA6U3L6j1C: StateReply { state: 333 bytes } (337 bytes)
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@AodAGCgEG5tEZ9G66zav6VgeBX9hfvyxnimA6U3L6j1C to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Undisclosed { commitment root: 68bcf51bd719dd2b5a9bb4d3f848b8fd3df3abc5e6942b89bdf655a853b87141 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1647 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0

The 8653 serialized bytes of the transaction were searched for the consumer's private inputs:
- the query (75 bytes): absent
- the account id the consumer acts for (32 bytes): absent
- the authorization secret key (32 bytes): absent
- the nullifier secret key (32 bytes): absent
- the nullifier public key (32 bytes): absent
- the viewing public key (its first 32 of 1,184 bytes) (32 bytes): absent
- the viewing key seed d (32 bytes): absent
- the viewing key seed z (32 bytes): absent
```

**The call** (`ReadPadded` with a query naming a contract, a storage slot and a deposit):

```
public_context.actors (2): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (6 steps):
  - PrivateToPublic from consumer@DYMXat67yG3gSP7GFoRaTUEHduv3mLWBRr5UousHBSho to ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB: ReadPadded { padding: 0x73746f72616765283078303030303030303032313961623534303335366342423833394362653035333033643737303546612c20736c6f74203729203d206465706f736974202334323432 } (80 bytes)
  - PublicToPrivate from ferrata@98nuLdy4rvh69odJA3nszo4zfisBxqtF27qVFuuKgvaB to consumer@DYMXat67yG3gSP7GFoRaTUEHduv3mLWBRr5UousHBSho: StateReply { state: 333 bytes } (337 bytes)
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@DYMXat67yG3gSP7GFoRaTUEHduv3mLWBRr5UousHBSho to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: consumer, image_id: f883e6a4dbd8e49439465f918884abe1379b02276ff88676f0a4bf958ee49068 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1568 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0

The 8717 serialized bytes of the transaction were searched for the consumer's private inputs:
- the query (75 bytes): FOUND
- the account id the consumer acts for (32 bytes): absent
- the authorization secret key (32 bytes): absent
- the nullifier secret key (32 bytes): absent
- the nullifier public key (32 bytes): absent
- the viewing public key (its first 32 of 1,184 bytes) (32 bytes): absent
- the viewing key seed d (32 bytes): absent
- the viewing key seed z (32 bytes): absent
```

The complete serialized transactions are in `results/stage3-read-tx.hex`,
`results/stage3-read-undisclosed-tx.hex` and `results/stage3-call-tx.hex` (with the decoded fields
in the matching `*-public-outputs.txt`). They are dev-mode transactions: their proof field holds
RISC Zero's fake receipt (4,383–4,415 bytes here), where a real one holds a succinct receipt (227,080
bytes in §3, for the earlier images). The search covers every byte, the fake receipt included; the tests fail if a
private input of the reader is found, or if anything but the query is found in the call.

**Finding.** Reading exposes *which* record was read (the `Read` to `header(id, slot)` and the
whole record in the `StateReply`), the consumer's program (as the read's sender, even when its
image claim is undisclosed), and an alias of its account; it does not expose what the consumer
does with the record or its query. Calling exposes the query itself, in the clear, in the
transaction (the test asserts the query's bytes are in the serialized transaction). This
supports the proposal's rule — a private consumer must read, not call — and qualifies it: the
slot read is public, so a consumer that must hide which block it proves against should read a
record many consumers read (the newest), as the proposal's EIP-2935 hops do.

Charged (dev mode; flat proof and storage costs as in §3): the read execution gas 421,641 (PRIVATE_VERIFY_GAS 409,764 + public part 11,877 cycles), storage gas 224,063. The call: execution gas 422,023 (PRIVATE_VERIFY_GAS 409,764 + public part 12,259 cycles), storage gas 224,063.


## 5. Stage 4: attestation through the program, on a real mainnet block

**Question.** Can a LEZ program act on a fact about Ethereum that is proven inside its own
transaction against a record the instance verified, and can a consumer executed privately do the
same without publishing what it proves?

**Answer: yes, both, on real data.** The program's `Attest` handler proves accounts, storage
slots and event logs of mainnet block 26,384,851 under the record the instance wrote for it; a
reference consumer credits an ERC-20 `Transfer` of that block once, in one transaction; and a
privately executed consumer verifies the same `Transfer` against the record it reads, with nothing
but the read leaving its private part. Everything in this section ran through LEZ's settlement in
the in-process harness, on a Modal container (16 CPUs; `results/environment.md`), not on the
laptop; the executor is deterministic, so the cycle counts do not depend on the machine. Private
parts ran in RISC Zero dev mode: executed, not proven.

**The data** (`../crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`; its `SOURCE.md`
lists every request, `fixture.json` every choice). `scripts/fetch-attest-fixture.py` fetched it on
10 November 2026 between 13:49:32 and 13:49:35 UTC from public endpoints, without an API key and
with a generic User-Agent: the finality update and a bootstrap from a beacon node
(`/eth/v1/beacon/light_client/finality_update`, `/bootstrap/{root}`), and from execution nodes
`eth_getProof`, `eth_call`, `eth_getBlockReceipts`, `eth_getCode` and `eth_getBlockByNumber`; a
second beacon and a second execution provider confirm the block roots.

| | |
|---|---|
| sync-committee period | 1907 |
| bootstrap (the checkpoint `Init` trusts) | slot 15,622,144 |
| finality update | attested slot 15,624,544, signature slot 15,624,545, 505 of 512 signers |
| finalized header | slot 15,624,480, execution block `n` = 26,384,851 (531 receipts, 14 of them failed, 1,477 logs) |
| execution head when the proofs at `n` were fetched | 26,384,915, 64 blocks later |
| older block for the history hop | `m` = n − 16 = 26,384,835 |

The instance is initialized from the bootstrap (three transactions) and verifies the finality
update through the public job (three transactions: `Open`, `Miller`, `Finish`), as in §2. After
that `header(id, 15,622,144)` and `header(id, 15,624,480)` hold their records, and every
attestation below is proven against the second: no test supplies a root.

**The interface** (`wire/src/lib.rs`, module `attest`). `Attest { request }` is sent to
`header(id, slot)`. A request names the instance and the chain id the caller is bound to, and
facts of one execution block with their Merkle-Patricia proofs: accounts with storage slots under
them (`eth_getProof`'s `accountProof` and `storageProof`), event logs by transaction index and
position in the receipt (with the receipt's proof in the receipts trie), leaves of a SHA-256
accumulator whose root one of the request's slots holds, conditions on each fact (`=, ≠, <, ≤, >,
≥` against a constant), relations between two facts, and optionally EIP-2935 hops from the
record's block to an older one. The handler checks, in this order: that its actor holds a record
(213) and is the `header(id, slot)` that record belongs to (201, a second line of defence: only
`Record`, accepted from `instance(id)` alone, writes a record, and only there); that the request names the
record's instance (211) and chain id (140); each hop; each proof against the state root or the
receipts root of the resulting block, each condition and each relation; each leaf against the
slot it names. It changes no state. To a caller it replies, in the same transaction, with an
`Attestation`: the instance, the chain id, the block, and for each fact its statement identifier
(`docs/design/attestation.md` §5.2) and its values. Sent as a transaction's root, with nobody to
reply to, it emits the identifiers as an event (`FRT-ATT1`). A refusal exits with a code, the
core's own for whatever failed, and with it the whole transaction reverts.

`attest::evaluate` is that verification as one function of the wire crate. The program's handler
runs it, a private consumer runs the same function on the record it read, and the harness runs it
natively to know what the program must answer.

### Accepted attestations, each sent as a transaction's root (`results/stage4-attest.md`)

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | account: the WETH contract (nonce, balance, storage root, code hash) | Attest 167,882 | 167,882 | 3% | 4,424 | 1,378,448 | applied |
| 2 | storage slot: a WETH balance, value >= 1 | Attest 274,842 | 274,842 | 5% | 7,338 | 2,257,440 | applied |
| 3 | storage slot: an absent slot is zero | Attest 269,564 | 269,564 | 5% | 7,291 | 2,214,840 | applied |
| 4 | account: an address without an account | Attest 150,757 | 150,757 | 3% | 4,150 | 1,239,256 | applied |
| 5 | event log: an ERC-20 Transfer (receipt with 7 logs) | Attest 96,584 | 96,584 | 2% | 2,623 | 793,656 | applied |
| 6 | event log: a Transfer in the block's largest receipt (306 logs) | Attest 1,618,872 | 1,618,872 | 32% | 58,812 | 13,421,472 | applied |
| 7 | storage slot 16 blocks back: one EIP-2935 hop, then the slot | Attest 498,204 | 498,204 | 10% | 13,578 | 4,094,256 | applied |
| 8 | four facts in one call (account, two slots, a log) and a relation | Attest 453,182 | 453,182 | 9% | 12,309 | 3,723,928 | applied |

Each answer is checked three ways. The event's identifiers equal those of the native evaluation
of the same request on the record the chain holds. The values equal JSON-RPC: the account's four
fields and the slot's value are those of `eth_getProof`, the WETH balance is also what `eth_call
balanceOf` returns at `n` and at `m`, the log is the one `eth_getBlockReceipts` reports, and the
receipts trie rebuilt from that response has the record's receipts root. And each identifier is
derived again with SHA-256 from the JSON fields alone (`harness/src/attest.rs`).

Every attestation is one transaction of one handler execution, the largest at 32% of the 5M
target. Row 8 has the shape the bridge's `ImportRoot` needs (`../docs/design/bridge.md` §5.4: two
slots of one contract under one record), with a log added. The cost follows the bytes hashed. A storage slot is 9 + 7 trie nodes (7,338 bytes of
transaction); a log needs the whole receipt as the trie leaf: 96,584 cycles for a receipt with 7
logs (2,623 bytes), 1,618,872 for the block's largest, with 306 logs (58,812 bytes). One hop adds
the history contract's account and slot proofs and a header: 498,204 for the hop and a slot,
against 274,842 for the slot alone.

### The public consumer path (`results/stage4-attest-consumer.md`)

Rows 1 and 2 are a public actor of the consumer calling `Attest` and keeping the reply
(`CallAttest`): the test reads the whole `Attestation` back from its state and compares it with
the native evaluation. Rows 3 and 4 are the reference consumer.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | a public actor calls Attest: storage slot | consumer CallAttest 87,006 → Attest 278,002 → consumer (reply) 19,768 | 384,776 | 8% | 7,507 | 3,138,264 | applied |
| 2 | a public actor calls Attest: event log | consumer CallAttest 43,100 → Attest 99,498 → consumer (reply) 21,106 | 163,704 | 3% | 2,792 | 1,331,968 | applied |
| 3 | reference consumer: claim an ERC-20 Transfer (credits its sender once) | consumer Claim 37,235 → Attest 99,498 → consumer (reply) 31,544 → consumer Credit 15,261 | 183,538 | 4% | 2,768 | 1,490,448 | applied |
| 4 | reference consumer: the same claim again (replay) | consumer Claim 14,835 | 14,835 | 0% | 2,768 | 140,824 | refused: exit 223 (consumer: AlreadyClaimed) |

**The reference consumer** (`guests/consumer`, module `consumer` of the wire crate) credits an
ERC-20 `Transfer` to a given Ethereum contract once (the first of RFP-021's two U.6 cases: token,
sender, recipient and amount, read from the event). Here the token is USDC and the transfer is
the first of block `n` whose recipient has code (`eth_getCode`). The consumer follows the
Recommended Consumer Pattern as far as a plain `Transfer` allows:

- **A pool is what it credits against**: the attestation program's account, the instance id, the
  chain id, the token and the recipient. Nothing stores a pool. Every actor of a pool is a PDA of
  the pool's hash, so there is no pointer to the attestation program that anyone could change,
  and a claim made under another program, instance, chain or token lands in another pool.
- **The nullifier is the claim actor.** `claim(pool, N)` with `N = H(pool id ‖ statement
  identifier)` holds the deposit once it is credited. `Claim` at that actor calls `Attest` at the
  pool's `header(id, slot)` with the pool's rule as conditions (emitter = token, topic 0 =
  `Transfer`, topic 2 = recipient). The reply is accepted only from that header actor; it must
  carry the pool's instance and chain id and exactly one such `Transfer`, and its identifier must
  hash to this actor's `N` (229 otherwise). Then `balance(pool, sender)` is credited, by a
  message it accepts from that claim actor alone.
- **The credit goes to the transfer's sender**, the one party the fact itself names, so it does
  not matter who submits the claim.
- **A refusal consumes nothing.** On LEZ v0.4 a handler that exits with a code reverts the whole
  transaction, so the consumer never sees a failed attestation: there is no path on which it
  could act without one, the nullifier stays unspent and the claim can be sent again. The code is
  the transaction's outcome.

The claim is one transaction of four handler executions and 183,538 cycles: the attestation
97,239, the consumer 82,876 (derived: 37,235 to take the claim and forward the proof, 31,544 for
the reply, 15,261 for the credit). The same log attested as a root costs 96,584; the reply to a
caller adds 2,641. A second claim of the same deposit is refused by the claim actor itself after
14,835 cycles (223), before anything is verified.

### Refusals (`results/stage4-attest-refusals.md`)

All against one chain, which no refusal changes (the test compares the instance's store, the
claim actor and the balance afterwards). Rows 1 to 32 are `Attest` sent as a root; those that turn
on a proof, a predicate or a hop are first refused by the native evaluation with the same code.
Rows 33 to 41 go through the reference consumer.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | a request for another chain id | Attest 36,915 | 36,915 | 1% | 7,304 | 353,752 | refused: exit 140 (ChainMismatch) |
| 2 | a request bound to another instance | Attest 36,689 | 36,689 | 1% | 7,304 | 351,944 | refused: exit 211 (WrongInstance) |
| 3 | a request bound to instance A, sent to instance B's record | Attest 36,689 | 36,689 | 1% | 7,304 | 351,944 | refused: exit 211 (WrongInstance) |
| 4 | a request naming instance B but expecting mainnet's chain id | Attest 36,915 | 36,915 | 1% | 7,304 | 353,752 | refused: exit 140 (ChainMismatch) |
| 5 | an actor that holds no record (no header at this slot) | Attest 29,638 | 29,638 | 1% | 7,369 | 296,056 | refused: exit 213 (NoRecord) |
| 6 | header(id, slot) of another deployment of the same image | Attest 29,638 | 29,638 | 1% | 7,369 | 296,056 | refused: exit 213 (NoRecord) |
| 7 | an actor of the program that is not a record (the instance) | Attest 31,431 | 31,431 | 1% | 7,304 | 309,880 | refused: exit 209 (BadState) |
| 8 | a storage proof valid for another header's block | Attest 50,339 | 50,339 | 1% | 7,304 | 461,144 | refused: exit 110 (ProofNodeMismatch) |
| 9 | a receipt proof valid for another header's block | Attest 29,810 | 29,810 | 1% | 2,519 | 258,632 | refused: exit 110 (ProofNodeMismatch) |
| 10 | a forged node in the account proof | Attest 50,339 | 50,339 | 1% | 7,304 | 461,144 | refused: exit 110 (ProofNodeMismatch) |
| 11 | a forged node in the storage proof (its leaf) | Attest 246,361 | 246,361 | 5% | 7,304 | 2,029,320 | refused: exit 110 (ProofNodeMismatch) |
| 12 | a forged receipt (one bit of the trie leaf) | Attest 67,401 | 67,401 | 1% | 2,519 | 559,360 | refused: exit 110 (ProofNodeMismatch) |
| 13 | a storage proof presented for another slot | Attest 184,830 | 184,830 | 4% | 7,304 | 1,537,072 | refused: exit 110 (ProofNodeMismatch) |
| 14 | an account proof presented for another address | Attest 65,058 | 65,058 | 1% | 7,304 | 578,896 | refused: exit 110 (ProofNodeMismatch) |
| 15 | a storage proof without its last node | Attest 243,977 | 243,977 | 5% | 7,257 | 2,009,872 | refused: exit 111 (ProofIncomplete) |
| 16 | a storage proof with a node appended | Attest 250,453 | 250,453 | 5% | 7,840 | 2,066,344 | refused: exit 112 (ProofTrailingNodes) |
| 17 | a predicate that does not hold (a positive balance < 1) | Attest 252,848 | 252,848 | 5% | 7,338 | 2,081,488 | refused: exit 130 (PredicateFailed) |
| 18 | a log that is not a Transfer to the expected recipient | Attest 86,647 | 86,647 | 2% | 2,623 | 714,160 | refused: exit 130 (PredicateFailed) |
| 19 | a condition on a field the fact does not have | Attest 252,318 | 252,318 | 5% | 7,339 | 2,077,256 | refused: exit 131 (QueryInvalid) |
| 20 | a relation that does not hold (a balance < the contract's nonce) | Attest 252,899 | 252,899 | 5% | 7,315 | 2,081,712 | refused: exit 130 (PredicateFailed) |
| 21 | a log index past the receipt's last log | Attest 77,197 | 77,197 | 2% | 2,519 | 637,728 | refused: exit 125 (LogIndexOutOfRange) |
| 22 | a transaction index past the block's last transaction | Attest 63,535 | 63,535 | 1% | 1,247 | 518,256 | refused: exit 122 (ReceiptNotIncluded) |
| 23 | a log of a transaction that failed | Attest 54,574 | 54,574 | 1% | 1,612 | 449,488 | refused: exit 124 (ReceiptFailed) |
| 24 | a receipt proof presented for another transaction index | Attest 37,681 | 37,681 | 1% | 2,519 | 321,600 | refused: exit 110 (ProofNodeMismatch) |
| 25 | a hop to a block that is not older than the record's | Attest 64,580 | 64,580 | 1% | 13,592 | 625,376 | refused: exit 150 (HistoryOutOfWindow) |
| 26 | a hop with a header the history contract does not hold | Attest 247,160 | 247,160 | 5% | 13,578 | 2,085,904 | refused: exit 151 (HistoryHashMismatch) |
| 27 | an older block's proof without its hop | Attest 50,339 | 50,339 | 1% | 7,304 | 461,144 | refused: exit 110 (ProofNodeMismatch) |
| 28 | an empty request | Attest 14,235 | 14,235 | 0% | 460 | 117,560 | refused: exit 131 (QueryInvalid) |
| 29 | an accumulator leaf whose root is not a storage fact | Attest 267,921 | 267,921 | 5% | 8,376 | 2,210,376 | refused: exit 214 (BadQuery) |
| 30 | a proof whose first node is replaced by two junk bytes | Attest 43,963 | 43,963 | 1% | 6,773 | 405,888 | refused: exit 110 (ProofNodeMismatch) |
| 31 | a message cut short by one byte | (undecodable) 29,447 | 29,447 | 1% | 7,303 | 294,000 | refused: exit 200 (BadMessage) |
| 32 | a message with a byte appended | (undecodable) 29,848 | 29,848 | 1% | 7,305 | 297,224 | refused: exit 200 (BadMessage) |
| 33 | consumer: a Transfer of the token to another recipient | consumer Claim 37,235 → Attest 87,766 | 125,001 | 3% | 2,768 | 1,022,152 | refused: exit 130 (PredicateFailed) |
| 34 | consumer: a Transfer of another token | consumer Claim 37,235 → Attest 86,407 | 123,642 | 2% | 2,768 | 1,011,280 | refused: exit 130 (PredicateFailed) |
| 35 | consumer: a claim sent to a claim actor the statement does not name | consumer Claim 37,235 → Attest 99,498 → consumer (reply) 22,302 | 159,035 | 3% | 2,768 | 1,294,424 | refused: exit 229 (consumer: NullifierMismatch) |
| 36 | consumer: a pool configured for another chain id | consumer Claim 37,235 → Attest 23,729 | 60,964 | 1% | 2,768 | 509,856 | refused: exit 140 (ChainMismatch) |
| 37 | consumer: a claim with a forged receipt | consumer Claim 37,235 → Attest 70,334 | 107,569 | 2% | 2,768 | 882,696 | refused: exit 110 (ProofNodeMismatch) |
| 38 | consumer: Credit sent as the transaction root | consumer Credit 9,113 | 9,113 | 0% | 564 | 77,416 | refused: exit 225 (consumer: WrongSender) |
| 39 | consumer: Credit from another program's actor | scripted 9,742 → consumer Credit 9,963 | 19,705 | 0% | 784 | 163,912 | refused: exit 225 (consumer: WrongSender) |
| 40 | consumer: an attestation it did not ask for, from another program's actor | scripted 11,383 → consumer (reply) 6,986 | 18,369 | 0% | 984 | 154,824 | refused: exit 220 (consumer: BadMessage) |
| 41 | consumer: the same deposit claimed again (replay) | consumer Claim 14,835 | 14,835 | 0% | 2,768 | 140,824 | refused: exit 223 (consumer: AlreadyClaimed) |

The second instance of rows 3 and 4 has the same checkpoint and committee and is configured with
another chain id (Sepolia's): the chain id is part of the instance id, so it is another instance
with records of its own, and a request bound to the first instance, or expecting mainnet's chain
id, is refused at its record. That is RFP-021's F.5 and F.15 as far as one chain's data shows
them: the test with one contract at the same address on mainnet and on a testnet is not built.
The consumer's own checks of a reply (another instance or chain: 226; anything but one `Transfer`
of the pool's token to its recipient: 227; an amount of 2^128 or more: 228) cannot be reached
through the program, which never produces such a reply; `harness/tests/attest_native.rs` runs
them on attestations built by hand.

### The same attestations by the program built without the accelerated Keccak (`results/stage4-attest-keccak-soft.md`)

The program hashes the Merkle-Patricia nodes with RISC Zero's accelerated Keccak (the patched
`tiny-keccak`, as `../bench/exec`): the executor computes each Keccak-f permutation and the guest
pays about 2,160 cycles for it instead of 17,500. `Attest` runs only at public actors, which the
sequencer executes and nobody proves, and no handler that is proven (`VerifyAll`) hashes with
Keccak. `guests/program-keccak-soft` is the same source without the patch, deployed at an address
of its own and run through the same scenario, in case a LEZ deployment does not offer the
accelerator:

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | account: the WETH contract (nonce, balance, storage root, code hash) | Attest 625,295 | 625,295 | 13% | 4,424 | 5,037,752 | applied |
| 2 | storage slot: a WETH balance, value >= 1 | Attest 1,099,689 | 1,099,689 | 22% | 7,338 | 8,856,216 | applied |
| 3 | storage slot: an absent slot is zero | Attest 1,079,227 | 1,079,227 | 22% | 7,291 | 8,692,144 | applied |
| 4 | account: an address without an account | Attest 577,801 | 577,801 | 12% | 4,150 | 4,655,608 | applied |
| 5 | event log: an ERC-20 Transfer (receipt with 7 logs) | Attest 340,097 | 340,097 | 7% | 2,623 | 2,741,760 | applied |
| 6 | event log: a Transfer in the block's largest receipt (306 logs) | Attest 8,226,393 | 8,226,393 | 165% | 58,812 | 66,281,640 | applied |
| 7 | storage slot 16 blocks back: one EIP-2935 hop, then the slot | Attest 2,104,035 | 2,104,035 | 42% | 13,578 | 16,940,904 | applied |
| 8 | four facts in one call (account, two slots, a log) and a relation | Attest 1,875,666 | 1,875,666 | 38% | 12,309 | 15,103,800 | applied |

Without the accelerator an attestation costs 3.6 to 5.1 times as many cycles: a storage slot
1,099,689 (22% of the target), a log 338,670, the hop and a slot 2,104,035. Every one stays under
LEZ's cap of 10M; the log in the receipt of 306 logs, 8,224,982, is above the 5M target. The
difference divided by the 15,372 cycles the accelerator saves per Keccak-f permutation (17,532 −
2,160, `../bench/RESULTS.md`) gives the permutations each attestation hashes (derived): about 54
for the storage slot, as the bare guest counted for a proof of this shape, 16 for the small
receipt and 430 for the large one.

### The private path: a consumer attests a fact against the record it reads

`AttestPrivately` at a private account's actor of the consumer sends `Read` to
`header(id, slot)`, keeps the query and its proof in its private state until the record comes
back in the `StateReply`, and then runs `attest::evaluate` and the pool's rule on it, inside its
own execution. It keeps the result (the pool id, the statement identifier, the block, the sender
and the amount: 120 bytes of state, of fixed size, the query and its proof being dropped) and
sends nothing else.
`harness/tests/attest_private.rs` runs it for the same USDC `Transfer` through
`lee::execute_and_prove` and LEZ's private settlement (dev mode) and checks:

- **What crosses into public execution** is the one-byte `Read` to the record, the record in the
  reply (333 bytes of state) and the fee transfer; the test asserts that the boundary holds
  exactly these.
- **Nothing of the query is in the transaction.** All 8,201 serialized bytes were searched for
  the token address, the sender, the recipient, the amount, the statement identifier, the pool
  id, the first and last 32 bytes of each proof node, the account id and its keys: all absent
  (`results/stage4-attest-private-public-outputs.txt`).
- **What LEZ charged**: 421,641 execution gas (`PRIVATE_VERIFY_GAS` 409,764 + 11,877 cycles for
  the public `Read`) and 224,063 storage gas, as for the read of §4.
- **What a prover has to prove for the consumer**: its two transitions, 34,337 user cycles for
  the call and 346,240 for the reply that verifies the fact, 380,577 together, one segment each
  (655,360 cycles with padding). The consumer hashes with software Keccak, so this is plain
  RISC-V proving. LEZ's privacy-preserving circuit over the two receipts and the fee transfer is
  proven as well; `lee` does not report its cycles, and they are not in these counts
  (`results/stage4-attest-private.md`).
- **A fact that does not verify cannot be proven.** A forged receipt, a `Transfer` to another
  recipient, a pool configured for another chain id, a proof valid for another header's block and
  a slot without a record each end the consumer's second transition with a code (110, 130, 140,
  110, 222), and LEZ's prover makes no receipt for a non-zero exit, so there is no transaction.
- **Calling instead of reading publishes everything.** The same consumer calling `Attest`
  privately (`CallAttest`) puts the 2,225-byte request and the 301-byte attestation into the
  transaction in the clear; the test finds the token, the sender, the amount and the statement
  identifier in its bytes. LEZ charged it 509,262 execution gas (409,764 + 99,498).

With RISC Zero's accelerated Keccak in the consumer (`guests/consumer-keccak-accel`, executor
only) the two transitions take 137,064 user cycles instead of 380,577, and the Keccak-f
permutations would be proven by RISC Zero's Keccak circuit, as an assumption that LEZ's
privacy-preserving circuit would have to resolve. That was not tried, so the consumer measured
here hashes in software. The cost follows the receipt: the same attestation for the `Transfer`
in the receipt of 306 logs takes 8,903,363 user cycles in ten segments (2,295,842 with the
accelerator), because the 58 KB proof passes through both transitions and is hashed in the
second.

**A real proof of this path, on a GPU, on these images.** `harness/src/bin/prove_attest.rs`
makes it, as `prove_private` does for §3, and `scripts/stage-prove-attest.sh` stages its input
(`../gpu-prover/inputs-lez-attest/MANIFEST.json`: the binary and the fixture files it reads, by
size and SHA-256). The binary built on the build box at commit ad40044 (sha256 `6146055b…11ab`,
glibc 2.39; program `9f3eb00f…`, consumer `f883e6a4…`) ran on one NVIDIA L4 on Modal, with r0vm
3.0.5 built with CUDA as its prover (`../gpu-prover/RESULTS.md`, run `gpu-l4-attest-2`), with
this command:

```sh
FERRATA_EXPECT_CONSUMER_IMAGE_ID=f883e6a4dbd8e49439465f918884abe1379b02276ff88676f0a4bf958ee49068 \
FERRATA_ATTEST_FIXTURE_DIR=/path/to/mainnet-attest-2026-11-10 RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/path/to/r0vm \
/usr/bin/time -v ./prove_attest --tx-out stage4-private-attest-tx.borsh > prove-attest.log 2>&1
```

| the private attestation, proven (`results/prove-attest.log`) | |
|---|---|
| proven | the consumer's two private transitions (`AttestPrivately`, which sends `Read`: 34,337 user cycles; the record's reply, which verifies the `Transfer`: 346,240), the fee transfer from the private account, and LEZ's privacy-preserving circuit over them (`lee::execute_and_prove`, `ProverOpts::succinct()`). The image claimed is the consumer's, `f883e6a4…9068`, disclosed. |
| wall clock | **16.1 s** for `execute_and_prove`; 0:21.28 for the whole run (setup and settlement included), 18.81 s of user time on one core |
| machine | one NVIDIA L4 (23,034 MiB), 4 CPU cores, 32 GiB; GPU memory peak 11,295 MiB; peak host memory 0.76 GiB (798,040 kB, `/usr/bin/time -v`) |
| receipt | **227,170 bytes** (the succinct `InnerReceipt`, borsh); the transaction is **231,214 bytes** (`results/stage4-private-attest-tx.borsh`, sha256 `5b1d6292…bdcf`) |
| settlement | in the run, with the receipt verified: execution gas 421,641 (`PRIVATE_VERIFY_GAS` 409,764 + public part 11,877 cycles), storage gas 224,063, base fee 5,165,632: the figures of the dev-mode run above |

The transaction was then settled again from the file on another machine (the laptop, no
proving), with the same binary: `RISC0_DEV_MODE=0 ./prove_attest --settle FILE` rebuilds the
state from the fixture, checks that the transaction claims this binary's consumer image, and
LEZ's settlement verifies the receipt (`results/prove-attest-settle.log`: "receipt verified",
the same charge). The first proof of this path was of the previous images (consumer
`711584ff…`, 382,336 user cycles): 13.5 s on another L4, the same receipt and transaction sizes
(run `gpu-l4-attest`); with one bit of its receipt flipped (bit 0 of byte 115,607 of the file)
the same settle command refused it: `Invalid privacy preserving execution circuit proof`
(`../gpu-prover/logs/lez-attest-tampered-settle.log`); that check was not repeated for the second
proof. `results/prove-attest-dry-run.log` is the proving command in dev mode. Each is a single
run on a shared cloud GPU, not a benchmark (the 2.6 s between them says nothing about the
images); the path was not proven on a CPU, and the attestation of the 306-log receipt (ten
segments) was not proven at all.

### Decisions where the design note was silent or did not fit LEZ v0.4

1. **The chain id was missing from the program.** `docs/design/attestation.md` (§3.1, §3.4) puts
   it in the configuration and in every instance id; `Config` had none. It is now a field of
   `Config`, so part of the instance id, and `Record` writes it into every record. The program
   cannot check that it belongs to `genesis_validators_root`, because nothing a light client
   verifies commits to the execution chain id; the consumer pins the instance id, and the
   off-chain checkpoint verification (not built) compares the two.
2. **One record type for the program and the attestations.** The program's record lacked the
   parent hash of the core's `attest::HeaderRecord` and could not be turned into one. The record
   now carries it (289 bytes; the actor's state is 333, was 293), it lives in the core's
   `witness.rs`, which a consumer can use without linking BLS, and
   `attest::HeaderRecord::from_recorded` builds the execution view from it; a native test on the
   mainnet end-to-end store shows it equal to the store's own record.
3. **`Attest` replies with statements, not with the record.** The design's table (§3.2, §6.1) had
   `Attest` reply with the actor's state. It now replies with the `Attestation`; `Read` is the
   message that returns the record.
4. **Typed errors reach the transaction, not the consumer.** Rule 3 of the consumer pattern asks
   a consumer to handle each code. On LEZ v0.4 a non-zero exit of any handler reverts the whole
   transaction (`lee/state_machine/src/program/mod.rs`, `check_exit_code`), so a consumer's code
   never runs after a failed attestation. The rule holds through the transaction's outcome, which
   carries the code; rule 4 (never act without an attestation) holds by construction.
5. **One request can hold several facts**, with relations between them and hops before them, as
   the core's `attest_batch` and `history_hop` allow; a single fact is a request with one entry.
6. **Keccak.** The program uses RISC Zero's accelerated Keccak, the consumer software Keccak. A
   private attestation proven with the accelerator would carry a Keccak-circuit assumption into
   LEZ's privacy-preserving circuit; whether LEZ's prover resolves it was not tried.
7. **The reference consumer's nullifier is `H(pool id ‖ statement identifier)`**, the RFP's S.4,
   without the secret of the design's rules 5 to 7. A plain ERC-20 `Transfer` commits to no
   secret, so those rules cannot be met by this fact: such a nullifier is computable from public
   Ethereum data, and a private claim that published it would name its deposit; and nothing in
   the log says who on LEZ may claim it. The consumer therefore has a public claim, which
   credits the Ethereum sender, and a private attestation, which publishes nothing, but no
   private claim. The secret-keyed nullifier needs a deposit event that carries a commitment,
   which is the bridge's vault.
8. **The identifier returned for a state fact is the default one** (§5.2); the block-bound
   variant can be derived from the reply's fields and is not returned.
9. **The old `Attest { query }`**, which ignored its query and replied with the record, is now
   `ReadPadded`; the shell-cost measurement of §2 and the Stage 3 call test use it.

### What is not built, or weaker than the design says

- **Rules 5 to 7 of the consumer pattern and a private claim** (decision 7); the second U.6
  reference consumer (a native ETH transfer to a contract); the F.8 test of §6.5 of the design
  note (two deposits whose transactions must be indistinguishable).
- **Accumulator leaves on real data.** They run in the guest since Stage 5 (§6), on a synthetic
  state holding the vault's own deposits under a record placed in LEZ's state, not written by
  the program; no deployed contract kept such a root when this ran (the vault is not deployed).
  `harness/tests/attest_native.rs` also runs the shared vectors (`../vectors/acc-leaf.json`: 14
  leaves, 7 refusals) through `evaluate` on the host.
- **Gloas records** are attested since Stage 5 (§6), on Sepolia; this section's mainnet block is
  of the Fulu era.
- **One hop, not a chain of hops**, was measured (the request allows several).
- **The private path has real proofs on a GPU only** (§5.5: 16.1 s on an NVIDIA L4). It was not
  proven on a CPU, so what a user's own machine needs for it is not measured, and the 306-log
  case was not proven. A read exposes which record was read (§4), and this fixture reads the
  record of the fact's own block, not the newest one.
- **Predicates compare 256-bit words** with a constant or with another fact of the same request;
  there are no arithmetic or boolean combinations beyond "all hold".
- **The transaction carries the proofs.** A log in a large receipt needs the whole receipt: 58.8
  KB and 1.6M cycles for 306 logs here. LEZ counts the bytes as storage gas (58,812 of the
  block's 1,000,000), and a receipt several times larger would not fit the target.


## 6. Stage 5: Sepolia through the Gloas fork, and attestation under a Gloas record

**Question.** Can an instance whose schedule holds a later fork cross it by itself, can a record
of the Gloas era, which binds only an execution block hash, be attested against, and do both
hold on a live testnet? And can the accumulator leaves of the vault's deposit tree be proven by
the program in the guest?

**Answer: yes, on real Sepolia data; the vault's leaves on a synthetic state until the vault is
deployed.** Everything here ran in the in-process harness on a Modal container (results at commit
ad40044, `results/environment.md`), on the images of the introduction; the executor is
deterministic. Public transactions only, except one dev-mode run of the proven path.

**The data.** `../crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10` (the core's Sepolia
fixture: the bootstrap of period 1378, the last before Gloas, and every period's update from 1378
to 1409; Sepolia activated Gloas at epoch 353,024, the first slot of period 1379) and
`…/sepolia-attest-2026-11-10`, fetched for this stage by `scripts/fetch-attest-fixture.py
--network sepolia` on 10 November 2026 at 17:10:50–17:10:53 UTC from public endpoints, without an
API key and with a generic User-Agent (`SOURCE.md` lists every request):

| | |
|---|---|
| finality update (Gloas layout) | attested slot 11,549,728, signature slot 11,549,729, 507 of 512 signers, period 1409 |
| finalized header | slot 11,549,664; it binds execution block 12,105,936 (the bid's parent block), hash `0x5a2f5671…27b6` |
| execution head when the proofs were fetched | 12,106,027, 91 blocks later |
| storage slot | Sepolia USDC (`0x1c7d…7238`, Circle's FiatToken proxy) slot 11, `totalSupply_`: the capture checked it equals `totalSupply()` at the block |
| log | the block's first ERC-20 `Transfer` (transaction 0, log 0; token `0x5543…925b`) |
| one contract at one address on both chains | Multicall3 (`0xcA11…CA11`): the capture read the same code hash at block 12,105,936 and at mainnet block 26,385,920 |

**The upgrade in place** (`ferrata_core::steps::store_for_update`, `steps::apply`; program
`Open` and `VerifyAll`). An update is decoded in the layout its encoding names. If that is a later
layout than the store's, it must be the layout of the fork that the instance's schedule (fixed at
`Init`) puts at the update's attested slot; the job is then verified against the store upgraded to
it, and applying the job writes the upgrade (`upgrade_lc_store_to_*` on the compact store changes
the layout only: the store holds no execution header) and emits `FRT-UPG1` with the two layouts.
Any other layout is `ForkMismatch` (61). Nothing else tells a store to upgrade, and a fork that is
not in the schedule still needs a successor (design §3.5). The core's `steps` test runs a second
compact store that is never told to upgrade beside the told one on all 52 official sync cases:
same verdicts and codes, 12 upgrades made by the store itself, and after each step it equals the
told store upgraded to that layout.

### Sepolia from period 1378 to 1409 (`results/stage5-sepolia-replay.md`)

`Init` from the period-1378 bootstrap (Electra layout, Sepolia's schedule), each period's update
as a public job in the layout the beacon node served it in, then the finality update of the
attestation fixture: 166 transactions (3 + 32 × 5 + 3), every one applied and under the 5M target,
together 525,494,412 execution gas and 1,776,529 storage gas (`tests/sepolia.rs` ends instead with the
replay fixture's own finality update: 526,306,154 and 1,788,049). After every job the instance's store
equals the compact form of the native store (which the harness upgrades by hand at the fork, as a
light client does), and every finalized root equals the root a second beacon provider reports. The
store changed layout once, in the job of period 1379, and the configuration is unchanged.

**What the upgrade costs**: the job that upgrades (period 1379) against the next one (1380), same
layout and committee size:

| job | `Open` (+ `JobOpen`) | `Miller` | adoption, two parts | `Finish` → `Apply` → `Record` | job's execution gas |
|---|---:|---:|---:|---:|---:|
| period 1378 (Electra) | 2,154,717 | 3,313,287 | 6,829,002 | 3,731,712 (Apply 61,380) | 16,028,718 |
| period 1379 (the upgrade) | 2,013,730 | 3,310,385 | 6,823,232 | 3,731,686 (Apply 64,230) | 15,879,033 |
| period 1380 (Gloas) | 2,013,742 | 3,310,385 | 6,823,232 | 3,728,633 (Apply 61,177) | 15,875,992 |

The upgrade costs 3,053 cycles in `Apply` (finding the upgraded store the job started from, and
the event) and nothing measurable in `Open`; 3,041 for the whole job (0.02%). A Gloas `Open` is
about 141,000 cycles cheaper than an Electra one: the Gloas header carries a block hash and its
branch instead of the execution payload header.

The proven path follows from the same code: in dev mode, `VerifyAll` of the period-1379 update
(Gloas layout, with the adoption of its committee) sent as one private call to a store still in
the Electra layout upgrades the public instance's store through `ApplyVerified` (public part 76,640
cycles; `tests/sepolia.rs`).

### Attestations under the Gloas record (`results/stage5-attest.md`)

A record of a finalized header in the Gloas layout holds the block hash its header binds, not the
block's roots. `Request::header` carries that block's RLP header (678 bytes here): `evaluate`
checks it hashes to the recorded hash and reads the number, the state root and the receipts root
from it, never from the caller. Each attestation below is the root of a transaction to
`header(id, 11,549,664)`, and its event names exactly the identifiers the native evaluation gives:

| attestation (Sepolia block 12,105,936) | execution gas (cycles) | storage gas (bytes) |
|---|---:|---:|
| storage slot: USDC `totalSupply_`, value checked | 278,253 | 7,633 |
| log: an ERC-20 `Transfer`, emitter and recipient checked | 96,713 | 2,586 |
| account: Multicall3, code hash checked | 169,611 | 4,723 |
| the three in one request | 469,040 | 12,658 |

What the header costs: the vault's request with one leaf (below) under a Gloas record with the
header is 186,719 cycles and 3,391 bytes; the same request under a record of the same block that
holds the payload's fields, without a header, is 166,056 and 2,700: 20,663 cycles and 691 bytes
per request.

### Refusals under the Gloas record (`results/stage5-attest-refusals.md`)

| request | exit | cycles |
|---|---|---:|
| no block header | 215 `HeaderRequired` | 37,744 |
| the header with one byte changed | 120 `HeaderHashMismatch` | 47,539 |
| the header of another block (12,103,422, the replay fixture's bound block) | 120 | 54,131 |
| the header truncated by one byte | 120 | 52,751 |
| the header with one byte appended | 120 | 52,953 |
| 021 F.5: the Multicall3 proof asked as a mainnet statement (chain id 1) | 140 `ChainMismatch` | 26,253 |
| 021 F.5: the USDC storage proof asked as a mainnet statement | 140 | 34,746 |
| a header sent to a record that holds the payload's fields (the bootstrap's) | 214 `BadQuery` | 22,585 |

On F.5: Multicall3 has one address and one code hash on both chains, so "this account has this
code" is true of both; asked under the Sepolia record with mainnet's chain id it is refused, and
its Sepolia statement identifier differs from the mainnet one (`tests/sepolia.rs`). 141
(`BindingMismatch`) is left for records with no execution block at all: the Altair layout, or a
zero block hash (`tests/attest_native.rs`, natively).

### Accumulator leaves in the guest: the vault's request (`results/stage5-vault-synthetic.md`)

Ferrata's vault on Sepolia (`0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F`, not yet deployed when
this ran) keeps its deposit tree's root in slot 0 and its leaf count in slot 1. `harness::vault`
builds the request a bridge needs before it mints: the vault's account, slot 0 (equal to the
root), slot 1 (above the highest leaf asked), and deposit leaves against the root in slot 0. Until
the vault exists it runs on a synthetic state that holds the vault's own eight deposits
(`../vectors/vault-tree-v2.json`, made on the Solidity vault by its tests; the harness recomputes
every leaf and the root from the deposits): a block whose header is a real Sepolia header with that
state root put in, and a record in the Gloas layout binding its hash. **That record is placed in
LEZ's state with `V03State::force_insert_account` (LEZ's test utilities), at the address where the
program writes records: the program did not write it.** From there the path is the program's own.

| request | execution gas (cycles) | storage gas (bytes) |
|---|---:|---:|
| slots 0 and 1, no leaf | 120,879 | 2,319 |
| slots 0 and 1 and one deposit leaf | 186,719 | 3,391 |
| slots 0 and 1 and two leaves | 252,205 | 4,463 |
| slots 0 and 1 and all eight leaves | 644,826 | 10,895 |

A leaf costs about 65,500 cycles (65,486 for the second; 65,493 on average over eight) and 1,072
bytes (its 32 siblings). Refused: a leaf not at its index, a leaf proven at another index, a leaf
against slot 1 (163 `AccumulatorRootMismatch`, 172,634 to 172,637 cycles); an empty position past the count
(130 `PredicateFailed`: slot 1's condition); the facts as mainnet statements (140); without the
header (215).

**The vault on Sepolia, once deployed.** One command captures its fixture, and one test attests
slots 0 and 1 and its newest deposit leaf through the program under the record of the captured
block, after following the chain from the period-1378 bootstrap through the replay fixture and the
captured updates:

```sh
lez/scripts/fetch-attest-fixture.py --network sepolia \
  --storage 0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F:0=depositRoot(),1=nextLeafIndex() \
  --deposits 0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F:DEPLOY_BLOCK --updates-from 1410 \
  crates/ferrata-spectests/fixtures/sepolia-vault
cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test vault -- --nocapture
```

Until that directory exists, `the_vault_on_sepolia` says so and passes; it has not run on real
vault data.

### Departures from the design note, and what Stage 5 does not cover

1. **Where the Gloas roots come from.** Design §4.7 has the record carry the roots read from the
   RLP header. Here the record holds the block hash only and each `Attest` carries the header
   (20,663 cycles and 691 bytes per request), so the update path never touches the execution
   layer. Writing the roots once at `Record` would need the header in the update's last
   transaction.
2. **A new exit code**, 215 `HeaderRequired`; a header sent to a record that holds the payload's
   fields is `BadQuery` (214), so a request has one encoding.
3. **Upgrade only within the schedule.** A fork added after `Init` is still a successor's (§3.5 of
   the design note), which is not built.
4. **Not covered:** a bootstrap of a Gloas-era checkpoint (no public node serves one; the
   instance starts one period before the fork); history hops under a Gloas record; the private
   path under a Gloas record (the consumer carries the header through, unmeasured); the vault on
   real data; Gloas on mainnet (no date yet).

## 7. What did not work, what is missing, what is weaker than asked

- **Settlement is a copy, not the crate.** The harness runs `chain_state::apply`'s
  `settle_charged_transaction` / `settle_private_transaction` and `classify` as copies (with
  attribution, `harness/src/settle.rs`), because `chain_state` and `common` depend on the Bedrock
  node (libp2p, RocksDB, native circuit builds). Every call into `lee` is the sequencer's. Left
  out: the bridge and cross-zone-inbox guards of `validate_user_state_modification` (they reject
  writes to accounts these transactions never touch), and the block tail (the fee program's
  `Distribute` and the clock); each transaction is settled in a block of its own. Fees are at
  the genesis base fees (8 per gas unit); no base-fee movement was simulated. On the running
  sequencer of §9 none of this is a copy, and the block tail and the base fee are real.
- **Deployment.** In the harness the program is deployed at genesis, immutable, with
  `V03State::with_genesis_programs` as LEZ's own tests do, not by `program_loader` transactions;
  the cost of deploying this branch's 593 KB image was not measured. On the running sequencer
  (§9) the earlier image `cec0bc68…` was deployed by `program_loader` transactions: 6 per 508 KB
  image, storage gas 479,496, execution gas 0.
- **Private paths in dev mode.** The Stage 2, 3 and 4 tests run with `RISC0_DEV_MODE=1`. Real
  proofs were made of a finality update on images before this branch, in the harness by CPU
  (`e7cec90e…`) and by GPU (`cec0bc68…`) (§3), and on the running sequencer of §9 against its
  live instance (`cec0bc68…`, by a rented machine's CPU, verified by the sequencer); and of one
  private attestation on this branch's images, by GPU (§5.5). None was made of a handoff, of
  Stage 3's private consumer (§4), or of the update path on this branch's image.
- **Per-handler cycles come from a replay.** `lee` reports one cycle count per transaction; the
  split by handler comes from `harness/src/trace.rs`, which re-executes each guest as `lee`'s
  `PublicBackend` does. Every test asserts that the split sums to LEZ's charge. Within a handler
  (input decoding vs. pairing vs. journal), the split was not measured inside LEZ; §2 compares
  with the bare-guest numbers of `../bench/` and with the shell-cost measurement instead.
- **Not implemented in the program:** a fork that is not in the schedule fixed at `Init` (a
  scheduled one is crossed in place since Stage 5, §6); the weak-subjectivity bound; successors and the root id of the proposal's §1; a submitter daemon
  (the harness plans each job natively, as one would); joining an open job; any clean-up of jobs
  whose final step fails (they stay, unapplied).
- **The execution layer's cost in the program** (§5, §6). The
  handler's 274,842 cycles for a storage slot are about 64,000 more than the bare-guest call on a
  proof of the same shape (210,502, `../bench/RESULTS.md`, another block), most of it the 7.3 KB
  of input the shell reads, decodes and echoes (§2). A record that binds a block hash (Gloas)
  adds the RLP header to every request: 20,663 cycles and 691 bytes (§6). A chain of several hops
  was not measured.
- **Refusals LEZ charges at the full gas limit.** Coded exits are charged only the cycles used,
  as designed. A transaction outside its validity window (an `Open` sent before its signature
  slot) is LEZ's own refusal and was charged the whole declared gas limit, 10M (refusal 17). By
  the same code a delivery to an account that is not admitted is too
  (`lee/state_machine/src/validated_state_diff/mod.rs:495–503`: every error other than
  `ProgramExitedWithCode` is charged `cycle_budget`; `ExecutionError::UnadmittedPublicActor`,
  `lee/state_machine/core/src/execution_state.rs:1209`, is such an error) — read in the source,
  not measured: the tests admit their targets so that the program's own checks are what refuse.
  A submitter bounds both by declaring a tight gas limit.
- **A duplicate `Open` is not cheap.** The instance cannot see whether the job exists, so the
  duplicate is refused by `JobOpen` after step 1 has run: 2,186,502 cycles. Making it cheap needs
  an extra round trip (the job asking the instance for its store), which would cost every update.
- **`scripts/check-bench-provenance.sh`** (not a CI gate) fails, as it does on `main` before the
  merge: it requires the core to equal commit f030475 up to the rename, file for file, and the
  core has since gained modules (`main`'s `accumulator.rs`, `attest.rs`, `exec.rs`, `keccak.rs`,
  `mpt.rs`, `rlp.rs`; this branch's `steps.rs` and `witness.rs`), codes in `error.rs`, module
  lines in `lib.rs`, `tiny-keccak` in its `Cargo.toml`, and `lez` in the root workspace's
  `exclude`. The files whose functions `bench/` measured (`bls/`, `fork.rs`, `hash.rs`,
  `merkle.rs`, `policy.rs`, `ssz.rs`, `store.rs`, `types.rs`, `upgrade.rs`, `validate.rs`) are
  still byte-identical to f030475 up to the rename.

### Where the earlier proposal text (§1–§2) turned out wrong or incomplete on v0.4

1. **No `committee(id)` actor.** The proposal kept both committees' compressed keys (49 KB) in a
   committee actor. On v0.4 a handler sees only its own actor's state, and every byte it loads
   is paid for (§2: 5.7 cycles per byte of input). The program keeps each committee as an
   80-byte summary in `instance(id)`; non-signer keys arrive with each update as uncompressed
   witnesses with Merkle branches to the key root (not "checked against the stored compressed
   bytes": none are stored); the full committee arrives only at its adoption.
2. **A committee adoption is two transactions, and it is needed more often.** The proposal
   costed the handoff as one 5.7M step (4.6M adoption + 1.1M committee root). On LEZ each half
   (256 keys) is 3.40–3.42M, the whole adoption 6.8M with the shell; one transaction would be
   above the 5M target. It is needed whenever the store learns a committee — at `Init` (the
   bootstrap committee), at an update that only supplies the next committee (mainnet update_0),
   and at each rotation — so `Init` is 3 transactions and every adopting update 5.
3. **"Refusing cheaply" holds for the program's own refusals only** — see the two LEZ refusals
   above. The proposal's clock rule (a validity window on each step) makes an early step cost its
   whole gas limit.
4. **"Every step checks its preconditions first (job present, …)"** holds for `Miller`, `Adopt`
   and `Finish` (refused for 5,981–24,159 cycles), not for opening a job (2,186,502).
5. **Private consumers' reads are public reads** (§4): the boundary shows which record was read,
   the record itself, and the consumer's program. An `Undisclosed` image claim hides the image
   from the claims list only; the program account still appears as the sender of the read. The
   proposal's "a private consumer cannot call without exposing its query" is confirmed; its
   implication that reading exposes nothing about the query is too strong: it exposes the slot.
6. **The gas bounds hold, with margin.** No transaction of the job is above 3.73M cycles (75% of
   `TARGET_GAS_EXEC`); a whole finality update also fits one transaction under `MAX_GAS_EXEC`
   (9,121,514 cycles), above the target. Since the caps are per block, at the target a block holds
   one step of a job (plus small transactions).
7. **The "about 15 cycles per byte" of the proposal** (from a comment in LEZ's integration tests)
   measured 5.7 cycles per byte of message for this program (§2).


## 8. Reproduce

Layout: LEZ at `upstream/lez-v0.4.0-rc1` beside the repository (`../../upstream/` from `lez/`; tag
`v0.4.0-rc1`, commit `1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21`, unmodified); the crates depend on
LEZ by path. The environment of the measurements is in
`results/environment.md` (`./results_env.sh` writes it).

Toolchains, matching LEZ: host Rust 1.98.1 (LEZ's `rust-toolchain.toml`; `lez/rust-toolchain.toml`
pins it), RISC Zero zkVM 3.0.5 (LEZ's `Cargo.toml`), guests built with RISC Zero's Rust 1.91.1 (the
toolchain of LEZ's own guest builds, `Justfile`: `r0.1.91.1`) and C++ 2024.1.5, r0vm 3.0.5. The
guest toolchain was installed into a separate `RISC0_HOME` so as not to change the machine's
default (1.88.0, which `../bench/` uses):

```sh
cd lez                                               # from the repository root
export RISC0_HOME=$PWD/../../risc0-home-lez          # rust 1.91.1 installed with `rzup install rust 1.91.1`;
                                                     # cpp 2024.1.5 and r0vm 3.0.5 linked from ~/.risc0
export TMPDIR=$PWD/../../tmp CARGO_TARGET_DIR=$PWD/target
export RISC0_BUILD_LOCKED=1                          # risc0-build then builds the guests with --locked
./results_env.sh                                     # rewrites results/environment.md (run it first, on a clean tree)
RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 1 --nocapture \
  > results/harness-tests-dev-mode.log               # Stages 1–5 and the exit codes, 27 tests: prints every
                                                     # table; writes results/stage3-* and
                                                     # stage4-attest-private-public-outputs.txt
cargo run -p harness --locked --bin tables           # rewrites results/stage1-*.md, shell-costs.md, images.md,
                                                     # stage4-attest*.md, stage5-*.md
cargo build -p harness --locked --bin prove_private --bin prove_attest
RISC0_DEV_MODE=1 target/debug/prove_private --allow-dev-mode --tx-out target/dry-run-tx.borsh \
  > results/prove-private-dry-run.log 2>&1           # dry runs: the commands of the real proofs with a
RISC0_DEV_MODE=1 target/debug/prove_attest --allow-dev-mode --tx-out target/attest-dry-run-tx.borsh \
  > results/prove-attest-dry-run.log 2>&1            # fake receipt (§3, §5)
RISC0_DEV_MODE=0 /usr/bin/time -v target/debug/prove_private --tx-out results/stage2-private-tx.borsh \
  > results/prove-private.log 2>&1                   # a real proof of §3 for this image, then its settlement
                                                     # (about an hour on a laptop CPU; not made for this image)
RISC0_DEV_MODE=0 target/debug/prove_private --settle FILE     # settle a proven transaction again
scripts/stage-prove-attest.sh OUTDIR                 # the input of a real proof of the private attestation (§5);
                                                     # ../gpu-prover/README.md has the commands of the GPU run
RISC0_DEV_MODE=0 target/debug/prove_attest --settle results/stage4-private-attest-tx.borsh \
  > results/prove-attest-settle.log 2>&1             # settle the proven attestation again, receipt verified
cd .. && cargo test -p ferrata-spectests --test steps --test mainnet_e2e --test sepolia_steps -- --nocapture
                                                     # the step API against process_update (with a store that
                                                     # upgrades itself); the record the program writes; Sepolia
lez/scripts/fetch-attest-fixture.py OUTDIR           # how the Stage 4 fixture was fetched: run today, it
                                                     # fetches the block finalized now into a new directory
lez/scripts/fetch-attest-fixture.py --network sepolia OUTDIR   # the same for Stage 5's Sepolia fixture; the
                                                     # vault's capture is in §6
```

The 27 tests take about two minutes on eight test threads (`--test-threads 8`; the longest
test binary, `tests/sepolia.rs`, 102 s); the figures above were produced that way, on a cloud
container, and do not depend on the thread count. The Stage 4 tests read
`../crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10` (or `FERRATA_ATTEST_FIXTURE_DIR`),
Stage 5 `…/sepolia-gloas-2026-11-10` and `…/sepolia-attest-2026-11-10` (or
`FERRATA_SEPOLIA_FIXTURE_DIR`, `FERRATA_SEPOLIA_ATTEST_DIR`); their SHA-256 sums are compiled in.

The harness builds in the dev profile with optimised dependencies: LEZ refuses to build its test
helpers (`lee/test-utils`, needed for private accounts) into a release build. The guests are
always built in release. Cycle counts do not depend on the host build.

Lint and format this workspace's own crates only. Clippy needs `RISC0_SKIP_BUILD=1`, or its
wrapper reaches the guest build and compiles the guests with the host toolchain:
`RISC0_SKIP_BUILD=1 cargo clippy -p ferrata-lez-wire -p harness -p methods --all-targets --locked -- -D warnings`
(clean). `cargo fmt --all` would also rewrite the path dependencies, LEZ's crates among them; use
`cargo fmt -p ferrata-lez-wire -p harness -p methods`, and `rustfmt --edition 2021` for the guests
(clean).

## 9. On a running sequencer (`live/`)

[`live/`](live/README.md) runs the program, at the image before the attestation (`cec0bc68…`,
without §5), on LEZ's own sequencer instead of the harness:
`sequencer_service` from the unmodified tag `v0.4.0-rc1`, set up and started as LEZ's Justfile
does, without `RISC0_DEV_MODE` (it verifies receipts), inscribing its blocks on a Logos
Blockchain (Bedrock) node at `d480efc6`, the revision LEZ's `Cargo.lock` pins, built by LEZ's own
resolver and run with LEZ's node configuration: **one validator, leader proofs in dev mode**
(`POL_PROOF_DEV_MODE=true`, as LEZ's container entrypoint sets it), 1 s slots, 100-slot epochs.
Every transaction was built, signed and sent by LEZ's wallet over the sequencer's RPC and counted
only once its block was `Finalized` on Bedrock; the driver reads each charge by settling the block
with `chain_state::apply`, whose totals the block's own fee transaction must match. One recorded
run, 10 November 2026, on a rented Linux container (a Modal build box, like the harness runs of this image's
figures, `results/environment.md`), re-checked there by
`live/verify-run.sh`: with the bytecode put back every block hash holds, and the whole chain
replays from the pre-genesis state to the recorded charges. Record: `live/results/` (manifest
`run.tsv`; block log `blocks.borsh`, the 13 program bytecode chunks elided and listed with their
SHA-256 in `segments.tsv`); tables: `live/results/tables.md`.

| | on the sequencer | in the harness |
|---|---|---|
| deployment | the exact binary of image `cec0bc68…` (SHA-256 `4b7702a4…1777`), twice, and LEZ's `scripted` program, through `program_loader`: 6 transactions per 508 KB image, storage gas 479,496, execution gas 0, base fees 3,835,968; the loader computed image `cec0bc68…` | at genesis (§7) |
| transactions | 96 sent: 73 applied, 19 refused by the program, 2 reverted otherwise, 2 dropped by the sequencer; 191 blocks, all final on Bedrock | one block each |
| the 65 with a row in §2 (as measured on `cec0bc68…`; §2 now shows `2f3566d4…`) | the same outcome and exit code in all 65; the same execution gas in 64; the other ran outside its validity window and pays its declared limit (4,000,000 here, 10,000,000 declared there) | §2 |
| transaction bytes (storage gas) | equal in 44 of those 65; 65 bytes more in 19 and 65 fewer in 1 (not traced to a field); 607 fewer in the validity-window case, which is built differently | §2 |
| base fee | 8; 9 in the block after the one block over the 5M target (block 92, two 3.6M transactions) | 8 throughout |
| the stored GPU-proven transaction | dropped: "Invalid input: Unknown program Ev5SfFrp…" (proven against the harness state, where the program sits at another account) | settled (§3) |
| the update with a fake receipt | dropped: "Invalid privacy preserving execution circuit proof" | refused (§3) |
| the update with a real proof | made against the live instance on the container's CPU with 32 prover threads, 895.6 s (image `e7cec90e…` took 3,240.5 s on the laptop's 16 threads, this image 49.5 s on an L4 GPU: §3); verified and applied: execution gas 479,854, storage gas 224,063, fee 5,631,336; receipt 237,412 bytes | the same gas and fee on this image (§3 now shows `2f3566d4…`: 71,979 public instead of 70,090) |

What the running system showed that the harness could not (details: `live/README.md`): the
sequencer's head is not its chain (blocks made in the last second before a Bedrock epoch led to a
rewind from block 37 to 16 in a first attempt, and to a rewind and 31 inscriptions given up when
repeated on purpose; never with blocks made 3 s or 5 s before, nor in the recorded run), so a
client must settle only on blocks final on Bedrock; finality took 5–6 s per block and a public
transaction was settled 8.5–19.7 s after it was sent (one validator in dev mode, not a network); a
proof is bound to the deployment it was made for; LEZ's wallet default gas limit (2,000,000) is
below what a job's transactions need (one reverted and paid 2,000,000); the base fee moves; and
Bedrock's storage gas price for the zone's inscriptions rose from 1 to 4 within twelve minutes.

Reproduce (Linux x86-64; ports and packages in `live/README.md`; the guest binaries as it says):
`lez/live/run.sh build`, `lez/live/run.sh build-bedrock`, `lez/live/run.sh run`,
`python3 lez/live/tables.py`, `lez/live/verify-run.sh`.
