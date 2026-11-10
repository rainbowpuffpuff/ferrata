# Rehearsal of sepolia-live-2 in the cloud, on a fork, with Sepolia's own gas

`e2e/sepolia-cloud.sh start --fork --yes cloud-fork-2` from a clean copy of [`../sepolia-live-2/`](../sepolia-live-2/README.md)
on 10 November 2026: the cloud runner (Modal, the owner's workspace) built the tools, started anvil on a fork of Sepolia
at block 12,107,373 and ran `e2e/sepolia-live.sh` with the deployer impersonated (`FERRATA_FORK=1`), exactly as the live
run will, except that nothing is signed and the two waits are passed by moving the fork's clock. Before every stage the
gas check priced the stage's transactions with **Sepolia's node** on the fork's state (`gas/<stage>.json`); the
transactions themselves ran on anvil, so the manifest's gas is Foundry's.

Result: `ALL CHECKS PASSED` and `DONE: 16 transactions on chain 11155111, 11030062 gas` at 21:46:51 UTC
(stages.done: deploy 21:43:37, the three deposits and the two capture stages, which a fork skips, by 21:43:49, submit,
beacon, the finalisation proof 21:44:18-21:45:58 (the GPU answered in 17 s; `identity_p254` 31 s; the prover image 52 s),
finalize, release, verify). `verify-sepolia.sh` re-checked every receipt, the wiring, the configuration, every seal
through the deployed stop, both recipients' balances and what must be refused.

Two earlier starts of this rehearsal stopped before any stage, and both causes are fixed (commits 28a19a1, 813cc4b,
and the fork-behind-the-head change after it): anvil could not fork the public endpoint at its head (a node behind
the load balancer answered "block not found" for the newest block; anvil now forks five blocks behind the head, the gas
check three, and both retry), and the gas check refused the deploy stage because forge's own re-simulation of a dry run
starts the sender at nonce 0, which put every creation one address off (the dry run now skips it).

**The dry pass** (`gas-sepolia.json`, `plan.json`): the sixteen transactions of this rehearsal, taken from forge's
records of it, played in order on a fresh fork of Sepolia at the deployer's real nonce 1 (block 12,107,410), each first
priced by Sepolia's node (reth 2.10.0, block 12,107,415) on that state: every `eth_estimateGas` succeeds, every
transaction succeeds on the node with its limit (the estimate x 1.25), the vault's creation at 48,093,059 (over 2^24,
valid under EIP-8037). Sepolia: 62,096,366 gas; Foundry: 11,030,062 (5.63x). Per transaction: `e2e/README.md`.
