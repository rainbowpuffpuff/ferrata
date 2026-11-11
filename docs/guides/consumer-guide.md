# Consuming Ferrata attestations from a LEZ program

**Consumer guide (RFP-021 S.3, S.4; VS.3, VS.5). First draft, 11 November 2026.** For the author of a
LEZ program that must act on a fact about Ethereum: a storage slot, an account, an event log or a
leaf of an accumulator kept in a contract. It says how to ask for an attestation on the public path
and on the private path, what a statement identifier is and how to key a nullifier on it, the
Recommended Consumer Pattern, every error code, chain binding, and how to move to a new version.
Every statement cites the file it rests on; paths are relative to the repository root. §12 walks
through a whole consumer, step by step: `ferrata_holdings`, which accepts an ERC-20 balance at a
finalized block, with its tests on real data, its gas and cycles, and a real proof.

Labels, as in [`docs/design/attestation.md`](../design/attestation.md) (opening section): **measured**
(a run reproducible from the repository, with where it ran), **derived** (arithmetic on measured
numbers), **estimated**, **not built**. "In-process harness" means LEZ v0.4.0-rc1's own state
transition functions called in-process with LEZ's fee settlement copied, one transaction per block,
at genesis base fees; private parts in RISC Zero dev mode (executed, not proven) unless a real proof
is named. The code is a prototype, not audited and not for production ([`README.md`](../../README.md),
"Not audited").

## 1. What the program gives you

