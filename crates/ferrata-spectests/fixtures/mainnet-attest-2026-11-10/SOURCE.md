# Mainnet fixture of the attestation tests: one finalized block and facts under it

Fetched by `lez/scripts/fetch-attest-fixture.py` between 2026-11-10T13:49:32Z and 2026-11-10T13:49:35Z (UTC). Every
request went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`. Responses are
stored byte for byte; `header_*.rlp` are rebuilt from `block_*.json` by the script, which checked
that each hashes to the block's hash; `fixture.json` records the script's choices. `SHA256SUMS`
pins every file.

| | |
|---|---|
| chain | Ethereum mainnet, chain id 1 |
| sync-committee period | 1907 |
| bootstrap (the checkpoint `Init` trusts) | slot 15622144, block root 0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433 |
| finality update | attested slot 15624544, signature slot 15624545, 505/512 signers |
| finalized beacon block | slot 15624480, root 0x46d069655d60786d109a3abec9a63e4db77560b9ca7e588c82f13ef988a4f40f |
| finalized execution block `n` | 26384851, hash 0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce |
| `n` state root / receipts root | 0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e / 0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2 |
| `n` receipts | 531 (14 failed; the first failed transaction is 37) |
| `m` = n - 16 (one EIP-2935 hop from n) | 26384835, hash 0x6563fc5beca44eb23beb284a22bdfc789749731a94fbac7a1457cbd245b8739d |
| execution head when the proofs at n were fetched | 26384915 |
| WETH | 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2 |
| holder (Uniswap v2 USDC/WETH pair) | 0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc |
| WETH balance slot, empty slot | 0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0, 0x00000000000000000000000000000000000000000000000000000000000000ff |
| address without an account | 0x3e05d7d15f0d45265117bcd6e6682f138ee60e60 (`keccak256("ferrata/absent-account")[12..]`) |
| history contract, slot of m | 0x0000f90827f1c53a10cb7a02335b175320002935, 0x0000000000000000000000000000000000000000000000000000000000000658 |

Logs chosen in block `n` (ERC-20 `Transfer`: three topics, one data word; positions count from zero):

| log | where | token | from | to | value |
|---|---|---|---|---|---:|
| the first `Transfer` whose recipient has code at n (`code_recipient_*.json`) | tx 0, log 0 of 7 in its receipt (`logIndex` 0) | 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48 | 0x000000000004444c5dc75cb358380d2e3de08a90 | 0x0000000aa232009084bd71a5797d089aa4edfad4 | 32141291 |
| the first `Transfer` of that token to another recipient | tx 0, log 4 of 7 in its receipt (`logIndex` 4) | 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48 | 0x0000000aa232009084bd71a5797d089aa4edfad4 | 0x000000000004444c5dc75cb358380d2e3de08a90 | 19981261002 |
| the first `Transfer` of another token | tx 0, log 1 of 7 in its receipt (`logIndex` 1) | 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2 | 0x000000000004444c5dc75cb358380d2e3de08a90 | 0x0000000aa232009084bd71a5797d089aa4edfad4 | 19308653000204600123 |
| the first `Transfer` in the receipt with the most logs | tx 1, log 0 of 306 in its receipt (`logIndex` 7) | 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2 | 0x4068038ab5490063a3ead93b3712ed082e62bd59 | 0xbdb3ba9ffe392549e1f8658dd2630c141fdf47b6 | 197470918353185602 |

Cross-checks made by the script: the finalized light-client header's execution block hash, state
root and receipts root equal those of `block_n`; a second execution provider (`block_*_second.json`)
reports the same hash, state root and receipts root for n and m; a second beacon provider
(`beacon_header_*_second.json`) reports the finalized block root computed from the update, and the
bootstrap's block root.

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-10T13:49:32Z | https://lodestar-mainnet.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | fulu |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", ["0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0"], "0x19299c3"]` | proof_weth_26384835.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_call [{"to": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x19299c3"]` | call_weth_balance_26384835.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", ["0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0", "0x00000000000000000000000000000000000000000000000000000000000000ff"], "0x19299d3"]` | proof_weth_26384851.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0x3e05d7d15f0d45265117bcd6e6682f138ee60e60", ["0x0000000000000000000000000000000000000000000000000000000000000000"], "0x19299d3"]` | proof_absent_26384851.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000000658"], "0x19299d3"]` | proof_history_26384851.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_call [{"to": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x19299d3"]` | call_weth_balance_26384851.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getBlockReceipts ["0x19299d3"]` | receipts_26384851.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getCode ["0x0000000aa232009084bd71a5797d089aa4edfad4", "0x19299d3"]` |  |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_getCode ["0x0000000aa232009084bd71a5797d089aa4edfad4", "0x19299d3"]` | code_recipient_26384851.json |  |
| 2026-11-10T13:49:33Z | https://ethereum-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-10T13:49:34Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19299d3", false]` | block_26384851.json |  |
| 2026-11-10T13:49:34Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19299c3", false]` | block_26384835.json |  |
| 2026-11-10T13:49:34Z | https://eth.drpc.org | `eth_getBlockByNumber ["0x19299d3", false]` | block_26384851_second.json |  |
| 2026-11-10T13:49:34Z | https://eth.drpc.org | `eth_getBlockByNumber ["0x19299c3", false]` | block_26384835_second.json |  |
| 2026-11-10T13:49:34Z | https://ethereum-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/15622144` |  |  |
| 2026-11-10T13:49:34Z | https://lodestar-mainnet.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433` | bootstrap.ssz | fulu |
| 2026-11-10T13:49:35Z | https://ethereum-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/15622144` | beacon_header_15622144_second.json |  |
| 2026-11-10T13:49:35Z | https://ethereum-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/15624480` | beacon_header_15624480_second.json |  |
