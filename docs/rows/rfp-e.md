# Stream rfp-E: relayer, monitor and client robustness (RFP-022)

Proposals for the integrator; this stream does not edit `requirements-status.md`, `traceability.md`, `EVIDENCE.md`
or the README's status lines. Branch `rfp-relayer-robustness`, from `f13faee2`; commits in the last section. Revised
after the first review: U.3 and R.6 are proposed **in part** with their exact remainders; R.6's relayer gaps that
review found (the gas drop not resumed, the `--send-via logos` route without a journal, the record's narrow windows
never hit, faults claimed that did not fire) are built, tested and recorded. Revised again after the second review:
its two defects in that new code are fixed and tested (M1: a gas drop sent again is built only on its recorded state,
so a credit that lands meanwhile no longer lets a second one apply; M2: a request `tx_sender_module` can no longer
account for is settled on the chain, never read as "nothing sent"), and two credits of one drop are shown reaching a
running sequencer with one applied.

Where everything ran: a Modal build box (CPU), 21 and 22 November 2026. Ethereum: local anvil only (nothing sent to
Sepolia or mainnet, no public endpoint read). LEZ: the v0.4.0-rc1 in-process harness, the mini-app's devnet (the
harness behind HTTP), LEZ v0.4.0-rc1's own sequencer and node (e2e/seqnet), and LEZ v0.4.0-rc3's own sequencer and
node 0.3.5-rc.1 (upstream's four-sequencer devnet). None of R.06, R.07, U.03, U.08 is about proving: dev-mode receipts
and mock or accept-all verifiers are used there and named. No program, guest or image id changed; no deployment is
affected. The second review's fixes change off-chain code only (`cli/`, `monitor/src/logos_send.rs`, test helpers).

## Summary

| Row | Today | Proposed | Shown by | Remaining |
|---|---|---|---|---|
| U.8 | in part | **shown** | `cli/tests/e2e.rs`, `e2e/runs/cli-freezes/` | — |
| R.6 | in part | **in part** (the relayer daemon shown on both chains, with the limits below) | `cli/tests/relayer_crash.rs` parts 1–6 (`e2e/runs/relayer-crash/`), `e2e/netfault/relayer-lez.sh` (`e2e/runs/relayer-lez/`), `relayer::tests` and `e2e/runs/seqnet-robust-2/`, `-3/` (gas drops), row 43 (monitor kills) | `monitor` under an RPC failure (only kills are tested), the checkpoint prover and the attestors under RPC failure; the relayer's root import under a fault on a real sequencer (its faults ran against the devnet stand-in); the `--send-via logos` route with the real `tx_sender_module` or Logos Core restarted, and `ferrata_relayer_module` itself killed (a stand-in of the module was restarted) |
| U.3 | in part | **in part** (narrower) | rows 43–48, 96, 106–107, 120; `e2e/runs/seqnet-robust-1/` and `-2/` (the daemon's root import on a running sequencer, by the standalone binary) | `ferrata_relayer_module` with a LEZ sequencer path; chain access through the LEZ and Ethereum Logos modules; `monitor`'s LEZ and Logos Blockchain RPC clients; the operator guide past its first draft |
| R.7 | in part | **shown** | `e2e/runs/seqnet-robust-1/`, `-2/` and `-3/` (claim killed before sending, burn killed after it, burn killed before it reached the zone then made with other parameters, gas drops across kills), with row 126 | — |
| U.5 | in part | in part, gap narrowed | the gas drop in the relayer daemon and the tool, journaled (`e2e/runs/seqnet-robust-2/`, `-3/`) | the mini-app's claim, burns with a drop, real proofs of this path, a network handover of the job to a third-party relayer |
| F.7 | in part | in part (narrower gap) | rc3 four-key zone (`zone-rc3-four-keys`), its stall traced (`STALL.md`) | the bridge, a t-of-n checkpoint and releases over an rc3 multi-key zone |

## U.8: actionable errors, including program or per-token frozen — proposed **shown**

RFP: "Return clear, actionable error messages for all failure modes: unsupported token, invalid amount, cap exceeded,
verification failure, insufficient finality, already claimed, and program or per-token frozen. Error messages must
not reveal which deposit or burn a failed attempt referred to."

What was missing: the freeze refusal was exercised in the mini-app only. Now, in the tool:

| Clause | Test | Result |
|---|---|---|
| frozen, program-wide and per token, deposit side | `cli/tests/e2e.rs` (`deposit_scan_claim_burn_release_and_recovery`): the vault's freeze authority `setDepositsFrozen`, then `setTokenDepositsFrozen(ETH)`; `ferrata deposit` (the binary) | `refused: vault error DepositsAreFrozen() (checked before sending: frozen until Ethereum time … (UTC date), unless the freeze authority lifts it sooner): deposits are frozen for the whole vault; nothing is sent: retry once the freeze ends (`ferrata status` shows it)`, and `TokenDepositsAreFrozen()` likewise; lifted, the dry run passes |
| frozen, claims (mints), whole configuration and token | the bridge program's freeze authority `Freeze` (token, then none); `claim` on the plaintext path, then the private path before its proof | `refused: LEZ bridge program exit code 242 FROZEN (checked before sending: mints (claims) of this token are frozen until Ethereum time …; nothing is sent and nothing is consumed: the deposit stays claimable): … retry once the freeze ends`; lifted, the same claim settles |
| frozen, burns | `Freeze` of burns (token, then configuration); `burn` | `… 242 FROZEN (checked before sending: burns of this token are frozen until …; nothing is sent and nothing is burned: the holding stays yours) …`; lifted, the burn settles |
| frozen, releases | `setReleasesFrozen`, `setTokenReleasesFrozen(ETH)`; `release` | `refused: vault error ReleasesAreFrozen() (checked before sending: frozen until …): releases are frozen for the whole vault; the burn stays releasable: retry once the freeze ends …`; lifted, released |
| no deposit or burn named | every refusal above checked with the test's `names_nothing` (no run of 40+ hex digits, no leaf index, no burn counter) | 9 of 9 |
| the other listed failures | unchanged, EVIDENCE row 85 (21 refusals, same test file) | as before |

Run: `cd cli && bash tests/e2e.sh`, commit `dfd4e2e4`: `test result: ok. 1 passed` in 34.47 s; again at `c7f1b352`
after the R.07 changes: 1 passed in 36.78 s; again at `d8ddc000` after the review fixes (the claim's gas-drop record per
relayer): 1 passed in 30.20 s; again at `8f99ec7f` after the second review's fixes: 1 passed in 27.59 s. Record:
`e2e/runs/cli-freezes/` (README with every refusal verbatim, `test.log`).

Proposed EVIDENCE row: "The `ferrata` tool under every freeze (RFP-022 U.08): the vault's deposit and release freezes
(whole vault, per token) and the bridge program's mint and burn freezes (whole configuration, per token) each refused
before anything is signed or proven, with its source and code, the freeze's end as a UTC date and what to do, naming
no deposit or burn; each lifted, the same command goes through | 9 freeze refusals in one run (deposit ×2, plaintext
claim ×2, burn ×2, release ×2, private claim ×1), all passing `names_nothing`; `test result: ok`, 34.47 s | Modal build
box; anvil (chain id 1), LEZ v0.4.0-rc1 harness | none (dev mode; mock verifier) | `cd cli && bash tests/e2e.sh` |
`e2e/runs/cli-freezes/`".

## R.6: temporary RPC or connectivity failure on either chain, recoverable, no work duplicated — proposed **in part**

RFP: "Temporary RPC or connectivity failure on either chain leaves any off-chain component in a recoverable state,
able to resume without duplicating work already done."

**The relayer daemon: every clause shown, on both chains**, with the limits named under the table (the `--send-via
logos` route ran against a stand-in of `tx_sender_module`; the root import's faults against the devnet's zone).

| Clause, for the relayer | Test | Result |
|---|---|---|
| killed mid-transaction, Ethereum, exactly-once effects | `cli/tests/relayer_crash.rs` part 1 (automine off; SIGKILL with the transaction pending, then restarted with it pending (P), mined before the restart (M), dropped from the pool (D)): submitCandidate P, finalizeAttested M, release D, release P, release with checkpoint fee M, claimOwed D | 6 transactions for 6 effects, none reverted; P: `inflight-pending`, nothing sent until mined; D: the same call sent again with the same nonce |
| killed at random | part 2: 20 releases, SIGKILL 0–40 ms after each start until all booked | 26 transactions for 26 effects (cumulative), every job once (3 kills sufficed in the recorded run: where they land is chance, hence part 4) |
| the record's two narrow windows, deterministically | part 4 (`e2e/netfault/holdproxy.py` in front of the anvil): the send's answer lost; the send forwarded, its answer held, the daemon SIGKILLed; the daemon SIGKILLed with its record written and nothing sent | each fault fired (3 of 3); the first two found under their nonce with no hash known (`inflight-found`); the third `inflight-lost`, sent again with the recorded nonce (40); 41 transactions for 41 effects |
| Ethereum endpoints failing | part 3: `e2e/netfault/faultnet.py` flaky, sends and lagging endpoints, with outages | all jobs done (2 s, 83 s, 19 s); 38 transactions for 38 effects; **the faults that fired** (results.json `faultsSeen`): flaky 3 × 429, 1 × 5xx, 4 receipts behind; sends 2 dropped, 1 duplicated, 6 not found, 4 receipts behind, 119 answers refused in outages; lagging 5 × 429, 28 receipts behind, 3 rounds `node-behind`. Drawn with a rate but not fired this time: flaky's "not found", timeouts and a head going backwards; a lost send answer is part 4's |
| the `--send-via logos` route (Logos Core's `tx_sender_module`) killed | part 5 (`cli/examples/relayer_logos_stub.rs` with `e2e/netfault/txsender_stub.py`, a stand-in for `tx_sender_module` that **survives each kill**; no key in the daemon): SIGKILL after the request was made with nothing broadcast; before the request's id reached the daemon; after the broadcast, before its hash reached it | each restart found its request (by id; or among `live_sends`, matched to the very call by the tag in its purpose) and drove it to its end: 3 requests for 3 jobs, each booked once; 44 transactions for 44 effects, none reverted |
| what `tx_sender_module` cannot account for (the second review's M2) | part 6, the stand-in with a durable history written before each broadcast (as the module's): (a) automine off, broadcast with its answer held, the daemon SIGKILLed, **the module restarted** (its requests gone), the daemon restarted, the transaction pending; (b) the request made, nothing broadcast, the daemon killed, the module restarted; (c) `stuck` (no hash) with the transaction out; (d) `stuck` with nothing sent | (a) `inflight-request-forgotten` with the request's history row (nonce, hash), `inflight-pending` while it waited, booked from its receipt: **no request after the restart**, nothing refused (before the fix the job went to `refused/` while its transaction then mined); (b) no row, so nothing left: planned again, 1 request; (c) found mined under the request's nonce (`inflight-found`), 1 request; (d) `inflight-lost`, the same call through the module **with the nonce pinned**: 2 requests, 1 transaction; 48 transactions for 48 effects, none reverted |
| LEZ connectivity failure, root imports | `e2e/netfault/relayer-lez.sh` (holdproxy in front of the devnet's zone): outage; ImportRoot applied but its answer lost; SIGKILL with the answer held; SIGKILL with the import held unsent | 9 of 9: each leaf count imported once; booked from the record, nothing sent again; the unsent one planned again after `--import-wait` and imported once |
| LEZ failure and kills, gas drops (the relayer's LEZ credits) | `relayer::tests` (10 drop tests: a model zone in which credits built on one account state apply once; among them the second review's race, `a_credit_landing_while_it_is_sent_again_is_found_and_nothing_more_is_built`) and, on LEZ v0.4.0-rc1's running sequencer, the drops daemon behind a second holdproxy: `e2e/runs/seqnet-robust-2/` (its first credit trapped unsent and the daemon SIGKILLed, then a 503 on its next send; its second credit forwarded with the answer held and the daemon SIGKILLed) and `e2e/runs/seqnet-robust-3/` (the same, the 503'd send **forwarded to the sequencer 35 s later**, after the daemon sent a third) | robust-2: 21 of 21; robust-3: 22 of 22. Drop 1 in robust-3: 3 sends; the third forwarded at 02:56:34 and applied (block 34, `15bb85f8…`); the second, answered 503 to the daemon at 02:56:14, reached the sequencer at 02:56:49 and was accepted (its hash `50a9abf9…`) but **never applied** (not among the zone's applied transactions through block 59, `lez/transactions.tsv`): two credits of one drop built on the same account state reached LEZ and one applied; booked from LEZ (`resolvedLater`). Drop 2: forwarded with its answer held and the daemon SIGKILLed; restarted 30 s later, booked from the drop account's nonce with nothing sent again (1 send). Each claim's dry run claims exactly 1 pending credit of 50,000,000; 0 jobs refused |

Runs (Modal build box, profile of this stream's workspace): `cd cli && bash tests/relayer-crash.sh` at `3f3804f9`: `test
result: ok. 1 passed` in 136.57 s, parts 1–6 (`e2e/runs/relayer-crash/results.json`, README); `cd cli && cargo test
--lib` at `3f3804f9`: 31 passed (the drop journal's 10 tests, `logos_send`'s 8); `cd monitor && cargo test --lib
logos_send`: 8 passed; `bash e2e/netfault/relayer-lez.sh OUT`: 9 passed, 0 failed at `01b57e13` (recorded), again at
`8f99ec7f` after the second review's fixes (9 passed, 0 failed); `bash e2e/seqnet/run.sh robust robust-2`: 21 passed, 0 failed (box 852 s, 22 November
2026), the sources of `d8ddc000`; `bash e2e/seqnet/run.sh robust robust-3`: 22 passed, 0 failed (box 845 s, the run
02:49–03:01 UTC, 22 November 2026), the sources of `f42663ce`.

What the daemon does now (`cli/src/relayerd.rs`, `relayer.rs`, `logos_send.rs`): one Ethereum transaction in flight,
journaled before it is sent (`<state>/inflight.json`) and resolved first in every round (mined: booked; pending:
waited for; lost: the same call with the same nonce; a call the node refuses because it is done waits for its nonce to
be booked); through `tx_sender_module` the same, with the request's id recorded as soon as `send` answers, a killed
daemon's open request driven to its end rather than made again, and a request the module cannot account for (one it no
longer holds after a restart, `stuck`, a leg failed after it began to leave, a withdrawal it did not confirm) settled on
the chain at the request's nonce (from `send_status`, or the module's history row; no row: never broadcast) and, lost,
sent again through the module with that nonce pinned; reads pinned at or above the relayer's own last mined block
(`<state>/fence.json`); one daemon per state directory; root imports kept in flight until applied; **gas drops
journaled** (`<state>/drops.json`: the drop account's private nonce recorded before the credit, one drop at a time, the
job kept until LEZ shows the nonce moved; a credit not applied within `--drop-wait` sent again, built only on that
recorded state: its own sync finds a credit that landed meanwhile and sends nothing, so LEZ applies at most one; given
up after `--drop-max-sends` credits or once the deposit is claimed without it, `gas-drop-given-up`); endpoint failures
never refuse a job; malformed jobs (a viewing key that does not decode included) are refused, not retried; gas with
headroom; a wrapped seal reused after a restart.

**Limits of the relayer's part:** parts 5 and 6 use a stand-in for `tx_sender_module` that follows the module's code at
`f1754c3` for what the daemon calls; the real module or Logos Core restarted under the daemon, `ferrata_relayer_module`
itself killed, the statuses the stand-in never gives (`broadcasting`, `blocked`, `locked`), approval through
`signer_manager_module` and the module's broadcast to a node other than the daemon's are not run. One gas-drop case is
not told apart: a credit of a drop given up (claimed without it, or past `--drop-max-sends`) that a sequencer still
holds and applies after the give-up moves the drop account as the next drop's own credit would; that next drop is then
booked applied while its own credit is refused (same state), and its claimant's tool waits out `--gas-drop-wait` and can
ask again or pay from its own account. A give-up waits `--drop-wait` after the last send for that reason; nothing of a
user's is at risk (the drops are the relayer's money).

**Not shown, hence in part:** `monitor` under an RPC outage (row 43 shows it killed mid-window and restarted, with
and without its state file; no fault injected under it); the checkpoint prover (`e2e/crown` stage 7, a separate
process) and the attestors (`ferrata_attestor`) under RPC failure: no test; the relayer's root import under a fault on
a real sequencer (its four LEZ faults ran against the devnet's synchronous stand-in; robust-1, -2 and -3 import cleanly
on the sequencer). A node that lies is not a temporary failure (the crown's two-endpoint agreement, `e2e/NETWORK.md`).

Proposed EVIDENCE row: "`ferrata-relayer` killed at each of its transactions, at random, in the record's narrow
windows, through Logos Core's send route (its stand-in surviving the kills, then restarted, then `stuck`), and behind
failing endpoints on both chains (RFP-022 R.06): one Ethereum transaction in flight, journaled (mined, pending, or lost
and sent again with the same nonce; through `tx_sender_module`, a request the module cannot account for settled at its
nonce and sent again only with that nonce pinned), reads fenced at the relayer's own last block; root imports and gas
drops journaled on LEZ (a credit sent again built only on its recorded state) | parts 1–6: 48 relayer transactions for
48 effects, 0 reverted, one Release per nullifier, every job booked once; the lost answer, the held answer with a kill,
and the kill before the send each made to happen and resolved (`inflight-found` ×2, `inflight-lost`); the logos route
killed in its three windows: 3 requests for 3 jobs; the module restarted after a broadcast: no request after the
restart, booked from its history row and receipt; restarted before one: planned again once; `stuck` with the
transaction out: found under its nonce; `stuck` with nothing sent: sent again with the nonce pinned, one transaction;
faultnet's faults that fired as recorded; the LEZ side: each root imported once (9/9, devnet stand-in); gas drops on
LEZ v0.4.0-rc1's running sequencer: a credit trapped unsent with a kill, then a send answered 503 and forwarded 35 s
later, a third sent on the same account state: two credits of one drop reached the sequencer, the third applied, the
late one accepted and never applied; a credit forwarded with its answer held and a kill, booked from LEZ after the
restart with nothing sent again; each claim claims exactly one credit (robust-2 21/21, robust-3 22/22) | Modal build
box; anvil (31337), FerrataVault v5 mode (B), accept-all verifier; the devnet's zone (LEZ v0.4.0-rc1 harness); LEZ
v0.4.0-rc1's sequencer and node (gas drops) | none needed (not about proving); a stand-in for tx_sender_module in parts
5–6 | `cd cli && bash tests/relayer-crash.sh`; `bash e2e/netfault/relayer-lez.sh OUT`; `bash e2e/seqnet/run.sh robust
NAME` | `e2e/runs/relayer-crash/`, `e2e/runs/relayer-lez/`, `e2e/runs/seqnet-robust-2/`, `e2e/runs/seqnet-robust-3/`".

## U.3: long-running components as Logos module plus headless daemon, documented — proposed **in part** (narrower)

RFP: "Any long-running off-chain component the design requires must be provided as a Logos module accompanied by a
Logos Core headless CLI/daemon, runnable standalone, using LEZ module and Ethereum module for the respective chains
access, configurable finality depth, structured logging, and a clean shutdown path. Document the operator journey
end-to-end: install, configure, run, monitor."

**New:** the relayer daemon's root import on a running LEZ sequencer (the remainder the status named):
`e2e/runs/seqnet-robust-1/` (and again in `-2/`): crown-b `--operator-only yes --import-roots no` (no ImportRoot), then
`ferrata-relayer --sequencer --import-roots` (its own LEZ wallet; a Gloas record: the block's header rebuilt from
`eth_getBlockByHash`, keccak-checked) imported root(cid, 2) on the running sequencer, applied two rounds after it was
sent; SIGTERM gave its `stop` line; nothing left in flight. With rows 43–48, 96, 106–107 and 120.

**Still missing (exact):**
1. `ferrata_relayer_module` links `cli/` without `lez-wallet` (`modules/ferrata_relayer_module/rust-lib/calls/Cargo.toml`;
   `--sequencer` is refused there, `cli/src/relayerd.rs`, `fn sequencer` without the feature): the module reaches LEZ
   only through the devnet's stand-in. The root import on a sequencer is shown for the standalone binary only. The way
   exists (`ferrata_updater_module` links LEZ's node-side crates, staging the circuits and rapidsnark libraries:
   `modules/scripts/stage-updater.sh`, row 104); not done for the relayer module.
2. Chain access through Logos Core's LEZ and Ethereum modules: the relayer's Ethereum transactions go through
   `tx_sender_module` (row 120, anvil; now journaled, R.6), its Ethereum reads through plain JSON-RPC, its LEZ access
   through LEZ's wallet library (or the devnet stand-in), not `lez_core`, which serves no zone history for a sequencer
   it does not run (`docs/not-built.md`).
3. `monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs (traceability's U.3 remainder): not built.
4. The operator journey: `docs/guides/operator-guide.md` is a first draft (its header). Its gas-drop text (§6.1,
   §6.2, §6.4) now matches the daemon on this branch (the second review's L1: it had told operators to remove a failed
   drop's entry by hand, which with the journal can pay twice).

## R.7: an interrupted user-side operation does not consume, corrupt, or expose the entitlement — proposed **shown**

**Runs:** `e2e/runs/seqnet-robust-1/` (14 of 14, commit `2c418019`, 21 November 2026), `e2e/runs/seqnet-robust-2/`
(21 of 21, commit `d8ddc000`, 22 November 2026) and `e2e/runs/seqnet-robust-3/` (22 of 22, commit `f42663ce`, 22
November 2026, after the second review's fixes), `bash e2e/seqnet/run.sh robust NAME` on a Modal build box:
LEZ v0.4.0-rc1's own sequencer and node (one validator, dev-mode leader proofs), dev-mode private proofs, a local anvil
with the vault tests' mock verifier (what the row needs is the tool's and the daemons' behaviour, not proofs).

- *A claim killed between its proof and its sending* (every run): `ferrata claim --send --yes` with its sequencer
  endpoint behind `e2e/netfault/holdproxy.py`, which traps the claim's `sendTransaction`; SIGKILL. The zone's private
  transactions unchanged; `ferrata recover` from the seed still finds the deposit claimable; the same command again
  claims it once (recover: 2 claimed), reusing the one-time account and its credit (no second job).
- *A burn killed after its sending* (every run): the proxy holds the burn's answer; SIGKILL; the burn applied once; the
  same command again finds it on the zone through the tool's interrupted-burn record and burns nothing more; its release
  through the relayer pays the fresh address.
- *A burn killed before it reached the zone, then made with other parameters* (robust-2 and -3, the first review's L5): the burn's
  `sendTransaction` trapped unforwarded; SIGKILL; the zone unchanged; the burn asked again with **another recipient**
  takes the interrupted burn's counter (so at most one of the two can ever be applied) and is applied once; the record
  cleared. (robust-2: the zone 8 → 8 private transactions after the kill, 8 → 9 after the burn with another recipient; counter 1 taken; the record cleared)
- *A gas drop across kills* (every run): the killed claim's one-time account and pending credit reused; robust-2 and -3
  also kill the relayer's drops daemon twice around its credits, and in robust-3 two credits of one drop reach the
  sequencer (R.6 above): each claim's dry run claims exactly one pending credit.

With row 126 (a view closed during a proof, in Basecamp) this covers the row. The interrupted-burn record is the
`ferrata` tool's (`<LEZ wallet home>/ferrata-pending-burn.json`); the mini-app's burn keeps none (its view closed during
a proof is row 126's case).

**Proposed EVIDENCE row:** "Interrupted user commands on LEZ v0.4.0-rc1's running sequencer, the `ferrata` tool (RFP-022
R.07): a claim killed after its proof with its send trapped (nothing reached the zone; still claimable; then claimed
once); a burn killed after its send (applied once; the same command burns nothing more); a burn killed before it
reached the zone, then asked with another recipient (it takes the interrupted burn's counter: at most one of the two can
be applied; applied once); gas drops across kills of the claim and of the relayer's daemon (each claim claims exactly
one credit) | robust-1 14 of 14; robust-2 21 of 21; robust-3 22 of 22 | Modal build box (CPU); LEZ v0.4.0-rc1 sequencer and node,
one validator, dev-mode leader proofs | dev-mode private proofs; mock verifier; local anvil; a stand-in beacon committee
| `bash e2e/seqnet/run.sh robust NAME` | `e2e/runs/seqnet-robust-1/`, `e2e/runs/seqnet-robust-2/`,
`e2e/runs/seqnet-robust-3/`, `e2e/seqnet/hook-robust.sh`"

## U.5: the fee-payer mitigation in the relayer daemon and the tool — stays **in part**, gap narrowed

**New (commits `2c418019`, `8d329dcb`, `b289dcb4`, `09eef7ab`; runs robust-1, robust-2 and robust-3):** the relayer daemon's gas drop is the
sealed credit of row 124, not a public transfer: a `gas-drop` job carries the receipt and the one-time account's
public keys, which must derive the receipt's one-time address (`KeysMismatch`, checked before anything is recorded:
`cli/tests/gasdrop.rs`); the credit goes from the relayer's private account (`--drop-from-private`, default `/0`)
sealed to that account, left pending; the drop is journaled (R.6: a credit sent again is built only on its recorded
state; given up after `--drop-max-sends` credits). `ferrata claim` with `--gas-drop-inbox` (the
relayer's inbox; `FERRATA_GAS_DROP_INBOX`) and `--relayer` pays a private claim's fee from that drop by default: a new
child of private account `/0` as the one-time account, recorded per `N` beside LEZ's wallet with one job per relayer
asked (`ferrata-gas-drops.json`), the receipt proven over the claim's witness, the job written to the inbox, the credit
awaited, the claim sent with the one-time account as fee payer after LEZ's wallet's dry run (it claims the credit).
Another relayer gets a receipt of its own for the same one-time account (robust-2: a claim first naming a relayer that
never answers, then the real one). The dry run names the fee payer and what it leaks; `--no-gas-drop` pays from the
proving account. Runs: robust-1, 2 drops (credit 50,000,000; fee at head 6,188,624 / 6,223,304); robust-2 2 drops sealed, each claim's dry run claims exactly 1 pending credit of 50,000,000; drop 1 sent 3 times across a kill and a 503 and applied once, drop 2 booked from LEZ after a kill (R.6); the silent relayer first, then the real one with a receipt of its own; robust-3 the same, drop 1's 503'd credit reaching the sequencer late beside the third (one applied, each claim's dry run claims exactly 1 pending credit).

**Still missing for shown:** (1) the mini-app: `ferrata_bridge_module`'s claim does not ask for a drop (its runs use
the stand-in, rows 102, 103, 125, 126); (2) burns: the burn's fee still comes from `seqnet-fund` (burns with a drop are
not built; the design says burns use the same drop); (3) the job's handover is a directory shared by the user and the
relayer (`--gas-drop-inbox DIR`): there is no network handover to a third-party relayer; (4) "by default" holds once a
relayer's inbox is configured; a tool with no relayer configured pays from the proving account, as the dry run says;
(5) this path ran with dev-mode proofs on rc1 (row 124 proved the same sealed credit and sweep for real on rc3, by
crown-driver, not by the daemon).

**Proposed EVIDENCE row:** "The gas drop in the relayer daemon and the `ferrata` tool (RFP-022 U.05, the fee payer): a
private claim's LEZ fee paid by a one-time private account from the relayer daemon's credit sealed to it (the receipt
and the account's public keys handed to the relayer's inbox; the drop journaled on the relayer's side; another relayer
asked with a receipt of its own for the same account) | robust-1: 2 drops sealed, both claims' dry runs claim the credit
(1 pending credit of 50,000,000 each; fee at head 6,188,624 and 6,223,304); robust-2: 2 drops sealed, each claim's dry run claims exactly 1 pending credit of 50,000,000; drop 1 sent 3 times across a kill and a 503 and applied once, drop 2 booked from LEZ after a kill (R.6); the silent relayer first, then the real one with a receipt of its own; robust-3 the same, with two credits of drop 1 at the sequencer and one applied | Modal build box;
LEZ v0.4.0-rc1 sequencer and node | dev-mode receipts and private proofs (row 124: real proofs on rc3, by crown-driver)
| `bash e2e/seqnet/run.sh robust NAME` | `e2e/runs/seqnet-robust-1/`, `e2e/runs/seqnet-robust-2/`, `e2e/runs/seqnet-robust-3/`"

## F.7: release on native verification of a valid burn, validity with more than one sequencer key — stays **in part**, gap narrowed

New: LEZ v0.4.0-rc3 runs upstream's four-sequencer devnet (`lez/configs/docker-all-in-one/devnet`, unchanged since
rc1) with its own node 0.3.5-rc.1: `checkpoint/attestor/zone.sh` (now with `ZONE_LEZ`, `ZONE_BEDROCK=bundle`) with
crown-rc3's bundle.

Run (`e2e/runs/zone-rc3-four-keys/`, 1,080 s, commit `fe05ed9a`): four keys accredited (configuration threshold 3,
posting timeframe 60 slots = 240 s at rc3's 4 s slots), the turn rotating in their order, every key inscribing: 93
inscriptions in the node's final blocks, 24/23/23/23 (`inscriptions.json`, every inscription's slot and signer). In
the last ~2 minutes the channel's tip stalled after one full rotation.

**The stall, traced** (`e2e/runs/zone-rc3-four-keys/STALL.md` with `stall-excerpts.txt`, commits `9c21e580`,
`29863cb6`; from the run's logs and the node code LEZ rc3 pins, Logos Blockchain `f6d4267f`): seq-1 published a block
94 in the last slot of its turn (slot 2447818, too late to be mined in it), seq-0 a different block 94 in the first
slot of its own (2447819); the node's leader removed one transaction at slot 2447819 and one at 2447820 (an epoch
boundary) and mined neither; seq-0 then built blocks 95…105 on its own unmined inscription, each removed in turn, so
the tip stayed at slot 2447817. By the pinned code seq-0's inscription passed both channel checks (parent = tip,
signer = the round-robin key 0), so another check of its transaction removed it. **Unknown:** which one (logged only at
TRACE; candidates: the four sequencers' shared funding key, `zone.sh` gives them one; or LEZ issue 1109's storage gas
step at the epoch), and whether the posting timeout (25 slots) would have handed the turn to a healthy sequencer: the
run stopped 12 slots before slot 2447867, where it would have. Upstream's zone SDK changed right after `f6d4267f` in
this area (missing inputs, lineage sheds; commit subjects in `STALL.md`). A 300 s run sees only two turns (the turn is 240 s on rc3), which is why the earlier 300 s rc1 runs saw all
four keys and a 300 s rc3 run does not.

Still missing for **shown**: the bridge program on an rc3 multi-key zone, a t-of-n `checkpoint_b_0_3_5` proof over its
blocks with an attestor beside each sequencer, and the releases through RISC Zero's verifier (rows 100–101 did exactly
this on rc1). The pieces exist for rc3 with one key (rows 129–130, 142); joining them is `zone-bridge.sh` with rc3's
crown-driver, `realblocks-crown` with the `bedrock-0-3-5` framing and the `checkpoint_b_0_3_5` guest. Validity rests on
t of the zone's n sequencer keys either way (`docs/trust.md` §3.2), as the matrix states. Not attempted in this stream
(budget); a run of it must expect the stall above (end the
checkpoint's window before a stall, or wait out the posting timeout, or give each sequencer its own funding key).

**Proposed EVIDENCE row:** "A zone of four LEZ v0.4.0-rc3 sequencers on Logos Blockchain 0.3.5-rc.1 (RFP-022 F.07,
validity with more than one sequencer key): upstream's four-sequencer devnet run without Docker, four keys accredited
(threshold 3, posting timeframe 60 slots), the turn rotating, every key inscribing; its end-of-run stall traced from the
logs and the pinned node code | 93 inscriptions in the node's final blocks, 24/23/23/23; the stall: two block-94
inscriptions across a turn boundary, both removed by the node, the next key's lineage built on its unmined one; the
removing check unknown (TRACE only) | Modal build box, 1,080 s, one validator, dev-mode leader proofs | no bridge, no
checkpoint or release on this zone | `ZONE_BEDROCK=bundle bash checkpoint/attestor/zone.sh <crown-rc3 bundle> OUT
1080` | `e2e/runs/zone-rc3-four-keys/` (README, STALL.md)"

## Main statements this branch makes stale (for the integrator)

Quoted from `README.md` ("Specified but not built") and `docs/traceability.md` as they stand at `f13faee2` (line
numbers move on `main`):

- README, the **Daemons** item: "the relayer's root imports on a running LEZ sequencer": now run by the standalone
  `ferrata-relayer` (robust-1, -2, -3); remaining there: the same through `ferrata_relayer_module`, and `monitor`'s LEZ
  and Logos Blockchain clients.
- README, the **Logos Blockchain 0.3.5-rc.1** item: "a zone of several sequencers on it": now run (rc3 four-key zone,
  F.7 above); remaining: the bridge and a t-of-n checkpoint on it.
- README, the **From the v3 specification** item: "the gas drop's sealed credit in the relayer daemon": now built and
  run (U.5 above); "and for burns" stays.
- `docs/traceability.md` U.5 ("The sealed credit is not yet in the relayer daemon"), R.6 (RFP-022: "A relayer killed in
  the middle of a checkpoint or a release, and an RPC failure under it: none yet"), U.3 ("Remains: the relayer's root
  imports on a running LEZ sequencer"): as above.
- `docs/guides/operator-guide.md` §6.1, §6.2, §6.4 (the gas drop): updated on this branch (a docs file, changed for
  the second review's L1); its "First draft" header stays.

## Commits

The stream's first round: `dfd4e2e4` U.08 freezes in the tool, the relayer's in-flight journal, fence and lock;
`389dfc6e` U.08 run recorded; `01b57e13` R.06 tests and records, the fixes the tests found; `c7f1b352` the seqnet robust
mode, the interrupted-burn record, crown-b `--import-roots no`, `seqnet-fund --public-only`; `fe05ed9a` the rc3 four-key
zone, zone.sh's overrides; `efcfc2c8` the first hand-back; `d674560b` the root import under a Gloas record; `9c21e580`,
`29863cb6`, `14af8de1` the rc3 stall traced; `2c418019` U.05 in the daemon and the tool, the wallet sync before private
sends; `2ce94555` robust-1 recorded; `e2b1b564`, `c457099d` rows file and cli README.

After the first review: `e57f23b8` M1, L1, L2 (no workspace name or agent path; the driver's comment; the rc3 bundle's
provenance); `8d329dcb` gas drops journaled and resumed (H2), malformed jobs refused (L3), a finalised candidate booked
from its event (L4), gas drops refused under dev mode unless asked for on a local devnet (L10); `b289dcb4` the claim's
record per relayer (M3); `516e397a` the logos route journaled (H2), crash test parts 4 (M2) and 5, the test's scratch
in `target/` (L8); `d8ddc000` robust-2 recorded (with the hook, holdproxy's `fail` mode, robust-1's drops log filtered:
L12) and the crash record; `55351870` the relayer-lez case-D wait and this file.

After the second review: `09eef7ab` M1 (a gas drop sent again built only on its recorded state; the review's race as a
regression test), L4 (`--drop-max-sends`, `gas-drop-given-up`), L6 (an undecodable viewing key refused), a drop left in
flight past `--max-job-age` no longer holding later drops back; `3f3804f9` M2 (what `tx_sender_module` cannot account
for settled on the chain, the pinned resend, crash part 6, the stand-in's history, pins and `stuck` modes) and L7 (the
call's tag in the purpose line); `89f7c413` the crash record at `3f3804f9` (L10); `f42663ce` robust: the late credit;
`1a1eff55` operator guide and cli README (L1, L2); `25f57234` robust-1's note (L9); `8f99ec7f` robust-3 recorded;
`12b775c9` clippy (L11: clean but for one line of `cli/examples/miniapp_devnet.rs`, not this branch's file); then this
file (L3, L8).

Files of other streams changed, minimal and default-preserving: `lez/crown/driver/src/{main,crown}.rs`
(`--import-roots no`; a comment), `checkpoint/attestor/zone.sh` (`ZONE_LEZ`, `ZONE_BEDROCK`), `e2e/seqnet/{run,drive}.sh`
(the `robust` mode; `run.sh build` also builds the gas-drop guest; `drive.sh` also scrubs `~/` paths),
`e2e/seqnet/README.md` (the robust mode), `monitor/src/logos_send.rs` (kept identical to `cli/src/logos_send.rs`: `send`
split into `open` and `drive`, plus `live` and `history_row`; `Outcome::Unknown` carries whether the module forgot the
request and the leg's nonce; a request the module no longer holds is `Unknown`, no longer `NotSent`; the request's
purpose line carries the call's tag; its `logos_send` tests 8 passed), `monitor/src/chain.rs` (one match arm, for
`Outcome::Unknown`'s fields; the monitor's behaviour otherwise unchanged), `docs/guides/operator-guide.md` (§6.1,
§6.2, §6.4). `cli/src/{lez,flow,main}.rs` (the tool's claim) are this stream's `cli/` bridge commands.
