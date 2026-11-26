# ADR 0044: Journal a deposit before signing and resolve an interrupted send from the chain before planning another

- **Date:** 24–25 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-022 R.7

## Context

`ferrata deposit --send` uses `j = depositCount(depositor)` and pays the depositor's Ethereum nonce. If the command is
killed after submission but before it receives a receipt, a later invocation sees the increased deposit count. Before
the journal, it treated that state as a fresh request and sent another user's deposit at `j + 1`. Retrying without
knowing whether the first transaction reached a node could spend money twice; never retrying could leave an unsent
deposit permanently unfinished.

## Options considered

1. Re-read `depositCount` on each invocation and plan the next deposit from it. *Recorded:* cli/src/deposits.rs:1–25
   identifies this as the behaviour before the journal and records the duplicate `j + 1` deposit after a lost answer.
2. Record the request and its nonce before signing, then reconcile every open entry against the vault count, latest
   and pending nonces, and the submitted hash. Resume only an unseen request with the same `j` and nonce, rather than
   planning a new deposit.

## Decision

The second. The journal lives beside the keystore that signs the deposit, independent of the separate LEZ wallet
location, and a lock serialises the tool's processes using it. The entry identifies chain, vault, depositor, `j`, token,
denomination, destination, commitment and nonce; it is written before signature, updated with the submitted hash and
removed when resolved. Journal replacement is atomic and synced.

The next run resolves open entries before planning: a count past `j` means on chain, so an identical request is only
reported; a known transaction or a pending nonce means pending, so the run waits; an unspent, unknown transaction is
sent again with its original `j` and nonce; a spent nonce with the count unchanged closes the entry as not deposited.
A different deposit is planned only after the open request resolves and is refused while that request is pending.

## Rationale and trade-offs

Only the depositor's key can make the vault count advance past that depositor's `j`, so the vault's own count is a
chain-level test that no second send is needed. Reusing both `j` and nonce for a transaction no node saw ensures at
most one of the copies can be mined. The vault independently refuses a wrong `j`, keeping two transactions for one
index apart. The recorded robust run made one deposit across three runs, and the unit tests cover the chain-state
decisions.

The guarantee is deliberately scoped to one depositor using the same keystore on one machine. A copied keystore on
another machine has another journal, although the vault's index check remains; the journal cannot coordinate those
copies. Recovery from the seed does not depend on the file: scan and recovery still find deposits from the chain. The
trade-off is small, private, locked local state and more reads before every deposit in exchange for not duplicating an
interrupted send.

## Sources

- Commits `d6796b8a` and `738353d3`; cli/src/deposits.rs (the former duplicate, journal record and chain-state machine)
- cli/src/main.rs, the deposit setup (the journal location beside the signing keystore)
- e2e/runs/seqnet-robust-7/README.md (one deposit across interrupted runs)
- docs/EVIDENCE.md rows 259–260 (the end-to-end fault run and scoped guarantee)
