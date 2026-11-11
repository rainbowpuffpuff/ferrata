# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `25b3cc900f38579ad530335b994e3ae327ac600afe335b734bd00af78309e776`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,736 → bridge Admit 35,566 → bridge Burn 21,612 | 166,477 | 3% | 839 | 1,338,528 | applied |
| 2 | Burn: the same note again | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,736 → bridge Admit 35,566 → bridge Burn 22,940 | 167,805 | 3% | 839 | 1,349,152 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 32,513 | 89,076 | 2% | 839 | 719,320 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 24,662 → bridge Burn 26,457 | 51,119 | 1% | 839 | 415,664 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 24,662 → bridge Burn 26,591 | 51,253 | 1% | 839 | 416,736 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 16,853 | 16,853 | 0% | 839 | 141,536 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 24,662 → bridge Burn 26,224 | 50,886 | 1% | 839 | 413,800 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 16,986 | 16,986 | 0% | 648 | 141,072 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 30,173 | 30,173 | 1% | 711 | 247,072 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 18,087 | 28,470 | 1% | 867 | 234,696 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 24,662 → bridge Burn 31,355 → bridge Burn 48,868 → bridge Admit 35,566 → bridge Burn 21,612 | 162,063 | 3% | 839 | 1,303,216 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 8,593 | 8,593 | 0% | 431 | 72,192 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 12,611 → bridge Sunset 24,832 | 37,443 | 1% | 495 | 303,504 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 52,736 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 35,566 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn: the same note again | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn: the same note again | bridge Burn | 52,736 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn: the same note again | bridge Admit | 35,566 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn: the same note again | bridge Burn | 22,940 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 32,513 | 1 | 1,354 | 185 | 1,031 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 26,457 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 10 ETH with 9 held | bridge Burn | 26,591 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 16,853 | 1 | 259 | 185 | 0 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 26,224 | 1 | 465 | 185 | 142 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 16,986 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 30,173 | 1 | 1,290 | 185 | 1,031 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 18,087 | 1 | 323 | 185 | 0 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 31,355 | 1 | 465 | 185 | 142 | 821 | 74 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 48,868 | 1 | 1,166 | 185 | 843 | 2,465 | 843 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 35,566 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 8,593 | 1 | 107 | 33 | 0 | 128 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 24,832 | 1 | 735 | 33 | 564 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 25,679 | 1 | 131,072 | 0 |

LEZ charged: execution gas 519,678, storage gas 224,063, base fee 5,949,928; transaction 7,857 bytes (dev mode).

The private claim that made the holding: execution gas 559,445, storage gas 224,063.

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

NOut v2 of the first note: `abbaffb93e5624615592ac6ad6e18c094a3f88051dc0a02a1cd4b6eef412eb93`.
