# The lez/ guests built against LEZ v0.4.0-rc1 and v0.4.0-rc1

Both builds on the same box, from the same tree, through `scripts/guest-rustc` (paths remapped: the LEZ sources appear in a guest as `/upstream/lez-<tag>/...`). "final": the id recorded in `docs/image-ids.md`. "rewritten": the v0.4.0-rc1 build with every `/upstream/lez-v0.4.0-rc1/` replaced by `/upstream/lez-v0.4.0-rc1/` (the same length), then compared with the v0.4.0-rc1 build; its image id by `r0vm --elf <R0BF> --id`. Made by `lez/scripts/compare-guest-builds.py`.

| guest | final id | v0.4.0-rc1 build | v0.4.0-rc3 build under rc1's name | user ELF bytes (v0.4.0-rc1 / v0.4.0-rc3 build under rc1's name) | bytes that differ | LEZ paths embedded (v0.4.0-rc1 / v0.4.0-rc3 build under rc1's name) | rewritten user ELF = v0.4.0-rc1's | rewritten image id |
|---|---|---|---|---|---:|---|---|---|
| `ferrata_bridge` | `0xccde6171…` | `0xccde6171…` = final | `0xe011f5c9…` | 560,416 / 560,416 | 707 | 2 / 2 | NO | `0xe011f5c9…` |
| `ferrata_consumer` | `0x8464603b…` | `0x8464603b…` = final | `0x1ea987c2…` | 438,984 / 438,984 | 707 | 2 / 2 | NO | `0x1ea987c2…` |
| `ferrata_consumer_keccak_accel` | `0xfa9bef90…` | `0xfa9bef90…` = final | `0x81354923…` | 435,984 / 435,984 | 707 | 2 / 2 | NO | `0x81354923…` |
| `ferrata_holdings` | `0xa79e3f5c…` | `0xa79e3f5c…` = final | `0x35b96448…` | 275,080 / 275,080 | 707 | 2 / 2 | NO | `0x35b96448…` |
| `ferrata_lez` | `0x816d1fef…` | `0x816d1fef…` = final | `0x43c8ecaf…` | 594,640 / 594,640 | 764 | 2 / 2 | NO | `0x43c8ecaf…` |
| `ferrata_lez_keccak_soft` | `0xefe9c58c…` | `0xefe9c58c…` = final | `0x5da2a88b…` | 597,868 / 597,868 | 764 | 2 / 2 | NO | `0x5da2a88b…` |
| `scripted` | `0x8cb0e3f5…` | `0x8cb0e3f5…` = final | `0x7e5096d7…` | 235,804 / 235,804 | 797 | 3 / 3 | NO | `0x7e5096d7…` |

0 of 7 guests: the v0.4.0-rc1 build has the final id, and the v0.4.0-rc3 build under rc1's name differs from it only in the tag inside the embedded LEZ source paths (rewritten, it is byte for byte the v0.4.0-rc1 build and has its image id).
