# check.sh summary

## Steps

| step | result | seconds |
|---|---|---:|
| versions | ok | 0.4 |
| guests | ok | 297.5 |
| stage | ok | 2.3 |
| sdk-source | ok | 1.2 |
| cargo-lockfiles | ok | 0.0 |
| requests | ok | 0.1 |
| bridge-requests | ok | 0.0 |
| daemon-requests | ok | 0.1 |
| native-test | ok | 697.6 |
| native-calls | ok | 1.8 |
| prepare-build | ok | 1.9 |
| build-lgx | ok | 671.1 |
| build-logosctl | ok | 62.9 |
| dev-daemon-start | ok | 0.4 |
| dev-install-ferrata_attest_module | ok | 0.5 |
| dev-load-ferrata_attest_module | ok | 0.1 |
| dev-show-ferrata_attest_module | ok | 0.0 |
| dev-install-ferrata_updater_module | ok | 4.4 |
| dev-load-ferrata_updater_module | ok | 0.1 |
| dev-show-ferrata_updater_module | ok | 0.0 |
| dev-module-ls | ok | 0.0 |
| dev-daemon-stop | ok | 0.6 |
| sizes | ok | 1.9 |

## Calls

| call | module | method | native verdict | code | expectations | dev: equal to native | dev: seconds |
|---|---|---|---|---:|---|---|---:|
| version | ferrata_attest_module | version | ferrata_attest_module |  | met | yes | 0.0 |
| attest_weth_balance | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.0 |
| attest_empty_slot | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.0 |
| attest_absent_account | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.0 |
| refuse_tampered_proof | ferrata_attest_module | dry_run_attest | refused | 110 | met | yes | 0.0 |
| refuse_other_block | ferrata_attest_module | dry_run_attest | refused | 110 | met | yes | 0.0 |
| refuse_wrong_chain | ferrata_attest_module | dry_run_attest | refused | 140 | met | yes | 0.0 |
| checkpoint_p1899 | ferrata_attest_module | verify_checkpoint | ok | 0 | met | yes | 0.1 |
| checkpoint_attest_fixture | ferrata_attest_module | verify_checkpoint | ok | 0 | met | yes | 0.1 |
| checkpoint_wrong_root | ferrata_attest_module | verify_checkpoint | failed | 10 | met | yes | 0.1 |
| checkpoint_chain_mismatch | ferrata_attest_module | verify_checkpoint | failed |  | met | yes | 0.0 |
| updater_version | ferrata_updater_module | version | ferrata_updater_module |  | met | yes | 0.1 |
| updater_config | ferrata_updater_module | check_config | ok |  | met | yes | 0.0 |
| updater_config_binary | ferrata_updater_module | check_config | ok |  | met | yes | 0.0 |
| updater_config_unknown_field | ferrata_updater_module | check_config | invalid_request |  | met | yes | 0.0 |

## Replies (native)

