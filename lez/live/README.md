# Ferrata on a running LEZ v0.4.0-rc1 sequencer

`../RESULTS.md` measures the light-client program through LEZ's state machine in a test harness.
This directory runs the same program on a running LEZ sequencer: LEZ's own `sequencer_service`,
built from the unmodified tag, inscribing its blocks on a running Logos Blockchain (Bedrock) node,
with every transaction built, signed and sent by LEZ's wallet library over the sequencer's RPC.
The record of that run is in [`results/`](results/); the numbers are in
[`results/tables.md`](results/tables.md) and in `../RESULTS.md` §7.

There is no public LEZ v0.4 network to run on (`../../docs/design/lez-v0.4-findings.md`,
"Unconfirmed"), so this is a local zone: one sequencer and one Bedrock validator on one machine.

## What ran, and in which mode

| part | what | mode |
|---|---|---|
| Sequencer | LEZ `sequencer_service` at tag `v0.4.0-rc1` (commit `1806e87`), source tree unmodified, built as LEZ's Justfile builds it for `just run-sequencer` (`cargo build --release -p sequencer_service`, default features) | set up and started as the Justfile does (`setup … --create-channel`, then `start`); **`RISC0_DEV_MODE` not set**, so it verifies the receipts of privacy-preserving transactions (`results/config/sequencer-process.txt` is its environment as the kernel reports it) |
| Bedrock | a Logos Blockchain node at `d480efc6`, the revision LEZ's `Cargo.lock` pins, obtained with LEZ's own resolver (`bedrock/tools/resolve_bedrock_node.py`, host mode): no published node release is of that commit, so the resolver built it from source with `--features testing` | run as a process with LEZ's `bedrock/node-config.yaml` and `bedrock/deployment-settings.yaml` and `POL_PROOF_DEV_MODE=true`, exactly what LEZ's container entrypoint does (`bedrock/scripts/run_logos_blockchain_node.sh`). **Not** LEZ's Docker compose: the machine the run was made on has no Docker. One validator; **its leader proofs are in dev mode**, as in LEZ's local Bedrock. 1 s slots, 100-slot epochs, finality 5 blocks deep |
| Configuration | LEZ's `lez/sequencer/service/networks/local.json`; the changes are in `results/config/sequencer_config.diff`: the sockets on loopback, and a block interval of 10 s (the value of LEZ's `testnet.json`) where `local.json` has 15 s (why: "The head is not the chain" below) | at most 20 user transactions and 1 MiB per block: LEZ's values |
| Program | the guest image `cec0bc68…4d78`, the image `../RESULTS.md` measures and the GPU proof is of, byte for byte (see "The guest binaries") | deployed twice through LEZ's `program_loader` by the wallet's `ProgramLoader::deploy`, immutable |
| Client | [`driver/`](driver/): LEZ's `wallet` crate for every transaction (`ProgramLoader::deploy`, `send_pub_tx_paid_by`, `send_privacy_preserving_tx_with_pre_check`), LEZ's `sequencer_service_rpc` client | the payers are LEZ's published test accounts, funded by the genesis of `local.json` |
| Where | a rented Linux container (`results/environment.md`), not the laptop the other measurements were made on | |

The harness calls LEZ's state transition functions in one process. Here nothing calls them
directly for the run: a transaction reaches the state machine only through the sequencer's RPC,
its mempool and its block builder, and counts only once the block that holds it is final on
Bedrock.

TBD(results)

## How results are read: the sequencer has no receipts

A LEZ v0.4.0-rc1 block lists the indices of its reverted transactions and carries the block's
gas totals in its forced fee transaction. The RPC serves nothing per transaction
(`lez/sequencer/service/rpc/src/lib.rs`: blocks, transactions by hash, accounts, balances,
nonces, the fee state): not the gas charged, not the reason of a revert, not an exit code; and a
transaction the block builder drops (an invalid proof, a wrong nonce) is only a line in the
sequencer's log (`lez/sequencer/core/src/lib.rs:1790–1806`). So the driver follows the chain the
way LEZ's indexer does (`driver/src/follower.rs`): it fetches every block from the RPC, settles
the block's user transactions one by one on a copy of the state with
`chain_state::apply::settle_transaction` (the function the sequencer's block builder itself
uses) to read each charge, applies the block with `chain_state::apply::apply_block` (which checks
the hash, the link, the reverted list and the fee transaction against its own execution), and
takes the reason of a revert from `lee::ValidatedStateDiff::from_public_transaction_metered` on
the state the transaction ran against. It applies a block only when the sequencer reports it
`Finalized` on Bedrock. Three checks tie this to the sequencer itself:

- the block's own fee transaction must carry exactly the totals the follower computes, or
  `apply_block` fails; the driver sends one transaction per block wherever it can, so for
  TBD(alone) transactions the sequencer's block total is that transaction's charge;
- the payer's balance is read from the sequencer before and after each transaction, and must
  fall by the fee;
- the instance's store is read from the sequencer after every job and must equal the compact
  form of the native light client's store, and each header record the native finalized header's.

TBD(findings)

## The record

