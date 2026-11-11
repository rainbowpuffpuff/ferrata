# A private holder's Transfer (`harness/tests/bridge_roles.rs`, dev mode)

Bridge program image `43615dc48098b3731990de7700204d2a1bea0e9f963ee7c4dd629048dd5ef19c`. The holder of the private claim of A's leaf 5 (1 ETH) moves 0.1 ETH to a public holder's `holder(account)`; settled through LEZ's private settlement (dev-mode receipt): 7696 transaction bytes, execution gas 463434, storage gas 224063.

## The private transition

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Transfer at the private holder (debits, sends Transfer) | 249 | 521 | 19,113 | 1 | 131,072 | 0 |

## Private to public deliveries (what the transaction shows)

| to | from | message |
|---|---|---:|
| 221a55a9b63114d37ca9fa005bedc6afc78d552de4c12c03531e2259e4c27626 | alias cd0ea9aa | 101 bytes |
| 7224eab6e02f8b37d841b163beb8b01470a42e73875ecccfcd72182be1b13b5c | alias cd0ea9aa | 17 bytes |

The private account's id does not occur in the transaction's bytes; its remaining 0.9 ETH is found only in its new commitment.
