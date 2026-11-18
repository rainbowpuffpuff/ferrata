# Image ids of the guests at `integration7`

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
| bridge program (`lez/guests/bridge`): `ferrata_bridge` | `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` | `11d832482ff0a26c8f283edda1a35b262274c066734895c62f696394dcab6070` | 592,840 | `6d038fd3628c9ba97f8d1ec2c35326696dcb92867eab7894f6f703e901d8604b` |
| reference consumer (`lez/guests/consumer`): `ferrata_consumer` | `0x8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96` | `d02e4aa394f73702fec3e9983f32e8c9743dbaa1a41d2d705fc06f1b63e94ca5` | 471,408 | `a403d4b2eca1ae9c205cbb87222a57f381ec512810403cf899d9036560eed754` |
| second consumer (`lez/guests/holdings`): `ferrata_holdings` | `0xa79e3f5c4496f2707ab6f55a980efb00dd80aaf562b51f5a504349e9a97c5591` | `504a67c071a103b5d99de10163ae97f925151b09b59c49048e50f3d315b710c5` | 307,504 | `74f7215ac2641e2772f78ac994b62bd52c9807970df947514a272e03505e4deb` |
| attestation program, software Keccak (cycle comparison): `ferrata_lez_keccak_soft` | `0xefe9c58caa0eacd4c837815d7dbb3c003011b478f9cc5b85fca083b6f0a5cb3e` | `727c5cda08f88274128e873badafd441b9112a5885a407ca6e829f9b53bc1995` | 630,292 | `025acb90bc29fd32472a59a3aeb3eb1330d1e9b446b40e2e695f3e1db3cb6004` |
| reference consumer, accelerated Keccak (cycle comparison): `ferrata_consumer_keccak_accel` | `0xfa9bef9049151bc92ef32de84e0fdc1153447f115b090b8b6414330ce9d243cc` | `bd8d296e0874fd911b3087789f8d57fb947e377242315ebdd9ce3617de5e36e2` | 468,408 | `e469df5cc5f0462abf0f270326f2eaa8dcaa4eeddf24ba85a08d95b91ee66e56` |
| scripted test program (`lez/guests/scripted`): `scripted` | `0x8cb0e3f54dd11a2aea7551227c43e6416a6218e101cc7779c98f4f1a53d64e61` | `29249a3ac807ad3268ff5499ccee70a75b687d5fe5517fa5843b333c2c6c1bc0` | 268,228 | `95ab4dfc4a2e988e69ce1c45566028d5394dca447cce61d68dad504e6fa07e11` |
| checkpoint, rule (A) (`checkpoint/guests/checkpoint`): `checkpoint` | `0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84` | `e9e800e9a8d823587ce4060ce1c0205277d31b02beebd6f9e398beb940b9a3ce` | 443,220 | `c173a69697be26ab254d87d6ac92d51e9c97f570839f1df03dea9c3a5dcbf1c1` |
| checkpoint, rule (B) (`checkpoint/guests/checkpoint-b`): `checkpoint_b` | `0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e` | `7b5c3735635ff116ea2f2a0a1eae8929dd716ce57a7839368df683b5ad500e3b` | 445,120 | `e6c39a2e4ccbf065cb0da63de1d39df01d90ab1c171e9c4bdbc867049f690d0f` |
| checkpoint micro-benchmark guest: `micro` | `0xcf7aa50c95fe0832ede59ebcef28c98020d83739c70cccb4d4687e1bf9dc5843` | `5aac454e68f929694a037019e7b508edd392756cf55c0784ef2140eab008ec81` | 408,408 | `1f1a19374b5e96bafce801d8a9f29f7b4a8fd7c712f41b5f2aef2a79a07943f5` |
| checkpoint segment (`checkpoint/methods-segment`): `segment` | `0x11e888e94b9520b08477ba5c5c29cc46bdffc7b1e7c5fe822eb0776f40c54592` | `2beb911014fa9b83bb27a61dbb66c396b291fd98df835424c4a45e82624960ed` | 269,664 | `ae969858932085bcfe0c47e2af02a4be67545bea62686cc23b1bebe9aca9a1a0` |
| release (`bridge/guests`): `release` | `0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9` | `7be1349cba34b670c0ef63e2093c76885e648d9f9875f5d378ce1706b73f11bf` | 230,360 | `235b2b8441eeb8036a9558932e751814b3c5f97da66cd46a80f958c790d6e714` |
| claim statement (`bridge/guests`): `claim` | `0xde8d67a46e07254f2fc31202cbae5a35d26f6bfc96e6e436ba732281015965f4` | `ca2c25bf41c307355c0f917427b35469d880b374c3ae16ab26017c73ad78bd02` | 229,408 | `48ab6ae540f4a292eedb6edd0845b93fb166167eacd71953284882f38d48bd2e` |
| gas-drop receipt (`bridge/guests`): `gasdrop` | `0x895804161df14b00e1cb02e5d27fe136efb82b178db511c7697c030676fc9eb4` | `c2119aee6f28359f383d8f1a2e14829f95bdc86e92d52790e442dd823cb43162` | 230,648 | `aa1f819bd46b25cac570db21b4abeb0d84b81c779dccbaad24e5a5fa50cb2114` |