- `version`: `{"bls_backend": "blst (advance runs the program's steps natively)", "methods": ["version", "dry_run_attest", "verify_checkpoint", "fetch_log_proof", "build_attest_tx", "read_attestation", "read_instance", "advance"], "module": "ferrata_attest_module", "staged": {"commit": "unknown", "crates": [{"from": "crates/ferrata-core", "name": "ferrata-core", "source_sha256": "f43255c33578f87ccd50386f936ea722ae2bbddc7fc60509e97a6eafc39864d8", "version": "0.0.1"}, {"from": "lez/wire", "name": "ferrata-lez-wire", "source_sha256": "759b29f064000656b2d3795496d72bdd0074ce13affc202e2edac6d860e4a183", "version" …`
- `attest_weth_balance`: `{"account_id": "0x7e5f4c87430552093b5b39e5195c6cd5c9210a2832f24b51638586730311555d", "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "code": 0, "fact": {"address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "kind": "storage", "slot": "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0", "value": "0x0000000000000000000000000000000000000000000000f22ca9b4cc5d0f3d61"}, "id": "0xabc1114e5c089b52e94b0980941b17a7d081c2444f5341b3823fdc4d3a1dfa53", "id_at_block": "0xfd7e6c60a6c14fee4d28571da503a22e6a3f64678d …`
- `attest_empty_slot`: `{"account_id": "0x7e5f4c87430552093b5b39e5195c6cd5c9210a2832f24b51638586730311555d", "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "code": 0, "fact": {"address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "kind": "storage", "slot": "0x00000000000000000000000000000000000000000000000000000000000000ff", "value": "0x0000000000000000000000000000000000000000000000000000000000000000"}, "id": "0x49bb018ccdfed73fe40c55f138a279f692c2dd9334ee7b14fb077635edf0f5a7", "id_at_block": "0x5203696b1eae4e90077ec32e2fcc21dc3fa9085b66 …`
- `attest_absent_account`: `{"account_id": "0x567af131df4aab8b29530d46307672993c062f7c49e9680b250dcb3c41d91a2e", "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "code": 0, "fact": {"address": "0x3e05d7d15f0d45265117bcd6e6682f138ee60e60", "kind": "storage", "slot": "0x0000000000000000000000000000000000000000000000000000000000000000", "value": "0x0000000000000000000000000000000000000000000000000000000000000000"}, "id": "0xba5e00d8343ced08f8be5c4acbb0e32ae692f099f08b0fcb8821b656afe9cfbf", "id_at_block": "0x356cfa3c7d9cd95e7303711c8bc4cd96c22ed4be22 …`
- `refuse_tampered_proof`: `{"code": 110, "error": "ProofNodeMismatch", "record": {"anchor_slot": 15624480, "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "receipts_root": "0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2", "state_root": "0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e"}, "stage": "proof", "verdict": "refused"}`
- `refuse_other_block`: `{"code": 110, "error": "ProofNodeMismatch", "record": {"anchor_slot": 15624480, "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "receipts_root": "0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2", "state_root": "0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e"}, "stage": "proof", "verdict": "refused"}`
- `refuse_wrong_chain`: `{"code": 140, "error": "ChainMismatch", "record": {"anchor_slot": 15624480, "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "receipts_root": "0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2", "state_root": "0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e"}, "stage": "proof", "verdict": "refused"}`
- `checkpoint_p1899`: `{"bootstrap": {"code": 0, "header_root": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "slot": 15556608, "verdict": "accepted"}, "chain_pair": {"network": "mainnet", "status": "known"}, "checkpoint": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "committee": {"check": "KeyValidate on every key (subgroup included); the keys sum to aggregate_pubkey", "code": 0, "keys": 512, "verdict": "accepted"}, "execution": null, "failed": [], "instance_id": "0x39860f39c97574b5eed493bdfce3429de1b8ff1b1f2a2452e72292460953b79c", "instance_id_matches": true, "min_p …`
- `checkpoint_attest_fixture`: `{"bootstrap": {"code": 0, "header_root": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "slot": 15622144, "verdict": "accepted"}, "chain_pair": {"network": "mainnet", "status": "known"}, "checkpoint": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "committee": {"check": "KeyValidate on every key (subgroup included); the keys sum to aggregate_pubkey", "code": 0, "keys": 512, "verdict": "accepted"}, "execution": null, "failed": [], "instance_id": "0x599a0de5f1dc6248b4e369c2aa066e2c66370f1d1027f16673639893309e4fcd", "instance_id_matches": null, "min_p …`
- `checkpoint_wrong_root`: `{"bootstrap": {"code": 10, "error": "CheckpointMismatch", "header_root": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "slot": 15622144, "stage": "init", "verdict": "refused"}, "chain_pair": {"network": "mainnet", "status": "known"}, "checkpoint": "0x46d069655d60786d109a3abec9a63e4db77560b9ca7e588c82f13ef988a4f40f", "committee": {"check": "KeyValidate on every key (subgroup included); the keys sum to aggregate_pubkey", "code": 0, "keys": 512, "verdict": "accepted"}, "execution": null, "failed": ["bootstrap", "observed"], "instance_id": "0x7b5a448d1517806feadfe8f762a42ef …`
- `checkpoint_chain_mismatch`: `{"bootstrap": null, "chain_pair": {"network": "mainnet", "status": "mismatch"}, "checkpoint": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "committee": null, "execution": null, "failed": ["chain_pair"], "instance_id": "0xe54b2c63a3f3c5b5b8406d7ea3b730136b81210315f787cf39ad972a26845543", "instance_id_matches": null, "min_providers": 0, "observed": [], "observed_agreeing": 0, "providers": [], "providers_agreeing": 0, "required_agreeing": 0, "verdict": "failed"}`
- `updater_version`: `{"dev_mode": "1", "ledgers": ["harness", "sequencer"], "methods": ["version", "check_config", "run", "start", "status", "metrics", "stop"], "module": "ferrata_updater_module", "program": {"bytes": 627064, "computed_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df", "declared_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df", "name": "ferrata_lez"}, "updater": {"circuits": {"artifact": "logos-blockchain-circuits-v0.5.7-linux-x86_64", "files_sha256": "d9f24f8912aca76dbab149292dc1ba156b9882ef5563b239c8e6025904884325"}, "commit": "unknown", "guests": …`
- `updater_config`: `{"chains": [{"checkpoint": "4baa732e5dbdbe97932abcf62c7caccc1cba8e9c14118d153aea629e25a4abff", "layout": null, "name": "mainnet", "preset": "mainnet", "sources": [{"chain_id": null, "dir": "../../../lez/lab/results/record/net/mainnet", "kind": "light-client", "name": "record-a", "url": null, "via": null}, {"chain_id": null, "dir": "../../../lez/lab/results/record/net/mainnet", "kind": "light-client", "name": "record-b", "url": null, "via": null}]}, {"checkpoint": "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d", "layout": null, "name": "sepolia", "preset": "sepolia", "sources …`
- `updater_config_binary`: `{"chains": [{"checkpoint": "3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c", "layout": "gloas", "name": "sepolia", "preset": "sepolia", "sources": [{"chain_id": null, "dir": null, "kind": "light-client", "name": "lodestar", "url": "https://lodestar-sepolia.chainsafe.io", "via": null}, {"chain_id": null, "dir": null, "kind": "headers", "name": "publicnode", "url": "https://ethereum-sepolia-beacon-api.publicnode.com", "via": null}]}, {"checkpoint": "4baa732e5dbdbe97932abcf62c7caccc1cba8e9c14118d153aea629e25a4abff", "layout": null, "name": "mainnet", "preset": "mainnet", "source …`

Step failures: 0. Check failures: 0.
