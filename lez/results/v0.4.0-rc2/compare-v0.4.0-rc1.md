# v0.4.0-rc2 against v0.4.0-rc1 (rerun), table by table

Written by `lez/scripts/compare-tables.py`: every cell of v0.4.0-rc2's tables that differs from
v0.4.0-rc1 (rerun)'s, rows paired in order.

| file | tables | rows | rows equal | rows that differ | text lines that differ |
|---|---:|---:|---:|---:|---:|
| `bridge-burn.md` | 4 | 39 | 39 | 0 | 2 |
| `bridge-inbound.md` | 2 | 90 | 90 | 0 | 1 |
| `bridge-private-claim.md` | 2 | 3 | 3 | 0 | 6 |
| `environment.md` | only in v0.4.0-rc2 | | | | |
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

## `images.md`

- program (`guests/program`) / `262cb95415b775b5ba5c3117c4653a8243bf5caee151131c1b46ff2b24b74883`: image id: ``59b68c3c8d9f5e7a3424124b891ecde4d1db92529c79b4114306674d589172ab`` → ``262cb95415b775b5ba5c3117c4653a8243bf5caee151131c1b46ff2b24b74883``
- consumer (`guests/consumer`) / `bfc974b5d0083c9544ffccd4ca19a881d1d462a7d718a8b184bf5572566e5b75`: image id: ``2b03539db9a7ce75bd302a7348161c58d70e72c908cbce6825e099ffb1c4f105`` → ``bfc974b5d0083c9544ffccd4ca19a881d1d462a7d718a8b184bf5572566e5b75``
- LEZ's `scripted` (`guests/scripted`) / `15591e6c407b329d2ef37f0eaea7c98ad9770dc87b5dabc41faee368c08aa355`: image id: ``4b597adb3f1a43db85829c411a987234cc84584d8bcd947f0774b852adf0da2b`` → ``15591e6c407b329d2ef37f0eaea7c98ad9770dc87b5dabc41faee368c08aa355``
- comparison only: the program with software Keccak (`guests/program-keccak-soft`) / `307e82726ac622a0a7be640573b26203626592a25b6f79d56c84d2f3230f2ac0`: image id: ``f27e8450407ebda4792103288a143b533e32bd2c345e6bcee917c9742b1a7a1c`` → ``307e82726ac622a0a7be640573b26203626592a25b6f79d56c84d2f3230f2ac0``
- comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) / `9af2e06a01c9a55c979af4f04cc8be8d64eb7dc1af8997b9375fb907dba89f5d`: image id: ``c3901b42ebf64b887862647b41a0aa6cb923881aa6b51b5c2ff89556ab4f3f35`` → ``9af2e06a01c9a55c979af4f04cc8be8d64eb7dc1af8997b9375fb907dba89f5d``
