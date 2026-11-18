# e2e/netfault results

One JSON line per seed (`check.py`): outcome (`complete`, `stopped-transient`, `stopped-permanent`), the stage it
stopped at, transient stops resumed (as `drive.py` does, up to 4), stage reruns (`*.transient-N` logs), transactions on
chain and journaled, seconds, the faults injected by kind, the proxy's events, the effects checked (deposits,
nullifiers used, wei left in the vault) and the violations of the invariants (`[]` when none). Times: each batch's end
is the mtime of its file as mrun copied it back, its start that minus mrun's own duration (commit 597b9f36's message
says 16:33-16:43Z for batch 1: estimated, these are measured).

| File | Where, when | Code | Command | Result |
|---|---|---|---|---|
| `b1-budget90.jsonl` | build box (Modal), forge/anvil 1.3.6, 18 Nov 2026 16:31-16:41Z | the tree at `8c9c3500` (commit `597b9f36`'s message says 37654d2c: wrong, 8c9c3500 includes it) | `bash e2e/netfault/many.sh 1 24 4` (stage budget 90 s) | 24 of 24 complete, 0 violations |
| `b2-budget30.jsonl` | build box, forge/anvil 1.3.6, 18 Nov 2026 17:39-17:48Z | `adbc42b4` | `NETFAULT_BATCH=b2-budget30 NETFAULT_STAGE_BUDGET_S=30 bash e2e/netfault/many.sh 25 48 4` (a 30 s stage budget: outages become transient stops) | 24 of 24 complete, 0 violations; 3 transient stops (exit 75: seeds 38, 45, 47) resumed and completed |
| `b3-nightly-a4d66e8e.jsonl` | build box, the crown runner image's Foundry (nightly a4d66e8e = 1.3.6-nightly), 18 Nov 2026 17:48-17:50Z | `3eb4b360` | `bash e2e/netfault/many.sh 49 52 4` | 4 of 4 complete, 0 violations |
| `b4a-aborted-f1f20f35.jsonl` | build box, nightly a4d66e8e, 17:50:35-18:15:42Z, ended by mrun's 25 min limit | `f1f20f35` | `NETFAULT_STAGE_BUDGET_S=30 bash e2e/netfault/many.sh 53 60 4` | **found two defects**, 0 violations: seeds 53, 55, 56, 59, 60 complete; 57 stopped (permanent) in deploy, as `cast tx` of a mined transaction asked only the lagging endpoint; 54 hung in deposit 0 (its broadcast reached no mempool during an outage, and forge waited 23 min for the receipt); 58 never started. Fixed in `067519c7` |
| `b4-final-nightly-budget30.jsonl` | build box, nightly a4d66e8e, ended 18:24:30Z (337 s) | `067519c7` | `NETFAULT_STAGE_BUDGET_S=30 bash e2e/netfault/many.sh 53 64 4` | 12 of 12 complete, 0 violations; 2,052 faults; seed 64 one transient stop, resumed |
