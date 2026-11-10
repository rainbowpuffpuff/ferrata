# Sepolia into Gloas: a pre-Gloas bootstrap, every period's update through the fork, a Gloas finality update

Fetched by `scripts/fetch-sepolia-gloas.py` between 2026-11-10T08:34:58Z and 2026-11-10T08:35:07Z (UTC). Every request
went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`. Responses are stored
byte for byte, except the `updates` stream, which is split into one file per period with its
8-byte length and 4-byte fork digest removed (the digests are listed below); `header.rlp` is
rebuilt from `block.json` by the script, which checked that it hashes to the bound block hash.
`SHA256SUMS` pins every file.

**Why the bootstrap is not under Gloas.** Of the public Sepolia beacon APIs tried, only Lodestar
(lodestar-sepolia.chainsafe.io) serves light-client data, and it serves no bootstrap for a
Gloas-era checkpoint: HTTP 500 "syncCommitteeWitness not available" (recorded below for this
period's first checkpoint; the same on 2026-11-10 for the first checkpoints of periods 1379, 1380,
1390, 1400 and 1408 and for the finalized checkpoint of epoch 360,844). Lighthouse
(ethereum-sepolia-beacon-api.publicnode.com) answers 404 to every light-client request.
Lodestar serves the bootstrap of the last pre-Gloas period (1378) and every period's
update since, so the fixture starts there and follows the chain through the fork.

| | |
|---|---|
| chain | Sepolia: chain id 11155111, `genesis_validators_root` 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078; Gloas since epoch 353,024 (version 0x90000076), the first slot of period 1379 |
| bootstrap (Fulu, Electra layout) | slot 11288576, block root 0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d |
| finality update (Gloas) | attested slot 11547168, signature slot 11547169, 477/512 signers |
| finalized beacon block | slot 11547104, root 0xface3c3725d1760873c4e115e74446e7e5c0164a71da7228bbb66b92bc8ed6ef |
| bound execution block (the bid's parent) | 12103422, hash 0xcd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435 |
| its state root / receipts root | 0x716fd4b341e22c5b73f4e5618d27e017beb75d3d2907ed7d3e5ce1fa6cbb790d / 0x9173c437032c431f47fcdf2557205c9dece7a48ec68847363f254875801cbcfe |

| period | layout | fork digest | attested slot | signature slot | finalized slot | signers |
|---|---|---|---|---|---|---|
| 1378 | electra | 0x74d01459 | 11290176 | 11290178 | 11290112 | 512/512 |
| 1379 | gloas | 0x669e6c11 | 11297248 | 11297249 | 11297184 | 512/512 |
| 1380 | gloas | 0x669e6c11 | 11305024 | 11305025 | 11304960 | 512/512 |
| 1381 | gloas | 0x669e6c11 | 11313395 | 11313396 | 11313312 | 512/512 |
| 1382 | gloas | 0x669e6c11 | 11321589 | 11321590 | 11321504 | 512/512 |
| 1383 | gloas | 0x669e6c11 | 11329600 | 11329601 | 11329536 | 512/512 |
| 1384 | gloas | 0x669e6c11 | 11337973 | 11337974 | 11337888 | 512/512 |
| 1385 | gloas | 0x669e6c11 | 11346000 | 11346001 | 11345920 | 512/512 |
| 1386 | gloas | 0x669e6c11 | 11354176 | 11354177 | 11354112 | 511/512 |
| 1387 | gloas | 0x669e6c11 | 11362513 | 11362514 | 11362432 | 512/512 |
| 1388 | gloas | 0x669e6c11 | 11370571 | 11370572 | 11370496 | 512/512 |
| 1389 | gloas | 0x669e6c11 | 11381395 | 11381396 | 11381312 | 512/512 |
| 1390 | gloas | 0x669e6c11 | 11386944 | 11386945 | 11386880 | 512/512 |
| 1391 | gloas | 0x669e6c11 | 11395136 | 11395137 | 11395072 | 511/512 |
| 1392 | gloas | 0x669e6c11 | 11403328 | 11403329 | 11403264 | 512/512 |
| 1393 | gloas | 0x669e6c11 | 11411521 | 11411522 | 11411456 | 511/512 |
| 1394 | gloas | 0x669e6c11 | 11419712 | 11419713 | 11419648 | 512/512 |
| 1395 | gloas | 0x669e6c11 | 11427904 | 11427905 | 11427840 | 507/512 |
| 1396 | gloas | 0x669e6c11 | 11438760 | 11438761 | 11438688 | 511/512 |
| 1397 | gloas | 0x669e6c11 | 11449809 | 11449810 | 11449728 | 512/512 |
| 1398 | gloas | 0x669e6c11 | 11452480 | 11452481 | 11452416 | 512/512 |
| 1399 | gloas | 0x669e6c11 | 11460672 | 11460673 | 11460608 | 512/512 |
| 1400 | gloas | 0x669e6c11 | 11468864 | 11468865 | 11468800 | 512/512 |
| 1401 | gloas | 0x669e6c11 | 11477056 | 11477057 | 11476992 | 512/512 |
| 1402 | gloas | 0x669e6c11 | 11485248 | 11485249 | 11485184 | 512/512 |
| 1403 | gloas | 0x669e6c11 | 11493440 | 11493441 | 11493376 | 506/512 |
| 1404 | gloas | 0x669e6c11 | 11501664 | 11501665 | 11501600 | 510/512 |
| 1405 | gloas | 0x669e6c11 | 11510387 | 11510388 | 11510304 | 506/512 |
| 1406 | gloas | 0x669e6c11 | 11518016 | 11518017 | 11517952 | 511/512 |
| 1407 | gloas | 0x669e6c11 | 11526208 | 11526209 | 11526144 | 506/512 |
| 1408 | gloas | 0x669e6c11 | 11537827 | 11537828 | 11537760 | 507/512 |
| 1409 | gloas | 0x669e6c11 | 11542593 | 11542594 | 11542528 | 508/512 |

Cross-checks made by the script: the RLP header rebuilt from `block.json` hashes to the bound hash; a
second execution provider (`block_second.json`) reports the same block; a second beacon provider
reports the root of the bootstrap block and of every finalized header named here
(`beacon_header_*_second.json`), and the finalized block, whose bid names the bound hash as
`parent_block_hash` (`beacon_block_finalized_second.json`).

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-10T08:34:58Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-10T08:34:58Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0xcd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435", false]` | block.json |  |
| 2026-11-10T08:34:59Z | https://1rpc.io/sepolia | `eth_getBlockByHash ["0xcd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435", false]` | block_second.json |  |
| 2026-11-10T08:34:59Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v2/beacon/blocks/0xface3c3725d1760873c4e115e74446e7e5c0164a71da7228bbb66b92bc8ed6ef` | beacon_block_finalized_second.json |  |
| 2026-11-10T08:34:59Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11542528` |  |  |
| 2026-11-10T08:35:00Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0xdd13fc9fdddd639ce9aa6c8e16316eb718a1d8046047f69ffc3c0ceac0dde419` |  | HTTP 500: {"code":500,"message":"syncCommitteeWitness not available 0xdd13fc9fdddd639ce9aa6c8e16316eb718a1d8046047f69ffc3c0ceac0dd |
| 2026-11-10T08:35:00Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11288576` |  |  |
| 2026-11-10T08:35:00Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d` | bootstrap.ssz | fulu |
| 2026-11-10T08:35:00Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/updates?start_period=1378&count=32` |  | fulu,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas,gloas |
| 2026-11-10T08:35:00Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11288576` | beacon_header_11288576_second.json |  |
| 2026-11-10T08:35:00Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11290112` | beacon_header_11290112_second.json |  |
| 2026-11-10T08:35:00Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11297184` | beacon_header_11297184_second.json |  |
| 2026-11-10T08:35:01Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11304960` | beacon_header_11304960_second.json |  |
| 2026-11-10T08:35:01Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11313312` | beacon_header_11313312_second.json |  |
| 2026-11-10T08:35:01Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11321504` | beacon_header_11321504_second.json |  |
| 2026-11-10T08:35:02Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11329536` | beacon_header_11329536_second.json |  |
| 2026-11-10T08:35:02Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11337888` | beacon_header_11337888_second.json |  |
| 2026-11-10T08:35:02Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11345920` | beacon_header_11345920_second.json |  |
| 2026-11-10T08:35:02Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11354112` | beacon_header_11354112_second.json |  |
| 2026-11-10T08:35:02Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11362432` | beacon_header_11362432_second.json |  |
| 2026-11-10T08:35:03Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11370496` | beacon_header_11370496_second.json |  |
| 2026-11-10T08:35:04Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11381312` | beacon_header_11381312_second.json |  |
| 2026-11-10T08:35:04Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11386880` | beacon_header_11386880_second.json |  |
| 2026-11-10T08:35:04Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11395072` | beacon_header_11395072_second.json |  |
| 2026-11-10T08:35:04Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11403264` | beacon_header_11403264_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11411456` | beacon_header_11411456_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11419648` | beacon_header_11419648_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11427840` | beacon_header_11427840_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11438688` | beacon_header_11438688_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11449728` | beacon_header_11449728_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11452416` | beacon_header_11452416_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11460608` | beacon_header_11460608_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11468800` | beacon_header_11468800_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11476992` | beacon_header_11476992_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11485184` | beacon_header_11485184_second.json |  |
| 2026-11-10T08:35:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11493376` | beacon_header_11493376_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11501600` | beacon_header_11501600_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11510304` | beacon_header_11510304_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11517952` | beacon_header_11517952_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11526144` | beacon_header_11526144_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11537760` | beacon_header_11537760_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11542528` | beacon_header_11542528_second.json |  |
| 2026-11-10T08:35:07Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11547104` | beacon_header_11547104_second.json |  |
