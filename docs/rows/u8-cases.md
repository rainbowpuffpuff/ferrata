# Stream u8-cases: RFP-022 U.8 restored to shown, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch
`u8-cases` from `a0f38827`. Box runs: the Modal build box, RISC Zero dev mode, local anvil (chain id 1) and the LEZ
harness only; nothing was pushed or sent to any network. No program, guest or image id changed; no dependency
changed.

## What the audit asked (audit-criteria §1.2 item 2)

U.8 was moved from shown to in part because the tool had no case for a verification failure or for insufficient
finality, and the mini-app asserted two of the RFP's seven failure modes by their messages (already claimed,
frozen), mapping the other five by pattern only. Asked: in the tool, four cases each refused before anything is
signed, with source, code and what to do, naming no deposit or burn (a verification failure, a claim before its root
is imported, a claim inside the depth, a release before its checkpoint finalises); in the mini-app, the five other
modes asserted on what the user sees; each test failing if its refusal or message is removed.

## Commits

- `e83239ff`: the tool's six cases (`cli/tests/e2e.rs`), the depth named in the refusal (`cli/src/lez.rs`), what to do
  for 249 and 252 (`cli/src/codes.rs`); the mini-app's seven cases (`modules/ferrata_bridge_ui/qmltest/tst_miniapp.qml`),
  their messages (`qml/js/format.js`), the devnet's three levers (`cli/examples/miniapp_devnet.rs`); the row map's note
  (`scripts/ci/row-tests.toml`).
- `98f4bdb1`: the mini-app's recorded run (`modules/ferrata_bridge_ui/results/`, 24 of 24).
- the third commit (after `98f4bdb1`): the tool's recorded run and its mutants (`e2e/runs/cli-u8/`), `cli/README.md`'s U.08 table, this
  file.

## The tool: four asked cases, six tested (`cli/tests/e2e.rs`, CI job `roundtrip`)

