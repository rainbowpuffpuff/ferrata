# ferrata-relayer killed, killed at random, and cut off (RFP-022 R.06)

**Run:** `cd cli && bash tests/relayer-crash.sh` (the test `relayer_killed_and_cut_off_does_each_job_once` of
[`cli/tests/relayer_crash.rs`](../../../cli/tests/relayer_crash.rs)), on a Modal build box (CPU), 22 November 2026
04:39–04:42 UTC, the sources of commit `8153e31b` (branch `rfp-relayer-robustness`; the run's files are that commit's,
byte for byte: part 6b now also requires the "no row" line, no `inflight-lost`/`inflight-resent` and no pinned request,
and 6d counts its pins from its own start, the third review's L-5). **Result:** `test result: ok. 1 passed` in
136.90 s; [`results.json`](results.json) is the test's own summary: 48 relayer transactions, 0 reverted, 45 releases,
part 6's requests 0, 1 (0 pinned), 1, 2 (1 pinned). Part 2's random kills differ between runs (34 daemon runs here, 36
in the record at `3f3804f9`, which this file's history keeps with the earlier ones: `41c0d843`, 126.04 s;
`516e397a`, parts 1–5). With the module's history not asked for a forgotten request (the third review's mutant M2C,
applied on the box only, then restored), the same test fails at part 6b: `no inflight-request-forgotten line with 'no
row: never broadcast'`. The root imports' counterpart, the LEZ
side failing under the relayer, is [`../relayer-lez/`](../relayer-lez/README.md); the gas drop's, on a running LEZ
sequencer, is [`../seqnet-robust-2/`](../seqnet-robust-2/robust.md).

**Where it ran:** a local anvil (chain id 31337, never a public network); FerrataVault v5 in mode (B) from
`vault/script/MonitorFixture.s.sol` with its accept-all verifier (R.06 is not about proving: no proof is checked
here); ETH registered with two denominations (0.1 ETH with a relay fee and no checkpoint fee; 1 ETH with a checkpoint
fee, which the relayer, having submitted the checkpoint, withdraws with `claimOwed`); 27 deposits escrowing what the
releases pay. The jobs are what a checkpoint prover and holders hand a relayer: a rule (B) checkpoint over a burn
root and 45 releases under it (journals encoded as the guests encode them). The daemon is the `ferrata-relayer`
binary signing with a keystore (parts 1–4) and, in parts 5 and 6, the same daemon with `--send-via logos` and no key of
its own; each run's JSON lines were kept by the test.

## What it shows

