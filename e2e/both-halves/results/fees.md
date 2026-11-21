# Fee schedules over both real halves (`cli/tests/both_halves.rs`, RFP-022 F.13, F.8)

Schedule 1 announced at 1795280560, in force at 1795885361 (seven days and 1 s later), from leaf 3: mint [500000000000000, 5000000000000000, 50000000000000000], burn [200000000000000, 2000000000000000, 20000000000000000], checkpoint [100000000000000, 1000000000000000, 10000000000000000] wei for the denominations 0.1, 1 and 10 ETH.

## Claims (plaintext, 1 ETH each)

| leaf | schedule | mint fee (wei, to the fee recipient on LEZ) | relay fee (wei) | claim's anonymity set |
|---:|---:|---:|---:|---:|
| 0 | 0 | 0 | 1000000000000000 | 3 |
| 1 | 0 | 0 | 1000000000000000 | 3 |
| 2 | 0 | 0 | 1000000000000000 | 3 |
| 3 | 1 | 5000000000000000 | 1000000000000000 | 3 |
| 4 | 1 | 5000000000000000 | 1000000000000000 | 3 |
| 5 | 1 | 5000000000000000 | 1000000000000000 | 3 |

Within a schedule every claim of 1 ETH mints the same fees and has the same anonymity set (3 and 3 with the change; 6 deposits of 1 ETH under the root): the fee change splits the set at leaf 3.

## Releases (1 ETH each, under schedule 1)

Each recipient received 997000000000000000 wei: 1 ETH less the burn fee 2000000000000000 and the checkpoint fee 1000000000000000, credited on the vault to the fee recipient and to the checkpoint's prover (the vault's `totalOwed` rose by 9000000000000000 wei over 3 releases).

Invariant checks: 12 (vault balance == escrowed + owed; supply == minted - burnt; supply + unminted + unreleased == escrowed). Forgeries refused: 7. 20 s in all.
