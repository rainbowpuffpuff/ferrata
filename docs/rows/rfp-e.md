# Stream rfp-E: relayer, monitor and client robustness (RFP-022)

Proposals for the integrator; this stream does not edit `requirements-status.md`, `traceability.md`, `EVIDENCE.md`
or the README's status lines. Branch `rfp-relayer-robustness`, from `f13faee2`; commits in the last section.

Where everything ran: a Modal build box (CPU), 21 November 2026 (two agents of this stream; the second added U.3's
and R.7's run, U.5's code and run, and F.7's stall analysis). Ethereum: local anvil only (nothing sent to Sepolia
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
| U.3 | in part | **shown** (the remainder the status names), see the caveat below | `e2e/runs/seqnet-robust-1/` (14/14), with rows 43–48, 96, 106–107, 120 |
| R.7 | in part | **shown** | `e2e/runs/seqnet-robust-1/` (claim killed before sending, burn killed after it, a gas drop across a kill), with row 126 |
| U.5 | in part | in part, gap narrowed | the gas drop is now inside the relayer daemon and the tool (`e2e/runs/seqnet-robust-1/`); the mini-app's claim, burns and real proofs of this path remain |
| F.7 | in part | in part (narrower gap) | rc3 runs a four-key zone (`zone-rc3-four-keys`); its end-of-run stall traced (`STALL.md`); the bridge and a t-of-n checkpoint over an rc3 multi-key zone are not run |

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

## U.3 and R.7: the seqnet robust run — proposed **shown** (U.3 with one caveat)

**Run:** `e2e/runs/seqnet-robust-1/` (`robust.md`: 14 passed, 0 failed; `robust.json`; `run.log`; `user/`: every
command the user ran and what it printed), commit `2c418019`, a Modal build box, 21 November 2026
21:18–21:27 UTC: `cd cli && cargo test --tests --no-run && cargo test --lib relayer::tests && env -u RISC0_DEV_MODE
cargo test --test gasdrop` (RISC0_HOME as in `lez/RESULTS.md`), then `bash e2e/seqnet/run.sh robust robust-1`
(alone on the box: the script first checks that ports 18080, 3040 and 8545 are free). LEZ v0.4.0-rc1's own sequencer
and node (one validator, dev-mode leader proofs), dev-mode private proofs, a local anvil with the vault tests' mock
verifier: what the row needs here is the daemons' and the tool's behaviour, not proofs.

Getting there found and fixed two defects (both in `2c418019`/`d674560b`, failed records kept beside the stream's
scratch, not committed):

1. **The relayer could not import under a Gloas record** (the stand-in light client's records, and Sepolia's since its
   Gloas fork, bind the execution block's hash only): the daemon waited for ever ("this relayer does not fetch the
   block's header yet"). Now it rebuilds the block's header from `eth_getBlockByHash`, checks its keccak against the
   record, and sends it in front of the account proof, as the programs read it (`d674560b`; unit test
   `relayer::tests::header_rlp_rebuilds_the_block_hash` on a mainnet block of 9 Nov 2026: its hash, a missing field
   refused, a changed field changing the hash).
2. **A second private claim from a new process was refused by the sequencer** ("Nullifier already seen", the
   sequencer's log): the tool did not sync LEZ's wallet before a private send, so it spent private account `/0`'s state
   from before the first claim. `Sequencer::submit` now syncs before every private transaction (`2c418019`).

**U.3, "Logos module plus headless daemon … using LEZ module and Ethereum module … clean shutdown"; the status's
remainder "the relayer's root imports on a running sequencer":** crown-b `--operator-only yes --import-roots no` (the
finality update, Init, Register; no ImportRoot), then `ferrata-relayer --sequencer --import-roots` (its own LEZ
wallet; its public account and private account `/0` funded by `seqnet-fund`) imported root(cid, 2) on the running
sequencer (tx `660dd6bd…`, applied, two rounds after it was sent); SIGTERM gave its `stop` line; nothing left in
flight. The gas-drop daemon (below) is the same binary, stopped the same way. With rows 43–48, 96, 106–107 and 120 this
covers the remainder the status names. **Caveat for the integrator:** `docs/traceability.md`'s U.3 row also lists
`monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs as remaining (README, "Specified but not built"); this
stream did not build it. If that is part of U.3's clause, U.3 stays **in part** with that item alone.

**R.7, "an interrupted user-side operation does not consume, corrupt, or expose the user's entitlement":**

- *A claim killed between its proof and its sending:* `ferrata claim --send --yes` with its sequencer endpoint behind
  `e2e/netfault/holdproxy.py`, which traps the claim's `sendTransaction` (built and proven by LEZ's wallet, never
  forwarded); SIGKILL. The zone held 2 private transactions before and after (the count taken at the trap, after the
  claim's gas drop had landed); `ferrata recover` from the seed still finds the deposit claimable; the same command
  again claims it once, and the second deposit after it (recover: 2 claimed). Nothing left the machine but the gas-drop
  job (a receipt and a one-time account's public keys).
- *A burn killed after its sending:* `ferrata burn` with the proxy holding its `sendTransaction`'s answer (the burn
  reached the sequencer, the tool never learned it); SIGKILL; the burn was applied once (6 → 7 private transactions);
  the same command again found that burn on the zone through the tool's interrupted-burn record
  (`<LEZ wallet home>/ferrata-pending-burn.json`: counter 0) and burnt nothing more (still 7); the release of that burn
  through the relayer paid 997000000000000 wei to the fresh address.
- *A gas drop across a kill:* the killed claim had made its one-time account and handed the relayer its job; the claim
  again reused that account and the credit already pending to it, and asked nothing new of the relayer ("no new job";
  the daemon sent exactly 2 drops for 2 deposits, 0 refused).

With row 126 (a view closed during a proof, in Basecamp) this covers the row.

**Proposed EVIDENCE row:** "The relayer daemon's root import, its gas drops and interrupted user commands on LEZ
v0.4.0-rc1's running sequencer: crown-b without ImportRoot, then `ferrata-relayer --sequencer --import-roots` imports
root(cid, 2) (Gloas record: the header rebuilt and checked); the daemon's two gas drops, each a native credit sealed to
the claim's one-time private account, and both claims paying their LEZ fee from it (LEZ's wallet's dry run: 1 pending
credit of 50,000,000 claimed to the one-time payer; fee at head 6,188,624 and 6,223,304); a claim killed after its
proof with its send trapped (nothing reached the zone; resumed: claimed once, the same one-time account, no second
drop); a burn killed after its send (applied once; the same command again burns nothing); release through the relayer.
14 of 14 checks | Modal build box (CPU), 21 Nov 2026; LEZ v0.4.0-rc1 sequencer and node, one validator, dev-mode leader
proofs | dev-mode private proofs; the vault's verifier a mock; local anvil; the beacon chain a stand-in committee |
`bash e2e/seqnet/run.sh robust NAME` | `e2e/runs/seqnet-robust-1/`, `e2e/seqnet/hook-robust.sh`, commit `2c418019`"

## U.5: the fee-payer mitigation in the relayer daemon and the tool — stays **in part**, gap narrowed

**New (commit `2c418019`, run above):** the relayer daemon's gas drop is the sealed credit of row 124, not a public
transfer: a `gas-drop` job carries the receipt and the one-time account's public keys (`oneTimeNpk`, `oneTimeVpk`), which
must derive the receipt's one-time address (`KeysMismatch`, checked before the drop is recorded:
`cli/tests/gasdrop.rs`, 1 passed on a real receipt); the credit goes from the relayer's private account
(`--drop-from-private`, default `/0`) sealed to that account (`seal_destination` with `PrivateForeign`, left pending).
`ferrata claim` with `--gas-drop-inbox` (the relayer's inbox; `FERRATA_GAS_DROP_INBOX`) and `--relayer` pays a private
claim's fee from that drop by default: a new child of private account `/0` as the one-time account, recorded per `N`
beside LEZ's wallet before the job is handed over (`ferrata-gas-drops.json`), the receipt proven over the claim's
witness, the job written to the inbox, the credit awaited, the claim sent with the one-time account as fee payer after
LEZ's wallet's dry run (it claims the credit). The dry run names the fee payer and what it leaks; `--no-gas-drop` pays
from the proving account. In the run: 2 drops, both sealed, both claims sweeping them (credit 50,000,000; fee at head
6,188,624 / 6,223,304; reserve 10,056,514 / 10,112,869; the credit pending 3 s and 9 s after the tool began asking; dev-mode receipts, proven in 0.0 s); the
gas-drop guest built on the box has `docs/image-ids.md`'s id `0x89580416…`.

**Still missing for shown:** (1) the mini-app: `ferrata_bridge_module`'s claim does not ask for a drop (its runs use the
stand-in, rows 102, 103, 125, 126); (2) burns: the burn's fee still comes from `seqnet-fund` (burns with a drop are
not built; the design says burns use the same drop); (3) "by default" holds once a relayer's inbox is configured
(`FERRATA_GAS_DROP_INBOX`); a tool with no relayer configured pays from the proving account, as the dry run says;
(4) this path ran with dev-mode proofs on rc1 (row 124 proved the same sealed credit and sweep for real on rc3, by
crown-driver, not by the daemon).

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
t of the zone's n sequencer keys either way (`docs/trust.md` §3.2), as the matrix states. Not attempted by the second
agent of this stream (budget): the plan is in the stream's report; a run of it must expect the stall above (end the
checkpoint's window before a stall, or wait out the posting timeout, or give each sequencer its own funding key).

## Commits

`dfd4e2e4` U.08 freezes in the tool, the relayer's in-flight journal, fence and lock; `389dfc6e` U.08 run recorded;
`01b57e13` R.06 tests and records, the fixes the tests found; `c7f1b352` the seqnet robust mode, the interrupted-burn
record, crown-b `--import-roots no`, `seqnet-fund --public-only`; `fe05ed9a` the rc3 four-key zone, zone.sh's
overrides, this file's draft; `efcfc2c8` the first hand-back. Second agent: `d674560b` the relayer's root import under
a Gloas record; `9c21e580`, `29863cb6` the rc3 stall traced; `2c418019` U.05 (the gas drop in the daemon and the tool)
and the wallet sync before private sends; `2ce94555` the robust run recorded (14/14), seqnet README, drive.sh's scrub
of `~/` paths; and this file's update after it.

Files of other streams changed, minimal and default-preserving: `lez/crown/driver/src/{main,crown}.rs`
(`--import-roots no`), `checkpoint/attestor/zone.sh` (`ZONE_LEZ`, `ZONE_BEDROCK`), `e2e/seqnet/{run,drive}.sh` (the
`robust` mode; `run.sh build` now also builds the gas-drop guest; `drive.sh` also scrubs `~/` paths),
`e2e/seqnet/README.md` (the robust mode's gas drop). `cli/src/{lez,flow,main}.rs` (the tool's claim) are this
stream's `cli/` bridge commands.
