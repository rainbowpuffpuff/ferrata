# Ferrata on LEZ v0.4.0-rc1: the light client as a program, measured through LEZ's state machine

This directory holds a LEZ v0.4.0-rc1 program that runs Ferrata's Ethereum sync-committee light
client, and a harness that drives it through LEZ's own state transition functions and fee
settlement with real mainnet data (`../crates/ferrata-spectests/fixtures/mainnet-p1899`: the
period-1899 bootstrap and the best updates of periods 1899–1903, Fulu, 512-member committees).
Every number below is LEZ's own accounting of a transaction that ran, unless it says otherwise.

**What ran end to end.** Through LEZ's own settlement path (fee reserve, metered action, refund,
over `lee`'s state transition functions): `Init` from the mainnet checkpoint, then all five mainnet
updates — each hands over or supplies a committee, four rotate it — as jobs of signed, fee-paying
public transactions. After every job the instance's store equals the compact form of the native
`FinalizedStore` (`Adoption::OnChain`), every finalized root equals the canonical root a second
provider reports, and `header(id, slot)` holds the native finalized header's record. A finality
update runs as three transactions; 18 refusals ran through the same path (§2). Stage 2 (one
private actor verifies a whole update and the public instance applies it) and Stage 3 (a private
consumer reads a record, or calls) ran through LEZ's privacy-preserving circuit and private
settlement in RISC Zero dev mode; one real proof was made on this machine's CPU (§3).

