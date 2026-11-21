# Recorded statements: one verification, many consumers

RFP-021 soft requirement 1, verbatim: "**Recorded-statement account.** In addition to the direct path (Functionality
#7), provide a consumption path that evaluates a predicate and records the resulting verified statement to a public
account, so cost can be amortised when many consumers care about the same Ethereum state. It must share a single
verification core with the direct path, following the pattern in RFP-020, so that the audit surface and the cost
profile stay common. Document that a consumer reading such an account trusts whoever wrote it, which the direct path
does not require."

**Status: built and tested in the in-process harness (LEZ v0.4.0-rc1's state machine, dev mode); not deployed, not
run on a sequencer.** Code: [`lez/statements-wire/`](../../lez/statements-wire/src/lib.rs) (the design, in its module
comment), [`lez/guests/statements/`](../../lez/guests/statements/src/main.rs) (the recorder,
`ferrata_statements`), [`lez/guests/statements-reader/`](../../lez/guests/statements-reader/src/main.rs) (a consumer
that reads it), the test [`lez/harness/tests/statements.rs`](../../lez/harness/tests/statements.rs), its records
[`lez/results/statements.md`](../../lez/results/statements.md) and
[`lez/results/statements-checks.md`](../../lez/results/statements-checks.md). Neither program changes the attestation program or
moves any deployed image id: they are new programs beside it.

## The two paths

| | Direct path (Functionality #7) | Recorded path |
|---|---|---|
| Who verifies | the attestation program, in the consumer's transaction (`Attest` to `header(id, slot)`) | the same attestation program, once, in the recorder's transaction (the recorder sends the same `Attest`) |
| What the consumer gets | the `Attestation` reply, in its own transaction | the same `Attestation`, stored as it came, by `Read` of `statement(key)` |
| Cost per consumer | a whole verification (the proofs of every fact) | a read of the stored entry; the verification is paid once |
| What the consumer trusts | the attestation program's code and the instance it binds to | the same, **and** the program that wrote the entry |

## How it works

1. **Record.** Anyone sends `Record { ferrata, slot, request }` to `statement(key)` of the recorder, where `key` is
   derived from the attestation program, the record's slot and the request **without its proofs** (the question, not
   the evidence: two requests that differ only in their proofs map to one account). The recorder sends the attestation
   program's own `Attest { request }` to `header(ferrata, request.instance_id, slot)`. A refusal there ends the whole
   transaction with the attestation program's code: nothing is recorded for a statement that was not proven. The reply
   is accepted only from that record actor while the recorder waits, and kept once (`Recorded`); a second `Record` of
   the same question is refused (67).
2. **Read.** Any program sends `Read` to `statement(key)` and receives the entry: the attestation program, its record
   actor, the key, and the `Attestation` with every statement and its identifier (Functionality #6). Nothing changes.
3. **Use** (the reference reader, `ferrata_statements_reader`, a consumer that makes the four checks below).
   `Use { recorder, ferrata, instance_id, chain_id, question, min_block, slot, statement_id, tag }` at
   `use(config, statement_id)`, where `config` hashes everything the consumer fixes: the recorder, the attestation
   program, the instance, the chain, the question (its request without proofs, predicate included), the oldest
   execution block it accepts, and its own namespace `tag`. Only the record's slot is chosen per use. The reader
   derives the entry's key from the question and the slot itself, reads the entry, accepts the reply only from that
   entry, checks the entry's program, key, instance and chain, that it holds `statement_id` and that its block is not
   older than `min_block`, then keeps `Used`. A second `Use` of the statement under the same configuration is refused
   (71): the nullifier keyed on the statement identifier, as the Recommended Consumer Pattern asks, on the recorded
   path.

## What a consumer of a recorded statement trusts

A consumer that reads an account trusts whoever wrote it. The direct path does not ask that: the consumer's own
transaction runs the verification.

Here the writer is the recorder's code, and that code writes only the reply of the attestation program named in the
entry's key. That does not stop anyone from writing an entry: anyone may `Record` naming any program as the attestation
program, including one of their own that answers whatever they like, and the recorder keeps that answer. Such an entry
sits at a key that contains that program's id, so a consumer that fixed the genuine program's id never reads it. A
consumer reading this recorder's entries therefore trusts the recorder's program (its image id), the attestation
program and the instance it fixed, nothing else, provided it makes four checks. The reference reader makes all four,
and the test shows each refusing what it must (`the_reader_makes_the_four_consumer_checks`; record
[`statements-checks.md`](../../lez/results/statements-checks.md)):

1. **Fix the recorder's program id and the attestation program's id**, in immutable state. The rule of Verification
   Security #3 applies to both: a consumer that holds either id in mutable state lets whoever controls that pointer
   substitute a program that records anything. The reader keeps both in its configuration, which keys every use
   actor. Tested: an entry another program answered (placed as the recorder would write it, claiming a balance of
   2^256 − 1) is never read under the genuine program's configuration (68), while a configuration that names that
   other program reads it, which is the trust that check is about; at the actor the genuine configuration derives, an
   entry naming another program or another key (placed: the recorder never writes one there) is refused (69).
2. **Derive `statement(key)` from your own question, and accept a reply only from it.** Statement identifiers do not
   encode the predicate, the key does: an entry for "balance ≥ 0" holds the very statements, under the same
   identifiers, as one for "balance ≥ 1 WETH". The reader derives the key from the question in its configuration.
   Tested: with only the weaker question recorded, a use under the stronger one is refused (68) and its configuration
   stays empty, while the weaker question's own consumer is served in its own actors; once the stronger question is
   recorded, the two entries carry the same identifiers and the stronger consumer is served. A forged entry sent to a
   waiting reader by any other actor is refused (65; the wait is placed, since it never outlives its transaction).
3. **Check the entry's instance and chain id, and the statement identifier**, as for a direct attestation. Tested:
   another instance (69), another chain (69), a statement the entry does not hold (70).
4. **Check `block_number`.** An entry is a statement about one finalized block, recorded once; it never becomes more
   recent. The reader refuses an entry older than its configuration's `min_block`. Tested: a bound one block above
   the entry's is refused (72), the entry's own block is accepted. A consumer that needs ever newer statements raises
   the bound in a new configuration; each configuration is its own nullifier namespace, so the same statement can be
   used once under each.

An account written by anything else (an operator's key, a program that copies data without verifying it) would ask
the consumer to trust that writer's honesty. This recorder does not.

## What the test shows

`RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- --nocapture` (from
`lez/`; RISC Zero's guest toolchain as `lez/RESULTS.md` §8, "Reproduce", says). On the Stage 4 chain, mainnet block 26,384,851, the
holdings consumer's question (WETH's balance of Uniswap v2's USDC/WETH pair is at least one WETH):

- **One verification core**: the recorded `Attestation` equals the attestation program's own answer to the request,
  and its storage statement carries the identifier the direct path's consumer (`ferrata_holdings`) keyed its holding on.
- **Several consumers**: two consumers use the recorded statement; neither verifies a proof; consumer A's second use
  under the same configuration is refused (71).
- **Refusals record nothing**: a false predicate (the attestation program's own code, 130, ends the transaction),
  `Record` at another actor (64), a second `Record` (67), `Use` before any `Record` (68), a `Read` that no program
  sent (65); the four checks above.
- **Cost** (measured, guest cycles summed over every program a transaction ran, dev mode, executor): the direct path
  (`ferrata_holdings` with `Attest`) 432,128; `Record` 393,331, of which `Attest` 279,023; one consumer's `Use`
  74,518 (it carries and hashes its question), about 17 % of a direct attestation. With `k` consumers of one question
  the recorded path costs one `Record` and `k` uses instead of `k` direct attestations: from `k = 2` it is the cheaper
  path (393,331 + 2 × 74,518 = 542,367 against 864,256, derived).

## Not done

Not deployed and not run on a sequencer; the recorder is not part of the day-one deployment or the modules. A batch of
several questions in one `Record` is not offered (each request may already hold several facts, soft requirement 2).
