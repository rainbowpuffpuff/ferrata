# R.11 on LEZ v0.4.0-rc3's own sequencer: several private reads in one block

RFP-021, Reliability 11: "Statement submissions are independent of one another, so that multiple private transactions
can submit statements in the same block, and a private transaction can compute its resulting state before inclusion."

The in-process test (`lez/harness/tests/security_privacy.rs`, [`../../lc-security/r11-one-block.md`](../../lc-security/r11-one-block.md))
settles four private attestations in a block loop of its own. Here the block is made by LEZ's own sequencer.

## What ran

One session on 22 November 2026, 04:10–04:15 UTC, on a Modal build box (a CPU container under gVisor;
[`environment.md`](environment.md)):

- **Sequencer:** LEZ v0.4.0-rc3's `sequencer_service`, built from the tag (`bafd07be`, tree unmodified) with LEZ's
  `standalone` feature: its mock Bedrock, so no Logos Blockchain node runs and no block becomes final (the driver
  follows the head). LEZ's `local.json` with a 6 s block interval ([`config/`](config/)).
- **Dev mode:** the sequencer runs with `RISC0_DEV_MODE=1`, LEZ's Justfile's development mode: it takes fake receipts
  and does not verify them ([`config/sequencer-process.txt`](config/sequencer-process.txt)); the wallet makes fake
  receipts. Everything else is the real path: the circuit and the programs execute, settlement and the block builder
  are rc3's own.
- **Programs:** the final programs of [`docs/image-ids.md`](../../../../docs/image-ids.md) (the attestation program
  `0x816d1fef…`, the reference consumer `0x8464603b…`, LEZ's `scripted`), deployed through rc3's program loader
  ([`programs.tsv`](programs.tsv)); sha256 and `r0vm --id` checked before use.
- **Client:** the live driver's phase `attest-block` ([`lez/live/driver/src/block.rs`](../../../live/driver/src/block.rs)),
  every transaction built by LEZ v0.4.0-rc3's wallet library and sent over the sequencer's RPC; command:

  ```sh
  LEZ_TAG=v0.4.0-rc3 FERRATA_LIVE_BEDROCK=mock FERRATA_LIVE_BLOCK_TIME=6s FERRATA_LIVE_SEQUENCER_DEV_MODE=1 \
    FERRATA_PROGRAM_BIN=<ferrata_lez.bin> FERRATA_SCRIPTED_BIN=<scripted.bin> FERRATA_LIVE_OUT=<dir> \
    FERRATA_LIVE_DRIVER_EXTRA="--consumer-bin <ferrata_consumer.bin>" \
    FERRATA_LIVE_PHASES="info deploy attest-block export" lez/live/run.sh run
  ```

  in a copy of the tree with rc3 selected (`lez/scripts/lez-select.sh v0.4.0-rc3`), after `run.sh build-standalone`
  and `run.sh build-driver`.

## What it did (measured; every figure is in [`run.tsv`](run.tsv), [`chain.tsv`](chain.tsv) and [`r11-one-block.json`](r11-one-block.json))

| Blocks | What |
|---|---|
| 2–27 | the attestation program deployed twice (`main`, `twin`), `scripted` and the reference consumer |
| 28–33 | instance S of `main` initialised from the harness's Stage 4 fixture (`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`, period 1907) and its finality update applied (`Open`, `Miller`, `Finish`): the record of slot 15,624,480, mainnet block 26,384,851 |
| 34–36 | instance E of `main` initialised from the mainnet end-to-end fixture (`mainnet-e2e-2026-11-09`, period 1906) |
| 37–40 | four new private accounts of one wallet, each funded with 1,000,000,000 by a native transfer from a public test account (LEZ's `NativeTokenTransfer`, as LEZ's own `funded_private_account`) |
| 40–41 | **the snapshot**: six `AttestPrivately` transactions built and proven against commitment root `ba64320a…` in 5.6 s, none sent (the client held them): the four new accounts, each reading the record and verifying another ERC-20 `Transfer` of block 26,384,851 (transaction 0 log 0, USDC; 1/0, WETH; 2/0, USDC; 3/2, token `0x35eb…f989`), and the control's two (LEZ's test private account 0, in two wallets that know nothing of each other, the same `Transfer` as the first reader) |
| 42–45 | **between proving and inclusion**: the control's first transaction applied (block 42; the commitment root moves to `02d14c57…`); instance E verifies its finality update and writes its record of slot 15,619,456 (`Open`, `Miller`, `Finish` in blocks 43–45) |
| 47 | **the four in one block**: sent back to back within 2 ms just after block 46 was made; all four applied in block 47, positions 0–3, each 422,032 execution gas, 224,063 storage gas and a base fee of 5,168,760 (the in-process test's figures). Block 47 holds six transactions (the four and the sequencer's clock and fee transactions), 1,688,128 execution gas and 896,252 storage gas: LEZ's cap is 1,000,000 a block (`MAX_GAS_STOR`) at a flat 224,063 a private transaction, so four is the most one block takes. The record's account is unchanged; the 28 commitments the four computed before inclusion (7 each) are all in the sequencer's tree after it (root `12e8d9aa…`). Nothing was proven again |
| — | **control**: the control's second transaction, from the same snapshot as its first, sent after block 47: dropped by the sequencer, "Commitment … already seen" — spending the same private state twice makes the same new commitment, which is refused (why it is the same: [`../../lc-security/f8-full-diff.md`](../../lc-security/f8-full-diff.md), "Those fields carry nothing of the account") |

