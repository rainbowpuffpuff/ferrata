# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `f11b1aba55fde1ea977f74a54cc927ff50253e8bf479c89dac0b3658177c58eb`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 26,461 → bridge Burn 49,322 → bridge Admit 34,250 → bridge Burn 20,835 | 130,868 | 3% | 775 | 1,053,144 | applied |
| 2 | Burn: the same note again | bridge Burn 26,461 → bridge Burn 49,033 → bridge Admit 33,961 → bridge Burn 22,658 | 132,113 | 3% | 775 | 1,063,104 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 26,461 → bridge Burn 30,457 | 56,918 | 1% | 775 | 461,544 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 20,758 | 20,758 | 0% | 775 | 172,264 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 21,059 | 21,059 | 0% | 775 | 174,672 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 19,575 | 19,575 | 0% | 775 | 162,800 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 20,525 | 20,525 | 0% | 775 | 170,400 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 16,224 | 16,224 | 0% | 648 | 134,976 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 28,347 | 28,347 | 1% | 711 | 232,464 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 21,508 | 31,891 | 1% | 867 | 262,064 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 25,917 → bridge Burn 46,346 → bridge Admit 34,250 → bridge Burn 20,835 | 127,348 | 3% | 775 | 1,024,984 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 11,947 | 11,947 | 0% | 431 | 99,024 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 15,509 → bridge Sunset 23,768 | 39,277 | 1% | 495 | 318,176 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 49,322 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 34,250 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 20,835 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn: the same note again | bridge Burn | 49,033 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn: the same note again | bridge Admit | 33,961 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn: the same note again | bridge Burn | 22,658 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 30,457 | 1 | 1,289 | 185 | 966 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 20,758 | 1 | 401 | 185 | 142 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 21,059 | 1 | 401 | 185 | 142 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 19,575 | 1 | 333 | 185 | 74 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 20,525 | 1 | 401 | 185 | 142 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 16,224 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 28,347 | 1 | 1,225 | 185 | 966 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 21,508 | 1 | 465 | 185 | 142 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 25,917 | 1 | 401 | 185 | 142 | 757 | 74 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 46,346 | 1 | 1,101 | 185 | 778 | 2,336 | 778 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 34,250 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 20,835 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 11,947 | 1 | 181 | 33 | 74 | 202 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 15,509 | 1 | 181 | 33 | 74 | 307 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 23,768 | 1 | 691 | 33 | 520 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 24,397 | 1 | 131,072 | 0 |

LEZ charged: execution gas 513,593, storage gas 224,063, base fee 5,901,248; transaction 7,857 bytes (dev mode).

The private claim that made the holding: execution gas 544,780, storage gas 224,063.

## What the checkpoint program collects (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the second burn marked reverted, as settlement recorded it):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| ImportRoot A n=8 | PublicRoot | 1499 | ImportRoot (its proofs skipped) | yes |
| Claim: leaf 6 (10 ETH to the payer) | PublicRoot | 1279 | ignored (variant 4 and up) | no |
| Burn 1 ETH (A) by the payer | PublicRoot | 185 | Burn | yes |
| Burn: the same note again | PublicRoot | 185 | Burn | no (reverted) |
| Burn 1 ETH (B) by the payer | PublicRoot | 185 | Burn | yes |
| Echo (rule (A)'s nonce) | PublicRoot | 33 | Echo | yes |
| private claim (leaf 5) | PrivateToPublic | 1 | ignored (variant 4 and up) | no |
| private claim (leaf 5) | PublicToPrivate | 121 | ignored (variant 4 and up) | no (not public execution) |
| private claim (leaf 5) | PrivateToPublic | 175 | ignored (variant 4 and up) | no |
| private burn | PrivateToPublic | 185 | Burn | yes |

NOut v2 of the first note: `5f39ce245042b28efdbc34bba22531748b015ed71d8b5f7fa664fd977672d890`.