The attestation program verifies Ethereum sync-committee signatures and writes one **record** per
finalized execution block it accepts, at the actor `header(id, slot)`, where `id` is the instance
and `slot` the finalized beacon slot. A record is written once and never changes: beacon header,
execution block number and hash, state and receipts roots, timestamp, chain id and parent hash, 333
bytes of actor state ([`attestation.md`](../design/attestation.md) §3.1;
[`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "Decisions", item 2). Under the Gloas fork a record
binds the block hash only, and the block's RLP header travels with each request (§4.4 below).

Two ways to use a record:

| Path | You send | The program does | Your consumer sees | Public in the transaction |
|---|---|---|---|---|
| **Public call** (F.7) | `Attest { request }` to `header(id, slot)`, as a synchronous message | proves every fact of the request under the record, checks every condition and relation, replies | an `Attestation`: instance, chain id, block, and for each fact its statement identifier and values | the whole request and reply |
| **Private read** (F.8) | `Read` (one byte) to `header(id, slot)` from a privately executed actor | replies with the record (`StateReply`) | the 333-byte record; your proof then runs the same verification function, `attest::evaluate`, on it | which record was read, the record, your program, an alias of your account |

Both run the same verification code: `attest::evaluate` in the wire crate
([`lez/wire/src/lib.rs`](../../lez/wire/src/lib.rs), module `attest`), built on the one `no_std`
crate `ferrata-core` ([`crates/ferrata-core`](../../crates/ferrata-core)), so verdicts and codes
cannot differ between the program, a private consumer and an off-chain dry run
([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "The interface").

## 2. Pin the program, never a pointer (VS.3)

The program is deployed with an immutable header; the v0.4 loader enforces it. The same bytecode
can be deployed at several addresses, so a program id alone does not identify *this* deployment
([`attestation.md`](../design/attestation.md) §3.3).

- **Derive, do not store.** Compute the address of every attestation actor you talk to
  (`header(id, slot)`) from constants of your own configuration: the attestation program's account,
  the instance id, the chain id. Hold nothing mutable that names the program.
- **The rule (VS.3) applies to you.** A consumer that keeps the attestation program's `ProgramId` in
  mutable state lets whoever controls that pointer substitute a program that attests to anything
  ([`attestation.md`](../design/attestation.md) §7, last list).
- **Check the reply's sender.** Accept an `Attestation` only from the `header(id, slot)` you sent
  `Attest` to, derived as above (§6.1 of the design note). Measured: an attestation-shaped reply from
  another program's actor is refused (exit 220), and `header(id, slot)` of another deployment of the
  same image holds no record (exit 213) ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, refusal rows 6
  and 40).

How the reference consumer does it: a **pool** is the tuple (attestation program's account,
instance id, chain id, token, recipient). Nothing stores a pool; every actor of a pool is a PDA of
the pool's hash, so a claim made under another program, instance, chain or token lands in another
pool ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "The public consumer path";
[`lez/guests/consumer/src/main.rs`](../../lez/guests/consumer/src/main.rs)).

## 3. The request

A request names the instance and the chain id the caller is bound to, and facts of **one execution
block** with their Merkle-Patricia proofs ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "The
interface"; [`attestation.md`](../design/attestation.md) §5.1):

- **accounts** (nonce, balance, storage root, code hash), with **storage slots** under them, from
  `eth_getProof`'s `accountProof` and `storageProof`; inclusion and exclusion (an absent slot is
  zero, an address without an account is attested as such);
- **event logs** by transaction index and position in the receipt, with the receipt's proof in the
  receipts trie. Receipts of EIP-2718 types 0 to 4 with status 1 only; pre-Byzantium and failed
  receipts are refused. `exec::locate_log` converts JSON-RPC's block-wide `logIndex`;
- **accumulator leaves**: a leaf of a SHA-256 accumulator of depth 32 whose root is one of the
  request's storage slots (inner node `SHA-256(0x01 ‖ left ‖ right)`, empty leaf zero, leaf index
  below 2^32);
- **conditions** on each fact (`=, ≠, <, ≤, >, ≥` over 256-bit words, against a constant) and
  **relations** between two facts of the same request; several facts under one record in one call
  ([`attestation.md`](../design/attestation.md) §5.3);
- optionally **EIP-2935 history hops** from the record's block to an older one, up to 8,191 blocks
  back per hop. The verifier takes the older block's number from the decoded header, never from the
  caller; hops chain, though only one hop has been measured through the program; blocks before
  Prague cannot be reached; every hop needs archive `eth_getProof` at the record's block
  ([`attestation.md`](../design/attestation.md) §5.4; [`lez/RESULTS.md`](../../lez/RESULTS.md) §5,
  "What is not built").

The handler checks, in order: that its actor holds a record (213) and is the `header(id, slot)` the
record belongs to (201); that the request names the record's instance (211) and chain id (140);
each hop; each proof against the state or receipts root; each condition and relation; each leaf
against the slot it names. It changes no state ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5).

Proof data travels in the transaction. A log needs its whole receipt as the trie leaf: the block's
largest receipt (306 logs) made a 58,812-byte transaction and cost 1,618,872 cycles; a receipt several
times larger would not fit the 5M execution target (measured, in-process harness;
[`lez/RESULTS.md`](../../lez/RESULTS.md) §5, row 6 and "What is not built").

## 4. The two paths in detail

### 4.1 Public consumers call (F.7)

Send `Attest { request }` synchronously to `header(id, slot)`; act in your reply handler after the
sender check of §2. LEZ v0.4 runs synchronous messages depth-first in one transaction, so the check
and the action succeed or fail together ([`attestation.md`](../design/attestation.md) §6.1).

Measured on mainnet block 26,384,851 (finalized 10 November 2026), in-process harness, RISC Zero's
accelerated Keccak in the program ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5,
`results/stage4-attest.md`, `results/stage4-attest-consumer.md`):

| Transaction | Execution gas | Of the 5M target | Storage gas (tx bytes) |
|---|---:|---:|---:|
| `Attest` as the transaction's root: an account | 167,882 | 3% | 4,424 |
| a storage slot, with a condition | 274,842 | 5% | 7,338 |
| an ERC-20 `Transfer` log (receipt of 7 logs) | 96,584 | 2% | 2,623 |
| the same, receipt of 306 logs | 1,618,872 | 32% | 58,812 |
| one history hop (16 blocks back), then a slot | 498,204 | 10% | 13,578 |
| four facts in one call, with a relation | 453,182 | 9% | 12,309 |
| a consumer actor calls `Attest` for a storage slot and keeps the reply | 384,776 | 8% | 7,507 |
| the same for a log | 163,704 | 3% | 2,792 |
| the reference consumer's claim of a `Transfer` (four handlers) | 183,538 | 4% | 2,768 |
| the same claim again, refused by the nullifier (223) | 14,835 | 0% | 2,768 |

Without the accelerated Keccak the same attestations cost 0.34M to 2.1M cycles, and 8.2M for the
306-log receipt ([`attestation.md`](../design/attestation.md) §13, P.1). Sent as a transaction's
root with nobody to reply to, `Attest` emits the identifiers as an event (`FRT-ATT1`)
([`lez/RESULTS.md`](../../lez/RESULTS.md) §5).

### 4.2 Private consumers read (F.8)

Every message from a private execution to a public actor is published in plaintext on v0.4: its
target, its bytes and the sending program ([`docs/design/lez-v0.4-findings.md`](../design/lez-v0.4-findings.md)
§6, cited in [`attestation.md`](../design/attestation.md) §6.2). So a private consumer must **not**
call `Attest`: the test `a_private_consumer_calling_attest_publishes_its_request`
([`lez/harness/tests/attest_private.rs`](../../lez/harness/tests/attest_private.rs)) finds the
token, the sender, the amount and the statement identifier in the transaction's bytes (2,225-byte
request, 301-byte attestation, in the clear).

Instead:

1. In a privately executed actor, keep the request and its proofs in private state and send `Read`
   to `header(id, slot)`.
2. On the `StateReply`, run `attest::evaluate` on the record and your own rule, inside your own
   execution. Keep only the fixed-size result (the reference consumer keeps 120 bytes: pool id,
   statement identifier, block, sender, amount) and send nothing else.
3. At settlement LEZ re-runs the public handler and refuses the transaction if the reply differs
   from what the prover predicted; a write-once record keeps the prediction valid, which is why you
   read `header(id, slot)` and never `instance(id)`, which every update changes.

What the transaction then shows (measured, in-process harness, dev mode; all 8,201 bytes searched;
[`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "The private path";
`results/stage4-attest-private-public-outputs.txt`): the one-byte `Read`, the record in the reply,
the fee transfer, your program's id and an alias of your account. Absent: the token, sender,
recipient, amount, statement identifier, pool id, every proof node, your account id and its keys.
Test: `a_private_consumer_attests_a_transfer_and_only_the_read_is_public`.

**Read the newest record.** A read shows which record was read. Read the newest one, which many
consumers read, and reach older blocks by hops inside your proof; the read then says only that some
transaction of your program acted after that record. Proving early and submitting later would date
the fact, so prove at submission time ([`attestation.md`](../design/attestation.md) §6.2, §6.5).
The measured fixture reads the record of the fact's own block, not the newest one
([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "What is not built").

**Cost** (measured): LEZ charged 421,641 execution gas (its flat `PRIVATE_VERIFY_GAS` of 409,764 plus
11,877 cycles for the public `Read`) and its flat 224,063 storage gas. Your prover proves your two
transitions: 380,577 user cycles with software Keccak (34,337 for the call, 346,240 for the reply
that verifies the fact), or 137,064 with the accelerated Keccak (executor only; whether LEZ's
privacy circuit resolves the Keccak assumption was not tried). One real proof, on one NVIDIA L4:
16.1 s for `execute_and_prove`, peak host memory 0.76 GiB, GPU memory 11,295 MiB, a 227,170-byte
receipt, settled with the receipt verified ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5;
[`gpu-prover/RESULTS.md`](../../gpu-prover/RESULTS.md), run `gpu-l4-attest-2`). **Not measured:**
this proof on a CPU or a user's laptop, and the 306-log case (ten segments).

**A fact that does not verify cannot be proven**: a forged receipt, a wrong recipient, a pool for
another chain, a proof for another block and a slot without a record end your second transition
with a code (110, 130, 140, 110, 222), and LEZ's prover makes no receipt for a non-zero exit, so no
transaction exists (test `a_private_attestation_that_does_not_verify_cannot_be_proven`).

**Independence (R.11).** Records never change and reads write nothing, so private transactions in
one block never conflict; LEZ admits at most four private transactions per block
([`attestation.md`](../design/attestation.md) §6.4).

### 4.3 Under a Gloas record

Since Gloas (EIP-7732) a record binds the block hash only. Put the block's RLP header in the
request: `Attest` checks that it hashes to the recorded hash and reads the number and roots from it.
Without it the program exits 215 (`HEADER_REQUIRED`); a header that does not hash to the record
gives 120 (`HeaderHashMismatch`). The header costs 20,663 cycles and 691 bytes per request.
Measured on Sepolia block 12,105,936: a storage slot 278,253 cycles, an ERC-20 `Transfer` 96,713, an
account 169,611 ([`attestation.md`](../design/attestation.md) §4.7; [`lez/RESULTS.md`](../../lez/RESULTS.md)
§6). **Not built:** the private path under a Gloas record ([`attestation.md`](../design/attestation.md)
§15).

## 5. Statement identifiers (F.6)

Each fact has a domain-separated SHA-256 identifier over fixed-width fields. It does not depend on
the instance, so two instances that record the same block give the same identifier
([`attestation.md`](../design/attestation.md) §5.2):

| Fact | Identifier over | One per |
|---|---|---|
| Log | chain id, block number, transaction index, the log's position in its receipt, the log's hash | event |
| Storage | chain id, address, slot, value | value: a balance that changes and changes back gets the same identifier |
| Account | chain id, address, its four fields | value |
| Block-bound variant (kinds `0x11`, `0x12`) | the storage or account fields plus the block number | value at a pinned block (not returned by the program; derive it from the reply) |
| Accumulator leaf | `"ferrata/fact/acc-leaf/v1"`, chain id, contract, slot, leaf index, leaf | leaf, under every later root |

**Which to key on.** A once-per-event consumer (a mint, a credit) keys on a log or an accumulator
leaf. A storage identifier suits "this value holds", not "this happened once". Never key on a
root's identifier: one deposit lies under every later root, so that keying would allow one claim per
root (bridge test: the same deposit claimed again under a second root, `root(A, 8)`, is refused, exit 250;
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2).

Status (native tests): the log, storage and account identifiers equal an independent Python
computation; through the program, those returned for the real mainnet block equal a derivation from
the JSON-RPC fields alone (`harness/src/attest.rs`); accumulator-leaf identifiers are covered by the
shared vectors [`vectors/acc-leaf.json`](../../vectors/acc-leaf.json) (14 leaves, 7 refusals, 4
identifier variants) ([`attestation.md`](../design/attestation.md) §5.1–§5.2).

## 6. The Recommended Consumer Pattern (S.4)

RFP-021 S.4 asks for a nullifier keyed on the statement identifier. RFP-021 F.8 forbids new
correlation signals. A nullifier that is a function of the identifier alone can be computed by
anyone from public Ethereum data and matches each private action to its deposit. The pattern
therefore has **seven rules: the RFP's four and three added**
([`attestation.md`](../design/attestation.md) §7).

**The RFP's four:**

1. **Enforce uniqueness with a nullifier keyed on the statement identifier** (§5), in the form rule
   6 gives it. Make the nullifier an account (a PDA) that exists once spent; a second claim finds it
   and is refused.
2. **Verify the chain binding**: the identifier's chain id and the instance (in the design, its root
   id) are the ones you expect (§7 below).
