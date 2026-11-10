# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `c0c80a6434cd0bcdaf4b32b71d4f71bfb28e5e23564fa23dfa4686792b48f1ed`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 27,672 → bridge Burn 18,195 | 45,867 | 1% | 647 | 372,112 | applied |
| 2 | Burn: the same note again | bridge Burn 27,672 → bridge Burn 18,852 | 46,524 | 1% | 647 | 377,368 | refused: exit 255 (BURN_EXISTS) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 19,322 | 19,322 | 0% | 647 | 159,752 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token | bridge Burn 18,958 | 18,958 | 0% | 647 | 156,840 | refused: exit 251 (NOT_REGISTERED) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 19,349 | 19,349 | 0% | 647 | 159,968 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 16,605 | 16,605 | 0% | 647 | 138,016 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn sent to the burn account itself | bridge Burn 13,652 | 13,652 | 0% | 648 | 114,400 | refused: exit 243 (WRONG_SENDER) |
| 8 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 19,427 | 29,810 | 1% | 867 | 245,416 | refused: exit 253 (NOT_AUTHORIZED) |
| 9 | Burn 1 ETH (B) by the payer | bridge Burn 26,985 → bridge Burn 18,195 | 45,180 | 1% | 647 | 366,616 | applied |
| 10 | Echo (rule (A)'s nonce) | bridge Echo 11,559 | 11,559 | 0% | 431 | 95,920 | applied |
| 11 | Sunset (not built) | bridge Sunset 10,050 | 10,050 | 0% | 431 | 83,848 | refused: exit 240 (BAD_MESSAGE) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 27,672 | 1 | 553 | 185 | 294 | 1,161 | 294 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 18,195 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 27,672 | 1 | 553 | 185 | 294 | 1,161 | 294 | 0 |
| Burn: the same note again | bridge Burn | 18,852 | 1 | 508 | 185 | 185 | 0 | – | 255 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 19,322 | 1 | 553 | 185 | 294 | 0 | – | 251 |
| Burn 1 unit of another token | bridge Burn | 18,958 | 1 | 553 | 185 | 294 | 0 | – | 251 |
| Burn 10 ETH with 9 held | bridge Burn | 19,349 | 1 | 553 | 185 | 294 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 16,605 | 1 | 409 | 185 | 150 | 0 | – | 253 |
| Burn sent to the burn account itself | bridge Burn | 13,652 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 19,427 | 1 | 617 | 185 | 294 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 26,985 | 1 | 553 | 185 | 294 | 1,017 | 150 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 18,195 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 11,559 | 1 | 257 | 33 | 150 | 278 | – | 0 |
| Sunset (not built) | bridge Sunset | 10,050 | 1 | 257 | 33 | 150 | 0 | – | 240 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 409 | 729 | 23,799 | 1 | 131,072 | 0 |

LEZ charged: execution gas 427,959, storage gas 224,063, base fee 5,216,176; transaction 7,817 bytes (dev mode).

The private claim that made the holding: execution gas 439,815, storage gas 224,063.

## What the checkpoint program collects (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the second burn marked reverted, as settlement recorded it):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| ImportRoot A n=8 | PublicRoot | 1177 | refused: TrailingBytes { at: 81 } | no |
| Claim: leaf 6 (10 ETH to the payer) | PublicRoot | 1214 | ignored (variant 4 and up) | no |
| Burn 1 ETH (A) by the payer | PublicRoot | 185 | Burn | yes |
| Burn: the same note again | PublicRoot | 185 | Burn | no (reverted) |
| Burn 1 ETH (B) by the payer | PublicRoot | 185 | Burn | yes |
| Echo (rule (A)'s nonce) | PublicRoot | 33 | Echo | yes |
| private claim (leaf 5) | PrivateToPublic | 1 | ignored (variant 4 and up) | no |
| private claim (leaf 5) | PublicToPrivate | 189 | ignored (variant 4 and up) | no (not public execution) |
| private claim (leaf 5) | PrivateToPublic | 186 | ignored (variant 4 and up) | no |
| private burn | PrivateToPublic | 185 | Burn | yes |

NOut v2 of the first note: `c4fd62d01b14ef56ff2e6b81f6b730ac52e6cbe4431398884b0b33cf703d9fc9`.
