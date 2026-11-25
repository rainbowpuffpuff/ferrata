# Rows of stream recovery-022: RFP-022 R.6 and R.7

Branch `recovery-022` (from `861c1c0d`). For the integrator: each row's proposed status, the test or run behind each
clause, and a proposed EVIDENCE row. Nothing here edits `docs/EVIDENCE.md`, `docs/requirements-status.md` or
`docs/traceability.md`. Every run below is on the Modal build box (CPU); every chain is local (a LEZ v0.4.0-rc1
sequencer with its Logos Blockchain node, anvil with Sepolia's chain id); nothing was sent to a public network.
Proofs: RISC Zero dev mode and a mock verifier for the round trip (R.6 and R.7 are not about proving).

## RFP-022 R.6: proposed **shown**

> Temporary RPC or connectivity failure on either chain leaves any off-chain component in a recoverable state, able to
> resume without duplicating work already done.

The off-chain components and where each is shown:

| Component | Fault | Shown by |
|---|---|---|
| relayer daemon, Ethereum side | kills at each transaction, lost and held answers, flaky, dropping and lagging endpoints, the send route's windows | EVIDENCE 46, 47, 203 (unchanged) |
| relayer daemon, LEZ side: root import | its `ImportRoot` forwarded to the running sequencer with the answer held, the daemon SIGKILLed; started again it books the import from LEZ (`recovered`), sends nothing: 1 sent, 1 on the zone | `e2e/runs/seqnet-robust-7/` (case j); before: the devnet zone only (203) |
| relayer daemon, LEZ side: gas drop | credits trapped, answered 503 and forwarded late, held with kills | 203 and robust-7 (case f, re-run) |
| `monitor` | SIGKILL, state lost (43, 96); **its own endpoint out** while a wrong candidate arrives (it keeps polling, alerts `poll-failed`), then back with its cancellation's answer lost: one cancellation, one transaction | `monitor/tests/anvil-e2e.sh` S12 (in CI's monitor job), `e2e/runs/monitor-endpoint-1/` (50 of 50) |
| rule (B) attestor | live, beside the running zone: SIGKILLed 5 times (4 inside a round) and restarted, its node and its LEZ node each cut off for 20 s, in turn (4 outages), while the user's commands run; its 65 attestations over 19 tips (blocks 30..53): one signature per tip, each tip the zone's block (19 of 19 checked against the checkpoint's `blocks.tsv`); its own attestation then carries the checkpoint (`realblocks-crown --att`) | robust-7 (case k, `attestor/`); the four attestors of a recorded four-sequencer zone killed mid-round 31 times: one signed message each, no torn state, the 3-of-4 input byte-identical to the proven one, the executor accepts it: `checkpoint/attestor/kill-replay.sh`, `e2e/runs/attestor-kill-replay-1/` |
| checkpoint and release prover and its client | the deployed client (`deploy/testnet/host/bin/ferrata-bridge-prover watch`) now spawns each proof and keeps the call id: killed mid-proof and started again it waits for the same call; the prover dying mid-proof is tried again; the service down keeps the request; a restart after delivery writes no second job; a proof of another journal fails at once | `deploy/testnet/tests/test_prover_client.py` (5 cases, in CI's links job); robust-7 (case k: the checkpoint proven through the client, its first call's prover dying 4 s in, the client SIGKILLed during the second: 2 calls for 1 proof, 1 inbox job, 1 `CandidateSubmitted`) |

Conditions to state with the row: the prover behind the client is a stand-in of Modal's call model
(`deploy/testnet/tests/standin_modal/`: a local server whose spawned calls outlive the client, as Modal's do; in the
round trip it runs checkpoint_b's executor and the mock verifier, no GPU proof); the live attestor is one (the zone has
one sequencer), the four-attestor case is a replay of recorded answers (no endpoint fails there). Still not shown, as
the matrix says today and unchanged here: `monitor`'s fallback send failing right after `tx_sender_module` burnt a
nonce (it does not recover), the send route with the real `tx_sender_module` or Logos Core restarted, and
`ferrata_relayer_module` itself killed: all on the optional Logos Core send route. If the integrator counts that route
as a component of the clause, the row stays **in part**, for that route only.

Proposed EVIDENCE row: **Off-chain components under faults on a running LEZ sequencer and a local Ethereum
(RFP-022 R.6, R.7)**: `e2e/seqnet/run.sh robust robust-7`, 33 of 33: the relayer's root import killed with its answer
held (booked from LEZ, 1 ImportRoot); the rule (B) attestor killed inside rounds and cut off from its node and its LEZ
node (one signature per tip, every tip the zone's block; its attestation carries the checkpoint); the checkpoint
proven through the prover client, its prover dying mid-proof and the client killed mid-proof (2 calls, 1 proof
delivered, 1 candidate); the release's relayer killed before and after its transaction (1 release); the user's deposit
killed twice (1 deposit); the claims, burns and gas drops of rows 203 and 204 again; case h. Where: the Modal build box;
LEZ v0.4.0-rc1 sequencer and node; anvil (chain id 11155111). Proofs: dev mode, mock verifier, stand-in prover.
Re-check: `bash e2e/seqnet/run.sh robust NAME` (after `run.sh build`; `R0VM31` an r0vm 3.1.0). Records:
`e2e/runs/seqnet-robust-7/`. And: `monitor/tests/anvil-e2e.sh` S12 (`e2e/runs/monitor-endpoint-1/`, 50/50),
`deploy/testnet/tests/test_prover_client.py` (5/5), `checkpoint/attestor/kill-replay.sh`
(`e2e/runs/attestor-kill-replay-1/`).

## RFP-022 R.7: proposed **shown**

> An interrupted user-side operation does not consume, corrupt, or expose the user's entitlement.

| Leg, interruption | Before | Shown by |
|---|---|---|
| deposit, killed after its Ethereum transaction was signed and trapped before any node | not shown | robust-7 (case i): nothing on the chain; the new deposit journal holds it open |
| deposit, the same command again: sent with the same `j` and nonce, mined, answer held, killed (after the Ethereum transaction, before the import) | not shown; **the tool deposited a second time** (it read `depositCount` past the first) | robust-7 (case i): the third run finds it on the chain and sends nothing; across the three runs one deposit (`depositCount` 1 to 2, the depositor's nonce 1 to 2, one `Deposit` event); then imported and claimed as any deposit. Code: `cli/src/deposits.rs` (3 unit tests) |
| after the import, before the claim | not shown | robust-7 (case j): the import's answer held and the relayer killed; booked from LEZ, one ImportRoot; both deposits then claimed once (`recover`: 2 claimed) |
| claim killed after its proof; burn killed after its send | 204, 216 | robust-7 (cases c, d, re-run) |
| release, after the proof (the release job built), before the transaction: the relayer killed with its transaction trapped | not shown | robust-7 (case l): nothing on the chain, in flight in its record |
| release, after the transaction, before it is recorded: started again, the lost release sent with its nonce, mined, the answer held, killed | not shown | robust-7 (case l): started again, it finds the release mined under the recorded nonce and books it (`inflight-found`); one `release` transaction on the chain, one `Release`, the fresh address paid once |
| the tool's interrupted burn (case h): a burn answered 503 and forwarded 90 s late, then a burn to another recipient | written, not re-run (204) | robust-7 (case h): the other burn takes a fresh counter and is applied once; the late transaction reaches the sequencer and is refused |

Still not shown: the Basecamp module killed between a proof and its sending (its view was closed during one, row 126).
The tool's path, which is the user-side operation of the CLI and the relayer, is shown at every interruption point above.

Proposed EVIDENCE row: the R.6 row above (one run shows both rows), and the deposit journal: **`ferrata deposit`
never deposits twice after an interruption (RFP-022 R.7)**: `cli/src/deposits.rs`, the deposit recorded before it is
signed and resolved from the chain by the next run (on chain: not sent again; never sent: sent again with the same
`j` and nonce; nonce spent: nothing deposited; pending: stop). Unit tests: `cd cli && cargo test --lib deposits` (3
passed, build box); the run: robust-7 case i.

## Commits (branch recovery-022)

| Commit | What |
|---|---|
| `83feb3b5` | prover client: spawn and resume, retries, delivery once; `test_prover_client.py` in CI |
| `d6796b8a` | `cli/src/deposits.rs`; the robust run's cases i-l (`deposit-leg.sh`, `robust-r06.sh`, `standin-prove.sh`) |
| `db5adcb1` | `monitor` S12 |
| `727e08a2`, `20444ac0` | `kill-replay.sh` and its record |
| `a1e518b3`, `a9ef3940` | fixes the seqnet harness needed at `861c1c0d`: the vault over the 3.1.0 verifier with the mock's code (`anvil_setCode`), r0vm 3.1 for the 3.1 tools; the monitor record |
| this commit | the robust run's records (`e2e/runs/seqnet-robust-7/`, 33 of 33) and this file |

No guest or program changed: no image id moved.
