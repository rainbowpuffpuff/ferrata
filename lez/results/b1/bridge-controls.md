# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,890 → Attest 86,790 → bridge (reply) 39,738 | 171,418 | 3% | 1,946 | 1,386,912 | applied |
| 2 | Init B | bridge Init 44,270 → Attest 84,699 → bridge (reply) 39,738 | 168,707 | 3% | 1,882 | 1,364,712 | applied |
| 3 | Tick A: block 102's record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 4 | Tick B: block 102's record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 5 | Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register 74,324 → Attest 151,273 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 333,303 | 7% | 2,747 | 2,688,400 | applied |
| 6 | Register ETH (A) again | bridge Register 81,051 → Attest 165,778 → bridge (reply) 75,130 → bridge WriteToken 35,309 | 357,268 | 7% | 3,080 | 2,882,784 | refused: exit 244 (ALREADY_WRITTEN) |
| 7 | Register T2 (A): vault A holds no entry for it | bridge Register 66,595 → Attest 147,735 → bridge (reply) 50,336 | 264,666 | 5% | 2,633 | 2,138,392 | refused: exit 247 (IMPORT_MISMATCH) |
| 8 | Register ETH (A'): configuration never initialised | bridge Register 20,942 | 20,942 | 0% | 2,812 | 190,032 | refused: exit 245 (NOT_FOUND) |
| 9 | Register ETH (B) with other relay fees than the vault's | bridge Register 77,277 → Attest 156,184 → bridge (reply) 50,989 | 284,450 | 6% | 2,770 | 2,297,760 | refused: exit 247 (IMPORT_MISMATCH) |
| 10 | Register: nine denominations | bridge Register 35,837 | 35,837 | 1% | 3,177 | 312,112 | refused: exit 240 (BAD_MESSAGE) |
| 11 | Register: a denomination not a multiple of the LEZ unit | bridge Register 37,159 | 37,159 | 1% | 2,770 | 319,432 | refused: exit 240 (BAD_MESSAGE) |
| 12 | Register: a relay fee of 2% | bridge Register 37,362 | 37,362 | 1% | 2,770 | 321,056 | refused: exit 240 (BAD_MESSAGE) |
| 13 | Register: a LEZ unit that is not a power of ten | bridge Register 36,589 | 36,589 | 1% | 2,770 | 314,872 | refused: exit 240 (BAD_MESSAGE) |
| 14 | Register: schedule 0 with a mint fee | bridge Register 41,265 | 41,265 | 1% | 2,770 | 352,280 | refused: exit 240 (BAD_MESSAGE) |
| 15 | Register ETH (B) | bridge Register 77,279 → Attest 156,184 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 341,169 | 7% | 2,770 | 2,751,512 | applied |
| 16 | Register T2 (B): a second token of the same configuration | bridge Register 73,335 → Attest 168,339 → bridge (reply) 73,544 → bridge WriteToken 31,305 | 346,523 | 7% | 2,850 | 2,794,984 | applied |
| 17 | Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register 82,127 → Attest 179,681 → bridge (reply) 75,130 → bridge WriteToken 32,357 | 369,295 | 7% | 3,254 | 2,980,392 | applied |
| 18 | ImportRoot A n=5 | bridge ImportRoot 50,522 → Attest 109,870 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 231,635 | 5% | 1,895 | 1,868,240 | applied |
| 19 | ImportRoot A n=8 | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,140 | 5% | 2,025 | 1,913,320 | applied |
| 20 | ImportRoot B n=8 | bridge ImportRoot 50,365 → Attest 115,561 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,169 | 5% | 1,918 | 1,912,696 | applied |
| 21 | Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim 107,796 → bridge Issue 26,985 | 134,781 | 3% | 2,053 | 1,094,672 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 22 | ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees 53,693 → Attest 104,326 → bridge (reply) 43,250 | 201,269 | 4% | 2,003 | 1,626,176 | refused: exit 247 (IMPORT_MISMATCH) |
| 23 | ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees 50,613 → Attest 108,223 → bridge (reply) 52,960 → bridge ExtendFees 27,225 | 239,021 | 5% | 1,945 | 1,927,728 | refused: exit 247 (IMPORT_MISMATCH) |
| 24 | ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 42,881 | 271,623 | 5% | 2,206 | 2,190,632 | applied |
| 25 | ImportFees ETH (A) schedule 1 again | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 30,054 | 258,796 | 5% | 2,206 | 2,088,016 | refused: exit 247 (IMPORT_MISMATCH) |
| 26 | Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim 107,796 → bridge Issue 28,431 | 136,227 | 3% | 2,053 | 1,106,240 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 27 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 50,613 → Attest 108,223 → bridge (reply) 52,960 → bridge ExtendFees 40,597 | 252,393 | 5% | 1,945 | 2,034,704 | applied |
| 28 | ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 30,685 | 259,427 | 5% | 2,206 | 2,093,064 | refused: exit 240 (BAD_MESSAGE) |
| 29 | ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 28,900 | 257,642 | 5% | 2,206 | 2,078,784 | refused: exit 240 (BAD_MESSAGE) |
| 30 | Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim 32,112 | 32,112 | 1% | 2,053 | 273,320 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 31 | Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim 32,109 | 32,109 | 1% | 2,053 | 273,296 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 32 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 16,352 → bridge SetCap 32,953 → bridge SetCap 42,528 | 91,833 | 2% | 789 | 740,976 | applied |
| 33 | SetCap (admin) below the floor | bridge SetCap 16,352 → bridge SetCap 32,953 → bridge SetCap 30,502 | 79,807 | 2% | 789 | 644,768 | refused: exit 240 (BAD_MESSAGE) |
| 34 | SetCap by the freeze authority | bridge SetCap 16,352 → bridge SetCap 28,331 | 44,683 | 1% | 789 | 363,776 | refused: exit 253 (NOT_AUTHORIZED) |
| 35 | SetCap by a signer with no role | bridge SetCap 16,352 → bridge SetCap 28,109 | 44,461 | 1% | 789 | 362,000 | refused: exit 253 (NOT_AUTHORIZED) |
| 36 | SetCap sent to the token's account itself | bridge SetCap 27,750 | 27,750 | 1% | 485 | 225,880 | refused: exit 253 (NOT_AUTHORIZED) |
| 37 | Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim 107,796 → bridge Issue 52,815 → bridge Admit 35,961 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 18,800 | 291,259 | 6% | 2,053 | 2,346,496 | applied |
| 38 | Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim 107,796 → bridge Issue 29,166 | 136,962 | 3% | 2,053 | 1,112,120 | refused: exit 255 (CAPPED) |
| 39 | Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim 107,796 → bridge Issue 29,166 | 136,962 | 3% | 2,053 | 1,112,120 | refused: exit 255 (CAPPED) |
| 40 | Tick ETH (A): the record 10 s later | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,964 | 95,719 | 2% | 525 | 769,952 | applied |
| 41 | Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim 107,796 → bridge Issue 52,950 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 23,177 | 270,454 | 5% | 2,053 | 2,180,056 | applied |
| 42 | SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap 13,540 → bridge SetCap 33,206 | 46,746 | 1% | 705 | 379,608 | applied |
| 43 | Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim 107,796 → bridge Issue 48,645 → bridge Admit 28,738 | 185,179 | 4% | 2,053 | 1,497,856 | refused: exit 255 (CAPPED) |
| 44 | Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim 107,796 → bridge Issue 46,853 → bridge Admit 35,961 → bridge Spend 25,003 → bridge Mint 18,800 | 234,413 | 5% | 2,053 | 1,891,728 | applied |
| 45 | SetCap (admin): global mints of B back to 200 ETH | bridge SetCap 13,540 → bridge SetCap 33,208 | 46,748 | 1% | 705 | 379,624 | applied |
| 46 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 16,353 → bridge SetCap 32,954 → bridge SetCap 42,963 | 92,270 | 2% | 789 | 744,472 | applied |
| 47 | Burn 1 ETH (A) | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,417 → bridge Admit 35,962 → bridge Burn 21,662 | 165,327 | 3% | 839 | 1,329,328 | applied |
| 48 | Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 33,322 | 88,608 | 2% | 839 | 715,576 | refused: exit 255 (CAPPED) |
| 49 | Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 33,322 | 88,608 | 2% | 839 | 715,576 | refused: exit 255 (CAPPED) |
| 50 | Tick ETH (A): the record 100 s later | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,964 | 95,719 | 2% | 525 | 769,952 | applied |
| 51 | Burn 1 ETH (A) after the Tick | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,833 → bridge Admit 35,963 → bridge Burn 21,662 | 165,744 | 3% | 839 | 1,332,664 | applied |
| 52 | Tick ETH (A): a record 100 s later again | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,964 | 95,719 | 2% | 525 | 769,952 | applied |
| 53 | SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap 16,353 → bridge SetCap 32,954 → bridge SetCap 42,990 | 92,297 | 2% | 789 | 744,688 | applied |
| 54 | Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 42,226 → Read 12,268 → bridge (reply) 36,773 | 140,383 | 3% | 839 | 1,129,776 | applied |
| 55 | Claim B's 1 ETH while its mints are frozen | bridge Claim 107,796 → bridge Issue 27,685 | 135,481 | 3% | 2,053 | 1,100,272 | refused: exit 242 (FROZEN) |
| 56 | Freeze by the admin: unfreeze ETH (B) | bridge Freeze 16,258 → bridge Freeze 28,324 | 44,582 | 1% | 839 | 363,368 | refused: exit 253 (NOT_AUTHORIZED) |
| 57 | Freeze for 30 days and 1 ms after the record's time | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 42,226 → Read 12,268 → bridge (reply) 26,994 | 130,604 | 3% | 839 | 1,051,544 | refused: exit 240 (BAD_MESSAGE) |
| 58 | Freeze until the record's time itself (already past) | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 42,226 → Read 12,268 → bridge (reply) 26,983 | 130,593 | 3% | 839 | 1,051,456 | refused: exit 240 (BAD_MESSAGE) |
| 59 | Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 42,226 → Read 12,268 → bridge (reply) 36,762 | 140,372 | 3% | 839 | 1,129,688 | applied |
| 60 | Freeze (freeze authority): all mints of B for 30 days | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,413 | 97,144 | 2% | 755 | 783,192 | applied |
| 61 | Claim B's 1 ETH under the global mint freeze | bridge Claim 107,796 → bridge Issue 49,060 → bridge Admit 28,708 | 185,564 | 4% | 2,053 | 1,500,936 | refused: exit 242 (FROZEN) |
| 62 | Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 45,961 → Read 12,268 → bridge (reply) 40,930 | 148,275 | 3% | 839 | 1,192,912 | applied |
| 63 | Burn 1 ETH (A) while its burns are frozen | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 33,293 | 88,579 | 2% | 839 | 715,344 | refused: exit 242 (FROZEN) |
| 64 | Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 45,961 → Read 12,268 → bridge (reply) 40,917 | 148,262 | 3% | 839 | 1,192,808 | applied |
| 65 | Freeze (freeze authority): all burns of A for one hour | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,413 | 97,144 | 2% | 755 | 783,192 | applied |
| 66 | Burn 1 ETH (A) under the global burn freeze | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,417 → bridge Admit 28,710 | 136,413 | 3% | 839 | 1,098,016 | refused: exit 242 (FROZEN) |
| 67 | Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,417 → bridge Admit 28,710 | 136,413 | 3% | 839 | 1,098,016 | refused: exit 242 (FROZEN) |
| 68 | Tick A: the record an hour later | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |
| 69 | Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn 24,855 → bridge Burn 30,431 → bridge Burn 52,417 → bridge Admit 36,257 → bridge Burn 21,662 | 165,622 | 3% | 839 | 1,331,688 | applied |
| 70 | Sunset B by a signer with no role | bridge Sunset 12,677 → bridge Sunset 25,058 | 37,735 | 1% | 671 | 307,248 | refused: exit 253 (NOT_AUTHORIZED) |
| 71 | Sunset B by the admin | bridge Sunset 12,677 → bridge Sunset 25,279 | 37,956 | 1% | 671 | 309,016 | refused: exit 253 (NOT_AUTHORIZED) |
| 72 | Sunset sent to config(B) itself | bridge Sunset 23,045 | 23,045 | 0% | 431 | 187,808 | refused: exit 253 (NOT_AUTHORIZED) |
| 73 | Sunset B (freeze authority) | bridge Sunset 12,677 → bridge Sunset 32,018 | 44,695 | 1% | 671 | 362,928 | applied |
| 74 | Sunset B again | bridge Sunset 12,677 → bridge Sunset 25,281 | 37,958 | 1% | 671 | 309,032 | refused: exit 244 (ALREADY_WRITTEN) |
| 75 | ImportRoot B after Sunset | bridge ImportRoot 28,566 | 28,566 | 1% | 1,918 | 243,872 | refused: exit 242 (FROZEN) |
| 76 | Tick B: the record 30 days later | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 77 | Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim 107,796 → bridge Issue 49,060 → bridge Admit 36,256 → bridge Spend 31,769 → bridge Mint 23,698 → bridge Mint 18,800 | 267,379 | 5% | 2,053 | 2,155,456 | applied |
| 78 | Burn 1 ETH (B) after Sunset: burns continue | bridge Burn 24,855 → bridge Burn 31,753 → bridge Burn 49,085 → bridge Admit 35,962 → bridge Burn 21,662 | 163,317 | 3% | 839 | 1,313,248 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 44,890 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Init B | bridge Init | 44,270 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B | Attest | 84,699 | 1 | 1,595 | 1,092 | 365 | 2,058 | – | 0 |
| Init B | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Tick A: block 102's record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: block 102's record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: block 102's record | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Tick B: block 102's record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick B: block 102's record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick B: block 102's record | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register | 74,324 | 1 | 2,875 | 2,221 | 580 | 5,971 | 966 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register ETH (A) again | bridge Register | 81,051 | 1 | 3,208 | 2,554 | 580 | 6,637 | 966 | 0 |
| Register ETH (A) again | Attest | 165,778 | 1 | 2,869 | 2,366 | 365 | 3,683 | – | 0 |
| Register ETH (A) again | bridge (reply) | 75,130 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (A) again | bridge WriteToken | 35,309 | 1 | 1,842 | 852 | 852 | 0 | – | 244 |
| Register T2 (A): vault A holds no entry for it | bridge Register | 66,595 | 1 | 2,761 | 2,107 | 580 | 5,743 | 886 | 0 |
| Register T2 (A): vault A holds no entry for it | Attest | 147,735 | 1 | 2,502 | 1,999 | 365 | 3,316 | – | 0 |
| Register T2 (A): vault A holds no entry for it | bridge (reply) | 50,336 | 1 | 1,745 | 721 | 886 | 0 | – | 247 |
| Register ETH (A'): configuration never initialised | bridge Register | 20,942 | 1 | 2,295 | 2,221 | 0 | 0 | – | 245 |
| Register ETH (B) with other relay fees than the vault's | bridge Register | 77,277 | 1 | 2,898 | 2,244 | 580 | 6,017 | 966 | 0 |
| Register ETH (B) with other relay fees than the vault's | Attest | 156,184 | 1 | 2,559 | 2,056 | 365 | 3,373 | – | 0 |
| Register ETH (B) with other relay fees than the vault's | bridge (reply) | 50,989 | 1 | 1,825 | 721 | 966 | 0 | – | 247 |
| Register: nine denominations | bridge Register | 35,837 | 1 | 3,305 | 2,651 | 580 | 0 | – | 240 |
| Register: a denomination not a multiple of the LEZ unit | bridge Register | 37,159 | 1 | 2,898 | 2,244 | 580 | 0 | – | 240 |
| Register: a relay fee of 2% | bridge Register | 37,362 | 1 | 2,898 | 2,244 | 580 | 0 | – | 240 |
| Register: a LEZ unit that is not a power of ten | bridge Register | 36,589 | 1 | 2,898 | 2,244 | 580 | 0 | – | 240 |
| Register: schedule 0 with a mint fee | bridge Register | 41,265 | 1 | 2,898 | 2,244 | 580 | 0 | – | 240 |
| Register ETH (B) | bridge Register | 77,279 | 1 | 2,898 | 2,244 | 580 | 6,017 | 966 | 0 |
| Register ETH (B) | Attest | 156,184 | 1 | 2,559 | 2,056 | 365 | 3,373 | – | 0 |
| Register ETH (B) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (B) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register T2 (B): a second token of the same configuration | bridge Register | 73,335 | 1 | 2,978 | 2,324 | 580 | 6,177 | 886 | 0 |
| Register T2 (B): a second token of the same configuration | Attest | 168,339 | 1 | 2,719 | 2,216 | 365 | 3,533 | – | 0 |
| Register T2 (B): a second token of the same configuration | bridge (reply) | 73,544 | 1 | 1,745 | 721 | 886 | 3,226 | 580 | 0 |
| Register T2 (B): a second token of the same configuration | bridge WriteToken | 31,305 | 1 | 910 | 772 | 0 | 1,707 | 772 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register | 82,127 | 1 | 3,382 | 2,728 | 580 | 6,985 | 966 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | Attest | 179,681 | 1 | 3,043 | 2,540 | 365 | 3,857 | – | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge (reply) | 75,130 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| ImportRoot A n=5 | bridge ImportRoot | 50,522 | 1 | 2,023 | 1,369 | 580 | 4,199 | 629 | 0 |
| ImportRoot A n=5 | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A n=5 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A n=5 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A n=8 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 50,365 | 1 | 2,046 | 1,392 | 580 | 4,245 | 629 | 0 |
| ImportRoot B n=8 | Attest | 115,561 | 1 | 1,976 | 1,473 | 365 | 2,556 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Issue | 26,985 | 1 | 1,189 | 199 | 852 | 0 | – | 248 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees | 53,693 | 1 | 2,131 | 1,477 | 580 | 4,411 | 806 | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge (reply) | 43,250 | 1 | 1,431 | 487 | 806 | 0 | – | 247 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees | 50,613 | 1 | 2,073 | 1,419 | 580 | 4,295 | 618 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ExtendFees | 27,225 | 1 | 1,060 | 70 | 852 | 0 | – | 247 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ExtendFees | 42,881 | 1 | 1,248 | 258 | 852 | 2,313 | 1,040 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees ETH (A) schedule 1 again | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH (A) schedule 1 again | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ExtendFees | 30,054 | 1 | 1,436 | 258 | 1,040 | 0 | – | 247 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Issue | 28,431 | 1 | 1,377 | 199 | 1,040 | 0 | – | 248 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 50,613 | 1 | 2,073 | 1,419 | 580 | 4,295 | 618 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 40,597 | 1 | 1,248 | 70 | 1,040 | 2,313 | 1,040 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ExtendFees | 30,685 | 1 | 1,248 | 258 | 852 | 0 | – | 240 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ExtendFees | 28,900 | 1 | 1,248 | 258 | 852 | 0 | – | 240 |
| Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim | 32,112 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim | 32,109 | 1 | 1,462 | 1,271 | 117 | 0 | – | 248 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 32,953 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,528 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| SetCap (admin) below the floor | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 32,953 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 30,502 | 1 | 1,265 | 87 | 1,040 | 0 | – | 240 |
| SetCap by the freeze authority | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by the freeze authority | bridge SetCap | 28,331 | 1 | 805 | 87 | 580 | 0 | – | 253 |
| SetCap by a signer with no role | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap by a signer with no role | bridge SetCap | 28,109 | 1 | 805 | 87 | 580 | 0 | – | 253 |
| SetCap sent to the token's account itself | bridge SetCap | 27,750 | 1 | 1,201 | 87 | 1,040 | 0 | – | 253 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Issue | 52,815 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Issue | 29,166 | 1 | 1,377 | 199 | 1,040 | 0 | – | 255 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 once a record 10 s later exists, no Tick: not refilled | bridge Issue | 29,166 | 1 | 1,377 | 199 | 1,040 | 0 | – | 255 |
| Tick ETH (A): the record 10 s later | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A): the record 10 s later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the record 10 s later | bridge (reply) | 40,964 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Issue | 52,950 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after the Tick (0.1 ETH refilled) | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 13,540 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 33,206 | 1 | 785 | 67 | 580 | 1,390 | 580 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Issue | 48,645 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Admit | 28,738 | 1 | 788 | 70 | 580 | 0 | – | 255 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Issue | 46,853 | 1 | 1,109 | 199 | 772 | 2,326 | 772 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Spend | 25,003 | 1 | 312 | 174 | 0 | 679 | 77 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 13,540 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 33,208 | 1 | 785 | 67 | 580 | 1,390 | 580 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 16,353 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 32,954 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,963 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| Burn 1 ETH (A) | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) | bridge Burn | 52,417 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (A) | bridge Burn | 21,662 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 33,322 | 1 | 1,363 | 185 | 1,040 | 0 | – | 255 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it | bridge Burn | 33,322 | 1 | 1,363 | 185 | 1,040 | 0 | – | 255 |
| Tick ETH (A): the record 100 s later | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A): the record 100 s later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the record 100 s later | bridge (reply) | 40,964 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 52,833 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Admit | 35,963 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 21,662 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A): a record 100 s later again | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A): a record 100 s later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record 100 s later again | bridge (reply) | 40,964 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 16,353 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 32,954 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 42,990 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 42,226 | 1 | 1,063 | 73 | 852 | 2,033 | 872 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge (reply) | 36,773 | 1 | 1,379 | 369 | 872 | 2,264 | 852 | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Issue | 27,685 | 1 | 1,189 | 199 | 852 | 0 | – | 242 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 28,324 | 1 | 791 | 73 | 580 | 0 | – | 253 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge Freeze | 42,226 | 1 | 1,063 | 73 | 852 | 2,033 | 872 | 0 |
| Freeze for 30 days and 1 ms after the record's time | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze for 30 days and 1 ms after the record's time | bridge (reply) | 26,994 | 1 | 1,379 | 369 | 872 | 0 | – | 240 |
| Freeze until the record's time itself (already past) | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze until the record's time itself (already past) | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze until the record's time itself (already past) | bridge Freeze | 42,226 | 1 | 1,063 | 73 | 852 | 2,033 | 872 | 0 |
| Freeze until the record's time itself (already past) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze until the record's time itself (already past) | bridge (reply) | 26,983 | 1 | 1,379 | 369 | 872 | 0 | – | 240 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 42,226 | 1 | 1,063 | 73 | 852 | 2,033 | 872 | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge (reply) | 36,762 | 1 | 1,379 | 369 | 872 | 2,264 | 852 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge (reply) | 34,413 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Issue | 49,060 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Admit | 28,708 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 45,961 | 1 | 1,251 | 73 | 1,040 | 2,409 | 1,060 | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge (reply) | 40,930 | 1 | 1,567 | 369 | 1,060 | 2,640 | 1,040 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 33,293 | 1 | 1,363 | 185 | 1,040 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 45,961 | 1 | 1,251 | 73 | 1,040 | 2,409 | 1,060 | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge (reply) | 40,917 | 1 | 1,567 | 369 | 1,060 | 2,640 | 1,040 | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze (freeze authority): all burns of A for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge (reply) | 34,413 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 52,417 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Admit | 28,710 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Burn | 52,417 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock | bridge Admit | 28,710 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Tick A: the record an hour later | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: the record an hour later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: the record an hour later | bridge (reply) | 34,214 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 30,431 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 52,417 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Admit | 36,257 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 21,662 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Sunset B by a signer with no role | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by a signer with no role | bridge Sunset | 25,058 | 1 | 751 | 33 | 580 | 0 | – | 253 |
| Sunset B by the admin | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by the admin | bridge Sunset | 25,279 | 1 | 751 | 33 | 580 | 0 | – | 253 |
| Sunset sent to config(B) itself | bridge Sunset | 23,045 | 1 | 687 | 33 | 580 | 0 | – | 253 |
| Sunset B (freeze authority) | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B (freeze authority) | bridge Sunset | 32,018 | 1 | 751 | 33 | 580 | 1,356 | 580 | 0 |
| Sunset B again | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B again | bridge Sunset | 25,281 | 1 | 751 | 33 | 580 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 28,566 | 1 | 2,046 | 1,392 | 580 | 0 | – | 242 |
| Tick B: the record 30 days later | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick B: the record 30 days later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick B: the record 30 days later | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Issue | 49,060 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Admit | 36,256 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 23,698 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 24,855 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 31,753 | 1 | 465 | 185 | 142 | 821 | 74 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 49,085 | 1 | 1,175 | 185 | 852 | 2,483 | 852 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 21,662 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |

## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Sunset B by a signer with no role | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B (freeze authority) | PublicRoot | 33 | Sunset | yes |
| Sunset B again | PublicRoot | 33 | Sunset | no (reverted) |
| ImportRoot B after Sunset | PublicRoot | 1392 | ImportRoot | no (reverted) |
| Burn 1 ETH (B) after Sunset: burns continue | PublicRoot | 185 | Burn | yes |
