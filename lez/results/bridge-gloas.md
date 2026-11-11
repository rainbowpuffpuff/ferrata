# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 45,192 → Attest 19,716 | 64,908 | 1% | 1,968 | 535,008 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 53,137 → Attest 35,873 | 89,010 | 2% | 2,695 | 733,640 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 53,137 → Attest 112,452 → bridge (reply) 39,144 | 204,733 | 4% | 2,695 | 1,659,424 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 72,023 → Attest 24,707 | 96,730 | 2% | 2,673 | 795,224 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 82,888 → Attest 175,195 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 362,812 | 7% | 3,400 | 2,929,696 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 49,210 → Attest 21,484 | 70,694 | 1% | 1,885 | 580,632 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 58,272 → Attest 134,981 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 262,007 | 5% | 2,612 | 2,116,952 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,479 → bridge Issue 48,214 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 17,560 | 254,999 | 5% | 2,053 | 2,056,416 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,479 → bridge Issue 48,214 → bridge Admit 33,886 → bridge Spend 19,394 | 208,973 | 4% | 2,053 | 1,688,208 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 45,192 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 19,716 | 1 | 1,537 | 1,178 | 221 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 53,137 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 35,873 | 1 | 2,228 | 1,869 | 221 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 53,137 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 112,452 | 1 | 2,228 | 1,869 | 221 | 2,691 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,144 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 72,023 | 1 | 2,741 | 2,147 | 520 | 5,703 | 906 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 24,707 | 1 | 2,318 | 1,959 | 221 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 82,888 | 1 | 3,468 | 2,874 | 520 | 7,121 | 906 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 175,195 | 1 | 3,009 | 2,650 | 221 | 3,823 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 49,210 | 1 | 1,953 | 1,359 | 520 | 4,059 | 569 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 21,484 | 1 | 1,799 | 1,440 | 221 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 58,272 | 1 | 2,680 | 2,086 | 520 | 5,477 | 569 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 134,981 | 1 | 2,490 | 2,131 | 221 | 3,070 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 48,214 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 48,214 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 19,394 | 1 | 437 | 222 | 77 | 0 | – | 250 |
