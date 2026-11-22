# `ferrata attest` on a running LEZ sequencer (run r4, 21 November 2026)

`bash cli/tests/attest-seq.sh r4` on the Modal build box, tree `4db5d1d2` (the commands of `cli/tests/attest-seq-hook.sh`).
A LEZ v0.4.0-rc1 zone we started (crown-live-3's sequencer, Logos Blockchain node and final programs; LEZ's local
configuration; dev-mode leader proofs), session A (`lez/crown/session.sh a`, `CROWN_CHAIN=sepolia`: the attestation
program `0x816d1fef…` deployed, the Sepolia instance `0xa5fb2591…` from `sepolia-2026-11-12`), then the zone again with
the hook. Every read from the zone's final blocks (Bedrock); every send waited for its block to be final; nothing sent
to or read from any public network (Ethereum: recorded data only, the mainnet proofs served by
`modules/scripts/recorded_rpc.py` from `mainnet-attest-2026-11-10`).

Files: `run.log` (each step: exit status, expected), `cli/NN-*.txt` (each command and its whole output),
`cli/summary.tsv`, `attest.jsonl` (70 records: every dry run, every sent transaction and its receipt), `cli/monitor-*.json`
(`attest-monitor` on the zone), `session-a.json`, `logs/session-*.log`, `binaries.sha256`.

| # | Command | Result on the zone (measured) |
|---|---|---|
| 1 | `config` of session A's Sepolia instance | chain 11,155,111, 512 keys, Sepolia's fork schedule, layout Gloas, bound 3,532 epochs, 2 jobs applied; the id derived again from the configuration and checkpoint `0x3b901f5e…` equal to `0xa5fb2591…` |
| 3–4 | `advance` (dry run only) with Sepolia's finality update of 12 Nov (the lab's record, entry 57) | verdict useful (advances); dry run: `Open` 2,074,542, `Miller` 3,305,374, `Finish` 3,721,523 execution gas, all applied; the instance unchanged afterwards (status: slot 11,562,112) |
| 5 | the same, `--send --yes` | blocks 26–28: each applied, **equal to its dry run** (exit, execution gas, storage gas, events); finalized slot 11,562,112 → 11,563,200 |
| 6 | `advance` with the period-1412 update (entry 64) | 5 transactions (`Open`, `Miller`, 2 × `Adopt` of 24,980 bytes, `Finish`), each applied and equal to its dry run; slot → 11,567,104 (period 1412) |
| 7 | the finality update of step 5 again | the program's step 1, natively: 23 `PeriodSkipped`; dry run: refused with 23; the zone: refused with 23 (75,458 gas), equal; exit 3 |
| 9 | `init` of a mainnet instance from `mainnet-attest-2026-11-10`'s bootstrap | `Init` 210,357 and two `Adopt` (3,399,141 and 3,485,575), each applied and equal; instance `0x599a0de5…` |
| 10–11 | `advance` with its finality update; `config` | 3 transactions applied and equal; finalized slot 15,624,480 (the record of block 26,384,851); the id derived again equal |
| 13–14 | `attest --storage WETH:balance-slot:eq:VALUE` (dry run, then sent) | verdict: 2 statements; dry run applied, 275,925 gas; the zone: applied, equal |
| 15 | the same with `eq:0x01` | verdict 130 `PredicateFailed`; dry run 130; the zone 130 (253,805 gas), equal; exit 3 |
| 16 | `ge:0x01` | applied (275,865), equal |
| 17–18 | `--log 0:0:emitter=USDC`; with WETH as the emitter | applied (94,795), equal; refused 130 (85,064), equal |
| 19 | `--account WETH:codehash` | applied (166,249), equal |
| 20 | `--log 37:0` (a failed transaction's log) | refused 124 `ReceiptFailed` (55,530), equal |
| 21 | storage and log in one request | applied (349,917), equal |
| 22 | the mainnet finality update again | step 1 natively: 52 `NotNewer`; dry run 52; the zone 52 (229,560), equal |
| monitor | `attest-monitor check --state lez:… --instance 0x599a0de5…` (two loopback providers serving publicnode's recorded header at slot 15,624,480) | at the record's time: divergence `confirmed`, no alert (exit 0); at the run's time: `ws-half` (the record is 11 days old: past half its weak-subjectivity period), exit 20 |

All 22 commands gave the expected exit status (the hook of this run checked the status, 0 or 3; the refusal codes in
the table are read from the outputs, where the CLI itself compares verdict, dry run and zone and exits 1 on a
difference); every sent transaction equalled its dry run in exit code, execution and storage gas and number of events.
The two monitor reads have no expectation. (After the review the hook expects each refusal's code: run `rf1`.)
