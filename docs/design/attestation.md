# Ferrata attestation: design note (RFP-021)

This note describes Ferrata's attestation primitive: a LEZ program that verifies Ethereum sync-committee signatures and keeps write-once records of finalized headers, and the shared core that lets any LEZ program prove an account, a storage slot, an event log or an accumulator leaf against such a record. It covers the scope of Logos RFP-021; the bridge built on it (RFP-022) has its own note, and the bridge's finality analysis is in `docs/design/finality-rule/LEMMA.md`, `docs/design/finality-rule/MODEL.md` and `docs/design/finality-rule/RESULTS.md`. LEZ facts are cited from the repository's findings note, `docs/design/lez-v0.4-findings.md`, by section. Requirement IDs are RFP-021's; RFP-022 IDs are prefixed `022:`. Milestone names (M0, A1–A4) are those of the proposal.

Four labels are used throughout:

- **Measured**: from a run reproducible from the repository, always with where it ran:
  - **in-process harness**: LEZ v0.4.0-rc1's own state transition functions (`lee`), called in-process, with LEZ's fee settlement functions (`chain_state::apply`'s `settle_charged_transaction` / `settle_private_transaction`) copied rather than linked, because `chain_state` pulls in the Bedrock node. No sequencer ran. Each transaction is settled in a block of its own, at the genesis base fees, with no base-fee movement; the program is deployed at genesis, as LEZ's own tests do. LEZ numbers are LEZ's own metering of a transaction settled this way. Private paths run in RISC Zero dev mode (executed, not proven) unless a real proof is named;
  - **laptop CPU**: the real proof of an update, on an AMD Ryzen 9 7940HS (8 cores, 16 threads, no GPU); **cloud GPU**: one NVIDIA L4 (the same update on an earlier image, and one private attestation; `gpu-prover/RESULTS.md`);
  - **bare guest**: RISC Zero zkVM 3.0.5 executor counts (user cycles, no proving);
  - **native tests**: `cargo test --workspace` on the host.
- **Derived**: arithmetic on measured numbers, with the inputs given.
- **Estimated**: extrapolated, with the basis given.
- **Not built**: designed, not yet implemented or not yet evidenced.

## 1. Purpose and scope

A LEZ program acting on an Ethereum fact needs a trusted header and a proof that the fact is under it. Ferrata provides both without a trusted relayer:

- **The attestation program** checks finality updates against the sync committee itself and writes a record of each finalized execution header it accepts. Anyone can submit updates; they stand or fall on their signatures.
- **`ferrata-core`**, one `no_std` Rust crate, runs in the program, consumers' proofs, the Logos module, the CLI dry run and the tests. It verifies facts against a record, evaluates predicates and derives statement identifiers.

The core's light client and execution layer, including accumulator-leaf facts, are built and tested natively (§5, §12). The program is in `lez/` and was driven with real mainnet data through the in-process harness over LEZ's state machine, not on a sequencer (§3, §4). Attestation through the program is built and was measured the same way on a mainnet block finalized on 10 November 2026: accounts, storage slots and event logs with predicates, identifiers and one history hop, a reference consumer on the public path and a private attestation (§6, §7; `lez/RESULTS.md` §5). On Sepolia the program follows the chain through the Gloas fork, its store upgrading itself at the fork of its schedule, and attests a storage slot, a log and an account under a Gloas-era record with the block's RLP header as witness; accumulator leaves run in the guest, on a synthetic state holding the vault's own deposits (§4.7, §5; `lez/RESULTS.md` §6). Successors, the weak-subjectivity bound and all off-chain software are not built (§15).

## 2. Chain of trust

```
checkpoint block root ─ Init: bootstrap verified; config fixed (fork schedule, genesis time, slot duration)
 └─ current committee (proven from the checkpoint state)
     └─ finality update, ≥ 2/3 of the committee, domain = (fork version at signature_slot − 1, genesis_validators_root)
         ├─ finality branch ─► finalized beacon header ─► execution binding
         │     Capella–Fulu: payload header · Gloas: bid.parent_block_hash + the RLP header that hashes to it
         │   └─► header(instance, slot): write-once record (number, hash, state root, receipts root)
         └─ next-committee branch ─► adopted at the handoff
consumer: record ─ state root ─ account ─ storage slot          (Merkle-Patricia proofs)
                 ─ storage slot holding an accumulator root ─ leaf (SHA-256 path)
                 ─ receipts root ─ receipt, status 1 ─ log
                 ─ older block: EIP-2935 hop (account proof, storage proof, RLP header)
```

The record also carries the beacon header, the execution root and the timestamp (§3.1). Two things must be trusted: the starting checkpoint, which anyone can compare with public Ethereum (§11.4), and an honest two-thirds supermajority of each sync committee (§11). Everything below the signature is checked by hashes and Merkle branches. Execution roots reach a consumer only from a record, never from the caller.

## 3. The program on LEZ v0.4

In LEZ v0.4 a handler sees only its own actor's state and communicates by messages. The attestation program is one immutable RISC Zero guest whose actors are program-derived accounts. Two builds were measured. The figures of §3 and §4 are those of image `e7cec90e8c2bb141de49c25c8d1e4f8496e37f0eaea9f349097bc7086e71d7d5` (508,240 bytes), which had no attestation handler, no chain id in its configuration and a 293-byte record. The attestation figures of §5 to §7 are those of image `2f3566d4f460620867a93b557776400b1039142ce92203072170c162ba2ec130` (592,752 bytes), whose configuration holds the chain id and whose record holds it and the parent hash (333 bytes); `lez/RESULTS.md` has that build's figures for the transactions of §4, which differ from the ones here by at most 0.08% for a transaction of a million cycles or more.

### 3.1 Actors

| Actor | State (measured size) | Written by |
|---|---|---|
| `instance(id)` | configuration fixed at `Init` (chain id, `genesis_validators_root`, genesis time, slot duration, preset, fork schedule; in the design also the weak-subjectivity bound and root instance id, not built), an active flag, and the compact store: finalized beacon header, committees as 80-byte summaries (key root, aggregate key). 425–505 bytes | `Init`; `Apply` / `ApplyVerified` |
| `job(id, job_id)` | one public-path update in progress: digests, signature-check phase, the 576-byte Miller product, adoption progress. 1,249–1,409 bytes | `JobOpen`, `Miller`, `Adopt`; cleared by `Finish` |
| `header(id, slot)` | the record: beacon header, execution root, block number and hash, state and receipts roots, timestamp. Written once; 293 bytes | `Record`, sent by `instance(id)` at the last step of `Init` and of each update that finalizes a new header |

`id = H(config ‖ checkpoint)`; `job_id = H(id ‖ store digest ‖ update digest)`.

There is no committee actor. An earlier version of the design kept both committees' compressed keys (49 KB) in one. On v0.4 a handler pays for every byte it loads (§4.3), so committees are kept as summaries and keys arrive as witnesses.

### 3.2 Messages and authentication

| Message | Sent to | Accepted from | Sender checked |
|---|---|---|---|
| `Init` | `instance(id)` | anyone | no |
| `Open` (step 1) | `instance(id)` | anyone | no |
| `Miller` (step 2) | job | anyone | no |
| `Adopt` (256 keys per part) | job | anyone | no |
| `Finish` (step 3) | job | anyone | no |
| `VerifyAll` (all steps in one call) | any actor | anyone | no |
| `Read` | any actor | another actor | no; it replies with the actor's state |
| `Attest` | `header(id, slot)` | anyone | no; it proves the request under the record and replies with the statements (§6.1) |
| `JobOpen` | job | only `instance(id)` of this program | yes |
| `Apply` | instance | only `job(id, job_id)` of this program | yes |
| `ApplyVerified` | instance | only an actor of this program (`from_own_program`) | yes, by program |
| `Record` | header | only `instance(id)` of this program | yes |

Every handler checks that its receiver is the PDA the message implies, that is, that the message arrived at its own address. Only the four internal messages also check that the sender is this program's expected actor; the external steps are open to anyone and stand or fall on what they carry. The runtime sets `from`; a sender cannot claim it (findings note §6). Refusals exit with a code and never panic.

`ApplyVerified` is authenticated by program, not by account. A private actor's message to a public actor carries the sending program's account id unblinded, and LEZ's circuit accepts a private transition only with a receipt of the image deployed at that program account, so `from.program_account_id == ferrata` means "produced by this program's code". Only `VerifyAll` sends `ApplyVerified`, after the whole verification, and the job carries the digest of the store it was verified against, so it applies once. The same message from a private actor of another program was refused (exit 102) and its transaction paid only its fee (in-process harness, dev mode).

### 3.3 Immutability (VS.3)

The program is deployed with an immutable header, which the v0.4 loader enforces (findings note §7). The planned deployment check (A1) compares the image id with a reproducible build. The same bytecode can be deployed at several addresses (findings note §7), so consumers pin the program's header account, never a program id held in mutable state. In the harness the program is deployed at genesis, immutable, as LEZ's own tests do; deployment by `program_loader` was not measured.

### 3.4 Instances (F.9, F.13–F.15)

An instance's id is the hash of its configuration and checkpoint, so no instruction can reconfigure an instance and nobody can squat a well-known id. One program serves mainnet, Sepolia, Hoodi and minimal-preset devnets. The chain id is in every instance id and statement identifier. In the measured refusals (in-process harness), an update meant for another chain's instance runs to `Finish` and is refused there with exit 40 (`SignatureInvalid`).

### 3.5 Successors and their invariants (F.10, F.15)

A fork scheduled after `Init` (for example Gloas on mainnet, which has no date yet) is handled by a successor instance, which consumers accept under the root id with no new trust, because `Init` enforces three invariants:

1. it copies every immutable field of its parent except the fork schedule, which must agree for every fork at or before the checkpoint;
2. its checkpoint is the parent's latest record;
3. that record is inside the weak-subjectivity bound.

A successor inherits the root id that consumers bind to, so the handoff to it needs no consumer redeploy. Separately, a fork that changes light-client containers or execution-layer formats needs a new `ferrata-core` release before it activates (§5.1), and consumers that run the core in their own proofs need a new version for it. **Not built**: the program fixes the store's layout at `Init` and has no successors or root id yet (milestone A1).

### 3.6 Replacement

An instance past its weak-subjectivity bound, or stranded by Ethereum non-finality (§4.1), is replaced from a fresh checkpoint under a new root id. That is a new trust decision; the consumer guide will carry the migration recipe. In the design the bound is fixed per instance from the specification's weak-subjectivity computation, about two weeks on mainnet, and `monitor` alarms at half of it. **Not built**: the bound (§4.6) and `monitor` (§8). A misconfigured checkpoint is handled the same way, never by mutation (F.13).

## 4. Advancing an instance

### 4.1 Policy and its liveness limit

A header enters only through a finality update signed by at least two thirds of the committee. The program keeps no optimistic header and never forces one. Run beside the specification's store over every official vector (native tests), the policy store (`FinalizedStore`) agrees on 150 steps until the specification's store forces a non-finalized header, and never holds a header that store did not finalize.

The cost is a liveness limit: if Ethereum finalizes nothing through a whole sync-committee period, the store cannot learn the next committee, stops, and the instance is replaced (§3.6). `stops_safely_after_a_period_without_finality` asserts the stop on all seven forks. Stalled submitters are a different case (R.7): the instance resumes when anyone sends the missing updates (within the weak-subjectivity bound, once that is built, §4.6).

### 4.2 Two update paths, one verification

LEZ v0.4 caps a block, and so a transaction, at 10M execution cycles (`MAX_GAS_EXEC`); the base fee stops rising at 5M (`TARGET_GAS_EXEC`). A finality update costs 9.2M as LEZ charges it, nearly a block of the zone's public capacity. The same update can be submitted two ways, verified by the same code:

- **Proven.** Anyone who can prove 9.0M cycles runs `VerifyAll` in a privately executed actor of the program, which sends `ApplyVerified` to the public instance (§3.2). One real proof was made on a laptop CPU and settled in the in-process harness.
- **Public job.** Anyone, with no prover, sends three transactions; five when a committee is adopted. `Init` and five real mainnet updates, a committee handoff among them, were settled in the in-process harness through LEZ's own state machine and metering, with LEZ's fee settlement copied.

Measured on mainnet periods 1899–1903 (Fulu, 512-member committees), in the in-process harness:

| Operation | Transactions | Largest transaction, execution gas | Total execution gas | Total storage gas |
|---|---:|---:|---:|---:|
| `Init`: bootstrap, then adoption in two parts | 3 | 3,479,974 | 7,085,450 | 51,968 |
| Finality update, public job | 3 | 3,719,489 | 9,216,121 | 4,451 |
| Update that rotates and adopts a committee | 5 | 3,730,145 | 16,076,120 | 54,491 |
| Update that only supplies the next committee | 5 | 3,694,714 | 15,984,062 | 53,675 |
| Finality update in one public transaction (`VerifyAll`) | 1 | 9,115,325 | 9,115,325 | 4,089 |
| Finality update, proven path | 1 | 479,854 | 479,854 | 224,063 |
| Handoff with no new header, proven path (dev mode) | 1 | 448,530 | 448,530 | 224,063 |

Execution gas is the user cycles LEZ metered, storage gas the transaction's serialized size. After every job the instance's store equals the native `FinalizedStore`'s, every finalized root equals the canonical root a second provider reports, and the record equals the native finalized header's.

**The public job** splits at the pairing. Step 1 (`Open`, 2.13–2.18M cycles): branches, the execution binding, the signing key as `aggregate − Σ non-signers` (non-signer keys arrive uncompressed with branches to the key root), and the hash to the curve. Step 2 (`Miller`, 3.31M): one two-pair Miller loop, storing the 576-byte product. Step 3 (`Finish`, 3.65M): the final exponentiation, then `Apply` and `Record`. Every path checks that the signature is in G2 and not the identity. The largest transaction is 75% of the 5M target, and a CI gate will hold it there (none exists today); at the target a block holds one step of a job.

**The proven path.** LEZ charges a private transaction a flat 409,764 execution gas for the receipt plus its public part's cycles (70,090 here), and a flat 224,063 storage gas. Against the public job that is about 19 times less execution gas and about 50 times more storage gas. With 1,000,000 storage gas per block, at most four such transactions fit in a block (findings note §6). The real proof (`VerifyAll`, 9,045,235 user cycles, plus the fee transfer and LEZ's circuit) took 3,240.5 s on the laptop CPU (54 min for the whole run), peaked at 9.18 GiB and produced a 227,080-byte succinct receipt: one run on a shared machine, not a benchmark. Other private-path tests ran in RISC Zero dev mode; no handoff has been proven.