3. **Handle each typed error code** (§8). A refused attestation consumes nothing and can be retried.
4. **Refuse the action when an attestation cannot be produced.** Never fall back to an unverified
   input: no "trusted relayer" branch, no admin override, no cached value.

**The three added:**

5. **The attested fact carries a binding commitment to a secret the depositor chose**, over
   fixed-width fields, so it has exactly one opening.
6. **The nullifier is a hash of that secret and the statement identifier.** It is keyed on the
   identifier, as S.4 and RFP-022 F.3 ask, and is the same on the public and the private path: a
   public claim reveals the secret, a private claim proves knowledge of it. One deposit cannot be
   claimed once on each path.
7. **Exactly one kind of fact is accepted as the deposit.** Other facts from the same receipt (for
   example an ERC-20 `Transfer` beside the vault's own event) are at most predicate inputs.

**Two further requirements:**

- check the reply's sender and pin the program's header account (§2);
- in a successor consumer, refuse any nullifier in the predecessor's set, and freeze the predecessor
  before the successor opens (§9).

### 6.1 Where each rule is built

| Rule | Reference consumer (`guests/consumer`) | Bridge program (`guests/bridge`) |
|---|---|---|
| 1 nullifier on the identifier | built: the claim actor `claim(pool, N)`, `N = H(pool id ‖ statement identifier)` | built: `N = H("ferrata/nf/v1" ‖ s ‖ fid)`, nullifier accounts keyed by `(evmChainId, vault, N)` for the whole program |
| 2 chain binding | built: pool derivation; reply must carry the pool's instance and chain id (226) | built: `configId` binds the EVM chain id, the vault and the attestation root id; `Init` refuses unless the vault's slot 2 holds it (246) |
| 3 codes | through the transaction's outcome (§8) | the same |
| 4 refuse without an attestation | by construction: a failed attestation reverts the transaction | the same |
| 5 commitment to a secret | **not possible**: a plain `Transfer` commits to no secret | built: `C = H("ferrata/C/v1" ‖ s ‖ kind ‖ dest)` in the deposit leaf |
| 6 nullifier from the secret | **not possible** for the same reason | built (rule 1's `N`) |
| 7 one kind of fact | built: exactly one `Transfer` of the pool's token to its recipient (227) | built: accumulator leaves of the vault's deposit tree only |

Sources: [`lez/RESULTS.md`](../../lez/RESULTS.md) §5 ("The public consumer path", "Decisions" item
7); [`docs/design/bridge.md`](../design/bridge.md) §3, §5.3; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md)
§2–§3. The reference consumer therefore has a **public claim** (credits the transfer's Ethereum
sender, whoever submits) and a **private attestation** (publishes nothing), but **no private claim**:
its nullifier is computable from public data, so a private claim publishing it would name its
deposit. A private claim needs a deposit event that carries a commitment, which is what the bridge's
vault provides ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "Decisions" item 7).
[`crates/ferrata-spectests/tests/nullifier_illustration.rs`](../../crates/ferrata-spectests/tests/nullifier_illustration.rs)
illustrates the linkage of an identifier-only nullifier; it runs no `ferrata-core` code.

