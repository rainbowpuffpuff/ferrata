# The relayer's root imports with the LEZ side failing (RFP-022 R.06)

**Run:** `bash e2e/netfault/relayer-lez.sh OUT` on a Modal build box (CPU), 21 November 2026, the sources of this
directory's commit. **Result: 9 passed, 0 failed** ([`summary.md`](summary.md), [`results.json`](results.json)); the
daemon's JSON lines per round in [`rounds/`](rounds/), the proxy's log of every LEZ call in
[`proxy.jsonl`](proxy.jsonl).

**Where it ran:** the mini-app's devnet (`cli/examples/miniapp_devnet.rs`: FerrataVault v5 on a local anvil, chain id
1; the zone in the LEZ v0.4.0-rc1 harness with the bridge program, RISC Zero dev mode; the light client's record of
anvil's block is the devnet's stand-in), `ferrata-relayer --import-roots --lez-devnet`, every LEZ call of the daemon
through [`e2e/netfault/holdproxy.py`](../../netfault/holdproxy.py), whose faults the script set as it went.

| Scenario | Fault | What the daemon did | Zone after |
|---|---|---|---|
| A | outage: every LEZ call answered 503 or closed | the round's import failed (`root-import-failed`), nothing sent; the next round, endpoint back, imported n = 2 | imported `[2]` |
| B | the ImportRoot forwarded and applied, its answer lost (connection closed) | kept it in flight (`keptInFlight`); the next round found `root(cid, 3)` holding what it had sent and booked it (`"recovered": true`), **no second ImportRoot sent** | `[2, 3]`, one more call applied |
| C | SIGKILL while the ImportRoot's answer was held (the import at the zone) | restarted: booked from its record (`recovered`), **nothing sent again**; SIGTERM: a clean `stop` line | `[2, 3, 4]` |
| D | SIGKILL while the ImportRoot was trapped, never forwarded | restarted with `--import-wait 3`: waited out its record (`root-import-pending`, then `root-import-expired`), then imported n = 5 once | `[2, 3, 4, 5]` |

The proxy's log shows each import sent once (A, B, C) and D's trapped call never forwarded, its import sent once
after; the zone's applied-call count rose by exactly one per scenario.
