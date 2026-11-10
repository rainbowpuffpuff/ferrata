# Rehearsal of sepolia-live-2 in the cloud, on the final code

The same rehearsal as [`../cloud-fork-2/`](../cloud-fork-2/README.md) (`e2e/sepolia-cloud.sh start --fork --yes
cloud-fork-3` from a clean copy of [`../sepolia-live-2/`](../sepolia-live-2/README.md), 10 November 2026), on the
commit the owner's run uses (ad222a5: anvil forks behind the endpoint's head, forge tried again on its race while
nothing was sent, the gas check with no manifest yet). anvil forked Sepolia at block 12,107,453; the runner started at
21:54:42 UTC and the driver ended at 22:03:31 with `ALL CHECKS PASSED` and `DONE: 16 transactions on chain 11155111,
11030050 gas` (Foundry's gas: the transactions ran on anvil; every stage was priced by Sepolia's node first,
`gas/<stage>.json`). The dry pass of these transactions against the node is in `../cloud-fork-2/` (the same
transactions but for the finalisation's seal).
