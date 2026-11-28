# Sepolia fixture of the attestation tests under a Gloas record

Fetched by `lez/scripts/fetch-attest-fixture.py --network sepolia` between 2026-11-28T07:28:13Z and 2026-11-28T07:28:15Z
(UTC). Every request went to a public endpoint without an API key, with User-Agent `curl/8`.
Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which
checked that it hashes to the bound block hash; `fixture.json` records the script's choices.
`SHA256SUMS` pins every file.

| | |
|---|---|
| chain | sepolia, chain id 11155111 |
| finality update (Gloas) | attested slot 11676416, signature slot 11676417, 474/512 signers (period 1425) |
| finalized beacon block | slot 11676352, root 0xb4de784c95280f3c85f7c05839b1edbf3d4d77320859f32d49e0c0b7f60f36a5 |
| bound execution block (the bid's parent) | 12228553, hash 0xd1c0498bc8570f9e0849c6bacc46b1caba22e3b52ab9cec717f4ea2be847d9f8 |
| its state root / receipts root | 0x0b29eb219d34d03eeb1291c52147dcc8b2b66783e2a17cc367e1d2e1433737a6 / 0x508245611f05e6acce4c69a469078b238a66a2c77d9d1a6bc7d296cd58a00e41 |
| execution head when the proofs were fetched | 12228635 |
| storage | 0x1c7d4b196cb0c7b01d743fbc6116a902379c7238 slot 0x000000000000000000000000000000000000000000000000000000000000000b = 0x0000000000000000000000000000000000000000000000009512c99f57706d09, equal to `totalSupply()` at the block |
| ERC-20 `Transfer` | tx 4, log 0 of 17 (`logIndex` 5): 0xf6081c62c44e85a025101c99fb9f74afc36b23ca from 0x3ec84659862c59619e82f16b787b798bd423d038 to 0x8b2a1415b29c72bc22c4d74a8b899badef451d18, value 77433485091956308 |

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-28T07:28:13Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-28T07:28:13Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0xd1c0498bc8570f9e0849c6bacc46b1caba22e3b52ab9cec717f4ea2be847d9f8", false]` | block.json |  |
| 2026-11-28T07:28:13Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", ["0x000000000000000000000000000000000000000000000000000000000000000b"], "0xba97c9"]` | proof_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_12228553.json |  |
| 2026-11-28T07:28:14Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", "data": "0x18160ddd"}, "0xba97c9"]` | call_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_totalSupply_12228553.json |  |
| 2026-11-28T07:28:14Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-28T07:28:14Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xba97c9"]` | receipts_12228553.json |  |
| 2026-11-28T07:28:15Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByHash ["0xd1c0498bc8570f9e0849c6bacc46b1caba22e3b52ab9cec717f4ea2be847d9f8", false]` | block_second.json |  |
| 2026-11-28T07:28:15Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11676352` | beacon_header_11676352_second.json |  |
