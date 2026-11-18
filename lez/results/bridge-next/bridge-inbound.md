# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `7c7d2b3acc2f18e5714fa9fc543015bd4dadc5ce625d2b2bbdec50b56ab71e2f`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 44,818 → Attest 86,790 → bridge (reply) 39,419 | 171,027 | 3% | 1,946 | 1,383,784 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 44,198 → Attest 84,699 → bridge (reply) 39,419 | 168,316 | 3% | 1,882 | 1,361,584 | applied |
| 3 | Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init 44,818 → Attest 86,790 → bridge (reply) 32,198 | 163,806 | 3% | 1,946 | 1,326,016 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 39,401 | 39,401 | 1% | 1,946 | 330,776 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register 73,907 → Attest 151,273 → bridge (reply) 74,463 → bridge WriteToken 32,147 | 331,790 | 7% | 2,747 | 2,676,296 | applied |
| 6 | Register ETH (B) | bridge Register 76,851 → Attest 156,184 → bridge (reply) 74,463 → bridge WriteToken 32,147 | 339,645 | 7% | 2,770 | 2,739,320 | applied |
| 7 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 50,128 → Attest 109,870 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 230,731 | 5% | 1,895 | 1,861,008 | applied |
| 8 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 51,721 → Attest 113,770 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 236,224 | 5% | 2,025 | 1,905,992 | applied |
| 9 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 51,721 → Attest 113,770 → bridge (reply) 53,650 → bridge WriteRoot 18,567 | 237,708 | 5% | 2,025 | 1,917,864 | refused: exit 244 (ALREADY_WRITTEN) |
| 10 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 51,721 → Attest 113,770 → bridge (reply) 39,074 | 204,565 | 4% | 2,025 | 1,652,720 | refused: exit 247 (IMPORT_MISMATCH) |
| 11 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 51,721 → Attest 113,770 → bridge (reply) 38,594 | 204,085 | 4% | 2,025 | 1,648,880 | refused: exit 247 (IMPORT_MISMATCH) |
| 12 | ImportRoot A with vault B's proofs | bridge ImportRoot 49,908 → Attest 44,014 | 93,922 | 2% | 1,918 | 766,720 | refused: exit 110 (ProofNodeMismatch) |
| 13 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 16,143 | 16,143 | 0% | 2,090 | 145,864 | refused: exit 245 (NOT_FOUND) |
| 14 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 51,721 → Attest 113,770 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 236,224 | 5% | 2,025 | 1,905,992 | applied |
| 15 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 49,908 → Attest 115,561 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 236,202 | 5% | 1,918 | 1,904,960 | applied |
| 16 | ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees 55,929 → Attest 113,770 → bridge (reply) 58,259 → bridge ExtendFees 41,726 | 269,684 | 5% | 2,206 | 2,175,120 | applied |
| 17 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 50,198 → Attest 108,223 → bridge (reply) 52,487 → bridge ExtendFees 39,427 | 250,335 | 5% | 1,945 | 2,018,240 | applied |
| 18 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 107,702 → bridge Issue 52,214 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761 | 264,728 | 5% | 2,053 | 2,134,248 | applied |
| 19 | Claim: the same again (replay) | bridge Claim 107,702 → bridge Issue 52,214 → bridge Admit 35,565 → bridge Spend 20,082 | 215,563 | 4% | 2,053 | 1,740,928 | refused: exit 250 (ALREADY_CLAIMED) |
| 20 | Claim: the same deposit under root(A, 8) | bridge Claim 107,702 → bridge Issue 52,214 → bridge Admit 35,565 → bridge Spend 20,082 | 215,563 | 4% | 2,053 | 1,740,928 | refused: exit 250 (ALREADY_CLAIMED) |
| 21 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 32,012 | 32,012 | 1% | 2,053 | 272,520 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | Claim: leaf 6, one sibling changed | bridge Claim 94,768 | 94,768 | 2% | 2,053 | 774,568 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 94,768 | 94,768 | 2% | 2,053 | 774,568 | refused: exit 249 (CLAIM_INVALID) |
| 24 | Claim: leaf 6's preimage at index 4 | bridge Claim 94,768 | 94,768 | 2% | 2,053 | 774,568 | refused: exit 249 (CLAIM_INVALID) |
| 25 | Claim: leaf 6 redirected to another destination | bridge Claim 94,768 | 94,768 | 2% | 2,053 | 774,568 | refused: exit 249 (CLAIM_INVALID) |
| 26 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 94,768 | 94,768 | 2% | 2,053 | 774,568 | refused: exit 249 (CLAIM_INVALID) |
| 27 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 107,702 → bridge Issue 28,187 | 135,889 | 3% | 2,053 | 1,103,536 | refused: exit 251 (NOT_REGISTERED) |
| 28 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 98,394 | 98,394 | 2% | 2,053 | 803,576 | refused: exit 252 (WRONG_DESTINATION) |
| 29 | Claim: under root(A, 3), never imported | bridge Claim 27,943 | 27,943 | 1% | 2,118 | 240,488 | refused: exit 245 (NOT_FOUND) |
| 30 | Mint sent directly to a holder | bridge Mint 12,072 | 12,072 | 0% | 624 | 101,568 | refused: exit 243 (WRONG_SENDER) |
| 31 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim 107,702 → bridge Issue 52,634 → bridge Admit 35,565 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 23,096 | 294,762 | 6% | 2,053 | 2,374,520 | applied |
| 32 | Claim: vault B's deposit under root(B, 8) | bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761 | 260,855 | 5% | 2,053 | 2,103,264 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 44,818 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Init B (another configuration, vault B) | bridge Init | 44,198 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B (another configuration, vault B) | Attest | 84,699 | 1 | 1,595 | 1,092 | 365 | 2,058 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init | 44,818 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge (reply) | 32,198 | 1 | 898 | 370 | 390 | 0 | – | 246 |
| Init A again | bridge Init | 39,401 | 1 | 2,122 | 1,484 | 564 | 0 | – | 244 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register | 73,907 | 1 | 2,859 | 2,221 | 564 | 5,939 | 950 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge (reply) | 74,463 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (B) | bridge Register | 76,851 | 1 | 2,882 | 2,244 | 564 | 5,985 | 950 | 0 |
| Register ETH (B) | Attest | 156,184 | 1 | 2,559 | 2,056 | 365 | 3,373 | – | 0 |
| Register ETH (B) | bridge (reply) | 74,463 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (B) | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 50,128 | 1 | 2,007 | 1,369 | 564 | 4,167 | 613 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,567 | 1 | 372 | 117 | 117 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 39,074 | 1 | 1,238 | 487 | 613 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 38,594 | 1 | 1,238 | 487 | 613 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 49,908 | 1 | 2,030 | 1,392 | 564 | 4,213 | 613 | 0 |
| ImportRoot A with vault B's proofs | Attest | 44,014 | 1 | 1,976 | 1,473 | 365 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 16,143 | 1 | 1,573 | 1,499 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 49,908 | 1 | 2,030 | 1,392 | 564 | 4,213 | 613 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 115,561 | 1 | 1,976 | 1,473 | 365 | 2,556 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees | 55,929 | 1 | 2,318 | 1,680 | 564 | 4,785 | 790 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ExtendFees | 41,726 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 50,198 | 1 | 2,057 | 1,419 | 564 | 4,263 | 602 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 39,427 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Issue | 52,214 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 31,725 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the same again (replay) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same again (replay) | bridge Issue | 52,214 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the same again (replay) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: the same again (replay) | bridge Spend | 20,082 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Issue | 52,214 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 20,082 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 32,012 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 94,768 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 94,768 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 94,768 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 94,768 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 94,768 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Issue | 28,187 | 1 | 1,368 | 199 | 1,031 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 98,394 | 1 | 1,462 | 1,271 | 117 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 27,943 | 1 | 1,345 | 1,271 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 12,072 | 1 | 235 | 161 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Issue | 52,634 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Spend | 38,243 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 23,096 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Issue | 48,341 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 31,725 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
