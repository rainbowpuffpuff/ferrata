# Image ids of the guests at `integration7`

> **RISC Zero 3.1 (23 November 2026).** The Ethereum-side guests are built with risc0-zkvm 3.1.0: the first table
> lists their ids (`*_31`; how they were built and proven: [the RISC Zero 3.1 section](#risc-zero-31-the-ethereum-side-guests-migration-phase-1)).
> The RISC Zero 3.0.5 ids they replace, which the four Sepolia vaults of November 2026 (capped at zero) and the
> recorded runs pin, are in [Superseded](#superseded-the-ids-the-november-vaults-and-the-recorded-runs-pin). The Sepolia vault of 23 November 2026 (`0x1949…2787`, EVIDENCE row 232) pins the 3.1 rows
> `checkpoint_b_0_3_5_31` (rule (B)), `checkpoint_0_3_5_31` (rule (A)) and `release_31`.

> **Milestone B1 (branch `rfp-bridge-hardening`, 21 November 2026).** In this tree `lez/guests/bridge` builds the bridge
> program of [the B1 section](#the-bridge-program-since-milestone-b1-every-account-keyed-by-the-configuration) below,
> listed in the first table as `ferrata_bridge_b1` (`0x847a4336…7515` since 22 November 2026, ADR 0032: a burn's tag used once;
> B1's first build was `0xac1fb59b…442e`). The `ferrata_bridge` row ([Superseded](#superseded-the-ids-the-november-vaults-and-the-recorded-runs-pin)) stays the
> program the live runs of November 2026 deployed (`crown-live-3`, `crown-live-4`, the LEZ sequencers we ran), which those
> deployments keep; its source is at commit `f13faee2`, which rebuilds it. The tools resolve the two by name:
> `scripts/verify-sepolia.py` holds the live vaults to `ferrata_bridge` and a run of B1's program to `ferrata_bridge_b1`;
> `scripts/reproduce-image-ids.sh compare` and `scripts/privacy/real.sh` expect this tree's build to be
> `ferrata_bridge_b1` (the crown run's other five programs and the other 13 guests are unchanged).


The first table: the 14 guests this tree builds. The LEZ programs' rows are the build of 12 November 2026 described
below (RISC Zero 3.0.5, LEZ's); the bridge program's is B1's ([its section](#the-bridge-program-since-milestone-b1-every-account-keyed-by-the-configuration));
the Ethereum-side guests' are the RISC Zero 3.1 build ([its section](#risc-zero-31-the-ethereum-side-guests-migration-phase-1)).
The 12 November build's other rows, superseded, are in [Superseded](#superseded-the-ids-the-november-vaults-and-the-recorded-runs-pin).

Every guest program of this tree, built on 12 November 2026 from commit 9b9bbd4 (`scripts/reproduce-image-ids.sh build`,
on the Modal build box: RISC Zero 3.0.5, guest Rust 1.88.0 for `checkpoint/` and `bridge/`, 1.91.1 for `lez/`). Guest
builds are **independent of the build path**: [`scripts/guest-rustc`](../scripts/guest-rustc) (cargo's
`build.rustc-wrapper`, [`.cargo/config.toml`](../.cargo/config.toml)) remaps the tree, the upstream sources, the cargo
home and the RISC Zero toolchains to fixed names and replaces cargo's path-dependent `-C metadata` for the zkVM target.
**Proven**: the same commit built at two paths, with two `$HOME`s and two RISC Zero homes (one of them real copies),
gives byte-identical user ELFs and R0BF binaries and equal image ids for all 14 guests (below: run on the Modal build box, 11 Nov 2026 23:03–23:09Z (`spike`-side script `twopath.sh`, logs outside the repository): build A with the tree at `/build/a/ferrata` and `HOME=/build/home-a`, build B with the tree at `/build/elsewhere/deeper/checkout-b`, `HOME=/build/users/other-home` and real copies of `~/.risc0` and of the LEZ RISC Zero home at other paths, both from a fresh target directory, in parallel (225 s and 227 s once the cargo registry was warm); their dep-info files show each compiled from its own paths (1,363 and 1,368 references to the tree, 17,190 and 17,322 to the cargo home), no binary names either build's tree, `HOME` or the box's own paths, and `ids.tsv` of A and B are identical: 14 of 14 guests with the same image id, R0BF binary and user ELF, byte for byte; the crown run's six programs and the four R0BF files compared with `cmp`, equal).
The id is risc0-build's own (`*_ID` in the methods crate's `methods.rs`) and equals `r0vm --elf <binary> --id` for
every row; sha256 and bytes are of the binary risc0-build embeds (`*_PATH`: the R0BF program, the user ELF with its
kernel); the user ELF's sha256 beside it.

EVIDENCE rows of earlier runs keep the ids they ran with (built at their worktrees' paths); the crown run's first
live run, `crown-live-1`, ran programs built at the maintainer's checkout path ([`e2e/crown/README.md`](../e2e/crown/README.md),
"The programs of crown-live-1"). These are the final programs: the live run `crown-live-3` deployed them on Sepolia (vault `0x04eE…e44c`,
[`e2e/runs/crown-live-3/`](../e2e/runs/crown-live-3/README.md); `scripts/reproduce-image-ids.sh compare --vault` on it:
6 of 6); `crown-live-2` stopped before deploying anything ([`e2e/runs/crown-live-2/`](../e2e/runs/crown-live-2/README.md)).

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| attestation program (`lez/guests/program`): `ferrata_lez` | `0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df` | `ddfde69bea89a5b882f55266fcb4b7c887df7debc1c9d4972d8ebea5a72eaad9` | 627,064 | `468754dd15a24c73eaa256aecd1c683531b17286f5db2592590b804fb086bedc` |
| bridge program since B1 (`lez/guests/bridge`): `ferrata_bridge_b1` | `0x847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515` | `537cbe7a4c6900419b54e556b9741a9e37d86adf53f5e65683aa2b22c8e9a036` | 607,448 | `6cbe0aa504ddec7d05189165a5bb6873896e99cc35f6677d91fb35220edbde9f` |
| reference consumer (`lez/guests/consumer`): `ferrata_consumer` | `0x8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96` | `d02e4aa394f73702fec3e9983f32e8c9743dbaa1a41d2d705fc06f1b63e94ca5` | 471,408 | `a403d4b2eca1ae9c205cbb87222a57f381ec512810403cf899d9036560eed754` |
| second consumer (`lez/guests/holdings`): `ferrata_holdings` | `0xa79e3f5c4496f2707ab6f55a980efb00dd80aaf562b51f5a504349e9a97c5591` | `504a67c071a103b5d99de10163ae97f925151b09b59c49048e50f3d315b710c5` | 307,504 | `74f7215ac2641e2772f78ac994b62bd52c9807970df947514a272e03505e4deb` |
| attestation program, software Keccak (cycle comparison): `ferrata_lez_keccak_soft` | `0xefe9c58caa0eacd4c837815d7dbb3c003011b478f9cc5b85fca083b6f0a5cb3e` | `727c5cda08f88274128e873badafd441b9112a5885a407ca6e829f9b53bc1995` | 630,292 | `025acb90bc29fd32472a59a3aeb3eb1330d1e9b446b40e2e695f3e1db3cb6004` |
| reference consumer, accelerated Keccak (cycle comparison): `ferrata_consumer_keccak_accel` | `0xfa9bef9049151bc92ef32de84e0fdc1153447f115b090b8b6414330ce9d243cc` | `bd8d296e0874fd911b3087789f8d57fb947e377242315ebdd9ce3617de5e36e2` | 468,408 | `e469df5cc5f0462abf0f270326f2eaa8dcaa4eeddf24ba85a08d95b91ee66e56` |
| scripted test program (`lez/guests/scripted`): `scripted` | `0x8cb0e3f54dd11a2aea7551227c43e6416a6218e101cc7779c98f4f1a53d64e61` | `29249a3ac807ad3268ff5499ccee70a75b687d5fe5517fa5843b333c2c6c1bc0` | 268,228 | `95ab4dfc4a2e988e69ce1c45566028d5394dca447cce61d68dad504e6fa07e11` |
| checkpoint, rule (A) (`checkpoint/guests/checkpoint`): `checkpoint_31` | `0x7c93425e9b6fedbd0d79092b3a0b6c164b9a6ee250b9e2e89b26a7fadc4497c1` | `9c39655875e4b308342a660eb044f6402c417d9e379fb4870a93a62636eef131` | 443,220 | `f2458fb0c81feb6204f03fed6d31e6e38be5fd1c1069c0cd22dd7635c24cf824` |
| checkpoint, rule (B) (`checkpoint/guests/checkpoint-b`): `checkpoint_b_31` | `0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0` | `6b8952089b4185bd37bbd844387ccc42c84a96535a46c04aa503f785bc988727` | 445,120 | `893f26688965bda281d98981378a41b30a64dc285979ccb4762c7fd973468b9d` |
| checkpoint micro-benchmark guest: `micro_31` | `0x0dc397afcb583efd3f28d7046029f7004b32c83cb7a28ffb78693faa75c588ff` | `356115aa560f9446582c9e4a7db77d9fc89f0a7cfa6abb5e24f75ac762011423` | 408,408 | `fb20b9c0c34ef3889b776e26d8c3b68393fe43ac77aa4e0a7f24db7d5ac05218` |
| checkpoint segment (`checkpoint/methods-segment`): `segment_31` | `0x240766633d834cb2617b3f75501513ebeef89cad61a8730a73c685c4f34b3ad9` | `d9ab28b78af89154a66de81d901c7a397f2d3c29ef97ce4c891987ea7863f4a6` | 269,664 | `02ece4d0ab4eaeeaa2563b634feee4054ca9f44a3d971c5f2d7881971320960d` |
| release (`bridge/guests`): `release_31` | `0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd` | `d0fdcac9fa71ee87d22014ccfbb90f4ac543ae2559838d85a9dd0315c7cae2ac` | 230,360 | `3303ae81cc7f69dd7a4902235f52b5c09061eafe6f5f092ff363dda051cc1722` |
| claim statement (`bridge/guests`): `claim_31` | `0x3c1afd01cf3f73513b9fe2ffa7c811726bfd4e68e00df61688a2fe811639e108` | `89ce51ccc86b8c259ca02a4812575f0535e8f0a6dd8ccfc62bbe1823811dfcfd` | 229,408 | `7f4bdbfbe586e649e7d85cbb3ff4222e3c3f5e84c588e24ed558a358b99c537e` |
| gas-drop receipt (`bridge/guests`): `gasdrop_31` | `0xce3ad533d2c72b38c9779f1c692dd784e3fd754612808696e1362da49c7f8125` | `b0a7dab9e24547e3e234e700c64335cb217810e34cbb08d930914b2f72c84395` | 230,648 | `efa336d2a43323d37dd55d56e96d1491732604e0f28ee66218c6bc880920af10` |

## RISC Zero 3.1: the Ethereum-side guests (migration phase 1)

[RISC Zero's advisories of 11 November 2026](security/risc0-groth16-advisory-2026-11-21/README.md) moved the
Ethereum side to risc0-zkvm 3.1.0 ([migration plan](design/risc0-3.1-migration.md), phase 1): the guests the vault
verifies (`release`, both checkpoint rules in both Bedrock layouts, the segment guest they compose with) and the others
built in the same workspaces (`micro`, and `claim` and `gasdrop`, whose ids are used from phase 2, when the relayer
links a LEZ on 3.1). Their sources did not change; risc0-zkvm 3.1.0 and risc0-build 3.0.7 did (`bridge/` and
`checkpoint/` manifests and lockfiles: circuits 4.1.0, risc0-zkp 3.0.6, risc0-groth16 3.0.6), so every id moved. Rows are
named `<guest>_31`: the rows above, of RISC Zero 3.0.5, stay what the deployed vaults and the recorded runs pin, and
`scripts/verify-sepolia.py` holds a run to the 3.1 rows when its release is `release_31`;
`scripts/reproduce-image-ids.sh compare` expects this tree to build the `_31` rows (`scripts/reproduce/compare_ids.py`,
`TREE_ENTRY`). The LEZ programs (attestation, bridge, consumers) stay on LEZ's RISC Zero (3.0.5) until LEZ is tagged on
3.1 (phase 2).

Built on 23 November 2026 on the Modal build box at `fce77353`: RISC Zero 3.1.0 in a RISC Zero
home of its own (`cargo-risczero` and `r0vm` 3.1.0, guest Rust 1.88.0, C++ 2024.1.5, installed with rzup), through
`scripts/guest-rustc`, `RISC0_BUILD_LOCKED=1`, `cargo build --release -p methods -p methods-0-3-5 -p methods-plain`
in `checkpoint/` and `-p methods` in `bridge/` (250 s, both at once), then `python3 -I scripts/reproduce/guest_ids.py`:
for every row r0vm 3.1.0 computes risc0-build's id from the binary. The binaries' sizes equal the 3.0.5 builds' (the
guests link the same v1compat kernel, risc0-zkos-v1compat 2.2.3), their bytes do not. `micro_plain`
(`checkpoint/guests-plain`, not in a table before) builds to `0x3cd9e8be5f4708f55a713fc53ad81cdeb5b06c645d564e5adb27ec8dfdacbd3b`.
Built a second time the same day on the same box from a copy of the tree at another path (`…/repro2/elsewhere/checkout`)
in a target directory that did not exist before (243 s): 9 of 9 ids equal the rows below, r0vm "= id" for each.

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF | Before (RISC Zero 3.0.5) |
|---|---|---|---:|---|---|
| checkpoint, rule (A), 0.3.5-rc.1 layout (`checkpoint/guests-0-3-5/checkpoint`): `checkpoint_0_3_5_31` | `0x9140ec7479debc9b40529b0805095d4cfdef93a1e0500c6687a1d7ec4d7c42de` | `0150091f569610763c4719f0f05964a7bfb620fb7c9de02a1fd04e9fcebc19a1` | 446,600 | `8dc8850d8f085377c172b058ab80975704abe37f36f88e31ec4720682f5c7c1f` | `0x259cd89e…9b0f` |
| checkpoint, rule (B), 0.3.5-rc.1 layout (`checkpoint/guests-0-3-5/checkpoint-b`): `checkpoint_b_0_3_5_31` | `0x8917f58e76aa805dedc04fc5e2b70404f5078b66b271ec44aaa0c78d9f5eaf04` | `86eaa5502bffbb85d8661e18d24ae83a797cab5e2cdc4db70c01b6a7ae20bbdc` | 448,132 | `e34f9e92369bf50535462df1720f6dbe9f9120274894d00a0049a6dc91101513` | `0xafbf4023…3bcf` |

## For zones on Logos Blockchain 0.3.5-rc.1

The second build of the two checkpoint guests: `checkpoint/guests-0-3-5/` (their own workspace and lockfile; the
lockfile is `checkpoint/guests/`'s with only the workspace members changed), built by `checkpoint/methods-0-3-5` with
checkpoint-core's cargo feature `bedrock-0-3-5`, which is off by default and which the 14 guests above do not compile
(their sources are byte for byte what they were). They read Bedrock transactions in the layout of Logos Blockchain
0.3.5-rc.1 (`f6d4267f`, the node LEZ v0.4.0-rc3 pins): a 32-byte fork digest before the operations, hashed with them,
required to equal the configuration's `bedrockGenesisId` (for such a zone, its fork digest); and a channel configuration
with an unreachable threshold refused ([`checkpoint-core/src/bedrock_0_3_5/`](../checkpoint/crates/checkpoint-core/src/bedrock_0_3_5/mod.rs)).
Built on 18 November 2026 on the Modal build box from this tree (RISC Zero 3.0.5, guest Rust 1.88.0, through
`scripts/guest-rustc`), after `scripts/reproduce-image-ids.sh build` and `compare --vault 0x04eE…e44c` on the same tree
and box gave 14 of 14 and 6 of 6 for the table above
([`checkpoint/results/bedrock-0-3-5/`](../checkpoint/results/bedrock-0-3-5/box1-compare.log)). Rebuild:
`cd checkpoint && cargo build --release -p methods-0-3-5 --locked`, then `python3 -I scripts/reproduce/guest_ids.py . OUT.tsv`
from the root lists these two with the others.

Their RISC Zero 3.0.5 rows (`checkpoint_0_3_5`, `checkpoint_b_0_3_5`), which the recorded runs on such zones
pin, are in [Superseded](#superseded-the-ids-the-november-vaults-and-the-recorded-runs-pin); this tree builds
`checkpoint_0_3_5_31` and `checkpoint_b_0_3_5_31` ([RISC Zero 3.1](#risc-zero-31-the-ethereum-side-guests-migration-phase-1)),
and the Sepolia vault of 23 November 2026 pins `checkpoint_b_0_3_5_31`.

## The bridge program since milestone B1: every account keyed by the configuration

`lez/guests/bridge` since B1: the second build of 18 November (`0xc5cad2cd…d6da`: `nullifier(cid, chain, vault, N)`
and `holder(cid, account)`, `Init` refusing a freeze authority that is the admin), now the program itself, with the
inbound finality depth set by the admin (`SetDepth`, RFP-022 F.12) and the LEZ mirror of the vault's delisting
(`AttestListing`, F.10) added; `lez/guests/bridge-next` and `lez/methods-bridge-next` are gone (their source is at
`f13faee2`). [`docs/design/bridge.md`](design/bridge.md) §3, "Known issues, 18 November 2026", describes what it fixes;
the regression tests are `lez/harness/tests/tenant.rs`. **Not deployed: no vault pins it.** A deployment of B1 deploys
it with a new vault, whose `configId` commits to this image id; the vaults and LEZ programs deployed before keep the
first table's program (their configuration ids commit to it), with its known issues. Built on 21 November 2026 on the
Modal build box from this tree (`cd lez && cargo test -p harness --no-run --locked`: RISC Zero 3.0.5, guest Rust 1.91.1,
through `scripts/guest-rustc`, in a target directory that did not exist before), where the attestation program of the
first table rebuilt to its recorded id `0x816d1fef…03df`; `r0vm --elf ferrata_bridge.bin --id` gives the same id.

**Rebuilt on 22 November 2026 ([ADR 0032](decisions/0032-burn-tag-used-once.md)): a burn's tag is used once.** The burn
record is `burn(cid, r)` instead of `burn(cid, H(note))`, so a second note with a used tag is refused (the reviews of
that day: two notes with one tag were both applied and shared one release nullifier). That moves the id below from
B1's first build, `0xac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e` (binary sha256
`5f47b065…7fd8`, 607,608 bytes, user ELF `e240c82e…d6ee`), which no deployment pins either. Built on the Modal build box
by `scripts/reproduce-image-ids.sh build` after `cargo clean -p methods` in `lez/` (one build output, no stale one),
then `compare`: the 13 other guests of the first table and the crown run's other five programs equal their recorded ids
(the bridge core lost `register_burn_tag`, a helper no guest calls: `release`, `claim` and `gasdrop` unchanged); `r0vm`
gives the same id. It ships with the RISC Zero 3.1 rebuild (phase 2,
[`design/risc0-3.1-migration.md`](design/risc0-3.1-migration.md)), whose LEZ guests all move again.

Its row is in the first table (`ferrata_bridge_b1`): this tree builds it.

## The attestation program with statement classes; not deployed

The second build of the attestation program: `lez/guests/program-class/` (its own workspace; its lockfile is
`lez/guests/program/`'s with the package renamed and `ferrata-lez-wire-class` added) compiles the deployed program's own
`lez/guests/program/src/main.rs` against the new crate `lez/wire-class` under the name `ferrata_lez_wire`, whose
`attest` replies with each statement's nullifier-safety class beside its unchanged identifier
([`docs/design/attestation.md`](design/attestation.md) §5.7); built by `lez/methods-program-class`. No file the 14
guests of the first table compile changed. Built on 18 November 2026 on the Modal build box from this tree (RISC Zero 3.0.5, guest
Rust 1.91.1, through `scripts/guest-rustc`, `RISC0_BUILD_LOCKED=1`), after `scripts/reproduce-image-ids.sh build` and
`compare --vault 0x04eE…e44c` on the same tree and box, on which no target directory had existed, gave 14 of 14 for the
first table and 6 of 6 for the crown run's recorded ids, the vault's `checkpointImageId` and `releaseImageId` among them
([`lez/results/history-hops/image-ids/`](../lez/results/history-hops/image-ids/compare.txt)); the
id compiled into the harness the tests ran is the same. Rebuild: `bash lez/results/history-hops/run-class-id.sh` from
the root after `run-image-ids.sh`, or `cd lez && cargo build -p methods-program-class --locked`, then
`python3 -I scripts/reproduce/guest_ids.py . OUT.tsv` from the root lists it with the others.

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| attestation program with statement classes (`lez/guests/program-class`): `ferrata_lez_class` | `0x0927560f3e6e6c795ede77041f872458f7802a4a4e1c84450b114fa9147f942c` | `0480cc6d1338ed4d45d5825b94ff9c776a313da342d7bfb9b4e22f6ab5ee478e` | 628,228 | `84f191eb2f447d9202631fac15fc4ae03e5a67311dd8300dc1c74570ab07e74e` |

## The recorded-statement programs and the rebate program; not deployed

`lez/guests/statements` (`ferrata_statements`), `lez/guests/statements-reader` (`ferrata_statements_reader`; RFP-021
soft requirement 1, [`docs/guides/recorded-statements.md`](guides/recorded-statements.md)) and `lez/guests/rebate`
(`ferrata_rebate`; soft requirement 5, [`docs/guides/operator-rebate.md`](guides/operator-rebate.md)): new programs
beside the attestation program, which none of them changes; built by `lez/methods-statements` and `lez/methods-rebate`
behind the harness features `statements` and `rebate`, and run only in the in-process harness. Built on 21 November
2026 on the Modal build box from branch `rfp-softs` after the review's fixes (exit codes 62–79, the reader's
configuration, the pool's terms), through `scripts/guest-rustc` like every guest of this tree; their path independence
was not shown separately (the two-path build above covers the 14 guests of the first table). The same build gave the
attestation program `0x816d1fef…03df`, the bridge program `0xccde6171…71dd` and the other LEZ guests of the first
table unchanged. Rebuild: `cd lez && cargo build -p harness --features statements,rebate --locked --tests`, then
`python3 -I scripts/reproduce/guest_ids.py . OUT.tsv` from the root.

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| recorded statements (`lez/guests/statements`): `ferrata_statements` | `0x86acef86dc73be13b76de63d21d41ac928639f3be7144132f22f14831d5bc663` | `439f6ac628ef691f321545b77b1819136fcef6965b7a4b31b031568f4c63a0fa` | 318,504 | `27220d1b3ca1ff253ef346af26ee3c7516bb2e682259199ed05fbfb8d914fd89` |
| their reference reader (`lez/guests/statements-reader`): `ferrata_statements_reader` | `0xd2a8b082f1e4326daad8aad58c2a525f76f17001bb362040fec67260d4d3e73d` | `a74b740eb72c18a06d62a8f55c845fa88a171c913604bf3bdb5d9e2c0d39aa18` | 316,572 | `8a804edfc5b9fa054717ca5c69e7552b2dd58b843f29461fccd6fcff00cf1acf` |
| the operator rebate (`lez/guests/rebate`): `ferrata_rebate` | `0xe8f1c1c1381a08faf284fd33226ae11fcb4b7b7d3c397faa113b76d9c7e6d902` | `9891fe89165220ae46fd5d1ad6f285b750ceb2b65a431b030356bee1b6e5b1bd` | 290,528 | `f800cf60d551e0a24e0161665205b7b94dfa8da0dd840392361eaa06e9560b04` |

## The test network's names program (`ferrata_names`; 23 November 2026)

`lez/guests/names` with `lez/names-wire` ([`names/README.md`](../names/README.md)): a new program, built by
`lez/methods-names` behind the harness feature `names`, on the Modal build box (RISC Zero 3.0.5, guest Rust 1.91.1,
through `scripts/guest-rustc`); no other program changes. Each id is risc0-build's (`FERRATA_NAMES_ID`) and equals
`r0vm --elf <binary> --id`; the rc1 and rc3 builds of one source differ only by the LEZ tag `lee_core` is taken from.
The rows are [`names/image-ids.md`](../names/image-ids.md)'s (raw: `names/e2e/results/image-ids.tsv`), each under its
own name here. Rebuild: `names/e2e/box.sh build && names/e2e/box.sh ids`.

- v1 is **live and immutable** on our test network (header `4EBU635H…`, `/programs.json`; the review read it on 23
  November 2026). Its source, before the review's fixes, differs from v2's in `lez/names-wire/src/lib.rs` only, kept as
  [`lez/names-wire-v1`](../lez/names-wire-v1/README.md): `scripts/reproduce/deployed.sh` rebuilds it to the v1 rows
  below (row 238).
- v2 (the review's fixes) is **not yet on the public test network**: a new program at a new address, which does not
  read v1's records.

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| names v2, the LEZ v0.4.0-rc3 build, to deploy (`lez/guests/names`): `ferrata_names_v2_rc3` | `0x28d63f2075ffb92d653149f9da00b20c85ab77a365f31263c6483387cd43be9d` | `d6f7c2497a5aa671f4fa6b53ddc51cc9c80310ee6c2d67d55385bd089e52c293` | 306,404 | `34bc4c0980d8e0d8aae4411e7cf9928d69d463cf46dd8260d60932f67ce15289` |
| names v2, the LEZ v0.4.0-rc1 build (the harness default): `ferrata_names_v2_rc1` | `0x8417e74bb939bf373abe69e1f34807cc7e64f5d7df4880766ab8a5c0a028255b` | `ac9f386fce3f39a1f62c22f86ad18eaa5b3f88f3a5de0715be4964fb09ace752` | 306,404 | `26f2cc4b718f27ca412f71e6bb615fd2ca1fdba213fa986ed499182127cfdc38` |
| names v1, the LEZ v0.4.0-rc3 build, live: `ferrata_names_v1_rc3` | `0xee9578cab413451e1f0a0cf267a7874ca3231ed0a30907f5d793648ac4e6f790` | `5653a2ee1c3a85f3b29f32a1a227030a2619c096d0dd7c6c92b872e9e7ce75ab` | 287,672 | `eacdfd305520939ba4f092371159a484ec07a1133b5c7e43b764b9a03e9844e5` |
| names v1, the LEZ v0.4.0-rc1 build: `ferrata_names_v1_rc1` | `0xe4b7d67291668c6309449e3b1d9627a91fdc5916eaa8344c949f3657ea54a71b` | `f232f4b0520b4038abaf4990eafb592ec53ffaaec3e5d93dd5a71c65ecf936c1` | 287,672 | `6e0c91ea65fee6fbef135977fbfb0837210640dec073377680a4da1e54746ec7` |

## The test network's messages programs (`ferrata_msg`, `ferrata_msg_v2`; 23 November 2026)

`lez/guests/msg` (`ferrata_msg`, v1) and `lez/guests/msg-v2` (`ferrata_msg_v2`): encrypted wallet-to-wallet messages
([`docs/design/messages.md`](design/messages.md)), new programs built against LEZ v0.4.0-rc3's `lee_core` by
`lez/methods-msg` behind the harness feature `msg`, on the Modal build box (RISC Zero 3.0.5, guest Rust 1.91.1, through
`scripts/guest-rustc`); no other program changes. Each id is risc0-build's (`FERRATA_MSG_ID`, `FERRATA_MSG_V2_ID`, printed
in `lez/results/messages.md` and `lez/results/messages-v2.md`) and equals `r0vm --elf <binary> --id`. Rebuild:
`cd lez && cargo build -p methods-msg --locked`.

- `ferrata_msg` is **live and immutable** on our test network since 23 November 2026 (header
  `ExZxU36vgBErfDHpnsM97euf4ubx7gL8WpX7DFsYrVTe`, `/programs.json`); this tree keeps its source byte for byte, and it
  rebuilt to the same binary after v2 was added beside it.
- `ferrata_msg_v2` replaces it (the review of 23 November 2026, `docs/design/messages.md` "v1"): deployed by LEZ's CLI on
  a local LEZ v0.4.0-rc3 sequencer (`deploy/testnet/web/msg/test/results/local-2/`), **not yet on the public test
  network**. A new program id: v1's inboxes stay where they are, readable by the clients (`ferrata-msg inbox --legacy`).

| Program | Image id | sha256 of the binary | Bytes |
|---|---|---|---:|
| encrypted messages, v1, live (`lez/guests/msg`): `ferrata_msg` | `0x04ae1988fe84eef6da472152a75b17e6c7453e6e4775264f12af1a750b945277` | `5b7af994f1169f2b6a905ccfda478f098f5bdef9e09cce0edae07fa8fe733134` | 275,444 |
| encrypted messages, one account per message (`lez/guests/msg-v2`): `ferrata_msg_v2` | `0x480f5c4eedb3bde85c26e3e408c962d91cdea4941391b3e5ac63aefdb2eb4f59` | `ebcc609a1bd79cb924813f8ce856b6ae6b0465a80d5e3311693ac8e922091e95` | 300,620 |

## Superseded: the ids the November vaults and the recorded runs pin

Rows this tree no longer builds, kept under their names because deployments and records pin them: the Ethereum-side
guests under RISC Zero 3.0.5 (the build of 12 November 2026 described above; for zones on Logos Blockchain
0.3.5-rc.1, the build of 18 November 2026), which the four Sepolia vaults of November 2026 (capped at zero, [security
note](security/risc0-groth16-advisory-2026-11-21/README.md)) and the recorded runs verify, and the bridge program
before B1, which the live runs deployed on LEZ (`crown-live-3`, `crown-live-4`). Each rebuilds from the tree before
its change: `3d984678` for the 3.0.5 rows (before RISC Zero 3.1), `f13faee2` for `ferrata_bridge` (before B1).
`scripts/verify-sepolia.py` holds a recorded run to these rows by name; `scripts/reproduce/compare_ids.py` compares
this tree's build with the first table.

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| bridge program (`lez/guests/bridge` before B1, at `f13faee2`; the live runs'): `ferrata_bridge` | `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` | `11d832482ff0a26c8f283edda1a35b262274c066734895c62f696394dcab6070` | 592,840 | `6d038fd3628c9ba97f8d1ec2c35326696dcb92867eab7894f6f703e901d8604b` |
| checkpoint, rule (A) (`checkpoint/guests/checkpoint`): `checkpoint` | `0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84` | `e9e800e9a8d823587ce4060ce1c0205277d31b02beebd6f9e398beb940b9a3ce` | 443,220 | `c173a69697be26ab254d87d6ac92d51e9c97f570839f1df03dea9c3a5dcbf1c1` |
| checkpoint, rule (B) (`checkpoint/guests/checkpoint-b`): `checkpoint_b` | `0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e` | `7b5c3735635ff116ea2f2a0a1eae8929dd716ce57a7839368df683b5ad500e3b` | 445,120 | `e6c39a2e4ccbf065cb0da63de1d39df01d90ab1c171e9c4bdbc867049f690d0f` |
| checkpoint micro-benchmark guest: `micro` | `0xcf7aa50c95fe0832ede59ebcef28c98020d83739c70cccb4d4687e1bf9dc5843` | `5aac454e68f929694a037019e7b508edd392756cf55c0784ef2140eab008ec81` | 408,408 | `1f1a19374b5e96bafce801d8a9f29f7b4a8fd7c712f41b5f2aef2a79a07943f5` |
| checkpoint segment (`checkpoint/methods-segment`): `segment` | `0x11e888e94b9520b08477ba5c5c29cc46bdffc7b1e7c5fe822eb0776f40c54592` | `2beb911014fa9b83bb27a61dbb66c396b291fd98df835424c4a45e82624960ed` | 269,664 | `ae969858932085bcfe0c47e2af02a4be67545bea62686cc23b1bebe9aca9a1a0` |
| release (`bridge/guests`): `release` | `0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9` | `7be1349cba34b670c0ef63e2093c76885e648d9f9875f5d378ce1706b73f11bf` | 230,360 | `235b2b8441eeb8036a9558932e751814b3c5f97da66cd46a80f958c790d6e714` |
| claim statement (`bridge/guests`): `claim` | `0xde8d67a46e07254f2fc31202cbae5a35d26f6bfc96e6e436ba732281015965f4` | `ca2c25bf41c307355c0f917427b35469d880b374c3ae16ab26017c73ad78bd02` | 229,408 | `48ab6ae540f4a292eedb6edd0845b93fb166167eacd71953284882f38d48bd2e` |
| gas-drop receipt (`bridge/guests`): `gasdrop` | `0x895804161df14b00e1cb02e5d27fe136efb82b178db511c7697c030676fc9eb4` | `c2119aee6f28359f383d8f1a2e14829f95bdc86e92d52790e442dd823cb43162` | 230,648 | `aa1f819bd46b25cac570db21b4abeb0d84b81c779dccbaad24e5a5fa50cb2114` |
| checkpoint, rule (A), 0.3.5-rc.1 layout (`checkpoint/guests-0-3-5/checkpoint`): `checkpoint_0_3_5` | `0x259cd89ebd255d34dfbfbbc22e4922d25f5f5d1a57858871475e187e399b9b0f` | `4e872fb563f99e176594889d382626144c4958e0a2cca5408178ff5a27569a0e` | 446,600 | `9f86858ee059cd28230b32c31d3f6f6b33ae46d7ce88976c34e38f6d89048f26` |
| checkpoint, rule (B), 0.3.5-rc.1 layout (`checkpoint/guests-0-3-5/checkpoint-b`): `checkpoint_b_0_3_5` | `0xafbf402314e9498f3854f02d533e2a355d4692ca2b64b913bcb0232200c63bcf` | `66ac3a9c20794dc36d35a2d21fedf18e78c79c442f82ac70a3e2d59cd5d8ec77` | 448,128 | `3fb2b8da3d82e9b454c7d4d73a56b61e68664e78e93401cded9b13189e131ff0` |
