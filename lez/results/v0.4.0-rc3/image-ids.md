# The lez/ guests built against LEZ v0.4.0-rc1 and v0.4.0-rc3

Both builds on the same box, from the same tree, through `scripts/guest-rustc` (paths remapped: the LEZ sources appear in a guest as `/upstream/lez-<tag>/...`). "final": the id recorded in `docs/image-ids.md`. "rewritten": the v0.4.0-rc3 build with every `/upstream/lez-v0.4.0-rc3/` replaced by `/upstream/lez-v0.4.0-rc1/` (the same length), then compared with the v0.4.0-rc1 build; its image id by `r0vm --elf <R0BF> --id`. Made by `lez/scripts/compare-guest-builds.py`.

| guest | final id | v0.4.0-rc1 build | v0.4.0-rc3 build | user ELF bytes (v0.4.0-rc1 / v0.4.0-rc3 build) | bytes that differ | LEZ paths embedded (v0.4.0-rc1 / v0.4.0-rc3 build) | rewritten user ELF = v0.4.0-rc1's | rewritten image id |
|---|---|---|---|---|---:|---|---|---|
| `ferrata_bridge` | `0xccde6171…` | `0xccde6171…` = final | `0x8f8efd14…` | 560,416 / 560,416 | 246,463 | 2 / 2 | NO | `0x0c760e7d…` |
| `ferrata_consumer` | `0x8464603b…` | `0x8464603b…` = final | `0x9dc21f01…` | 438,984 / 438,984 | 122,830 | 2 / 2 | NO | `0x49cd7e19…` |
| `ferrata_consumer_keccak_accel` | `0xfa9bef90…` | `0xfa9bef90…` = final | `0x36b90b3e…` | 435,984 / 435,984 | 174,512 | 2 / 2 | NO | `0xb6ee0473…` |
| `ferrata_holdings` | `0xa79e3f5c…` | `0xa79e3f5c…` = final | `0xca94902d…` | 275,080 / 275,080 | 80,072 | 2 / 2 | NO | `0x7ff68a51…` |
| `ferrata_lez` | `0x816d1fef…` | `0x816d1fef…` = final | `0xefd9aef7…` | 594,640 / 594,640 | 75,251 | 2 / 2 | NO | `0xedf328a8…` |
| `ferrata_lez_keccak_soft` | `0xefe9c58c…` | `0xefe9c58c…` = final | `0x44f54150…` | 597,868 / 597,868 | 216,921 | 2 / 2 | NO | `0x4f3baa50…` |
| `scripted` | `0x8cb0e3f5…` | `0x8cb0e3f5…` = final | `0x3af31a8d…` | 235,804 / 235,804 | 50,248 | 3 / 3 | NO | `0x26e97e0d…` |

0 of 7 guests: the v0.4.0-rc1 build has the final id, and the v0.4.0-rc3 build differs from it only in the tag inside the embedded LEZ source paths (rewritten, it is byte for byte the v0.4.0-rc1 build and has its image id).