| file | what |
|---|---|
| `results/run.tsv` | the manifest: one row per transaction sent (step, transaction hash, block, position, status, exit code, execution and storage gas charged, gas limit declared, base fees, fee, what the payer's balance fell by, the block's totals, the harness's figures for the same step, LEZ's reason or the sequencer's log line) |
| `results/tables.md` | the manifest as tables (`tables.py`) |
| `results/blocks.borsh` | every block of the chain as the sequencer served it (a borsh `Vec<Block>`), with the bytecode of the 13 `WriteSegment` transactions removed |
| `results/segments.tsv` | the 13 removed chunks: block, position, transaction hash, bytes, SHA-256 |
| `results/chain.tsv` | one line per block: hash, timestamp, transactions, reverted indices, opening base fees, gas totals, Bedrock status when the record was written |
| `results/programs.tsv`, `results/instances.tsv` | the deployed programs (header account, image id as the loader computed it) and the instances as the chain holds them at the end |
| `results/config/` | the sequencer's configuration as it ran, its difference to LEZ's `local.json`, the node configuration's difference to LEZ's, the sequencer's command line and `RISC0_*` environment |
| `results/logs/` | the sequencer's log, the driver's output, an excerpt of the node's log, the node's gas prices |
| `results/private/` | the private transactions that were built and, for each attempt, every RPC answer the wallet built it from |
| `results/environment.md` | versions, the binaries' SHA-256, the machine, ports, the block phase, the proof's duration |
| `observations/` | runs that are not the record: what the three programs logged when the head was rewound, and the runs made to find out why |

`blocks.borsh` holds no program bytecode because the guest binaries embed the absolute paths
of the machine that built them (`../RESULTS.md` §3). Putting the chunks back from the binaries
reproduces the blocks byte for byte, which the block hashes prove.

## Reproduce

Layout as in `../RESULTS.md` §6 (LEZ at `upstream/lez-v0.4.0-rc1` beside the repository, r0vm
3.0.5 on `PATH`). System packages: LEZ's (`build-essential clang libclang-dev libssl-dev
pkg-config`) and `libpcsclite-dev` (LEZ's wallet links keycard support). Everything the scripts
write goes to `tmp/sequencer` beside the repository or to `results/`; ports 3040 (sequencer RPC),
9000 (its metrics), 18080 (node API), 3000 and 3400/udp (the node's peer-to-peer ports), all on
loopback except 3400.

```sh
lez/live/run.sh build            # LEZ's sequencer (about 5 min on 16 cores) and the driver
lez/live/run.sh build-bedrock    # the Bedrock node, by LEZ's resolver (about 10 min on 16 cores)
lez/live/run.sh run              # the whole run; writes lez/live/results/
python3 lez/live/tables.py       # results/tables.md
lez/live/verify-run.sh           # re-check the record (see below)
```

`FERRATA_LIVE_JOBS=16 FERRATA_LIVE_NICE=0` uses a build machine fully (default: 4 jobs under
`nice -n 19`). `run.sh run` starts the node and the sequencer, runs the driver's phases, stops
both and collects the records, in one invocation; nothing it starts survives it. It starts the
zone more than once if the sequencer's blocks do not fall where it wants them against Bedrock's
epochs (see below; `results/environment.md` says how often).
`FERRATA_LIVE_BEDROCK=mock` runs LEZ's `standalone` sequencer (its mock Bedrock) instead, after
`run.sh build-standalone`; `FERRATA_LIVE_BLOCK_TIME=3s` shortens the block interval for a
rehearsal; `FERRATA_LIVE_PROOF_SECONDS=0` skips the real proof and `FERRATA_LIVE_PROOF_THREADS`
sets its thread count; `FERRATA_LIVE_PHASES` selects phases; `FERRATA_PROGRAM_BIN` /
`FERRATA_SCRIPTED_BIN` name other guest binaries (a new image: the accounts of a deployment are
derived from labels, so a re-run deploys at the same addresses on a fresh zone). The LEZ tree is
a parameter (`LEZ_TAG`, `LEZ_SRC`, `LEZ_COMMIT` in `env.sh`).

## Re-check

`verify-run.sh` needs the driver built (it links LEZ's crates), nothing running.

- **Without the guest binaries** it checks what needs no execution: the chain links, the hash
  of every block whose bytes are whole, and the manifest against the blocks: each transaction is
  at the recorded block and position, the block's reverted list agrees with the recorded
  status, the block's gas totals in its fee transaction are the recorded ones and, where a
  transaction is alone in its block, equal its recorded gas and fee.
- **With the two guest binaries** (arguments, or `lez/live/programs/*.bin`, or
  `tmp/sequencer/program/*.bin` beside the repository) it puts the bytecode back, checks every
  block hash, replays the whole chain through `chain_state::apply::apply_block` from the
  pre-genesis state (`testnet_initial_state::initial_state`), and compares every row of the
  manifest with what LEZ's settlement charges; it also checks the deployed image ids and
  instance A's final finalized header against the canonical root.
- **`--rpc http://127.0.0.1:3040`**: a running sequencer must serve the recorded blocks (same
  hashes) and hold the manifest's transactions at the manifest's blocks.

## The guest binaries

The run deploys the exact binary of image `cec0bc68…`, not a rebuild: a rebuild under other paths
is another image (`../RESULTS.md` §3). The binary was taken out of the `prove_private` host
binary that made the GPU proof (`../../gpu-prover/inputs-lez/MANIFEST.json`: SHA-256
`f23fba6a…0a26`), where risc0-build embedded it: 508,284 bytes at offset 647,638, SHA-256
`4b7702a4491ba9c5f9520155ee04e486328902bc7e0b64a2d1214c44fae01777`; LEZ's `scripted` test
program likewise: 268,468 bytes at offset 1,410,142, SHA-256
`94b0f2ab3a87406f9e684fdf4a64b1c9f2c35f1b24e0bb9132097840b42755a5`. The sequencer's loader
recomputes the image id from the uploaded segments; `results/programs.tsv` holds what it
computed. The binaries are not in the repository (they embed the build machine's paths); the
driver takes any `ProgramBinary` file, for instance the ones `cargo build -p methods` leaves in
`lez/target/riscv-guest/methods/*/riscv32im-risc0-zkvm-elf/release/*.bin`.
