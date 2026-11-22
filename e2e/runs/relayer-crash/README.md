# ferrata-relayer killed, killed at random, and cut off (RFP-022 R.06)

**Run:** `cd cli && bash tests/relayer-crash.sh` (the test `relayer_killed_and_cut_off_does_each_job_once` of
[`cli/tests/relayer_crash.rs`](../../../cli/tests/relayer_crash.rs)), on a Modal build box (CPU), 22 November 2026
00:21–00:23 UTC, the sources of commit `516e397a` (branch `rfp-relayer-robustness`). **Result:** `test result: ok. 1
passed` in 121.75 s; [`results.json`](results.json) is the test's own summary. The root imports' counterpart, the LEZ
side failing under the relayer, is [`../relayer-lez/`](../relayer-lez/README.md); the gas drop's, on a running LEZ
sequencer, is [`../seqnet-robust-2/`](../seqnet-robust-2/robust.md).

**Where it ran:** a local anvil (chain id 31337, never a public network); FerrataVault v5 in mode (B) from
`vault/script/MonitorFixture.s.sol` with its accept-all verifier (R.06 is not about proving: no proof is checked
here); ETH registered with two denominations (0.1 ETH with a relay fee and no checkpoint fee; 1 ETH with a checkpoint
fee, which the relayer, having submitted the checkpoint, withdraws with `claimOwed`); 27 deposits escrowing what the
releases pay. The jobs are what a checkpoint prover and holders hand a relayer: a rule (B) checkpoint over a burn
root and 41 releases under it (journals encoded as the guests encode them). The daemon is the `ferrata-relayer`
binary signing with a keystore (parts 1–4) and, in part 5, the same daemon with `--send-via logos` and no key of its
own; each run's JSON lines were kept by the test.

## What it shows

