# check.sh summary

## Steps

| step | result | seconds |
|---|---|---:|
| versions | ok | 0.1 |
| stage | ok | 0.1 |
| sdk-source | ok | 0.2 |
| cargo-lockfiles | ok | 0.0 |
| requests | ok | 0.1 |
| bridge-requests | ok | 0.1 |
| native-test | ok | 4.1 |
| native-calls | ok | 0.7 |
| prepare-build | ok | 0.2 |
| build-lgx | ok | 196.2 |
| build-logosctl | ok | 37.8 |
| dev-daemon-start | ok | 0.3 |
| dev-install-ferrata_attest_module | ok | 0.4 |
| dev-load-ferrata_attest_module | ok | 0.1 |
| dev-show-ferrata_attest_module | ok | 0.0 |
| dev-install-ferrata_bridge_module | ok | 0.7 |
| dev-load-ferrata_bridge_module | ok | 0.1 |
| dev-show-ferrata_bridge_module | ok | 0.1 |
| dev-e2e | ok | 70.5 |
| dev-bridge-e2e | ok | 19.1 |
| dev-module-ls | ok | 0.1 |
| dev-daemon-stop | ok | 0.7 |
| sizes | ok | 0.7 |

## Calls

| call | module | method | native verdict | code | expectations | dev: equal to native | dev: seconds |
|---|---|---|---|---:|---|---|---:|
| version | ferrata_attest_module | version | ferrata_attest_module |  | met | yes | 0.1 |
| attest_weth_balance | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.0 |
| attest_empty_slot | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.0 |
| attest_absent_account | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.0 |
| refuse_tampered_proof | ferrata_attest_module | dry_run_attest | refused | 110 | met | yes | 0.0 |
| refuse_other_block | ferrata_attest_module | dry_run_attest | refused | 110 | met | yes | 0.0 |
| refuse_wrong_chain | ferrata_attest_module | dry_run_attest | refused | 140 | met | yes | 0.1 |
| checkpoint_p1899 | ferrata_attest_module | verify_checkpoint | ok | 0 | met | yes | 0.0 |
| checkpoint_attest_fixture | ferrata_attest_module | verify_checkpoint | ok | 0 | met | yes | 0.0 |
| checkpoint_wrong_root | ferrata_attest_module | verify_checkpoint | failed | 10 | met | yes | 0.1 |
| checkpoint_chain_mismatch | ferrata_attest_module | verify_checkpoint | failed |  | met | yes | 0.0 |
| bridge_version | ferrata_bridge_module | version | ferrata_bridge_module |  | met | yes | 0.0 |
| bridge_deposit_case0 | ferrata_bridge_module | deposit | ok |  | met | yes | 0.0 |
| bridge_deposit_case1 | ferrata_bridge_module | deposit | ok |  | met | yes | 0.0 |
| bridge_deposit_case2 | ferrata_bridge_module | deposit | ok |  | met | yes | 0.0 |
| bridge_claim_vector | ferrata_bridge_module | claim | claimable |  | met | yes | 0.0 |
| bridge_claim_wrong_amount | ferrata_bridge_module | claim | refused |  | met | yes | 0.0 |
| bridge_burn_case0 | ferrata_bridge_module | burn | ok |  | met | yes | 0.0 |
| bridge_burn_case1 | ferrata_bridge_module | burn | ok |  | met | yes | 0.0 |
| bridge_burn_case2 | ferrata_bridge_module | burn | ok |  | met | yes | 0.0 |
| bridge_release_vector | ferrata_bridge_module | release | releasable |  | met | yes | 0.0 |
| bridge_release_wrong_recipient | ferrata_bridge_module | release | refused |  | met | yes | 0.0 |
| bridge_recover | ferrata_bridge_module | recover | ok |  | met | yes | 0.0 |
| bridge_status | ferrata_bridge_module | status | ok |  | met | yes | 0.0 |

## End to end through Logos Core (dev): 43 checks, 0 failed

