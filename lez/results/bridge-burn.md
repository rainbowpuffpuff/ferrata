# Burns (`harness/tests/bridge_burn.rs`); the private burn in RISC Zero dev mode

Bridge program image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`.

## Public burns and refusals

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH (A) by the payer | bridge Burn 26,722 → bridge Burn 54,037 → bridge Admit 33,983 → bridge Burn 21,107 | 135,849 | 3% | 775 | 1,092,992 | applied |
| 2 | Burn: the same note again | bridge Burn 26,722 → bridge Burn 54,037 → bridge Admit 33,983 → bridge Burn 22,936 | 137,678 | 3% | 775 | 1,107,624 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.5 ETH (not a denomination) | bridge Burn 26,722 → bridge Burn 33,773 | 60,495 | 1% | 775 | 490,160 | refused: exit 251 (NOT_REGISTERED) |
| 4 | Burn 1 unit of another token (no holding of it) | bridge Burn 21,025 | 21,025 | 0% | 775 | 174,400 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 5 | Burn 10 ETH with 9 held | bridge Burn 21,326 | 21,326 | 0% | 775 | 176,808 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 6 | Burn at the vector's destination, not signed by it | bridge Burn 19,838 | 19,838 | 0% | 775 | 164,904 | refused: exit 253 (NOT_AUTHORIZED) |
| 7 | Burn under a configuration the payer holds nothing of | bridge Burn 20,792 | 20,792 | 0% | 775 | 172,536 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 8 | Burn sent to the burn account itself | bridge Burn 16,492 | 16,492 | 0% | 648 | 137,120 | refused: exit 243 (WRONG_SENDER) |
| 9 | Burn sent to the token account itself (a top-level call) | bridge Burn 29,987 | 29,987 | 1% | 711 | 245,584 | refused: exit 243 (WRONG_SENDER) |
| 10 | Burn as a nested call from another program, in a transaction the holder signed | scripted 10,383 → bridge Burn 21,781 | 32,164 | 1% | 867 | 264,248 | refused: exit 253 (NOT_AUTHORIZED) |
| 11 | Burn 1 ETH (B) by the payer | bridge Burn 26,178 → bridge Burn 50,169 → bridge Admit 33,983 → bridge Burn 21,107 | 131,437 | 3% | 775 | 1,057,696 | applied |
| 12 | Echo (rule (A)'s nonce) | bridge Echo 12,231 | 12,231 | 0% | 431 | 101,296 | applied |
| 13 | Sunset by the payer (not the freeze authority) | bridge Sunset 16,342 → bridge Sunset 25,052 | 41,394 | 1% | 495 | 335,112 | refused: exit 253 (NOT_AUTHORIZED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn 1 ETH (A) by the payer | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 54,037 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) by the payer | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) by the payer | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn: the same note again | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn: the same note again | bridge Burn | 54,037 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn: the same note again | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn: the same note again | bridge Burn | 22,936 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 0.5 ETH (not a denomination) | bridge Burn | 33,773 | 1 | 1,354 | 185 | 1,031 | 0 | – | 251 |
| Burn 1 unit of another token (no holding of it) | bridge Burn | 21,025 | 1 | 401 | 185 | 142 | 0 | – | 254 |
| Burn 10 ETH with 9 held | bridge Burn | 21,326 | 1 | 401 | 185 | 142 | 0 | – | 254 |
| Burn at the vector's destination, not signed by it | bridge Burn | 19,838 | 1 | 333 | 185 | 74 | 0 | – | 253 |
| Burn under a configuration the payer holds nothing of | bridge Burn | 20,792 | 1 | 401 | 185 | 142 | 0 | – | 254 |
| Burn sent to the burn account itself | bridge Burn | 16,492 | 1 | 259 | 185 | 0 | 0 | – | 243 |
| Burn sent to the token account itself (a top-level call) | bridge Burn | 29,987 | 1 | 1,290 | 185 | 1,031 | 0 | – | 243 |
| Burn as a nested call from another program, in a transaction the holder signed | scripted | 10,383 | 1 | 350 | 276 | 0 | 628 | – | 0 |
| Burn as a nested call from another program, in a transaction the holder signed | bridge Burn | 21,781 | 1 | 465 | 185 | 142 | 0 | – | 253 |
| Burn 1 ETH (B) by the payer | bridge Burn | 26,178 | 1 | 401 | 185 | 142 | 757 | 74 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 50,169 | 1 | 1,166 | 185 | 843 | 2,465 | 843 | 0 |
| Burn 1 ETH (B) by the payer | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (B) by the payer | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Echo (rule (A)'s nonce) | bridge Echo | 12,231 | 1 | 181 | 33 | 74 | 202 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 16,342 | 1 | 181 | 33 | 74 | 307 | – | 0 |
| Sunset by the payer (not the freeze authority) | bridge Sunset | 25,052 | 1 | 691 | 33 | 520 | 0 | – | 253 |

## The private burn

| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |
|---|---:|---:|---:|---:|---:|---:|
| Burn at the private holder (debits, sends Burn) | 333 | 621 | 24,654 | 1 | 131,072 | 0 |

LEZ charged: execution gas 518,891, storage gas 224,063, base fee 5,943,632; transaction 7,857 bytes (dev mode).

The private claim that made the holding: execution gas 551,134, storage gas 224,063.

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

NOut v2 of the first note: `315ed2ed045632ca7f100e00deb70c869b08844d33d13405f0940ba705ae0b02`.
