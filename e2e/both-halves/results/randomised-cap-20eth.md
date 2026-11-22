# Randomised runs over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.6 and F.8)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; checkpoint (rule B) `0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e`; release `0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9`. The vault on anvil (chain id 1), the bridge program in LEZ v0.4.0-rc1's state machine, the two guests executed in RISC Zero dev mode; the mock verifier accepts only the journals the guests committed. 302 s in all.

After every step: vault balance == escrowed + owed; zone supply == minted - burnt; supply <= escrowed; and supply + unminted deposits + unreleased burns == escrowed.

ETH's mint bucket set to 20 ETH after pairing (`SetCap` by the admin; refilled at 0.01 ETH per second of attested time), so that claims meet a spent bucket.

| seed | steps | deposits | imports | claims | claims capped (of them claimed later) | burns | releases | forgeries refused | unminted at the end | unreleased at the end | invariant checks |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 60 | 19 | 3 | 12 | 0 (0) | 6 | 1 | 15 | 7 | 5 | 62 |
| 2 | 60 | 21 | 5 | 8 | 0 (0) | 6 | 2 | 8 | 13 | 4 | 62 |
| 3 | 60 | 15 | 6 | 10 | 1 (0) | 6 | 3 | 10 | 5 | 3 | 62 |
| 4 | 60 | 21 | 8 | 4 | 8 (4) | 5 | 4 | 17 | 17 | 1 | 62 |
| 5 | 60 | 15 | 3 | 6 | 8 (6) | 4 | 2 | 11 | 9 | 2 | 62 |
| 6 | 60 | 16 | 5 | 4 | 9 (7) | 12 | 3 | 5 | 12 | 9 | 62 |
| 7 | 60 | 18 | 7 | 9 | 4 (1) | 10 | 4 | 8 | 9 | 6 | 62 |
| 8 | 60 | 17 | 7 | 10 | 0 (0) | 6 | 4 | 13 | 7 | 2 | 62 |

## Anonymity sets of the claims (F.8)

For each claim, the set a private claim of its deposit would have at that moment (`ferrata_wallet::claim::anonymity_set`, what `ferrata claim` shows for a private claim: the deposits of the same token and amount under the root and fee schedule range it names, less those a plaintext claim has published, applied or refused on chain: a refused transaction stays in its block, witness and all), by denomination over every run. Every claim of these runs is plaintext, for which the client shows 1 (it names its deposit), and each leaves every later set, as does a claim the bucket refused, whose deposit's own later claim shows 1: "as run". Beside it, the same claims had every earlier claim been private, the default, which publishes no deposit, refused or not: "all private". Median: the lower and the upper middle of the sorted sets (the same when the count is odd).

| denomination (ETH) | claims | as run: smallest | median | largest | sets of 1 | all private: smallest | median | largest | sets of 1 |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0.1 | 33 | 1 | 3 | 7 | 9 | 1 | 4 | 10 | 3 |
| 1 | 22 | 1 | 2 | 6 | 8 | 1 | 4 | 7 | 5 |
| 10 | 8 | 1 | 2 | 5 | 3 | 1 | 2 | 5 | 3 |

Sets of 1 (the claim's amount alone links it): 20 of 63 claims as run, 11 of 63 had every earlier claim been private; the plaintext claims made 39 of the 63 sets smaller.
