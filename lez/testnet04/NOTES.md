# How a program is deployed and paid for on a LEZ v0.4 network, read from source

Read on 18 November 2026 from LEZ **v0.4.0-rc3** (`bafd07be`, the newest tag; `spike/upstream/lez-v0.4.0-rc3` beside the
repository) and, for what changed since, from the comparison of `v0.4.0-rc3...dev` on GitHub (`dev` is LEZ's default
branch; `main` is still v0.3.1 at `1a003bf`). Paths are LEZ's. Line numbers are rc3's. What [`day1.sh`](day1.sh) does
with each fact is said beside it.

## 1. Deploying a program: `program_loader`, immutable

- A program is deployed by LEZ's native **`program_loader`** at `PROGRAM_LOADER_ACCOUNT_ID = [0xFE; 32]`
  (`lee/state_machine/core/src/program/mod.rs:29`). Its messages (`lez/programs/program_loader/core/src/lib.rs:28-46`):
  `WriteSegment { bytecode, next_segment }`, `CreateHeader { first_segment, immutable }`, `UpdateHeader { .. }`.
- The user ELF is cut into chunks of `MAX_SEGMENT_DATA_LEN = 96 KiB` (`lib.rs:23`, a client-side choice), one segment
  account each, **written last first**: `next_segment` must already hold a segment (`lib.rs:114-117`). The attestation
  program (627,064 bytes) is 7 segments; with its header, 8 transactions.
- **Every target must be empty and authorized** (`lib.rs:105-112` for `WriteSegment`, `lib.rs:129-133` for `CreateHeader`): the
  account's own key signs. So a third party cannot write one of our segments first (on v0.3.1 it could:
  `lez-v03/README.md`, "What a third party can do"). The accounts must be **fresh keys**: the live driver's keys are
  derived from public labels (`lez/live/driver/src/keys.rs`, `derived_key`), fine on a zone of our own, not on a
  public one. day1.sh makes random keys (`ferrata-day1 keygen`, mode 600, beside the repository, never in it).
- `CreateHeader` walks the segments and records the image id (`ProgramHeader { image_id, program_first_segment,
  immutable }`, `lee/state_machine/core/src/program/mod.rs:516-523`). **`immutable: true`**: `UpdateHeader` is then
  refused (`lib.rs:143-146`, "UpdateHeader target is immutable"). The program's account is the header account.
- The loader assumes RISC Zero's v1compat kernel: LEZ's wallet refuses another (`lez/wallet/src/program_facades/
  program_loader.rs`, `UnsupportedKernelElf`). The image id is therefore a function of the user ELF and that kernel:
  **a network on another RISC Zero release whose kernel differs would give our binary another image id** (see 6).
- The wallet's facade sends one transaction per segment, paid by a separate account that co-signs without joining the
  message (`lez/wallet/src/lib.rs:2089-2093`, `program_loader.rs` `deploy`), waiting for each to land. day1.sh builds
  the same transactions itself (`ferrata-day1 sign --stage deploy-program`: target's key with nonce 0 and the payer's
  with consecutive nonces, no admission evidence, as the wallet's account manager gives none for signing accounts,
  `lez/wallet/src/account_manager.rs:576-588`), stores them, then sends them in order.
- From rc2 on, deployment pays execution gas (EVIDENCE rows 22 and 112: the final program 1,091,406 execution gas on rc3).

## 2. Funds: genesis, or a deposit from Logos Blockchain through the zone's bridge

- There is no faucet program. **Native tokens enter a zone only through its bridge's `Deposit`**: at genesis, every
  `GenesisAction` (`SupplyAccount`, `SupplyPrivateAccount`, `SupplyBridgeLockHolding`; `StakeSequencer` stakes) "is
  funded through the bridge's `Deposit`" (`lez/sequencer/core/src/config.rs:46-73`), built into the genesis block
  (`lez/sequencer/core/src/lib.rs:2909-2941`, `build_supply_account_genesis_transaction` at `:3280`, `bridge_deposit_tx`
  at `:3289`); afterwards the sequencer's Bedrock actor collects the channel's **finalized deposit operations** and
  mints them on the zone (`lez/sequencer/actors/bedrock/src/actor.rs:225-249`, `FinalizedOp::Deposit`;
  `protocol.rs:115-116`, "Finalized Bedrock deposit events, to record and mint on L2").
