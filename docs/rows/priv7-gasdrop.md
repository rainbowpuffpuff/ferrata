# Rows: RFP-022 Priv.7 and U.5 (stream priv7-gasdrop)

Branch `priv7-gasdrop` from `861c1c0d`; review round 1 (`F1`–`F9`) answered. For the integrator: proposed status, the
evidence per clause, a proposed EVIDENCE row. Strict reading as in `docs/requirements-status.md`'s header.

## What was built

- **The tool uses the network's relayer by default** (`cli/src/gasdrop_net.rs`, `cli/src/main.rs`, `cli/src/flow.rs`):
  with no `--gas-drop-inbox`, `ferrata claim` reads `<origin of --sequencer>/relayer/gas-drop` (over https, or http from
  this machine only): the relayer's LEZ account, the guest's image id, the native LEZ per drop, the inbox URL (https or
  the terms' origin), whether it takes jobs. The guest it serves is used only if the tool pins its image id; the receipt
  must verify with exactly the natively computed journal before it leaves the machine. **Only an answer 404 or 410**
  ("no relayer published") leaves the fee to the user's own account (with the warning line, as without a drop); every
  other answer (429 with its Retry-After, any other 4xx, 5xx, a timeout, a transport error, a malformed document, a
  relayer paused or not taking jobs) stops the claim and names `--no-gas-drop` (review F1).
- **Who is paid the relay fee**: the claim names the network's relayer, so the deposit's registered relay fee (the same
  amount whoever is named; without a relayer it goes to the configuration's fee recipient) is paid to that relayer. The
  dry run, the gateway's terms (`relayFeeTo`) and the web wallet say so (F6). The terms read tells the network's host
  that this address is preparing a claim, as its history reads do; the dry run says that too (F7).
- **Burns**: a drop is sized for the claim and one burn; `ferrata burn` (private) pays its LEZ fee from a recorded drop
  account that still holds native LEZ, asking nothing new of any relayer (LEZ derives a private account's nonce and
  nullifier from its `nsk`, in rc1 and rc3: `lee/state_machine/core/src/account.rs`, so the relayer that knows the
  one-time keys cannot recognise that spend). With no drop left, the user's own account pays only where no relayer is
  published (404/410); with a relayer published the burn stops until `--no-gas-drop` confirms; an unreadable network
  stops it too. A remainder too small for the fee stops the burn with that remedy (F8b).
- **Relayer side**: `ferrata-relayer --gas-drops-only` (no Ethereum key, other jobs refused); the test network's
  gateway serves `GET /relayer/gas-drop` (404 only when unset; 503 when unreadable or paused), `GET
  /relayer/gas-drop/guest`, `POST /relayer/gas-drop`. A posted job's **receipt is verified before it counts against any
  cap** (`ferrata-gasdrop check-job`, `relayer::check_drop_job`: the STARK against the published image id, this relayer,
  chain and vault, the one-time keys deriving the receipt's one-time address); only then the inbox (8 pending), hourly,
  per-/24 and per-IP caps (F2). An N it has admitted is refused (409) before any check or cap while its job is in the
  inbox and for 7 days after it left it (persisted, no address; review round 2, R1). The gateway logs neither a job's address nor its N; Caddy keeps no access log of these
  routes (F5). `ferrata-gasdrop.service`, `ferrata-gasdrop-setup`, README "The gas-drop relayer: deploying it". Not
  deployed: the lead deploys after review.
- **Web wallet**: the bridge tab reads the same terms and names the relayer, what it learns, who is paid the relay fee
  and the opt-out; only 404/410 read as "none published" (F8a).

## Priv.7: proposed **in part** (narrower)

| Clause | Evidence |
|---|---|
| private by default: the fee payer's mitigation with no user action, a relayer reachable by default over the network | `e2e/runs/seqnet-gasdrop-4/gasdrop.md`: 20 of 20 PASS. With no gas-drop setting and no native LEZ of the user's own (its wallet reads public /0 and private /0 as 0 before the claim, after it and after the burn: a check that fails if anything funds the user), the claim's dry run names the network's relayer, read over HTTP from the test network's gateway, and that the relay fee is paid to it; the private claim applied (LEZ tx `031be023…f9cc`), its fee paid by one-time account `0xa94c…06f3` from the relayer's sealed credit of 200,000,000 (relayer tx `0e848a1e…fd93`), the job verified by the gateway's checker and posted through it |
| weakening needs explicit confirmation | only 404/410 lead to the user's own account without `--no-gas-drop`: `gasdrop_net::tests::{only_404_and_410_mean_no_relayer, every_other_answer_stops_the_claim_and_the_burn, a_timeout_or_no_listener_stops_the_claim, plain_http_elsewhere_stops_before_any_request, a_burn_without_a_drop_left}` (12 tests, box: 12 passed); on the running sequencer: same run: the claim stops, naming `--no-gas-drop` and never printing the own-account fee payer, on a paused relayer (503), unreadable terms (503), a 429 and a timeout (20 s); a network publishing none (404) leads to the own-account line, as without a drop; a junk job is refused (400) by the gateway before it counts (inbox empty, still accepting) |
| the opt-out, sent | same run: `ferrata claim --no-gas-drop --send --yes`: the user's own private account is the fee payer and, holding no native LEZ, LEZ's wallet refuses it before proving ("Fee reserve of 6211136 from payer … fails"); no private transaction added, nothing asked of the relayer |

Still short: the mini-app's claim and burn (its drop is a stand-in, rows 102, 103); the live test network run (the
relayer is not deployed there yet; no unclaimed deposit or wrapped balance exists on it: the lead's deploy and one
Sepolia deposit by the owner); this path with real proofs (the e2e is dev mode, on rc1's sequencer; the tool's
receipt check and the gateway's checker are tested on a real rc3 receipt). The branch now compiles against rc3 as `deploy/testnet/cli-rc3/build.sh` builds it (below), but nothing of it has run on an rc3 zone yet.

## U.5: proposed **in part** (narrower)

| Clause | Evidence |
|---|---|
| fee-payer mitigation on by default (CLI) | as Priv.7 above |
| fee-payer mitigation for burns | same run: the burn's dry run names the drop account (189,900,781 left after the claim; the claim's fee 10,099,219, derived); with the drop record set aside the burn stops while a relayer is published (it needs `--no-gas-drop`) and on a 429, and names the own account only on 404; the burn applied (LEZ tx `7e4b206e…aaab`), its fee paid by the drop's one-time account; the user still holds 0 native LEZ |
| indicator of what leaks | the claim's and the burn's `fee payer:` lines (the relayer, what it learns: N, the one-time account, a time, the posting address, never which deposit; who is paid the relay fee; the opt-out); the web wallet's line (`deploy/testnet/web/wallet/test/gasdrop.test.mjs`, 4 tests, in CI job `msg`) |
| the relayer serves it with bounds | `deploy/testnet/tests/test_gateway_gasdrop.py` (58 checks, CI job `links`): six fake receipts refused 400 leave the inbox, hourly, per-network and per-IP caps untouched; no checker or a checker past its timeout: 503, nothing counted; then the caps. Every N the gateway admits is remembered on disk (no address) while its job is in the inbox and for 7 days after it left it: the same receipt posted again once the relayer has moved its job out (done, or refused: a root the relayer did not import) is answered 409 before any check or cap, also with other one-time keys and after a restart (review round 2, R1; on the running sequencer: `e2e/runs/seqnet-gasdrop-5/gasdrop.md`, 21 of 21 PASS: the claim's own job, posted again once the relayer had finished it, answered 409 (taken already), nothing checked, counted or handed over; the other 20 checks as seqnet-gasdrop-4's). What the gateway cannot check (no Ethereum or zone history there) the relayer does: the root imported, N unclaimed, one drop per N. So a job that spends a cap needs an N the gateway has not taken in the last 7 days after its job finished: a new proof (about 13 s of CPU, M0 check 2) over a deposit tree the asker may make up; the caps then bound how many such jobs reach the relayer (12 an hour), not how many proofs an asker makes |

Still short: the mini-app (U.5 names the mini-app and the CLI); the live run; real proofs on this path.

## Tests and commands

- `cargo test --locked --lib gasdrop_net` (cli/, `RISC0_HOME` = LEZ's guest toolchain): 12 passed (box); CI job
  `roundtrip` (`.github/workflows/ci.yml`).
- `cargo test --locked --lib relayer` (cli/): the gateway's checker on the recorded real rc3 receipt
  (`relayer::tests::a_gateway_admits_only_a_verified_job_of_this_relayer`, `a_fake_receipt_passes_only_a_dev_mode_verifier`):
  passed (box); CI job `roundtrip`.
- `cargo check --locked --bins --tests --features seqnet` (cli/): clean (box).
- `python3 -I deploy/testnet/tests/test_gateway_gasdrop.py`: 58 checks (13 for R1); CI job `links`. `test_units.py`: all passed.
- `node --test deploy/testnet/web/wallet/test/*.test.mjs`: 48 passed; `gasdrop.test.mjs` in CI job `msg`.
- `bash e2e/seqnet/test-vault-deploy.sh`: passed (box); CI job `vault` (review F3: the seqnet vault deploy, every mode).
- `e2e/seqnet/run.sh gasdrop gasdrop-5` (box, after the R1 fix): 21 of 21 PASS (gasdrop-4's 20 and the re-post of the finished job:
  409); a first attempt stopped in LEZ session A (the zone's start-up, "connection refused", before the vault or the hook), the
  rerun passed.
- `e2e/seqnet/run.sh gasdrop gasdrop-4` (box; `CROWN_BRIDGE_BIN` = B1's bridge program, sha256 `537cbe7a…`): `seqnet-gasdrop-4`: 20 of 20 PASS (dev-mode proofs). After the hook, LEZ session's own export step logs "export failed (1)", as in seqnet-gasdrop-3 and seqnet-robust-2 (pre-existing, `lez/crown/session.sh` after the hook; the zone saved). Earlier attempts this round: none failed; round 1's gasdrop-1 (vault deploy, now F3) and gasdrop-2 (the viewing-key bound) are recorded in the commits
- `deploy/testnet/cli-rc3/build.sh SRC` on this tree (box): first refused (`--locked`: the rc3 lockfile of 23 Nov lacked two path crates cli/ gained since, `ferrata-lez-msg-wire` and `-v2-wire`); the lockfile re-resolved minimally (18 lines) and committed; then rc=0 for `ferrata`, `ferrata-relayer`, `ferrata-gasdrop` and `seqnet-fund` (`deploy/testnet/results/2026-11-25-cli-rc3-gasdrop.txt`, sha256s)
- `scripts/ci/row-tests.toml`: U.5 and Priv.7 list these tests; `python3 -I scripts/ci/check-row-tests.py`: consistent (every hard row has an entry, every path tracked, every test said to run in a CI job run by it)

## Proposed EVIDENCE row

| n | The gas drop by default: `ferrata claim` and `ferrata burn` with no gas-drop setting pay their LEZ fees from the network's relayer, read from the network's gateway; every answer but 404/410 stops them; the gateway verifies a job's receipt before it counts against its caps | 20 of 20: relayer published (relay fee to it); the user holds no native LEZ before, after the claim and after the burn; the claim stops on paused, unreadable, 429 and timeout, own account only on 404; junk job refused before it counts; the opt-out sent stops for want of native LEZ; claim applied (fee from the one-time account, credit 200,000,000); burn stops with no drop left unless --no-gas-drop (and on 429), own account on 404; burn applied from the 189,900,781 left | local LEZ v0.4.0-rc1 sequencer and node, anvil, the test network's gateway on loopback (build box) | dev-mode proofs (the gateway's checker also on a real rc3 receipt, unit test) | `e2e/seqnet/run.sh gasdrop NAME`; `cargo test --lib gasdrop_net`; `test_gateway_gasdrop.py` | `e2e/runs/seqnet-gasdrop-4/gasdrop.md` |
