# ferrata_rebate: whoever lands an advance through the pool is paid (dev mode, in-process harness)

Written by `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture`. RFP-021 soft requirement 5. The mainnet instance of periods 1899–1903; rebate 1000000 native units for an advance of at least 8192 finalized slots since the last one paid for, pool funded with 3000000 by a native transfer. Update 0 through the pool (no advance: nobody paid), update 1 through the pool (operator 1 paid), update 2 directly (nobody paid), update 3 through the pool (operator 2 paid). Finalized slots 15556608 → 15556608 → 15564800 → 15572992 → 15581184. Execution gas of `Land` [3801436, 3843871, 3843871], of a direct `Finish` [3736661].

## Transactions

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Configure the pool (rebate 1,000,000, one period of finalized progress at least) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 13,822 → Read 13,394 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,439 | 45,655 | 1% | 616 | 370,168 | applied |
| 2 | fund the pool (a native transfer) |  | 0 | 0% | 511 | 4,088 | applied |
| 3 | update_0 via the pool Open (step 1) | Open 2,129,836 → JobOpen 26,464 | 2,156,300 | 43% | 2,853 | 17,273,224 | applied |
| 4 | update_0 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 5 | update_0 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 6 | update_0 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 7 | update_0 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 13,394 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 20,136 → Finish 3,654,943 → Apply 45,260 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 20,734 | 3,801,436 | 76% | 719 | 30,417,240 | applied |
| 8 | update_1 via the pool Open (step 1) | Open 2,186,674 → JobOpen 26,464 | 2,213,138 | 44% | 3,605 | 17,733,944 | applied |
| 9 | update_1 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 10 | update_1 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 11 | update_1 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 12 | update_1 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 21,043 → Finish 3,654,943 → Apply 65,473 → Record 17,350 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 23,890 | 3,843,871 | 77% | 783 | 30,757,232 | applied |
| 13 | update_2 direct Open (step 1) | Open 2,187,248 → JobOpen 26,464 | 2,213,712 | 44% | 3,615 | 17,738,616 | applied |
| 14 | update_2 direct Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 15 | update_2 direct adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 16 | update_2 direct adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 17 | update_2 direct Finish (step 3) | Finish 3,653,838 → Apply 65,473 → Record 17,350 | 3,736,661 | 75% | 527 | 29,897,504 | applied |
| 18 | update_3 via the pool Open (step 1) | Open 2,131,882 → JobOpen 26,464 | 2,158,346 | 43% | 2,852 | 17,289,584 | applied |
| 19 | update_3 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 20 | update_3 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 21 | update_3 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 22 | update_3 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 21,043 → Finish 3,654,943 → Apply 65,473 → Record 17,350 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 23,890 | 3,843,871 | 77% | 783 | 30,757,232 | applied |

