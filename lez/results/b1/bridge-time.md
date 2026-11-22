# The bridge program's clock: attested Ethereum time (`harness/tests/bridge_time.rs`)

Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`; every transaction settled by LEZ's own functions in a block of its own, stamped with the world's LEZ time (a year ahead where the label says so).

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Tick A: the newest record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 2 | Tick ETH (A): the newest record | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,965 | 95,720 | 2% | 525 | 769,960 | applied |
| 3 | Tick B: the newest record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 4 | Freeze B's mints for one hour | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,416 | 97,147 | 2% | 755 | 783,216 | applied |
| 5 | A block stamped a year ahead: Tick B with the newest record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,218 | 81,442 | 2% | 505 | 655,576 | applied |
| 6 | Claim B's 1 ETH in that block: the freeze holds | bridge Claim 107,796 → bridge Issue 48,645 → bridge Admit 28,709 | 185,150 | 4% | 2,053 | 1,497,624 | refused: exit 242 (FROZEN) |
| 7 | Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,417 | 97,148 | 2% | 755 | 783,224 | applied |
| 8 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 16,352 → bridge SetCap 32,953 → bridge SetCap 42,527 | 91,832 | 2% | 789 | 740,968 | applied |
| 9 | Claim leaf 6 (10.06 ETH of the 11) | bridge Claim 107,796 → bridge Issue 52,815 → bridge Admit 35,961 → bridge Spend 38,287 → bridge Mint 18,800 → bridge Mint 18,800 → bridge Mint 18,800 | 291,259 | 6% | 2,053 | 2,346,496 | applied |
| 10 | A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,966 | 95,721 | 2% | 525 | 769,968 | applied |
| 11 | Tick ETH (A) with an older record (block 100's) | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,966 | 95,721 | 2% | 525 | 769,968 | applied |
| 12 | Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim 107,796 → bridge Issue 29,166 | 136,962 | 3% | 2,053 | 1,112,120 | refused: exit 255 (CAPPED) |
| 13 | Tick ETH (A): a record 10 s later | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,964 | 95,719 | 2% | 525 | 769,952 | applied |
| 14 | Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim 107,796 → bridge Issue 52,950 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 23,177 | 270,454 | 5% | 2,053 | 2,180,056 | applied |
| 15 | Tick B: a record 30 days after the freeze's start | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 16 | Claim B's 1 ETH: the freeze has lapsed | bridge Claim 107,796 → bridge Issue 48,645 → bridge Admit 35,962 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 261,772 | 5% | 2,053 | 2,110,600 | applied |
| 17 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 16,353 → bridge SetCap 32,954 → bridge SetCap 42,529 | 91,836 | 2% | 789 | 741,000 | applied |
| 18 | Tick ETH (A): the newest record (30 days on) | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,965 | 95,720 | 2% | 525 | 769,960 | applied |
| 19 | Freeze A's burns for one hour | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,414 | 97,145 | 2% | 755 | 783,200 | applied |
| 20 | A year of LEZ blocks later, no newer record: Tick A | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,218 | 81,442 | 2% | 505 | 655,576 | applied |
| 21 | Burn 1 ETH (A): A's burns still frozen | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,546 → bridge Admit 28,710 | 130,219 | 3% | 839 | 1,048,464 | refused: exit 242 (FROZEN) |
| 22 | Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,404 | 97,135 | 2% | 755 | 783,120 | applied |
| 23 | Burn 1 ETH (A) | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,546 → bridge Admit 35,962 → bridge Burn 19,498 | 156,969 | 3% | 839 | 1,262,464 | applied |
| 24 | Tick ETH (A), still no newer record | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,966 | 95,721 | 2% | 525 | 769,968 | applied |
| 25 | Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 31,161 | 82,124 | 2% | 839 | 663,704 | refused: exit 255 (CAPPED) |
| 26 | SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap 16,353 → bridge SetCap 32,954 → bridge SetCap 42,530 | 91,837 | 2% | 789 | 741,008 | applied |
| 27 | Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 31,161 | 82,124 | 2% | 839 | 663,704 | refused: exit 255 (CAPPED) |
| 28 | Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,965 | 10,000,000 | 200% | 525 | 80,004,200 | reverted: Execution outside of the validity window |
| 29 | The same Tick in a block its producer stamps an hour ahead too | bridge Tick 42,487 → Read 12,268 → bridge (reply) 40,965 | 95,720 | 2% | 525 | 769,960 | applied |
| 30 | Tick A: a Gloas record without its block's header | bridge Tick 34,956 → Read 11,329 → bridge (reply) 25,154 | 71,439 | 1% | 505 | 575,552 | refused: exit 247 (IMPORT_MISMATCH) |
| 31 | Tick A: a Gloas record with a header that does not hash to it | bridge Tick 42,838 → Read 11,329 → bridge (reply) 131,322 | 185,489 | 4% | 1,196 | 1,493,480 | refused: exit 247 (IMPORT_MISMATCH) |
| 32 | Tick A: a Gloas record with its block's header | bridge Tick 42,838 → Read 11,329 → bridge (reply) 144,525 | 198,692 | 4% | 1,196 | 1,599,104 | applied |
| 33 | Tick A: a payload record with a header | bridge Tick 42,838 → Read 12,268 → bridge (reply) 27,958 | 83,064 | 2% | 1,196 | 674,080 | refused: exit 247 (IMPORT_MISMATCH) |
| 34 | Tick A: a slot with no record | bridge Tick 34,956 → Read 8,973 → bridge (reply) 21,294 | 65,223 | 1% | 570 | 526,344 | refused: exit 245 (NOT_FOUND) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Tick A: the newest record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: the newest record | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Tick ETH (A): the newest record | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A): the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the newest record | bridge (reply) | 40,965 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Tick B: the newest record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick B: the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick B: the newest record | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Freeze B's mints for one hour | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for one hour | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze B's mints for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze B's mints for one hour | bridge (reply) | 34,416 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| A block stamped a year ahead: Tick B with the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge (reply) | 34,218 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Issue | 48,645 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Admit | 28,709 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge (reply) | 34,417 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 32,953 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,527 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Issue | 52,815 | 1 | 1,377 | 199 | 1,040 | 2,958 | 1,040 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Spend | 38,287 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge (reply) | 40,966 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A) with an older record (block 100's) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge (reply) | 40,966 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Issue | 29,166 | 1 | 1,377 | 199 | 1,040 | 0 | – | 255 |
| Tick ETH (A): a record 10 s later | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A): a record 10 s later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record 10 s later | bridge (reply) | 40,964 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Issue | 52,950 | 1 | 1,377 | 199 | 1,040 | 2,910 | 1,040 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Tick B: a record 30 days after the freeze's start | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick B: a record 30 days after the freeze's start | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick B: a record 30 days after the freeze's start | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Issue | 48,645 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 16,353 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 32,954 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,529 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A): the newest record (30 days on) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge (reply) | 40,965 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Freeze A's burns for one hour | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze A's burns for one hour | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze A's burns for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze A's burns for one hour | bridge (reply) | 34,414 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge (reply) | 34,218 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 50,546 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Admit | 28,710 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge (reply) | 34,404 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| Burn 1 ETH (A) | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) | bridge Burn | 50,546 | 1 | 1,363 | 185 | 1,040 | 2,859 | 1,040 | 0 |
| Burn 1 ETH (A) | bridge Admit | 35,962 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Burn 1 ETH (A) | bridge Burn | 19,498 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A), still no newer record | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A), still no newer record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A), still no newer record | bridge (reply) | 40,966 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 31,161 | 1 | 1,363 | 185 | 1,040 | 0 | – | 255 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 16,353 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 32,954 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 42,530 | 1 | 1,265 | 87 | 1,040 | 2,330 | 1,040 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 22,693 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 28,270 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 31,161 | 1 | 1,363 | 185 | 1,040 | 0 | – | 255 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge (reply) | 40,965 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge Tick | 42,487 | 1 | 1,177 | 63 | 1,040 | 2,325 | 1,050 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge (reply) | 40,965 | 1 | 1,557 | 369 | 1,050 | 2,630 | 1,040 | 0 |
| Tick A: a Gloas record without its block's header | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: a Gloas record without its block's header | Read | 11,329 | 1 | 392 | 1 | 253 | 742 | – | 0 |
| Tick A: a Gloas record without its block's header | bridge (reply) | 25,154 | 1 | 986 | 257 | 591 | 0 | – | 247 |
| Tick A: a Gloas record with a header that does not hash to it | bridge Tick | 42,838 | 1 | 1,388 | 734 | 580 | 2,768 | 1,282 | 0 |
| Tick A: a Gloas record with a header that does not hash to it | Read | 11,329 | 1 | 392 | 1 | 253 | 742 | – | 0 |
| Tick A: a Gloas record with a header that does not hash to it | bridge (reply) | 131,322 | 1 | 1,677 | 257 | 1,282 | 0 | – | 247 |
| Tick A: a Gloas record with its block's header | bridge Tick | 42,838 | 1 | 1,388 | 734 | 580 | 2,768 | 1,282 | 0 |
| Tick A: a Gloas record with its block's header | Read | 11,329 | 1 | 392 | 1 | 253 | 742 | – | 0 |
| Tick A: a Gloas record with its block's header | bridge (reply) | 144,525 | 1 | 1,677 | 257 | 1,282 | 2,290 | 580 | 0 |
| Tick A: a payload record with a header | bridge Tick | 42,838 | 1 | 1,388 | 734 | 580 | 2,768 | 1,282 | 0 |
| Tick A: a payload record with a header | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: a payload record with a header | bridge (reply) | 27,958 | 1 | 1,789 | 369 | 1,282 | 0 | – | 247 |
| Tick A: a slot with no record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: a slot with no record | Read | 8,973 | 1 | 139 | 1 | 0 | 236 | – | 0 |
| Tick A: a slot with no record | bridge (reply) | 21,294 | 1 | 733 | 4 | 591 | 0 | – | 245 |

