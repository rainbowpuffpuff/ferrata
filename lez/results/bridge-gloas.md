# The bridge program under a record in the Gloas layout (`harness/tests/bridge_inbound.rs`)

Bridge program image `66722fc7681beaeaacdfce978ae95ba9ba9f2b576b87791d523d6cd10815f4fa`; the record binds the hash of a real Sepolia header (block 12103422) whose state root is replaced by the synthetic state's; every transaction settled by LEZ's own functions in a block of its own.

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Init C, Gloas record, no header | bridge Init 40,700 → Attest 19,520 | 60,220 | 1% | 1,780 | 496,000 | refused: exit 215 (HeaderRequired) |
| 2 | Init C, Gloas record, another block's header | bridge Init 48,775 → Attest 35,677 | 84,452 | 2% | 2,507 | 695,672 | refused: exit 120 (HeaderHashMismatch) |
| 3 | Init C, Gloas record, header in front of the account proof | bridge Init 48,775 → Attest 110,361 → bridge (reply) 33,174 | 192,310 | 4% | 2,507 | 1,558,536 | applied |
| 4 | ImportRoot C, Gloas record, no header | bridge ImportRoot 38,847 → Attest 21,092 | 59,939 | 1% | 1,757 | 493,568 | refused: exit 215 (HeaderRequired) |
| 5 | ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot 47,321 → Attest 130,799 → bridge (reply) 44,676 → bridge WriteRoot 17,859 | 240,655 | 5% | 2,484 | 1,945,112 | applied |
| 6 | Claim: leaf 0 under root(C, 2) | bridge Claim 109,271 → bridge Spend 17,756 → bridge Mint 15,405 | 142,432 | 3% | 1,740 | 1,153,376 | applied |
| 7 | Claim: leaf 0 again (replay) | bridge Claim 109,271 → bridge Spend 15,210 | 124,481 | 2% | 1,740 | 1,009,768 | refused: exit 250 (ALREADY_CLAIMED) |

## Handler executions

| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Init C, Gloas record, no header | bridge Init | 40,700 | 1 | 1,327 | 1,253 | 0 | 2,804 | 266 | 0 |
| Init C, Gloas record, no header | Attest | 19,520 | 1 | 1,473 | 1,114 | 221 | 0 | – | 215 |
| Init C, Gloas record, another block's header | bridge Init | 48,775 | 1 | 2,054 | 1,980 | 0 | 4,222 | 266 | 0 |
| Init C, Gloas record, another block's header | Attest | 35,677 | 1 | 2,164 | 1,805 | 221 | 0 | – | 120 |
| Init C, Gloas record, header in front of the account proof | bridge Init | 48,775 | 1 | 2,054 | 1,980 | 0 | 4,222 | 266 | 0 |
| Init C, Gloas record, header in front of the account proof | Attest | 110,361 | 1 | 2,164 | 1,805 | 221 | 2,627 | – | 0 |
| Init C, Gloas record, header in front of the account proof | bridge (reply) | 33,174 | 1 | 774 | 370 | 266 | 1,058 | 259 | 0 |
| ImportRoot C, Gloas record, no header | bridge ImportRoot | 38,847 | 1 | 1,564 | 1,231 | 259 | 3,280 | 307 | 0 |
| ImportRoot C, Gloas record, no header | Attest | 21,092 | 1 | 1,671 | 1,312 | 221 | 0 | – | 215 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge ImportRoot | 47,321 | 1 | 2,291 | 1,958 | 259 | 4,698 | 307 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | Attest | 130,799 | 1 | 2,362 | 2,003 | 221 | 2,942 | – | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge (reply) | 44,676 | 1 | 932 | 487 | 307 | 1,505 | 259 | 0 |
| ImportRoot C, Gloas record, with the header (2 leaves) | bridge WriteRoot | 17,859 | 1 | 323 | 185 | 0 | 533 | 185 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Claim | 109,271 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Spend | 17,756 | 1 | 356 | 218 | 0 | 687 | 57 | 0 |
| Claim: leaf 0 under root(C, 2) | bridge Mint | 15,405 | 1 | 315 | 177 | 0 | 490 | 150 | 0 |
| Claim: leaf 0 again (replay) | bridge Claim | 109,271 | 1 | 1,473 | 1,214 | 185 | 1,816 | – | 0 |
| Claim: leaf 0 again (replay) | bridge Spend | 15,210 | 1 | 413 | 218 | 57 | 0 | – | 250 |
