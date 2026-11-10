# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `f11b1aba55fde1ea977f74a54cc927ff50253e8bf479c89dac0b3658177c58eb`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 44,953 → Attest 19,716 | 64,669 | 1% | 1,968 | 533,096 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 52,893 → Attest 35,873 | 88,766 | 2% | 2,695 | 731,688 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 52,893 → Attest 112,452 → bridge (reply) 39,214 | 204,559 | 4% | 2,695 | 1,658,032 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 71,768 → Attest 24,707 | 96,475 | 2% | 2,673 | 793,184 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 82,628 → Attest 175,195 → bridge (reply) 73,421 → bridge WriteToken 29,035 | 360,279 | 7% | 3,400 | 2,909,432 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 48,884 → Attest 21,484 | 70,368 | 1% | 1,885 | 578,024 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 57,941 → Attest 134,981 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 262,195 | 5% | 2,612 | 2,118,456 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,526 → bridge Issue 44,380 → bridge Admit 34,291 → bridge Spend 21,901 → bridge Mint 14,237 → bridge Mint 14,237 | 236,572 | 5% | 2,061 | 1,909,064 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,526 → bridge Issue 44,092 → bridge Admit 34,003 → bridge Spend 19,227 | 204,848 | 4% | 2,061 | 1,655,272 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 44,953 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 19,716 | 1 | 1,537 | 1,178 | 221 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 52,893 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 35,873 | 1 | 2,228 | 1,869 | 221 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 52,893 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 112,452 | 1 | 2,228 | 1,869 | 221 | 2,691 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,214 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 71,768 | 1 | 2,741 | 2,147 | 520 | 5,703 | 906 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 24,707 | 1 | 2,318 | 1,959 | 221 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 82,628 | 1 | 3,468 | 2,874 | 520 | 7,121 | 906 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 175,195 | 1 | 3,009 | 2,650 | 221 | 3,823 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 73,421 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 29,035 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 48,884 | 1 | 1,953 | 1,359 | 520 | 4,059 | 569 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 21,484 | 1 | 1,799 | 1,440 | 221 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 57,941 | 1 | 2,680 | 2,086 | 520 | 5,477 | 569 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 134,981 | 1 | 2,490 | 2,131 | 221 | 3,070 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 44,380 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 34,291 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 21,901 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 44,092 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 34,003 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 19,227 | 1 | 437 | 222 | 77 | 0 | – | 250 |