| Part | What was done to the daemon | Result |
|---|---|---|
| 1, a kill at each kind of transaction | anvil's automine off: SIGKILL while its transaction waited in the pool, then **P** restarted with it still pending (mined after), **M** mined then restarted, **D** dropped from the pool then restarted: submitCandidate (P), finalizeAttested (M), a release (D), a release (P), a release with a checkpoint fee (M), the claimOwed after it (D) | 12 daemon runs; 6 transactions of the relayer for 6 effects, none reverted; 1 `CandidateSubmitted`, 1 `CheckpointFinalized`, one `Release` per nullifier, 1 `OwedClaimed`, owed 0 after; P: the restarted daemon logged `inflight-pending` and sent nothing until the block was mined; D: `inflight-lost`, then the same call sent again **with the same nonce**; every booking after a restart marked `"recovered": true` |
| 2, random kills | automine on, 20 releases in the inbox, SIGKILL 0 to 40 ms after each start-up line, started again until every job was booked | 2 kills were enough this time (a job takes milliseconds); 26 transactions for 26 effects (cumulative), none reverted; one `Release` per nullifier; every job in `done/` once, none refused. Where random kills land is chance: the two narrow windows are made to happen in part 4 |
| 3, flaky endpoint | `e2e/netfault/faultnet.py` seed 11, the daemon on its `flaky` endpoint, 4 releases | done in 2 s; **faults that fired:** 3 × 429, 1 × 5xx, 4 receipts behind (the endpoint's other drawn faults, late answers and a head going backwards, did not fire in this run); 30 transactions for 30 effects, none reverted |
| 3, sends endpoint | seed 16, the `sends` endpoint and its outages, 4 releases | done in 73 s; **fired:** 1 raw transaction dropped with its hash answered, 1 duplicated, 2 "not found", 4 receipts behind, 118 answers refused during outages (a send forwarded with its answer lost did not fire here: part 4 makes it happen); 34 transactions for 34 effects, none reverted |
| 3, lagging endpoint | seed 12, the `lagging` endpoint (4 blocks behind), a block mined every second, 4 releases | done in 20 s; **fired:** 29 receipts behind, 4 × 429; 3 rounds waited as `node-behind` (the node below the relayer's own last transaction); 38 transactions for 38 effects, none reverted |
| 4, the record's narrow windows, made to happen | `e2e/netfault/holdproxy.py` in front of the anvil (automine on), one release each: **E1** the send's answer lost (forwarded, connection closed), no kill; **E2** the send forwarded with its answer held and the daemon SIGKILLed, restarted on the anvil; **E3** the send trapped unforwarded and the daemon SIGKILLed with its record written (no hash; nothing in the pool), restarted | 3 faults fired; E1, E2: the transaction mined under the recorded nonce with no hash known, found in the blocks since the record (`inflight-found`) and booked once; E3: `inflight-lost`, the same call sent again with the recorded nonce (40), booked once; 41 transactions for 41 effects, none reverted |
| 5, the `--send-via logos` route killed in each window | `cli/examples/relayer_logos_stub.rs` (the daemon with Logos Core's module call replaced by a client of `e2e/netfault/txsender_stub.py`, a stand-in for `tx_sender_module` over the anvil; no key in the daemon, so no plain fallback), one release each: the daemon SIGKILLed **after its request was made, before any broadcast** (`send_status` stalled); **after the request was made, before its id reached the daemon** (`send`'s answer held); **after the broadcast, before its hash reached the daemon** (`send_status`'s answer held) | each restart found its request (by its recorded id; in the second case among `live_sends`: `inflight-request-found`) and drove it to its end (`inflight-request-driven`): **3 requests made for 3 jobs**, each booked once; 44 transactions for 44 effects, none reverted |

Checked after every part (the test fails otherwise): one `CandidateSubmitted` and one `CheckpointFinalized`; one
`Release` per nullifier and every nullifier used; every job in `done/`, none in `refused/` or left in the inbox; each
with at least one `job-done` line, all naming one transaction; the relayer's nonce equal to the number of effects,
nothing pending; every transaction of the relayer in the chain mined with status 1.

## What the daemon does for it (`cli/src/relayerd.rs`, `cli/src/relayer.rs`, `cli/src/eth.rs`, `cli/src/logos_send.rs`)

- **One transaction in flight, journaled.** `<state>/inflight.json` (what the transaction does, the inbox job it books,
  its calldata, its nonce; its hash once the node has it) is written before the send and removed only once the effect
  is booked (the job moved to `done/`, its line written). Each round first resolves a record left by a killed or
  cut-off daemon: mined, its effect is booked (`"recovered": true`); pending, the round waits; lost, the same call is
  sent again with the same nonce, so at most one of the two can be mined; a spent nonce without a receipt for the
  recorded hash is looked up in the blocks since the record (sender, nonce and calldata).
- **The same through `tx_sender_module`** (`--send-via logos`): the record before the request, the request's id as
  soon as `send` answers, the hash once `send_status` broadcast it. `tx_sender_module` broadcasts only when polled, so
  a restarted daemon drives its open request to its end (by id, or found among `live_sends` by its purpose) instead of
  making a second one; a request that ended with nothing sent is planned again.
- **A fence against stale nodes.** `<state>/fence.json` keeps the block of the relayer's last mined transaction and
  the nonce after it; a round reads at one block at or above it (waiting while the node is behind) and no nonce
  below it is used, whatever a lagging node answers.
- **Endpoint failures never refuse a job.** A job is refused for good only on a refusal (the relayer's or the
  vault's, or a malformed job); a rate limit, a 5xx, a closed connection or a node behind keeps it in the inbox
  (found by this test: a 429 had refused a release before). Single reads are retried on transient errors
  (`eth::retry_rpc`).
- **Gas with headroom** (the estimate plus 30 % and 20,000; found by this test: a candidate estimated at the pinned
  block ran out of gas when mined later). **One daemon per state directory** (`<state>/lock`). **The endpoint is
  waited for at start** (`--startup-wait`). A **wrapped seal is reused** after a restart (no second Groth16 wrap).

## What it does not show

- Part 5's `tx_sender_module` is a stand-in that follows the module's documented behaviour (`send` reserves and
  answers a request id, `send_status` is the broadcast, `live_sends` lists open requests); the real module in a
  headless Logos Core sent the daemon's transactions on anvil (EVIDENCE row 120) but was not killed there.
- The gas drop (the LEZ side of the relayer's work) is shown on a running sequencer, not here:
  [`../seqnet-robust-2/`](../seqnet-robust-2/robust.md) and the unit tests `relayer::tests` (a credit that failed, a
  kill before and after the send, one drop at a time).
- A node that lies (answers wrong state that looks right) is not a temporary failure and is outside this test: the
  daemon trusts its own endpoint (the crown's scripts' two-endpoint agreement against lying endpoints is
  [`../../NETWORK.md`](../../NETWORK.md)).
