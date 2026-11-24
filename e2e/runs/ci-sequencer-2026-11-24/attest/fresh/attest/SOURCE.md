# Sepolia fixture of the attestation tests under a Gloas record

Fetched by `lez/scripts/fetch-attest-fixture.py --network sepolia` between 2026-11-24T14:45:20Z and 2026-11-24T14:45:21Z
(UTC). Every request went to a public endpoint without an API key, with User-Agent `curl/8`.
Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which
checked that it hashes to the bound block hash; `fixture.json` records the script's choices.
`SHA256SUMS` pins every file.

| | |
|---|---|
| chain | sepolia, chain id 11155111 |
| finality update (Gloas) | attested slot 11649824, signature slot 11649825, 494/512 signers (period 1422) |
| finalized beacon block | slot 11649760, root 0xa31a73bbb64878fcad0fb07c6966c1e503ef4af34f3d142d55f424d10dfbd79e |
| bound execution block (the bid's parent) | 12203185, hash 0x74a8942836cc69ef12c4eaad07abddb2d26d5ae8d4c1fd3dbd1a498ab3e47240 |
| its state root / receipts root | 0xc200412a8f931bee3cf5416b92f1f35aeba6d9544458f96cef718f64ff6a4493 / 0xecb0b25c5ad4bac7be5f4cbddb3157ec082673321d0adf4c2acd4b85a5f7f848 |
| execution head when the proofs were fetched | 12203250 |
| storage | 0x1c7d4b196cb0c7b01d743fbc6116a902379c7238 slot 0x000000000000000000000000000000000000000000000000000000000000000b = 0x000000000000000000000000000000000000000000000000951404984abb7b6c, equal to `totalSupply()` at the block |
| ERC-20 `Transfer` | tx 1, log 0 of 1 (`logIndex` 1): 0xfffffffffffffffffffffffffffffffffffffffe from 0x37ea8d73e68eeb1ec06fba8d8c369757313e8a5a to 0x46bc78edd02638cd5c00211c2787c0cebd7a99ea, value 9590799996931000 |

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-24T14:45:20Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-24T14:45:20Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0x74a8942836cc69ef12c4eaad07abddb2d26d5ae8d4c1fd3dbd1a498ab3e47240", false]` | block.json |  |
| 2026-11-24T14:45:20Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getProof ["0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", ["0x000000000000000000000000000000000000000000000000000000000000000b"], "0xba34b1"]` | proof_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_12203185.json |  |
| 2026-11-24T14:45:20Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", "data": "0x18160ddd"}, "0xba34b1"]` | call_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_totalSupply_12203185.json |  |
| 2026-11-24T14:45:20Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-24T14:45:20Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xba34b1"]` | receipts_12203185.json |  |
| 2026-11-24T14:45:21Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByHash ["0x74a8942836cc69ef12c4eaad07abddb2d26d5ae8d4c1fd3dbd1a498ab3e47240", false]` | block_second.json |  |
| 2026-11-24T14:45:21Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11649760` | beacon_header_11649760_second.json |  |
