# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `f11b1aba55fde1ea977f74a54cc927ff50253e8bf479c89dac0b3658177c58eb`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 43,668 → Attest 85,725 → bridge (reply) 39,214 | 168,607 | 3% | 1,946 | 1,364,424 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 43,048 → Attest 83,634 → bridge (reply) 39,214 | 165,896 | 3% | 1,882 | 1,342,224 | applied |
| 3 | Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init 43,668 → Attest 85,725 → bridge (reply) 32,368 | 161,761 | 3% | 1,946 | 1,309,656 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 38,101 | 38,101 | 1% | 1,946 | 320,376 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register 72,084 → Attest 150,208 → bridge (reply) 73,421 → bridge WriteToken 29,035 | 324,748 | 6% | 2,747 | 2,619,960 | applied |
| 6 | Register ETH (B) | bridge Register 74,987 → Attest 155,163 → bridge (reply) 73,421 → bridge WriteToken 29,035 | 332,606 | 7% | 2,770 | 2,683,008 | applied |
| 7 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 47,969 → Attest 108,849 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 226,091 | 5% | 1,895 | 1,823,888 | applied |
| 8 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 49,536 → Attest 112,749 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 231,558 | 5% | 2,025 | 1,868,664 | applied |
| 9 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 49,536 → Attest 112,749 → bridge (reply) 52,945 → bridge WriteRoot 18,206 | 233,436 | 5% | 2,025 | 1,883,688 | refused: exit 244 (ALREADY_WRITTEN) |
| 10 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 49,536 → Attest 112,749 → bridge (reply) 38,218 | 200,503 | 4% | 2,025 | 1,620,224 | refused: exit 247 (IMPORT_MISMATCH) |
| 11 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 49,536 → Attest 112,749 → bridge (reply) 37,738 | 200,023 | 4% | 2,025 | 1,616,384 | refused: exit 247 (IMPORT_MISMATCH) |
| 12 | ImportRoot A with vault B's proofs | bridge ImportRoot 47,772 → Attest 43,057 | 90,829 | 2% | 1,918 | 741,976 | refused: exit 110 (ProofNodeMismatch) |
| 13 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 15,124 | 15,124 | 0% | 2,090 | 137,712 | refused: exit 245 (NOT_FOUND) |
| 14 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 49,536 → Attest 112,749 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 231,558 | 5% | 2,025 | 1,868,664 | applied |
| 15 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 47,772 → Attest 114,375 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 231,420 | 5% | 1,918 | 1,866,704 | applied |
| 16 | ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees 54,087 → Attest 112,749 → bridge (reply) 57,503 → bridge ExtendFees 38,134 | 262,473 | 5% | 2,206 | 2,117,432 | applied |
| 17 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,326 → Attest 107,315 → bridge (reply) 51,842 → bridge ExtendFees 36,134 | 243,617 | 5% | 1,945 | 1,964,496 | applied |
| 18 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 107,526 → bridge Issue 47,239 → bridge Admit 34,291 → bridge Spend 21,901 → bridge Mint 14,237 → bridge Mint 14,237 | 239,431 | 5% | 2,061 | 1,931,936 | applied |
| 19 | Claim: the same again (replay) | bridge Claim 107,526 → bridge Issue 46,951 → bridge Admit 34,003 → bridge Spend 19,227 | 207,707 | 4% | 2,061 | 1,678,144 | refused: exit 250 (ALREADY_CLAIMED) |
| 20 | Claim: the same deposit under root(A, 8) | bridge Claim 107,526 → bridge Issue 46,951 → bridge Admit 34,003 → bridge Spend 19,227 | 207,707 | 4% | 2,061 | 1,678,144 | refused: exit 250 (ALREADY_CLAIMED) |
| 21 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 31,738 | 31,738 | 1% | 2,061 | 270,392 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | Claim: leaf 6, one sibling changed | bridge Claim 94,494 | 94,494 | 2% | 2,061 | 772,440 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 94,494 | 94,494 | 2% | 2,061 | 772,440 | refused: exit 249 (CLAIM_INVALID) |
| 24 | Claim: leaf 6's preimage at index 4 | bridge Claim 94,494 | 94,494 | 2% | 2,061 | 772,440 | refused: exit 249 (CLAIM_INVALID) |
| 25 | Claim: leaf 6 redirected to another destination | bridge Claim 94,494 | 94,494 | 2% | 2,061 | 772,440 | refused: exit 249 (CLAIM_INVALID) |
| 26 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 94,494 | 94,494 | 2% | 2,061 | 772,440 | refused: exit 249 (CLAIM_INVALID) |
| 27 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 107,526 → bridge Issue 25,886 | 133,412 | 3% | 2,061 | 1,083,784 | refused: exit 251 (NOT_REGISTERED) |
| 28 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 98,120 | 98,120 | 2% | 2,061 | 801,448 | refused: exit 252 (WRONG_DESTINATION) |
| 29 | Claim: under root(A, 3), never imported | bridge Claim 27,303 | 27,303 | 1% | 2,126 | 235,432 | refused: exit 245 (NOT_FOUND) |
| 30 | Mint sent directly to a holder | bridge Mint 10,237 | 10,237 | 0% | 527 | 86,112 | refused: exit 243 (WRONG_SENDER) |
| 31 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim 107,526 → bridge Issue 47,250 → bridge Admit 34,003 → bridge Spend 24,145 → bridge Mint 14,237 → bridge Mint 14,237 → bridge Mint 19,181 | 260,579 | 5% | 2,061 | 2,101,120 | applied |
| 32 | Claim: vault B's deposit under root(B, 8) | bridge Claim 107,526 → bridge Issue 44,393 → bridge Admit 34,291 → bridge Spend 21,901 → bridge Mint 19,465 → bridge Mint 19,465 | 247,041 | 5% | 2,061 | 1,992,816 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 43,668 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 39,214 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B (another configuration, vault B) | bridge Init | 43,048 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B (another configuration, vault B) | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 39,214 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init | 43,668 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge (reply) | 32,368 | 1 | 898 | 370 | 390 | 0 | – | 246 |
| Init A again | bridge Init | 38,101 | 1 | 2,078 | 1,484 | 520 | 0 | – | 244 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register | 72,084 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | Attest | 150,208 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge (reply) | 73,421 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge WriteToken | 29,035 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register ETH (B) | bridge Register | 74,987 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,421 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 29,035 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 47,969 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 49,536 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 49,536 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,206 | 1 | 372 | 117 | 117 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 49,536 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 38,218 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 49,536 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 37,738 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 47,772 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot A with vault B's proofs | Attest | 43,057 | 1 | 1,944 | 1,473 | 333 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 15,124 | 1 | 1,573 | 1,499 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 49,536 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 47,772 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees | 54,087 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge (reply) | 57,503 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ExtendFees | 38,134 | 1 | 1,174 | 258 | 778 | 2,165 | 966 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,326 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,842 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 36,134 | 1 | 1,174 | 70 | 966 | 2,165 | 966 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Issue | 47,239 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Admit | 34,291 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 21,901 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: the same again (replay) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: the same again (replay) | bridge Issue | 46,951 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim: the same again (replay) | bridge Admit | 34,003 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: the same again (replay) | bridge Spend | 19,227 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Issue | 46,951 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Admit | 34,003 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 19,227 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 31,738 | 1 | 1,470 | 1,279 | 117 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 94,494 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 94,494 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 94,494 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 94,494 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 94,494 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Issue | 25,886 | 1 | 1,311 | 207 | 966 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 98,120 | 1 | 1,470 | 1,279 | 117 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 27,303 | 1 | 1,353 | 1,279 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 10,237 | 1 | 203 | 129 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Issue | 47,250 | 1 | 1,311 | 207 | 966 | 2,843 | 966 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Admit | 34,003 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Spend | 24,145 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 19,181 | 1 | 341 | 129 | 74 | 440 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Issue | 44,393 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Admit | 34,291 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 21,901 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 19,465 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 19,465 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
