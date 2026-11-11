# Transfers, the handover, the multisig, watchers and the keyless Sunset (`harness/tests/bridge_roles.rs`)

Bridge program image `43615dc48098b3731990de7700204d2a1bea0e9f963ee7c4dd629048dd5ef19c`; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A | bridge Init 44,818 → Attest 85,725 → bridge (reply) 39,419 | 169,962 | 3% | 1,946 | 1,375,264 | applied |
| 2 | Init B | bridge Init 44,198 → Attest 83,634 → bridge (reply) 39,419 | 167,251 | 3% | 1,882 | 1,353,064 | applied |
| 3 | Tick A: block 102's record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 4 | Tick B: block 102's record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 5 | Register ETH (A) | bridge Register 73,920 → Attest 150,201 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 330,744 | 7% | 2,747 | 2,667,928 | applied |
| 6 | Register ETH (B) | bridge Register 76,864 → Attest 155,163 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 338,650 | 7% | 2,770 | 2,731,360 | applied |
| 7 | ImportRoot A n=8 | bridge ImportRoot 51,721 → Attest 112,749 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 235,203 | 5% | 2,025 | 1,897,824 | applied |
| 8 | ImportRoot B n=8 | bridge ImportRoot 49,908 → Attest 114,375 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 235,016 | 5% | 1,918 | 1,895,472 | applied |
| 9 | ImportFees ETH (A): schedule 1 | bridge ImportFees 55,929 → Attest 112,749 → bridge (reply) 58,259 → bridge ExtendFees 41,726 | 268,663 | 5% | 2,206 | 2,166,952 | applied |
| 10 | ImportFees ETH (A): no schedule 2 at 8 leaves | bridge ImportFees 50,198 → Attest 107,315 → bridge (reply) 52,487 → bridge ExtendFees 39,427 | 249,427 | 5% | 1,945 | 2,010,976 | applied |
| 11 | Claim A's leaf 6: 10 ETH to the payer | bridge Claim 107,702 → bridge Issue 51,586 → bridge Admit 35,564 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 18,161 | 286,626 | 6% | 2,053 | 2,309,432 | applied |
| 12 | Transfer 1 ETH (A): the payer to a holder | bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 36,829 → bridge Credit 16,863 | 94,664 | 2% | 691 | 762,840 | applied |
| 13 | Transfer 0 | bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 26,520 | 67,492 | 1% | 691 | 545,464 | refused: exit 240 (BAD_MESSAGE) |
| 14 | Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 26,696 | 67,668 | 1% | 691 | 546,872 | refused: exit 240 (BAD_MESSAGE) |
| 15 | Transfer 100 ETH (more than the payer holds) | bridge Transfer 17,578 → bridge Transfer 19,446 | 37,024 | 1% | 691 | 301,720 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 16 | Transfer of a token with no holding (B's ETH) | bridge Transfer 17,578 → bridge Transfer 19,062 | 36,640 | 1% | 691 | 298,648 | refused: exit 254 (INSUFFICIENT_BALANCE) |
| 17 | Transfer from the recipient's actor, not signed by it | bridge Transfer 10,606 | 10,606 | 0% | 691 | 90,376 | refused: exit 253 (NOT_AUTHORIZED) |
| 18 | Transfer sent to holder(recipient) itself | bridge Transfer 14,220 | 14,220 | 0% | 691 | 119,288 | refused: exit 253 (NOT_AUTHORIZED) |
| 19 | Transfer sent to token(A, ETH) itself (a credit from nothing) | bridge Transfer 24,386 | 24,386 | 0% | 691 | 200,616 | refused: exit 243 (WRONG_SENDER) |
| 20 | Credit sent to holder(payer) itself | bridge Credit 14,059 | 14,059 | 0% | 499 | 116,464 | refused: exit 243 (WRONG_SENDER) |
| 21 | Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 36,807 → bridge Credit 21,307 | 99,086 | 2% | 867 | 799,624 | applied |
| 22 | ProposeRoles by a signer with no role | bridge ProposeRoles 16,270 → bridge ProposeRoles 26,995 | 43,265 | 1% | 881 | 353,168 | refused: exit 253 (NOT_AUTHORIZED) |
| 23 | ProposeRoles by the freeze authority | bridge ProposeRoles 16,270 → bridge ProposeRoles 27,217 | 43,487 | 1% | 881 | 354,944 | refused: exit 253 (NOT_AUTHORIZED) |
| 24 | ProposeRoles sent to config(A) itself | bridge ProposeRoles 24,975 | 24,975 | 0% | 577 | 204,416 | refused: exit 253 (NOT_AUTHORIZED) |
| 25 | ProposeRoles: threshold 0 | bridge ProposeRoles 15,499 → bridge ProposeRoles 26,775 | 42,274 | 1% | 849 | 344,984 | refused: exit 240 (BAD_MESSAGE) |
| 26 | ProposeRoles: threshold 4 of 3 | bridge ProposeRoles 15,499 → bridge ProposeRoles 26,776 | 42,275 | 1% | 849 | 344,992 | refused: exit 240 (BAD_MESSAGE) |
| 27 | ProposeRoles: the admin as a member | bridge ProposeRoles 14,769 → bridge ProposeRoles 26,576 | 41,345 | 1% | 817 | 337,296 | refused: exit 240 (BAD_MESSAGE) |
| 28 | ProposeRoles: a member twice | bridge ProposeRoles 14,769 → bridge ProposeRoles 26,614 | 41,383 | 1% | 817 | 337,600 | refused: exit 240 (BAD_MESSAGE) |
| 29 | ProposeRoles: four watchers | bridge ProposeRoles 18,478 → bridge ProposeRoles 28,969 | 47,447 | 1% | 977 | 387,392 | refused: exit 240 (BAD_MESSAGE) |
| 30 | ApplyRoles with nothing proposed | bridge ApplyRoles 21,491 | 21,491 | 0% | 504 | 175,960 | refused: exit 245 (NOT_FOUND) |
| 31 | CancelRoles with nothing proposed | bridge CancelRoles 12,610 → bridge CancelRoles 25,054 | 37,664 | 1% | 671 | 306,680 | refused: exit 245 (NOT_FOUND) |
| 32 | ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge ProposeRoles 16,270 → bridge ProposeRoles 40,075 → Read 11,877 → bridge (reply) 36,354 | 104,576 | 2% | 881 | 843,656 | applied |
| 33 | CancelRoles by the freeze authority | bridge CancelRoles 12,610 → bridge CancelRoles 27,106 | 39,716 | 1% | 671 | 323,096 | refused: exit 253 (NOT_AUTHORIZED) |
| 34 | CancelRoles by the admin | bridge CancelRoles 12,610 → bridge CancelRoles 34,606 | 47,216 | 1% | 671 | 383,096 | applied |
| 35 | ProposeRoles by the admin again | bridge ProposeRoles 16,270 → bridge ProposeRoles 40,075 → Read 11,877 → bridge (reply) 36,354 | 104,576 | 2% | 881 | 843,656 | applied |
| 36 | ApplyRoles at the proposal's own record | bridge ApplyRoles 36,048 → Read 11,877 → bridge (reply) 27,680 | 75,605 | 2% | 504 | 608,872 | refused: exit 242 (FROZEN) |
| 37 | Pause by the proposed watcher, before the handover | bridge Pause 13,143 → bridge Pause 27,253 | 40,396 | 1% | 747 | 329,144 | refused: exit 253 (NOT_AUTHORIZED) |
| 38 | ApplyRoles with a record 13 days later | bridge ApplyRoles 36,048 → Read 11,877 → bridge (reply) 27,679 | 75,604 | 2% | 504 | 608,864 | refused: exit 242 (FROZEN) |
| 39 | ApplyRoles with a record 14 days later (anyone) | bridge ApplyRoles 36,048 → Read 11,877 → bridge (reply) 36,007 | 83,932 | 2% | 504 | 675,488 | applied |
| 40 | ApplyRoles again | bridge ApplyRoles 22,897 | 22,897 | 0% | 504 | 187,208 | refused: exit 245 (NOT_FOUND) |
| 41 | Freeze by the replaced freeze authority | bridge Freeze 13,251 → bridge Freeze 26,675 | 39,926 | 1% | 755 | 325,448 | refused: exit 253 (NOT_AUTHORIZED) |
| 42 | Freeze by one member alone (2 of 3) | bridge Freeze 13,251 → bridge Freeze 26,675 | 39,926 | 1% | 755 | 325,448 | refused: exit 253 (NOT_AUTHORIZED) |
| 43 | Cosigned freeze by one member | bridge Cosigned 13,723 → bridge Cosigned 28,757 | 42,480 | 1% | 792 | 346,176 | refused: exit 253 (NOT_AUTHORIZED) |
| 44 | Cosigned freeze naming M1, M2, signed by M1 only | bridge Cosigned 14,546 → bridge Cosigned 12,252 | 26,798 | 1% | 888 | 221,488 | refused: exit 253 (NOT_AUTHORIZED) |
| 45 | Cosigned freeze by M1 and a non-member | bridge Cosigned 14,546 → bridge Cosigned 16,289 → bridge Cosigned 29,534 | 60,369 | 1% | 1,064 | 491,464 | refused: exit 253 (NOT_AUTHORIZED) |
| 46 | Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned 14,546 → bridge Cosigned 16,289 → bridge Cosigned 42,270 → Read 11,877 → bridge (reply) 35,646 | 120,628 | 2% | 1,064 | 973,536 | applied |
| 47 | Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned 15,055 → bridge Cosigned 16,798 → bridge Cosigned 37,416 → bridge Freeze 46,445 → Read 11,877 → bridge (reply) 40,799 | 168,390 | 3% | 1,148 | 1,356,304 | applied |
| 48 | Cosigned freeze for 31 days | bridge Cosigned 14,546 → bridge Cosigned 16,289 → bridge Cosigned 42,270 → Read 11,877 → bridge (reply) 26,852 | 111,834 | 2% | 1,064 | 903,184 | refused: exit 240 (BAD_MESSAGE) |
| 49 | Pause by a signer with no role | bridge Pause 13,143 → bridge Pause 26,598 | 39,741 | 1% | 747 | 323,904 | refused: exit 253 (NOT_AUTHORIZED) |
| 50 | Pause by a member of the freeze authority | bridge Pause 13,143 → bridge Pause 26,598 | 39,741 | 1% | 747 | 323,904 | refused: exit 253 (NOT_AUTHORIZED) |
| 51 | Pause of neither mints nor burns | bridge Pause 13,145 → bridge Pause 38,569 → Read 11,877 → bridge (reply) 26,803 | 90,394 | 2% | 747 | 729,128 | refused: exit 240 (BAD_MESSAGE) |
| 52 | Pause A's mints and burns (the watcher) | bridge Pause 13,143 → bridge Pause 38,567 → Read 11,877 → bridge (reply) 35,447 | 99,034 | 2% | 747 | 798,248 | applied |
| 53 | Pause ETH (A) mints (the watcher) | bridge Pause 15,913 → bridge Pause 34,119 → bridge Pause 46,321 → Read 11,877 → bridge (reply) 40,755 | 148,985 | 3% | 831 | 1,198,528 | applied |
| 54 | Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned 14,546 → bridge Cosigned 16,289 → bridge Cosigned 42,286 → Read 11,877 → bridge (reply) 35,646 | 120,644 | 2% | 1,064 | 973,664 | applied |
| 55 | Pause A's mints again (the watcher) | bridge Pause 13,143 → bridge Pause 38,567 → Read 11,877 → bridge (reply) 35,450 | 99,037 | 2% | 747 | 798,272 | applied |
| 56 | Unfreeze by the watcher | bridge Freeze 13,250 → bridge Freeze 26,893 | 40,143 | 1% | 755 | 327,184 | refused: exit 253 (NOT_AUTHORIZED) |
| 57 | Cosigned unfreeze of A by M2 and M3 | bridge Cosigned 14,546 → bridge Cosigned 16,289 → bridge Cosigned 42,302 → Read 11,877 → bridge (reply) 35,636 | 120,650 | 2% | 1,064 | 973,712 | applied |
| 58 | SetCap ETH (A) by the admin, through config(A) | bridge SetCap 16,279 → bridge SetCap 34,311 → bridge SetCap 42,394 | 92,984 | 2% | 789 | 750,184 | applied |
| 59 | SetCap ETH (A) below the floor | bridge SetCap 16,279 → bridge SetCap 34,311 → bridge SetCap 30,244 | 80,834 | 2% | 789 | 652,984 | refused: exit 240 (BAD_MESSAGE) |
| 60 | Sunset sent to config(A) by anyone, not approved | bridge Sunset 24,224 | 24,224 | 0% | 431 | 197,240 | refused: exit 253 (NOT_AUTHORIZED) |
| 61 | Sunset A by the replaced freeze authority | bridge Sunset 12,611 → bridge Sunset 26,236 | 38,847 | 1% | 671 | 316,144 | refused: exit 253 (NOT_AUTHORIZED) |
| 62 | Sunset A by one member alone | bridge Sunset 12,611 → bridge Sunset 26,236 | 38,847 | 1% | 671 | 316,144 | refused: exit 253 (NOT_AUTHORIZED) |
| 63 | Cosigned Sunset by one member | bridge Cosigned 13,255 → bridge Cosigned 28,524 | 41,779 | 1% | 708 | 339,896 | refused: exit 253 (NOT_AUTHORIZED) |
| 64 | Cosigned Sunset approval by M1 and M2 | bridge Cosigned 14,078 → bridge Cosigned 15,821 → bridge Cosigned 37,392 | 67,291 | 1% | 980 | 546,168 | applied |
| 65 | Cosigned Sunset approval again | bridge Cosigned 14,078 → bridge Cosigned 15,821 → bridge Cosigned 29,549 | 59,448 | 1% | 980 | 483,424 | refused: exit 244 (ALREADY_WRITTEN) |
| 66 | Sunset A sent to config(A) by anyone, approved | bridge Sunset 31,151 | 31,151 | 1% | 431 | 252,656 | applied |
| 67 | Sunset A again | bridge Sunset 24,224 | 24,224 | 0% | 431 | 197,240 | refused: exit 244 (ALREADY_WRITTEN) |
| 68 | ImportRoot A after Sunset | bridge ImportRoot 29,544 | 29,544 | 1% | 2,025 | 252,552 | refused: exit 242 (FROZEN) |
| 69 | Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer 17,578 → bridge Transfer 23,394 → bridge Transfer 36,829 → bridge Credit 21,307 | 99,108 | 2% | 691 | 798,392 | applied |
| 70 | Sunset sent to config(B) by anyone, vault not retired | bridge Sunset 22,818 | 22,818 | 0% | 431 | 185,992 | refused: exit 253 (NOT_AUTHORIZED) |
| 71 | AttestRetired B under block 100 (slot 37 empty) | bridge AttestRetired 45,584 → Attest 81,962 → bridge (reply) 35,361 | 162,907 | 3% | 1,473 | 1,315,040 | refused: exit 247 (IMPORT_MISMATCH) |
| 72 | AttestRetired B: a freeze authority, not retired | bridge AttestRetired 46,387 → Attest 92,457 → bridge (reply) 35,361 | 174,205 | 3% | 1,489 | 1,405,552 | refused: exit 247 (IMPORT_MISMATCH) |
| 73 | AttestRetired B: retired (anyone) | bridge AttestRetired 45,415 → Attest 92,785 → bridge (reply) 43,181 | 181,381 | 4% | 1,490 | 1,462,968 | applied |
| 74 | AttestRetired B again | bridge AttestRetired 26,394 | 26,394 | 1% | 1,490 | 223,072 | refused: exit 244 (ALREADY_WRITTEN) |
| 75 | Sunset B forwarded by a signer's own actor (not top-level at config(B)) | bridge Sunset 12,611 → bridge Sunset 24,832 | 37,443 | 1% | 671 | 304,912 | refused: exit 253 (NOT_AUTHORIZED) |
| 76 | Sunset B sent to config(B) by anyone, retired | bridge Sunset 29,079 | 29,079 | 1% | 431 | 236,080 | applied |
| 77 | Sunset B again | bridge Sunset 22,818 | 22,818 | 0% | 431 | 185,992 | refused: exit 244 (ALREADY_WRITTEN) |
| 78 | ImportRoot B after Sunset | bridge ImportRoot 28,339 | 28,339 | 1% | 1,918 | 242,056 | refused: exit 242 (FROZEN) |

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
| Register ETH (A) | bridge Register | 73,920 | 1 | 2,859 | 2,221 | 564 | 5,939 | 950 | 0 |
| Register ETH (A) | Attest | 150,201 | 1 | 2,504 | 2,033 | 333 | 3,318 | – | 0 |
| Register ETH (A) | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (A) | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register ETH (B) | bridge Register | 76,864 | 1 | 2,882 | 2,244 | 564 | 5,985 | 950 | 0 |
| Register ETH (B) | Attest | 155,163 | 1 | 2,527 | 2,056 | 333 | 3,341 | – | 0 |
| Register ETH (B) | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (B) | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| ImportRoot A n=8 | bridge ImportRoot | 51,721 | 1 | 2,137 | 1,499 | 564 | 4,427 | 613 | 0 |
| ImportRoot A n=8 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportRoot A n=8 | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A n=8 | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportRoot B n=8 | bridge ImportRoot | 49,908 | 1 | 2,030 | 1,392 | 564 | 4,213 | 613 | 0 |
| ImportRoot B n=8 | Attest | 114,375 | 1 | 1,944 | 1,473 | 333 | 2,524 | – | 0 |
| ImportRoot B n=8 | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot B n=8 | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): schedule 1 | bridge ImportFees | 55,929 | 1 | 2,318 | 1,680 | 564 | 4,785 | 790 | 0 |
| ImportFees ETH (A): schedule 1 | Attest | 112,749 | 1 | 2,051 | 1,580 | 333 | 2,631 | – | 0 |
| ImportFees ETH (A): schedule 1 | bridge (reply) | 58,259 | 1 | 1,415 | 487 | 790 | 2,366 | 564 | 0 |
| ImportFees ETH (A): schedule 1 | bridge ExtendFees | 41,726 | 1 | 1,239 | 258 | 843 | 2,295 | 1,031 | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | bridge ImportFees | 50,198 | 1 | 2,057 | 1,419 | 564 | 4,263 | 602 | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | Attest | 107,315 | 1 | 1,978 | 1,507 | 333 | 2,558 | – | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH (A): no schedule 2 at 8 leaves | bridge ExtendFees | 39,427 | 1 | 1,239 | 70 | 1,031 | 2,295 | 1,031 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Issue | 51,586 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Spend | 37,291 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim A's leaf 6: 10 ETH to the payer | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Transfer | 23,394 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Transfer | 36,829 | 1 | 1,270 | 101 | 1,031 | 1,496 | – | 0 |
| Transfer 1 ETH (A): the payer to a holder | bridge Credit | 16,863 | 1 | 239 | 101 | 0 | 338 | 74 | 0 |
| Transfer 0 | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 0 | bridge Transfer | 23,394 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 0 | bridge Transfer | 26,520 | 1 | 1,270 | 101 | 1,031 | 0 | – | 240 |
| Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer | 23,394 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 1.5 gwei (not a multiple of the LEZ unit) | bridge Transfer | 26,696 | 1 | 1,270 | 101 | 1,031 | 0 | – | 240 |
| Transfer 100 ETH (more than the payer holds) | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 100 ETH (more than the payer holds) | bridge Transfer | 19,446 | 1 | 313 | 101 | 74 | 0 | – | 254 |
| Transfer of a token with no holding (B's ETH) | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer of a token with no holding (B's ETH) | bridge Transfer | 19,062 | 1 | 313 | 101 | 74 | 0 | – | 254 |
| Transfer from the recipient's actor, not signed by it | bridge Transfer | 10,606 | 1 | 175 | 101 | 0 | 0 | – | 253 |
| Transfer sent to holder(recipient) itself | bridge Transfer | 14,220 | 1 | 249 | 101 | 74 | 0 | – | 253 |
| Transfer sent to token(A, ETH) itself (a credit from nothing) | bridge Transfer | 24,386 | 1 | 1,206 | 101 | 1,031 | 0 | – | 243 |
| Credit sent to holder(payer) itself | bridge Credit | 14,059 | 1 | 249 | 101 | 74 | 0 | – | 243 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer | 23,394 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Transfer | 36,807 | 1 | 1,270 | 101 | 1,031 | 1,496 | – | 0 |
| Transfer 0.1 ETH (A): the holder, signed, to the payer | bridge Credit | 21,307 | 1 | 313 | 101 | 74 | 412 | 74 | 0 |
| ProposeRoles by a signer with no role | bridge ProposeRoles | 16,270 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by a signer with no role | bridge ProposeRoles | 26,995 | 1 | 881 | 179 | 564 | 0 | – | 253 |
| ProposeRoles by the freeze authority | bridge ProposeRoles | 16,270 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by the freeze authority | bridge ProposeRoles | 27,217 | 1 | 881 | 179 | 564 | 0 | – | 253 |
| ProposeRoles sent to config(A) itself | bridge ProposeRoles | 24,975 | 1 | 817 | 179 | 564 | 0 | – | 253 |
| ProposeRoles: threshold 0 | bridge ProposeRoles | 15,499 | 1 | 221 | 147 | 0 | 461 | – | 0 |
| ProposeRoles: threshold 0 | bridge ProposeRoles | 26,775 | 1 | 849 | 147 | 564 | 0 | – | 240 |
| ProposeRoles: threshold 4 of 3 | bridge ProposeRoles | 15,499 | 1 | 221 | 147 | 0 | 461 | – | 0 |
| ProposeRoles: threshold 4 of 3 | bridge ProposeRoles | 26,776 | 1 | 849 | 147 | 564 | 0 | – | 240 |
| ProposeRoles: the admin as a member | bridge ProposeRoles | 14,769 | 1 | 189 | 115 | 0 | 397 | – | 0 |
| ProposeRoles: the admin as a member | bridge ProposeRoles | 26,576 | 1 | 817 | 115 | 564 | 0 | – | 240 |
| ProposeRoles: a member twice | bridge ProposeRoles | 14,769 | 1 | 189 | 115 | 0 | 397 | – | 0 |
| ProposeRoles: a member twice | bridge ProposeRoles | 26,614 | 1 | 817 | 115 | 564 | 0 | – | 240 |
| ProposeRoles: four watchers | bridge ProposeRoles | 18,478 | 1 | 349 | 275 | 0 | 717 | – | 0 |
| ProposeRoles: four watchers | bridge ProposeRoles | 28,969 | 1 | 977 | 275 | 564 | 0 | – | 240 |
| ApplyRoles with nothing proposed | bridge ApplyRoles | 21,491 | 1 | 680 | 42 | 564 | 0 | – | 245 |
| CancelRoles with nothing proposed | bridge CancelRoles | 12,610 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| CancelRoles with nothing proposed | bridge CancelRoles | 25,054 | 1 | 735 | 33 | 564 | 0 | – | 245 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge ProposeRoles | 16,270 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge ProposeRoles | 40,075 | 1 | 881 | 179 | 564 | 1,691 | 712 | 0 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| ProposeRoles by the admin: 2 of 3, one watcher (block 102's record) | bridge (reply) | 36,354 | 1 | 1,187 | 337 | 712 | 1,937 | 709 | 0 |
| CancelRoles by the freeze authority | bridge CancelRoles | 12,610 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| CancelRoles by the freeze authority | bridge CancelRoles | 27,106 | 1 | 880 | 33 | 709 | 0 | – | 253 |
| CancelRoles by the admin | bridge CancelRoles | 12,610 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| CancelRoles by the admin | bridge CancelRoles | 34,606 | 1 | 880 | 33 | 709 | 1,469 | 564 | 0 |
| ProposeRoles by the admin again | bridge ProposeRoles | 16,270 | 1 | 253 | 179 | 0 | 525 | – | 0 |
| ProposeRoles by the admin again | bridge ProposeRoles | 40,075 | 1 | 881 | 179 | 564 | 1,691 | 712 | 0 |
| ProposeRoles by the admin again | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| ProposeRoles by the admin again | bridge (reply) | 36,354 | 1 | 1,187 | 337 | 712 | 1,937 | 709 | 0 |
| ApplyRoles at the proposal's own record | bridge ApplyRoles | 36,048 | 1 | 825 | 42 | 709 | 1,643 | 720 | 0 |
| ApplyRoles at the proposal's own record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| ApplyRoles at the proposal's own record | bridge (reply) | 27,680 | 1 | 1,195 | 337 | 720 | 0 | – | 242 |
| Pause by the proposed watcher, before the handover | bridge Pause | 13,143 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause by the proposed watcher, before the handover | bridge Pause | 27,253 | 1 | 892 | 45 | 709 | 0 | – | 253 |
| ApplyRoles with a record 13 days later | bridge ApplyRoles | 36,048 | 1 | 825 | 42 | 709 | 1,643 | 720 | 0 |
| ApplyRoles with a record 13 days later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| ApplyRoles with a record 13 days later | bridge (reply) | 27,679 | 1 | 1,195 | 337 | 720 | 0 | – | 242 |
| ApplyRoles with a record 14 days later (anyone) | bridge ApplyRoles | 36,048 | 1 | 825 | 42 | 709 | 1,643 | 720 | 0 |
| ApplyRoles with a record 14 days later (anyone) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| ApplyRoles with a record 14 days later (anyone) | bridge (reply) | 36,007 | 1 | 1,195 | 337 | 720 | 1,888 | 660 | 0 |
| ApplyRoles again | bridge ApplyRoles | 22,897 | 1 | 776 | 42 | 660 | 0 | – | 245 |
| Freeze by the replaced freeze authority | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze by the replaced freeze authority | bridge Freeze | 26,675 | 1 | 851 | 53 | 660 | 0 | – | 253 |
| Freeze by one member alone (2 of 3) | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze by one member alone (2 of 3) | bridge Freeze | 26,675 | 1 | 851 | 53 | 660 | 0 | – | 253 |
| Cosigned freeze by one member | bridge Cosigned | 13,723 | 1 | 164 | 90 | 0 | 347 | – | 0 |
| Cosigned freeze by one member | bridge Cosigned | 28,757 | 1 | 888 | 90 | 660 | 0 | – | 253 |
| Cosigned freeze naming M1, M2, signed by M1 only | bridge Cosigned | 14,546 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze naming M1, M2, signed by M1 only | bridge Cosigned | 12,252 | 1 | 260 | 122 | 0 | 0 | – | 253 |
| Cosigned freeze by M1 and a non-member | bridge Cosigned | 14,546 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze by M1 and a non-member | bridge Cosigned | 16,289 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze by M1 and a non-member | bridge Cosigned | 29,534 | 1 | 920 | 122 | 660 | 0 | – | 253 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned | 14,546 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned | 16,289 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge Cosigned | 42,270 | 1 | 920 | 122 | 660 | 1,699 | 681 | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Cosigned freeze of A's mints for an hour by M1 and M2 | bridge (reply) | 35,646 | 1 | 1,156 | 337 | 681 | 1,849 | 660 | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned | 15,055 | 1 | 216 | 142 | 0 | 451 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned | 16,798 | 1 | 280 | 142 | 0 | 515 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Cosigned | 37,416 | 1 | 940 | 142 | 660 | 1,106 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge Freeze | 46,445 | 1 | 1,242 | 73 | 1,031 | 2,391 | 1,051 | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Cosigned freeze of ETH (A) burns by M3 and M2 | bridge (reply) | 40,799 | 1 | 1,526 | 337 | 1,051 | 2,590 | 1,031 | 0 |
| Cosigned freeze for 31 days | bridge Cosigned | 14,546 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze for 31 days | bridge Cosigned | 16,289 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze for 31 days | bridge Cosigned | 42,270 | 1 | 920 | 122 | 660 | 1,699 | 681 | 0 |
| Cosigned freeze for 31 days | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Cosigned freeze for 31 days | bridge (reply) | 26,852 | 1 | 1,156 | 337 | 681 | 0 | – | 240 |
| Pause by a signer with no role | bridge Pause | 13,143 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause by a signer with no role | bridge Pause | 26,598 | 1 | 843 | 45 | 660 | 0 | – | 253 |
| Pause by a member of the freeze authority | bridge Pause | 13,143 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause by a member of the freeze authority | bridge Pause | 26,598 | 1 | 843 | 45 | 660 | 0 | – | 253 |
| Pause of neither mints nor burns | bridge Pause | 13,145 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause of neither mints nor burns | bridge Pause | 38,569 | 1 | 843 | 45 | 660 | 1,614 | 673 | 0 |
| Pause of neither mints nor burns | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Pause of neither mints nor burns | bridge (reply) | 26,803 | 1 | 1,148 | 337 | 673 | 0 | – | 240 |
| Pause A's mints and burns (the watcher) | bridge Pause | 13,143 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause A's mints and burns (the watcher) | bridge Pause | 38,567 | 1 | 843 | 45 | 660 | 1,614 | 673 | 0 |
| Pause A's mints and burns (the watcher) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Pause A's mints and burns (the watcher) | bridge (reply) | 35,447 | 1 | 1,148 | 337 | 673 | 1,841 | 660 | 0 |
| Pause ETH (A) mints (the watcher) | bridge Pause | 15,913 | 1 | 139 | 65 | 0 | 297 | – | 0 |
| Pause ETH (A) mints (the watcher) | bridge Pause | 34,119 | 1 | 863 | 65 | 660 | 1,021 | – | 0 |
| Pause ETH (A) mints (the watcher) | bridge Pause | 46,321 | 1 | 1,234 | 65 | 1,031 | 2,375 | 1,043 | 0 |
| Pause ETH (A) mints (the watcher) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Pause ETH (A) mints (the watcher) | bridge (reply) | 40,755 | 1 | 1,518 | 337 | 1,043 | 2,582 | 1,031 | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned | 14,546 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned | 16,289 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge Cosigned | 42,286 | 1 | 920 | 122 | 660 | 1,699 | 681 | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Cosigned freeze of A's mints for two days by M1 and M3 | bridge (reply) | 35,646 | 1 | 1,156 | 337 | 681 | 1,849 | 660 | 0 |
| Pause A's mints again (the watcher) | bridge Pause | 13,143 | 1 | 119 | 45 | 0 | 257 | – | 0 |
| Pause A's mints again (the watcher) | bridge Pause | 38,567 | 1 | 843 | 45 | 660 | 1,614 | 673 | 0 |
| Pause A's mints again (the watcher) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Pause A's mints again (the watcher) | bridge (reply) | 35,450 | 1 | 1,148 | 337 | 673 | 1,841 | 660 | 0 |
| Unfreeze by the watcher | bridge Freeze | 13,250 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze by the watcher | bridge Freeze | 26,893 | 1 | 851 | 53 | 660 | 0 | – | 253 |
| Cosigned unfreeze of A by M2 and M3 | bridge Cosigned | 14,546 | 1 | 196 | 122 | 0 | 411 | – | 0 |
| Cosigned unfreeze of A by M2 and M3 | bridge Cosigned | 16,289 | 1 | 260 | 122 | 0 | 475 | – | 0 |
| Cosigned unfreeze of A by M2 and M3 | bridge Cosigned | 42,302 | 1 | 920 | 122 | 660 | 1,699 | 681 | 0 |
| Cosigned unfreeze of A by M2 and M3 | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Cosigned unfreeze of A by M2 and M3 | bridge (reply) | 35,636 | 1 | 1,156 | 337 | 681 | 1,849 | 660 | 0 |
| SetCap ETH (A) by the admin, through config(A) | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap ETH (A) by the admin, through config(A) | bridge SetCap | 34,311 | 1 | 885 | 87 | 660 | 1,065 | – | 0 |
| SetCap ETH (A) by the admin, through config(A) | bridge SetCap | 42,394 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| SetCap ETH (A) below the floor | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap ETH (A) below the floor | bridge SetCap | 34,311 | 1 | 885 | 87 | 660 | 1,065 | – | 0 |
| SetCap ETH (A) below the floor | bridge SetCap | 30,244 | 1 | 1,256 | 87 | 1,031 | 0 | – | 240 |
| Sunset sent to config(A) by anyone, not approved | bridge Sunset | 24,224 | 1 | 767 | 33 | 660 | 0 | – | 253 |
| Sunset A by the replaced freeze authority | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset A by the replaced freeze authority | bridge Sunset | 26,236 | 1 | 831 | 33 | 660 | 0 | – | 253 |
| Sunset A by one member alone | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset A by one member alone | bridge Sunset | 26,236 | 1 | 831 | 33 | 660 | 0 | – | 253 |
| Cosigned Sunset by one member | bridge Cosigned | 13,255 | 1 | 144 | 70 | 0 | 307 | – | 0 |
| Cosigned Sunset by one member | bridge Cosigned | 28,524 | 1 | 868 | 70 | 660 | 0 | – | 253 |
| Cosigned Sunset approval by M1 and M2 | bridge Cosigned | 14,078 | 1 | 176 | 102 | 0 | 371 | – | 0 |
| Cosigned Sunset approval by M1 and M2 | bridge Cosigned | 15,821 | 1 | 240 | 102 | 0 | 435 | – | 0 |
| Cosigned Sunset approval by M1 and M2 | bridge Cosigned | 37,392 | 1 | 900 | 102 | 660 | 1,585 | 660 | 0 |
| Cosigned Sunset approval again | bridge Cosigned | 14,078 | 1 | 176 | 102 | 0 | 371 | – | 0 |
| Cosigned Sunset approval again | bridge Cosigned | 15,821 | 1 | 240 | 102 | 0 | 435 | – | 0 |
| Cosigned Sunset approval again | bridge Cosigned | 29,549 | 1 | 900 | 102 | 660 | 0 | – | 244 |
| Sunset A sent to config(A) by anyone, approved | bridge Sunset | 31,151 | 1 | 767 | 33 | 660 | 1,452 | 660 | 0 |
| Sunset A again | bridge Sunset | 24,224 | 1 | 767 | 33 | 660 | 0 | – | 244 |
| ImportRoot A after Sunset | bridge ImportRoot | 29,544 | 1 | 2,233 | 1,499 | 660 | 0 | – | 242 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer | 17,578 | 1 | 175 | 101 | 0 | 401 | – | 0 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer | 23,394 | 1 | 313 | 101 | 74 | 585 | 74 | 0 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Transfer | 36,829 | 1 | 1,270 | 101 | 1,031 | 1,496 | – | 0 |
| Transfer 1 ETH (A) after Sunset: balances still move | bridge Credit | 21,307 | 1 | 313 | 101 | 74 | 412 | 74 | 0 |
| Sunset sent to config(B) by anyone, vault not retired | bridge Sunset | 22,818 | 1 | 671 | 33 | 564 | 0 | – | 253 |
| AttestRetired B under block 100 (slot 37 empty) | bridge AttestRetired | 45,584 | 1 | 1,649 | 1,011 | 564 | 3,411 | 573 | 0 |
| AttestRetired B under block 100 (slot 37 empty) | Attest | 81,962 | 1 | 1,563 | 1,092 | 333 | 2,026 | – | 0 |
| AttestRetired B under block 100 (slot 37 empty) | bridge (reply) | 35,361 | 1 | 1,081 | 370 | 573 | 0 | – | 247 |
| AttestRetired B: a freeze authority, not retired | bridge AttestRetired | 46,387 | 1 | 1,665 | 1,027 | 564 | 3,443 | 573 | 0 |
| AttestRetired B: a freeze authority, not retired | Attest | 92,457 | 1 | 1,579 | 1,108 | 333 | 2,042 | – | 0 |
| AttestRetired B: a freeze authority, not retired | bridge (reply) | 35,361 | 1 | 1,081 | 370 | 573 | 0 | – | 247 |
| AttestRetired B: retired (anyone) | bridge AttestRetired | 45,415 | 1 | 1,666 | 1,028 | 564 | 3,445 | 573 | 0 |
| AttestRetired B: retired (anyone) | Attest | 92,785 | 1 | 1,580 | 1,109 | 333 | 2,043 | – | 0 |
| AttestRetired B: retired (anyone) | bridge (reply) | 43,181 | 1 | 1,081 | 370 | 573 | 1,670 | 564 | 0 |
| AttestRetired B again | bridge AttestRetired | 26,394 | 1 | 1,666 | 1,028 | 564 | 0 | – | 244 |
| Sunset B forwarded by a signer's own actor (not top-level at config(B)) | bridge Sunset | 12,611 | 1 | 107 | 33 | 0 | 233 | – | 0 |
| Sunset B forwarded by a signer's own actor (not top-level at config(B)) | bridge Sunset | 24,832 | 1 | 735 | 33 | 564 | 0 | – | 253 |
| Sunset B sent to config(B) by anyone, retired | bridge Sunset | 29,079 | 1 | 671 | 33 | 564 | 1,260 | 564 | 0 |
| Sunset B again | bridge Sunset | 22,818 | 1 | 671 | 33 | 564 | 0 | – | 244 |
| ImportRoot B after Sunset | bridge ImportRoot | 28,339 | 1 | 2,030 | 1,392 | 564 | 0 | – | 242 |

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
