# Mainnet light-client data, sync-committee periods 1899 to 1903 (Fulu)

Fetched on 6 Nov 2026 from a public beacon node's standard light-client API, as SSZ
(`Accept: application/octet-stream`):

- `bootstrap.ssz`: `GET https://lodestar-mainnet.chainsafe.io/eth/v1/beacon/light_client/bootstrap/0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac` (`Eth-Consensus-Version: fulu`). The block is the finalized checkpoint at slot 15,556,608, the first slot of period 1899.
- `update_0.ssz` … `update_4.ssz`: `GET …/eth/v1/beacon/light_client/updates?start_period=1899&count=5`. The response is a sequence of chunks, each `u64 length ‖ fork digest ‖ SSZ`. All five carry digest `0x8c9f62fe`, which is mainnet Fulu after the second blob-parameter fork (epoch 419,072). The 4-byte digest is stripped here.

Independent check: the finalized block roots of the five updates equal the canonical block roots that a
second provider (`ethereum-beacon-api.publicnode.com`, `GET /eth/v1/beacon/headers/{slot}`) reports for those slots:

| slot | canonical root |
|---|---|
| 15,556,608 | 0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac |
| 15,564,800 | 0x96949350abfe8a04e6f1ae22b3064fdcee177dec32133348a2a69d200151e09a |
| 15,572,992 | 0x7e2771b1939afa205540003849bca0b0aa6ee71dd5a767cca2e35e084a008a4c |
| 15,581,184 | 0xe7c0ae95dd6a82d9eca7d9dc4e1b42a4c8197fbb0e4cd880bc5f9631de2b992b |
| 15,589,599 | 0x5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573 |

`SHA256SUMS` pins the files.
