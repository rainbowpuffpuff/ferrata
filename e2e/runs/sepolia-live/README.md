# The return leg on Sepolia

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
