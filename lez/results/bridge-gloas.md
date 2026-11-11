# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 45,336 → Attest 19,716 | 65,052 | 1% | 1,968 | 536,160 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 53,281 → Attest 35,873 | 89,154 | 2% | 2,695 | 734,792 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 53,281 → Attest 112,452 → bridge (reply) 39,242 | 204,975 | 4% | 2,695 | 1,661,360 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 72,152 → Attest 24,707 | 96,859 | 2% | 2,673 | 796,256 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 83,017 → Attest 175,195 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 363,303 | 7% | 3,400 | 2,933,624 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 49,195 → Attest 21,484 | 70,679 | 1% | 1,885 | 580,512 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 58,257 → Attest 134,981 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 262,290 | 5% | 2,612 | 2,119,216 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,610 → bridge Issue 48,443 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 14,616 | 241,439 | 5% | 2,053 | 1,947,936 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,610 → bridge Issue 48,443 → bridge Admit 33,983 → bridge Spend 19,502 | 209,538 | 4% | 2,053 | 1,692,728 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 45,336 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 19,716 | 1 | 1,537 | 1,178 | 221 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 53,281 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 35,873 | 1 | 2,228 | 1,869 | 221 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 53,281 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 112,452 | 1 | 2,228 | 1,869 | 221 | 2,691 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,242 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 72,152 | 1 | 2,741 | 2,147 | 520 | 5,703 | 906 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 24,707 | 1 | 2,318 | 1,959 | 221 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 83,017 | 1 | 3,468 | 2,874 | 520 | 7,121 | 906 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 175,195 | 1 | 3,009 | 2,650 | 221 | 3,823 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 49,195 | 1 | 1,953 | 1,359 | 520 | 4,059 | 569 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 21,484 | 1 | 1,799 | 1,440 | 221 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 58,257 | 1 | 2,680 | 2,086 | 520 | 5,477 | 569 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 134,981 | 1 | 2,490 | 2,131 | 221 | 3,070 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 48,443 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 48,443 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 19,502 | 1 | 437 | 222 | 77 | 0 | – | 250 |
