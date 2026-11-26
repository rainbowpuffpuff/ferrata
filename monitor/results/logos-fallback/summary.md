# monitor --send-via logos: the fallback for what tx_sender_module cannot account for (2026-11-26T09:41:53Z)

anvil Version: 1.5.1-stable, chain id 31337; vault 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 (rule B, wait 1 h), watcher 0x31b5F9Db2288bD4Be3dC3729b78Aad7E785FA9B8 (its key held by
the monitor, so the fallback is on; the stand-in sends from the same address). Each case: one candidate whose tip
conflicts with the node's final tier, then `monitor run` for 3 polls (1 s apart) through the stand-in.

- PASS L1 the fault fired: tx_sender_module answered failed, no hash, its leg left at nonce 0 (burnt)
- PASS L1 cancelled the plain way (the fallback, its reason the module's failed request): the candidate gone
- PASS L1 the plain cancel was mined at the nonce the module burnt (0): the gap filled
- PASS L1 one request through the module, no cancel-failed alert
- PASS L2 cancelled through tx_sender_module at nonce 1, mined
- PASS L3 the fault fired (stuck, nothing sent, nonce 2 burnt); cancelled the plain way at that nonce
- PASS L4 cancelled through tx_sender_module at nonce 3, mined
- PASS L5 the module burnt nonce 4 (failed, nothing sent) and the plain send after it failed (the account empty): the candidate still pending, nothing mined
- PASS L5 then the module's next request (5) was held behind the gap, and the cancel went the plain way at the burnt nonce 4, mined: the gap filled, the candidate gone
- PASS the watcher's account: 6 transactions mined (L5's held request after the gap fill), nothing pending or queued behind a gap (latest 6, pending 6, queued 0)

| Case | Fault armed | Requests to the module | Request's nonce | The cancel went | Its nonce (mined) | cancel-failed alerts |
|---|---|---|---|---|---|---|
| L1 | `fail send_status` | 1 | 0 | plain (fallback) | 0 | 0 |
| L2 | `pass` | 1 | 1 | logos | 1 | 0 |
| L3 | `stuck-unsent send_status` | 1 | 2 | plain (fallback) | 2 | 0 |
| L4 | `pass` | 1 | 3 | logos | 3 | 0 |
| L5 | `fail send_status`, the account empty for one poll; then `pass` | 2 | 4, 5 | plain (gap fill) | 4 | 1 |

10 passed, 0 failed.
