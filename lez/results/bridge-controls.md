# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)

Bridge program image `25b3cc900f38579ad530335b994e3ae327ac600afe335b734bd00af78309e776`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,818 → Attest 85,725 → bridge (reply) 39,419 | 169,962 | 3% | 1,946 | 1,375,264 | applied |
| 2 | Init B | bridge Init 44,198 → Attest 83,634 → bridge (reply) 39,419 | 167,251 | 3% | 1,882 | 1,353,064 | applied |
| 3 | Tick A: block 102's record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 4 | Tick B: block 102's record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 5 | Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register 73,920 → Attest 150,201 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 330,744 | 7% | 2,747 | 2,667,928 | applied |
| 6 | Register ETH (A) again | bridge Register 80,680 → Attest 164,757 → bridge (reply) 74,257 → bridge WriteToken 34,994 | 354,688 | 7% | 3,080 | 2,862,144 | refused: exit 244 (ALREADY_WRITTEN) |
| 7 | Register T2 (A): vault A holds no entry for it | bridge Register 66,224 → Attest 146,721 → bridge (reply) 50,017 | 262,962 | 5% | 2,633 | 2,124,760 | refused: exit 247 (IMPORT_MISMATCH) |
| 8 | Register ETH (A'): configuration never initialised | bridge Register 20,895 | 20,895 | 0% | 2,812 | 189,656 | refused: exit 245 (NOT_FOUND) |
| 9 | Register ETH (B) with other relay fees than the vault's | bridge Register 76,862 → Attest 155,163 → bridge (reply) 50,670 | 282,695 | 6% | 2,770 | 2,283,720 | refused: exit 247 (IMPORT_MISMATCH) |
| 10 | Register: nine denominations | bridge Register 35,642 | 35,642 | 1% | 3,177 | 310,552 | refused: exit 240 (BAD_MESSAGE) |
| 11 | Register: a denomination not a multiple of the LEZ unit | bridge Register 36,964 | 36,964 | 1% | 2,770 | 317,872 | refused: exit 240 (BAD_MESSAGE) |
| 12 | Register: a relay fee of 2% | bridge Register 37,167 | 37,167 | 1% | 2,770 | 319,496 | refused: exit 240 (BAD_MESSAGE) |
| 13 | Register: a LEZ unit that is not a power of ten | bridge Register 36,394 | 36,394 | 1% | 2,770 | 313,312 | refused: exit 240 (BAD_MESSAGE) |
| 14 | Register: schedule 0 with a mint fee | bridge Register 41,070 | 41,070 | 1% | 2,770 | 350,720 | refused: exit 240 (BAD_MESSAGE) |
| 15 | Register ETH (B) | bridge Register 76,864 → Attest 155,163 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 338,650 | 7% | 2,770 | 2,731,360 | applied |
| 16 | Register T2 (B): a second token of the same configuration | bridge Register 72,964 → Attest 167,274 → bridge (reply) 72,789 → bridge WriteToken 30,974 | 344,001 | 7% | 2,850 | 2,774,808 | applied |
| 17 | Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register 81,712 → Attest 178,613 → bridge (reply) 74,257 → bridge WriteToken 32,147 | 366,729 | 7% | 3,254 | 2,959,864 | applied |
| 18 | ImportRoot A n=5 | bridge ImportRoot 50,128 → Attest 108,849 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 229,710 | 5% | 1,895 | 1,852,840 | applied |
| 19 | ImportRoot A n=8 | bridge ImportRoot 51,721 → Attest 112,749 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 235,203 | 5% | 2,025 | 1,897,824 | applied |
| 20 | ImportRoot B n=8 | bridge ImportRoot 49,908 → Attest 114,375 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 235,016 | 5% | 1,918 | 1,895,472 | applied |
| 21 | Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim 107,702 → bridge Issue 26,771 | 134,473 | 3% | 2,053 | 1,092,208 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees 53,169 → Attest 103,426 → bridge (reply) 42,997 | 199,592 | 4% | 2,003 | 1,612,760 | refused: exit 247 (IMPORT_MISMATCH) |
| 23 | ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees 50,198 → Attest 107,315 → bridge (reply) 52,487 → bridge ExtendFees 27,095 | 237,095 | 5% | 1,945 | 1,912,320 | refused: exit 247 (IMPORT_MISMATCH) |
| 24 | ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees 55,929 → Attest 112,749 → bridge (reply) 58,259 → bridge ExtendFees 41,726 | 268,663 | 5% | 2,206 | 2,166,952 | applied |
| 25 | ImportFees ETH (A) schedule 1 again | bridge ImportFees 55,929 → Attest 112,749 → bridge (reply) 58,259 → bridge ExtendFees 29,907 | 256,844 | 5% | 2,206 | 2,072,400 | refused: exit 247 (IMPORT_MISMATCH) |
| 26 | Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim 107,702 → bridge Issue 28,212 | 135,914 | 3% | 2,053 | 1,103,736 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 27 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 50,198 → Attest 107,315 → bridge (reply) 52,487 → bridge ExtendFees 39,427 | 249,427 | 5% | 1,945 | 2,010,976 | applied |
| 28 | ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees 55,929 → Attest 112,749 → bridge (reply) 58,259 → bridge ExtendFees 30,545 | 257,482 | 5% | 2,206 | 2,077,504 | refused: exit 240 (BAD_MESSAGE) |
| 29 | ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees 55,929 → Attest 112,749 → bridge (reply) 58,259 → bridge ExtendFees 28,760 | 255,697 | 5% | 2,206 | 2,063,224 | refused: exit 240 (BAD_MESSAGE) |
| 30 | Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim 32,018 | 32,018 | 1% | 2,053 | 272,568 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 31 | Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim 32,015 | 32,015 | 1% | 2,053 | 272,544 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 32 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 16,279 → bridge SetCap 32,683 → bridge SetCap 42,057 | 91,019 | 2% | 789 | 734,464 | applied |
| 33 | SetCap (admin) below the floor | bridge SetCap 16,279 → bridge SetCap 32,683 → bridge SetCap 30,244 | 79,206 | 2% | 789 | 639,960 | refused: exit 240 (BAD_MESSAGE) |
| 34 | SetCap by the freeze authority | bridge SetCap 16,279 → bridge SetCap 28,098 | 44,377 | 1% | 789 | 361,328 | refused: exit 253 (NOT_AUTHORIZED) |
| 35 | SetCap by a signer with no role | bridge SetCap 16,279 → bridge SetCap 27,876 | 44,155 | 1% | 789 | 359,552 | refused: exit 253 (NOT_AUTHORIZED) |
| 36 | SetCap sent to the token's account itself | bridge SetCap 27,514 | 27,514 | 1% | 485 | 223,992 | refused: exit 253 (NOT_AUTHORIZED) |
| 37 | Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim 107,702 → bridge Issue 51,585 → bridge Admit 35,564 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 18,161 | 286,625 | 6% | 2,053 | 2,309,424 | applied |
| 38 | Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim 107,702 → bridge Issue 28,947 | 136,649 | 3% | 2,053 | 1,109,616 | refused: exit 255 (CAPPED) |
| 39 | Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim 107,702 → bridge Issue 28,947 | 136,649 | 3% | 2,053 | 1,109,616 | refused: exit 255 (CAPPED) |
| 40 | Tick ETH (A): the record 10 s later | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,592 | 94,604 | 2% | 525 | 761,032 | applied |
| 41 | Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim 107,702 → bridge Issue 51,549 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 22,496 | 266,389 | 5% | 2,053 | 2,147,536 | applied |
| 42 | SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap 13,467 → bridge SetCap 32,809 | 46,276 | 1% | 705 | 375,848 | applied |
| 43 | Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 28,502 | 183,497 | 4% | 2,053 | 1,484,400 | refused: exit 255 (CAPPED) |
| 44 | Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim 107,702 → bridge Issue 45,451 → bridge Admit 35,564 → bridge Spend 24,293 → bridge Mint 22,784 | 235,794 | 5% | 2,053 | 1,902,776 | applied |
| 45 | SetCap (admin): global mints of B back to 200 ETH | bridge SetCap 13,467 → bridge SetCap 32,811 | 46,278 | 1% | 705 | 375,864 | applied |
| 46 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 16,280 → bridge SetCap 32,684 → bridge SetCap 42,492 | 91,456 | 2% | 789 | 737,960 | applied |
| 47 | Burn 1 ETH (A) | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 35,565 → bridge Burn 21,612 | 166,475 | 3% | 839 | 1,338,512 | applied |
| 48 | Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129 | 89,692 | 2% | 839 | 724,248 | refused: exit 255 (CAPPED) |
| 49 | Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129 | 89,692 | 2% | 839 | 724,248 | refused: exit 255 (CAPPED) |
| 50 | Tick ETH (A): the record 100 s later | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,592 | 94,604 | 2% | 525 | 761,032 | applied |
| 51 | Burn 1 ETH (A) after the Tick | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 53,151 → bridge Admit 35,566 → bridge Burn 21,612 | 166,892 | 3% | 839 | 1,341,848 | applied |
| 52 | Tick ETH (A): a record 100 s later again | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,592 | 94,604 | 2% | 525 | 761,032 | applied |
| 53 | SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap 16,280 → bridge SetCap 32,684 → bridge SetCap 42,519 | 91,483 | 2% | 789 | 738,176 | applied |
| 54 | Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 42,595 → Read 11,877 → bridge (reply) 36,513 | 139,607 | 3% | 839 | 1,123,568 | applied |
| 55 | Claim B's 1 ETH while its mints are frozen | bridge Claim 107,702 → bridge Issue 27,471 | 135,173 | 3% | 2,053 | 1,097,808 | refused: exit 242 (FROZEN) |
| 56 | Freeze by the admin: unfreeze ETH (B) | bridge Freeze 16,189 → bridge Freeze 28,095 | 44,284 | 1% | 839 | 360,984 | refused: exit 253 (NOT_AUTHORIZED) |
| 57 | Freeze for 30 days and 1 ms after the record's time | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 42,595 → Read 11,877 → bridge (reply) 26,357 | 129,451 | 3% | 839 | 1,042,320 | refused: exit 240 (BAD_MESSAGE) |
| 58 | Freeze until the record's time itself (already past) | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 42,595 → Read 11,877 → bridge (reply) 26,346 | 129,440 | 3% | 839 | 1,042,232 | refused: exit 240 (BAD_MESSAGE) |
| 59 | Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 42,595 → Read 11,877 → bridge (reply) 36,502 | 139,596 | 3% | 839 | 1,123,480 | applied |
| 60 | Freeze (freeze authority): all mints of B for 30 days | bridge Freeze 13,251 → bridge Freeze 36,542 → Read 11,877 → bridge (reply) 33,432 | 95,102 | 2% | 755 | 766,856 | applied |
| 61 | Claim B's 1 ETH under the global mint freeze | bridge Claim 107,702 → bridge Issue 47,708 → bridge Admit 28,472 | 183,882 | 4% | 2,053 | 1,487,480 | refused: exit 242 (FROZEN) |
| 62 | Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 46,445 → Read 11,877 → bridge (reply) 40,800 | 147,744 | 3% | 839 | 1,188,664 | applied |
| 63 | Burn 1 ETH (A) while its burns are frozen | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,100 | 89,663 | 2% | 839 | 724,016 | refused: exit 242 (FROZEN) |
| 64 | Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 46,445 → Read 11,877 → bridge (reply) 40,787 | 147,731 | 3% | 839 | 1,188,560 | applied |
| 65 | Freeze (freeze authority): all burns of A for one hour | bridge Freeze 13,251 → bridge Freeze 36,542 → Read 11,877 → bridge (reply) 33,432 | 95,102 | 2% | 755 | 766,856 | applied |
| 66 | Burn 1 ETH (A) under the global burn freeze | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 28,474 | 137,772 | 3% | 839 | 1,108,888 | refused: exit 242 (FROZEN) |
| 67 | Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 28,474 | 137,772 | 3% | 839 | 1,108,888 | refused: exit 242 (FROZEN) |
| 68 | Tick A: the record an hour later | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,234 | 79,626 | 2% | 505 | 641,048 | applied |
| 69 | Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 52,735 → bridge Admit 35,860 → bridge Burn 21,612 | 166,770 | 3% | 839 | 1,340,872 | applied |
| 70 | Sunset B by a signer with no role | bridge Sunset 12,611 → bridge Sunset 24,832 | 37,443 | 1% | 671 | 304,912 | refused: exit 253 (NOT_AUTHORIZED) |
| 71 | Sunset B by the admin | bridge Sunset 12,611 → bridge Sunset 25,053 | 37,664 | 1% | 671 | 306,680 | refused: exit 253 (NOT_AUTHORIZED) |
| 72 | Sunset sent to config(B) itself | bridge Sunset 22,818 | 22,818 | 0% | 431 | 185,992 | refused: exit 253 (NOT_AUTHORIZED) |
| 73 | Sunset B (freeze authority) | bridge Sunset 12,611 → bridge Sunset 31,582 | 44,193 | 1% | 671 | 358,912 | applied |
| 74 | Sunset B again | bridge Sunset 12,611 → bridge Sunset 25,055 | 37,666 | 1% | 671 | 306,696 | refused: exit 244 (ALREADY_WRITTEN) |
| 75 | ImportRoot B after Sunset | bridge ImportRoot 28,339 | 28,339 | 1% | 1,918 | 242,056 | refused: exit 242 (FROZEN) |
| 76 | Tick B: the record 30 days later | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 77 | Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim 107,702 → bridge Issue 47,708 → bridge Admit 35,859 → bridge Spend 30,916 → bridge Mint 24,538 → bridge Mint 22,784 | 269,507 | 5% | 2,053 | 2,172,480 | applied |
| 78 | Burn 1 ETH (B) after Sunset: burns continue | bridge Burn 24,662 → bridge Burn 33,236 → bridge Burn 49,283 → bridge Admit 35,565 → bridge Burn 21,612 | 164,358 | 3% | 839 | 1,321,576 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 44,818 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Init B | bridge Init | 44,198 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Tick A: block 102's record | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: block 102's record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: block 102's record | bridge (reply) | 33,235 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Tick B: block 102's record | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick B: block 102's record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: block 102's record | bridge (reply) | 33,235 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register | 73,920 | 1 | 2,859 | 2,221 | 564 | 5,939 | 950 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (A) again | bridge Register | 80,680 | 1 | 3,192 | 2,554 | 564 | 6,605 | 950 | 0 |
| Register ETH (A) again | Attest | 164,757 | 1 | 2,837 | 2,366 | 333 | 3,651 | – | 0 |
| Register ETH (A) again | bridge (reply) | 74,257 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (A) again | bridge WriteToken | 34,994 | 1 | 1,824 | 843 | 843 | 0 | – | 244 |
| Register T2 (A): vault A holds no entry for it | bridge Register | 66,224 | 1 | 2,745 | 2,107 | 564 | 5,711 | 870 | 0 |
| Register T2 (A): vault A holds no entry for it | Attest | 146,721 | 1 | 2,470 | 1,999 | 333 | 3,284 | – | 0 |
| Register T2 (A): vault A holds no entry for it | bridge (reply) | 50,017 | 1 | 1,729 | 721 | 870 | 0 | – | 247 |
| Register ETH (A'): configuration never initialised | bridge Register | 20,895 | 1 | 2,295 | 2,221 | 0 | 0 | – | 245 |
| Register ETH (B) with other relay fees than the vault's | bridge Register | 76,862 | 1 | 2,882 | 2,244 | 564 | 5,985 | 950 | 0 |
| Register ETH (B) with other relay fees than the vault's | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) with other relay fees than the vault's | bridge (reply) | 50,670 | 1 | 1,809 | 721 | 950 | 0 | – | 247 |
| Register: nine denominations | bridge Register | 35,642 | 1 | 3,289 | 2,651 | 564 | 0 | – | 240 |
| Register: a denomination not a multiple of the LEZ unit | bridge Register | 36,964 | 1 | 2,882 | 2,244 | 564 | 0 | – | 240 |
| Register: a relay fee of 2% | bridge Register | 37,167 | 1 | 2,882 | 2,244 | 564 | 0 | – | 240 |
| Register: a LEZ unit that is not a power of ten | bridge Register | 36,394 | 1 | 2,882 | 2,244 | 564 | 0 | – | 240 |
| Register: schedule 0 with a mint fee | bridge Register | 41,070 | 1 | 2,882 | 2,244 | 564 | 0 | – | 240 |
| Register ETH (B) | bridge Register | 76,864 | 1 | 2,882 | 2,244 | 564 | 5,985 | 950 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (B) | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register T2 (B): a second token of the same configuration | bridge Register | 72,964 | 1 | 2,962 | 2,324 | 564 | 6,145 | 870 | 0 |
| Register T2 (B): a second token of the same configuration | Attest | 167,274 | 1 | 2,687 | 2,216 | 333 | 3,501 | – | 0 |
| Register T2 (B): a second token of the same configuration | bridge (reply) | 72,789 | 1 | 1,729 | 721 | 870 | 3,185 | 564 | 0 |
| Register T2 (B): a second token of the same configuration | bridge WriteToken | 30,974 | 1 | 901 | 763 | 0 | 1,689 | 763 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register | 81,712 | 1 | 3,366 | 2,728 | 564 | 6,953 | 950 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | Attest | 178,613 | 1 | 3,011 | 2,540 | 333 | 3,825 | – | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge (reply) | 74,257 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A n=5 | bridge ImportRoot | 50,128 | 1 | 2,007 | 1,369 | 564 | 4,167 | 613 | 0 |
| ImportRoot A n=5 | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A n=5 | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A n=5 | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A n=8 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 49,908 | 1 | 2,030 | 1,392 | 564 | 4,213 | 613 | 0 |
| ImportRoot B n=8 | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Issue | 26,771 | 1 | 1,180 | 199 | 843 | 0 | – | 248 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees | 53,169 | 1 | 2,115 | 1,477 | 564 | 4,379 | 790 | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | Attest | 103,426 | 1 | 1,848 | 1,377 | 333 | 2,428 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge (reply) | 42,997 | 1 | 1,415 | 487 | 790 | 0 | – | 247 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees | 50,198 | 1 | 2,057 | 1,419 | 564 | 4,263 | 602 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ExtendFees | 27,095 | 1 | 1,051 | 70 | 843 | 0 | – | 247 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees | 55,929 | 1 | 2,318 | 1,680 | 564 | 4,785 | 790 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ExtendFees | 41,726 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ImportFees | 55,929 | 1 | 2,318 | 1,680 | 564 | 4,785 | 790 | 0 |
| ImportFees ETH (A) schedule 1 again | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 again | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ExtendFees | 29,907 | 1 | 1,427 | 258 | 1,031 | 0 | – | 247 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Issue | 28,212 | 1 | 1,368 | 199 | 1,031 | 0 | – | 248 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 50,198 | 1 | 2,057 | 1,419 | 564 | 4,263 | 602 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 39,427 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees | 55,929 | 1 | 2,318 | 1,680 | 564 | 4,785 | 790 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ExtendFees | 30,545 | 1 | 1,239 | 258 | 843 | 0 | – | 240 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees | 55,929 | 1 | 2,318 | 1,680 | 564 | 4,785 | 790 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ExtendFees | 28,760 | 1 | 1,239 | 258 | 843 | 0 | – | 240 |
| Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim | 32,018 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim | 32,015 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 32,683 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,057 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| SetCap (admin) below the floor | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 32,683 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 30,244 | 1 | 1,256 | 87 | 1,031 | 0 | – | 240 |
| SetCap by the freeze authority | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by the freeze authority | bridge SetCap | 28,098 | 1 | 789 | 87 | 564 | 0 | – | 253 |
| SetCap by a signer with no role | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by a signer with no role | bridge SetCap | 27,876 | 1 | 789 | 87 | 564 | 0 | – | 253 |
| SetCap sent to the token's account itself | bridge SetCap | 27,514 | 1 | 1,192 | 87 | 1,031 | 0 | – | 253 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Issue | 51,585 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Spend | 37,291 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Issue | 28,947 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Issue | 28,947 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): the record 10 s later | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the record 10 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the record 10 s later | bridge (reply) | 40,592 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Issue | 51,549 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 13,467 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 32,809 | 1 | 769 | 67 | 564 | 1,358 | 564 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Issue | 47,293 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Admit | 28,502 | 1 | 772 | 70 | 564 | 0 | – | 255 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Issue | 45,451 | 1 | 1,100 | 199 | 763 | 2,308 | 763 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Spend | 24,293 | 1 | 312 | 174 | 0 | 679 | 77 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Mint | 22,784 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 13,467 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 32,811 | 1 | 769 | 67 | 564 | 1,358 | 564 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 16,280 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 32,684 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,492 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 52,735 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (A) | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 33,129 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 33,129 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A): the record 100 s later | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the record 100 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the record 100 s later | bridge (reply) | 40,592 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 53,151 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Admit | 35,566 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A): a record 100 s later again | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 100 s later again | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): a record 100 s later again | bridge (reply) | 40,592 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 16,280 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 32,684 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 42,519 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 42,595 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge (reply) | 36,513 | 1 | 1,338 | 337 | 863 | 2,214 | 843 | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Issue | 27,471 | 1 | 1,180 | 199 | 843 | 0 | – | 242 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 28,095 | 1 | 775 | 73 | 564 | 0 | – | 253 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 42,595 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze for 30 days and 1 ms after the record's time | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge (reply) | 26,357 | 1 | 1,338 | 337 | 863 | 0 | – | 240 |
| Freeze until the record's time itself (already past) | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze until the record's time itself (already past) | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze until the record's time itself (already past) | bridge Freeze | 42,595 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze until the record's time itself (already past) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze until the record's time itself (already past) | bridge (reply) | 26,346 | 1 | 1,338 | 337 | 863 | 0 | – | 240 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 42,595 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge (reply) | 36,502 | 1 | 1,338 | 337 | 863 | 2,214 | 843 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 36,542 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge (reply) | 33,432 | 1 | 1,060 | 337 | 585 | 1,657 | 564 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Issue | 47,708 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Admit | 28,472 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 46,445 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge (reply) | 40,800 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 33,100 | 1 | 1,354 | 185 | 1,031 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 46,445 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge (reply) | 40,787 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 36,542 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze (freeze authority): all burns of A for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge (reply) | 33,432 | 1 | 1,060 | 337 | 585 | 1,657 | 564 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 52,735 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Admit | 28,474 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 52,735 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Admit | 28,474 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Tick A: the record an hour later | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: the record an hour later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: the record an hour later | bridge (reply) | 33,234 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 52,735 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Admit | 35,860 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Sunset B by a signer with no role | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by a signer with no role | bridge Sunset | 24,832 | 1 | 735 | 33 | 564 | 0 | – | 253 |
| Sunset B by the admin | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by the admin | bridge Sunset | 25,053 | 1 | 735 | 33 | 564 | 0 | – | 253 |
| Sunset sent to config(B) itself | bridge Sunset | 22,818 | 1 | 671 | 33 | 564 | 0 | – | 253 |
| Sunset B (freeze authority) | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B (freeze authority) | bridge Sunset | 31,582 | 1 | 735 | 33 | 564 | 1,324 | 564 | 0 |
| Sunset B again | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B again | bridge Sunset | 25,055 | 1 | 735 | 33 | 564 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 28,339 | 1 | 2,030 | 1,392 | 564 | 0 | – | 242 |
| Tick B: the record 30 days later | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick B: the record 30 days later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: the record 30 days later | bridge (reply) | 33,235 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Issue | 47,708 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Admit | 35,859 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 24,538 | 1 | 441 | 161 | 142 | 676 | 210 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 22,784 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 33,236 | 1 | 533 | 185 | 210 | 957 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 49,283 | 1 | 1,166 | 185 | 843 | 2,465 | 843 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |

## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Sunset B by a signer with no role | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B (freeze authority) | PublicRoot | 33 | Sunset | yes |
| Sunset B again | PublicRoot | 33 | Sunset | no (reverted) |
| ImportRoot B after Sunset | PublicRoot | 1392 | ImportRoot | no (reverted) |
| Burn 1 ETH (B) after Sunset: burns continue | PublicRoot | 185 | Burn | yes |
