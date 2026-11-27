# ADR 0049: Updater turns count the chain's height, and the updates of other parties' jobs are read from their `Open`s

- **Date:** 27 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 R.9, R.10, F.12

## Context

RFP-021 R.10 asks that submission timing "not systematically concentrate cost on whichever party reacts fastest".
Under ADR 0011 and ADR 0036 each updater waited `H(key, job, step) mod defer_window` of its own polling rounds before
sending a step, and join or wait (ADR 0047) bounded its waits in rounds too. An operator polling more often waited fewer
seconds, so it sent first more often and, with join or wait, carried the jobs it opened: on a running sequencer a daemon
that never deferred and polled every 2 s against 4 s applied 93 of 99 steps (EVIDENCE row 172), and the honest runs
since gave both daemons the same `poll_secs`.

R.9 asks that several instances not multiply the gas spent on an update "beyond what contention makes unavoidable".
Join or wait finds another party's job by its id, which hashes the whole update, `signature_slot` included, and the
committee's signature does not cover that slot; beacon nodes also serve different finality updates for one finalized
checkpoint. Two operators whose sources serve different copies of an update could not see each other's jobs, and each
opened its own.

## Options considered

1. Keep counting each updater's own polling rounds, and tell operators to configure the same `poll_secs`. *Ruled out:*
   RFP-021 R.10 (cost must not fall systematically on whichever party reacts fastest; an operator chooses its own
   polling).
2. Count the wait on the update's beacon slot (its `signature_slot`) and wall time. *Reconstructed for contrast*: the
   slot is not covered by the signature, so a source can move it and with it the clock; it marks when an update was
   signed, not when a job's later steps fall due (when a LEZ block applies the previous step); and in the recorded
   replays the updates' slots lie days before they are served, so every wait would be over at once.
3. For R.9, make a duplicate job cheap or impossible in the program (the job's account keyed by the update's finalized
   header). *Recorded:* docs/decisions/0047-updater-join-or-wait.md (its option 2): it moves the attestation image id
   that deployed instances pin.
4. Count the turns on the height of the LEZ chain the instance lives on, the newest block the backend's reads reflect
   (`Ledger::height`; the sequencer backend: its follower's last block, final on Bedrock), and, for R.9, read the
   instance's applied `Open` transactions in each round it checks other jobs (`Ledger::messages_to`) and rebuild from each the bytes a source
   would serve (`ferrata_advance::served_update_of_open`: the compact update the `Open` carries, its next committee
   completed from the full committee whose summary it names, encoded in SSZ), so that the job of any copy any party
   opened is found, recognized by where it leaves the store, and joined or waited for as join or wait already does for
   copies its sources served.

## Decision

The fourth, in `ferrata-updater` (`Updater::read_clock`, `defer`, `join_or_wait`, `learn_from_chain`; `Ledger::height`
and `Ledger::messages_to`, given by the sequencer backend and the in-process chain). Every party reading the chain reads
the same heights and none can bring a block sooner; a step falls due when a block lands (or when an update is served,
between two blocks), so reading more often than once a block learns nothing that changes a turn; LEZ makes a block every
`block_create_timeout` whatever it holds, so the clock moves while nobody sends; and the duplicate rate the design note
gives, `1 − (1 − Δ/W)^n`, is in block intervals. The turns (`defer_window`), join or wait's bounds (`join_wait_rounds`,
`JOIN_WAIT_CAP`) and the stall alarm count blocks of the chain; a backend that gives no height keeps counting rounds.
The stall alarm also counts the rounds the height has not moved for. The deferral is still drawn per instance
(`H(key, job, step)`, the key the updater's own) and per job and step, and an updater still sends as soon as its turn
has come and no other party's step is on chain. The program and its image id are unchanged.

## Rationale and trade-offs

Two updaters that read the chain at least once a block find a step due at the same height (the block that applied the
previous step, or the first they read after the update was served) and reach their turns at the heights their draws
give, however often each reads: the faster one notices its turn sooner only within the block it comes in. An updater
that reads less often than once a block notices its turns late, by up to its reading interval, and is credited at most
one block for the start of its count: the edge the faster party keeps is bounded by the slower party's interval, not
multiplied by the window. In process (`lez/updater/results/gate-polls.txt`) updaters reading three times and once a
block pay 0.533 and 0.467 of the honest fees (counted in rounds, as before: 0.847 and 0.153), one reading every 1.5
blocks pays 0.549, one reading every third block 0.329: operators should read the chain at least once a block. Turns are coarser: a window of 8 is 8 blocks (80 s at LEZ's 10-s blocks) where it was 8 rounds (about 48 s at
4-s polls on the gate box), so the deferral adds more latency for the same window (the operator guide §3.4 measures it).
Reading the chain's `Open`s costs a scan of the new blocks each round and a native check of each new `Open`'s update; a
party can now make an updater wait for its job without serving it the update, within the same bounds as before (the
job must move the store, and waits end after `join_wait_rounds` blocks without a move and `JOIN_WAIT_CAP` times that on
a state). An `Open` encoded in a layout above the one served is not learned (its job unseen, as before); a period update's
`Open` whose full committee the updater holds nowhere yet waits in a bounded queue until it does. A turn that starts at
a reading is credited at most the one block before it (the block after the previous reading, when that is the one
before), so an updater reading every two blocks loses no block to the start of its count and one back after a long gap
waits its draw.

## Sources

- lez/updater/src/updater.rs (`read_clock`, `defer`, `join_or_wait`, `learn_from_chain`, `health`), lez/updater/src/ledger.rs, lez/updater/sequencer/src/ledger.rs, lez/advance/src/lib.rs (`encode_update`, `encode_finality_update`, `served_update_of_open`)
- lez/updater/tests/turns.rs, lez/updater/tests/copies.rs, lez/updater/tests/recorded.rs (`gate_poll_speeds_share_the_cost`, `gate_copies_join_or_wait`)
- docs/guides/operator-guide.md §3.4; lez/updater/README.md items 7 and 8
