# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `25dfd9b283ba14b0b83a522d7d307de57288c4730f538d8862b53ad73a4a8c49`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 39,167 → Attest 82,188 → bridge (reply) 33,174 | 154,529 | 3% | 1,726 | 1,250,040 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 37,653 → Attest 80,080 → bridge (reply) 33,174 | 150,907 | 3% | 1,662 | 1,220,552 | applied |
| 3 | Init A' (vault A, other denominations): slot 2 is configId(A) | bridge Init 39,167 → Attest 82,188 → bridge (reply) 28,289 | 149,644 | 3% | 1,726 | 1,210,960 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 30,282 | 30,282 | 1% | 1,661 | 255,544 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 39,151 → Attest 101,934 → bridge (reply) 44,676 → bridge WriteRoot 17,859 | 203,620 | 4% | 1,703 | 1,642,584 | applied |
| 6 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 39,151 → Attest 101,934 → bridge (reply) 44,676 → bridge WriteRoot 17,859 | 203,620 | 4% | 1,703 | 1,642,584 | applied |
| 7 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 39,151 → Attest 101,934 → bridge (reply) 44,676 → bridge WriteRoot 18,967 | 204,728 | 4% | 1,703 | 1,651,448 | refused: exit 244 (ALREADY_WRITTEN) |
| 8 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 39,151 → Attest 101,934 → bridge (reply) 32,255 | 173,340 | 3% | 1,703 | 1,400,344 | refused: exit 247 (IMPORT_MISMATCH) |
| 9 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 39,151 → Attest 101,934 → bridge (reply) 31,773 | 172,858 | 3% | 1,703 | 1,396,488 | refused: exit 247 (IMPORT_MISMATCH) |
| 10 | ImportRoot A with vault B's proofs | bridge ImportRoot 38,483 → Attest 41,559 | 80,042 | 2% | 1,639 | 653,448 | refused: exit 110 (ProofNodeMismatch) |
| 11 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 14,442 | 14,442 | 0% | 1,768 | 129,680 | refused: exit 245 (NOT_FOUND) |
| 12 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 39,151 → Attest 101,934 → bridge (reply) 44,676 → bridge WriteRoot 17,859 | 203,620 | 4% | 1,703 | 1,642,584 | applied |
| 13 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 38,483 → Attest 99,841 → bridge (reply) 44,676 → bridge WriteRoot 17,859 | 200,859 | 4% | 1,639 | 1,619,984 | applied |
| 14 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 109,283 → bridge Spend 17,756 → bridge Mint 15,405 | 142,444 | 3% | 1,740 | 1,153,472 | applied |
| 15 | Claim: the same again (replay) | bridge Claim 109,283 → bridge Spend 15,210 | 124,493 | 2% | 1,740 | 1,009,864 | refused: exit 250 (ALREADY_CLAIMED) |
| 16 | Claim: the same deposit under root(A, 8) | bridge Claim 109,283 → bridge Spend 15,210 | 124,493 | 2% | 1,740 | 1,009,864 | refused: exit 250 (ALREADY_CLAIMED) |
| 17 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 30,946 | 30,946 | 1% | 1,740 | 261,488 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 18 | Claim: leaf 6, one sibling changed | bridge Claim 93,788 | 93,788 | 2% | 1,740 | 764,224 | refused: exit 249 (CLAIM_INVALID) |
| 19 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 93,776 | 93,776 | 2% | 1,740 | 764,128 | refused: exit 249 (CLAIM_INVALID) |
| 20 | Claim: leaf 6's preimage at index 4 | bridge Claim 93,788 | 93,788 | 2% | 1,740 | 764,224 | refused: exit 249 (CLAIM_INVALID) |
| 21 | Claim: leaf 6 redirected to another destination | bridge Claim 93,788 | 93,788 | 2% | 1,740 | 764,224 | refused: exit 249 (CLAIM_INVALID) |
| 22 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 93,788 | 93,788 | 2% | 1,740 | 764,224 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 31,302 | 31,302 | 1% | 1,740 | 264,336 | refused: exit 251 (NOT_REGISTERED) |
| 24 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 97,336 | 97,336 | 2% | 1,740 | 792,608 | refused: exit 252 (WRONG_DESTINATION) |
| 25 | Claim: under root(A, 3), never imported | bridge Claim 26,640 | 26,640 | 1% | 1,805 | 227,560 | refused: exit 245 (NOT_FOUND) |
| 26 | Mint sent directly to a holder | bridge Mint 10,731 | 10,731 | 0% | 575 | 90,448 | refused: exit 243 (WRONG_SENDER) |
| 27 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) | bridge Claim 109,295 → bridge Spend 17,756 → bridge Mint 15,405 | 142,456 | 3% | 1,740 | 1,153,568 | applied |
| 28 | Claim: vault B's deposit under root(B, 8) | bridge Claim 109,283 → bridge Spend 17,756 → bridge Mint 20,581 | 147,620 | 3% | 1,740 | 1,194,880 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 39,167 | 1 | 1,273 | 1,199 | 0 | 2,695 | 266 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 82,188 | 1 | 1,530 | 1,059 | 333 | 1,993 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 33,174 | 1 | 774 | 370 | 266 | 1,058 | 259 | 0 |
| Init B (another configuration, vault B) | bridge Init | 37,653 | 1 | 1,209 | 1,135 | 0 | 2,567 | 266 | 0 |
| Init B (another configuration, vault B) | Attest | 80,080 | 1 | 1,466 | 995 | 333 | 1,929 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 33,174 | 1 | 774 | 370 | 266 | 1,058 | 259 | 0 |
| Init A' (vault A, other denominations): slot 2 is configId(A) | bridge Init | 39,167 | 1 | 1,273 | 1,199 | 0 | 2,695 | 266 | 0 |
| Init A' (vault A, other denominations): slot 2 is configId(A) | Attest | 82,188 | 1 | 1,530 | 1,059 | 333 | 1,993 | – | 0 |
| Init A' (vault A, other denominations): slot 2 is configId(A) | bridge (reply) | 28,289 | 1 | 774 | 370 | 266 | 0 | – | 246 |
| Init A again | bridge Init | 30,282 | 1 | 1,532 | 1,199 | 259 | 0 | – | 244 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 39,151 | 1 | 1,510 | 1,177 | 259 | 3,171 | 307 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 101,934 | 1 | 1,728 | 1,257 | 333 | 2,308 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 44,676 | 1 | 932 | 487 | 307 | 1,505 | 259 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 17,859 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 39,151 | 1 | 1,510 | 1,177 | 259 | 3,171 | 307 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 101,934 | 1 | 1,728 | 1,257 | 333 | 2,308 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 44,676 | 1 | 932 | 487 | 307 | 1,505 | 259 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 17,859 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 39,151 | 1 | 1,510 | 1,177 | 259 | 3,171 | 307 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 101,934 | 1 | 1,728 | 1,257 | 333 | 2,308 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 44,676 | 1 | 932 | 487 | 307 | 1,505 | 259 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,967 | 1 | 508 | 185 | 185 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 39,151 | 1 | 1,510 | 1,177 | 259 | 3,171 | 307 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 101,934 | 1 | 1,728 | 1,257 | 333 | 2,308 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 32,255 | 1 | 932 | 487 | 307 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 39,151 | 1 | 1,510 | 1,177 | 259 | 3,171 | 307 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 101,934 | 1 | 1,728 | 1,257 | 333 | 2,308 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 31,773 | 1 | 932 | 487 | 307 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 38,483 | 1 | 1,446 | 1,113 | 259 | 3,043 | 307 | 0 |
| ImportRoot A with vault B's proofs | Attest | 41,559 | 1 | 1,664 | 1,193 | 333 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 14,442 | 1 | 1,251 | 1,177 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 39,151 | 1 | 1,510 | 1,177 | 259 | 3,171 | 307 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 101,934 | 1 | 1,728 | 1,257 | 333 | 2,308 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 44,676 | 1 | 932 | 487 | 307 | 1,505 | 259 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 17,859 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 38,483 | 1 | 1,446 | 1,113 | 259 | 3,043 | 307 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 99,841 | 1 | 1,664 | 1,193 | 333 | 2,244 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 44,676 | 1 | 932 | 487 | 307 | 1,505 | 259 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 17,859 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 109,283 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 17,756 | 1 | 356 | 218 | 0 | 687 | 57 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 15,405 | 1 | 315 | 177 | 0 | 490 | 150 | 0 |
| Claim: the same again (replay) | bridge Claim | 109,283 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: the same again (replay) | bridge Spend | 15,210 | 1 | 413 | 218 | 57 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 109,283 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 15,210 | 1 | 413 | 218 | 57 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 30,946 | 1 | 1,473 | 1,214 | 185 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 93,788 | 1 | 1,473 | 1,214 | 185 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 93,776 | 1 | 1,473 | 1,214 | 185 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 93,788 | 1 | 1,473 | 1,214 | 185 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 93,788 | 1 | 1,473 | 1,214 | 185 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 93,788 | 1 | 1,473 | 1,214 | 185 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 31,302 | 1 | 1,473 | 1,214 | 185 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 97,336 | 1 | 1,473 | 1,214 | 185 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 26,640 | 1 | 1,288 | 1,214 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 10,731 | 1 | 251 | 177 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) | bridge Claim | 109,295 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) | bridge Spend | 17,756 | 1 | 356 | 218 | 0 | 687 | 57 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) | bridge Mint | 15,405 | 1 | 315 | 177 | 0 | 490 | 150 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 109,283 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 17,756 | 1 | 356 | 218 | 0 | 687 | 57 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 20,581 | 1 | 465 | 177 | 150 | 784 | 294 | 0 |
