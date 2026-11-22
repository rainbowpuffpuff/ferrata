# A burn's tag is used once (`burn(cid, r)`, ADR 0032)

The bridge program records a burn at `burn(cid, r)`: a second note with a used tag is refused and its whole
transaction reverts, so every applied burn has its own release nullifier. `lez/harness/tests/bridge_burn_tag.rs`;
LEZ v0.4.0-rc1's state machine and fee settlement in the harness, RISC Zero dev mode.

Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`.

- A's burn with counter 0 applied (156678 execution gas)
- P1 / H-1: the second note with counter 0's tag (another holder, another recipient) refused with 244 ALREADY_WRITTEN; B's holding whole, A's debited once; burn(cid, r) holds the first note
- C (private), counter 1 again: the simulation of its public part is refused (code 244); nothing sent
- C (private), counter 1 proven before B's burn landed: settled reverted (Program exited with non-zero code 244 after 20873 cycles)
- C after it: the fee paid, the 1 ETH holding whole
- C (private), counter 2: applied (515479 execution gas, 224063 storage gas); its holding burnt once
- a copy of A's pending tag (counter 3), applied first, made A's burn revert (244); A's holding whole; A burnt again with counter 4
- the wallet finds its burns with counters [0, 1, 2, 4], 4 distinct NOut, next counter 5

| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |
|---|---|---|---:|---:|---:|---:|---|
| 1 | Burn 1 ETH by A, counter 0, to e1 | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 19,498 | 156,678 | 3% | 839 | 1,260,136 | applied |
| 2 | Burn 1 ETH by B, the same counter 0, to e2 (H-1: in flight from two holders) | bridge Burn 22,693 → bridge Burn 27,848 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 20,873 | 157,631 | 3% | 1,015 | 1,269,168 | refused: exit 244 (ALREADY_WRITTEN) |
| 3 | Burn 0.1 ETH by A with counter 0's tag again (another amount) | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,217 → bridge Admit 35,963 → bridge Burn 20,873 | 158,016 | 3% | 839 | 1,270,840 | refused: exit 244 (ALREADY_WRITTEN) |
| 4 | Burn: A's first note again | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 20,873 | 158,053 | 3% | 839 | 1,271,136 | refused: exit 244 (ALREADY_WRITTEN) |
| 5 | Burn 1 ETH by B, counter 1, to e2 | bridge Burn 22,693 → bridge Burn 27,848 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 19,498 | 156,256 | 3% | 1,015 | 1,258,168 | applied |
| 6 | The copier burns 1 ETH under A's pending tag (counter 3) first | bridge Burn 22,693 → bridge Burn 27,848 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 19,498 | 156,256 | 3% | 1,015 | 1,258,168 | applied |
| 7 | A's pending burn with counter 3, after the copy | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 20,873 | 158,053 | 3% | 839 | 1,271,136 | refused: exit 244 (ALREADY_WRITTEN) |
| 8 | A burns again with its next counter (4) | bridge Burn 22,693 → bridge Burn 28,270 → bridge Burn 50,254 → bridge Admit 35,963 → bridge Burn 19,498 | 156,678 | 3% | 839 | 1,260,136 | applied |