The proven path is far cheaper on chain but needs a prover; the public job keeps liveness from depending on anyone owning one.

### 4.3 Committee adoption

A handler pays 5.7 cycles for every byte it receives and about 3.4 more for every byte it sends on (measured in the in-process harness, linear to within 0.5%). Committees are therefore stored as 80-byte summaries, and keys arrive only when a committee is adopted: 512 uncompressed keys, 256 per transaction, at 3.40–3.42M cycles each (measured, in-process harness). Each key must be canonical, on the curve, not the identity, and re-compress to the bytes hashed into the key root; together they must sum to the committed aggregate.

Adoption is needed whenever the store learns a committee: at `Init`, at an update that only supplies the next committee, and at each rotation. An earlier version of the design costed it as one 5.7M step; on LEZ the whole adoption is 6.8M, above the target, so it takes two transactions. About 0.3M of its excess over the bare guest is the shell; the remaining 0.8M was not broken down.

Decompressing the keys in the program would cost 48.7M cycles, and the per-key subgroup check 185M per committee (bare guest). The subgroup check is left to the off-chain tools, because every key in a genuine beacon state passed `KeyValidate` at deposit; the repository runs both committee checks over all vectors.

### 4.4 Refusals and what they cost

A coded refusal pays only the cycles it used and changes no state. Eighteen refusals ran through LEZ's settlement path in the in-process harness (measured):

