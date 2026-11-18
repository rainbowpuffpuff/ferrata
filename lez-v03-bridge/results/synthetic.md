# Synthetic vault storage: the inbound leg and the burns on LEZ v0.3.1's state machine

Written by `harness/tests/synthetic.rs` (`cargo test -p harness03 --test synthetic`, RISC Zero dev mode for the two private transactions' proofs). Bridge image id `eaab41521b27760e3767e2972e899948b5c16cc97df64cab17d772583b544bb4`; configuration `9a7cf42f8ea9f80d3186f17b16cf83bef9323ba48ddd1e3c4661aa3f469fd8dc`. 31 transactions, 9 applied. Execution gas is LEZ's metered cycles (`ValidatedStateDiff::from_public_transaction_metered`), storage gas the transaction's bytes; private transactions are fee-exempt on v0.3.1 (`lez/chain_state/src/classify.rs`).

| # | transaction | kind | result | execution gas | storage gas (bytes) |
|---:|---|---|---|---:|---:|
| 1 | Init without the block's header (Gloas record) | public | 215 `HEADER_REQUIRED` (public) | 32375 | 1786 |
| 2 | Init of another configuration of the vault (wrong pairing) | public | 246 `PAIRING_MISMATCH` (public) | 103334 | 2411 |
| 3 | ImportRoot before Init | public | 245 `NOT_FOUND` (public) | 150428 | 2713 |
| 4 | Init (the vault's slot 2 holds configId) | public | applied | 140483 | 2447 |
| 5 | Init again | public | 244 `ALREADY_WRITTEN` (public) | 137693 | 2447 |
| 6 | ImportRoot of a root the vault does not hold | public | 247 `IMPORT_MISMATCH` (public) | 124756 | 2713 |
| 7 | ImportRoot naming a record slot it is not keyed by | public | 247 `IMPORT_MISMATCH` (public) | 37306 | 2713 |
| 8 | ImportRoot n = 4 | public | applied | 173632 | 2713 |
| 9 | ImportRoot n = 4 again | public | 244 `ALREADY_WRITTEN` (public) | 171899 | 2713 |
| 10 | Claim deposit 0 (public kind, 0.001 ETH to alice) | public | applied | 152231 | 1972 |
| 11 | the same claim again (replay) | public | 250 `ALREADY_CLAIMED` (public) | 142263 | 1972 |
| 12 | deposit 0 claimed to another destination | public | 249 `CLAIM_INVALID` (public) | 100151 | 1972 |
| 13 | deposit 2 under a root never imported (n = 5) | public | 245 `NOT_FOUND` (public) | 132008 | 1972 |
| 14 | deposit 2 under a made-up root | public | 249 `CLAIM_INVALID` (public) | 100151 | 1972 |
| 15 | deposit 4 (leaf index 4) under root(cid, 4) | public | 248 `LEAF_NOT_IMPORTED` (public) | 37411 | 1972 |
| 16 | deposit 1 (private kind) by the plaintext path | public | 252 `WRONG_DESTINATION` (public) | 103601 | 1972 |
| 17 | deposit 3 (0.005 ETH, not registered) | public | 251 `NOT_REGISTERED` (public) | 103788 | 1972 |
| 18 | Claim deposit 2 (100 tokens to alice) | public | applied | 154742 | 1972 |
| 19 | ClaimPrivately of deposit 1 by another private account | private | 252 `WRONG_DESTINATION` (private part: no proof) | exempt | 0 |
| 20 | ClaimPrivately of deposit 1 (private kind, to the private account) | private | applied | exempt | 3781 |
| 21 | ClaimPrivately of deposit 1 again (replay) | private | 250 `ALREADY_CLAIMED` (settlement) | exempt | 3781 |
| 22 | ImportRoot n = 5 (payload record) | public | applied | 156274 | 2164 |
| 23 | Claim deposit 4 under root(cid, 5) | public | applied | 155685 | 1972 |
| 24 | deposit 0 again under the later root | public | 250 `ALREADY_CLAIMED` (public) | 142263 | 1972 |
| 25 | Burn 0.001 ETH (alice, public) | public | applied | 279815 | 1012 |
| 26 | the same note again (alice holds 0.001 ETH more) | public | 244 `ALREADY_WRITTEN` (public) | 66072 | 1012 |
| 27 | Burn 0.002 ETH (not a denomination) | public | 251 `NOT_REGISTERED` (public) | 26289 | 1012 |
| 28 | Burn 100 tokens by bob, who holds none | public | 254 `INSUFFICIENT_BALANCE` (public) | 51848 | 1012 |
| 29 | alice's burn sent without alice's signature | public | 253 `NOT_AUTHORIZED` (public) | 26027 | 900 |
| 30 | Burn 0.001 ETH (the private account, private) | private | applied | exempt | 4165 |
| 31 | a second private burn (the private account holds nothing) | private | 254 `INSUFFICIENT_BALANCE` (private part: no proof) | exempt | 0 |


**The private claim's transaction** (dev-mode proof): 3780 bytes searched; present: configId, root(cid, 4)'s account; absent: the destination (the private account's id), s, C, the deposit leaf, the depositor, the borsh witness, N, the leaf index (8-byte LE, with the depositor), the imported root, a path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling, another path sibling.

**Burn tree** at `burns(cid)`: 2 leaves, root `9e54cc4acf63b1b648bbd479491358c2c6dead95645dd41ee254622d79ba8c67` = the bridge core's `Tree` of the two notes' `burn_leaf`.
