# Randomised runs over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.6 and F.8)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; checkpoint (rule B) `0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e`; release `0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9`. The vault on anvil (chain id 1), the bridge program in LEZ v0.4.0-rc1's state machine, the two guests executed in RISC Zero dev mode; the mock verifier accepts only the journals the guests committed. 302 s in all.

After every step: vault balance == escrowed + owed; zone supply == minted - burnt; supply <= escrowed; and supply + unminted deposits + unreleased burns == escrowed.

| seed | steps | deposits | imports | claims | claims capped (claimed later) | burns | releases | forgeries refused | unminted at the end | unreleased at the end | invariant checks |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 60 | 19 | 3 | 12 | 0 | 6 | 1 | 15 | 7 | 5 | 62 |
| 2 | 60 | 21 | 5 | 8 | 0 | 6 | 2 | 8 | 13 | 4 | 62 |
| 3 | 60 | 15 | 6 | 11 | 0 | 6 | 3 | 11 | 4 | 3 | 62 |
| 4 | 60 | 22 | 6 | 10 | 0 | 6 | 5 | 20 | 12 | 1 | 62 |
| 5 | 60 | 14 | 5 | 10 | 0 | 6 | 2 | 14 | 4 | 4 | 62 |
| 6 | 60 | 15 | 5 | 7 | 0 | 13 | 4 | 2 | 8 | 9 | 62 |
| 7 | 60 | 18 | 7 | 13 | 0 | 10 | 4 | 10 | 5 | 6 | 62 |
| 8 | 60 | 17 | 7 | 10 | 0 | 6 | 4 | 13 | 7 | 2 | 62 |

## Anonymity sets of the claims (F.8)

The set the client shows for each claim (`ferrata_wallet::claim::anonymity_set`: the deposits of the same token and amount under the root and fee schedule range it names), by denomination over every run.

| denomination (ETH) | claims | smallest | median | largest |
|---:|---:|---:|---:|---:|
| 0.1 | 33 | 1 | 4 | 10 |
| 1 | 28 | 1 | 4 | 7 |
| 10 | 20 | 1 | 3 | 6 |
