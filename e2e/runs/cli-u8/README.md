# The `ferrata` tool's verification-failure and finality refusals (RFP-022 U.08)

**Run:** `cli/tests/e2e.sh` (the test `deposit_scan_claim_burn_release_and_recovery` of
[`cli/tests/e2e.rs`](../../../cli/tests/e2e.rs)), on a Modal build box (CPU), 23 November 2026, the tool's sources of
commit `e83239ff` (branch `u8-cases`). **Result:** `test result: ok. 1 passed; 0 failed` in 50.98 s
([`test.log`](test.log), the test's own output; the build's lines before it are left out and the checkout's path is
written `<repo>`). The same test runs in CI's `roundtrip` job (`.github/workflows/ci.yml`).

**Where it ran:** FerrataVault v5 on a local anvil (chain id 1, never a public network) with the vault tests'
`MockRiscZeroVerifier`; the zone in the LEZ v0.4.0-rc1 in-process harness (LEZ's own state transition with the bridge
guest, RISC Zero dev mode). Proofs are not what U.08 is about; none is real here. Stand-ins as the test names them: the
relayer's imports and `Tick` through the harness's calls, the admin's `SetDepth` signed by the harness's admin role
key, the checkpoint prover and relayer by the mock verifier and anvil's first account.

## What the tool says (verbatim, from `test.log`)

Each case is refused **before anything is signed**: for a claim, the zone's applied and refused transactions are
counted before and after (unchanged); for the release, the signer's Ethereum nonce (unchanged). Each refusal is
checked for its source and code, what to do, and with `names_none_of`: no 40 hex digits in a row, no leaf named by
number, no burn counter, **and** none of the case's own values (8 hex digits in a row of its nullifier, commitment,
depositor, destination, burn tag, `c_out` or recipient, any case; its leaf index or burn counter as a number).

| U.08 mode | The case | Refusal |
|---|---|---|
| insufficient finality | the plaintext claim of leaf 0 before any root is imported (`flow::claim`, what `ferrata claim` runs) | `refused: LEZ bridge program exit code 245 NOT_FOUND (checked before sending: no imported root covers this leaf yet; wait for the next root import): no configuration, or no imported root at this leaf count` |
| verification failure | a private-kind deposit claimed by the plaintext path (`claim --plaintext`) | `refused: LEZ bridge program exit code 252 WRONG_DESTINATION (checked before sending: a private-kind deposit is claimed privately): a destination this path cannot credit: a plaintext claim of a private-kind deposit, or a private claim not run as its destination; the deposit stays claimable: claim a private-kind deposit privately (the default, without --plaintext), as its destination account` |
| verification failure | leaf 0's opening redirected to the user's public `/1`, not the account its commitment names (`lez::plan_claim`, the function `claim` calls; `ferrata claim` itself offers no destination) | `refused: LEZ bridge program exit code 249 CLAIM_INVALID (checked before sending with the program's verify_claim): the opening, the leaf and the path do not hash to the root (a wrong leaf, path or destination); the deposit stays claimable: claim it with the seed that made it, to the destination it commits to (`ferrata scan` lists this seed's deposits)` |
| insufficient finality | the private claim of leaf 16 before its root (17) is imported, `root(16)` imported | `refused: LEZ bridge program exit code 248 LEAF_NOT_IMPORTED (checked before sending: no imported root covers this leaf yet; wait for the next root import): the leaf index is at or above the imported root's leaf count, or outside the fee schedule range the claim names` |
| insufficient finality (F.12's admin depth) | the admin sets an inbound depth of 64 beacon slots; the relayer's `ImportRoot` of root 17 under the newest record is held (242); the same claim | `refused: LEZ bridge program exit code 248 LEAF_NOT_IMPORTED (checked before sending: no imported root covers this leaf yet; wait for the next root import; this configuration imports a root only once the block it reads is 64 beacon slots (about 13 min) past Ethereum finality (its inbound finality depth, set by its admin), so a deposit becomes claimable that much later): …` |
| insufficient finality (outbound) | the twelve burns' checkpoint a submitted candidate, not finalised; the private burn's release with a signer, `--send --yes` (`flow::release_burn`, `--self-send`) | `refused: vault error BurnRootNotAccepted() (checked before sending: no finalized checkpoint covers this burn yet; wait for the next checkpoint): the burn root is not one of a finalized, unrevoked checkpoint` |

After the depth case a `Tick` reads a record 64 slots on, the same `ImportRoot` applies and the depth is set back to
0; the claim then goes on as before (held by its delay, then proven privately). After the release case the candidate
is finalised and the test goes on as before. Every other case of the test is unchanged and passed.

## What changed in the tool for this (commit `e83239ff`)

- `cli/src/lez.rs` `plan_claim`: a claim no imported root covers is refused with the program's code (245 or 248) as
  before, and now also names the configuration's inbound finality depth when its admin set one (`depth_note`: the
  slots and about how many minutes; read from `config(cid)`, the same for every deposit).
