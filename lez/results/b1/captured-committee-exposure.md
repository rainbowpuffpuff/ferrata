# A captured sync committee's exposure, bounded by the consumer's caps and freeze (`harness/tests/captured_committee_exposure.rs`)

Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`; every transaction settled by LEZ's own functions in a block of its own. RFP-021 Verification Security #4; `docs/trust.md` §2.2 (VS.4) and §6.

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
| 1 | Init A (an honest record, block 100) | bridge Init 44,890 → Attest 86,790 → bridge (reply) 39,738 | 171,418 | 3% | 1,946 | 1,386,912 | applied |
| 2 | Tick A: the captured committee's record | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,215 | 81,439 | 2% | 505 | 655,552 | applied |
| 3 | Register ETH (A) | bridge Register 74,324 → Attest 151,273 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 333,303 | 7% | 2,747 | 2,688,400 | applied |
| 4 | Register T3 (A) | bridge Register 78,167 → Attest 166,997 → bridge (reply) 75,349 → bridge WriteToken 32,357 | 352,870 | 7% | 2,921 | 2,846,328 | applied |
| 5 | SetCap (admin): the global mint bucket at 4 claims a window | bridge SetCap 13,540 → bridge SetCap 33,206 | 46,746 | 1% | 705 | 379,608 | applied |
| 6 | SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap 16,352 → bridge SetCap 32,953 → bridge SetCap 38,627 | 87,932 | 2% | 789 | 709,768 | applied |
| 7 | SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap 16,352 → bridge SetCap 32,953 → bridge SetCap 38,627 | 87,932 | 2% | 789 | 709,768 | applied |
| 8 | ImportRoot A: 18 leaves under the fabricated record | bridge ImportRoot 50,522 → Attest 109,870 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 231,635 | 5% | 1,895 | 1,868,240 | applied |
| 9 | ImportFees ETH (A): the schedules cover 18 leaves | bridge ImportFees 50,786 → Attest 104,326 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 244,769 | 5% | 1,815 | 1,972,672 | applied |
| 10 | ImportFees T3 (A): the schedules cover 18 leaves | bridge ImportFees 50,786 → Attest 104,326 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 244,769 | 5% | 1,815 | 1,972,672 | applied |
| 11 | Claim the invented ETH deposit at leaf 8 | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 18,800 → bridge Mint 18,800 | 261,770 | 5% | 2,053 | 2,110,584 | applied |
| 12 | Claim the invented ETH deposit at leaf 9 | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 23,177 → bridge Mint 23,177 | 270,524 | 5% | 2,053 | 2,180,616 | applied |
| 13 | Claim the invented ETH deposit at leaf 10 | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 23,177 → bridge Mint 23,177 | 270,524 | 5% | 2,053 | 2,180,616 | applied |
| 14 | Claim the invented ETH deposit at leaf 11: the token's bucket is empty | bridge Claim 107,796 → bridge Issue 27,714 | 135,510 | 3% | 2,053 | 1,100,504 | refused: exit 255 (CAPPED) |
| 15 | Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 23,698 → bridge Mint 23,698 | 271,566 | 5% | 2,053 | 2,188,952 | applied |
| 16 | Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 28,738 | 185,178 | 4% | 2,053 | 1,497,848 | refused: exit 255 (CAPPED) |
| 17 | Tick the global gate: a record one refill window later | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |
| 18 | Tick ETH (A): a record one refill window later | bridge Tick 38,571 → Read 12,268 → bridge (reply) 36,663 | 87,502 | 2% | 525 | 704,216 | applied |
| 19 | Tick T3 (A): a record one refill window later | bridge Tick 38,571 → Read 12,268 → bridge (reply) 36,663 | 87,502 | 2% | 525 | 704,216 | applied |
| 20 | Claim the invented T3 deposit at leaf 14 | bridge Claim 107,796 → bridge Issue 49,072 → bridge Admit 36,389 → bridge Spend 31,769 → bridge Mint 24,894 → bridge Mint 24,894 | 274,814 | 5% | 2,053 | 2,214,936 | applied |
| 21 | Claim the invented T3 deposit at leaf 15 | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 24,894 → bridge Mint 24,894 | 273,958 | 5% | 2,053 | 2,208,088 | applied |
| 22 | Claim the invented T3 deposit at leaf 16 | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 24,894 → bridge Mint 24,894 | 273,958 | 5% | 2,053 | 2,208,088 | applied |
| 23 | Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | bridge Claim 107,796 → bridge Issue 27,714 | 135,510 | 3% | 2,053 | 1,100,504 | refused: exit 255 (CAPPED) |
| 24 | Claim the invented ETH deposit at leaf 11 again, the next window | bridge Claim 107,796 → bridge Issue 49,072 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 24,646 → bridge Mint 24,646 | 273,890 | 5% | 2,053 | 2,207,544 | applied |
| 25 | Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 28,738 | 185,178 | 4% | 2,053 | 1,497,848 | refused: exit 255 (CAPPED) |
| 26 | Freeze (freeze authority): all mints of A for one hour of attested time | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,416 | 97,147 | 2% | 755 | 783,216 | applied |
| 27 | Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 28,709 | 185,149 | 4% | 2,053 | 1,497,616 | refused: exit 242 (FROZEN) |
| 28 | ImportRoot A: 20 leaves under the second fabricated record | bridge ImportRoot 50,522 → Attest 109,870 → bridge (reply) 54,112 → bridge WriteRoot 17,131 | 231,635 | 5% | 1,895 | 1,868,240 | applied |
| 29 | ImportFees ETH (A): the schedules cover 20 leaves | bridge ImportFees 50,786 → Attest 104,326 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 244,769 | 5% | 1,815 | 1,972,672 | applied |
| 30 | ImportFees T3 (A): the schedules cover 20 leaves | bridge ImportFees 50,786 → Attest 104,326 → bridge (reply) 52,960 → bridge ExtendFees 36,697 | 244,769 | 5% | 1,815 | 1,972,672 | applied |
| 31 | Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 28,709 | 185,149 | 4% | 2,053 | 1,497,616 | refused: exit 242 (FROZEN) |
| 32 | Tick the global gate: the second fabricated record, a minute later | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |
| 33 | Tick ETH (A): the second fabricated record, a minute later | bridge Tick 38,571 → Read 12,268 → bridge (reply) 36,663 | 87,502 | 2% | 525 | 704,216 | applied |
| 34 | Tick T3 (A): the second fabricated record, a minute later | bridge Tick 38,571 → Read 12,268 → bridge (reply) 36,663 | 87,502 | 2% | 525 | 704,216 | applied |
| 35 | Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Claim 107,796 → bridge Issue 48,977 → bridge Admit 28,709 | 185,482 | 4% | 2,053 | 1,500,280 | refused: exit 242 (FROZEN) |
| 36 | Tick A with a record two hours ahead of every LEZ block | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,214 | 10,000,000 | 200% | 505 | 80,004,040 | reverted: Execution outside of the validity window |
| 37 | Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Claim 107,796 → bridge Issue 48,977 → bridge Admit 28,709 | 185,482 | 4% | 2,053 | 1,500,280 | refused: exit 242 (FROZEN) |
| 38 | Freeze (freeze authority): unfreeze A's mints | bridge Freeze 13,320 → bridge Freeze 37,143 → Read 12,268 → bridge (reply) 34,403 | 97,134 | 2% | 755 | 783,112 | applied |
| 39 | Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze 16,258 → bridge Freeze 32,858 → bridge Freeze 42,226 → Read 12,268 → bridge (reply) 36,776 | 140,386 | 3% | 839 | 1,129,800 | applied |
| 40 | Tick the global gate: a record one refill window later again | bridge Tick 34,956 → Read 12,268 → bridge (reply) 34,214 | 81,438 | 2% | 505 | 655,544 | applied |
| 41 | Tick ETH (A): a record one refill window later again | bridge Tick 38,571 → Read 12,268 → bridge (reply) 36,663 | 87,502 | 2% | 525 | 704,216 | applied |
| 42 | Tick T3 (A): a record one refill window later again | bridge Tick 38,571 → Read 12,268 → bridge (reply) 36,663 | 87,502 | 2% | 525 | 704,216 | applied |
| 43 | Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | bridge Claim 107,796 → bridge Issue 27,684 | 135,480 | 3% | 2,053 | 1,100,264 | refused: exit 242 (FROZEN) |
| 44 | Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Claim 107,796 → bridge Issue 49,063 → bridge Admit 36,380 → bridge Spend 31,769 → bridge Mint 24,894 → bridge Mint 24,894 | 274,796 | 5% | 2,053 | 2,214,792 | applied |
| 45 | Claim the second record's invented T3 deposit at leaf 19 | bridge Claim 107,796 → bridge Issue 48,644 → bridge Admit 35,961 → bridge Spend 31,769 → bridge Mint 24,894 → bridge Mint 24,894 | 273,958 | 5% | 2,053 | 2,208,088 | applied |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init A (an honest record, block 100) | bridge Init | 44,890 | 1 | 1,493 | 1,419 | 0 | 3,136 | 390 | 0 |
| Init A (an honest record, block 100) | Attest | 86,790 | 1 | 1,659 | 1,156 | 365 | 2,122 | – | 0 |
| Init A (an honest record, block 100) | bridge (reply) | 39,738 | 1 | 898 | 370 | 390 | 1,503 | 580 | 0 |
| Tick A: the captured committee's record | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A: the captured committee's record | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A: the captured committee's record | bridge (reply) | 34,215 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Register ETH (A) | bridge Register | 74,324 | 1 | 2,875 | 2,221 | 580 | 5,971 | 966 | 0 |
| Register ETH (A) | Attest | 151,273 | 1 | 2,536 | 2,033 | 365 | 3,350 | – | 0 |
| Register ETH (A) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register ETH (A) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| Register T3 (A) | bridge Register | 78,167 | 1 | 3,049 | 2,395 | 580 | 6,319 | 966 | 0 |
| Register T3 (A) | Attest | 166,997 | 1 | 2,710 | 2,207 | 365 | 3,524 | – | 0 |
| Register T3 (A) | bridge (reply) | 75,349 | 1 | 1,825 | 721 | 966 | 3,386 | 580 | 0 |
| Register T3 (A) | bridge WriteToken | 32,357 | 1 | 990 | 852 | 0 | 1,867 | 852 | 0 |
| SetCap (admin): the global mint bucket at 4 claims a window | bridge SetCap | 13,540 | 1 | 141 | 67 | 0 | 301 | – | 0 |
| SetCap (admin): the global mint bucket at 4 claims a window | bridge SetCap | 33,206 | 1 | 785 | 67 | 580 | 1,390 | 580 | 0 |
| SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap | 32,953 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): ETH (A) mints at 3 claims a window | bridge SetCap | 38,627 | 1 | 1,077 | 87 | 852 | 1,954 | 852 | 0 |
| SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap | 16,352 | 1 | 161 | 87 | 0 | 341 | – | 0 |
| SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap | 32,953 | 1 | 805 | 87 | 580 | 985 | – | 0 |
| SetCap (admin): T3 (A) mints at 3 claims a window | bridge SetCap | 38,627 | 1 | 1,077 | 87 | 852 | 1,954 | 852 | 0 |
| ImportRoot A: 18 leaves under the fabricated record | bridge ImportRoot | 50,522 | 1 | 2,023 | 1,369 | 580 | 4,199 | 629 | 0 |
| ImportRoot A: 18 leaves under the fabricated record | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A: 18 leaves under the fabricated record | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A: 18 leaves under the fabricated record | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | bridge ImportFees | 50,786 | 1 | 1,943 | 1,289 | 580 | 4,035 | 618 | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH (A): the schedules cover 18 leaves | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | bridge ImportFees | 50,786 | 1 | 1,943 | 1,289 | 580 | 4,035 | 618 | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees T3 (A): the schedules cover 18 leaves | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim the invented ETH deposit at leaf 8 | bridge Mint | 18,800 | 1 | 299 | 161 | 0 | 398 | 74 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 9 | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 10 | bridge Mint | 23,177 | 1 | 373 | 161 | 74 | 472 | 74 | 0 |
| Claim the invented ETH deposit at leaf 11: the token's bucket is empty | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 11: the token's bucket is empty | bridge Issue | 27,714 | 1 | 1,189 | 199 | 852 | 0 | – | 255 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Mint | 23,698 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim the invented T3 deposit at leaf 13 (the global bucket's last claim) | bridge Mint | 23,698 | 1 | 373 | 161 | 74 | 540 | 142 | 0 |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented T3 deposit at leaf 14: the global bucket is empty | bridge Admit | 28,738 | 1 | 788 | 70 | 580 | 0 | – | 255 |
| Tick the global gate: a record one refill window later | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick the global gate: a record one refill window later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick the global gate: a record one refill window later | bridge (reply) | 34,214 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Tick ETH (A): a record one refill window later | bridge Tick | 38,571 | 1 | 989 | 63 | 852 | 1,949 | 862 | 0 |
| Tick ETH (A): a record one refill window later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record one refill window later | bridge (reply) | 36,663 | 1 | 1,369 | 369 | 862 | 2,254 | 852 | 0 |
| Tick T3 (A): a record one refill window later | bridge Tick | 38,571 | 1 | 989 | 63 | 852 | 1,949 | 862 | 0 |
| Tick T3 (A): a record one refill window later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick T3 (A): a record one refill window later | bridge (reply) | 36,663 | 1 | 1,369 | 369 | 862 | 2,254 | 852 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Issue | 49,072 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Admit | 36,389 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 14 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 15 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 16 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 17: T3's bucket is empty again | bridge Issue | 27,714 | 1 | 1,189 | 199 | 852 | 0 | – | 255 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Issue | 49,072 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Mint | 24,646 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented ETH deposit at leaf 11 again, the next window | bridge Mint | 24,646 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 12: the global bucket is empty again | bridge Admit | 28,738 | 1 | 788 | 70 | 580 | 0 | – | 255 |
| Freeze (freeze authority): all mints of A for one hour of attested time | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): all mints of A for one hour of attested time | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze (freeze authority): all mints of A for one hour of attested time | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): all mints of A for one hour of attested time | bridge (reply) | 34,416 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 12 while mints are frozen | bridge Admit | 28,709 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| ImportRoot A: 20 leaves under the second fabricated record | bridge ImportRoot | 50,522 | 1 | 2,023 | 1,369 | 580 | 4,199 | 629 | 0 |
| ImportRoot A: 20 leaves under the second fabricated record | Attest | 109,870 | 1 | 1,953 | 1,450 | 365 | 2,533 | – | 0 |
| ImportRoot A: 20 leaves under the second fabricated record | bridge (reply) | 54,112 | 1 | 1,254 | 487 | 629 | 2,080 | 580 | 0 |
| ImportRoot A: 20 leaves under the second fabricated record | bridge WriteRoot | 17,131 | 1 | 255 | 117 | 0 | 397 | 117 | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | bridge ImportFees | 50,786 | 1 | 1,943 | 1,289 | 580 | 4,035 | 618 | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees ETH (A): the schedules cover 20 leaves | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | bridge ImportFees | 50,786 | 1 | 1,943 | 1,289 | 580 | 4,035 | 618 | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | Attest | 104,326 | 1 | 1,880 | 1,377 | 365 | 2,460 | – | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | bridge (reply) | 52,960 | 1 | 1,243 | 487 | 618 | 2,022 | 580 | 0 |
| ImportFees T3 (A): the schedules cover 20 leaves | bridge ExtendFees | 36,697 | 1 | 1,060 | 70 | 852 | 1,937 | 852 | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while mints are frozen | bridge Admit | 28,709 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Tick the global gate: the second fabricated record, a minute later | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick the global gate: the second fabricated record, a minute later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick the global gate: the second fabricated record, a minute later | bridge (reply) | 34,214 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Tick ETH (A): the second fabricated record, a minute later | bridge Tick | 38,571 | 1 | 989 | 63 | 852 | 1,949 | 862 | 0 |
| Tick ETH (A): the second fabricated record, a minute later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): the second fabricated record, a minute later | bridge (reply) | 36,663 | 1 | 1,369 | 369 | 862 | 2,254 | 852 | 0 |
| Tick T3 (A): the second fabricated record, a minute later | bridge Tick | 38,571 | 1 | 989 | 63 | 852 | 1,949 | 862 | 0 |
| Tick T3 (A): the second fabricated record, a minute later | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick T3 (A): the second fabricated record, a minute later | bridge (reply) | 36,663 | 1 | 1,369 | 369 | 862 | 2,254 | 852 | 0 |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Issue | 48,977 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 12 a minute of attested time later | bridge Admit | 28,709 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Tick A with a record two hours ahead of every LEZ block | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick A with a record two hours ahead of every LEZ block | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick A with a record two hours ahead of every LEZ block | bridge (reply) | 34,214 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Issue | 48,977 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented ETH deposit at leaf 12 after the far-future record | bridge Admit | 28,709 | 1 | 788 | 70 | 580 | 0 | – | 242 |
| Freeze (freeze authority): unfreeze A's mints | bridge Freeze | 13,320 | 1 | 127 | 53 | 0 | 273 | – | 0 |
| Freeze (freeze authority): unfreeze A's mints | bridge Freeze | 37,143 | 1 | 771 | 53 | 580 | 1,470 | 601 | 0 |
| Freeze (freeze authority): unfreeze A's mints | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): unfreeze A's mints | bridge (reply) | 34,403 | 1 | 1,108 | 369 | 601 | 1,721 | 580 | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze | 16,258 | 1 | 147 | 73 | 0 | 313 | – | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze | 32,858 | 1 | 791 | 73 | 580 | 957 | – | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge Freeze | 42,226 | 1 | 1,063 | 73 | 852 | 2,033 | 872 | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Freeze (freeze authority): ETH (A) mints for 30 days of attested time | bridge (reply) | 36,776 | 1 | 1,379 | 369 | 872 | 2,264 | 852 | 0 |
| Tick the global gate: a record one refill window later again | bridge Tick | 34,956 | 1 | 697 | 43 | 580 | 1,386 | 591 | 0 |
| Tick the global gate: a record one refill window later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick the global gate: a record one refill window later again | bridge (reply) | 34,214 | 1 | 1,098 | 369 | 591 | 1,711 | 580 | 0 |
| Tick ETH (A): a record one refill window later again | bridge Tick | 38,571 | 1 | 989 | 63 | 852 | 1,949 | 862 | 0 |
| Tick ETH (A): a record one refill window later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick ETH (A): a record one refill window later again | bridge (reply) | 36,663 | 1 | 1,369 | 369 | 862 | 2,254 | 852 | 0 |
| Tick T3 (A): a record one refill window later again | bridge Tick | 38,571 | 1 | 989 | 63 | 852 | 1,949 | 862 | 0 |
| Tick T3 (A): a record one refill window later again | Read | 12,268 | 1 | 504 | 1 | 365 | 966 | – | 0 |
| Tick T3 (A): a record one refill window later again | bridge (reply) | 36,663 | 1 | 1,369 | 369 | 862 | 2,254 | 852 | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen | bridge Issue | 27,684 | 1 | 1,189 | 199 | 852 | 0 | – | 242 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Issue | 49,063 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Admit | 36,380 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the invented T3 deposit at leaf 17 while only ETH is frozen | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Claim | 107,796 | 1 | 1,462 | 1,271 | 117 | 1,754 | – | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Issue | 48,644 | 1 | 1,189 | 199 | 852 | 2,534 | 852 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Admit | 35,961 | 1 | 788 | 70 | 580 | 1,393 | 580 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Spend | 31,769 | 1 | 360 | 222 | 0 | 992 | 77 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |
| Claim the second record's invented T3 deposit at leaf 19 | bridge Mint | 24,894 | 1 | 441 | 161 | 142 | 608 | 142 | 0 |

