# The private claim's hops inside the user's proof (RFP-021 P.4)

Written by `lez/harness/tests/vault_claim_hops.rs`. The reference consumer's private claim (`ClaimDepositPrivately`) of the vault fixture's deposit 0 (Sepolia block 12111477) under the record of block 12112773 (finalized slot 11556608), the deposit's block 1296 blocks back reached through a chain of `k` EIP-2935 hops; the consumer's two private transitions in the executor (consumer image `8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96`). LEZ's privacy-preserving circuit over them, most of a private transaction's proving time, does not run in dev mode: the proofs are in `lez/results/p4-hops/`.

| hops | root call (sends `Read`), user cycles | the record's reply (verifies, sends `Spend`), user cycles | both | over 1 hop | segments (both) | input bytes (reply) |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 112,597 | 1,392,412 | 1,505,009 | — | 3 | 8,979 |
| 2 | 160,653 | 2,388,815 | 2,549,468 | +1,044,459 | 4 | 15,094 |
| 3 | 244,891 | 3,386,217 | 3,631,108 | +2,126,099 | 5 | 21,292 |
| 4 | 309,252 | 4,407,921 | 4,717,173 | +3,212,164 | 6 | 27,325 |
| 5 | 397,999 | 5,474,697 | 5,872,696 | +4,367,687 | 7 | 33,769 |

Every chain verifies the same deposit: the reply sends the same `Spend` (nullifier `N = H(s ‖ statement id)` = `8c00fa9059bb883b…`) and the claimant keeps the same state, byte for byte, for 1 to 5 hops.
