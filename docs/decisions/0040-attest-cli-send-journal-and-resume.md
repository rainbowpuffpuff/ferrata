# ADR 0040: Journal every attestation CLI send before submission and resume the same intent byte for byte

- **Date:** 23 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 R.5, U.5, R.12

## Context

The sequencer does not report transaction outcomes through its submission answer: an answer can be lost after
admission, and a transaction can be in the pool, in a non-final block, or only partly through a multi-transaction
update. Reconstructing and signing the command again could change its bytes or nonce, while blindly resending after
every failure could repeat work. RFP-021 R.5 asks that an interrupted send be recoverable without leakage or loss;
U.5 also requires the stale-checkpoint refusal to be explained and acted on.

## Options considered

1. Treat every unsuccessful invocation as though nothing was sent and generate the command afresh on the next run.
   *Reconstructed for contrast* the implemented journal's cases show why this is unsafe: a transaction may already be
   in the pool or a block when the answer is lost, and another transaction may have taken the newly generated nonce.
2. Never resend after an interruption; require the user to inspect the chain and start another command explicitly.
   *Reconstructed for contrast* the code instead records receipts and resume states automatically. This option would
   abandon an unfinished multi-step update even when its exact signed transactions were still usable.
3. Require `--state-dir` for `--send --yes`; journal the canonical intent and every signed transaction before the first
   send, then record each attempt and receipt. On the same command, report what applied, wait for what is non-final,
   and resend only an unapplied transaction's original signed bytes. Permit a newly planned repeat only with `--again`.

## Decision

The third. Each journal lives under `sent/<intent>.json`, keyed by the parsed command rather than its spelling: instance,
input-file hashes and canonical statements identify the intent; endpoints do not. A resumed run signs nothing new. An
original transaction whose validity window has ended or ends within one minute is not resent. If nothing applied, the
journal is cleared once the zone's own clock is past the window; while the earlier copy may still be pending, the run
stays unfinished. `--again` plans a new send only after the prior run finished.

A first submission rejected before admission is recorded as finished with exit 1, so `--again` can plan afresh. A
rejected resend, or a submission with no answer, leaves the run unfinished and the same command must discover whether
the original landed. One process holds the state directory's lock, and the follower state and journals are replaced
atomically and synced after each write.

## Rationale and trade-offs

Resending the same signed bytes preserves hash and nonce, so LEZ applies the transaction at most once. Recording the
whole command before any send also covers an interruption after an `Open`, allowing its later steps to continue.
Resume distinguishes every state the sequencer can leave behind instead of interpreting silence as failure. The tests
kill the client while reading Ethereum, with a transaction in the pool and in a non-final block, and replay runs
without a second application; spelling changes and concurrent uses of one directory are guarded.

The cost is persistent signed local state, a mandatory state directory and explicit resume semantics. A nonce consumed
by another transaction is reported not applied; it cannot be treated globally from the journal alone. A rejected first
submission and a rejected resend deliberately produce different repeat behaviour because only the latter can leave an
earlier copy pending. Validity clocks add another limitation: a stalled zone or pool backlog can still include a sent
transaction after its window, and the no-send margin reduces but does not remove that case.

## Sources

- Commits `3dd2853f` and `d89f46e5`; lez/ATTEST-CLI.md, "Interrupted and run again" and the following exit statuses
- cli/src/attest.rs (the state-dir, journal and resume implementation)
- cli/tests/attest_faults.rs (interruption, replay, nonce, lock, refusal and validity-window cases)
- cli/results/attest-faults-r2/README.md (the recorded fault run)
- docs/EVIDENCE.md row 228 (attestation CLI reliability and exit statuses)