## 7. Chain binding (F.5)

- The chain id is in every statement identifier and every instance id
  ([`attestation.md`](../design/attestation.md) §3.4, §5.2), and every request names the chain id the
  caller expects. A request for another chain id is refused with 140; a request bound to another
  instance, or sent to another instance's record, with 211 ([`lez/RESULTS.md`](../../lez/RESULTS.md)
  §5, refusal rows 1–4).
- Measured with two instances of the same checkpoint and committee, one configured with mainnet's
  chain id and one with Sepolia's: they are different instances with records of their own, and a
  request bound to one is refused at the other's record. A pool configured for another chain id is
  refused the same way (row 36). **Not built:** the test with one contract at the same address on
  mainnet and on a testnet ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5).
- The program cannot check that a chain id belongs to a `genesis_validators_root`, because nothing a
  light client verifies commits to the execution chain id. You pin the instance id; the off-chain
  `verify_checkpoint` compares the two (**not built**) ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5,
  "Decisions" item 1).
- An update meant for another chain's instance runs to `Finish` and is refused there with 40
  (`SignatureInvalid`): the signing domain includes `genesis_validators_root`
  ([`attestation.md`](../design/attestation.md) §3.4, §4.7).

## 8. Error codes (U.5)

### 8.1 How a code reaches you