| Refusal | Execution gas | Code |
|---|---:|---|
| Forged signature, at `Finish` (steps 1 and 2 pass and are paid) | 3,635,478 | 40 `SignatureInvalid` |
| Update for another chain's instance, at `Finish` | 3,637,696 | 40 `SignatureInvalid` |
| Malformed signature, at `Open` | 353,214 | 41 `SignatureMalformed` |
| Duplicate `Open` of the same job | 2,183,111 | 107 `JobExists` |
| `Open` of an already applied update | 207,687 | 52 `NotNewer` |
| `Finish` before `Miller`; `Miller` replayed | 23,420; 20,491 | 81 `StepError::Phase` |
| `Finish` after the job applied; a job id sent to another instance | 5,731 | 105 `NoJob` |
| Internal message from a wrong sender (five cases) | 17,325–31,350 | 102 `WrongSender` |
| `Init` of an existing instance | 17,491 | 103 `AlreadyInitialized` |
| `Open` before its signature slot | 10,000,000 | LEZ: outside the validity window |

Two kinds of refusal are not cheap:

- **A duplicate first step costs 2.2M.** The instance cannot see whether the job exists, so `JobOpen` refuses the duplicate after step 1 has run; making it cheap would add a round trip to every update.
- **LEZ's own refusals are charged the whole declared gas limit**: measured for a transaction outside its validity window, and read in the source (not measured) for a delivery to an unadmitted account. A submitter bounds both with a tight gas limit.

### 4.5 Concurrency (R.2, R.8, F.11, VS.6)

A job's id is the hash of the instance, its starting store and the update, so a job can finish only on the store it began on, and a duplicate is refused once its job is stored (§4.4). The proven path applies with a compare-and-swap on the same store digest. No external step checks who sent the transaction (§3.2): an account with no prior relationship to the instance can advance it, and an update is accepted on validity alone. **Not built**: a daemon joining another party's open job, and clean-up of jobs whose final step fails (they stay, unapplied).

### 4.6 Weak subjectivity (F.10, R.3)

**Not built.** Neither the program nor `ferrata-core` enforces a weak-subjectivity bound today, and nothing configures one. `FinalizedStore::process_update` checks only the specification's slot order and period rule, so an instance can be advanced from an old checkpoint, one period at a time, by any chain of handoffs its committees signed. The design (milestone A1): the bound is fixed in the instance's configuration (§3.6), and an update whose signature slot is too far past the store's finalized slot is refused, with no clock needed.

