# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 43,907 → Attest 85,725 → bridge (reply) 39,144 | 168,776 | 3% | 1,946 | 1,365,776 | applied |
| 2 | Init B | bridge Init 43,287 → Attest 83,634 → bridge (reply) 39,144 | 166,065 | 3% | 1,882 | 1,343,576 | applied |
| 3 | Tick A: block 102's record | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,537 | 76,845 | 2% | 505 | 618,800 | applied |
| 4 | Tick B: block 102's record | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,537 | 76,845 | 2% | 505 | 618,800 | applied |
| 5 | Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register 72,339 → Attest 150,201 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 327,269 | 7% | 2,747 | 2,640,128 | applied |
| 6 | Register ETH (A) again | bridge Register 79,022 → Attest 164,757 → bridge (reply) 73,021 → bridge WriteToken 34,666 | 351,466 | 7% | 3,080 | 2,836,368 | refused: exit 244 (ALREADY_WRITTEN) |
| 7 | Register T2 (A): vault A holds no entry for it | bridge Register 64,547 → Attest 146,721 → bridge (reply) 49,115 | 260,383 | 5% | 2,633 | 2,104,128 | refused: exit 247 (IMPORT_MISMATCH) |
| 8 | Register ETH (A'): configuration never initialised | bridge Register 20,329 | 20,329 | 0% | 2,812 | 185,128 | refused: exit 245 (NOT_FOUND) |
| 9 | Register ETH (B) with other relay fees than the vault's | bridge Register 75,240 → Attest 155,163 → bridge (reply) 49,768 | 280,171 | 6% | 2,770 | 2,263,528 | refused: exit 247 (IMPORT_MISMATCH) |
| 10 | Register: nine denominations | bridge Register 34,493 | 34,493 | 1% | 3,177 | 301,360 | refused: exit 240 (BAD_MESSAGE) |
| 11 | Register: a denomination not a multiple of the LEZ unit | bridge Register 35,817 | 35,817 | 1% | 2,770 | 308,696 | refused: exit 240 (BAD_MESSAGE) |
| 12 | Register: a relay fee of 2% | bridge Register 36,021 | 36,021 | 1% | 2,770 | 310,328 | refused: exit 240 (BAD_MESSAGE) |
| 13 | Register: a LEZ unit that is not a power of ten | bridge Register 35,246 | 35,246 | 1% | 2,770 | 304,128 | refused: exit 240 (BAD_MESSAGE) |
| 14 | Register: schedule 0 with a mint fee | bridge Register 39,934 | 39,934 | 1% | 2,770 | 341,632 | refused: exit 240 (BAD_MESSAGE) |
| 15 | Register ETH (B) | bridge Register 75,242 → Attest 155,163 → bridge (reply) 73,240 → bridge WriteToken 31,489 | 335,134 | 7% | 2,770 | 2,703,232 | applied |
| 16 | Register T2 (B): a second token of the same configuration | bridge Register 71,294 → Attest 167,274 → bridge (reply) 71,448 → bridge WriteToken 30,316 | 340,332 | 7% | 2,850 | 2,745,456 | applied |
| 17 | Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register 80,103 → Attest 178,613 → bridge (reply) 73,021 → bridge WriteToken 31,489 | 363,226 | 7% | 3,254 | 2,931,840 | applied |
| 18 | ImportRoot A n=5 | bridge ImportRoot 48,295 → Attest 108,849 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 225,898 | 5% | 1,895 | 1,822,344 | applied |
| 19 | ImportRoot A n=8 | bridge ImportRoot 49,862 → Attest 112,749 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 231,365 | 5% | 2,025 | 1,867,120 | applied |
| 20 | ImportRoot B n=8 | bridge ImportRoot 48,098 → Attest 114,375 → bridge (reply) 52,241 → bridge WriteRoot 16,513 | 231,227 | 5% | 1,918 | 1,865,160 | applied |
| 21 | Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim 107,479 → bridge Issue 27,758 | 135,237 | 3% | 2,053 | 1,098,320 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees 51,565 → Attest 103,426 → bridge (reply) 42,097 | 197,088 | 4% | 2,003 | 1,592,728 | refused: exit 247 (IMPORT_MISMATCH) |
| 23 | ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees 48,536 → Attest 107,315 → bridge (reply) 51,094 → bridge ExtendFees 26,810 | 233,755 | 5% | 1,945 | 1,885,600 | refused: exit 247 (IMPORT_MISMATCH) |
| 24 | ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees 54,297 → Attest 112,749 → bridge (reply) 56,755 → bridge ExtendFees 41,418 | 265,219 | 5% | 2,206 | 2,139,400 | applied |
| 25 | ImportFees ETH (A) schedule 1 again | bridge ImportFees 54,297 → Attest 112,749 → bridge (reply) 56,755 → bridge ExtendFees 29,622 | 253,423 | 5% | 2,206 | 2,045,032 | refused: exit 247 (IMPORT_MISMATCH) |
| 26 | Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim 107,479 → bridge Issue 29,199 | 136,678 | 3% | 2,053 | 1,109,848 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 27 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,536 → Attest 107,315 → bridge (reply) 51,094 → bridge ExtendFees 39,119 | 246,064 | 5% | 1,945 | 1,984,072 | applied |
| 28 | ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees 54,297 → Attest 112,749 → bridge (reply) 56,755 → bridge ExtendFees 30,260 | 254,061 | 5% | 2,206 | 2,050,136 | refused: exit 240 (BAD_MESSAGE) |
| 29 | ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees 54,297 → Attest 112,749 → bridge (reply) 56,755 → bridge ExtendFees 28,475 | 252,276 | 5% | 2,206 | 2,035,856 | refused: exit 240 (BAD_MESSAGE) |
| 30 | Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim 31,812 | 31,812 | 1% | 2,053 | 270,920 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 31 | Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim 31,809 | 31,809 | 1% | 2,053 | 270,896 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 32 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 13,489 → bridge SetCap 42,316 | 55,805 | 1% | 725 | 452,240 | applied |
| 33 | SetCap (admin) below the floor | bridge SetCap 13,489 → bridge SetCap 30,231 | 43,720 | 1% | 725 | 355,560 | refused: exit 240 (BAD_MESSAGE) |
| 34 | SetCap by the freeze authority | bridge SetCap 13,489 → bridge SetCap 29,760 | 43,249 | 1% | 725 | 351,792 | refused: exit 253 (NOT_AUTHORIZED) |
| 35 | SetCap by a signer with no role | bridge SetCap 13,489 → bridge SetCap 29,760 | 43,249 | 1% | 725 | 351,792 | refused: exit 253 (NOT_AUTHORIZED) |
| 36 | SetCap sent to the token's account itself | bridge SetCap 28,571 | 28,571 | 1% | 485 | 232,448 | refused: exit 253 (NOT_AUTHORIZED) |
| 37 | Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim 107,479 → bridge Issue 52,518 → bridge Admit 33,885 → bridge Spend 36,659 → bridge Mint 17,560 → bridge Mint 17,560 → bridge Mint 17,560 | 283,221 | 6% | 2,053 | 2,282,192 | applied |
| 38 | Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim 107,479 → bridge Issue 29,931 | 137,410 | 3% | 2,053 | 1,115,704 | refused: exit 255 (CAPPED) |
| 39 | Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim 107,479 → bridge Issue 29,931 | 137,410 | 3% | 2,053 | 1,115,704 | refused: exit 255 (CAPPED) |
| 40 | Tick ETH (A): the record 10 s later | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,737 | 94,925 | 2% | 525 | 763,600 | applied |
| 41 | Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim 107,479 → bridge Issue 52,483 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 22,280 | 263,988 | 5% | 2,053 | 2,128,328 | applied |
| 42 | SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap 12,842 → bridge SetCap 32,547 | 45,389 | 1% | 705 | 368,752 | applied |
| 43 | Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim 107,479 → bridge Issue 48,227 → bridge Admit 27,132 | 182,838 | 4% | 2,053 | 1,479,128 | refused: exit 255 (CAPPED) |
| 44 | Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim 107,479 → bridge Issue 46,385 → bridge Admit 33,885 → bridge Spend 23,693 → bridge Mint 22,568 | 234,010 | 5% | 2,053 | 1,888,504 | applied |
| 45 | SetCap (admin): global mints of B back to 200 ETH | bridge SetCap 12,842 → bridge SetCap 32,550 | 45,392 | 1% | 705 | 368,776 | applied |
| 46 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 13,490 → bridge SetCap 42,749 | 56,239 | 1% | 725 | 455,712 | applied |
| 47 | Burn 1 ETH (A) | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 53,773 → bridge Admit 33,885 → bridge Burn 20,934 | 164,151 | 3% | 839 | 1,319,920 | applied |
| 48 | Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 34,219 | 89,778 | 2% | 839 | 724,936 | refused: exit 255 (CAPPED) |
| 49 | Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 34,219 | 89,778 | 2% | 839 | 724,936 | refused: exit 255 (CAPPED) |
| 50 | Tick ETH (A): the record 100 s later | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,737 | 94,925 | 2% | 525 | 763,600 | applied |
| 51 | Burn 1 ETH (A) after the Tick | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 54,189 → bridge Admit 33,886 → bridge Burn 20,934 | 164,568 | 3% | 839 | 1,323,256 | applied |
| 52 | Tick ETH (A): a record 100 s later again | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,737 | 94,925 | 2% | 525 | 763,600 | applied |
| 53 | SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap 13,490 → bridge SetCap 42,778 | 56,268 | 1% | 725 | 455,944 | applied |
| 54 | Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze 13,405 → bridge Freeze 42,265 → Read 11,877 → bridge (reply) 36,629 | 104,176 | 2% | 775 | 839,608 | applied |
| 55 | Claim B's 1 ETH while its mints are frozen | bridge Claim 107,479 → bridge Issue 28,455 | 135,934 | 3% | 2,053 | 1,103,896 | refused: exit 242 (FROZEN) |
| 56 | Freeze by the admin: unfreeze ETH (B) | bridge Freeze 13,405 → bridge Freeze 28,442 | 41,847 | 1% | 775 | 340,976 | refused: exit 253 (NOT_AUTHORIZED) |
| 57 | Freeze for 30 days and 1 ms after the record's time | bridge Freeze 13,405 → bridge Freeze 42,265 → Read 11,877 → bridge (reply) 26,496 | 94,043 | 2% | 775 | 758,544 | refused: exit 240 (BAD_MESSAGE) |
| 58 | Freeze until the record's time itself (already past) | bridge Freeze 13,405 → bridge Freeze 42,265 → Read 11,877 → bridge (reply) 26,485 | 94,032 | 2% | 775 | 758,456 | refused: exit 240 (BAD_MESSAGE) |
| 59 | Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze 13,405 → bridge Freeze 42,265 → Read 11,877 → bridge (reply) 36,617 | 104,164 | 2% | 775 | 839,512 | applied |
| 60 | Freeze (freeze authority): all mints of B for 30 days | bridge Freeze 12,631 → bridge Freeze 36,354 → Read 11,877 → bridge (reply) 32,564 | 93,426 | 2% | 755 | 753,448 | applied |
| 61 | Claim B's 1 ETH under the global mint freeze | bridge Claim 107,479 → bridge Issue 48,642 → bridge Admit 27,102 | 183,223 | 4% | 2,053 | 1,482,208 | refused: exit 242 (FROZEN) |
| 62 | Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze 13,405 → bridge Freeze 46,115 → Read 11,877 → bridge (reply) 40,915 | 112,312 | 2% | 775 | 904,696 | applied |
| 63 | Burn 1 ETH (A) while its burns are frozen | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 34,190 | 89,749 | 2% | 839 | 724,704 | refused: exit 242 (FROZEN) |
| 64 | Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze 13,405 → bridge Freeze 46,115 → Read 11,877 → bridge (reply) 40,901 | 112,298 | 2% | 775 | 904,584 | applied |
| 65 | Freeze (freeze authority): all burns of A for one hour | bridge Freeze 12,631 → bridge Freeze 36,354 → Read 11,877 → bridge (reply) 32,563 | 93,425 | 2% | 755 | 753,440 | applied |
| 66 | Burn 1 ETH (A) under the global burn freeze | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 53,773 → bridge Admit 27,103 | 136,435 | 3% | 839 | 1,098,192 | refused: exit 242 (FROZEN) |
| 67 | Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 53,773 → bridge Admit 27,103 | 136,435 | 3% | 839 | 1,098,192 | refused: exit 242 (FROZEN) |
| 68 | Tick A: the record an hour later | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,536 | 76,844 | 2% | 505 | 618,792 | applied |
| 69 | Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 53,773 → bridge Admit 34,180 → bridge Burn 20,934 | 164,446 | 3% | 839 | 1,322,280 | applied |
| 70 | Sunset B by a signer with no role | bridge Sunset 11,926 → bridge Sunset 25,001 | 36,927 | 1% | 671 | 300,784 | refused: exit 253 (NOT_AUTHORIZED) |
| 71 | Sunset B by the admin | bridge Sunset 11,926 → bridge Sunset 25,001 | 36,927 | 1% | 671 | 300,784 | refused: exit 253 (NOT_AUTHORIZED) |
| 72 | Sunset sent to config(B) itself | bridge Sunset 23,810 | 23,810 | 0% | 431 | 193,928 | refused: exit 253 (NOT_AUTHORIZED) |
| 73 | Sunset B (freeze authority) | bridge Sunset 11,926 → bridge Sunset 32,076 | 44,002 | 1% | 671 | 357,384 | applied |
| 74 | Sunset B again | bridge Sunset 11,926 → bridge Sunset 25,461 | 37,387 | 1% | 671 | 304,464 | refused: exit 244 (ALREADY_WRITTEN) |
| 75 | ImportRoot B after Sunset | bridge ImportRoot 26,992 | 26,992 | 1% | 1,918 | 231,280 | refused: exit 242 (FROZEN) |
| 76 | Tick B: the record 30 days later | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,537 | 76,845 | 2% | 505 | 618,800 | applied |
| 77 | Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim 107,479 → bridge Issue 48,642 → bridge Admit 34,180 → bridge Spend 30,300 → bridge Mint 24,334 → bridge Mint 22,568 | 267,503 | 5% | 2,053 | 2,156,448 | applied |
| 78 | Burn 1 ETH (B) after Sunset: burns continue | bridge Burn 23,952 → bridge Burn 32,954 → bridge Burn 50,321 → bridge Admit 33,885 → bridge Burn 20,934 | 162,046 | 3% | 839 | 1,303,080 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 43,907 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A | bridge (reply) | 39,144 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B | bridge Init | 43,287 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B | bridge (reply) | 39,144 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Tick A: block 102's record | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: block 102's record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: block 102's record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Tick B: block 102's record | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: block 102's record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: block 102's record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register | 72,339 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (A) again | bridge Register | 79,022 | 1 | 3,148 | 2,554 | 520 | 6,517 | 906 | 0 |
| Register ETH (A) again | Attest | 164,757 | 1 | 2,837 | 2,366 | 333 | 3,651 | – | 0 |
| Register ETH (A) again | bridge (reply) | 73,021 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (A) again | bridge WriteToken | 34,666 | 1 | 1,824 | 843 | 843 | 0 | – | 244 |
| Register T2 (A): vault A holds no entry for it | bridge Register | 64,547 | 1 | 2,701 | 2,107 | 520 | 5,623 | 826 | 0 |
| Register T2 (A): vault A holds no entry for it | Attest | 146,721 | 1 | 2,470 | 1,999 | 333 | 3,284 | – | 0 |
| Register T2 (A): vault A holds no entry for it | bridge (reply) | 49,115 | 1 | 1,685 | 721 | 826 | 0 | – | 247 |
| Register ETH (A'): configuration never initialised | bridge Register | 20,329 | 1 | 2,295 | 2,221 | 0 | 0 | – | 245 |
| Register ETH (B) with other relay fees than the vault's | bridge Register | 75,240 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) with other relay fees than the vault's | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) with other relay fees than the vault's | bridge (reply) | 49,768 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register: nine denominations | bridge Register | 34,493 | 1 | 3,245 | 2,651 | 520 | 0 | – | 240 |
| Register: a denomination not a multiple of the LEZ unit | bridge Register | 35,817 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a relay fee of 2% | bridge Register | 36,021 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a LEZ unit that is not a power of ten | bridge Register | 35,246 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: schedule 0 with a mint fee | bridge Register | 39,934 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register ETH (B) | bridge Register | 75,242 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,240 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register T2 (B): a second token of the same configuration | bridge Register | 71,294 | 1 | 2,918 | 2,324 | 520 | 6,057 | 826 | 0 |
| Register T2 (B): a second token of the same configuration | Attest | 167,274 | 1 | 2,687 | 2,216 | 333 | 3,501 | – | 0 |
| Register T2 (B): a second token of the same configuration | bridge (reply) | 71,448 | 1 | 1,685 | 721 | 826 | 3,097 | 520 | 0 |
| Register T2 (B): a second token of the same configuration | bridge WriteToken | 30,316 | 1 | 901 | 763 | 0 | 1,689 | 763 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register | 80,103 | 1 | 3,322 | 2,728 | 520 | 6,865 | 906 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | Attest | 178,613 | 1 | 3,011 | 2,540 | 333 | 3,825 | – | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge (reply) | 73,021 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge WriteToken | 31,489 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A n=5 | bridge ImportRoot | 48,295 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A n=5 | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A n=5 | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=5 | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 49,862 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A n=8 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 48,098 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B n=8 | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 52,241 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 16,513 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Issue | 27,758 | 1 | 1,180 | 199 | 843 | 0 | – | 248 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees | 51,565 | 1 | 2,071 | 1,477 | 520 | 4,291 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | Attest | 103,426 | 1 | 1,848 | 1,377 | 333 | 2,428 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge (reply) | 42,097 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees | 48,536 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ExtendFees | 26,810 | 1 | 1,051 | 70 | 843 | 0 | – | 247 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees | 54,297 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge (reply) | 56,755 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ExtendFees | 41,418 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ImportFees | 54,297 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A) schedule 1 again | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 again | bridge (reply) | 56,755 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ExtendFees | 29,622 | 1 | 1,427 | 258 | 1,031 | 0 | – | 247 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Issue | 29,199 | 1 | 1,368 | 199 | 1,031 | 0 | – | 248 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,536 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,094 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 39,119 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees | 54,297 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge (reply) | 56,755 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ExtendFees | 30,260 | 1 | 1,239 | 258 | 843 | 0 | – | 240 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees | 54,297 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge (reply) | 56,755 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ExtendFees | 28,475 | 1 | 1,239 | 258 | 843 | 0 | – | 240 |
| Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim | 31,812 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim | 31,809 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 13,489 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,316 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| SetCap (admin) below the floor | bridge SetCap | 13,489 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 30,231 | 1 | 1,256 | 87 | 1,031 | 0 | – | 240 |
| SetCap by the freeze authority | bridge SetCap | 13,489 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by the freeze authority | bridge SetCap | 29,760 | 1 | 1,256 | 87 | 1,031 | 0 | – | 253 |
| SetCap by a signer with no role | bridge SetCap | 13,489 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by a signer with no role | bridge SetCap | 29,760 | 1 | 1,256 | 87 | 1,031 | 0 | – | 253 |
| SetCap sent to the token's account itself | bridge SetCap | 28,571 | 1 | 1,192 | 87 | 1,031 | 0 | – | 253 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Issue | 52,518 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Admit | 33,885 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Spend | 36,659 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Issue | 29,931 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Issue | 29,931 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): the record 10 s later | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the record 10 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the record 10 s later | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Issue | 52,483 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 22,280 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 12,842 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 32,547 | 1 | 725 | 67 | 520 | 1,270 | 520 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Issue | 48,227 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Admit | 27,132 | 1 | 728 | 70 | 520 | 0 | – | 255 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Issue | 46,385 | 1 | 1,100 | 199 | 763 | 2,308 | 763 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Admit | 33,885 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Spend | 23,693 | 1 | 312 | 174 | 0 | 679 | 77 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Mint | 22,568 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 12,842 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 32,550 | 1 | 725 | 67 | 520 | 1,270 | 520 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 13,490 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,749 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 53,773 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 33,885 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 34,219 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 34,219 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A): the record 100 s later | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the record 100 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the record 100 s later | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 54,189 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A): a record 100 s later again | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 100 s later again | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): a record 100 s later again | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 13,490 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 42,778 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 42,265 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge (reply) | 36,629 | 1 | 1,338 | 337 | 863 | 2,214 | 843 | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Issue | 28,455 | 1 | 1,180 | 199 | 843 | 0 | – | 242 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 28,442 | 1 | 1,054 | 73 | 843 | 0 | – | 253 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 42,265 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze for 30 days and 1 ms after the record's time | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge (reply) | 26,496 | 1 | 1,338 | 337 | 863 | 0 | – | 240 |
| Freeze until the record's time itself (already past) | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze until the record's time itself (already past) | bridge Freeze | 42,265 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze until the record's time itself (already past) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze until the record's time itself (already past) | bridge (reply) | 26,485 | 1 | 1,338 | 337 | 863 | 0 | – | 240 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 42,265 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge (reply) | 36,617 | 1 | 1,338 | 337 | 863 | 2,214 | 843 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 12,631 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 36,354 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge (reply) | 32,564 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Issue | 48,642 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Admit | 27,102 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 46,115 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge (reply) | 40,915 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 34,190 | 1 | 1,354 | 185 | 1,031 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 13,405 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 46,115 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge (reply) | 40,901 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 12,631 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 36,354 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze (freeze authority): all burns of A for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge (reply) | 32,563 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 53,773 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Admit | 27,103 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 53,773 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Admit | 27,103 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Tick A: the record an hour later | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: the record an hour later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: the record an hour later | bridge (reply) | 32,536 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 53,773 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Admit | 34,180 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Sunset B by a signer with no role | bridge Sunset | 11,926 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by a signer with no role | bridge Sunset | 25,001 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset B by the admin | bridge Sunset | 11,926 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by the admin | bridge Sunset | 25,001 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset sent to config(B) itself | bridge Sunset | 23,810 | 1 | 627 | 33 | 520 | 0 | – | 253 |
| Sunset B (freeze authority) | bridge Sunset | 11,926 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B (freeze authority) | bridge Sunset | 32,076 | 1 | 691 | 33 | 520 | 1,236 | 520 | 0 |
| Sunset B again | bridge Sunset | 11,926 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B again | bridge Sunset | 25,461 | 1 | 691 | 33 | 520 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 26,992 | 1 | 1,986 | 1,392 | 520 | 0 | – | 242 |
| Tick B: the record 30 days later | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: the record 30 days later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: the record 30 days later | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Issue | 48,642 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Admit | 34,180 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 24,334 | 1 | 441 | 161 | 142 | 676 | 210 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 22,568 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 32,954 | 1 | 533 | 185 | 210 | 957 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 50,321 | 1 | 1,166 | 185 | 843 | 2,465 | 843 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Admit | 33,885 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |

## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Sunset B by a signer with no role | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B (freeze authority) | PublicRoot | 33 | Sunset | yes |
| Sunset B again | PublicRoot | 33 | Sunset | no (reverted) |
| ImportRoot B after Sunset | PublicRoot | 1392 | ImportRoot | no (reverted) |
| Burn 1 ETH (B) after Sunset: burns continue | PublicRoot | 185 | Burn | yes |
