# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 44,051 → Attest 85,725 → bridge (reply) 39,242 | 169,018 | 3% | 1,946 | 1,367,712 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 43,431 → Attest 83,634 → bridge (reply) 39,242 | 166,307 | 3% | 1,882 | 1,345,512 | applied |
| 3 | Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init 44,051 → Attest 85,725 → bridge (reply) 32,395 | 162,171 | 3% | 1,946 | 1,312,936 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 38,446 | 38,446 | 1% | 1,946 | 323,136 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register 72,468 → Attest 150,201 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 327,760 | 7% | 2,747 | 2,644,056 | applied |
| 6 | Register ETH (B) | bridge Register 75,371 → Attest 155,163 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 335,625 | 7% | 2,770 | 2,707,160 | applied |
| 7 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 48,280 → Attest 108,849 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 226,181 | 5% | 1,895 | 1,824,608 | applied |
| 8 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 49,847 → Attest 112,749 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 231,648 | 5% | 2,025 | 1,869,384 | applied |
| 9 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 49,847 → Attest 112,749 → bridge (reply) 52,437 → bridge WriteRoot 18,479 | 233,512 | 5% | 2,025 | 1,884,296 | refused: exit 244 (ALREADY_WRITTEN) |
| 10 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 49,847 → Attest 112,749 → bridge (reply) 38,263 | 200,859 | 4% | 2,025 | 1,623,072 | refused: exit 247 (IMPORT_MISMATCH) |
| 11 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 49,847 → Attest 112,749 → bridge (reply) 37,783 | 200,379 | 4% | 2,025 | 1,619,232 | refused: exit 247 (IMPORT_MISMATCH) |
| 12 | ImportRoot A with vault B's proofs | bridge ImportRoot 48,083 → Attest 43,057 | 91,140 | 2% | 1,918 | 744,464 | refused: exit 110 (ProofNodeMismatch) |
| 13 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 15,404 | 15,404 | 0% | 2,090 | 139,952 | refused: exit 245 (NOT_FOUND) |
| 14 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 49,847 → Attest 112,749 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 231,648 | 5% | 2,025 | 1,869,384 | applied |
| 15 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 48,083 → Attest 114,375 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 231,510 | 5% | 1,918 | 1,867,424 | applied |
| 16 | ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees 54,391 → Attest 112,749 → bridge (reply) 57,003 → bridge ExtendFees 41,515 | 265,658 | 5% | 2,206 | 2,142,912 | applied |
| 17 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,630 → Attest 107,315 → bridge (reply) 51,342 → bridge ExtendFees 39,216 | 246,503 | 5% | 1,945 | 1,987,584 | applied |
| 18 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 107,610 → bridge Issue 52,329 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 14,616 | 245,325 | 5% | 2,053 | 1,979,024 | applied |
| 19 | Claim: the same again (replay) | bridge Claim 107,610 → bridge Issue 52,329 → bridge Admit 33,983 → bridge Spend 19,502 | 213,424 | 4% | 2,053 | 1,723,816 | refused: exit 250 (ALREADY_CLAIMED) |
| 20 | Claim: the same deposit under root(A, 8) | bridge Claim 107,610 → bridge Issue 52,329 → bridge Admit 33,983 → bridge Spend 19,502 | 213,424 | 4% | 2,053 | 1,723,816 | refused: exit 250 (ALREADY_CLAIMED) |
| 21 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 31,888 | 31,888 | 1% | 2,053 | 271,528 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | Claim: leaf 6, one sibling changed | bridge Claim 94,644 | 94,644 | 2% | 2,053 | 773,576 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 94,644 | 94,644 | 2% | 2,053 | 773,576 | refused: exit 249 (CLAIM_INVALID) |
| 24 | Claim: leaf 6's preimage at index 4 | bridge Claim 94,644 | 94,644 | 2% | 2,053 | 773,576 | refused: exit 249 (CLAIM_INVALID) |
| 25 | Claim: leaf 6 redirected to another destination | bridge Claim 94,644 | 94,644 | 2% | 2,053 | 773,576 | refused: exit 249 (CLAIM_INVALID) |
| 26 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 94,644 | 94,644 | 2% | 2,053 | 773,576 | refused: exit 249 (CLAIM_INVALID) |
| 27 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 107,610 → bridge Issue 29,208 | 136,818 | 3% | 2,053 | 1,110,968 | refused: exit 251 (NOT_REGISTERED) |
| 28 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 98,270 | 98,270 | 2% | 2,053 | 802,584 | refused: exit 252 (WRONG_DESTINATION) |
| 29 | Claim: under root(A, 3), never imported | bridge Claim 27,464 | 27,464 | 1% | 2,118 | 236,656 | refused: exit 245 (NOT_FOUND) |
| 30 | Mint sent directly to a holder | bridge Mint 10,496 | 10,496 | 0% | 527 | 88,184 | refused: exit 243 (WRONG_SENDER) |
| 31 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim 107,610 → bridge Issue 52,748 → bridge Admit 33,983 → bridge Spend 24,414 → bridge Mint 14,616 → bridge Mint 14,616 → bridge Mint 19,553 | 267,540 | 5% | 2,053 | 2,156,744 | applied |
| 32 | Claim: vault B's deposit under root(B, 8) | bridge Claim 107,610 → bridge Issue 48,456 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 19,837 → bridge Mint 19,837 | 251,894 | 5% | 2,053 | 2,031,576 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 44,051 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 39,242 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B (another configuration, vault B) | bridge Init | 43,431 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B (another configuration, vault B) | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 39,242 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init | 44,051 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge (reply) | 32,395 | 1 | 898 | 370 | 390 | 0 | – | 246 |
| Init A again | bridge Init | 38,446 | 1 | 2,078 | 1,484 | 520 | 0 | – | 244 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register | 72,468 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (B) | bridge Register | 75,371 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 48,280 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 49,847 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 49,847 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,479 | 1 | 372 | 117 | 117 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 49,847 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 38,263 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 49,847 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 37,783 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 48,083 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot A with vault B's proofs | Attest | 43,057 | 1 | 1,944 | 1,473 | 333 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 15,404 | 1 | 1,573 | 1,499 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 49,847 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 48,083 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees | 54,391 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge (reply) | 57,003 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ExtendFees | 41,515 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,630 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 39,216 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Issue | 52,329 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: the same again (replay) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same again (replay) | bridge Issue | 52,329 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the same again (replay) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: the same again (replay) | bridge Spend | 19,502 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Issue | 52,329 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 19,502 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 31,888 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 94,644 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 94,644 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 94,644 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 94,644 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 94,644 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Issue | 29,208 | 1 | 1,368 | 199 | 1,031 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 98,270 | 1 | 1,462 | 1,271 | 117 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 27,464 | 1 | 1,345 | 1,271 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 10,496 | 1 | 203 | 129 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Issue | 52,748 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Spend | 24,414 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 19,553 | 1 | 341 | 129 | 74 | 440 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Issue | 48,456 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 19,837 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 19,837 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
