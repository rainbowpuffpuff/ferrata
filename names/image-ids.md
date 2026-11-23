# Image ids of the names program (`ferrata_names`)

In the style of [`docs/image-ids.md`](../docs/image-ids.md): the id is risc0-build's own (`FERRATA_NAMES_ID` in
`lez/methods-names`' `methods.rs`) and equals `r0vm --elf <binary> --id` (column "r0vm": `= id`); sha256 and bytes
are of the binary risc0-build embeds (the R0BF program, the user ELF with its kernel); the user ELF's sha256 beside
it. Built on the Modal build box through `scripts/guest-rustc` like every guest of this tree (RISC Zero 3.0.5, guest
Rust 1.91.1), read by `scripts/reproduce/guest_ids.py`. The rc1 and rc3 builds of one source differ only by the LEZ tag
`lee_core` is taken from.

## v2: the build to deploy (the review's fixes, 23 November 2026)

Source: `lez/names-wire` + `lez/guests/names` as in this tree. Raw rows:
[`e2e/results/image-ids.tsv`](e2e/results/image-ids.tsv) (`names/e2e/box.sh build && names/e2e/box.sh ids`). **Not
deployed on any public network yet**: deploying it is a new deployment at a new address (the steps in
[`README.md`](README.md) "Deploying it").

| Build | Image id | sha256 of the binary | Bytes | sha256 of the user ELF | r0vm |
|---|---|---|---:|---|---|
| LEZ v0.4.0-rc1 (the harness default) | `0x8417e74bb939bf373abe69e1f34807cc7e64f5d7df4880766ab8a5c0a028255b` | `ac9f386fce3f39a1f62c22f86ad18eaa5b3f88f3a5de0715be4964fb09ace752` | 306,404 | `26f2cc4b718f27ca412f71e6bb615fd2ca1fdba213fa986ed499182127cfdc38` | = id |
| **LEZ v0.4.0-rc3** (the test network; deployed in `e2e/results/local-rc3-v2`) | `0x28d63f2075ffb92d653149f9da00b20c85ab77a365f31263c6483387cd43be9d` | `d6f7c2497a5aa671f4fa6b53ddc51cc9c80310ee6c2d67d55385bd089e52c293` | 306,404 | `34bc4c0980d8e0d8aae4411e7cf9928d69d463cf46dd8260d60932f67ce15289` | = id |

## v1: live on the public test network (immutable)

The build the review read on the public network on 23 November 2026 (`/programs.json`; the header `4EBU635H…`
(`getAccount`) shows this image id and immutable), from the commit-reveal source before the review's fixes (only
`lez/names-wire/src/lib.rs` differs from v2's; that version is not in this repository's history). It stays
live: it cannot be changed or retired, and its names stay on it. v2 does not read its records.

| Build | Image id | sha256 of the binary | Bytes | sha256 of the user ELF | r0vm |
|---|---|---|---:|---|---|
| LEZ v0.4.0-rc1 (the harness default) | `0xe4b7d67291668c6309449e3b1d9627a91fdc5916eaa8344c949f3657ea54a71b` | `f232f4b0520b4038abaf4990eafb592ec53ffaaec3e5d93dd5a71c65ecf936c1` | 287,672 | `6e0c91ea65fee6fbef135977fbfb0837210640dec073377680a4da1e54746ec7` | = id |
| **LEZ v0.4.0-rc3** (live) | `0xee9578cab413451e1f0a0cf267a7874ca3231ed0a30907f5d793648ac4e6f790` | `5653a2ee1c3a85f3b29f32a1a227030a2619c096d0dd7c6c92b872e9e7ce75ab` | 287,672 | `eacdfd305520939ba4f092371159a484ec07a1133b5c7e43b764b9a03e9844e5` | = id |

Deploying a program gives it an account id of the deployer's choosing (the loader's header account); the image id is
what that header's `image_id` must show.