- LEZ's own **testnet configuration has no genesis supply**: `lez/sequencer/service/networks/testnet.json` and
  `devnet.json` have `"actions": []`; only `local.json` funds accounts (four `supply_account` actions, LEZ's published
  test keys, which the rehearsal's stand-in uses: `ferrata-day1 fund`).
- The user's way in (logos-docs, `docs/blockchain/transfer-tokens/bridge-assets-from-logos-blockchain-to-zone-on-
  basecamp.md`, written for testnet v0.3, read 18 November): lock Logos Blockchain wallet notes into the zone's channel
  with a **Channel Deposit** whose metadata is the LEZ public account id; "a LEZ deposit without a public account ID in
  Metadata is locked on the Logos Blockchain but never credited on the LEZ"; it arrives "about an hour after it is
  included in a block". The notes come from mining or the Logos Blockchain testnet's own means.
- **What the owner must do first** (day1.sh `plan` prints both account ids and amounts): hold Logos Blockchain testnet
  notes before testnet 0.4 opens, and deposit to the payer's and the daemon key's account ids as soon as the 0.4
  channel id is published. The hour of crediting is the longest step of day one; everything else is minutes.
- The pre-genesis state (`lez/testnet_initial_state/src/lib.rs:230-235`, `initial_state(cross_zone)`) holds LEZ's
  published test accounts. If testnet 0.4 starts from it, those keys (public in LEZ's repository) hold funds anyone can
  spend; `discover` reports their balances through the tool (`inspect` of this deployment's accounts only; not used).

## 3. Fees

- Every public transaction declares `FeeDeclaration { payer, gas_limit, tip, max_fee }` (`lez/wallet/src/lib.rs:2252`);
  the sequencer holds the reserve from the payer before the action runs and drops (does not revert) a transaction whose payer cannot cover it (`lib.rs:2225-2245`); the fee is
  `gas_exec × base_fee_exec + gas_stor × base_fee_stor + tip` (`lez/programs/fee/core/src/assess.rs:82-92`); storage gas
  is the transaction's size. Targets and caps per block: execution 5,000,000 and 10,000,000, storage 500,000 and
  1,000,000 (`lez/programs/fee/core/src/market.rs:10-27`).
- The wallet's defaults: gas limit 2,000,000 (`lez/wallet/src/lib.rs:78`) and a maximum fee sized at base fee 64,
  "8x the genesis minimum, so defaults survive early congestion without re-signing" (`lib.rs:80-88`). day1.sh signs
  each maximum fee at the larger of 64 and 8 times the endpoint's base fee (`ferrata-day1`, `max_fee`): a stored
  transaction stays admissible through a rise in base fees.
- `getFeeState` serves the base fees and caps (`lez/sequencer/service/rpc/src/lib.rs:52-53`); `plan` prices every
  stored transaction with it.

## 4. The wallet and the RPC

- LEZ's wallet defaults to the public testnet: `sequencer_addr: "https://testnet.lez.logos.co"`
  (`lez/wallet/src/config.rs:69-72`); its home is `LEE_WALLET_HOME_DIR` (`lez/wallet/src/lib.rs:70`). Ferrata's code
  always writes its own wallet configuration (`lez/live/driver/src/live.rs`, `open_wallet`) and talks only to a
  sequencer on its own machine unless `FERRATA_SEQUENCER_REMOTE` names the URL; sending there also needs
  `FERRATA_TESTNET_SEND=yes` (`check_loopback`, `check_send`).
- The sequencer's JSON-RPC (`lez/sequencer/service/rpc/src/lib.rs:47-170`): `sendTransaction`, `getFeeState`,
  `checkHealth`, `getBlock`, `getBlockRange`, `getLastBlockId`, `getAccountBalance`, `getTransaction`,
  `getAccountsNonces`, `getAccountView`, `getPublications`, `getMessagePath`, `getRecoveryBinding`, `getProofsAndRoot`,
  `getAccount`, `getProgramIds`, `getChannelId`, cross-zone dead letters. No receipts and no version method: results are
  read by replaying blocks (the follower, `lez/live/driver/src/follower.rs`). v0.3.1's has no `getPublications`,
  `getMessagePath`, `getRecoveryBinding`: `discover` calls `getPublications` to tell a v0.4 network from a v0.3 one.
- The message a signature covers is hashed under `/LEE/v0.3/Message/Public/` (`lee/state_machine/src/public_transaction/
  message.rs:16, 94-96`): **no chain or zone id**. A signed transaction is valid on any LEZ network where its signers'
  nonces match; only fresh keys keep day one's stored transactions to the network they were meant for.

## 5. What a public network may differ in

| | rc3's local network (the rehearsal) | rc3's `testnet.json` | where |
|---|---|---|---|
| channel id | `0101…01` | `0101…01` (the current v0.3 testnet's too, logos-docs) | `networks/*.json` `bedrock_config.channel_id`; `getChannelId` |
| block interval | 15 s (`local.json`; the rehearsal used 10 s) | 10 s | `block_create_timeout` |
| genesis supply | four accounts | none | `genesis.actions` |
| sequencer stake, posting timeframe, timeout, exit delay | 149, 300, 25, 100 | 25,000,000,000, 900, 150, 240 | `genesis.channel_params`; defaults `lez/sequencer/core/src/config.rs:274-277` |
| Logos Blockchain node | 0.3.5-rc.1 (`f6d4267f`), one validator, dev-mode leader proofs | Logos's network | LEZ's resolver |
| cross-zone programs | not configured | not in the file | `getProgramIds`; the follower's pre-genesis state (`FERRATA_FOLLOWER_CROSS_ZONE=1`) |

- **Fork digest.** Logos Blockchain 0.3.5-rc.1 puts a 32-byte fork digest before a transaction's operations; the
  checkpoint guests of the return leg read it only in their second build (`docs/image-ids.md`, "For zones on Logos
  Blockchain 0.3.5-rc.1"; EVIDENCE rows 112, 129). The LEZ RPC does not serve it; `discover` reads the node's
  `/cryptarchia/info` when given a node (`FERRATA_DAY1_NODE`). Day one's stages (a)-(d) do not depend on it.
- **The clock.** LEZ has a clock program (`lez/programs/clock/core/src/lib.rs:9-12`); the attestation program bounds
  `Init` and every update by the instance's weak-subjectivity period through the transaction's validity window, by the
  sequencer's clock (`lez/wire/src/lib.rs`, `Config::ws_end_ms`; EVIDENCE row 112). `discover` compares the head block's
  time with this machine's; `ferrata-day1 sign --stage init` refuses a checkpoint whose period ends within a day.
- **Genesis of the follower.** Ferrata's follower replays a zone from LEZ's pre-genesis state without the cross-zone
  programs, as `local.json` runs (`lez/live/driver/src/follower.rs`, `Follower::new`). A public network configured with
  them needs `FERRATA_FOLLOWER_CROSS_ZONE=1`; one with another pre-genesis state cannot be followed by this follower at
  all (the first block's hash would not hold): `ferrata-day1` then fails at its first read, before anything is sent.

## 6. Since rc3 (`dev`, 78 commits by 18 November; none tagged)

From the GitHub comparison (`gh api repos/logos-blockchain/logos-execution-zone/compare/v0.4.0-rc3...dev`, saved
beside the repository): breaking changes that testnet 0.4 may carry if it runs a later build than rc3:

- `6aaabe5` "chore(deps)!: bump risc0 to 3.1.0": if the loader's kernel or image-id computation moved with it, the final
  programs' ids on that network would differ from `docs/image-ids.md` (the binaries would still be byte for byte ours;
  `verify` compares the header's id with the binary's own and says which).
- `7e0767c` "feat!: upgrade builtin system programs (#946)" and `program_loader` moved to `lee/program_loader` (+136
  lines): the loader's messages and its actor states may change; `discover` compares `getProgramIds` with rc3's
  ([`reference-rc3.json`](reference-rc3.json)).
- `f99ba43` "feat(common)!: add transaction receipts in place of reverted (#1107)": blocks change shape; Ferrata's
  follower, built on rc3's `chain_state`, cannot read them: the tool must be rebuilt against the network's tag.
- `13e3a52` "feat(lee)!: allow public `payer` for pptx (#1080)", `5cebe5a` "feat(fees)!: meter privacy-preserving
  transaction storage by its size (#1044)", `b760e45` "feat(lez)!: remove exit_delay in favor of L1 finality (#1026)",
  `13b7de0` committee voting on LEZ parameters, `6111089` "refactor(wallet): bump default max gas exec to block cap".

So day one starts with `discover`: if the network is not rc3, `ferrata-day1` and `ferrata-updater` are rebuilt against
its tag (`lez/scripts/lez-select.sh` knows rc1-rc3; a new tag needs its commit there and a lockfile patch, and any API
change a `lez-rc4`-style feature) before `run`. The programs themselves are never rebuilt.