48 transactions sent: 47 applied, 1 dropped (the control's second); 51 blocks, 6.0 s apart; execution base fee 8 in
every block; the storage base fee rose from 8 to 9 for block 48, block 47 being over the 500,000 storage target.

## What this shows, clause by clause

- **Several private transactions submit statements in the same block:** four private attestations, one per account,
  each a statement about another log of the same recorded block, applied by rc3's own block builder in one block,
  with no conflict; one more would not fit LEZ's block storage cap, not because of anything Ferrata does.
- **A private transaction computes its resulting state before inclusion:** each transaction was complete (its
  nullifiers, its new commitments, its encrypted notes, the reply it expects from the record) when it was proven,
  36 to 42 s and six blocks before inclusion (the block timestamps of 41, 40 and 47); the chain moved in between (a private transaction moved the commitment tree; the
  attestation program verified a finality update and wrote a record of another instance), and the four were applied
  as proven, each of their commitments in the tree after. Records are never written again and reads write nothing,
  so the reply each proof predicted is the reply settlement gets.

## What it does not show

- **Dev mode:** the receipts are fake and the sequencer did not verify them. Real proofs of the same transaction kind
  on rc3's sequencer without dev mode: EVIDENCE row 123 (`../real-proofs/`, the private attestation proven in 279.9 s).
- **No Bedrock:** the mock settles no block on a Logos Blockchain node, so nothing here is final in that sense; the
  runs of `../real-proofs/` and `lez/live/results/` are on a node.
- **The finality update between proving and inclusion is another instance's** (E): no recorded update follows the
  attestation fixture's in its period. The in-process test applies a later update to the instance read itself.
- One session, four readers, one control.

The fixtures' checkpoints leave their weak-subjectivity periods at slots 15,727,008 (E, 24 November 2026, 19:21:59
UTC) and 15,735,200 (S, 25 November 2026, 22:40:23 UTC) (derived: `weak_subjectivity_end_slot` with the mainnet
configuration's `ws_period_epochs` of 3,532); after that, LEZ refuses `Init` from them on the validity window the program declares, and a rerun
needs fresher fixtures.

## Re-check

`verify-run.sh` replays the record ([`verify-run.log`](verify-run.log)): 51 blocks, the chain links hold, every block
hash holds once the 22 elided program chunks are put back, the 51 blocks applied by rc3's `chain_state` from the
pre-genesis state, the 47 included rows equal what LEZ's settlement charges, the four program images as recorded. The
receipts are fake, so the replay runs in dev mode as the sequencer did:

```sh
LEZ_TAG=v0.4.0-rc3 FERRATA_VERIFY_DEV_MODE=1 lez/live/verify-run.sh --dir lez/results/v0.4.0-rc3/r11-one-block \
  <ferrata_lez.bin> <scripted.bin> <ferrata_consumer.bin>
```

## Files

| File | What |
|---|---|
| `r11-one-block.json` | the phase's own summary: the snapshot, what moved in between, the block and its four transactions, the control |
| `run.tsv`, `tables.md` | one row per transaction sent (block, position, status, gas, fee, the sequencer's words for the dropped one) |
| `chain.tsv`, `blocks.borsh`, `segments.tsv` | every block as the sequencer served it (program bytecode elided, its chunks listed) |
| `private/block/` | the transactions the client held, as proven: `reader-0..3.borsh`, `control-0.borsh`, `control-1.borsh`; `funding.borsh` is the last funding transaction as the wallet built it (all four are in `blocks.borsh`) |
| `programs.tsv`, `instances.tsv` | the deployments, and instance S as the chain holds it at the end |
| `config/`, `logs/`, `environment.md`, `summary.txt` | the sequencer's configuration and environment, the sequencer's and the driver's logs |
