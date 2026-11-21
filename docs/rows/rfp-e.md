# Stream rfp-E: relayer, monitor and client robustness (RFP-022)

Proposals for the integrator; this stream does not edit `requirements-status.md`, `traceability.md`, `EVIDENCE.md`
or the README's status lines. Branch `rfp-relayer-robustness`, from `f13faee2`; commits in the last section.

Where everything ran: a Modal build box (CPU), 21 November 2026. Ethereum: local anvil only (nothing sent to Sepolia
or mainnet, no public endpoint read). LEZ: the v0.4.0-rc1 in-process harness, the mini-app's devnet (the harness
behind HTTP), LEZ v0.4.0-rc1's own sequencer and node (e2e/seqnet), and LEZ v0.4.0-rc3's own sequencer and node
0.3.5-rc.1 (upstream's four-sequencer devnet). None of R.06, R.07, U.03, U.08 is about proving: dev-mode receipts and
mock or accept-all verifiers are used there and named. No program, guest or image id changed; no deployment is
affected.

## Summary

| Row | Today | Proposed | Shown by |
|---|---|---|---|
| U.8 | in part | **shown** | `cli/tests/e2e.rs`, run recorded in `e2e/runs/cli-freezes/` |
| R.6 | in part | **shown** | `cli/tests/relayer_crash.rs` (`e2e/runs/relayer-crash/`), `e2e/netfault/relayer-lez.sh` (`e2e/runs/relayer-lez/`), with row 43's monitor restarts |
| U.3 | in part | see below (the seqnet robust run) | `e2e/seqnet/run.sh robust` (`e2e/runs/seqnet-robust-*`) |
| R.7 | in part | see below (the seqnet robust run) | `e2e/seqnet/run.sh robust`, with row 126 |
| U.5 | in part | in part (unchanged) | the gas drop is still not inside the relayer daemon; the plan is in the report |
| F.7 | in part | in part (narrower gap) | rc3 runs a four-key zone (`checkpoint/attestor/zone.sh` with `ZONE_BEDROCK=bundle`); the bridge and a t-of-n checkpoint over an rc3 multi-key zone are not run |

## U.8: actionable errors, including program or per-token frozen — proposed **shown**

RFP: "Return clear, actionable error messages for all failure modes: unsupported token, invalid amount, cap exceeded,
verification failure, insufficient finality, already claimed, and program or per-token frozen. Error messages must
not reveal which deposit or burn a failed attempt referred to."

What was missing: the freeze refusal was exercised in the mini-app only. Now, in the tool:

| Clause | Test | Result |
|---|---|---|
| frozen, program-wide and per token, deposit side | `cli/tests/e2e.rs` (`deposit_scan_claim_burn_release_and_recovery`): the vault's freeze authority `setDepositsFrozen`, then `setTokenDepositsFrozen(ETH)`; `ferrata deposit` (the binary) | `refused: vault error DepositsAreFrozen() (checked before sending: frozen until Ethereum time … (UTC date), unless the freeze authority lifts it sooner): deposits are frozen for the whole vault; nothing is sent: retry once the freeze ends (`ferrata status` shows it)`, and `TokenDepositsAreFrozen()` likewise; lifted, the dry run passes |
| frozen, claims (mints), whole configuration and token | the bridge program's freeza authority `Freeze` (token, then none); `claim` on the plaintext path, then the private path before its proof | `refused: LEZ bridge program exit code 242 FROZEN (checked before sending: mints (claims) of this token are frozen until Ethereum time …; nothing is sent and nothing is consumed: the deposit stays claimable): … retry once the freeze ends`; lifted, the same claim settles |
| frozen, burns | `Freeze` of burns (token, then configuration); `burn` | `… 242 FROZEN (checked before sending: burns of this token are frozen until …; nothing is sent and nothing is burned: the holding stays yours) …`; lifted, the burn settles |
| frozen, releases | `setReleasesFrozen`, `setTokenReleasesFrozen(ETH)`; `release` | `refused: vault error ReleasesAreFrozen() (checked before sending: frozen until …): releases are frozen for the whole vault; the burn stays releasable: retry once the freeze ends …`; lifted, released |
| no deposit or burn named | every refusal above checked with the test's `names_nothing` (no run of 40+ hex digits, no leaf index, no burn counter) | 9 of 9 |
| the other listed failures | unchanged, EVIDENCE row 85 (21 refusals, same test file) | as before |

Run: `cd cli && bash tests/e2e.sh`, commit `dfd4e2e4`: `test result: ok. 1 passed` in 34.47 s; again at `c7f1b352`
after the R.07 changes: 1 passed in 36.78 s. Record: `e2e/runs/cli-freezes/` (README with every refusal verbatim,
`test.log`).

Proposed EVIDENCE row: "The `ferrata` tool under every freeze (RFP-022 U.08): the vault's deposit and release freezes
(whole vault, per token) and the bridge program's mint and burn freezes (whole configuration, per token) each refused
before anything is signed or proven, with its source and code, the freeze's end as a UTC date and what to do, naming
no deposit or burn; each lifted, the same command goes through | 9 freeze refusals in one run (deposit ×2, plaintext
claim ×2, burn ×2, release ×2, private claim ×1), all passing `names_nothing`; `test result: ok`, 34.47 s | Modal build
box; anvil (chain id 1), LEZ v0.4.0-rc1 harness | none (dev mode; mock verifier) | `cd cli && bash tests/e2e.sh` |
`e2e/runs/cli-freezes/`".

## R.6: temporary RPC or connectivity failure on either chain, recoverable, no work duplicated — proposed **shown**

RFP: "Temporary RPC or connectivity failure on either chain leaves any off-chain component in a recoverable state,
able to resume without duplicating work already done."

What was missing: "A relayer killed in the middle of a checkpoint or a release, and an RPC failure under it: none yet."

| Clause | Test | Result |
|---|---|---|
| the relayer killed mid-transaction, exactly-once effects | `cli/tests/relayer_crash.rs` part 1 (anvil automine off; SIGKILL with the transaction pending, then restarted with it pending (P), mined before the restart (M), dropped from the pool (D)): submitCandidate P, finalizeAttested M, release D, release P, release with checkpoint fee M, claimOwed D | 6 transactions for 6 effects, none reverted; 1 `CandidateSubmitted`, 1 `CheckpointFinalized`, one `Release` per nullifier, 1 `OwedClaimed`; P: `inflight-pending` and nothing sent until mined; D: the same call sent again with the same nonce |
| killed at any point | part 2: 20 releases queued, SIGKILL 0–40 ms after each start line until all booked | 26 transactions for 26 effects (cumulative), none reverted, every job once |
| Ethereum RPC failure | part 3: `e2e/netfault/faultnet.py` flaky (429, 5xx, late answers, head backwards), sends (dropped, duplicated, answer lost), lagging (4 blocks behind), with outages of every endpoint | all jobs done (2 s, 73 s, 20 s); 38 transactions for 38 effects, none reverted; faults seen recorded per part |
| LEZ connectivity failure | `e2e/netfault/relayer-lez.sh` (`e2e/netfault/holdproxy.py` in front of the devnet's zone): outage; ImportRoot applied but its answer lost; SIGKILL with the answer held; SIGKILL with the import held unsent | 9 of 9: each leaf count imported once; booked from the record (`recovered`), nothing sent again; the unsent one planned again after `--import-wait` and imported once |
| the monitor (the other long-running component) | EVIDENCE row 43 (S5, S6: SIGKILLed mid-window, restarted from its state file or with it lost, exactly one cancellation), unchanged | as before |

Runs: `cd cli && bash tests/relayer-crash.sh` at `01b57e13` (sources as committed): `test result: ok. 1 passed` in
117.85 s (`e2e/runs/relayer-crash/results.json`); `bash e2e/netfault/relayer-lez.sh OUT` at `01b57e13`: 9 passed, 0
failed (`e2e/runs/relayer-lez/`).

What the daemon does now (all in `cli/src/relayerd.rs`, `relayer.rs`, `eth.rs`): one transaction in flight,
journaled before it is sent (`<state>/inflight.json`) and resolved first in every round (mined: booked, pending:
waited for, lost: the same call with the same nonce); reads pinned at or above the relayer's own last mined block
(`<state>/fence.json`); one daemon per state directory; the start-up waits for the endpoint; root imports kept in
flight until applied; endpoint failures never refuse a job; gas with headroom; a wrapped seal reused after a restart.
Bugs the tests found and the fixes are in commit `01b57e13`.

Not covered, stated: the `--send-via logos` path (Logos Core's `tx_sender_module` keeps its own requests); the gas
drop stays at most once (recorded before its credit); a node that lies is not a temporary failure (the crown's
two-endpoint agreement covers it, `e2e/NETWORK.md`).

Proposed EVIDENCE row: "`ferrata-relayer` killed at each of its transactions, killed at random, and run behind failing
endpoints (RFP-022 R.06): a journal of the one transaction in flight (resolved first in every round: mined, pending,
or lost and sent again with the same nonce), reads fenced at the relayer's own last block, one daemon per state |
part 1: 6 kills (P, M, D) at submitCandidate, finalizeAttested, releases, claimOwed; part 2: 20 releases under kills
0–40 ms after start; part 3: faultnet flaky/sends/lagging with outages: 38 relayer transactions for 38 effects, 0
reverted, one Release per nullifier, every job booked once; the LEZ side: outage, lost answer, kill while held, kill
while unsent: each root imported once (9/9) | Modal build box; anvil (31337), FerrataVault v5 mode (B), accept-all
verifier; the devnet's zone (LEZ v0.4.0-rc1 harness) | none needed (not about proving) | `cd cli && bash
tests/relayer-crash.sh`; `bash e2e/netfault/relayer-lez.sh OUT` | `e2e/runs/relayer-crash/`, `e2e/runs/relayer-lez/`".

## U.3 and R.7: the seqnet robust run

ROBUST_RUN_PLACEHOLDER

## U.5: the fee-payer mitigation inside the relayer daemon — stays **in part**

The gap is unchanged: the relayer daemon's credit (`impl Credit for lez::Sequencer`, `cli/src/relayer.rs`) is still a
public transfer to the one-time address, which no private claim can sweep; the sealed credit of EVIDENCE row 124 was
sent by crown-driver's `gas-drop-credit` (rc3), not by the daemon. What it takes (the report's next piece): the
GasDrop job carrying the one-time account's public keys (npk, vpk); the sealed credit from the relayer's private
account (`seal_destination` with `PrivateForeign`, `send_tx_with_pre_check`: present in LEZ v0.4.0-rc1's wallet, which
the daemon links); in the tool, a one-time private account from the seed, the receipt, the job, and the claim with the
one-time account as fee payer (`send_privacy_preserving_tx_with_pre_check(…, Some(one_time), …)`), on by default when a
relayer is configured; run in `e2e/seqnet` (rc1) in place of `seqnet-fund`.

## F.7: release on native verification of a valid burn, validity with more than one sequencer key — stays **in part**, gap narrowed

New: LEZ v0.4.0-rc3 runs upstream's four-sequencer devnet (`lez/configs/docker-all-in-one/devnet`, unchanged since
rc1) with its own node 0.3.5-rc.1: `checkpoint/attestor/zone.sh` (now with `ZONE_LEZ`, `ZONE_BEDROCK=bundle`) with
crown-rc3's bundle.

RC3_ZONE_PLACEHOLDER

Still missing for **shown**: the bridge program on an rc3 multi-key zone, a t-of-n `checkpoint_b_0_3_5` proof over its
blocks with an attestor beside each sequencer, and the releases through RISC Zero's verifier (rows 100–101 did exactly
this on rc1). The pieces exist for rc3 with one key (rows 129–130, 142); joining them is `zone-bridge.sh` with rc3's
crown-driver, `realblocks-crown` with the `bedrock-0-3-5` framing and the `checkpoint_b_0_3_5` guest. Validity rests on
t of the zone's n sequencer keys either way (`docs/trust.md` §3.2), as the matrix states.

## Commits

COMMITS_PLACEHOLDER
