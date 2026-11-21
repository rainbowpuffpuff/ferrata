# Transfers, the handover, the multisig, watchers and the keyless Sunset (`harness/tests/bridge_roles.rs`)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,890 → Attest 86,790 → bridge (reply) 39,738 | 171,418 | 3% | 1,946 | 1,386,912 | applied |
| 2 | Init B | bridge Init 44,270 → Attest 84,699 → bridge (reply) 39,738 | 168,707 | 3% | 1,882 | 1,364,712 | applied |
| 3 | Tick A: block 102's record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 4 | Tick B: block 102's record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 5 | Register ETH (A) | bridge Register 74,324 → Attest 151,273 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 333,303 | 7% | 2,747 | 2,688,400 | applied |
| 6 | Register ETH (B) | bridge Register 77,279 → Attest 156,184 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 341,169 | 7% | 2,770 | 2,751,512 | applied |
| 7 | ImportRoot A n=8 | bridge ImportRoot 52,127 → Attest 113,770 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,140 | 5% | 2,025 | 1,913,320 | applied |
| 8 | ImportRoot B n=8 | bridge ImportRoot 50,365 → Attest 115,561 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 237,169 | 5% | 1,918 | 1,912,696 | applied |
| 9 | ImportFees ETH (A): schedule 1 | bridge ImportFees 56,345 → Attest 113,770 → bridge (reply) 58,627 → bridge ExtendFees 42,881 | 271,623 | 5% | 2,206 | 2,190,632 | applied |
| 10 | ImportFees ETH (A): no schedule 2 at 8 leaves | bridge ImportFees 50,613 → Attest 108,223 → bridge (reply) 52,960 → bridge ExtendFees 40,597 | 252,393 | 5% | 1,945 | 2,034,704 | applied |
| 11 | Claim A's leaf 6: 10 ETH to the payer | bridge Claim 107,796 → bridge Issue 52,816 → bridge Admit 35,961 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 18,800 | 291,260 | 6% | 2,053 | 2,346,504 | applied |
| 12 | Transfer 1 ETH (A): the payer to a holder | bridge Transfer 17,778 → bridge Transfer 23,563 → bridge Transfer 37,061 → bridge Credit 16,990 | 95,392 | 2% | 691 | 768,664 | applied |
| 13 | Transfer 0 | bridge Transfer 17,778 → bridge Transfer 23,563 → bridge Transfer 26,718 | 68,059 | 1% | 691 | 550,000 | refused: exit 240 (BAD_MESSAGE) |
| 14 | Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer 17,778 → bridge Transfer 23,563 → bridge Transfer 26,894 | 68,235 | 1% | 691 | 551,408 | refused: exit 240 (BAD_MESSAGE) |
| 15 | Transfer 100 ETH (more than the payer holds) | bridge Transfer 17,778 → bridge Transfer 19,615 | 37,393 | 1% | 691 | 304,672 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 16 | Transfer of a token with no holding (B's ETH) | bridge Transfer 17,778 → bridge Transfer 15,131 | 32,909 | 1% | 691 | 268,800 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 17 | Transfer from the recipient's actor, not signed by it | bridge Transfer 10,662 | 10,662 | 0% | 691 | 90,824 | refused: exit 253 (NOT_AUTHORIZED) |
| 18 | Transfer sent to holder(recipient) itself | bridge Transfer 14,318 | 14,318 | 0% | 691 | 120,072 | refused: exit 253 (NOT_AUTHORIZED) |
| 19 | Transfer sent to token(A, ETH) itself (a credit from nothing) | bridge Transfer 24,573 | 24,573 | 0% | 691 | 202,112 | refused: exit 243 (WRONG_SENDER) |
| 20 | Credit sent to holder(payer) itself | bridge Credit 14,157 | 14,157 | 0% | 499 | 117,248 | refused: exit 243 (WRONG_SENDER) |
| 21 | Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer 17,778 → bridge Transfer 23,563 → bridge Transfer 37,039 → bridge Credit 21,476 | 99,856 | 2% | 867 | 805,784 | applied |
| 22 | ProposeRoles by a signer with no role | bridge ProposeRoles 16,337 → bridge ProposeRoles 27,222 | 43,559 | 1% | 881 | 355,520 | refused: exit 253 (NOT_AUTHORIZED) |
| 23 | ProposeRoles by the freeze authority | bridge ProposeRoles 16,337 → bridge ProposeRoles 27,444 | 43,781 | 1% | 881 | 357,296 | refused: exit 253 (NOT_AUTHORIZED) |
| 24 | ProposeRoles sent to config(A) itself | bridge ProposeRoles 25,203 | 25,203 | 1% | 577 | 206,240 | refused: exit 253 (NOT_AUTHORIZED) |
| 25 | ProposeRoles: threshold 0 | bridge ProposeRoles 15,566 → bridge ProposeRoles 27,002 | 42,568 | 1% | 849 | 347,336 | refused: exit 240 (BAD_MESSAGE) |
| 26 | ProposeRoles: threshold 4 of 3 | bridge ProposeRoles 15,566 → bridge ProposeRoles 27,003 | 42,569 | 1% | 849 | 347,344 | refused: exit 240 (BAD_MESSAGE) |
| 27 | ProposeRoles: the admin as a member | bridge ProposeRoles 14,836 → bridge ProposeRoles 26,803 | 41,639 | 1% | 817 | 339,648 | refused: exit 240 (BAD_MESSAGE) |
| 28 | ProposeRoles: a member twice | bridge ProposeRoles 14,836 → bridge ProposeRoles 26,841 | 41,677 | 1% | 817 | 339,952 | refused: exit 240 (BAD_MESSAGE) |
| 29 | ProposeRoles: four watchers | bridge ProposeRoles 18,545 → bridge ProposeRoles 29,196 | 47,741 | 1% | 977 | 389,744 | refused: exit 240 (BAD_MESSAGE) |
| 30 | ApplyRoles with nothing proposed | bridge ApplyRoles 21,689 | 21,689 | 0% | 504 | 177,544 | refused: exit 245 (NOT_FOUND) |
| 31 | CancelRoles with nothing proposed | bridge CancelRoles 12,676 → bridge CancelRoles 25,280 | 37,956 | 1% | 671 | 309,016 | refused: exit 245 (NOT_FOUND) |
| 32 | ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge ProposeRoles 16,337 → bridge ProposeRoles 40,676 → Read 12,268 → bridge (reply) 37,356 | 106,637 | 2% | 881 | 860,144 | applied |
| 33 | CancelRoles by the freeze authority | bridge CancelRoles 12,676 → bridge CancelRoles 27,329 | 40,005 | 1% | 671 | 325,408 | refused: exit 253 (NOT_AUTHORIZED) |
| 34 | CancelRoles by the admin | bridge CancelRoles 12,676 → bridge CancelRoles 34,876 | 47,552 | 1% | 671 | 385,784 | applied |
| 35 | ProposeRoles by the admin again | bridge ProposeRoles 16,337 → bridge ProposeRoles 40,676 → Read 12,268 → bridge (reply) 37,356 | 106,637 | 2% | 881 | 860,144 | applied |
| 36 | ApplyRoles at the proposal's own record | bridge ApplyRoles 36,493 → Read 12,268 → bridge (reply) 28,374 | 77,135 | 2% | 504 | 621,112 | refused: exit 242 (FROZEN) |
| 37 | Pause by the proposed watcher, before the handover | bridge Pause 13,218 → bridge Pause 27,484 | 40,702 | 1% | 747 | 331,592 | refused: exit 253 (NOT_AUTHORIZED) |
| 38 | ApplyRoles with a record 13 days later | bridge ApplyRoles 36,493 → Read 12,268 → bridge (reply) 28,373 | 77,134 | 2% | 504 | 621,104 | refused: exit 242 (FROZEN) |
| 39 | ApplyRoles with a record 14 days later (anyone) | bridge ApplyRoles 36,493 → Read 12,268 → bridge (reply) 36,974 | 85,735 | 2% | 504 | 689,912 | applied |
| 40 | ApplyRoles again | bridge ApplyRoles 23,095 | 23,095 | 0% | 504 | 188,792 | refused: exit 245 (NOT_FOUND) |
| 41 | Freeze by the replaced freeze authority | bridge Freeze 13,320 → bridge Freeze 26,904 | 40,224 | 1% | 755 | 327,832 | refused: exit 253 (NOT_AUTHORIZED) |
| 42 | Freeze by one member alone (2 of 3) | bridge Freeze 13,320 → bridge Freeze 26,904 | 40,224 | 1% | 755 | 327,832 | refused: exit 253 (NOT_AUTHORIZED) |
| 43 | Cosigned freeze by one member | bridge Cosigned 13,772 → bridge Cosigned 28,994 | 42,766 | 1% | 792 | 348,464 | refused: exit 253 (NOT_AUTHORIZED) |
| 44 | Cosigned freeze naming M1, M2, signed by M1 only | bridge Cosigned 14,595 → bridge Cosigned 12,301 | 26,896 | 1% | 888 | 222,272 | refused: exit 253 (NOT_AUTHORIZED) |
| 45 | Cosigned freeze by M1 and a non-member | bridge Cosigned 14,595 → bridge Cosigned 16,338 → bridge Cosigned 29,771 | 60,704 | 1% | 1,064 | 494,144 | refused: exit 253 (NOT_AUTHORIZED) |
| 46 | Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned 14,595 → bridge Cosigned 16,338 → bridge Cosigned 42,781 → Read 12,268 → bridge (reply) 36,629 | 122,611 | 2% | 1,064 | 989,400 | applied |
| 47 | Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned 15,104 → bridge Cosigned 16,847 → bridge Cosigned 37,689 → bridge Freeze 45,961 → Read 12,268 → bridge (reply) 40,929 | 168,798 | 3% | 1,148 | 1,359,568 | applied |
| 48 | Cosigned freeze for 31 days | bridge Cosigned 14,595 → bridge Cosigned 16,338 → bridge Cosigned 42,781 → Read 12,268 → bridge (reply) 27,550 | 113,532 | 2% | 1,064 | 916,768 | refused: exit 240 (BAD_MESSAGE) |
| 49 | Pause by a signer with no role | bridge Pause 13,218 → bridge Pause 26,832 | 40,050 | 1% | 747 | 326,376 | refused: exit 253 (NOT_AUTHORIZED) |
| 50 | Pause by a member of the freeze authority | bridge Pause 13,218 → bridge Pause 26,832 | 40,050 | 1% | 747 | 326,376 | refused: exit 253 (NOT_AUTHORIZED) |
| 51 | Pause of neither mints nor burns | bridge Pause 13,220 → bridge Pause 39,016 → Read 12,268 → bridge (reply) 27,501 | 92,005 | 2% | 747 | 742,016 | refused: exit 240 (BAD_MESSAGE) |
| 52 | Pause A's mints and burns (the watcher) | bridge Pause 13,218 → bridge Pause 39,014 → Read 12,268 → bridge (reply) 36,430 | 100,930 | 2% | 747 | 813,416 | applied |
| 53 | Pause ETH (A) mints (the watcher) | bridge Pause 15,987 → bridge Pause 34,261 → bridge Pause 45,860 → Read 12,268 → bridge (reply) 40,880 | 149,256 | 3% | 831 | 1,200,696 | applied |
| 54 | Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned 14,595 → bridge Cosigned 16,338 → bridge Cosigned 42,797 → Read 12,268 → bridge (reply) 36,629 | 122,627 | 2% | 1,064 | 989,528 | applied |
| 55 | Pause A's mints again (the watcher) | bridge Pause 13,218 → bridge Pause 39,014 → Read 12,268 → bridge (reply) 36,433 | 100,933 | 2% | 747 | 813,440 | applied |
| 56 | Unfreeze by the watcher | bridge Freeze 13,319 → bridge Freeze 27,122 | 40,441 | 1% | 755 | 329,568 | refused: exit 253 (NOT_AUTHORIZED) |
| 57 | Cosigned unfreeze of A by M2 and M3 | bridge Cosigned 14,595 → bridge Cosigned 16,338 → bridge Cosigned 42,813 → Read 12,268 → bridge (reply) 36,619 | 122,633 | 2% | 1,064 | 989,576 | applied |
| 58 | SetCap ETH (A) by the admin, through config(A) | bridge SetCap 16,352 → bridge SetCap 34,746 → bridge SetCap 42,865 | 93,963 | 2% | 789 | 758,016 | applied |
| 59 | SetCap ETH (A) below the floor | bridge SetCap 16,352 → bridge SetCap 34,746 → bridge SetCap 30,502 | 81,600 | 2% | 789 | 659,112 | refused: exit 240 (BAD_MESSAGE) |
| 60 | Sunset sent to config(A) by anyone, not approved | bridge Sunset 24,451 | 24,451 | 0% | 431 | 199,056 | refused: exit 253 (NOT_AUTHORIZED) |
| 61 | Sunset A by the replaced freeze authority | bridge Sunset 12,677 → bridge Sunset 26,462 | 39,139 | 1% | 671 | 318,480 | refused: exit 253 (NOT_AUTHORIZED) |
| 62 | Sunset A by one member alone | bridge Sunset 12,677 → bridge Sunset 26,462 | 39,139 | 1% | 671 | 318,480 | refused: exit 253 (NOT_AUTHORIZED) |
| 63 | Cosigned Sunset by one member | bridge Cosigned 13,304 → bridge Cosigned 28,761 | 42,065 | 1% | 708 | 342,184 | refused: exit 253 (NOT_AUTHORIZED) |
| 64 | Cosigned Sunset approval by M1 and M2 | bridge Cosigned 14,127 → bridge Cosigned 15,870 → bridge Cosigned 37,840 | 67,837 | 1% | 980 | 550,536 | applied |
| 65 | Cosigned Sunset approval again | bridge Cosigned 14,127 → bridge Cosigned 15,870 → bridge Cosigned 29,786 | 59,783 | 1% | 980 | 486,104 | refused: exit 244 (ALREADY_WRITTEN) |
| 66 | Sunset A sent to config(A) by anyone, approved | bridge Sunset 31,573 | 31,573 | 1% | 431 | 256,032 | applied |
| 67 | Sunset A again | bridge Sunset 24,451 | 24,451 | 0% | 431 | 199,056 | refused: exit 244 (ALREADY_WRITTEN) |
| 68 | ImportRoot A after Sunset | bridge ImportRoot 29,771 | 29,771 | 1% | 2,025 | 254,368 | refused: exit 242 (FROZEN) |
| 69 | Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer 17,778 → bridge Transfer 23,563 → bridge Transfer 37,061 → bridge Credit 21,476 | 99,878 | 2% | 691 | 804,552 | applied |
| 70 | Sunset sent to config(B) by anyone, vault not retired | bridge Sunset 23,045 | 23,045 | 0% | 431 | 187,808 | refused: exit 253 (NOT_AUTHORIZED) |
| 71 | AttestRetired B under block 100 (slot 37 empty) | bridge AttestRetired 45,958 → Attest 83,027 → bridge (reply) 35,607 | 164,592 | 3% | 1,473 | 1,328,520 | refused: exit 247 (IMPORT_MISMATCH) |
| 72 | AttestRetired B: a freeze authority, not retired | bridge AttestRetired 46,807 → Attest 93,357 → bridge (reply) 35,607 | 175,771 | 4% | 1,489 | 1,418,080 | refused: exit 247 (IMPORT_MISMATCH) |
| 73 | AttestRetired B: retired (anyone) | bridge AttestRetired 45,841 → Attest 93,685 → bridge (reply) 43,641 | 183,167 | 4% | 1,490 | 1,477,256 | applied |
| 74 | AttestRetired B again | bridge AttestRetired 26,590 | 26,590 | 1% | 1,490 | 224,640 | refused: exit 244 (ALREADY_WRITTEN) |
| 75 | Sunset B forwarded by a signer's own actor (not top-level at config(B)) | bridge Sunset 12,677 → bridge Sunset 25,058 | 37,735 | 1% | 671 | 307,248 | refused: exit 253 (NOT_AUTHORIZED) |
| 76 | Sunset B sent to config(B) by anyone, retired | bridge Sunset 29,503 | 29,503 | 1% | 431 | 239,472 | applied |
| 77 | Sunset B again | bridge Sunset 23,045 | 23,045 | 0% | 431 | 187,808 | refused: exit 244 (ALREADY_WRITTEN) |
| 78 | ImportRoot B after Sunset | bridge ImportRoot 28,566 | 28,566 | 1% | 1,918 | 243,872 | refused: exit 242 (FROZEN) |

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
| Register ETH (A) | bridge Register | 74,324 | 1 | 2,875 | 2,221 | 580 | 5,971 | 966 | 0 |
| Register ETH (A) | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (A) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (A) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register ETH (B) | bridge Register | 77,279 | 1 | 2,898 | 2,244 | 580 | 6,017 | 966 | 0 |
| Register ETH (B) | Attest | 156,184 | 1 | 2,559 | 2,056 | 365 | 3,373 | – | 0 |
| Register ETH (B) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (B) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 52,127 | 1 | 2,153 | 1,499 | 580 | 4,459 | 629 | 0 |
| ImportRoot A n=8 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 50,365 | 1 | 2,046 | 1,392 | 580 | 4,245 | 629 | 0 |
| ImportRoot B n=8 | Attest | 115,561 | 1 | 1,976 | 1,473 | 365 | 2,556 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 | bridge ImportFees | 56,345 | 1 | 2,334 | 1,680 | 580 | 4,817 | 806 | 0 |
| ImportFees ETH (A): schedule 1 | Attest | 113,770 | 1 | 2,083 | 1,580 | 365 | 2,663 | – | 0 |
| ImportFees ETH (A): schedule 1 | bridge (reply) | 58,627 | 1 | 1,431 | 487 | 806 | 2,398 | 580 | 0 |
| ImportFees ETH (A): schedule 1 | bridge ExtendFees | 42,881 | 1 | 1,248 | 258 | 852 | 2,313 | 1,040 | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | bridge ImportFees | 50,613 | 1 | 2,073 | 1,419 | 580 | 4,295 | 618 | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | Attest | 108,223 | 1 | 2,010 | 1,507 | 365 | 2,590 | – | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | bridge ExtendFees | 40,597 | 1 | 1,248 | 70 | 1,040 | 2,313 | 1,040 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Issue | 52,816 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Transfer | 23,563 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Transfer | 37,061 | 1 | 1,279 | 101 | 1,040 | 1,505 | – | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Credit | 16,990 | 1 | 239 | 101 | 0 | 338 | 74 | 0 |
| Transfer 0 | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 0 | bridge Transfer | 23,563 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 0 | bridge Transfer | 26,718 | 1 | 1,279 | 101 | 1,040 | 0 | – | 240 |
| Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer | 23,563 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer | 26,894 | 1 | 1,279 | 101 | 1,040 | 0 | – | 240 |
| Transfer 100 ETH (more than the payer holds) | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 100 ETH (more than the payer holds) | bridge Transfer | 19,615 | 1 | 313 | 101 | 74 | 0 | – | 254 |
| Transfer of a token with no holding (B's ETH) | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer of a token with no holding (B's ETH) | bridge Transfer | 15,131 | 1 | 239 | 101 | 0 | 0 | – | 254 |
| Transfer from the recipient's actor, not signed by it | bridge Transfer | 10,662 | 1 | 175 | 101 | 0 | 0 | – | 253 |
| Transfer sent to holder(recipient) itself | bridge Transfer | 14,318 | 1 | 249 | 101 | 74 | 0 | – | 253 |
| Transfer sent to token(A, ETH) itself (a credit from nothing) | bridge Transfer | 24,573 | 1 | 1,215 | 101 | 1,040 | 0 | – | 243 |
| Credit sent to holder(payer) itself | bridge Credit | 14,157 | 1 | 249 | 101 | 74 | 0 | – | 243 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer | 23,563 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer | 37,039 | 1 | 1,279 | 101 | 1,040 | 1,505 | – | 0 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Credit | 21,476 | 1 | 313 | 101 | 74 | 412 | 74 | 0 |
| ProposeRoles by a signer with no role | bridge ProposeRoles | 16,337 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by a signer with no role | bridge ProposeRoles | 27,222 | 1 | 897 | 179 | 580 | 0 | – | 253 |
| ProposeRoles by the freeze authority | bridge ProposeRoles | 16,337 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by the freeze authority | bridge ProposeRoles | 27,444 | 1 | 897 | 179 | 580 | 0 | – | 253 |
| ProposeRoles sent to config(A) itself | bridge ProposeRoles | 25,203 | 1 | 833 | 179 | 580 | 0 | – | 253 |
| ProposeRoles: threshold 0 | bridge ProposeRoles | 15,566 | 1 | 221 | 147 | 0 | 461 | – | 0 |
| ProposeRoles: threshold 0 | bridge ProposeRoles | 27,002 | 1 | 865 | 147 | 580 | 0 | – | 240 |
| ProposeRoles: threshold 4 of 3 | bridge ProposeRoles | 15,566 | 1 | 221 | 147 | 0 | 461 | – | 0 |
| ProposeRoles: threshold 4 of 3 | bridge ProposeRoles | 27,003 | 1 | 865 | 147 | 580 | 0 | – | 240 |
| ProposeRoles: the admin as a member | bridge ProposeRoles | 14,836 | 1 | 189 | 115 | 0 | 397 | – | 0 |
| ProposeRoles: the admin as a member | bridge ProposeRoles | 26,803 | 1 | 833 | 115 | 580 | 0 | – | 240 |
| ProposeRoles: a member twice | bridge ProposeRoles | 14,836 | 1 | 189 | 115 | 0 | 397 | – | 0 |
| ProposeRoles: a member twice | bridge ProposeRoles | 26,841 | 1 | 833 | 115 | 580 | 0 | – | 240 |
| ProposeRoles: four watchers | bridge ProposeRoles | 18,545 | 1 | 349 | 275 | 0 | 717 | – | 0 |
| ProposeRoles: four watchers | bridge ProposeRoles | 29,196 | 1 | 993 | 275 | 580 | 0 | – | 240 |
| ApplyRoles with nothing proposed | bridge ApplyRoles | 21,689 | 1 | 696 | 42 | 580 | 0 | – | 245 |
| CancelRoles with nothing proposed | bridge CancelRoles | 12,676 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| CancelRoles with nothing proposed | bridge CancelRoles | 25,280 | 1 | 751 | 33 | 580 | 0 | – | 245 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge ProposeRoles | 16,337 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge ProposeRoles | 40,676 | 1 | 897 | 179 | 580 | 1,723 | 728 | 0 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge (reply) | 37,356 | 1 | 1,235 | 369 | 728 | 2,001 | 725 | 0 |
| CancelRoles by the freeze authority | bridge CancelRoles | 12,676 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| CancelRoles by the freeze authority | bridge CancelRoles | 27,329 | 1 | 896 | 33 | 725 | 0 | – | 253 |
| CancelRoles by the admin | bridge CancelRoles | 12,676 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| CancelRoles by the admin | bridge CancelRoles | 34,876 | 1 | 896 | 33 | 725 | 1,501 | 580 | 0 |
| ProposeRoles by the admin again | bridge ProposeRoles | 16,337 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by the admin again | bridge ProposeRoles | 40,676 | 1 | 897 | 179 | 580 | 1,723 | 728 | 0 |
| ProposeRoles by the admin again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| ProposeRoles by the admin again | bridge (reply) | 37,356 | 1 | 1,235 | 369 | 728 | 2,001 | 725 | 0 |
| ApplyRoles at the proposal's own record | bridge ApplyRoles | 36,493 | 1 | 841 | 42 | 725 | 1,675 | 736 | 0 |
| ApplyRoles at the proposal's own record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| ApplyRoles at the proposal's own record | bridge (reply) | 28,374 | 1 | 1,243 | 369 | 736 | 0 | – | 242 |
| Pause by the proposed watcher, before the handover | bridge Pause | 13,218 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause by the proposed watcher, before the handover | bridge Pause | 27,484 | 1 | 908 | 45 | 725 | 0 | – | 253 |
| ApplyRoles with a record 13 days later | bridge ApplyRoles | 36,493 | 1 | 841 | 42 | 725 | 1,675 | 736 | 0 |
| ApplyRoles with a record 13 days later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| ApplyRoles with a record 13 days later | bridge (reply) | 28,373 | 1 | 1,243 | 369 | 736 | 0 | – | 242 |
| ApplyRoles with a record 14 days later (anyone) | bridge ApplyRoles | 36,493 | 1 | 841 | 42 | 725 | 1,675 | 736 | 0 |
| ApplyRoles with a record 14 days later (anyone) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| ApplyRoles with a record 14 days later (anyone) | bridge (reply) | 36,974 | 1 | 1,243 | 369 | 736 | 1,952 | 676 | 0 |
| ApplyRoles again | bridge ApplyRoles | 23,095 | 1 | 792 | 42 | 676 | 0 | – | 245 |
| Freeze by the replaced freeze authority | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze by the replaced freeze authority | bridge Freeze | 26,904 | 1 | 867 | 53 | 676 | 0 | – | 253 |
| Freeze by one member alone (2 of 3) | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze by one member alone (2 of 3) | bridge Freeze | 26,904 | 1 | 867 | 53 | 676 | 0 | – | 253 |
| Cosigned freeze by one member | bridge Cosigned | 13,772 | 1 | 164 | 90 | 0 | 347 | – | 0 |
| Cosigned freeze by one member | bridge Cosigned | 28,994 | 1 | 904 | 90 | 676 | 0 | – | 253 |
| Cosigned freeze naming M1, M2, signed by M1 only | bridge Cosigned | 14,595 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze naming M1, M2, signed by M1 only | bridge Cosigned | 12,301 | 1 | 260 | 122 | 0 | 0 | – | 253 |
| Cosigned freeze by M1 and a non-member | bridge Cosigned | 14,595 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze by M1 and a non-member | bridge Cosigned | 16,338 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze by M1 and a non-member | bridge Cosigned | 29,771 | 1 | 936 | 122 | 676 | 0 | – | 253 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned | 14,595 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned | 16,338 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned | 42,781 | 1 | 936 | 122 | 676 | 1,731 | 697 | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge (reply) | 36,629 | 1 | 1,204 | 369 | 697 | 1,913 | 676 | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned | 15,104 | 1 | 216 | 142 | 0 | 451 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned | 16,847 | 1 | 280 | 142 | 0 | 515 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned | 37,689 | 1 | 956 | 142 | 676 | 1,122 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Freeze | 45,961 | 1 | 1,251 | 73 | 1,040 | 2,409 | 1,060 | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge (reply) | 40,929 | 1 | 1,567 | 369 | 1,060 | 2,640 | 1,040 | 0 |
| Cosigned freeze for 31 days | bridge Cosigned | 14,595 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze for 31 days | bridge Cosigned | 16,338 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze for 31 days | bridge Cosigned | 42,781 | 1 | 936 | 122 | 676 | 1,731 | 697 | 0 |
| Cosigned freeze for 31 days | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Cosigned freeze for 31 days | bridge (reply) | 27,550 | 1 | 1,204 | 369 | 697 | 0 | – | 240 |
| Pause by a signer with no role | bridge Pause | 13,218 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause by a signer with no role | bridge Pause | 26,832 | 1 | 859 | 45 | 676 | 0 | – | 253 |
| Pause by a member of the freeze authority | bridge Pause | 13,218 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause by a member of the freeze authority | bridge Pause | 26,832 | 1 | 859 | 45 | 676 | 0 | – | 253 |
| Pause of neither mints nor burns | bridge Pause | 13,220 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause of neither mints nor burns | bridge Pause | 39,016 | 1 | 859 | 45 | 676 | 1,646 | 689 | 0 |
| Pause of neither mints nor burns | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Pause of neither mints nor burns | bridge (reply) | 27,501 | 1 | 1,196 | 369 | 689 | 0 | – | 240 |
| Pause A's mints and burns (the watcher) | bridge Pause | 13,218 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause A's mints and burns (the watcher) | bridge Pause | 39,014 | 1 | 859 | 45 | 676 | 1,646 | 689 | 0 |
| Pause A's mints and burns (the watcher) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Pause A's mints and burns (the watcher) | bridge (reply) | 36,430 | 1 | 1,196 | 369 | 689 | 1,905 | 676 | 0 |
| Pause ETH (A) mints (the watcher) | bridge Pause | 15,987 | 1 | 139 | 65 | 0 | 297 | – | 0 |
| Pause ETH (A) mints (the watcher) | bridge Pause | 34,261 | 1 | 879 | 65 | 676 | 1,037 | – | 0 |
| Pause ETH (A) mints (the watcher) | bridge Pause | 45,860 | 1 | 1,243 | 65 | 1,040 | 2,393 | 1,052 | 0 |
| Pause ETH (A) mints (the watcher) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Pause ETH (A) mints (the watcher) | bridge (reply) | 40,880 | 1 | 1,559 | 369 | 1,052 | 2,632 | 1,040 | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned | 14,595 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned | 16,338 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned | 42,797 | 1 | 936 | 122 | 676 | 1,731 | 697 | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge (reply) | 36,629 | 1 | 1,204 | 369 | 697 | 1,913 | 676 | 0 |
| Pause A's mints again (the watcher) | bridge Pause | 13,218 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause A's mints again (the watcher) | bridge Pause | 39,014 | 1 | 859 | 45 | 676 | 1,646 | 689 | 0 |
| Pause A's mints again (the watcher) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Pause A's mints again (the watcher) | bridge (reply) | 36,433 | 1 | 1,196 | 369 | 689 | 1,905 | 676 | 0 |
| Unfreeze by the watcher | bridge Freeze | 13,319 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze by the watcher | bridge Freeze | 27,122 | 1 | 867 | 53 | 676 | 0 | – | 253 |
| Cosigned unfreeze of A by M2 and M3 | bridge Cosigned | 14,595 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned unfreeze of A by M2 and M3 | bridge Cosigned | 16,338 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned unfreeze of A by M2 and M3 | bridge Cosigned | 42,813 | 1 | 936 | 122 | 676 | 1,731 | 697 | 0 |
| Cosigned unfreeze of A by M2 and M3 | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Cosigned unfreeze of A by M2 and M3 | bridge (reply) | 36,619 | 1 | 1,204 | 369 | 697 | 1,913 | 676 | 0 |
| SetCap ETH (A) by the admin, through config(A) | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap ETH (A) by the admin, through config(A) | bridge SetCap | 34,746 | 1 | 901 | 87 | 676 | 1,081 | – | 0 |
| SetCap ETH (A) by the admin, through config(A) | bridge SetCap | 42,865 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| SetCap ETH (A) below the floor | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap ETH (A) below the floor | bridge SetCap | 34,746 | 1 | 901 | 87 | 676 | 1,081 | – | 0 |
| SetCap ETH (A) below the floor | bridge SetCap | 30,502 | 1 | 1,265 | 87 | 1,040 | 0 | – | 240 |
| Sunset sent to config(A) by anyone, not approved | bridge Sunset | 24,451 | 1 | 783 | 33 | 676 | 0 | – | 253 |
| Sunset A by the replaced freeze authority | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset A by the replaced freeze authority | bridge Sunset | 26,462 | 1 | 847 | 33 | 676 | 0 | – | 253 |
| Sunset A by one member alone | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset A by one member alone | bridge Sunset | 26,462 | 1 | 847 | 33 | 676 | 0 | – | 253 |
| Cosigned Sunset by one member | bridge Cosigned | 13,304 | 1 | 144 | 70 | 0 | 307 | – | 0 |
| Cosigned Sunset by one member | bridge Cosigned | 28,761 | 1 | 884 | 70 | 676 | 0 | – | 253 |
| Cosigned Sunset approval by M1 and M2 | bridge Cosigned | 14,127 | 1 | 176 | 102 | 0 | 371 | – | 0 |
| Cosigned Sunset approval by M1 and M2 | bridge Cosigned | 15,870 | 1 | 240 | 102 | 0 | 435 | – | 0 |
| Cosigned Sunset approval by M1 and M2 | bridge Cosigned | 37,840 | 1 | 916 | 102 | 676 | 1,617 | 676 | 0 |
| Cosigned Sunset approval again | bridge Cosigned | 14,127 | 1 | 176 | 102 | 0 | 371 | – | 0 |
| Cosigned Sunset approval again | bridge Cosigned | 15,870 | 1 | 240 | 102 | 0 | 435 | – | 0 |
| Cosigned Sunset approval again | bridge Cosigned | 29,786 | 1 | 916 | 102 | 676 | 0 | – | 244 |
| Sunset A sent to config(A) by anyone, approved | bridge Sunset | 31,573 | 1 | 783 | 33 | 676 | 1,484 | 676 | 0 |
| Sunset A again | bridge Sunset | 24,451 | 1 | 783 | 33 | 676 | 0 | – | 244 |
| ImportRoot A after Sunset | bridge ImportRoot | 29,771 | 1 | 2,249 | 1,499 | 676 | 0 | – | 242 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer | 17,778 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer | 23,563 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer | 37,061 | 1 | 1,279 | 101 | 1,040 | 1,505 | – | 0 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Credit | 21,476 | 1 | 313 | 101 | 74 | 412 | 74 | 0 |
| Sunset sent to config(B) by anyone, vault not retired | bridge Sunset | 23,045 | 1 | 687 | 33 | 580 | 0 | – | 253 |
| AttestRetired B under block 100 (slot 37 empty) | bridge AttestRetired | 45,958 | 1 | 1,665 | 1,011 | 580 | 3,443 | 589 | 0 |
| AttestRetired B under block 100 (slot 37 empty) | Attest | 83,027 | 1 | 1,595 | 1,092 | 365 | 2,058 | – | 0 |
| AttestRetired B under block 100 (slot 37 empty) | bridge (reply) | 35,607 | 1 | 1,097 | 370 | 589 | 0 | – | 247 |
| AttestRetired B: a freeze authority, not retired | bridge AttestRetired | 46,807 | 1 | 1,681 | 1,027 | 580 | 3,475 | 589 | 0 |
| AttestRetired B: a freeze authority, not retired | Attest | 93,357 | 1 | 1,611 | 1,108 | 365 | 2,074 | – | 0 |
| AttestRetired B: a freeze authority, not retired | bridge (reply) | 35,607 | 1 | 1,097 | 370 | 589 | 0 | – | 247 |
| AttestRetired B: retired (anyone) | bridge AttestRetired | 45,841 | 1 | 1,682 | 1,028 | 580 | 3,477 | 589 | 0 |
| AttestRetired B: retired (anyone) | Attest | 93,685 | 1 | 1,612 | 1,109 | 365 | 2,075 | – | 0 |
| AttestRetired B: retired (anyone) | bridge (reply) | 43,641 | 1 | 1,097 | 370 | 589 | 1,702 | 580 | 0 |
| AttestRetired B again | bridge AttestRetired | 26,590 | 1 | 1,682 | 1,028 | 580 | 0 | – | 244 |
| Sunset B forwarded by a signer's own actor (not top-level at config(B)) | bridge Sunset | 12,677 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B forwarded by a signer's own actor (not top-level at config(B)) | bridge Sunset | 25,058 | 1 | 751 | 33 | 580 | 0 | – | 253 |
| Sunset B sent to config(B) by anyone, retired | bridge Sunset | 29,503 | 1 | 687 | 33 | 580 | 1,292 | 580 | 0 |
| Sunset B again | bridge Sunset | 23,045 | 1 | 687 | 33 | 580 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 28,566 | 1 | 2,046 | 1,392 | 580 | 0 | – | 242 |

## What the checkpoint program collects (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)

One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):

| transaction | origin | message bytes | checkpoint's decoder | collected |
|---|---|---:|---|---|
| Cosigned Sunset approval by M1 and M2 | PublicRoot | 102 | ignored (variant 4 and up) | no |
| Sunset sent to config(A) by anyone, not approved | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset A sent to config(A) by anyone, approved | PublicRoot | 33 | Sunset | yes |
| Sunset A again | PublicRoot | 33 | Sunset | no (reverted) |
| AttestRetired B: retired (anyone) | PublicRoot | 1028 | ignored (variant 4 and up) | no |
| Sunset B forwarded by a signer's own actor (not top-level at config(B)) | PublicRoot | 33 | Sunset | no (reverted) |
| Sunset B sent to config(B) by anyone, retired | PublicRoot | 33 | Sunset | yes |
| Transfer 1 ETH (A): the payer to a holder | PublicRoot | 101 | ignored (variant 4 and up) | no |
