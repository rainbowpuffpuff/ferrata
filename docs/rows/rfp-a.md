# Stream rfp-A: light-client verification security through the program (RFP-021)

Proposals for the integrator; this stream does not edit `requirements-status.md`, `traceability.md`, `EVIDENCE.md`
or the README's status lines. Branch `rfp-lc-security`, from `f13faee2`; its commits are listed in the last
section.

Where everything ran: the build box (a Modal CPU container), 21 November 2026, LEZ v0.4.0-rc1's state machine and
fee settlement in process (the harness), RISC Zero dev mode (the guests and LEZ's privacy-preserving circuit
executed, not proven). None of these requirements is about proving; the private transactions of the same kinds were
proven for real and settled before (EVIDENCE rows 5, 73, 123). No program, guest or image id changed; no deployment
is affected.

Re-run (from `lez/`, `RISC0_HOME` set as `lez/RESULTS.md` §6 says):

```sh
RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc --test security_privacy -- --test-threads 4
```

Result of the run at `fa0cbccb` (and of `u5` again at `471e5cde`): `security_lc` 5 of 5, `security_privacy` 2 of
2; in the same run the red team's `redteam_lc` 11 of 11 and `redteam_attest` 11 of 11 (they share the test kit these
tests extended: `header_with_payload`, `update_over`, `fabricated_bootstrap`, with the old functions' outputs
unchanged). At the branch's last commit: `cargo fmt --all -- --check`, `cargo clippy -p ferrata-spectests
--all-targets --locked -- -D warnings` and the whole `ferrata-spectests` suite pass, and F.8 again. Every table is in
[`lez/results/lc-security/`](../../lez/results/lc-security/README.md).

## Summary

| Row | Today | Proposed | Shown by |
|---|---|---|---|
| F.2 | in part | **shown** | `f2-finalised-only.md`, `vs2-forgeries.md` §3–4 |
| F.4 | in part | **shown** | `f4-comparisons.md` |
| F.8 | in part | **shown** | `f8-full-diff.md` (the full-diff test) |
| U.5 | in part | **shown** | `u5-failure-modes.md` |
| R.3 | not yet | **shown** | `r3-rotations.md` |
| R.11 | not yet | **shown** | `r11-one-block.md` |
| VS.1 | in part | **shown** | `vs1-fabricated-chain.md` |
| VS.2 | in part | **shown** | `vs2-forgeries.md` |
| R.6 | in part | in part (audit) | `crates/ferrata-spectests/HELIOS-DIFFERENTIAL.md`; see below |

## F.2 — accept only finalised headers; refuse all others

| Clause (RFP-021 Functionality 2) | Shown by (through the program, `security_lc::vs2_and_f2_forgeries_are_refused_through_the_program`) |
|---|---|
| verify that a supplied header is finalised | a record is written only by a verified finality update, for its finalized header (the update applies; the record of the finalized header exists) |
| reject any header that is not | an update without finality: 50 `NotFinalized`; a finality branch with a flipped sibling: 26; one node short: 1; the finalized header's slot changed under its branch: 26; a finalized header with a tampered execution state root or receipts root (beacon header intact): 25 and 25; a valid inclusion proof against the attested, never-finalised header: 213 `NO_RECORD` at its slot and 110 at the finalized header's record (natively, against a record of that header, the same request verifies, 2 statements) |

Note on 213: `Attest` at a slot that holds no record is sent with the record actor's PDA evidence (as the red team's
AT-RO-03), so that the program runs and decides (213); without the evidence LEZ refuses the transaction before the
program runs, with no code ("Program violated execution rules"). Either way nothing is attested.

Proposed status row: `| F.2 | shown | A header enters only by a verified finality update; every other way of offering a header is refused through the program: no finality (50), a forged or short finality branch (26, 1), a header changed under its branch (26), tampered execution roots (25), a proof against an attested, never-finalised header (213, 110). | NEW-1; lez/results/lc-security/f2-finalised-only.md |`

## F.4 — predicates, at least equality and inequality, set by the consumer

| Clause (Functionality 4) | Shown by (`security_lc::f4_every_comparison_from_the_consumer`) |
|---|---|
| evaluate a predicate over the verified state and emit a verified statement | each request is the reference consumer's `CallAttest`: the consumer program calls `Attest` at the real mainnet record of block 26,384,851, keeps the reply, and the reply equals the native evaluation's |
| at least equality and inequality over attested values | all six comparisons `=, ≠, <, ≤, >, ≥`, each against a constant on two facts (WETH's balance, a USDC `Transfer` amount) where it holds (applied) and where it fails (130), 24 cases; and each as a relation between two attested facts of one request, holding and failing, 12 cases: 36 cases, 18 applied, 18 refused with 130 |
| expressible by a consuming program without modifying the module | the predicate is carried in the consumer program's request; the attestation program is the deployed one, unchanged |

Proposed status row: `| F.4 | shown | All six comparisons against a constant and as relations between two attested facts, carried in the consumer program's request through the attestation program: 36 cases, each applied where it holds and refused with 130 where it fails. | NEW-1; lez/results/lc-security/f4-comparisons.md |`

