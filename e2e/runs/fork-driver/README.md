# The driver on a local fork

`e2e/sepolia-live.sh fork-driver` run to its `DONE` line on `anvil --fork-url <a public Sepolia endpoint>`
(fork block 12,105,709, a block every 2 s, the deployer impersonated: `FERRATA_FORK=1`, no key, no env
file), from a clean copy of [`../sepolia-live/`](../sepolia-live/). The script was started four times;
each start read the chain and continued after the last complete stage:

| Start (UTC) | What that start did |
|---|---|
| 16:11:11 | pre-flight; `deploy` (10 transactions), `submit`; stopped after the beacon transaction was sent, before the stage was marked complete |
| 16:12:10 | found deploy, submit and beacon complete on the chain; built the finalisation input for the beacon's nonce; queued the GPU proof (16:12:43); stopped while it was queued |
| 16:29:20 | found that job still on the GPU app and waited for it instead of queuing a second one; stopped with the agent that ran it |
| 16:54:29 | the job had waited 42 min without being scheduled on an L4, and its app had been stopped by hand: queued it again (16:55:47), proof at 17:00:28, then `finalize`, the release delay, `release` (2), `verify-sepolia.sh`; `DONE` at 17:01:19 |

The `DONE` line: 15 transactions on chain 11155111, 10,898,492 gas, every status 1 (`manifest.json`):
deploy 9,086,587, submit 507,370, beacon 77,823, finalize 475,300, releases 363,169 and 388,243. The
finalize costs 12 gas less than in [`../fork/`](../fork/README.md) (475,312): another seal, other calldata.
The finalisation input was built three times for the same beacon, with the same sha256 each time
(`1d0a14de…12b9`); its proof took 15.6 s on one NVIDIA L4 (`receipts/cp-e2e-finalize/gpu-run.json`), and
the two wrap steps 22 s and 17 s between the driver's log lines. On the fork the driver moves the clock (`evm_increaseTime`) for the candidate's
wait and the release delay; the beacon is anvil's random `prevrandao`, not Ethereum's.

Also tried on a third fork, not kept: a `deploy` stopped after 4 of its 10 transactions was finished on
the next start from forge's record (`--resume`), with the same addresses.

The hashes are those of a private fork: they exist on no public network.
