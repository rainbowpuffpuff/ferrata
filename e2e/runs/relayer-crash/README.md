# ferrata-relayer killed, killed at random, and cut off (RFP-022 R.06)

**Run:** `cd cli && bash tests/relayer-crash.sh` (the test `relayer_killed_and_cut_off_does_each_job_once` of
[`cli/tests/relayer_crash.rs`](../../../cli/tests/relayer_crash.rs)), on a Modal build box (CPU), 21 November 2026, the
sources of this directory's commit (branch `rfp-relayer-robustness`). **Result:** `test result: ok. 1 passed` in
117.85 s; [`results.json`](results.json) is the test's own summary. The root imports' counterpart, the LEZ side
failing under the relayer, is [`../relayer-lez/`](../relayer-lez/README.md).

**Where it ran:** a local anvil (chain id 31337, never a public network); FerrataVault v5 in mode (B) from
`vault/script/MonitorFixture.s.sol` with its accept-all verifier (R.06 is not about proving: no proof is checked
here); ETH registered with two denominations (0.1 ETH with a relay fee and no checkpoint fee; 1 ETH with a checkpoint
fee, which the relayer, having submitted the checkpoint, withdraws with `claimOwed`); 19 deposits escrowing what the
releases pay. The jobs are what a checkpoint prover and holders hand a relayer: a rule (B) checkpoint over a burn
root and 35 releases under it (journals encoded as the guests encode them). The daemon is the `ferrata-relayer`
binary signing with a keystore; each run's JSON lines were kept by the test.

## What it shows

| Part | What was done to the daemon | Result |
|---|---|---|
| 1, a kill at each kind of transaction | anvil's automine off: SIGKILL while its transaction waited in the pool, then **P** restarted with it still pending (mined after), **M** mined then restarted, **D** dropped from the pool then restarted: submitCandidate (P), finalizeAttested (M), a release (D), a release (P), a release with a checkpoint fee (M), the claimOwed after it (D) | 12 daemon runs; 6 transactions of the relayer for 6 effects, none reverted; 1 `CandidateSubmitted`, 1 `CheckpointFinalized`, one `Release` per nullifier, 1 `OwedClaimed`, owed 0 after; P: the restarted daemon logged `inflight-pending` and sent nothing until the block was mined; D: `inflight-lost`, then the same call sent again **with the same nonce**; every booking after a restart marked `"recovered": true` |
| 2, random kills | automine on, 20 releases in the inbox, SIGKILL 0 to 40 ms after each start-up line, started again until every job was booked | 26 transactions for 26 effects (cumulative), none reverted; one `Release` per nullifier; every job in `done/` once, none refused |
| 3, flaky endpoint | `e2e/netfault/faultnet.py` seed 11, the daemon on its `flaky` endpoint (429s, 5xx, answers later than the client waits, a head that goes backwards), outages of every endpoint, 4 releases | done in 2 s; faults seen: 3 × 429, 1 × 5xx, 4 receipts behind; 30 transactions for 30 effects, none reverted |
| 3, sends endpoint | seed 16, the `sends` endpoint (raw transactions dropped with their hash answered, duplicated, or forwarded with the answer lost; "block not found" now and then), and its outages (118 answers refused during them) | done in 73 s; 1 drop, 1 duplicate, 2 not found; 34 transactions for 34 effects, none reverted |
| 3, lagging endpoint | seed 12, the `lagging` endpoint (4 blocks behind, 429s), a block mined every second | done in 20 s; 29 receipts behind, 5 × 429, 3 rounds waited as `node-behind` (the node below the relayer's own last transaction); 38 transactions for 38 effects, none reverted |

Checked after every part (the test fails otherwise): one `CandidateSubmitted` and one `CheckpointFinalized`; one
`Release` per nullifier and every nullifier used; every job in `done/`, none in `refused/` or left in the inbox; each
with at least one `job-done` line, all naming one transaction; the relayer's nonce equal to the number of effects,
nothing pending; every transaction of the relayer in the chain mined with status 1.

## What the daemon does for it (`cli/src/relayerd.rs`, `cli/src/relayer.rs`, `cli/src/eth.rs`)

- **One transaction in flight, journaled.** `<state>/inflight.json` (what the transaction does, the inbox job it books,
  its calldata, its nonce; its hash once the node has it) is written before the send and removed only once the effect
  is booked (the job moved to `done/`, its line written). Each round first resolves a record left by a killed or
  cut-off daemon: mined, its effect is booked (`"recovered": true`); pending, the round waits; lost, the same call is
  sent again with the same nonce, so at most one of the two can be mined; a spent nonce without a receipt for the
  recorded hash is looked up in the blocks since the record (sender, nonce and calldata).
- **A fence against stale nodes.** `<state>/fence.json` keeps the block of the relayer's last mined transaction and
  the nonce after it; a round reads at one block at or above it (waiting while the node is behind) and no nonce
  below it is used, whatever a lagging node answers.
- **Endpoint failures never refuse a job.** A job is refused for good only on a refusal (the relayer's or the
  vault's); a rate limit, a 5xx, a closed connection or a node behind keeps it in the inbox (found by this test: a
  429 had refused a release before). Single reads are retried on transient errors (`eth::retry_rpc`).
- **Gas with headroom** (the estimate plus 30 % and 20,000; found by this test: a candidate estimated at the pinned
  block ran out of gas when mined later). **One daemon per state directory** (`<state>/lock`). **The endpoint is
  waited for at start** (`--startup-wait`). A **wrapped seal is reused** after a restart (no second Groth16 wrap).

## What it does not show

- The `--send-via logos` path (Logos Core's `tx_sender_module`) is not journaled: `tx_sender_module` keeps its own
  requests.
- The gas drop stays at most once: the drop is recorded before its credit is sent, so a relayer killed in between
  sends no credit for that `N` again (the user asks another relayer, with a receipt naming it).
- A node that lies (answers wrong state that looks right) is not a temporary failure and is outside this test: the
  daemon trusts its own endpoint (the crown's scripts' two-endpoint agreement against lying endpoints is
  [`../../NETWORK.md`](../../NETWORK.md)).
