# Sepolia fixture of the attestation tests under a Gloas record

Fetched by `lez/scripts/fetch-attest-fixture.py --network sepolia` between 2026-11-10T17:10:50Z and 2026-11-10T17:10:53Z
(UTC). Every request went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`.
Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which
checked that it hashes to the bound block hash; `fixture.json` records the script's choices.
`SHA256SUMS` pins every file.

| | |
|---|---|
| chain | sepolia, chain id 11155111 |
| finality update (Gloas) | attested slot 11549728, signature slot 11549729, 507/512 signers (period 1409) |
| finalized beacon block | slot 11549664, root 0xeed903288a43d417dad9e91d4bb01bfce99dbb2f8157274e3cc63cb499156657 |
| bound execution block (the bid's parent) | 12105936, hash 0x5a2f5671d4d93d4e9a188ec55743ae20a86cce5f91e570f2e5dc22b2b34b27b6 |
| its state root / receipts root | 0x4d8a992af4d3f194a5d5d3d4bbb8075a7f6690007499b4b5324460af0f7468b9 / 0xc241ec3df1c2d3d7609fea59220a68e2ec435efedf8a5ab5a94b79bfa7a59207 |
| execution head when the proofs were fetched | 12106027 |
| storage | 0x1c7d4b196cb0c7b01d743fbc6116a902379c7238 slot 0x000000000000000000000000000000000000000000000000000000000000000b = 0x0000000000000000000000000000000000000000000000009513ef76177584ee, equal to `totalSupply()` at the block |
| same address on mainnet | 0xca11bde05977b3631167028862be2a173976ca11: code hash 0xd5c15df687b16f2ff992fc8d767b4216323184a2bbc6ee2f9c398c318e770891 at 12105936 and at mainnet block 26385920 |
| ERC-20 `Transfer` | tx 0, log 0 of 3 (`logIndex` 0): 0x5543ca71ed8c88daf217488b1808994e080d925b from 0x03680e32ce8a16ec4ebb1b20ab9ffc1327ef087e to 0x18eb5daceb55e69de7154ae5e9067bc1c6f2f42b, value 30992770064991972 |

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-10T17:10:50Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-10T17:10:51Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0x5a2f5671d4d93d4e9a188ec55743ae20a86cce5f91e570f2e5dc22b2b34b27b6", false]` | block.json |  |
| 2026-11-10T17:10:51Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", ["0x000000000000000000000000000000000000000000000000000000000000000b"], "0xb8b8d0"]` | proof_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_12105936.json |  |
| 2026-11-10T17:10:52Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238", "data": "0x18160ddd"}, "0xb8b8d0"]` | call_0x1c7d4b196cb0c7b01d743fbc6116a902379c7238_totalSupply_12105936.json |  |
| 2026-11-10T17:10:52Z | https://1rpc.io/sepolia | `eth_getProof ["0xca11bde05977b3631167028862be2a173976ca11", [], "0xb8b8d0"]` | proof_0xca11bde05977b3631167028862be2a173976ca11_12105936.json |  |
| 2026-11-10T17:10:52Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-10T17:10:52Z | https://ethereum-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-10T17:10:52Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xca11bde05977b3631167028862be2a173976ca11", [], "0x1929e00"]` | proof_0xca11bde05977b3631167028862be2a173976ca11_mainnet_26385920.json |  |
| 2026-11-10T17:10:52Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xb8b8d0"]` | receipts_12105936.json |  |
| 2026-11-10T17:10:53Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByHash ["0x5a2f5671d4d93d4e9a188ec55743ae20a86cce5f91e570f2e5dc22b2b34b27b6", false]` | block_second.json |  |
| 2026-11-10T17:10:53Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11549664` | beacon_header_11549664_second.json |  |