## F.8 — no correlation signal, over full event and state diffs

The test `docs/design/attestation.md` §6.5 specifies, built: `security_privacy::f8_two_deposits_full_diff`. Two
deposits of the same token and amount (0.001 ETH, private-kind) on a synthetic mainnet-preset chain: deposit A in
the recorded block B, deposit D three EIP-2935 hops back. Each is claimed by the reference consumer's
`ClaimDepositPrivately` from the same state against the same record, in the wallet's shape (seven padded actions,
padded notes, fresh randomness).

| Clause (Functionality 8) | Shown by |
|---|---|
| no correlation signal beyond what is public on Ethereum | the two transactions are 32,963 bytes each (about 29,800 bytes differ: 29,794 and 29,778 in two runs, as fresh random bytes sometimes coincide); after taking from A every field that is fresh randomness or the claim's nullifier N (12 groups, each with the same encoded size in both: the private side's aliases, the 7 private actions, the recovery binding, the fee fallback's action and crossing, `Spend`'s N and the claim actor's address and PDA seed), the two messages are byte-identical: the depth of the deposit (0 or 3 hops) and which deposit it is leave no trace |
| public inputs limited to what the consumer chooses to expose | what differs is N (the consumer's own nullifier, spent publicly by design) and fresh randomness; none of the secret, the statement id, the destination account, the vault's address, the receipt proof or any hop header is in either transaction's bytes (13 searches, all absent) |
| not revealing the consuming LEZ account or transaction | the claimant accounts X and Y never appear; their private actions are indistinguishable (same sizes, same commitment root) |
| over full event and state diffs | events: identical (0 each); charge identical (435,096 execution gas, 224,063 storage gas, base fee 5,273,272), so the three hops cost nothing visible; every public account of the state before and after each claim, decoded from the state's encoding: the same two change (the fee inbox, `claim(pool, N)` at N's address) the same way; the whole state encodes to 2,831,693 bytes after either; the record is read, never written |
| the control | the same consumer calling `Attest` (`CallAttest`) instead of reading fails the same comparison (boundary step 0's message: 811 bytes against 3,361) and publishes D's three hop headers and its receipt proof |

What stays visible, as the design says: which record is read, the consumer's program, and that some transaction of
it acted after that record. Dev mode: the proof field (16,538 bytes in dev mode) has the same size in both; a real
receipt is a fixed-size zero-knowledge proof (227,170 bytes, row 5).

Proposed status row: `| F.8 | shown | The full-diff test: two private claims (a deposit in the recorded block, one three hops back) are byte-identical after taking only fresh randomness and the claim's nullifier from one; events, charges and every public account's change identical; no secret, statement id, account, vault, proof or hop header in either; the calling variant fails the same test. Private attestation proven for real and applied on rc3 (row 123). | 5, 73, 123, NEW-2; lez/results/lc-security/f8-full-diff.md |`. README, "Specified but not built": drop "the F.8 test".

## U.5 — clear, actionable errors for each listed failure mode

`security_lc::u5_every_listed_failure_mode_returns_its_code_through_the_program`: 22 cases, every one sent through
the program; each code's name and message are read from the IDL (`idl/ferrata_attestation.idl.json`), which
`ferrata-attest` prints (EVIDENCE row 56).

| Failure mode (Usability 5) | Through the program |
|---|---|
| header not finalised | 50 `NotFinalized`; 26 `FinalityBranchInvalid`; 213 `NO_RECORD` |
| sync-committee signature invalid | 40 `SignatureInvalid` (a committee never handed off); 41 `SignatureMalformed` |
| committee handoff not verified | 27 `NextCommitteeBranchInvalid`; 45 `CommitteeInvalid` (an adopted key replaced); 23 `PeriodSkipped` (no handoff before the next period) — the refusal of a handoff through LEZ the status table asked for |
| checkpoint stale or outside the weak-subjectivity period | 53 `OutsideWeakSubjectivityPeriod` (by `Open` and by `VerifyAll`); 217 `STALE_CHECKPOINT` (a successor from a checkpoint that is not the instance's latest record); and, by the sequencer's clock, LEZ's own refusal "Execution outside of the validity window" of a valid update sent after the end and of an `Init` from a checkpoint whose period has ended (the program declares the window; the same `Init` one millisecond earlier applies) |
| inclusion proof invalid | 110 `ProofNodeMismatch`; 111 `ProofIncomplete`; 112 `ProofTrailingNodes` |
| predicate not satisfied | 130 `PredicateFailed` |
| chain-ID mismatch | 140 `ChainMismatch` |
| malformed input | 1 `Malformed`; 131 `QueryInvalid`; 214 `BAD_QUERY`; 200 `BAD_MESSAGE` |

The one refusal without a program code is LEZ's, by design (the program has no clock; it declares the validity
window and LEZ enforces it); the CLI's dry run shows it before anything is sent, and the IDL's message for 53 says
what to do ("recover from a new bootstrap").

Proposed status row: `| U.5 | shown | Every listed failure mode refused through the program with its code (22 cases, a handoff refused with 27, 45 and 23 among them), each code's name and message from the IDL the CLI prints; a stale checkpoint by the sequencer's clock is refused by LEZ on the window the program declares. | 39, 56, 69, 70, NEW-1; lez/results/lc-security/u5-failure-modes.md |`

## R.3 — valid statements stay valid forever, across committee rotations

`security_lc::r3_records_stay_valid_after_committee_rotations`.

| Clause (Reliability 3) | Shown by |
|---|---|
| a statement over a finalised header remains valid indefinitely | synthetic mainnet-preset instance: two records (the checkpoint's block 500 and block 600) attested (a storage slot with a condition and a `Transfer` log) before any rotation and again ten years after the last update, the instance long past its weak-subjectivity period: applied, event data byte-identical (`Attest` declares no validity window); only advancing the stale instance stops (LEZ's window) |
| subsequent sync-committee rotations never invalidate it | the same two records attested after each of four rotations (committees 1 to 4 current in turn): applied, event data byte-identical, record bytes unchanged; mainnet, real data: the five records of periods 1899–1903 read by the reference consumer (`ReadRecord`) after the four real rotations and ten years later, each the bytes written (10 reads; no execution proofs exist for those blocks, so read, not attested); Sepolia, real data: USDC's `totalSupply_` under the record of period 1409, attested before and after the real handoff and rotation into period 1410, byte-identical |

Proposed status row: `| R.3 | shown | Records attested after four committee rotations and ten years later with byte-identical statements (synthetic chain); five real mainnet records read after four real rotations; a real Sepolia record attested across a real rotation. | NEW-1; lez/results/lc-security/r3-rotations.md |`

## R.11 — statement submissions independent; several private transactions per block

`security_privacy::r11_several_private_reads_in_one_block`.

| Clause (Reliability 11) | Shown by |
|---|---|
| multiple private transactions submit statements in the same block | four private accounts each prove the reference consumer's `AttestPrivately` of one `Transfer` against the same snapshot; the four are settled in one block as LEZ's block application settles one (`chain_state::apply`: one opening fee state, one fee summary, one block id and timestamp, each transaction in order): all four applied, each 422,032 execution gas, 224,063 storage gas, base fee 5,168,760; every nullifier spent and every commitment in the tree, as proven |
| a private transaction computes its resulting state before inclusion | every transaction is complete (its nullifiers, new commitments, its predicted reply from the record) before any is included; between proving and inclusion a later finality update is verified and writes a new record; none is re-proven; the record read is unchanged (reads write nothing); LEZ keeps past commitment roots, so proofs against the snapshot settle |
| (control) | one account's two transactions proven against the same snapshot: the second is dropped, "Nullifier already seen" — independence holds across accounts, not for one account's double spend |

Limits: the block is assembled by the test from the harness's copy of rc1's settlement, as `chain_state::apply`
does; the block's own fee and clock transactions (no user data) are not modelled; no live sequencer ran this.

Proposed status row: `| R.11 | shown | Four private attestations proven against one snapshot, a finality update applied in between, then settled in one block with nothing re-proven; one account's second transaction on the same snapshot dropped. In process (rc1's block settlement), not on a sequencer. | NEW-2; lez/results/lc-security/r11-one-block.md |`

## VS.1 — reject a fabricated chain, tested explicitly

`security_lc::vs1_a_fabricated_chain_is_refused_through_the_program`. The canonical instance: Stage 4's mainnet
instance (the checkpoint of slot 15,622,144, advanced by the real finality update of slot 15,624,480, block
26,384,851). The forger fabricates a self-consistent chain in the store's own period: a checkpoint proving a
committee of 512 keys he holds, a finality update signed by all 512 whose finalized header carries block 26,384,856
with real Merkle-Patricia tries (an account with 10^24 wei; a USDC `Transfer` to the pool's recipient).

| Clause (Verification Security 1) | Shown by |
|---|---|
| the chain is self-consistent | controls: the forger initialises his own instance from his checkpoint (`Init` is permissionless), it verifies his update (the specification's store bootstrapped from his checkpoint agrees), and his record attests the fabricated `Transfer` and balance |
| verification must fail | at the canonical instance: the fabricated update refused 40 `SignatureInvalid` (Open–Miller–Finish, and in one `VerifyAll`); the fabricated proofs refused 213 at the fabricated slot and 110 at the real record; the forger's record asked by a request bound to the canonical instance: 211 `WRONG_INSTANCE`; the reference consumer's claim of the fabricated `Transfer`: 213 and 110; controls: the real `Transfer` attested and claimed |
| exercised explicitly | the test sends the fabricated chain itself, as transactions, through the program |

Proposed status row: `| VS.1 | shown | A self-consistent fabricated chain (a forger's committee, checkpoint, finality update and block with real tries) accepted by the forger's own instance and refused through the program by the canonical one (40), at its records (213, 110), across instances (211) and by the reference consumer (213, 110). | 1, NEW-1; lez/results/lc-security/vs1-fabricated-chain.md |`

## VS.2 — reject the six listed forgeries

`security_lc::vs2_and_f2_forgeries_are_refused_through_the_program`, every case through the program.

| Forgery (Verification Security 2) | Through the program |
|---|---|
| a header signed by a committee never validly handed off | 40 (512 keys of a committee never handed off); 27 (a handoff's next committee replaced); 45 (an adopted key replaced); 23 (the next period with no handoff); 40 (after a handoff, that period signed by another committee) |
| a signature below the participation threshold | 341 of 512: 51 `BelowSupermajority`; 342 of 512: applied (control) |
| a valid inclusion proof against a header that is not finalised | 213 at the attested header's slot; 110 at the finalized header's record |
| a valid finalised header with a tampered state or receipts root | 25 and 25 (execution branch no longer verifies); an account proof re-linked to a tampered state root: 110 |
| a proof replayed against another chain id | 140 `ChainMismatch`; another instance: 211 |
| a predicate over state the proof does not cover | 110 (a storage proof for another slot; an account proof for another address); 130 (a condition on a fourth topic of a three-topic log); 131 (a relation naming a fact the request does not prove) |

Proposed status row: `| VS.2 | shown | All six forgeries refused through the program: never-handed-off committee (40, 27, 45, 23), 341 of 512 signers (51; 342 applies), a proof against a non-finalised header (213, 110), tampered roots (25, 110), another chain (140, 211), a predicate the proof does not cover (110, 130, 131). | NEW-1; lez/results/lc-security/vs2-forgeries.md |`

## R.6 — mature, audited cryptography and light-client code (stays in part)

What this stream added: `helios_oracle.rs::random_mutations_of_real_updates_agree_with_helios`, 2,000 seeded
mutations (one to three distinct bit flips in a uniformly chosen region of the SSZ encoding) of mainnet periods
1899–1903's five real updates, each judged by ferrata's policy store, its spec store and Helios 0.12.0 against the
stores just before that update: 1,987 refused by all three, 13 accepted by all three with the same heads (flips of
the signature slot's low bytes, which the signature does not cover), 0 disagreements
(`crates/ferrata-spectests/results/helios-random-mutations-2026-11-21.txt`; the section "Seeded random mutations"
of `HELIOS-DIFFERENTIAL.md`). A second client: Lodestar's light client (`@lodestar/light-client` 1.43.0,
TypeScript) judged the same 2,000 mutations and the five updates as served (`lodestar/diff.mjs` over the corpus
`export_random_mutations_for_a_second_client` writes): the five accepted with the same heads, 1,987 mutations refused
by all four (ferrata's two stores, Helios, Lodestar), the same 13 accepted with the same heads, 0 disagreements
(`crates/ferrata-spectests/results/lodestar-random-mutations-2026-11-21.txt`; the section "A second client:
Lodestar").

Why it stays in part: the RFP asks for "mature, audited implementations"; BLS is `blst` (audited), but the
light-client logic is Ferrata's own transcription of the specification. Differential testing shows agreement with
other implementations on the inputs tried, nothing more. What only an audit adds: a reading of the code against the
specification by someone who wrote neither (a mistake shared with the specification or with the other clients
agrees with itself; random mutations do not reach inputs that need several coordinated changes); the review of the
program's adaptations that no other client has (the step API split over transactions, the adoption parts, the
compact store and its digests, the weak-subjectivity window by the sequencer's clock, the record and its PDA); and
the review of what the program assumes of LEZ (validity windows, PDA admission, the commitment-root history).

Proposed status row: `| R.6 | in part | BLS is audited blst; the light-client logic is Ferrata's own transcription, checked against the official vectors, Helios (the vectors, 220 recorded updates, 2,000 random mutations of real updates) and Lodestar (the same mutations), 0 disagreements; not audited. | 1, 140, NEW-3; crates/ferrata-spectests/HELIOS-DIFFERENTIAL.md |`

## Proposed EVIDENCE rows

**NEW-1** | **RFP-021's verification security through the attestation program: a fabricated chain, the six forgeries, every listed failure mode, all six comparisons and records across rotations, each sent as the transactions a submitter or a consumer sends** | VS.1: a forger's self-consistent chain (his committee of 512, checkpoint, finality update and block with real tries) accepted by his own instance and refused by the canonical mainnet instance (40), at its records (213, 110), across instances (211) and by the reference consumer (213, 110); VS.2 and F.2: 40, 27, 45, 23, 40 (committee), 51 at 341 of 512 (342 applies), 213/110 (not finalised), 25, 25, 110 (tampered roots), 140, 211 (chain), 110, 110, 130, 131 (uncovered predicate), 50, 26, 1, 26 (finality); U.5: 22 cases over the eight listed modes, each code with the IDL's message (stale: 53, 217, and LEZ's window on `Open` and on `Init`); F.4: 36 cases (`=, ≠, <, ≤, >, ≥` against a constant and between facts, from the consumer's request), 18 applied, 18 refused with 130; R.3: two records attested after each of four rotations and ten years later, byte-identical; five real mainnet records read after four real rotations; a real Sepolia record attested across a real rotation | in-process harness (build box) | none (dev mode) | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc` | [`lez/results/lc-security/`](../lez/results/lc-security/README.md) |

**NEW-2** | **The F.8 full-diff test and several private statements in one block (R.11)** | F.8: two private claims (deposit A in the recorded block, deposit D three hops back), 32,963 bytes each, byte-identical once fresh randomness and the claim's nullifier are taken from one (12 groups, equal sizes); 13 searches find no secret, statement id, account, vault address, receipt-proof node or hop header; events, charge (435,096 execution gas, 224,063 storage gas) and every public account's change identical; the calling variant fails the comparison and publishes the hops; R.11: four private attestations proven against one snapshot, a finality update applied between proving and inclusion, all four settled in one block (422,032 execution gas each), nothing re-proven; one account's second transaction on the same snapshot dropped ("Nullifier already seen") | in-process harness (build box) | none (dev mode; the same transaction kinds proven for real in rows 5, 73, 123) | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_privacy` | [`f8-full-diff.md`](../lez/results/lc-security/f8-full-diff.md), [`r11-one-block.md`](../lez/results/lc-security/r11-one-block.md) |

**NEW-3** | **Random mutations of real updates against two other light clients, Helios (Rust) and Lodestar (TypeScript)** | 2,000 seeded mutations (one to three distinct bit flips in a uniformly chosen region) of mainnet periods 1899–1903's five updates: ferrata's policy and spec stores, Helios 0.12.0 and Lodestar's light client 1.43.0 refuse the same 1,987 and accept the same 13 (signature-slot flips the signature does not cover) with the same finalized and optimistic headers; the five updates as served accepted by all with the same heads; 0 disagreements | build box | none (native) | `cargo test --locked -p ferrata-spectests --test helios_oracle random_mutations -- --nocapture`; Lodestar: `HELIOS-DIFFERENTIAL.md` §"A second client" | [`HELIOS-DIFFERENTIAL.md`](../crates/ferrata-spectests/HELIOS-DIFFERENTIAL.md) |

## Traceability (evidence column), proposed wording

- F.2: replace "A non-finality update sent to the program through LEZ is not among the recorded refusals." with
  "Through the program, every other way of offering a header is refused: no finality (50), a forged or short
  finality branch (26, 1), a header changed under its branch (26), tampered execution roots (25)
  (`lez/results/lc-security/f2-finalised-only.md`)."
- F.4: replace "`≠` and `≤` are in the wire and the core, not in a request run through the program" with "all six
  comparisons and relations run through the program from the consumer's request (36 cases,
  `lez/results/lc-security/f4-comparisons.md`)".
- F.8: replace "The F.8 two-deposit test is not built (...)" with "The F.8 two-deposit full-diff test:
  `harness/tests/security_privacy.rs` (`lez/results/lc-security/f8-full-diff.md`)."
- U.5: replace "it is not among the refusals recorded through LEZ" with "through LEZ: 27, 45 and 23
  (`lez/results/lc-security/u5-failure-modes.md`, every listed mode)".
- R.3: replace "A test reading an old record after later rotations: none yet." with the R.3 row above; tests:
  `harness/tests/security_lc.rs`.
- R.11: replace "Several reads in one block: none yet." with the R.11 row above; tests:
  `harness/tests/security_privacy.rs`.
- VS.1: replace "The fabricated chain itself has not been sent through the program." with "Through the program:
  `lez/results/lc-security/vs1-fabricated-chain.md`."
- VS.2: replace "they are not among the refusals recorded through LEZ" with "through LEZ:
  `lez/results/lc-security/vs2-forgeries.md`".

## Commits on `rfp-lc-security`

| Commit | What |
|---|---|
| `16453ade` | the security tests through the program, first version; the test kit's `header_with_payload`, `update_over`, `fabricated_bootstrap` (old functions delegate, same outputs) |
| `056d5027` | Helios: seeded random mutations of the five real mainnet updates |
| `efcaeeab` | compile fix; F.8 compares events and state diffs with D's N read as A's; mutations never cancel |
| `8ba925aa` | `Attest` at a slot with no record sent with the record actor's PDA evidence (the program decides: 213) |
| `fa0cbccb` | F.8: the fee fallback aligned (its fresh private action and the payer alias's crossing); a failure names the field |
| `dd53d18d` | `HELIOS-DIFFERENTIAL.md`: the random-mutation section and its recorded run |
| `471e5cde` | the results (`lez/results/lc-security/`), U.5's two stale-checkpoint cases |
| `9c102906` | Lodestar as a second client: the shared generator, the corpus export, `lodestar/diff.mjs`, its recorded run |
| the commit adding this file | `rustfmt` of two test files (formatting only) and this file |

Files outside this stream's own (`lez/harness/tests/`, `crates/ferrata-spectests/`, new `lez/results/` files): none
changed. The attestation program, the guests, the IDL and every image id are unchanged.
