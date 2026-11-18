# LEZ v0.3.1: the program model, read from its source

Read on 18 November 2026 from a clone of `logos-blockchain/logos-execution-zone` at tag **v0.3.1** (commit
`1a003bfbfb89fcf987f62975cd08a401169e1b8c`, 10 Oct 2026), at `spike/upstream/lez-v0.3.1` beside the repository.
Paths below are relative to that clone. Tag **v0.3.2** (`ee6ed447`, 28 Oct 2026; the LEZ docs' CLI page says the
testnet runs it, the endpoints page says v0.3.1) differs from v0.3.1 in 22 files, all in the sequencer's Bedrock and
indexer plumbing, `Cargo.toml`'s Logos Blockchain pins, CI and tests: `lee/` (the state machine), `lez/programs/`,
`lez/chain_state/` and `lez/wallet/` are byte-identical, so everything here holds for both
(`diff -rq lez-v0.3.1 lez-v0.3.2 -x .git`).

## How a program runs: plan, then apply, over per-program shards

LEZ v0.3.1 is **not** LEZ v0.4's actor model. An account holds one *shard* of data per program
(`lee/state_machine/core/src/account.rs` `AccountData::shards`, a map program → bytes), plus its nonce. A program
runs in two kinds of call, each a separate zkVM session (`lee/state_machine/core/src/program/mod.rs`):

- **Plan** (`PlanInput`, mod.rs:142): the program's own id, the caller (for a chained call), the accounts as
  `AccountMeta { account_id, is_authorized, program_account_id }` (which shard of which account), and the
  instruction bytes. **No account data.** It returns a `PlanOutput` (mod.rs:560): *effects* (one per shard it
  wants to change, with data for the apply), chained calls (at most `MAX_NUMBER_CHAINED_CALLS` = 10 per
  transaction, mod.rs:17), events, and block-id and timestamp validity windows. `validate_plan` (mod.rs:885) checks
  that the output echoes the input and that every effect names one of the call's shards.
- **Apply** (`ApplyInput`, mod.rs:115): one effect at a time: the shard's current data (`pre_data`) and the effect's
  data. It returns the shard's new data or `None` to keep it. `validate_apply_output` (mod.rs:932) refuses a write
  to another program's shard (`ForeignShardWrite`, mod.rs:692) and any chained call from an apply. An apply cannot
  emit events.

So: **a program reads state only in an apply, and an apply sees exactly one shard.** A program may write its own
shard of *any* account it is given (no signature needed); the runtime does not restrict this beyond system accounts
(`lez/common/src/transaction.rs:472` `validate_user_state_modification`: clock, fee, bridge, inbox). Ownership and
squatting are the program's business: Ferrata's program accepts each shard only at the address its instruction
implies, a public PDA (`AccountId::for_public_pda(program, seed)`, mod.rs:242).

A transaction is atomic: any failing call reverts every effect (`lee/state_machine/src/validated_state_diff/mod.rs`
129-184). How a guest is entered and reads its call: `read_input_frame` / `read_program_call` (mod.rs:775-806); the
output is a borsh frame committed to the journal (`GuestOutput::write`, mod.rs:650).

## Limits

| | value | source |
|---|---|---|
| shard data | 100 KiB | `lee/state_machine/core/src/account/data.rs:7` `DATA_MAX_LENGTH` |
| execution per transaction | **10,000,000 gas** (the declared `gas_limit` may not exceed it) | `lez/programs/fee/core/src/market.rs:15` `MAX_GAS_EXEC`; `validity.rs:23` `GasLimitAboveCap` |
| execution per block | 10,000,000 gas (target 5,000,000) | market.rs:10, 15; `validity.rs` `accumulate_exec_gas` |
| storage per block | 1,000,000 bytes of transactions (target 500,000) | market.rs:27 `MAX_GAS_STOR` |
| transaction size | `max_block_size` − 2,048 bytes (the debug config: 1 MiB) | `lez/sequencer/actors/rpc_server/src/actor/service.rs:57`; `lez/sequencer/core/src/config.rs:24` |
| transactions per block | `max_num_tx_in_block` (debug config: 20) | `lez/sequencer/service/configs/debug/sequencer_config.json:3` |
| chained calls | 10 per transaction | mod.rs:17 |
| program size | 20 segments of 96 KiB of the user ELF (1,966,080 bytes) | mod.rs:20; `lez/programs/program_loader/core/src/lib.rs:26` |

## Gas and fees

