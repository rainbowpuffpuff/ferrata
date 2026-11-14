# A private holder's Transfer (`harness/tests/bridge_roles.rs`, dev mode)

Bridge program image `8f8efd14b5e236f16ba3d7776bf501c4bf9c5585247e35e627e8c2caeaf5f395`. The holder of the private claim of A's leaf 5 (1 ETH) moves 0.1 ETH to a public holder's `holder(account)`; settled through LEZ's private settlement (dev-mode receipt): 7696 transaction bytes, execution gas 463434, storage gas 224063.

## The private transition

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Transfer at the private holder (debits, sends Transfer) | 249 | 521 | 19,113 | 1 | 131,072 | 0 |

## Private to public deliveries (what the transaction shows)

| to | from | message |
|---|---|---:|
| 8914f75d554d9a55301e88d198fd3aa72d8a042d8cdca4aa807fa7edb8706639 | alias cd0ea9aa | 101 bytes |
| 7224eab6e02f8b37d841b163beb8b01470a42e73875ecccfcd72182be1b13b5c | alias cd0ea9aa | 17 bytes |

The private account's id does not occur in the transaction's bytes; its remaining 0.9 ETH is found only in its new commitment.
