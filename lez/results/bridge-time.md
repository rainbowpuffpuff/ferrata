# The bridge program's clock: attested Ethereum time (`harness/tests/bridge_time.rs`)

Bridge program image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`; every transaction settled by LEZ's own functions in a block of its own, stamped with the world's LEZ time (a year ahead where the label says so).

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Tick A: the newest record | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,537 | 76,899 | 2% | 505 | 619,232 | applied |
| 2 | Tick ETH (A): the newest record | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,738 | 94,980 | 2% | 525 | 764,040 | applied |
| 3 | Tick B: the newest record | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,537 | 76,899 | 2% | 505 | 619,232 | applied |
| 4 | Freeze B's mints for one hour | bridge Freeze 12,680 → bridge Freeze 36,403 → Read 11,877 → bridge (reply) 32,565 | 93,525 | 2% | 755 | 754,240 | applied |
| 5 | A block stamped a year ahead: Tick B with the newest record | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,538 | 76,900 | 2% | 505 | 619,240 | applied |
| 6 | Claim B's 1 ETH in that block: the freeze holds | bridge Claim 107,610 → bridge Issue 48,456 → bridge Admit 27,200 | 183,266 | 4% | 2,053 | 1,482,552 | refused: exit 242 (FROZEN) |
| 7 | Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze 12,680 → bridge Freeze 36,403 → Read 11,877 → bridge (reply) 32,566 | 93,526 | 2% | 755 | 754,248 | applied |
| 8 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 13,538 → bridge SetCap 42,364 | 55,902 | 1% | 725 | 453,016 | applied |
| 9 | Claim leaf 6 (10.06 ETH of the 11) | bridge Claim 107,610 → bridge Issue 52,747 → bridge Admit 33,982 → bridge Spend 24,414 → bridge Mint 14,616 → bridge Mint 14,616 → bridge Mint 14,616 | 262,601 | 5% | 2,053 | 2,117,232 | applied |
| 10 | A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,739 | 94,981 | 2% | 525 | 764,048 | applied |
| 11 | Tick ETH (A) with an older record (block 100's) | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,739 | 94,981 | 2% | 525 | 764,048 | applied |
| 12 | Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim 107,610 → bridge Issue 29,965 | 137,575 | 3% | 2,053 | 1,117,024 | refused: exit 255 (CAPPED) |
| 13 | Tick ETH (A): a record 10 s later | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,737 | 94,979 | 2% | 525 | 764,032 | applied |
| 14 | Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim 107,610 → bridge Issue 52,712 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 14,616 → bridge Mint 19,553 | 250,645 | 5% | 2,053 | 2,021,584 | applied |
| 15 | Tick B: a record 30 days after the freeze's start | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,537 | 76,899 | 2% | 505 | 619,232 | applied |
| 16 | Claim B's 1 ETH: the freeze has lapsed | bridge Claim 107,610 → bridge Issue 48,456 → bridge Admit 33,983 → bridge Spend 22,171 → bridge Mint 19,837 → bridge Mint 19,837 | 251,894 | 5% | 2,053 | 2,031,576 | applied |
| 17 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 13,539 → bridge SetCap 42,364 | 55,903 | 1% | 725 | 453,024 | applied |
| 18 | Tick ETH (A): the newest record (30 days on) | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,738 | 94,980 | 2% | 525 | 764,040 | applied |
| 19 | Freeze A's burns for one hour | bridge Freeze 12,680 → bridge Freeze 36,403 → Read 11,877 → bridge (reply) 32,564 | 93,524 | 2% | 755 | 754,232 | applied |
| 20 | A year of LEZ blocks later, no newer record: Tick A | bridge Tick 32,485 → Read 11,877 → bridge (reply) 32,538 | 76,900 | 2% | 505 | 619,240 | applied |
| 21 | Burn 1 ETH (A): A's burns still frozen | bridge Burn 26,722 → bridge Burn 54,329 → bridge Admit 27,200 | 108,251 | 2% | 775 | 872,208 | refused: exit 242 (FROZEN) |
| 22 | Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze 12,680 → bridge Freeze 36,403 → Read 11,877 → bridge (reply) 32,551 | 93,511 | 2% | 755 | 754,128 | applied |
| 23 | Burn 1 ETH (A) | bridge Burn 26,722 → bridge Burn 54,329 → bridge Admit 33,982 → bridge Burn 21,107 | 136,140 | 3% | 775 | 1,095,320 | applied |
| 24 | Tick ETH (A), still no newer record | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,739 | 94,981 | 2% | 525 | 764,048 | applied |
| 25 | Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn 26,722 → bridge Burn 34,388 | 61,110 | 1% | 775 | 495,080 | refused: exit 255 (CAPPED) |
| 26 | SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap 13,539 → bridge SetCap 42,367 | 55,906 | 1% | 725 | 453,048 | applied |
| 27 | Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn 26,722 → bridge Burn 34,388 | 61,110 | 1% | 775 | 495,080 | refused: exit 255 (CAPPED) |
| 28 | Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,738 | 10,000,000 | 200% | 525 | 80,004,200 | reverted: Execution outside of the validity window |
| 29 | The same Tick in a block its producer stamps an hour ahead too | bridge Tick 42,365 → Read 11,877 → bridge (reply) 40,738 | 94,980 | 2% | 525 | 764,040 | applied |
| 30 | Tick A: a Gloas record without its block's header | bridge Tick 32,485 → Read 10,921 → bridge (reply) 23,718 | 67,124 | 1% | 505 | 541,032 | refused: exit 247 (IMPORT_MISMATCH) |
| 31 | Tick A: a Gloas record with a header that does not hash to it | bridge Tick 40,372 → Read 10,921 → bridge (reply) 129,894 | 181,187 | 4% | 1,196 | 1,459,064 | refused: exit 247 (IMPORT_MISMATCH) |
| 32 | Tick A: a Gloas record with its block's header | bridge Tick 40,372 → Read 10,921 → bridge (reply) 142,904 | 194,197 | 4% | 1,196 | 1,563,144 | applied |
| 33 | Tick A: a payload record with a header | bridge Tick 40,372 → Read 11,877 → bridge (reply) 26,530 | 78,779 | 2% | 1,196 | 639,800 | refused: exit 247 (IMPORT_MISMATCH) |
| 34 | Tick A: a slot with no record | bridge Tick 32,485 → Read 8,844 → bridge (reply) 20,366 | 61,695 | 1% | 570 | 498,120 | refused: exit 245 (NOT_FOUND) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Tick A: the newest record | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: the newest record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Tick ETH (A): the newest record | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the newest record | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick B: the newest record | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: the newest record | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Freeze B's mints for one hour | bridge Freeze | 12,680 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for one hour | bridge Freeze | 36,403 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze B's mints for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze B's mints for one hour | bridge (reply) | 32,565 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| A block stamped a year ahead: Tick B with the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge (reply) | 32,538 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Issue | 48,456 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Admit | 27,200 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 12,680 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 36,403 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge (reply) | 32,566 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 13,538 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,364 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Issue | 52,747 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Admit | 33,982 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Spend | 24,414 | 1 | 408 | 270 | 0 | 1,113 | 77 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge (reply) | 40,739 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with an older record (block 100's) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge (reply) | 40,739 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Issue | 29,965 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): a record 10 s later | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 10 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): a record 10 s later | bridge (reply) | 40,737 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Issue | 52,712 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 14,616 | 1 | 267 | 129 | 0 | 366 | 74 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 19,553 | 1 | 341 | 129 | 74 | 440 | 74 | 0 |
| Tick B: a record 30 days after the freeze's start | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick B: a record 30 days after the freeze's start | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: a record 30 days after the freeze's start | bridge (reply) | 32,537 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Claim | 107,610 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Issue | 48,456 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Admit | 33,983 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Spend | 22,171 | 1 | 360 | 222 | 0 | 864 | 77 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 19,837 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 19,837 | 1 | 341 | 129 | 74 | 508 | 142 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 13,539 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,364 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record (30 days on) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Freeze A's burns for one hour | bridge Freeze | 12,680 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze A's burns for one hour | bridge Freeze | 36,403 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Freeze A's burns for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze A's burns for one hour | bridge (reply) | 32,564 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge (reply) | 32,538 | 1 | 1,006 | 337 | 531 | 1,559 | 520 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 54,329 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Admit | 27,200 | 1 | 728 | 70 | 520 | 0 | – | 242 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 12,680 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 36,403 | 1 | 711 | 53 | 520 | 1,350 | 541 | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge (reply) | 32,551 | 1 | 1,016 | 337 | 541 | 1,569 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 54,329 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 33,982 | 1 | 728 | 70 | 520 | 1,273 | 520 | 0 |
| Burn 1 ETH (A) | bridge Burn | 21,107 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A), still no newer record | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A), still no newer record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A), still no newer record | bridge (reply) | 40,739 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 34,388 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 13,539 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 42,367 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 26,722 | 1 | 401 | 185 | 142 | 825 | 142 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 34,388 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge Tick | 42,365 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge (reply) | 40,738 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick A: a Gloas record without its block's header | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: a Gloas record without its block's header | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record without its block's header | bridge (reply) | 23,718 | 1 | 894 | 225 | 531 | 0 | – | 247 |
| Tick A: a Gloas record with a header that does not hash to it | bridge Tick | 40,372 | 1 | 1,328 | 734 | 520 | 2,648 | 1,222 | 0 |
| Tick A: a Gloas record with a header that does not hash to it | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record with a header that does not hash to it | bridge (reply) | 129,894 | 1 | 1,585 | 225 | 1,222 | 0 | – | 247 |
| Tick A: a Gloas record with its block's header | bridge Tick | 40,372 | 1 | 1,328 | 734 | 520 | 2,648 | 1,222 | 0 |
| Tick A: a Gloas record with its block's header | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record with its block's header | bridge (reply) | 142,904 | 1 | 1,585 | 225 | 1,222 | 2,138 | 520 | 0 |
| Tick A: a payload record with a header | bridge Tick | 40,372 | 1 | 1,328 | 734 | 520 | 2,648 | 1,222 | 0 |
| Tick A: a payload record with a header | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: a payload record with a header | bridge (reply) | 26,530 | 1 | 1,697 | 337 | 1,222 | 0 | – | 247 |
| Tick A: a slot with no record | bridge Tick | 32,485 | 1 | 637 | 43 | 520 | 1,266 | 531 | 0 |
| Tick A: a slot with no record | Read | 8,844 | 1 | 139 | 1 | 0 | 236 | – | 0 |
| Tick A: a slot with no record | bridge (reply) | 20,366 | 1 | 673 | 4 | 531 | 0 | – | 245 |

