# Delisting on LEZ (`harness/tests/bridge_delist.rs`, RFP-022 F.10)

Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`; LEZ v0.4.0-rc1's state machine, every transaction settled in a block of its own (RISC Zero dev mode). The vault's listing slot of ETH is placed in synthetic blocks of vault A's storage, as `FerrataVault.setDepositsEnabled` writes it (`vault/test/Admin.t.sol`).

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,890 → Attest 86,790 → bridge (reply) 39,738 | 171,418 | 3% | 1,946 | 1,386,912 | applied |
| 2 | Register ETH (block 100) | bridge Register 74,324 → Attest 151,273 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 333,303 | 7% | 2,747 | 2,688,400 | applied |
| 3 | ImportRoot root(A, 8) (block 101) | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,140 | 5% | 2,025 | 1,913,320 | applied |
| 4 | ImportFees ETH schedule 1 | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 42,881 | 271,623 | 5% | 2,206 | 2,190,632 | applied |
| 5 | ImportFees ETH, no schedule 2 | bridge ImportFees 50,613 → Attest 108,223 → bridge (reply) 52,960 → bridge ExtendFees 40,597 | 252,393 | 5% | 1,945 | 2,034,704 | applied |
| 6 | Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Claim 107,796 → bridge Issue 52,816 → bridge Admit 35,962 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 18,800 | 291,261 | 6% | 2,053 | 2,346,512 | applied |
| 7 | AttestListing ETH: the vault's slot holds 1 | bridge AttestListing 46,567 → Attest 85,880 → bridge (reply) 50,702 → bridge SetListing 40,383 | 223,532 | 4% | 1,523 | 1,800,440 | applied |
| 8 | AttestListing ETH under block 101 (older, slot unwritten) | bridge AttestListing 47,542 → Attest 83,130 → bridge (reply) 50,704 → bridge SetListing 28,464 | 209,840 | 4% | 1,613 | 1,691,624 | refused: exit 247 (IMPORT_MISMATCH) |
| 9 | AttestListing ETH under the same record again | bridge AttestListing 46,567 → Attest 85,880 → bridge (reply) 50,702 → bridge SetListing 28,463 | 211,612 | 4% | 1,523 | 1,705,080 | refused: exit 247 (IMPORT_MISMATCH) |
| 10 | Claim of leaf 2 (1 ETH), after the delisting | bridge Claim 107,796 → bridge Issue 52,566 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 23,177 | 270,070 | 5% | 2,053 | 2,176,984 | applied |
| 11 | Burn of 1 ETH by the payer, after the delisting | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,253 → bridge Admit 35,963 → bridge Burn 19,498 | 156,677 | 3% | 839 | 1,260,128 | applied |
| 12 | Transfer of 1 ETH from the payer, after the delisting | bridge Transfer 17,778 → bridge Transfer 23,563 → bridge Transfer 37,060 → bridge Credit 21,476 | 99,877 | 2% | 691 | 804,544 | applied |
| 13 | AttestListing ETH: the slot holds 2 | bridge AttestListing 46,567 → Attest 85,880 → bridge (reply) 37,063 | 169,510 | 3% | 1,523 | 1,368,264 | refused: exit 247 (IMPORT_MISMATCH) |
| 14 | AttestListing ETH: relisted (the slot holds 0) | bridge AttestListing 45,068 → Attest 78,910 → bridge (reply) 50,704 → bridge SetListing 40,383 | 215,065 | 4% | 1,452 | 1,732,136 | applied |
| 15 | AttestListing ETH under the delisting's record (now older) | bridge AttestListing 46,567 → Attest 85,880 → bridge (reply) 50,702 → bridge SetListing 28,464 | 211,613 | 4% | 1,523 | 1,705,088 | refused: exit 247 (IMPORT_MISMATCH) |

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
| ImportRoot root(A, 8) (block 101) | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot root(A, 8) (block 101) | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot root(A, 8) (block 101) | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot root(A, 8) (block 101) | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH schedule 1 | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees ETH schedule 1 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH schedule 1 | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH schedule 1 | bridge ExtendFees | 42,881 | 1 | 1,248 | 258 | 852 | 2,313 | 1,040 | 0 |
| ImportFees ETH, no schedule 2 | bridge ImportFees | 50,613 | 1 | 2,073 | 1,419 | 580 | 4,295 | 618 | 0 |
| ImportFees ETH, no schedule 2 | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH, no schedule 2 | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH, no schedule 2 | bridge ExtendFees | 40,597 | 1 | 1,248 | 70 | 1,040 | 2,313 | 1,040 | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Issue | 52,816 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 6 (10 ETH to the payer), before the delisting | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| AttestListing ETH: the vault's slot holds 1 | bridge AttestListing | 46,567 | 1 | 1,651 | 997 | 580 | 3,415 | 609 | 0 |
| AttestListing ETH: the vault's slot holds 1 | Attest | 85,880 | 1 | 1,561 | 1,058 | 365 | 2,024 | – | 0 |
| AttestListing ETH: the vault's slot holds 1 | bridge (reply) | 50,702 | 1 | 1,117 | 370 | 609 | 1,888 | 580 | 0 |
| AttestListing ETH: the vault's slot holds 1 | bridge SetListing | 40,383 | 1 | 1,240 | 62 | 1,040 | 2,305 | 1,040 | 0 |
| AttestListing ETH under block 101 (older, slot unwritten) | bridge AttestListing | 47,542 | 1 | 1,741 | 1,087 | 580 | 3,595 | 609 | 0 |
| AttestListing ETH under block 101 (older, slot unwritten) | Attest | 83,130 | 1 | 1,651 | 1,148 | 365 | 2,114 | – | 0 |
| AttestListing ETH under block 101 (older, slot unwritten) | bridge (reply) | 50,704 | 1 | 1,117 | 370 | 609 | 1,888 | 580 | 0 |
| AttestListing ETH under block 101 (older, slot unwritten) | bridge SetListing | 28,464 | 1 | 1,240 | 62 | 1,040 | 0 | – | 247 |
| AttestListing ETH under the same record again | bridge AttestListing | 46,567 | 1 | 1,651 | 997 | 580 | 3,415 | 609 | 0 |
| AttestListing ETH under the same record again | Attest | 85,880 | 1 | 1,561 | 1,058 | 365 | 2,024 | – | 0 |
| AttestListing ETH under the same record again | bridge (reply) | 50,702 | 1 | 1,117 | 370 | 609 | 1,888 | 580 | 0 |
| AttestListing ETH under the same record again | bridge SetListing | 28,463 | 1 | 1,240 | 62 | 1,040 | 0 | – | 247 |
| Claim of leaf 2 (1 ETH), after the delisting | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim of leaf 2 (1 ETH), after the delisting | bridge Issue | 52,566 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim of leaf 2 (1 ETH), after the delisting | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim of leaf 2 (1 ETH), after the delisting | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim of leaf 2 (1 ETH), after the delisting | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim of leaf 2 (1 ETH), after the delisting | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Burn of 1 ETH by the payer, after the delisting | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn of 1 ETH by the payer, after the delisting | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn of 1 ETH by the payer, after the delisting | bridge Burn | 50,253 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn of 1 ETH by the payer, after the delisting | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn of 1 ETH by the payer, after the delisting | bridge Burn | 19,498 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Transfer of 1 ETH from the payer, after the delisting | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer of 1 ETH from the payer, after the delisting | bridge Transfer | 23,563 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer of 1 ETH from the payer, after the delisting | bridge Transfer | 37,060 | 1 | 1,279 | 101 | 1,040 | 1,505 | – | 0 |
| Transfer of 1 ETH from the payer, after the delisting | bridge Credit | 21,476 | 1 | 313 | 101 | 74 | 412 | 74 | 0 |
| AttestListing ETH: the slot holds 2 | bridge AttestListing | 46,567 | 1 | 1,651 | 997 | 580 | 3,415 | 609 | 0 |
| AttestListing ETH: the slot holds 2 | Attest | 85,880 | 1 | 1,561 | 1,058 | 365 | 2,024 | – | 0 |
| AttestListing ETH: the slot holds 2 | bridge (reply) | 37,063 | 1 | 1,117 | 370 | 609 | 0 | – | 247 |
| AttestListing ETH: relisted (the slot holds 0) | bridge AttestListing | 45,068 | 1 | 1,580 | 926 | 580 | 3,273 | 609 | 0 |
| AttestListing ETH: relisted (the slot holds 0) | Attest | 78,910 | 1 | 1,490 | 987 | 365 | 1,953 | – | 0 |
| AttestListing ETH: relisted (the slot holds 0) | bridge (reply) | 50,704 | 1 | 1,117 | 370 | 609 | 1,888 | 580 | 0 |
| AttestListing ETH: relisted (the slot holds 0) | bridge SetListing | 40,383 | 1 | 1,240 | 62 | 1,040 | 2,305 | 1,040 | 0 |
| AttestListing ETH under the delisting's record (now older) | bridge AttestListing | 46,567 | 1 | 1,651 | 997 | 580 | 3,415 | 609 | 0 |
| AttestListing ETH under the delisting's record (now older) | Attest | 85,880 | 1 | 1,561 | 1,058 | 365 | 2,024 | – | 0 |
| AttestListing ETH under the delisting's record (now older) | bridge (reply) | 50,702 | 1 | 1,117 | 370 | 609 | 1,888 | 580 | 0 |
| AttestListing ETH under the delisting's record (now older) | bridge SetListing | 28,464 | 1 | 1,240 | 62 | 1,040 | 0 | – | 247 |