## For zones on Logos Blockchain 0.3.5-rc.1; not deployed on Sepolia

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

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| checkpoint, rule (A), 0.3.5-rc.1 layout (`checkpoint/guests-0-3-5/checkpoint`): `checkpoint_0_3_5` | `0x259cd89ebd255d34dfbfbbc22e4922d25f5f5d1a57858871475e187e399b9b0f` | `4e872fb563f99e176594889d382626144c4958e0a2cca5408178ff5a27569a0e` | 446,600 | `9f86858ee059cd28230b32c31d3f6f6b33ae46d7ce88976c34e38f6d89048f26` |
| checkpoint, rule (B), 0.3.5-rc.1 layout (`checkpoint/guests-0-3-5/checkpoint-b`): `checkpoint_b_0_3_5` | `0xafbf402314e9498f3854f02d533e2a355d4692ca2b64b913bcb0232200c63bcf` | `66ac3a9c20794dc36d35a2d21fedf18e78c79c442f82ac70a3e2d59cd5d8ec77` | 448,128 | `3fb2b8da3d82e9b454c7d4d73a56b61e68664e78e93401cded9b13189e131ff0` |

## The bridge program's second build: nullifier and holder accounts keyed by the configuration; not deployed

`lez/guests/bridge-next/` (a workspace of its own; its lockfile is `lez/guests/bridge/`'s with only the package's name
changed), built by `lez/methods-bridge-next`: the bridge program above with `nullifier(cid, chain, vault, N)` and
`holder(cid, account)` in place of `nullifier(chain, vault, N)` and `holder(account)` (bridge-wire's cargo feature
`config-keyed`, off by default, which the deployed program does not compile: `lez/bridge-wire/src/config_keyed.rs`,
declared at the end of `lib.rs`, so no line of the deployed program's sources moved). It fixes the two known issues
of [`docs/design/bridge.md`](design/bridge.md) §3 ([`lez/results/bridge-next.md`](../lez/results/bridge-next.md),
[EVIDENCE](EVIDENCE.md) row 143). **Not deployed: no vault pins it**; milestone B1 would deploy it with a new vault,
whose `configId` commits to this image id. Built on 18 November 2026 on the Modal build box from this tree (RISC Zero
3.0.5, guest Rust 1.91.1, through `scripts/guest-rustc`), in the same fresh target directories in which
`scripts/reproduce-image-ids.sh build` had rebuilt every guest of the first table (14), and after `compare --vault 0x04eE…e44c` had
compared them: 14 of 14 and 6 of 6, the vault's `checkpointImageId` and `releaseImageId` equal to the rebuilt ids; then
15 of 15 with this one ([`lez/results/bridge-next/reproduce/`](../lez/results/bridge-next/reproduce/compare.log); the
target directories did not exist when the build started: [`lez/results/bridge-next.md`](../lez/results/bridge-next.md)). Rebuild: `cd lez && cargo build -p methods-bridge-next --locked` (with `RISC0_HOME` the
LEZ RISC Zero home, as for `methods`), then `python3 -I scripts/reproduce/guest_ids.py . OUT.tsv` from the root lists it
with the others.

| Program | Image id | sha256 of the binary | Bytes | sha256 of the user ELF |
|---|---|---|---:|---|
| bridge program, second build (`lez/guests/bridge-next`): `ferrata_bridge_next` | `0x7c7d2b3acc2f18e5714fa9fc543015bd4dadc5ce625d2b2bbdec50b56ab71e2f` | `692b30c16f817771a7bffbc00229a42452b76e39c8b4ecf17bcd593a9e189ae4` | 593,224 | `cbbee74111c528cd2d0de11183c81fcc6a18255c0d28a3a9ec231c5cd3b39671` |

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
