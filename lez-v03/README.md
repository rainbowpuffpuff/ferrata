# Ferrata's light client on LEZ v0.3.1, the public LEZ testnet's version

**EXPERIMENTAL AND UNAUDITED. Not the proposal's deliverable: the LEZ v0.4 programs of
[`../lez/`](../lez/README.md) are.** Its one-transaction finality update replaces the final exponentiation by
the residue-witness check of [`../bench/residue`](../bench/RESIDUE.md) and may compose the signing key from one
multiproof of the non-signers ([`../bench/residue-agg`](../bench/RESIDUE.md)): bench code, experimental and
unaudited, which nothing else in this repository deploys. No program or guest with an image id in
[`../docs/image-ids.md`](../docs/image-ids.md) changed.

Every program of this repository targets LEZ v0.4 (the actor model), which has no public network. The public LEZ
testnet runs **v0.3.1** (`https://testnet.lez.logos.co`; its state machine, programs and wallet are the same in
v0.3.2). This directory is Ferrata's Ethereum sync-committee light client as a v0.3.1 program, its tests through
v0.3.1's own block application, a run on a v0.3.1 sequencer, and the script that would run it on the public
testnet. What v0.3.1 is, read from its source with file and line: [`NOTES.md`](NOTES.md).

## The program

v0.3.1 runs a program in two phases: a *plan* that sees the instruction and the accounts' ids but no state and
returns *effects* on the program's own shards of those accounts, then one *apply* per effect that sees one
shard and returns its new data; the transaction is atomic. So the plan here only routes (each account must be
the program's public PDA the instruction implies), and every check runs in an apply:

| account (PDA of the program) | shard | written by |
|---|---|---|
| `instance(id)`, `id = H(config ‖ checkpoint)` | config, the compact store, the job in progress | `Init`, `Update`, `Continue` |
| `header(id, slot)` | a finalized header's record, written once | the transaction that applies the job |
| `statement(id, s)` | an attested storage slot, written once | `Attest` |

An apply sees one shard, so v0.4's separate job actor is impossible: the job lives in the instance's shard. A
record or a statement is written by its own apply, which cannot see the store: the instruction *claims* it, the
apply at the instance (or at the record) checks the claim against what it verified and refuses otherwise, which
reverts the whole transaction. The timestamp validity window, which the plan sets without seeing the store, is
claimed the same way and must equal v0.4's (from the signature slot to the end of the weak-subjectivity period of
the stored finalized header). Refusals exit with a code, never panic: v0.3.1 then charges the cycles used, not
the gas limit.

Instructions ([`wire/src/lib.rs`](wire/src/lib.rs), [`guest/src/main.rs`](guest/src/main.rs)):

- `Init { config, checkpoint, bootstrap, adopt, record, window }`: the same trusted checkpoint and `Config` as the
  v0.4 program; with the committee's 512 keys in `adopt` it is one transaction.
- `Update { instance_id, update, signers, verify, adopt, record, window }`, `verify` one of `Open` (step 1),
  `Miller` (steps 1-2), `Core` (steps 1-3, the core alone), `Residue { c, w }` (steps 1-2 and the residue check).
- `Continue { instance_id, steps, record }`: the job's next steps (Miller, FinalExp, Residue, Adopt).
- `Attest { instance_id, slot, query, statements }`: storage slots of one account under `header(id, slot)`
  (`ferrata_core::attest::attest_storage`, the block's RLP header for a Gloas record), each written to
  `statement(id, s)`.

The verification is `ferrata-core`'s, unchanged (`steps::initialize`, `step1`, `Job::step2`, `step3`, `adopt`,
`apply`, `store_for_update`). Image `8d3b59ff9b4bf6992f80e797e7e25fad1ed372c4b4e1386d96e6dd33ba8e1bb5`, 597,336 ELF
bytes, 6 segments; built through the repository's `scripts/guest-rustc`, so the id does not depend on the build
path.

## Through v0.3.1's block application, with real data