On LEZ v0.4 a non-zero exit of any handler reverts the whole transaction (LEZ's
`check_exit_code`), so **your handler never runs after a failed attestation**: there is no path on
which a consumer could act without one, the nullifier stays unspent and the claim can be sent again.
The code is the transaction's outcome, which the submitting client reads
([`lez/RESULTS.md`](../../lez/RESULTS.md) §5, "Decisions" item 4). Rule 3 is therefore met by the
client: it maps the code to a message and to retry or give up. A private consumer's refusal makes no
receipt and no transaction; the client sees the code from its own prover (§4.2). The planned
module's dry run will report the same codes off chain (U.2, **not built**,
[`attestation.md`](../design/attestation.md) §8).

### 8.2 Blocks

Each code has one owner, and a test fails if two owners share one
([`lez/harness/tests/exit_codes.rs`](../../lez/harness/tests/exit_codes.rs);
`CODE_BLOCKS` in [`lez/wire/src/lib.rs`](../../lez/wire/src/lib.rs)):

| Block | Owner | Defined in |
|---|---|---|
| 1–79 (1–61 in use) | `ferrata-core`, light client | [`crates/ferrata-core/src/error.rs`](../../crates/ferrata-core/src/error.rs) |
| 80–99 (80–84) | the step API of the public job | `StepError` in [`crates/ferrata-core/src/steps.rs`](../../crates/ferrata-core/src/steps.rs) |
| 100–199 (100–163) | `ferrata-core`, execution layer and attestations | `error.rs` |
| 200–219 (200–215) | the attestation program | `code` in [`lez/wire/src/lib.rs`](../../lez/wire/src/lib.rs) |
| 220–239 (220–229) | the reference consumer | `consumer::code` in `lez/wire/src/lib.rs` |
| 230–239 (all ten) | `ferrata_holdings`, the worked example of §12: in the consumers' block, unused by the reference consumer | `code` in [`lez/holdings-wire/src/lib.rs`](../../lez/holdings-wire/src/lib.rs) |
| 240–255 | the bridge program | `code` in [`lez/bridge-wire/src/lib.rs`](../../lez/bridge-wire/src/lib.rs) |

The program's own codes moved from 100–112 to 200–212; the refusal table of
[`attestation.md`](../design/attestation.md) §4.4 still shows the earlier numbers (for example 107
`JobExists` is now 207 `JOB_EXISTS`, 102 `WrongSender` now 202) ([`attestation.md`](../design/attestation.md)
§5.5). [`lez/RESULTS.md`](../../lez/RESULTS.md) §5 shows current numbers.

### 8.3 The codes a consumer meets, and what to do

The RFP's failure modes (U.5) map to codes as follows. "Retry" means the same action may succeed
with corrected input; nothing was consumed.

| RFP failure mode | Codes | Meaning | Client should |
|---|---|---|---|
| header not finalised | 213 `NO_RECORD`; 209 `BAD_STATE` | no record at that `header(id, slot)` (not yet written, or an actor that is not a record) | wait for the updater, or use the newest record |
| inclusion proof invalid | 110 `ProofNodeMismatch`, 111 `ProofIncomplete`, 112 `ProofTrailingNodes`, 113 `ProofNodeInvalid`; 122 `ReceiptNotIncluded`, 123 `ReceiptUnsupported`, 124 `ReceiptFailed`, 125 `LogIndexOutOfRange` | the proof is not of this record's block, or not of this key, or the receipt is absent, failed or pre-Byzantium | refetch proofs at the record's block number; a failed receipt never attests |
| predicate not satisfied | 130 `PredicateFailed` | a condition or relation does not hold | refuse the action (rule 4) |
| malformed input | 1 `Malformed`, 100 `RlpNonCanonical`, 101 `RlpMalformed`, 131 `QueryInvalid`, 200 `BAD_MESSAGE`, 214 `BAD_QUERY` | undecodable message, an empty query, a condition on a field the fact lacks, a leaf whose root is not a storage fact of the request | fix the request |
| chain-ID mismatch | 140 `ChainMismatch`; 211 `WRONG_INSTANCE`; consumer 226 `BINDING_MISMATCH` | request or pool bound to another chain or instance | do not retry: configuration error |
| Gloas header | 215 `HEADER_REQUIRED`; 120 `HeaderHashMismatch`; 121 `HeaderUnsupported` | the record binds a hash; header missing, wrong, or of an unknown layout | add the right RLP header |
| history hop | 150 `HistoryOutOfWindow`, 151 `HistoryHashMismatch`, 152 `HistoryContractMissing` | older block outside the 8,191-block window, wrong header, no EIP-2935 contract | hop from a nearer record; chain hops |
| accumulator leaf | 160–163 | path not 32 siblings, index ≥ 2^32, empty leaf, path does not reach the root | rebuild the path against the attested root |
| sync-committee signature invalid | 40 `SignatureInvalid`, 41 `SignatureMalformed`, 43 `SigningKeyIdentity`, 20 `ParticipationLow`, 51 `BelowSupermajority` | met by updaters, not consumers: a record exists only after these pass | — |
| committee handoff not verified | 27, 28, 29, 45 `CommitteeInvalid`, 60 `CommitteeUnknown`; step API 83, 84 | updaters only | — |
| checkpoint stale or outside the weak-subjectivity period | 10, 11, 12 at `Init`; **the bound itself is not built** ([`attestation.md`](../design/attestation.md) §4.6) | — | rely on `monitor` and replacement (§9) |
| already consumed | consumer 223 `ALREADY_CLAIMED`; bridge 250 `ALREADY_CLAIMED` | the nullifier exists | stop |
| wrong claimant actor | consumer 229 `NULLIFIER_MISMATCH` | the claim was sent to a claim actor the statement does not name | send it to `claim(pool, N)` |

