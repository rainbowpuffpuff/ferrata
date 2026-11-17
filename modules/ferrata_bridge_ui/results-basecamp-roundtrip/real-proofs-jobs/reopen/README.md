# R2, first run: the view unloaded and loaded again in Basecamp while the burn was proven (`seqnet-basecamp-reopen-real`)

17 Nov 2026, 15:57–16:17 UTC, the Modal build box; the module and the view of commit `48a3b13d` (as R1), the driver of
`f7cfee48` with `BASECAMP_REOPEN=1` and `SEQNET_PROOFS=real`. The chains' side:
[`e2e/runs/seqnet-basecamp-reopen-real/`](../../../../../e2e/runs/seqnet-basecamp-reopen-real/SUMMARY.md).

**The job outlived the view.** 6 s into the burn's real proof (job 2, "proving"), the driver unloaded `ferrata_bridge_ui`
with its toggle in Basecamp's Apps Inspector (16:12:32; no instance of the view left), loaded it again (16:12:33: a new
instance, not connected, its call log new) and opened it. The new view listed the module's jobs when it opened and
found job 2 still proving (8,740 ms in); it showed the proof being made, kept the burn disabled, polled the job, and
applied its reply when it settled: the proof 223,483 ms, LEZ block 95, the burn's own release delay (3,600,000 ms)
holding the release, which the relayer then paid (997,000,000,000,000 wei to the fresh address, one `Release` log).
13 of 13 steps; the claim (job 1) proven in 242,813 ms, LEZ block 72. The views' calls: the first 106 (84 job polls,
the longest 3,186 ms: its `jobs` at opening), the reopened one 85 (73 polls, the longest 86 ms); none cut.

**What it found**: one call of the reopened view was refused. Opening, the view applied the finished claim job (which
refreshes the positions) before it had adopted the running burn job, so it sent a `scan`, and the module refused it at
once ("busy: job 2 (the private burn) is proving since 8 s …", 1 ms; `view-calls.json`, call 1). The next call cleared
the banner, so picture 17d shows none. Fixed in `3b7ffe1d` (the view adopts both jobs before applying either), and the
driver now checks that the reopened view's calls are all answered (the run on the fixed view: `../final/`). The
reopened view had not read the registry yet (the view reads it on its Registry tab), so its burn button reads "Burn –".

Pictures ([`pictures/`](pictures/)): 17b the burn's job proving in the first view; 17c the Apps Inspector with the view
unloaded ("NOT LOADED", Load); 17d the reopened view on the job, "15 s so far"; 18 settled; 19 the release held by the
job's delay; 21 released. Records: `events.jsonl`, `view-calls-before-reopen.json` (the first view),
`view-calls.json` (the reopened view), `jobs.json`, `chain-checks.jsonl`, `chain-calls.jsonl`, `lez-report.json`,
`session.json`, `summary.json`, `basecamp-run.log`, `basecamp-versions.txt`.
