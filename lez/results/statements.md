# ferrata_statements: a recorded statement serves several consumers (dev mode, in-process harness)

Written by `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- --nocapture`. RFP-021 soft requirement 1. Holder 0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc (Uniswap v2 USDC/WETH), WETH 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2, record `header(id, 15624480)`, balance 0x0000000000000000000000000000000000000000000000f22ca9b4cc5d0f3d61 wei, question: balance ≥ 1 WETH.

Summed guest cycles of each transaction (every program it ran): direct path (`ferrata_holdings` with `Attest`) 432128; `Record` 393331; one consumer's `Use` (reads the entry, verifies no proof) 63438. With `k` consumers of one question the recorded path costs `Record` + k × `Use`, the direct path k × direct.

## Transactions

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | direct path: ferrata_holdings Check (Attest in the same transaction) | holdings Check 111,180 → Attest 279,023 → holdings (reply) 41,925 | 432,128 | 9% | 7,493 | 3,516,968 | applied |
| 2 | Record the question (Attest through the recorder) | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 93,490 → Attest 279,023 → program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 20,818 | 393,331 | 8% | 7,507 | 3,206,704 | applied |
| 3 | consumer A: Use (Read of the entry) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP 19,115 → program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 20,827 → program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP 23,496 | 63,438 | 1% | 793 | 513,848 | applied |
| 4 | consumer B: Use (Read of the entry) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP 19,115 → program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 20,827 → program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP 23,496 | 63,438 | 1% | 793 | 513,848 | applied |
| 5 | consumer A again (same configuration) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP 8,119 | 8,119 | 0% | 793 | 71,296 | refused: exit 71 (statements: AlreadyUsed) |
| 6 | Record the same question again | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 38,556 | 38,556 | 1% | 7,507 | 368,504 | refused: exit 67 (statements: AlreadyRecorded) |

## Handlers

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| direct path: ferrata_holdings Check (Attest in the same transaction) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| direct path: ferrata_holdings Check (Attest in the same transaction) | Attest | 279,023 | 1 | 7,443 | 6,940 | 365 | 7,906 | – | 0 |
| direct path: ferrata_holdings Check (Attest in the same transaction) | holdings (reply) | 41,925 | 1 | 750 | 370 | 242 | 940 | 165 | 0 |
| Record the question (Attest through the recorder) | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 | 93,490 | 1 | 7,054 | 6,980 | 0 | 14,220 | 129 | 0 |
| Record the question (Attest through the recorder) | Attest | 279,023 | 1 | 7,443 | 6,940 | 365 | 7,906 | – | 0 |
| Record the question (Attest through the recorder) | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 | 20,818 | 1 | 637 | 370 | 129 | 1,161 | 499 | 0 |
| consumer A: Use (Read of the entry) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP | 19,115 | 1 | 275 | 201 | 0 | 574 | 201 | 0 |
| consumer A: Use (Read of the entry) | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 | 20,827 | 1 | 638 | 1 | 499 | 1,229 | – | 0 |
| consumer A: Use (Read of the entry) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP | 23,496 | 1 | 837 | 498 | 201 | 999 | 137 | 0 |
| consumer B: Use (Read of the entry) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP | 19,115 | 1 | 275 | 201 | 0 | 574 | 201 | 0 |
| consumer B: Use (Read of the entry) | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 | 20,827 | 1 | 638 | 1 | 499 | 1,229 | – | 0 |
| consumer B: Use (Read of the entry) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP | 23,496 | 1 | 837 | 498 | 201 | 999 | 137 | 0 |
| consumer A again (same configuration) | program F68xED6atTggdCN2q7N9YYR8Bk6WUDxkquafLajk3bqP | 8,119 | 1 | 412 | 201 | 137 | 0 | – | 71 |
| Record the same question again | program 6efcqq6qFjJPucoTqowFXVqzWKm9VvGbFVWq9ktoGa43 | 38,556 | 1 | 7,553 | 6,980 | 499 | 0 | – | 67 |