Recorded: [`e2e/runs/cli-u8/`](../../e2e/runs/cli-u8/README.md) (box, 23 November 2026, `e83239ff`'s sources,
`test result: ok`, 50.98 s; measured). Each is refused before anything is signed (the zone's transactions counted
before and after; for the release, the signer's nonce), with source, code and what to do, and passes `names_none_of`
(no 40 hex digits in a row, no leaf or burn counter by number, and none of the case's own nullifier, commitment,
depositor, destination, tag, `c_out`, recipient by 8 hex digits, nor its index):

| Asked | Tested as | Code |
|---|---|---|
| a verification failure | a private-kind deposit by `claim --plaintext` (`flow::claim`); an opening redirected to another account (`lez::plan_claim`, the library function `claim` calls: the binary offers no destination) | 252 `WRONG_DESTINATION`; 249 `CLAIM_INVALID` |
| a claim before its root is imported | before any import (`flow::claim`, plaintext); before its own root's, another imported (`flow::claim`, private, the defaults) | 245 `NOT_FOUND`; 248 `LEAF_NOT_IMPORTED` |
| a claim inside the depth | the admin's `SetDepth 64`; the relayer's `ImportRoot` held (242); the claim refused naming the depth ("64 beacon slots (about 13 min) past Ethereum finality"); then `Tick`, the import applies, depth back to 0 | 248, depth named |
| a release before its checkpoint finalises | the checkpoint a submitted candidate in its wait; `flow::release_burn` with a signer, `--send --yes` | vault `BurnRootNotAccepted` |

## The mini-app: the five other modes on screen (`qmltest/tst_miniapp.qml`, `box-test.sh`, the build box)

Recorded: `modules/ferrata_bridge_ui/results/qmltest.txt` (23 November 2026, 24 passed, 0 failed, 41.0 s of Qt Quick
Test; measured), screenshots `13b`, `19b`–`19f`, `21a`. Each case asserts the banner's message (`error.message`, what
to do) and code (`error.detail`), that the raw refusal was found before sending ("checked before sending" or the
vault's "eth_call before sending"), and that it names nothing (no 8 hex digits in a row, no leaf, burn or counter by
number, not the position's index):

| U.08 mode | Case (test function) | The user sees |
|---|---|---|
| unsupported token | a deposit of an unregistered token (`test_09b`) | "Unsupported token: this token is not registered with the bridge. Only the tokens the registry lists can be bridged." (`TokenNotRegistered`) |
| invalid amount | denomination 7 (`test_09c`) | "Invalid amount: only this token's fixed denominations can be bridged. Choose one of the amounts the registry lists." (`InvalidDenominationIndex`) |
| cap exceeded | the vault's ETH deposit cap set to 1 wei, a deposit (`test_09d`; the cap put back) | "Cap exceeded: the rate limit for this token is used up. …" (`TokenDepositCapExceeded`, from `eth_call`) |
| verification failure | a claim to another account than the commitment's (`test_09e`); a private-kind deposit by the plaintext path (`test_09f`) | "Verification failed: the claim does not check against the imported root …" (249); "Verification failed: a deposit to a private account can only be claimed privately. …" (252) |
| insufficient finality | a claim before the relayer's import (`test_06b`); a release before any checkpoint (`test_10b`) | "Not final yet: the deposit is not covered by an imported root. …" (245); "Not final yet: no checkpoint covers this burn. …" (`BurnRootNotAccepted`) |

`test_09e` and `test_09f` use the two deposits already claimed by then (the view's claims of `test_07`, `test_08`):
the program's checks put the opening (249) and the path (252) before the spent nullifier (250), so each is refused
for its own reason; with the pre-send check removed they get 250 instead (Q2 below).

Stand-ins, named in the devnet (`cli/examples/miniapp_devnet.rs`): `deposit` and `price` take a `token`; `claim`
takes a `to_account` (a request naming another account than the commitment's, as a faulty or tampered request would;
the view itself offers none); `admin.set_cap` with `vault: true` sends the vault's `setTokenCap` from its admin
(anvil's first account, which deployed it). The devnet's `release` now reports a burn no checkpoint covers as the
vault's `BurnRootNotAccepted`, as the tool does.

Pre-existing, fixed in the test only: `test_12_admin`'s "a cap below the floor is refused" read the banner's error,
which the registry refresh after every admin step clears; the refusal (240) itself was there. The base commit
`a0f38827`'s view, test and backend fail it the same way on the box (run B0 below); the check now reads the admin
view's result line, "Not done: refused: LEZ bridge program exit code 240 BAD_MESSAGE …". The banner vanishing after a
failed admin step is left as it is (a view matter, not U.8's).

## Each test fails without its refusal or its message (mutants, the build box, 23 November 2026)

Each mutant was applied to the tree, the test run, and the file restored (script: the stream's `mutants.py`, kept
outside the repository; the markers of every mutant counted 0 in the tree afterwards). Tool, in `cli/`: `cargo test --locked --test e2e -- --nocapture` with `FERRATA_E2E=1 RISC0_DEV_MODE=1
RISC0_HOME=<spike>/risc0-home-lez`, `~/.foundry/bin` on the path and the variables of
`python3 -I scripts/fetch_native.py env` (what e2e.sh sets); mini-app, from the root: `bash
modules/ferrata_bridge_ui/scripts/box-test.sh` in the same environment. Measured, 23 November 2026, on `e83239ff`.

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

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-022 U.8 | in part (audit-criteria §1.2 item 2) | **shown** | Every listed mode refused with source, code and what to do and naming no deposit or burn, in the tool (rows 85, 202 and the proposed row below: verification failure 249 and 252, insufficient finality 245, 248, the admin's depth and the outbound `BurnRootNotAccepted`) and in the mini-app on screen (all seven modes: already claimed and frozen as before, row 62; the other five in the proposed row). |

Proposed text for `requirements-status.md`'s U.8 reason: "Refused before anything is signed, with source, code and
what to do and naming no deposit or burn, in the tool (`cli/tests/e2e.rs`, CI `roundtrip`): an unsupported token, an
invalid amount, a cap, already claimed, already released, every freeze of both halves (9 of 9, row 202), a release
inside its delay, a verification failure (252 by the plaintext path, 249 on a redirected opening), insufficient
finality (a claim before any import 245, before its root's import 248, inside the admin's inbound depth 248 naming the
depth, a release whose checkpoint is not final `BurnRootNotAccepted`), each checked by value to name none of its own
deposit's or burn's values (`e2e/runs/cli-u8/`); an Ethereum RPC of another chain refused before anything is planned
or sent. The mini-app asserts all seven modes by what the user sees (`qmltest/tst_miniapp.qml`; the five added in
row NEW), each failing when its message or its pre-send refusal is removed."

Traceability U.8, "Named test or artefact": add `e2e/runs/cli-u8/`, `modules/ferrata_bridge_ui/results/qmltest.txt`;
remove the "(not exercised …)" and "of the RFP's seven modes it asserts two …" clauses. Line 306 (open items): drop
U.8 from the heading's list. `docs/guides/user-guide.md` §11's table (another stream's file, not edited here): the
"insufficient finality" row could add 248 `LEAF_NOT_IMPORTED` (a root imported, not yet one covering the deposit,
e.g. inside the configuration's inbound depth) and the vault's `BurnRootNotAccepted` for a release.

## Proposed EVIDENCE row (231, or the next free number)

Links as in `docs/EVIDENCE.md` (relative to `docs/`): `cli/tests/e2e.rs`, `e2e/runs/cli-u8/README.md`,
`modules/ferrata_bridge_ui/qmltest/tst_miniapp.qml`, `modules/ferrata_bridge_ui/results/qmltest.txt`.

| NEW | **The tool's and the mini-app's remaining U.8 failure modes (RFP-022 U.8)**: in the tool (`cli/tests/e2e.rs`, CI `roundtrip`; recorded `e2e/runs/cli-u8/`, box, 23 November 2026, `ok` in 50.98 s), a verification failure (252 for a private-kind deposit by `--plaintext`, 249 for an opening redirected to another account) and insufficient finality (245 before any import, 248 before the claim's root import, 248 naming "64 beacon slots (about 13 min) past Ethereum finality" while the admin's inbound depth holds the import, the vault's `BurnRootNotAccepted` for a release whose checkpoint is a candidate), each refused before anything is signed (zone transactions and signer nonce unchanged), with source, code and what to do, and checked to name none of its own deposit's or burn's values; in the mini-app (`qmltest/tst_miniapp.qml`, `results/qmltest.txt`, 24 of 24), unsupported token, invalid amount, cap exceeded, verification failure (249, 252) and insufficient finality (a claim, a release) asserted on the banner's message and code, before sending and naming nothing. Mutants: 8 of 8 in the tool and 2 of 2 in the mini-app make their cases fail (messages or pre-send refusals removed; `e2e/runs/cli-u8/`) | `e2e/runs/cli-u8/`; `modules/ferrata_bridge_ui/results/` | U.8 |
