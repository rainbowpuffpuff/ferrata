# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 53,774 → bridge Admit 33,886 → bridge Burn 20,934 | 164,153 | 3% | 839 | 1,319,936 | applied |
| 2 | Burn: the same note again | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 53,774 → bridge Admit 33,886 → bridge Burn 22,759 | 165,978 | 3% | 839 | 1,334,536 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 33,603 | 89,162 | 2% | 839 | 720,008 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 23,952 → bridge Burn 26,189 | 50,141 | 1% | 839 | 407,840 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 23,952 → bridge Burn 26,323 | 50,275 | 1% | 839 | 408,912 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 16,183 | 16,183 | 0% | 839 | 136,176 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 23,952 → bridge Burn 25,956 | 49,908 | 1% | 839 | 405,976 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 16,317 | 16,317 | 0% | 648 | 135,720 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 29,812 | 29,812 | 1% | 711 | 244,184 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 17,418 | 27,801 | 1% | 867 | 229,344 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 23,952 → bridge Burn 31,061 → bridge Burn 49,906 → bridge Admit 33,886 → bridge Burn 20,934 | 159,739 | 3% | 839 | 1,284,624 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 8,041 | 8,041 | 0% | 431 | 67,776 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 11,926 → bridge Sunset 25,001 | 36,927 | 1% | 495 | 299,376 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 53,774 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn: the same note again | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn: the same note again | bridge Burn | 53,774 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn: the same note again | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn: the same note again | bridge Burn | 22,759 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 33,603 | 1 | 1,354 | 185 | 1,031 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 26,189 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 10 ETH with 9 held | bridge Burn | 26,323 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 16,183 | 1 | 259 | 185 | 0 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 25,956 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 16,317 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 29,812 | 1 | 1,290 | 185 | 1,031 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 17,418 | 1 | 323 | 185 | 0 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 31,061 | 1 | 465 | 185 | 142 | 821 | 74 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 49,906 | 1 | 1,166 | 185 | 843 | 2,465 | 843 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 8,041 | 1 | 107 | 33 | 0 | 128 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 11,926 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 25,001 | 1 | 691 | 33 | 520 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 25,377 | 1 | 131,072 | 0 |

LEZ charged: execution gas 518,358, storage gas 224,063, base fee 5,939,368; transaction 7,857 bytes (dev mode).

The private claim that made the holding: execution gas 557,703, storage gas 224,063.

## What the checkpoint program collects (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the second burn marked reverted, as settlement recorded it):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| ImportRoot A n=8 | PublicRoot | 1499 | ImportRoot (its proofs skipped) | yes |
| Claim: leaf 6 (10 ETH to the payer) | PublicRoot | 1271 | ignored (variant 4 and up) | no |
| Burn 1 ETH (A) by the payer | PublicRoot | 185 | Burn | yes |
| Burn: the same note again | PublicRoot | 185 | Burn | no (reverted) |
| Burn 1 ETH (B) by the payer | PublicRoot | 185 | Burn | yes |
| Echo (rule (A)'s nonce) | PublicRoot | 33 | Echo | yes |
| private claim (leaf 5) | PrivateToPublic | 1 | ignored (variant 4 and up) | no |
| private claim (leaf 5) | PublicToPrivate | 121 | ignored (variant 4 and up) | no (not public execution) |
| private claim (leaf 5) | PrivateToPublic | 167 | ignored (variant 4 and up) | no |
| private burn | PrivateToPublic | 185 | Burn | yes |

NOut v2 of the first note: `3c5318be0ec4224615f5ea10db2fed860a1ba2d5ef5f30159524fa7953d6bfbe`.
