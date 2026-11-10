# The return leg on Sepolia

**The attempt of 10 November 2026 ran out of gas at its first transaction; this run can no longer happen.**
The cloud runner (`e2e/sepolia-cloud.sh start`, commit b39eac1) started at 19:50:40 UTC and sent nonce 0 of
`0x79A9…5C07`, the creation of RISC Zero's Groth16 verifier, with forge's gas limit of 1,533,981 (1.3 x Foundry's
own estimate of 1,179,986): transaction `0x673cdb72b0ff42b66d02b4a4e4c2a95ff82feb9fd4d8907b1ccde35a0640c7c1`,
block 12,106,853, status 0, gas used = limit, at 74 wei. Sepolia's node asks 7,795,331 for it: since Glamsterdam
(6 October 2026) a byte of code costs 1,530 gas of state gas instead of 200 (EIP-8037; `vault/GAS.md`, "On Sepolia
since Glamsterdam"), which Foundry's EVM, and so every rehearsal on anvil, did not know. The driver stopped there
and sent nothing more. The sender is at nonce 1 and the proofs of this directory bind the vault of nonce 3,
which is no longer reachable: the files below are kept as the record of the attempt, and the round trip moved
to [`../sepolia-live-2/`](../sepolia-live-2/) (deployer nonce 1, every gas limit from the node's estimate).

The run directory of the round trip on the public Sepolia testnet (chain id 11155111), made from
[`../sepolia/`](../sepolia/): the same deployment, wallet, zone, candidate proof and release proofs
(their succinct receipts are in `../sepolia/receipts/`). Its own files appear as the stages run
(`vault/README.md`, "Sepolia round trip"):

| File | What |
|---|---|
| `manifest.json` | every transaction: hash, block, gas, status; `vault/script/verify-sepolia.sh` re-checks it against any Sepolia endpoint |
| `beacon.json` | the Sepolia block that mined `recordBeacon`: number, `prevrandao`, the nonce |
| `cp-e2e-finalize.{input,json}`, `receipts/cp-e2e-finalize/` | the finalisation for that nonce, built and proven after the beacon existed |
| `fixture.json`, `lez.json` | the fixture with that finalisation |
