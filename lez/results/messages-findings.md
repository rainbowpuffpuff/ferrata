# The review's program findings on v1 (live) and v2 (harness/tests/messages_findings.rs)

Measured in LEZ's state machine (RISC0_DEV_MODE executor), the same scenario on both programs. Every check fails on `ferrata_msg` (v1) and passes on `ferrata_msg_v2`.

| check | v1 | v2 |
|---|---|---|
| M2 send between near-full inboxes fits the send limit | 1550595 cycles > 1500000 (limit at review); GAS_V1.send 2000000 | 219172 cycles <= GAS_V2.send 600000 |
| M3 owner's SetPolicy without / with 32 KiB of strangers' messages | 9292 -> 338022 (36.4x) | 9092 -> 9092 (1.00x) |
| M3 owner's own send without / with 32 KiB of strangers' messages | 34758 -> 388388 (11.2x) | 74798 -> 74798 (1.00x) |
| L6 hybrid envelope in 512 B refused | stored: true | stored: false |
| L7 at most 30 sends within any hour | 59 landed in 90 s | 30 landed in 90 s |
| L8 accounts to occupy every stranger slot | 4 | 8 |
