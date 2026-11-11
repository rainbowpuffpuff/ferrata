# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)

Bridge program image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,051 → Attest 85,725 → bridge (reply) 39,242 | 169,018 | 3% | 1,946 | 1,367,712 | applied |
| 2 | Init B | bridge Init 43,431 → Attest 83,634 → bridge (reply) 39,242 | 166,307 | 3% | 1,882 | 1,345,512 | applied |
| 3 | Tick A: block 102's record | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,537 | 76,899 | 2% | 505 | 619,232 | applied |
| 4 | Tick B: block 102's record | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,537 | 76,899 | 2% | 505 | 619,232 | applied |
| 5 | Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register 72,468 → Attest 150,201 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 327,760 | 7% | 2,747 | 2,644,056 | applied |
| 6 | Register ETH (A) again | bridge Register 79,151 → Attest 164,757 → bridge (reply) 73,259 → bridge WriteToken 34,790 | 351,957 | 7% | 3,080 | 2,840,296 | refused: exit 244 (ALREADY_WRITTEN) |
| 7 | Register T2 (A): vault A holds no entry for it | bridge Register 64,676 → Attest 146,721 → bridge (reply) 49,255 | 260,652 | 5% | 2,633 | 2,106,280 | refused: exit 247 (IMPORT_MISMATCH) |
| 8 | Register ETH (A'): configuration never initialised | bridge Register 20,416 | 20,416 | 0% | 2,812 | 185,824 | refused: exit 245 (NOT_FOUND) |
| 9 | Register ETH (B) with other relay fees than the vault's | bridge Register 75,369 → Attest 155,163 → bridge (reply) 49,908 | 280,440 | 6% | 2,770 | 2,265,680 | refused: exit 247 (IMPORT_MISMATCH) |
| 10 | Register: nine denominations | bridge Register 34,580 | 34,580 | 1% | 3,177 | 302,056 | refused: exit 240 (BAD_MESSAGE) |
| 11 | Register: a denomination not a multiple of the LEZ unit | bridge Register 35,904 | 35,904 | 1% | 2,770 | 309,392 | refused: exit 240 (BAD_MESSAGE) |
| 12 | Register: a relay fee of 2% | bridge Register 36,108 | 36,108 | 1% | 2,770 | 311,024 | refused: exit 240 (BAD_MESSAGE) |
| 13 | Register: a LEZ unit that is not a power of ten | bridge Register 35,333 | 35,333 | 1% | 2,770 | 304,824 | refused: exit 240 (BAD_MESSAGE) |
| 14 | Register: schedule 0 with a mint fee | bridge Register 40,021 | 40,021 | 1% | 2,770 | 342,328 | refused: exit 240 (BAD_MESSAGE) |
| 15 | Register ETH (B) | bridge Register 75,371 → Attest 155,163 → bridge (reply) 73,478 → bridge WriteToken 31,613 | 335,625 | 7% | 2,770 | 2,707,160 | applied |
| 16 | Register T2 (B): a second token of the same configuration | bridge Register 71,423 → Attest 167,274 → bridge (reply) 71,686 → bridge WriteToken 30,440 | 340,823 | 7% | 2,850 | 2,749,384 | applied |
| 17 | Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register 80,232 → Attest 178,613 → bridge (reply) 73,259 → bridge WriteToken 31,613 | 363,717 | 7% | 3,254 | 2,935,768 | applied |
| 18 | ImportRoot A n=5 | bridge ImportRoot 48,280 → Attest 108,849 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 226,181 | 5% | 1,895 | 1,824,608 | applied |
| 19 | ImportRoot A n=8 | bridge ImportRoot 49,847 → Attest 112,749 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 231,648 | 5% | 2,025 | 1,869,384 | applied |
| 20 | ImportRoot B n=8 | bridge ImportRoot 48,083 → Attest 114,375 → bridge (reply) 52,437 → bridge WriteRoot 16,615 | 231,510 | 5% | 1,918 | 1,867,424 | applied |
| 21 | Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim 107,610 → bridge Issue 27,792 | 135,402 | 3% | 2,053 | 1,099,640 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees 51,659 → Attest 103,426 → bridge (reply) 42,247 | 197,332 | 4% | 2,003 | 1,594,680 | refused: exit 247 (IMPORT_MISMATCH) |
| 23 | ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees 48,630 → Attest 107,315 → bridge (reply) 51,342 → bridge ExtendFees 26,907 | 234,194 | 5% | 1,945 | 1,889,112 | refused: exit 247 (IMPORT_MISMATCH) |
| 24 | ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees 54,391 → Attest 112,749 → bridge (reply) 57,003 → bridge ExtendFees 41,515 | 265,658 | 5% | 2,206 | 2,142,912 | applied |
| 25 | ImportFees ETH (A) schedule 1 again | bridge ImportFees 54,391 → Attest 112,749 → bridge (reply) 57,003 → bridge ExtendFees 29,719 | 253,862 | 5% | 2,206 | 2,048,544 | refused: exit 247 (IMPORT_MISMATCH) |
| 26 | Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim 107,610 → bridge Issue 29,233 | 136,843 | 3% | 2,053 | 1,111,168 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 27 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,630 → Attest 107,315 → bridge (reply) 51,342 → bridge ExtendFees 39,216 | 246,503 | 5% | 1,945 | 1,987,584 | applied |
| 28 | ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees 54,391 → Attest 112,749 → bridge (reply) 57,003 → bridge ExtendFees 30,357 | 254,500 | 5% | 2,206 | 2,053,648 | refused: exit 240 (BAD_MESSAGE) |
| 29 | ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees 54,391 → Attest 112,749 → bridge (reply) 57,003 → bridge ExtendFees 28,572 | 252,715 | 5% | 2,206 | 2,039,368 | refused: exit 240 (BAD_MESSAGE) |
| 30 | Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim 31,894 | 31,894 | 1% | 2,053 | 271,576 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 31 | Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim 31,891 | 31,891 | 1% | 2,053 | 271,552 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 32 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 13,538 → bridge SetCap 42,365 | 55,903 | 1% | 725 | 453,024 | applied |
| 33 | SetCap (admin) below the floor | bridge SetCap 13,538 → bridge SetCap 30,280 | 43,818 | 1% | 725 | 356,344 | refused: exit 240 (BAD_MESSAGE) |
| 34 | SetCap by the freeze authority | bridge SetCap 13,538 → bridge SetCap 29,809 | 43,347 | 1% | 725 | 352,576 | refused: exit 253 (NOT_AUTHORIZED) |
| 35 | SetCap by a signer with no role | bridge SetCap 13,538 → bridge SetCap 29,809 | 43,347 | 1% | 725 | 352,576 | refused: exit 253 (NOT_AUTHORIZED) |
| 36 | SetCap sent to the token's account itself | bridge SetCap 28,620 | 28,620 | 1% | 485 | 232,840 | refused: exit 253 (NOT_AUTHORIZED) |
| 37 | Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim 107,610 → bridge Issue 52,747 → bridge Admit 33,982 → bridge Spend 24,414 → bridge Mint 14,616 → bridge Mint 14,616 → bridge Mint 14,616 | 262,601 | 5% | 2,053 | 2,117,232 | applied |
| 38 | Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim 107,610 → bridge Issue 29,965 | 137,575 | 3% | 2,053 | 1,117,024 | refused: exit 255 (CAPPED) |
| 39 | Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim 107,610 → bridge Issue 29,965 | 137,575 | 3% | 2,053 | 1,117,024 | refused: exit 255 (CAPPED) |
| 40 | Tick ETH (A): the record 10 s later | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,737 | 94,979 | 2% | 525 | 764,032 | applied |
| 41 | Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim 107,610 → bridge Issue 52,712 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 19,553 | 250,645 | 5% | 2,053 | 2,021,584 | applied |
| 42 | SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap 12,898 → bridge SetCap 32,596 | 45,494 | 1% | 705 | 369,592 | applied |
| 43 | Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim 107,610 → bridge Issue 48,456 → bridge Admit 27,229 | 183,295 | 4% | 2,053 | 1,482,784 | refused: exit 255 (CAPPED) |
| 44 | Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim 107,610 → bridge Issue 46,614 → bridge Admit 33,982 → bridge Spend 19,649 → bridge Mint 19,837 | 227,692 | 5% | 2,053 | 1,837,960 | applied |
| 45 | SetCap (admin): global mints of B back to 200 ETH | bridge SetCap 12,898 → bridge SetCap 32,599 | 45,497 | 1% | 705 | 369,616 | applied |
| 46 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 13,539 → bridge SetCap 42,798 | 56,337 | 1% | 725 | 456,496 | applied |
| 47 | Burn 1 ETH (A) | bridge Burn 26,722 → bridge Burn 54,036 → bridge Admit 33,982 → bridge Burn 21,107 | 135,847 | 3% | 775 | 1,092,976 | applied |
| 48 | Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn 26,722 → bridge Burn 34,388 | 61,110 | 1% | 775 | 495,080 | refused: exit 255 (CAPPED) |
| 49 | Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn 26,722 → bridge Burn 34,388 | 61,110 | 1% | 775 | 495,080 | refused: exit 255 (CAPPED) |
| 50 | Tick ETH (A): the record 100 s later | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,737 | 94,979 | 2% | 525 | 764,032 | applied |
| 51 | Burn 1 ETH (A) after the Tick | bridge Burn 26,722 → bridge Burn 54,452 → bridge Admit 33,983 → bridge Burn 21,107 | 136,264 | 3% | 775 | 1,096,312 | applied |
| 52 | Tick ETH (A): a record 100 s later again | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,737 | 94,979 | 2% | 525 | 764,032 | applied |
| 53 | SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap 13,539 → bridge SetCap 42,827 | 56,366 | 1% | 725 | 456,728 | applied |
| 54 | Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze 13,454 → bridge Freeze 42,314 → Read 11,877 → bridge (reply) 36,629 | 104,274 | 2% | 775 | 840,392 | applied |
| 55 | Claim B's 1 ETH while its mints are frozen | bridge Claim 107,610 → bridge Issue 28,489 | 136,099 | 3% | 2,053 | 1,105,216 | refused: exit 242 (FROZEN) |
| 56 | Freeze by the admin: unfreeze ETH (B) | bridge Freeze 13,454 → bridge Freeze 28,491 | 41,945 | 1% | 775 | 341,760 | refused: exit 253 (NOT_AUTHORIZED) |
| 57 | Freeze for 30 days and 1 ms after the record's time | bridge Freeze 13,454 → bridge Freeze 42,314 → Read 11,877 → bridge (reply) 26,496 | 94,141 | 2% | 775 | 759,328 | refused: exit 240 (BAD_MESSAGE) |
| 58 | Freeze until the record's time itself (already past) | bridge Freeze 13,454 → bridge Freeze 42,314 → Read 11,877 → bridge (reply) 26,485 | 94,130 | 2% | 775 | 759,240 | refused: exit 240 (BAD_MESSAGE) |
| 59 | Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze 13,454 → bridge Freeze 42,314 → Read 11,877 → bridge (reply) 36,617 | 104,262 | 2% | 775 | 840,296 | applied |
| 60 | Freeze (freeze authority): all mints of B for 30 days | bridge Freeze 12,680 → bridge Freeze 36,403 → Read 11,877 → bridge (reply) 32,564 | 93,524 | 2% | 755 | 754,232 | applied |
| 61 | Claim B's 1 ETH under the global mint freeze | bridge Claim 107,610 → bridge Issue 48,871 → bridge Admit 27,199 | 183,680 | 4% | 2,053 | 1,485,864 | refused: exit 242 (FROZEN) |
| 62 | Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze 13,454 → bridge Freeze 46,164 → Read 11,877 → bridge (reply) 40,915 | 112,410 | 2% | 775 | 905,480 | applied |
| 63 | Burn 1 ETH (A) while its burns are frozen | bridge Burn 26,722 → bridge Burn 34,359 | 61,081 | 1% | 775 | 494,848 | refused: exit 242 (FROZEN) |
| 64 | Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze 13,454 → bridge Freeze 46,164 → Read 11,877 → bridge (reply) 40,901 | 112,396 | 2% | 775 | 905,368 | applied |
| 65 | Freeze (freeze authority): all burns of A for one hour | bridge Freeze 12,680 → bridge Freeze 36,403 → Read 11,877 → bridge (reply) 32,563 | 93,523 | 2% | 755 | 754,224 | applied |
| 66 | Burn 1 ETH (A) under the global burn freeze | bridge Burn 26,722 → bridge Burn 54,036 → bridge Admit 27,200 | 107,958 | 2% | 775 | 869,864 | refused: exit 242 (FROZEN) |
| 67 | Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn 26,722 → bridge Burn 54,036 → bridge Admit 27,200 | 107,958 | 2% | 775 | 869,864 | refused: exit 242 (FROZEN) |
| 68 | Tick A: the record an hour later | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,536 | 76,898 | 2% | 505 | 619,224 | applied |
| 69 | Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn 26,722 → bridge Burn 54,036 → bridge Admit 34,277 → bridge Burn 21,107 | 136,142 | 3% | 775 | 1,095,336 | applied |
| 70 | Sunset B by a signer with no role | bridge Sunset 11,977 → bridge Sunset 25,052 | 37,029 | 1% | 671 | 301,600 | refused: exit 253 (NOT_AUTHORIZED) |
| 71 | Sunset B by the admin | bridge Sunset 11,984 → bridge Sunset 25,052 | 37,036 | 1% | 671 | 301,656 | refused: exit 253 (NOT_AUTHORIZED) |
| 72 | Sunset sent to config(B) itself | bridge Sunset 23,861 | 23,861 | 0% | 431 | 194,336 | refused: exit 253 (NOT_AUTHORIZED) |
| 73 | Sunset B (freeze authority) | bridge Sunset 11,977 → bridge Sunset 32,127 | 44,104 | 1% | 671 | 358,200 | applied |
| 74 | Sunset B again | bridge Sunset 11,977 → bridge Sunset 25,512 | 37,489 | 1% | 671 | 305,280 | refused: exit 244 (ALREADY_WRITTEN) |
| 75 | ImportRoot B after Sunset | bridge ImportRoot 27,040 | 27,040 | 1% | 1,918 | 231,664 | refused: exit 242 (FROZEN) |
| 76 | Tick B: the record 30 days later | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,537 | 76,899 | 2% | 505 | 619,232 | applied |
| 77 | Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim 107,610 → bridge Issue 48,871 → bridge Admit 34,277 → bridge Spend 22,171 → bridge Mint 21,482 → bridge Mint 19,837 | 254,248 | 5% | 2,053 | 2,050,408 | applied |
| 78 | Burn 1 ETH (B) after Sunset: burns continue | bridge Burn 28,071 → bridge Burn 50,584 → bridge Admit 33,982 → bridge Burn 21,107 | 133,744 | 3% | 775 | 1,076,152 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 44,051 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A | bridge (reply) | 39,242 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B | bridge Init | 43,431 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B | bridge (reply) | 39,242 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Tick A: block 102's record | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: block 102's record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: block 102's record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Tick B: block 102's record | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: block 102's record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: block 102's record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register | 72,468 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (A) again | bridge Register | 79,151 | 1 | 3,148 | 2,554 | 520 | 6,517 | 906 | 0 |
| Register ETH (A) again | Attest | 164,757 | 1 | 2,837 | 2,366 | 333 | 3,651 | – | 0 |
| Register ETH (A) again | bridge (reply) | 73,259 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (A) again | bridge WriteToken | 34,790 | 1 | 1,824 | 843 | 843 | 0 | – | 244 |
| Register T2 (A): vault A holds no entry for it | bridge Register | 64,676 | 1 | 2,701 | 2,107 | 520 | 5,623 | 826 | 0 |
| Register T2 (A): vault A holds no entry for it | Attest | 146,721 | 1 | 2,470 | 1,999 | 333 | 3,284 | – | 0 |
| Register T2 (A): vault A holds no entry for it | bridge (reply) | 49,255 | 1 | 1,685 | 721 | 826 | 0 | – | 247 |
| Register ETH (A'): configuration never initialised | bridge Register | 20,416 | 1 | 2,295 | 2,221 | 0 | 0 | – | 245 |
| Register ETH (B) with other relay fees than the vault's | bridge Register | 75,369 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) with other relay fees than the vault's | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) with other relay fees than the vault's | bridge (reply) | 49,908 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register: nine denominations | bridge Register | 34,580 | 1 | 3,245 | 2,651 | 520 | 0 | – | 240 |
| Register: a denomination not a multiple of the LEZ unit | bridge Register | 35,904 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a relay fee of 2% | bridge Register | 36,108 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a LEZ unit that is not a power of ten | bridge Register | 35,333 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: schedule 0 with a mint fee | bridge Register | 40,021 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register ETH (B) | bridge Register | 75,371 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,478 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register T2 (B): a second token of the same configuration | bridge Register | 71,423 | 1 | 2,918 | 2,324 | 520 | 6,057 | 826 | 0 |
| Register T2 (B): a second token of the same configuration | Attest | 167,274 | 1 | 2,687 | 2,216 | 333 | 3,501 | – | 0 |
| Register T2 (B): a second token of the same configuration | bridge (reply) | 71,686 | 1 | 1,685 | 721 | 826 | 3,097 | 520 | 0 |
| Register T2 (B): a second token of the same configuration | bridge WriteToken | 30,440 | 1 | 901 | 763 | 0 | 1,689 | 763 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register | 80,232 | 1 | 3,322 | 2,728 | 520 | 6,865 | 906 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | Attest | 178,613 | 1 | 3,011 | 2,540 | 333 | 3,825 | – | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge (reply) | 73,259 | 1 | 1,765 | 721 | 906 | 3,257 | 520 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge WriteToken | 31,613 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A n=5 | bridge ImportRoot | 48,280 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A n=5 | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A n=5 | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=5 | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 49,847 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A n=8 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 48,083 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B n=8 | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 52,437 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 16,615 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Issue | 27,792 | 1 | 1,180 | 199 | 843 | 0 | – | 248 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees | 51,659 | 1 | 2,071 | 1,477 | 520 | 4,291 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | Attest | 103,426 | 1 | 1,848 | 1,377 | 333 | 2,428 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge (reply) | 42,247 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees | 48,630 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ExtendFees | 26,907 | 1 | 1,051 | 70 | 843 | 0 | – | 247 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees | 54,391 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge (reply) | 57,003 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ExtendFees | 41,515 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ImportFees | 54,391 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A) schedule 1 again | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 again | bridge (reply) | 57,003 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ExtendFees | 29,719 | 1 | 1,427 | 258 | 1,031 | 0 | – | 247 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Issue | 29,233 | 1 | 1,368 | 199 | 1,031 | 0 | – | 248 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,630 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,342 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 39,216 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees | 54,391 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge (reply) | 57,003 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ExtendFees | 30,357 | 1 | 1,239 | 258 | 843 | 0 | – | 240 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees | 54,391 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge (reply) | 57,003 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ExtendFees | 28,572 | 1 | 1,239 | 258 | 843 | 0 | – | 240 |
| Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim | 31,894 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim | 31,891 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 13,538 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,365 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| SetCap (admin) below the floor | bridge SetCap | 13,538 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 30,280 | 1 | 1,256 | 87 | 1,031 | 0 | – | 240 |
| SetCap by the freeze authority | bridge SetCap | 13,538 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by the freeze authority | bridge SetCap | 29,809 | 1 | 1,256 | 87 | 1,031 | 0 | – | 253 |
| SetCap by a signer with no role | bridge SetCap | 13,538 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by a signer with no role | bridge SetCap | 29,809 | 1 | 1,256 | 87 | 1,031 | 0 | – | 253 |
| SetCap sent to the token's account itself | bridge SetCap | 28,620 | 1 | 1,192 | 87 | 1,031 | 0 | – | 253 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Issue | 52,747 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Admit | 33,982 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Spend | 24,414 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Issue | 29,965 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Issue | 29,965 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): the record 10 s later | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the record 10 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the record 10 s later | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Issue | 52,712 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 19,553 | 1 | 341 | 129 | 74 | 440 | 74 | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 12,898 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 32,596 | 1 | 725 | 67 | 520 | 1,270 | 520 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Issue | 48,456 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Admit | 27,229 | 1 | 728 | 70 | 520 | 0 | – | 255 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Issue | 46,614 | 1 | 1,100 | 199 | 763 | 2,308 | 763 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Admit | 33,982 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Spend | 19,649 | 1 | 312 | 174 | 0 | 615 | 77 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Mint | 19,837 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 12,898 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 32,599 | 1 | 725 | 67 | 520 | 1,270 | 520 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 13,539 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,798 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 54,036 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 33,982 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 34,388 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 34,388 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A): the record 100 s later | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the record 100 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the record 100 s later | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 54,452 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A): a record 100 s later again | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 100 s later again | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): a record 100 s later again | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 13,539 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 42,827 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 42,314 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge (reply) | 36,629 | 1 | 1,338 | 337 | 863 | 2,214 | 843 | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Issue | 28,489 | 1 | 1,180 | 199 | 843 | 0 | – | 242 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 28,491 | 1 | 1,054 | 73 | 843 | 0 | – | 253 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 42,314 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze for 30 days and 1 ms after the record's time | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge (reply) | 26,496 | 1 | 1,338 | 337 | 863 | 0 | – | 240 |
| Freeze until the record's time itself (already past) | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze until the record's time itself (already past) | bridge Freeze | 42,314 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze until the record's time itself (already past) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze until the record's time itself (already past) | bridge (reply) | 26,485 | 1 | 1,338 | 337 | 863 | 0 | – | 240 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 42,314 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge (reply) | 36,617 | 1 | 1,338 | 337 | 863 | 2,214 | 843 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 12,680 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 36,403 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge (reply) | 32,564 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Issue | 48,871 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Admit | 27,199 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 46,164 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge (reply) | 40,915 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 34,359 | 1 | 1,354 | 185 | 1,031 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 13,454 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 46,164 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge (reply) | 40,901 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 12,680 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 36,403 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze (freeze authority): all burns of A for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge (reply) | 32,563 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 54,036 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Admit | 27,200 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 54,036 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Admit | 27,200 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Tick A: the record an hour later | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: the record an hour later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: the record an hour later | bridge (reply) | 32,536 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 54,036 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Admit | 34,277 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Sunset B by a signer with no role | bridge Sunset | 11,977 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by a signer with no role | bridge Sunset | 25,052 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset B by the admin | bridge Sunset | 11,984 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by the admin | bridge Sunset | 25,052 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset sent to config(B) itself | bridge Sunset | 23,861 | 1 | 627 | 33 | 520 | 0 | – | 253 |
| Sunset B (freeze authority) | bridge Sunset | 11,977 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B (freeze authority) | bridge Sunset | 32,127 | 1 | 691 | 33 | 520 | 1,236 | 520 | 0 |
| Sunset B again | bridge Sunset | 11,977 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B again | bridge Sunset | 25,512 | 1 | 691 | 33 | 520 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 27,040 | 1 | 1,986 | 1,392 | 520 | 0 | – | 242 |
| Tick B: the record 30 days later | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: the record 30 days later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: the record 30 days later | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Issue | 48,871 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Admit | 34,277 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 21,482 | 1 | 409 | 129 | 142 | 644 | 210 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 19,837 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 28,071 | 1 | 469 | 185 | 210 | 893 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 50,584 | 1 | 1,166 | 185 | 843 | 2,465 | 843 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Admit | 33,982 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |

## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Sunset B by a signer with no role | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B (freeze authority) | PublicRoot | 33 | Sunset | yes |
| Sunset B again | PublicRoot | 33 | Sunset | no (reverted) |
| ImportRoot B after Sunset | PublicRoot | 1392 | ImportRoot | no (reverted) |
| Burn 1 ETH (B) after Sunset: burns continue | PublicRoot | 185 | Burn | yes |
