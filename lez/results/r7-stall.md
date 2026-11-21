# R.7: a stalled instance keeps attesting and resumes without repair

`lez/harness/tests/stall.rs` (`a_stalled_instance_keeps_attesting_and_resumes_without_repair`), LEZ v0.4.0-rc1's state
machine in process (the harness), RISC Zero dev mode, real Sepolia data only. Run on the Modal build box on 21 November
2026 at commit `80a55d7d` (the test as committed then); 1 passed, 297.9 s. Run again at `bce25e02` (the comparison with
a second provider's roots asserted, `checked >= 2`, and the fixture's path printed relative to the repository): the same
output line for line but that path, 1 passed, 315.8 s, and the fixture it writes byte-identical to the committed one
(its output: [`r7-stall.txt`](r7-stall.txt)).

```sh
cd lez && RISC0_HOME=$PWD/../../risc0-home-lez RISC0_BUILD_LOCKED=1 RISC0_DEV_MODE=1 \
  cargo test -p harness --locked --test stall -- --nocapture
```

| Step | What happened (measured) |
|---|---|
| Follow | The Sepolia instance from the period-1378 bootstrap through the Gloas fork to the attestation fixture's finality update (`sepolia::Stage5`): record R at slot 11,549,664 (block 12,105,936, 10 Nov 2026). |
| Before the stall | Three attestations under R, each applied: a storage slot with a condition (USDC `totalSupply_` = its value, 279,157 execution gas, 2 statements), an ERC-20 `Transfer` log (97,794, 1 statement), Multicall3's code hash (170,523, 1 statement). |
| The stall | Nobody advances the instance for 706,020 s (8.2 days) while Sepolia moves on: the lab recorded 106 updates after R (`lez/lab/results/record/net/sepolia/updates/`), with the period updates of 1410 to 1417. R's weak-subjectivity period (3,532 epochs) ends at slot 11,662,720; the resume is at slot 11,608,565, inside it. |
| During the stall | At the resume's block time, the same three attestations under R: applied, the same statements and identifiers as before (`assert_eq!`), the instance's state unchanged by them. |
| No gap | The newest period's update (1417), sent first, is refused at `Open` with exit 23 `PeriodSkipped` (its signing committee is not known to the store); the instance is unchanged. |
| The resume | Every outstanding update in order, each checked natively first (as an updater does), every job at the same late block time: 106 jobs applied (334 transactions), none refused, none skipped; a committee adopted from the update of every period 1410 to 1417, in order, none missing; finalized slot 11,549,664 to 11,608,192, the newest recorded. No other instruction: no repair, no re-`Init`. Two records written on the resume equal a second provider's canonical roots (the `sepolia-2026-11-12` fixture's recorded headers; asserted, at least two, since `bce25e02`). |
| After the resume | Eight committee rotations later, R's three attestations again: applied, identical (R.3: an old record read after later rotations). |

The test also writes the instance's state at the stall and after the resume (`FERRATA_STALL_FIXTURES`), which the
attestation side's monitor's tests use: [`monitor/attest/tests/fixtures/stall.json`](../../monitor/attest/tests/fixtures/stall.json).
Not covered here: a stall past the weak-subjectivity period (the instance is then replaced, F.10: `tests/successor.rs`),
and a sequencer (this is the in-process state machine).
