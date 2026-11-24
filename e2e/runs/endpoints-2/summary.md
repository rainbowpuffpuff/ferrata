# endpoints run endpoints-2: the summary

Every `wallet set_endpoints` the view sent to `ferrata_bridge_module` inside the headless Logos Core (the runner's log of its `logosctl call`s), in order; `session` is the deployment the module's session was opened on (`open`) and its Ethereum path (`eth`). Written by `e2e/endpoints/summary.py` from `calls.jsonl`.

| # | session | Ethereum RPC asked | LEZ sequencer asked | answer | ms |
|---:|---|---|---|---|---:|
| 1 | chain service http://127.0.0.1:18700/ | http://127.0.0.1:8546 | http://127.0.0.1:19301 | switched: eth_rpc, sequencer | 160 |
| 2 | chain service http://127.0.0.1:18700/ | http://127.0.0.1:8545 | http://127.0.0.1:19302 | switched: eth_rpc, sequencer | 160 |
| 3 | chain service http://127.0.0.1:18700/ | http://127.0.0.1:8545 | http://127.0.0.1:19399 | refused: not switched: the LEZ sequencer http://127.0.0.1:19399: getLastBlockId: client error (Connect). Kept: Ethereum RPC http://127.0.0.1:8545 (the chain service (seqnet-chain)), LEZ sequencer http://127.0.0.1:19302 (the chain service (seqnet-chain)) | 37 |
| 4 | chain service http://127.0.0.1:18700/ | http://127.0.0.1:39708 | http://127.0.0.1:19302 | refused: not switched: the Ethereum RPC http://127.0.0.1:39708: eth_chainId: error sending request for url (http://127.0.0.1:39708/): error sending request for url (http://127.0.0.1:39708/): client error (Connect): tcp connect error: Connection refused (os error 111). Kept: Ethereum RPC http://127.0.0.1:8545 (the chain service (seqnet-chain)), LEZ sequencer http://127.0.0.1:19302 (the chain servi… | 49 |
| 5 | chain service http://127.0.0.1:18700/ | http://127.0.0.1:39708 | http://127.0.0.1:19300 | refused: not switched: the Ethereum RPC http://127.0.0.1:39708: eth_chainId: error sending request for url (http://127.0.0.1:39708/): error sending request for url (http://127.0.0.1:39708/): client error (Connect): tcp connect error: Connection refused (os error 111). Kept: Ethereum RPC http://127.0.0.1:8545 (the chain service (seqnet-chain)), LEZ sequencer http://127.0.0.1:19302 (the chain servi… | 49 |
| 6 | chain service http://127.0.0.1:18700/ | http://127.0.0.1:8545 | http://127.0.0.1:19300 | switched: sequencer | 130 |

6 calls: 3 switched an endpoint, 3 refused (the endpoints in use kept).

The owners' own records after each step (`ops.sh`: the chain service's `module.endpoints`, eth_rpc_module's `get_chain_config`), in order:

- `chain 18700` exit 0: `{"chain_id":11155111,"config_id":"0xae4fc07b281473feb6b892cfe981f35adf2acd903b9675ee6f55cb1319f2887d","eth_rpc":"http://127.0.0.1:8546","program":"0x8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d","sequencer":"http://127.0.0.1:19301","vault":"0x0012962bc1b196d230cb1a466f5d39409a6e…`
- `chain 18700` exit 0: `{"chain_id":11155111,"config_id":"0xae4fc07b281473feb6b892cfe981f35adf2acd903b9675ee6f55cb1319f2887d","eth_rpc":"http://127.0.0.1:8545","program":"0x8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d","sequencer":"http://127.0.0.1:19302","vault":"0x0012962bc1b196d230cb1a466f5d39409a6e…`
- `chain 18700` exit 0: `{"chain_id":11155111,"config_id":"0xae4fc07b281473feb6b892cfe981f35adf2acd903b9675ee6f55cb1319f2887d","eth_rpc":"http://127.0.0.1:8545","program":"0x8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d","sequencer":"http://127.0.0.1:19302","vault":"0x0012962bc1b196d230cb1a466f5d39409a6e…`
- `chain 18700` exit 0: `{"chain_id":11155111,"config_id":"0xae4fc07b281473feb6b892cfe981f35adf2acd903b9675ee6f55cb1319f2887d","eth_rpc":"http://127.0.0.1:8545","program":"0x8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d","sequencer":"http://127.0.0.1:19302","vault":"0x0012962bc1b196d230cb1a466f5d39409a6e…`
- `chain 18700` exit 0: `{"chain_id":11155111,"config_id":"0xae4fc07b281473feb6b892cfe981f35adf2acd903b9675ee6f55cb1319f2887d","eth_rpc":"http://127.0.0.1:8545","program":"0x8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d","sequencer":"http://127.0.0.1:19302","vault":"0x0012962bc1b196d230cb1a466f5d39409a6e…`
- `chain 18700` exit 0: `{"chain_id":11155111,"config_id":"0xae4fc07b281473feb6b892cfe981f35adf2acd903b9675ee6f55cb1319f2887d","eth_rpc":"http://127.0.0.1:8545","program":"0x8face982c2b7368a23541e8b00f94f163fafd3f47f7ab624db2899255144e68d","sequencer":"http://127.0.0.1:19300","vault":"0x0012962bc1b196d230cb1a466f5d39409a6e…`

Qt Quick Test (`qmltest.txt`):

- PASS   : ferrata_miniapp_logos::FerrataEndpointsLogos::initTestCase()
- PASS   : ferrata_miniapp_logos::FerrataEndpointsLogos::test_1_local()
- SKIP   : ferrata_miniapp_logos::FerrataEndpointsLogos::test_2_live() U9_PUBLIC=1 runs the live deployment's endpoints (public Sepolia RPCs, our public LEZ network), read only
- PASS   : ferrata_miniapp_logos::FerrataEndpointsLogos::cleanupTestCase()
- Totals: 3 passed, 0 failed, 1 skipped, 0 blacklisted, 3219ms
