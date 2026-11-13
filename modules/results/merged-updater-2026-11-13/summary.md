# check.sh summary

## Steps

| step | result | seconds |
|---|---|---:|
| versions | ok | 0.2 |
| guests | ok | 3.3 |
| stage | ok | 1.6 |
| sdk-source | ok | 12.5 |
| cargo-lockfiles | ok | 0.0 |
| requests | ok | 0.1 |
| bridge-requests | ok | 0.1 |
| daemon-requests | ok | 0.1 |
| native-test | ok | 146.7 |
| native-calls | ok | 2.5 |
| native-daemons | ok | 193.5 |
| prepare-build | ok | 1.3 |
| build-lgx | ok | 553.2 |
| build-logosctl | ok | 71.8 |
| dev-daemon-start | ok | 0.4 |
| dev-install-ferrata_attest_module | ok | 0.6 |
| dev-load-ferrata_attest_module | ok | 0.1 |
| dev-show-ferrata_attest_module | ok | 0.1 |
| dev-install-ferrata_bridge_module | ok | 0.9 |
| dev-load-ferrata_bridge_module | ok | 0.1 |
| dev-show-ferrata_bridge_module | ok | 0.1 |
| dev-install-ferrata_attestor_module | ok | 0.4 |
| dev-load-ferrata_attestor_module | ok | 0.1 |
| dev-show-ferrata_attestor_module | ok | 0.1 |
| dev-install-ferrata_monitor_module | ok | 1.2 |
| dev-load-ferrata_monitor_module | ok | 0.1 |
| dev-show-ferrata_monitor_module | ok | 0.1 |
| dev-install-ferrata_updater_module | ok | 1.8 |
| dev-load-ferrata_updater_module | ok | 0.1 |
| dev-show-ferrata_updater_module | ok | 0.1 |
| dev-e2e | ok | 96.3 |
| dev-bridge-e2e | ok | 29.5 |
| dev-attestor-e2e | ok | 4.1 |
| dev-updater-e2e | ok | 118.5 |
| dev-monitor-e2e | ok | 42.2 |
| dev-module-ls | ok | 0.1 |
| dev-daemon-stop | ok | 0.8 |
| build-lgx-portable | ok | 72.7 |
| build-logosctl-portable | ok | 55.2 |
| portable-daemon-start | ok | 0.4 |
| portable-install-ferrata_attest_module | ok | 0.9 |
| portable-load-ferrata_attest_module | ok | 0.1 |
| portable-show-ferrata_attest_module | ok | 0.1 |
| portable-install-ferrata_bridge_module | ok | 1.3 |
| portable-load-ferrata_bridge_module | ok | 0.1 |
| portable-show-ferrata_bridge_module | ok | 0.1 |
| portable-install-ferrata_attestor_module | ok | 0.8 |
| portable-load-ferrata_attestor_module | ok | 0.1 |
| portable-show-ferrata_attestor_module | ok | 0.1 |
| portable-install-ferrata_monitor_module | ok | 1.6 |
| portable-load-ferrata_monitor_module | ok | 0.1 |
| portable-show-ferrata_monitor_module | ok | 0.1 |
| portable-install-ferrata_updater_module | ok | 2.0 |
| portable-load-ferrata_updater_module | ok | 0.1 |
| portable-show-ferrata_updater_module | ok | 0.1 |
| portable-e2e | ok | 88.5 |
| portable-bridge-e2e | ok | 27.7 |
| portable-attestor-e2e | ok | 4.0 |
| portable-updater-e2e | ok | 113.0 |
| portable-monitor-e2e | ok | 41.8 |
| portable-module-ls | ok | 0.1 |
| portable-daemon-stop | ok | 0.8 |
| sizes | ok | 4.5 |

## Calls