| group | check | passed | detail |
|---|---|---|---|
| chain | harness info read | yes | instance 0x599a0de5f1dc6248…, block 26384851, slot 15624480 |
| chain | read_instance decodes the instance's state; the id recomputed from its config and checkpoint | yes | chain 1, finalized slot 15624480, 2 jobs applied, store Electra |
| log transfer | fetch_log_proof over JSON-RPC: receipts rebuild the block's receiptsRoot | yes | 531 receipts, 3 proof nodes, 2 RPC calls |
| log transfer | the log located is the fixture's | yes | tx 0, log 0, token 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48 |
| log transfer | dry_run_attest on the record read from the chain: attested, code 0 | yes | id 0x4910a599ca62fab4… |
| log transfer | the attested fact is the provider's log | yes | 3 topics, data 0x000000000000000000… |
| log transfer | build_attest_tx names header(id, slot) | yes | message 2121 bytes |
| log transfer | the program decodes the module's bytes (canonical Borsh) | yes |  |
| log transfer | the transaction is applied | yes | gas_exec 93316, gas_stor 2519, tx 2519 bytes |
| log transfer | the program's event = native evaluation | yes |  |
| log transfer | the program's statement ids = the module's dry-run ids | yes |  |
| log transfer | read_attestation: event matches the dry run and the instance | yes | block 26384851 |
| log largest_receipt | fetch_log_proof over JSON-RPC: receipts rebuild the block's receiptsRoot | yes | 531 receipts, 3 proof nodes, 2 RPC calls |
| log largest_receipt | the log located is the fixture's | yes | tx 1, log 0, token 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2 |
| log largest_receipt | dry_run_attest on the record read from the chain: attested, code 0 | yes | id 0xc76c6d570eeb6ccc… |
| log largest_receipt | the attested fact is the provider's log | yes | 3 topics, data 0x000000000000000000… |
| log largest_receipt | build_attest_tx names header(id, slot) | yes | message 58310 bytes |
| log largest_receipt | the program decodes the module's bytes (canonical Borsh) | yes |  |
| log largest_receipt | the transaction is applied | yes | gas_exec 1615769, gas_stor 58708, tx 58708 bytes |
| log largest_receipt | the program's event = native evaluation | yes |  |
| log largest_receipt | the program's statement ids = the module's dry-run ids | yes |  |
| log largest_receipt | read_attestation: event matches the dry run and the instance | yes | block 26384851 |
| storage slot (WETH balance) | dry_run_attest: attested, the provider's value | yes | value 0x000000000000000000… |
| storage slot (WETH balance) | build_attest_tx names header(id, slot); two facts (the account, then the slot) | yes | message 6906 bytes |
| storage slot (WETH balance) | decoded (canonical), applied, event = native = dry-run ids (account, slot) | yes | gas_exec 274613, gas_stor 7304 |
| storage slot (WETH balance) | read_attestation matches | yes |  |
| refusal: tampered receipt proof | dry run refuses | yes | code 110 ProofNodeMismatch |
| refusal: tampered receipt proof | the program refuses with the predicted code | yes | exit 110, gas_exec 68357 |
| refusal: another instance | the program refuses with WRONG_INSTANCE (211) | yes | exit 211 |
| advance (finality update) | advance plans the update with the updater's code: useful, it advances the instance | yes | job 0xfdb24409059fc23b…, finalized slot 15624480, 505 signers |
| advance (finality update) | its steps are Open, Miller, Finish (a finality update adopts no committee) | yes | open 4990 B, miller 1 B, finish 1 B |
| advance (finality update) | the program decodes each message (canonical) and applies each transaction | yes | Open: gas_exec 2330188, gas_stor 5452; Miller: gas_exec 3308048, gas_stor 399; Finish: gas_exec 3723960, gas_stor 527 |
| advance (finality update) | the instance's finalized slot moves to the update's; its record is written | yes | 15622144 -> 15624480 |
| advance (finality update) | the instance and the record equal those the harness's own job leaves (job id equal) | yes | harness job: 3 transactions |
| advance (finality update) | read_instance on the advanced state: the new finalized slot, 2 jobs applied | yes | finalized slot 15624480, applied 2 |
| advance (finality update) | the same update against the advanced instance: no change | yes | no_change |
| bridge Burn | the burn note equals the specification vector | yes | r, memo, mac, C_out |
| bridge Burn | leaf, burn id and nullifier equal the vector | yes |  |
| bridge Burn | the bridge program decodes the message (canonical) to the same note | yes | 185 bytes |
| bridge Claim | claimable; nullifier and root equal the vector | yes |  |
| bridge Claim | the path equals the vector's | yes |  |
| bridge Claim | the bridge program decodes the message (canonical) to the same witness | yes | 1271 bytes |
| JSON-RPC | every read went over the network to the recorded-data server | yes | eth_getBlockByNumber x2, eth_getBlockReceipts x2, 3,301,898 bytes served |

