# The light client on the public LEZ testnet: the run of 18-19 November 2026

**Experimental and unaudited, and not the proposal's deliverable** (the LEZ v0.4 programs of
[`../../../lez/`](../../../lez/) are; LEZ testnet 0.4 does not exist). This folder is the record of the
one public run of [`../../testnet.sh`](../../testnet.sh): Ferrata's Ethereum sync-committee light client as a LEZ
v0.3.1 program ([`../../README.md`](../../README.md)) on the public LEZ testnet, sequencer
`https://testnet.lez.logos.co` (LEZ v0.3.1), explorer `https://explorer.testnet.lez.logos.co`. Every file is as the
run wrote it; this README is the only addition. Every figure below is read from these files, except what the runs
printed on their consoles and the watcher wrote in its log, neither committed: the payer's balance before each run,
run 1's refusal (`Incorrect fee`), run 2's start, and the watcher's reads of the head and the balance (below).

## What was sent

Eleven public transactions from one payer, `yEnfeJdX68Qrihb5yz3cxcidYZK13aupgYR1T2R3C3R`, with its nonces 0 to 10
([`signed.tsv`](signed.tsv); the signed bytes in [`signed/`](signed/)). Blocks and verdicts from
[`txs.tsv`](txs.tsv); execution gas is the run's simulation on v0.3.1's state machine (v0.3.1's RPC reports neither a
transaction's gas nor whether it succeeded), of the 10,000,000 cap:

| # | transaction | hash | block | bytes | declared gas | execution gas (simulated) |
|---:|---|---|---:|---:|---:|---:|
| 0 | `WriteSegment` 6 of 6 | `b4606d58d403feda163757a166f0fb9535113c4cfb8a6cff21aedf3d728d1767` | 83019 | 73,688 | 100,000 | 0 |
| 1 | `WriteSegment` 5 of 6 | `5d77eed2a979a75a955609f4800eb50f06fb084fb157a44a346a25af3ebe4c4f` | 83019 | 98,696 | 100,000 | 0 |
| 2 | `WriteSegment` 4 of 6 | `9b226680fc577b1a2eb15c329b9583f7e894503123639996530af16c3ecc98cd` | 83019 | 98,696 | 100,000 | 0 |
| 3 | `WriteSegment` 3 of 6 | `52ebe87d2fb2969a5a4fa30c97f8199bcce457592375069b4f94e350e1b6b270` | 83019 | 98,696 | 100,000 | 0 |
| 4 | `WriteSegment` 2 of 6 | `d616532d223541f79b3e99c1e461a5062bf27637a6d2e1f3ff697b77bc6404b5` | 83019 | 98,696 | 100,000 | 0 |
| 5 | `WriteSegment` 1 of 6 | `b49d2dfd59f1cd3a7acdb8eb371cc6becbabcff3f7a560978feac99e908a2d25` | 83019 | 98,696 | 100,000 | 0 |
| 6 | `CreateHeader` (immutable) | `042e117eca845240b7c82a522f55b9a7c2ad9c538ac2236fbd301b0600f756a0` | 83019 | 820 | 100,000 | 0 |
| 7 | **`Init`**, the committee's 512 keys adopted | `834f78cb118c99ad313bc8b4399c4ead535ccd90c6bb8c8b0f496dd75e41e8df` | 83019 | 50,887 | 8,160,000 | 7,884,541 (78.8%) |
| 8 | **finality update, one transaction** (residue witness), finalized slot 11,606,368 | `5a86e718013eaff360fc49b4056e2adfaf3c68293eeaf92abbd6c83eb7f76347` | 83020 | 11,260 | 7,300,000 | 7,046,803 (70.5%) |
| 9 | **finality update, one transaction** (residue witness), finalized slot 11,606,400 | `7ee4a19de52426422f5b71c8bf3b558dee4aefa0098b3d2f74d83b8b8a6ae2a7` | 83021 | 17,020 | 7,760,000 | 7,502,303 (75.0%) |
| 10 | **`Attest`** of two storage slots of the vault under slot 11,606,400 | `e6fe291d5a2b851c310c06974abcafd7942301d2dd07b8d0c714e1e77171e1d8` | 83021 | 6,820 | 550,000 | 501,007 (5.0%) |

- **The program**: image `8d3b59ff9b4bf6992f80e797e7e25fad1ed372c4b4e1386d96e6dd33ba8e1bb5` (a 597,336-byte program
  binary whose 564,912-byte user ELF fills six segments, 567,988 bytes of deployment transactions), immutable, at
  `J4G2Leu5WUVu66FF23nKqQM7apUajyV5Av7mmGeKrFj1` ([`state.json`](state.json)): the reviewed image of
  [EVIDENCE](../../../docs/EVIDENCE.md) row 131, not this tree's newer one ([`../../README.md`](../../README.md), "Two
  images").
- **The checkpoint** ([`chain.json`](chain.json), [`bootstrap.ssz`](bootstrap.ssz)): Sepolia slot 11,606,336
  (period 1416), block root `0x8ef0115979d741ba168ce2e8d6dbc5efc4f41d571473b624145f9fb8465462c1`, fetched from
  Lodestar's public Sepolia endpoint and its root checked against publicnode's beacon header. The instance
  `26caef1f2a5cfc93eea9600192e8e556c8033420c08d6b8e91d935089d7fdf5e` (account
  `Ft2i22BuuJBVzRSvFJWzpBJajUwygfLxkVxZSXcqrZAi`).
- **The two finality updates** ([`updates/`](updates/)): signed by 492 and 477 of the 512 (the sync aggregate's bits
  in each SSZ file; the same counts in [`requests.tsv`](requests.tsv)), each verified and applied in one
  transaction; the instance's records of slots 11,606,336, 11,606,368 and 11,606,400 ([`record.json`](record.json)).
- **The attestation** ([`proofs/11606400/`](proofs/11606400/): the execution block's header, the block and
  `eth_getProof`): FerrataVault v5 on Sepolia, `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`, at block 12,161,902, the
  block the record of slot 11,606,400 binds: slot 0 (`depositRoot`) =
  `0xa67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469`, slot 1 (`nextLeafIndex`) = 2, each written to
  its statement account.
- **The fees**: the payer's balance fell from 200,000,000,000 (as the run printed it) to 199,788,226,285 native units
  ([`run-log.tsv`](run-log.tsv)), by 211,773,715, the sum of the simulated fees in `signed.tsv`.

## When, and by what command

`cd lez-v03 && scripts/build.sh --lez && FERRATA_TESTNET_SEND=yes FERRATA_WAIT=900 ./testnet.sh run`, at commit
`ea13bfb4` (the client and image the run was prepared, planned and reviewed with), on the Modal build box, started by
an unattended watcher outside the repository: every 20 minutes it read the testnet's head and the payer's balance
(two read-only calls) and, once the payer held funds, ran that command. The owner decided the send. The payer's keys
stayed beside the run's state, outside the repository.

- **Run 1, 18 November.** Started 14:00:22 UTC; fetched the checkpoint and waited for the two finality updates and the
  vault's proofs (14:00:22 to 14:13:05, [`requests.tsv`](requests.tsv)); at 14:13:29 read the chain (head 83005; the
  payer's nonce 0 and balance 0, where the run had read 200,000,000,000 at its start) and signed the eleven transactions
  ([`run-log.tsv`](run-log.tsv)). The sequencer refused the first, `Incorrect fee`, and the run stopped.
- Until 23:39 the watcher's reads found the head at 83006 with the payer funded or at 83005 with the payer at 0, in
  turn (one read got no head); at 23:59:15 the head was 83018 and the payer funded.
- **Run 2, 18 November 23:59 to 19 November 00:06 UTC.** Started 00:00:59 (the payer's balance 200,000,000,000, as the
  run printed it); `queue` found the eleven signed transactions in no block, sent them again byte for byte and signed
  nothing new; at 00:05:46 every one was in a block (`run-log.tsv`: nonce 11, head 83021); the goal held in state after
  that one round, and `record` wrote [`record.json`](record.json) from state.

## Status as of 19 November 2026, 11:21 UTC

**In the sequencer's blocks 83019, 83020 and 83021, and in its state (29 of 29 checks match), and the three blocks
Finalized**: the sequencer reports each one's status on the Logos Blockchain (Bedrock) as `Finalized`, and the explorer,
which shows a block once it is final there (in LEZ v0.3.1's code), shows each of the eleven transactions on its own
page and on its block's page. When the run wrote [`txs.tsv`](txs.tsv) (19 November, 00:05 UTC) all three blocks were
`Pending`, the state in which a rollback of the testnet can remove a block (on 18 November the payer's funds came and
went with block 83006). `Finalized` is the sequencer's and the explorer's word: we did not check the Logos Blockchain
itself, and a public testnet run by others can still be reset.

`./testnet.sh verify`, run on the Modal build box from the run's own tree (commit `ea13bfb4`, this folder in it, the
client the run built and the run's wallet home, which `verify` only reads the chain through), 11:21:21 to 11:21:52
UTC, exit 0; read-only (RPC reads of the sequencer, two explorer pages per transaction); paths shortened to `<state>`
and `<worktree>`:

```text
MATCH    program image id
MATCH    instance state
MATCH    record of slot 11606336
MATCH    record of slot 11606368
MATCH    record of slot 11606400
MATCH    statement "0x0000000000000000000000000000000000000000000000000000000000000000" = "0xa67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469"
MATCH    statement "0x0000000000000000000000000000000000000000000000000000000000000001" = "0x0000000000000000000000000000000000000000000000000000000000000002"
MATCH    transaction "deploy: WriteSegment 6 of 6 (73392 bytes)" in block 83019
MATCH    transaction "deploy: WriteSegment 5 of 6 (98304 bytes)" in block 83019
MATCH    transaction "deploy: WriteSegment 4 of 6 (98304 bytes)" in block 83019
MATCH    transaction "deploy: WriteSegment 3 of 6 (98304 bytes)" in block 83019
MATCH    transaction "deploy: WriteSegment 2 of 6 (98304 bytes)" in block 83019
MATCH    transaction "deploy: WriteSegment 1 of 6 (98304 bytes)" in block 83019
MATCH    transaction "deploy: CreateHeader (immutable)" in block 83019
MATCH    transaction "Init" in block 83019
MATCH    transaction "update 000-finality-11606368.ssz (residue witness, one transaction)" in block 83020
MATCH    transaction "update 001-finality-11606400.ssz (residue witness, one transaction)" in block 83021
MATCH    transaction "Attest 2 slot(s) of 0x04ee8ff19cc11cee4ae65d03e12b9888043de44c under slot 11606400" in block 83021
MATCH    effect of deploy: WriteSegment 6 of 6 (73392 bytes): segment holds its chunk
MATCH    effect of deploy: WriteSegment 5 of 6 (98304 bytes): segment holds its chunk
MATCH    effect of deploy: WriteSegment 4 of 6 (98304 bytes): segment holds its chunk
MATCH    effect of deploy: WriteSegment 3 of 6 (98304 bytes): segment holds its chunk
MATCH    effect of deploy: WriteSegment 2 of 6 (98304 bytes): segment holds its chunk
MATCH    effect of deploy: WriteSegment 1 of 6 (98304 bytes): segment holds its chunk
MATCH    effect of deploy: CreateHeader (immutable): program at J4G2Leu5WUVu66FF23nKqQM7apUajyV5Av7mmGeKrFj1, image 8d3b59ff9b4bf6992f80e797e7e25fad1ed372c4b4e1386d96e6dd33ba8e1bb5, immutable
MATCH    effect of Init: instance 26caef1f2a5cfc93 active; record of the checkpoint (slot 11606336) equal to the bootstrap's; initial store 21153159d9100a8a
MATCH    effect of update 000-finality-11606368.ssz (residue witness, one transaction): record of finalized slot 11606368 equal to the update's header
MATCH    effect of update 001-finality-11606400.ssz (residue witness, one transaction): record of finalized slot 11606400 equal to the update's header
MATCH    effect of Attest 2 slot(s) of 0x04ee8ff19cc11cee4ae65d03e12b9888043de44c under slot 11606400: slot 0 = 0xa67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469, slot 1 = 0x0000000000000000000000000000000000000000000000000000000000000002 at Sepolia block 12161902
verify: 29 checks: 7 compare state (image id, instance, records, statements) with record.json; 11 compare a transaction's block of inclusion (v0.3.1 includes a refused transaction too); 11 compare each transaction's effect in state with what the data implies
verify: every check matches (<state>/verify.SqE66T/record.json)
MATCH    explorer: deploy: WriteSegment 6 of 6 (73392 bytes) (b4606d58d403feda163757a166f0fb9535113c4cfb8a6cff21aedf3d728d1767) in block 83019 (Finalized)
MATCH    explorer: deploy: WriteSegment 5 of 6 (98304 bytes) (5d77eed2a979a75a955609f4800eb50f06fb084fb157a44a346a25af3ebe4c4f) in block 83019 (Finalized)
MATCH    explorer: deploy: WriteSegment 4 of 6 (98304 bytes) (9b226680fc577b1a2eb15c329b9583f7e894503123639996530af16c3ecc98cd) in block 83019 (Finalized)
MATCH    explorer: deploy: WriteSegment 3 of 6 (98304 bytes) (52ebe87d2fb2969a5a4fa30c97f8199bcce457592375069b4f94e350e1b6b270) in block 83019 (Finalized)
MATCH    explorer: deploy: WriteSegment 2 of 6 (98304 bytes) (d616532d223541f79b3e99c1e461a5062bf27637a6d2e1f3ff697b77bc6404b5) in block 83019 (Finalized)
MATCH    explorer: deploy: WriteSegment 1 of 6 (98304 bytes) (b49d2dfd59f1cd3a7acdb8eb371cc6becbabcff3f7a560978feac99e908a2d25) in block 83019 (Finalized)
MATCH    explorer: deploy: CreateHeader (immutable) (042e117eca845240b7c82a522f55b9a7c2ad9c538ac2236fbd301b0600f756a0) in block 83019 (Finalized)
MATCH    explorer: Init (834f78cb118c99ad313bc8b4399c4ead535ccd90c6bb8c8b0f496dd75e41e8df) in block 83019 (Finalized)
MATCH    explorer: update 000-finality-11606368.ssz (residue witness, one transaction) (5a86e718013eaff360fc49b4056e2adfaf3c68293eeaf92abbd6c83eb7f76347) in block 83020 (Finalized)
MATCH    explorer: update 001-finality-11606400.ssz (residue witness, one transaction) (7ee4a19de52426422f5b71c8bf3b558dee4aefa0098b3d2f74d83b8b8a6ae2a7) in block 83021 (Finalized)
MATCH    explorer: Attest 2 slot(s) of 0x04ee8ff19cc11cee4ae65d03e12b9888043de44c under slot 11606400 (e6fe291d5a2b851c310c06974abcafd7942301d2dd07b8d0c714e1e77171e1d8) in block 83021 (Finalized)
verify: every check matches the record (<worktree>/lez-v03/results/public); 11 transactions checked
```

## Re-check

From the run's commit, with this folder in place (a client built from a later tree is not the one the run was
signed with: the wire changed after it, [`../../README.md`](../../README.md), "Two images"). The build needs
`libpcsclite-dev` (`pcsc-lite-devel` on Fedora; LEZ v0.3.1's wallet links PC/SC), RISC Zero's guest Rust 1.91.1
(`RISC0_HOME`) and `r0vm` 3.0.5 on `PATH` (`scripts/build.sh`). `verify` reads the chain through LEZ v0.3.1's wallet
library, which needs a wallet: the `new-wallet` line makes a throwaway one (local keys, never funded; nothing is sent).

```sh
git worktree add ../ferrata-public-run ea13bfb4
cp -r lez-v03/results/public ../ferrata-public-run/lez-v03/results/
cd ../ferrata-public-run/lez-v03
scripts/build.sh --lez     # the client ferrata-v03, on LEZ v0.3.1's wallet (the README's Re-check at that commit)
export FERRATA_STATE="$PWD/../../ferrata-public-recheck"    # a new folder outside the repository
LEE_WALLET_HOME_DIR="$FERRATA_STATE/wallet" target/release/ferrata-v03 new-wallet "$FERRATA_STATE/run" https://testnet.lez.logos.co > /dev/null
./testnet.sh verify        # read-only; never sends
```

What each line means:

- `MATCH` (7, state): the program's image id, the instance, the three records and the two statements in the
  sequencer's state now equal [`record.json`](record.json).
- `MATCH transaction ... in block N` (11): the transaction is in that block. On its own this says nothing of success,
  since v0.3.1 puts a refused transaction in a block too; hence the next eleven.
- `MATCH effect of ...` (11): that transaction's effect is in state, in full, as the run's data implies (each segment's
  chunk; the image id at the program account; the checkpoint's record and the active instance; each update's
  record; the two statements).
- `MATCH explorer:` (11): the sequencer reports the block as not `Pending`, and both the explorer's transaction page and
  its block's page show the hash.
- `PENDING explorer:` the sequencer reports the block `Pending` on the Logos Blockchain (not final there; a rollback
  could still remove it); the explorer is not read. `verify` then exits 3: state matches, the blocks are not final.
- `MISMATCH`: anything else, or fewer checks than signed transactions, or the blocks unreadable; exit 1. Exit 0 only if
  every line is `MATCH`.

## Files

- [`record.json`](record.json): what the run read back from state at the end (image id, instance, records,
  statements, each transaction's block).
- [`txs.tsv`](txs.tsv): each transaction's block, its Bedrock status when written, its size, declared and simulated
  gas, and the verdict read from state; [`signed.tsv`](signed.tsv) and [`signed/`](signed/): every signed
  transaction, its nonce, the simulated block, gas and fee.
- [`run-log.tsv`](run-log.tsv): the payer's nonce, balance and the head at signing and at the end.
- The data: [`chain.json`](chain.json), [`bootstrap.ssz`](bootstrap.ssz), [`updates/`](updates/),
  [`proofs/`](proofs/), [`applied.txt`](applied.txt) (the updates applied), [`requests.tsv`](requests.tsv) (every
  request to Sepolia's public APIs, none with a key).
- [`state.json`](state.json): the run's accounts and image (no key); [`batch.json`](batch.json): the same sequence as
  a `ferrata_ops batch` file (its ELF path written `<run>`).
- [`notes.txt`](notes.txt) is empty: the run writes a line there only when the chain it reads before signing differs
  from a fresh start (a deployment account already written, an instance, update or handoff of someone else's, a
  statement already on chain, a record or proof missing: `plan_steps` in `client/src/main.rs` at `ea13bfb4`). When
  run 1 read the chain at 14:13:29 there was none of these; run 2 only sent the same bytes again, which writes no note.

## Not claimed

- The proposal's deliverable on a public network: these are the v0.3.1 port's transactions; the LEZ v0.4 programs
  run nowhere public (LEZ testnet 0.4 does not exist).
- Soundness beyond what [EVIDENCE](../../../docs/EVIDENCE.md) row 131 states: the one-transaction update rests on the experimental, unaudited residue check;
  the weak-subjectivity period is the v0.4 work's test value, not computed from Sepolia.
- The gas each transaction was charged: v0.3.1 does not report it; the figures are the simulation's, and the payer's
  balance fell by exactly the simulated fees.
- A light client kept current: nothing updates the public instance; at the verify above it still held Sepolia's
  finalized slot 11,606,400.
- Finality on the Logos Blockchain beyond the sequencer's status and the explorer's pages (above).
