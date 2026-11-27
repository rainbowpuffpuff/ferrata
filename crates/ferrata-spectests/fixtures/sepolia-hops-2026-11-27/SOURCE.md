# EIP-2935 hop chains on sepolia from block 12112773

Fetched by `lez/scripts/fetch-hop-chain.py --network sepolia --record 12112773 --chain 12112341,12111909,12111477 --chain 12112449,12112125,12111801,12111477 --chain 12112514,12112255,12111996,12111737,12111477 OUTDIR` between 2026-11-27T15:18:45Z and 2026-11-27T15:18:54Z (UTC). Every request went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`. Responses are stored byte for byte; each `header_N.rlp` is rebuilt from `block_N.json` and was checked to hash to the hash both providers gave; each hop's history slot was checked to hold that hash; `fixture.json` records the choices. `SHA256SUMS` pins every file.

Chains (newest first, from the record's block): 3 hops to 12111477; 4 hops to 12111477; 5 hops to 12111477.

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-27T15:18:45Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d385", false]` | block_12112773.json |  |
| 2026-11-27T15:18:46Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d385", false]` | block_12112773_second.json |  |
| 2026-11-27T15:18:46Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d282", false]` | block_12112514.json |  |
| 2026-11-27T15:18:46Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d282", false]` | block_12112514_second.json |  |
| 2026-11-27T15:18:47Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d241", false]` | block_12112449.json |  |
| 2026-11-27T15:18:47Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d241", false]` | block_12112449_second.json |  |
| 2026-11-27T15:18:47Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d1d5", false]` | block_12112341.json |  |
| 2026-11-27T15:18:47Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d1d5", false]` | block_12112341_second.json |  |
| 2026-11-27T15:18:48Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d17f", false]` | block_12112255.json |  |
| 2026-11-27T15:18:48Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d17f", false]` | block_12112255_second.json |  |
| 2026-11-27T15:18:48Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d0fd", false]` | block_12112125.json |  |
| 2026-11-27T15:18:48Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d0fd", false]` | block_12112125_second.json |  |
| 2026-11-27T15:18:49Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d07c", false]` | block_12111996.json |  |
| 2026-11-27T15:18:49Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d07c", false]` | block_12111996_second.json |  |
| 2026-11-27T15:18:49Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8d025", false]` | block_12111909.json |  |
| 2026-11-27T15:18:49Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8d025", false]` | block_12111909_second.json |  |
| 2026-11-27T15:18:50Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8cfb9", false]` | block_12111801.json |  |
| 2026-11-27T15:18:50Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8cfb9", false]` | block_12111801_second.json |  |
| 2026-11-27T15:18:50Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8cf79", false]` | block_12111737.json |  |
| 2026-11-27T15:18:50Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8cf79", false]` | block_12111737_second.json |  |
| 2026-11-27T15:18:51Z | https://ethereum-sepolia-rpc.publicnode.com | `eth_getBlockByNumber ["0xb8ce75", false]` | block_12111477.json |  |
| 2026-11-27T15:18:51Z | https://rpc.sepolia.ethpandaops.io | `eth_getBlockByNumber ["0xb8ce75", false]` | block_12111477_second.json |  |
| 2026-11-27T15:18:51Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000179b", "0x0000000000000000000000000000000000000000000000000000000000001807", "0x0000000000000000000000000000000000000000000000000000000000001848"], "0xb8d385"]` | proof_history_12112773.json |  |
| 2026-11-27T15:18:51Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000001745"], "0xb8d282"]` | proof_history_12112514.json |  |
| 2026-11-27T15:18:52Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x00000000000000000000000000000000000000000000000000000000000016c3"], "0xb8d241"]` | proof_history_12112449.json |  |
| 2026-11-27T15:18:52Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x00000000000000000000000000000000000000000000000000000000000015eb"], "0xb8d1d5"]` | proof_history_12112341.json |  |
| 2026-11-27T15:18:52Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000001642"], "0xb8d17f"]` | proof_history_12112255.json |  |
| 2026-11-27T15:18:53Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000157f"], "0xb8d0fd"]` | proof_history_12112125.json |  |
| 2026-11-27T15:18:53Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000153f"], "0xb8d07c"]` | proof_history_12111996.json |  |
| 2026-11-27T15:18:53Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000143b"], "0xb8d025"]` | proof_history_12111909.json |  |
| 2026-11-27T15:18:54Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000143b"], "0xb8cfb9"]` | proof_history_12111801.json |  |
| 2026-11-27T15:18:54Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x000000000000000000000000000000000000000000000000000000000000143b"], "0xb8cf79"]` | proof_history_12111737.json |  |
| 2026-11-27T15:18:54Z | https://rpc.sepolia.ethpandaops.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", [], "0xb8ce75"]` | proof_history_12111477.json |  |
