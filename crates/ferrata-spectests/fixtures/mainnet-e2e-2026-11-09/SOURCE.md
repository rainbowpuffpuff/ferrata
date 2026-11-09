# Mainnet end-to-end fixture: light-client finality to execution facts

Fetched by `scripts/fetch-mainnet-e2e.py` between 2026-11-09T21:08:26Z and 2026-11-09T21:08:32Z (UTC). Every request
went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`. Responses are stored
byte for byte; `header_*.rlp` are rebuilt from `block_*.json` by the script, which checked that
each hashes to the block's hash. `SHA256SUMS` pins every file.

| | |
|---|---|
| sync-committee period | 1906 |
| bootstrap | slot 15613952, block root 0xdab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3 |
| finality update | attested slot 15619520, signature slot 15619521, 512/512 signers |
| finalized beacon block | slot 15619456, root 0x816b21ca5caac9461691752d78c06fa70df01537dce36caa77a753bdf46f1f0c |
| finalized execution block `n` | 26379842, hash 0xa0a6c70dc174ba93e011f8990eda677686c1bdceee85e581c3b922223928cc27 |
| `n` state root / receipts root | 0x9a2f30cbb65273a090fdadd392a192ab51955d0e8a19e2209edd5eddaa047901 / 0x5575ed0d3b2119f39004ae7721a9421ac09a0e65ba54c6f7b212a73cfb2ea914 |
| `n` receipts | 245 (0 failed) |
| `k`, newest block before n with a failed transaction | 26379841, hash 0xdb671db7805ae4dd311e99a3190ebd2efd46ae1a7feb374c0917e945ec1e595f, 270 receipts (3 failed) |
| `m1` = n - 16 (one EIP-2935 hop from n) | 26379826, hash 0x29b6e6f8fe4fb6ce91370da23b23aeb89ecf9cc84b317d3e1140e210e574c5b6 |
| `m2` = m1 - 8191 (one hop from m1, the oldest block it reaches) | 26371635, hash 0xbb9db6f0ca125ed8823585bf3af2fa37aa0102e48b1f2f05e5e158ce8d937fdc; state served by https://rpc.mevblocker.io |
| `m2 - 1` (one block past m1's window) | 26371634, hash 0x91eb9e93780736ae07d61531b1be110908d8f45ecf807dedefa221339e2633f2 |
| execution head when the proofs at n were fetched | 26379926 |
| holder (Uniswap v2 USDC/WETH pair) | 0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc |
| WETH balance slot, empty slot | 0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0, 0x00000000000000000000000000000000000000000000000000000000000000ff |
| USDC balance slot | 0x28c0947f463362aab09a2d7896ccc847fb7e52633a16b978fb2caf3db1a9b8f8 |
| address without an account | 0x3e05d7d15f0d45265117bcd6e6682f138ee60e60 (`keccak256("ferrata/absent-account")[12..]`) |

Cross-checks made by the script: the finalized light-client header's execution block hash and state
root equal those of `block_n`; a second execution provider (`block_*_second.json`) reports the same
hash, state root and receipts root for n, k, m1 and m2; a second beacon provider
(`beacon_header_*_second.json`) reports the finalized block root computed from the update, and the
bootstrap's block root.

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-09T21:08:26Z | https://lodestar-mainnet.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | fulu |
| 2026-11-09T21:08:27Z | https://ethereum-rpc.publicnode.com | `eth_getBlockReceipts ["0x1928641"]` |  |  |
| 2026-11-09T21:08:27Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", ["0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0"], "0x1928632"]` | proof_weth_26379826.json |  |
| 2026-11-09T21:08:27Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x00000000000000000000000000000000000000000000000000000000000012c6"], "0x1928632"]` | proof_history_26379826.json |  |
| 2026-11-09T21:08:27Z | https://ethereum-rpc.publicnode.com | `eth_call [{"to": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x1928632"]` | call_weth_balance_26379826.json |  |
| 2026-11-09T21:08:27Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", ["0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0", "0x00000000000000000000000000000000000000000000000000000000000000ff"], "0x1928642"]` | proof_weth_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", ["0x28c0947f463362aab09a2d7896ccc847fb7e52633a16b978fb2caf3db1a9b8f8"], "0x1928642"]` | proof_usdc_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0x3e05d7d15f0d45265117bcd6e6682f138ee60e60", ["0x0000000000000000000000000000000000000000000000000000000000000000"], "0x1928642"]` | proof_absent_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x00000000000000000000000000000000000000000000000000000000000012c6", "0x00000000000000000000000000000000000000000000000000000000000012d5"], "0x1928642"]` | proof_history_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_call [{"to": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x1928642"]` | call_weth_balance_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_call [{"to": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x1928642"]` | call_usdc_balance_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_getCode ["0x0000f90827f1c53a10cb7a02335b175320002935", "0x1928642"]` | code_history_26379842.json |  |
| 2026-11-09T21:08:28Z | https://ethereum-rpc.publicnode.com | `eth_blockNumber []` |  |  |
| 2026-11-09T21:08:29Z | https://rpc.mevblocker.io | `eth_getProof ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", ["0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0"], "0x1926633"]` | proof_weth_26371635.json |  |
| 2026-11-09T21:08:30Z | https://rpc.mevblocker.io | `eth_call [{"to": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x1926633"]` | call_weth_balance_26371635.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x1928642", false]` | block_26379842.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x1928641", false]` | block_26379841.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x1928632", false]` | block_26379826.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x1926633", false]` | block_26371635.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x1926632", false]` | block_26371634.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockReceipts ["0x1928642"]` | receipts_26379842.json |  |
| 2026-11-09T21:08:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockReceipts ["0x1928641"]` | receipts_26379841.json |  |
| 2026-11-09T21:08:31Z | https://eth.drpc.org | `eth_getBlockByNumber ["0x1928642", false]` | block_26379842_second.json |  |
| 2026-11-09T21:08:31Z | https://eth.drpc.org | `eth_getBlockByNumber ["0x1928641", false]` | block_26379841_second.json |  |
| 2026-11-09T21:08:31Z | https://eth.drpc.org | `eth_getBlockByNumber ["0x1928632", false]` | block_26379826_second.json |  |
| 2026-11-09T21:08:31Z | https://eth.drpc.org | `eth_getBlockByNumber ["0x1926633", false]` | block_26371635_second.json |  |
| 2026-11-09T21:08:31Z | https://ethereum-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/15613952` |  |  |
| 2026-11-09T21:08:31Z | https://lodestar-mainnet.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0xdab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3` | bootstrap.ssz | fulu |
| 2026-11-09T21:08:32Z | https://ethereum-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/15613952` | beacon_header_15613952_second.json |  |
| 2026-11-09T21:08:32Z | https://ethereum-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/15619456` | beacon_header_15619456_second.json |  |
