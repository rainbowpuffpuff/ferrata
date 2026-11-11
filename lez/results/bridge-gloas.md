# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `fe2f248724ddbeada455f2f132ae178d84434db3f26ddce50e06d72cc7ae946d`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 44,815 → Attest 19,716 | 64,531 | 1% | 1,968 | 531,992 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 52,755 → Attest 35,873 | 88,628 | 2% | 2,695 | 730,584 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 52,755 → Attest 112,452 → bridge (reply) 39,116 | 204,323 | 4% | 2,695 | 1,656,144 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 71,645 → Attest 24,707 | 96,352 | 2% | 2,673 | 792,200 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 82,505 → Attest 175,195 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 359,795 | 7% | 3,400 | 2,905,560 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 48,885 → Attest 21,484 | 70,369 | 1% | 1,885 | 578,032 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 57,942 → Attest 134,981 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 261,897 | 5% | 2,612 | 2,116,072 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,397 → bridge Issue 44,154 → bridge Admit 34,187 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 17,155 | 250,082 | 5% | 2,061 | 2,017,144 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,397 → bridge Issue 43,866 → bridge Admit 33,899 → bridge Spend 19,125 | 204,287 | 4% | 2,061 | 1,650,784 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 44,815 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 19,716 | 1 | 1,537 | 1,178 | 221 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 52,755 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 35,873 | 1 | 2,228 | 1,869 | 221 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 52,755 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 112,452 | 1 | 2,228 | 1,869 | 221 | 2,691 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,116 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 71,645 | 1 | 2,741 | 2,147 | 520 | 5,703 | 906 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 24,707 | 1 | 2,318 | 1,959 | 221 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 82,505 | 1 | 3,468 | 2,874 | 520 | 7,121 | 906 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 175,195 | 1 | 3,009 | 2,650 | 221 | 3,823 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 48,885 | 1 | 1,953 | 1,359 | 520 | 4,059 | 569 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 21,484 | 1 | 1,799 | 1,440 | 221 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 57,942 | 1 | 2,680 | 2,086 | 520 | 5,477 | 569 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 134,981 | 1 | 2,490 | 2,131 | 221 | 3,070 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 44,154 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 34,187 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 43,866 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 19,125 | 1 | 437 | 222 | 77 | 0 | – | 250 |
