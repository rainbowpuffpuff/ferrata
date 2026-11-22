# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 44,890 → Attest 86,790 → bridge (reply) 39,738 | 171,418 | 3% | 1,946 | 1,386,912 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 44,270 → Attest 84,699 → bridge (reply) 39,738 | 168,707 | 3% | 1,882 | 1,364,712 | applied |
| 3 | Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init 44,890 → Attest 86,790 → bridge (reply) 32,295 | 163,975 | 3% | 1,946 | 1,327,368 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 39,682 | 39,682 | 1% | 1,946 | 333,024 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register 74,324 → Attest 151,273 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 333,303 | 7% | 2,747 | 2,688,400 | applied |
| 6 | Register ETH (B) | bridge Register 77,279 → Attest 156,184 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 341,169 | 7% | 2,770 | 2,751,512 | applied |
| 7 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 50,522 → Attest 109,870 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 231,635 | 5% | 1,895 | 1,868,240 | applied |
| 8 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,140 | 5% | 2,025 | 1,913,320 | applied |
| 9 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 18,660 | 238,669 | 5% | 2,025 | 1,925,552 | refused: exit 244 (ALREADY_WRITTEN) |
| 10 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 39,322 | 205,219 | 4% | 2,025 | 1,657,952 | refused: exit 247 (IMPORT_MISMATCH) |
| 11 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 38,842 | 204,739 | 4% | 2,025 | 1,654,112 | refused: exit 247 (IMPORT_MISMATCH) |
| 12 | ImportRoot A with vault B's proofs | bridge ImportRoot 50,365 → Attest 44,014 | 94,379 | 2% | 1,918 | 770,376 | refused: exit 110 (ProofNodeMismatch) |
| 13 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 16,189 | 16,189 | 0% | 2,090 | 146,232 | refused: exit 245 (NOT_FOUND) |
| 14 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,140 | 5% | 2,025 | 1,913,320 | applied |
| 15 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 50,365 → Attest 115,561 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,169 | 5% | 1,918 | 1,912,696 | applied |
| 16 | ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 42,881 | 271,623 | 5% | 2,206 | 2,190,632 | applied |
| 17 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 50,613 → Attest 108,223 → bridge (reply) 52,960 → bridge ExtendFees 40,597 | 252,393 | 5% | 1,945 | 2,034,704 | applied |
| 18 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 107,796 → bridge Issue 52,567 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 265,694 | 5% | 2,053 | 2,141,976 | applied |
| 19 | Claim: the same again (replay) | bridge Claim 107,796 → bridge Issue 52,567 → bridge Admit 35,962 → bridge Spend 20,152 | 216,477 | 4% | 2,053 | 1,748,240 | refused: exit 250 (ALREADY_CLAIMED) |
| 20 | Claim: the same deposit under root(A, 8) | bridge Claim 107,796 → bridge Issue 52,567 → bridge Admit 35,962 → bridge Spend 20,152 | 216,477 | 4% | 2,053 | 1,748,240 | refused: exit 250 (ALREADY_CLAIMED) |
| 21 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 32,106 | 32,106 | 1% | 2,053 | 273,272 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | Claim: leaf 6, one sibling changed | bridge Claim 94,862 | 94,862 | 2% | 2,053 | 775,320 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 94,862 | 94,862 | 2% | 2,053 | 775,320 | refused: exit 249 (CLAIM_INVALID) |
| 24 | Claim: leaf 6's preimage at index 4 | bridge Claim 94,862 | 94,862 | 2% | 2,053 | 775,320 | refused: exit 249 (CLAIM_INVALID) |
| 25 | Claim: leaf 6 redirected to another destination | bridge Claim 94,862 | 94,862 | 2% | 2,053 | 775,320 | refused: exit 249 (CLAIM_INVALID) |
| 26 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 94,862 | 94,862 | 2% | 2,053 | 775,320 | refused: exit 249 (CLAIM_INVALID) |
| 27 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 107,796 → bridge Issue 28,406 | 136,202 | 3% | 2,053 | 1,106,040 | refused: exit 251 (NOT_REGISTERED) |
| 28 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 98,488 | 98,488 | 2% | 2,053 | 804,328 | refused: exit 252 (WRONG_DESTINATION) |
| 29 | Claim: under root(A, 3), never imported | bridge Claim 27,992 | 27,992 | 1% | 2,118 | 240,880 | refused: exit 245 (NOT_FOUND) |
| 30 | Mint sent directly to a holder | bridge Mint 12,111 | 12,111 | 0% | 624 | 101,880 | refused: exit 243 (WRONG_SENDER) |
| 31 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim 107,796 → bridge Issue 52,816 → bridge Admit 35,962 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 23,177 | 295,638 | 6% | 2,053 | 2,381,528 | applied |
| 32 | Claim: vault B's deposit under root(B, 8) | bridge Claim 107,796 → bridge Issue 48,645 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 261,772 | 5% | 2,053 | 2,110,600 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 44,890 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Init B (another configuration, vault B) | bridge Init | 44,270 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B (another configuration, vault B) | Attest | 84,699 | 1 | 1,595 | 1,092 | 365 | 2,058 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init | 44,890 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge (reply) | 32,295 | 1 | 898 | 370 | 390 | 0 | – | 246 |
| Init A again | bridge Init | 39,682 | 1 | 2,138 | 1,484 | 580 | 0 | – | 244 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register | 74,324 | 1 | 2,875 | 2,221 | 580 | 5,971 | 966 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register ETH (B) | bridge Register | 77,279 | 1 | 2,898 | 2,244 | 580 | 6,017 | 966 | 0 |
| Register ETH (B) | Attest | 156,184 | 1 | 2,559 | 2,056 | 365 | 3,373 | – | 0 |
| Register ETH (B) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (B) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 50,522 | 1 | 2,023 | 1,369 | 580 | 4,199 | 629 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,660 | 1 | 372 | 117 | 117 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 39,322 | 1 | 1,254 | 487 | 629 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 38,842 | 1 | 1,254 | 487 | 629 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 50,365 | 1 | 2,046 | 1,392 | 580 | 4,245 | 629 | 0 |
| ImportRoot A with vault B's proofs | Attest | 44,014 | 1 | 1,976 | 1,473 | 365 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 16,189 | 1 | 1,573 | 1,499 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 50,365 | 1 | 2,046 | 1,392 | 580 | 4,245 | 629 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 115,561 | 1 | 1,976 | 1,473 | 365 | 2,556 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ExtendFees | 42,881 | 1 | 1,248 | 258 | 852 | 2,313 | 1,040 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 50,613 | 1 | 2,073 | 1,419 | 580 | 4,295 | 618 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 40,597 | 1 | 1,248 | 70 | 1,040 | 2,313 | 1,040 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Issue | 52,567 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the same again (replay) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same again (replay) | bridge Issue | 52,567 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim: the same again (replay) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: the same again (replay) | bridge Spend | 20,152 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Issue | 52,567 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 20,152 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 32,106 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 94,862 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 94,862 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 94,862 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 94,862 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 94,862 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Issue | 28,406 | 1 | 1,377 | 199 | 1,040 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 98,488 | 1 | 1,462 | 1,271 | 117 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 27,992 | 1 | 1,345 | 1,271 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 12,111 | 1 | 235 | 161 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Issue | 52,816 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Issue | 48,645 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
