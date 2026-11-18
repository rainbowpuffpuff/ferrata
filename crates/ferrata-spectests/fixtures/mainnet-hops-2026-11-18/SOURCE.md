# EIP-2935 hop chains on mainnet from block 26384851

Fetched by `lez/scripts/fetch-hop-chain.py --network mainnet --record 26384851 --chain 26384835,26376644,26368453,26360262 --receipts 26360262 --storage 26360262:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2:0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0:balanceOf(address):0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc --header 26384852 --header 26376659 OUTDIR` between 2026-11-18T16:09:30Z and 2026-11-18T16:09:44Z (UTC). Every request went to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`. Responses are stored byte for byte; each `header_N.rlp` is rebuilt from `block_N.json` and was checked to hash to the hash both providers gave; each hop's history slot was checked to hold that hash; `fixture.json` records the choices. `SHA256SUMS` pins every file.

Chains (newest first, from the record's block): 4 hops to 26360262.

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-18T16:09:30Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19299d4", false]` | block_26384852.json |  |
| 2026-11-18T16:09:30Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19299d4", false]` | block_26384852_second.json |  |
| 2026-11-18T16:09:31Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19299d3", false]` | block_26384851.json |  |
| 2026-11-18T16:09:31Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19299d3", false]` | block_26384851_second.json |  |
| 2026-11-18T16:09:32Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19299c3", false]` | block_26384835.json |  |
| 2026-11-18T16:09:32Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19299c3", false]` | block_26384835_second.json |  |
| 2026-11-18T16:09:33Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19279d3", false]` | block_26376659.json |  |
| 2026-11-18T16:09:34Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19279d3", false]` | block_26376659_second.json |  |
| 2026-11-18T16:09:34Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19279c4", false]` | block_26376644.json |  |
| 2026-11-18T16:09:36Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19279c4", false]` | block_26376644_second.json |  |
| 2026-11-18T16:09:36Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19259c5", false]` | block_26368453.json |  |
| 2026-11-18T16:09:36Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19259c5", false]` | block_26368453_second.json |  |
| 2026-11-18T16:09:37Z | https://ethereum-rpc.publicnode.com | `eth_getBlockByNumber ["0x19239c6", false]` | block_26360262.json |  |
| 2026-11-18T16:09:37Z | https://rpc.mevblocker.io | `eth_getBlockByNumber ["0x19239c6", false]` | block_26360262_second.json |  |
| 2026-11-18T16:09:38Z | https://rpc.mevblocker.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000000658"], "0x19299d3"]` | proof_history_26384851.json |  |
| 2026-11-18T16:09:38Z | https://rpc.mevblocker.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000000658"], "0x19299c3"]` | proof_history_26384835.json |  |
| 2026-11-18T16:09:41Z | https://rpc.mevblocker.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000000658"], "0x19279c4"]` | proof_history_26376644.json |  |
| 2026-11-18T16:09:41Z | https://rpc.mevblocker.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", ["0x0000000000000000000000000000000000000000000000000000000000000658"], "0x19259c5"]` | proof_history_26368453.json |  |
| 2026-11-18T16:09:41Z | https://rpc.mevblocker.io | `eth_getProof ["0x0000f90827f1c53a10cb7a02335b175320002935", [], "0x19239c6"]` | proof_history_26360262.json |  |
| 2026-11-18T16:09:42Z | https://ethereum-rpc.publicnode.com | `eth_getBlockReceipts ["0x19239c6"]` | receipts_26360262.json |  |
| 2026-11-18T16:09:43Z | https://rpc.mevblocker.io | `eth_getProof ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", ["0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0"], "0x19239c6"]` | proof_0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2_26360262.json |  |
| 2026-11-18T16:09:44Z | https://rpc.mevblocker.io | `eth_call [{"to": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "data": "0x70a08231000000000000000000000000b4e16d0168e52d35cacd2c6185b44281ec28c9dc"}, "0x19239c6"]` | call_0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2_balanceOf_26360262.json |  |
