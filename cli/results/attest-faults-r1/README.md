# `ferrata attest` under a failing Ethereum endpoint, interrupted, and a stale checkpoint (run r1)

RFP-021 Reliability #5 and Usability #5. The test [`../../tests/attest_faults.rs`](../../tests/attest_faults.rs) at the
tree of commit `42c378d4` (branch `cli-rows`), run once on the Modal build box (Linux x86-64), 23 November 2026:

```sh
cd cli
RISC0_DEV_MODE=1 cargo test --locked --test attest_faults -- --nocapture --test-threads 2
```

Result (measured): 2 tests passed in 32.4 s; the library's tests of the mapping (`cargo test --locked --lib attest`):
5 passed. CI's `roundtrip` job runs both. Each file here is one command's output, in order (machine paths replaced
by `<repo>`); `[exit N]` is its exit status, `[killed: …]` a SIGKILL from the test. 22 commands on the R.5 zone and
8 on the four U.5 zones: 14 exit 0, 4 exit 3, 8 exit 4, 4 killed.

## What ran

- **The zone**: a stand-in for a LEZ v0.4.0-rc1 sequencer, in the test process: LEZ's own state machine and block
  application (`chain_state::apply`: blocks built as LEZ's block builder builds them, a transaction it rejects
  dropped; `apply_block`), the attestation program deployed at genesis (`with_genesis_programs`, `lez/harness`'s
  program), served on 127.0.0.1 over the four JSON-RPC methods the tool calls. The test sets its clock, when blocks
  are made and when they are final on Bedrock. Not LEZ's sequencer binary: no mempool limits, no Bedrock node.
- **The Ethereum endpoint**: a scripted JSON-RPC server on 127.0.0.1 answering from the recorded mainnet responses of
  `crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`, with faults on demand.
- **The tool**: the `ferrata` binary built from this tree, run as a process, its follower starting from the zone's
  genesis state. Nothing left 127.0.0.1.

## R.5 (`r5/`): nothing done twice

| # | Case | Exit | What it shows |
|---:|---|---:|---|
| 01 | `init --send --yes` | 0 | the instance: `Init` 210,357 execution gas, `Adopt` 3,399,141 and 3,485,575 |
| 02–03 | `advance --finality` killed with its `Open` in a block not yet final, then run again | killed, 0 | the rerun reads its journal: `open` "applied … since the earlier run sent it: nothing sent"; `miller` and `finish` "not sent by the earlier run: sending it as that run signed it"; each equal to its dry run; the record `header(id, 15,624,480)` written; one `Open` on the zone |
| 04 | the same `advance` again | 0 | "already sent by an earlier run … nothing sent" |
| 05 | `attest`, endpoint refusing connections | 4 | "refused the connection or is unreachable (curl exit 7)", retried once, "nothing was sent" |
| 06 | endpoint holding `eth_getProof` without an answer | 4 | "did not answer in time (curl exit 28)", 2 attempts of 2 s |
| 07 | `eth_getProof`'s answer cut mid-body | 4 | "cut short (curl exit 18)", retried once |
| 08 | HTTP 503 | 4 | retried once |
| 09 | a JSON-RPC error | 4 | not retried (one request) |
| 10 | `eth_chainId` 11,155,111 for a chain-1 record | 4 | "serves chain 11155111 (eth_chainId), the record is of chain 1"; no proof asked |
| 11 | block 26,384,851 with another hash | 4 | "has hash 0xabab…, the record binds 0x3e02…": no proof asked |
| 12 | `eth_getBlockReceipts`' answer cut (`--log`) | 4 | the second method, cut after the proofs' requests answered |
| 13 | `eth_getProof` cut once, then answered | 0 | "trying again (1 of 1)", applied: 275,925 execution gas |
| 14 | 12's command, endpoint answering | 0 | applied once (94,795) |
| 15 | 13's command again | 0 | "already sent by an earlier run": nothing sent |
| 16–17 | killed while the endpoint held its request; run again | killed, 0 | nothing sent by the killed run; applied once (166,249) |
| 18–19 | killed with its transaction in the pool (no block); run again | killed, 0 | "in no block yet: sending the same signed transaction again (hash c372…, nonce 9)": the zone took the same hash twice and applied it once (the second copy dropped for its used nonce, as LEZ's builder drops it) |
| 20–21 | killed with its transaction in a block not yet final; run again | killed, 0 | "in a block of the sequencer not yet final: waiting for it, nothing sent" |
| 22 | 20's command again | 0 | "already sent by an earlier run" |

Checked by the test after the cases: 12 submissions, 11 transactions included, each once (the one resubmission is the
pool case's, byte for byte); no `Attest` request and no job step applied twice; none reverted.
[`r5/journal-example.json`](r5/journal-example.json) is the journal of cases 20–22 (the signed transaction cut short
here).

## U.5 (`u5-*/`): a stale checkpoint as code 53's message

- `u5-past-end`: the zone's clock 60 s past the end of the checkpoint's weak-subjectivity period (2026-11-25 22:40:23
  UTC for the bootstrap at slot 15,622,144): `init`'s dry run shows LEZ's refusal as "refused with 53
  OutsideWeakSubjectivityPeriod … the checkpoint 0x7c27… (its header at slot 15622144) is stale … recover from a new
  bootstrap … `ferrata attest init`", and "not sent"; exit 3; the zone received nothing.
- `u5-crossing-end`: the dry run an hour before the end (applied), the zone's block 60 s after it: the receipt's
  refusal ("Execution outside of the validity window", charged the whole gas limit, 10,000,000) shown as the same code
  53 message; exit 3; the adoption parts not sent. `receipt --tx` of that `Init` (how a caller of the Logos module,
  whose `lez_submit` reports inclusion only, reads the outcome) prints the same message.
- `u5-update-crossing-end`: an instance made an hour before the end, then `advance --finality --send --yes` with the
  zone's next block 60 s after the end of the period of the instance's newest finalized header (slot 15,622,144): the
  `Open`'s dry run applied, the zone refused it, shown as code 53's message naming that header; exit 3; `Miller` and
  `Finish` not sent; `receipt --tx` of the `Open` prints the same message.
- `u5-not-open`: `advance` before its update's signature slot: "refused by LEZ: \"Execution outside of the validity
  window\": its window opens at 2026-11-10 13:49:23 UTC …": LEZ's text with the window's start, no code put on it;
  exit 3.

The library's tests (`cli/src/attest.rs`, `tests`) map the refusal a real LEZ v0.4.0-rc1 sequencer gave, the `twin
Init` of [`../../../e2e/runs/ci-sequencer-attest-2026-11-21/`](../../../e2e/runs/ci-sequencer-attest-2026-11-21/README.md)
(its `run.tsv` row and its block in `records/blocks.borsh`: the block's time, 2026-11-21 16:31:51 UTC, past the period
of that `Init`'s checkpoint), to the code 53 message; and they pin the constant to LEZ's own error text.
