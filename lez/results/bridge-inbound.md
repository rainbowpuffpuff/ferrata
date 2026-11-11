# The bridge program's inbound leg (`harness/tests/bridge_inbound.rs`)

Bridge program image `fe2f248724ddbeada455f2f132ae178d84434db3f26ddce50e06d72cc7ae946d`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A: vault A's slot 2 holds configId(A) | bridge Init 43,530 → Attest 85,725 → bridge (reply) 39,116 | 168,371 | 3% | 1,946 | 1,362,536 | applied |
| 2 | Init B (another configuration, vault B) | bridge Init 42,910 → Attest 83,634 → bridge (reply) 39,116 | 165,660 | 3% | 1,882 | 1,340,336 | applied |
| 3 | Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init 43,530 → Attest 85,725 → bridge (reply) 32,270 | 161,525 | 3% | 1,946 | 1,307,768 | refused: exit 246 (PAIRING_MISMATCH) |
| 4 | Init A again | bridge Init 37,929 | 37,929 | 1% | 1,946 | 319,000 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register 71,961 → Attest 150,201 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 324,257 | 6% | 2,747 | 2,616,032 | applied |
| 6 | Register ETH (B) | bridge Register 74,864 → Attest 155,163 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 332,122 | 7% | 2,770 | 2,679,136 | applied |
| 7 | ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot 47,970 → Attest 108,849 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 225,793 | 5% | 1,895 | 1,821,504 | applied |
| 8 | ImportRoot A: block 101 (8 leaves) | bridge ImportRoot 49,537 → Attest 112,749 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 231,260 | 5% | 2,025 | 1,866,280 | applied |
| 9 | ImportRoot A: block 101 again (same count) | bridge ImportRoot 49,537 → Attest 112,749 → bridge (reply) 52,749 → bridge WriteRoot 18,103 | 233,138 | 5% | 2,025 | 1,881,304 | refused: exit 244 (ALREADY_WRITTEN) |
| 10 | ImportRoot A: block 101, message claims count 7 | bridge ImportRoot 49,537 → Attest 112,749 → bridge (reply) 38,120 | 200,406 | 4% | 2,025 | 1,619,448 | refused: exit 247 (IMPORT_MISMATCH) |
| 11 | ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot 49,537 → Attest 112,749 → bridge (reply) 37,640 | 199,926 | 4% | 2,025 | 1,615,608 | refused: exit 247 (IMPORT_MISMATCH) |
| 12 | ImportRoot A with vault B's proofs | bridge ImportRoot 47,773 → Attest 43,057 | 90,830 | 2% | 1,918 | 741,984 | refused: exit 110 (ProofNodeMismatch) |
| 13 | ImportRoot A': no configuration (never initialised) | bridge ImportRoot 15,062 | 15,062 | 0% | 2,090 | 137,216 | refused: exit 245 (NOT_FOUND) |
| 14 | ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot 49,537 → Attest 112,749 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 231,260 | 5% | 2,025 | 1,866,280 | applied |
| 15 | ImportRoot B: block 100 (8 leaves) | bridge ImportRoot 47,773 → Attest 114,375 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 231,122 | 5% | 1,918 | 1,864,320 | applied |
| 16 | ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees 53,995 → Attest 112,749 → bridge (reply) 57,255 → bridge ExtendFees 38,021 | 262,020 | 5% | 2,206 | 2,113,808 | applied |
| 17 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,234 → Attest 107,315 → bridge (reply) 51,594 → bridge ExtendFees 36,021 | 243,164 | 5% | 1,945 | 1,960,872 | applied |
| 18 | Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim 107,397 → bridge Issue 47,013 → bridge Admit 34,187 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 17,155 | 252,941 | 5% | 2,061 | 2,040,016 | applied |
| 19 | Claim: the same again (replay) | bridge Claim 107,397 → bridge Issue 46,725 → bridge Admit 33,899 → bridge Spend 19,125 | 207,146 | 4% | 2,061 | 1,673,656 | refused: exit 250 (ALREADY_CLAIMED) |
| 20 | Claim: the same deposit under root(A, 8) | bridge Claim 107,397 → bridge Issue 46,725 → bridge Admit 33,899 → bridge Spend 19,125 | 207,146 | 4% | 2,061 | 1,673,656 | refused: exit 250 (ALREADY_CLAIMED) |
| 21 | Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim 31,658 | 31,658 | 1% | 2,061 | 269,752 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | Claim: leaf 6, one sibling changed | bridge Claim 94,414 | 94,414 | 2% | 2,061 | 771,800 | refused: exit 249 (CLAIM_INVALID) |
| 23 | Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim 94,414 | 94,414 | 2% | 2,061 | 771,800 | refused: exit 249 (CLAIM_INVALID) |
| 24 | Claim: leaf 6's preimage at index 4 | bridge Claim 94,414 | 94,414 | 2% | 2,061 | 771,800 | refused: exit 249 (CLAIM_INVALID) |
| 25 | Claim: leaf 6 redirected to another destination | bridge Claim 94,414 | 94,414 | 2% | 2,061 | 771,800 | refused: exit 249 (CLAIM_INVALID) |
| 26 | Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim 94,414 | 94,414 | 2% | 2,061 | 771,800 | refused: exit 249 (CLAIM_INVALID) |
| 27 | Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim 107,397 → bridge Issue 25,855 | 133,252 | 3% | 2,061 | 1,082,504 | refused: exit 251 (NOT_REGISTERED) |
| 28 | Claim: leaf 5 (private kind) by the plaintext path | bridge Claim 98,040 | 98,040 | 2% | 2,061 | 800,808 | refused: exit 252 (WRONG_DESTINATION) |
| 29 | Claim: under root(A, 3), never imported | bridge Claim 27,223 | 27,223 | 1% | 2,126 | 234,792 | refused: exit 245 (NOT_FOUND) |
| 30 | Mint sent directly to a holder | bridge Mint 10,676 | 10,676 | 0% | 624 | 90,400 | refused: exit 243 (WRONG_SENDER) |
| 31 | Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim 107,397 → bridge Issue 47,024 → bridge Admit 33,899 → bridge Spend 36,393 → bridge Mint 17,155 → bridge Mint 17,155 → bridge Mint 21,882 | 280,905 | 6% | 2,061 | 2,263,728 | applied |
| 32 | Claim: vault B's deposit under root(B, 8) | bridge Claim 107,397 → bridge Issue 44,167 → bridge Admit 34,187 → bridge Spend 30,034 → bridge Mint 22,170 → bridge Mint 22,170 | 260,125 | 5% | 2,061 | 2,097,488 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A: vault A's slot 2 holds configId(A) | bridge Init | 43,530 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A: vault A's slot 2 holds configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A: vault A's slot 2 holds configId(A) | bridge (reply) | 39,116 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B (another configuration, vault B) | bridge Init | 42,910 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B (another configuration, vault B) | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B (another configuration, vault B) | bridge (reply) | 39,116 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge Init | 43,530 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A' (vault A, another fee recipient): slot 2 is configId(A) | bridge (reply) | 32,270 | 1 | 898 | 370 | 390 | 0 | – | 246 |
| Init A again | bridge Init | 37,929 | 1 | 2,078 | 1,484 | 520 | 0 | – | 244 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge Register | 71,961 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (A): entry and schedule 0 attested at block 100 | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register ETH (B) | bridge Register | 74,864 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge ImportRoot | 47,970 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: slots 0 and 1 at block 100 (5 leaves) | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge ImportRoot | 49,537 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 (8 leaves) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 (8 leaves) | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A: block 101 again (same count) | bridge ImportRoot | 49,537 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101 again (same count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101 again (same count) | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 101 again (same count) | bridge WriteRoot | 18,103 | 1 | 372 | 117 | 117 | 0 | – | 244 |
| ImportRoot A: block 101, message claims count 7 | bridge ImportRoot | 49,537 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims count 7 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims count 7 | bridge (reply) | 38,120 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge ImportRoot | 49,537 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 101, message claims the 5-leaf root | bridge (reply) | 37,640 | 1 | 1,194 | 487 | 569 | 0 | – | 247 |
| ImportRoot A with vault B's proofs | bridge ImportRoot | 47,773 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot A with vault B's proofs | Attest | 43,057 | 1 | 1,944 | 1,473 | 333 | 0 | – | 110 |
| ImportRoot A': no configuration (never initialised) | bridge ImportRoot | 15,062 | 1 | 1,573 | 1,499 | 0 | 0 | – | 245 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge ImportRoot | 49,537 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A: block 102, root of 8 leaves, count 6 (a lagging count) | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge ImportRoot | 47,773 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B: block 100 (8 leaves) | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B: block 100 (8 leaves) | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ImportFees | 53,995 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge (reply) | 57,255 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A): schedule 1 at block 101 | bridge ExtendFees | 38,021 | 1 | 1,174 | 258 | 778 | 2,165 | 966 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,234 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 36,021 | 1 | 1,174 | 70 | 966 | 2,165 | 966 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Issue | 47,013 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Admit | 34,187 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the shared vector's deposit (leaf 2) under root(A, 5) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: the same again (replay) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: the same again (replay) | bridge Issue | 46,725 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim: the same again (replay) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: the same again (replay) | bridge Spend | 19,125 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: the same deposit under root(A, 8) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: the same deposit under root(A, 8) | bridge Issue | 46,725 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: the same deposit under root(A, 8) | bridge Spend | 19,125 | 1 | 437 | 222 | 77 | 0 | – | 250 |
| Claim: leaf 6 under root(A, 6) (block 102: index >= count) | bridge Claim | 31,658 | 1 | 1,470 | 1,279 | 117 | 0 | – | 248 |
| Claim: leaf 6, one sibling changed | bridge Claim | 94,414 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6 as 1 ETH instead of 10 (another leaf) | bridge Claim | 94,414 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6's preimage at index 4 | bridge Claim | 94,414 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6 redirected to another destination | bridge Claim | 94,414 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 6 under root(B, 8) (another configuration's vault) | bridge Claim | 94,414 | 1 | 1,470 | 1,279 | 117 | 0 | – | 249 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 7, 0.5 ETH (not a denomination) | bridge Issue | 25,855 | 1 | 1,311 | 207 | 966 | 0 | – | 251 |
| Claim: leaf 5 (private kind) by the plaintext path | bridge Claim | 98,040 | 1 | 1,470 | 1,279 | 117 | 0 | – | 252 |
| Claim: under root(A, 3), never imported | bridge Claim | 27,223 | 1 | 1,353 | 1,279 | 0 | 0 | – | 245 |
| Mint sent directly to a holder | bridge Mint | 10,676 | 1 | 235 | 161 | 0 | 0 | – | 243 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Issue | 47,024 | 1 | 1,311 | 207 | 966 | 2,843 | 966 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Admit | 33,899 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Spend | 36,393 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim: leaf 6, 10 ETH to the payer, under root(A, 8) (fee schedule 1) | bridge Mint | 21,882 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Issue | 44,167 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Admit | 34,187 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 22,170 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim: vault B's deposit under root(B, 8) | bridge Mint | 22,170 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
