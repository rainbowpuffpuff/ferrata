# ADR 0045: Resume a prover call from its recorded id, retry a failed prover and deliver the resulting job once

- **Date:** 24–25 November 2026
- **Status:** accepted; built, exercised against the recorded call-model stand-in
- **Requirements:** RFP-022 R.6

## Context

The test network's prover client turns a checkpoint or release request into a remote `prove_bytes` call, then places a
job in the relayer's inbox. A client can be killed or lose its connection while the remote prover continues, and the
prover process itself can fail. Starting another call after every lost answer can duplicate expensive work; treating a
process failure as final loses a request that could succeed; writing the inbox job again after delivery can make the
relayer handle one proof twice.

## Options considered

1. Start a fresh remote call whenever the client restarts or loses an answer. *Reconstructed for contrast* the client
   instead keeps the spawned call id and calls `FunctionCall.from_id` after reconnecting. A fresh call would abandon a
   proof that may still be running and does not provide the single-delivery record implemented here.
2. Make every prover or transport error terminal for the request, leaving an operator to submit it again.
   *Reconstructed for contrast* the implementation separates permanent request failures from prover-process and
   connectivity failures and has explicit resume and retry states rather than an operator-only recovery path.
3. Spawn the remote call, persist its id before waiting and resume that same call after a connection failure. If the
   prover process answers failure, discard the call id and start another attempt up to the configured bound. Record
   delivery in the call journal so a restart never writes the inbox job twice.

## Decision

The third. Each call record sits beside its request. Reconnecting resumes the recorded remote call without counting a
new attempt. A prover response with `ok` false retains the request but removes the id, so the next poll starts again,
up to `FERRATA_PROVE_ATTEMPTS` (5). An unreadable request, a program other than the pinned one, a missing journal or a
proof of another journal goes to the failed requests immediately rather than being retried.

Once the prover returns success and its journal matches the request, the inbox job is written once and the call record
is marked delivered. A later restart only finishes the request and reports that it was delivered before. The watcher
handles one request at a time and keeps unsuccessful requests available for the next poll.

## Rationale and trade-offs

Recording before waiting closes the restart window between spawning remote work and being able to find it again.
Distinguishing connection loss from a negative prover result avoids both needless duplicate calls and endless retry of
a prover that has actually failed. A separate permanent-failure class fails closed on inputs and proof journals instead
of spending the attempt budget. The delivery marker gives the relayer a single job across client restarts.

The cost is per-request persistent call state and the remote call model's identifier; completed and failed requests
remain distinct from work still being retried. The five-attempt bound means an outage or repeated process failure can
still exceed automatic recovery and require intervention. The fault evidence uses a stand-in of the remote call model:
it exercised connection cuts, a prover killed mid-proof, a client killed mid-proof and duplicate-delivery prevention,
but it is not a demonstration of faults in the hosted proving service itself.

## Sources

- Commits `83feb3b5` and `738353d3`; deploy/testnet/host/bin/ferrata-bridge-prover:1–27,83–150,177–225 (the call
  states and delivery marker)
- deploy/testnet/tests/test_prover_client.py (restarts, transport faults, process failures and permanent refusals)
- deploy/testnet/tests/standin_modal/server.py (the recorded call-model behaviour used by those tests)
- e2e/runs/seqnet-robust-7/README.md (the checkpoint proof under prover and client faults)
- docs/EVIDENCE.md row 259 (one proof, one candidate and the stand-in's stated scope)
