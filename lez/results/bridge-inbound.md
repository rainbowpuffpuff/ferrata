# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 43,907 → Attest 85,725 → bridge (reply) 39,144 | 168,776 | 3% | 1,946 | 1,365,776 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 43,287 → Attest 83,634 → bridge (reply) 39,144 | 166,065 | 3% | 1,882 | 1,343,576 | applied |
| 3 | Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init 43,907 → Attest 85,725 → bridge (reply) 32,297 | 161,929 | 3% | 1,946 | 1,311,000 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 38,268 | 38,268 | 1% | 1,946 | 321,712 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register 72,339 → Attest 150,201 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 327,269 | 7% | 2,747 | 2,640,128 | applied |
| 6 | Register ETH (B) | bridge Register 75,242 → Attest 155,163 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 335,134 | 7% | 2,770 | 2,703,232 | applied |
| 7 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 48,295 → Attest 108,849 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 225,898 | 5% | 1,895 | 1,822,344 | applied |
| 8 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 49,862 → Attest 112,749 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 231,365 | 5% | 2,025 | 1,867,120 | applied |
| 9 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 49,862 → Attest 112,749 → bridge (reply) 52,241 → bridge WriteRoot 18,377 | 233,229 | 5% | 2,025 | 1,882,032 | refused: exit 244 (ALREADY_WRITTEN) |
| 10 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 49,862 → Attest 112,749 → bridge (reply) 38,165 | 200,776 | 4% | 2,025 | 1,622,408 | refused: exit 247 (IMPORT_MISMATCH) |
| 11 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 49,862 → Attest 112,749 → bridge (reply) 37,685 | 200,296 | 4% | 2,025 | 1,618,568 | refused: exit 247 (IMPORT_MISMATCH) |
| 12 | ImportRoot A with vault B's proofs | bridge ImportRoot 48,098 → Attest 43,057 | 91,155 | 2% | 1,918 | 744,584 | refused: exit 110 (ProofNodeMismatch) |
| 13 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 15,356 | 15,356 | 0% | 2,090 | 139,568 | refused: exit 245 (NOT_FOUND) |
| 14 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 49,862 → Attest 112,749 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 231,365 | 5% | 2,025 | 1,867,120 | applied |
| 15 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 48,098 → Attest 114,375 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 231,227 | 5% | 1,918 | 1,865,160 | applied |
| 16 | ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees 54,297 → Attest 112,749 → bridge (reply) 56,755 → bridge ExtendFees 41,418 | 265,219 | 5% | 2,206 | 2,139,400 | applied |
| 17 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,536 → Attest 107,315 → bridge (reply) 51,094 → bridge ExtendFees 39,119 | 246,064 | 5% | 1,945 | 1,984,072 | applied |
| 18 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 107,479 → bridge Issue 52,100 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 17,560 | 258,885 | 5% | 2,053 | 2,087,504 | applied |
| 19 | Claim: the same again (replay) | bridge Claim 107,479 → bridge Issue 52,100 → bridge Admit 33,886 → bridge Spend 19,394 | 212,859 | 4% | 2,053 | 1,719,296 | refused: exit 250 (ALREADY_CLAIMED) |
| 20 | Claim: the same deposit under root(A, 8) | bridge Claim 107,479 → bridge Issue 52,100 → bridge Admit 33,886 → bridge Spend 19,394 | 212,859 | 4% | 2,053 | 1,719,296 | refused: exit 250 (ALREADY_CLAIMED) |
| 21 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 31,806 | 31,806 | 1% | 2,053 | 270,872 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | Claim: leaf 6, one sibling changed | bridge Claim 94,562 | 94,562 | 2% | 2,053 | 772,920 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 94,562 | 94,562 | 2% | 2,053 | 772,920 | refused: exit 249 (CLAIM_INVALID) |
| 24 | Claim: leaf 6's preimage at index 4 | bridge Claim 94,562 | 94,562 | 2% | 2,053 | 772,920 | refused: exit 249 (CLAIM_INVALID) |
| 25 | Claim: leaf 6 redirected to another destination | bridge Claim 94,562 | 94,562 | 2% | 2,053 | 772,920 | refused: exit 249 (CLAIM_INVALID) |
| 26 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 94,562 | 94,562 | 2% | 2,053 | 772,920 | refused: exit 249 (CLAIM_INVALID) |
| 27 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 107,479 → bridge Issue 29,174 | 136,653 | 3% | 2,053 | 1,109,648 | refused: exit 251 (NOT_REGISTERED) |
| 28 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 98,188 | 98,188 | 2% | 2,053 | 801,928 | refused: exit 252 (WRONG_DESTINATION) |
| 29 | Claim: under root(A, 3), never imported | bridge Claim 27,382 | 27,382 | 1% | 2,118 | 236,000 | refused: exit 245 (NOT_FOUND) |
| 30 | Mint sent directly to a holder | bridge Mint 10,959 | 10,959 | 0% | 624 | 92,664 | refused: exit 243 (WRONG_SENDER) |
| 31 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim 107,479 → bridge Issue 52,519 → bridge Admit 33,886 → bridge Spend 36,659 → bridge Mint 17,560 → bridge Mint 17,560 → bridge Mint 22,280 | 287,943 | 6% | 2,053 | 2,319,968 | applied |
| 32 | Claim: vault B's deposit under root(B, 8) | bridge Claim 107,479 → bridge Issue 48,227 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 22,568 → bridge Mint 22,568 | 265,028 | 5% | 2,053 | 2,136,648 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 43,907 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 39,144 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B (another configuration, vault B) | bridge Init | 43,287 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B (another configuration, vault B) | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 39,144 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init | 43,907 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge (reply) | 32,297 | 1 | 898 | 370 | 390 | 0 | – | 246 |
| Init A again | bridge Init | 38,268 | 1 | 2,078 | 1,484 | 520 | 0 | – | 244 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register | 72,339 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (B) | bridge Register | 75,242 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 48,295 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 49,862 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 49,862 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,377 | 1 | 372 | 117 | 117 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 49,862 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 38,165 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 49,862 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 37,685 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 48,098 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot A with vault B's proofs | Attest | 43,057 | 1 | 1,944 | 1,473 | 333 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 15,356 | 1 | 1,573 | 1,499 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 49,862 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 48,098 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees | 54,297 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge (reply) | 56,755 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ExtendFees | 41,418 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,536 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 39,119 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Issue | 52,100 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the same again (replay) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same again (replay) | bridge Issue | 52,100 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the same again (replay) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: the same again (replay) | bridge Spend | 19,394 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Issue | 52,100 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 19,394 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 31,806 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 94,562 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 94,562 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 94,562 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 94,562 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 94,562 | 1 | 1,462 | 1,271 | 117 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Issue | 29,174 | 1 | 1,368 | 199 | 1,031 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 98,188 | 1 | 1,462 | 1,271 | 117 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 27,382 | 1 | 1,345 | 1,271 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 10,959 | 1 | 235 | 161 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Issue | 52,519 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Spend | 36,659 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 22,280 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Issue | 48,227 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 22,568 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 22,568 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