- `cli/src/codes.rs`: 249 `CLAIM_INVALID` and 252 `WRONG_DESTINATION` now say what to do.
- `cli/tests/e2e.rs`: the six cases; `names_nothing` names a leaf only by number (the program's own meanings of 248
  and 249 speak of "the leaf index" and "the leaf and the path" in general), and `names_none_of` adds the case's own
  values.

## Each test fails without its refusal or its message (mutants, 23 November 2026)

Each mutant was applied to the tree, the test run, and the file restored (script: the stream's `mutants.py`, kept
outside the repository; the markers of every mutant counted 0 in the tree afterwards). Tool: `cargo test --test e2e`
(e2e.sh's environment); mini-app: `box-test.sh`. Measured, 23 November 2026, on `e83239ff`.

| Mutant | Must break | Result |
|---|---|---|
| T1 `pick_import` (cli/wallet) takes "no import" as covering: the pre-send refusal removed | 245 case | fails: `refused: … 245 NOT_FOUND (checked before sending: root(cid, 1) is not on the zone)` lacks "no imported root covers this leaf yet" |
| T2 "wait for the next root import" removed | 245 case | fails on the missing what-to-do |
| T3 the refusal names the leaf (`covers leaf 0 yet`) | 245 case | fails: `names_none_of` |
| T4 `verify_claim` not run before sending | 249 case | fails: the redirected claim is planned |
| T5 the plaintext path of a private-kind deposit not checked before sending | 252 case | fails: refused only by the program, "returned by the program in the harness", after sending |
| T6 252's what-to-do removed | 252 case | fails |
| T7 the inbound depth not named | the depth case | fails |
| T8 `BurnRootNotAccepted`'s "wait for the next checkpoint" removed | the release case | fails |
| Q1 the seven U.08 entries of `format.js` removed | `test_06b`, `09b`–`09f`, `10b` | 7 failed, 17 passed (each showing "The operation failed. Nothing was changed.") |
| Q2 pre-send refusals removed: T1, T4, T5 and `deposit_price`'s registry check | `test_06b`, `09b`, `09e`, `09f` | 4 failed, 20 passed (`09b` then shows "Invalid amount"; `06b` gets the root read's 245 "root(cid, 1) is not on the zone", `09e` and `09f` the spent nullifier's 250: each fails its own refusal's check) |
| B0 the base commit `a0f38827`'s view, test and backend | (base check) | `test_12_admin` fails "a cap below the floor is refused", as on this branch before its fix: pre-existing |
| F0 this branch, no mutant (the recorded run) | nothing | 24 passed, 0 failed |

Not mutated: the vault's own refusals (`TokenNotRegistered`, `InvalidDenominationIndex`, `TokenDepositCapExceeded`,
`BurnRootNotAccepted` on chain are Solidity's; the cases show the tool's and the view's handling of them).
