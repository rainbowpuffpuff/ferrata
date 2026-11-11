# The bridge program's clock: attested Ethereum time (`harness/tests/bridge_time.rs`)

Bridge program image `cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390`; every transaction settled by LEZ's own functions in a block of its own, stamped with the world's LEZ time (a year ahead where the label says so).

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Tick A: the newest record | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,537 | 76,845 | 2% | 505 | 618,800 | applied |
| 2 | Tick ETH (A): the newest record | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,738 | 94,926 | 2% | 525 | 763,608 | applied |
| 3 | Tick B: the newest record | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,537 | 76,845 | 2% | 505 | 618,800 | applied |
| 4 | Freeze B's mints for one hour | bridge Freeze 12,631 → bridge Freeze 36,354 → Read 11,877 → bridge (reply) 32,565 | 93,427 | 2% | 755 | 753,456 | applied |
| 5 | A block stamped a year ahead: Tick B with the newest record | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,538 | 76,846 | 2% | 505 | 618,808 | applied |
| 6 | Claim B's 1 ETH in that block: the freeze holds | bridge Claim 107,479 → bridge Issue 48,227 → bridge Admit 27,103 | 182,809 | 4% | 2,053 | 1,478,896 | refused: exit 242 (FROZEN) |
| 7 | Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze 12,631 → bridge Freeze 36,354 → Read 11,877 → bridge (reply) 32,566 | 93,428 | 2% | 755 | 753,464 | applied |
| 8 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 13,489 → bridge SetCap 42,315 | 55,804 | 1% | 725 | 452,232 | applied |
| 9 | Claim leaf 6 (10.06 ETH of the 11) | bridge Claim 107,479 → bridge Issue 52,518 → bridge Admit 33,885 → bridge Spend 36,659 → bridge Mint 17,560 → bridge Mint 17,560 → bridge Mint 17,560 | 283,221 | 6% | 2,053 | 2,282,192 | applied |
| 10 | A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,739 | 94,927 | 2% | 525 | 763,616 | applied |
| 11 | Tick ETH (A) with an older record (block 100's) | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,739 | 94,927 | 2% | 525 | 763,616 | applied |
| 12 | Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim 107,479 → bridge Issue 29,931 | 137,410 | 3% | 2,053 | 1,115,704 | refused: exit 255 (CAPPED) |
| 13 | Tick ETH (A): a record 10 s later | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,737 | 94,925 | 2% | 525 | 763,600 | applied |
| 14 | Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim 107,479 → bridge Issue 52,483 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 17,560 → bridge Mint 22,280 | 263,988 | 5% | 2,053 | 2,128,328 | applied |
| 15 | Tick B: a record 30 days after the freeze's start | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,537 | 76,845 | 2% | 505 | 618,800 | applied |
| 16 | Claim B's 1 ETH: the freeze has lapsed | bridge Claim 107,479 → bridge Issue 48,227 → bridge Admit 33,886 → bridge Spend 30,300 → bridge Mint 22,568 → bridge Mint 22,568 | 265,028 | 5% | 2,053 | 2,136,648 | applied |
| 17 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 13,490 → bridge SetCap 42,315 | 55,805 | 1% | 725 | 452,240 | applied |
| 18 | Tick ETH (A): the newest record (30 days on) | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,738 | 94,926 | 2% | 525 | 763,608 | applied |
| 19 | Freeze A's burns for one hour | bridge Freeze 12,631 → bridge Freeze 36,354 → Read 11,877 → bridge (reply) 32,564 | 93,426 | 2% | 755 | 753,448 | applied |
| 20 | A year of LEZ blocks later, no newer record: Tick A | bridge Tick 32,431 → Read 11,877 → bridge (reply) 32,538 | 76,846 | 2% | 505 | 618,808 | applied |
| 21 | Burn 1 ETH (A): A's burns still frozen | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 54,066 → bridge Admit 27,103 | 136,728 | 3% | 839 | 1,100,536 | refused: exit 242 (FROZEN) |
| 22 | Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze 12,631 → bridge Freeze 36,354 → Read 11,877 → bridge (reply) 32,551 | 93,413 | 2% | 755 | 753,344 | applied |
| 23 | Burn 1 ETH (A) | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 54,066 → bridge Admit 33,885 → bridge Burn 20,934 | 164,444 | 3% | 839 | 1,322,264 | applied |
| 24 | Tick ETH (A), still no newer record | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,739 | 94,927 | 2% | 525 | 763,616 | applied |
| 25 | Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 34,219 | 89,778 | 2% | 839 | 724,936 | refused: exit 255 (CAPPED) |
| 26 | SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap 13,490 → bridge SetCap 42,318 | 55,808 | 1% | 725 | 452,264 | applied |
| 27 | Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn 23,952 → bridge Burn 31,607 → bridge Burn 34,219 | 89,778 | 2% | 839 | 724,936 | refused: exit 255 (CAPPED) |
| 28 | Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,738 | 10,000,000 | 200% | 525 | 80,004,200 | reverted: Execution outside of the validity window |
| 29 | The same Tick in a block its producer stamps an hour ahead too | bridge Tick 42,311 → Read 11,877 → bridge (reply) 40,738 | 94,926 | 2% | 525 | 763,608 | applied |
| 30 | Tick A: a Gloas record without its block's header | bridge Tick 32,431 → Read 10,921 → bridge (reply) 23,718 | 67,070 | 1% | 505 | 540,600 | refused: exit 247 (IMPORT_MISMATCH) |
| 31 | Tick A: a Gloas record with a header that does not hash to it | bridge Tick 40,318 → Read 10,921 → bridge (reply) 129,894 | 181,133 | 4% | 1,196 | 1,458,632 | refused: exit 247 (IMPORT_MISMATCH) |
| 32 | Tick A: a Gloas record with its block's header | bridge Tick 40,318 → Read 10,921 → bridge (reply) 142,904 | 194,143 | 4% | 1,196 | 1,562,712 | applied |
| 33 | Tick A: a payload record with a header | bridge Tick 40,318 → Read 11,877 → bridge (reply) 26,530 | 78,725 | 2% | 1,196 | 639,368 | refused: exit 247 (IMPORT_MISMATCH) |
| 34 | Tick A: a slot with no record | bridge Tick 32,431 → Read 8,844 → bridge (reply) 20,366 | 61,641 | 1% | 570 | 497,688 | refused: exit 245 (NOT_FOUND) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Tick A: the newest record | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: the newest record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Tick ETH (A): the newest record | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the newest record | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick B: the newest record | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: the newest record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Freeze B's mints for one hour | bridge Freeze | 12,631 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for one hour | bridge Freeze | 36,354 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze B's mints for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze B's mints for one hour | bridge (reply) | 32,565 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| A block stamped a year ahead: Tick B with the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge (reply) | 32,538 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Issue | 48,227 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Admit | 27,103 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 12,631 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 36,354 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge (reply) | 32,566 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 13,489 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,315 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Issue | 52,518 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Admit | 33,885 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Spend | 36,659 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge (reply) | 40,739 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with an older record (block 100's) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge (reply) | 40,739 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Issue | 29,931 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): a record 10 s later | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 10 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): a record 10 s later | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Issue | 52,483 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 17,560 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 22,280 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Tick B: a record 30 days after the freeze's start | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: a record 30 days after the freeze's start | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: a record 30 days after the freeze's start | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Claim | 107,479 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Issue | 48,227 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Admit | 33,886 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Spend | 30,300 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 22,568 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 22,568 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 13,490 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,315 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record (30 days on) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Freeze A's burns for one hour | bridge Freeze | 12,631 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze A's burns for one hour | bridge Freeze | 36,354 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze A's burns for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze A's burns for one hour | bridge (reply) | 32,564 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge (reply) | 32,538 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 54,066 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Admit | 27,103 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 12,631 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 36,354 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge (reply) | 32,551 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 54,066 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 33,885 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 20,934 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A), still no newer record | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A), still no newer record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A), still no newer record | bridge (reply) | 40,739 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 34,219 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 13,490 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 42,318 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 23,952 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 31,607 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 34,219 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge Tick | 42,311 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick A: a Gloas record without its block's header | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: a Gloas record without its block's header | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record without its block's header | bridge (reply) | 23,718 | 1 | 894 | 225 | 531 | 0 | – | 247 |
| Tick A: a Gloas record with a header that does not hash to it | bridge Tick | 40,318 | 1 | 1,328 | 734 | 520 | 2,648 | 1,222 | 0 |
| Tick A: a Gloas record with a header that does not hash to it | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record with a header that does not hash to it | bridge (reply) | 129,894 | 1 | 1,585 | 225 | 1,222 | 0 | – | 247 |
| Tick A: a Gloas record with its block's header | bridge Tick | 40,318 | 1 | 1,328 | 734 | 520 | 2,648 | 1,222 | 0 |
| Tick A: a Gloas record with its block's header | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record with its block's header | bridge (reply) | 142,904 | 1 | 1,585 | 225 | 1,222 | 2,138 | 520 | 0 |
| Tick A: a payload record with a header | bridge Tick | 40,318 | 1 | 1,328 | 734 | 520 | 2,648 | 1,222 | 0 |
| Tick A: a payload record with a header | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: a payload record with a header | bridge (reply) | 26,530 | 1 | 1,697 | 337 | 1,222 | 0 | – | 247 |
| Tick A: a slot with no record | bridge Tick | 32,431 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: a slot with no record | Read | 8,844 | 1 | 139 | 1 | 0 | 236 | – | 0 |
| Tick A: a slot with no record | bridge (reply) | 20,366 | 1 | 673 | 4 | 531 | 0 | – | 245 |

