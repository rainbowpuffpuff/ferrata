# `ferrata-attest`: the attestation side's command-line tool

**Prototype, unaudited.** Milestone A3 ("the CLI with dry runs returning the program's codes").
Source: [`harness/src/bin/ferrata-attest.rs`](harness/src/bin/ferrata-attest.rs).

`ferrata-attest` advances an attestation instance with a light-client update, and attests a
storage slot, an account, a log or a whole receipt of a finalized Ethereum block through the
attestation program, end to end; by default it only says what it would do. Today it runs on the harness: LEZ v0.4.0-rc1's state transition in process, with the
program's guest in RISC Zero dev mode, and real Sepolia data recorded in fixtures. The module and
the daemons that will run it against a live sequencer and live Ethereum endpoints are separate
deliverables (`modules/`, the updater).

## What one run does

1. **Follows Sepolia.** `Init` from a bootstrap in the Electra layout (the last period before
   the Gloas fork), then every period's update from 1378 to 1409 (through the Gloas fork, where the instance's store upgrades
   itself), then the finality update of `sepolia-attest-2026-11-10`, each as public jobs through
   the program. The record `header(id, 11,549,664)` then binds Sepolia block 12,105,936 (Gloas
   layout: the block hash only). Data: `crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10`
   and `.../sepolia-attest-2026-11-10`, checked against their pinned `SHA256SUMS`.
2. **Builds the request** from recorded `eth_getProof` (USDC at `0x1c7d…7238` with its slot 11,
   Multicall3 at `0xca11…ca11`) and `eth_getBlockReceipts` (538 receipts, 732 logs) responses: the
   receipts trie is rebuilt and checked against the block's receipts root; the block's RLP header
   is carried because the record binds the hash only.
