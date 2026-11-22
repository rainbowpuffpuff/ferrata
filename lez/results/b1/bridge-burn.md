# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,418 → bridge Admit 35,963 → bridge Burn 21,662 | 165,329 | 3% | 839 | 1,329,344 | applied |
| 2 | Burn: the same note again | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,418 → bridge Admit 35,963 → bridge Burn 23,037 | 166,704 | 3% | 839 | 1,340,344 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 32,706 | 87,992 | 2% | 839 | 710,648 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 24,855 → bridge Burn 25,714 | 50,569 | 1% | 839 | 411,264 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 24,855 → bridge Burn 25,864 | 50,719 | 1% | 839 | 412,464 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 16,903 | 16,903 | 0% | 839 | 141,936 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 24,862 → bridge Burn 21,440 | 46,302 | 1% | 839 | 377,128 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 17,036 | 17,036 | 0% | 648 | 141,472 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 30,355 | 30,355 | 1% | 711 | 248,528 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 18,137 | 28,520 | 1% | 867 | 235,096 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 24,855 → bridge Burn 30,009 → bridge Burn 48,670 → bridge Admit 35,963 → bridge Burn 21,662 | 161,159 | 3% | 839 | 1,295,984 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 8,636 | 8,636 | 0% | 431 | 72,536 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 12,677 → bridge Sunset 25,058 | 37,735 | 1% | 495 | 305,840 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 52,418 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 21,662 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn: the same note again | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn: the same note again | bridge Burn | 52,418 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn: the same note again | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn: the same note again | bridge Burn | 23,037 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 32,706 | 1 | 1,363 | 185 | 1,040 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 25,714 | 1 | 397 | 185 | 74 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 10 ETH with 9 held | bridge Burn | 25,864 | 1 | 397 | 185 | 74 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 16,903 | 1 | 259 | 185 | 0 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 24,862 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 21,440 | 1 | 323 | 185 | 0 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 17,036 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 30,355 | 1 | 1,299 | 185 | 1,040 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 18,137 | 1 | 323 | 185 | 0 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 30,009 | 1 | 397 | 185 | 74 | 685 | 6 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 48,670 | 1 | 1,175 | 185 | 852 | 2,483 | 852 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 21,662 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 8,636 | 1 | 107 | 33 | 0 | 128 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 25,058 | 1 | 751 | 33 | 580 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 25,773 | 1 | 131,072 | 0 |

LEZ charged: execution gas 519,807, storage gas 224,063, base fee 5,950,960; transaction 7,857 bytes (dev mode).

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

NOut v2 of the first note: `3922b646252d6295aee59c10b124f5256e1799dce59c6d0646e66a756d45098f`.