- **Gas is RISC Zero cycles.** Every guest call runs with `session_limit` = the cycles left of the transaction's
  budget (`lee/state_machine/src/program/mod.rs:112`) and reports `SessionInfo::cycles()` (mod.rs:133); the
  calls of one transaction (plans and applies, chained calls included) share the budget. The budget is the
  transaction's declared `gas_limit` (`lez/chain_state/src/apply.rs:436-446`, `charged_cycles =
  charge.cycles.min(gas_limit)`). The public testnet's fee state, read 18 Nov 2026: `max_gas_exec` 10,000,000,
  `max_gas_stor` 1,000,000, base fees 8 and 8 (`getFeeState`).
- **Every user public transaction must declare a fee** (`lez/chain_state/src/classify.rs:81`
  `MissingFeeDeclaration`): `FeeDeclaration { payer, gas_limit, tip, max_fee }` (`lee/state_machine/src/fees.rs`),
  signed by the payer, who must be in the witness set. Settlement (`lez/chain_state/src/apply.rs:323`
  `settle_transaction`): reserve `gas_limit·base_fee_exec + bytes·base_fee_stor + tip` from the payer
  (`lez/programs/fee/core/src/assess.rs:75`), run the action metered at `gas_limit`, refund the difference with the
  actual charge `cycles·base_fee_exec + bytes·base_fee_stor`. Base fees start at 8 (market.rs:17) and move ±12.5% per
  block around the targets.
- **Refusals.** A guest that exits with a non-zero code is charged the cycles it used; a panic, running out of gas or
  a transaction outside its validity window is charged the **whole gas limit**
  (`validated_state_diff/mod.rs:166`). A chargeable failure keeps the transaction in the block with no effect but
  the payer's nonce and fee (`error.rs:106` `is_chargeable`). Ferrata's program therefore exits with codes, never
  panics, on bad input.
- **Validity windows** are checked against the block's timestamp, in milliseconds
  (`lee/state_machine/src/validated_state_diff/public_backend.rs:117-121`; the sequencer stamps blocks with its
  clock, `lez/sequencer/core/src/lib.rs:1679`).
- **Block building.** The sequencer screens a transaction's *declared* gas against the block's remaining budget and
  defers it to the next block if it does not fit (`lez/sequencer/core/src/lib.rs:1798`): a transaction declaring the
  10M cap is the first of a block.
- **The RPC reports no outcome.** `getTransaction` returns the transaction and its block only
  (`lez/sequencer/service/rpc/src/lib.rs:72`); blocks carry no receipts or events. A verdict must be readable from
  account state (`getAccount`, rpc lib.rs:98), which is how Ferrata's program writes its verdicts.

## Deploying a program

Through the native `program_loader` (`lez/programs/program_loader/core/src/lib.rs`): one `WriteSegment`
(lib.rs:38, no authorization, into an empty account's loader shard) per 96 KiB chunk of the **user ELF** (the
wallet refuses a binary whose kernel is not the default `V1COMPAT_ELF`,
`lez/wallet/src/program_facades/program_loader.rs:186`), tail first, then `CreateHeader` (lib.rs:49) at an
account whose key signs; the loader recomputes the image id from the chain. The program's id is that header
account, chosen by the deployer (`ProgramHeader`, mod.rs:390). The loader is native: its transactions charge no
cycles, only storage bytes. The wallet does all of it (`wallet program-loader deploy`, facade `deploy`
program_loader.rs:139); its transactions declare the wallet's default gas limit, 2,000,000
(`lez/wallet/src/lib.rs:61`). Cost measured here (through v0.3.1's block application, with a 100,000 gas limit): 7 transactions, 567,988 bytes, 4,543,904 native units at base
fee 8 (`results/mainnet-residue.tsv`).

## Funds on the public testnet

The docs name no faucet for v0.3.x. `logos-docs` `docs/lez/get-started/run-lez-wallet-via-cli.md`, "Fund the sender
account": bridge tokens from the Logos Blockchain with a channel deposit (Basecamp; the account id without its
`Public/` prefix in Metadata), or have another testnet user transfer native tokens. Endpoints:
`docs/get-started/testnet-endpoints.md` (RPC `https://testnet.lez.logos.co`, explorer
`https://explorer.testnet.lez.logos.co/`, release v0.3.1, channel `0101…01`). The wallet's `testnet` network is the
same RPC (`lez/wallet/src/cli/network.rs:5`).

## Running a sequencer

`sequencer_service` with the feature `standalone` runs with a mock Bedrock actor (`lez/sequencer/service/Cargo.toml:65`
`standalone = ["sequencer_bedrock_actor/mock"]`; README "Standalone mode"): no Logos Blockchain node. Its gossip
network is optional (`gossip: None` runs alone); in the Modal sandbox its QUIC transport fails ("Operation not
supported"), so the local config drops it. Genesis: `supply_account` actions fund accounts.

## Versions

risc0-zkvm 3.0.5 (`Cargo.toml:149`), guests built with RISC Zero's Rust 1.91.1 (`Justfile:27`
`r0.1.91.1`), host Rust 1.98.1 (`rust-toolchain.toml:4`): the same as this repository's `lez/` (v0.4.0-rc1).
