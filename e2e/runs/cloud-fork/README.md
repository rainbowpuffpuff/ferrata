# The driver in the cloud, on a fork

`e2e/sepolia-cloud.sh start --fork cloud-fork` run to its `DONE` line in the cloud (the Modal app
`ferrata-sepolia`, [`../../cloud/app.py`](../../cloud/app.py)), from a clean copy of
[`../sepolia-live/`](../sepolia-live/): the runner started `anvil --fork-url <a public Sepolia endpoint>`
(fork block 12,106,352, a block every 2 s, senders impersonated: `FERRATA_FORK=1`, no key) and ran
`e2e/sepolia-live.sh cloud-fork` itself. Nothing ran on the laptop between `start` and `fetch`; `status` and
`fetch` were run during the build, during the GPU wait and after `DONE`.

| UTC, 10 Nov 2026 | |
|---|---|
| 18:09:49 | `start --fork cloud-fork` on the laptop: checks, upload, spawn; it returned at 18:10:53 |
| 18:11:01 | the runner has the repository at the laptop's path (commit `bbdacd0`) and the run directory from the Volume |
| 18:16:07 | built in 304 s: `ferrata-e2e`, the zone builder, the vault |
| 18:16:12–35 | pre-flight; `deploy` (10 transactions), `submit`, the wait passed by moving the fork's clock, `beacon` (block 12,106,366) |
| 18:16:36–54 | the finalisation input for that beacon's nonce (sha256 `17bcf0cd…7b64`) |
| 18:16:55–19:31:04 | the GPU job: 74 minutes in the queue, then 7.9 s on an NVIDIA L40S (`receipts/cp-e2e-finalize/gpu-run.json`) |
| 19:31:04–34 | `identity_p254` on the runner's CPU |
| 19:31:34–56 | RISC Zero's prover image |
| 19:31:57–19:32:07 | `finalize`, the release delay passed, `release` (2), `verify-sepolia.sh`: ALL CHECKS PASSED; `DONE` |

The `DONE` line: 15 transactions on chain 11155111, 10,898,492 gas, every status 1 (`manifest.json`), each
stage's gas equal to [`../fork-driver/`](../fork-driver/README.md)'s on the laptop: deploy 9,086,587, submit
507,370, beacon 77,823, finalize 475,300, releases 363,169 and 388,243. The finalisation was proven for this
fork's own beacon, so its input, seal and fixture are this run's.

The GPU wait: no L4 was free. At 18:46:55 and 19:16:55 the hook of that start cancelled its job and queued
it again, but a cancelled input keeps its place in the queue: an L4 at 19:07 and a GPU at 19:20 were
handed the two cancelled inputs and dropped them. At 19:18 `gpu_prove` was deployed again with an L40S as
the fallback (the same sm_89 binary); the third request ran on one. The hook now queues once and waits.

A first start at 18:00 stopped before its first transaction: the image had Foundry's 1.3.6 release, which
refuses `forge script --unlocked` while `ETH_FROM` is set. The image now has the laptop's 1.3.6-nightly
(same sha256 for forge, cast and anvil); the run's cloud state was deleted and this run started again from
the clean copy.

The hashes are those of a private fork: they exist on no public network.
