# The lez/ guests built against LEZ v0.4.0

LEZ v0.4.0 (a255b42c, 27 November 2026) moved to RISC Zero 3.1.0 (LEZ #1124: risc0-zkvm 3.1.0, risc0-build 3.0.7,
risc0-zkos-v1compat 2.2.3) and changed the guest ABI (LEZ #1144: `ReceiveInput.grants`), so every program's image id
changes. Built on the Modal build box with `lez/scripts/guest-ids.sh --tag TAG --out DIR` (every methods crate of
lez/, through scripts/guest-rustc: paths remapped, guest Rust 1.91.1 as LEZ's own builds), on 28 November 2026.

- **v0.4.0-rc1 build** (the committed default, after the port): equal to the base commit fb27c1e2's build on the same
  box for all 15 guests, and to the ids docs/image-ids.md records: the port leaves the release candidates' programs
  as they were. The five guests whose source reads the old API (bridge, consumer, msg, msg-v2 and names, through
  names-wire) build on v0.4.0 from generated twins (`*_lez040.rs`, lez/scripts/lez040-sources.py), and the selector
  points their manifests at the twins on v0.4.0 only.
- **v0.4.0-rc3 build**: as recorded in ../v0.4.0-rc3/image-ids.md (the 7 guests of `methods` then).
- **v0.4.0 build**: new ids, none deployed anywhere.

| guest | v0.4.0-rc1 build | = base | v0.4.0-rc3 build | v0.4.0 build | v0.4.0 binary sha256 | bytes |
|---|---|---|---|---|---|---:|
| `ferrata_bridge` | `0x9d02d54d…` | yes | `0x8f8efd14…` | `0x7e39359d8d447d6e98696a4a64266c0d12aaa2a2bdcfd0a3704545fbbfbc26e0` | `6c943d7f71392b93bf85073a11341f4e8cfe1287af5b04b54ad6d99cdf7328c9` | 646,252 |
| `ferrata_consumer` | `0x8464603b…` | yes | `0x9dc21f01…` | `0x41a36bde4976fb08fafc8df730d0be5dac5520f8eb9081aa42a25c97f01029c2` | `6fc67c84d1d9fa8441addc2f919d370bb4f45c3eea96137643bc5f3205e04388` | 485,892 |
| `ferrata_consumer_keccak_accel` | `0xfa9bef90…` | yes | `0x36b90b3e…` | `0x52ff31ab56eeade382cc71593d43e11ceb2d9b2bef7b938901945c36c885faa5` | `6fdfe82b15e9b266cb47c83af35d5785ccffdd6763a9b6a2d2dc422217664314` | 482,888 |
| `ferrata_holdings` | `0xa79e3f5c…` | yes | `0xca94902d…` | `0x79a7bab4b979842ed2032f8813fcf60208dea4a7a920c642c1b94be74b64f9fb` | `eacdb5db3149860457f5592c4200dc115345df408bdeec3da06f10cd53bf9298` | 322,220 |
| `ferrata_lez` | `0x816d1fef…` | yes | `0xefd9aef7…` | `0x95a93abfe4073a7eaad8f86be311af45e7729f1c3dba7f1b0ce016b7b1b8e2f1` | `66466e83abdfecfca097690b05c5e45eb8ed50bb4f9f5536ca8a02bd9550e761` | 641,184 |
| `ferrata_lez_class` | `0x0927560f…` | yes | - | `0xa971293b2d597dad6cd7fd7d8aac78c74478e20bed28f00014449effed4836ea` | `855e328a6fdbe25a8e56b2b51dfa122e01cbba381cb74150ca21dcabeb143cd6` | 642,348 |
| `ferrata_lez_keccak_soft` | `0xefe9c58c…` | yes | `0x44f54150…` | `0x8acaa28d8d9bc6fbeb203b0883dcc2e1e2e086e017fdf9b163b264008239afcf` | `09bc46b2c6dc48fef5e471783015fcd4579fee9c845df38aba815c7db169ffc4` | 644,412 |
| `ferrata_lez_residue_experimental` | `0xe4ae0234…` | yes | - | `0xc4db35ad583b10ffdc66e0429db14d4512ae7dbdf8a0d3906d5795f09b0e939f` | `79661455036bef803245cb54d501f86926c0b52c0f344bf072eebcf23c6ef245` | 661,620 |
| `ferrata_msg` | `0x04ae1988…` | yes | - | `0xcefe356029b86d4efd67cd57738cc7010afa8842fd0c35ec12caf7f7eacbe553` | `0bb512045eb65a8c0bb90f7a9a1329b11584f78a3827d48b470f6497dbe268b8` | 290,008 |
| `ferrata_msg_v2` | `0x480f5c4e…` | yes | - | `0xe3c2b44875ad8c5d9bdc93629e991ec40b3960e2f0403ea625b3a65e9bbc7313` | `df1f18aa682427baeb1e516a1c173c7a5f22fbaa33de8c6ed1aacdbdcaade3c4` | 315,224 |
| `ferrata_names` | `0x8417e74b…` | yes | - | `0x9339342a2c1251e00321740f8f4cea330a2f3632f99e5c58a555f17d031937a4` | `9f7645c4ac2ba0236c7a71b2dab11052748c0f582f1da68c3c0d1c0d310d39c8` | 321,920 |
| `ferrata_rebate` | `0xe8f1c1c1…` | yes | - | `0xeddda0baf6daac70d4baf3aa2446b8907578f9f863817ff1353d05511eb18712` | `d20c19ee4210cb87dd5aceb0a9a901669866d91970bd18fa412ad32e9da287a4` | 305,052 |
| `ferrata_statements` | `0x86acef86…` | yes | - | `0x513ee15e7bfacc75b44a75a9b096a3adc779683debcfb79701e0b457f89f4b89` | `c1b2ff5bb1539dc353c47b75605ff635870fff15b7dcdfaf5ab5f035cc73a8b1` | 333,132 |
| `ferrata_statements_reader` | `0xd2a8b082…` | yes | - | `0x5086019aba6ce2849ad50d53e47445a934b61fbbff4b8f2dcfc914587a552be2` | `d90084a6400765e2bd3fb0fa3afd5456356aea732ee42691c88d4e8d8671b780` | 330,900 |
| `scripted` | `0x8cb0e3f5…` | yes | `0x3af31a8d…` | `0x0a37d9ba1f59c9abb457f3fd62d6b7bd7e15ccb327d24f847d8cd61f51ce329c` | `acf5aad4ea41d112a8025dcdbe4ddfd5aabc00e3b98141c2e843e5de935c35e1` | 284,236 |

15 of 15 v0.4.0-rc1 builds equal the base's; 14 of 15 appear in docs/image-ids.md (ferrata_lez_residue_experimental is not recorded there).

Files: `ids-v0.4.0.tsv`, `ids-v0.4.0-rc1.tsv`, `ids-base-fb27c1e2-rc1.tsv` (guest, image id, sha256 and bytes of the
binary, sha256 of the user ELF), as guest-ids.sh wrote them on the box.