Built and measured: `Read` and `Attest` carry no validity window, so records never expire (R.3). Separately, `Open` is valid only from its signature slot's start; sent early, it is LEZ's own refusal at the full gas limit (§4.4). LEZ timestamps are producer-chosen (findings note §7).

### 4.7 Forks and Gloas

The signing domain uses the fork version at `signature_slot − 1` from the configured schedule and `genesis_validators_root`. The core passes the official light-client vectors from Altair to Gloas, including fork transitions (minimal preset, native tests).

Under EIP-7732 (Gloas) a block commits to a payload bid, and the payload can be withheld. A Gloas record therefore carries the bid's parent block, one execution block behind, from an RLP header that must hash to the bound `parent_block_hash` (gindex 2,856). Guard tests check the zero padding of older headers carried into a Gloas store. The native Gloas vectors bind a single constant block hash, so the tampering tests on Gloas headers matter more than the vectors.

Status: Gloas is tested on real Sepolia data (Gloas since epoch 353,024, 6 October 2026; fixture fetched 10 November 2026): the light client starts from the bootstrap of the last pre-Gloas period (1378) and follows a Fulu-era update, the store's upgrade to the Gloas layout, 31 Gloas-era handoffs and a Gloas finality update, with both BLS libraries, both adoption checks and both stores; every finalized root equals a second provider's. `HeaderRecord::from_block_hash` is built from the real bid's parent block hash and the block's real RLP header (Amsterdam layout, 23 fields) and equals two providers' data for Sepolia block 12,103,422; tampering with the real Gloas data is refused with the expected codes. No public Sepolia node found serves a bootstrap for a Gloas-era checkpoint, so the bootstrap predates the fork. In the program (measured in the in-process harness, `lez/RESULTS.md` §6): an instance initialised from that bootstrap with Sepolia's schedule follows every period's update to 1409 over the public job path, and its store upgrades itself to the Gloas layout with the first update attested under Gloas (`steps::store_for_update`; a fork that is not in the schedule fixed at `Init` still needs a successor, §3.5). A Gloas record holds the bound block hash only; `Attest` takes the block's RLP header as a witness, checks that it hashes to the recorded hash and reads the number and the roots from it (exit 215 without it, 120 for a header that does not hash to the record: another block's, tampered, truncated or over-long). **Departure from the paragraph above:** the record does not carry the roots; each request carries the header, 20,663 cycles and 691 bytes per request. On Sepolia block 12,105,936 (finalized 10 November 2026) a storage slot, an ERC-20 `Transfer` and an account were attested this way (278,253, 96,713 and 169,611 cycles), and the same proofs asked as mainnet statements were refused.

## 5. The verification core

`ferrata-core` builds for `riscv32im-unknown-none-elf` with the pure-Rust backend (the `blst` backend needs RISC Zero's RISC-V C toolchain). One crate everywhere means verdicts and error codes cannot drift.

### 5.1 Inclusion

A `HeaderRecord` holds one block's number, hash, state and receipts roots, the chain id and the binding it came from. It is built from a `FinalizedStore`'s finalized header, derived through an EIP-2935 hop, or restored from the program's record bytes. Every attestation takes a record and the chain id the consumer expects; nothing takes a root from the caller.

- **Headers**: the Cancun (20 fields), Prague/Osaka (21) and Amsterdam (23) layouts; anything else is refused, including unknown trailing fields, so a fork that adds a header field needs a new release before it activates.
- **State**: accounts and storage values by Merkle-Patricia proof, inclusion and exclusion.
- **Receipts and logs**: by position in the receipt, EIP-2718 types 0 to 4, status 1 required, pre-Byzantium refused. `exec::locate_log` converts JSON-RPC's block-wide `logIndex`.
- **Accumulator leaves**: `attest_accumulator_root` is a storage attestation whose value is a SHA-256 accumulator root; `attest_accumulator_leaf` proves a leaf against it (depth 32, inner node `SHA-256(0x01 ‖ left ‖ right)`, empty leaf zero, direction bits from the leaf index, which must be below 2^32). The leaf is opaque to the core.

Measured end to end on mainnet block 26,379,842, finalized by the light client (native tests): WETH, USDC and an address with no account each equal `eth_getProof` and `balanceOf`; all 870 logs of its 245 receipts are attested; tampering with each kind of fixture byte is refused with its code. Accumulator-leaf facts are tested against `vectors/acc-leaf.json`, written by an independent Python implementation and shared with the bridge crate: 14 leaves in two snapshots, 7 refusals with their codes, 4 identifier variants; every sibling and every index bit is bound, and an index of 2^32 or more and the empty leaf are refused.

Bare-guest cost (measured, executor only, RISC Zero's accelerated Keccak; without it in brackets):

| Call | User cycles | Keccak-f permutations |
|---|---:|---:|
| `attest_storage`, WETH balance slot | 210,502 (1,040,590) | 54 |
| `attest_log`, one-log receipt | 42,445 (211,537) | 11 |
| `attest_log`, 96-log receipt | 432,102 (2,584,182) | 140 |
| `history_hop`, n to n − 16 | 182,627 (951,227) | 50 |
| `storage_id` (SHA-256 identifier) | 1,629 | 0 |

With the accelerator, the permutations are proven by RISC Zero's separate Keccak circuit and are not in these counts. Proving time and receipt size were not measured. Accumulator-leaf attestation has no bare-guest measurement.

### 5.2 Identifiers (F.6)

Each fact has a domain-separated SHA-256 identifier over fixed-width fields, independent of the instance:

| Fact | Identifier over |
|---|---|
| Log | chain id, block number, transaction index, the log's position in its receipt, the log's hash |
| Storage | chain id, address, slot, value |
| Account | chain id, address, its four fields |
| Block-bound variant (`id_at_block`, kinds `0x11` and `0x12`) | the same fields plus the block number |
| Accumulator leaf | `"ferrata/fact/acc-leaf/v1"`, chain id, contract address, slot, leaf index, leaf |

The storage identifier leaves the block out, which suits "this value holds" but not "this event happened once": a balance that changes and changes back gets the same identifier. Once-per-event consumers key on logs or on accumulator leaves, whose identifiers are one per event. The block-bound variant is for consumers that pin a block (a snapshot) and want one use per pinned block; it does not identify events either, since an unchanged value at two blocks gets two identifiers.

The accumulator-leaf identifier leaves the root out, so it is the same under every later root, and uniqueness keyed on it holds across roots, where keying on the root's identifier would allow one claim per root. The bridge's deposits are this type.

Status (native tests): the log, storage and account identifiers equal an independent Python computation; the block-bound variant is pinned by tests against an independent implementation; the accumulator-leaf identifiers are covered by the shared vectors (§5.1), and a leaf proven under two roots attested at two blocks has one identifier. Through the program (in-process harness, `lez/RESULTS.md` §5): the account, storage and log identifiers the program returns for a real mainnet block equal a derivation from the JSON-RPC fields alone. The accumulator-leaf identifiers are checked through the program's request function on the host, not in the guest, and the program does not return the block-bound variant (a consumer derives it from the reply).

### 5.3 Predicates (F.4, soft #4)

`=, ≠, <, ≤, >, ≥` over 256-bit words, against a constant or another attested value of the same record (`Relation`), and several facts under one record in one call. Predicates and relations are among the mutation-tested code (§12).

### 5.4 History hops (soft #3)

EIP-2935 keeps the last 8,191 block hashes in a system contract. One hop proves an older block from a recorded one: an account proof of the history contract under the record's state root, a storage proof of slot `n mod 8191`, and the RLP header of block `n`. The verifier takes `n` from the decoded header, never from the caller, and requires `R − 8191 ≤ n < R`, the contract code EIP-2935 specifies, and the slot to hold the header's hash. Hops chain. The mainnet test makes three real hops: to the parent block (whose 3 failed transactions are refused), to n − 16, and to n − 16 − 8191, the window's edge. Blocks before Prague cannot be reached this way, and on mainnet every hop needs archive `eth_getProof` at the record's block.

### 5.5 Error codes (U.5)

One enum of stable numeric codes is shared by the program and the dry run, covering the U.5 failure modes. Codes seen in the measured program runs are listed in §4.4.

Each code has one owner: the core's are 1–79 and 100–199, the step API's 80–99, the program's own 200–214 and the reference consumer's 220–229, and a test fails if two owners share a code (`lez/harness/tests/exit_codes.rs`). The table of §4.4 was measured before the program's codes moved from 100–112 to 200–212 and shows the earlier numbers.

### 5.6 No scan (P.9)

No action reads a variable-length collection. Records, roots and nullifiers are addressed by key; committees are fixed-size summaries.

## 6. Consumption

### 6.1 Public consumers call (F.7)

A public consumer sends `Attest{query}` synchronously to `header(id, slot)` and acts in its reply handler after checking that the reply comes from `header(id, slot)` of the pinned program. The program id alone would not do, because the same image can be deployed elsewhere. v0.4 runs synchronous messages depth-first in one transaction, so the check and the action succeed or fail together. A public consumer's whole read of a record costs 35,262 cycles (measured, in-process harness). **Built** (measured in the in-process harness on a real mainnet block, `lez/RESULTS.md` §5): `Attest` proves the request's accounts, storage slots and logs under the record, checks its conditions and relations, and replies with the statement identifiers and values. A storage slot costs 274,842 cycles, a log 96,584 (1,618,872 in a receipt of 306 logs), one history hop and a slot 498,204, with RISC Zero's accelerated Keccak; without it 1,099,689, 340,097 (8,226,393) and 2,104,035. A refusal exits with a code, which on v0.4 reverts the whole transaction: the consumer's handler never runs after a failed attestation.

### 6.2 Private consumers read (F.8)

Every message from a private execution to a public actor is published in plaintext (findings note §6). A private consumer therefore sends one fixed-size `Read` to a write-once record, receives it in a `StateReply` (333 bytes of state; 293 before the record carried the chain id and the parent hash), and runs `ferrata-core` inside its own proof. At settlement LEZ re-runs the public handler and refuses the transaction if the reply differs from the prover's prediction. A record written once keeps the prediction valid; `instance(id)`, which every update changes, would not.

What a read exposes (measured in the in-process harness, dev mode; every byte of the transaction searched): which record was read, the record, the consumer's program (even with an undisclosed image claim), and an alias of its account. Absent: the query, the account the consumer acts for, and its keys. The consumer therefore reads the newest record, which many consumers read, and reaches older blocks by hops inside its proof; the read then shows only that some transaction of that program acted after that record. LEZ charged the read 421,641 execution gas and 224,063 storage gas. A consumer that verifies an ERC-20 `Transfer` of a real mainnet block this way takes 380,577 user cycles for its two transitions, with no byte of the query, the proof or the result in the transaction (measured, in-process harness and executor, dev mode; `lez/RESULTS.md` §5). That transaction, which contains such a read, was proven once on one NVIDIA L4 (13.5 s, a 227,170-byte receipt), on the previous consumer image (382,336 user cycles), and settled with the receipt verified; it has not been proven on a CPU.

### 6.3 Why not a private call

The same consumer calling `Attest` privately publishes its query in the clear; the test asserts that the query's bytes are found and nothing else of the consumer's is (in-process harness, dev mode).

### 6.4 Independence (R.11)

Records never change and reads write nothing, so private transactions in one block never conflict. LEZ bounds the rate: at most four private transactions per block (findings note §6).

### 6.5 The F.8 test (not built)

Two deposits of the same token and amount, in different blocks, one needing no hop and one needing three, are claimed against the same record. Their serialized transactions, block bodies, events and state diffs must be identical except for fresh randomness, the nullifier and base-fee movement. A private consumer that calls instead of reading must fail the same test. The read/call dump of §6.2–6.3 is the measured precursor. The wallet proves at submission time against the newest record; proving early and submitting later would date the deposit. The F.8 test is part of milestone A2's done gate.

## 7. The Recommended Consumer Pattern (S.4)

S.4 asks for a nullifier keyed on the statement identifier, and F.8 forbids new correlation signals. A nullifier that is a function of the identifier alone is computable from public Ethereum data for every deposit, and matches each private action to its deposit. The pattern therefore has seven rules: the RFP's four, and three added.

The RFP's four:

1. **Enforce uniqueness with a nullifier keyed on the statement identifier** (§5.2), in the form rule 6 gives it.
2. **Verify the chain binding**: the identifier's chain id and the instance's root id are the ones the consumer expects.
3. **Handle each typed error code** (§5.5). A refused attestation consumes nothing and can be retried.
4. **Refuse the action when an attestation cannot be produced**; never fall back to an unverified input.

The three added:

5. **The attested fact carries a binding commitment to a secret the depositor chose**, over fixed-width fields so that it has exactly one opening.
6. **The nullifier is a hash of that secret and the statement identifier.** It is keyed on the identifier, as S.4 and `022:` F.3 ask, and is the same on the public and the private path: a public claim reveals the secret, a private claim proves it. One deposit cannot be claimed once on each.
7. **Exactly one kind of fact is accepted as the deposit.** Other facts from the same receipt, such as an ERC-20 `Transfer` log, are at most predicate inputs.

The pattern also requires consumers to:

- check the reply's sender (§6.1) and pin the program's header account: a consumer holding the program id in mutable state lets whoever controls that pointer substitute a module that attests to anything (VS.3);
- in a successor consumer, refuse any nullifier in the predecessor's set, with the predecessor frozen before the successor opens.

The two U.6 reference consumers (an ERC-20 and a native ETH transfer to a contract) attest token, sender, recipient and amount from the vault's event, one attestation per deposit; the bridge uses accumulator leaves. **Built** (in-process harness, `lez/RESULTS.md` §5): one reference consumer for the first case, on a plain ERC-20 `Transfer` of a real mainnet block rather than on the vault's event. It follows rules 1 to 4 and the two requirements above: it accepts the reply only from the record it asked, and it pins the program by deriving every one of its actors' addresses from its configuration (the program's account, the instance, the chain id, the token, the recipient), so that nothing mutable points at the program. Its nullifier is `H(configuration ‖ statement identifier)`, and it credits the transfer's sender. Rules 5 to 7 are **not built**: a plain `Transfer` commits to no secret, so this consumer has a private attestation and no private claim. **Not built** either: the native-ETH consumer (milestone A2) and the consumer guide that carries this section (milestone A4). `tests/nullifier_illustration.rs` illustrates the linkage problem; it runs no `ferrata-core` code.

## 8. Off-chain components (U.1–U.4, S.9, S.10)

All planned as Logos modules; none is built. The module, CLI, updater daemon, IDL and `verify_checkpoint` are milestone A3; the catalog release is milestone A4; `monitor` is planned but is not among milestone A1–A4's deliverables.

- **`ferrata_attest_module`** (Rust `cdylib`, built with `logos-module-builder`): produce and dry-run an attestation with the program's codes, advance an instance, read its configuration, verify a checkpoint. The CLI wraps each method.
- **Daemons**, each a module run under `logosctl daemon`, with structured logs, clean shutdown and configurable endpoints:
  - the **updater** follows Ethereum, submits updates, defers (§9) and resumes from on-chain state;
  - **`monitor`** compares records with several providers' canonical roots, alarms at half the weak-subjectivity bound (§3.6), and feeds consumers' freeze authorities (§11.3).
- **Ethereum access**: light-client data through `beacon_api_module.lc_request` (unverified until the program checks it), until the light-client API Logos R&D announced on 6 November exists; execution reads through `eth_rpc_module`, with `verified_proxy_module` in required mode wherever it covers the call. The proxy has no `eth_getProof`, so storage proofs use a second chain configuration with the proxy off, which is safe because every proof is checked against a record.
- **LEZ access** through `lez_core`, which pins v0.3.1; v0.4 support is in open pull requests #72 and #74 (pin the pull request, else call the sequencer directly).
- **IDL**: hand-written and checked in CI by decoding every accepted message from it alone; SPEL's stable release targets LEZ v0.2.4, and generated output replaces the hand-written IDL when SPEL supports v0.4. **Catalog**: the project's own, from `logos-modules-release-base`, released with `logos-modules-release-action` (milestone A4).

## 9. Liveness and deferral (R.9, R.10, S.7, F.12)

Anyone running the daemon sends updates: protocols whose users need fresh records, ecosystem operators, and the author in the post-delivery year. Nobody is obliged to; liveness rests on interested parties choosing to spend LEZ gas. An operator needs a consensus endpoint and LEZ gas, and no standing on Ethereum.

Each daemon waits a random share of a window `W`, re-checks the chain, then sends. With block interval Δ and n daemons, duplicates occur with probability `1 − (1 − Δ/W)^n`; the operator guide will tabulate duplicate rate against added latency. A concurrent operator's duplicate costs what §4.4 lists. Planned acceptance run (milestone A3 gate), not yet done: two honest daemons and one adversarial daemon run 100 updates; every update is applied once, the honest daemons' gas on refused steps is at most 10% of their gas on applied steps, and each honest daemon pays 35–65% of the honest total.

Incentives (soft #5) are design only: the bridge can pay a fixed rebate out of its protocol fee to whoever lands an update.

## 10. Tracking cost and update cadence (P.3, S.7)

Tracking cost is what keeping an instance current costs, shared by every consumer of its records; attestation cost (§5.1, §6) is paid per fact by each consumer. The two amortise differently, so they are reported apart.

**Recommended cadence** (design): one update per sync-committee period, the minimum the store needs to learn each next committee (§4.1), plus one per hour while roots are imported for the bridge. A sync-committee period is 8,192 slots of 12 s, 27.3 h; a year has 321.

**Per-update cost** (measured, in-process harness, mainnet periods 1899–1903; §4.2):

| Update | Path | Execution gas | Storage gas | Where |
|---|---|---:|---:|---|
| Finality update | public job, 3 transactions | 9,216,121 | 4,451 | in-process harness |
| Handoff that rotates and adopts a committee | public job, 5 transactions | 16,076,120 | 54,491 | in-process harness |
| Finality update | proven, 1 transaction | 479,854 | 224,063 | in-process harness; the proof made on the laptop CPU in 3,240.5 s |
| Handoff with no new header | proven, 1 transaction | 448,530 | 224,063 | in-process harness, dev mode (no real proof) |

A proven finality update costs 0.70M gas with execution and storage gas added together. A proven handoff that also finalizes a new header was not measured.

**Amortised** (derived from the table above):

| Cadence | Path | Execution gas | Storage gas |
|---|---|---:|---:|
| Once per period (one handoff) | public | 16,076,120 per period; about 5.16 billion per year | 54,491 per period |
| Once per period (one handoff, no new header) | proven | 448,530 per period | 224,063 per period |
| Once per hour while roots are imported | public | 221,186,904 per day of importing | 106,824 per day of importing |
| Once per hour while roots are imported | proven | 11,516,496 per day of importing | 5,377,512 per day of importing |

The amortised tracking cost per attestation is the cost over an interval divided by the attestations served from that interval's records. Proving one update per hour on the laptop CPU would take about 54 of every 60 minutes (derived from the one real proof, 3,240.5 s; one run on a shared machine). The figure at the recommended cadence, with real proving and recorded hardware, is a deliverable of the A4 performance report.

## 11. Trust (S.6, VS.4, VS.5, F.12)

### 11.1 Capture by stake

Seats are drawn in proportion to effective balance, so a share *p* of stake fills each of the 512 seats with probability about *p*. The chance of holding at least 342 seats (computed) is a lower bound: whoever proposes the last slots before the seed is fixed can bias it by withholding RANDAO reveals.

| *p* | Per period (27.3 h) | At least once over a year's 321 periods |
|---|---|---|
| 0.50 | 1.2 × 10⁻¹⁴ | 3.8 × 10⁻¹² |
| 0.55 | 3.6 × 10⁻⁸ | 1.1 × 10⁻⁵ |
| 0.60 | 8.9 × 10⁻⁴ | 0.25 |
| 0.65 | 0.21 | ≈ 1 |

### 11.2 Capture by bribery

Bribery is the realistic threat, and it has no protocol cost: each committee is known a period ahead, and a bad sync-committee signature is neither slashed nor loses a reward, so the bribe has no floor. The design therefore bounds the loss, not the cost: a consumer sizes what its caps let through before a freeze to what it can afford to lose. The bridge's trust document gives its numbers.

### 11.3 If a committee is captured (VS.4)

It can sign a forged handoff and hold the instance until someone notices. `monitor` compares records with several providers' canonical roots, and consumers wire its alarm to their freeze authority; caps bound how fast value can be lost. The fabricated-chain test (VS.1, consensus half; native tests) shows that a self-consistent update about a chain that never existed is refused on the signature alone.

### 11.4 Checkpoint verification (F.9)

`verify_checkpoint` recomputes the instance id, compares the checkpoint root with at least three independent beacon providers and with `verified_proxy_module`'s finalized block, checks that the chain id and `genesis_validators_root` belong together, and, as an off-chain tool, runs the full `KeyValidate` on the committee (§4.3). It ships in `ferrata_attest_module` and is offered upstream as a pull request to the Ethereum core module in milestone A3. **Not built.**

### 11.5 Participants

| Who | Provides | If it lies | If it stops | Learns |
|---|---|---|---|---|
| Any submitter or prover | updates | refused: signatures are checked | anyone else submits | public data |
| Beacon and execution providers | light-client data, proofs, receipts | refused: checked against signatures and records | another provider, or your own node | which blocks are fetched |
| Checkpoint source | each fresh checkpoint | a false root; `verify_checkpoint` catches it | nothing to stop | nothing |
| LEZ sequencers | inclusion, timestamps | cannot forge a record | the zone stops | public transactions |
| Deployer | the immutable deployment | nothing after deployment | nothing | nothing |

Apart from the checkpoint source, whose root `verify_checkpoint` lets anyone check, no participant can make the program accept a false record, and none is individually required to produce an attestation.

## 12. Verification programme (R.1, R.12, S.2, VS.1, VS.2)

| Gate | In the repository today (measured) | Added (not built) |
|---|---|---|
| Spec vectors | consensus-specs v1.7.0-beta.4, minimal preset, Altair to Gloas: `sync` 52/52 (in four BLS configurations: blst with three key aggregations, and zkcrypto), `update_ranking` 7/7; 61 official BLS cases, two libraries, three compositions, both verification paths agree; TrieTests 25/25; the program's step API matches `process_update` (outcome, store, code) on all 160 updates (native tests) | through the v0.4 program (milestone A1 gate); the `single_merkle_proof` vectors |
| Shared vectors | `vectors/acc-leaf.json` from an independent Python generator: 14 leaves, 7 refusals, 4 identifier variants (native tests); the same leaves and refusals through the program's request function on the host, on a synthetic state | in the guest, through the program (milestone A2) |
| Real chain | mainnet periods 1899–1903 (four handoffs) natively and in the in-process harness; the period-1906 execution test with 870 logs; a block of period 1907 verified and attested through the program in the in-process harness (accounts, storage slots, logs, one hop) | the Sepolia stream through the program (milestone A1 gate); a real Sepolia (Gloas) update (M0) |
| Differential | Helios (`helios-consensus-core` at a16z/helios 0.12.0) on 33 of the 52 official cases step by step (129 steps; the other 19 carry Altair-layout data, which Helios does not decode), comparing verdict, slot, beacon root and execution root; on mainnet periods 1899–1903 and the end-to-end update; on Sepolia through the Gloas fork; on 8 mutated mainnet and 11 mutated Sepolia Gloas updates and a wrong-committee store; and on participation 0 to 32 of 32. alloy-trie and alloy-rlp for the execution layer (§5) | on every stream in CI (milestone A1) |
| Adversarial | 16 light-client tests (incl. the fabricated chain; participation at 21, 22, 23, 32 of 32), 23 guard and 7 execution tests; three inputs on which Helios 0.12.0 departs from the specification, pinned as tests; 21 mutated mainnet updates through the step API; 18 refusals of updates and 41 of attestations and of the reference consumer in the in-process harness | withheld Gloas payloads; VS.1 inclusion half and VS.2 through the program (milestone A2 gate) |
| Mutation | 107 hand-picked mutants, all killed (43 light client, 64 execution layer) | `cargo-mutants` over the whole core; the mutation report (milestone A4) |
| Fuzzing | — | SSZ, RLP, Merkle-Patricia and query decoding (milestone A4) |

The Helios departures, none produced by a genuine chain with an honest committee. At 0.12.0 three remain: Helios accepts a signer key that is the identity point; accepts an empty finalized header whose finality branch does not prove it; and refuses, when more than half sign, an update from a committee whose aggregate-key field is not the sum of its keys, which the specification accepts. Two found at 0.9.4, a next committee contradicting the stored one and a forced update without finality (a panic), were fixed upstream (a16z/helios#819, #820). Each is pinned by a test.

## 13. Measurement plan (P.1–P.9)

A `just bench` target will regenerate every number from a fresh checkout with real proving and record the hardware. It is a deliverable of milestone A4 and is not in the repository yet; today each number comes from the test or script named with it. The declared minimum for user-side proving is 8 GB of memory and 4 cores, x86-64 or Apple Silicon.

| Item | Measured today | Not yet measured |
|---|---|---|
| P.1 one transaction per attestation | bare guest: storage 0.21M, log 0.04–0.43M, hop 0.18M user cycles (§5.1); through the program (in-process harness, one transaction each): account 0.17M, storage slot 0.27M, log 0.09M (1.62M in a 306-log receipt), a hop and a slot 0.49M, a consumer's claim of a log 0.18M; without the Keccak accelerator 0.34M to 2.1M, and 8.2M for the 306-log receipt | an accumulator leaf through the program; a chain of hops |
| P.2 cycles, gas, proof time and size | every update transaction in the in-process harness (§4.2); one real update proof on the laptop CPU: 3,240.5 s, 9.18 GiB, 227,080 bytes | attestation proof time and size |
| P.3 tracking cost, apart from attestation cost | per-update costs in the in-process harness; amortised figures derived from them (§10) | the A4 report at the recommended cadence, with real proving |
| P.4 user proving | no proof; the update proof is not a user proof (its 9.18 GiB peak exceeds the declared user minimum). A private attestation of a log: 380,577 user cycles for the consumer's two transitions (executor, software Keccak), LEZ's privacy circuit not counted | private read, attestation and claim proofs (time, memory); a 320-hop claim is estimated at about 58M accelerated user cycles |
| P.5 latency; P.6 off-chain resources | — | finality wait (about 13–19 minutes), update availability, the job's blocks, deferral; daemon CPU, memory, bandwidth |
| P.7 state growth | record 293 bytes, job 1,249–1,409, instance 425–505 (in-process harness); committee storage constant | projections over one and five years |
| P.8 BLS against a precompile | the two pairing steps (`Miller` 3.31M, `Finish` 3.65M) are 7.0M of an update's 9.2M (in-process harness); in the bare guest, Miller loops 40%, final exponentiation 35%, hash to G2 11% of 10.21M | the precompile delta; a precompile is reachable only from public transactions |
| P.9 no scan | by construction (§5.6) | — |

Caveats: per-handler cycles come from a replay whose sum each test checks against LEZ's charge; fees are at genesis base fees, with no base-fee movement simulated; each transaction is settled in a block of its own, without the block tail.

## 14. Reuse (R.6)

| Component | Decision | Basis |
|---|---|---|
| Light-client logic | own transcription of the specification, function by function | passes every vector; allows the finalized-only policy and the split at the pairing |
| Helios `consensus-core` (a16z/helios, tag 0.12.0, rev `2af9b41`) | not adopted as code; kept as a test oracle, a dev-dependency of the test crate only | the closest existing code, and the lineage SP1 Helios and `r0vm-helios` derive from; it needs `std`, cannot decode Altair-layout data, and at 0.12.0 departs from the specification on three inputs the repository pins as tests (§12) |
| BLS | `blst` (upstream audited by NCC Group, 2021) in RISC Zero's accelerated fork, `v0.3.16-risczero.0` | 2.45× cheaper per finality update than accelerated zkcrypto (10.21M against 25.02M); the tag in the v3.0.5 documentation costs 19% more; blst's raw API exposes the Miller-loop product |
| Differential BLS oracle | zkcrypto `bls12_381` (unaudited), off chain, a cross-check only | its Miller-loop result is private, so it cannot split |
| Merkle-Patricia and RLP | a small strict decoder in the core | alloy-trie and alloy-rlp as differential oracles, with two documented differences |

Everything new (the transcription, SSZ, Merkle-Patricia and RLP code, identifiers, accumulator facts, the adoption check, the split, the program shell) is in the scope of the third-party audit (S.8); audits of reused libraries cover only those libraries. The code is marked unfit for production until the reports are published.

## 15. What is not built yet

In the program: successors, the root id and the weak-subjectivity bound (§3.5, §4.6), and with them a fork that is not in the schedule fixed at `Init` (a scheduled fork is crossed in place, `lez/RESULTS.md` §6); accumulator leaves on real data (they run in the guest on a synthetic state; the vault is not deployed yet); a chain of several hops (one was measured); joining an open job and clean-up of failed jobs; deployment by `program_loader`; a run on a v0.4 sequencer. Attestation on the public call path and the private read path is built for accounts, storage slots, event logs, predicates, identifiers and one hop (§6, `lez/RESULTS.md` §5), on the public path also under Gloas records and for accumulator leaves (`lez/RESULTS.md` §6).

In the consumers: rules 5 to 7 of the pattern (the nullifier keyed on a depositor's secret) and with them a private claim; the native-ETH reference consumer.

In the core: the weak-subjectivity bound (§4.6).

Evidence: real proofs of a handoff, and of a private attestation on a user's machine (one was made on a cloud GPU, `lez/RESULTS.md` §5); the private path under a Gloas record; the vault's leaves on real data; the `single_merkle_proof` vectors; fuzzing; `cargo-mutants` over the whole core; the F.8 test; `just bench` and the P.1–P.9 report, including the amortised tracking cost at the recommended cadence (§10); a deployment on LEZ testnet 0.4, which does not exist yet (until it does, a standalone v0.4 sequencer with its block log published, recorded as a shortfall against S.1).

Off chain: the module, CLI, updater daemon, `monitor`, IDL check, `verify_checkpoint`, the catalog release, and the consumer, operator, CLI and trust documents.