| Part | What was done to the daemon | Result |
|---|---|---|
| 1, a kill at each kind of transaction | anvil's automine off: SIGKILL while its transaction waited in the pool, then **P** restarted with it still pending (mined after), **M** mined then restarted, **D** dropped from the pool then restarted: submitCandidate (P), finalizeAttested (M), a release (D), a release (P), a release with a checkpoint fee (M), the claimOwed after it (D) | 12 daemon runs; 6 transactions of the relayer for 6 effects, none reverted; 1 `CandidateSubmitted`, 1 `CheckpointFinalized`, one `Release` per nullifier, 1 `OwedClaimed`, owed 0 after; P: the restarted daemon logged `inflight-pending` and sent nothing until the block was mined; D: `inflight-lost`, then the same call sent again **with the same nonce**; every booking after a restart marked `"recovered": true` |
| 2, random kills | automine on, 20 releases in the inbox, SIGKILL 0 to 40 ms after each start-up line, started again until every job was booked | 1 kill was enough this time (a job takes milliseconds; 3 in the record at `3f3804f9`); 26 transactions for 26 effects (cumulative), none reverted; one `Release` per nullifier; every job in `done/` once, none refused. Where random kills land is chance: the two narrow windows are made to happen in part 4 |
| 3, flaky endpoint | `e2e/netfault/faultnet.py` seed 11, the daemon on its `flaky` endpoint, 4 releases | done in 2 s; **faults that fired:** 3 × 429, 1 × 5xx, 4 receipts behind (drawn with a rate but not fired in this run: "not found" answers, timeouts and a head going backwards; `results.json` has the rates and `faultsSeen`); 30 transactions for 30 effects, none reverted |
| 3, sends endpoint | seed 16, the `sends` endpoint and its outages, 4 releases | done in 83 s; **fired:** 2 raw transactions dropped with their hash answered, 1 duplicated, 6 "not found", 4 receipts behind, 119 answers refused during outages (a send forwarded with its answer lost did not fire here: part 4 makes it happen); 34 transactions for 34 effects, none reverted |
| 3, lagging endpoint | seed 12, the `lagging` endpoint (4 blocks behind), a block mined every second, 4 releases | done in 20 s; **fired:** 29 receipts behind, 4 × 429; 3 rounds waited as `node-behind` (the node below the relayer's own last transaction); 38 transactions for 38 effects, none reverted |
| 4, the record's narrow windows, made to happen | `e2e/netfault/holdproxy.py` in front of the anvil (automine on), one release each: **E1** the send's answer lost (forwarded, connection closed), no kill; **E2** the send forwarded with its answer held and the daemon SIGKILLed, restarted on the anvil; **E3** the send trapped unforwarded and the daemon SIGKILLed with its record written (no hash; nothing in the pool), restarted | 3 faults fired; E1, E2: the transaction mined under the recorded nonce with no hash known, found in the blocks since the record (`inflight-found`) and booked once; E3: `inflight-lost`, the same call sent again with the recorded nonce (40), booked once; 41 transactions for 41 effects, none reverted |
| 5, the `--send-via logos` route killed in each window | `cli/examples/relayer_logos_stub.rs` (the daemon with Logos Core's module call replaced by a client of `e2e/netfault/txsender_stub.py`, a stand-in for `tx_sender_module` over the anvil; no key in the daemon, so no plain fallback), one release each: the daemon SIGKILLed **after its request was made, before any broadcast** (`send_status` stalled); **after the request was made, before its id reached the daemon** (`send`'s answer held); **after the broadcast, before its hash reached the daemon** (`send_status`'s answer held). **The stand-in survives each kill** (part 6 restarts it) | each restart found its request (by its recorded id; in the second case among `live_sends`, matched to the very call by the tag in its purpose: `inflight-request-found`) and drove it to its end (`inflight-request-driven`): **3 requests made for 3 jobs**, each booked once; 44 transactions for 44 effects, none reverted |
| 6, what `tx_sender_module` cannot account for (the second review's M2) | the stand-in with a durable history (`--history`, written before each broadcast, as the module's), one release each: **(a)** automine off, the broadcast done and its answer held, the daemon SIGKILLed, **the stand-in restarted** (its requests gone, its history kept), the daemon restarted, the transaction still pending; **(b)** the request made and nothing broadcast (`send_status` stalled), the daemon killed, the stand-in restarted; **(c)** `send_status` answers `stuck` (no hash) after it sent the transaction; **(d)** `stuck` with nothing sent | (a) `inflight-request-forgotten` (the request's history row: its nonce and hash), `inflight-pending` while the transaction waited, booked from its receipt once mined: **no request made after the restart**, nothing refused; (b) no history row (`inflight-request-forgotten`, `history: no row: never broadcast`), so nothing had left: the call planned again, **1 request, unpinned**, no `inflight-lost` or `inflight-resent`; (c) `inflight-request-unknown`, then found mined under the request's nonce (`inflight-found`): 1 request; (d) `inflight-request-unknown`, `inflight-lost`, then the same call through the stand-in **with the nonce pinned** (`inflight-resent`): 2 requests, 1 transaction; 48 transactions for 48 effects, none reverted |

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
  a restarted daemon drives its open request to its end (by id, or found among `live_sends` by its purpose and the
  call's tag) instead of making a second one. A request is planned again only on the module's own word that nothing
  left a request it holds (rejected, cancelled, withdrawn); one it cannot account for (a request it no longer holds
  after a restart, `stuck`, a leg that failed after it began to leave, a withdrawal it did not confirm) is settled on
  the chain at the request's nonce (from `send_status`, or the module's history row; no row means never broadcast),
  and, lost, sent again through the module with that nonce pinned, so at most one of the two can be mined.
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

- Parts 5 and 6 use a stand-in for `tx_sender_module` that follows the module's code at `f1754c3` for what the daemon
  calls (`send` reserves and answers a request id, a pinned `nonce` included; `send_status` is the broadcast, a final
  refusal for a request it does not hold, `stuck`; `live_sends`; `history` with rows written before the broadcast;
  a restart burns the nonces its history holds). Not shown with the real module: the real `tx_sender_module` or Logos
  Core restarted or killed, `ferrata_relayer_module` itself killed (the real module in a headless Logos Core sent the
  daemon's transactions on anvil, EVIDENCE row 120, without kills); the statuses the stand-in never gives
  (`broadcasting`, the `blocked` and `locked` holds); approval through `signer_manager_module` (the stand-in approves
  at once); the module's own fee and nonce choice and its broadcast through `eth_rpc_module` to a node other than the
  daemon's (where a forgotten transaction the daemon's node does not see is sent again with its nonce pinned: at most
  one of the two mined).
- The gas drop (the LEZ side of the relayer's work) is shown on a running sequencer, not here:
  [`../seqnet-robust-2/`](../seqnet-robust-2/robust.md) and the unit tests `relayer::tests` (a credit that failed, a
  kill before and after the send, one drop at a time).
- A node that lies (answers wrong state that looks right) is not a temporary failure and is outside this test: the
  daemon trusts its own endpoint (the crown's scripts' two-endpoint agreement against lying endpoints is
  [`../../NETWORK.md`](../../NETWORK.md)).
