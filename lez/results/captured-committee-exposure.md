# A captured sync committee's exposure, bounded by the consumer's caps and freeze (`harness/tests/captured_committee_exposure.rs`)

Bridge program image `ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd`; every transaction settled by LEZ's own functions in a block of its own. RFP-021 Verification Security #4; `docs/trust.md` §2.2 (VS.4) and §6.

## The fabricated record

A captured committee signs a finality update for a block Ethereum never finalized; the attestation program records it as any other (`World::add_block` places it at `header(instance, slot)` the way the light client writes one after a finality update). Its state trie holds vault A's storage with ten invented deposits appended to the tree (leaves 8-17), each paying the forger 1 ETH; `ImportRoot` accepts the root, and claims of the invented leaves are indistinguishable from honest ones. A second such record (leaves 18-19, stamped a minute after the freeze's) arrives while mints are frozen, and a third stamped two hours ahead of every LEZ block — which only a captured committee could have had written — cannot even move the consumer's clock.

## What each step minted or was refused

| step | outcome |
|---|---|
| Caps set before the attack | per token 3 claims a window, globally 4 (below the tokens' 6) |
| Claim the invented ETH deposit at leaf 8 | minted (1001000000 value units of the caps so far) |
| Claim the invented ETH deposit at leaf 9 | minted (2002000000 value units of the caps so far) |
| Claim the invented ETH deposit at leaf 10 | minted (3003000000 value units of the caps so far) |
| Claim the invented ETH deposit at leaf 11: the token's bucket is empty | refused, exit 255 (CAPPED): the per-token mint bucket is empty |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | minted (1001000000 value units of the caps so far) |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | refused, exit 255 (CAPPED): the global mint bucket is empty while T3's still holds two claims |
| Claim the invented T3 deposit at leaf 14 | minted (2002000000 value units of the caps so far) |
| Claim the invented T3 deposit at leaf 15 | minted (3003000000 value units of the caps so far) |
| Claim the invented T3 deposit at leaf 16 | minted (4004000000 value units of the caps so far) |
| Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | refused, exit 255 (CAPPED): T3's mint bucket is empty (the global one still holds a claim) |
| Claim the invented ETH deposit at leaf 11 again, the next window | minted (4004000000 value units of the caps so far) |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | refused, exit 255 (CAPPED): the global mint bucket is empty while ETH's still holds two claims |
| Freeze (freeze authority): all mints of A | applied, until one hour of attested time passes |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | refused, exit 242 (FROZEN): mints frozen globally |
| ImportRoot under the second fabricated record | imported: a freeze stops mints, not imports |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | refused, exit 242 (FROZEN): a fresh fabricated record does not unfreeze |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | refused, exit 242 (FROZEN): the freeze ends on attested time, not on records |
| Claim the invented ETH deposit at leaf 12 after the far-future record | refused, exit 242 (FROZEN): the far-future record ended no freeze |
| Freeze (freeze authority): ETH (A) mints only | applied, for 30 days of attested time |
| Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | refused, exit 242 (FROZEN): ETH's own gate is frozen, a check that runs before its bucket |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | minted (5005000000 value units of the caps so far) |
| Claim the second record's invented T3 deposit at leaf 19 | minted (6006000000 value units of the caps so far) |
| The bound | per token at most capacity + elapsed × refill; over both tokens the global capacity's share: over 80140 s of attested time, 10010000000 of the 12018000000 global bound (4004000000 ETH, 6006000000 T3) |

## The bound

Until the freeze authority froze mints, the caps admitted at most `capacity + elapsed × refill` value units per token, and at most the same with the global capacity over all tokens together: 4004000000 units per 40040 s window (1001000000 per claim). Over 80140 s of attested time the forger took 4004000000 value units of ETH (bound 11017000000), 6006000000 of T3 (bound 11017000000), 10010000000 in total (bound 12018000000); while the freezes held, every claim was refused with 242; after the 30-day freeze of ETH alone, only ETH's claims stopped. A captured committee that keeps its signatures can take the cap per window until the freeze — nothing more.

## The transactions

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init A (an honest record, block 100) | bridge Init 44,818 → Attest 86,790 → bridge (reply) 39,419 | 171,027 | 3% | 1,946 | 1,383,784 | applied |
| 2 | Tick A: the captured committee's record | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,846 | 80,628 | 2% | 505 | 649,064 | applied |
| 3 | Register ETH (A) | bridge Register 73,920 → Attest 151,273 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 331,816 | 7% | 2,747 | 2,676,504 | applied |
| 4 | Register T3 (A) | bridge Register 77,796 → Attest 166,997 → bridge (reply) 74,476 → bridge WriteToken 32,147 | 351,416 | 7% | 2,921 | 2,834,696 | applied |
| 5 | SetCap (admin): the global mint bucket at 4 claims a window | bridge SetCap 13,467 → bridge SetCap 32,809 | 46,276 | 1% | 705 | 375,848 | applied |
| 6 | SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap 16,279 → bridge SetCap 32,683 → bridge SetCap 38,192 | 87,154 | 2% | 789 | 703,544 | applied |
| 7 | SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap 16,279 → bridge SetCap 32,683 → bridge SetCap 38,192 | 87,154 | 2% | 789 | 703,544 | applied |
| 8 | ImportRoot A: 18 leaves under the fabricated record | bridge ImportRoot 50,128 → Attest 109,870 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 230,731 | 5% | 1,895 | 1,861,008 | applied |
| 9 | ImportFees ETH (A): the schedules cover 18 leaves | bridge ImportFees 50,383 → Attest 104,326 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 243,056 | 5% | 1,815 | 1,958,968 | applied |
| 10 | ImportFees T3 (A): the schedules cover 18 leaves | bridge ImportFees 50,383 → Attest 104,326 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 243,056 | 5% | 1,815 | 1,958,968 | applied |
| 11 | Claim the invented ETH deposit at leaf 8 | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 18,161 → bridge Mint 18,161 | 257,796 | 5% | 2,053 | 2,078,792 | applied |
| 12 | Claim the invented ETH deposit at leaf 9 | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 22,496 → bridge Mint 22,496 | 266,466 | 5% | 2,053 | 2,148,152 | applied |
| 13 | Claim the invented ETH deposit at leaf 10 | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 22,496 → bridge Mint 22,496 | 266,466 | 5% | 2,053 | 2,148,152 | applied |
| 14 | Claim the invented ETH deposit at leaf 11: the token's bucket is empty | bridge Claim 107,702 → bridge Issue 27,500 | 135,202 | 3% | 2,053 | 1,098,040 | refused: exit 255 (CAPPED) |
| 15 | Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 23,017 → bridge Mint 23,017 | 267,508 | 5% | 2,053 | 2,156,488 | applied |
| 16 | Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 28,502 | 183,496 | 4% | 2,053 | 1,484,392 | refused: exit 255 (CAPPED) |
| 17 | Tick the global gate: a record one refill window later | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,845 | 80,627 | 2% | 505 | 649,056 | applied |
| 18 | Tick ETH (A): a record one refill window later | bridge Tick 38,211 → Read 12,268 → bridge (reply) 37,033 | 87,512 | 2% | 525 | 704,296 | applied |
| 19 | Tick T3 (A): a record one refill window later | bridge Tick 38,211 → Read 12,268 → bridge (reply) 37,033 | 87,512 | 2% | 525 | 704,296 | applied |
| 20 | Claim the invented T3 deposit at leaf 14 | bridge Claim 107,702 → bridge Issue 47,720 → bridge Admit 35,992 → bridge Spend 30,916 → bridge Mint 24,213 → bridge Mint 24,213 | 270,756 | 5% | 2,053 | 2,182,472 | applied |
| 21 | Claim the invented T3 deposit at leaf 15 | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 24,213 → bridge Mint 24,213 | 269,900 | 5% | 2,053 | 2,175,624 | applied |
| 22 | Claim the invented T3 deposit at leaf 16 | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 24,213 → bridge Mint 24,213 | 269,900 | 5% | 2,053 | 2,175,624 | applied |
| 23 | Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | bridge Claim 107,702 → bridge Issue 27,500 | 135,202 | 3% | 2,053 | 1,098,040 | refused: exit 255 (CAPPED) |
| 24 | Claim the invented ETH deposit at leaf 11 again, the next window | bridge Claim 107,702 → bridge Issue 47,720 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 23,965 → bridge Mint 23,965 | 269,832 | 5% | 2,053 | 2,175,080 | applied |
| 25 | Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 28,502 | 183,496 | 4% | 2,053 | 1,484,392 | refused: exit 255 (CAPPED) |
| 26 | Freeze (freeze authority): all mints of A for one hour of attested time | bridge Freeze 13,251 → bridge Freeze 36,541 → Read 12,268 → bridge (reply) 33,880 | 95,940 | 2% | 755 | 773,560 | applied |
| 27 | Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 28,473 | 183,467 | 4% | 2,053 | 1,484,160 | refused: exit 242 (FROZEN) |
| 28 | ImportRoot A: 20 leaves under the second fabricated record | bridge ImportRoot 50,128 → Attest 109,870 → bridge (reply) 53,650 → bridge WriteRoot 17,083 | 230,731 | 5% | 1,895 | 1,861,008 | applied |
| 29 | ImportFees ETH (A): the schedules cover 20 leaves | bridge ImportFees 50,383 → Attest 104,326 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 243,056 | 5% | 1,815 | 1,958,968 | applied |
| 30 | ImportFees T3 (A): the schedules cover 20 leaves | bridge ImportFees 50,383 → Attest 104,326 → bridge (reply) 52,487 → bridge ExtendFees 35,860 | 243,056 | 5% | 1,815 | 1,958,968 | applied |
| 31 | Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 28,473 | 183,467 | 4% | 2,053 | 1,484,160 | refused: exit 242 (FROZEN) |
| 32 | Tick the global gate: the second fabricated record, a minute later | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,845 | 80,627 | 2% | 505 | 649,056 | applied |
| 33 | Tick ETH (A): the second fabricated record, a minute later | bridge Tick 38,211 → Read 12,268 → bridge (reply) 37,033 | 87,512 | 2% | 525 | 704,296 | applied |
| 34 | Tick T3 (A): the second fabricated record, a minute later | bridge Tick 38,211 → Read 12,268 → bridge (reply) 37,033 | 87,512 | 2% | 525 | 704,296 | applied |
| 35 | Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Claim 107,702 → bridge Issue 47,625 → bridge Admit 28,473 | 183,800 | 4% | 2,053 | 1,486,824 | refused: exit 242 (FROZEN) |
| 36 | Tick A with a record two hours ahead of every LEZ block | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,845 | 10,000,000 | 200% | 505 | 80,004,040 | reverted: Execution outside of the validity window |
| 37 | Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Claim 107,702 → bridge Issue 47,625 → bridge Admit 28,473 | 183,800 | 4% | 2,053 | 1,486,824 | refused: exit 242 (FROZEN) |
| 38 | Freeze (freeze authority): unfreeze A's mints | bridge Freeze 13,251 → bridge Freeze 36,541 → Read 12,268 → bridge (reply) 33,867 | 95,927 | 2% | 755 | 773,456 | applied |
| 39 | Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze 16,189 → bridge Freeze 32,433 → bridge Freeze 42,594 → Read 12,268 → bridge (reply) 37,128 | 140,612 | 3% | 839 | 1,131,608 | applied |
| 40 | Tick the global gate: a record one refill window later again | bridge Tick 34,514 → Read 12,268 → bridge (reply) 33,845 | 80,627 | 2% | 505 | 649,056 | applied |
| 41 | Tick ETH (A): a record one refill window later again | bridge Tick 38,211 → Read 12,268 → bridge (reply) 37,033 | 87,512 | 2% | 525 | 704,296 | applied |
| 42 | Tick T3 (A): a record one refill window later again | bridge Tick 38,211 → Read 12,268 → bridge (reply) 37,033 | 87,512 | 2% | 525 | 704,296 | applied |
| 43 | Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | bridge Claim 107,702 → bridge Issue 27,470 | 135,172 | 3% | 2,053 | 1,097,800 | refused: exit 242 (FROZEN) |
| 44 | Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Claim 107,702 → bridge Issue 47,711 → bridge Admit 35,983 → bridge Spend 30,916 → bridge Mint 24,213 → bridge Mint 24,213 | 270,738 | 5% | 2,053 | 2,182,328 | applied |
| 45 | Claim the second record's invented T3 deposit at leaf 19 | bridge Claim 107,702 → bridge Issue 47,292 → bridge Admit 35,564 → bridge Spend 30,916 → bridge Mint 24,213 → bridge Mint 24,213 | 269,900 | 5% | 2,053 | 2,175,624 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A (an honest record, block 100) | bridge Init | 44,818 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A (an honest record, block 100) | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A (an honest record, block 100) | bridge (reply) | 39,419 | 1 | 898 | 370 | 390 | 1,487 | 564 | 0 |
| Tick A: the captured committee's record | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A: the captured committee's record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: the captured committee's record | bridge (reply) | 33,846 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Register ETH (A) | bridge Register | 73,920 | 1 | 2,859 | 2,221 | 564 | 5,939 | 950 | 0 |
| Register ETH (A) | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (A) | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register ETH (A) | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| Register T3 (A) | bridge Register | 77,796 | 1 | 3,033 | 2,395 | 564 | 6,287 | 950 | 0 |
| Register T3 (A) | Attest | 166,997 | 1 | 2,710 | 2,207 | 365 | 3,524 | – | 0 |
| Register T3 (A) | bridge (reply) | 74,476 | 1 | 1,809 | 721 | 950 | 3,345 | 564 | 0 |
| Register T3 (A) | bridge WriteToken | 32,147 | 1 | 981 | 843 | 0 | 1,849 | 843 | 0 |
| SetCap (admin): the global mint bucket at 4 claims a window | bridge SetCap | 13,467 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): the global mint bucket at 4 claims a window | bridge SetCap | 32,809 | 1 | 769 | 67 | 564 | 1,358 | 564 | 0 |
| SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap | 32,683 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap | 38,192 | 1 | 1,068 | 87 | 843 | 1,936 | 843 | 0 |
| SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap | 16,279 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap | 32,683 | 1 | 789 | 87 | 564 | 969 | – | 0 |
| SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap | 38,192 | 1 | 1,068 | 87 | 843 | 1,936 | 843 | 0 |
| ImportRoot A: 18 leaves under the fabricated record | bridge ImportRoot | 50,128 | 1 | 2,007 | 1,369 | 564 | 4,167 | 613 | 0 |
| ImportRoot A: 18 leaves under the fabricated record | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A: 18 leaves under the fabricated record | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A: 18 leaves under the fabricated record | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | bridge ImportFees | 50,383 | 1 | 1,927 | 1,289 | 564 | 4,003 | 602 | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | bridge ImportFees | 50,383 | 1 | 1,927 | 1,289 | 564 | 4,003 | 602 | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Mint | 18,161 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Mint | 22,496 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 11: the token's bucket is empty | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 11: the token's bucket is empty | bridge Issue | 27,500 | 1 | 1,180 | 199 | 843 | 0 | – | 255 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Mint | 23,017 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Mint | 23,017 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Admit | 28,502 | 1 | 772 | 70 | 564 | 0 | – | 255 |
| Tick the global gate: a record one refill window later | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick the global gate: a record one refill window later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick the global gate: a record one refill window later | bridge (reply) | 33,845 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Tick ETH (A): a record one refill window later | bridge Tick | 38,211 | 1 | 980 | 63 | 843 | 1,931 | 853 | 0 |
| Tick ETH (A): a record one refill window later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record one refill window later | bridge (reply) | 37,033 | 1 | 1,360 | 369 | 853 | 2,236 | 843 | 0 |
| Tick T3 (A): a record one refill window later | bridge Tick | 38,211 | 1 | 980 | 63 | 843 | 1,931 | 853 | 0 |
| Tick T3 (A): a record one refill window later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick T3 (A): a record one refill window later | bridge (reply) | 37,033 | 1 | 1,360 | 369 | 853 | 2,236 | 843 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Issue | 47,720 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Admit | 35,992 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | bridge Issue | 27,500 | 1 | 1,180 | 199 | 843 | 0 | – | 255 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Issue | 47,720 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Mint | 23,965 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Mint | 23,965 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Admit | 28,502 | 1 | 772 | 70 | 564 | 0 | – | 255 |
| Freeze (freeze authority): all mints of A for one hour of attested time | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all mints of A for one hour of attested time | bridge Freeze | 36,541 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze (freeze authority): all mints of A for one hour of attested time | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): all mints of A for one hour of attested time | bridge (reply) | 33,880 | 1 | 1,092 | 369 | 585 | 1,689 | 564 | 0 |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Admit | 28,473 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| ImportRoot A: 20 leaves under the second fabricated record | bridge ImportRoot | 50,128 | 1 | 2,007 | 1,369 | 564 | 4,167 | 613 | 0 |
| ImportRoot A: 20 leaves under the second fabricated record | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A: 20 leaves under the second fabricated record | bridge (reply) | 53,650 | 1 | 1,238 | 487 | 613 | 2,048 | 564 | 0 |
| ImportRoot A: 20 leaves under the second fabricated record | bridge WriteRoot | 17,083 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | bridge ImportFees | 50,383 | 1 | 1,927 | 1,289 | 564 | 4,003 | 602 | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | bridge ImportFees | 50,383 | 1 | 1,927 | 1,289 | 564 | 4,003 | 602 | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | bridge (reply) | 52,487 | 1 | 1,227 | 487 | 602 | 1,990 | 564 | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | bridge ExtendFees | 35,860 | 1 | 1,051 | 70 | 843 | 1,919 | 843 | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Admit | 28,473 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Tick the global gate: the second fabricated record, a minute later | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick the global gate: the second fabricated record, a minute later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick the global gate: the second fabricated record, a minute later | bridge (reply) | 33,845 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Tick ETH (A): the second fabricated record, a minute later | bridge Tick | 38,211 | 1 | 980 | 63 | 843 | 1,931 | 853 | 0 |
| Tick ETH (A): the second fabricated record, a minute later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the second fabricated record, a minute later | bridge (reply) | 37,033 | 1 | 1,360 | 369 | 853 | 2,236 | 843 | 0 |
| Tick T3 (A): the second fabricated record, a minute later | bridge Tick | 38,211 | 1 | 980 | 63 | 843 | 1,931 | 853 | 0 |
| Tick T3 (A): the second fabricated record, a minute later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick T3 (A): the second fabricated record, a minute later | bridge (reply) | 37,033 | 1 | 1,360 | 369 | 853 | 2,236 | 843 | 0 |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Issue | 47,625 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Admit | 28,473 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Tick A with a record two hours ahead of every LEZ block | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick A with a record two hours ahead of every LEZ block | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A with a record two hours ahead of every LEZ block | bridge (reply) | 33,845 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Issue | 47,625 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Admit | 28,473 | 1 | 772 | 70 | 564 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze A's mints | bridge Freeze | 13,251 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): unfreeze A's mints | bridge Freeze | 36,541 | 1 | 755 | 53 | 564 | 1,438 | 585 | 0 |
| Freeze (freeze authority): unfreeze A's mints | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): unfreeze A's mints | bridge (reply) | 33,867 | 1 | 1,092 | 369 | 585 | 1,689 | 564 | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze | 16,189 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze | 32,433 | 1 | 775 | 73 | 564 | 941 | – | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze | 42,594 | 1 | 1,054 | 73 | 843 | 2,015 | 863 | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge (reply) | 37,128 | 1 | 1,370 | 369 | 863 | 2,246 | 843 | 0 |
| Tick the global gate: a record one refill window later again | bridge Tick | 34,514 | 1 | 681 | 43 | 564 | 1,354 | 575 | 0 |
| Tick the global gate: a record one refill window later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick the global gate: a record one refill window later again | bridge (reply) | 33,845 | 1 | 1,082 | 369 | 575 | 1,679 | 564 | 0 |
| Tick ETH (A): a record one refill window later again | bridge Tick | 38,211 | 1 | 980 | 63 | 843 | 1,931 | 853 | 0 |
| Tick ETH (A): a record one refill window later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record one refill window later again | bridge (reply) | 37,033 | 1 | 1,360 | 369 | 853 | 2,236 | 843 | 0 |
| Tick T3 (A): a record one refill window later again | bridge Tick | 38,211 | 1 | 980 | 63 | 843 | 1,931 | 853 | 0 |
| Tick T3 (A): a record one refill window later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick T3 (A): a record one refill window later again | bridge (reply) | 37,033 | 1 | 1,360 | 369 | 853 | 2,236 | 843 | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | bridge Issue | 27,470 | 1 | 1,180 | 199 | 843 | 0 | – | 242 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Issue | 47,711 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Admit | 35,983 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Claim | 107,702 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Issue | 47,292 | 1 | 1,180 | 199 | 843 | 2,516 | 843 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Admit | 35,564 | 1 | 772 | 70 | 564 | 1,361 | 564 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Spend | 30,916 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Mint | 24,213 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |

