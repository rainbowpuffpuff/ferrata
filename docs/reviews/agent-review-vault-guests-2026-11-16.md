# Second independent review of the vault and the guests against the specifications (docs/REVIEW-vault.md, 52 rows; docs/REVIEW-guests.md, 55 rows)

Reviewer: a headless agent (Claude Code, Fable 5.2, max effort) in a separate context, 16 November 2026,
16:00–16:48 CET, on the rows as drafted, before the author's sign-off. Code: `2c8ae862` (every `file:line` below is
at that commit). No reviewed source differs from `89e6d6d`, the tables' commit: `git diff --stat 89e6d6d 2c8ae862 --
vault/src bridge/guests bridge/crates/ferrata-bridge-core/src checkpoint/crates checkpoint/guests lez/guests
lez/bridge-wire/src lez/wire/src` shows only two added test files, and the line numbers of both tables have not
drifted (checked by script for the 100 function names of REVIEW-vault.md, by hand for every row read of
REVIEW-guests.md). Nothing was built or run: every finding is confirmed by reading, where it says so, against the
vault source, the guests and, for LEZ's behaviour, LEZ's own source at `v0.4.0-rc1` and `v0.4.0-rc3`. The review
changed nothing in the repository. It is an agent's review, not an audit.

What was done with it is at the end ("Disposition").

## 1. Verdict

No Critical, High or Medium finding. Three Low: F-2, a nested `Transfer` moves a holder's LEZ balance; F-9, the
vault still takes deposits after the verifier stop, and those can never come back; F-1, the admin can stall
releases and refunds through the release caps, at once and without lapse. No row's code differs from its row.
Do not sign as written: vault `submitCandidate`, `_checkpointDigest`, `_releaseJournalDigest` (wrong test counts);
vault `deposit`, `setTokenCap…`, `constructor` and guests `transfer` (Deviation cell incomplete). See section 3.

## 2. Findings (most severe first)

Counts: Critical 0, High 0, Medium 0, Low 3, Info 6. The ids are in order of discovery; the sections are in order of
severity (F-2 theft from a holder who calls a hostile program; F-9 permanent loss after a rare event; F-1 a stall
by a trusted role).

### F-2 (Low) `Transfer` accepts a nested call from any program in a transaction the holder signed; the row does not say so

- Row: guests `transfer`, `credit` (B:806, 862). Deviation cell: "amounts in multiples of `lezUnit`; not gated by
  freezes or caps".
- Code: `lez/guests/bridge/src/main.rs:838-858`: at the holder's own actor the only test is
  `if !input.is_authorized { return Err(NOT_AUTHORIZED) }`; `input.from` is not required to be `None` (it is for
  `Burn`, B:765, and for role messages, B:943).
- What is wrong (as a statement of the row, not of the plan): on LEZ v0.4 `is_authorized` of a public account is
  per transaction, not per call: `lee/state_machine/core/src/execution_state.rs:1028`
  (`is_authorized: public_context.authorized_accounts.contains(&account_id)`) and `authorize()` (`:1431-1466`), and
  `authorized_accounts` must equal the signers (`transaction_message.rs:82`). So any program reached in a
  transaction a holder signed can send `Transfer { to: anyone }` to the holder's own bridge actor and it is
  authorised.
- Scenario: holder V signs a transaction whose root call goes to a program M (a third-party LEZ program; the
  transaction's declared actors include `(V, bridge)`, `holder(V)`, `token(cid, t)`, `holder(A)`). M sends the sync
  message `Transfer { cid, token: t, amount: V's whole balance, to: A }` to `(V, bridge)`. B:839 does not match
  (`from` is M's actor), B:848 passes (V signed), V's own state has no holding, so B:856 forwards to `holder(V)`
  with the PDA seed; there B:840 matches, the balance is debited and `token(cid, t)` credits `holder(A)`. V's
  wrapped tokens are A's. The same nested call with `Burn` is refused (B:793;
  `bridge_burn.rs:226-241` tests it).
- Confirmed: read only (the handler and LEZ's `authorize`, rc1 and rc3). No harness test sends a nested `Transfer`.
  CONFIRMED as behaviour; whether it is acceptable is the owner's decision.
- It is deliberate: `lez/BRIDGE.md` §2, message 26: "a call the holder authorised (top-level, private root, or
  nested)"; §5 says in passing "LEZ marks a signer's account authorised in every call of the transaction"; and
  LEZ's own native token debits on the same test (`native_token.rs:92`). But "what a program the
  holder calls can do with the holder's bridge balance" is the kind of statement the Deviation column exists for,
  and neither the row nor `BRIDGE.md` §5 "Transfers" states the consequence.
- Smallest fix: row text (section 3). In a later program: accept `Transfer` at the own actor only with
  `input.from.is_none()` (top-level or private root), as `Burn` does, or add an allowance.
- A test that would show it (a sketch, NOT written to the tree and NOT run): in
  `lez/harness/tests/bridge_burn.rs` the block at lines 226-241 builds
  `Script::default().send_sync(holder, &BridgeMessage::burn(cid_a, &n2))`, submits it at the `scripted` program's
  PDA actor in a transaction the payer signs, and expects `NOT_AUTHORIZED`. The same block with
  `&BridgeMessage::Transfer { config_id: cid_a, token: ETH, amount: DENOMINATIONS[0], to: <another account> }`,
  the actor list extended by `holder_actor(program, payer)`, `token_actor(program, &cid_a, &ETH)` and
  `holder_actor(program, <the other account>)` (with that PDA's admission evidence), should by my reading be
  accepted (`expect(&r, None)`) and move `DENOMINATIONS[0]` from the payer's balance to the other account's.
- New or known: the behaviour is documented (BRIDGE.md message 26); its consequence and its absence from the row
  are new. Not in the security review of 11 November or the agent review of 12 November.

### F-9 (Low) Deposits are still accepted after the verifier stop, and such a deposit can be neither released nor refunded

- Rows: vault `deposit`, `depositETH`, `_checkDeposit` (FV:676, 694, 1190), `_stopped` (FV:1527).
- Code: `_checkDeposit` (`FerrataVault.sol:1190-1208`) refuses a deposit when `retired || finalSunset`, under a
  deposit freeze, for an unknown or delisted token, and so on; it does not ask `_stopped()`. `_stopped()` is used
  only at FV:717 (`refund`) and FV:805 (`finalizeAttested`).
- What is wrong: the stop is never undone (v2 §6: "once stopped, nothing more is accepted by this vault"), and
  after it nothing is released (the wrapped verifier reverts), nothing is finalised and nothing is refunded (D13,
  FV:717). The vault is then a one-way sink, and it stays open: every deposit made after the stop is escrow that
  no path can return. The `_stopped` row's Deviation cell says "none".
- Scenario: RISC Zero's `estop` is tripped at time S (a proof of exploit; the owner is renounced, so nobody can
  undo it). The freeze authority is slow or its key is lost. At S + 1 day a user who has not seen the news calls
  `depositETH{value: 1.002 ether}(1, C, 0)`: it succeeds (FV:694-698), the leaf is appended, `escrowed[ETH]` grows.
  LEZ may still import the root and mint, but the wrapped tokens can never be released; `refund` reverts
  `VerifierStopped`. The ether is gone for good. Deposits stay open until someone freezes them (lapses after
  `maxFreeze`), the freeze authority retires the vault, or anyone does after the keyless period (30 days plus
  `delta` and the turn term after the last submission, `retireOpensAt`: no candidate can be submitted after a
  stop, so that clock does run out).
- Confirmed: read only (the three uses of `_stopped()`; `EmergencyStop.t.sol` deposits only before the stop and
  asserts nothing about a deposit after it). CONFIRMED.
- Smallest fix: in the next vault, `if (_stopped()) revert VerifierStopped();` in `_checkDeposit` (one static
  call per deposit), or open `retire()` to anyone once the stop is tripped (no cost on the deposit path). For the
  deployed vault: the runbook and the monitor retire the vault (or freeze deposits) the moment the stop trips,
  the wallet and the CLI refuse to deposit when `verifier.paused()`, and the `deposit` row and README "Known
  limits" say that deposits after a stop are lost.
- No guard elsewhere: a search of `cli/`, `monitor/` and `docs/guides/` for `paused`, `estop` and
  `VerifierStopped` finds only the error-code text (`cli/src/codes.rs:95`) and a list of revert names
  (`docs/guides/user-guide.md:239`): the CLI does not check `paused()` before a deposit and the monitor does not
  alert on a stop.
- New or known: "a tripped stop strands all escrow" is a known limit (D8, README, `docs/trust.md` line 341). That the vault
  keeps accepting deposits afterwards is stated nowhere I searched (README, bridge.md, trust.md, the two earlier
  reviews) and no test covers it. New.

### F-1 (Low) The release-cap floor does not guarantee that a release can pass: zero is accepted, and nothing relates the floor or a cap to the denominations

- Rows: vault `setTokenCap`, `setGlobalCap`, `checkReleaseFloor`, …; `registerToken` (`_checkRegisteredFloor`);
  `constructor`. The same structure on LEZ: guests row `role_message` (`SetCap`, `set_cap`, B:1168) and
  `Controls::check` (BW:562).
- Code: `vault/src/FerrataRegistry.sol:385-397` (`checkReleaseFloor`), `:419-428`, `:198-242`;
  `vault/src/FerrataVault.sol:607-614` (constructor), `:1467-1470`, `:1481-1488`.
- What is wrong: D13's "the admin cannot set a cap below an immutable floor" exists so that the admin cannot stall
  exits (bridge.md §14: the admin "cannot … cut a cap below the floor or unfreeze"; README: "an admin who closes
  deposits stalls no exit"), but the constructor accepts a floor of (0, 0) and neither it nor
  `registerToken`/`setTokenCap` checks that a release bucket can ever hold one release of the largest denomination.
- Scenario 1 (all six scripts in `vault/script/` that build a `Deployment`, `DeployVault.s.sol:232` among them,
  set the floor to (0, 0)): the admin
  calls `setGlobalCap(Direction.Release, 0, 0)`. `checkReleaseFloor(0, 0, 1, 1, 0, 0)` passes (0 < 0 is false);
  `_updateBucket` sets capacity 0 and level 0. From then on `_tryConsume(_globalReleaseCap, v)` is false for every
  v > 0: every `release` and every `refund` reverts `GlobalReleaseCapExceeded`, with no lapse (a freeze of the
  freeze authority lapses after `maxFreeze`; this does not). The admin cannot freeze
  (`Admin::test_roles_adminCannotFreezeOrUnfreeze`) but can do this.
- Scenario 2 (a non-zero floor): floor capacity 1e9 value units; a token whose largest denomination is worth 1e10
  value units is registered with a release capacity worth exactly 1e9. Registration passes; a release of the
  largest denomination can never pass (`amount > level`, and level ≤ capacity). The repository's own test registers
  exactly this and expects success: `vault/test/Roles.t.sol:383-389` (denominations 100e18, 1000e18, 10_000e18;
  release cap 1e21 base units).
- Confirmed: read only (the code paths above; the existing test shows the registration is accepted). CONFIRMED.
- Smallest fix: the vault is immutable, so (a) in the deployment check require a floor whose capacity is at least
  the value of the largest denomination that will be registered and whose refill is non-zero, and say in the rows
  and in README "Known limits" that with a smaller floor the admin can stop that denomination's releases and
  refunds, with floor (0, 0) all of them; (b) in the next vault, refuse a zero floor in the constructor and in
  `registerToken`/`setTokenCap(Release)` require `capacity >= largest denomination`.
- How it differs from the known limit: README "Known limits" already says a compromised admin can, after 14 days
  and a public proposal, install its own freeze authority, which can then renew freezes. So "the admin can stall
  exits" is not new as an outcome. What F-1 adds: through the caps it is immediate (one call, no timelock, no
  public proposal), it does not lapse (no `maxFreeze`), and the freeze authority cannot undo it.
- New or known: the script's "release floor 0" default is known (I-2). That a floor below a denomination protects
  nothing, and that the rows, README and bridge.md §14 state the guarantee without this condition, is new.

### F-3 (Info) Three "Pinned by" cells give test counts the suite does not have

- Rows: vault `submitCandidate` ("`JournalBinding::test_checkpointJournal_submit_*` (all 17 words)"),
  `_checkpointDigest` ("`JournalBinding` (34 tests, each word at submission and finalisation)"),
  `_releaseJournalDigest` ("`JournalBinding::test_releaseJournal_binds*` (all 11 words)").
- At 2c8ae862 `vault/test/JournalBinding.t.sol` has 48 tests: 16 `test_checkpointJournal_submit_binds*`, 16
  `test_checkpointJournal_finalize_binds*` (32, not 34), 10 `test_releaseJournal_binds*`, 6 `test_attack_*`.
  The word without a `binds*` test in each journal is the domain tag (`CHECKPOINT_DOMAIN`, `RELEASE_DOMAIN`). The
  tags are pinned all the same, but elsewhere: every mock-verifier test accepts only a digest that
  `VaultTestBase.sol:323, 348` computes from `sha256("ferrata/vault/checkpoint/v3")` and
  `sha256("ferrata/vault/release/v2")`, and the real seals bind them.
- Not drift: `git show 89e6d6d:vault/test/JournalBinding.t.sol` has the same 32, 10 and 48, so the cells were
  already wrong at the tables' commit.
- Confirmed: `grep -c` on the suite at both commits. CONFIRMED. New.

### F-4 (Info) The mutant lists of the three proof-checking rows have no mutant on the proof check or the image id

- Rows: vault `release` (V101–V117, V210–V216), `submitCandidate`, `finalize`.
- None of the 254 entries of `MUTANTS` removes or weakens a `verifier.verify(...)` call, swaps
  `releaseImageId`/`checkpointImageId`, or removes a `nonReentrant` (searched the list for `verify(`, `ImageId`,
  `nonReentrant`: only V8, the balance check, touches an external call). "0–253, all killed" therefore says
  nothing about the proof checks.
- They are pinned by tests: the mock verifier accepts only an `(imageId, journalDigest)` pair a test registered
  (`test/mocks/MockVerifiers.sol`), `Release::test_rejectingVerifier_rejectsCheckpointsAndReleases`,
  `Checkpoint::test_submitCandidate_needsAValidProof`, `test_finalize_needsItsOwnProof`,
  `ModeB::test_modeB_aProofOfTheOtherRulesProgramIsRefused`, the flipped-seal tests of `real/`, and the reentrancy
  tests of `Release` and `PullPayment`. `refund` has no reentrancy test; it does not need the guard (the leaf is
  marked before the payment).
- Read only. Suggestion: add the five mutants (three `verify` removals, the image-id swap in `release`,
  `nonReentrant` off `release`) so the rows can cite them. New.

### F-5 (Info) LD-2 of the security review can be closed: LEZ lists a privacy-preserving transaction whose public part failed in `reverted`

- Rows: guests `apply_message`/`extend`, `burn`, `link_block` (the agent review of 12 November, §2.8, left this as
  "not checked here either").
- `lee/state_machine/src/validated_state_diff/mod.rs:419-453` (rc1; `:427` in rc3): a chargeable failure of the
  public part with a fee fallback returns `ExecutionCharge { revert: Some(..) }`; without a fallback it is an error
  and the transaction is not in a block. `lez/chain_state/src/apply.rs:555, 699` return `charge.revert.is_some()`
  and `:207-209, 330-332` push the position into `reverted`. So the `PrivateToPublic` `Burn` delivery of such a
  transaction is skipped by the checkpoint (`CC/rule.rs:131-134`), as `checkpoint/README.md:162` assumes.
- Read only, in rc1 and rc3. This is a positive result, not a defect.

### F-6 (Info) Deployment parameters the constructor row does not name as left to the deployment

- Row: vault `constructor`. Its cell names the stop's owner, `lezConfigId`, the image ids and `turnSeconds` (I-1).
  Also unchecked by the constructor, and each changes what a role can do: `nMin = tMin = 1` is accepted (I-1 says
  so; D6's "outside testnets `nMin >= 3`, `tMin >= 2`" is enforced nowhere); the release-cap floor may be (0, 0)
  (F-1); `maxFreeze` has no ceiling (`FV:589` refuses only 0; v2 §7's "a lost key cannot lock the vault forever"
  holds only for a sensible value); `tMin <= nMin` is not required; the watchers are not checked against the roles.
- Read only. Known in part (I-1, I-2).

### F-8 (Info) The invariant campaign's reach is logged, not asserted

- Rows: every vault row that cites an `I:` pin (`release`, `refund`, `claimOwed`, `_finalize`, `revokeFrom`, the
  fee rows, …).
- `vault/foundry.toml:58-64` runs the campaign with `fail_on_revert = false`; the handler discards calls with
  `vm.assume` and wraps several in `try/catch`; `VaultInvariants.t.sol:264-288` (`afterInvariant`) only prints
  the counts of deposits, releases, refunds, finalisations, revocations, claims, burns and fee changes. Nothing
  fails if a campaign completes with zero releases or zero refunds, in which case
  `usedNullifierIsNeverPaidAgain`, `refundedLeafIsNeverPaidAgain` and the escrow and balance equalities hold
  trivially. The reach is in the run's printed table (which I did not re-run), not in the test.
- The handler itself is real: `release` (`VaultHandler.sol:616-651`) calls the vault without `try` and compares
  the recipient's, the fee recipient's and the prover's amounts with its own fee model; `refund` (`:563-597`)
  decides from its own records whether the vault should pay and counts both a wrong payment and a wrong refusal;
  `replayNullifier` and `replayRefund` count a second payment. So the pins are sound when the campaign reaches
  them.
- Read only. Smallest fix: a fixed-seed test (or a check on the campaign's totals) that fails unless at least one
  release, refund, revocation and fee change completed. New.

### F-7 (Info) Smaller observations (no change to any verdict)

- `_creditReleaseFees` / `_finalize` rows: "a root finalised twice keeps its first non-revoked checkpoint (and so
  that checkpoint's time and release delay)" also means it keeps that checkpoint's **prover**: when later
  checkpoints carry the same burn root (no burn in between), the first prover is credited the checkpoint fee of
  every release against it and the later provers nothing. The cell could say "time, release delay and prover".
- `FV:1083-1084` says "proposing the current roles withdraws it"; `FerrataRegistry.proposeRoles` (FR:298-309) always
  stores a proposal with a new `effectiveAt`, so a proposal stays pending and `applyRoles` later re-installs the
  same roles and emits the events. The row ("a new proposal replaces the pending one") is right; the source
  comment is not. (Fees do withdraw: FR:113.)
- A watcher does not need to renew 12-hour pauses to hold off keyless retirement: one `pauseCheckpoints()` moves
  `checkpointsFrozenUntil` to now + 12 h and `retireOpensAt` counts the whole `delta + _keylessSpan` (30 days or
  more) from there (`FV:1008-1010`), so one call every 30 days suffices. README "Known limits" has the limit
  ("a rogue watcher can delay it"); the mechanism is cheaper than "renewing pauses" suggests.
- A token with a transfer hook to the sender can re-enter the vault's unguarded functions during `deposit`
  (`retire`, `finalizeAttested`, `submitCandidate`, `applyFees`): the deposit then lands after `retired` or after
  a sunset finalisation was set in the same transaction. I found no loss: the leaf is at or above the boundary
  and refundable, the fee schedule cannot change in between (`_checkDeposit` already applied a due one), and
  `deposit`/`refund`/`release`/`claimOwed` are guarded. `refund`'s comment "depositRoot … no longer moves because
  the vault must be retired" has this one exception, for hook tokens only.
- The front-running of `submitCandidate` for the prover credit is known (README "Still open"); the
  `submitCandidate` row says only "the sender is recorded as the candidate's prover (M-2)".

## 3. Rows whose text is wrong or incomplete

Vault (docs/REVIEW-vault.md):

| Row | The words | What they should say |
|---|---|---|
| `submitCandidate` | "`JournalBinding::test_checkpointJournal_submit_*` (all 17 words)" | "(16 tests: every word but the domain tag, which the test base's digest pins)" |
| `_checkpointDigest` | "`JournalBinding` (34 tests, each word at submission and finalisation)" | "`JournalBinding` (32 of its 48 tests: 16 words at submission and at finalisation; the domain tag through the test base's digest)" |
| `_releaseJournalDigest` | "`JournalBinding::test_releaseJournal_binds*` (all 11 words)" | "(10 tests: every word but the domain tag, which the test base's digest pins)" |
| `deposit` (and `_checkDeposit`) | "refused after a finalised sunset checkpoint (L-3) as after `retire()`" | add: "not refused after the verifier stop: a deposit made then can be neither released nor refunded (F-9); only `retire()` or a deposit freeze closes deposits" |
| `_stopped` | Deviation "none" | "deposits are not stopped (v2 §6 says nothing more is accepted): F-9" |
| `setTokenCap`, `setGlobalCap`, `checkReleaseFloor`, … | "the floor covers the release buckets only, in value units, saturating; no default "10% of escrow" cap" | add: "a floor of (0, 0) is accepted (no floor: the admin can then set a release bucket to zero, which stops releases and refunds with no lapse), and neither the floor nor a cap is checked against the denominations (a bucket at the floor can be smaller than one release); the deployment must set the floor above the largest denomination's value" |
| `constructor` | "…renouncing the stop's owner and checking `lezConfigId`, the image ids and `turnSeconds` are left to the deployment (I-1)" | add: "so are the committee floors (`nMin = tMin = 1` is accepted), a non-zero release-cap floor and a bounded `maxFreeze`" |
| `_finalize` (optional) | "(and so that checkpoint's time and release delay)" | "(and so that checkpoint's time, release delay and prover)" |
| `submitCandidate` (optional) | "the sender is recorded as the candidate's prover (M-2)" | add: "(a copied pending submission takes the credit: README, still open)" |

Guests (docs/REVIEW-guests.md):

| Row | The words | What they should say |
|---|---|---|
| `transfer`, `credit` | "amounts in multiples of `lezUnit`; not gated by freezes or caps" | add: "accepted at the holder's own actor from any call of a transaction the holder signed, nested calls from other programs included (LEZ's authorisation is per transaction; BRIDGE.md message 26), so a program a holder calls can move the holder's balance; `Burn` is top-level only" |
| `up_to_tip` (optional) | "rotations are applied to the parent's key set before the blocks; the parent's own set is checked against the floors only when no rotation replaces it" | add two choices the checkpoint README's table has and no row names: "at least one block up to `T` (`CC/rule.rs:261`: no pure key-rotation checkpoint, which is also why the pinned programs cannot make the vault's no-op checkpoint)" and "a parent state with `postingTimeframe = 0` is not refused (left to the deployment check)" |
| `role_message` (optional) | "role messages are top-level calls at the role's own actor…" | add for `SetCap`: "never below the configuration's floor, which may be zero (as on the vault, F-1)" |

Functions that no row names (script over every `function` and `modifier` of the two vault files, 112 names, and
every `fn` of the guests I read): vault `_requireRegistered` (FV:1531), `_depositRoot` and `_leafCount` (FR:595,
601; the `refundAmount` row describes them without naming them); bridge program `set_cap` (B:1168, the admin's cap
setter with the floor check: F-1), and the state helpers `ready_config`, `token_state`, `load_scope`, `ask_time`,
`from_account`, `not_before`; attestation program `instance_state`, `job_state`, `open_job`. None hides behaviour
a row misstates; `_requireRegistered` and `set_cap` deserve a mention in their callers' rows.

Not wrong, for the record: the guest row for the rule (A) guest already names the "416-byte" comment; the vault
row for `revokeFrom` already names the test that shows the 1,001 bound rests on the programs.

## 4. Coverage

"In depth": every line of the function read against the cited specification text, the deviation cell compared,
and attack scenarios tried on paper. "Read": every line read once and the row compared, no scenario work. Order
as in the tables.

Specification texts read in full: bridge-spec-v2.md; bridge-spec-v3-changes.md D3 to D13; `lez/BRIDGE.md` §2
(the actor and message tables, lines 51-148), §4 and §5; the checkpoint README's "What the program checks",
"Transaction shapes" and "Choices" tables; vault/README.md "Where I interpreted" and "Known limits". Read in part:
bridge.md §13 and §14, attestation.md §3.5. Not read: the rest of bridge.md and attestation.md, BRIDGE.md §1, §3,
§6. Comparing BRIDGE §2's 28 messages ("to" and "from" columns) and §5 with the handlers, and the README's eight
steps and its choices table with `CC/rule.rs`, I found no difference beyond those in section 3. So the
attestation-program and consumer rows, whose specification I did not read, are "read", not "in depth".

What I did about the "Pinned by" cells (the same for every row, so not repeated in the table):
- every test and file name in both tables exists (script over the backticked names: 166 names of the vault
  table, 132 of the guests table; the only misses were file names, which exist as files);
- every test count in both tables was recounted: all right except the three of F-3;
- all 254 mutant descriptions were read and every vault row's `Vn` list is about that row's function (no mismatch;
  F-4 is what the list lacks);
- the 14 invariants and the 19 Halmos function names exist; the handler's `release`, `refund`, `replayNullifier`
  and `replayRefund` and the bodies of the seven fund and fee invariants were read (F-8);
- test bodies opened and found to assert what their row uses them for: `Release::test_release_waitsForReleaseDelay`
  (both sides of the second), `test_release_cannotExceedEscrow` (forced ETH is not escrow),
  `Refund::test_refund_noSecondAllowsBothRefundAndRevocation`,
  `ReleaseDelay::test_releaseDelay_equalToRevokeWindowOpensAfterTheLastRevocableSecond`,
  `PullPayment::test_refundToADepositorThatCannotReceive_isCredited`, `Roles::test_capFloor_*` (three),
  `lez/harness/tests/bridge_burn.rs:205-262`, `bridge_roles.rs:150-190`. No other test body was opened: for the
  rest, "pinned" rests on the name, the mutant list and the earlier agent review's mutant runs.

### Vault (52 rows): all reached

| Row | Coverage |
|---|---|
| `deposit` | in depth (F-9) |
| `depositETH` | in depth |
| `_checkDeposit` | in depth |
| `_recordDeposit` | in depth |
| `_leafHash` | in depth |
| `_insertLeaf`, `_hashNode`, the empty root | in depth |
| `receive`, `fallback` | read |
| `recordBeacon`, `_recordBeacon` | in depth |
| `submitCandidate` | in depth |
| `_checkCommittee` | in depth |
| `_checkpointDigest` | in depth |
| `finalize` | in depth |
| `finalizeAttested` | in depth |
| `_liveCandidate`, `_epoch` | in depth |
| `_wait` | in depth |
| `_checkMonotone` | in depth |
| `_finalize` | in depth |
| `_emitCandidateSubmitted`, `_emitCheckpointFinalized` | read |
| `cancelCandidate` | in depth |
| `pauseCheckpoints` | in depth |
| `setCheckpointsFrozen` | in depth |
| `revokeFrom` | in depth |
| `raiseDelta` | read |
| `release` | in depth |
| `_checkRelease` | in depth |
| `_releaseJournalDigest` | in depth |
| `_creditReleaseFees` | in depth |
| `_releaseDelayFor`, `releaseDelayFor` | in depth |
| `_payOrCredit`, `_credit`, `_trySendEth` | in depth |
| `claimOwed` | in depth |
| `refund` | in depth |
| `refundAmount`, `_inboundFeesOfLeaf` | in depth |
| `_refundsOpenAt` | in depth |
| `retire` | in depth |
| `retireOpensAt` | in depth |
| `transferAdmin`, `acceptAdmin` | in depth |
| `proposeRoles`, `applyRoles` | in depth |
| the five freeze setters, `_freezeUntil`, `_frozen` | read |
| `_checkAdmin`, `_checkFreezeAuthority`, `_isWatcher`, modifiers | read |
| `_stopped` | in depth (F-9) |
| `registerToken` | in depth |
| `_checkLezUnit`, `_checkDenominations`, `_checkRelayFees`, `_checkRegisteredFloor` | in depth (compared with LEZ's `TokenEntry::check`, BW:215-236: the same bounds, so no entry the vault accepts is refused on LEZ) |
| `_initFees`, `_publishEntry`, `_publishSchedule`, `_feeWords`, `_store` | in depth (preimages and slots compared field by field with bridge-wire's `TokenEntry::hash`, `FeeSchedule::hash`, `registry_slot`, `fee_slot`, BW:91-99, 196-213, 273-288, by reading: they agree, and every schedule the vault publishes passes `FeeSchedule::check`'s notice rule; not recomputed on data) |
| `announceFees`, `_storeFees` | in depth |
| `applyFees`, `_feesInForce` | in depth |
| the views and public getters | read |
| `setDepositsEnabled` | read |
| `setReleaseDelay`, `checkReleaseDelay` | in depth |
| `setTokenCap`, `setGlobalCap`, `checkReleaseFloor`, `_toValue`, `_updateBucket` | in depth (F-1) |
| `_consume…`, `_tryConsume`, `_available`, `_valueUnits`, buckets, `currentLevel`, `initBucket` | in depth |
| `_limits` | read |
| `constructor` | in depth |

### Guests (55 rows)

| Row | Coverage |
|---|---|
| `release` guest `main` | in depth |
| `verify_release` | in depth |
| `burn_commitment`, `burn_leaf`, `burn_nullifier`, `burn_id` | in depth (preimage layouts read, not recomputed) |
| `verify_path`, `root_from_path`, `ordered` | in depth |
| `ReleaseInput::decode`, `ReleaseJournal::encode` | in depth |
| `claim` guest `main` | not reached (a measurement guest) |
| `verify_claim`, `commitment`, `deposit_leaf`, `deposit_id`, `nullifier` (bridge core) | in depth |
| `gasdrop` guest and its functions | not reached |
| `ClaimInput::decode`, `claim_journal` | not reached |
| `checkpoint` guest `main` | in depth |
| `checkpoint_b` guest `main` | in depth |
| `segment` guest `main`, `run_segment` | in depth |
| the three `Input::decode` | read (CC/input.rs:225-341: every count bounded by the bytes left, `finish()` after the last field) |
| `up_to_tip` | in depth |
| `link_block`, `parse_block`, `hash_parsed_block` | read (the parser against LEZ's `Block` type was not re-derived) |
| `apply_message`, `extend`, `BridgeMessage::decode` | in depth |
| `apply_config_change`, `check_key_set`, `threshold` | in depth |
| `run_checkpoint` | in depth |
| `ElidedInscription::check`, `verify_strict`, `tx_hash` | in depth |
| `run_checkpoint_b` | in depth |
| `SignedAttestation::verify`, the messages, `key_set_hash` | in depth |
| `Frontier::*`, `burn_leaf` | in depth |
| `State::digest`, `Config::config_id`, `allow_single_key` | in depth |
| `CheckpointJournal::encode`, `SegmentJournal::*` | in depth |
| attestation `main`, `handle` | read |
| `init`, `check_config`, `period_window` | read |
| `open`, `update_and_store`, `update_window` | read |
| `job_open`, `miller`, `adopt`, `finish`, `complete` | read |
| `apply`, `apply_job` | read |
| `verify_all`, `apply_verified` | read |
| `write_record` | read |
| `attest`, `attest::evaluate` (P:435; W:684) | read (the wrapper only; the core's `attest_batch`, `history_hop` and the proofs are REVIEW-core's) |
| `read` | read |
| `succeed`, `check_successor`, `spawn` | read |
| bridge `main`, `handle` | in depth |
| `init`, `check_pairing`, `config_reply` | in depth |
| `import_root`, `check_import`, `write_root` | in depth |
| `register`, `check_register`, `WriteToken` | in depth |
| `import_fees`, `check_fees`, `extend_fees` | in depth |
| `claim`, `verify_claim` (BW), `issue_message` | in depth |
| `claim_privately`, `claim_reply` | in depth |
| `issue`, `fees_for`, `admit` | in depth |
| `spend`, `Mint` | in depth |
| `burn` | in depth |
| `transfer`, `credit` | in depth (F-2) |
| `role_message` | in depth |
| `cosigned` | read |
| `attest_retired`, `check_retired` | in depth (slot 37, byte 11 checked against the vault's layout by hand) |
| `apply_roles`, `tick`, `apply_time`, `token_time_reply`, `record_time` | read (B:1049-1244, `Gate`/`Bucket` BW:340-470; `record_time` BW:521 itself not read) |
| `Echo`, `Read` | read |
| reference consumer `ReadRecord`, `ReadPadded`, `CallAttest` | read |
| consumer `Claim` | read |
| consumer `AttestPrivately` | read |
| consumer `ClaimDeposit`, `ClaimDepositPrivately`, `Spend`, `CreditHolder`, `Credit` | read (`claim_deposit`'s body, C:127-150, not read) |
| holdings `Check` | not reached |

## 5. What I did not check, and why

- **Nothing was run.** No forge, cargo, Halmos or mutant run: reading settled every doubt I had, and the review
  had a two-hour limit. So "the suite is green at 2c8ae862", the Halmos results, the path counts, the
  100% coverage and "254 of 254 killed" (EVIDENCE row 12; its log is not in the tree, `docs/mutants-run.log` is
  the core's 107) are taken from the documents, not re-measured.
- **The light-client core** (`ferrata-core`, `steps.rs`: BLS, SSZ, Merkle-Patricia inclusion and exclusion proofs)
  and `attest::evaluate` in `lez/wire`: REVIEW-core's rows, out of this brief, and every inbound mint rests on
  them. In particular the bridge's fee import trusts an attested zero for "schedule not yet in force"
  (`check_register`, `check_fees`), that is, an exclusion proof.
- **Byte-for-byte encodings**: I compared the registry entry and fee-schedule preimages and slots of the vault
  with bridge-wire's by reading (they agree) but recomputed nothing on data; the preimage layouts of the bridge
  core and the block parser against LEZ's types were read only. They are pinned by vectors and by the live round
  trips.
- **LEZ's semantics beyond what I looked up**: I confirmed in the upstream source that authorisation is per
  transaction, that a fallback-settled transaction is listed in `reverted`, that the program API has no way to
  catch a failed synchronous send, and that `authorized_accounts` must be the signers. I did not check how
  asynchronous messages and promotions are delivered (the bridge refuses every sender it does not expect, so I
  expect no effect), nor private-account aliasing (the security review did).
- **The rule (A) analysis** (docs/design/finality-rule, the simulation): the row for `_wait` rests on it; I checked
  that the vault computes what D4 states, not that D4 is right.
- **The deployment script, the monitor, the CLI and the wallet-side rules** (newest root, gap limits): outside
  the rows.
- **Guest rows marked "not reached"** above: the gas drop, the claim measurement guest and its input decoder, the
  holdings consumer. None of them holds or moves bridge funds.

Unsure of:
- F-2's level: it is LEZ's own model and the plan says "or nested"; I rate it Low because it moves a holder's
  funds and the row is silent. It was not run (the sketch in F-2 is my reading of the harness).
- F-9's level: the loss is permanent and the fix is one line, but it needs the stop, which only a proof of
  exploit trips; Low on likelihood, not on impact.
- F-1: whether a deployment check for the floor already exists outside the repository (inside it,
  `DeployVault.s.sol` sets the floor to zero).
- The private-account side of LEZ's authorisation (aliases, how a role's private actor is recognised at
  `config(cid)`): I read the public path only.

## Disposition (16 November 2026)

- Rows corrected as section 3 says: vault `submitCandidate`, `_checkpointDigest`, `_releaseJournalDigest` (the test
  counts), `deposit` and `_stopped` (F-9), `setTokenCap`… (F-1), `constructor` (F-6); guests `transfer` (F-2). The
  rows marked optional in section 3 are unchanged.
- F-9 and F-1 are in `vault/README.md` "Known limits"; F-2's consequence is in `lez/BRIDGE.md` §5 "Transfers".
- Not fixed: the vault is immutable and the programs with image ids are not changed before the milestones. F-9's
  guard in `_checkDeposit`, F-1's refusal of a zero floor and F-2's top-level-only `Transfer` (or an allowance) are
  for the next vault and the next bridge program. Until then F-9 and F-1 rest on the deployment's runbook and
  parameters, as the README says; the `ferrata` tool and the monitor do not yet check the stop before a deposit.
- F-4 (five mutants on the proof checks), F-8 (asserting the invariant campaign's reach) and F-7's source comment
  are open.
