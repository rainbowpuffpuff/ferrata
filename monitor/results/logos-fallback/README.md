# The monitor's `--send-via logos` fallback (RFP-022 R.06; third review, M-1; the gap fill of 26 November 2026)

`bash tests/logos-fallback.sh` from `monitor/`, local anvil only. What it shows is in the script's header; the checks
and the table are in [`summary.md`](summary.md).

| Run | Code | Result |
|---|---|---|
| this directory (26 November 2026, a laptop, anvil 1.5.1) | `monitor/src/chain.rs` with the gap fill (`gap_to_fill`: a transaction the module broadcast, not mined within `--tx-timeout`, above the account's pending nonce: the same call sent the plain way at that nonce) | **10 passed, 0 failed**: L1 to L4 as below; L5 the module burnt nonce 4 and the plain send after it failed (the account empty for that poll): nothing mined; then, funded again, the module's request took 5, held by anvil behind the gap, and the cancel went the plain way at 4 (`via.gapFill`), mined, the held request mined after it; the watcher's account at 6, nothing pending or queued |
| (26 November 2026, the Modal build box, anvil 1.3.6, CI's) | this tree, the script mining one block before its last check (on 1.3.6 the module's held request, made ready by the gap fill, waits for the next block; 1.5.1 had included it at once) | **10 passed, 0 failed** (without that block: 9 passed, the last check seeing the held request pending, not mined); 10 of 10 again on the laptop's anvil 1.5.1 |
| [`before-gap-fix/`](before-gap-fix/) (the same day and machine) | `monitor/src/chain.rs` as before the gap fill (this tree's parent commit) | **8 passed, 2 failed**: L1 to L4 pass; L5's cancel is never mined: each poll's request queued behind nonce 4, given up after `not mined within 8s` (the account at 4, three requests queued) |
| (22 November 2026, the Modal build box) | the branch at `60bef60c` with the script's fix to its own helpers | **8 passed, 0 failed** (the script then had no L5): L1 the module's broadcast failed after its leg left (nonce 0 burnt): the cancel went the plain way and was mined at nonce 0; L2 through the module at 1; L3 stuck with nothing sent (2 burnt): plain, mined at 2; L4 through the module at 3; the watcher's account at 4, nothing pending |
| [`before-fix/`](before-fix/) | the same, with `monitor/src/chain.rs` as at `bff8b821` (an unknown outcome an error, never the fallback) | **1 passed, 7 failed**: no cancel mined in any case; after L1's failed request (nonce 0 burnt) every request took the module's next number, behind the gap at 0 (`txsender.jsonl`: 30 requests, as each poll tries every candidate still pending, 3, 6, 9 and 12 in the four cases; nonces 0 to 29; 28 broadcast and held by anvil, never mined, `not mined within 8s`; one failed, one stuck); a `cancel-failed` alert for each candidate (paged once per 5 minutes of chain time); the watcher's account still at nonce 0 |

The stand-in is `e2e/netfault/txsender_stub.py` (its `fail send_status` fault follows `tx_sender_module` f1754c3:
`glue.rs` `advance_send` leaves the row `unknown` and settles `failed` after `leaving` set the leg's `left`; `send.rs`
`reserve` never hands out a spent number). The real module, Logos Core and `ferrata_monitor_module` are not in this
run. The node view's path in the alerts is written as `<checkout>` (as in `anvil-e2e/`) and, for `before-fix/`,
`<scratch>`; the runs' logs (`run/`) were not kept.
