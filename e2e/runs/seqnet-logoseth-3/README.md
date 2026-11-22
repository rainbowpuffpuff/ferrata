# seqnet logoseth-3: the mini-app's round trip and admin, Ethereum through Logos Core's own modules (21 Nov 2026)

**Superseded (22 Nov 2026) by `../seqnet-logoseth-4/`** (the second review's fixes: the registry's LEZ side read again
through `lez_core`, every LEZ role message held for a person's approval at the role's wallet).

As `../seqnet-logoseth-1/` (a LEZ v0.4.0-rc1 zone we ran, anvil with chain id 11155111, the mock verifier, dev-mode
private proofs), with the review's fixes: branch `rfp-logos-core-eth` at `3ace689f` plus the test inputs of `25eae689`'s
tree. On the Modal build box, from the worktree's root, as run: `export SEQNET_TMP=<scratch>/seqnet
SEQNET_SHORT=/var/tmp/rfx; bash e2e/seqnet/run.sh logoseth logoseth-3` (the tree built by `run.sh build-logoseth`, the
module's `.lgx` from `modules/results/bridge-send-stack-r2/`'s build, sha256 `6b638097…`). Run 518 s.

Qt Quick Test: 12 passed of 12 (10 test functions plus `initTestCase`/`cleanupTestCase`; `miniapp/qmltest.txt`).
Beyond logoseth-1: the cap and the lift read back from the vault; a `setTokenCap` forced to revert after its approval
(the admin's own gas limit 30,000) reported in the admin tab as "Not done: ... reverted ... block 20 ... 29997 gas of its
limit 30000" with the cap unchanged (screenshot 22b); the bridge program's `SetCap` built by the module at the admin's
own actor, signed by the chain service's LEZ wallet holding the run's admin key (a stand-in), accepted in LEZ block 39
and read back from `token(cid, ETH)`; the same from the user's public account refused by the program (reverted, block
40); the registry listing both tokens the vault registered; from the admin tab's panel, the second token delisted and
listed again, a fee schedule announced and a release delay of 7,200 s set, each read back (screenshots 26-29). The
privacy panel says the release goes from the user's release account (no relayer on this path).

Every user and admin Ethereum send went through `tx_sender_module`, approved at `evm_signer_cli` (`approver/`).
Outside the stack: the stand-in prover's `accept` to the mock verifier and the relayer's checkpoint transactions
(`eth/transactions.tsv`); the LEZ admin message's signer (above).
