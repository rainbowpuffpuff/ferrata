# R1 and R2 on the final code: real proofs inside Basecamp, the view reopened while the burn was proven (`seqnet-basecamp-final-real`)

17 Nov 2026, 16:31–16:49 UTC, the Modal build box; the module and the view of commit `3b7ffe1d` (the final code: the
module as in `48a3b13d`, the view with the reopening fix found by the first R2 run, [`../reopen/`](../reopen/README.md)),
`SEQNET_PROOFS=real`, `BASECAMP_REOPEN=1`. The chains' side:
[`e2e/runs/seqnet-basecamp-final-real/`](../../../../../e2e/runs/seqnet-basecamp-final-real/SUMMARY.md).

**The whole private round trip with real proofs inside Basecamp, and the view closed and opened again while the burn
was being proven: 13 of 13 steps, no call of the view cut, none refused.**

| | Job 1, the private claim | Job 2, the private burn |
|---|---|---|
| started by the view's call | answered at once (1 ms) | answered at once (1 ms), with `release_delay_ms` 7,200,000 (2 h) |
| phases (ms after the start) | queued 0, proving 50, sent 245,958, settled 254,123 | queued 0, proving 33, sent 229,065, settled 235,229 |
| the proof (LEZ's wallet inside `lez_core`, in Basecamp) | 245,908 ms, receipt 239,532 B | 229,032 ms, receipt 239,046 B |
| applied by the sequencer, verifying | LEZ block 73, transaction 256,068 B, private, not reverted | LEZ block 97, 255,031 B, private, not reverted |
| while it proved | the claim button disabled ("Proof being made (job 1)"); a second `claim` answered the job (`started: false`, 7 ms); a `scan` refused at once (6 ms) | the same (8 ms, 6 ms); then the view was closed and opened again (below) |

**The reopening (R2)**, 6 s into the burn's proof: the driver unloaded `ferrata_bridge_ui` with its toggle in Basecamp's
Apps Inspector (16:44:23, no instance of the view left; picture 17c), loaded it again (16:44:24: a new instance, not
connected, its call log new) and opened it. The new view listed the module's jobs as it opened and found job 2
proving (8,753 ms in), and none of its calls was refused (checked: all answered, no error in its banner). It showed the proof being made ("15 s so far", picture 17d), polled the job, and when it settled
applied its reply: the burn's text and the release held from the burn's settlement for the job's own delay (7,200,000
ms), which the view's clock then passed (as the user waiting). The checkpoint followed, and the relayer released
997,000,000,000,000 wei to the fresh address (one `Release` log).

**The view's calls**: the first instance 109 (87 job polls of 0–5 ms; the longest 3,252 ms, its `jobs` as it opened,
answered once Basecamp had loaded the core module), the reopened one 87 (76 polls; the longest 88 ms); 196 in all,
none cut by Basecamp's 20 s, none an error ([`view-calls-before-reopen.json`](view-calls-before-reopen.json),
[`view-calls.json`](view-calls.json)).

**Pictures** ([`pictures/`](pictures/), Basecamp's window at every step, 21–66 kB each): 14b and 17b a job proving,
15 and 18 settled, 17c the view unloaded in the Apps Inspector, 17d the reopened view on the running job, 19 the
release held by the job's delay, 21 released, and every other step (01–13, 14, 16, 17, 20, 22, 23). All data in them
is the run's own. The reopened view had not read the registry (the view reads it on its Registry tab), so its burn
button reads "Burn –".

**What is real here, and what is not**: as in R1 ([`../README.md`](../README.md)): the claim and burn proven for real
inside Basecamp and verified by the sequencer; Ethereum a local anvil finalized for LEZ by a stand-in committee, the
vault's verifier the mock, Ethereum's reads, signing and sending through `seqnet-chain`, the gas drop, the operator's
import and the checkpoint by `ops.sh`, the session opened by the test, `lez_core` patched, the user's waiting the
view's clock moved on. The view was closed by Basecamp's own unload of the ui module, not by a person closing a window.

**Which code**: `3b7ffe1d`; the module's `.lgx` rebuilt from it on the box (`modules/scripts/check.sh`: 0 step and 0
check failures), byte for byte the one of the first R1 run (sha256 `f19d92e4…99853d`, `basecamp-versions.txt`); the
view's install root built from `3b7ffe1d`; Basecamp `b642a9a`; `lez_core`
`cc73ebf` patched. Command: `SEQNET_TMP=$T BASECAMP_BUILD=<basecamp-build> SEQNET_PROOFS=real BASECAMP_REOPEN=1 bash
e2e/seqnet/run.sh basecamp basecamp-final-real` after `check.sh` and `basecamp-build.sh` as in [`../README.md`](../README.md).

Records: `events.jsonl`, `chain-checks.jsonl`, `chain-calls.jsonl`, `jobs.json`, `lez-report.json`, `session.json`,
`summary.json`, `basecamp-run.log`, `basecamp-versions.txt`, the two call logs, `pictures/`.
