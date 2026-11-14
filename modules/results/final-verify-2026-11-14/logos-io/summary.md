# Logos Core's own modules beside Ferrata's (logos-io.sh)

## Reads through eth_rpc_module, compared

| run | reply | path | routes (eth_rpc_module's) | equal to |
|---|---|---|---|---|
| e2e | `fetch_transfer` | eth_rpc_module | `["direct","direct"]` | the plain-HTTP read: **true** |
| e2e | `fetch_largest_receipt` | eth_rpc_module | `["direct","direct"]` | the plain-HTTP read: **true** |
| e2e | `dry_storage` | eth_rpc_module | `["direct"]` | the plain-HTTP read: **true** |
| e2e | `tx_storage` | eth_rpc_module | `["direct"]` | the plain-HTTP read: **true** |
| bridge-e2e | `import_read_plain` | http | `["http"]` | the proofs given: **true** |
| bridge-e2e | `import_read_eth` | eth_rpc_module | `["direct"]` | the proofs given: **true** |

## What the recorded server saw (e2e.sh)

| method | Accept | User-Agent | requests |
|---|---|---|---:|
| eth_getBlockByNumber | */* | None | 2 |
| eth_getBlockByNumber | application/json | None | 2 |
| eth_getBlockReceipts | */* | None | 2 |
| eth_getBlockReceipts | application/json | None | 2 |
| eth_getProof | */* | None | 2 |
| eth_getProof | application/json | None | 2 |

## End to end through the modules

- `e2e`: 43 checks, 0 failed (call mode: logosctl; Ethereum read via: eth_rpc_module)
- `bridge-e2e`: 24 checks, 0 failed (call mode: logosctl; Ethereum read via: eth_rpc_module)

## The send stack against anvil (tx_sender_module.prepare)

```

## eth_rpc_module chain 31337 = anvil
{"method":"set_chain_config","module":"eth_rpc_module","result":true,"status":"ok"}

## eth_rpc_module block_number
{"method":"block_number","module":"eth_rpc_module","result":"{\"ok\":true,\"result\":\"0x0\",\"route\":\"direct\"}","status":"ok"}

## tx_sender_module prepare
{"code":"MODULE_NOT_LOADED","message":"Module 'tx_sender_module' is not loaded. Load it with: logosctl module load tx_sender_module","status":"error"}

## keystore_module caller_identity
{"code":"MODULE_NOT_LOADED","message":"Module 'keystore_module' is not loaded. Load it with: logosctl module load keystore_module","status":"error"}
caller identity: unknown
```

## Steps

| step | result | seconds |
|---|---|---:|
| src-eth_rpc_module | ok | 2 |
| build-eth_rpc_module | ok | 5 |
| build-beacon_api_module | ok | 6 |
| build-keystore_module | FAILED (exit 1) | 28 |
| build-signer_manager_module | ok | 7 |
| build-fee_module | ok | 6 |
| build-tx_sender_module | ok | 6 |
| check | ok | 57 |
| daemon-start | ok | 0 |
| install-eth_rpc_module | ok | 0 |
| install-beacon_api_module | ok | 1 |
| install-signer_manager_module | ok | 1 |
| install-fee_module | ok | 0 |
| install-tx_sender_module | ok | 0 |
| install-ferrata_attest_module | ok | 1 |
| install-ferrata_bridge_module | ok | 0 |
| load-eth_rpc_module | ok | 0 |
| load-ferrata_attest_module | ok | 0 |
| load-ferrata_bridge_module | ok | 0 |
| load-beacon_api_module | ok | 1 |
| load-tx_sender_module | FAILED (exit 3) | 0 |
| e2e | ok | 68 |
| bridge-e2e | ok | 22 |

**Step failures 2, comparison or end-to-end failures 0.**
