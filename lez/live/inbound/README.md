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
- **The real session (not run yet).** The same, on public Sepolia: the instance from the period-1378
  bootstrap (block root `5f71d7f6…b20d`), every period's update to the deposits, and the finality
  updates captured after each deposit is final, with the vault's proofs at the finalized blocks. It
  waits for the deposits into the new vault, which pins this configuration's `configId`.

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

**The program this crossing uses is the frozen one, not the tree's.** Since the merge of branch
`bridge-complete` into `integration`, `../../guests/bridge` and `../../bridge-wire` are a later
program: the registry, fees by leaf index, caps, freezes and `Sunset`, with this crossing's header
in front of the account proof kept (built at the integration worktree's path, its image is
`f11b1aba…58eb`; image ids depend on the build path). Its messages and `configId` differ, so the
real Sepolia crossing deploys the frozen binaries above (bridge image `66722fc7…f4fa`), and the live
driver builds against a byte-for-byte copy of the frozen program's wire,
[`../frozen/bridge-wire`](../frozen/README.md).

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

Once the owner's Sepolia run has made the two deposits (`sepolia-plan.json` beside the repository
names them: j 0 public to LEZ's public test account 0, j 1 private to its private test account 0,
ETH 0.001 each) and captured each after its finality (`lez/scripts/fetch-attest-fixture.py
--network sepolia --storage VAULT:0=depositRoot(),1=nextLeafIndex(),2=storedLezConfigId()
--deposits VAULT:DEPLOY_BLOCK --updates-from 1410 OUTDIR`), from `lez/` on the build box:

```sh
live/inbound/run.sh check-real CAPTURE_1 CAPTURE_2     # natively, no zone
live/inbound/run.sh real CAPTURE_1 CAPTURE_2           # the session; records in results/sepolia/
```

It initialises the instance from the period-1378 bootstrap, applies every period's update
(1378–1409 from `crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10`, the later ones from
the captures; the store upgrades to the Gloas layout at period 1379, natively and in the program),
then each capture's finality update and the same bridge transactions as above. It refuses to start
if the vault's slot 2 is not this configuration's `configId`.

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