**Which image these numbers come from.** Every number in this file, except those of the real
proof, was measured on the images built after this branch merged `main` (commit 4d5fe7b, which
brought the core's execution layer) and after the program's exit codes were renumbered (§1):
program `cec0bc68…`, consumer `aec03bcd…` (`results/images.md`), in runs at commit 62f4f70
(`results/environment.md`; the dry run of §3 at 1b3984e, whose host code adds the proof's
handoff). The numbers measured on the earlier image (`e7cec90e…`, consumer
`620e6390…`; commit 7dfae96, the branch before the merge) are superseded. Measured again, every
cycle count, gas charge, fee and transaction size in §2 to §4 is the same as on the earlier
image; what changed are the image ids, the account ids derived from them in the dumps of §3 and
§4, and the exit codes of the refusals in §2. **The real proof (§3) and its settlement are still
the earlier image's** (9,045,235 cycles, 54 min, 9.18 GiB, a 227,080-byte receipt); the proof of
the new image is pending, and §3 says how to make it.

| operation (mainnet, 512-member committees) | transactions | largest transaction, execution gas (cycles) | total execution gas | total storage gas (bytes) |
|---|---:|---:|---:|---:|
| `Init`: bootstrap checks, then the adoption of its committee in two parts | 3 | 3,479,974 | 7,085,450 | 51,968 |
| finality update (update_1 without its next committee) | 3 | 3,719,489 | 9,216,121 | 4,451 |
| update that rotates and adopts a committee (mainnet updates 1–4; update_1 shown) | 5 | 3,730,145 | 16,076,120 | 54,491 |
| update that only supplies the next committee (mainnet update_0) | 5 | 3,694,714 | 15,984,062 | 53,675 |
| finality update as one public transaction (`VerifyAll`) | 1 | 9,115,325 | 9,115,325 | 4,089 |
| finality update verified privately (Stage 2; one private transaction) | 1 | 479,854 | 479,854 | 224,063 |

Every transaction of the jobs is under `TARGET_GAS_EXEC` (5,000,000), the largest at 75% (3,730,145) of
it; none is near `MAX_GAS_EXEC` (10,000,000). Execution gas is the cycles LEZ metered (`lee`
reports `SessionInfo::cycles`, user cycles); storage gas is the transaction's serialized size.


## 1. What was built

**The program** (`guests/program`, one RISC Zero guest; image id `cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78`, 508,284 bytes). Its actors are public
PDAs of the program (`wire/src/lib.rs`):

| actor | seed | state | written by |
|---|---|---|---|
| `instance(id)` | `H(…/instance ‖ id)` | the config, an active flag and the compact store: fork, finalized beacon header, current and next committee as *summaries* (key root + aggregate key, 80 bytes each); 425 bytes, 505 with a next committee | `Init`; `Apply` / `ApplyVerified` |
| `job(id, job_id)` | `H(…/job ‖ id ‖ job_id)` | one job: store and update digests, the signature check's phase (H(m), −key, signature: 480 bytes; then the 576-byte Miller product), the adoption's progress, what to apply; 1,249–1,409 bytes | `JobOpen`; `Miller`, `Adopt`; cleared by `Finish` |
| `header(id, slot)` | `H(…/header ‖ id ‖ slot)` | the record of a finalized header (beacon header, execution root, block number, block hash, state and receipts roots, timestamp), written once; 293 bytes | `Record` |

`id = H(config ‖ checkpoint)`; `job_id = H(id ‖ store digest ‖ update digest)`.

| message | sent to | by | does |
|---|---|---|---|
| `Init` | `instance(id)` | anyone | verifies the bootstrap against the checkpoint (committee as summary), creates the instance (inactive) and opens the adoption job |
| `Open` | `instance(id)` | anyone | step 1 (`ferrata_core::steps::step1`), opens `job(id, job_id)`; valid only from the signature slot's start (validity window) |
| `Miller` | job | anyone | step 2: the Miller loops |
| `Adopt{part, keys}` | job | anyone | one part (256 keys, uncompressed) of a committee adoption |
| `Finish` | job | anyone | step 3: final exponentiation; clears the job, sends `Apply` |
| `Read`, `Attest` | any actor | another actor | replies `StateReply` with the actor's state (LEZ v0.4's read idiom) |
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
| 200–219 | the program's own refusals | `wire/src/lib.rs`, `code` | 200–212 |
| 220–239 | the Stage 3 consumer (§4) | `wire/src/lib.rs`, `consumer::code` | 220–222 |
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
| 1 | Init Init | Init 188,943 → JobOpen 18,462 | 207,405 | 4% | 1,880 | 1,674,280 | applied |
| 2 | Init adopt 1/2 | Adopt 3,398,071 | 3,398,071 | 68% | 24,980 | 27,384,408 | applied |
| 3 | Init adopt 2/2 | Adopt 3,408,181 → Apply 55,931 → Record 15,862 | 3,479,974 | 70% | 25,108 | 28,040,656 | applied |
| 4 | update_0 Open (step 1) | Open 2,125,354 → JobOpen 25,857 | 2,151,211 | 43% | 2,853 | 17,232,512 | applied |
| 5 | update_0 Miller (step 2) | Miller 3,311,227 | 3,311,227 | 66% | 399 | 26,493,008 | applied |
| 6 | update_0 adopt 1/2 | Adopt 3,409,859 | 3,409,859 | 68% | 24,980 | 27,478,712 | applied |
| 7 | update_0 adopt 2/2 | Adopt 3,417,051 | 3,417,051 | 68% | 24,980 | 27,536,248 | applied |
| 8 | update_0 Finish (step 3) | Finish 3,652,728 → Apply 41,986 | 3,694,714 | 74% | 463 | 29,561,416 | applied |
| 9 | update_1 Open (step 1) | Open 2,181,981 → JobOpen 25,857 | 2,207,838 | 44% | 3,605 | 17,691,544 | applied |
| 10 | update_1 Miller (step 2) | Miller 3,311,227 | 3,311,227 | 66% | 399 | 26,493,008 | applied |
| 11 | update_1 adopt 1/2 | Adopt 3,409,859 | 3,409,859 | 68% | 24,980 | 27,478,712 | applied |
| 12 | update_1 adopt 2/2 | Adopt 3,417,051 | 3,417,051 | 68% | 24,980 | 27,536,248 | applied |
| 13 | update_1 Finish (step 3) | Finish 3,652,728 → Apply 61,555 → Record 15,862 | 3,730,145 | 75% | 527 | 29,845,376 | applied |
| 14 | update_2 Open (step 1) | Open 2,182,721 → JobOpen 25,857 | 2,208,578 | 44% | 3,615 | 17,697,544 | applied |
| 15 | update_2 Miller (step 2) | Miller 3,311,227 | 3,311,227 | 66% | 399 | 26,493,008 | applied |
| 16 | update_2 adopt 1/2 | Adopt 3,409,859 | 3,409,859 | 68% | 24,980 | 27,478,712 | applied |
| 17 | update_2 adopt 2/2 | Adopt 3,417,051 | 3,417,051 | 68% | 24,980 | 27,536,248 | applied |
| 18 | update_2 Finish (step 3) | Finish 3,652,728 → Apply 61,555 → Record 15,862 | 3,730,145 | 75% | 527 | 29,845,376 | applied |
| 19 | update_3 Open (step 1) | Open 2,127,081 → JobOpen 25,857 | 2,152,938 | 43% | 2,852 | 17,246,320 | applied |
| 20 | update_3 Miller (step 2) | Miller 3,311,227 | 3,311,227 | 66% | 399 | 26,493,008 | applied |
| 21 | update_3 adopt 1/2 | Adopt 3,409,859 | 3,409,859 | 68% | 24,980 | 27,478,712 | applied |
| 22 | update_3 adopt 2/2 | Adopt 3,417,051 | 3,417,051 | 68% | 24,980 | 27,536,248 | applied |
| 23 | update_3 Finish (step 3) | Finish 3,652,728 → Apply 61,555 → Record 15,862 | 3,730,145 | 75% | 527 | 29,845,376 | applied |
| 24 | update_4 Open (step 1) | Open 2,154,986 → JobOpen 25,857 | 2,180,843 | 44% | 3,221 | 17,472,512 | applied |
| 25 | update_4 Miller (step 2) | Miller 3,311,227 | 3,311,227 | 66% | 399 | 26,493,008 | applied |
| 26 | update_4 adopt 1/2 | Adopt 3,409,859 | 3,409,859 | 68% | 24,980 | 27,478,712 | applied |
| 27 | update_4 adopt 2/2 | Adopt 3,417,051 | 3,417,051 | 68% | 24,980 | 27,536,248 | applied |
| 28 | update_4 Finish (step 3) | Finish 3,652,728 → Apply 61,555 → Record 15,862 | 3,730,145 | 75% | 527 | 29,845,376 | applied |

