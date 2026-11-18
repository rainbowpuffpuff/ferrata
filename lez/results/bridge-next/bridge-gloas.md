# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `c5cad2cd470807a1799613a1f5f699f9d4f4074821a9fb26389ca917a4d0d6da`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 46,182 → Attest 20,673 | 66,855 | 1% | 1,968 | 550,584 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 54,127 → Attest 36,830 | 90,957 | 2% | 2,695 | 749,216 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 54,127 → Attest 113,473 → bridge (reply) 39,419 | 207,019 | 4% | 2,695 | 1,677,712 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 73,634 → Attest 25,664 | 99,298 | 2% | 2,673 | 815,768 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 84,482 → Attest 176,262 → bridge (reply) 74,463 → bridge WriteToken 32,147 | 367,354 | 7% | 3,400 | 2,966,032 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 51,024 → Attest 22,441 | 73,465 | 1% | 1,885 | 602,800 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 60,259 → Attest 135,887 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 266,879 | 5% | 2,612 | 2,155,928 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,702 → bridge Issue 48,328 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761 | 260,842 | 5% | 2,053 | 2,103,160 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,702 → bridge Issue 48,328 → bridge Admit 35,565 → bridge Spend 20,082 | 211,677 | 4% | 2,053 | 1,709,840 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 46,182 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 20,673 | 1 | 1,569 | 1,178 | 253 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 54,127 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 36,830 | 1 | 2,260 | 1,869 | 253 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 54,127 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 113,473 | 1 | 2,260 | 1,869 | 253 | 2,723 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 73,634 | 1 | 2,785 | 2,147 | 564 | 5,791 | 950 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 25,664 | 1 | 2,350 | 1,959 | 253 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 84,482 | 1 | 3,512 | 2,874 | 564 | 7,209 | 950 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 176,262 | 1 | 3,041 | 2,650 | 253 | 3,855 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 74,463 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 51,024 | 1 | 1,997 | 1,359 | 564 | 4,147 | 613 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 22,441 | 1 | 1,831 | 1,440 | 253 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 60,259 | 1 | 2,724 | 2,086 | 564 | 5,565 | 613 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 135,887 | 1 | 2,522 | 2,131 | 253 | 3,102 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 48,328 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 31,725 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 48,328 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 20,082 | 1 | 437 | 222 | 77 | 0 | – | 250 |
