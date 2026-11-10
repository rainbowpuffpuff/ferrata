# rc1 rerun against rc1 committed, table by table

Written by `lez/scripts/compare-tables.py`: every cell of rc1 rerun's tables that differs from
rc1 committed's, rows paired in order.

| file | tables | rows | rows equal | rows that differ | text lines that differ |
|---|---:|---:|---:|---:|---:|
| `bridge-burn.md` | 4 | 39 | 39 | 0 | 2 |
| `bridge-inbound.md` | 2 | 90 | 90 | 0 | 1 |
| `bridge-private-claim.md` | 2 | 3 | 3 | 0 | 6 |
| `images.md` | 1 | 5 | 1 | 4 | 0 |
| `shell-costs.md` | 2 | 20 | 20 | 0 | 0 |
| `stage1-finality.md` | 2 | 21 | 21 | 0 | 0 |
| `stage1-one-transaction.md` | 2 | 4 | 4 | 0 | 0 |
| `stage1-refusals.md` | 2 | 43 | 43 | 0 | 0 |
| `stage1-replay.md` | 2 | 73 | 73 | 0 | 0 |
| `stage4-attest-consumer.md` | 2 | 15 | 15 | 0 | 0 |
| `stage4-attest-keccak-soft.md` | 2 | 16 | 16 | 0 | 0 |
| `stage4-attest-private.md` | 4 | 8 | 8 | 0 | 0 |
| `stage4-attest-refusals.md` | 2 | 90 | 90 | 0 | 0 |
| `stage4-attest.md` | 2 | 16 | 16 | 0 | 0 |
| `environment.md` | only in rc1 committed | | | | |

## `images.md`

- program (`guests/program`) / `59b68c3c8d9f5e7a3424124b891ecde4d1db92529c79b4114306674d589172ab`: image id: ``2f3566d4f460620867a93b557776400b1039142ce92203072170c162ba2ec130`` → ``59b68c3c8d9f5e7a3424124b891ecde4d1db92529c79b4114306674d589172ab``; ELF bytes: `592752` → `592728` (-24)
- consumer (`guests/consumer`) / `2b03539db9a7ce75bd302a7348161c58d70e72c908cbce6825e099ffb1c4f105`: image id: ``711584ffe09f0b115132e33ce1485666710bc73e696e66fcc54d00aad0db949f`` → ``2b03539db9a7ce75bd302a7348161c58d70e72c908cbce6825e099ffb1c4f105``; ELF bytes: `411108` → `411092` (-16)
- comparison only: the program with software Keccak (`guests/program-keccak-soft`) / `f27e8450407ebda4792103288a143b533e32bd2c345e6bcee917c9742b1a7a1c`: image id: ``04465c59781d63382c330465dcf9b242f855546c374555eacb994cc23c018716`` → ``f27e8450407ebda4792103288a143b533e32bd2c345e6bcee917c9742b1a7a1c``; ELF bytes: `596032` → `596008` (-24)
- comparison only: the consumer with accelerated Keccak (`guests/consumer-keccak-accel`) / `c3901b42ebf64b887862647b41a0aa6cb923881aa6b51b5c2ff89556ab4f3f35`: image id: ``040c0b25b30af7abeae5d2c6cfd1df746149db9eb2324832cae8d2b264530b80`` → ``c3901b42ebf64b887862647b41a0aa6cb923881aa6b51b5c2ff89556ab4f3f35``; ELF bytes: `408024` → `408008` (-16)
