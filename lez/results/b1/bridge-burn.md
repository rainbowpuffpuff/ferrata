# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 19,498 | 156,678 | 3% | 839 | 1,260,136 | applied |
| 2 | Burn: the same note again | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 20,873 | 158,053 | 3% | 839 | 1,271,136 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 30,544 | 81,507 | 2% | 839 | 658,768 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 22,693 → bridge Burn 23,553 | 46,246 | 1% | 839 | 376,680 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 22,693 → bridge Burn 23,703 | 46,396 | 1% | 839 | 377,880 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 14,742 | 14,742 | 0% | 839 | 124,648 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 22,693 → bridge Burn 19,279 | 41,972 | 1% | 839 | 342,488 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 14,872 | 14,872 | 0% | 648 | 124,160 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 28,191 | 28,191 | 1% | 711 | 231,216 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 15,978 | 26,361 | 1% | 867 | 217,824 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 22,693 → bridge Burn 27,848 → bridge Burn 46,506 → bridge Admit 35,963 → bridge Burn 19,498 | 152,508 | 3% | 839 | 1,226,776 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 8,636 | 8,636 | 0% | 431 | 72,536 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 12,677 → bridge Sunset 25,058 | 37,735 | 1% | 495 | 305,840 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 50,254 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 19,498 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn: the same note again | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn: the same note again | bridge Burn | 50,254 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn: the same note again | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn: the same note again | bridge Burn | 20,873 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 30,544 | 1 | 1,363 | 185 | 1,040 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 23,553 | 1 | 397 | 185 | 74 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 10 ETH with 9 held | bridge Burn | 23,703 | 1 | 397 | 185 | 74 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 14,742 | 1 | 259 | 185 | 0 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 19,279 | 1 | 323 | 185 | 0 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 14,872 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 28,191 | 1 | 1,299 | 185 | 1,040 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 15,978 | 1 | 323 | 185 | 0 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 27,848 | 1 | 397 | 185 | 74 | 685 | 6 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 46,506 | 1 | 1,175 | 185 | 852 | 2,483 | 852 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 19,498 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 8,636 | 1 | 107 | 33 | 0 | 128 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 25,058 | 1 | 751 | 33 | 580 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 23,610 | 1 | 131,072 | 0 |

LEZ charged: execution gas 515,479, storage gas 224,063, base fee 5,916,336; transaction 7,857 bytes (dev mode).

The private claim that made the holding: execution gas 561,278, storage gas 224,063.

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

NOut v2 of the first note: `df3ad7c7c13caac83d5f8b97cb9f0bfcde6b1db25339ede556b1c050b46955a9`.
