# Randomised runs over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.6 and F.8)

Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`; checkpoint (rule B) `0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e`; release `0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9`. The vault on anvil (chain id 1), the bridge program in LEZ v0.4.0-rc1's state machine, the two guests executed in RISC Zero dev mode; the mock verifier accepts only the journals the guests committed. 279 s in all.

After every step: vault balance == escrowed + owed; zone supply == minted - burnt; supply <= escrowed; and supply + unminted deposits + unreleased burns == escrowed.

| seed | steps | deposits | imports | claims | claims capped (of them claimed later) | burns | releases | forgeries refused | unminted at the end | unreleased at the end | invariant checks |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 60 | 19 | 3 | 12 | 0 (0) | 6 | 1 | 15 | 7 | 5 | 62 |
| 2 | 60 | 21 | 5 | 8 | 0 (0) | 6 | 2 | 8 | 13 | 4 | 62 |
| 3 | 60 | 15 | 6 | 11 | 0 (0) | 6 | 3 | 11 | 4 | 3 | 62 |
| 4 | 60 | 22 | 6 | 10 | 0 (0) | 6 | 5 | 20 | 12 | 1 | 62 |
| 5 | 60 | 14 | 5 | 10 | 0 (0) | 6 | 2 | 14 | 4 | 4 | 62 |
| 6 | 60 | 15 | 5 | 7 | 0 (0) | 13 | 4 | 2 | 8 | 9 | 62 |
| 7 | 60 | 18 | 7 | 13 | 0 (0) | 10 | 4 | 10 | 5 | 6 | 62 |
| 8 | 60 | 17 | 7 | 10 | 0 (0) | 6 | 4 | 13 | 7 | 2 | 62 |

## Anonymity sets of the claims (F.8)

For each claim, the set a private claim of its deposit would have at that moment (`ferrata_wallet::claim::anonymity_set`, what `ferrata claim` shows for a private claim: the deposits of the same token and amount under the root and fee schedule range it names, less those a plaintext claim has published, applied or refused on chain: a refused transaction stays in its block, witness and all), by denomination over every run. Every claim of these runs is plaintext, for which the client shows 1 (it names its deposit), and each leaves every later set, as does a claim the bucket refused, whose deposit's own later claim shows 1: "as run". Beside it, the same claims had every earlier claim been private, the default, which publishes no deposit, refused or not: "all private". Median: the lower and the upper middle of the sorted sets (the same when the count is odd).

| denomination (ETH) | claims | as run: smallest | median | largest | sets of 1 | all private: smallest | median | largest | sets of 1 |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0.1 | 33 | 1 | 2 | 7 | 8 | 1 | 4 | 10 | 2 |
| 1 | 28 | 1 | 2 | 6 | 10 | 1 | 4 | 7 | 5 |
| 10 | 20 | 1 | 2 | 5 | 9 | 1 | 3 | 6 | 3 |

Sets of 1 (the claim's amount alone links it): 27 of 81 claims as run, 10 of 81 had every earlier claim been private; the plaintext claims made 57 of the 81 sets smaller.
