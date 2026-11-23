# `ferrata attest` and `ferrata_attest_module` under a failing Ethereum endpoint, interrupted, and a stale checkpoint (run r2)

RFP-021 Reliability #5 and Usability #5, second round (after the first review; replaces run r1). The tests at the tree
of commit `8385fe9e` (branch `cli-rows`), run once on the Modal build box (Linux x86-64), 23 November 2026, as CI
runs them:

```sh
cd cli && export RISC0_HOME="$HOME/risc0-home-lez" && vars=$(python3 -I ../scripts/fetch_native.py env) && eval "$vars"
cargo test --locked --lib attest
RISC0_DEV_MODE=1 cargo test --locked --test attest_faults -- --nocapture --test-threads 3
cd .. && scripts/ci/modules-resolve.sh ferrata_attest_module
cd modules/ferrata_attest_module/rust-lib && cargo test --locked -p ferrata-attest-calls --test failing_endpoint -- --nocapture
```

Result (measured): the library's tests 7 passed; `attest_faults` 3 passed in 31.3 s; `failing_endpoint` 1 passed
in 5.7 s (16 failing calls). Clippy: no warning in this branch's lines.

Each `*.txt` under the directories is one command's output, in order (machine paths replaced by `<repo>`); `[exit N]` is
its exit status, `[killed: …]` a SIGKILL from the test. 53 commands: 23 exit 0, 4 exit 1, 3 exit 2, 8 exit 3, 8 exit
4, 7 killed. The R.5 zone: 12 submissions, 11 transactions included, each once (one byte-for-byte resubmission); the
guards' zone: 14 submissions admitted, 1 turned away, 12 included, each once. `Attest` cost 94,795 (a log) to 275,925
(a slot with its predicate) execution gas; a refused `Init` was charged 10,000,000 (the gas limit). `test-output.log` is the test binary's own output;
`module/failing_endpoint.txt` the module test's. No transaction hash is quoted here: signatures are randomized, so a
rerun gives other hashes with the same exits, gas and messages.

## What ran

- **The zones**: stand-ins for a LEZ v0.4.0-rc1 sequencer, in the test process: LEZ's own state machine and block
  application (`chain_state::apply`: blocks built as LEZ's block builder builds them, a transaction it rejects dropped;
  `apply_block`), the attestation program deployed at genesis (`with_genesis_programs`, `lez/harness`'s program),
  served on 127.0.0.1 over the four JSON-RPC methods the tool calls. The test sets their clock, when blocks are made and
  when they are final on Bedrock, and can turn a submission away with LEZ's own answer ("Transaction too large: size N,
  max M"). Not LEZ's sequencer binary.
- **The Ethereum endpoints**: scripted JSON-RPC servers on 127.0.0.1 answering from the recorded mainnet responses of
  `crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`, with faults on demand.
- **The tool**: the `ferrata` binary built from this tree, run as a process, its follower starting from the zone's
  genesis state. **The module**: `ferrata_attest_module`'s calls crate (`fetch_log_proof`, `build_attest_tx`,
  `dry_run_attest`), its plain `rpc_url` path. Nothing left 127.0.0.1.

## R.5 (`r5/`, `r5-guards/`, `module/`): nothing done twice

`r5/`: an instance (`01`); `advance` killed with its `Open` in a block not yet final, run again: the `Open` "applied …
since the earlier run sent it: nothing sent", `miller` and `finish` sent as that run signed them (`02–04`); `attest`
under eight endpoint faults, each exit 4 and "nothing was sent" (`05–12`: refused connections, a held request, the
proof's answer cut, 503, a JSON-RPC error, chain id 11,155,111 for a chain-1 record, another block hash, the receipts'
answer cut); an answer cut once, retried and applied (`13`); the failed commands run again, applied once or "already
sent" (`14`, `15`); killed while the endpoint held its request, run again, applied once (`16`, `17`); killed with its
transaction in the pool, run again: the same signed transaction sent again, applied once (`18`, `19`); killed with it in
a block not yet final, run again: it waits and sends nothing (`20–22`). `r5/journal-example.json` is the journal of
`20–22` (the signed transaction cut short here).

`r5-guards/`: the same command spelled otherwise (no `0x`, capitals, the value unpadded): "already sent" (`03`, `04`);
`--send --yes` without `--state-dir`: exit 2, nothing sent (`05`); a submission the sequencer turns away (too large):
exit 1, recorded; run again, reported; with `--again`, applied once (`06–08`); `--again` on an interrupted run: exit 2,
nothing sent; without it, finished (`09–11`); a transaction whose nonce another took (the pool lost, then another command
sent at that nonce): "NOT APPLIED", exit 1; run again, reported; with `--again`, applied once (`12–16`); a `status`
beside a run awaiting its transaction: exit 2, the state directory held (`17`, `18`).

`module/failing_endpoint.txt`: each of `fetch_log_proof`, `build_attest_tx` and `dry_run_attest` against a refused, a
held (cut by its 1,000 ms bound), a cut, a 503 and an erroring endpoint, and `fetch_log_proof` cut in its second request
after its first was answered: 16 error replies naming the method and the failure, each within its bound, each call
answered once the endpoint was back.

## U.5 (`u5-*/`): a stale checkpoint as code 53's message

- `u5-past-end`: the zone's clock 60 s past the end of the checkpoint's weak-subjectivity period (2026-11-25 22:40:23
  UTC for the bootstrap at slot 15,622,144): `init`'s dry run shows code 53's message, and "not sent"; exit 3.
- `u5-crossing-end`: the dry run an hour before the end, the zone's next block 60 s after it (no block between, which no
  client can see coming): the receipt's refusal (the whole gas limit, 10,000,000, charged) shown as code 53's message;
  `receipt --tx` of that `Init` prints the same.
- `u5-update-crossing-end`: the same for an `advance`'s `Open`, the message naming the instance's newest finalized
  header; `receipt --tx` of the `Open` prints the same.
- `u5-not-open`: `advance` before its update's signature slot: LEZ's text with the window's start, no code put on it.
- `u5-final-tip-lagging`: the follower at block 1 (final), the zone's newest block past the end (not final): the dry run,
  made at the zone's clock, shows code 53's message, and nothing is sent.
- `u5-resume-past-end` (the first review's case): `init` killed with its `Init` in the pool, the pool lost, the zone's
  clock 60 s past the end (a block stamped then): run again, "not sent again", code 53's message, the journal cleared,
  nothing applied or charged; run once more, a fresh dry run holds it.

The library's tests (`cli/src/attest.rs`, `tests`) map the refusal a real LEZ v0.4.0-rc1 sequencer gave, the `twin Init`
of [`../../../e2e/runs/ci-sequencer-attest-2026-11-21/`](../../../e2e/runs/ci-sequencer-attest-2026-11-21/README.md)
(its `run.tsv` row and its block in `records/blocks.borsh`), to the code 53 message, pin the constant to LEZ's own error
text, and check the journal key and the send-time window check.
