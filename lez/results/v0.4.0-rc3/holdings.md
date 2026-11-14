# ferrata_holdings on the Stage 4 chain (dev mode, in-process harness)

Written by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test holdings -- --nocapture`.
Holder 0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc (Uniswap v2 USDC/WETH), WETH 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2, block 26384851, record `header(id, 15624480)`,
balance 0x0000000000000000000000000000000000000000000000f22ca9b4cc5d0f3d61 wei.

## Transactions

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | wrong chain (a gate for Sepolia) | holdings Check 111,180 → Attest 39,317 | 150,497 | 3% | 7,493 | 1,263,920 | refused: exit 140 (ChainMismatch) |
| 2 | unfinalized block (the attested, not finalized, header) | holdings Check 111,180 → Attest 31,333 | 142,513 | 3% | 7,558 | 1,200,568 | refused: exit 213 (NoRecord) |
| 3 | unknown block (no record at this slot) | holdings Check 111,180 → Attest 31,333 | 142,513 | 3% | 7,558 | 1,200,568 | refused: exit 213 (NoRecord) |
| 4 | proof of another block (the bootstrap's record) | holdings Check 111,180 → Attest 52,872 | 164,052 | 3% | 7,493 | 1,372,360 | refused: exit 110 (ProofNodeMismatch) |
| 5 | bad proof (a storage-proof byte flipped) | holdings Check 111,180 → Attest 248,894 | 360,074 | 7% | 7,493 | 2,940,536 | refused: exit 110 (ProofNodeMismatch) |
| 6 | bad proof (an account-proof byte flipped) | holdings Check 111,180 → Attest 52,872 | 164,052 | 3% | 7,493 | 1,372,360 | refused: exit 110 (ProofNodeMismatch) |
| 7 | mismatched value (minimum = balance + 1 wei) | holdings Check 111,180 → Attest 254,701 | 365,881 | 7% | 7,493 | 2,986,992 | refused: exit 130 (PredicateFailed) |
| 8 | mismatched holder (the pair's proofs for another address) | holdings Check 111,180 → Attest 187,363 | 298,543 | 6% | 7,493 | 2,448,288 | refused: exit 110 (ProofNodeMismatch) |
| 9 | wrong actor (a check sent to another holder's actor) | holdings Check 36,215 | 36,215 | 1% | 7,493 | 349,664 | refused: exit 232 (holdings: WrongReceiver) |
| 10 | accepted: the pair held ≥ 1,000 WETH | holdings Check 111,180 → Attest 279,023 → holdings (reply) 41,925 | 432,128 | 9% | 7,493 | 3,516,968 | applied |
| 11 | accepted: minimum = the balance exactly | holdings Check 111,180 → Attest 279,083 → holdings (reply) 41,985 | 432,248 | 9% | 7,493 | 3,517,928 | applied |
| 12 | stale: the same block again | holdings Check 109,689 → Attest 279,023 → holdings (reply) 38,145 | 426,857 | 9% | 7,493 | 3,474,800 | refused: exit 239 (holdings: Stale) |

## Handlers

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| wrong chain (a gate for Sepolia) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| wrong chain (a gate for Sepolia) | Attest | 39,317 | 1 | 7,443 | 6,940 | 365 | 0 | – | 140 |
| unfinalized block (the attested, not finalized, header) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| unfinalized block (the attested, not finalized, header) | Attest | 31,333 | 1 | 7,078 | 6,940 | 0 | 0 | – | 213 |
| unknown block (no record at this slot) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| unknown block (no record at this slot) | Attest | 31,333 | 1 | 7,078 | 6,940 | 0 | 0 | – | 213 |
| proof of another block (the bootstrap's record) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| proof of another block (the bootstrap's record) | Attest | 52,872 | 1 | 7,443 | 6,940 | 365 | 0 | – | 110 |
| bad proof (a storage-proof byte flipped) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| bad proof (a storage-proof byte flipped) | Attest | 248,894 | 1 | 7,443 | 6,940 | 365 | 0 | – | 110 |
| bad proof (an account-proof byte flipped) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| bad proof (an account-proof byte flipped) | Attest | 52,872 | 1 | 7,443 | 6,940 | 365 | 0 | – | 110 |
| mismatched value (minimum = balance + 1 wei) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| mismatched value (minimum = balance + 1 wei) | Attest | 254,701 | 1 | 7,443 | 6,940 | 365 | 0 | – | 130 |
| mismatched holder (the pair's proofs for another address) | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| mismatched holder (the pair's proofs for another address) | Attest | 187,363 | 1 | 7,443 | 6,940 | 365 | 0 | – | 110 |
| wrong actor (a check sent to another holder's actor) | holdings Check | 36,215 | 1 | 7,040 | 6,966 | 0 | 0 | – | 232 |
| accepted: the pair held ≥ 1,000 WETH | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| accepted: the pair held ≥ 1,000 WETH | Attest | 279,023 | 1 | 7,443 | 6,940 | 365 | 7,906 | – | 0 |
| accepted: the pair held ≥ 1,000 WETH | holdings (reply) | 41,925 | 1 | 750 | 370 | 242 | 940 | 165 | 0 |
| accepted: minimum = the balance exactly | holdings Check | 111,180 | 1 | 7,040 | 6,966 | 0 | 14,319 | 242 | 0 |
| accepted: minimum = the balance exactly | Attest | 279,083 | 1 | 7,443 | 6,940 | 365 | 7,906 | – | 0 |
| accepted: minimum = the balance exactly | holdings (reply) | 41,985 | 1 | 750 | 370 | 242 | 940 | 165 | 0 |
| stale: the same block again | holdings Check | 109,689 | 1 | 7,205 | 6,966 | 165 | 14,492 | 250 | 0 |
| stale: the same block again | Attest | 279,023 | 1 | 7,443 | 6,940 | 365 | 7,906 | – | 0 |
| stale: the same block again | holdings (reply) | 38,145 | 1 | 758 | 370 | 250 | 0 | – | 239 |
