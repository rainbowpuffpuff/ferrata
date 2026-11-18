# A private holder's Transfer (`harness/tests/bridge_roles.rs`, dev mode)

Bridge program image `c5cad2cd470807a1799613a1f5f699f9d4f4074821a9fb26389ca917a4d0d6da`. The holder of the private claim of A's leaf 5 (1 ETH) moves 0.1 ETH to a public holder's `holder(account)`; settled through LEZ's private settlement (dev-mode receipt): 7696 transaction bytes, execution gas 463648, storage gas 224063.

## The private transition

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Transfer at the private holder (debits, sends Transfer) | 249 | 521 | 19,114 | 1 | 131,072 | 0 |

## Private to public deliveries (what the transaction shows)

| to | from | message |
|---|---|---:|
| 9006abe4cd8ef1b789df66e3543177fe13b323e52010ab47648a3ecd0937c571 | alias cd0ea9aa | 101 bytes |
| 7224eab6e02f8b37d841b163beb8b01470a42e73875ecccfcd72182be1b13b5c | alias cd0ea9aa | 17 bytes |

The private account's id does not occur in the transaction's bytes; its remaining 0.9 ETH is found only in its new commitment.
