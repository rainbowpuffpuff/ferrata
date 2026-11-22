# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 46,175 → Attest 20,673 | 66,848 | 1% | 1,968 | 550,528 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 54,120 → Attest 36,830 | 90,950 | 2% | 2,695 | 749,160 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 54,120 → Attest 113,473 → bridge (reply) 39,738 | 207,331 | 4% | 2,695 | 1,680,208 | applied |
| 4 | Register ETH (C), Gloas record, no header | bridge Register 74,163 → Attest 25,664 | 99,827 | 2% | 2,673 | 820,000 | refused: exit 215 (HeaderRequired) |
| 5 | Register ETH (C), Gloas record, with the header | bridge Register 84,910 → Attest 176,262 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 368,878 | 7% | 3,400 | 2,978,224 | applied |
| 6 | ImportRoot C, Gloas record, no header | bridge ImportRoot 51,475 → Attest 22,441 | 73,916 | 1% | 1,885 | 606,408 | refused: exit 215 (HeaderRequired) |
| 7 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 60,550 → Attest 135,887 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 267,680 | 5% | 2,612 | 2,162,336 | applied |
| 8 | Claim: leaf 0 under root(C, 2) | bridge Claim 107,796 → bridge Issue 48,632 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 261,759 | 5% | 2,053 | 2,110,496 | applied |
| 9 | Claim: leaf 0 again (replay) | bridge Claim 107,796 → bridge Issue 48,632 → bridge Admit 35,962 → bridge Spend 20,152 | 212,542 | 4% | 2,053 | 1,716,760 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 46,175 | 1 | 1,515 | 1,441 | 0 | 3,180 | 390 | 0 |
| Init C, Gloas record, no header | Attest | 20,673 | 1 | 1,569 | 1,178 | 253 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 54,120 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, another block's header | Attest | 36,830 | 1 | 2,260 | 1,869 | 253 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 54,120 | 1 | 2,242 | 2,168 | 0 | 4,598 | 390 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 113,473 | 1 | 2,260 | 1,869 | 253 | 2,723 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Register ETH (C), Gloas record, no header | bridge Register | 74,163 | 1 | 2,801 | 2,147 | 580 | 5,823 | 966 | 0 |
| Register ETH (C), Gloas record, no header | Attest | 25,664 | 1 | 2,350 | 1,959 | 253 | 0 | – | 215 |
| Register ETH (C), Gloas record, with the header | bridge Register | 84,910 | 1 | 3,528 | 2,874 | 580 | 7,241 | 966 | 0 |
| Register ETH (C), Gloas record, with the header | Attest | 176,262 | 1 | 3,041 | 2,650 | 253 | 3,855 | – | 0 |
| Register ETH (C), Gloas record, with the header | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (C), Gloas record, with the header | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 51,475 | 1 | 2,013 | 1,359 | 580 | 4,179 | 629 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 22,441 | 1 | 1,831 | 1,440 | 253 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 60,550 | 1 | 2,740 | 2,086 | 580 | 5,597 | 629 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 135,887 | 1 | 2,522 | 2,131 | 253 | 3,102 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Issue | 48,632 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Issue | 48,632 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim: leaf 0 again (replay) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 20,152 | 1 | 437 | 222 | 77 | 0 | – | 250 |
