# The `ferrata` tool's U.08 refusals: verification failure and insufficient finality (RFP-022 U.08)

**Run:** `RISC0_HOME=<spike>/risc0-home-lez cli/tests/e2e.sh` from the repository's root (the tests of
[`cli/tests/e2e.rs`](../../../cli/tests/e2e.rs)), on a Modal build box (CPU), 23 November 2026, the tree of commit
`da4c9de7` (branch `u8-cases`). **Result:** `test result: ok. 2 passed; 0 failed` in 55.05 s:
the form check on its own
(`names_nothing_catches_every_form`) and the sequence (`deposit_scan_claim_burn_release_and_recovery`)
([`test.log`](test.log), the tests' own output; the build's lines before it are left out and the checkout's path is
written `<repo>`). The same tests run in CI's `roundtrip` job (`.github/workflows/ci.yml`, `cli/tests/e2e.sh`).

**Where it ran:** FerrataVault v5 on a local anvil (chain id 1, never a public network) with the vault tests'
`MockRiscZeroVerifier`; the zone in the LEZ v0.4.0-rc1 in-process harness (LEZ's own state transition with the bridge
guest, RISC Zero dev mode). Proofs are not what U.08 is about; none is real here. Stand-ins as the test names them: the
relayer's imports and `Tick` through the harness's calls, the admin's `SetDepth` signed by the harness's admin role
key, the checkpoint prover and relayer by the mock verifier and anvil's first account.

## How each refusal is checked

- **Refused before anything is signed, by state.** On the zone (claims, burns), `unmoved`: the calls the tool handed to
  the zone (every `Ledger::submit`, applied, refused or not proven), its block height, and the nonces of the fee payer
  and of the wallet's private accounts are as they were, checked first and on their own. The control: the same
  command, once nothing refuses it, sends one transaction and the same marks see it (the plaintext and the private
  claim). On Ethereum (releases), the signer's nonce is unchanged. The deposit refusals are dry runs (`--from`, no
  keystore): nothing can be signed there.