| call | module | method | native verdict | code | expectations | dev: equal to native | dev: seconds | portable: equal to native | portable: seconds |
|---|---|---|---|---:|---|---|---:|---|---:|
| version | ferrata_attest_module | version | ferrata_attest_module |  | met | yes | 0.1 | yes | 0.0 |
| attest_weth_balance | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.1 | yes | 0.0 |
| attest_empty_slot | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.1 | yes | 0.0 |
| attest_absent_account | ferrata_attest_module | dry_run_attest | attested | 0 | met | yes | 0.1 | yes | 0.1 |
| refuse_tampered_proof | ferrata_attest_module | dry_run_attest | refused | 110 | met | yes | 0.1 | yes | 0.0 |
| refuse_other_block | ferrata_attest_module | dry_run_attest | refused | 110 | met | yes | 0.1 | yes | 0.0 |
| refuse_wrong_chain | ferrata_attest_module | dry_run_attest | refused | 140 | met | yes | 0.1 | yes | 0.0 |
| checkpoint_p1899 | ferrata_attest_module | verify_checkpoint | ok | 0 | met | yes | 0.1 | yes | 0.0 |
| checkpoint_attest_fixture | ferrata_attest_module | verify_checkpoint | ok | 0 | met | yes | 0.1 | yes | 0.0 |
| checkpoint_wrong_root | ferrata_attest_module | verify_checkpoint | failed | 10 | met | yes | 0.1 | yes | 0.0 |
| checkpoint_chain_mismatch | ferrata_attest_module | verify_checkpoint | failed |  | met | yes | 0.1 | yes | 0.0 |
| bridge_version | ferrata_bridge_module | version | ferrata_bridge_module |  | met | yes | 0.0 | yes | 0.0 |
| bridge_deposit_case0 | ferrata_bridge_module | deposit | ok |  | met | yes | 0.0 | yes | 0.0 |
| bridge_deposit_case1 | ferrata_bridge_module | deposit | ok |  | met | yes | 0.1 | yes | 0.0 |
| bridge_deposit_case2 | ferrata_bridge_module | deposit | ok |  | met | yes | 0.0 | yes | 0.0 |
| bridge_claim_vector | ferrata_bridge_module | claim | claimable |  | met | yes | 0.1 | yes | 0.0 |
| bridge_claim_wrong_amount | ferrata_bridge_module | claim | refused |  | met | yes | 0.1 | yes | 0.0 |
| bridge_burn_case0 | ferrata_bridge_module | burn | ok |  | met | yes | 0.1 | yes | 0.0 |
| bridge_burn_case1 | ferrata_bridge_module | burn | ok |  | met | yes | 0.1 | yes | 0.0 |
| bridge_burn_case2 | ferrata_bridge_module | burn | ok |  | met | yes | 0.1 | yes | 0.0 |
| bridge_release_vector | ferrata_bridge_module | release | releasable |  | met | yes | 0.1 | yes | 0.0 |
| bridge_release_wrong_recipient | ferrata_bridge_module | release | refused |  | met | yes | 0.1 | yes | 0.0 |
| bridge_recover | ferrata_bridge_module | recover | ok |  | met | yes | 0.1 | yes | 0.0 |
| bridge_status | ferrata_bridge_module | status | ok |  | met | yes | 0.1 | yes | 0.0 |
| attestor_version | ferrata_attestor_module | version | ferrata_attestor_module |  | met | yes | 0.1 | yes | 0.0 |
| attestor_tier | ferrata_attestor_module | tier | ok |  | met | yes | 0.1 | yes | 0.1 |
| attestor_deployment | ferrata_attestor_module | deployment | ok |  | met | yes | 0.1 | yes | 0.0 |
| attestor_pubkey | ferrata_attestor_module | pubkey | ok |  | met | yes | 0.1 | yes | 0.0 |
| attestor_certify | ferrata_attestor_module | certify | certified |  | met | yes | 0.1 | yes | 0.0 |
| attestor_attest | ferrata_attestor_module | attest | attested |  | met | yes | 0.1 | yes | 0.0 |
| attestor_verify | ferrata_attestor_module | verify | verified |  | met | yes | 0.1 | yes | 0.0 |
| attestor_collect_3 | ferrata_attestor_module | collect | collected |  | met | yes | 0.1 | yes | 0.1 |
| attestor_collect_2 | ferrata_attestor_module | collect | refused |  | met | yes | 0.1 | yes | 0.1 |
| attestor_assemble_2 | ferrata_attestor_module | assemble | assembled |  | met | yes | 0.1 | yes | 0.1 |
| attestor_refuse_cert | ferrata_attestor_module | attest | refused |  | met | yes | 0.1 | yes | 0.1 |
| attestor_invalid | ferrata_attestor_module | collect | invalid_request |  | met | yes | 0.1 | yes | 0.0 |
| monitor_version | ferrata_monitor_module | version | ferrata_monitor_module |  | met | yes | 0.1 | yes | 0.0 |
| monitor_check_pass | ferrata_monitor_module | check | pass |  | met | yes | 0.1 | yes | 0.0 |
| monitor_check_conflict | ferrata_monitor_module | check | cancel |  | met | yes | 0.1 | yes | 0.0 |
| monitor_check_stale_set | ferrata_monitor_module | check | cancel |  | met | yes | 0.1 | yes | 0.0 |
| monitor_check_wait | ferrata_monitor_module | check | wait |  | met | yes | 0.1 | yes | 0.0 |
| monitor_node_view_lab | ferrata_monitor_module | node_view | ok |  | met | yes | 0.1 | yes | 0.0 |
| monitor_run_unbounded | ferrata_monitor_module | run | invalid_request |  | met | yes | 0.1 | yes | 0.0 |
| monitor_invalid | ferrata_monitor_module | check | invalid_request |  | met | yes | 0.1 | yes | 0.0 |
| updater_version | ferrata_updater_module | version | ferrata_updater_module |  | met | yes | 0.2 | yes | 0.2 |
| updater_config | ferrata_updater_module | check_config | ok |  | met | yes | 0.1 | yes | 0.1 |
| updater_config_binary | ferrata_updater_module | check_config | ok |  | met | yes | 0.1 | yes | 0.0 |
| updater_config_unknown_field | ferrata_updater_module | check_config | invalid_request |  | met | yes | 0.1 | yes | 0.0 |

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

