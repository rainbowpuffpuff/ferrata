# L4 measured: real proofs inside Basecamp, the view's calls cut at 20 s

`e2e/seqnet/run.sh basecamp basecamp-real-1` with `SEQNET_PROOFS=real`, 16 Nov 2026, 17:09–17:29 UTC, on the same box,
Basecamp, modules and driver as [`../README.md`](../README.md) (the driver's commit `1a5d22ba`). The sequencer verified
every receipt; `lez_core` proved without dev mode. The chains' side: [`e2e/runs/seqnet-basecamp-real-1/`](../../../../e2e/runs/seqnet-basecamp-real-1/SUMMARY.md)
(235 LEZ and 12 Ethereum transactions, none reverted).

| Step | What happened (`events.jsonl`, `chain-checks.jsonl`, `chain-calls.jsonl`) |
|---|---|
| 1–8 | as in the dev-mode run (the gas drop's shield proven for real: the step took 266 s) |
| private claim | the view's call **cut by Basecamp after 20 s**: `Logos host: Module source unavailable: call to 'ferrata_bridge_module.wallet' timed out after 20000ms` (Basecamp's log: `RemoteLogosObject: async callMethod timed out`); `lez_core` went on proving: the claim was **applied in LEZ block 73, 234.4 s after the cut**, private, not reverted, transaction 256,068 B (a real proof's size: row 114's); the view, refreshed, showed the deposit claimed |
| private burn | cut the same way after 20 s; **applied in LEZ block 97, 220.7 s after the cut**, 255,031 B |
| release | **not reached**: the view sets the release's random delay from the burn's reply, which never came, so the burn's release was not held; the driver's check "the release is held by its random delay" stopped the run |

The view told the user, at the cut, "The operation failed. Nothing was changed." ([`pictures/14b-claim-cut-by-the-host.png`](pictures/14b-claim-cut-by-the-host.png),
[`pictures/17b-burn-cut-by-the-host.png`](pictures/17b-burn-cut-by-the-host.png)). That was false: the claim and the
burn landed minutes later. The view now says "No answer in time: the step may still complete. Refresh your positions
before repeating it." (commit `edfb9162`; Basecamp's ui-tests 10 of 10 on it). It still loses the release delay of a
burn whose reply was cut. So real proofs inside Basecamp need the call not to be cut: the upstream change in
[`../README.md`](../README.md), "Not reached: L4", or the module's `claim` and `burn` as a job the view polls.
