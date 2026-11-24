# Sepolia fixture of the attestation tests under a Gloas record

Fetched by `lez/scripts/fetch-attest-fixture.py --network sepolia` between 2026-11-24T03:25:38Z and 2026-11-24T03:25:40Z
(UTC). Every request went to a public endpoint without an API key, with User-Agent `curl/8`.
Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which
checked that it hashes to the bound block hash; `fixture.json` records the script's choices.
`SHA256SUMS` pins every file.

| | |
|---|---|
| chain | sepolia, chain id 11155111 |
| finality update (Gloas) | attested slot 11646400, signature slot 11646401, 486/512 signers (period 1421) |
| finalized beacon block | slot 11646336, root 0x18ee938f06b720bc37a2e00517063f8a1ddc2efbdeb032ff85a481acca82f4fe |
| bound execution block (the bid's parent) | 12199909, hash 0xc19a5b4a93b314918604f2a3c5bcfe8fd21b924b697a650c1db427a62e21faa0 |
| its state root / receipts root | 0x659e7b6c237e7972a6eb30b3209d6199ad2c2de0a8f5e5a98d3e384fb169b84c / 0xa538d9847ed70ddd08444e3460df2fd1c792c5d27035628845a12bae6f59d105 |
| execution head when the proofs were fetched | 12199997 |
| storage | 0x1c7d4b196cb0c7b01d743fbc6116a902379c7238 slot 0x000000000000000000000000000000000000000000000000000000000000000b = 0x000000000000000000000000000000000000000000000000951404989cd91159, equal to `totalSupply()` at the block |
| ERC-20 `Transfer` | tx 0, log 0 of 1 (`logIndex` 0): 0xd4778aa86d4ef3513a5a1c4d8d6e121f3ba58236 from 0x81a1ed90703d0fbd8fa097d4a42420363f936cfc to 0x9749ced506ad8431f793628615b0a1c351e9d718, value 2590814 |

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-24T03:25:38Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-24T03:25:39Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0xc19a5b4a93b314918604f2a3c5bcfe8fd21b924b697a650c1db427a62e21faa0", false]` | block.json |  |
| 2026-11-24T03:25:39Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", ["0x000000000000000000000000000000000000000000000000000000000000000b"], "0xba27e5"]` | proof_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_12199909.json |  |
| 2026-11-24T03:25:39Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", "data": "0x18160ddd"}, "0xba27e5"]` | call_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_totalSupply_12199909.json |  |
| 2026-11-24T03:25:39Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-24T03:25:39Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xba27e5"]` | receipts_12199909.json |  |
| 2026-11-24T03:25:39Z | https://1rpc.io/sepolia | `eth_getBlockByHash ["0xc19a5b4a93b314918604f2a3c5bcfe8fd21b924b697a650c1db427a62e21faa0", false]` | block_second.json |  |
| 2026-11-24T03:25:40Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11646336` | beacon_header_11646336_second.json |  |
