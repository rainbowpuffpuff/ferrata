# The Sepolia stages on a local fork

A copy of [`../sepolia/`](../sepolia/) taken for the rehearsal of `vault/script/` on
`anvil --fork-url <a public Sepolia endpoint>` (fork block 12,105,330, the deployer impersonated, no
key): the same deployment, wallet, zone, candidate and releases, without the succinct receipts,
which are in `../sepolia/receipts/`. What is its own:

| File | What |
|---|---|
| `manifest.json` | every transaction of the rehearsal: hash, block, gas, status |
| `beacon.json` | the fork block that mined `recordBeacon`: number, anvil's random `prevrandao`, the nonce |
| `cp-e2e-finalize.{input,json}`, `lez.json`, `fixture.json` | the finalisation for that nonce, built on the candidate's stored blocks after the beacon existed |
| `receipts/cp-e2e-finalize/` | its proof, once made |

The hashes are those of a private fork: they exist on no public network.
