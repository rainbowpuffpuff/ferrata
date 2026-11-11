# Sepolia fixture of the attestation tests under a Gloas record

Fetched by `lez/scripts/fetch-attest-fixture.py --network sepolia` between 2026-11-11T16:16:25Z and 2026-11-11T16:16:30Z
(UTC). Every request went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`.
Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which
checked that it hashes to the bound block hash; `fixture.json` records the script's choices.
`SHA256SUMS` pins every file.

| | |
|---|---|
| chain | sepolia, chain id 11155111 |
| finality update (Gloas) | attested slot 11556672, signature slot 11556673, 509/512 signers (period 1410) |
| finalized beacon block | slot 11556608, root 0x04246d06b2b0a00b4bf23ea2e637f8f5e9512d910439503e737141ed623a2b6a |
| bound execution block (the bid's parent) | 12112773, hash 0xa267d3df04f31713cf7f42d15362d3ea1b12bc3626d0ed537b7bfb67ead2e126 |
| its state root / receipts root | 0xbf810e0a587ef6c5c977c069ee132008c7ef93a6cc1dfdcb76d895a3f62deda8 / 0x711fbca356a1df8364d03fc483d4f2089577598c94ead93f7f611dcdde6ad19b |
| execution head when the proofs were fetched | 12112847 |
| storage | 0xfe3199fa2634413cbac317371076cae3eb2d9e54 slot 0x0000000000000000000000000000000000000000000000000000000000000000 = 0xfaad123258a78fc60cfa12c793a7d68bab13c2d319d68e847a1bd3f2e140579d, equal to `depositRoot()` at the block |
| storage | 0xfe3199fa2634413cbac317371076cae3eb2d9e54 slot 0x0000000000000000000000000000000000000000000000000000000000000001 = 0x0000000000000000000000000000000000000000000000000000000000000002, equal to `nextLeafIndex()` at the block |
| same address on mainnet | 0xca11bde05977b3631167028862be2a173976ca11: code hash 0xd5c15df687b16f2ff992fc8d767b4216323184a2bbc6ee2f9c398c318e770891 at 12112773 and at mainnet block 26392819 |
| ERC-20 `Transfer` | tx 1, log 2 of 5 (`logIndex` 3): 0xfffffffffffffffffffffffffffffffffffffffe from 0x31027ed37f740ae6b3880e68dc465d45fe45c9a8 to 0xdfd8302f44727a6348f702ff7b594f127de3a902, value 1 |
| vault deposits | 2 `Deposit` logs of 0xfe3199fa2634413cbac317371076cae3eb2d9e54 from block 12111048 to 12112773 |
| EIP-2935 hop | block 12111477 (0x60d2f893c196a6089554dfe86e40b8648a88104296fdca735e81c74048ac8095), slot 0x000000000000000000000000000000000000000000000000000000000000143b of 0x0000f90827f1c53a10cb7a02335b175320002935 at 12112773; 479 receipts, receipts root 0xc76cac0c41c7eaa0d3e8eae2150778bde287ed0c648ae6042376dc24838bbc3a |
| EIP-2935 hop | block 12111574 (0xe65a7a4f401c77c451feaafb3449733e457a14cd3bea540d5f6f38c3ea013b36), slot 0x000000000000000000000000000000000000000000000000000000000000149c of 0x0000f90827f1c53a10cb7a02335b175320002935 at 12112773; 117 receipts, receipts root 0x5ceda58128e7866554997b1306830ac54d5691bb9ef9c02852c76a903edc0ba5 |
| address without an account | 0x3e05d7d15f0d45265117bcd6e6682f138ee60e60 at 12112773 |

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-11T16:16:25Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-11T16:16:26Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0xa267d3df04f31713cf7f42d15362d3ea1b12bc3626d0ed537b7bfb67ead2e126", false]` | block.json |  |
| 2026-11-11T16:16:27Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0xfe3199fa2634413cbac317371076cae3eb2d9e54", ["0x0000000000000000000000000000000000000000000000000000000000000000", "0x0000000000000000000000000000000000000000000000000000000000000001"], "0xb8d385"]` | proof_0xfe3199fa2634413cbac317371076cae3eb2d9e54_12112773.json |  |
| 2026-11-11T16:16:27Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0xfe3199fa2634413cbac317371076cae3eb2d9e54", "data": "0x8955fe8a"}, "0xb8d385"]` | call_0xfe3199fa2634413cbac317371076cae3eb2d9e54_depositRoot_12112773.json |  |
| 2026-11-11T16:16:27Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0xfe3199fa2634413cbac317371076cae3eb2d9e54", "data": "0x0be4f422"}, "0xb8d385"]` | call_0xfe3199fa2634413cbac317371076cae3eb2d9e54_nextLeafIndex_12112773.json |  |
| 2026-11-11T16:16:27Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0xca11bde05977b3631167028862be2a173976ca11", [], "0xb8d385"]` | proof_0xca11bde05977b3631167028862be2a173976ca11_12112773.json |  |
| 2026-11-11T16:16:28Z | https://1rpc.io/sepolia | `eth_getProof ["0x3e05d7d15f0d45265117bcd6e6682f138ee60e60", ["0x0000000000000000000000000000000000000000000000000000000000000000"], "0xb8d385"]` | proof_absent_12112773.json |  |
| 2026-11-11T16:16:28Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getLogs [{"address": "0xfE3199fa2634413CbAC317371076Cae3EB2d9E54", "topics": ["0x83d2d1c34c8825843c9af84bec80378e997084e8c1d6d5a3c2a6e758fe05538e"], "fromBlock": "0xb8ccc8", "toBlock": "0xb8d385"}]` |  |  |
| 2026-11-11T16:16:28Z | https://1rpc.io/sepolia | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000143b", "0x000000000000000000000000000000000000000000000000000000000000149c"], "0xb8d385"]` | proof_history_12112773.json |  |
| 2026-11-11T16:16:29Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8ce75", false]` | block_12111477.json |  |
| 2026-11-11T16:16:29Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xb8ce75"]` | receipts_12111477.json |  |
| 2026-11-11T16:16:29Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8ced6", false]` | block_12111574.json |  |
| 2026-11-11T16:16:29Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xb8ced6"]` | receipts_12111574.json |  |
| 2026-11-11T16:16:29Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-11T16:16:29Z | https://ethereum-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-11T16:16:29Z | https://eth.drpc.org | `eth_getProof ["0xca11bde05977b3631167028862be2a173976ca11", [], "0x192b8f3"]` | proof_0xca11bde05977b3631167028862be2a173976ca11_mainnet_26392819.json |  |
| 2026-11-11T16:16:30Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xb8d385"]` | receipts_12112773.json |  |
| 2026-11-11T16:16:30Z | https://1rpc.io/sepolia | `eth_getBlockByHash ["0xa267d3df04f31713cf7f42d15362d3ea1b12bc3626d0ed537b7bfb67ead2e126", false]` | block_second.json |  |
| 2026-11-11T16:16:30Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11556608` | beacon_header_11556608_second.json |  |
