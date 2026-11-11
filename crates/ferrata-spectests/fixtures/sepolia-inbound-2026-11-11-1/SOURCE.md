# Sepolia fixture of the attestation tests under a Gloas record

Fetched by `lez/scripts/fetch-attest-fixture.py --network sepolia` between 2026-11-11T00:03:34Z and 2026-11-11T00:04:06Z
(UTC). Every request went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`.
Responses are stored byte for byte; `header.rlp` is rebuilt from `block.json` by the script, which
checked that it hashes to the bound block hash; `fixture.json` records the script's choices.
`SHA256SUMS` pins every file.

| | |
|---|---|
| chain | sepolia, chain id 11155111 |
| finality update (Gloas) | attested slot 11551808, signature slot 11551809, 509/512 signers (period 1410) |
| finalized beacon block | slot 11551744, root 0xb4d61ff6b1ba8fccd18e30ade9922708105bf0c1c5ab01c9a37d0b97f98311ef |
| bound execution block (the bid's parent) | 12107997, hash 0x6ffe0da75fde7ac9c14b244be3d8963945c229e85776370d02d81e86dacda255 |
| its state root / receipts root | 0x31c38d8294e12938e5982a52d61123749c0b3d3451d0be8399d3a30aafe0ccf5 / 0x200936de92535fb0d5f0c2d37f61515ea26f4baad72baa13624ebe089ff70b8c |
| execution head when the proofs were fetched | 12108072 |
| storage | 0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7 slot 0x0000000000000000000000000000000000000000000000000000000000000000 = 0xa5b5c43fea8123ea584de580eb9f19de945f0e435ca9ef21f25fd87844a74875, equal to `depositRoot()` at the block |
| storage | 0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7 slot 0x0000000000000000000000000000000000000000000000000000000000000001 = 0x0000000000000000000000000000000000000000000000000000000000000001, equal to `nextLeafIndex()` at the block |
| storage | 0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7 slot 0x0000000000000000000000000000000000000000000000000000000000000002 = 0xa2e880cb8da77b08dad9833be7a4c9aa08dd856f4648da5486937c798611d3b8, equal to `storedLezConfigId()` at the block |
| same address on mainnet | 0xca11bde05977b3631167028862be2a173976ca11: code hash 0xd5c15df687b16f2ff992fc8d767b4216323184a2bbc6ee2f9c398c318e770891 at 12107997 and at mainnet block 26387975 |
| ERC-20 `Transfer` | tx 3, log 1 of 3 (`logIndex` 3): 0xae9190aeca45f50dcda0483c0223e191e6811ad2 from 0x9bc68f8b2fea572edc0813f76c186a31e4150f6f to 0xfb028c28d85b2e115a221739713a8d3a84bc405d, value 60000000000000 |
| vault deposits | 1 `Deposit` logs of 0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7 from block 12107894 to 12107997 |

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-11T00:03:34Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-11T00:03:34Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByHash ["0x6ffe0da75fde7ac9c14b244be3d8963945c229e85776370d02d81e86dacda255", false]` | block.json |  |
| 2026-11-11T00:03:47Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7", ["0x0000000000000000000000000000000000000000000000000000000000000000", "0x0000000000000000000000000000000000000000000000000000000000000001", "0x0000000000000000000000000000000000000000000000000000000000000002"], "0xb8c0dd"]` | proof_0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7_12107997.json |  |
| 2026-11-11T00:03:47Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7", "data": "0x8955fe8a"}, "0xb8c0dd"]` | call_0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7_depositRoot_12107997.json |  |
| 2026-11-11T00:03:47Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7", "data": "0x0be4f422"}, "0xb8c0dd"]` | call_0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7_nextLeafIndex_12107997.json |  |
| 2026-11-11T00:03:47Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_call [{"to": "0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7", "data": "0x8469b930"}, "0xb8c0dd"]` | call_0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7_storedLezConfigId_12107997.json |  |
| 2026-11-11T00:03:47Z | https://1rpc.io/sepolia | `eth_getProof ["0xca11bde05977b3631167028862be2a173976ca11", [], "0xb8c0dd"]` | proof_0xca11bde05977b3631167028862be2a173976ca11_12107997.json |  |
| 2026-11-11T00:03:56Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-11T00:03:56Z | https://ethereum-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-11T00:03:56Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xca11bde05977b3631167028862be2a173976ca11", [], "0x192a607"]` | proof_0xca11bde05977b3631167028862be2a173976ca11_mainnet_26387975.json |  |
| 2026-11-11T00:03:57Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockReceipts ["0xb8c0dd"]` | receipts_12107997.json |  |
| 2026-11-11T00:03:57Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getLogs [{"address": "0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7", "topics": ["0x83d2d1c34c8825843c9af84bec80378e997084e8c1d6d5a3c2a6e758fe05538e"], "fromBlock": "0xb8c076", "toBlock": "0xb8c0dd"}]` |  |  |
| 2026-11-11T00:03:57Z | https://1rpc.io/sepolia | `eth_getBlockByHash ["0x6ffe0da75fde7ac9c14b244be3d8963945c229e85776370d02d81e86dacda255", false]` | block_second.json |  |
| 2026-11-11T00:04:06Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11551744` | beacon_header_11551744_second.json |  |
