# monitor --send-via logos: the fallback for what tx_sender_module cannot account for (2026-11-22T04:30:27Z)

anvil Version: 1.3.6-v1.3.6, chain id 31337; vault 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 (rule B, wait 1 h), watcher 0x794E22Ef856BbfF71C081CcE19ac6bfdAD4Ed58c (its key held by
the monitor, so the fallback is on; the stand-in sends from the same address). Each case: one candidate whose tip
conflicts with the node's final tier, then `monitor run` for 3 polls (1 s apart) through the stand-in.

- PASS L1 the fault fired: tx_sender_module answered failed, no hash, its leg left at nonce 0 (burnt)
- FAIL L1 cancelled the plain way (the fallback, its reason the module's failed request): the candidate gone
- FAIL L1 the plain cancel was mined at the nonce the module burnt (0): the gap filled
- FAIL L1 one request through the module, no cancel-failed alert
- FAIL L2 cancelled through tx_sender_module at nonce 1, mined
- FAIL L3 the fault fired (stuck, nothing sent, nonce 2 burnt); cancelled the plain way at that nonce
- FAIL L4 cancelled through tx_sender_module at nonce 3, mined
- FAIL the watcher's account: 4 transactions mined, nothing pending behind a gap (latest 0, pending 0)

| Case | Fault armed | Requests to the module | Request's nonce | The cancel went | Its nonce (mined) | cancel-failed alerts |
|---|---|---|---|---|---|---|
| L1 | `fail send_status` | 3 | 0 | none | none | 1 |
| L2 | `pass` | 6 | 3 | none | none | 1 |
| L3 | `stuck-unsent send_status` | 9 | 9 | none | none | 1 |
| L4 | `pass` | 12 | 18 | none | none | 1 |

1 passed, 7 failed.