### A finality update, and an update that only supplies the next committee (`results/stage1-finality.md`)

update_1 without its next committee (a finality update, after update_0), then update_1 as served,
which then only supplies the period-1901 committee (it does not advance the header):

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | update_1 as finality update Open (step 1) | Open 2,166,945 → JobOpen 22,694 | 2,189,639 | 44% | 3,525 | 17,545,312 | applied |
| 2 | update_1 as finality update Miller (step 2) | Miller 3,306,993 | 3,306,993 | 66% | 399 | 26,459,136 | applied |
| 3 | update_1 as finality update Finish (step 3) | Finish 3,646,542 → Apply 57,085 → Record 15,862 | 3,719,489 | 74% | 527 | 29,760,128 | applied |
| 4 | update_1 (next committee only) Open (step 1) | Open 2,180,660 → JobOpen 25,857 | 2,206,517 | 44% | 3,605 | 17,680,976 | applied |
| 5 | update_1 (next committee only) Miller (step 2) | Miller 3,311,227 | 3,311,227 | 66% | 399 | 26,493,008 | applied |
| 6 | update_1 (next committee only) adopt 1/2 | Adopt 3,409,859 | 3,409,859 | 68% | 24,980 | 27,478,712 | applied |
| 7 | update_1 (next committee only) adopt 2/2 | Adopt 3,417,051 | 3,417,051 | 68% | 24,980 | 27,536,248 | applied |
| 8 | update_1 (next committee only) Finish (step 3) | Finish 3,652,728 → Apply 41,986 | 3,694,714 | 74% | 463 | 29,561,416 | applied |

### A finality update in one public transaction (`results/stage1-one-transaction.md`)

`VerifyAll` at a public actor of the program does steps 1–3 in one handler and hands the job to
the instance (`ApplyVerified`, accepted from an actor of the program):

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | finality update in one transaction (VerifyAll) | VerifyAll 9,045,235 → ApplyVerified 54,228 → Record 15,862 | 9,115,325 | 182% | 4,089 | 72,955,312 | applied |

### Refusals (`results/stage1-refusals.md`)

