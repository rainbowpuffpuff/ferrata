# Real proofs on LEZ v0.4.0-rc3: M0 check 4

M0's check 4 asks for "real proofs on the newest LEZ tag of the private update, attestation, claim and burn, settled
by its sequencer". The newest tag on 16 November 2026 is `v0.4.0-rc3` (commit `bafd07be0701398f9bc435f73ab05e9a7936c301`;
`gh api repos/logos-blockchain/logos-execution-zone/tags`, read that day). Everything below ran on 16 November 2026 on
a Modal build box (16 physical cores, 64 GiB at most, under gVisor; `nproc` says 2, so the provers ran with
`RAYON_NUM_THREADS=32`), from this tree, on **LEZ v0.4.0-rc3's own `sequencer_service`** (built from the tag, tree
unmodified) with **its Logos Blockchain node 0.3.5-rc.1** (the release LEZ's resolver fetched, `f6d4267f`), one
validator, LEZ's local configuration (leader proofs in dev mode, as LEZ's container runs it). **The sequencer ran
without `RISC0_DEV_MODE`**: it verifies the receipt of every privacy-preserving transaction
(`*/config/sequencer-process.txt`), and it dropped the fake receipt it was sent (below).

**What was proven and deployed: the final programs, the rc1 builds of [`docs/image-ids.md`](../../../../docs/image-ids.md)**
(the attestation program `0x816d1fef…`, the bridge `0xccde6171…`, the reference consumer `0x8464603b…`, LEZ's
`scripted` `0x8cb0e3f5…`; each binary checked by sha256 and `r0vm --id` before use), deployed through rc3's program
loader. Not the rc3 rebuilds (whose ids differ: [`../images.md`](../images.md)). The proofs were made by LEZ v0.4.0-rc3's
own wallet library (its privacy-preserving circuit, which is rc2's), on the box's CPU, and checked by rc3's sequencer.

## Result

**4 of 4 pass**: each was proven for real, sent to rc3's sequencer, logged by it as validated ("Validated transaction
with hash …, including it in block"), applied, and its block was final on the 0.3.5-rc.1 node 14 s after it was
published (the sequencer's "Channel update: … finalized N..=N").

| | program (image, the final build) | proven and sent | transaction / receipt | block (final after) | session |
|---|---|---|---|---|---|
| (a) the private update: `update_2` as a finality update, `VerifyAll` at a private account's actor, the public instance advancing | attestation program `0x816d1fef…` (twin deployment) | 887.3 s | 251,843 / 237,452 B | 271 (14.2 s) | `update/` |
| (b) a private attestation: the reference consumer's `AttestPrivately` reads the record of mainnet block 26,384,851 and verifies an ERC-20 `Transfer` log of USDC (`0xa0b8…eb48`) against it; only the `Read` is public | consumer `0x8464603b…`, reading the attestation program `0x816d1fef…` | 279.9 s | 251,630 / 237,378 B | 73 (14.2 s) | `attest-claim-burn/` |
| (c) a private claim: the bridge's `ClaimPrivately` of deposit 1 into LEZ's private test account 0 | bridge `0xccde6171…` | 264.3 s | 252,267 / 237,664 B | 151 (14.2 s) | `attest-claim-burn/crown/` |
| (d) a private burn: the bridge's `Burn` of 0.001 ETH from that private account, note 1 | bridge `0xccde6171…` | 302.2 s | 255,031 / 239,046 B | 185 (14.1 s) | `attest-claim-burn/crown/` |

"Proven and sent" is LEZ's wallet's `execute_and_prove` plus the submission, on 32 CPU threads of the box (no GPU),
below the node and the sequencer (nice 10). Transaction hashes: `9fff9aac…`, `e1adeae3…`, `726cb5a1…`, `bd284b9e…`
(`summary.json`). The fake receipt of the private update (RISC0_DEV_MODE=1 in the wallet only) was dropped by the
sequencer as an "Invalid privacy preserving execution circuit proof".

`summary.json` holds these values, written by `lez/scripts/real-proofs-summary.py` from the records below; it is what
`python3 -I scripts/m0.py` reads for check 4.

## The sessions

**`update/`: the private update** (`lez/live/run.sh run`, 12:24–13:11 UTC; phases `info deploy replay scenarios
private-fake export private-real export`). The final attestation program deployed twice (main, twin) and `scripted`;
the replay of instance A and the twin's finality path and refusals as on 14 November (61 of 62 transactions with a
harness row at the harness's execution gas; the other the designed validity-window case); then the private update:
`update_2` of the mainnet period-1899 fixture as a finality update, `VerifyAll` executed at LEZ's first test private
account's actor of the twin deployment, the instance applying the verified job publicly. With `RISC0_DEV_MODE=1` in
the wallet (a fake receipt) the sequencer **dropped** it: `failed settlement: State transition failed at transaction 0:
Invalid privacy preserving execution circuit proof` (`update/logs/sequencer.log`, 12:55:08). With a real proof it
was **proven and sent in 887.3 s**, a **251,843-byte transaction with a 237,452-byte receipt** (7 private actions, 4
boundary steps), and the sequencer logged `Validated transaction with hash 9fff9aac…d3232, including it in block`
(13:10:29), published **block 271** (252,554 bytes) and logged it finalized on Bedrock at 13:10:43; the instance then
held the native light client's store and the new record. `verify-run.sh`: the 271 blocks replay through rc3's
`chain_state` from the pre-genesis state, every block hash holds, 95 rows equal LEZ's settlement
(`update/verify-run.log`).

**`attest-claim-burn/`: the private attestation, claim and burn** (one zone; phases `info deploy attest-private hook
export`). 13:42–14:15 UTC. The final attestation program and `scripted` deployed as in `update/`; then **`attest-private`**
(new, `lez/live/driver/src/attest.rs`): the reference consumer deployed, a new mainnet instance of the main deployment
initialised from the harness's Stage 4 fixture (`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`, period
1907) and its finality update applied publicly (the record of slot 15,624,480 written), then the private attestation
with LEZ's second test private account, proven and sent in 279.9 s, applied in block 73. Then **`hook`**
(`lez/live/hooks/crown.sh`, new) while the same zone ran: crown-driver (`lez/crown/driver`, built against rc3)
`crown-a` deployed the attestation program and the bridge (`attest`, `bridge`) and the stand-in instance; a local
anvil (chain id 11155111) got the vault tests' mock verifier, FerrataVault v5 in mode (B) with this zone's `configId`
and `genesisState`, and two deposits of 0.001 ETH, each captured; `crown-b` then applied the captures' updates and
ran the bridge's `Init`, Register ETH, `ImportFees`, `ImportRoot` n=1 and n=2, the public claim, **the private claim
(264.3 s, block 151)**, the public burn and **the private burn (302.2 s, block 185)**. 188 blocks, all final; the live
driver's 33 transactions and crown-driver's 33, all applied; no head rewind in this session. `verify-run.sh` with the
four final binaries: every block hash holds, the 188 blocks replay through rc3's `chain_state`, 33 rows equal LEZ's
settlement (`attest-claim-burn/verify-run.log`). crown-driver's own export is in `crown/export/` without its
`blocks.borsh` (the session's `blocks.borsh` holds every block).

**`attest-claim-burn-try1/`: the first attempt, stopped by rc3's node and sequencer, not by a proof.** The same zone
setup; after block 28 (the consumer's deployment was in blocks 26–31) the 0.3.5-rc.1 node mined none of the
sequencer's next 120 inscriptions for 20 minutes, while the sequencer kept making a block every 10 s. At 13:36:59 the
sequencer logged `Pending tx … expired unmined; orphaned` 120 times, `Channel conflict: orphaned 29..=148 (120)` and
`Head rewound from 148 to 28` (`attest-claim-burn-try1/logs/sequencer.log`). The driver was reading blocks up to the
old head and stopped on `the sequencer has no block 29` before any proof started. This is the "120 blocks final only
after 1,214–1,216 s" of the 14 November rc3 session ([`../README.md`](../README.md)), now seen to its end: a
rewind to the last final block, the orphaned blocks' transactions included again afterwards. Its cause in the node
(why one validator stops including the channel's inscriptions for 300 slots) is not traced here, but the timing is
that of [LEZ issue 1109](https://github.com/logos-blockchain/logos-execution-zone/issues/1109) (our report of
12 November: a storage-price rise at an epoch rotation strands an inscription funded with the 12 % reserve): block 29's
inscription was submitted at 13:17:09 and the node's storage gas price rose from 2 to 3 at 13:17:11
(`attest-claim-burn-try1/logs/bedrock-gas-prices.log`), a rise of 50 %. The node's log excerpt holds no rejection
line, so this is a match by timing, not a trace. The live driver now
waits through a rewind (`lez/live/driver/src/live.rs`, `sync`), and a settlement may wait 3,000 s; the session was run
again (above).

## What this does not show

- One validator on a local network, with dev-mode leader proofs; not Logos's devnet, not testnet 0.4 (which does not
  exist yet). No checkpoint was made over this zone: checkpoint-core cannot read 0.3.5-rc.1's transactions
  ([`../README.md`](../README.md), "Logos Blockchain 0.3.5-rc.1"), which is B1's work.
- The proofs ran on 32 CPU threads of a cloud box, not on a user's laptop (M0 check 1 measures that, on rc1's
  circuit); no GPU was used.
- The Ethereum side of the claim and burn is a local anvil whose blocks a stand-in committee finalizes (as e2e/seqnet),
  with the vault tests' mock verifier; nothing was sent to a public network.
- The node's 20-minute stall and the rewind are recorded; they match LEZ issue 1109 by timing, and the node's own
  rejection of the inscription was not captured.

## Files and the re-check

- `summary.json`; `update/` and `attest-claim-burn/`: each session's `run.tsv` (every transaction: status, block, gas,
  note with the proving time and sizes), `chain.tsv` (every block with its Bedrock status), `blocks.borsh`,
  `programs.tsv`, `instances.tsv`, `environment.md`, `config/` (the sequencer's configuration and process, the node's
  configuration diff), `logs/` (sequencer, driver, node excerpt), `private/` (the proven transactions and the RPC
  transcripts of the wallet), `verify-run.log`, `tables.md`; `attest-claim-burn/crown/`: crown-driver's records
  (`session-a.json`, `session-b.json`, its `export/`, the proven claim and burn, the Ethereum side's `eth/`).
- Re-check the records without running anything:

```sh
python3 -I lez/scripts/real-proofs-summary.py lez/results/v0.4.0-rc3/real-proofs   # exit 0: 4 of 4 passed
lez/live/verify-run.sh --dir lez/results/v0.4.0-rc3/real-proofs/update <final ferrata_lez.bin> <final scripted.bin>
python3 -I scripts/m0.py                                                           # M0.4 from summary.json
```

- Run it again (on a build box, not a laptop; about 2 h and two real proofs of 15 min, two of 5–10 min):

```sh
scripts/fetch-upstream.sh && lez/scripts/lez-select.sh v0.4.0-rc3        # in a copy of the tree
LEZ_TAG=v0.4.0-rc3 lez/live/run.sh build && LEZ_TAG=v0.4.0-rc3 lez/live/run.sh build-bedrock
(cd lez/crown/driver && cp ../../live/driver/Cargo.lock . && cargo build --release)   # crown-driver on rc3
# the final programs: cargo build -p methods in lez/ on v0.4.0-rc1 (scripts/guest-rustc), checked against docs/image-ids.md
LEZ_TAG=v0.4.0-rc3 FERRATA_PROGRAM_BIN=…/ferrata_lez.bin FERRATA_SCRIPTED_BIN=…/scripted.bin FERRATA_LIVE_PROOF_THREADS=32 \
  FERRATA_LIVE_PHASES="info deploy replay scenarios private-fake export private-real export" lez/live/run.sh run
LEZ_TAG=v0.4.0-rc3 FERRATA_PROGRAM_BIN=… FERRATA_SCRIPTED_BIN=… FERRATA_LIVE_PROOF_THREADS=32 \
  FERRATA_LIVE_DRIVER_EXTRA="--consumer-bin …/ferrata_consumer.bin" FERRATA_LIVE_HOOK=lez/live/hooks/crown.sh \
  CROWN_BRIDGE_BIN=…/ferrata_bridge.bin FERRATA_LIVE_PHASES="info deploy attest-private hook export" lez/live/run.sh run
```

The box jobs that ran these (`boxA.sh`, `boxB.sh`, `boxB2.sh`) are beside the repository, not in it.
