# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `fe2f248724ddbeada455f2f132ae178d84434db3f26ddce50e06d72cc7ae946d`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 49,057 → bridge Admit 34,146 → bridge Burn 20,661 | 158,905 | 3% | 839 | 1,277,952 | applied |
| 2 | Burn: the same note again | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 48,768 → bridge Admit 33,857 → bridge Burn 22,480 | 160,146 | 3% | 839 | 1,287,880 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 30,285 | 85,326 | 2% | 839 | 689,320 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 23,688 → bridge Burn 25,928 | 49,616 | 1% | 839 | 403,640 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 23,688 → bridge Burn 26,062 | 49,750 | 1% | 839 | 404,712 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 15,914 | 15,914 | 0% | 839 | 134,024 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 23,688 → bridge Burn 25,695 | 49,383 | 1% | 839 | 401,776 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 16,048 | 16,048 | 0% | 648 | 133,568 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 28,171 | 28,171 | 1% | 711 | 231,056 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 17,146 | 27,529 | 1% | 867 | 227,168 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 23,688 → bridge Burn 30,807 → bridge Burn 46,081 → bridge Admit 34,146 → bridge Burn 20,661 | 155,383 | 3% | 839 | 1,249,776 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 7,749 | 7,749 | 0% | 431 | 65,440 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 11,086 → bridge Sunset 23,714 | 34,800 | 1% | 495 | 282,360 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 49,057 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 34,146 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 20,661 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn: the same note again | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn: the same note again | bridge Burn | 48,768 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn: the same note again | bridge Admit | 33,857 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn: the same note again | bridge Burn | 22,480 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 30,285 | 1 | 1,289 | 185 | 966 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 25,928 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 10 ETH with 9 held | bridge Burn | 26,062 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 15,914 | 1 | 259 | 185 | 0 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 25,695 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 16,048 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 28,171 | 1 | 1,225 | 185 | 966 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 17,146 | 1 | 323 | 185 | 0 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 30,807 | 1 | 465 | 185 | 142 | 821 | 74 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 46,081 | 1 | 1,101 | 185 | 778 | 2,336 | 778 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 34,146 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 20,661 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 7,749 | 1 | 107 | 33 | 0 | 128 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 11,086 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 23,714 | 1 | 691 | 33 | 520 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 25,130 | 1 | 131,072 | 0 |

LEZ charged: execution gas 513,050, storage gas 224,063, base fee 5,896,904; transaction 7,857 bytes (dev mode).

The private claim that made the holding: execution gas 551,321, storage gas 224,063.

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

NOut v2 of the first note: `35a5a256e7f23b1e1e3c4f4d5f39e5c3761f1620c6aff10f96eb4671ce894b70`.
