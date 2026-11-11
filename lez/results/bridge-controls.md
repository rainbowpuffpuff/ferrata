# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)

Bridge program image `fe2f248724ddbeada455f2f132ae178d84434db3f26ddce50e06d72cc7ae946d`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 43,530 → Attest 85,725 → bridge (reply) 39,116 | 168,371 | 3% | 1,946 | 1,362,536 | applied |
| 2 | Init B | bridge Init 42,910 → Attest 83,634 → bridge (reply) 39,116 | 165,660 | 3% | 1,882 | 1,340,336 | applied |
| 3 | Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register 71,961 → Attest 150,201 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 324,257 | 6% | 2,747 | 2,616,032 | applied |
| 4 | Register ETH (A) again | bridge Register 78,644 → Attest 164,757 → bridge (reply) 72,964 → bridge WriteToken 31,163 | 347,528 | 7% | 3,080 | 2,804,864 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Register T2 (A): vault A holds no entry for it | bridge Register 64,169 → Attest 146,721 → bridge (reply) 49,066 | 259,956 | 5% | 2,633 | 2,100,712 | refused: exit 247 (IMPORT_MISMATCH) |
| 6 | Register ETH (A'): configuration never initialised | bridge Register 19,983 | 19,983 | 0% | 2,812 | 182,360 | refused: exit 245 (NOT_FOUND) |
| 7 | Register ETH (B) with other relay fees than the vault's | bridge Register 74,862 → Attest 155,163 → bridge (reply) 49,719 | 279,744 | 6% | 2,770 | 2,260,112 | refused: exit 247 (IMPORT_MISMATCH) |
| 8 | Register: nine denominations | bridge Register 34,151 | 34,151 | 1% | 3,177 | 298,624 | refused: exit 240 (BAD_MESSAGE) |
| 9 | Register: a denomination not a multiple of the LEZ unit | bridge Register 35,475 | 35,475 | 1% | 2,770 | 305,960 | refused: exit 240 (BAD_MESSAGE) |
| 10 | Register: a relay fee of 2% | bridge Register 35,679 | 35,679 | 1% | 2,770 | 307,592 | refused: exit 240 (BAD_MESSAGE) |
| 11 | Register: a LEZ unit that is not a power of ten | bridge Register 34,904 | 34,904 | 1% | 2,770 | 301,392 | refused: exit 240 (BAD_MESSAGE) |
| 12 | Register: schedule 0 with a mint fee | bridge Register 39,592 | 39,592 | 1% | 2,770 | 338,896 | refused: exit 240 (BAD_MESSAGE) |
| 13 | Register ETH (B) | bridge Register 74,864 → Attest 155,163 → bridge (reply) 73,183 → bridge WriteToken 28,912 | 332,122 | 7% | 2,770 | 2,679,136 | applied |
| 14 | Register T2 (B): a second token of the same configuration | bridge Register 70,916 → Attest 167,274 → bridge (reply) 71,389 → bridge WriteToken 27,924 | 337,503 | 7% | 2,850 | 2,722,824 | applied |
| 15 | Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register 79,725 → Attest 178,613 → bridge (reply) 72,964 → bridge WriteToken 28,912 | 360,214 | 7% | 3,254 | 2,907,744 | applied |
| 16 | ImportRoot A n=5 | bridge ImportRoot 47,970 → Attest 108,849 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 225,793 | 5% | 1,895 | 1,821,504 | applied |
| 17 | ImportRoot A n=8 | bridge ImportRoot 49,537 → Attest 112,749 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 231,260 | 5% | 2,025 | 1,866,280 | applied |
| 18 | ImportRoot B n=8 | bridge ImportRoot 47,773 → Attest 114,375 → bridge (reply) 52,749 → bridge WriteRoot 16,225 | 231,122 | 5% | 1,918 | 1,864,320 | applied |
| 19 | Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim 107,397 → bridge Issue 24,439 | 131,836 | 3% | 2,061 | 1,071,176 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 20 | ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees 51,263 → Attest 103,426 → bridge (reply) 42,048 | 196,737 | 4% | 2,003 | 1,589,920 | refused: exit 247 (IMPORT_MISMATCH) |
| 21 | ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees 48,234 → Attest 107,315 → bridge (reply) 51,594 → bridge ExtendFees 24,493 | 231,636 | 5% | 1,945 | 1,868,648 | refused: exit 247 (IMPORT_MISMATCH) |
| 22 | ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees 53,995 → Attest 112,749 → bridge (reply) 57,255 → bridge ExtendFees 38,021 | 262,020 | 5% | 2,206 | 2,113,808 | applied |
| 23 | ImportFees ETH (A) schedule 1 again | bridge ImportFees 53,995 → Attest 112,749 → bridge (reply) 57,255 → bridge ExtendFees 27,191 | 251,190 | 5% | 2,206 | 2,027,168 | refused: exit 247 (IMPORT_MISMATCH) |
| 24 | Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim 107,397 → bridge Issue 25,880 | 133,277 | 3% | 2,061 | 1,082,704 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 25 | ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees 48,234 → Attest 107,315 → bridge (reply) 51,594 → bridge ExtendFees 36,021 | 243,164 | 5% | 1,945 | 1,960,872 | applied |
| 26 | ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees 53,995 → Attest 112,749 → bridge (reply) 57,255 → bridge ExtendFees 27,829 | 251,828 | 5% | 2,206 | 2,032,272 | refused: exit 240 (BAD_MESSAGE) |
| 27 | ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees 53,995 → Attest 112,749 → bridge (reply) 57,255 → bridge ExtendFees 26,044 | 250,043 | 5% | 2,206 | 2,017,992 | refused: exit 240 (BAD_MESSAGE) |
| 28 | Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim 31,664 | 31,664 | 1% | 2,061 | 269,800 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 29 | Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim 31,661 | 31,661 | 1% | 2,061 | 269,776 | refused: exit 248 (LEAF_NOT_IMPORTED) |
| 30 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 12,729 → bridge SetCap 36,775 | 49,504 | 1% | 733 | 401,896 | applied |
| 31 | SetCap (admin) below the floor | bridge SetCap 12,729 → bridge SetCap 26,531 | 39,260 | 1% | 733 | 319,944 | refused: exit 240 (BAD_MESSAGE) |
| 32 | SetCap by the freeze authority | bridge SetCap 12,729 → bridge SetCap 26,065 | 38,794 | 1% | 733 | 316,216 | refused: exit 253 (NOT_AUTHORIZED) |
| 33 | SetCap by a signer with no role | bridge SetCap 12,729 → bridge SetCap 26,065 | 38,794 | 1% | 733 | 316,216 | refused: exit 253 (NOT_AUTHORIZED) |
| 34 | SetCap sent to the token's account itself | bridge SetCap 25,017 | 25,017 | 1% | 493 | 204,080 | refused: exit 253 (NOT_AUTHORIZED) |
| 35 | Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim 107,397 → bridge Issue 47,023 → bridge Admit 34,187 → bridge Spend 36,393 → bridge Mint 17,155 → bridge Mint 17,155 → bridge Mint 17,155 | 276,465 | 6% | 2,061 | 2,228,208 | applied |
| 36 | Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim 107,397 → bridge Issue 26,623 | 134,020 | 3% | 2,061 | 1,088,648 | refused: exit 255 (CAPPED) |
| 37 | Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Claim 107,397 → bridge Issue 47,102 → bridge Admit 34,277 → bridge Spend 30,034 → bridge Mint 17,155 → bridge Mint 21,882 | 257,847 | 5% | 2,061 | 2,079,264 | applied |
| 38 | SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap 12,220 → bridge SetCap 31,845 | 44,065 | 1% | 713 | 358,224 | applied |
| 39 | Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim 107,397 → bridge Issue 44,167 → bridge Admit 27,045 | 178,609 | 4% | 2,061 | 1,445,360 | refused: exit 255 (CAPPED) |
| 40 | Claim B's T2 with a time one hour ahead of the block | bridge Claim 107,397 → bridge Issue 42,401 → bridge Admit 34,298 → bridge Spend 23,427 → bridge Mint 22,170 | 10,000,000 | 200% | 2,061 | 80,016,488 | reverted: Execution outside of the validity window |
| 41 | Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim 107,397 → bridge Issue 42,401 → bridge Admit 33,898 → bridge Spend 23,427 → bridge Mint 22,170 | 229,293 | 5% | 2,061 | 1,850,832 | applied |
| 42 | SetCap (admin): global mints of B back to 200 ETH | bridge SetCap 12,220 → bridge SetCap 31,573 | 43,793 | 1% | 713 | 356,048 | applied |
| 43 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 12,730 → bridge SetCap 36,777 | 49,507 | 1% | 733 | 401,920 | applied |
| 44 | Burn 1 ETH (A) | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 48,767 → bridge Admit 34,146 → bridge Burn 20,661 | 158,615 | 3% | 839 | 1,275,632 | applied |
| 45 | Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 30,899 | 85,940 | 2% | 839 | 694,232 | refused: exit 255 (CAPPED) |
| 46 | Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 30,899 | 85,940 | 2% | 839 | 694,232 | refused: exit 255 (CAPPED) |
| 47 | Tick ETH (A) | bridge Tick 32,765 | 32,765 | 1% | 460 | 265,800 | applied |
| 48 | Burn 1 ETH (A) after the Tick | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 49,179 → bridge Admit 33,857 → bridge Burn 20,661 | 158,738 | 3% | 839 | 1,276,616 | applied |
| 49 | SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap 12,730 → bridge SetCap 36,903 | 49,633 | 1% | 733 | 402,928 | applied |
| 50 | Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze 12,426 → bridge Freeze 34,003 | 46,429 | 1% | 710 | 377,112 | applied |
| 51 | Claim B's 1 ETH while its mints are frozen | bridge Claim 107,397 → bridge Issue 25,149 | 132,546 | 3% | 2,061 | 1,076,856 | refused: exit 242 (FROZEN) |
| 52 | Freeze by the admin: unfreeze ETH (B) | bridge Freeze 12,426 → bridge Freeze 24,510 | 36,936 | 1% | 710 | 301,168 | refused: exit 253 (NOT_AUTHORIZED) |
| 53 | Freeze for 30 days and 1 ms | bridge Freeze 12,426 → bridge Freeze 24,984 | 37,410 | 1% | 710 | 304,960 | refused: exit 240 (BAD_MESSAGE) |
| 54 | Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze 12,426 → bridge Freeze 33,985 | 46,411 | 1% | 710 | 376,968 | applied |
| 55 | Freeze (freeze authority): all mints of B for 30 days | bridge Freeze 11,814 → bridge Freeze 31,323 | 43,137 | 1% | 690 | 350,616 | applied |
| 56 | Claim B's 1 ETH under the global mint freeze | bridge Claim 107,397 → bridge Issue 44,168 → bridge Admit 27,017 | 178,582 | 4% | 2,061 | 1,445,144 | refused: exit 242 (FROZEN) |
| 57 | Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze 12,426 → bridge Freeze 37,007 | 49,433 | 1% | 710 | 401,144 | applied |
| 58 | Burn 1 ETH (A) while its burns are frozen | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 30,872 | 85,913 | 2% | 839 | 694,016 | refused: exit 242 (FROZEN) |
| 59 | Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze 12,426 → bridge Freeze 36,990 | 49,416 | 1% | 710 | 401,008 | applied |
| 60 | Freeze (freeze authority): all burns of A for one hour | bridge Freeze 11,814 → bridge Freeze 31,322 | 43,136 | 1% | 690 | 350,608 | applied |
| 61 | Burn 1 ETH (A) under the global burn freeze | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 48,767 → bridge Admit 26,958 | 130,766 | 3% | 839 | 1,052,840 | refused: exit 242 (FROZEN) |
| 62 | Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 48,767 → bridge Admit 26,958 | 130,766 | 3% | 839 | 1,052,840 | refused: exit 242 (FROZEN) |
| 63 | Tick A | bridge Tick 27,157 | 27,157 | 1% | 440 | 220,776 | applied |
| 64 | Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn 23,688 → bridge Burn 31,353 → bridge Burn 48,767 → bridge Admit 34,147 → bridge Burn 20,661 | 158,616 | 3% | 839 | 1,275,640 | applied |
| 65 | Sunset B by a signer with no role | bridge Sunset 11,086 → bridge Sunset 23,714 | 34,800 | 1% | 671 | 283,768 | refused: exit 253 (NOT_AUTHORIZED) |
| 66 | Sunset B by the admin | bridge Sunset 11,086 → bridge Sunset 23,714 | 34,800 | 1% | 671 | 283,768 | refused: exit 253 (NOT_AUTHORIZED) |
| 67 | Sunset sent to config(B) itself | bridge Sunset 22,530 | 22,530 | 0% | 431 | 183,688 | refused: exit 253 (NOT_AUTHORIZED) |
| 68 | Sunset B (freeze authority) | bridge Sunset 11,086 → bridge Sunset 30,790 | 41,876 | 1% | 671 | 340,376 | applied |
| 69 | Sunset B again | bridge Sunset 11,086 → bridge Sunset 24,173 | 35,259 | 1% | 671 | 287,440 | refused: exit 244 (ALREADY_WRITTEN) |
| 70 | ImportRoot B after Sunset | bridge ImportRoot 26,702 | 26,702 | 1% | 1,918 | 228,960 | refused: exit 242 (FROZEN) |
| 71 | Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim 107,397 → bridge Issue 44,167 → bridge Admit 34,188 → bridge Spend 30,034 → bridge Mint 23,932 → bridge Mint 22,170 | 261,888 | 5% | 2,061 | 2,111,592 | applied |
| 72 | Burn 1 ETH (B) after Sunset: burns continue | bridge Burn 23,688 → bridge Burn 32,696 → bridge Burn 46,081 → bridge Admit 34,146 → bridge Burn 20,661 | 157,272 | 3% | 839 | 1,264,888 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A | bridge Init | 43,530 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A | Attest | 85,725 | 1 | 1,627 | 1,156 | 333 | 2,090 | – | 0 |
| Init A | bridge (reply) | 39,116 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Init B | bridge Init | 42,910 | 1 | 1,429 | 1,355 | 0 | 3,008 | 390 | 0 |
| Init B | Attest | 83,634 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| Init B | bridge (reply) | 39,116 | 1 | 898 | 370 | 390 | 1,443 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge Register | 71,961 | 1 | 2,815 | 2,221 | 520 | 5,851 | 906 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (A): entry, schedules 0 and 1, count at block 100 | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register ETH (A) again | bridge Register | 78,644 | 1 | 3,148 | 2,554 | 520 | 6,517 | 906 | 0 |
| Register ETH (A) again | Attest | 164,757 | 1 | 2,837 | 2,366 | 333 | 3,651 | – | 0 |
| Register ETH (A) again | bridge (reply) | 72,964 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (A) again | bridge WriteToken | 31,163 | 1 | 1,694 | 778 | 778 | 0 | – | 244 |
| Register T2 (A): vault A holds no entry for it | bridge Register | 64,169 | 1 | 2,701 | 2,107 | 520 | 5,623 | 826 | 0 |
| Register T2 (A): vault A holds no entry for it | Attest | 146,721 | 1 | 2,470 | 1,999 | 333 | 3,284 | – | 0 |
| Register T2 (A): vault A holds no entry for it | bridge (reply) | 49,066 | 1 | 1,685 | 721 | 826 | 0 | – | 247 |
| Register ETH (A'): configuration never initialised | bridge Register | 19,983 | 1 | 2,295 | 2,221 | 0 | 0 | – | 245 |
| Register ETH (B) with other relay fees than the vault's | bridge Register | 74,862 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) with other relay fees than the vault's | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) with other relay fees than the vault's | bridge (reply) | 49,719 | 1 | 1,765 | 721 | 906 | 0 | – | 247 |
| Register: nine denominations | bridge Register | 34,151 | 1 | 3,245 | 2,651 | 520 | 0 | – | 240 |
| Register: a denomination not a multiple of the LEZ unit | bridge Register | 35,475 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a relay fee of 2% | bridge Register | 35,679 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: a LEZ unit that is not a power of ten | bridge Register | 34,904 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register: schedule 0 with a mint fee | bridge Register | 39,592 | 1 | 2,838 | 2,244 | 520 | 0 | – | 240 |
| Register ETH (B) | bridge Register | 74,864 | 1 | 2,838 | 2,244 | 520 | 5,897 | 906 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 73,183 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register ETH (B) | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| Register T2 (B): a second token of the same configuration | bridge Register | 70,916 | 1 | 2,918 | 2,324 | 520 | 6,057 | 826 | 0 |
| Register T2 (B): a second token of the same configuration | Attest | 167,274 | 1 | 2,687 | 2,216 | 333 | 3,501 | – | 0 |
| Register T2 (B): a second token of the same configuration | bridge (reply) | 71,389 | 1 | 1,685 | 721 | 826 | 3,032 | 520 | 0 |
| Register T2 (B): a second token of the same configuration | bridge WriteToken | 27,924 | 1 | 836 | 698 | 0 | 1,559 | 698 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge Register | 79,725 | 1 | 3,322 | 2,728 | 520 | 6,865 | 906 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | Attest | 178,613 | 1 | 3,011 | 2,540 | 333 | 3,825 | – | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge (reply) | 72,964 | 1 | 1,765 | 721 | 906 | 3,192 | 520 | 0 |
| Register T3 (A) at block 101: schedule 1 present, covers nothing yet | bridge WriteToken | 28,912 | 1 | 916 | 778 | 0 | 1,719 | 778 | 0 |
| ImportRoot A n=5 | bridge ImportRoot | 47,970 | 1 | 1,963 | 1,369 | 520 | 4,079 | 569 | 0 |
| ImportRoot A n=5 | Attest | 108,849 | 1 | 1,921 | 1,450 | 333 | 2,501 | – | 0 |
| ImportRoot A n=5 | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=5 | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 49,537 | 1 | 2,093 | 1,499 | 520 | 4,339 | 569 | 0 |
| ImportRoot A n=8 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 47,773 | 1 | 1,986 | 1,392 | 520 | 4,125 | 569 | 0 |
| ImportRoot B n=8 | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 52,749 | 1 | 1,194 | 487 | 569 | 1,960 | 520 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 16,225 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 6 before its schedule is known: range [0, 8) | bridge Issue | 24,439 | 1 | 1,123 | 207 | 778 | 0 | – | 248 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge ImportFees | 51,263 | 1 | 2,071 | 1,477 | 520 | 4,291 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | Attest | 103,426 | 1 | 1,848 | 1,377 | 333 | 2,428 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 100 (not there yet) | bridge (reply) | 42,048 | 1 | 1,371 | 487 | 746 | 0 | – | 247 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ImportFees | 48,234 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): schedule 2 before 1 | bridge ExtendFees | 24,493 | 1 | 986 | 70 | 778 | 0 | – | 247 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ImportFees | 53,995 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge (reply) | 57,255 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 at block 101 | bridge ExtendFees | 38,021 | 1 | 1,174 | 258 | 778 | 2,165 | 966 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ImportFees | 53,995 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees ETH (A) schedule 1 again | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A) schedule 1 again | bridge (reply) | 57,255 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees ETH (A) schedule 1 again | bridge ExtendFees | 27,191 | 1 | 1,362 | 258 | 966 | 0 | – | 247 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 6 in schedule 1's range [6, 8) before it is closed | bridge Issue | 25,880 | 1 | 1,311 | 207 | 966 | 0 | – | 248 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ImportFees | 48,234 | 1 | 2,013 | 1,419 | 520 | 4,175 | 558 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge (reply) | 51,594 | 1 | 1,183 | 487 | 558 | 1,902 | 520 | 0 |
| ImportFees ETH (A): no schedule 2 at block 101 (8 leaves) | bridge ExtendFees | 36,021 | 1 | 1,174 | 70 | 966 | 2,165 | 966 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ImportFees | 53,995 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge (reply) | 57,255 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: one day of notice | bridge ExtendFees | 27,829 | 1 | 1,174 | 258 | 778 | 0 | – | 240 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ImportFees | 53,995 | 1 | 2,274 | 1,680 | 520 | 4,697 | 746 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge (reply) | 57,255 | 1 | 1,371 | 487 | 746 | 2,278 | 520 | 0 |
| ImportFees T3 (A) schedule 1: a 2% mint fee | bridge ExtendFees | 26,044 | 1 | 1,174 | 258 | 778 | 0 | – | 240 |
| Claim leaf 6 under schedule 0's range [0, 6) | bridge Claim | 31,664 | 1 | 1,470 | 1,279 | 117 | 0 | – | 248 |
| Claim leaf 2 under schedule 1's range [6, 8) | bridge Claim | 31,661 | 1 | 1,470 | 1,279 | 117 | 0 | – | 248 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 12,729 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 36,775 | 1 | 1,199 | 95 | 966 | 2,206 | 966 | 0 |
| SetCap (admin) below the floor | bridge SetCap | 12,729 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin) below the floor | bridge SetCap | 26,531 | 1 | 1,199 | 95 | 966 | 0 | – | 240 |
| SetCap by the freeze authority | bridge SetCap | 12,729 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap by the freeze authority | bridge SetCap | 26,065 | 1 | 1,199 | 95 | 966 | 0 | – | 253 |
| SetCap by a signer with no role | bridge SetCap | 12,729 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap by a signer with no role | bridge SetCap | 26,065 | 1 | 1,199 | 95 | 966 | 0 | – | 253 |
| SetCap sent to the token's account itself | bridge SetCap | 25,017 | 1 | 1,135 | 95 | 966 | 0 | – | 253 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Issue | 47,023 | 1 | 1,311 | 207 | 966 | 2,843 | 966 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Admit | 34,187 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Spend | 36,393 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket | bridge Issue | 26,623 | 1 | 1,311 | 207 | 966 | 0 | – | 255 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Issue | 47,102 | 1 | 1,311 | 207 | 966 | 2,795 | 966 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Admit | 34,277 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Mint | 17,155 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 ten seconds later (0.1 ETH refilled) | bridge Mint | 21,882 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 12,220 | 1 | 149 | 75 | 0 | 317 | – | 0 |
| SetCap (admin): global mints of B at the floor (1 ETH) | bridge SetCap | 31,845 | 1 | 733 | 75 | 520 | 1,294 | 520 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Issue | 44,167 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap | bridge Admit | 27,045 | 1 | 737 | 79 | 520 | 0 | – | 255 |
| Claim B's T2 with a time one hour ahead of the block | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Issue | 42,401 | 1 | 1,043 | 207 | 698 | 2,211 | 698 | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Admit | 34,298 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Spend | 23,427 | 1 | 312 | 174 | 0 | 679 | 77 | 0 |
| Claim B's T2 with a time one hour ahead of the block | bridge Mint | 22,170 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Issue | 42,401 | 1 | 1,043 | 207 | 698 | 2,211 | 698 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Admit | 33,898 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Spend | 23,427 | 1 | 312 | 174 | 0 | 679 | 77 | 0 |
| Claim B's 1,000 T2 (1 ETH of value units) under the same global cap | bridge Mint | 22,170 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 12,220 | 1 | 149 | 75 | 0 | 317 | – | 0 |
| SetCap (admin): global mints of B back to 200 ETH | bridge SetCap | 31,573 | 1 | 733 | 75 | 520 | 1,294 | 520 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 12,730 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 36,777 | 1 | 1,199 | 95 | 966 | 2,206 | 966 | 0 |
| Burn 1 ETH (A) | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 48,767 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) | bridge Admit | 34,146 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 20,661 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) again: the burn bucket is empty | bridge Burn | 30,899 | 1 | 1,289 | 185 | 966 | 0 | – | 255 |
| Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) 100 s later, no Tick: the stored time has not moved | bridge Burn | 30,899 | 1 | 1,289 | 185 | 966 | 0 | – | 255 |
| Tick ETH (A) | bridge Tick | 32,765 | 1 | 1,102 | 62 | 966 | 2,109 | 966 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 49,179 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Admit | 33,857 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) after the Tick | bridge Burn | 20,661 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 12,730 | 1 | 169 | 95 | 0 | 357 | – | 0 |
| SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first) | bridge SetCap | 36,903 | 1 | 1,199 | 95 | 966 | 2,206 | 966 | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 12,426 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): ETH (B) mints for one hour | bridge Freeze | 34,003 | 1 | 988 | 72 | 778 | 1,807 | 778 | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH while its mints are frozen | bridge Issue | 25,149 | 1 | 1,123 | 207 | 778 | 0 | – | 242 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 12,426 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze by the admin: unfreeze ETH (B) | bridge Freeze | 24,510 | 1 | 988 | 72 | 778 | 0 | – | 253 |
| Freeze for 30 days and 1 ms | bridge Freeze | 12,426 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze for 30 days and 1 ms | bridge Freeze | 24,984 | 1 | 988 | 72 | 778 | 0 | – | 240 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 12,426 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (B) | bridge Freeze | 33,985 | 1 | 988 | 72 | 778 | 1,807 | 778 | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 11,814 | 1 | 126 | 52 | 0 | 271 | – | 0 |
| Freeze (freeze authority): all mints of B for 30 days | bridge Freeze | 31,323 | 1 | 710 | 52 | 520 | 1,271 | 520 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Issue | 44,168 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim B's 1 ETH under the global mint freeze | bridge Admit | 27,017 | 1 | 737 | 79 | 520 | 0 | – | 242 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 12,426 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): ETH (A) burns for one hour | bridge Freeze | 37,007 | 1 | 1,176 | 72 | 966 | 2,183 | 966 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) while its burns are frozen | bridge Burn | 30,872 | 1 | 1,289 | 185 | 966 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 12,426 | 1 | 146 | 72 | 0 | 311 | – | 0 |
| Freeze (freeze authority): unfreeze ETH (A) burns | bridge Freeze | 36,990 | 1 | 1,176 | 72 | 966 | 2,183 | 966 | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 11,814 | 1 | 126 | 52 | 0 | 271 | – | 0 |
| Freeze (freeze authority): all burns of A for one hour | bridge Freeze | 31,322 | 1 | 710 | 52 | 520 | 1,271 | 520 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Burn | 48,767 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) under the global burn freeze | bridge Admit | 26,958 | 1 | 729 | 71 | 520 | 0 | – | 242 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Burn | 48,767 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) an hour later, no Tick: still frozen by the stored time | bridge Admit | 26,958 | 1 | 729 | 71 | 520 | 0 | – | 242 |
| Tick A | bridge Tick | 27,157 | 1 | 636 | 42 | 520 | 1,197 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 31,353 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 48,767 | 1 | 1,289 | 185 | 966 | 2,712 | 966 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Admit | 34,147 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (A) after the Tick: the freeze has lapsed | bridge Burn | 20,661 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Sunset B by a signer with no role | bridge Sunset | 11,086 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by a signer with no role | bridge Sunset | 23,714 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset B by the admin | bridge Sunset | 11,086 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B by the admin | bridge Sunset | 23,714 | 1 | 691 | 33 | 520 | 0 | – | 253 |
| Sunset sent to config(B) itself | bridge Sunset | 22,530 | 1 | 627 | 33 | 520 | 0 | – | 253 |
| Sunset B (freeze authority) | bridge Sunset | 11,086 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B (freeze authority) | bridge Sunset | 30,790 | 1 | 691 | 33 | 520 | 1,236 | 520 | 0 |
| Sunset B again | bridge Sunset | 11,086 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B again | bridge Sunset | 24,173 | 1 | 691 | 33 | 520 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 26,702 | 1 | 1,986 | 1,392 | 520 | 0 | – | 242 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Claim | 107,397 | 1 | 1,470 | 1,279 | 117 | 1,770 | – | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Issue | 44,167 | 1 | 1,123 | 207 | 778 | 2,419 | 778 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Admit | 34,188 | 1 | 737 | 79 | 520 | 1,298 | 520 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Spend | 30,034 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 23,932 | 1 | 441 | 161 | 142 | 676 | 210 | 0 |
| Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue | bridge Mint | 22,170 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 23,688 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 32,696 | 1 | 533 | 185 | 210 | 957 | 142 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 46,081 | 1 | 1,101 | 185 | 778 | 2,336 | 778 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Admit | 34,146 | 1 | 729 | 71 | 520 | 1,274 | 520 | 0 |
| Burn 1 ETH (B) after Sunset: burns continue | bridge Burn | 20,661 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |

## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Sunset B by a signer with no role | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B (freeze authority) | PublicRoot | 33 | Sunset | yes |
| Sunset B again | PublicRoot | 33 | Sunset | no (reverted) |
| ImportRoot B after Sunset | PublicRoot | 1392 | ImportRoot | no (reverted) |
| Burn 1 ETH (B) after Sunset: burns continue | PublicRoot | 185 | Burn | yes |
