# ferrata_rebate: a pool's terms cannot be squatted; split progress is paid once per gap (dev mode)

Written by `RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture` (`a_pools_terms_cannot_be_squatted_and_split_progress_is_paid_once_per_gap`). RFP-021 soft requirement 5; review-rfp-I M-2 and L-1. The funder's terms: rebate 1000000, at least 16384 finalized slots since the last advance paid for; the squatter's: rebate 2^128 - 1, no gap. Pool funded with 3000000. Finalized slots 15556608 → 15556608 → 15564800 → 15572992 → 15581184; paid once, 1000000.

## Transactions

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | squat: Configure the funder's pool address with rebate 2^128 - 1 and no gap | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 8,648 | 8,648 | 0% | 616 | 74,112 | refused: exit 75 (rebate: WrongReceiver) |
| 2 | Configure the squatter's own terms (its own address) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 13,822 → Read 13,394 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,439 | 45,655 | 1% | 616 | 370,168 | applied |
| 3 | Configure the funder's pool (rebate 1,000,000, two periods at least) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 13,822 → Read 13,394 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,439 | 45,655 | 1% | 616 | 370,168 | applied |
| 4 | Configure the funder's pool again | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 9,067 | 9,067 | 0% | 616 | 77,464 | refused: exit 78 (rebate: AlreadyConfigured) |
| 5 | fund the pool (a native transfer) |  | 0 | 0% | 511 | 4,088 | applied |
| 6 | update_0 via the pool Open (step 1) | Open 2,129,836 → JobOpen 26,464 | 2,156,300 | 43% | 2,853 | 17,273,224 | applied |
| 7 | update_0 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 8 | update_0 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 9 | update_0 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 10 | update_0 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 13,394 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 20,136 → Finish 3,654,943 → Apply 45,260 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 20,734 | 3,801,436 | 76% | 719 | 30,417,240 | applied |
| 11 | update_1 via the pool Open (step 1) | Open 2,186,674 → JobOpen 26,464 | 2,213,138 | 44% | 3,605 | 17,733,944 | applied |
| 12 | update_1 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 13 | update_1 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 14 | update_1 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 15 | update_1 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 21,043 → Finish 3,654,943 → Apply 65,473 → Record 17,350 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 20,749 | 3,840,730 | 77% | 783 | 30,732,104 | applied |
| 16 | update_2 via the pool Open (step 1) | Open 2,187,248 → JobOpen 26,464 | 2,213,712 | 44% | 3,615 | 17,738,616 | applied |
| 17 | update_2 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 18 | update_2 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 19 | update_2 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 20 | update_2 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 21,043 → Finish 3,654,943 → Apply 65,473 → Record 17,350 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 23,890 | 3,843,871 | 77% | 783 | 30,757,232 | applied |
| 21 | update_3 via the pool Open (step 1) | Open 2,131,882 → JobOpen 26,464 | 2,158,346 | 43% | 2,852 | 17,289,584 | applied |
| 22 | update_3 via the pool Miller (step 2) | Miller 3,313,372 | 3,313,372 | 66% | 399 | 26,510,168 | applied |
| 23 | update_3 via the pool adopt 1/2 | Adopt 3,410,995 | 3,410,995 | 68% | 24,980 | 27,487,800 | applied |
| 24 | update_3 via the pool adopt 2/2 | Adopt 3,418,189 | 3,418,189 | 68% | 24,980 | 27,545,352 | applied |
| 25 | update_3 via the pool Land (Finish through the pool) | program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 18,222 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 14,544 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 21,043 → Finish 3,654,943 → Apply 65,473 → Record 17,350 → Read 14,203 → program GgKQ24mnLZSLdSNYiGV4bXMZdj34AWByMzPUUkYJ7LCy 20,749 | 3,840,730 | 77% | 783 | 30,732,104 | applied |
