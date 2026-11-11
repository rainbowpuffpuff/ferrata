# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `25b3cc900f38579ad530335b994e3ae327ac600afe335b734bd00af78309e776`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 46,103 → Attest 19,716 | 65,819 | 1% | 1,968 | 542,296 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 54,048 → Attest 35,873 | 89,921 | 2% | 2,695 | 740,928 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 54,048 → Attest 112,452 → bridge (reply) 39,419 | 205,919 | 4% | 2,695 | 1,668,912 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 73,647 → Attest 24,707 | 98,354 | 2% | 2,673 | 808,216 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 84,495 → Attest 175,195 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 366,313 | 7% | 3,400 | 2,957,704 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 51,024 → Attest 21,484 | 72,508 | 1% | 1,885 | 595,144 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 60,259 → Attest 134,981 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 265,973 | 5% | 2,612 | 2,148,680 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,702 → bridge Issue 47,280 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 18,161 | 257,785 | 5% | 2,053 | 2,078,704 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,702 → bridge Issue 47,280 → bridge Admit 35,565 → bridge Spend 19,559 | 210,106 | 4% | 2,053 | 1,697,272 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 46,103 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 19,716 | 1 | 1,537 | 1,178 | 221 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 54,048 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 35,873 | 1 | 2,228 | 1,869 | 221 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 54,048 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 112,452 | 1 | 2,228 | 1,869 | 221 | 2,691 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 73,647 | 1 | 2,785 | 2,147 | 564 | 5,791 | 950 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 24,707 | 1 | 2,318 | 1,959 | 221 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 84,495 | 1 | 3,512 | 2,874 | 564 | 7,209 | 950 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 175,195 | 1 | 3,009 | 2,650 | 221 | 3,823 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 51,024 | 1 | 1,997 | 1,359 | 564 | 4,147 | 613 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 21,484 | 1 | 1,799 | 1,440 | 221 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 60,259 | 1 | 2,724 | 2,086 | 564 | 5,565 | 613 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 134,981 | 1 | 2,490 | 2,131 | 221 | 3,070 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 47,280 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 47,280 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 19,559 | 1 | 437 | 222 | 77 | 0 | – | 250 |
