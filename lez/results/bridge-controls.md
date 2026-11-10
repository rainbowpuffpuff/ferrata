# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)

Bridge program image `b5d177bb06a625166e8d39d10113655e113b768d3ebb101d496689b5d6691132`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 43,855 → Attest 84,756 → bridge (reply) 39,214 | 167,825 | 3% | 1,946 | 1,358,168 | applied |
| 2 | Init B | bridge Init 43,235 → Attest 82,663 → bridge (reply) 39,214 | 165,112 | 3% | 1,882 | 1,335,952 | applied |
| 3 | Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register 72,604 → Attest 147,234 → bridge (reply) 73,421 → bridge WriteToken 29,035 | 322,294 | 6% | 2,747 | 2,600,328 | applied |
| 4 | Register ETH (A) again | bridge Register 78,596 → Attest 161,526 → bridge (reply) 73,202 → bridge WriteToken 31,286 | 344,610 | 7% | 3,080 | 2,781,520 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register T2 (A): vault A holds no entry for it | bridge Register 64,015 → Attest 143,668 → bridge (reply) 49,206 | 256,889 | 5% | 2,633 | 2,076,176 | refused: exit 247 (IMPORT_MISMATCH) |
| 6 | Register ETH (A'): configuration never initialised | bridge Register 20,064 | 20,064 | 0% | 2,812 | 183,008 | refused: exit 245 (NOT_FOUND) |
| 7 | Register ETH (B) with other relay fees than the vault's | bridge Register 75,459 → Attest 153,207 → bridge (reply) 49,859 | 278,525 | 6% | 2,770 | 2,250,360 | refused: exit 247 (IMPORT_MISMATCH) |
| 8 | Register: nine denominations | bridge Register 34,232 | 34,232 | 1% | 3,177 | 299,272 | refused: exit 240 (BAD_MESSAGE) |
| 9 | Register: a denomination not a multiple of the LEZ unit | bridge Register 35,556 | 35,556 | 1% | 2,770 | 306,608 | refused: exit 240 (BAD_MESSAGE) |
| 10 | Register: a relay fee of 2% | bridge Register 35,760 | 35,760 | 1% | 2,770 | 308,240 | refused: exit 240 (BAD_MESSAGE) |
| 11 | Register: a LEZ unit that is not a power of ten | bridge Register 34,985 | 34,985 | 1% | 2,770 | 302,040 | refused: exit 240 (BAD_MESSAGE) |
| 12 | Register: schedule 0 with a mint fee | bridge Register 39,673 | 39,673 | 1% | 2,770 | 339,544 | refused: exit 240 (BAD_MESSAGE) |
| 13 | Register ETH (B) | bridge Register 75,461 → Attest 153,207 → bridge (reply) 73,421 → bridge WriteToken 29,035 | 331,124 | 7% | 2,770 | 2,671,152 | applied |
| 14 | Register T2 (B): a second token of the same configuration | bridge Register 71,060 → Attest 164,572 → bridge (reply) 71,627 → bridge WriteToken 28,047 | 335,306 | 7% | 2,850 | 2,705,248 | applied |
| 15 | Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register 80,402 → Attest 176,416 → bridge (reply) 73,202 → bridge WriteToken 29,035 | 359,055 | 7% | 3,254 | 2,898,472 | applied |
| 16 | ImportRoot A n=5 | bridge ImportRoot 48,263 → Attest 107,114 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 224,650 | 4% | 1,895 | 1,812,360 | applied |
| 17 | ImportRoot A n=8 | bridge ImportRoot 49,391 → Attest 111,079 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 229,743 | 5% | 2,025 | 1,854,144 | applied |
| 18 | ImportRoot B n=8 | bridge ImportRoot 48,069 → Attest 112,079 → bridge (reply) 52,945 → bridge WriteRoot 16,328 | 229,421 | 5% | 1,918 | 1,850,712 | applied |
| 19 | Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim 107,526 → bridge Issue 24,470 | 131,996 | 3% | 2,061 | 1,072,456 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 20 | ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees 51,417 → Attest 101,290 → bridge (reply) 42,198 | 194,905 | 4% | 2,003 | 1,575,264 | refused: exit 247 (IMPORT_MISMATCH) |
| 21 | ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees 48,745 → Attest 105,917 → bridge (reply) 51,842 → bridge ExtendFees 24,606 | 231,110 | 5% | 1,945 | 1,864,440 | refused: exit 247 (IMPORT_MISMATCH) |
| 22 | ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees 53,935 → Attest 111,079 → bridge (reply) 57,503 → bridge ExtendFees 38,134 | 260,651 | 5% | 2,206 | 2,102,856 | applied |
| 23 | ImportFees ETH (A) schedule 1 again | bridge ImportFees 53,935 → Attest 111,079 → bridge (reply) 57,503 → bridge ExtendFees 27,304 | 249,821 | 5% | 2,206 | 2,016,216 | refused: exit 247 (IMPORT_MISMATCH) |
| 24 | Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim 107,526 → bridge Issue 25,911 | 133,437 | 3% | 2,061 | 1,083,984 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 25 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,745 → Attest 105,917 → bridge (reply) 51,842 → bridge ExtendFees 36,134 | 242,638 | 5% | 1,945 | 1,956,664 | applied |
| 26 | ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees 53,935 → Attest 111,079 → bridge (reply) 57,503 → bridge ExtendFees 27,942 | 250,459 | 5% | 2,206 | 2,021,320 | refused: exit 240 (BAD_MESSAGE) |
| 27 | ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees 53,935 → Attest 111,079 → bridge (reply) 57,503 → bridge ExtendFees 26,157 | 248,674 | 5% | 2,206 | 2,007,040 | refused: exit 240 (BAD_MESSAGE) |
| 28 | Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim 31,744 | 31,744 | 1% | 2,061 | 270,440 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 29 | Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim 31,741 | 31,741 | 1% | 2,061 | 270,416 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 30 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 12,790 → bridge SetCap 36,839 | 49,629 | 1% | 733 | 402,896 | applied |
| 31 | SetCap (admin) below the floor | bridge SetCap 12,790 → bridge SetCap 26,595 | 39,385 | 1% | 733 | 320,944 | refused: exit 240 (BAD_MESSAGE) |
| 32 | SetCap by the freeze authority | bridge SetCap 12,790 → bridge SetCap 26,127 | 38,917 | 1% | 733 | 317,200 | refused: exit 253 (NOT_AUTHORIZED) |
| 33 | SetCap by a signer with no role | bridge SetCap 12,790 → bridge SetCap 26,127 | 38,917 | 1% | 733 | 317,200 | refused: exit 253 (NOT_AUTHORIZED) |
| 34 | SetCap sent to the token's account itself | bridge SetCap 25,079 | 25,079 | 1% | 493 | 204,576 | refused: exit 253 (NOT_AUTHORIZED) |
| 35 | Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim 107,526 → bridge Issue 47,249 → bridge Admit 34,291 → bridge Spend 24,145 → bridge Mint 14,237 → bridge Mint 14,237 → bridge Mint 14,237 | 255,922 | 5% | 2,061 | 2,063,864 | applied |
| 36 | Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim 107,526 → bridge Issue 26,654 | 134,180 | 3% | 2,061 | 1,089,928 | refused: exit 255 (CAPPED) |
| 37 | Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Claim 107,526 → bridge Issue 47,328 → bridge Admit 34,381 → bridge Spend 21,901 → bridge Mint 14,237 → bridge Mint 19,181 | 244,554 | 5% | 2,061 | 1,972,920 | applied |
| 38 | SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap 12,281 → bridge SetCap 31,909 | 44,190 | 1% | 713 | 359,224 | applied |
| 39 | Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim 107,526 → bridge Issue 44,393 → bridge Admit 27,149 | 179,068 | 4% | 2,061 | 1,449,032 | refused: exit 255 (CAPPED) |
| 40 | Claim B's T2 with a time one hour ahead of the block | bridge Claim 107,526 → bridge Issue 42,627 → bridge Admit 34,402 → bridge Spend 19,378 → bridge Mint 19,465 | 10,000,000 | 200% | 2,061 | 80,016,488 | reverted: Execution outside of the validity window |
| 41 | Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim 107,526 → bridge Issue 42,627 → bridge Admit 34,002 → bridge Spend 19,378 → bridge Mint 19,465 | 222,998 | 4% | 2,061 | 1,800,472 | applied |
| 42 | SetCap (admin): global mints of B back to 200 ETH | bridge SetCap 12,281 → bridge SetCap 31,637 | 43,918 | 1% | 713 | 357,048 | applied |
| 43 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 12,791 → bridge SetCap 36,841 | 49,632 | 1% | 733 | 402,920 | applied |
| 44 | Burn 1 ETH (A) | bridge Burn 26,461 → bridge Burn 49,032 → bridge Admit 34,250 → bridge Burn 20,835 | 130,578 | 3% | 775 | 1,050,824 | applied |
| 45 | Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn 26,461 → bridge Burn 31,070 | 57,531 | 1% | 775 | 466,448 | refused: exit 255 (CAPPED) |
| 46 | Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn 26,461 → bridge Burn 31,070 | 57,531 | 1% | 775 | 466,448 | refused: exit 255 (CAPPED) |
| 47 | Tick ETH (A) | bridge Tick 32,821 | 32,821 | 1% | 460 | 266,248 | applied |
| 48 | Burn 1 ETH (A) after the Tick | bridge Burn 26,461 → bridge Burn 49,444 → bridge Admit 33,961 → bridge Burn 20,835 | 130,701 | 3% | 775 | 1,051,808 | applied |
| 49 | SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap 12,791 → bridge SetCap 36,967 | 49,758 | 1% | 733 | 403,928 | applied |
| 50 | Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze 12,483 → bridge Freeze 34,060 | 46,543 | 1% | 710 | 378,024 | applied |
| 51 | Claim B's 1 ETH while its mints are frozen | bridge Claim 107,526 → bridge Issue 25,180 | 132,706 | 3% | 2,061 | 1,078,136 | refused: exit 242 (FROZEN) |
| 52 | Freeze by the admin: unfreeze ETH (B) | bridge Freeze 12,483 → bridge Freeze 24,567 | 37,050 | 1% | 710 | 302,080 | refused: exit 253 (NOT_AUTHORIZED) |
| 53 | Freeze for 30 days and 1 ms | bridge Freeze 12,483 → bridge Freeze 25,041 | 37,524 | 1% | 710 | 305,872 | refused: exit 240 (BAD_MESSAGE) |
| 54 | Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze 12,483 → bridge Freeze 34,042 | 46,525 | 1% | 710 | 377,880 | applied |
| 55 | Freeze (freeze authority): all mints of B for 30 days | bridge Freeze 11,871 → bridge Freeze 31,380 | 43,251 | 1% | 690 | 351,528 | applied |
| 56 | Claim B's 1 ETH under the global mint freeze | bridge Claim 107,526 → bridge Issue 44,394 → bridge Admit 27,121 | 179,041 | 4% | 2,061 | 1,448,816 | refused: exit 242 (FROZEN) |
| 57 | Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze 12,483 → bridge Freeze 37,064 | 49,547 | 1% | 710 | 402,056 | applied |
| 58 | Burn 1 ETH (A) while its burns are frozen | bridge Burn 26,461 → bridge Burn 31,043 | 57,504 | 1% | 775 | 466,232 | refused: exit 242 (FROZEN) |
| 59 | Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze 12,483 → bridge Freeze 37,047 | 49,530 | 1% | 710 | 401,920 | applied |
| 60 | Freeze (freeze authority): all burns of A for one hour | bridge Freeze 11,871 → bridge Freeze 31,379 | 43,250 | 1% | 690 | 351,520 | applied |
| 61 | Burn 1 ETH (A) under the global burn freeze | bridge Burn 26,461 → bridge Burn 49,032 → bridge Admit 27,062 | 102,555 | 2% | 775 | 826,640 | refused: exit 242 (FROZEN) |
| 62 | Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn 26,461 → bridge Burn 49,032 → bridge Admit 27,062 | 102,555 | 2% | 775 | 826,640 | refused: exit 242 (FROZEN) |
| 63 | Tick A | bridge Tick 27,213 | 27,213 | 1% | 440 | 221,224 | applied |
| 64 | Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn 26,461 → bridge Burn 49,032 → bridge Admit 34,251 → bridge Burn 20,835 | 130,579 | 3% | 775 | 1,050,832 | applied |
| 65 | Sunset B by a signer with no role | bridge Sunset 11,140 → bridge Sunset 23,768 | 34,908 | 1% | 671 | 284,632 | refused: exit 253 (NOT_AUTHORIZED) |
| 66 | Sunset B by the admin | bridge Sunset 11,140 → bridge Sunset 23,768 | 34,908 | 1% | 671 | 284,632 | refused: exit 253 (NOT_AUTHORIZED) |
| 67 | Sunset sent to config(B) itself | bridge Sunset 22,584 | 22,584 | 0% | 431 | 184,120 | refused: exit 253 (NOT_AUTHORIZED) |
| 68 | Sunset B (freeze authority) | bridge Sunset 11,140 → bridge Sunset 30,844 | 41,984 | 1% | 671 | 341,240 | applied |
| 69 | Sunset B again | bridge Sunset 11,140 → bridge Sunset 24,227 | 35,367 | 1% | 671 | 288,304 | refused: exit 244 (ALREADY_WRITTEN) |
| 70 | ImportRoot B after Sunset | bridge ImportRoot 26,764 | 26,764 | 1% | 1,918 | 229,456 | refused: exit 242 (FROZEN) |
| 71 | Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim 107,526 → bridge Issue 44,393 → bridge Admit 34,292 → bridge Spend 21,901 → bridge Mint 21,106 → bridge Mint 19,465 | 248,683 | 5% | 2,061 | 2,005,952 | applied |
| 72 | Burn 1 ETH (B) after Sunset: burns continue | bridge Burn 27,806 → bridge Burn 46,346 → bridge Admit 34,250 → bridge Burn 20,835 | 129,237 | 3% | 775 | 1,040,096 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 43,855 | 1 | 1,493 | 1,419 | 0 | 3,135 | 390 | 0 |
| Init A | Attest | 84,756 | 1 | 1,626 | 1,155 | 333 | 2,089 | – | 0 |
| Init A | bridge (reply) | 39,214 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B | bridge Init | 43,235 | 1 | 1,429 | 1,355 | 0 | 3,007 | 390 | 0 |
| Init B | Attest | 82,663 | 1 | 1,562 | 1,091 | 333 | 2,025 | – | 0 |
| Init B | bridge (reply) | 39,214 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register | 72,604 | 1 | 2,815 | 2,221 | 520 | 5,850 | 906 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | Attest | 147,234 | 1 | 2,503 | 2,032 | 333 | 3,317 | – | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge (reply) | 73,421 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge WriteToken | 29,035 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register ETH (A) again | bridge Register | 78,596 | 1 | 3,148 | 2,554 | 520 | 6,516 | 906 | 0 |
| Register ETH (A) again | Attest | 161,526 | 1 | 2,836 | 2,365 | 333 | 3,650 | – | 0 |
| Register ETH (A) again | bridge (reply) | 73,202 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (A) again | bridge WriteToken | 31,286 | 1 | 1,694 | 778 | 778 | 0 | – | 244 |
| Register T2 (A): vault A holds no entry for it | bridge Register | 64,015 | 1 | 2,701 | 2,107 | 520 | 5,622 | 826 | 0 |
| Register T2 (A): vault A holds no entry for it | Attest | 143,668 | 1 | 2,469 | 1,998 | 333 | 3,283 | – | 0 |
| Register T2 (A): vault A holds no entry for it | bridge (reply) | 49,206 | 1 | 1,685 | 721 | 826 | 0 | – | 247 |
| Register ETH (A'): configuration never initialised | bridge Register | 20,064 | 1 | 2,295 | 2,221 | 0 | 0 | – | 245 |
| Register ETH (B) with other relay fees than the vault's | bridge Register | 75,459 | 1 | 2,838 | 2,244 | 520 | 5,896 | 906 | 0 |
| Register ETH (B) with other relay fees than the vault's | Attest | 153,207 | 1 | 2,526 | 2,055 | 333 | 3,340 | – | 0 |
| Register ETH (B) with other relay fees than the vault's | bridge (reply) | 49,859 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register: nine denominations | bridge Register | 34,232 | 1 | 3,245 | 2,651 | 520 | 0 | – | 240 |
| Register: a denomination not a multiple of the LEZ unit | bridge Register | 35,556 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a relay fee of 2% | bridge Register | 35,760 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a LEZ unit that is not a power of ten | bridge Register | 34,985 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: schedule 0 with a mint fee | bridge Register | 39,673 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register ETH (B) | bridge Register | 75,461 | 1 | 2,838 | 2,244 | 520 | 5,896 | 906 | 0 |
| Register ETH (B) | Attest | 153,207 | 1 | 2,526 | 2,055 | 333 | 3,340 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,421 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 29,035 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register T2 (B): a second token of the same configuration | bridge Register | 71,060 | 1 | 2,918 | 2,324 | 520 | 6,056 | 826 | 0 |
| Register T2 (B): a second token of the same configuration | Attest | 164,572 | 1 | 2,686 | 2,215 | 333 | 3,500 | – | 0 |
| Register T2 (B): a second token of the same configuration | bridge (reply) | 71,627 | 1 | 1,685 | 721 | 826 | 3,032 | 520 | 0 |
| Register T2 (B): a second token of the same configuration | bridge WriteToken | 28,047 | 1 | 836 | 698 | 0 | 1,559 | 698 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register | 80,402 | 1 | 3,322 | 2,728 | 520 | 6,864 | 906 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | Attest | 176,416 | 1 | 3,010 | 2,539 | 333 | 3,824 | – | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge (reply) | 73,202 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge WriteToken | 29,035 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| ImportRoot A n=5 | bridge ImportRoot | 48,263 | 1 | 1,963 | 1,369 | 520 | 4,078 | 569 | 0 |
| ImportRoot A n=5 | Attest | 107,114 | 1 | 1,920 | 1,449 | 333 | 2,500 | – | 0 |
| ImportRoot A n=5 | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=5 | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 49,391 | 1 | 2,093 | 1,499 | 520 | 4,338 | 569 | 0 |
| ImportRoot A n=8 | Attest | 111,079 | 1 | 2,050 | 1,579 | 333 | 2,630 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 48,069 | 1 | 1,986 | 1,392 | 520 | 4,124 | 569 | 0 |
| ImportRoot B n=8 | Attest | 112,079 | 1 | 1,943 | 1,472 | 333 | 2,523 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 52,945 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 16,328 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Issue | 24,470 | 1 | 1,123 | 207 | 778 | 0 | – | 248 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees | 51,417 | 1 | 2,071 | 1,477 | 520 | 4,290 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | Attest | 101,290 | 1 | 1,847 | 1,376 | 333 | 2,427 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge (reply) | 42,198 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees | 48,745 | 1 | 2,013 | 1,419 | 520 | 4,174 | 558 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | Attest | 105,917 | 1 | 1,977 | 1,506 | 333 | 2,557 | – | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge (reply) | 51,842 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ExtendFees | 24,606 | 1 | 986 | 70 | 778 | 0 | – | 247 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees | 53,935 | 1 | 2,274 | 1,680 | 520 | 4,696 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | Attest | 111,079 | 1 | 2,050 | 1,579 | 333 | 2,630 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge (reply) | 57,503 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ExtendFees | 38,134 | 1 | 1,174 | 258 | 778 | 2,165 | 966 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ImportFees | 53,935 | 1 | 2,274 | 1,680 | 520 | 4,696 | 746 | 0 |
| ImportFees ETH (A) schedule 1 again | Attest | 111,079 | 1 | 2,050 | 1,579 | 333 | 2,630 | – | 0 |
| ImportFees ETH (A) schedule 1 again | bridge (reply) | 57,503 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ExtendFees | 27,304 | 1 | 1,362 | 258 | 966 | 0 | – | 247 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Issue | 25,911 | 1 | 1,311 | 207 | 966 | 0 | – | 248 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,745 | 1 | 2,013 | 1,419 | 520 | 4,174 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 105,917 | 1 | 1,977 | 1,506 | 333 | 2,557 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,842 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 36,134 | 1 | 1,174 | 70 | 966 | 2,165 | 966 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees | 53,935 | 1 | 2,274 | 1,680 | 520 | 4,696 | 746 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | Attest | 111,079 | 1 | 2,050 | 1,579 | 333 | 2,630 | – | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge (reply) | 57,503 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ExtendFees | 27,942 | 1 | 1,174 | 258 | 778 | 0 | – | 240 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees | 53,935 | 1 | 2,274 | 1,680 | 520 | 4,696 | 746 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | Attest | 111,079 | 1 | 2,050 | 1,579 | 333 | 2,630 | – | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge (reply) | 57,503 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ExtendFees | 26,157 | 1 | 1,174 | 258 | 778 | 0 | – | 240 |
| Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim | 31,744 | 1 | 1,470 | 1,279 | 117 | 0 | – | 248 |
| Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim | 31,741 | 1 | 1,470 | 1,279 | 117 | 0 | – | 248 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 12,790 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 36,839 | 1 | 1,199 | 95 | 966 | 2,206 | 966 | 0 |
| SetCap (admin) below the floor | bridge SetCap | 12,790 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 26,595 | 1 | 1,199 | 95 | 966 | 0 | – | 240 |
| SetCap by the freeze authority | bridge SetCap | 12,790 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap by the freeze authority | bridge SetCap | 26,127 | 1 | 1,199 | 95 | 966 | 0 | – | 253 |
| SetCap by a signer with no role | bridge SetCap | 12,790 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap by a signer with no role | bridge SetCap | 26,127 | 1 | 1,199 | 95 | 966 | 0 | – | 253 |
| SetCap sent to the token's account itself | bridge SetCap | 25,079 | 1 | 1,135 | 95 | 966 | 0 | – | 253 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Issue | 47,249 | 1 | 1,311 | 207 | 966 | 2,843 | 966 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Admit | 34,291 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Spend | 24,145 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Issue | 26,654 | 1 | 1,311 | 207 | 966 | 0 | – | 255 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Issue | 47,328 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Admit | 34,381 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Spend | 21,901 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Mint | 14,237 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Mint | 19,181 | 1 | 341 | 129 | 74 | 440 | 74 | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 12,281 | 1 | 149 | 75 | 0 | 317 | – | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 31,909 | 1 | 733 | 75 | 520 | 1,294 | 520 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Issue | 44,393 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Admit | 27,149 | 1 | 737 | 79 | 520 | 0 | – | 255 |
| Claim B's T2 with a time one hour ahead of the block | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Issue | 42,627 | 1 | 1,043 | 207 | 698 | 2,211 | 698 | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Admit | 34,402 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Spend | 19,378 | 1 | 312 | 174 | 0 | 615 | 77 | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Mint | 19,465 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Issue | 42,627 | 1 | 1,043 | 207 | 698 | 2,211 | 698 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Admit | 34,002 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Spend | 19,378 | 1 | 312 | 174 | 0 | 615 | 77 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Mint | 19,465 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 12,281 | 1 | 149 | 75 | 0 | 317 | – | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 31,637 | 1 | 733 | 75 | 520 | 1,294 | 520 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 12,791 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 36,841 | 1 | 1,199 | 95 | 966 | 2,206 | 966 | 0 |
| Burn 1 ETH (A) | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 49,032 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) | bridge Admit | 34,250 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 20,835 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 31,070 | 1 | 1,289 | 185 | 966 | 0 | – | 255 |
| Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn | 31,070 | 1 | 1,289 | 185 | 966 | 0 | – | 255 |
| Tick ETH (A) | bridge Tick | 32,821 | 1 | 1,102 | 62 | 966 | 2,109 | 966 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 49,444 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Admit | 33,961 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 20,835 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 12,791 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 36,967 | 1 | 1,199 | 95 | 966 | 2,206 | 966 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 12,483 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 34,060 | 1 | 988 | 72 | 778 | 1,807 | 778 | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Issue | 25,180 | 1 | 1,123 | 207 | 778 | 0 | – | 242 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 12,483 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 24,567 | 1 | 988 | 72 | 778 | 0 | – | 253 |
| Freeze for 30 days and 1 ms | bridge Freeze | 12,483 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze for 30 days and 1 ms | bridge Freeze | 25,041 | 1 | 988 | 72 | 778 | 0 | – | 240 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 12,483 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 34,042 | 1 | 988 | 72 | 778 | 1,807 | 778 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 11,871 | 1 | 126 | 52 | 0 | 271 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 31,380 | 1 | 710 | 52 | 520 | 1,271 | 520 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Issue | 44,394 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Admit | 27,121 | 1 | 737 | 79 | 520 | 0 | – | 242 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 12,483 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 37,064 | 1 | 1,176 | 72 | 966 | 2,183 | 966 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 31,043 | 1 | 1,289 | 185 | 966 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 12,483 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 37,047 | 1 | 1,176 | 72 | 966 | 2,183 | 966 | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 11,871 | 1 | 126 | 52 | 0 | 271 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 31,379 | 1 | 710 | 52 | 520 | 1,271 | 520 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 49,032 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Admit | 27,062 | 1 | 729 | 71 | 520 | 0 | – | 242 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn | 49,032 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Admit | 27,062 | 1 | 729 | 71 | 520 | 0 | – | 242 |
| Tick A | bridge Tick | 27,213 | 1 | 636 | 42 | 520 | 1,197 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 26,461 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 49,032 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Admit | 34,251 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 20,835 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Sunset B by a signer with no role | bridge Sunset | 11,140 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by a signer with no role | bridge Sunset | 23,768 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset B by the admin | bridge Sunset | 11,140 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by the admin | bridge Sunset | 23,768 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset sent to config(B) itself | bridge Sunset | 22,584 | 1 | 627 | 33 | 520 | 0 | – | 253 |
| Sunset B (freeze authority) | bridge Sunset | 11,140 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B (freeze authority) | bridge Sunset | 30,844 | 1 | 691 | 33 | 520 | 1,236 | 520 | 0 |
| Sunset B again | bridge Sunset | 11,140 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B again | bridge Sunset | 24,227 | 1 | 691 | 33 | 520 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 26,764 | 1 | 1,986 | 1,392 | 520 | 0 | – | 242 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim | 107,526 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Issue | 44,393 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Admit | 34,292 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Spend | 21,901 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 21,106 | 1 | 409 | 129 | 142 | 644 | 210 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 19,465 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 27,806 | 1 | 469 | 185 | 210 | 893 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 46,346 | 1 | 1,101 | 185 | 778 | 2,336 | 778 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Admit | 34,250 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 20,835 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |

## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Sunset B by a signer with no role | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B (freeze authority) | PublicRoot | 33 | Sunset | yes |
| Sunset B again | PublicRoot | 33 | Sunset | no (reverted) |
| ImportRoot B after Sunset | PublicRoot | 1392 | ImportRoot | no (reverted) |
| Burn 1 ETH (B) after Sunset: burns continue | PublicRoot | 185 | Burn | yes |