Full lists with one-line meanings: the doc comments of the files in §8.2. The reference consumer's
own reply checks (226, 227, 228) cannot be reached through the program, which never produces such
a reply; [`lez/harness/tests/attest_native.rs`](../../lez/harness/tests/attest_native.rs) runs them on
attestations built by hand ([`lez/RESULTS.md`](../../lez/RESULTS.md) §5).

**Errors must not identify a deposit.** For a privacy-preserving consumer (RFP-022 Privacy #5), a
private claim that fails produces no transaction at all (§4.2); a public refusal shows only what the
public request already showed. The bridge's refusals are bare exit codes (240–255), which carry no leaf index
([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2–§3).

## 9. Versions, successors and migration (VS.3)

The verification logic cannot be swapped: no consensus or inclusion check is reached through a
program id held in mutable state ([`attestation.md`](../design/attestation.md) §3.3). Change means a
new deployment that consumers choose to move to.

| Change | What happens | What a consumer does |
|---|---|---|
| A fork **in the schedule fixed at `Init`** | the instance crosses it in place; the store upgrades its layout at the fork (measured on Sepolia through Gloas, periods 1378–1409) | nothing; a new `ferrata-core` release is needed if the fork changes containers or header layouts, and consumers that run the core in their own proofs rebuild with it |
| A fork **scheduled after `Init`** | a successor instance under the same root id, with three invariants checked by `Init` (immutable fields copied, checkpoint = parent's latest record, inside the bound) | nothing: it accepts the successor under the root id (**not built**: no successors or root id yet) |
| An instance past its weak-subjectivity bound, or stranded by a period without Ethereum finality | replacement from a fresh checkpoint under a **new root id**: a new trust decision | decide to accept it after checking the checkpoint (`verify_checkpoint`, not built); move with the successor recipe below |
| A new program version (new image) | a new deployment at a new header account | the successor recipe below |

Sources: [`attestation.md`](../design/attestation.md) §3.5, §3.6, §4.1, §4.7, §15;
[`lez/RESULTS.md`](../../lez/RESULTS.md) §6.

**Successor recipe** (design, [`attestation.md`](../design/attestation.md) §7; not built):
1. freeze the predecessor consumer's claims;
2. deploy the successor consumer pinned to the new header account or instance;
3. make the successor refuse every nullifier in the predecessor's set; how it checks that set is not
   specified beyond this rule;
4. open the successor.

**Why step 3 matters (derived).** The reference consumer's pool includes the instance id, and its
nullifier is `H(pool id ‖ statement identifier)`; a pool bound to a new instance is a new pool, so
its nullifiers differ and, without step 3, every old deposit could be claimed once more. The
bridge's nullifiers do not include the instance (`N = H(s ‖ fid)`, accounts keyed by
`(evmChainId, vault, N)`), so a new attestation instance under the same bridge program keeps them
([`docs/design/bridge.md`](../design/bridge.md) §3, §5.3).

## 10. Bounding what a forged record can cost you (VS.4)

A captured sync committee (two thirds of 512 keys) can sign a header that was never on the
canonical chain, and your consumer would act on facts under it. The design bounds the loss, not the
attack's cost, because a bribe has no protocol floor: size what your caps let through before a
freeze to what you can afford to lose, and wire `monitor`'s alarm (**not built**) to your freeze
authority ([`attestation.md`](../design/attestation.md) §11.2–§11.3). The probabilities and the
bridge's numbers are in [`docs/trust.md`](../trust.md).

## 11. What is not built

From [`attestation.md`](../design/attestation.md) §15 and [`lez/RESULTS.md`](../../lez/RESULTS.md) §5
("What is not built"):

- the second U.6 reference consumer (native ETH to a contract); rules 5–7 and a private claim in a
  reference consumer (the bridge has them);
- the F.8 test (two deposits whose private claims must be indistinguishable);
- successors, root id, weak-subjectivity bound; migration tooling;
- the private path under a Gloas record; a chain of several hops through the program; accumulator
  leaves on real (deployed-vault) data through the program;
- a real proof of a private attestation on a CPU or a user's machine (one exists on a cloud GPU);
- the Logos module, CLI, dry run, IDL and `verify_checkpoint` (milestone A3);
- an attestation on a running sequencer: the attestation program ran on LEZ's own sequencer only at
  an image before `Attest` ([`lez/RESULTS.md`](../../lez/RESULTS.md) §9); the bridge's `Init`
  and `ImportRoot`, which call `Attest`, ran on a live sequencer on a stand-in beacon chain
  ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §11).

## 12. Worked example, step by step: `ferrata_holdings`

A second consumer, written against the public interface only, to show the attestation is a
primitive and not the bridge's: a LEZ program that accepts **"`holder` held at least `minimum` of
the ERC-20 `token` at a finalized Ethereum block"**. It is a storage-slot statement, where the
reference consumer's is a log. Everything below is in four files:
[`lez/holdings-wire/src/lib.rs`](../../lez/holdings-wire/src/lib.rs) (messages, state, the
acceptance check), [`lez/guests/holdings/src/main.rs`](../../lez/guests/holdings/src/main.rs) (the
program, 66 lines), [`lez/harness/src/holdings.rs`](../../lez/harness/src/holdings.rs) (the
client's side) and [`lez/harness/tests/holdings.rs`](../../lez/harness/tests/holdings.rs) (the
cases). It uses nothing of the bridge or the reference consumer; of Ferrata it uses
`ferrata-lez-wire`'s `header_actor`, `Message::Attest`, `attest::Request` and `attest::Attestation`,
and `ferrata_core::keccak::keccak256`.

**Step 1. Write the statement as facts of one block.** An ERC-20 balance is an entry of the token's
`mapping(address => uint256)`; Solidity stores entry `holder` of a mapping declared at slot `p` at
`keccak256(pad32(holder) ‖ pad32(p))`. WETH9 declares `balanceOf` at slot 3. So the statement is two
facts: the token's account (whose storage root the slot is proven under) and that slot, with the
condition `value ≥ minimum` (`Gate::request`). The test checks the key against the fixture
(`mapping_key(holder, 3)` is the slot `eth_getProof` was asked for).

**Step 2. Pin what you trust, in a value you never store.** A `Gate` is the attestation program's
account, the instance id, the chain id, the token, the mapping's slot and the minimum. Its hash
names the program's only actor kind, `holding(gate, holder)` (a PDA of the gate id and the holder),
so a check under another program, instance, chain, token or minimum lands in another actor (§2).

**Step 3. Build the request in your program, not in the caller.** In `Check { gate, slot, holder,
account_proof, storage_proof, header }` the caller names the gate, the record and the holder and
supplies the proofs; the program derives the slot key and the condition from the gate itself, so a
holding written under a gate always means that gate's statement. It then sends
`Attest { request }` synchronously to `header(id, slot)`, derived from the gate (§2), and keeps
`Pending { gate, holder, record }`.

**Step 4. Accept the reply only from the record you asked, and check its shape.** In the reply
handler: the sender must be `record` (233 otherwise); the reply must decode as an `Attestation`
(234); its instance and chain id must be the gate's (235, 236); it must hold exactly the two facts
asked, the token's account then the holder's slot, at the record's own block (237); the balance must
be at least the minimum (238; the attestation program already checked the condition, and
`Gate::accept` checks it again so that one function states what the program accepts). Then write
a `Holding`: balance, beacon slot, block number and hash, and the storage fact's statement
identifier (§5). A later check may replace it with a newer block only (239).

**Step 5. Fetch the proofs off chain.** Pick a record: the beacon slot of a finalized header the
instance recorded (the `FRT-FIN1` event, or `header(id, slot)`'s state), and its execution block
number. Ask any Ethereum node `eth_getProof(token, [key], block)`: its `accountProof` and
`storageProof[0].proof` are the two proofs. Under a Gloas record, also send the block's RLP header
(§4.3). The test uses the committed response of 10 November 2026
(`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/proof_weth_26384851.json`):
Uniswap v2's USDC/WETH pair `0xb4e16d…c9dc` held 0xf22ca9b4cc5d0f3d61 wei (4,467.33 WETH) of WETH at
block 26,384,851, the value the committed `eth_call balanceOf` gives too.

**Step 6. Send one transaction.** Its root is `Check` at `holding(gate, holder)`, with the record
actor in its actor list and PDA evidence for the holding actor (`Holdings::check_with`). LEZ runs
three handlers depth-first: `Check`, the attestation program's `Attest`, the reply. A refusal of
any of them reverts the transaction (§8.1), so the holding is written only on a proven statement.

**Step 7. Test the refusals with real data.** One chain, every case
(`RISC0_DEV_MODE=1 cargo test -p harness --locked --test holdings -- --nocapture`; the table is
[`lez/results/holdings.md`](../../lez/results/holdings.md)); each refusal leaves the holding as it
was, and wherever a record exists the native `attest::evaluate` on it gives the same code:

| Case | Refused by | Exit |
|---|---|---|
| a gate for Sepolia (chain id 11155111) against mainnet's records | the attestation program | 140 `ChainMismatch` |
| an unfinalized block: the update's attested header (slot 15,624,544), which no record holds | the attestation program | 213 `NO_RECORD` |
| an unknown block: no record at slot 15,624,481 | the attestation program | 213 `NO_RECORD` |
| the right proofs against another block's record (the bootstrap's, slot 15,622,144) | the attestation program | 110 `ProofNodeMismatch` |
| a bad proof: one byte of the storage proof's leaf flipped | the attestation program | 110 `ProofNodeMismatch` |
| a bad proof: one byte of the account proof's root node flipped | the attestation program | 110 `ProofNodeMismatch` |
| a mismatched value: a minimum of the balance plus 1 wei | the attestation program | 130 `PredicateFailed` |
| a mismatched holder: the pair's proofs claimed for another address | the attestation program | 110 `ProofNodeMismatch` |
| `Check` sent to another holder's actor | this program, before asking | 232 `WRONG_RECEIVER` |
| a second check at the same block | this program, after the attestation | 239 `STALE` |

Accepted: a minimum of 1,000 WETH, and a minimum equal to the balance to the wei. The holding then
holds the balance `eth_call` gives, block 26,384,851, its hash, and the statement identifier
`harness::attest::storage_statement_id` computes from the JSON alone.

**Step 8. Measure.** In-process harness (LEZ v0.4.0-rc1's state machine and fee settlement), on the
build box, 11 November 2026:

| Transaction | Handlers (cycles each) | Execution gas | Of the 5M target | Storage gas (tx bytes) |
|---|---|---:|---:|---:|
| accepted, at least 1,000 WETH | `Check` 111,198 → `Attest` 278,002 → reply 41,910 | 431,110 | 9% | 7,493 |
| refused by the condition (balance + 1 wei) | `Check` 111,198 → `Attest` 253,744 | 364,942 | 7% | 7,493 |
| refused for another chain | `Check` 111,198 → `Attest` 38,360 | 149,558 | 3% | 7,493 |
| refused before asking (wrong actor) | `Check` 36,232 | 36,232 | 1% | 7,493 |

LEZ charges execution gas equal to the cycles it metered; the accepted check's base fee at genesis
base fees is 3,508,824 ([`lez/results/holdings.md`](../../lez/results/holdings.md), every case and
every handler's bytes). `Check` reads the proofs in its 6,966-byte message and writes them again into the
`Attest` it sends (a 14,319-byte journal); refused before it builds the request, it costs 36,232
cycles. The program adds 153,108 cycles to the attestation's 278,002 (derived: 431,110 − 278,002),
36% of the transaction.

**Step 9. Prove.** LEZ v0.4 does not prove a public transaction (a sequencer executes it); what a
prover of this one would prove is its three handler executions. `prove_holdings`
([`lez/harness/src/bin/prove_holdings.rs`](../../lez/harness/src/bin/prove_holdings.rs)) settles the
accepted check, replays it with each handler's input recorded, proves each execution with `r0vm`
3.0.5 on the CPU (`RISC0_DEV_MODE=0`, succinct receipts), verifies each receipt against its
program's image id and compares each journal with the executor's:

| # | Program | Handler | User cycles | Padded cycles (segments) | Proving time | Receipt |
|---|---|---|---:|---:|---:|---|
| 1 | `ferrata_holdings` | `Check` | 111,198 | 262,144 (1) | 21.1 s | succinct, 251,813 bytes |
| 2 | the attestation program | `Attest` | 278,002 | 524,288 (1) | 64.4 s | succinct, 238,923 bytes |
| 3 | `ferrata_holdings` | reply | 41,910 | 131,072 (1) | 13.6 s | succinct, 225,055 bytes |

99.1 s of proving in all; the whole run (settlement, replay, three proofs) took 1 min 43.29 s wall,
2,598 s of CPU (2,616% of one core) and 4.93 GB peak memory (GNU `time -v`), on a Modal CPU container
(32 processors listed, 64 GB, no GPU), 11 November 2026; an earlier run of the same image took
99.3 s
([`lez/results/prove-holdings.log`](../../lez/results/prove-holdings.log)). Image ids of that build:
`ferrata_holdings` `4985868d…cd4f`, the attestation program `d2fb9aec…e600`; a guest's image id
embeds the build's absolute paths, so another checkout gives other ids. To run it:

```sh
cd lez                                     # toolchains and environment: lez/RESULTS.md §8
RISC0_DEV_MODE=1 cargo test -p harness --locked --test holdings -- --nocapture   # writes results/holdings.md
cargo build -p harness --locked --bin prove_holdings
RISC0_DEV_MODE=0 target/debug/prove_holdings > results/prove-holdings.log 2>&1   # --composite: no lift and join
```

**What this example does not do.** It runs on the public path only: the request names the token
and the holder, and the transaction shows them; a private balance check would read the record and
run `attest::evaluate` itself (§4.2), which this program does not. No hops (the record's own block
only), no Gloas record in its tests (`header: None`), no accumulator leaves. It ran in the
in-process harness, not on a live sequencer. It was built and run on v0.4.0-rc1 only:
`lez/scripts/lez-select.sh` switches its manifests to another tag, but
`lez/lockfiles/v0.4.0-rc2.patch` does not cover its guest's lockfile. Not audited.
