# ADR 0056: A user's own proof carries at most two EIP-2935 hops; an older fact is read under an older record

- **Date:** 27 November 2026
- **Status:** accepted; measured at the declared minimum's x86-64 shape, not on the Apple M1; not enforced in code
- **Requirements:** RFP-021 P.4, F.8

## Context

A private consumer reads a record of the attestation program (its slot is public: the transaction's `Read` names the
record's actor) and verifies the fact inside its own proof. A fact in a block more than 8,191 blocks before the
record's block is reached through EIP-2935 hops, each verified inside the user's proof, and in the reference consumer
only the private deposit claim (`ClaimDepositPrivately`) carries them. The consumer hashes with software Keccak, as a
private proof with RISC Zero's Keccak accelerator would carry an assumption into LEZ's privacy-preserving circuit
(`lez/RESULTS.md` §5, "Decisions", item 6), so a hop costs 1.04M to 1.16M user cycles in the consumer's private
transitions (`lez/results/vault-claim-hops.md`), not the 0.18M to 0.23M accelerated cycles a hop that the earlier
estimate used. Until this record no limit was stated: P.4's row said that a fact more than 8,191 blocks back "has no
stated user-side cost or limit". On 27 November 2026 the padded private claim was proven through 1, 2, 3 and 5 hops in
containers held to the declared minimum's x86-64 shape (16 vCPU, 16 GiB): 926.6, 1,035.6, 1,233.4 and 1,511.3 s, and
through 2 and 5 hops at 8 vCPU: 2,031.7 and 3,255.1 s (`lez/results/p4-hops/README.md`); M0's bound of practical is 20
minutes and 16 GB at the minimum (`scripts/m0.py`, `LIMIT_S`).

## Options considered

1. No limit: a user's proof carries as many hops as the fact's age needs. *Recorded:* `docs/requirements-status.md`
   (P.4 as of this record: "has no stated user-side cost or limit"). The cost is unbounded: 5 hops took 1,511.3 s at
   the minimum's shape, above M0's 20 minutes.
2. A limit of 5 hops, from derived figures. *Recorded:* `docs/requirements-status.md` (P.4 as of this record:
   "Proposed, not declared: user-side proving for facts up to 5 hops behind the newest record"). Its per-hop figure was
   the accelerated one; measured, 3 hops already take 1,233.4 s at the minimum's shape.
3. At most 2 hops in a user's own proof, the most the measured claim keeps within M0's 20 minutes at the minimum's
   x86-64 shape; an older fact is read under an older record, one within two hops of it, or attested publicly.

## Decision

The third. User devices are declared to prove facts reached through at most two hops behind the record they read: up
to 16,382 blocks, about 2.3 days of Ethereum blocks at 12 s. A consumer's client with an older fact chooses a record
from at most two hops after the fact's block (records never expire: `docs/design/attestation.md`), or uses the
public path. The programs are unchanged: the consumer still verifies any number of hops it is given, and no client
enforces the limit yet.

## Rationale and trade-offs

The limit follows what was measured, not an estimate: at the minimum's x86-64 shape the padded private claim settles
within 20 minutes through two hops and not through three, with the same peak memory (9.2 GiB) and the same
transaction and receipt sizes at every hop count, so the transaction does not say how many hops the proof carried.
The costs: reading an older record publishes its slot, so a fact more than two hops before the newest record is
claimed under a record that narrows when it happened, a smaller anonymity set than the newest record's; the Apple M1
alternative of the declared minimum is not measured with hops, and by the 1.35 to 1.36 ratio of every earlier proof
on it (`lez/results/p4-macos/README.md`) even one hop would take more than 20 minutes there; and the limit is a
statement in the documentation, not a check in code.

## Sources

- lez/results/p4-hops/README.md and lez/results/p4-hops/table.md (the proofs; the records in bench/perf/results/p4-hops/)
- lez/results/vault-claim-hops.md and lez/harness/tests/vault_claim_hops.rs (the cycles of each hop in the executor)
- lez/harness/src/bin/prove_vault_claim.rs and lez/harness/src/hops.rs (`claim_hops`, the chains of 1 to 5 hops)
- crates/ferrata-spectests/fixtures/sepolia-hops-2026-11-27/SOURCE.md (the chains to the deposit's block)
- scripts/m0.py (M0's check 1: 20 minutes and 16 GB at the declared minimum)
- lez/RESULTS.md §5 (the consumer's software Keccak)
