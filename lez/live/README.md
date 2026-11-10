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

## The recorded run

One invocation of `run.sh run` on 10 November 2026, 16:58–17:30 UTC, with the scripts of commit
`9582b15`, then `verify-run.sh` once on the same machine
([`results/verify-run.log`](results/verify-run.log)). Every figure below is in
[`results/tables.md`](results/tables.md), `results/environment.md` or the logs of `results/`.

| | |
|---|---|
| chain | 191 blocks, every one `Finalized` on Bedrock when the record was written, 10.00 s apart; 97 hold no user transaction (most of them made while the proof ran) |
| transactions | 96 sent by the wallet: 73 applied, 19 refused by the program with its exit code, 2 reverted otherwise (an adoption sent with the wallet's default gas limit; a job opened before its signature slot), 2 dropped by the sequencer (the first two private transactions below); none rejected at the RPC, none left unfinished |
| deployment | three images through `program_loader`: 16 transactions in blocks 2–17, execution gas 0. A 508,284-byte image takes 6 (5 `WriteSegment`, 1 `CreateHeader`): storage gas 479,496, base fees 3,835,968 |
| against the harness | 65 transactions have a row in `../results/stage1-*.md`. Execution gas is equal in 64; the other ran outside its validity window and is charged its declared limit (4,000,000 here, 10,000,000 declared in the harness). Transaction bytes (storage gas) are equal in 44; 19 are 65 bytes longer here and one is 65 bytes shorter (not traced to a field), and the validity-window case is 607 bytes shorter (built differently: the harness sets the block time, here the instance's slots start ten years later) |
| fees | execution base fee 8 in every block but one: block 92, the only block over the 5M target (two transactions, 7,270,956), raised it to 9 for block 93. The payer's balance on the sequencer fell by exactly the fee in 77 of the 77 transactions where it was read before and after |
| the private path | (i) the GPU-proven transaction `../results/stage2-private-tx.borsh`: dropped; the sequencer's words: "failed settlement: State transition failed at transaction 0: Invalid input: Unknown program Ev5SfFrpL4nCs5iccBFMXFmUvz5ckzXz37Vgy4B6GT1h, skipping it" (it was proven against the harness state, where the program sits at another account). (ii) The same update built for the live instance with a fake receipt (the wallet in dev mode): dropped, "Invalid privacy preserving execution circuit proof, skipping it". (iii) The same update with a real proof made on this machine's CPU (32 prover threads; proven and sent in 895.6 s; receipt 237,412 bytes, transaction 251,763): verified by the sequencer and applied in block 191, settled 17.6 s after it was sent, at execution gas 479,854, storage gas 224,063 and fee 5,631,336, the harness's figures; the instance then held the native store |
| Bedrock | the sequencer's blocks were made 3 s after the seconds at which an epoch can begin (181 blocks; 10 at 4 s), never closer than 6 s before an epoch began (20 epochs began); no head rewound, no inscription given up; the node's storage gas price rose 1 → 2 → 3 → 4 |
| re-check | `verify-run.sh` with the two guest binaries: every block hash holds with the bytecode put back; the 191 blocks replay through LEZ's `chain_state` from the pre-genesis state; the 94 included rows equal what LEZ's settlement charges; the three image ids are as recorded; instance A applied 6 jobs and holds finalized slot 15,589,599 with root `5d1200f2…d573` |
| machine | a rented Linux container under gVisor, 64 GiB; it reports 2 CPUs, but the whole 34-minute command used 7.20 core-hours of CPU time |

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
  92 of the 94 included transactions the sequencer's block total is that transaction's charge;
- the payer's balance is read from the sequencer before and after each transaction, and must
  fall by the fee;
- the instance's store is read from the sequencer after every job and must equal the compact
  form of the native light client's store, and each header record the native finalized header's.

## What the running system showed that the harness could not

- **The head is not the chain.** In the first recorded attempt (15 s blocks, the driver following
  the sequencer's head), the sequencer rewound its head from block 37 to block 16 and the driver
  failed ([`observations/rewind/`](observations/rewind/)). The runs of
  [`observations/epoch-boundary/`](observations/epoch-boundary/) narrow it to the second before a
  Bedrock epoch begins: with the sequencer's blocks made 1 s before, its head was rewound once and
  it gave up 31 inscriptions; made 3 s or 5 s before, never. The node raises its storage gas price
  at the epoch rotations that follow inscriptions (1 → 2 → 3 → 4 in the recorded run).
  `run-live.sh` gives our reading of the node's source: an inscription funded just before a
  rotation is left out after it, every later inscription chains on it, and after 300 slots the
  sequencer gives them all up and rewinds. So the recorded run settles a transaction only in a
  block `Finalized` on Bedrock, and keeps the sequencer's blocks out of an epoch's last seconds.
  Every client of a LEZ zone has to do the first; the second is the sequencer operator's.
- **Finality took 5–6 s.** Each of the 190 blocks after genesis was reported `Finalized` 5 or 6 s
  after the sequencer created it (whole seconds of its log; at the end the node's last
  irreversible block was 5 slots behind its tip), and a public transaction was settled 8.5–19.7 s
  after it was sent (mean 10.3 s, 78 transactions). This is one validator with leader proofs in
  dev mode, not a network.
- **A proof is bound to the deployment it was made for.** The GPU-proven transaction, valid in
  the harness, is refused by the sequencer as "Unknown program": the harness deploys the program
  at genesis, the zone through `program_loader`, and the program's account differs. A real proof
  has to be made against the live instance; on this machine's CPU that took 895.6 s, and the
  sequencer verified and applied it.
- **The wallet's default gas limit is too low for this program.** An adoption sent with LEZ's
  wallet default (2,000,000) reverted ("Execution exceeded its cycle budget of 2000000 cycles")
  and paid 2,000,000; a job's transactions have to declare their limit (the largest needs
  3,730,145).
- **The base fee moves.** Two 3.6M transactions in block 92 put it over the 5M target and the
  next block's execution base fee was 9, not 8; the harness settles everything at genesis fees.
- **Deployment is a series of transactions, not a genesis entry.** What the harness could not
  measure: 6 `program_loader` transactions per 508 KB image, storage gas 479,496, execution gas 0.

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
| `results/summary.txt` | the driver's summary at the end of the run |
| `results/verify-run.log` | the output of `verify-run.sh` with the guest binaries, run once after the run on the same machine (paths shortened) |
| `observations/` | runs that are not the record: what the three programs logged when the head was rewound, and the runs made to find out why |

`blocks.borsh` holds no program bytecode because the guest binaries embed the absolute paths
of the machine that built them (`../RESULTS.md` §3). Putting the chunks back from the binaries
reproduces the blocks byte for byte, which the block hashes prove.

`environment.md` says the driver was built "at HEAD (uncommitted paths: 18)": the machine's copy
of the repository has the files but no git history, so `git rev-parse HEAD` printed `HEAD`. The
sources were those of commit `9582b15` (the tree was clean); the run's command rebuilt the driver
from them first.

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
