# Image ids of the guests at `integration7`

Every guest program rebuilt on 11 November 2026 from the tree of `integration7` after its tenth merge (4b1713c; no later
commit touches a guest) with `scripts/reproduce-image-ids.sh build`, at the crown build path
(`<spike>/ferrata-wt-crown`, the path the deployed programs were built at, so the ids are comparable with
[`e2e/crown/README.md`](../e2e/crown/README.md)), on the Modal build box (RISC Zero 3.0.5, LEZ guests with
`RISC0_HOME=spike/risc0-home-lez`). A guest built without Docker embeds the absolute paths of its sources, so a
build at another path gives other ids. The id is risc0-build's own (`*_ID` in the methods crate's `methods.rs`) and
equals `r0vm --elf <binary> --id` for every row; sha256 and bytes are of the binary risc0-build embeds
(`*_PATH`: the R0BF program, the user ELF with its kernel). Log: `spike/tmp/integ7/logs/post/ids.tsv`
(outside the repository).

EVIDENCE rows of earlier runs keep the ids they ran with; these ids are of the merged programs, which no run
recorded in EVIDENCE has used yet (the branches' own rows name ids built at their worktrees' paths).

| Program | Image id | sha256 of the binary | Bytes | State | What changed it |
|---|---|---|---:|---|---|
| attestation program (`lez/guests/program`): `ferrata_lez` | `0xe6e9bd0f786bbefa283f680e4194a4484aa68cbbc05ebbf43ff1a64b24f3b76f` | `8ae8acb44ac05e137467ce3be43c3a744a95b60d6ac88c39d8cc08be9bd5c80d` | 627,792 | **new** | lc-a1: `Succeed`/`Spawn`, the root id, the weak-subjectivity bound (program and `ferrata-core`, error 53); attest-a2: the shared wire crate (consumer module, `CODE_BLOCKS`); the merge: `BAD_SUCCESSOR` 216, `STALE_CHECKPOINT` 217. Was `0x2228bb46…67cf` (crown run) |
| bridge program (`lez/guests/bridge`): `ferrata_bridge` | `0xc15840dbd33dca7eee38541fd373f7c742e246c371f1a462582905aa010c8850` | `c913375e4b596e9283a916ee6489faac105a7f4b38e42e5b857e1b0b5760072b` | 593,144 | **new** | bridge-controls: the B2 controls (messages 20–27, `Roles`); lc-a1 and attest-a2: the attestation wire it links (`RecordState.root_id`, codes). Was `0x0e9dcd79…bbb6` (crown run) |
| reference consumer (`lez/guests/consumer`): `ferrata_consumer` | `0x1ef33ec6b6ff7a2ab80282dd6ccef90cf71e1f573b4e01fa803e2c4028c97510` | `6db8d57c4046613d0389067b1b34e142c394b87a462bce83aafbb0d2cb92dada` | 471,832 | **new** | attest-a2: rules 5–7, `ClaimDeposit`, `ClaimDepositPrivately`, `Spend`, `CreditHolder`, codes 218–229; lc-a1: the wire |
| `ferrata_holdings` (second consumer, `lez/guests/holdings`): `ferrata_holdings` | `0xa1c96cf7c6d58b9cb941a60af23da8572ac6bddcbf34a7bf5fdddb57e1ec9e9d` | `247f08058405bae3f26498847669baf978668090465101c075fed0fac05497ed` | 307,696 | **new** | lc-a1 and attest-a2: the attestation wire it links (`RecordState.root_id`); its own source unchanged |
| gas-drop receipt (`bridge/guests/src/bin/gasdrop.rs`): `gasdrop` | `0x8223792f78b5bc592384a09eafec759cb13dd9a3bc64a6b43f79ea7e3688db6e` | `0788d2c9601678609cc08a3a3170567d4d9a78fd1e472dc1910984d0a6138842` | 230,776 | **new guest** | relayer: the gas-drop claim statement, 212-byte journal binding `(N, oneTimeAddress, relayer)` |
| attestation program, software Keccak (cycle comparison): `ferrata_lez_keccak_soft` | `0x5d2687152adcc56b2b256d14451d7907ed0f812a422315b839ea0b6768b2a67e` | `dc0380fb3a3e304b824f74a5716e6df89e1508f3b793a68eb5b04268959767de` | 631,020 | **new** | as the attestation program |
| reference consumer, accelerated Keccak (cycle comparison): `ferrata_consumer_keccak_accel` | `0xdd8a2e4dddd719252f0035722155c7a76d95d91534d0374b1eb11b641a71b1cb` | `20c6a4f91ebc8e84741fedd4f687444654af00f1b0c43f919f521cad103b9a6c` | 468,832 | **new** | as the reference consumer |
| scripted test program (`lez/guests/scripted`): `scripted` | `0x4b597adb3f1a43db85829c411a987234cc84584d8bcd947f0774b852adf0da2b` | `94b0f2ab3a87406f9e684fdf4a64b1c9f2c35f1b24e0bb9132097840b42755a5` | 268,468 | unchanged | no source change; the same id as attest-a2's build at another path (it embeds no path) |
| checkpoint, rule (A) (`checkpoint/methods`): `checkpoint` | `0xcf38b945a70b66d9132fb493b4baf1db308de73ccb8b1108e1c1f8a4363525e6` | `9886faadaad3c6604633f9e036f7a1ed2d9c36b567283b491f368a0e63060fc9` | 443,604 | unchanged | equal to the crown run's (the vault's immutable) |
| checkpoint, rule (B): `checkpoint_b` | `0xa475e8f9aaa41d0ae3bf9a20ae7d937ebc637a961808765f0ff175158760d3b2` | `2bcda6f114eb9d4868f81bceb78b24ba27eddf8efc0bf20f43b55438a2bd05c3` | 445,564 | unchanged | equal to the crown run's (the vault's immutable) |
| checkpoint segment: `segment` | `0xd2b30667292418ccf3cf68ee4fa2dc9ed7873de4034fcf1fb2ed3a6305a61cca` | `565cb04955daee7d65a631262093d84ac9125ff8ead7e94cd5ba230b762761c3` | 269,960 | unchanged | equal to the crown run's |
| checkpoint micro-benchmark guest: `micro` | `0x6ad39fe97799c33b286d2ceb245948d1f2de9533a50d7328be30b3935c0c1f06` | `351ef0255d71675b3ed7f02f98f25abc5c0a1e679186f26ffa7cb7a427007bdb` | 408,672 | unchanged sources | no earlier build at this path recorded |
| release (`bridge/guests`): `release` | `0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd` | `055e65011320c6dfd40c18f7fb8e7eba806b89fbe33c0039cb5031a266fc645a` | 230,480 | unchanged | equal to the crown run's (the vault's immutable) |
| claim statement (`bridge/guests`): `claim` | `0x3b42cc69b1c29896203319e64f6934ffba0bbc9dffa3d2faab8fa456a190bc99` | `fa7f4ec1bd341484407c2ac42d50f089a91709f55b3bd48edf500801a3bdd6ba` | 229,536 | unchanged sources | the guest package gained the gasdrop bin; no earlier build of claim at this path recorded |
