# e2e.sh (call mode: native)

| group | check | passed | detail |
|---|---|---|---|
| chain | harness info read | yes | instance 0xc112f29279d8b73e…, block 26384851, slot 15624480 |
| log transfer | fetch_log_proof over JSON-RPC: receipts rebuild the block's receiptsRoot | yes | 531 receipts, 3 proof nodes, 2 RPC calls |
| log transfer | the log located is the fixture's | yes | tx 0, log 0, token 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48 |
| log transfer | dry_run_attest on the record read from the chain: attested, code 0 | yes | id 0x4910a599ca62fab4… |
| log transfer | the attested fact is the provider's log | yes | 3 topics, data 0x000000000000000000… |
| log transfer | build_attest_tx names header(id, slot) | yes | message 2121 bytes |
| log transfer | the program decodes the module's bytes (canonical Borsh) | yes |  |
| log transfer | the transaction is applied | yes | gas_exec 92295, gas_stor 2519, tx 2519 bytes |
| log transfer | the program's event = native evaluation | yes |  |
| log transfer | the program's statement id = the module's dry-run id | yes |  |
| log transfer | read_attestation: event matches the dry run and the instance | yes | block 26384851 |
| log largest_receipt | fetch_log_proof over JSON-RPC: receipts rebuild the block's receiptsRoot | yes | 531 receipts, 3 proof nodes, 2 RPC calls |
| log largest_receipt | the log located is the fixture's | yes | tx 1, log 0, token 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2 |
| log largest_receipt | dry_run_attest on the record read from the chain: attested, code 0 | yes | id 0xc76c6d570eeb6ccc… |
| log largest_receipt | the attested fact is the provider's log | yes | 3 topics, data 0x000000000000000000… |
| log largest_receipt | build_attest_tx names header(id, slot) | yes | message 58310 bytes |
| log largest_receipt | the program decodes the module's bytes (canonical Borsh) | yes |  |
| log largest_receipt | the transaction is applied | yes | gas_exec 1614747, gas_stor 58708, tx 58708 bytes |
| log largest_receipt | the program's event = native evaluation | yes |  |
| log largest_receipt | the program's statement id = the module's dry-run id | yes |  |
| log largest_receipt | read_attestation: event matches the dry run and the instance | yes | block 26384851 |
| refusal: tampered receipt proof | dry run refuses | yes | code 110 ProofNodeMismatch |
| refusal: tampered receipt proof | the program refuses with the predicted code | yes | exit 110, gas_exec 67401 |
| refusal: another instance | the program refuses with WRONG_INSTANCE (211) | yes | exit 211 |
| bridge Burn | the burn note equals the specification vector | yes | r, memo, mac, C_out |
| bridge Burn | leaf, burn id and nullifier equal the vector | yes |  |
| bridge Burn | the bridge program decodes the message (canonical) to the same note | yes | 185 bytes |
| bridge Claim | claimable; nullifier and root equal the vector | yes |  |
| bridge Claim | the path equals the vector's | yes |  |
| bridge Claim | the bridge program decodes the message (canonical) to the same witness | yes | 1271 bytes |
| JSON-RPC | every read went over the network to the recorded-data server | yes | eth_getBlockByNumber x2, eth_getBlockReceipts x2, 3,301,898 bytes served |

Checks: 31, failed: 0.