## End to end through Logos Core (portable): 43 checks, 0 failed

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
| claim (private) | deposit 0: proven and settled privately on the zone | yes | proof dev, prove 1015 ms, balance 100000000000000000 |
| claim (private) | deposit 1: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor | yes | 1239 bytes, 6 accounts, mint fee 0 |
| claim (private) | deposit 1: proven and settled privately on the zone | yes | proof dev, prove 1026 ms, balance 200000000000000000 |
| claim (private) | deposit 0 again: refused from the nullifier's account (ALREADY_CLAIMED, 250) | yes |  |
| claim (private) | the private account holds both denominations | yes | balance 200000000000000000 |
| release (the wallet's rule) | two accepted roots: over burn 0 (delay passed) and over both (in its delay) | yes | now 1794546500, opens [1794545719, 1794546620] |
| release (the wallet's rule) | burn 0: the newest root whose delay has passed is the older one | yes | picked checkpoint 0, root 0x89409baec579a74d… |
| release (the wallet's rule) | burn 1: only the newer root covers it, in its delay: ReleaseDelayNotElapsed, with when it opens | yes | first open 1794546620 |
| release (the wallet's rule) | after the delay: burn 1 under the newer root | yes |  |
| prove_release | the release guest run on the module's input through r0vm: its journal is the native one | yes | image 0xb64900bcf7416685…, dev mode True, 67 ms, receipt 2344 bytes |
| vault | both releases the module planned are accepted by the vault (mock verifier on the exact journals) | yes | paid 100000000000000000 and 100000000000000000 wei |
| status | both burns released | yes |  |

## Bridge module end to end through Logos Core (portable, the mini-app devnet): 24 checks, 0 failed

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
| claim (private) | deposit 0: proven and settled privately on the zone | yes | proof dev, prove 952 ms, balance 100000000000000000 |
| claim (private) | deposit 1: the tool's claim (ferrata-wallet): claimable, ClaimPrivately, root = the private account's own actor | yes | 1239 bytes, 6 accounts, mint fee 0 |
| claim (private) | deposit 1: proven and settled privately on the zone | yes | proof dev, prove 971 ms, balance 200000000000000000 |
| claim (private) | deposit 0 again: refused from the nullifier's account (ALREADY_CLAIMED, 250) | yes |  |
| claim (private) | the private account holds both denominations | yes | balance 200000000000000000 |
| release (the wallet's rule) | two accepted roots: over burn 0 (delay passed) and over both (in its delay) | yes | now 1794546921, opens [1794546140, 1794547041] |
| release (the wallet's rule) | burn 0: the newest root whose delay has passed is the older one | yes | picked checkpoint 0, root 0x89409baec579a74d… |
| release (the wallet's rule) | burn 1: only the newer root covers it, in its delay: ReleaseDelayNotElapsed, with when it opens | yes | first open 1794547041 |
| release (the wallet's rule) | after the delay: burn 1 under the newer root | yes |  |
| prove_release | the release guest run on the module's input through r0vm: its journal is the native one | yes | image 0xb64900bcf7416685…, dev mode True, 63 ms, receipt 2344 bytes |
| vault | both releases the module planned are accepted by the vault (mock verifier on the exact journals) | yes | paid 100000000000000000 and 100000000000000000 wei |
| status | both burns released | yes |  |

## attestor (native): 23 passed, 0 failed

- PASS version: the module names the attestor it was built from
- PASS tier: tip 30, four keys, the blocks per signer = the run's tier.json
- PASS base: blocks 3..=30; its deployment = the run's deployment.json
- PASS certify 0: the channel key's certificate = the run's cert-0.json
- PASS attest 0: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-0.json
- PASS certify 1: the channel key's certificate = the run's cert-1.json
- PASS attest 1: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-1.json
- PASS certify 2: the channel key's certificate = the run's cert-2.json
- PASS attest 2: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-2.json
- PASS certify 3: the channel key's certificate = the run's cert-3.json
- PASS attest 3: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-3.json
- PASS verify: the four attestations verify
- PASS collect 3 of 4 (sequencer 3 silent): accepted natively, n = 4, t = 3, the input = the one the checkpoint program proved (sha256 f376ad54cf979cefcd7996dd56b6aacabfca67bb9c74161cc66e71da1389c575)
- PASS collect 3 of 4: its journal = the run's (rbz-zone-n4-t3.json)
- PASS collect 4 of 4: accepted, its journal = the run's (rbz-zone-n4-all.json)
- PASS collect 2 of 4: refused (t(4) = 3)
- PASS assemble 2 of 4: the program's own rule refuses it (InsufficientAttestations { have: 2, need: 3 }), as the run's refusal input
- PASS refused: a final tip (30) below the tip it signed (31): it never signs two histories
- PASS refused: a certificate of another attestation key
- PASS refused: the same certificate for another vault (a certificate binds chain, vault, configuration and channel)
- PASS loop: 3 rounds against the node over HTTP: signed tip 30 once, then found it unchanged twice; nothing refused
- PASS loop: its published attestation = the one-shot attest's (att-30.json, latest.json)
- PASS loop: it read the node each round (info, channel, blocks)

## updater (native): 8 passed, 0 failed

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is refused, as by the harness build of the binary (it is the build of lez/updater/sequencer)
- PASS up to date after 172 rounds (109 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

## monitor (native): 46 passed, 0 failed

- PASS S1 honest candidate still pending after 3 polls
- PASS S1 monitor passed it
- PASS S1 no cancellation of it
- PASS S1 its tip attested by 3 live certified keys (final-tier messages v2 checked with attestor-core)
- PASS S1 the certified key that signed a forged tip raised a critical attestation-finding (conflictingTip)
- PASS S1 finalised after the wait
- PASS S2 conflicting candidate cancelled
- PASS S2 cancelled before its deadline
- PASS S2 cannot be finalised after the wait
- PASS S3 stale key set cancelled (keySetNotLive)
- PASS S4 wrong one cancelled
- PASS S4 honest one untouched
- PASS S4 honest one finalised
- PASS S5 a second monitor on the same state file is refused while the first runs
- PASS S5 monitor saw it and was killed while waiting (node lag allowance)
- PASS S5 its state file survived the kill and names the candidate
- PASS S5 restarted monitor cancelled it before the deadline
- PASS S5 the reason is tipNotFinal
- PASS S6 fresh monitor cancelled the pending wrong candidate
- PASS S6 and nothing else (one cancellation in that run)
- PASS S6 it audited the two finalised checkpoints as good
- PASS S7 wrong checkpoint finalised with nobody watching
- PASS S7 watcher raised finalized-wrong with the revocation deadline
- PASS S7 the freeze authority's monitor revoked checkpoint 2
- PASS S9 a dry run raises would-cancel and leaves the candidate
- PASS S9 without gas money: cancel-failed (critical), candidate still pending
- PASS S9 funded again: cancelled at the next poll, before the deadline
- PASS S10 SIGTERM: exit 0, a stopped alert, the state file saved
- PASS S10 the lock is free at once: the next monitor starts
- PASS S11 cancelled on the forked node's contradiction (tipConflict in lez-forked)
- PASS S8 first cancellation
- PASS S8 second cancellation and the watcher's pause (about 12 h)
- PASS S8 a submission is refused during the pause
- PASS A0 the second vault is rule (A) (mode 0), turnSeconds 900
- PASS A1 honest candidate passed and untouched
- PASS A2 stale key set cancelled at once (keySetNotLive)
- PASS A3 tip not final: waiting until submittedAt + delta_g
- PASS A3 cancelled after delta_g, before its deadline (tipNotFinal)
- PASS A1 finalised with a beacon recorded after its wait
- PASS A1 audited as good after finalisation
- PASS A4 final tier stalled: alert and the watcher's 12 h pause (rule A)
- PASS R1 the lab record's final tier: 258 blocks
- PASS R2 a candidate at the real tip 258 passes
- PASS R3 the same id with another hash is cancelled
- PASS R4 a tip above the final tier waits
- PASS alerts: all 23 critical alerts reached the pager command

## attestor (dev): 23 passed, 0 failed

- PASS version: the module names the attestor it was built from
- PASS tier: tip 30, four keys, the blocks per signer = the run's tier.json
- PASS base: blocks 3..=30; its deployment = the run's deployment.json
- PASS certify 0: the channel key's certificate = the run's cert-0.json
- PASS attest 0: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-0.json
- PASS certify 1: the channel key's certificate = the run's cert-1.json
- PASS attest 1: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-1.json
- PASS certify 2: the channel key's certificate = the run's cert-2.json
- PASS attest 2: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-2.json
- PASS certify 3: the channel key's certificate = the run's cert-3.json
- PASS attest 3: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-3.json
- PASS verify: the four attestations verify
- PASS collect 3 of 4 (sequencer 3 silent): accepted natively, n = 4, t = 3, the input = the one the checkpoint program proved (sha256 f376ad54cf979cefcd7996dd56b6aacabfca67bb9c74161cc66e71da1389c575)
- PASS collect 3 of 4: its journal = the run's (rbz-zone-n4-t3.json)
- PASS collect 4 of 4: accepted, its journal = the run's (rbz-zone-n4-all.json)
- PASS collect 2 of 4: refused (t(4) = 3)
- PASS assemble 2 of 4: the program's own rule refuses it (InsufficientAttestations { have: 2, need: 3 }), as the run's refusal input
- PASS refused: a final tip (30) below the tip it signed (31): it never signs two histories
- PASS refused: a certificate of another attestation key
- PASS refused: the same certificate for another vault (a certificate binds chain, vault, configuration and channel)
- PASS loop: 3 rounds against the node over HTTP: signed tip 30 once, then found it unchanged twice; nothing refused
- PASS loop: its published attestation = the one-shot attest's (att-30.json, latest.json)
- PASS loop: it read the node each round (info, channel, blocks)

## updater (dev): 10 passed, 0 failed

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is refused, as by the harness build of the binary (it is the build of lez/updater/sequencer)
- PASS start: the daemon runs in the module
- PASS status: polled 23 times while the daemon ran; then stop ended it
- PASS up to date after 501 rounds (117 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

## monitor (dev): 46 passed, 0 failed

- PASS S1 honest candidate still pending after 3 polls
- PASS S1 monitor passed it
- PASS S1 no cancellation of it
- PASS S1 its tip attested by 3 live certified keys (final-tier messages v2 checked with attestor-core)
- PASS S1 the certified key that signed a forged tip raised a critical attestation-finding (conflictingTip)
- PASS S1 finalised after the wait
- PASS S2 conflicting candidate cancelled
- PASS S2 cancelled before its deadline
- PASS S2 cannot be finalised after the wait
- PASS S3 stale key set cancelled (keySetNotLive)
- PASS S4 wrong one cancelled
- PASS S4 honest one untouched
- PASS S4 honest one finalised
- PASS S5 a second monitor on the same state file is refused while the first runs
- PASS S5 monitor saw it and was killed while waiting (node lag allowance)
- PASS S5 its state file survived the kill and names the candidate
- PASS S5 restarted monitor cancelled it before the deadline
- PASS S5 the reason is tipNotFinal
- PASS S6 fresh monitor cancelled the pending wrong candidate
- PASS S6 and nothing else (one cancellation in that run)
- PASS S6 it audited the two finalised checkpoints as good
- PASS S7 wrong checkpoint finalised with nobody watching
- PASS S7 watcher raised finalized-wrong with the revocation deadline
- PASS S7 the freeze authority's monitor revoked checkpoint 2
- PASS S9 a dry run raises would-cancel and leaves the candidate
- PASS S9 without gas money: cancel-failed (critical), candidate still pending
- PASS S9 funded again: cancelled at the next poll, before the deadline
- PASS S10 SIGTERM: exit 0, a stopped alert, the state file saved
- PASS S10 the lock is free at once: the next monitor starts
- PASS S11 cancelled on the forked node's contradiction (tipConflict in lez-forked)
- PASS S8 first cancellation
- PASS S8 second cancellation and the watcher's pause (about 12 h)
- PASS S8 a submission is refused during the pause
- PASS A0 the second vault is rule (A) (mode 0), turnSeconds 900
- PASS A1 honest candidate passed and untouched
- PASS A2 stale key set cancelled at once (keySetNotLive)
- PASS A3 tip not final: waiting until submittedAt + delta_g
- PASS A3 cancelled after delta_g, before its deadline (tipNotFinal)
- PASS A1 finalised with a beacon recorded after its wait
- PASS A1 audited as good after finalisation
- PASS A4 final tier stalled: alert and the watcher's 12 h pause (rule A)
- PASS R1 the lab record's final tier: 258 blocks
- PASS R2 a candidate at the real tip 258 passes
- PASS R3 the same id with another hash is cancelled
- PASS R4 a tip above the final tier waits
- PASS alerts: all 23 critical alerts reached the pager command

## attestor (portable): 23 passed, 0 failed

- PASS version: the module names the attestor it was built from
- PASS tier: tip 30, four keys, the blocks per signer = the run's tier.json
- PASS base: blocks 3..=30; its deployment = the run's deployment.json
- PASS certify 0: the channel key's certificate = the run's cert-0.json
- PASS attest 0: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-0.json
- PASS certify 1: the channel key's certificate = the run's cert-1.json
- PASS attest 1: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-1.json
- PASS certify 2: the channel key's certificate = the run's cert-2.json
- PASS attest 2: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-2.json
- PASS certify 3: the channel key's certificate = the run's cert-3.json
- PASS attest 3: tip 30 signed after checking blocks 2..=30; the attestation = the run's att-3.json
- PASS verify: the four attestations verify
- PASS collect 3 of 4 (sequencer 3 silent): accepted natively, n = 4, t = 3, the input = the one the checkpoint program proved (sha256 f376ad54cf979cefcd7996dd56b6aacabfca67bb9c74161cc66e71da1389c575)
- PASS collect 3 of 4: its journal = the run's (rbz-zone-n4-t3.json)
- PASS collect 4 of 4: accepted, its journal = the run's (rbz-zone-n4-all.json)
- PASS collect 2 of 4: refused (t(4) = 3)
- PASS assemble 2 of 4: the program's own rule refuses it (InsufficientAttestations { have: 2, need: 3 }), as the run's refusal input
- PASS refused: a final tip (30) below the tip it signed (31): it never signs two histories
- PASS refused: a certificate of another attestation key
- PASS refused: the same certificate for another vault (a certificate binds chain, vault, configuration and channel)
- PASS loop: 3 rounds against the node over HTTP: signed tip 30 once, then found it unchanged twice; nothing refused
- PASS loop: its published attestation = the one-shot attest's (att-30.json, latest.json)
- PASS loop: it read the node each round (info, channel, blocks)

## updater (portable): 10 passed, 0 failed

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is refused, as by the harness build of the binary (it is the build of lez/updater/sequencer)
- PASS start: the daemon runs in the module
- PASS status: polled 22 times while the daemon ran; then stop ended it
- PASS up to date after 375 rounds (113 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

## monitor (portable): 46 passed, 0 failed

- PASS S1 honest candidate still pending after 3 polls
- PASS S1 monitor passed it
- PASS S1 no cancellation of it
- PASS S1 its tip attested by 3 live certified keys (final-tier messages v2 checked with attestor-core)
- PASS S1 the certified key that signed a forged tip raised a critical attestation-finding (conflictingTip)
- PASS S1 finalised after the wait
- PASS S2 conflicting candidate cancelled
- PASS S2 cancelled before its deadline
- PASS S2 cannot be finalised after the wait
- PASS S3 stale key set cancelled (keySetNotLive)
- PASS S4 wrong one cancelled
- PASS S4 honest one untouched
- PASS S4 honest one finalised
- PASS S5 a second monitor on the same state file is refused while the first runs
- PASS S5 monitor saw it and was killed while waiting (node lag allowance)
- PASS S5 its state file survived the kill and names the candidate
- PASS S5 restarted monitor cancelled it before the deadline
- PASS S5 the reason is tipNotFinal
- PASS S6 fresh monitor cancelled the pending wrong candidate
- PASS S6 and nothing else (one cancellation in that run)
- PASS S6 it audited the two finalised checkpoints as good
- PASS S7 wrong checkpoint finalised with nobody watching
- PASS S7 watcher raised finalized-wrong with the revocation deadline
- PASS S7 the freeze authority's monitor revoked checkpoint 2
- PASS S9 a dry run raises would-cancel and leaves the candidate
- PASS S9 without gas money: cancel-failed (critical), candidate still pending
- PASS S9 funded again: cancelled at the next poll, before the deadline
- PASS S10 SIGTERM: exit 0, a stopped alert, the state file saved
- PASS S10 the lock is free at once: the next monitor starts
- PASS S11 cancelled on the forked node's contradiction (tipConflict in lez-forked)
- PASS S8 first cancellation
- PASS S8 second cancellation and the watcher's pause (about 12 h)
- PASS S8 a submission is refused during the pause
- PASS A0 the second vault is rule (A) (mode 0), turnSeconds 900
- PASS A1 honest candidate passed and untouched
- PASS A2 stale key set cancelled at once (keySetNotLive)
- PASS A3 tip not final: waiting until submittedAt + delta_g
- PASS A3 cancelled after delta_g, before its deadline (tipNotFinal)
- PASS A1 finalised with a beacon recorded after its wait
- PASS A1 audited as good after finalisation
- PASS A4 final tier stalled: alert and the watcher's 12 h pause (rule A)
- PASS R1 the lab record's final tier: 258 blocks
- PASS R2 a candidate at the real tip 258 passes
- PASS R3 the same id with another hash is cancelled
- PASS R4 a tip above the final tier waits
- PASS alerts: all 23 critical alerts reached the pager command

## Replies (native)

- `version`: `{"bls_backend": "blst (advance runs the program's steps natively)", "methods": ["version", "dry_run_attest", "verify_checkpoint", "fetch_log_proof", "build_attest_tx", "read_attestation", "read_instance", "advance"], "module": "ferrata_attest_module", "staged": {"commit": "6dcf6af6358007104a48b0e3a27d134946684c49", "crates": [{"from": "crates/ferrata-core", "name": "ferrata-core", "source_sha256": "f43255c33578f87ccd50386f936ea722ae2bbddc7fc60509e97a6eafc39864d8", "version": "0.0.1"}, {"from": "lez/wire", "name": "ferrata-lez-wire", "source_sha256": "759b29f064000656b2d3795496d72bdd0074ce13aff …`
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
- `bridge_version`: `{"methods": ["version", "deposit", "claim", "burn", "release", "recover", "status", "prove_release", "import_root", "registry"], "module": "ferrata_bridge_module", "spec": "v2", "staged": {"commit": "6dcf6af6358007104a48b0e3a27d134946684c49", "crates": [{"from": "crates/ferrata-core", "name": "ferrata-core", "source_sha256": "f43255c33578f87ccd50386f936ea722ae2bbddc7fc60509e97a6eafc39864d8", "version": "0.0.1"}, {"from": "bridge/crates/ferrata-bridge-core", "name": "ferrata-bridge-core", "source_sha256": "29349d5bd266412e48c4c0d1df40d609c0992301fe90922ea7176fb51183c4db", "version": "0.2.0"}, { …`
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
- `attestor_version`: `{"ferrata_attestor": {"commit": "6dcf6af6358007104a48b0e3a27d134946684c49", "source_sha256": "343cd081c50b7683cc966fcb7db9fa4fdd6663c9a0978ad01dfe1dfd8f7b8c83", "version": "0.1.0"}, "methods": ["version", "keygen", "pubkey", "deployment", "certify", "tier", "attest", "verify", "collect", "assemble", "base", "start", "status", "stop"], "module": "ferrata_attestor_module", "record_version": "ferrata-attestation/v1", "version": "0.1.0"}`
- `attestor_tier`: `{"blocks_checked": 29, "blocks_per_signer": {"6462ecf103d4074bbdbed04e115233548af773d62506b30ef25a2501af597fac": {"count": 6, "first": 7, "last": 12}, "90a548e0db5eaf858e31e437bdf3c6d960627e80ed5e1d9d49c5a07fb74f0729": {"count": 6, "first": 13, "last": 18}, "99273f3e47ddce773ac19173cde1fa5b11588e3b285228f0da3d33abb68166e3": {"count": 6, "first": 19, "last": 24}, "fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096": {"count": 11, "first": 2, "last": 30}}, "final_msg_id": "1cf04ac650090ce1a0a329f6a50c97d768717e86a1ad6e8eca74b894e075e33d", "first_block": 2, "keys": ["fce0d0fadf637a6 …`
- `attestor_deployment`: `{"deployment": {"channel_id": "0101010101010101010101010101010101010101010101010101010101010101", "evm_chain_id": "0000000000000000000000000000000000000000000000000000000000007a69", "lez_config_id": "21effc46a860f3cbe66cacdd28fc11d4e0691ce7239e9ca330657d7ecc19523e", "vault": "0xcc12f910bdbe58196fe071d29859119be398fc38"}, "verdict": "ok"}`
- `attestor_pubkey`: `{"public_key": "fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096", "verdict": "ok"}`
- `attestor_certify`: `{"cert": {"attestation_key": "c5e721a5205298125bb29075b3d912ea091f23e0ac098ff00ddb4f5ce8af02df", "certificate": "62ce15a6dd8367508d5151f7381c2e192d2117ce1754ac6060d8d53f08e57787450447195103210722a1f30567f55156199b69ef5393d06f3f7b9e0646faa407", "certificate_message": "666572726174612f6174746573742d6b65792f76310000000000000000000000000000000000000000000000000000000000007a69cc12f910bdbe58196fe071d29859119be398fc3821effc46a860f3cbe66cacdd28fc11d4e0691ce7239e9ca330657d7ecc19523e0101010101010101010101010101010101010101010101010101010101010101c5e721a5205298125bb29075b3d912ea091f23e0ac098ff00ddb4f5ce8 …`
- `attestor_attest`: `{"attestation": {"attestation_key": "c5e721a5205298125bb29075b3d912ea091f23e0ac098ff00ddb4f5ce8af02df", "certificate": "62ce15a6dd8367508d5151f7381c2e192d2117ce1754ac6060d8d53f08e57787450447195103210722a1f30567f55156199b69ef5393d06f3f7b9e0646faa407", "channel_key": "fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096", "deployment": {"channel_id": "0101010101010101010101010101010101010101010101010101010101010101", "evm_chain_id": "0000000000000000000000000000000000000000000000000000000000007a69", "lez_config_id": "21effc46a860f3cbe66cacdd28fc11d4e0691ce7239e9ca330657d7ecc19523e",  …`
- `attestor_verify`: `{"refused": 0, "results": [{"channel_key": "fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096", "name": "spike/ferrata-wt-merge-updater/checkpoint/attestor/runs/zone/att-0.json", "tip_id": 30, "verifies": true}, {"channel_key": "99273f3e47ddce773ac19173cde1fa5b11588e3b285228f0da3d33abb68166e3", "name": "spike/ferrata-wt-merge-updater/checkpoint/attestor/runs/zone/att-1.json", "tip_id": 30, "verifies": true}, {"channel_key": "6462ecf103d4074bbdbed04e115233548af773d62506b30ef25a2501af597fac", …`
- `attestor_collect_3`: `{"dropped": [], "input_bytes": 26166, "input_sha256": "f376ad54cf979cefcd7996dd56b6aacabfca67bb9c74161cc66e71da1389c575", "journal_sha256": "57d90e168cc24420aafeb0e6f5bfece785c8add07b31532ba696abf8695ec985", "kept": ["spike/ferrata-wt-merge-updater/checkpoint/attestor/runs/zone/att-0.json", "spike/ferrata-wt-merge-updater/checkpoint/attestor/runs/zone/att-1.json", "spike/ferrata-wt-merge-updater/checkpoint/attestor/runs/zone/att-2.json"], "meta": {"attestations …`
- `attestor_collect_2`: `{"error": "too few attestations: 2 valid, t(n = 4) = 3 needed; dropped []", "verdict": "refused"}`
- `attestor_assemble_2`: `{"input_bytes": 25902, "input_sha256": "e23e03c8d3a19737c87fda0ade5c19a0d3e2c98dce8debe355e47ab75da56fc7", "meta": {"attestations": [{"attestation_key": "c5e721a5205298125bb29075b3d912ea091f23e0ac098ff00ddb4f5ce8af02df", "channel_key": "fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096", "tip_id": 30}, {"attestation_key": "9c2d7b57e5915bb782173d30cb1e2316252cf46beccfdc0db0e184cbbb542d6c", "channel_key": "99273f3e47ddce773ac19173cde1fa5b11588e3b285228f0da3d33abb68166e3", "tip_id": 30}], "block_bytes": 23492, "blocks": 28, "blocks_to_t": 28, "config_tip_hash": "e502602b81aa7b2ae5b …`
- `attestor_refuse_cert`: `{"error": "refused: the certificate does not certify attestation key c5e721a5205298125bb29075b3d912ea091f23e0ac098ff00ddb4f5ce8af02df by channel key 99273f3e47ddce773ac19173cde1fa5b11588e3b285228f0da3d33abb68166e3 for this deployment", "verdict": "refused"}`
- `monitor_version`: `{"methods": ["version", "check", "node_view", "run", "start", "status", "stop"], "module": "ferrata_monitor_module", "monitor": {"commit": "6dcf6af6358007104a48b0e3a27d134946684c49", "source_sha256": "528e7d633d13d164379a8674510484c56b1148b4ab8cb15f11967b45c6eadd0a", "version": "0.1.0"}, "version": "0.1.0"}`
- `monitor_check_pass`: `{"verdict": "pass"}`
- `monitor_check_conflict`: `{"reason": {"finalHash": "0x3333333333333333333333333333333333333333333333333333333333333333", "kind": "tipConflict", "node": "synthetic", "tipId": 3}, "verdict": "cancel"}`
- `monitor_check_stale_set`: `{"reason": {"candidateN": 3, "candidateT": 2, "kind": "keySetNotLive", "liveN": 4, "liveT": 3, "node": "synthetic"}, "verdict": "cancel"}`
- `monitor_check_wait`: `{"until": 1300, "verdict": "wait", "why": "tip 5 not yet final in synthetic (final height 3)"}`
- `monitor_node_view_lab`: `{"verdict": "ok", "view": {"attestations": [], "finalBlocks": {"1": "0x19e0b86b3a6abf2b6587c89d5f7e08027e694861905aca9c3f83b58e5807407f", "10": "0xca477453ec9d7218203ee5e693f6fffb51138bcbf20a0e95f7e2a33599b83a6c", "100": "0xa124229c3aa4269c0c977ebc2f30f975193cd609ed9867b72fbda30873b62120", "101": "0x2392024053cef4827c98040b14220751279b0d9d6b09eaac34fa25868bc0b86f", "102": "0x2f733c7d2527727474620239fa4161048ece69aab3f573da18d0edd102ea26f5", "103": "0x38f6bf1bf0c75e40e77ddf3829f702f13175733ab96b158543ad27795201434c", "104": "0x7e3fa352ee0c0430cd2926bbb1e2d1f90a9aae24a2953332c4278bc70260315d", " …`
- `updater_version`: `{"dev_mode": "1", "ledgers": ["harness"], "methods": ["version", "check_config", "run", "start", "status", "metrics", "stop"], "module": "ferrata_updater_module", "program": {"bytes": 627064, "computed_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df", "declared_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df", "name": "ferrata_lez"}, "updater": {"commit": "6dcf6af6358007104a48b0e3a27d134946684c49", "guests": {"ferrata_bridge": "0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd", "ferrata_consumer": "0x8464603bcf9ea9c69c0eac1006 …`
- `updater_config`: `{"chains": [{"checkpoint": "4baa732e5dbdbe97932abcf62c7caccc1cba8e9c14118d153aea629e25a4abff", "layout": null, "name": "mainnet", "preset": "mainnet", "sources": [{"dir": "../../../lez/lab/results/record/net/mainnet", "kind": "light-client", "name": "record-a", "url": null}, {"dir": "../../../lez/lab/results/record/net/mainnet", "kind": "light-client", "name": "record-b", "url": null}]}, {"checkpoint": "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d", "layout": null, "name": "sepolia", "preset": "sepolia", "sources": [{"dir": "../../../lez/lab/results/record/net/sepolia", "ki …`
- `updater_config_binary`: `{"chains": [{"checkpoint": "3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c", "layout": "gloas", "name": "sepolia", "preset": "sepolia", "sources": [{"dir": null, "kind": "light-client", "name": "lodestar", "url": "https://lodestar-sepolia.chainsafe.io"}, {"dir": null, "kind": "headers", "name": "publicnode", "url": "https://ethereum-sepolia-beacon-api.publicnode.com"}]}, {"checkpoint": "4baa732e5dbdbe97932abcf62c7caccc1cba8e9c14118d153aea629e25a4abff", "layout": null, "name": "mainnet", "preset": "mainnet", "sources": [{"dir": null, "kind": "light-client", "name": "lodestar", …`

Step failures: 0. Check failures: 0.
