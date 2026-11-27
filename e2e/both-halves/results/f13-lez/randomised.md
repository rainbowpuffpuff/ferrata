# Randomised runs over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.6 and F.8)

Bridge program image `9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321`; checkpoint (rule B) `0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0`; release `0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd`. The vault on anvil (chain id 1), the bridge program in LEZ v0.4.0-rc1's state machine, the two guests executed in RISC Zero dev mode; the mock verifier accepts only the journals the guests committed. 58 s in all.

After every step: vault balance == escrowed + owed; zone supply == minted - burnt; supply <= escrowed; and supply + unminted deposits + unreleased burns + fees escrowed for LEZ not yet collected (none here: every deposit and release is under schedule 0) == escrowed.

| seed | steps | deposits | imports | claims | claims capped (of them claimed later) | burns | releases | forgeries refused | unminted at the end | unreleased at the end | invariant checks |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 40 | 16 | 2 | 6 | 0 (0) | 1 | 0 | 8 | 10 | 1 | 42 |
| 2 | 40 | 15 | 2 | 3 | 0 (0) | 3 | 0 | 0 | 12 | 3 | 42 |
| 3 | 40 | 10 | 4 | 5 | 0 (0) | 5 | 1 | 4 | 5 | 4 | 42 |

## Anonymity sets of the claims (F.8)

For each claim, the set a private claim of its deposit would have at that moment (`ferrata_wallet::claim::anonymity_set`, what `ferrata claim` shows for a private claim: the deposits of the same token and amount under the root, on either side of a fee change, less those a plaintext claim has published, applied or refused on chain: a refused transaction stays in its block, witness and all), by denomination over every run. Every claim of these runs is plaintext, for which the client shows 1 (it names its deposit), and each leaves every later set, as does a claim the bucket refused, whose deposit's own later claim shows 1: "as run". Beside it, the same claims had every earlier claim been private, the default, which publishes no deposit, refused or not: "all private". Median: the lower and the upper middle of the sorted sets (the same when the count is odd).

| denomination (ETH) | claims | as run: smallest | median | largest | sets of 1 | all private: smallest | median | largest | sets of 1 |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0.1 | 5 | 2 | 3 | 5 | 0 | 2 | 3 | 7 | 0 |
| 1 | 8 | 1 | 3, 4 | 6 | 1 | 1 | 4 | 6 | 1 |
| 10 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |

Sets of 1 (the claim's amount alone links it): 2 of 14 claims as run, 2 of 14 had every earlier claim been private; the plaintext claims made 8 of the 14 sets smaller.
