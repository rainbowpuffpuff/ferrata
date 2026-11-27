# Fee schedules over both real halves (`cli/tests/both_halves.rs`, RFP-022 F.13, F.8)

Schedule 1 announced at 1795793759, in force at 1796398560 (seven days and 1 s later), from leaf 3: mint [500000000000000, 5000000000000000, 50000000000000000], burn [200000000000000, 2000000000000000, 20000000000000000], checkpoint [100000000000000, 1000000000000000, 10000000000000000] wei for the denominations 0.1, 1 and 10 ETH.

## Claims (plaintext, 1 ETH each)

| leaf | schedule at deposit | relay fee (wei) | set before any claim | set at this claim |
|---:|---:|---:|---:|---:|
| 0 | 0 | 1000000000000000 | 6 | 6 |
| 1 | 0 | 1000000000000000 | 6 | 5 |
| 2 | 0 | 1000000000000000 | 6 | 4 |
| 3 | 1 | 1000000000000000 | 6 | 3 |
| 4 | 1 | 1000000000000000 | 6 | 2 |
| 5 | 1 | 1000000000000000 | 6 | 1 |

Every claim of 1 ETH mints the same fixed relay fee and no mint fee. Before any claim, a private claim of any deposit has the same anonymity set (6 and 6 on both sides of the change; 6 deposits of 1 ETH under the root): the fee change does not split the set at leaf 3. These claims are plaintext, so each publishes its deposit and the next claim has one fewer ("set at this claim"); private claims would leave it whole.

## Protocol fees collected on LEZ (ADR 0048)

After the claims, one `CollectFees` under the record of the claims' root minted 15000000000000000 wei, the vault's `lezFees` then (15000000000000000: the three schedule-1 deposits' mint fees), to the fee recipient on LEZ, in the wrapped token; the same collection again minted nothing (244). Nothing was credited on Ethereum at the deposits.

## Releases (1 ETH each, under schedule 1)

Each recipient received 997000000000000000 wei: 1 ETH less the burn fee 2000000000000000 and the checkpoint fee 1000000000000000; the checkpoint fee credited on the vault to the checkpoint's prover (the vault's `totalOwed` rose by 3000000000000000 wei over 3 releases), the burn fee kept in escrow and counted in `lezFees` (+6000000000000000 wei). A second `CollectFees`, under a record after the releases, minted 6000000000000000 wei (the burn fees) to the fee recipient; the earlier record's smaller total was then refused (247). `fees(cid, ETH)` counts 21000000000000000 wei collected. Every deposit claimed, every burn released, every fee collected: the wrapped supply on LEZ, 3027000000000000000 wei, is the vault's escrow, 3027000000000000000 wei.

Invariant checks: 14 (vault balance == escrowed + owed; supply == minted - burnt; supply + unminted + unreleased + uncollected fees == escrowed). Forgeries refused: 7. 25 s in all.
