# The monitor's `--send-via logos` fallback (RFP-022 R.06; third review, M-1)

`bash tests/logos-fallback.sh` from `monitor/`, on the Modal build box (CPU), 22 November 2026, local anvil only.
What it shows is in the script's header; the checks and the table are in [`summary.md`](summary.md).

| Run | Code | Result |
|---|---|---|
| this directory | the branch at `b1069ddf` with the script's fix to its own helpers (this commit) | **8 passed, 0 failed**: L1 the module's broadcast failed after its leg left (nonce 0 burnt): the cancel went the plain way and was mined at nonce 0; L2 through the module at 1; L3 stuck with nothing sent (2 burnt): plain, mined at 2; L4 through the module at 3; the watcher's account at 4, nothing pending |
| [`before-fix/`](before-fix/) | the same, with `monitor/src/chain.rs` as at `83cf4eb0` (an unknown outcome an error, never the fallback) | **1 passed, 7 failed**: no cancel mined in any case; after L1's failed request (nonce 0 burnt) every request took the module's next number, behind the gap at 0 (`txsender.jsonl`: 30 requests, as each poll tries every candidate still pending, 3, 6, 9 and 12 in the four cases; nonces 0 to 29; 28 broadcast and held by anvil, never mined, `not mined within 8s`; one failed, one stuck); a `cancel-failed` alert for each candidate (paged once per 5 minutes of chain time); the watcher's account still at nonce 0 |

The stand-in is `e2e/netfault/txsender_stub.py` (its `fail send_status` fault follows `tx_sender_module` f1754c3:
`glue.rs` `advance_send` leaves the row `unknown` and settles `failed` after `leaving` set the leg's `left`; `send.rs`
`reserve` never hands out a spent number). The real module, Logos Core and `ferrata_monitor_module` are not in this
run. The node view's path in the alerts is written as `<checkout>` (as in `anvil-e2e/`) and, for `before-fix/`,
`<scratch>`; the run's logs (`run/`) stay on the box.