## Handlers

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Configure the pool (rebate 1,000,000, one period of finalized progress at least) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 13,822 | 1 | 163 | 89 | 0 | 382 | 121 | 0 |
| Configure the pool (rebate 1,000,000, one period of finalized progress at least) | Read | 13,394 | 1 | 644 | 1 | 505 | 1,246 | – | 0 |
| Configure the pool (rebate 1,000,000, one period of finalized progress at least) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 18,439 | 1 | 768 | 509 | 121 | 914 | 121 | 0 |
| update_0 via the pool Open (step 1) | Open | 2,129,836 | 3 | 2,970 | 2,391 | 505 | 4,393 | – | 0 |
| update_0 via the pool Open (step 1) | JobOpen | 26,464 | 1 | 1,420 | 1,282 | 0 | 2,726 | 1,281 | 0 |
| update_0 via the pool Miller (step 2) | Miller | 3,313,372 | 4 | 1,356 | 1 | 1,281 | 2,758 | 1,377 | 0 |
| update_0 via the pool adopt 1/2 | Adopt | 3,410,995 | 4 | 26,033 | 24,582 | 1,377 | 27,467 | 1,409 | 0 |
| update_0 via the pool adopt 2/2 | Adopt | 3,418,189 | 4 | 26,065 | 24,582 | 1,409 | 27,531 | 1,441 | 0 |
| update_0 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 18,222 | 1 | 260 | 65 | 121 | 732 | 155 | 0 |
| update_0 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 14,544 | 1 | 313 | 20 | 155 | 509 | 171 | 0 |
| update_0 via the pool Land (Finish through the pool) | Read | 13,394 | 1 | 644 | 1 | 505 | 1,246 | – | 0 |
| update_0 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 20,136 | 1 | 818 | 509 | 171 | 1,022 | 179 | 0 |
| update_0 via the pool Land (Finish through the pool) | Finish | 3,654,943 | 4 | 1,580 | 1 | 1,441 | 2,539 | 0 | 0 |
| update_0 via the pool Land (Finish through the pool) | Apply | 45,260 | 1 | 1,505 | 862 | 505 | 2,115 | 585 | 0 |
| update_0 via the pool Land (Finish through the pool) | Read | 14,203 | 1 | 724 | 1 | 585 | 1,406 | – | 0 |
| update_0 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 20,734 | 1 | 906 | 589 | 179 | 1,052 | 121 | 0 |
| update_1 via the pool Open (step 1) | Open | 2,186,674 | 3 | 3,802 | 3,143 | 585 | 5,225 | – | 0 |
| update_1 via the pool Open (step 1) | JobOpen | 26,464 | 1 | 1,420 | 1,282 | 0 | 2,726 | 1,281 | 0 |
| update_1 via the pool Miller (step 2) | Miller | 3,313,372 | 4 | 1,356 | 1 | 1,281 | 2,758 | 1,377 | 0 |
| update_1 via the pool adopt 1/2 | Adopt | 3,410,995 | 4 | 26,033 | 24,582 | 1,377 | 27,467 | 1,409 | 0 |
| update_1 via the pool adopt 2/2 | Adopt | 3,418,189 | 4 | 26,065 | 24,582 | 1,409 | 27,531 | 1,441 | 0 |
| update_1 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 18,222 | 1 | 260 | 65 | 121 | 732 | 155 | 0 |
| update_1 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 14,544 | 1 | 313 | 20 | 155 | 509 | 171 | 0 |
| update_1 via the pool Land (Finish through the pool) | Read | 14,203 | 1 | 724 | 1 | 585 | 1,406 | – | 0 |
| update_1 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 21,043 | 1 | 898 | 589 | 171 | 1,102 | 179 | 0 |
| update_1 via the pool Land (Finish through the pool) | Finish | 3,654,943 | 4 | 1,580 | 1 | 1,441 | 2,539 | 0 | 0 |
| update_1 via the pool Land (Finish through the pool) | Apply | 65,473 | 1 | 1,585 | 862 | 585 | 2,725 | 585 | 0 |
| update_1 via the pool Land (Finish through the pool) | Record | 17,350 | 1 | 512 | 374 | 0 | 902 | 365 | 0 |
| update_1 via the pool Land (Finish through the pool) | Read | 14,203 | 1 | 724 | 1 | 585 | 1,406 | – | 0 |
| update_1 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 23,890 | 1 | 906 | 589 | 179 | 1,205 | 121 | 0 |
| update_2 direct Open (step 1) | Open | 2,187,248 | 3 | 3,812 | 3,153 | 585 | 5,235 | – | 0 |
| update_2 direct Open (step 1) | JobOpen | 26,464 | 1 | 1,420 | 1,282 | 0 | 2,726 | 1,281 | 0 |
| update_2 direct Miller (step 2) | Miller | 3,313,372 | 4 | 1,356 | 1 | 1,281 | 2,758 | 1,377 | 0 |
| update_2 direct adopt 1/2 | Adopt | 3,410,995 | 4 | 26,033 | 24,582 | 1,377 | 27,467 | 1,409 | 0 |
| update_2 direct adopt 2/2 | Adopt | 3,418,189 | 4 | 26,065 | 24,582 | 1,409 | 27,531 | 1,441 | 0 |
| update_2 direct Finish (step 3) | Finish | 3,653,838 | 4 | 1,516 | 1 | 1,441 | 2,475 | 0 | 0 |
| update_2 direct Finish (step 3) | Apply | 65,473 | 1 | 1,585 | 862 | 585 | 2,725 | 585 | 0 |
| update_2 direct Finish (step 3) | Record | 17,350 | 1 | 512 | 374 | 0 | 902 | 365 | 0 |
| update_3 via the pool Open (step 1) | Open | 2,131,882 | 3 | 3,049 | 2,390 | 585 | 4,472 | – | 0 |
| update_3 via the pool Open (step 1) | JobOpen | 26,464 | 1 | 1,420 | 1,282 | 0 | 2,726 | 1,281 | 0 |
| update_3 via the pool Miller (step 2) | Miller | 3,313,372 | 4 | 1,356 | 1 | 1,281 | 2,758 | 1,377 | 0 |
| update_3 via the pool adopt 1/2 | Adopt | 3,410,995 | 4 | 26,033 | 24,582 | 1,377 | 27,467 | 1,409 | 0 |
| update_3 via the pool adopt 2/2 | Adopt | 3,418,189 | 4 | 26,065 | 24,582 | 1,409 | 27,531 | 1,441 | 0 |
| update_3 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 18,222 | 1 | 260 | 65 | 121 | 732 | 155 | 0 |
| update_3 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 14,544 | 1 | 313 | 20 | 155 | 509 | 171 | 0 |
| update_3 via the pool Land (Finish through the pool) | Read | 14,203 | 1 | 724 | 1 | 585 | 1,406 | – | 0 |
| update_3 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 21,043 | 1 | 898 | 589 | 171 | 1,102 | 179 | 0 |
| update_3 via the pool Land (Finish through the pool) | Finish | 3,654,943 | 4 | 1,580 | 1 | 1,441 | 2,539 | 0 | 0 |
| update_3 via the pool Land (Finish through the pool) | Apply | 65,473 | 1 | 1,585 | 862 | 585 | 2,725 | 585 | 0 |
| update_3 via the pool Land (Finish through the pool) | Record | 17,350 | 1 | 512 | 374 | 0 | 902 | 365 | 0 |
| update_3 via the pool Land (Finish through the pool) | Read | 14,203 | 1 | 724 | 1 | 585 | 1,406 | – | 0 |
| update_3 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy | 23,890 | 1 | 906 | 589 | 179 | 1,205 | 121 | 0 |