3. **Dry run** (the default):
   - the **verdict**: `ferrata_lez_wire::attest::evaluate`, the function the program runs, on the
     record the zone holds: the statements and their identifiers, or the refusal's exit code with
     its name and meaning (read from [`../idl/ferrata_attestation.idl.json`](../idl/ferrata_attestation.idl.json));
   - **what would be sent**: the `Attest` message (variant 7), its size, the actor it goes to
     (`header(instance, slot)`), the request's parts;
   - **the cost**: the transaction, signed as the harness's fee payer would sign it, is traced and
     settled by LEZ's own fee settlement on a **scratch copy** of the zone's state: execution gas,
     metered cycles, storage gas (the transaction's bytes), base fee, and the events it would emit,
     or the revert and its code. The zone's state is not changed.
   - The tool checks that the verdict and the zone's execution agree (same exit code; the
     `FRT-ATT1` event names the verdict's identifiers).
4. **`--send`** settles the same transaction on the zone's state, and checks that the settled
   transaction equals the dry run: exit code, cycles, execution and storage gas, base fee, size and
   events.

## Use

```sh
cd lez
export RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1    # as for the harness (RESULTS.md §8)
cargo build -p harness --locked --bin ferrata-attest
RISC0_DEV_MODE=1 target/debug/ferrata-attest [--follow-to P] [--message-out DIR] COMMAND [ARGS] [--send] [--then COMMAND ...]
```

| Command | Attests |
|---|---|
| `status` | nothing: the instance, its store, the newest record, what following cost |
| `config` | nothing: the instance's configuration as the zone holds it (chain id, committee, genesis, the fork schedule, the layout at `Init`), with the instance id derived again from it and the checkpoint |
| `advance --period P` / `advance --finality` | the update of period P (1378–1409) or the attestation fixture's finality update, as the public job: `Open`, `Miller`, `Adopt` in `adoption_parts` parts when it adopts a committee, `Finish`. The dry run settles the whole job on one scratch state, step after step, and gives each step's cost and the verdict of step 1 (`ferrata_core::steps::step1`, the program's function) with the job's planned outcome |
| `slot [--address A] [--slot S] [--expect-eq V]` | a storage slot (default: USDC's `totalSupply_`, slot 11); `--expect-eq` adds the condition `value == V` |
| `account [--address A] [--expect-code-hash H]` | an account's nonce, balance, storage root and code hash (default: Multicall3) |
| `log [--tx I --log J \| --block-log-index K]` | one log, by its position in its receipt or in the block (default: the block's first ERC-20 `Transfer`, with its emitter and recipient checked) |
| `receipt --tx I` | every log of transaction I's receipt (the receipt must have status 1) |

`--then` chains commands in one run (the zone is followed once). `--follow-to P` stops following at
period P (default 1409, which also applies the attestation fixture's finality update), so that
`advance` has updates left; the attestation commands need that record (`advance --finality --send`). `--message-out DIR` writes each
message as `message-N.hex` (borsh) and `message-N.debug` (its Rust `Debug` text), which the IDL
check decodes with the IDL alone:

```sh
python3 -I idl/check.py --decode ferrata_attestation Message DIR/message-2.hex DIR/message-2.debug
```

Exit status: 0 attested (or would be); 3 refused, as the dry run predicted; 1 the dry run and the
execution disagree; 2 a usage error. An address or slot without a recorded proof is an error that
lists what the recorded data holds.

## Results

One run on the Modal build box, 11 November 2026 ([`results/attest-cli/`](results/attest-cli/):
the output, the messages, the exit status). Measured, in the harness's zone (dev mode): execution
gas is LEZ's metered cycles; storage gas is the transaction's borsh size; the base fee is at the
harness's opening fee state.

Following Sepolia: 166 transactions, all applied, 525,494,412 execution gas and 1,776,529 storage
gas in all, 100.9 s; the record `header(id, 11,549,664)` binds block 12,105,936.

| Command | Message bytes | Verdict | Execution gas | Storage gas | Sent: equal to the dry run |
|---|---:|---|---:|---:|---|
| `slot` (USDC `totalSupply_`) | 7,201 | 2 statements (the account, the slot) | 276,607 | 7,599 | yes, applied |
| `slot --expect-eq 0x01` | 7,235 | refused, 130 `PredicateFailed` | 255,482 | 7,633 | yes, reverted with 130 |
| `log` (first ERC-20 `Transfer`, emitter and recipient checked) | 2,153 | 1 statement | 95,412 | 2,551 | yes, applied |
| `receipt --tx 1` (11 logs) | 31,654 | 11 statements | 1,067,000 | 32,052 | yes, applied |
| `receipt --tx 56` (39 logs), dry run only | 345,894 | 39 statements | 9,968,157 | 346,292 | not sent |
| `log --tx 84 --log 0` (a failed transaction) | 1,897 | refused, 124 `ReceiptFailed` | 71,660 | 2,295 | yes, reverted with 124 |
| `account` (Multicall3) | 4,291 | 1 statement | 168,116 | 4,689 | yes, applied |
| `log --block-log-index 100` (tx 51, log 10) | 7,794 | 1 statement | 224,310 | 8,192 | yes, applied |

`advance`, in a second run that followed only to period 1408 (93.5 s; [`results/attest-cli/advance.txt`](results/attest-cli/advance.txt)):

| Command | Transactions | Verdict | Execution gas | Storage gas | Sent: equal to the dry run |
|---|---|---|---:|---:|---|
| `advance --period 1409` | `Open` 2,123,138; `Miller` 3,310,385; `Adopt` 3,408,020 and 3,415,212; `Finish` 3,728,633 | step 1 accepts; the job advances the finalized header (slot 11,537,760 → 11,542,528), adopts the next committee and rotates | 15,985,388 | 54,785 | yes, 5 applied |
| `advance --period 1409` again | `Open` 144,160 | refused at step 1 with 52 `NotNewer` (duplicate or stale) | 144,160 | 3,899 | yes, reverted with 52 |
| `advance --finality` | `Open` 2,127,275; `Miller` 3,305,289; `Finish` 3,718,605 | advances to slot 11,549,664 (block 12,105,936) | 9,151,169 | 5,129 | yes, 3 applied |

Then `log --send` attested the first `Transfer` under the new record, as in the first run (95,412).

`config` ([`results/attest-cli/config.txt`](results/attest-cli/config.txt)): the instance's state on
the zone, chain 11,155,111, 512 keys, Sepolia's fork schedule (Gloas from epoch 353,024), layout at
`Init` Electra, 34 jobs applied; its id derived again from the configuration and the checkpoint
equals the instance id. `config --send` is refused as a usage error (exit 2).

Every dry run's verdict (the program's function, natively) equalled the zone's execution (exit
code; the `FRT-ATT1` event's identifiers); every sent transaction (7 of the 13 commands) equalled its dry run in exit
code, cycles, execution and storage gas, base fee, size and events. The IDL alone decoded messages
2, 6 and 8 (7,201, 2,153 and 31,654 bytes) to their Rust `Debug` text.

The wire carries one receipt proof per log, so attesting a whole receipt repeats its proof: the
39-log receipt is 345,894 bytes and 9,968,157 cycles, just under v0.4's 10M execution gas cap.

## On a running sequencer: `ferrata attest`

The same commands against a running LEZ v0.4 sequencer are in the `ferrata` tool (`cli/`, source
[`../cli/src/attest.rs`](../cli/src/attest.rs)): `ferrata attest SUBCOMMAND`, built with `cargo build --release` in
`cli/` (LEZ v0.4.0-rc1's wallet library and node-side crates, as the tool's other sequencer commands).

```sh
F="ferrata attest --lez-rpc http://127.0.0.1:3040 --program 0x<program account> --key payer.key --state-dir follower/ --record run.jsonl"
$F config  --instance 0x<id> [--checkpoint 0x<root>]          # the instance's configuration; its id derived again
$F status  --instance 0x<id>                                  # store, finalized slot and root, newest record
$F init    --preset mainnet|sepolia --bootstrap bootstrap.ssz --checkpoint 0x<root> [--layout electra|gloas] [--parts 2] [--send --yes]
$F advance --instance 0x<id> --update update.ssz [--finality] --committee bootstrap:FILE|update:FILE ... [--send --yes]
$F attest  --instance 0x<id> --record-slot S --eth URL --block N \
           [--storage ADDRESS:SLOT[:eq|ne|ge|gt|le|lt:VALUE]] [--account ADDRESS[:codehash]] [--log TX:LOG[:emitter=ADDRESS]] \
           [--header-rlp FILE] [--send --yes]
```

- **Reads** come from the zone: the tool follows the sequencer's blocks with LEZ's own block application
  (`lez/live/driver/src/follower.rs`, included), only those final on Bedrock unless `--wait-final no`, and checks each
  actor state it reads against the sequencer's `getAccount`. `--state-dir` keeps the followed state between runs.
- **Every command that sends is a dry run** unless `--send --yes` (which may follow the command). The dry run gives the
  verdict of the program's own function natively (`attest::evaluate`; for an update `ferrata_advance::evaluate` and the
  program's step code; for an instance `steps::initialize`), with the program's code and its name from the IDL, and the
  zone's execution: the exact signed transaction settled by LEZ's `settle_transaction` on a scratch copy of the
  followed state (exit code, execution and storage gas, events). The tool checks the two agree.
- **`--send --yes`** sends the same signed bytes, waits for the block that includes each to be final on Bedrock, and
  compares the zone's receipt (the follower's: the sequencer's RPC reports no outcome) with the dry run: exit code,
  execution gas, storage gas, events.
- Proofs come from an Ethereum JSON-RPC endpoint (`--eth`: `eth_getProof`, `eth_getBlockReceipts`); the program checks
  them against the record, so the endpoint is not trusted.
- Exit status: 0 applied (or would be); 3 refused, as the dry run said; 1 the dry run and the zone disagree, or the
  verdict and the dry run; 2 a usage error.

Run on a zone we started (`cli/tests/attest-seq.sh`: crown session A, then the commands in `cli/tests/attest-seq-hook.sh`):
see [Results on a sequencer](#results-on-a-sequencer).

## Not done

- Live endpoints for the in-process tool: `ferrata-attest` reads recorded responses; `ferrata attest` (above) is the
  sequencer and live-endpoint tool.
- The private placement (`VerifyAll`, the proven path) has no command; `advance` sends the public job.
- Accumulator leaves and EIP-2935 hops are in the request format (and in the IDL) but have no
  command.
