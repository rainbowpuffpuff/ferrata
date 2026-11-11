# The inbound crossing on a live LEZ sequencer

Ferrata's first crossing from Ethereum to LEZ, end to end on a running LEZ v0.4.0-rc1 sequencer
and Logos Blockchain node, as [`../README.md`](../README.md) runs the attestation program: the
attestation program and the bridge program deployed through LEZ's program loader; an attestation
instance initialised and following a beacon chain; the bridge's configuration paired with the vault
(`Init` attests the vault's slot 2 = `configId`); the vault's deposit root imported (`ImportRoot`
attests slots 0 and 1); one deposit claimed by the plaintext path, one privately with a real proof
that the sequencer verifies; and the refusals.

Two sessions share every line of code ([`../driver/src/inbound.rs`](../driver/src/inbound.rs)) and
differ only in the chain they read:

- **The stand-in session (recorded below).** Run before any deposit exists. Its beacon chain is
  **not Ethereum**: a 512-key sync committee whose keys this code holds, a bootstrap in the Gloas
  layout and two finality updates it signs, finalizing two execution blocks made from a real
  Sepolia header (the replay fixture's bound block) whose state root is that of a synthetic state
  holding a synthetic vault (`0x5e…5e`) with the plan's two deposits (ETH 0.001 each, to LEZ's
  published test accounts). Everything on the LEZ side is real: the deployment, the sequencer, the
  Bedrock finality of every block, the real proof of the private claim and its verification.
- **The real session (below; deposit 0 run on 11 November 2026, deposit 1 waits).** The same, on
  public Sepolia: the instance from the period-1378 bootstrap (block root `5f71d7f6…b20d`), every
  period's update to the deposits, and the finality updates captured after each deposit is final,
  with the vault's proofs at the finalized blocks, for the live vault that pins this
  configuration's `configId`.

## The configuration's identity

The vault pins `configId` (its constructor argument, copied to slot 2) before the zone exists, so
every part is fixed in advance and reproduced by any later session:

| part | value | from |
|---|---|---|
| bridge program (`bridgeHeader`) | `8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d` | the account of `derived_key("program/bridge/header")` ([`../driver/src/keys.rs`](../driver/src/keys.rs)): independent of the image and of the order of deployment |
| bridge image id | `66722fc7681beaeaacdfce978ae95ba9ba9f2b576b87791d523d6cd10815f4fa` | the frozen `ferrata_bridge.bin` (393,236 bytes, SHA-256 `aff2205a…605d`); the loader recomputed it on deployment |
| attestation program | `86ce4d7f54bd68b4c7356398cfda192f791c83f6fa3c3102a18ac20fefdfa04c` | `derived_key("program/attest/header")`; the frozen `ferrata_lez.bin` (602,920 bytes, SHA-256 `f99871e7…82bb`, image `650e3c90…2e4c`) |
| Sepolia instance | `6d65e4ef2af67eed1ce3a5c2d7b04fa02517707d2dff044869389f310b69f6b4` | `instance_id(sepolia_config(2), 5f71d7f6…b20d)`, computed by the program at `Init` |
| `attestationRootId` | `0f0feabd0d70d3202282037513e256d817c4749d3925d3457b077f9ca5e5d524` | `H(…/attestation-root ‖ attestation program ‖ instance)` |
| salt | `badde82cf76c7f5a543569fd2dd5b326637d336d948734a291572f4e05fa7bf5` | `H(…/terms ‖ ETH ‖ 0.001, 0.01, 0.1 ETH)` |
| `bedrockGenesisId`, `channelId` | `64b4418d…068d`, `0a358e03…40a6` | SHA-256 of the tags `ferrata-live/v1/inbound/bedrock-genesis-id` and `…/channel-id`: a scripted session's local devnet gets its genesis and channel only when it starts, and the program does not check either |

`configId = sha256("ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖ bridgeImageId
‖ evmChainId ‖ vault ‖ attestationRootId ‖ salt)` (261 bytes); `live-driver inbound-plan
--attest-bin … --bridge-bin … --vault ADDR` prints every part and the id for a vault. The binaries
are not in the repository (they embed the build machine's paths); [`run.sh`](run.sh) refuses any
other file. The bridge guest, `bridge-wire`, the attestation wire and the bridge core must not
change: any change gives another image id, and so another `configId`.

The block's RLP header, which the attestation program needs under a record in the Gloas layout
(every Sepolia record since Gloas binds the block hash only), travels inside the account proof list
of `Init` and `ImportRoot` behind a marker (`bridge-wire` `HEADER_MARKER`, `with_header`), so that
`ImportRoot`'s encoding, which the checkpoint walks, is unchanged.

## The stand-in session (10 November 2026)

Recorded by `run.sh stand-in` on the Modal build box (`../run-live.sh`, the same sequencer and node
binaries as `../results/`), 21:12–21:22 UTC; records in [`results/stand-in/`](results/stand-in/)
(`run.tsv`, the blocks, logs, `inbound-summary.json`, the private claim's transaction). 57 blocks,
all final on Bedrock at export; 30 transactions, 26 applied and 4 refused with the expected code.

The deployment: 12 transactions (six and four `WriteSegment`s, two `CreateHeader`s), 938,580 bytes
of storage gas, 7,508,640 in fees; both headers immutable, the loader's image ids as above.

Stand-in instance `6dcc9682…da39` (the Sepolia config with the store in the Gloas layout, trusting
the stand-in bootstrap's root); stand-in vault `0x5e…5e`; stand-in `configId` `887afdb1…a36d` (the
same parts as above with this instance and this vault).

| # | phase | transaction | block | result | execution gas | storage gas (bytes) | fee (native units) |
|---:|---|---|---:|---|---:|---:|---:|
| 13 | session | stand-in Init Init | 14 | applied | 113,477 | 1,756 | 921,864 |
| 14 | session | stand-in Init adopt 1/2 | 15 | applied | 3,396,985 | 24,980 | 27,375,720 |
| 15 | session | stand-in Init adopt 2/2 | 16 | applied | 3,477,181 | 25,173 | 28,018,832 |
| 16 | session | stage 1 finality update Open (step 1) | 17 | applied | 1,994,178 | 2,348 | 15,972,208 |
| 17 | session | stage 1 finality update Miller (step 2) | 18 | applied | 3,305,289 | 399 | 26,445,504 |
| 18 | session | stage 1 finality update Finish (step 3) | 19 | applied | 3,715,991 | 592 | 29,732,664 |
| 19 | session | bridge Init without the block's header (Gloas record) | 20 | refused 215 (code 215) | 57,872 | 1,694 | 476,528 |
| 20 | session | bridge Init (configId in the vault's slot 2) | 21 | applied | 181,965 | 2,418 | 1,475,064 |
| 21 | session | bridge ImportRoot n=1 (stage 1, block 2) | 22 | applied | 232,278 | 2,460 | 1,877,904 |
| 22 | session | bridge Claim: deposit 0 (public) under root(cid, 1) | 23 | applied | 142,432 | 1,805 | 1,153,896 |
| 23 | session | bridge Claim: deposit 0 again (replay) | 24 | refused 250 (code 250) | 124,481 | 1,740 | 1,009,768 |
| 24 | session | stage 2 finality update Open (step 1) | 25 | applied | 1,994,354 | 2,348 | 15,973,616 |
| 25 | session | stage 2 finality update Miller (step 2) | 26 | applied | 3,305,289 | 399 | 26,445,504 |
| 26 | session | stage 2 finality update Finish (step 3) | 27 | applied | 3,715,991 | 592 | 29,732,664 |
| 27 | session | bridge ImportRoot n=2 (stage 2, block 3) | 28 | applied | 232,278 | 2,460 | 1,877,904 |
| 28 | session | bridge Claim: deposit 0 under root(cid, 2) (a later root) | 29 | refused 250 (code 250) | 124,481 | 1,740 | 1,009,768 |
| 29 | session | bridge Claim: deposit 1 redirected to a public destination (a wrong opening) | 30 | refused 249 (code 249) | 93,764 | 1,805 | 764,552 |
| 30 | session | bridge ClaimPrivately: deposit 1 at LEZ's private test account 0 (real proof) | 57 | applied | 439,815 | 224,063 | 5,311,024 |

- The private claim: `ClaimPrivately` at LEZ's private test account 0 (the deposit's destination),
  built and proven by LEZ's wallet (`send_privacy_preserving_tx_with_pre_check`) on the box's CPU in
  254.8 s, a transaction of 252,185 bytes, included in block 57; afterwards the nullifier is spent
  (`leaf_count` 2, 0.001 ETH) and public test account 0 holds 0.001 ETH of this configuration.
- The refusals: `Init` without the header (215 `HEADER_REQUIRED`, the attestation program's);
  the replay and the same deposit under `root(cid, 2)` (250 `ALREADY_CLAIMED`); deposit 1's opening
  with another destination (249 `CLAIM_INVALID`).
- Re-checked with `../verify-run.sh --dir inbound/results/stand-in ferrata_lez.bin ferrata_bridge.bin`
  (on the build box): 57 blocks, chain links hold, 30 rows at their recorded blocks; with the
  bytecode put back every block hash holds, the chain replays through LEZ's `chain_state` from the
  pre-genesis state, every row equals what LEZ's settlement charges, both images as recorded.
- Before the session, `run.sh check-stand-in` runs the chain through the native light client and
  the bridge's requests through the attestation program's own evaluation (no zone).

What the stand-in does not show: anything about Ethereum. Its committee, beacon headers and blocks
are made here; only the header template is Sepolia's. The real session replaces the chain, nothing
else.

## The real session

### On Sepolia

Ferrata's vault, deployed by the owner's run (`vault/script/RoundTrip.s.sol` `deploy()`), and the
first deposit, as read from public Sepolia RPCs (read only) on 11 November 2026:

| | |
|---|---|
| vault | [`0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7`](https://sepolia.etherscan.io/address/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7) |
| its creation | transaction [`0x0a5c3a64b6357c76e273f3ecd91ad58078f9c872f3ab90e68dba521de0589f12`](https://sepolia.etherscan.io/tx/0x0a5c3a64b6357c76e273f3ecd91ad58078f9c872f3ab90e68dba521de0589f12), block [12107894](https://sepolia.etherscan.io/block/12107894) (index 51, status 1; the receipt's `contractAddress` is the vault; no code at 12107893, 23,650 bytes at 12107894) |
| deposit 0 (j 0, public kind) | transaction [`0xf3235de7115886851c2a92b6471e1cdce9c8aa9fa3c39558d60cade17a5789a0`](https://sepolia.etherscan.io/tx/0xf3235de7115886851c2a92b6471e1cdce9c8aa9fa3c39558d60cade17a5789a0), block [12107901](https://sepolia.etherscan.io/block/12107901) (status 1, 0.001 ETH, one `Deposit` log: leaf 0, `C` `34964d20…5c4c`) |
| `storedLezConfigId()` | `0xa2e880cb8da77b08dad9833be7a4c9aa08dd856f4648da5486937c798611d3b8`, this configuration's `configId` for this vault (the plan's standard-library command and the driver agree) |

Capture 1, taken read only after deposit 0 was final (`lez/scripts/fetch-attest-fixture.py
--network sepolia --storage VAULT:0=depositRoot(),1=nextLeafIndex(),2=storedLezConfigId()
--deposits VAULT:12107894 --updates-from 1410 OUTDIR`, 00:03–00:04 UTC), is in the repository:
[`crates/ferrata-spectests/fixtures/sepolia-inbound-2026-11-11-1`](../../../crates/ferrata-spectests/fixtures/sepolia-inbound-2026-11-11-1)
(`SOURCE.md` lists every request; `SHA256SUMS`). Its Gloas finality update (attested slot
11551808, 509 of 512 signers, period 1410) finalizes slot 11551744, root `b4d61ff6…11ef`, which
binds execution block 12107997 (hash `6ffe0da7…a255`, state root `31c38d82…ccf5`); there the vault
holds one leaf (slot 0 `a5b5c43f…4875`, slot 1 = 1) and slot 2 = `configId`.

### Deposit 0 on LEZ (11 November 2026, capture 1)

Recorded by `run.sh real CAPTURE_1` (capture 1 alone, the fixture above) on a Modal build box,
01:03–01:35 UTC: the same sequencer and Bedrock node binaries as the stand-in session and
`../results/` (sha256 `fdf1125e…5b76` and `2e5390b5…ccfc`, rebuilt from source on a fresh box and
equal bit for bit), the frozen programs, the driver of commit `c55644c`. Records in
[`results/sepolia-capture-1/`](results/sepolia-capture-1/) (`run.tsv` the manifest, `blocks.borsh` and
`chain.tsv` the block log, `logs/`, `inbound-summary.json`, `verify-run.log`). 186 blocks, all final on
Bedrock at export; 185 transactions, 181 applied and 4 refused with the expected code.

- The deployment: 12 transactions as in the stand-in (938,580 bytes of storage gas, 7,508,640 in fees).
- The instance: `Init` from the period-1378 bootstrap and its two adoption parts (blocks 14–16), Sepolia
  instance `6d65e4ef…f6b4` as planned; then the 32 period updates 1378–1409 (blocks 17–176, 160
  transactions, all applied: Open 2,013,315–2,177,331 execution gas, Miller 3,310,385–3,313,287, two
  adoption parts 3,408,020–3,418,098 each, Finish 3,728,633–3,731,712; 509,252,551 execution gas in all).
  The store moves from the Electra layout to the Gloas layout at period 1379, natively and in the program.
- Then capture 1's Gloas finality update and the bridge:

| # | transaction | block | result | execution gas | storage gas (bytes) | fee (native units) |
|---:|---|---:|---|---:|---:|---:|
| 176 | stage 1 finality update Open (step 1) | 177 | applied | 2,073,229 | 3,500 | 16,613,832 |
| 177 | stage 1 finality update Miller (step 2) | 178 | applied | 3,305,289 | 399 | 26,445,504 |
| 178 | stage 1 finality update Finish (step 3): header(instance, slot 11551744) | 179 | applied | 3,718,075 | 592 | 29,749,336 |
| 179 | bridge Init without the block's header (Gloas record) | 180 | refused 215 | 115,415 | 5,280 | 965,560 |
| 180 | bridge Init (configId in the vault's slot 2) | 181 | applied | 355,935 | 6,004 | 2,895,512 |
| 181 | bridge ImportRoot n=1 (block 12107997) | 182 | applied | 426,479 | 6,646 | 3,465,000 |
| 182 | bridge Claim: deposit 0 at leaf index 1 under root(cid, 1) (a leaf index at the count) | 183 | refused 248 | 30,946 | 1,805 | 262,008 |
| 183 | bridge Claim: deposit 0 redirected to another public account (a wrong opening) | 184 | refused 249 | 93,764 | 1,805 | 764,552 |
| 184 | bridge Claim: deposit 0 (public) under root(cid, 1) | 185 | applied | 142,432 | 1,805 | 1,153,896 |
| 185 | bridge Claim: deposit 0 again (replay) | 186 | refused 250 | 124,481 | 1,740 | 1,009,768 |

- `Init` attests the live vault's slot 2 = `configId` `a2e880cb…d3b8` at block 12107997 under the record
  the instance wrote for finalized slot 11551744; `ImportRoot` attests slots 0 and 1 (root
  `a5b5c43f…4875`, one leaf) and writes `root(cid, 1)`. Both cost more than in the stand-in (181,965 and
  232,278) because the account proof is a real Sepolia state-trie path behind a real block header.
- The claim: after it, LEZ's public test account 0 holds 1,000,000,000,000,000 (0.001 ETH) of this
  configuration and deposit 0's nullifier `f89da0ad…` is spent; the other account (local.json's first
  genesis account, the redirect) holds nothing; the replay leaves the holding unchanged.
- The refusals: `Init` without the header (215 `HEADER_REQUIRED`, the attestation program's); a leaf
  index at the root's count (248 `LEAF_NOT_IMPORTED`); deposit 0's opening with another destination
  (249 `CLAIM_INVALID`), sent before the claim, while the deposit was still claimable; the replay (250
  `ALREADY_CLAIMED`).
- Re-checked with `../verify-run.sh --dir inbound/results/sepolia-capture-1 ferrata_lez.bin
  ferrata_bridge.bin` on the box (`verify-run.log`): 186 blocks, chain links hold, 185 rows at their
  recorded block and position; with the bytecode put back every block hash holds, the chain replays
  through LEZ's `chain_state` from the pre-genesis state, every row equals what LEZ's settlement
  charges, both images as recorded.
- Before it: `run.sh check-real CAPTURE_1` (natively) passed. A first attempt (00:25–00:56 UTC) stopped
  at the wrong opening, which then named LEZ's public test account 1, the session's fee payer: LEZ's
  wallet refuses to build a transaction that mentions its fee payer as non-signing. Everything before
  it had gone as above (blocks 1–183); its records are not kept in the repository.

What it does not show yet: deposit 1, the private kind, its `ClaimPrivately` with a real proof, and a
second root (below).

### Deposit 1 (waits for capture 2)

The owner's run makes deposit 1 (j 1, private kind, to LEZ's private test account 0) and capture 2
after its finality. Then the whole session runs again on a fresh zone with both captures: stage 1
exactly as above, then capture 2's finality update, `ImportRoot` of `root(cid, 2)`, deposit 0 under
the later root (250), deposit 1 redirected to a public destination (249) and deposit 1's
`ClaimPrivately` with a real proof that the sequencer verifies:

```sh
live/inbound/run.sh check-real CAPTURE_1 CAPTURE_2     # natively, no zone
live/inbound/run.sh real CAPTURE_1 CAPTURE_2           # records in results/sepolia/
../verify-run.sh --dir inbound/results/sepolia ferrata_lez.bin ferrata_bridge.bin
```

`run.sh` takes one or two captures (and optionally the vault); capture k must hold the vault's
first k deposits, and `check-real` and the session refuse a vault whose slot 2 is not `configId`,
whose slot 1 is not the number of deposits read, or whose slot 0 is not their root.

## What was found on the way

- The vault and the bridge core agree (the vault's `leafHash` is the core's `deposit_leaf`; slots 0,
  1, 2 are the deposit root, `nextLeafIndex` alone in its slot, and `storedLezConfigId`); a
  deposit's `j` is the vault's `depositCount[depositor]`.
- The deposits of the e2e fixture cannot be claimed on LEZ: their destinations are hashes of labels,
  not LEZ accounts. The plan's deposits name LEZ's published test accounts, whose keys a session holds.
- The bridge sent its attestation requests without a header, which the attestation program refuses
  under any Gloas record (215): fixed (the header in front of the account proof), tested in the
  harness (`../../BRIDGE-RESULTS.md` §2a) and live (above).
- The live driver had not been built since the attestation program's `Config` and `Record` gained
  the chain id; it now builds against the merged wire, sends updates in the layout a beacon node
  serves them in, and upgrades its native store at a fork as the harness does.
- Only Lodestar serves Sepolia light-client data, and no bootstrap of the Gloas era: the instance
  starts at period 1378, the last before Gloas.