- **Names no deposit or burn, by form** (`names_nothing`): no 40 hex digits in a row; no leaf at all, in any case or
  form, once the program's and the tool's exact general wordings are removed (248's name, the meanings of 245, 248
  and 249, the tool's "no imported root covers this leaf yet"); no deposit, burn, position, index, counter, nullifier,
  commitment or tag followed within two words by a number, nor "#5". The test `names_nothing_catches_every_form` checks
  the tool's messages pass and 31 named forms fail, alone and inside each message: among them the seven the round-1
  check missed ("leaf (5)", "leaf number 5", "leaf no. 5", "leaf at index 5", "leaf indices 5", "leaf indexes 5",
  "the 5th leaf"), and "deposit 5", "position #16", "burn 5", "index 5".
- **Names no deposit or burn, by value** (`names_none_of`), for every U.08 refusal of the sequence: none of the case's
  own values as 8 hex digits in a row (a deposit's nullifier, commitment, depositor, destination, `s` and leaf; a
  burn's tag, `c_out`, recipient, release secret, leaf and `NOut`), nor its leaf index or burn counter as a number.

## What the tool says (verbatim, from `test.log`)

| U.08 mode | The case | Refusal |
|---|---|---|
| insufficient finality | the plaintext claim of leaf 0 before any root is imported (`flow::claim`, what `ferrata claim` runs) | `refused: LEZ bridge program exit code 245 NOT_FOUND (checked before sending: no imported root covers this leaf yet; wait for the next root import): no configuration, or no imported root at this leaf count` |
| verification failure | a private-kind deposit claimed by the plaintext path (`claim --plaintext`) | `refused: LEZ bridge program exit code 252 WRONG_DESTINATION (checked before sending: a private-kind deposit is claimed privately): … the deposit stays claimable: claim a private-kind deposit privately (the default, without --plaintext), as its destination account` |
| verification failure | leaf 0's opening redirected to the user's public `/1`, handed to the tool's claim-and-send step (`flow::claim_row`, what `ferrata claim` runs once it has scanned the deposit) with `--send --yes`; the binary itself offers no destination | ``refused: LEZ bridge program exit code 249 CLAIM_INVALID (checked before sending with the program's verify_claim): the opening, the leaf and the path do not hash to the root (a wrong leaf, path or destination); the deposit stays claimable: claim it with the seed that made it, to the destination it commits to (`ferrata scan` lists this seed's deposits)`` |
| insufficient finality | the private claim of leaf 16 before its root (17) is imported, `root(16)` imported | `refused: LEZ bridge program exit code 248 LEAF_NOT_IMPORTED (checked before sending: no imported root covers this leaf yet; wait for the next root import): the leaf index is at or above the imported root's leaf count, or outside the fee schedule range the claim names` |
| insufficient finality (F.12's admin depth) | the admin sets an inbound depth of 64 beacon slots; the relayer's `ImportRoot` of root 17 under the newest record is held (242); the same claim | `refused: LEZ bridge program exit code 248 LEAF_NOT_IMPORTED (checked before sending: no imported root covers this leaf yet; wait for the next root import; this configuration imports a root only once the block it reads is 64 beacon slots (about 13 min) past Ethereum finality (its inbound finality depth, set by its admin), so a deposit becomes claimable that much later): …` |
| insufficient finality (outbound) | the twelve burns' checkpoint a submitted candidate, not finalised; the private burn's self-sent release with a signer, a seal, `--send --yes` (`flow::release_burn`) | `refused: vault error BurnRootNotAccepted() (checked before sending: no finalized checkpoint covers this burn yet; wait for the next checkpoint): the burn root is not one of a finalized, unrevoked checkpoint` |
| verification failure (outbound) | the same release once its root is final, with a seal the verifier rejects (the mock verifier was told of no proof of its journal, as with a seal made for another recipient, relay fee or root) | ``refused: vault error VerificationFailed() (eth_call, nothing sent): the RISC Zero verifier rejects the proof: the seal is not a proof of this call's journal (another recipient, relay fee, root or image id) or is damaged; nothing is sent: prove it again for this journal (`ferrata release --prove`, or a prover given `--input-out`'s input, then `--seal`), or ask the relayer for a new proof`` |

The other U.08 refusals of the sequence, now checked by value too (and by state where the command would send: the
frozen claims and burns, the repeated private claim and the private burn's refusals on the zone, the frozen releases
by the signer's nonce; the others are dry runs or plans): an unsupported
token (``…TokenNotRegistered() (checked before sending): the token is not registered: only registered tokens are
bridged (`ferrata status --token <address>` shows whether a token is, and its denominations)``), an invalid amount
(``…InvalidDenominationIndex() (checked before sending): no denomination has this index: a token is bridged in its fixed
denominations only (`ferrata status --token <address>` lists them by index)``), a cap (`TokenDepositCapExceeded`, from
`eth_call`), the vault's deposit and release freezes and the program's mint and burn freezes (9 of 9), already claimed
(250, plaintext and private), already released (`NullifierAlreadyUsed`), a release in its delay
(`ReleaseDelayNotElapsed`), and a burn over the holding (254), not a denomination (251) or with too many decimals.

After the depth case a `Tick` reads a record 64 slots on, the same `ImportRoot` applies and the depth is set back to
0; the claim then goes on (held by its delay, then proven privately). After the not-final release the candidate is
finalised; after the rejected proof the test goes on with the relayer's release.

## What changed in the tool for this (commits `e83239ff`, `5e92dc04`)

- `cli/src/codes.rs`: the RISC Zero verifier's `VerificationFailed` (`0x439cc0cd`), raised through the vault, named
  with what to do (41 vault errors; the admin view's table and self-test follow); what to do for 249, 252,
  `TokenNotRegistered` and `InvalidDenominationIndex`.
- `cli/src/lez.rs` `plan_claim`: a claim no imported root covers names the configuration's inbound depth when its admin
  set one (`depth_note`; a failed read of it leaves the refusal as it was).
- `cli/src/flow.rs`: `claim` picks the scanned deposit and hands it to `claim_row`, which refuses, plans, shows and
  sends (a test can hand it a row of its own).
- `cli/tests/e2e.rs`: the cases above; the ledger counts every send (`Marks`, `unmoved`); `names_nothing` at full
  strength with its own test; `names_none_of` with `deposit_values` and `burn_values`.

## Each check fails when what it checks is broken (mutants, the build box, 23 November 2026)

[`mutants.py`](mutants.py) applies each mutant to the checkout, runs the tests and restores every file (also on
error; a marker count after each batch found none left). Tool: `python3 -I e2e/runs/cli-u8/mutants.py tool OUTDIR`
(`cargo test --locked --test e2e -- --nocapture` in `cli/`, with e2e.sh's environment), on the sources of `5e92dc04`
(the recorded run's tree adds only the nonce check of the already-released case and rewrapped lines); 688 s for the
twelve, about 0.10 USD. Measured.

| Mutant | Must break | Result |
|---|---|---|
| T1 `pick_import` takes "no import" as covering (the pre-send refusal removed) | the 245 case | fails: the root read's backstop answers `root(cid, 1) is not on the zone`, without what to do |
| T2 "wait for the next root import" removed | the 245 case | fails |
| T3 the refusal names the leaf ("covers leaf 0") | the 245 case | fails (`names_none_of`) |
| T4 `verify_claim` not run before sending | the 249 case | fails on its state check: `something was sent (Marks { submitted: 0, block: 10, nonces: ["payer Nonce(8)"] } -> Marks { submitted: 1, block: 11, nonces: ["payer Nonce(9)"] })` |
| T5 the plaintext path of a private-kind deposit not checked before sending | the 252 case | fails on its state check, the same way |
| T6 252's what to do removed | the 252 case | fails |
| T7 the inbound depth not named | the depth case | fails |
| T8 `BurnRootNotAccepted`'s what to do removed | the not-final release | fails |
| N1 `VerificationFailed` not mapped (the tool before this fix) | the rejected-proof release | fails with the raw revert the review measured: `the vault would revert: server returned an error response: error code 3: execution reverted: custom error 0x439cc0cd, data: "0x439cc0cd"` (the form test fails too: its message list names the error) |
| N2 `names_nothing` as in round 1 (a leaf by number only) | `names_nothing_catches_every_form` | fails: `names a position, yet passed: leaf (5)` |
| N3 a freeze refusal names a leaf by number ("(leaf number 3)", the review's R4, which passed every check in round 1) | the frozen claim | fails |
| N4 `TokenNotRegistered`'s what to do removed | the unsupported token case | fails |

Mini-app: `python3 -I e2e/runs/cli-u8/mutants.py miniapp OUTDIR` (`modules/ferrata_bridge_ui/scripts/box-test.sh` per
mutant, the view driven through its devnet backend), on the tree of `da4c9de7`; 431 s for the seven,
about 0.04 USD.
The last run, without a mutant, is the one in `modules/ferrata_bridge_ui/results/`. Measured.

| Mutant | Must break | Result |
|---|---|---|
| Q0 the messages for already claimed and frozen removed from `format.js` | `test_09`, `test_12` | 25 passed, 2 failed: exactly those |
| Q1 the other U.08 messages removed (cap, 240, invalid amount, unsupported token, 251, not final, no checkpoint, 249, 252, `VerificationFailed`) | `test_00`, `06b`, `09b`–`09f`, `10b`, `11c`, `12` | 17 passed, 10 failed: exactly those |
| Q2 the pre-send refusals removed (T1, T4, T5 and the deposit's registry check) | `test_06b`, `09b`, `09e`, `09f` | 23 passed, 4 failed: exactly those (each gets another refusal, found later or after sending) |
| Q3 the view refreshes the registry after a failed admin step (as `ffcf0da3` did) | `test_12` | 26 passed, 1 failed: "the banner shows the refusal" |
| Q4 `VerificationFailed` not mapped in the tool's library the devnet links (N1) | `test_11c` | 26 passed, 1 failed: the code is not shown |
| Q5 `format.js` as before this fix (`b5147257`) | `test_00`, `11c`, `12` | 24 passed, 3 failed: exactly those |
| F0 no mutant | nothing | 27 passed, 0 failed |

Not mutated: the vault's own refusals (Solidity), which the cases show the tool and the view handling.
