# The inbound finality depth (`harness/tests/bridge_depth.rs`, RFP-022 F.12)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; LEZ v0.4.0-rc1's state machine, every transaction settled in a block of its own (RISC Zero dev mode). Depth 0 (the default) is Ethereum finality; the admin's depth is in beacon slots beyond it, measured against the newest record the configuration's clock read.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,890 → Attest 86,790 → bridge (reply) 39,738 | 171,418 | 3% | 1,946 | 1,386,912 | applied |
| 2 | Register ETH (block 100) | bridge Register 74,324 → Attest 151,273 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 333,303 | 7% | 2,747 | 2,688,400 | applied |
| 3 | ImportRoot root(A, 5) under block 100 (slot 9,000,000), depth 0 | bridge ImportRoot 50,522 → Attest 109,870 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 231,635 | 5% | 1,895 | 1,868,240 | applied |
| 4 | SetDepth 65 by the freeze authority | bridge SetDepth 12,913 → bridge SetDepth 25,311 | 38,224 | 1% | 679 | 311,224 | refused: exit 253 (NOT_AUTHORIZED) |
| 5 | SetDepth 65 by an outsider | bridge SetDepth 12,913 → bridge SetDepth 25,089 | 38,002 | 1% | 679 | 309,448 | refused: exit 253 (NOT_AUTHORIZED) |
| 6 | SetDepth 216,001 by the admin | bridge SetDepth 12,913 → bridge SetDepth 25,319 | 38,232 | 1% | 679 | 311,288 | refused: exit 240 (BAD_MESSAGE) |
| 7 | SetDepth 65 by the admin | bridge SetDepth 12,913 → bridge SetDepth 32,060 | 44,973 | 1% | 679 | 365,216 | applied |
| 8 | ImportRoot root(A, 8) under block 101 (slot 9,000,032), depth 65, no record read | bridge ImportRoot 28,380 | 28,380 | 1% | 2,025 | 243,240 | refused: exit 242 (FROZEN) |
| 9 | Tick A: block 102 (slot 9,000,064) | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 10 | ImportRoot root(A, 8) under block 101: 32 slots behind the tip, 65 needed | bridge ImportRoot 28,380 | 28,380 | 1% | 2,025 | 243,240 | refused: exit 242 (FROZEN) |
| 11 | Tick A: a record at slot 9,000,096 | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |
| 12 | ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 65 needed (one short) | bridge ImportRoot 28,380 | 28,380 | 1% | 2,025 | 243,240 | refused: exit 242 (FROZEN) |
| 13 | SetDepth 64 by the admin | bridge SetDepth 12,913 → bridge SetDepth 32,060 | 44,973 | 1% | 679 | 365,216 | applied |
| 14 | ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed | bridge ImportRoot 52,138 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,151 | 5% | 2,025 | 1,913,408 | applied |
| 15 | ImportFees ETH schedule 1 (block 101) | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 42,881 | 271,623 | 5% | 2,206 | 2,190,632 | applied |
| 16 | ImportFees ETH, no schedule 2 (block 101) | bridge ImportFees 50,613 → Attest 108,223 → bridge (reply) 52,960 → bridge ExtendFees 40,597 | 252,393 | 5% | 1,945 | 2,034,704 | applied |
| 17 | SetDepth 216,000 (the maximum) by the admin | bridge SetDepth 12,913 → bridge SetDepth 32,060 | 44,973 | 1% | 679 | 365,216 | applied |
| 18 | Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Claim 107,796 → bridge Issue 52,567 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 265,693 | 5% | 2,053 | 2,141,968 | applied |
| 19 | Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Claim 107,796 → bridge Issue 52,816 → bridge Admit 35,962 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 23,177 | 295,638 | 6% | 2,053 | 2,381,528 | applied |
| 20 | ImportRoot under block 102 (slot 9,000,064) at depth 216,000 | bridge ImportRoot 28,380 | 28,380 | 1% | 2,025 | 243,240 | refused: exit 242 (FROZEN) |
| 21 | SetDepth 0 by the admin | bridge SetDepth 12,913 → bridge SetDepth 32,060 | 44,973 | 1% | 679 | 365,216 | applied |
| 22 | the same ImportRoot at depth 0 | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,140 | 5% | 2,025 | 1,913,320 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 44,890 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Register ETH (block 100) | bridge Register | 74,324 | 1 | 2,875 | 2,221 | 580 | 5,971 | 966 | 0 |
| Register ETH (block 100) | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (block 100) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (block 100) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| ImportRoot root(A, 5) under block 100 (slot 9,000,000), depth 0 | bridge ImportRoot | 50,522 | 1 | 2,023 | 1,369 | 580 | 4,199 | 629 | 0 |
| ImportRoot root(A, 5) under block 100 (slot 9,000,000), depth 0 | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot root(A, 5) under block 100 (slot 9,000,000), depth 0 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot root(A, 5) under block 100 (slot 9,000,000), depth 0 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| SetDepth 65 by the freeze authority | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 65 by the freeze authority | bridge SetDepth | 25,311 | 1 | 759 | 41 | 580 | 0 | – | 253 |
| SetDepth 65 by an outsider | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 65 by an outsider | bridge SetDepth | 25,089 | 1 | 759 | 41 | 580 | 0 | – | 253 |
| SetDepth 216,001 by the admin | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 216,001 by the admin | bridge SetDepth | 25,319 | 1 | 759 | 41 | 580 | 0 | – | 240 |
| SetDepth 65 by the admin | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 65 by the admin | bridge SetDepth | 32,060 | 1 | 759 | 41 | 580 | 1,364 | 580 | 0 |
| ImportRoot root(A, 8) under block 101 (slot 9,000,032), depth 65, no record read | bridge ImportRoot | 28,380 | 1 | 2,153 | 1,499 | 580 | 0 | – | 242 |
| Tick A: block 102 (slot 9,000,064) | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: block 102 (slot 9,000,064) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: block 102 (slot 9,000,064) | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| ImportRoot root(A, 8) under block 101: 32 slots behind the tip, 65 needed | bridge ImportRoot | 28,380 | 1 | 2,153 | 1,499 | 580 | 0 | – | 242 |
| Tick A: a record at slot 9,000,096 | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: a record at slot 9,000,096 | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: a record at slot 9,000,096 | bridge (reply) | 34,214 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 65 needed (one short) | bridge ImportRoot | 28,380 | 1 | 2,153 | 1,499 | 580 | 0 | – | 242 |
| SetDepth 64 by the admin | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 64 by the admin | bridge SetDepth | 32,060 | 1 | 759 | 41 | 580 | 1,364 | 580 | 0 |
| ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed | bridge ImportRoot | 52,138 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH schedule 1 (block 101) | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees ETH schedule 1 (block 101) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH schedule 1 (block 101) | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH schedule 1 (block 101) | bridge ExtendFees | 42,881 | 1 | 1,248 | 258 | 852 | 2,313 | 1,040 | 0 |
| ImportFees ETH, no schedule 2 (block 101) | bridge ImportFees | 50,613 | 1 | 2,073 | 1,419 | 580 | 4,295 | 618 | 0 |
| ImportFees ETH, no schedule 2 (block 101) | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH, no schedule 2 (block 101) | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH, no schedule 2 (block 101) | bridge ExtendFees | 40,597 | 1 | 1,248 | 70 | 1,040 | 2,313 | 1,040 | 0 |
| SetDepth 216,000 (the maximum) by the admin | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 216,000 (the maximum) by the admin | bridge SetDepth | 32,060 | 1 | 759 | 41 | 580 | 1,364 | 580 | 0 |
| Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Issue | 52,567 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000 | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Issue | 52,816 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000 | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| ImportRoot under block 102 (slot 9,000,064) at depth 216,000 | bridge ImportRoot | 28,380 | 1 | 2,153 | 1,499 | 580 | 0 | – | 242 |
| SetDepth 0 by the admin | bridge SetDepth | 12,913 | 1 | 115 | 41 | 0 | 249 | – | 0 |
| SetDepth 0 by the admin | bridge SetDepth | 32,060 | 1 | 759 | 41 | 580 | 1,364 | 580 | 0 |
| the same ImportRoot at depth 0 | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| the same ImportRoot at depth 0 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| the same ImportRoot at depth 0 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| the same ImportRoot at depth 0 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