Each against an instance that has applied update_0. A refusal leaves every state as it was (the
tests check the store); the fee is paid and the payer's nonce advances.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | forged signature: Open (passes) | Open 2,167,071 → JobOpen 22,694 | 2,189,765 | 44% | 3,525 | 17,546,320 | applied |
| 2 | forged signature: Miller (passes) | Miller 3,306,993 | 3,306,993 | 66% | 399 | 26,459,136 | applied |
| 3 | forged signature: Finish | Finish 3,635,478 | 3,635,478 | 73% | 463 | 29,087,528 | refused: exit 40 (SignatureInvalid) |
| 4 | malformed signature: Open | Open 353,214 | 353,214 | 7% | 3,525 | 2,853,912 | refused: exit 41 (SignatureMalformed) |
| 5 | duplicate Open (same job) | Open 2,166,945 → JobOpen 16,166 | 2,183,111 | 44% | 3,525 | 17,493,088 | refused: exit 207 (JobExists) |
| 6 | Finish before Miller (skipped step) | Finish 23,420 | 23,420 | 0% | 463 | 191,064 | refused: exit 81 (StepError::Phase) |
| 7 | Miller replayed | Miller 20,491 | 20,491 | 0% | 399 | 167,120 | refused: exit 81 (StepError::Phase) |
| 8 | Finish replayed after the job applied | Finish 5,731 | 5,731 | 0% | 527 | 50,064 | refused: exit 205 (NoJob) |
| 9 | Open an applied update again | Open 207,687 | 207,687 | 4% | 3,525 | 1,689,696 | refused: exit 52 (NotNewer) |
| 10 | Apply sent as the transaction root | Apply 17,325 | 17,325 | 0% | 905 | 145,840 | refused: exit 202 (WrongSender) |
| 11 | Apply from another program's actor | scripted 13,172 → Apply 18,178 | 31,350 | 1% | 1,125 | 259,800 | refused: exit 202 (WrongSender) |
| 12 | ApplyVerified from another program's actor | scripted 12,910 → ApplyVerified 15,658 | 28,568 | 1% | 1,093 | 237,288 | refused: exit 202 (WrongSender) |
| 13 | JobOpen from another program's actor | scripted 13,095 → JobOpen 10,676 | 23,771 | 0% | 1,194 | 199,720 | refused: exit 202 (WrongSender) |
| 14 | Record from another program's actor | scripted 9,305 → Record 8,869 | 18,174 | 0% | 731 | 151,240 | refused: exit 202 (WrongSender) |
| 15 | update for another chain's instance: Finish | Finish 3,637,696 | 3,637,696 | 73% | 463 | 29,105,272 | refused: exit 40 (SignatureInvalid) |
| 16 | instance A's job id sent to instance B | Finish 5,731 | 5,731 | 0% | 464 | 49,560 | refused: exit 205 (NoJob) |
| 17 | Open before its signature slot (validity window) | Open 2,166,945 → JobOpen 22,694 | 10,000,000 | 200% | 3,525 | 80,028,200 | reverted: Execution outside of the validity window |
| 18 | Init of an existing instance | Init 17,491 | 17,491 | 0% | 1,816 | 154,456 | refused: exit 203 (AlreadyInitialized) |

### Against the bare guest

`../bench/RESULTS.md` measured the same work in bare RISC Zero 3.0.5 guests (executor, accelerated
blst and SHA-256, core of commit f030475, RISC Zero Rust 1.88). Here, per handler:

| work | bare guest (`../bench/`), user cycles | on LEZ: the handler(s), user cycles as LEZ metered them |
|---|---:|---|
| step 1 | 2,305,240 (execution total: decodes the 26.9 KB SSZ update and a 24.6 KB committee, then everything before the Miller loops) | `Open` 2,125,354–2,182,721, then `JobOpen` 22,694–25,857. The program decodes a 2.9–3.6 KB compact update and checks the non-signers' branches (two per update here) instead of decoding a committee. |
| step 2 | 4,203,683 (two single Miller loops); the two-pair loop alone: 3,251,802 (phase 8m) | `Miller` 3,306,993–3,311,227 (the two-pair loop since commit 133dde9; 4,142,918–4,147,152 with two loops, measured on an image before that commit and not again) |
| step 3 | 3,597,597 (final exponentiation, execution total) | `Finish` 3,646,542–3,652,728 (also checks that the stored product is canonical, and encodes the job for `Apply`), then `Apply` 41,986–61,555 and `Record` 15,862 |
| adoption, 512 keys | 5,685,329 (10b0: 4,562,038 for the keys, plus 3a: 1,123,291 for the committee root) | `Adopt` 3,398,071–3,417,051, twice: 6,806,252–6,826,910 |
| a whole finality update | 10,369,179 (one execution: saved state and SSZ update decoded, policy, new root) | `VerifyAll` 9,045,235, then `ApplyVerified` 54,228 and `Record` 15,862 |

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
a public consumer actor calls `Attest` on a header record with a growing query; the header's
handler then differs only in the bytes it receives and echoes.

| query bytes | `Attest` handler: input bytes | journal bytes | cycles | the consumer's handler, which also sends the query: cycles |
|---:|---:|---:|---:|---:|
| 0 | 436 | 826 | 11,556 | 13,955 |
| 8,192 | 8,628 | 9,018 | 58,236 | 88,794 |
| 24,576 | 25,012 | 25,402 | 151,316 | 236,590 |
| 49,152 | 49,588 | 49,978 | 290,924 | 458,287 |

Linear to within 0.5%: a handler pays **5.7 cycles per byte it receives** (reading the input,
decoding it, echoing it into the journal and hashing the journal) and about **3.4 more per byte
it sends on** (this consumer pays 9.0 per byte received and sent); a handler that does almost
nothing costs 9,000–26,000 cycles (`Read` 11,256, `Record` 15,862, `JobOpen` 18,462–25,857). A
public consumer's whole read of a record (consumer → `Read` → `StateReply` → consumer) costs
35,262 cycles. At 5.7 cycles per byte, a handler that loaded a
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
  instance refused the message (exit 202 after 15,658 cycles) and the transaction paid only its
  fee (execution gas 425,422, storage gas 224,063).
