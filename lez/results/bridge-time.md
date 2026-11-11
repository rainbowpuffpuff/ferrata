# The bridge program's clock: attested Ethereum time (`harness/tests/bridge_time.rs`)

Bridge program image `43615dc48098b3731990de7700204d2a1bea0e9f963ee7c4dd629048dd5ef19c`; every transaction settled by LEZ's own functions in a block of its own, stamped with the world's LEZ time (a year ahead where the label says so).

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Tick A: the newest record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 2 | Tick ETH (A): the newest record | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,593 | 94,605 | 2% | 525 | 761,040 | applied |
| 3 | Tick B: the newest record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 4 | Freeze B's mints for one hour | bridge Freeze 13,251 → bridge Freeze 36,542 → Read 11,877 → bridge (reply) 33,433 | 95,103 | 2% | 755 | 766,864 | applied |
| 5 | A block stamped a year ahead: Tick B with the newest record | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,236 | 79,628 | 2% | 505 | 641,064 | applied |
| 6 | Claim B's 1 ETH in that block: the freeze holds | bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 28,473 | 183,468 | 4% | 2,053 | 1,484,168 | refused: exit 242 (FROZEN) |
| 7 | Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze 13,251 → bridge Freeze 36,542 → Read 11,877 → bridge (reply) 33,434 | 95,104 | 2% | 755 | 766,872 | applied |
| 8 | SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap 16,279 → bridge SetCap 32,683 → bridge SetCap 42,056 | 91,018 | 2% | 789 | 734,456 | applied |
| 9 | Claim leaf 6 (10.06 ETH of the 11) | bridge Claim 107,702 → bridge Issue 51,585 → bridge Admit 35,564 → bridge Spend 37,291 → bridge Mint 18,161 → bridge Mint 18,161 → bridge Mint 18,161 | 286,625 | 6% | 2,053 | 2,309,424 | applied |
| 10 | A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,594 | 94,606 | 2% | 525 | 761,048 | applied |
| 11 | Tick ETH (A) with an older record (block 100's) | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,594 | 94,606 | 2% | 525 | 761,048 | applied |
| 12 | Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim 107,702 → bridge Issue 28,947 | 136,649 | 3% | 2,053 | 1,109,616 | refused: exit 255 (CAPPED) |
| 13 | Tick ETH (A): a record 10 s later | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,592 | 94,604 | 2% | 525 | 761,032 | applied |
| 14 | Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim 107,702 → bridge Issue 51,549 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 22,496 | 266,389 | 5% | 2,053 | 2,147,536 | applied |
| 15 | Tick B: a record 30 days after the freeze's start | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,235 | 79,627 | 2% | 505 | 641,056 | applied |
| 16 | Claim B's 1 ETH: the freeze has lapsed | bridge Claim 107,702 → bridge Issue 47,293 → bridge Admit 35,565 → bridge Spend 30,916 → bridge Mint 22,784 → bridge Mint 22,784 | 267,044 | 5% | 2,053 | 2,152,776 | applied |
| 17 | SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap 16,280 → bridge SetCap 32,684 → bridge SetCap 42,058 | 91,022 | 2% | 789 | 734,488 | applied |
| 18 | Tick ETH (A): the newest record (30 days on) | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,593 | 94,605 | 2% | 525 | 761,040 | applied |
| 19 | Freeze A's burns for one hour | bridge Freeze 13,251 → bridge Freeze 36,542 → Read 11,877 → bridge (reply) 33,433 | 95,103 | 2% | 755 | 766,864 | applied |
| 20 | A year of LEZ blocks later, no newer record: Tick A | bridge Tick 34,515 → Read 11,877 → bridge (reply) 33,236 | 79,628 | 2% | 505 | 641,064 | applied |
| 21 | Burn 1 ETH (A): A's burns still frozen | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 53,028 → bridge Admit 28,474 | 138,065 | 3% | 839 | 1,111,232 | refused: exit 242 (FROZEN) |
| 22 | Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze 13,251 → bridge Freeze 36,542 → Read 11,877 → bridge (reply) 33,421 | 95,091 | 2% | 755 | 766,768 | applied |
| 23 | Burn 1 ETH (A) | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 53,028 → bridge Admit 35,565 → bridge Burn 21,612 | 166,768 | 3% | 839 | 1,340,856 | applied |
| 24 | Tick ETH (A), still no newer record | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,594 | 94,606 | 2% | 525 | 761,048 | applied |
| 25 | Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129 | 89,692 | 2% | 839 | 724,248 | refused: exit 255 (CAPPED) |
| 26 | SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap 16,280 → bridge SetCap 32,684 → bridge SetCap 42,059 | 91,023 | 2% | 789 | 734,496 | applied |
| 27 | Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn 24,662 → bridge Burn 31,901 → bridge Burn 33,129 | 89,692 | 2% | 839 | 724,248 | refused: exit 255 (CAPPED) |
| 28 | Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,593 | 10,000,000 | 200% | 525 | 80,004,200 | reverted: Execution outside of the validity window |
| 29 | The same Tick in a block its producer stamps an hour ahead too | bridge Tick 42,135 → Read 11,877 → bridge (reply) 40,593 | 94,605 | 2% | 525 | 761,040 | applied |
| 30 | Tick A: a Gloas record without its block's header | bridge Tick 34,515 → Read 10,921 → bridge (reply) 24,456 | 69,892 | 1% | 505 | 563,176 | refused: exit 247 (IMPORT_MISMATCH) |
| 31 | Tick A: a Gloas record with a header that does not hash to it | bridge Tick 42,352 → Read 10,921 → bridge (reply) 130,648 | 183,921 | 4% | 1,196 | 1,480,936 | refused: exit 247 (IMPORT_MISMATCH) |
| 32 | Tick A: a Gloas record with its block's header | bridge Tick 42,352 → Read 10,921 → bridge (reply) 143,591 | 196,864 | 4% | 1,196 | 1,584,480 | applied |
| 33 | Tick A: a payload record with a header | bridge Tick 42,352 → Read 11,877 → bridge (reply) 27,286 | 81,515 | 2% | 1,196 | 661,688 | refused: exit 247 (IMPORT_MISMATCH) |
| 34 | Tick A: a slot with no record | bridge Tick 34,515 → Read 8,844 → bridge (reply) 21,104 | 64,463 | 1% | 570 | 520,264 | refused: exit 245 (NOT_FOUND) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Tick A: the newest record | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: the newest record | bridge (reply) | 33,235 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Tick ETH (A): the newest record | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the newest record | bridge (reply) | 40,593 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick B: the newest record | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick B: the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: the newest record | bridge (reply) | 33,235 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Freeze B's mints for one hour | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for one hour | bridge Freeze | 36,542 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze B's mints for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze B's mints for one hour | bridge (reply) | 33,433 | 1 | 1,060 | 337 | 585 | 1,657 | 564 | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| A block stamped a year ahead: Tick B with the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A block stamped a year ahead: Tick B with the newest record | bridge (reply) | 33,236 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Issue | 47,293 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH in that block: the freeze holds | bridge Admit | 28,473 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge Freeze | 36,542 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze B's mints for 30 days, in that block: accepted (bounded by attested time) | bridge (reply) | 33,434 | 1 | 1,060 | 337 | 585 | 1,657 | 564 | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 32,683 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s | bridge SetCap | 42,056 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Issue | 51,585 | 1 | 1,368 | 199 | 1,031 | 2,940 | 1,031 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Spend | 37,291 | 1 | 408 | 270 | 0 | 1,305 | 77 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 6 (10.06 ETH of the 11) | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A block stamped a year ahead: Tick ETH (A) with the newest record | bridge (reply) | 40,594 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with an older record (block 100's) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A) with an older record (block 100's) | bridge (reply) | 40,594 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled | bridge Issue | 28,947 | 1 | 1,368 | 199 | 1,031 | 0 | – | 255 |
| Tick ETH (A): a record 10 s later | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): a record 10 s later | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): a record 10 s later | bridge (reply) | 40,592 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Issue | 51,549 | 1 | 1,368 | 199 | 1,031 | 2,892 | 1,031 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim leaf 2 after it: 0.1 ETH refilled | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Tick B: a record 30 days after the freeze's start | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick B: a record 30 days after the freeze's start | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick B: a record 30 days after the freeze's start | bridge (reply) | 33,235 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Issue | 47,293 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 22,784 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim B's 1 ETH: the freeze has lapsed | bridge Mint | 22,784 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 16,280 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 32,684 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s | bridge SetCap | 42,058 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A): the newest record (30 days on) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A): the newest record (30 days on) | bridge (reply) | 40,593 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Freeze A's burns for one hour | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze A's burns for one hour | bridge Freeze | 36,542 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze A's burns for one hour | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Freeze A's burns for one hour | bridge (reply) | 33,433 | 1 | 1,060 | 337 | 585 | 1,657 | 564 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| A year of LEZ blocks later, no newer record: Tick A | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| A year of LEZ blocks later, no newer record: Tick A | bridge (reply) | 33,236 | 1 | 1,050 | 337 | 575 | 1,647 | 564 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Burn | 53,028 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A): A's burns still frozen | bridge Admit | 28,474 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge Freeze | 36,542 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Unfreeze A's burns (freeze authority), still no newer record | bridge (reply) | 33,421 | 1 | 1,060 | 337 | 585 | 1,657 | 564 | 0 |
| Burn 1 ETH (A) | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A) | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A) | bridge Burn | 53,028 | 1 | 1,354 | 185 | 1,031 | 2,841 | 1,031 | 0 |
| Burn 1 ETH (A) | bridge Admit | 35,565 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Burn 1 ETH (A) | bridge Burn | 21,612 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Tick ETH (A), still no newer record | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A), still no newer record | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A), still no newer record | bridge (reply) | 40,594 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A): its burn bucket has not refilled | bridge Burn | 33,129 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 16,280 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 32,684 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) burns 100 ETH, still no newer record | bridge SetCap | 42,059 | 1 | 1,256 | 87 | 1,031 | 2,312 | 1,031 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 24,662 | 1 | 259 | 185 | 0 | 569 | – | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 31,901 | 1 | 465 | 185 | 142 | 889 | 142 | 0 |
| Burn 1 ETH (A): raising the capacity refills nothing | bridge Burn | 33,129 | 1 | 1,354 | 185 | 1,031 | 0 | – | 255 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick ETH (A) with a record an hour ahead of the block (a captured committee's) | bridge (reply) | 40,593 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge Tick | 42,135 | 1 | 1,168 | 63 | 1,031 | 2,307 | 1,041 | 0 |
| The same Tick in a block its producer stamps an hour ahead too | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| The same Tick in a block its producer stamps an hour ahead too | bridge (reply) | 40,593 | 1 | 1,516 | 337 | 1,041 | 2,580 | 1,031 | 0 |
| Tick A: a Gloas record without its block's header | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: a Gloas record without its block's header | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record without its block's header | bridge (reply) | 24,456 | 1 | 938 | 225 | 575 | 0 | – | 247 |
| Tick A: a Gloas record with a header that does not hash to it | bridge Tick | 42,352 | 1 | 1,372 | 734 | 564 | 2,736 | 1,266 | 0 |
| Tick A: a Gloas record with a header that does not hash to it | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record with a header that does not hash to it | bridge (reply) | 130,648 | 1 | 1,629 | 225 | 1,266 | 0 | – | 247 |
| Tick A: a Gloas record with its block's header | bridge Tick | 42,352 | 1 | 1,372 | 734 | 564 | 2,736 | 1,266 | 0 |
| Tick A: a Gloas record with its block's header | Read | 10,921 | 1 | 360 | 1 | 221 | 678 | – | 0 |
| Tick A: a Gloas record with its block's header | bridge (reply) | 143,591 | 1 | 1,629 | 225 | 1,266 | 2,226 | 564 | 0 |
| Tick A: a payload record with a header | bridge Tick | 42,352 | 1 | 1,372 | 734 | 564 | 2,736 | 1,266 | 0 |
| Tick A: a payload record with a header | Read | 11,877 | 1 | 472 | 1 | 333 | 902 | – | 0 |
| Tick A: a payload record with a header | bridge (reply) | 27,286 | 1 | 1,741 | 337 | 1,266 | 0 | – | 247 |
| Tick A: a slot with no record | bridge Tick | 34,515 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: a slot with no record | Read | 8,844 | 1 | 139 | 1 | 0 | 236 | – | 0 |
| Tick A: a slot with no record | bridge (reply) | 21,104 | 1 | 717 | 4 | 575 | 0 | – | 245 |