## Bridge module end to end through Logos Core (dev, the mini-app devnet): 24 checks, 0 failed

| group | check | passed | detail |
|---|---|---|---|
| devnet | two deposits to the user's private account /0 | yes | amount 100000000000000000 wei each, vault 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9 |
| import_root | import_root dry-runs the import with the programs' functions: importable, root(cid, 2) as the vault holds it | yes | record slot 9000064, message 1231 bytes |
| import_root | the call names config(cid) (root, its seed), header(id, slot) and root(cid, n) | yes |  |
| import_root | a wrong leaf count: the dry run refuses with IMPORT_MISMATCH (247) | yes | IMPORT_MISMATCH |
| import_root | submitted, the program refuses it with the same code | yes | exit 247 |
| import_root | the module's ImportRoot is applied on the zone | yes | gas_exec 231020, gas_stor 1757 |
| import_root | (stand-in) the relayer's ImportFees for the same block | yes |  |
| registry | token(cid, ETH) decoded: the deposit's amount is a denomination | yes | denominations ['100000000000000000', '1000000000000000000', '10000000000000000000'], relay fees ['100000000000000', '1000000000000000', '10000000000000000'] |
| registry | a claim of leaf 0: range from the token's schedules, mint and relay fees | yes | range {'from_leaf': 0, 'index': 0, 'until_leaf': 2}, claim {'denomination_index': 0, 'mint_fee': '0', 'relay_fee': '100000000000000', 'value': '100000000'} |
| registry | config(cid) decoded; configId recomputed from it equals the devnet's | yes | instance 0x1d1d1d1d1d1d1d1d… |
| registry | root(cid, 2) decoded | yes |  |
| claim (private) | deposit 0: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor | yes | 1239 bytes, 6 accounts, mint fee 0 |
| claim (private) | deposit 0: proven and settled privately on the zone | yes | proof dev, prove 815 ms, balance 100000000000000000 |
| claim (private) | deposit 1: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor | yes | 1239 bytes, 6 accounts, mint fee 0 |
| claim (private) | deposit 1: proven and settled privately on the zone | yes | proof dev, prove 870 ms, balance 200000000000000000 |
| claim (private) | deposit 0 again: refused from the nullifier's account (ALREADY_CLAIMED, 250) | yes |  |
| claim (private) | the private account holds both denominations | yes | balance 200000000000000000 |
| release (the wallet's rule) | two accepted roots: over burn 0 (delay passed) and over both (in its delay) | yes | now 1794516414, opens [1794515633, 1794516534] |
| release (the wallet's rule) | burn 0: the newest root whose delay has passed is the older one | yes | picked checkpoint 0, root 0x89409baec579a74d… |
| release (the wallet's rule) | burn 1: only the newer root covers it, in its delay: ReleaseDelayNotElapsed, with when it opens | yes | first open 1794516534 |
| release (the wallet's rule) | after the delay: burn 1 under the newer root | yes |  |
| prove_release | the release guest run on the module's input through r0vm: its journal is the native one | yes | image 0xb64900bcf7416685…, dev mode True, 56 ms, receipt 2344 bytes |
| vault | both releases the module planned are accepted by the vault (mock verifier on the exact journals) | yes | paid 100000000000000000 and 100000000000000000 wei |
| status | both burns released | yes |  |

## Replies (native)

- `version`: `{"bls_backend": "blst (advance runs the program's steps natively)", "methods": ["version", "dry_run_attest", "verify_checkpoint", "fetch_log_proof", "build_attest_tx", "read_attestation", "read_instance", "advance"], "module": "ferrata_attest_module", "staged": {"commit": "61e7b3d60583dc23bfc856e1b50f62ea1c734d58", "crates": [{"from": "crates/ferrata-core", "name": "ferrata-core", "source_sha256": "f43255c33578f87ccd50386f936ea722ae2bbddc7fc60509e97a6eafc39864d8", "version": "0.0.1"}, {"from": "lez/wire", "name": "ferrata-lez-wire", "source_sha256": "759b29f064000656b2d3795496d72bdd0074ce13aff …`
- `attest_weth_balance`: `{"account_id": "0x7e5f4c87430552093b5b39e5195c6cd5c9210a2832f24b51638586730311555d", "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "code": 0, "fact": {"address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "kind": "storage", "slot": "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0", "value": "0x0000000000000000000000000000000000000000000000f22ca9b4cc5d0f3d61"}, "id": "0xabc1114e5c089b52e94b0980941b17a7d081c2444f5341b3823fdc4d3a1dfa53", "id_at_block": "0xfd7e6c60a6c14fee4d28571da503a22e6a3f64678d …`
- `attest_empty_slot`: `{"account_id": "0x7e5f4c87430552093b5b39e5195c6cd5c9210a2832f24b51638586730311555d", "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "code": 0, "fact": {"address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "kind": "storage", "slot": "0x00000000000000000000000000000000000000000000000000000000000000ff", "value": "0x0000000000000000000000000000000000000000000000000000000000000000"}, "id": "0x49bb018ccdfed73fe40c55f138a279f692c2dd9334ee7b14fb077635edf0f5a7", "id_at_block": "0x5203696b1eae4e90077ec32e2fcc21dc3fa9085b66 …`
- `attest_absent_account`: `{"account_id": "0x567af131df4aab8b29530d46307672993c062f7c49e9680b250dcb3c41d91a2e", "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "code": 0, "fact": {"address": "0x3e05d7d15f0d45265117bcd6e6682f138ee60e60", "kind": "storage", "slot": "0x0000000000000000000000000000000000000000000000000000000000000000", "value": "0x0000000000000000000000000000000000000000000000000000000000000000"}, "id": "0xba5e00d8343ced08f8be5c4acbb0e32ae692f099f08b0fcb8821b656afe9cfbf", "id_at_block": "0x356cfa3c7d9cd95e7303711c8bc4cd96c22ed4be22 …`
- `refuse_tampered_proof`: `{"code": 110, "error": "ProofNodeMismatch", "record": {"anchor_slot": 15624480, "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "receipts_root": "0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2", "state_root": "0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e"}, "stage": "proof", "verdict": "refused"}`
- `refuse_other_block`: `{"code": 110, "error": "ProofNodeMismatch", "record": {"anchor_slot": 15624480, "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "receipts_root": "0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2", "state_root": "0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e"}, "stage": "proof", "verdict": "refused"}`
- `refuse_wrong_chain`: `{"code": 140, "error": "ChainMismatch", "record": {"anchor_slot": 15624480, "block_hash": "0x3e02e5baa81bf1604b6447e9ca014a1ca85e3c7785ca6b9a5430b6c93cba7bce", "block_number": 26384851, "chain_id": 1, "receipts_root": "0x9f7aae4f6f937ea551a4ca11c31e71be6a309d1aeccbdca49dfd3eff7d4379e2", "state_root": "0x219719cc8484e9aed746f92b10c47e48f4213fbb0f3074afecd9bb020db4247e"}, "stage": "proof", "verdict": "refused"}`
- `checkpoint_p1899`: `{"bootstrap": {"code": 0, "header_root": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "slot": 15556608, "verdict": "accepted"}, "chain_pair": {"network": "mainnet", "status": "known"}, "checkpoint": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "failed": [], "instance_id": "0x39860f39c97574b5eed493bdfce3429de1b8ff1b1f2a2452e72292460953b79c", "instance_id_matches": true, "observed": [{"agrees": true, "root": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "source": "second beacon provider, slot 15556608 (lez/harness EXPECTED …`
- `checkpoint_attest_fixture`: `{"bootstrap": {"code": 0, "header_root": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "slot": 15622144, "verdict": "accepted"}, "chain_pair": {"network": "mainnet", "status": "known"}, "checkpoint": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "failed": [], "instance_id": "0x599a0de5f1dc6248b4e369c2aa066e2c66370f1d1027f16673639893309e4fcd", "instance_id_matches": null, "observed": [{"agrees": true, "root": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "source": "ethereum-beacon-api.publicnode.com, /eth/v1/beacon/headers/ …`
- `checkpoint_wrong_root`: `{"bootstrap": {"code": 10, "error": "CheckpointMismatch", "header_root": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433", "slot": 15622144, "stage": "init", "verdict": "refused"}, "chain_pair": {"network": "mainnet", "status": "known"}, "checkpoint": "0x46d069655d60786d109a3abec9a63e4db77560b9ca7e588c82f13ef988a4f40f", "failed": ["bootstrap", "observed"], "instance_id": "0x7b5a448d1517806feadfe8f762a42ef38212574d2e157bb8fb29d7e69e77d348", "instance_id_matches": null, "observed": [{"agrees": false, "root": "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433" …`
- `checkpoint_chain_mismatch`: `{"bootstrap": null, "chain_pair": {"network": "mainnet", "status": "mismatch"}, "checkpoint": "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac", "failed": ["chain_pair"], "instance_id": "0xe54b2c63a3f3c5b5b8406d7ea3b730136b81210315f787cf39ad972a26845543", "instance_id_matches": null, "observed": [], "observed_agreeing": 0, "required_agreeing": 0, "verdict": "failed"}`
- `bridge_version`: `{"methods": ["version", "deposit", "claim", "burn", "release", "recover", "status", "prove_release", "import_root", "registry"], "module": "ferrata_bridge_module", "spec": "v2", "staged": {"commit": "61e7b3d60583dc23bfc856e1b50f62ea1c734d58", "crates": [{"from": "crates/ferrata-core", "name": "ferrata-core", "source_sha256": "f43255c33578f87ccd50386f936ea722ae2bbddc7fc60509e97a6eafc39864d8", "version": "0.0.1"}, {"from": "bridge/crates/ferrata-bridge-core", "name": "ferrata-bridge-core", "source_sha256": "29349d5bd266412e48c4c0d1df40d609c0992301fe90922ea7176fb51183c4db", "version": "0.2.0"}, { …`
- `bridge_deposit_case0`: `{"commitment": "0xfeca64e68270002b498592eb9e94c83960660c22124ce0dc4693becdb982d994", "deposit_id": "0x11b636d17fb7e50c6426f49e6ac439c4c320070be71b57e919191f41a3003589", "kind": "private", "leaf": "0x63b02a52331676f290debc0bef44a5b4562bf2c4e31c20dc517ae97990a57e9f", "leaf_index": 0, "nullifier": "0xd9049bc8927d4a76ffdd8ac7387b801bd110a7cd628ae5fb22a44ef43198ac2d", "vault_call": {"data": "0x7fde1952000000000000000000000000d59b99d6b6394e84dc9c86bfe0502b68ed0acff60000000000000000000000000000000000000000000000000000000000000000feca64e68270002b498592eb9e94c83960660c22124ce0dc4693becdb982d99400000000 …`
- `bridge_deposit_case1`: `{"commitment": "0x98797c474a30d7f35b791c1c17d5f1964bfdb28170a32398813cf1cea60354ac", "deposit_id": "0x3eed65632601190fcc7f7502cf1e25bbc37df052bff760e910f17ef49783237b", "kind": "public", "leaf": "0x3f3da4efc146d1e2d5138ef8a7a802db50142c8c241b0bd7b9b8d6b37a3a728e", "leaf_index": 41, "nullifier": "0x13b66647c0dcb358dd662015f4f83e5384d0531cb69f8b37afcd3f9eadc073a9", "vault_call": {"data": "0x7fde1952000000000000000000000000c07d37f72c68b53042121f94913c8f2da71a9581000000000000000000000000000000000000000000000000000000000000000098797c474a30d7f35b791c1c17d5f1964bfdb28170a32398813cf1cea60354ac00000000 …`
- `bridge_deposit_case2`: `{"commitment": "0x4546c0d6c2d6bd776c946f1b875b48da9b42b6e28e06a31674919bfe551eb4b2", "deposit_id": "0x2f04c5ff0e24741cb893a58c4c5417151841375ed4b70493b27118aea91b655b", "kind": "private", "leaf": "0xaaf498c683cef5788ba0fc1d2347e77de44e8de0f5456c0b4d4a7d6e63364682", "leaf_index": 4294967295, "nullifier": "0xa0a3c1dc23b4e10f31cc7da08e0aa7cae2343b27a93efd38d863906087bb5f39", "vault_call": {"data": "0x7fde1952000000000000000000000000d881c54b2030adcbf7014c36c477d9476e1f989d00000000000000000000000000000000000000000000000000000000000000004546c0d6c2d6bd776c946f1b875b48da9b42b6e28e06a31674919bfe551eb4b …`
- `bridge_claim_vector`: `{"commitment": "0x2580f8635f7ec9336661194dcc8a81cc9deaad0325fd7d5c152833f14ee22163", "deposit_id": "0xa036267fe112924dc4839131399ff99ac3ecd26d545b631aef3296ca945ed91f", "kind": "public", "leaf": "0x93f3a50939e491f8ebe14916a3d31a517932ef11b1beb5975a94d30c7082132e", "nullifier": "0x57f4ad38820937881c7f9ddb991831c53c18025f090d9a7b766e17c06077e547", "path": {"directions": 2, "siblings": ["0x3f56037433be788a5375372eebd80758fc412f72d6e06a2388d1f83a4ae4d0ec", "0x7ef4afaa83e049f563146795d4c835a88adf077eeebcaf55df4aa4de94185689", "0xf986c56fb43d6ed1762a854b07a1a6e8873019310808d2475a792735de82a1ac", "0x …`
- `bridge_claim_wrong_amount`: `{"error": "RootMismatch", "leaf": "0xf771d99957275abb6169fc0339c354eaea578adf042a0cd6d5a157ea7aba32f2", "root": "0xfbff87727dacd07304897fd522396cd6f656ea53b8f024853734c31feb3127f6", "verdict": "refused"}`
- `bridge_burn_case0`: `{"burn_id": "0x93dc07c1a9f52bd0274cd2e666f187f600927f4f15d251c2271b0a304cc291d1", "burn_index": 0, "burn_leaf": "0xb79278d66c46b25f3bc170a29db7437f1b1b0b9e2d4a7f5d3b3c11aad4905cd7", "i": 0, "message": "0x00fb0c17416520e93c52f6f714a1f8c1f394a4390da85102dd734d2bfc50e3e9e89735f2361402a6387d7fb1c790d98095a3e38187f5b7465d5fab6909cd8dfe98374ceef07b7764b6d683bb07c053d6ba93ca6ce0c47920cb0032113ae1da1283b2e9a4a7d59b99d6b6394e84dc9c86bfe0502b68ed0acff600000000000000000000000000000000000000000000000006f05b59d3b20000eb7737840c58bf26176ec0ec41b12e1e4edf60d3b8fb0ecce4e92c7888363dde", "message_bytes": 185, " …`
- `bridge_burn_case1`: `{"burn_id": "0xd3ed1a52029555bbdcee527aa6038819a1a62f849d9819511002924f17580f51", "burn_index": 1234, "burn_leaf": "0x88eae9c866c876b3cce0316e3e8f675395bb3e6fd7d892e028cc5818cd1ddc53", "i": 7, "message": "0x00342a9e15327643c59ca9acd5ae81b5f8023a8aa5219e44d6ffefb4e1051849796d0f9708ea35589e4272749e6442059e7d6894fc8d5b4b24c33d8e6aa39053aa9fe5ed9cd0d75166d14f0327accd6d88ae296c5c7e80b87b4f8520598951519250128484c07d37f72c68b53042121f94913c8f2da71a95810000000000000000000000000000000000000000000000000de0b6b3a764000082110803373236ac84e7b9eddccadf1ceb5569200656746c7c192976a0fcf05d", "message_bytes": 185 …`
- `bridge_burn_case2`: `{"burn_id": "0x54b1a2b7978943237d83cbd2ffb51daf1904d2a96e95fe4acea3bac85af7d674", "burn_index": 4294967295, "burn_leaf": "0xde6feedf84ea04fd4e357312f96efe82419fb575f292c0dab4ce02f2483a992e", "i": 18446744073709551615, "message": "0x000e5068542ed2e82c8f3f97c27497357c65629c6be339619aab7ae4d7e8fd0b038d5853cb2f12987e2c849efd2ba7cecea44d09190f24225a1a97fb93a021e0ba3f8394616dedd2f79b0b084305077bfbcc48951fd82843c4a4d634f562429dba4531a098d881c54b2030adcbf7014c36c477d9476e1f989d00000000000000000000000000000000000000000000000014d1120d7b160000c7c5e04b5e53180fac2f69268f23abf310a6a7230e6e01159863b6acf89a53 …`
- `bridge_release_vector`: `{"burn_root": "0x135d4f47f3992bf60569020eb68650c18ad774766bcad7719893173a306a4175", "journal": "0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e510000000000000000000000000000000000000000000000000000000000007a690000000000000000000000007eca90a64a01212112c15bd4b32ff83b883f77100e5068542ed2e82c8f3f97c27497357c65629c6be339619aab7ae4d7e8fd0b03135d4f47f3992bf60569020eb68650c18ad774766bcad7719893173a306a4175332e5c5d2a1620b9fc4a56d4864f279abc7ed5541b2b5a7924ddc8a2736ea4b8000000000000000000000000464d0bb80e1ef69857d3a5bd58aac119bfdf75f1000000000000000000000000000000000000000000000000000000 …`
- `bridge_release_wrong_recipient`: `{"burn_root": "0x135d4f47f3992bf60569020eb68650c18ad774766bcad7719893173a306a4175", "error": "RootMismatch", "r": "0x05ec967992ad7c1d96f8207cf429662bc92ffb3c87784fbf657f1f6fc90bb5a5", "verdict": "refused"}`
- `bridge_recover`: `{"deposits": [{"account_index": 0, "amount": "0x00000000000000000000000000000000000000000000000000000000000f4240", "depositor": "0x7d9e5f529335bd113d2d4336fa6d2a5539dadb80", "dest": "0xf125a0266dddf8b700d34a7a38f1fb21e1ce325423fbdf546f3b5b0cc3931598", "j": 0, "kind": "private", "leaf": "0x63b02a52331676f290debc0bef44a5b4562bf2c4e31c20dc517ae97990a57e9f", "leaf_index": 0, "nullifier": "0xd9049bc8927d4a76ffdd8ac7387b801bd110a7cd628ae5fb22a44ef43198ac2d", "token": "0xd59b99d6b6394e84dc9c86bfe0502b68ed0acff6"}], "deposits_scanned": 2, "verdict": "ok"}`
- `bridge_status`: `{"burns": [{"burn_index": 0, "nullifier": "0xb1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1", "status": "released"}, {"burn_index": 1, "nullifier": "0xb2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2", "status": "releasable"}, {"burn_index": 5, "nullifier": "0xb3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3", "status": "awaiting_checkpoint"}], "checkpointed_burn_count": 2, "deposits": [{"leaf_index": 0, "nullifier": "0xa1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1", "status": "claimed"}, {"leaf_index": 1, "nullifier": "0xa2a …`

Step failures: 0. Check failures: 0.