- A forged update (update_2's signature on update_1) cannot be proven: the private transition
  exits with code 40 (`SignatureInvalid`) after 9,028,059 cycles, and LEZ's prover refuses a
  non-zero exit (`lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs:674–697`).

**What LEZ charges the private transaction** (`settle_private_transaction`,
`lez/chain_state/src/apply.rs:435–556`): a flat `PRIVATE_VERIFY_GAS` = 409,764 execution gas for the
receipt (`lez/programs/fee/core/src/market.rs:49`), plus the cycles of its public part, plus a flat
`PRIVATE_GAS_STOR` = 224,063 storage gas (`market.rs:51–56`). Dev mode, from the test's output; the
base fee is 8 × (execution gas + storage gas). The real proof's settlement (below, on the earlier
image) charged the finality update the same:

| private transaction | public part (cycles, metered) | execution gas | storage gas | base fee at genesis prices |
|---|---:|---:|---:|---:|
| finality update (`ApplyVerified` → `Record`) | 70,090 | 479,854 | 224,063 | 5,631,336 |
| update handing over a committee, no new header (`ApplyVerified`) | 38,766 | 448,530 | 224,063 | 5,380,744 |

For comparison, the same finality update through the public job costs 9,216,121
execution gas and 4,451 storage gas in three transactions (§2). The private
placement trades on-chain execution (about 19× less) for storage gas (about 50× more: a
succinct receipt is 223,551 bytes by LEZ's constant) and for a proof made off chain. With
`MAX_GAS_STOR` = 1,000,000 per block, at most four such transactions fit in a block.

What the transaction makes public (the finality update on the new image, from
`results/prove-private-dry-run.log`, `describe_public_outputs` in `harness/src/private.rs`; a real
proof publishes the same fields, with a succinct receipt where the dry run has a fake one):

```
public_context.actors (3): ferrata@2nhBc476XLwdGYxHuku4C47GPQQU3t6Pzn9sxyQjxhXA, native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (4 steps):
  - PrivateToPublic from ferrata@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to ferrata@2nhBc476XLwdGYxHuku4C47GPQQU3t6Pzn9sxyQjxhXA: ApplyVerified { instance_id: 62a32b1c2734, job: 438 bytes } (475 bytes)
  - EndPublicSubtree
  - PrivateToPublic from native@EoTYfFi4rVfLkTBefMr8sB6GcyhRTZv682hg9J1S1QsL to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: ferrata, image_id: cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78 }
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

**The real proof was made on the earlier image; the new image's is pending.** The one real proof
of this path (`harness/src/bin/prove_private.rs`, `RISC0_DEV_MODE=0`, CPU only) proved the
program image `e7cec90e…` (the image of commit 7dfae96, before the merge) and was settled through
LEZ's state machine: proven, then settled with the real receipt verified (`Proof::is_valid_for`,
`validated_state_diff/mod.rs` via `check_privacy_preserving_circuit_proof_is_valid`), and
applied; afterwards the instance's store equalled the native store and the record was written.
Its transaction is the one above with the earlier image's id and the account ids derived from
it. Log: `results/prove-private-e7cec90e.log`. Every figure in this table is the earlier image's:

| earlier image `e7cec90e…`, commit 7dfae96 | |
|---|---|
| proven | the program's `VerifyAll` transition (step 1, Miller loops, final exponentiation, `ApplyVerified`) at a private account's actor, the fee transfer from that account, and LEZ's privacy-preserving circuit over both (`lee::execute_and_prove`, `ProverOpts::succinct()`) |
| the program's private transition | 9,045,235 user cycles in 10 segments (9,699,328 cycles padded); on the new image the executor counts the same (`results/prove-private-dry-run.log`) |
| wall clock | **3,240.5 s** for `execute_and_prove` (54 min 04 s for the whole run, setup and settlement included) |
| CPU | AMD Ryzen 9 7940HS (8 cores, 16 threads), no GPU: 39,206 s of user time, 1,211% average |
| peak memory | **9.18 GiB** maximum resident set (`/usr/bin/time -v`: 9,628,928 kB); a 2-second `ps` sample of all `r0vm` processes peaked at 10.2 GiB (10,710,560 kB) |
| receipt | **227,080 bytes** (the succinct `InnerReceipt`, borsh); the transaction is 231,034 bytes. LEZ charges a flat 224,063 bytes of storage gas whatever the size (`PROOF_BYTES` + 512) |
| settlement | execution gas 479,854 (`PRIVATE_VERIFY_GAS` 409,764 + public part 70,090 cycles), storage gas 224,063, base fee 5,631,336; the dev-mode settlement on the new image charges the same |
| load | not recorded during the run (2026-11-09, 22:53–23:47). For 8 of its 54 minutes my own executor-only test suites ran beside it at `nice 19`; whether other agents used the CPU is not known. Load average when the result was collected (01:51): 2.46 |

Proving time is that of a single run on a shared machine, not a benchmark. A proof of the new
image was started on this machine (2026-11-10, 08:27) and stopped after about 29 minutes to free
the machine; it is to be made on a GPU instead.

**Proving the new image (pending).** What a run elsewhere needs; `results/prove-private-dry-run.log`
shows these commands on the new image in dev mode:

- **The image.** Program `cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78`
  (508,284 bytes; consumer `aec03bcd…`). The guest ELF holds the absolute paths of the sources it
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
  FERRATA_EXPECT_IMAGE_ID=cec0bc687443be08c29c3216de8a3ed5f00a29d224fc266fe272f78cfe0f4d78 \
  FERRATA_FIXTURE_DIR=/path/to/mainnet-p1899 RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/path/to/r0vm \
  /usr/bin/time -v ./prove_private --tx-out stage2-private-tx.borsh > prove-private.log 2>&1
  ```
  It proves, settles the proof there through LEZ's settlement (receipt verified), checks the
  store and the record, and writes the proven transaction, its receipt included (about 231 KB).
- **The receipt, back here.** The transaction goes to `lez/results/stage2-private-tx.borsh`, the
  log to `lez/results/prove-private.log`. From `lez/`,
  `RISC0_DEV_MODE=0 target/debug/prove_private --settle results/stage2-private-tx.borsh` settles
  it again on this machine through LEZ's state machine, verifying the receipt, in seconds; it
  refuses a fake receipt and any transaction that does not claim image `cec0bc68…`.


## 4. Stage 3: a private consumer reads a record; the same consumer calling instead

`guests/consumer` is a minimal actor of another program. Run privately, `ReadRecord` sends `Read`
to `header(id, slot)` of the Ferrata program; the program replies with `StateReply` (the
`ReadState` → `StateReply` idiom of LEZ v0.4's native token and fee programs); the consumer
checks that the reply comes from the actor it asked and keeps the record in its private state.
`CallAttest` sends `Attest { query }` to the same actor instead. `harness/tests/private_consumer.rs`
runs both through `execute_and_prove` and LEZ's private settlement (dev mode) and prints every
public field of each transaction.

**Mechanism, from the source.** The read leaves the private part as a `PrivateToPublic` boundary
step (`execution_state.rs:781`) and the reply enters it as a `PublicToPrivate` step
(`execution_state.rs:805`). The prover predicts the reply from the public state it assumes; at
settlement LEZ runs the public handler again and refuses the transaction if its reply differs
from the prediction (`execution_state.rs:939–940`, `CrossMessageMismatch`). A record written once
keeps the prediction valid; `instance(id)`, which every update changes, would not.

**What became public — the read** (record of slot 15,564,800, the consumer's image disclosed):

```
public_context.actors (2): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (6 steps):
  - PrivateToPublic from consumer@61yQrZPL8iZ8NbnqXkUEs1JR6DiuSkX3bWxdZSjYSWEu to ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv: Read (1 bytes)
  - PublicToPrivate from ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv to consumer@61yQrZPL8iZ8NbnqXkUEs1JR6DiuSkX3bWxdZSjYSWEu: StateReply { state: 293 bytes } (297 bytes)
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@61yQrZPL8iZ8NbnqXkUEs1JR6DiuSkX3bWxdZSjYSWEu to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: consumer, image_id: aec03bcd8eedf5ff1fe14673be82d21ae1c8f95954fb0b9c4db7066d538e932a }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1612 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0

The 8567 serialized bytes of the transaction were searched for the consumer's private inputs:
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
public_context.actors (2): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (6 steps):
  - PrivateToPublic from consumer@AodAGCgEG5tEZ9G66zav6VgeBX9hfvyxnimA6U3L6j1C to ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv: Read (1 bytes)
  - PublicToPrivate from ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv to consumer@AodAGCgEG5tEZ9G66zav6VgeBX9hfvyxnimA6U3L6j1C: StateReply { state: 293 bytes } (297 bytes)
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@AodAGCgEG5tEZ9G66zav6VgeBX9hfvyxnimA6U3L6j1C to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Undisclosed { commitment root: c0c485f792dc61648b658b160d18b68d2d816ab976b685ae3d66b492b18bc341 }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1612 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0

The 8503 serialized bytes of the transaction were searched for the consumer's private inputs:
- the query (75 bytes): absent
- the account id the consumer acts for (32 bytes): absent
- the authorization secret key (32 bytes): absent
- the nullifier secret key (32 bytes): absent
- the nullifier public key (32 bytes): absent
- the viewing public key (its first 32 of 1,184 bytes) (32 bytes): absent
- the viewing key seed d (32 bytes): absent
- the viewing key seed z (32 bytes): absent
```

**The call** (`CallAttest` with a query naming a contract, a storage slot and a deposit):

```
public_context.actors (2): native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq, ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv
public_context.authorized_accounts: {}
execution.public_root: None
execution.boundary (6 steps):
  - PrivateToPublic from consumer@DYMXat67yG3gSP7GFoRaTUEHduv3mLWBRr5UousHBSho to ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv: Attest { query: 0x73746f72616765283078303030303030303032313961623534303335366342423833394362653035333033643737303546612c20736c6f74203729203d206465706f736974202334323432 } (80 bytes)
  - PublicToPrivate from ferrata@9xq6bB9mVU9PcSbkerXw2tz2j6fUi5LsgcFCPJ86Vciv to consumer@DYMXat67yG3gSP7GFoRaTUEHduv3mLWBRr5UousHBSho: StateReply { state: 293 bytes } (297 bytes)
  - EndPrivateSubtree
  - EndPublicSubtree
  - PrivateToPublic from native@DYMXat67yG3gSP7GFoRaTUEHduv3mLWBRr5UousHBSho to native@8ga8NDaWPPnpAo2sWwj4Cjrhcb8hvNa7nrxJaAZh6jwq: Some(Credit(100000000)) (17 bytes)
  - EndPublicSubtree
execution.program_image_claims (1):
  - Disclosed { program: consumer, image_id: aec03bcd8eedf5ff1fe14673be82d21ae1c8f95954fb0b9c4db7066d538e932a }
execution.private_actions (1): each a nullifier, a commitment-tree root, a new commitment and an encrypted note (1533 bytes)
execution.async_messages: 0
execution.recovery_bindings: 0
execution.validity: ValidityWindows { blocks: ValidityWindow { from: None, to: None }, timestamps: ValidityWindow { from: None, to: None } }
execution.fee: Some("refund_to Some(78Skt7EQTvVwfWVYnA1XMjghv3aUR1V5wgQAfVLKJZG5), fallback true")
nonces: 0, admission evidence: 0

The 8567 serialized bytes of the transaction were searched for the consumer's private inputs:
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
RISC Zero's fake receipt (4,308–4,340 bytes here), where a real one holds a succinct receipt (227,080
bytes in §3, on the earlier image). The search covers every byte, the fake receipt included; the tests fail if a
private input of the reader is found, or if anything but the query is found in the call.

**Finding.** Reading exposes *which* record was read (the `Read` to `header(id, slot)` and the
whole record in the `StateReply`), the consumer's program (as the read's sender, even when its
image claim is undisclosed), and an alias of its account; it does not expose what the consumer
does with the record or its query. Calling exposes the query itself, in the clear, in the
transaction (the test asserts the query's bytes are in the serialized transaction). This
supports the proposal's rule — a private consumer must read, not call — and qualifies it: the
slot read is public, so a consumer that must hide which block it proves against should read a
record many consumers read (the newest), as the proposal's EIP-2935 hops do.

Charged (dev mode; flat proof and storage costs as in §3): the read execution gas 421,020 (PRIVATE_VERIFY_GAS 409,764 + public part 11,256 cycles), storage gas 224,063. The call: execution gas 421,475 (PRIVATE_VERIFY_GAS 409,764 + public part 11,711 cycles), storage gas 224,063.


## 5. What did not work, what is missing, what is weaker than asked

- **Settlement is a copy, not the crate.** The harness runs `chain_state::apply`'s
  `settle_charged_transaction` / `settle_private_transaction` and `classify` as copies (with
  attribution, `harness/src/settle.rs`), because `chain_state` and `common` depend on the Bedrock
  node (libp2p, RocksDB, native circuit builds). Every call into `lee` is the sequencer's. Left
  out: the bridge and cross-zone-inbox guards of `validate_user_state_modification` (they reject
  writes to accounts these transactions never touch), and the block tail (the fee program's
  `Distribute` and the clock); each transaction is settled in a block of its own. Fees are at
  the genesis base fees (8 per gas unit); no base-fee movement was simulated.
- **Deployment.** The program is deployed at genesis, immutable, with
  `V03State::with_genesis_programs` as LEZ's own tests do, not by `program_loader` transactions;
  the cost of deploying a 508 KB image was not measured.
- **Private paths in dev mode.** The Stage 2 and 3 tests run with `RISC0_DEV_MODE=1`. One real
  proof was made (§3), for a finality update only, not for a handoff, and not for a consumer read,
  and on the earlier image: the proof of the new image is pending.
- **Per-handler cycles come from a replay.** `lee` reports one cycle count per transaction; the
  split by handler comes from `harness/src/trace.rs`, which re-executes each guest as `lee`'s
  `PublicBackend` does. Every test asserts that the split sums to LEZ's charge. Within a handler
  (input decoding vs. pairing vs. journal), the split was not measured inside LEZ; §2 compares
  with the bare-guest numbers of `../bench/` and with the shell-cost measurement instead.
- **Not implemented in the program:** fork upgrades (the store's light-client layout is fixed at
  `Init`); Gloas's RLP execution header (records carry the bound block hash only); the
  weak-subjectivity bound; successors and the root id of the proposal's §1; a submitter daemon
  (the harness plans each job natively, as one would); joining an open job; any clean-up of jobs
  whose final step fails (they stay, unapplied).
- **The execution layer is in the core, not yet in the program.** Since the merge with `main`
  the core verifies storage, receipts, logs and EIP-2935 hops against an `attest::HeaderRecord`
  (chain id, anchor slot, block number, timestamp, block hash, parent hash, state and receipts
  roots, binding). The program's `header(id, slot)` holds a `steps::HeaderRecord` (the beacon
  header, the execution root, and the payload's block number, block hash, state and receipts
  roots and timestamp): no parent hash, no chain id. The two are not yet one type, and the
  program has no handler that attests anything. The execution layer's cycle counts so far are
  bare-guest ones (`../bench/RESULTS.md`), not LEZ's.
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
  duplicate is refused by `JobOpen` after step 1 has run: 2,183,111 cycles. Making it cheap needs
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
   and `Finish` (refused for 5,731–23,420 cycles), not for opening a job (2,183,111).
5. **Private consumers' reads are public reads** (§4): the boundary shows which record was read,
   the record itself, and the consumer's program. An `Undisclosed` image claim hides the image
   from the claims list only; the program account still appears as the sender of the read. The
   proposal's "a private consumer cannot call without exposing its query" is confirmed; its
   implication that reading exposes nothing about the query is too strong: it exposes the slot.
6. **The gas bounds hold, with margin.** No transaction of the job is above 3.73M cycles (75% of
   `TARGET_GAS_EXEC`); a whole finality update also fits one transaction under `MAX_GAS_EXEC`
   (9,115,325 cycles), above the target. Since the caps are per block, at the target a block holds
   one step of a job (plus small transactions).
7. **The "about 15 cycles per byte" of the proposal** (from a comment in LEZ's integration tests)
   measured 5.7 cycles per byte of message for this program (§2).


## 6. Reproduce

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
  > results/harness-tests-dev-mode.log               # Stages 1–3 and the exit codes, 13 tests: prints every
                                                     # table; writes results/stage3-*
cargo run -p harness --locked --bin tables           # rewrites results/stage1-*.md, shell-costs.md, images.md
cargo build -p harness --locked --bin prove_private
RISC0_DEV_MODE=0 /usr/bin/time -v target/debug/prove_private --tx-out results/stage2-private-tx.borsh \
  > results/prove-private.log 2>&1                   # the real proof of §3, then its settlement (about an
                                                     # hour on this CPU; see §3 for running it elsewhere)
RISC0_DEV_MODE=0 target/debug/prove_private --settle results/stage2-private-tx.borsh   # settle it again
RISC0_DEV_MODE=1 target/debug/prove_private --allow-dev-mode --tx-out target/dry-run-tx.borsh  # a dry run
cd .. && cargo test -p ferrata-spectests --test steps -- --nocapture       # the step API against process_update
```

The harness builds in the dev profile with optimised dependencies: LEZ refuses to build its test
helpers (`lee/test-utils`, needed for private accounts) into a release build. The guests are
always built in release. Cycle counts do not depend on the host build.

Lint and format this workspace's own crates only. Clippy needs `RISC0_SKIP_BUILD=1`, or its
wrapper reaches the guest build and compiles the guests with the host toolchain:
`RISC0_SKIP_BUILD=1 cargo clippy -p ferrata-lez-wire -p harness -p methods --all-targets --locked -- -D warnings`
(clean). `cargo fmt --all` would also rewrite the path dependencies, LEZ's crates among them; use
`cargo fmt -p ferrata-lez-wire -p harness -p methods`, and `rustfmt --edition 2021` for the guests
(clean).

