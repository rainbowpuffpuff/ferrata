# The bridge program's clock: attested Ethereum time (`harness/tests/bridge_time.rs`)

Bridge program image `c5cad2cd470807a1799613a1f5f699f9d4f4074821a9fb26389ca917a4d0d6da`; every transaction settled by LEZ's own functions in a block of its own, stamped with the world's LEZ time (a year ahead where the label says so).

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Tick A: the newest record | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,846 | 80,628 | 2% | 505 | 649,064 | applied |
| 2 | Tick ETH (A): the newest record | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,153 | 95,555 | 2% | 525 | 768,640 | applied |
| 3 | Tick B: the newest record | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,846 | 80,628 | 2% | 505 | 649,064 | applied |
| 4 | Freeze B's mints for one hour | bridge Freeze 13,251 → bridge Freeze 36,541 → Read 12,268 → bridge (reply) 33,880 | 95,940 | 2% | 755 | 773,560 | applied |
| 5 | A block stamped a year ahead: Tick B with the newest record | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,847 | 80,629 | 2% | 505 | 649,072 | applied |
| 6 | Claim B's 1 ETH in that block: the freeze holds | bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 28,473 | 184,516 | 4% | 2,053 | 1,492,552 | refused: exit 242 (FROZEN) |
| 7 | Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze 13,251 → bridge Freeze 36,541 → Read 12,268 → bridge (reply) 33,881 | 95,941 | 2% | 755 | 773,568 | applied |
| 8 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 16,279 → bridge SetCap 32,683 → bridge SetCap 42,056 | 91,018 | 2% | 789 | 734,456 | applied |
| 9 | Claim leaf 6 (10.06 ETH of the 11) | bridge Claim 107,702 → bridge Issue 52,633 → bridge Admit 35,564 → bridge Spend 38,243 → bridge Mint 18,761 → bridge Mint 18,761 → bridge Mint 18,761 | 290,425 | 6% | 2,053 | 2,339,824 | applied |
| 10 | A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,154 | 95,556 | 2% | 525 | 768,648 | applied |
| 11 | Tick ETH (A) with an older record (block 100's) | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,154 | 95,556 | 2% | 525 | 768,648 | applied |
| 12 | Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim 107,702 → bridge Issue 28,947 | 136,649 | 3% | 2,053 | 1,109,616 | refused: exit 255 (CAPPED) |
| 13 | Tick ETH (A): a record 10 s later | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,152 | 95,554 | 2% | 525 | 768,632 | applied |
| 14 | Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim 107,702 → bridge Issue 52,597 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 23,096 | 269,446 | 5% | 2,053 | 2,171,992 | applied |
| 15 | Tick B: a record 30 days after the freeze's start | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,846 | 80,628 | 2% | 505 | 649,064 | applied |
| 16 | Claim B's 1 ETH: the freeze has lapsed | bridge Claim 107,702 → bridge Issue 48,341 → bridge Admit 35,565 → bridge Spend 31,725 → bridge Mint 18,761 → bridge Mint 18,761 | 260,855 | 5% | 2,053 | 2,103,264 | applied |
| 17 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 16,280 → bridge SetCap 32,684 → bridge SetCap 42,058 | 91,022 | 2% | 789 | 734,488 | applied |
| 18 | Tick ETH (A): the newest record (30 days on) | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,153 | 95,555 | 2% | 525 | 768,640 | applied |
| 19 | Freeze A's burns for one hour | bridge Freeze 13,251 → bridge Freeze 36,541 → Read 12,268 → bridge (reply) 33,880 | 95,940 | 2% | 755 | 773,560 | applied |
| 20 | A year of LEZ blocks later, no newer record: Tick A | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,847 | 80,629 | 2% | 505 | 649,072 | applied |
| 21 | Burn 1 ETH (A): A's burns still frozen | bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 53,028 → bridge Admit 28,474 | 136,643 | 3% | 839 | 1,099,856 | refused: exit 242 (FROZEN) |
| 22 | Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze 13,251 → bridge Freeze 36,541 → Read 12,268 → bridge (reply) 33,868 | 95,928 | 2% | 755 | 773,464 | applied |
| 23 | Burn 1 ETH (A) | bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 53,028 → bridge Admit 35,565 → bridge Burn 21,612 | 165,346 | 3% | 839 | 1,329,480 | applied |
| 24 | Tick ETH (A), still no newer record | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,154 | 95,556 | 2% | 525 | 768,648 | applied |
| 25 | Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,129 | 88,270 | 2% | 839 | 712,872 | refused: exit 255 (CAPPED) |
| 26 | SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap 16,280 → bridge SetCap 32,684 → bridge SetCap 42,059 | 91,023 | 2% | 789 | 734,496 | applied |
| 27 | Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn 24,804 → bridge Burn 30,337 → bridge Burn 33,129 | 88,270 | 2% | 839 | 712,872 | refused: exit 255 (CAPPED) |
| 28 | Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,153 | 10,000,000 | 200% | 525 | 80,004,200 | reverted: Execution outside of the validity window |
| 29 | The same Tick in a block its producer stamps an hour ahead too | bridge Tick 42,134 → Read 12,268 → bridge (reply) 41,153 | 95,555 | 2% | 525 | 768,640 | applied |
| 30 | Tick A: a Gloas record without its block's header | bridge Tick 34,514 → Read 11,329 → bridge (reply) 24,964 | 70,807 | 1% | 505 | 570,496 | refused: exit 247 (IMPORT_MISMATCH) |
| 31 | Tick A: a Gloas record with a header that does not hash to it | bridge Tick 42,351 → Read 11,329 → bridge (reply) 131,132 | 184,812 | 4% | 1,196 | 1,488,064 | refused: exit 247 (IMPORT_MISMATCH) |
| 32 | Tick A: a Gloas record with its block's header | bridge Tick 42,351 → Read 11,329 → bridge (reply) 144,157 | 197,837 | 4% | 1,196 | 1,592,264 | applied |
| 33 | Tick A: a payload record with a header | bridge Tick 42,351 → Read 12,268 → bridge (reply) 27,768 | 82,387 | 2% | 1,196 | 668,664 | refused: exit 247 (IMPORT_MISMATCH) |
| 34 | Tick A: a slot with no record | bridge Tick 34,514 → Read 8,973 → bridge (reply) 21,104 | 64,591 | 1% | 570 | 521,288 | refused: exit 245 (NOT_FOUND) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Tick A: the newest record | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: the newest record | bridge (reply) | 33,846 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Tick ETH (A): the newest record | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the newest record | bridge (reply) | 41,153 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Tick B: the newest record | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick B: the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick B: the newest record | bridge (reply) | 33,846 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Freeze B's mints for one hour | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for one hour | bridge Freeze | 36,541 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze B's mints for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze B's mints for one hour | bridge (reply) | 33,880 | 1 | 1,092 | 369 | 585 | 1,689 | 564 | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| A block stamped a year ahead: Tick B with the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge (reply) | 33,847 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Issue | 48,341 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Admit | 28,473 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 36,541 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge (reply) | 33,881 | 1 | 1,092 | 369 | 585 | 1,689 | 564 | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 32,683 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,056 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Issue | 52,633 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Spend | 38,243 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge (reply) | 41,154 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with an older record (block 100's) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge (reply) | 41,154 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Issue | 28,947 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): a record 10 s later | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 10 s later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record 10 s later | bridge (reply) | 41,152 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Issue | 52,597 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Spend | 31,725 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 23,096 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Tick B: a record 30 days after the freeze's start | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick B: a record 30 days after the freeze's start | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick B: a record 30 days after the freeze's start | bridge (reply) | 33,846 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Issue | 48,341 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Spend | 31,725 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 18,761 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 16,280 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 32,684 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,058 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record (30 days on) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge (reply) | 41,153 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Freeze A's burns for one hour | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze A's burns for one hour | bridge Freeze | 36,541 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze A's burns for one hour | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze A's burns for one hour | bridge (reply) | 33,880 | 1 | 1,092 | 369 | 585 | 1,689 | 564 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge (reply) | 33,847 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 24,804 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 30,337 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 53,028 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Admit | 28,474 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 36,541 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge (reply) | 33,868 | 1 | 1,092 | 369 | 585 | 1,689 | 564 | 0 |
| Burn 1 ETH (A) | bridge Burn | 24,804 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 30,337 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A) | bridge Burn | 53,028 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (A) | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A), still no newer record | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A), still no newer record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A), still no newer record | bridge (reply) | 41,154 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 24,804 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 30,337 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 33,129 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 16,280 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 32,684 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 42,059 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 24,804 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 30,337 | 1 | 397 | 185 | 74 | 753 | 74 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 33,129 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge (reply) | 41,153 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge Tick | 42,134 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge (reply) | 41,153 | 1 | 1,548 | 369 | 1,041 | 2,612 | 1,031 | 0 |
| Tick A: a Gloas record without its block's header | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: a Gloas record without its block's header | Read | 11,329 | 1 | 392 | 1 | 253 | 742 | – | 0 |
| Tick A: a Gloas record without its block's header | bridge (reply) | 24,964 | 1 | 970 | 257 | 575 | 0 | – | 247 |
| Tick A: a Gloas record with a header that does not hash to it | bridge Tick | 42,351 | 1 | 1,372 | 734 | 564 | 2,736 | 1,266 | 0 |
| Tick A: a Gloas record with a header that does not hash to it | Read | 11,329 | 1 | 392 | 1 | 253 | 742 | – | 0 |
| Tick A: a Gloas record with a header that does not hash to it | bridge (reply) | 131,132 | 1 | 1,661 | 257 | 1,266 | 0 | – | 247 |
| Tick A: a Gloas record with its block's header | bridge Tick | 42,351 | 1 | 1,372 | 734 | 564 | 2,736 | 1,266 | 0 |
| Tick A: a Gloas record with its block's header | Read | 11,329 | 1 | 392 | 1 | 253 | 742 | – | 0 |
| Tick A: a Gloas record with its block's header | bridge (reply) | 144,157 | 1 | 1,661 | 257 | 1,266 | 2,258 | 564 | 0 |
| Tick A: a payload record with a header | bridge Tick | 42,351 | 1 | 1,372 | 734 | 564 | 2,736 | 1,266 | 0 |
| Tick A: a payload record with a header | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: a payload record with a header | bridge (reply) | 27,768 | 1 | 1,773 | 369 | 1,266 | 0 | – | 247 |
| Tick A: a slot with no record | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: a slot with no record | Read | 8,973 | 1 | 139 | 1 | 0 | 236 | – | 0 |
| Tick A: a slot with no record | bridge (reply) | 21,104 | 1 | 717 | 4 | 575 | 0 | – | 245 |

