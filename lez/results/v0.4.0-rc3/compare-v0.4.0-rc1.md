# v0.4.0-rc3 against v0.4.0-rc1, table by table

Written by `lez/scripts/compare-tables.py`: every cell of v0.4.0-rc3's tables that differs from
v0.4.0-rc1's, rows paired in order.

| file | tables | rows | rows equal | rows that differ | text lines that differ |
|---|---:|---:|---:|---:|---:|
| `bridge-burn.md` | 4 | 56 | 56 | 0 | 2 |
| `bridge-gloas.md` | 2 | 38 | 38 | 0 | 1 |
| `bridge-inbound.md` | 2 | 125 | 125 | 0 | 1 |
| `bridge-private-claim.md` | 2 | 3 | 3 | 0 | 6 |
| `bridge-roles.md` | 3 | 295 | 295 | 0 | 1 |
| `bridge-transfer-private.md` | 2 | 3 | 2 | 1 | 1 |
| `environment.md` | 0 | 0 | 0 | 0 | 2 |
| `holdings.md` | 2 | 38 | 38 | 0 | 0 |
| `image-ids-rc3-as-rc1.md` | only in v0.4.0-rc3 | | | | |
| `image-ids.md` | only in v0.4.0-rc3 | | | | |
| `images.md` | 1 | 5 | 0 | 5 | 0 |
| `shell-costs.md` | 2 | 20 | 20 | 0 | 0 |
| `stage1-finality.md` | 2 | 21 | 21 | 0 | 0 |
| `stage1-one-transaction.md` | 2 | 4 | 4 | 0 | 0 |
| `stage1-refusals.md` | 2 | 43 | 43 | 0 | 1 |
| `stage1-replay.md` | 2 | 73 | 73 | 0 | 1 |
| `stage4-attest-consumer.md` | 2 | 15 | 15 | 0 | 0 |
| `stage4-attest-keccak-soft.md` | 2 | 16 | 16 | 0 | 0 |
| `stage4-attest-private.md` | 4 | 8 | 8 | 0 | 0 |
| `stage4-attest-refusals.md` | 2 | 90 | 90 | 0 | 0 |
| `stage4-attest.md` | 2 | 16 | 16 | 0 | 0 |
| `stage5-attest-refusals.md` | 2 | 16 | 16 | 0 | 0 |
| `stage5-attest.md` | 2 | 8 | 8 | 0 | 0 |
| `stage5-consumer-private-kind.md` | 1 | 2 | 2 | 0 | 0 |
| `stage5-consumer-private.md` | 1 | 2 | 2 | 0 | 0 |
| `stage5-consumer-public.md` | 2 | 9 | 9 | 0 | 0 |
| `stage5-sepolia-replay.md` | 4 | 181 | 181 | 0 | 0 |
| `stage5-vault-synthetic.md` | 2 | 22 | 22 | 0 | 0 |
| `stage5-vault-v5.md` | 2 | 16 | 16 | 0 | 0 |

## `bridge-transfer-private.md`

- 8914f75d554d9a55301e88d198fd3aa72d8a042d8cdca4aa807fa7edb8706639 / alias cd0ea9aa: to: `0840c609b5de392eab03c82b5a8fb7d5719e34edaa363995a53441132d2f173f` → `8914f75d554d9a55301e88d198fd3aa72d8a042d8cdca4aa807fa7edb8706639`

## `images.md`

- program (`guests/program`) / `efd9aef79f65f57f9fc38c9bba69dc87058de2229de6d1be1e3de76935d23254`: image id: ``816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df`` → ``efd9aef79f65f57f9fc38c9bba69dc87058de2229de6d1be1e3de76935d23254``
- consumer (`guests/consumer`) / `9dc21f01d175ad6a384fb2e69732fa304aa2185bea6b826a5535fc3cfac71df3`: image id: ``8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96`` → ``9dc21f01d175ad6a384fb2e69732fa304aa2185bea6b826a5535fc3cfac71df3``
- LEZ's `scripted` (`guests/scripted`) / `3af31a8d2cfef9ecc9291221af63fe9fb7a11e161b7f255e56c038d60d2add48`: image id: ``8cb0e3f54dd11a2aea7551227c43e6416a6218e101cc7779c98f4f1a53d64e61`` → ``3af31a8d2cfef9ecc9291221af63fe9fb7a11e161b7f255e56c038d60d2add48``
- comparison only: the program with software Keccak (`guests/program-keccak-soft`) / `44f54150925cc1d4f3695ef0b185e1f90aefcfe0809a11e86a58ff67ef189f9d`: image id: ``efe9c58caa0eacd4c837815d7dbb3c003011b478f9cc5b85fca083b6f0a5cb3e`` → ``44f54150925cc1d4f3695ef0b185e1f90aefcfe0809a11e86a58ff67ef189f9d``
- comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) / `36b90b3ecd1918f4c55c06c7b9d3fe413df9792c5b358013c5c060582e4d957f`: image id: ``fa9bef9049151bc92ef32de84e0fdc1153447f115b090b8b6414330ce9d243cc`` → ``36b90b3ecd1918f4c55c06c7b9d3fe413df9792c5b358013c5c060582e4d957f``
