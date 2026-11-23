# Stream u8-cases: RFP-022 U.8, proposed rows (after review-u8)

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch `u8-cases`
from `a0f38827`. Box runs: the Modal build box, RISC Zero dev mode, local anvil (chain id 1) and the LEZ harness only;
laptop: the `links` and `admin` jobs' checks. Nothing was pushed or sent to any network. No program, guest or image id
changed; no dependency changed.

## What was asked, what the review found, what changed

The audit (audit-criteria §1.2 item 2) moved U.8 to in part: the tool had no case for a verification failure or for
insufficient finality, and the mini-app asserted two of the RFP's seven failure modes by their messages. Round 1
(`e83239ff`, `98f4bdb1`, `d81ee8fb`, `b5147257`) added them. The independent review (review-u8) found one gap that
kept U.8 in part and nine smaller ones; round 2 (`5e92dc04` and the commit after it) fixes them, each with a test that
fails before its fix (the mutant named; [`e2e/runs/cli-u8/`](../../e2e/runs/cli-u8/README.md)):

| Finding | Fix | Fails before the fix |
|---|---|---|
| M1: a release whose proof the verifier rejects was reported as the raw revert (`custom error 0x439cc0cd`), in the tool and the mini-app | `VerificationFailed` (the RISC Zero verifier's error, through the vault) named with what to do (`cli/src/codes.rs`, 41 vault errors; the admin view's table and self-test, 235 checks); `format.js` | the e2e's rejected-proof release (N1: the raw revert); `test_11c` (Q4, Q5); `test_00` (Q5) |
| L1: the no-naming check was weaker than the base for seven forms, and the only one for the freeze checks | `names_nothing` refuses any leaf once the exact general wordings are removed, and a number after deposit, burn, position, index, counter, nullifier, commitment or tag, or "#5"; `names_none_of` (by value) on every U.8 refusal of the sequence | `names_nothing_catches_every_form` (N2: "leaf (5)"); the frozen claim (N3, the review's R4, which passed every check before) |
| L2: "nothing signed" rested on the text for both 248 cases and 249 | the ledger counts every send; each refused claim or burn checks the zone's sends, block and nonces first, on its own; the 249 case goes through the tool's claim-and-send step (`flow::claim_row`) with `--send --yes`; the successful claims are the control (one send each) | the 249 case (T4) and the 252 case (T5): "something was sent (… submitted: 0 … -> … submitted: 1 …)" |
| L3: `test_12`'s fix hid a view regression | `Main.qml` refreshes the registry only after a change again (as before `ffcf0da3`); `test_12` asserts the banner and the admin line; 240 has its own message (a SetCap below the floor read "Cap exceeded") | `test_12` (Q3, Q5) |
| L4: a program-returned 251 read "Unsupported token"; "249 " matched "0.249 ETH" | 251 without the tool's check keeps "Unsupported token or amount"; codes anchored as "code NNN " | `test_00` (Q5) |
| L5: screenshots unshrunk, stale numbers | `scripts/shrink-screenshots.py` (48 screenshots, 2.2 MB); `RESULTS.md` dates the new run; `docs/components.md` and `admin/README.md` counts | — |
| L6: an agent folder in a committed log | `box-test.sh` gives Qt its own runtime directory and scrubs scratch folder names | `results/qmltest.txt` no longer carries the line |
| L7, L8: wording | this file and the record say which checks cover which cases, the stand-ins, and "through the devnet backend" | — |
| L9: no action for `TokenNotRegistered`, `InvalidDenominationIndex`; `cli/README.md`'s frozen row | both say what to do (`ferrata status --token <address>`); the row matches row 202 | the unsupported token case (N4) |
| L10: nits | the 249 index from the deposit; the deposit's `s` and leaf and the burn's `NOut` among the values; a failed depth read leaves the refusal; the record listed in `row-tests.toml`; the mutants script in the repository | — |

## The tool (`cli/tests/e2e.rs`, CI job `roundtrip`)

Recorded: [`e2e/runs/cli-u8/`](../../e2e/runs/cli-u8/README.md) (box, 23 November 2026, the tree of the commit after
`5e92dc04`: `test result: ok. 2 passed` in 55.05 s; on `5e92dc04` the same 2 passed in 59.15 s and `cargo test --lib
codes` 3 passed; measured). Every U.08 mode of the sequence
is refused with its source, code and what to do; every refusal is checked by form (`names_nothing`) and by value
(`names_none_of`); those from a command that would send are checked by state (on the zone: nothing handed to it, no
block, no nonce; releases: the signer's nonce). The four cases the audit asked for, and the one the review added:

| Asked | Tested as | Code |
|---|---|---|
| a verification failure | a private-kind deposit by `claim --plaintext`; leaf 0's opening redirected to another account, handed to `flow::claim_row` with `--send --yes` (the binary offers no destination); a self-sent release with a seal the verifier rejects | 252; 249; the verifier's `VerificationFailed` (from `eth_call`, nothing sent) |
| a claim before its root is imported | before any import; before its own root's, another imported | 245; 248 |
| a claim inside the depth | the admin's `SetDepth 64`; the relayer's `ImportRoot` held (242); the claim refused naming "64 beacon slots (about 13 min) past Ethereum finality"; then `Tick`, the import applies, depth back to 0 | 248, depth named |
| a release before its checkpoint finalises | the checkpoint a submitted candidate in its wait; the self-sent release with a signer, `--send --yes` | `BurnRootNotAccepted` |

## The mini-app (`qmltest/tst_miniapp.qml`, `box-test.sh`, the build box, not in CI)

Recorded: `modules/ferrata_bridge_ui/results/` (23 November 2026, the tree of the commit after `5e92dc04`: 27 of 27
in 40.5 s, 48 screenshots, the backend's 76 calls, 15 of them refusals; measured). Through the devnet backend
(`cli/examples/miniapp_devnet.rs`, the tool's library), not `ferrata_bridge_module` inside Logos Core. All seven
modes, each asserted on what the user sees (the banner's message and code), found before sending ("checked before
sending" or "eth_call before sending" in the raw error) and naming nothing (no hex run with a letter or of 40 digits, no
leaf, burn or counter by number, not the position's index). The state of the chains is not checked in the view's
tests (the tool's are):

| U.08 mode | Case (test function) | The user sees |
|---|---|---|
| unsupported token | a deposit of an unregistered token (`test_09b`) | "Unsupported token: this token is not registered with the bridge. …" |
| invalid amount | denomination 7 (`test_09c`) | "Invalid amount: only this token's fixed denominations can be bridged. …" |
| cap exceeded | the vault's ETH deposit cap set to 1 wei, a deposit (`test_09d`; the cap put back) | "Cap exceeded: the rate limit for this token is used up. …" |
| verification failure | a claim to another account than the commitment's (`test_09e`); a private-kind deposit by the plaintext path (`test_09f`); a release with a proof the verifier rejects (`test_11c`) | "Verification failed: the claim does not check against the imported root …"; "… a deposit to a private account can only be claimed privately. …"; "… the release's proof does not check against the vault. …" |
| insufficient finality | a claim before the relayer's import (`test_06b`); a release before any checkpoint (`test_10b`) | "Not final yet: the deposit is not covered by an imported root. …"; "Not final yet: no checkpoint covers this burn. …" |
| already claimed | a repeated claim (`test_09`) | "Already claimed: this position was claimed before. …" |
| frozen | a claim while mints are frozen (`test_12`), lifted, then accepted | "Frozen: the bridge or this token is frozen by its freeze authority …" |

Also `test_00` (the message table on its own) and `test_12`'s banner for a SetCap below the floor. Stand-ins, named in
the devnet: unsupported token, invalid amount, the cap, the redirected claim and the rejected proof are direct calls
(`host.app.call`) with the devnet's levers (`token`, `denomination`, `admin.set_cap {vault: true}` from the vault's
admin, `to_account`, `wrong_proof`); the view itself offers ETH and its denominations only. `test_09e` and `test_09f`
use deposits claimed by then: the program checks the opening (249) and the path (252) before the nullifier (250), and
without the pre-send check each gets 250 instead (Q2).

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-022 U.8 | in part (audit-criteria §1.2 item 2; review-u8 M1) | **shown** | the tool: every listed mode, verification failure on both legs included (rows 85, 202, the proposed row below); the mini-app: all seven on screen through the devnet backend (row 62, the proposed row). |

Proposed reason for `requirements-status.md`: "Refused before anything is signed, with source, code and what to do and
naming no deposit or burn, in the tool (`cli/tests/e2e.rs`, CI `roundtrip`; `e2e/runs/cli-u8/`): an unsupported
token, an invalid amount, a cap, already claimed, already released, every freeze of both halves (9 of 9, row 202), a
release inside its delay, a verification failure (252 by the plaintext path, 249 on a redirected opening handed to the
tool's claim-and-send step, the verifier's `VerificationFailed` on a release), insufficient finality (245 before any
import, 248 before the claim's root's, 248 naming the admin's inbound depth, `BurnRootNotAccepted` while the checkpoint
is a candidate); each refusal checked by form and by the case's own values to name nothing, and, where the command would
send, by state (nothing handed to the zone, no block, no nonce; the signer's nonce on Ethereum). The mini-app, through
its devnet backend (box runs, not in CI), shows all seven modes on its banner with what to do and the code, found
before sending and naming nothing (`qmltest/tst_miniapp.qml`). Mutants break each check (`e2e/runs/cli-u8/`)."

Traceability U.8, "Named test or artefact": add `e2e/runs/cli-u8/` and `modules/ferrata_bridge_ui/results/qmltest.txt`;
drop the "(not exercised …)" and "of the RFP's seven modes it asserts two …" clauses. Line 306 (open items): drop U.8.
`docs/guides/user-guide.md` §11 (another stream's file, not edited here): its "verification failure" row could add the
vault's `VerificationFailed` (a release's proof), and "insufficient finality" 248 (a root imported, not yet one covering
the deposit, e.g. inside the configuration's inbound depth) and `BurnRootNotAccepted`.

## Proposed EVIDENCE row (231, or the next free number)

Links as in `docs/EVIDENCE.md` (relative to `docs/`): `cli/tests/e2e.rs`, `e2e/runs/cli-u8/README.md`,
`modules/ferrata_bridge_ui/qmltest/tst_miniapp.qml`, `modules/ferrata_bridge_ui/results/qmltest.txt`.

| NEW | **Every U.8 failure mode in the tool and the mini-app (RFP-022 U.8)**: in the tool (`cli/tests/e2e.rs`, CI `roundtrip`; recorded `e2e/runs/cli-u8/`, box, 23 November 2026, `ok`, 2 tests in 55.05 s), a verification failure (252 by `--plaintext`; 249 on a redirected opening handed to the tool's claim-and-send step with `--send --yes`; the RISC Zero verifier's `VerificationFailed` on a self-sent release, now named with what to do) and insufficient finality (245 before any import; 248 before the claim's root; 248 naming "64 beacon slots (about 13 min) past Ethereum finality" while the admin's inbound depth holds the import; `BurnRootNotAccepted` while the checkpoint is a candidate), with the older modes, each refused before anything is signed (on the zone: nothing handed to it, no block, no nonce, checked on their own; releases: the signer's nonce), with source, code and what to do, and checked by form (`names_nothing`, which refuses any leaf outside the program's exact general wordings and a number after deposit, burn, position or index; its own test) and by the case's own values (`names_none_of`) to name nothing; in the mini-app (`qmltest/tst_miniapp.qml`, `results/qmltest.txt`, 27 of 27, through the devnet backend), all seven modes asserted on the banner's message and code, found before sending and naming nothing, and the admin view's banner kept after a failed step. Mutants: 12 of 12 in the tool (T4, T5 caught by the state checks), 6 of 6 in the mini-app, each failing only its own cases (`e2e/runs/cli-u8/mutants.py`) | `e2e/runs/cli-u8/`; `modules/ferrata_bridge_ui/results/` | U.8 |

## Files outside this stream's own, changed minimally

`admin/dump.mjs`, `admin/index.html`, `admin/README.md` (`VerificationFailed` in the admin view's table and self-test,
which cross-checks `cli/src/codes.rs`; 235 checks), `docs/components.md` (that count),
`modules/ferrata_bridge_ui/qml/Main.qml` (one line, L3), `modules/ferrata_bridge_ui/scripts/box-test.sh` (L6),
`modules/ferrata_bridge_ui/RESULTS.md` (L5), `cli/README.md` (the U.08 table), `scripts/ci/row-tests.toml` (the U.8
entry), `modules/ferrata_bridge_ui/results/` (the new run; every screenshot regenerated and shrunk).

## Not covered

- The RISC Zero router's `SelectorUnknown(bytes4)` (a seal for a verifier the router does not know) is not mapped: it
  takes an argument, which the table's selectors (`Name()`) do not, and the local vault has no router to test it.
- The mini-app's cases run on the build box, not in CI, and through the devnet backend, not `ferrata_bridge_module`.