[`harness/tests/v03.rs`](harness/tests/v03.rs): every transaction a public transaction with a fee, settled in a
block as v0.3.1's sequencer settles it (`chain_state::apply::derive_block_summary` and `apply_block`: the fee
reserve, the action metered at the declared limit, the refund, the block caps), from v0.3.1's testnet genesis; the
program deployed through v0.3.1's program loader. Data: the mainnet replay (`mainnet-p1899`) and Sepolia's
`sepolia-gloas-2026-11-10` (bootstrap of period 1378 and every period's update to 1409, through the Gloas fork),
`sepolia-inbound-2026-11-11-1` and `sepolia-vault-v5-2026-11-11` (two finality updates of period 1410, each with
the vault's proofs). Ran on the Modal build box, 18 November 2026; results in [`results/`](results/).

| transaction | execution gas | of the 10M cap | bytes |
|---|---:|---:|---:|
| **finality update in ONE transaction, residue witness** (mainnet update_1, 510 of 512) | **6,647,546** | 66.5% | 4,934 |
| the same, Sepolia (509 of 512, finalized slots 11,551,744 and 11,556,608) | 6,528,660 and 6,530,554 | 65.3% | 4,732 |
| finality update in one transaction by the core alone (mainnet update_4) | 9,103,128 | 91.0% | 3,390 |
| `Init` with the committee's 512 keys adopted, one transaction (mainnet; Sepolia) | 7,975,001; 7,885,607 | 79.8%; 78.9% | 51,196 |
| handoff (period update): `Update` (residue) + `Continue` (adopt 512 keys, apply) | 6,567,497 + 7,707,328 | 65.7% + 77.1% | 3,897 + 49,486 |
| handoff by the core alone, stepwise: Open, Miller, FinalExp, then the adoption | 2,232,918, 3,337,230, 3,670,637, 7,704,852 | | |
| handoff, steps 1-3 by the core in one transaction, then the adoption | 9,114,176 + 7,704,852 | 91.1% + 77.0% | |
| handoff with the multiproof of the non-signers and the residue witness | 6,566,158 + 7,704,852 | | |
| `Attest` of two slots (`depositRoot`, `nextLeafIndex`) under a Gloas record | 526,118 and 512,552 | 5.3% | 6,970 |
| deployment: 6 `WriteSegment` + `CreateHeader` (the loader is native) | 0 | | 567,988 in all |

The v0.4 bench measured 6,629,246 for the same mainnet update in one transaction through LEZ v0.4's state machine
([`../bench/RESIDUE.md`](../bench/RESIDUE.md), EVIDENCE row 127); v0.3.1 charges 6,647,546. A finality update by the
core alone fits one transaction too at this participation; each non-signer adds about 25,000 cycles, so below about
475 signers it does not (derived), and the residue path with the multiproof keeps it under the cap down to the
policy's floor of 342 (row 128, measured on v0.4's state machine). The handoff does not fit one transaction (about 14M): two. Sepolia: 76 transactions
applied, 32 handoffs from period 1378 to 1409, the store upgrading itself to the Gloas layout at 1379, then the two
finality updates and the attestations (slot 0 = `0xa5b5c43f…4875` and slot 1 = 1 of vault `0xd0b5…8af7` at block
12,107,997; slot 0 = `0xfaad1232…579d` and slot 1 = 2 of vault `0xfe31…9e54` at block 12,112,773).

Refused, each with its code and nothing changed: a replayed `Init` (203); a stale update (52, `NotNewer`); an update
replayed as sent (218, its window is the old store's); a participation bit cleared (40, `SignatureInvalid`); a
finalized header changed (26); a tampered residue witness (40); a wrong claimed record (219); a widened window
(218); an update while a verified job waits for its adoption (207); an attestation claiming a wrong value (220),
with a changed proof node (110) or without the block's header under a Gloas record (215). A transaction sent before
its signature slot is outside its window: v0.3.1 includes it and charges the whole declared limit.

## On a LEZ v0.3.1 sequencer

`./testnet.sh local` on the Modal build box, 18 November 2026, 10:35-10:49 UTC: LEZ v0.3.1's own `sequencer_service`
(its standalone mode: a mock Bedrock, no Logos Blockchain node; one sequencer, 10 s blocks), the payer funded at
genesis, every transaction signed and sent with v0.3.1's wallet (its key storage, signing and sequencer client).
Fresh Sepolia data first: the finalized checkpoint at slot 11,605,312 (period 1416, block root `0x2e9a9e27…b379`,
Lodestar's bootstrap checked against publicnode's beacon header), the next two finality updates (finalized slots
11,605,344 and 11,605,376) and `eth_getProof` of the vault `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`, slots 0 and
1, at Sepolia block 12,160,907, the block the second update's finalized (Gloas) header binds. Then all eleven
transactions were signed with explicit nonces and declared limits of the simulated gas plus 3% and 30,000, and
queued at once. The sequencer made exactly the three blocks the simulation predicted:

| block | transactions | execution gas (declared) | bytes |
|---|---|---|---:|
| 4 | 6 `WriteSegment` + `CreateHeader` (no execution gas); **`Init`**, the committee's 512 keys adopted | 7,884,541 (8,160,000) | 567,988 + 50,887 |
| 5 | **finality update 1, one transaction** (residue witness) | 7,046,809 (7,300,000) | 11,260 |
| 6 | **finality update 2, one transaction**; **`Attest`** of the vault's slots 0 and 1 | 7,472,244 (7,730,000); 501,007 (550,000) | 16,636; 6,820 |

Read back over RPC from state (a v0.3.1 transaction reports no status): the program header (image `8d3b59ff…1bb5`,
computed by the loader), the instance (3 jobs applied, finalized slot 11,605,376), the records of slots 11,605,312,
11,605,344 and 11,605,376, and the statements: slot 0 (`depositRoot`) = `0xa67ef5a1…7469`, slot 1 (`nextLeafIndex`) =
2, at block 12,160,907. The payer paid 211,470,107 native units: exactly the simulation's total at the same base fees
(8, 9, 10), so the sequencer charged what the simulation measured. 51 s from queueing to the third block. While the
last two waited behind block 5, the client sent them again once (byte-identical; dropped at the nonce check, nothing
charged). `./testnet.sh verify` then re-read all 18 verdicts: all match. The fresh updates had fewer signers than the
fixtures (their signer witnesses are 11,260 and 16,636 bytes), hence 7.0M and 7.5M instead of 6.5M. Record:
[`results/local/`](results/local/) (`signed/` and `signed.tsv`: every signed transaction; `txs.tsv`: blocks and
verdicts; `record.json`; the data and `requests.tsv`; `batch.json`; the sequencer's configuration).

## The public testnet: prepared, not sent

[`testnet.sh`](testnet.sh) `plan` signs the whole run for a fresh payer (keys under `spike/tmp/testnet-v03/`, never in
the repository) and prints every transaction with its block, size, declared and simulated gas and fee, and the
funds needed and how the LEZ docs say to get them; `run` (refuses unless `FERRATA_TESTNET_SEND=yes`) fetches a fresh
checkpoint, two finality updates and the vault's proofs, signs everything with explicit nonces and declared limits of
the simulated gas plus 3% and 30,000, and queues it at once; `resend` sends the same signed bytes again (after a
rewind); `verify` re-reads every verdict from state and the explorer (PENDING while a block is Pending on Bedrock).
The same sequence is written as testnet-ops's `ferrata_ops batch` file (`batch.json`). Read-only calls only so far:
the public testnet answered `getLastBlockId` 83005 and its fee state (base fees 8 and 8, caps 10,000,000 and
1,000,000) on 18 November; on that day it made a block every few hours and rewound one. **Nothing has been sent to it.**

## What it does not show

- No public run yet: the owner decides it (see the main session's runbook); until then no public LEZ network runs any
  of Ferrata.
- No bridge on v0.3.1, no private path (v0.3.1's privacy circuit is not used), no successors; the LEZ v0.4 programs
  remain the proposal's deliverable.
- The one-transaction finality update rests on the experimental residue check (and, optionally, the multiproof):
  not audited; the core-only path (`Verify::Core`, stepwise `Continue`) is there for the comparison.
- The local sequencer ran with a mock Bedrock (v0.3.1's standalone mode): no Logos Blockchain finality.
- The weak-subjectivity period is the test value of the v0.4 work (3,532 epochs), not computed from Sepolia's
  balance at the checkpoint.

## Re-check

```sh
cd lez-v03
scripts/build.sh --lez                       # program, harness, client; LEZ v0.3.1's sequencer_service and wallet
cargo test --release --locked -p harness-v03 --test v03 -- --nocapture --test-threads 4
./testnet.sh local                           # a v0.3.1 sequencer here, the whole queued sequence, verify
./testnet.sh plan                            # the public run, signed and simulated; sends nothing
FERRATA_SEQUENCER=http://127.0.0.1:3040 ./testnet.sh verify   # against a running sequencer holding the local run
```

Files: [`NOTES.md`](NOTES.md) (v0.3.1 from source), `wire/` (instructions, effects, state, addresses), `guest/` (the
program), `methods/`, `harness/` (the chain and the relayer's side), `client/` (`ferrata-v03`, on v0.3.1's wallet),
`scripts/fetch.py` (public beacon and execution APIs; generic User-Agent; every request in `requests.tsv`),
[`testnet.sh`](testnet.sh), [`results/`](results/).
